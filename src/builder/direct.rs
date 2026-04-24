use alloy::{consensus::TxEip1559, primitives::TxKind};
use eyre::Result;

use super::{TxBuilder, TxRequest};

/// 直接构建 builder：每个 TxRequest 产出一笔独立的 EIP-1559 未签名交易
pub struct DirectBuilder {
    chain_id: u64,
}

impl DirectBuilder {
    pub fn new(chain_id: u64) -> Self {
        Self { chain_id }
    }
}

impl TxBuilder for DirectBuilder {
    fn build_txs(
        &self,
        requests: &[TxRequest],
        nonce: u64,
        max_fee_per_gas: u128,
        max_priority_fee_per_gas: u128,
    ) -> Result<Vec<TxEip1559>> {
        Ok(requests
            .iter()
            .enumerate()
            .map(|(i, req)| TxEip1559 {
                chain_id: self.chain_id,
                nonce: nonce + i as u64,
                gas_limit: req.gas_limit,
                to: TxKind::Call(req.to),
                value: req.value,
                input: req.data.clone(),
                max_fee_per_gas,
                max_priority_fee_per_gas,
                ..Default::default()
            })
            .collect())
    }
}
