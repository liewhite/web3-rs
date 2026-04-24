use alloy::{
    network::AnyNetwork,
    primitives::B256,
    providers::{DynProvider, Provider, ProviderBuilder},
};
use eyre::Result;

use super::{RawTx, TxSender};

/// 通过 RPC 逐笔广播签名交易
pub struct RpcSender {
    provider: DynProvider<AnyNetwork>,
}

impl RpcSender {
    pub fn new(rpc_url: &str) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .network::<AnyNetwork>()
            .connect_http(rpc_url.parse()?)
            .erased();
        Ok(Self { provider })
    }
}

impl TxSender for RpcSender {
    /// 按顺序逐笔广播，保证 nonce 递增的交易按序提交。
    async fn send_txs(&self, txs: &[RawTx]) -> Result<Vec<B256>> {
        let mut hashes = Vec::with_capacity(txs.len());
        for tx in txs {
            let pending = self.provider.send_raw_transaction(&tx.0).await?;
            hashes.push(*pending.tx_hash());
        }
        Ok(hashes)
    }
}
