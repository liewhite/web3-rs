//! 本地 fork 端到端测试：从空白 CoboSafe 到 delegate 成功执行一笔业务 call。
//!
//! 覆盖生产 CoboSafe 调用链的每一个配置环节：
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────────────┐
//! │  1. Fork 主网到 latest（fork_for_simulation 顺手关掉 balance/nonce 检查） │
//! │  2. 读 CoboSafe 当前 owner / safe（get_owner / get_safe）                │
//! │  3. fund ETH 给 owner / safe / delegate（set_eth_balance）               │
//! │  4. 从 Foundry artifact 读 runtime bytecode + ABI，set_code 部署 ACL    │
//! │  5. owner → CoboSafe.setAuthorizer(mock_acl)                           │
//! │  6. owner → CoboSafe.addDelegate(test_delegate)                        │
//! │  7. 检查 + (必要时) Safe.enableModule(CoboSafe) —— Safe 要 self-call    │
//! │  8. delegate → CoboSafe.execTransactions([Safe → WETH.deposit(1 ETH)]) │
//! │     用 AbiDecoder + display_result 打印可读的 calldata / events         │
//! │  9. 断言 Safe 的 WETH 余额 +1 ETH                                       │
//! └────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! 生产迁移：把 `setAuthorizer(mock_acl)` 换成真实已部署的 ACL 地址，
//! 其余步骤完全一致。
//!
//! # 前置：准备 ACL 合约的 Foundry artifact
//!
//! 自己搭一个最小 Foundry 项目，把下面 Solidity 源码放进去 `forge build` 即可。
//! Foundry 会在 `out/<source>.sol/<contract>.json` 产出含 `abi` / `bytecode` /
//! `deployedBytecode` 的 artifact，本 example 只吃 `deployedBytecode.object`
//! （runtime bytecode，set_code 用的就是这个）和 `abi`。
//!
//! ```bash
//! forge init mock-acl --no-git
//! cd mock-acl
//! cat > src/MockAuthorizer.sol <<'EOF'
//! // SPDX-License-Identifier: MIT
//! pragma solidity ^0.8.20;
//!
//! /// 对任意 calldata（Cobo ACL 的 preExecCheck / postExecCheck 都走 fallback）
//! /// 返回 AuthorizerReturnData{ result: SUCCESS, message: "", data: "" }。
//! contract MockAuthorizer {
//!     enum AuthResult { FAILED, SUCCESS }
//!     struct AuthorizerReturnData { AuthResult result; string message; bytes data; }
//!
//!     fallback() external payable {
//!         AuthorizerReturnData memory ret = AuthorizerReturnData({
//!             result: AuthResult.SUCCESS, message: "", data: ""
//!         });
//!         bytes memory encoded = abi.encode(ret);
//!         assembly { return(add(encoded, 32), mload(encoded)) }
//!     }
//! }
//! EOF
//! forge build
//! ```
//!
//! 运行：
//!   AUTHORIZER_ARTIFACT=./mock-acl/out/MockAuthorizer.sol/MockAuthorizer.json \
//!   RPC_URL=https://... COBOSAFE=0x... \
//!     cargo run --example cobosafe_fork_e2e

use alloy::{
    json_abi::JsonAbi,
    primitives::{address, Address, Bytes, TxKind, U256},
    sol,
    sol_types::SolCall,
};
use eyre::{Result, WrapErr};
use revm::context::TxEnv;

use flashseal_rs::{
    app, display_result, simulator::erc20::balance as fork_erc20_balance,
    utils::cobosafe, AbiDecoder, CoboSafeBuilder, ForkSimulator, TxBuilder, TxRequest,
};

// 主网地址
const WETH: Address = address!("C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");
// 本 example 使用的固定测试地址
const MOCK_ACL: Address = address!("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
const TEST_DELEGATE: Address = address!("d00dd00dd00dd00dd00dd00dd00dd00dd00dd00d");

// 主网 chain_id。CoboSafeBuilder 用它构造 EIP-1559 tx 的 chain_id 字段。
const CHAIN_ID: u64 = 1;

sol! {
    function deposit() external payable;
    function isModuleEnabled(address module) external view returns (bool);
    function enableModule(address module) external;
}

/// WETH ABI — 给 `AbiDecoder` 注册用。`display_result` 会按注册的 ABI 把
/// Deposit / Transfer event 解成 `Deposit(dst: 0x..., wad: ...)` 的可读形式。
const WETH_ABI: &str = r#"[
    { "inputs": [], "name": "deposit", "outputs": [], "stateMutability": "payable", "type": "function" },
    { "inputs": [{"name":"guy","type":"address"},{"name":"wad","type":"uint256"}], "name": "approve", "outputs": [{"type":"bool"}], "stateMutability": "nonpayable", "type": "function" },
    { "inputs": [{"name":"dst","type":"address"},{"name":"wad","type":"uint256"}], "name": "transfer", "outputs": [{"type":"bool"}], "stateMutability": "nonpayable", "type": "function" },
    {
        "anonymous": false,
        "inputs": [
            { "indexed": true, "name": "dst", "type": "address" },
            { "indexed": false, "name": "wad", "type": "uint256" }
        ],
        "name": "Deposit",
        "type": "event"
    },
    {
        "anonymous": false,
        "inputs": [
            { "indexed": true, "name": "src", "type": "address" },
            { "indexed": true, "name": "dst", "type": "address" },
            { "indexed": false, "name": "wad", "type": "uint256" }
        ],
        "name": "Transfer",
        "type": "event"
    }
]"#;

#[tokio::main]
async fn main() -> Result<()> {
    app::init_tracing();

    let rpc_url = std::env::var("RPC_URL").expect("RPC_URL env required");
    let cobosafe_addr: Address = std::env::var("COBOSAFE")
        .expect("COBOSAFE env required (mainnet CoboSafe 合约地址)")
        .parse()?;
    let artifact_path = std::env::var("AUTHORIZER_ARTIFACT")
        .expect("AUTHORIZER_ARTIFACT env required (Foundry 产物 json 路径)");

    // ─── 1. Fork ───
    let mut sim = ForkSimulator::fork_for_simulation(&rpc_url, None).await?;
    tracing::info!("Forked at block {:?}", sim.block_env().number);

    // ─── 2. 读 CoboSafe 当前 owner 和 Safe 地址 ───
    let owner = cobosafe::get_owner(&sim, cobosafe_addr)?;
    let safe = cobosafe::get_safe(&sim, cobosafe_addr)?;
    tracing::info!("CoboSafe: {cobosafe_addr}");
    tracing::info!("  owner:  {owner}");
    tracing::info!("  safe:   {safe}");

    // ─── 3. fund ETH ───
    let one_eth = U256::from(10u64).pow(U256::from(18u64));
    let ten_eth = one_eth * U256::from(10u64);
    sim.set_eth_balance(owner, ten_eth)?;
    sim.set_eth_balance(TEST_DELEGATE, ten_eth)?;
    sim.set_eth_balance(safe, ten_eth)?; // Safe 要 ETH 做 WETH.deposit

    // ─── 4. 从 Foundry artifact 读 bytecode + ABI，部署到 MOCK_ACL 地址 ───
    let (acl_bytecode, acl_abi) = load_foundry_artifact(&artifact_path)?;
    tracing::info!(
        "Loaded artifact ({}): {} bytes runtime bytecode",
        artifact_path,
        acl_bytecode.len()
    );
    sim.set_code(MOCK_ACL, Bytes::from(acl_bytecode))?;
    tracing::info!("Deployed ACL at {MOCK_ACL}");

    // ─── 5. setAuthorizer(mock_acl) ───
    cobosafe::set_authorizer(&mut sim, owner, cobosafe_addr, MOCK_ACL)?;
    tracing::info!("setAuthorizer({MOCK_ACL}) OK");

    // ─── 6. addDelegate(test_delegate) ───
    cobosafe::add_delegate(&mut sim, owner, cobosafe_addr, TEST_DELEGATE)?;
    tracing::info!("addDelegate({TEST_DELEGATE}) OK");

    // ─── 7. Safe 是否已启用 CoboSafe 作为 module ───
    ensure_module_enabled(&mut sim, safe, cobosafe_addr)?;

    // ─── 8. delegate → CoboSafe.execTransactions([Safe → WETH.deposit(1 ETH)]) ───
    let inner = TxRequest {
        to: WETH,
        value: one_eth,
        data: Bytes::from(depositCall {}.abi_encode()),
        gas_limit: 200_000,
    };
    let builder = CoboSafeBuilder::new(cobosafe_addr, CHAIN_ID);
    let nonce = sim.get_nonce(TEST_DELEGATE)?;
    let unsigned = builder.build_txs(&[inner], nonce, 0, 0)?;
    let tx = unsigned
        .into_iter()
        .next()
        .expect("CoboSafeBuilder must return one tx");

    let bal_before = fork_erc20_balance(&sim, WETH, safe)?;
    tracing::info!("Safe WETH before: {bal_before}");

    let tx_env = TxEnv {
        caller: TEST_DELEGATE,
        nonce,
        kind: tx.to,
        data: tx.input.clone(),
        value: U256::ZERO,
        gas_limit: tx.gas_limit,
        ..Default::default()
    };

    // 注册 ABI 给 decoder：
    // - WETH：解码 Deposit/Transfer event（本 example 会看到 Deposit）+ calldata
    // - MOCK_ACL：本 example 本身不 emit 事件，但生产 ACL 可能 emit，
    //   作为模式演示一并注册
    let mut decoder = AbiDecoder::new();
    let weth_abi: JsonAbi = serde_json::from_str(WETH_ABI)?;
    decoder.register_abi(WETH, weth_abi);
    decoder.register_abi(MOCK_ACL, acl_abi);

    let result = sim.simulate_and_commit(tx_env.clone())?;

    // 打印可读的模拟结果：Status / From / To / Calldata（raw + decoded）/
    // Gas / Output / Events（按注册 ABI 解码）/ Revert（若有）。
    display_result(&result, &tx_env, Some(&decoder));

    eyre::ensure!(
        result.success,
        "execTransactions reverted: {:?}",
        result.revert_reason
    );

    // ─── 9. 断言 Safe 的 WETH 余额 +1 ETH ───
    let bal_after = fork_erc20_balance(&sim, WETH, safe)?;
    let delta = bal_after - bal_before;
    tracing::info!("Safe WETH after:  {bal_after} (delta +{delta})");
    eyre::ensure!(
        delta == one_eth,
        "expected delta {one_eth} (1 ETH), got {delta}"
    );

    tracing::info!("✓ cobosafe fork e2e 全部步骤通过");
    Ok(())
}

/// 检查 `safe.isModuleEnabled(cobosafe)`；未启用则以 Safe 自身为 caller
/// 调用 `Safe.enableModule(cobosafe)`（Gnosis Safe 规定 enableModule 只能 self-call）。
fn ensure_module_enabled(
    sim: &mut ForkSimulator,
    safe: Address,
    cobosafe_addr: Address,
) -> Result<()> {
    let check = TxEnv {
        caller: Address::ZERO,
        kind: TxKind::Call(safe),
        data: Bytes::from(isModuleEnabledCall { module: cobosafe_addr }.abi_encode()),
        gas_limit: 100_000,
        ..Default::default()
    };
    let r = sim.simulate(check)?;
    let out = r
        .output
        .ok_or_else(|| eyre::eyre!("isModuleEnabled returned no output"))?;
    let enabled = isModuleEnabledCall::abi_decode_returns(&out)?;

    if enabled {
        tracing::info!("CoboSafe already enabled as Safe module, skip enableModule");
        return Ok(());
    }

    let enable = TxEnv {
        caller: safe,
        nonce: sim.get_nonce(safe)?,
        kind: TxKind::Call(safe),
        data: Bytes::from(enableModuleCall { module: cobosafe_addr }.abi_encode()),
        gas_limit: 200_000,
        ..Default::default()
    };
    let r = sim.simulate_and_commit(enable)?;
    eyre::ensure!(r.success, "enableModule failed: {:?}", r.revert_reason);
    tracing::info!("enableModule({cobosafe_addr}) OK");
    Ok(())
}

/// 从 Foundry 编译产物 `out/<file>.sol/<contract>.json` 读 runtime bytecode 和 ABI。
///
/// - `deployedBytecode.object` = 合约部署后的 runtime 字节码，正是 `set_code` 要写入的内容。
///   （`bytecode.object` 则是 constructor bytecode，用于 create tx；set_code 不适合它。）
/// - `abi` = 合约的 JSON ABI，注册进 `AbiDecoder` 能解码后续 tx/events。
fn load_foundry_artifact(path: &str) -> Result<(Vec<u8>, JsonAbi)> {
    let content = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("failed to read foundry artifact: {path}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&content).wrap_err("foundry artifact is not valid JSON")?;

    let hex_str = v
        .get("deployedBytecode")
        .and_then(|o| o.get("object"))
        .and_then(|s| s.as_str())
        .ok_or_else(|| eyre::eyre!("no `deployedBytecode.object` in artifact: {path}"))?;
    let hex = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    let bytecode = alloy::hex::decode(hex).wrap_err("deployedBytecode.object hex decode")?;
    eyre::ensure!(!bytecode.is_empty(), "deployedBytecode.object is empty in {path}");

    let abi_value = v
        .get("abi")
        .ok_or_else(|| eyre::eyre!("no `abi` in artifact: {path}"))?;
    let abi: JsonAbi = serde_json::from_value(abi_value.clone())
        .wrap_err("parse `abi` field as JsonAbi")?;

    Ok((bytecode, abi))
}
