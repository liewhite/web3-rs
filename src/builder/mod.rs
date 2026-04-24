mod cobosafe;
mod direct;

pub use cobosafe::CoboSafeBuilder;
pub use direct::DirectBuilder;

use alloy::{
    consensus::TxEip1559,
    primitives::{Address, Bytes, U256},
};
use eyre::Result;

/// 高层交易请求，描述要执行的操作
#[derive(Debug, Clone)]
pub struct TxRequest {
    pub to: Address,
    pub value: U256,
    pub data: Bytes,
    pub gas_limit: u64,
}

/// 交易构建 trait：TxRequest → 未签名 TxEip1559
///
/// nonce / gas pricing 由调用方提供，builder 只负责构建未签名交易。
pub trait TxBuilder: Send + Sync {
    fn build_txs(
        &self,
        requests: &[TxRequest],
        nonce: u64,
        max_fee_per_gas: u128,
        max_priority_fee_per_gas: u128,
    ) -> Result<Vec<TxEip1559>>;
}
