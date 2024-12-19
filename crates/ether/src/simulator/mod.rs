pub mod argus;
pub mod evm;
use std::marker::PhantomData;
use std::thread::panicking;
use std::time::SystemTime;
use std::{collections::BTreeSet, sync::Arc};

use alloy::consensus::{Transaction, TxEnvelope};
use alloy::eips::eip2718::Decodable2718;
use alloy::eips::{BlockId, BlockNumberOrTag};
use alloy::network::{BlockResponse, Ethereum, HeaderResponse, Network};
use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::ProviderBuilder;
use alloy::providers::{Provider, RootProvider, WsConnect};
use alloy::rpc::types::{self, Block, TransactionRequest};
use alloy::transports::{BoxTransport, Transport};
use alloy::{hex::FromHex as _, network::TransactionBuilder};
use evm::TEVM;
use eyre::{eyre, Context, OptionExt, Result};
use foundry_evm::backend::{BlockchainDb, BlockchainDbMeta, SharedBackend};
use foundry_evm::traces::TracingInspectorConfig;
use log::{debug, info};
use revm::db::AlloyDB;
use revm::inspectors::CustomPrintTracer;
use revm::primitives::bitvec::ptr::replace;
use revm::primitives::{BlockEnv, ExecutionResult, TransactTo, TxEnv, TxKind};
use revm::{db::CacheDB, primitives::CancunSpec, Evm, Handler};
use revm::{Database, EvmBuilder, Inspector};
use revm_inspectors::tracing::TracingInspector;
use revm_trace::TxInspector;
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

pub struct Simulator {
    pub evm: Box<dyn TEVM>,
}

impl Simulator {
    pub async fn new(
        rpc: &str,
        height: Option<u64>,
        timestamp: Option<u64>,
        tracer: bool,
    ) -> Simulator {
        let block_id = match height {
            Some(h) => BlockNumberOrTag::Number(h),
            None => BlockNumberOrTag::Latest,
        };
        let evm: Box<dyn TEVM> = if rpc.starts_with("http") {
            Self::new_on_http(rpc, block_id, timestamp, tracer).await
        } else if rpc.starts_with("ws") {
            Self::new_on_ws(rpc, block_id, timestamp, tracer).await
        } else {
            panic!("bad rpc {}", rpc)
        };
        Simulator { evm: evm }
    }
    async fn new_on_ws(
        rpc: &str,
        block_id: BlockNumberOrTag,
        timestamp: Option<u64>,
        tracer: bool,
    ) -> Box<dyn TEVM> {
        let cli = ProviderBuilder::new()
            .on_ws(WsConnect::new(rpc))
            .await
            .unwrap();
        Self::create_evm_from_cli(cli, block_id, timestamp, tracer).await
    }
    async fn new_on_http(
        rpc: &str,
        block_id: BlockNumberOrTag,
        timestamp: Option<u64>,
        tracer: bool,
    ) -> Box<dyn TEVM> {
        let cli = ProviderBuilder::new().on_builtin(rpc).await.unwrap();
        Self::create_evm_from_cli(cli, block_id, timestamp, tracer).await
    }

    async fn create_evm_from_cli<T: Transport + Clone, N: Network, P: Provider<T, N> + 'static>(
        cli: P,
        block_id: BlockNumberOrTag,
        timestamp: Option<u64>,
        tracer: bool,
    ) -> Box<dyn TEVM> {
        let block_ts = cli
            .get_block_by_number(block_id, false)
            .await
            .unwrap()
            .unwrap()
            .header()
            .timestamp();

        let adb = AlloyDB::new(cli, block_id.into()).unwrap();
        let db = CacheDB::new(adb);
        let mut evm_builder = revm::Evm::builder()
            .with_db(db)
            .with_handler(Handler::mainnet::<CancunSpec>());

        let mut te: Box<dyn TEVM> = if tracer {
            Box::new(
                evm_builder
                    .with_external_context(TxInspector::new())
                    .build(),
            )
        } else {
            Box::new(evm_builder.build())
        };
        match timestamp {
            Some(ts) => te.modify_ts(ts),
            None => {
                te.modify_ts(block_ts);
            }
        }
        te.disable_eip3607();
        te
    }

    pub fn trace<T>(&self, tx: T)
    where
        SimulateTxMsg: From<T>,
        T: Clone,
    {
        // let db = CacheDB::new(AlloyDB);
        // let tracer = TracingInspector::new(TracingInspectorConfig::all());
        // let mut evm = revm::Evm::builder()
        //     .with_db(db)
        //     .with_handler(Handler::mainnet::<CancunSpec>())
        //     .with_external_context(tracer)
        //     .build();
        // evm.context.evm.env.block = self.evm.block().clone();
        // exec_transaction_on_evm(&mut evm, tx.into());
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
        let result = self.evm.exec_transaction_on_evm(msg)?;
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
        let ele: SimulateTxMsg = tx.into();
        return self.evm.exec_transaction_on_evm(ele);
    }
}