//! Flashbots bundle：把两笔 CoboSafe 交易打包成一个原子 bundle，抢先 include。
//!
//! 典型应用：MEV 场景下希望候选 tx 与我们的 withdraw/swap 在同一 block 连续执行，
//! 避免被夹 / 被抢跑。Flashbots bundle 由 builder 直接打包进区块。
//!
//! 展示：
//! - `FlashbotsSender::new(auth_signer, rpc_url)`
//! - `auth_signer` 由 `AppConfigBase::resolve_flashbots_auth_signer` 提供
//!   （未配置 → 随机；配置了 → 固定，积累 relay reputation）
//! - `CoboSafeBuilder` + 多个 `TxRequest` → `send_txs(&[raw1, raw2])`
//!
//! 注意：Flashbots **仅支持主网**（chain_id = 1）。本 example 不实际发送，
//! 只展示构造 + 签名流程。
//!
//! 运行：
//!   cargo run --example flashbots_bundle -- ./config.json

use alloy::{
    primitives::{address, Address, Bytes, U256},
    providers::Provider,
    sol_types::SolCall,
};
use eyre::Result;
use serde::Deserialize;

use flashseal_rs::{
    app::{self, AppConfigBase},
    utils::{cobosafe, erc20::transferCall},
    CoboSafeBuilder, FlashbotsSender, TxBuilder, TxRequest, TxSender, TxSigner,
};

const WETH: Address = address!("C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");

#[derive(Deserialize)]
struct Config {
    #[serde(flatten)]
    base: AppConfigBase,

    recipient: Address,
    amount_wei: U256,
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
    eyre::ensure!(chain_id == 1, "Flashbots requires mainnet");

    let cobosafe_addr = config.base.require_cobosafe()?;
    let safe = cobosafe::query_safe(&provider, cobosafe_addr).await?;
    tracing::info!("Safe: {safe}");

    let signer = config.base.build_remote_signer().await?;
    let operator = signer.address();
    let nonce = provider.get_transaction_count(operator).await?;

    // 两笔 inner call，打包成**同一笔** CoboSafe.execTransactions
    // （Flashbots bundle 里也可以放多笔独立顶层 tx，这里演示单笔内 batch + 未来多笔）
    let calldata1 = transferCall {
        to: config.recipient,
        amount: config.amount_wei,
    }
    .abi_encode();

    let builder = CoboSafeBuilder::new(cobosafe_addr, chain_id);
    let gas = config.base.resolve_gas_fee(&provider).await?;

    let first = TxRequest {
        to: WETH,
        value: U256::ZERO,
        data: Bytes::from(calldata1),
        gas_limit: 100_000,
    };
    let unsigned1 = builder.build_txs(&[first], nonce, gas, gas)?;

    // 第二笔：nonce+1，内容可以不同（这里演示完全同构）
    let calldata2 = transferCall {
        to: config.recipient,
        amount: config.amount_wei,
    }
    .abi_encode();
    let second = TxRequest {
        to: WETH,
        value: U256::ZERO,
        data: Bytes::from(calldata2),
        gas_limit: 100_000,
    };
    let unsigned2 = builder.build_txs(&[second], nonce + 1, gas, gas)?;

    let raw1 = signer.sign(unsigned1.into_iter().next().unwrap()).await?;
    let raw2 = signer.sign(unsigned2.into_iter().next().unwrap()).await?;

    // 构造 Flashbots sender
    let auth_signer = config.base.resolve_flashbots_auth_signer()?;
    tracing::info!("Flashbots auth: {}", auth_signer.address());
    let flashbots = FlashbotsSender::new(auth_signer, &config.base.rpc_url)?;

    // 发 bundle。FlashbotsSender 内部对多个 builder RPC 并发广播，
    // 返回每个目标区块的 bundle hash（通常 3 个：block+1, +2, +3）。
    // 若不想公开 bundle 而是走私有 mempool，换成 `PrivateSender::new(auth_signer, rpc_url)`
    // 接口完全一致。
    let hashes = flashbots.send_txs(&[raw1, raw2]).await?;
    for (i, h) in hashes.iter().enumerate() {
        tracing::info!("block+{}: {h}", i + 1);
    }
    Ok(())
}
