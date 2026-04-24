mod flashbots;
mod private;
mod rpc;

pub use flashbots::FlashbotsSender;
pub use private::PrivateSender;
pub use rpc::RpcSender;

use std::future::Future;

use alloy::{
    consensus::TxEnvelope,
    eips::Encodable2718,
    network::AnyTxEnvelope,
    primitives::{Bytes, B256},
};
use eyre::Result;

/// RLP 编码的签名交易（raw bytes）
#[derive(Clone)]
pub struct RawTx(pub Bytes);

impl From<Bytes> for RawTx {
    fn from(b: Bytes) -> Self {
        Self(b)
    }
}

impl From<Vec<u8>> for RawTx {
    fn from(v: Vec<u8>) -> Self {
        Self(Bytes::from(v))
    }
}

impl TryFrom<&str> for RawTx {
    type Error = eyre::Error;

    fn try_from(s: &str) -> Result<Self> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        Ok(Self(Bytes::from(alloy::hex::decode(s)?)))
    }
}

impl TryFrom<String> for RawTx {
    type Error = eyre::Error;

    fn try_from(s: String) -> Result<Self> {
        Self::try_from(s.as_str())
    }
}

impl From<TxEnvelope> for RawTx {
    fn from(tx: TxEnvelope) -> Self {
        Self(Bytes::from(tx.encoded_2718()))
    }
}

impl From<AnyTxEnvelope> for RawTx {
    fn from(tx: AnyTxEnvelope) -> Self {
        Self(Bytes::from(tx.encoded_2718()))
    }
}

/// 交易发送 trait
///
/// 注意：使用 RPITIT（return-position impl Trait in trait），不支持 `dyn TxSender`。
/// 如需动态分发，请使用泛型 `impl TxSender` 或 enum dispatch。
pub trait TxSender: Send + Sync {
    fn send_txs(&self, txs: &[RawTx]) -> impl Future<Output = Result<Vec<B256>>> + Send;
}
