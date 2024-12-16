use alloy::primitives::*;
use alloy::sol_types::{SolCall, SolValue};
use alloy::{network::TransactionBuilder, rpc::types::TransactionRequest};

use crate::abi::argus::Argus;

fn transaction_to_calldata_item(tx: TransactionRequest) -> Argus::CallData {
    let origin_to = tx.to.unwrap();
    let origin_value = tx.value.unwrap_or(U256::ZERO);

    // data
    let origin_input = tx.input.input().map(|x| x.clone()).unwrap_or(Bytes::new());
    Argus::CallData {
        flag: U256::from(0),
        to: origin_to.to().unwrap().clone(),
        value: origin_value,
        data: origin_input,
        hint: Bytes::new(),
        extra: Bytes::new(),
    }
}

pub fn build_transaction(argus_module: Address, tx: TransactionRequest) -> TransactionRequest {
    let calldata: Argus::CallData = transaction_to_calldata_item(tx.clone());
    let call = Argus::execTransactionCall { callData: calldata };
    tx.with_to(argus_module)
        .with_value(U256::from(0))
        .with_call(&call)
}

pub fn build_transactions(
    argus_module: Address,
    txs: Vec<TransactionRequest>,
) -> TransactionRequest {
    let mut calls = vec![];
    for tx in txs {
        let calldata = transaction_to_calldata_item(tx.clone());
        calls.push(calldata);
    }
    let data = Argus::execTransactionsCall {
        callDataList: calls,
    };
    TransactionRequest::default()
        .with_to(argus_module)
        .with_value(U256::from(0))
        .with_call(&data)
}
