use std::str::FromStr;

use alloy::{
    consensus::TxEnvelope,
    hex::FromHex,
    network::{EthereumWallet, TransactionBuilder},
    primitives::*,
    providers::{Provider, ProviderBuilder, WsConnect},
    pubsub::SubscriptionStream,
    rpc::types::{Transaction, TransactionRequest},
    signers::{
        k256::elliptic_curve::consts::U12,
        local::{LocalSigner, PrivateKeySigner},
    },
};
use futures::{FutureExt, StreamExt};
use revm::primitives::Address;

use crate::{
    mev::flashbot::Flashbot,
    simulator::{SimulateTxMsg, Simulator},
};

/**
 * 监控发送eth到本地址的交易， 创建一笔返还1/10的金额的交易， 通过bundle发送这两笔交易
 */
#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn test_listen_and_bundle() {
    let private_key = "";
    let sender = Flashbot::new();
    let simulator = Simulator::new("");
    let cli = ProviderBuilder::new()
        .on_ws(WsConnect::new(""))
        .await
        .unwrap();
    let mut tx_stream = cli
        .subscribe_full_pending_transactions()
        .await
        .unwrap()
        .into_stream();
    let signer = EthereumWallet::from(PrivateKeySigner::from_str(private_key).unwrap());
    while let Some(tx) = tx_stream.next().await {
        if tx.to.is_some() && tx.to.unwrap() == signer.default_signer().address() {
            println!("tx: {:?}", tx);
            let return_value = tx.value / U256::from(10);
            // 本地模拟bundle
            let (success, results) = simulator.simulate(vec![
                SimulateTxMsg {
                    from: tx.from,
                    to: tx.to.unwrap(),
                    value: tx.value,
                    data: tx.input.clone(),
                },
                SimulateTxMsg {
                    from: tx.to.unwrap(),
                    to: tx.from,
                    value: return_value,
                    data: Bytes::new(),
                },
            ]);
            // bundle成功则发送flashbot
            if success {
                let nonce = cli
                    .get_transaction_count(signer.default_signer().address())
                    .await
                    .unwrap();
                println!("sim result: {:?}", results);
                let encoded_tx1 = TxEnvelope::try_from(tx.clone()).unwrap();
                let tx2 = TransactionRequest::default()
                    .with_to(tx.from)
                    .with_value(return_value)
                    .with_gas_limit(21000)
                    .with_gas_price(30000000000_u128)
                    .with_nonce(nonce)
                    .build(&signer)
                    .await
                    .unwrap();
                let bn = cli.get_block_number().await.unwrap();
                let bundle_result = sender.send_bundle(vec![encoded_tx1, tx2], bn + 1).await;
                println!("bundle: {:?}", bundle_result);
                break;
            }
        }
        // println!("{:?}", tx);
    }
}
