//! 完整的 bin 骨架：config → provider → signer → gas → submit 一条龙。
//!
//! 场景：通过 CoboSafe delegate → Safe → ERC20.transfer(to, amount)，把 Safe 持有的
//! 任意 ERC20 token 转到 recipient。生产路径一致（经 CoboSafe.execTransactions），
//! ACL 需要事先授权 Safe 对 token 的 transfer 白名单。
//!
//! 展示：
//! - `app::AppConfigBase`（rpc_url / signer_url / signer_project / ed25519_seed /
//!   cobosafe_address / gas_price_gwei / flashbots_auth_key） + `#[serde(flatten)]`
//! - `app::init_tracing` / `app::load_json`
//! - `utils::cobosafe::query_safe` / `submit_transactions`
//! - `utils::erc20` sol 接口 + RPC 查询
//! - `RemoteSigner` + `RpcSender`
//!
//! config.json 示例：
//! ```json
//! {
//!   "rpc_url": "https://...",
//!   "signer_url": "https://signer.example.com",
//!   "signer_project": "my-project",
//!   "ed25519_seed": "0xabc...",
//!   "cobosafe_address": "0x...",
//!   "gas_price_gwei": null,
//!   "flashbots_auth_key": null,
//!   "token": "0xdAC17F958D2ee523a2206206994597C13D831ec7",
//!   "recipient": "0x...",
//!   "amount_human": "100.5"
//! }
//! ```
//!
//! 运行：
//!   cargo run --example cobosafe_submit -- ./config.json

use alloy::{
    primitives::{Address, Bytes, U256},
    providers::Provider,
    sol_types::SolCall,
};
use eyre::Result;
use serde::Deserialize;

use flashseal_rs::{
    app::{self, AppConfigBase},
    utils::{
        cobosafe,
        decimal::{parse_decimal_units, raw_to_human},
        erc20::{self, transferCall},
    },
    RpcSender, TxRequest, TxSigner,
};

#[derive(Deserialize)]
struct Config {
    #[serde(flatten)]
    base: AppConfigBase,

    /// ERC20 合约地址
    token: Address,
    /// 接收方（Safe 转给他）
    recipient: Address,
    /// human 单位数量，按 token.decimals 解析
    amount_human: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    app::init_tracing();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.json".into());
    let config: Config = app::load_json(&config_path)?;

    let provider = config.base.build_provider()?;
    let chain_id = provider.get_chain_id().await?;
    tracing::info!("Config:   {config_path}");
    tracing::info!("Chain ID: {chain_id}");

    let cobosafe_addr = config.base.require_cobosafe()?;
    let safe = cobosafe::query_safe(&provider, cobosafe_addr).await?;
    tracing::info!("CoboSafe: {cobosafe_addr}");
    tracing::info!("Safe:     {safe}");

    let signer = config.base.build_remote_signer().await?;
    tracing::info!("Operator: {}", signer.address());

    // 查 token decimals 并解析 amount
    let token_decimals = erc20::decimals(&provider, config.token).await?;
    let amount_raw = parse_decimal_units(&config.amount_human, token_decimals)?;
    tracing::info!(
        "Amount:   {} (10^{token_decimals}, raw {amount_raw})",
        config.amount_human
    );

    // 查 Safe 当前余额做 sanity check
    let safe_bal = erc20::balance(&provider, config.token, safe).await?;
    eyre::ensure!(
        safe_bal >= amount_raw,
        "Safe {safe} balance {safe_bal} < {amount_raw}"
    );
    tracing::info!(
        "Safe bal: {:.4} ({safe_bal} raw)",
        raw_to_human(safe_bal, token_decimals)
    );

    // 构造 inner call：Safe → ERC20.transfer(recipient, amount)
    let calldata = transferCall {
        to: config.recipient,
        amount: amount_raw,
    }
    .abi_encode();
    let inner = TxRequest {
        to: config.token,
        value: U256::ZERO,
        data: Bytes::from(calldata),
        gas_limit: 100_000,
    };

    // gas + 通过 RpcSender 发送（max_fee == priority_fee == gas）
    let gas = config.base.resolve_gas_fee(&provider).await?;
    let sender = RpcSender::new(&config.base.rpc_url)?;

    cobosafe::submit_transactions(
        &provider,
        &signer,
        cobosafe_addr,
        &sender,
        &[inner],
        gas,
        gas,
    )
    .await?;

    Ok(())
}
