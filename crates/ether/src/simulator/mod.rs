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
use foundry_evm::traces::TracingInspectorConfig;
use log::{debug, info};
use revm::db::AlloyDB;
use revm::inspectors::CustomPrintTracer;
use revm::primitives::bitvec::ptr::replace;
use revm::primitives::{BlockEnv, ExecutionResult, TransactTo, TxEnv, TxKind};
use revm::{db::CacheDB, primitives::CancunSpec, Evm, Handler};
use revm::{Database, Inspector};
use revm_inspectors::tracing::TracingInspector;
use std::str::FromStr;

use crate::abi::argus::Argus::rescueCall;

#[derive(Debug, Clone)]
pub struct SimulateTxMsg {
    pub from: Address,
    pub to: TxKind,
    pub value: U256,
    pub data: Bytes,
}

impl From<TransactionRequest> for SimulateTxMsg {
    fn from(x: TransactionRequest) -> Self {
        SimulateTxMsg {
            from: x.from.unwrap(),
            to: x.to.unwrap(),
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
            to: decoded.kind(),
            value: decoded.value(),
            data: decoded.input().clone(),
        }
    }
}
impl From<TxEnvelope> for SimulateTxMsg {
    fn from(decoded: TxEnvelope) -> Self {
        SimulateTxMsg {
            from: decoded.recover_signer().unwrap(),
            to: decoded.kind(),
            value: decoded.value(),
            data: decoded.input().clone(),
        }
    }
}

impl From<types::Transaction> for SimulateTxMsg {
    fn from(tx: types::Transaction) -> Self {
        SimulateTxMsg {
            from: tx.from,
            // to 为空则是部署合约
            to: tx.to.map(|t| TxKind::Call(t)).unwrap_or(TxKind::Create),
            value: tx.value,
            data: tx.input,
        }
    }
}

#[derive(Debug, Default)]
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
        if self._height.is_some() {
            simulator_backend
                .set_pinned_block(self._height.unwrap())
                .unwrap();
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
        return SimulatorBuilder::default();
    }
    pub fn new(backend: SharedBackend, block_env: BlockEnv) -> Simulator {
        let db = CacheDB::new(backend.clone());
        let mut evm = revm::Evm::builder()
            .with_db(db)
            .with_handler(Handler::mainnet::<CancunSpec>())
            .build();
        evm.context.evm.env.block = block_env;
        Simulator { evm: evm }
    }

    pub fn trace<T>(&self, tx: T)
    where
        SimulateTxMsg: From<T>,
        T: Clone,
    {
        let db = self.evm.db().clone();
        let tracer = TracingInspector::new(TracingInspectorConfig::all());
        let mut evm = revm::Evm::builder()
            .with_db(db)
            .with_handler(Handler::mainnet::<CancunSpec>())
            .with_external_context(tracer)
            .build();
        // let a = db;
        evm.context.evm.env.block = self.evm.block().clone();
        Self::exec_transaction_on_evm(&mut evm, tx.into());
        // evm.context.external

        // Simulator { evm: evm }
    }

    pub fn deploy_contract(&mut self, from: Address, bytecode: Bytes) -> Result<Address> {
        let msg = SimulateTxMsg {
            from,
            to: TxKind::Create,
            value: U256::ZERO,
            data: bytecode,
        };
        let result = Self::exec_transaction_on_evm(&mut self.evm, msg)?;
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

    fn exec_transaction_on_evm<'a, T>(
        _evm: &mut Evm<'a, T, CacheDB<SharedBackend>>,
        tx: SimulateTxMsg,
    ) -> Result<ExecutionResult> {
        let start = SystemTime::now();
        let data: Bytes = tx.data.clone();
        let to = tx.to;
        let env = _evm.context.evm.env.as_mut();
        env.tx = TxEnv::default();
        env.tx.caller = tx.from;
        env.tx.data = data.clone();
        env.tx.value = tx.value;
        env.tx.transact_to = to.clone();
        let result = _evm.transact_commit();
        let success = if result.is_ok() && result.as_ref().unwrap().is_success() {
            "✅"
        } else {
            "❌"
        };
        let elapsed = start.elapsed().unwrap();
        info!(
            "{} simulation elapsed {:?} request: {:?} result: {:?}",
            success, elapsed, tx, result
        );
        result.wrap_err("simulating error")
    }
    pub fn exec_transaction<T>(&mut self, tx: T) -> Result<ExecutionResult>
    where
        SimulateTxMsg: From<T>,
        T: Clone,
    {
        let ele: SimulateTxMsg = tx.into();
        return Self::exec_transaction_on_evm(&mut self.evm, ele);
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
