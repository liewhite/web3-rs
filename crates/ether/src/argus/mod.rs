use alloy::primitives::*;
use alloy::sol_types::SolCall;
use alloy::{network::TransactionBuilder, rpc::types::TransactionRequest};

use crate::abi;
use crate::abi::argus::CallData;
use eyre::{Error, Report, Result};

pub fn build_argus_tx(argus_module: Address, tx: TransactionRequest) -> Result<TransactionRequest> {
    // to地址改为argus module
    let origin_to = tx.to.ok_or(Report::msg("argus to address is empty"))?;
    let tx = tx.with_to(argus_module);
    // 交易的value必须是0
    let origin_value = tx.value.unwrap_or(U256::ZERO);
    let tx = tx.with_value(U256::from(0));

    // data
    let origin_input = tx.input.input().map(|x| x.clone()).unwrap_or(Bytes::new());
    let data = abi::argus::IAccount::execTransactionCall {
        callData: CallData {
            flag: U256::from(0),
            to: origin_to.to().unwrap().clone(),
            value: origin_value,
            data: origin_input,
            hint: Bytes::new(),
            extra: Bytes::new(),
        },
    };
    let tx = tx.with_input(data.abi_encode());
    return Result::Ok(tx);
}

