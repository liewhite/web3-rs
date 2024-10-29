use std::{collections::BTreeSet, sync::Arc};

use alloy::consensus::{Transaction, TxEnvelope};
use alloy::eips::eip2718::Decodable2718;
use alloy::primitives::{Address, Bytes, U256};
use alloy::rpc::types::TransactionRequest;
use alloy::{hex::FromHex as _, network::TransactionBuilder};
use eyre::{Context, Result};
use foundry_common::provider::ProviderBuilder;
use foundry_evm::backend::{BlockchainDb, BlockchainDbMeta, SharedBackend};
use revm::primitives::{ExecutionResult, TransactTo, TxEnv};
use revm::{db::CacheDB, primitives::CancunSpec, Evm, Handler};

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

#[derive(Debug, Clone)]
pub struct Simulator {
    backend: SharedBackend,
}

impl Simulator {
    pub fn new(url: &str) -> Simulator {
        let backend = shared_backend(url);
        Simulator { backend: backend }
    }
    pub fn simulate<T>(&self, bundle: &Vec<T>) -> (bool, Vec<Result<ExecutionResult>>)
    where
        SimulateTxMsg: From<T>,
        T: Clone,
    {
        let bundle: Vec<SimulateTxMsg> = bundle
            .iter()
            .map(|x| SimulateTxMsg::from(x.clone()))
            .collect();
        let mut evm = new_evm(self.backend.clone());
        let mut results = vec![];
        for ele in bundle {
            let env = evm.context.evm.env.as_mut();
            let to = TransactTo::Call(ele.to);
            let data: Bytes = ele.data.clone();

            env.tx = TxEnv::default();
            env.tx.caller = ele.from;
            env.tx.data = data.clone();
            env.tx.value = ele.value;
            env.tx.transact_to = to.clone();
            let result = evm.transact_commit();
            results.push(result.wrap_err("simulation error"));
        }
        return (results.iter().all(|x| x.is_ok()), results);
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

pub fn new_evm(backend: SharedBackend) -> revm::Evm<'static, (), CacheDB<SharedBackend>> {
    let db = CacheDB::new(backend);
    let ctx = revm::Context::new_with_db(db);
    let evm: Evm<'static, (), CacheDB<SharedBackend>> =
        revm::Evm::new(ctx, Handler::mainnet::<CancunSpec>());
    return evm;
}

#[test]
fn test_bundle() {
    let simulator = Simulator::new("");
    let mut bundle = vec![];
    let from = Address::from_hex("").unwrap();
    for i in 0..11 {
        bundle.push(SimulateTxMsg {
            from: from,
            to: Address::from_hex("").unwrap(),
            value: U256::from_str_radix("10000000000000000", 10).unwrap(),
            data: Bytes::new(),
        });
    }
    let (success, sim_result) = simulator.simulate(&bundle);
    println!("{:?}", success);
    for ele in sim_result {
        println!("{:?}", ele);
    }
}
