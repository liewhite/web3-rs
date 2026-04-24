use alloy::{
    consensus::{SignableTransaction, TxEip1559, TxEnvelope},
    primitives::Address,
    signers::{local::PrivateKeySigner, SignerSync},
};
use eyre::Result;

use super::TxSigner;
use crate::RawTx;

/// 本地私钥签名器：使用 PrivateKeySigner 对未签名交易进行签名
pub struct LocalSigner {
    signer: PrivateKeySigner,
}

impl LocalSigner {
    pub fn new(signer: PrivateKeySigner) -> Self {
        Self { signer }
    }
}

impl TxSigner for LocalSigner {
    fn address(&self) -> Address {
        self.signer.address()
    }

    async fn sign(&self, tx: TxEip1559) -> Result<RawTx> {
        let sig = self.signer.sign_hash_sync(&tx.signature_hash())?;
        let envelope = TxEnvelope::Eip1559(tx.into_signed(sig));
        Ok(RawTx::from(envelope))
    }
}
