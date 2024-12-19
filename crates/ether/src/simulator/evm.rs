use std::{error::Error, time::SystemTime};

use eyre::{self, Context};
use log::info;
use revm::{
    primitives::{Bytes, ExecutionResult, TxEnv, U256},
    Database, DatabaseCommit, Evm,
};

use super::SimulateTxMsg;

pub trait TEVM {
    fn exec_transaction_on_evm(&mut self, tx: SimulateTxMsg) -> eyre::Result<ExecutionResult>;

    fn modify_ts(&mut self, ts: u64);
    fn disable_eip3607(&mut self);
}

impl<'a, EXT, DB: Database + DatabaseCommit> TEVM for Evm<'a, EXT, DB>
where
    <DB as revm::Database>::Error: Error + Send + Sync + 'static,
{
    fn exec_transaction_on_evm(&mut self, tx: SimulateTxMsg) -> eyre::Result<ExecutionResult> {
        let start = SystemTime::now();
        let data: Bytes = tx.data.clone();
        let to = tx.to;
        let env = self.context.evm.env.as_mut();
        env.tx = TxEnv::default();
        env.tx.caller = tx.from;
        env.tx.data = data.clone();
        env.tx.value = tx.value;
        env.tx.transact_to = to.clone();
        let result = self.transact_commit();
        let success = match result.as_ref() {
            Ok(o) => o.is_success(),
            Err(e) => false,
        };
        let success = if success { "✅" } else { "❌" };
        let elapsed = start.elapsed().unwrap();
        info!(
            "{} simulation elapsed {:?} request: {:?} result: {:?}",
            success, elapsed, tx, result
        );
        result.wrap_err("simulating err")
    }

    fn modify_ts(&mut self, ts: u64) {
        self.block_mut().timestamp = U256::from(ts);
    }
    
    fn disable_eip3607(&mut self) {
        self.cfg_mut().disable_eip3607 = true;
    }
}
