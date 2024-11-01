use std::{collections::BTreeSet, sync::Arc};

use alloy::consensus::{Transaction, TxEnvelope};
use alloy::eips::eip2718::Decodable2718;
use alloy::eips::BlockId;
use alloy::primitives::{Address, Bytes, U256};
use alloy::rpc::types::{self, Block, TransactionRequest};
use alloy::{hex::FromHex as _, network::TransactionBuilder};
use eyre::{Context, Result};
use foundry_common::provider::ProviderBuilder;
use foundry_evm::backend::{BlockchainDb, BlockchainDbMeta, SharedBackend};
use revm::inspectors::CustomPrintTracer;
use revm::primitives::{BlockEnv, ExecutionResult, TransactTo, TxEnv, TxKind};
use revm::Inspector;
use revm::{db::CacheDB, primitives::CancunSpec, Evm, Handler};
use std::str::FromStr;

use crate::abi::argus::Argus::rescueCall;

#[derive(Debug, Clone)]
pub struct SimulateTxMsg {
    pub from: Address,
    pub to: Address,
    pub value: U256,
    pub data: Bytes,
}

impl From<TransactionRequest> for SimulateTxMsg {
    fn from(x: TransactionRequest) -> Self {
        SimulateTxMsg {
            from: x.from.unwrap(),
            to: x.to.unwrap().to().unwrap().clone(),
            value: x.value.unwrap(),
            data: x.input.input().unwrap().clone(),
        }
    }
}

impl From<Bytes> for SimulateTxMsg {
    fn from(x: Bytes) -> Self {
        let decoded = TxEnvelope::decode_2718(&mut x.as_ref()).unwrap();
        SimulateTxMsg {
            from: decoded.recover_signer().unwrap(),
            to: decoded.kind().to().unwrap().clone(),
            value: decoded.value(),
            data: decoded.input().clone(),
        }
    }
}
impl From<TxEnvelope> for SimulateTxMsg {
    fn from(decoded: TxEnvelope) -> Self {
        SimulateTxMsg {
            from: decoded.recover_signer().unwrap(),
            to: decoded.kind().to().unwrap().clone(),
            value: decoded.value(),
            data: decoded.input().clone(),
        }
    }
}

impl From<types::Transaction> for SimulateTxMsg {
    fn from(tx: types::Transaction) -> Self {
        SimulateTxMsg {
            from: tx.from,
            to: tx.to.unwrap(),
            value: tx.value,
            data: tx.input,
        }
    }
}

#[derive(Debug)]
pub struct Simulator {
    // pub backend: SharedBackend,
    pub evm: Evm<'static, (), CacheDB<SharedBackend>>,
}

impl Simulator {
    pub fn new(backend: SharedBackend, block_env: BlockEnv) -> Simulator {
        let db = CacheDB::new(backend.clone());
        let mut evm = revm::Evm::builder()
            .with_db(db)
            .with_handler(Handler::mainnet::<CancunSpec>())
            // .with_external_context(CustomPrintTracer::default())
            .build();
        evm.context.evm.env.block = block_env;
        Simulator { evm: evm }
    }
    pub fn exec_transaction<T>(&mut self, tx: T) -> Result<ExecutionResult>
    where
        SimulateTxMsg: From<T>,
        T: Clone,
    {
        let ele: SimulateTxMsg = tx.into();
        let to = TxKind::Call(ele.to);
        let data: Bytes = ele.data.clone();

        let env = self.evm.context.evm.env.as_mut();
        env.tx = TxEnv::default();
        env.tx.caller = ele.from;
        env.tx.data = data.clone();
        env.tx.value = ele.value;
        env.tx.transact_to = to.clone();
        let result = self.evm.transact_commit();
        result.wrap_err("simulating error")
    }
}

pub fn shared_backend(url: &str) -> SharedBackend {
    let provider = Arc::new(ProviderBuilder::new(url).build().expect("backend build"));
    let shared_backend = SharedBackend::spawn_backend_thread(
        provider.clone(),
        BlockchainDb::new(
            BlockchainDbMeta {
                cfg_env: Default::default(),
                block_env: Default::default(),
                hosts: BTreeSet::from(["".to_string()]),
            },
            None,
        ),
        None,
    );
    shared_backend
}

#[test]
fn test_bundle() {
    let backend =
        shared_backend("");
    // backend.set_pinned_block(21087781).unwrap();
    let mut block_env = BlockEnv::default();
    block_env.timestamp = U256::from(u64::MAX);


    let mut simulator = Simulator::new(backend, block_env);
    simulator.evm.cfg_mut().disable_eip3607 = true;
    // 开额度
    let update_cap_call = SimulateTxMsg {
        from: Address::from_hex("0x47c71dFEB55Ebaa431Ae3fbF99Ea50e0D3d30fA8").unwrap(),
        to: Address::from_hex("0x3843b29118fFC18d5d12EE079d0324E1bF115e69").unwrap(),
        value: U256::ZERO,
        data: Bytes::from_hex("0x55caa163000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000020000000000000000000000007f39c581f595b53c5cb19bd0b3f8da6c935e2ca0ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd5000000000000000000000000000000000000000000000000000000000000dac0000000000000000000000000bf5495efe5db9ce00f80364c8b423567e58d2110000000000000000000000000000000000000000000000000000000000000ea60ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd5").unwrap(),
    };
    // println!("{:?}", update_cap_call);

    let result = simulator.exec_transaction(update_cap_call);
    println!("{:?}", result);
}
