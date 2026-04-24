use std::sync::Arc;

use alloy::{
    network::AnyNetwork,
    primitives::{keccak256, B256},
    providers::{DynProvider, Provider, ProviderBuilder},
    signers::{local::PrivateKeySigner, SignerSync},
};
use eyre::Result;
use futures::future::join_all;
use serde::Deserialize;

use super::{RawTx, TxSender};

/// 硬编码 builder 列表（name, rpc url）。来源：Flashbots dowg builder registry。
/// 没带 scheme 的 URL 会在 send_bundle 里自动补 https://。
const BUILDERS: &[(&str, &str)] = &[
    ("flashbots", "rpc.flashbots.net"),
    ("f1b.io", "https://rpc.f1b.io"),
    ("rsync", "rsync-builder.xyz"),
    ("beaverbuild.org", "mevshare-rpc.beaverbuild.org"),
    ("builder0x69", "builder0x69.io"),
    ("Titan", "rpc.titanbuilder.xyz"),
    ("EigenPhi", "builder.eigenphi.io"),
    ("boba-builder", "boba-builder.com/searcher/bundle"),
    ("Gambit Labs", "https://builder.gmbit.co/rpc"),
    ("payload", "rpc.payload.de"),
    ("Loki", "rpc.lokibuilder.xyz"),
    ("BuildAI", "https://buildai.net"),
    ("JetBuilder", "rpc.mevshare.jetbldr.xyz"),
    ("tbuilder", "flashbots.rpc.tbuilder.xyz"),
    ("penguinbuild", "rpc.penguinbuild.org"),
    ("bobthebuilder", "rpc.bobthebuilder.xyz"),
    ("BTCS", "flashbots.btcs.com"),
    ("bloXroute", "rpc-builder.blxrbdn.com"),
    ("Blockbeelder", "https://blockbeelder.com/rpc"),
    ("Quasar", "rpc.quasar.win"),
    ("Eureka", "rpc.eurekabuilder.xyz"),
];

/// 并发将 bundle 发送到所有 Flashbots-兼容 builder 的 RPC。不通过单一 relay 做
/// `builders` 过滤；每笔 bundle 直接打到上面 const 里的每个 builder。
pub struct FlashbotsSender {
    auth_signer: PrivateKeySigner,
    provider: DynProvider<AnyNetwork>,
    client: reqwest::Client,
}

impl FlashbotsSender {
    pub fn new(auth_signer: PrivateKeySigner, rpc_url: &str) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .network::<AnyNetwork>()
            .connect_http(rpc_url.parse()?)
            .erased();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(1))
            .build()?;
        Ok(Self { auth_signer, provider, client })
    }

    /// 对指定目标区块的 bundle 并发打到所有 builder；每个响应都打印出来，
    /// 返回第一个成功解析到 bundleHash 的结果（仅用于 TxSender trait 兼容）。
    pub async fn send_bundle(&self, txs: &[RawTx], target_block: u64) -> Result<B256> {
        let raw_txs: Vec<String> = txs
            .iter()
            .map(|tx| format!("0x{}", alloy::hex::encode(&tx.0)))
            .collect();

        // 注意：不带 `builders` 字段。
        let params = serde_json::json!({
            "txs": raw_txs,
            "blockNumber": format!("0x{target_block:x}"),
        });
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_sendBundle",
            "params": [params]
        });
        let body_bytes = serde_json::to_vec(&body)?;

        // Flashbots 风格签名（EIP-191 personal_sign）；大部分 builder 都认这个 header，
        // 不认的会忽略。
        let body_hash = keccak256(&body_bytes);
        let hash_hex = format!("{body_hash:#x}");
        let sig = self
            .auth_signer
            .sign_message_sync(hash_hex.as_bytes())
            .map_err(|e| eyre::eyre!("{e}"))?;
        let sig_header = format!(
            "{}:0x{}",
            self.auth_signer.address(),
            alloy::hex::encode(sig.as_bytes()),
        );

        let body_bytes = Arc::new(body_bytes);
        let sig_header = Arc::new(sig_header);

        let futures = BUILDERS.iter().map(|(name, raw_url)| {
            let client = self.client.clone();
            let url = normalize_url(raw_url);
            let name = *name;
            let body = body_bytes.clone();
            let sig = sig_header.clone();
            async move {
                let resp = client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .header("X-Flashbots-Signature", sig.as_str())
                    .body((*body).clone())
                    .send()
                    .await;
                match resp {
                    Ok(r) => {
                        let status = r.status().as_u16();
                        let text = r
                            .text()
                            .await
                            .unwrap_or_else(|e| format!("<body read error: {e}>"));
                        (name, status, text)
                    }
                    Err(e) => (name, 0, format!("<http error: {e}>")),
                }
            }
        });

        let results = join_all(futures).await;
        let mut first_hash: Option<B256> = None;
        for (name, status, text) in &results {
            tracing::info!(
                "[builder:{name}] block=0x{:x} status={} body={}",
                target_block, status, text
            );
            if first_hash.is_none() && *status == 200 {
                if let Ok(parsed) = serde_json::from_str::<FlashbotsResponse>(text) {
                    if let Some(r) = parsed.result {
                        first_hash = Some(r.bundle_hash);
                    }
                }
            }
        }

        Ok(first_hash.unwrap_or_default())
    }
}

fn normalize_url(raw: &str) -> String {
    if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_string()
    } else {
        format!("https://{raw}")
    }
}

impl TxSender for FlashbotsSender {
    /// 并发发送 bundle 到 current_block + 1 ~ current_block + 3，每个 target block 内部
    /// 再 fan-out 到所有 builder。
    async fn send_txs(&self, txs: &[RawTx]) -> Result<Vec<B256>> {
        let block = self.provider.get_block_number().await?;
        let futures = (1..=3u64).map(|offset| self.send_bundle(txs, block + offset));
        join_all(futures).await.into_iter().collect()
    }
}

#[derive(Deserialize)]
struct FlashbotsResponse {
    result: Option<BundleResult>,
}

#[derive(Deserialize)]
struct BundleResult {
    #[serde(rename = "bundleHash")]
    bundle_hash: B256,
}
