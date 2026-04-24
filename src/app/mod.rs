//! 应用脚手架：所有 bin 入口共用的配置加载、provider/signer 构造、gas 估算。
//!
//! 典型用法：
//! ```ignore
//! use flashseal_rs::app::{self, AppConfigBase};
//!
//! #[derive(serde::Deserialize)]
//! struct Config {
//!     #[serde(flatten)]
//!     base: AppConfigBase,
//!     // ... 业务特化字段
//! }
//!
//! #[tokio::main]
//! async fn main() -> eyre::Result<()> {
//!     app::init_tracing();
//!     let config: Config = app::load_json(config_path)?;
//!     let provider = config.base.build_provider()?;
//!     let signer = config.base.build_remote_signer().await?;
//!     let gas = config.base.resolve_gas_fee(&provider).await?;
//!     // ...
//! }
//! ```

use alloy::{
    eips::BlockNumberOrTag,
    network::AnyNetwork,
    primitives::Address,
    providers::{DynProvider, Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
};
use eyre::{Result, WrapErr};
use serde::Deserialize;

use crate::RemoteSigner;

/// 所有 bin 共享的基础配置。业务 Config 通过 `#[serde(flatten)] base: AppConfigBase`
/// 嵌入即可白拿下面所有 helper。
///
/// 字段划分原则：
/// - **必填**：`rpc_url` + 远程签名三件套（signer_url / signer_project / ed25519_seed）
/// - **可选**：`cobosafe_address`（Direct 签发的 bin 不填）、`gas_price_gwei`（不填走
///   estimate）、`flashbots_auth_key`（不填每次随机）
#[derive(Debug, Deserialize)]
pub struct AppConfigBase {
    pub rpc_url: String,
    pub signer_url: String,
    pub signer_project: String,
    pub ed25519_seed: String,
    #[serde(default)]
    pub cobosafe_address: Option<Address>,
    /// 手动指定 EIP-1559 max_fee / max_priority（gwei，两者相等）。
    /// None 或 0 时走 `basefee × 1.5`。
    #[serde(default)]
    pub gas_price_gwei: Option<u64>,
    /// Flashbots relay auth signer 私钥（hex）。不填则每次运行随机生成新 key
    /// —— relay reputation 从 0 起，生产环境建议配固定 key。
    #[serde(default)]
    pub flashbots_auth_key: Option<String>,
}

impl AppConfigBase {
    /// 解析 `ed25519_seed` 为 32 字节。支持可选的 `0x` 前缀。
    pub fn seed_bytes(&self) -> Result<[u8; 32]> {
        let hex = self
            .ed25519_seed
            .strip_prefix("0x")
            .unwrap_or(&self.ed25519_seed);
        let bytes = alloy::hex::decode(hex).wrap_err("ed25519_seed must be valid hex")?;
        bytes
            .try_into()
            .map_err(|v: Vec<u8>| eyre::eyre!("ed25519_seed must be 32 bytes, got {}", v.len()))
    }

    /// 构造 HTTP provider（AnyNetwork、erased 为 `DynProvider`）。
    pub fn build_provider(&self) -> Result<DynProvider<AnyNetwork>> {
        Ok(ProviderBuilder::new()
            .network::<AnyNetwork>()
            .connect_http(self.rpc_url.parse()?)
            .erased())
    }

    /// 构造 `RemoteSigner`，`account_index = 0`。
    pub async fn build_remote_signer(&self) -> Result<RemoteSigner> {
        let seed = self.seed_bytes()?;
        RemoteSigner::new(
            self.signer_url.clone(),
            self.signer_project.clone(),
            seed,
            0,
        )
        .await
    }

    /// gas 决策：`gas_price_gwei > 0` 用配置值，否则 `basefee × 1.5`。
    /// 返回单值 `fee_wei`，调用 `build_txs` / `submit_transactions` 时
    /// 把 `max_fee_wei` 和 `priority_fee_wei` 都传此值即可。
    ///
    /// `× 1.5` 的语义：设 `max_fee = priority_fee = 1.5 × base`，则
    /// `actual_fee = min(max_fee, base + priority) = 1.5 × base`。矿工 tip =
    /// `0.5 × base`，被烧 `base`。总付 `1.5 × base`。这个 buffer 覆盖了 next
    /// block basefee 最大 12.5% 涨幅，并给矿工留了合理 tip。
    ///
    /// 若需要 `max_fee ≠ priority_fee`（例如 MEV 场景想 max_fee 封顶但
    /// priority 拉很高），直接调 [`estimate_gas_fee`] 拿 basefee 自己算。
    pub async fn resolve_gas_fee(&self, provider: &DynProvider<AnyNetwork>) -> Result<u128> {
        match self.gas_price_gwei {
            Some(g) if g > 0 => {
                let wei = (g as u128) * 1_000_000_000;
                tracing::info!("Gas:        {g} gwei (from config.gas_price_gwei)");
                Ok(wei)
            }
            _ => {
                let base = estimate_gas_fee(provider).await?;
                let fee = base * 3 / 2;
                tracing::info!(
                    "Gas:        {:.3} gwei (base_fee × 1.5, base={:.3} gwei)",
                    fee as f64 / 1_000_000_000.0,
                    base as f64 / 1_000_000_000.0,
                );
                Ok(fee)
            }
        }
    }

    /// 解析 `flashbots_auth_key`，空串 / 未填则随机生成。
    pub fn resolve_flashbots_auth_signer(&self) -> Result<PrivateKeySigner> {
        let hex = self
            .flashbots_auth_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        match hex {
            Some(h) => h
                .parse::<PrivateKeySigner>()
                .wrap_err("invalid flashbots_auth_key"),
            None => {
                tracing::warn!(
                    "flashbots_auth_key 未配置，使用一次性随机 key；\
                     生产环境请配置固定 key 以累积 relay reputation。"
                );
                Ok(PrivateKeySigner::random())
            }
        }
    }

    /// 取出 `cobosafe_address`，未配置则 bail。给 CoboSafe 路径的 bin 用。
    pub fn require_cobosafe(&self) -> Result<Address> {
        self.cobosafe_address
            .ok_or_else(|| eyre::eyre!("cobosafe_address missing in config"))
    }
}

/// 统一的 tracing subscriber 初始化。默认 `info`，被 `RUST_LOG` 覆盖。
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}

/// 从 JSON 文件加载任意 deserialize 类型。业务 Config 嵌 `AppConfigBase` 后直接用此函数。
pub fn load_json<T: for<'de> Deserialize<'de>>(path: &str) -> Result<T> {
    let content = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("failed to read config file: {path}"))?;
    serde_json::from_str(&content)
        .wrap_err_with(|| format!("failed to parse config file: {path}"))
}

/// 基于 `eth_feeHistory` 返回**下一个 block 的建议 basefee**（wei / gas）。
///
/// 本函数只给出原始数据。怎么把它转成 `(max_fee, priority_fee)` 由下游按场景决策：
/// - 公共 mempool：一般 `max_fee = basefee × 1.5`（覆盖 basefee 12.5% 的最大涨幅外加 tip）
/// - MEV bundle：可能拉到 `basefee × 10+` 抢跑
/// - 私有池：只要高于 basefee 即可，优先费可设 0
///
/// 参考实现见 [`AppConfigBase::resolve_gas_fee`]。
pub async fn estimate_gas_fee(provider: &DynProvider<AnyNetwork>) -> Result<u128> {
    let fh = provider
        .get_fee_history(1, BlockNumberOrTag::Latest, &[])
        .await?;
    let next_base: u128 = *fh
        .base_fee_per_gas
        .last()
        .ok_or_else(|| eyre::eyre!("fee_history returned empty base_fee_per_gas"))?;
    Ok(next_base)
}
