use alloy::{
    consensus::{TxEip1559, TxLegacy},
    primitives::{Address, Bytes, TxKind, U256},
    sol,
    sol_types::SolCall,
};
use eyre::Result;

use super::{TxBuilder, TxRequest};

sol! {
    struct CallData {
        uint256 flag;
        address to;
        uint256 value;
        bytes data;
        bytes hint;
        bytes extra;
    }

    function execTransaction(CallData callData) external returns (bool, bytes, bytes);
    function execTransactions(CallData[] callDataList) external returns (bool, bytes, bytes);
}

/// CoboSafe delegate builder：将交易包装为 execTransaction(s) 调用
///
/// 单个 request 使用 `execTransaction`，多个 requests 批量打包为一笔 `execTransactions`。
/// 产出未签名的 EIP-1559 交易，由 signer 负责签名。
pub struct CoboSafeBuilder {
    cobosafe_address: Address,
    chain_id: u64,
}

impl CoboSafeBuilder {
    pub fn new(cobosafe_address: Address, chain_id: u64) -> Self {
        Self { cobosafe_address, chain_id }
    }

    /// 构建 legacy (type 0x0) 交易 —— 使用单一 gas_price，不走 EIP-1559。
    /// CoboSafe 单/多 request 逻辑与 `build_txs` 一致。
    pub fn build_legacy_tx(
        &self,
        requests: &[TxRequest],
        nonce: u64,
        gas_price: u128,
    ) -> Result<TxLegacy> {
        eyre::ensure!(!requests.is_empty(), "requests must not be empty");

        let input = if requests.len() == 1 {
            execTransactionCall { callData: to_call_data(&requests[0]) }.abi_encode()
        } else {
            let call_data_list: Vec<CallData> = requests.iter().map(to_call_data).collect();
            execTransactionsCall { callDataList: call_data_list }.abi_encode()
        };
        let gas_limit = requests.iter().map(|r| r.gas_limit).sum::<u64>();

        Ok(TxLegacy {
            chain_id: Some(self.chain_id),
            nonce,
            gas_price,
            gas_limit,
            to: TxKind::Call(self.cobosafe_address),
            value: U256::ZERO,
            input: input.into(),
        })
    }
}

/// 将 TxRequest 转换为 CoboSafe CallData
fn to_call_data(req: &TxRequest) -> CallData {
    CallData {
        // flag=0: standard CALL (vs 1=DELEGATECALL)
        flag: U256::ZERO,
        to: req.to,
        value: req.value,
        data: req.data.clone(),
        // hint/extra: unused by standard execTransaction
        hint: Bytes::new(),
        extra: Bytes::new(),
    }
}

impl TxBuilder for CoboSafeBuilder {
    fn build_txs(
        &self,
        requests: &[TxRequest],
        nonce: u64,
        max_fee_per_gas: u128,
        max_priority_fee_per_gas: u128,
    ) -> Result<Vec<TxEip1559>> {
        eyre::ensure!(!requests.is_empty(), "requests must not be empty");

        let input = if requests.len() == 1 {
            execTransactionCall { callData: to_call_data(&requests[0]) }.abi_encode()
        } else {
            let call_data_list: Vec<CallData> = requests.iter().map(to_call_data).collect();
            execTransactionsCall { callDataList: call_data_list }.abi_encode()
        };

        let gas_limit = requests.iter().map(|r| r.gas_limit).sum::<u64>();
        let tx = TxEip1559 {
            chain_id: self.chain_id,
            nonce,
            gas_limit,
            to: TxKind::Call(self.cobosafe_address),
            value: U256::ZERO,
            input: input.into(),
            max_fee_per_gas,
            max_priority_fee_per_gas,
            ..Default::default()
        };

        Ok(vec![tx])
    }
}
