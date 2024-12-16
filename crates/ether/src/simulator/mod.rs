pub mod argus;
use std::time::SystemTime;
use std::{collections::BTreeSet, sync::Arc};

use alloy::consensus::{Transaction, TxEnvelope};
use alloy::eips::eip2718::Decodable2718;
use alloy::eips::BlockId;
use alloy::primitives::{Address, Bytes, U256};
use alloy::rpc::types::{self, Block, TransactionRequest};
use alloy::{hex::FromHex as _, network::TransactionBuilder};
use eyre::{eyre, Context, OptionExt, Result};
use foundry_common::provider::ProviderBuilder;
use foundry_evm::backend::{BlockchainDb, BlockchainDbMeta, SharedBackend};
use log::{debug, info};
use revm::inspectors::CustomPrintTracer;
use revm::primitives::bitvec::ptr::replace;
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
            value: x.value.unwrap_or(U256::from(0_u32)),
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

#[derive(Debug,Default)]
pub struct SimulatorBuilder {
    _rpc: String,
    _height: Option<u64>,
    _timestamp: Option<u64>,
}

impl SimulatorBuilder {
    pub fn build(self) -> Simulator {
        let mut block_env = BlockEnv::default();
        if self._timestamp.is_some() {
            block_env.timestamp = U256::from(self._timestamp.unwrap());
        }
        let simulator_backend = shared_backend(&self._rpc);
        if self._height.is_some(){
            simulator_backend.set_pinned_block(self._height.unwrap()).unwrap();
        }
        let mut sim = Simulator::new(simulator_backend, block_env);
        sim.evm.cfg_mut().disable_eip3607 = true;
        return sim;
    }
    pub fn rpc(mut self, url: &str) -> Self {
        self._rpc = url.to_string();
        return self;
    }
    pub fn height(mut self, height: u64) -> Self {
        self._height = Some(height);
        return self;
    }
    pub fn _timestamp(mut self, ts: u64) -> Self {
        self._timestamp = Some(ts);
        return self;
    }
}

#[derive(Debug)]
pub struct Simulator {
    // pub backend: SharedBackend,
    pub evm: Evm<'static, (), CacheDB<SharedBackend>>,
}

impl Simulator {
    pub fn builder() -> SimulatorBuilder {
        return SimulatorBuilder::default()
    }
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
    pub fn deploy_contract(&mut self, from: Address, bytecode: Bytes) -> Result<Address> {
        let start = SystemTime::now();
        let env = self.evm.context.evm.env.as_mut();
        env.tx = TxEnv::default();
        env.tx.caller = from;
        env.tx.data = bytecode;
        env.tx.transact_to = TransactTo::Create;
        let result = self.evm.transact_commit();
        let success = if result.is_ok() && result.as_ref().unwrap().is_success() {
            "✅"
        } else {
            "❌"
        };
        let elapsed = start.elapsed().unwrap();
        info!(
            "{} depoly contract elapsed {:?} result: {:?}",
            success, elapsed, result
        );
        let result = result.wrap_err("deploy error")?;
        let addr: Result<Address> = match result {
            ExecutionResult::Success {
                reason,
                gas_used,
                gas_refunded,
                logs,
                output,
            } => output
                .address()
                .map(|x| x.clone())
                .ok_or_eyre("deploy failed"),
            ExecutionResult::Revert { gas_used, output } => {
                eyre::Result::Err(eyre!("deploy failed {} {:?}", gas_used, output))
            }
            ExecutionResult::Halt { reason, gas_used } => {
                eyre::Result::Err(eyre!("deploy out of gas {:?} {}", reason, gas_used))
            }
        };
        addr
    }

    pub fn exec_transaction<T>(&mut self, tx: T) -> Result<ExecutionResult>
    where
        SimulateTxMsg: From<T>,
        T: Clone,
    {
        let start = SystemTime::now();
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
        let success = if result.is_ok() && result.as_ref().unwrap().is_success() {
            "✅"
        } else {
            "❌"
        };
        let elapsed = start.elapsed().unwrap();
        info!(
            "{} simulation elapsed {:?} request: {:?} result: {:?}",
            success, elapsed, ele, result
        );
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