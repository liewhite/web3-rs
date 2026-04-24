mod local;
mod remote;

pub use local::LocalSigner;
pub use remote::RemoteSigner;

use std::future::Future;

use alloy::{consensus::TxEip1559, primitives::Address};
use eyre::Result;

use crate::RawTx;

/// 交易签名 trait：未签名 TxEip1559 → 签名后的 RawTx
///
/// 注意：使用 RPITIT，不支持 `dyn TxSigner`。
pub trait TxSigner: Send + Sync {
    fn address(&self) -> Address;
    fn sign(&self, tx: TxEip1559) -> impl Future<Output = Result<RawTx>> + Send;
}
