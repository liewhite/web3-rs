//! CoboSafe + FlatRoleManager + Safe module 的辅助函数。
//!
//! - **RPC 查询**：`query_safe`（查 CoboSafe 后面的 Safe 地址）
//! - **生产流水线**：`submit_transactions`（build → sign → send 一条龙）
//! - **Fork 辅助**（按 "admin 调 setter" 的模式，caller 传谁就由谁发起）：
//!   - `set_authorizer` / `add_delegate` / `get_owner` / `get_safe`（CoboSafe）
//!   - `add_roles` / `grant_roles`（FlatRoleManager）
//!   - `is_module_enabled` / `enable_module`（Gnosis Safe module 管理）
//!   - `setup_fork_test_env`：一键串起上面所有步骤（读 owner/safe + fund +
//!     setAuthorizer + addDelegate + enableModule）

use alloy::{
    consensus::TxEip1559,
    network::{AnyNetwork, TransactionBuilder},
    primitives::{Address, Bytes, TxKind, B256, U256},
    providers::{DynProvider, Provider},
    rpc::types::TransactionRequest,
    sol,
    sol_types::SolCall,
};
use eyre::Result;
use revm::context::TxEnv;

use crate::{CoboSafeBuilder, ForkSimulator, TxBuilder, TxRequest, TxSender, TxSigner};

sol! {
    // CoboSafeAccount（Cobo Argus）接口
    function setAuthorizer(address _authorizer) external;
    function addDelegate(address _delegate) external;
    function owner() external view returns (address);
    function safe() external view returns (address);

    // FlatRoleManager 接口
    function addRoles(bytes32[] _roles) external;
    function grantRoles(bytes32[] _roles, address[] _delegates) external;

    // Gnosis Safe module 管理接口（Safe 的 module 设置只允许 self-call）
    function isModuleEnabled(address module) external view returns (bool);
    function enableModule(address module) external;
}

// ── RPC 查询 ──

/// 查 CoboSafe 合约背后的 Safe 地址（`cobosafe.safe()`）。
pub async fn query_safe(
    provider: &DynProvider<AnyNetwork>,
    cobosafe: Address,
) -> Result<Address> {
    let data = safeCall {}.abi_encode();
    let req = TransactionRequest::default()
        .with_to(cobosafe)
        .with_input(Bytes::from(data));
    let result = provider.call(req.into()).await?;
    Ok(safeCall::abi_decode_returns(&result)?)
}

// ── 生产流水线 ──

/// 把 `requests` 打包成**一笔** `CoboSafe.execTransactions`，签名后经 `sender` 广播。
///
/// - `requests` 的每一条作为 inner call；gas_limit 按 sum 累加由 `CoboSafeBuilder` 计算外层。
/// - `signer` 接受任意实现 `TxSigner` 的签名器（`RemoteSigner` / `LocalSigner`）。
/// - `sender` 决定广播路径（`RpcSender` public mempool / `FlashbotsSender` bundle /
///   `PrivateSender` 私有池）。
///
/// nonce 自动从 `provider.get_transaction_count(signer.address())` 取。
/// 返回 `sender` 的 hashes（Flashbots bundle 场景按目标区块数返回多个）。
#[allow(clippy::too_many_arguments)]
pub async fn submit_transactions<S: TxSender, Sg: TxSigner>(
    provider: &DynProvider<AnyNetwork>,
    signer: &Sg,
    cobosafe: Address,
    sender: &S,
    requests: &[TxRequest],
    max_fee_wei: u128,
    priority_fee_wei: u128,
) -> Result<Vec<B256>> {
    let operator = signer.address();
    let chain_id = provider.get_chain_id().await?;
    let builder = CoboSafeBuilder::new(cobosafe, chain_id);
    let nonce = provider.get_transaction_count(operator).await?;
    let unsigned: Vec<TxEip1559> =
        builder.build_txs(requests, nonce, max_fee_wei, priority_fee_wei)?;
    let tx = unsigned
        .into_iter()
        .next()
        .expect("CoboSafeBuilder must return one tx");
    tracing::info!("nonce={nonce}, gas_limit={}", tx.gas_limit);
    tracing::info!("signing ...");
    let raw = signer.sign(tx).await?;
    tracing::info!("raw tx:     0x{}", alloy::hex::encode(&raw.0));
    tracing::info!("broadcasting ...");
    let hashes = sender.send_txs(&[raw]).await?;
    for h in &hashes {
        tracing::info!("tx hash: {h}");
    }
    Ok(hashes)
}

// ── Fork 辅助 ──

/// 在 fork 上设置 CoboSafe 的 authorizer（ACL 合约）
pub fn set_authorizer(
    sim: &mut ForkSimulator,
    owner: Address,
    cobosafe: Address,
    authorizer: Address,
) -> Result<()> {
    let data = setAuthorizerCall { _authorizer: authorizer }.abi_encode();
    exec_admin_call(sim, owner, cobosafe, data)
}

/// 在 fork 上为 CoboSafe 添加 delegate
pub fn add_delegate(
    sim: &mut ForkSimulator,
    owner: Address,
    cobosafe: Address,
    delegate: Address,
) -> Result<()> {
    let data = addDelegateCall { _delegate: delegate }.abi_encode();
    exec_admin_call(sim, owner, cobosafe, data)
}

/// 查询 CoboSafe 合约的 owner 地址
pub fn get_owner(sim: &ForkSimulator, cobosafe: Address) -> Result<Address> {
    let data = ownerCall {}.abi_encode();
    let tx = TxEnv {
        caller: Address::ZERO,
        kind: TxKind::Call(cobosafe),
        data: Bytes::from(data),
        gas_limit: 100_000,
        ..Default::default()
    };
    let result = sim.simulate(tx)?;
    let output = result
        .output
        .ok_or_else(|| eyre::eyre!("owner() returned no output"))?;
    let decoded = ownerCall::abi_decode_returns(&output)?;
    Ok(decoded)
}

/// 查询 CoboSafe 关联的 Gnosis Safe 地址
pub fn get_safe(sim: &ForkSimulator, cobosafe: Address) -> Result<Address> {
    let data = safeCall {}.abi_encode();
    let tx = TxEnv {
        caller: Address::ZERO,
        kind: TxKind::Call(cobosafe),
        data: Bytes::from(data),
        gas_limit: 100_000,
        ..Default::default()
    };
    let result = sim.simulate(tx)?;
    let output = result
        .output
        .ok_or_else(|| eyre::eyre!("safe() returned no output"))?;
    let decoded = safeCall::abi_decode_returns(&output)?;
    Ok(decoded)
}

fn exec_admin_call(
    sim: &mut ForkSimulator,
    caller: Address,
    to: Address,
    data: Vec<u8>,
) -> Result<()> {
    let nonce = sim.get_nonce(caller)?;
    let tx = TxEnv {
        caller,
        nonce,
        kind: TxKind::Call(to),
        data: Bytes::from(data),
        gas_limit: 500_000,
        ..Default::default()
    };
    let result = sim.simulate_and_commit(tx)?;
    if !result.success {
        let reason = result.revert_reason.unwrap_or_else(|| {
            result
                .output
                .as_ref()
                .map(|b| format!("0x{}", alloy::hex::encode(b)))
                .unwrap_or_else(|| "unknown".into())
        });
        return Err(eyre::eyre!("admin call reverted: {reason}"));
    }
    Ok(())
}

// ── FlatRoleManager（Cobo Argus）──

/// FlatRoleManager.addRoles(roles) —— 创建若干新角色（空委托，后续用
/// `grant_roles` 绑定 delegates）。caller 一般是 Safe。
pub fn add_roles(
    sim: &mut ForkSimulator,
    caller: Address,
    role_manager: Address,
    roles: &[B256],
) -> Result<()> {
    let data = addRolesCall {
        _roles: roles.to_vec(),
    }
    .abi_encode();
    exec_admin_call(sim, caller, role_manager, data)
}

/// FlatRoleManager.grantRoles(roles, delegates) —— 同时 add role 和 add delegate，
/// 把 `delegates` 里每个地址授予 `roles` 里每个角色。caller 一般是 Safe。
pub fn grant_roles(
    sim: &mut ForkSimulator,
    caller: Address,
    role_manager: Address,
    roles: &[B256],
    delegates: &[Address],
) -> Result<()> {
    let data = grantRolesCall {
        _roles: roles.to_vec(),
        _delegates: delegates.to_vec(),
    }
    .abi_encode();
    exec_admin_call(sim, caller, role_manager, data)
}

// ── Safe module 管理 ──

/// 查询 `safe.isModuleEnabled(module)`。
pub fn is_module_enabled(sim: &ForkSimulator, safe: Address, module: Address) -> Result<bool> {
    let tx = TxEnv {
        caller: Address::ZERO,
        kind: TxKind::Call(safe),
        data: Bytes::from(isModuleEnabledCall { module }.abi_encode()),
        gas_limit: 100_000,
        ..Default::default()
    };
    let r = sim.simulate(tx)?;
    let out = r
        .output
        .ok_or_else(|| eyre::eyre!("isModuleEnabled returned no output"))?;
    Ok(isModuleEnabledCall::abi_decode_returns(&out)?)
}

/// Safe self-call `enableModule(module)`。Gnosis Safe 的 module 管理只接受 self-call。
///
/// 已启用时跳过（不重复执行）。
pub fn enable_module(sim: &mut ForkSimulator, safe: Address, module: Address) -> Result<()> {
    if is_module_enabled(sim, safe, module)? {
        return Ok(());
    }
    let data = enableModuleCall { module }.abi_encode();
    // caller == safe：Gnosis Safe 的 enableModule 硬性要求 self-call
    exec_admin_call(sim, safe, safe, data)
}

// ── 一键 fork 环境 ──

/// Fork 测试环境配好之后返回给调用方的关键地址。
#[derive(Debug, Clone, Copy)]
pub struct ForkSetup {
    /// `CoboSafe.owner()` —— 通常是 Safe（权限位拥有者）。
    pub owner: Address,
    /// `CoboSafe.safe()` —— 背后的 Gnosis Safe。
    pub safe: Address,
}

/// 一键配置 CoboSafe fork 测试环境，等价于手工走完：
///
/// 1. 读 CoboSafe 的 owner 和 Safe 地址
/// 2. 给 owner / safe / delegate 各充 10 ETH（gas fund；fork_for_simulation
///    已禁 balance check，这里充是为了让路径更接近生产）
/// 3. owner → `CoboSafe.setAuthorizer(acl)`
/// 4. owner → `CoboSafe.addDelegate(delegate)`
/// 5. 若 Safe 尚未启用 CoboSafe 作为 module，`Safe.enableModule(CoboSafe)` self-call
///
/// **前置要求**：`acl` 地址在 fork 上必须已有 code。调用前用
/// `sim.set_code(acl, runtime_bytecode)` 部署，或 `acl` 指向 fork 已经存在的
/// ACL 合约。
///
/// **不做**的事：FlatRoleManager 的 `addRoles / grantRoles`（不通用，skill
/// 或业务测试按需自行调 [`add_roles`] / [`grant_roles`]）。
pub fn setup_fork_test_env(
    sim: &mut ForkSimulator,
    cobosafe: Address,
    acl: Address,
    delegate: Address,
) -> Result<ForkSetup> {
    let owner = get_owner(sim, cobosafe)?;
    let safe = get_safe(sim, cobosafe)?;

    let ten_eth = U256::from(10u64) * U256::from(10u64).pow(U256::from(18u64));
    sim.set_eth_balance(owner, ten_eth)?;
    sim.set_eth_balance(safe, ten_eth)?;
    sim.set_eth_balance(delegate, ten_eth)?;

    set_authorizer(sim, owner, cobosafe, acl)?;
    add_delegate(sim, owner, cobosafe, delegate)?;
    enable_module(sim, safe, cobosafe)?;

    Ok(ForkSetup { owner, safe })
}
