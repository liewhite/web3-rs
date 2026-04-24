//! EOA 直签一笔 ETH transfer，走 public mempool（RpcSender）。
//!
//! 展示：
//! - `LocalSigner`（用 `PrivateKeySigner` 私钥）
//! - `DirectBuilder`（EOA → target，nonce/gas/chain_id 自动组装）
//! - `RpcSender`（`eth_sendRawTransaction`）
//! - `utils::decimal::parse_decimal_units`（"1.5" → wei）
//!
//! 运行：
//!   RPC_URL=https://... PRIVATE_KEY=0x... RECIPIENT=0x... AMOUNT=0.01 \
//!       cargo run --example direct_transfer

use alloy::{
    network::AnyNetwork,
    primitives::{Address, Bytes},
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
};
use eyre::Result;

use flashseal_rs::{
    app,
    utils::decimal::parse_decimal_units,
    DirectBuilder, LocalSigner, RpcSender, TxBuilder, TxRequest, TxSender, TxSigner,
};

const ETH_TRANSFER_GAS_LIMIT: u64 = 21_000;
const ETH_DECIMALS: u8 = 18;

#[tokio::main]
async fn main() -> Result<()> {
    app::init_tracing();

    let rpc_url = std::env::var("RPC_URL").expect("RPC_URL env required");
    let privkey_hex = std::env::var("PRIVATE_KEY").expect("PRIVATE_KEY env required");
    let recipient: Address = std::env::var("RECIPIENT")
        .expect("RECIPIENT env required")
        .parse()?;
    let amount_str = std::env::var("AMOUNT").unwrap_or_else(|_| "0.01".into());

    let provider = ProviderBuilder::new()
        .network::<AnyNetwork>()
        .connect_http(rpc_url.parse()?)
        .erased();
    let chain_id = provider.get_chain_id().await?;
    tracing::info!("Chain ID: {chain_id}");

    // 用本地私钥构造 signer
    let pk: PrivateKeySigner = privkey_hex.parse()?;
    let signer = LocalSigner::new(pk);
    let from = signer.address();
    tracing::info!("From:     {from}");
    tracing::info!("To:       {recipient}");

    // gas fee：拿 basefee 建议值，自己决策 × 1.5 作为 EIP-1559 的 max_fee/priority
    // （两者相等，总付 = 1.5 × base）
    let base = app::estimate_gas_fee(&provider).await?;
    let gas = base * 3 / 2;
    tracing::info!(
        "Gas:      {:.3} gwei (base {:.3} gwei × 1.5)",
        gas as f64 / 1_000_000_000.0,
        base as f64 / 1_000_000_000.0,
    );

    // 解析金额
    let value = parse_decimal_units(&amount_str, ETH_DECIMALS)?;
    tracing::info!("Value:    {amount_str} ETH ({value} wei)");

    // 取 nonce
    let nonce = provider.get_transaction_count(from).await?;

    // 构造 + 签名 + 发送
    let builder = DirectBuilder::new(chain_id);
    let req = TxRequest {
        to: recipient,
        value,
        data: Bytes::new(),
        gas_limit: ETH_TRANSFER_GAS_LIMIT,
    };
    let unsigned = builder.build_txs(&[req], nonce, gas, gas)?;
    let tx = unsigned.into_iter().next().unwrap();
    let raw = signer.sign(tx).await?;

    let sender = RpcSender::new(&rpc_url)?;
    let hashes = sender.send_txs(&[raw]).await?;
    for h in hashes {
        tracing::info!("tx hash: {h}");
    }
    Ok(())
}
