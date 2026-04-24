//! Fork 模拟：从主网 fork 一份状态，模拟一笔 ERC20 transfer，检查余额变化，
//! 用 `AbiDecoder` 解码 Transfer event。
//!
//! 展示：
//! - `ForkSimulator::fork_for_simulation`（= fork + disable_balance_check +
//!   disable_nonce_check，一行搞定）
//! - `ForkSimulator::set_eth_balance` / `set_erc20_balance`（写 state 做 faucet）
//! - `ForkSimulator::simulate`（ephemeral）vs `simulate_and_commit`（持久）
//! - `simulator::erc20::balance`（fork 版 ERC20 balance 查询）
//! - `AbiDecoder::register_abi` + `display_result`（解码事件）
//!
//! 运行：
//!   RPC_URL=https://... cargo run --example fork_simulate

use alloy::{
    primitives::{address, Address, Bytes, TxKind, U256},
    sol_types::SolCall,
};
use eyre::Result;
use revm::context::TxEnv;

use flashseal_rs::{
    app, display_result, simulator::erc20::balance as fork_balance,
    utils::erc20::transferCall, AbiDecoder, ForkSimulator,
};

const USDT: Address = address!("dAC17F958D2ee523a2206206994597C13D831ec7");

const TRANSFER_EVENT_ABI: &str = r#"[
    {
        "anonymous": false,
        "inputs": [
            {"indexed": true, "name": "from", "type": "address"},
            {"indexed": true, "name": "to", "type": "address"},
            {"indexed": false, "name": "value", "type": "uint256"}
        ],
        "name": "Transfer",
        "type": "event"
    }
]"#;

#[tokio::main]
async fn main() -> Result<()> {
    app::init_tracing();

    let rpc_url = std::env::var("RPC_URL").expect("RPC_URL env required");

    // Fork + 默认禁用 balance/nonce check（方便跑任意 caller）
    let mut sim = ForkSimulator::fork_for_simulation(&rpc_url, None).await?;
    tracing::info!("Forked at block {:?}", sim.block_env().number);

    // 测试地址
    let from = address!("0000000000000000000000000000000000001234");
    let to = address!("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045"); // vitalik.eth

    // Faucet：给 from 充 10 ETH + 1000 USDT
    let ten_eth = U256::from(10u128).pow(U256::from(19u64));
    sim.set_eth_balance(from, ten_eth)?;
    let thousand_usdt = U256::from(1_000_000_000u64); // 1000 * 1e6
    sim.set_erc20_balance(USDT, from, thousand_usdt)?;

    let bal_before = fork_balance(&sim, USDT, from)?;
    tracing::info!("USDT bal before: {bal_before} (= 1000 USDT)");

    // 注册 Transfer event ABI 给 decoder
    let mut decoder = AbiDecoder::new();
    let abi: alloy::json_abi::JsonAbi = serde_json::from_str(TRANSFER_EVENT_ABI)?;
    decoder.register_abi(USDT, abi);

    // 模拟 transfer(to, 100 USDT)
    let amount = U256::from(100_000_000u64); // 100 * 1e6
    let calldata = transferCall { to, amount }.abi_encode();

    let tx = TxEnv {
        caller: from,
        nonce: sim.get_nonce(from)?,
        kind: TxKind::Call(USDT),
        data: Bytes::from(calldata),
        gas_limit: 100_000,
        ..Default::default()
    };

    // simulate（不 commit，仅观察）
    let result = sim.simulate(tx.clone())?;
    tracing::info!("=== simulate（ephemeral）===");
    display_result(&result, &tx, Some(&decoder));

    let bal_after_sim = fork_balance(&sim, USDT, from)?;
    tracing::info!("bal after (ephemeral): {bal_after_sim}（未变，simulate 不 commit）");
    assert_eq!(bal_after_sim, bal_before);

    // simulate_and_commit（真正落 state）
    let _ = sim.simulate_and_commit(tx)?;
    let bal_after_commit = fork_balance(&sim, USDT, from)?;
    tracing::info!("bal after (commit):    {bal_after_commit}（= 900 USDT）");
    assert_eq!(bal_after_commit, bal_before - amount);

    Ok(())
}
