use std::str::FromStr;
use std::sync::Arc;

use alloy::consensus::TxEnvelope;
use alloy::eips::{eip2718::Encodable2718, BlockId};
use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, Bytes, Uint};
use alloy::rpc::types::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;
use alloy::sol_types::SolCall;
use alloy::{
    hex::FromHex,
    network::EthereumWallet,
    providers::{self, Provider, ProviderBuilder, WsConnect},
    rpc::types::Transaction,
};
use foundry_evm::backend::SharedBackend;
use futures::StreamExt;
use log::info;
use revm::primitives::{TransactTo, TxEnv, U256};
use tokio::sync::broadcast::{self, Sender};

use crate::abi::argus::CallData;
use crate::abi::{aave_pool, argus};
use crate::mev::flashbot;
use crate::simulator::{new_evm, shared_backend};

async fn do_subscribe(ws: String, to_addr: Address, sender: Sender<Transaction>) {
    let url = ws.clone();
    info!("subscribe tx from {:?}", url);
    let ws_provider = providers::ProviderBuilder::new()
        .on_ws(WsConnect::new(url))
        .await;
    let pv = match ws_provider {
        Ok(p) => p,
        Err(e) => panic!("{}", e),
    };
    let txs = pv.subscribe_full_pending_transactions().await;
    let mut stream = (match txs {
        Ok(p) => p,
        Err(e) => panic!("{}", e),
    })
    .into_stream();
    // let mut parsed = stream.map(parser);
    while let Some(tx) = stream.next().await {
        // info!("new tx: {:?}", tx);
        match tx.to {
            Some(to) => {
                // if to == to_addr {
                sender.send(tx).unwrap();
                // }
            }
            None => break,
        }
    }
}

// 机器人主循环
// 监听swap event, 寻找path， 模拟利润， 发送交易
pub async fn main_loop(ws: String, rpc: String, private_key: String) {
    let backend = shared_backend(&rpc);
    let (sender, mut receiver) = broadcast::channel::<Transaction>(10000);
    tokio::spawn(do_subscribe(
        ws,
        Address::from_hex("").unwrap(),
        sender.clone(),
    ));
    let rpc = Arc::new(ProviderBuilder::new().on_builtin(&rpc).await.unwrap());
    let signer = PrivateKeySigner::from_str(&private_key).unwrap();
    let wallet = EthereumWallet::from(signer);

    let supply_tx = aave_pool::IPool::supplyCall {
        asset: Address::from_hex("").unwrap(),
        amount: Uint::from_str_radix("6255830000000000000000", 10).unwrap(),
        onBehalfOf: Address::from_hex("").unwrap(),
        referralCode: 0,
    };

    let supply_tx_encoded = supply_tx.abi_encode();
    let argus_addr = Address::from_hex("").unwrap();
    let argus_tx = argus::IAccount::execTransactionCall {
        callData: CallData {
            flag: Uint::from(0),
            to: argus_addr,
            value: Uint::from(0),
            data: Bytes::from_iter(&supply_tx_encoded.clone()),
            hint: Bytes::new(),
            extra: Bytes::new(),
        },
    };
    let argus_tx_encoded = argus_tx.abi_encode();
    let mev = Flashbot::new("blox_token");

    while let Result::Ok(tx) = receiver.recv().await {
        let tx_cp = tx.clone();
        let w = wallet.clone();

        let raw_tx = SimTx {
            caller: tx.from,
            to: tx.to.unwrap(),
            data: tx.input,
        };

        let our_tx = SimTx {
            caller: w.default_signer().address(),
            to: argus_addr,
            data: Bytes::from_iter(&argus_tx_encoded.clone()),
        };
        match sim(vec![raw_tx, our_tx], backend.clone()) {
            true => {
                let nonce = rpc
                    .get_transaction_count(wallet.default_signer().address())
                    .await
                    .unwrap();
                let bn = rpc.get_block_number().await.unwrap();
                let rawtx1 =
                    Bytes::from_iter(TxEnvelope::try_from(tx_cp.clone()).unwrap().encoded_2718());
                let rawTx2 = Bytes::from_iter(
                    TransactionRequest::default()
                        .with_from(wallet.default_signer().address())
                        .with_to(argus_addr)
                        .with_gas_limit(800000)
                        .with_gas_price(u128::from_str("82000000000").unwrap())
                        .with_nonce(nonce)
                        .with_value(U256::from(0))
                        .with_input(argus_tx_encoded)
                        .build(&wallet)
                        .await
                        .unwrap()
                        .encoded_2718(),
                );
                mev.send_bundle(vec![rawtx1, rawTx2], bn + 1).await.unwrap();
                break;
            }
            false => {
                continue;
            }
        };
    }
}

pub struct SimTx {
    pub caller: Address,
    pub to: Address,
    pub data: Bytes,
}

fn sim(simtxs: Vec<SimTx>, backend: SharedBackend) -> bool {
    let mut evm = new_evm(backend);
    for ele in simtxs {
        let env = evm.context.evm.env.as_mut();
        let to = TransactTo::Call(ele.to);
        let data: Bytes = ele.data;

        env.tx = TxEnv::default();
        env.tx.caller = ele.caller;
        env.tx.data = data.clone();
        env.tx.transact_to = to.clone();
        let result = evm.transact().unwrap();
        match result.result {
            revm::primitives::ExecutionResult::Success {
                reason,
                gas_used,
                gas_refunded,
                logs,
                output,
            } => continue,
            revm::primitives::ExecutionResult::Revert { gas_used, output } => return false,
            revm::primitives::ExecutionResult::Halt { reason, gas_used } => return false,
        }
    }
    return true;
}
