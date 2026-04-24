use alloy::{
    network::AnyNetwork,
    primitives::{keccak256, B256},
    providers::{DynProvider, Provider, ProviderBuilder},
    signers::{local::PrivateKeySigner, SignerSync},
};
use eyre::{eyre, Result};
use serde::Deserialize;

use super::{RawTx, TxSender};

/// Flashbots relay 的 eth_sendPrivateTransaction 端点。
/// Relay 收到后会把交易中继到所有合作 builder，调用方不需要自己 fan-out。
/// 参考：https://docs.flashbots.net/flashbots-auction/advanced/rpc-endpoint#eth_sendprivatetransaction
const DEFAULT_RELAY_URL: &str = "https://relay.flashbots.net";

/// `eth_sendPrivateTransaction` 默认的 maxBlockNumber 偏移（Flashbots 规范）。
const DEFAULT_MAX_BLOCK_OFFSET: u64 = 25;

/// 通过 Flashbots relay 中继的私有交易发送器。
///
/// 与 `FlashbotsSender` 的区别：relay 负责把私有交易分发给所有合作 builder，
/// 我们只需把请求发到 **一个** relay 端点（默认 `https://relay.flashbots.net`）。
///
/// 调用方须保证 `rpc_url`（用于读 block number）与 `relay_url` 在同一 chain，
/// 否则 `maxBlockNumber` 会以错误 chain 的值传给 relay。
pub struct PrivateSender {
    auth_signer: PrivateKeySigner,
    provider: DynProvider<AnyNetwork>,
    client: reqwest::Client,
    relay_url: String,
}

impl PrivateSender {
    /// 使用默认 Flashbots relay。
    pub fn new(auth_signer: PrivateKeySigner, rpc_url: &str) -> Result<Self> {
        Self::with_relay(auth_signer, rpc_url, DEFAULT_RELAY_URL)
    }

    /// 指定 relay endpoint（比如 Protect RPC 或其他 Flashbots-兼容 relay）。
    pub fn with_relay(
        auth_signer: PrivateKeySigner,
        rpc_url: &str,
        relay_url: &str,
    ) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .network::<AnyNetwork>()
            .connect_http(rpc_url.parse()?)
            .erased();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()?;
        Ok(Self {
            auth_signer,
            provider,
            client,
            relay_url: relay_url.to_string(),
        })
    }

    /// 把单笔私有交易发给 relay；返回 relay 确认的 tx hash。
    /// 任何 HTTP / JSON-RPC 错误都会返回 `Err`，不会吞掉。
    pub async fn send_private_tx(&self, tx: &RawTx, max_block_number: u64) -> Result<B256> {
        let raw_tx = format!("0x{}", alloy::hex::encode(&tx.0));
        let params = serde_json::json!({
            "tx": raw_tx,
            "maxBlockNumber": format!("0x{max_block_number:x}"),
        });
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_sendPrivateTransaction",
            "params": [params],
        });
        let body_bytes = serde_json::to_vec(&body)?;

        // Flashbots 风格签名（EIP-191 personal_sign 对 body 的 keccak256 hex）。
        let body_hash = keccak256(&body_bytes);
        let hash_hex = format!("{body_hash:#x}");
        let sig = self
            .auth_signer
            .sign_message_sync(hash_hex.as_bytes())
            .map_err(|e| eyre!("{e}"))?;
        let sig_header = format!(
            "{}:0x{}",
            self.auth_signer.address(),
            alloy::hex::encode(sig.as_bytes()),
        );

        let resp = self
            .client
            .post(&self.relay_url)
            .header("Content-Type", "application/json")
            .header("X-Flashbots-Signature", sig_header)
            .body(body_bytes)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;
        tracing::info!(
            "[flashbots-private] maxBlock=0x{:x} status={} body={}",
            max_block_number,
            status.as_u16(),
            text
        );

        if !status.is_success() {
            return Err(eyre!("relay http {}: {}", status, text));
        }

        let parsed: PrivateTxResponse = serde_json::from_str(&text)
            .map_err(|e| eyre!("decode relay response failed: {e}; body={text}"))?;
        if let Some(err) = parsed.error {
            return Err(eyre!("relay error {}: {}", err.code, err.message));
        }
        parsed
            .result
            .ok_or_else(|| eyre!("relay response missing result: {text}"))
    }
}

impl TxSender for PrivateSender {
    /// 顺序把每笔交易发给 relay，保证 nonce 递增的交易按序到达；
    /// maxBlockNumber = current + 25（Flashbots 默认窗口）。
    ///
    /// 语义与 `RpcSender` 一致：若第 k 笔失败则早退返 `Err`，前 k-1 笔已被 relay 接受
    /// 但它们的 hash 会被丢弃，调用方需根据 nonce/账户另行追踪。
    async fn send_txs(&self, txs: &[RawTx]) -> Result<Vec<B256>> {
        let block = self.provider.get_block_number().await?;
        let max_block = block + DEFAULT_MAX_BLOCK_OFFSET;
        let mut hashes = Vec::with_capacity(txs.len());
        for tx in txs {
            hashes.push(self.send_private_tx(tx, max_block).await?);
        }
        Ok(hashes)
    }
}

#[derive(Deserialize)]
struct PrivateTxResponse {
    result: Option<B256>,
    error: Option<PrivateTxError>,
}

#[derive(Deserialize)]
struct PrivateTxError {
    code: i64,
    message: String,
}
