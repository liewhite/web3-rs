use alloy::{
    consensus::{TxEip1559, TxLegacy},
    primitives::{Address, TxKind},
};
use ed25519_dalek::{Signer as _, SigningKey};
use eyre::Result;
use reqwest::Client;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::TxSigner;
use crate::RawTx;

/// 远程签名器：通过 HTTP 调用签名服务获取 address 和签名
///
/// 认证方式：Ed25519 签名 SHA256(timestamp + data)
pub struct RemoteSigner {
    base_url: String,
    project: String,
    signing_key: SigningKey,
    public_key_hex: String,
    account: Address,
    client: Client,
}

#[derive(Serialize)]
struct AuthRequest {
    project: String,
    signature: String,
    public_key: String,
    timestamp: i64,
    data: String,
}

impl RemoteSigner {
    pub async fn new(
        base_url: String,
        project: String,
        ed25519_seed: [u8; 32],
        account_index: i64,
    ) -> Result<Self> {
        let signing_key = SigningKey::from_bytes(&ed25519_seed);
        let public_key_hex = alloy::hex::encode(signing_key.verifying_key().as_bytes());
        let client = Client::new();

        let mut signer = Self {
            base_url,
            project,
            signing_key,
            public_key_hex,
            account: Address::ZERO,
            client,
        };

        signer.account = signer.fetch_address(account_index).await?;
        Ok(signer)
    }

    fn build_auth_request(&self, data: &str) -> AuthRequest {
        // system clock always >= UNIX_EPOCH
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_secs() as i64;

        let content = format!("{timestamp}{data}");
        let hash = Sha256::digest(content.as_bytes());
        let signature = self.signing_key.sign(&hash);

        AuthRequest {
            project: self.project.clone(),
            signature: format!("0x{}", alloy::hex::encode(signature.to_bytes())),
            public_key: self.public_key_hex.clone(),
            timestamp,
            data: data.to_string(),
        }
    }

    /// 发送认证 POST 请求并解析响应
    async fn post<T: serde::de::DeserializeOwned>(&self, path: &str, data: &str) -> Result<T> {
        let req = self.build_auth_request(data);
        let resp = self.client
            .post(format!("{}{path}", self.base_url))
            .json(&req)
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;

        if !status.is_success() {
            let msg = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["msg"].as_str().map(String::from))
                .unwrap_or(body);
            return Err(eyre::eyre!("signer service error ({status}): {msg}"));
        }

        Ok(serde_json::from_str(&body)?)
    }

    async fn fetch_address(&self, index: i64) -> Result<Address> {
        let data = serde_json::json!({ "index": index }).to_string();

        #[derive(serde::Deserialize)]
        struct Resp {
            data: String,
        }

        let resp: Resp = self.post("/v1/address", &data).await?;
        Ok(resp.data.parse()?)
    }

    /// 签名 legacy (type 0x0) 交易，使用 `gasPrice` 字段替代 maxFee/priority。
    pub async fn sign_legacy(&self, tx: TxLegacy) -> Result<RawTx> {
        let to = match tx.to {
            TxKind::Call(addr) => addr.to_string(),
            TxKind::Create => {
                return Err(eyre::eyre!("RemoteSigner does not support contract creation"))
            }
        };
        let chain_id = tx.chain_id.unwrap_or(0);
        let go_tx = serde_json::json!({
            "chainId": format!("0x{:x}", chain_id),
            "type": "0x0",
            "nonce": format!("0x{:x}", tx.nonce),
            "to": to,
            "value": format!("0x{:x}", tx.value),
            "gas": format!("0x{:x}", tx.gas_limit),
            "gasPrice": format!("0x{:x}", tx.gas_price),
            "input": format!("0x{}", alloy::hex::encode(&tx.input)),
            "hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        });
        let data = serde_json::json!({
            "chain_id": chain_id,
            "account": self.account.to_string(),
            "transaction": go_tx.to_string(),
        })
        .to_string();

        #[derive(serde::Deserialize)]
        struct Resp {
            tx_hex: String,
        }

        let resp: Resp = self.post("/v1/sign/transaction", &data).await?;
        RawTx::try_from(resp.tx_hex.as_str())
    }
}

impl TxSigner for RemoteSigner {
    fn address(&self) -> Address {
        self.account
    }

    async fn sign(&self, tx: TxEip1559) -> Result<RawTx> {
        let to = match tx.to {
            TxKind::Call(addr) => addr.to_string(),
            TxKind::Create => {
                return Err(eyre::eyre!("RemoteSigner does not support contract creation"))
            }
        };

        // 转换为签名服务期望的 Go 风格交易 JSON（所有数值为 hex 字符串）
        let go_tx = serde_json::json!({
            "chainId": format!("0x{:x}", tx.chain_id),
            "type": "0x2",
            "nonce": format!("0x{:x}", tx.nonce),
            "to": to,
            "value": format!("0x{:x}", tx.value),
            "gas": format!("0x{:x}", tx.gas_limit),
            "maxPriorityFeePerGas": format!("0x{:x}", tx.max_priority_fee_per_gas),
            "maxFeePerGas": format!("0x{:x}", tx.max_fee_per_gas),
            "input": format!("0x{}", alloy::hex::encode(&tx.input)),
            // Go 端要求的占位哈希
            "hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "accessList": [],
        });

        let data = serde_json::json!({
            "chain_id": tx.chain_id,
            "account": self.account.to_string(),
            "transaction": go_tx.to_string(),
        })
        .to_string();

        #[derive(serde::Deserialize)]
        struct Resp {
            tx_hex: String,
        }

        let resp: Resp = self.post("/v1/sign/transaction", &data).await?;
        RawTx::try_from(resp.tx_hex.as_str())
    }
}
