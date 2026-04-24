//! CoboSafe 辅助函数：
//!
//! - **RPC 查询**：`query_safe`（查 CoboSafe 后面的 Safe 地址）
//! - **生产流水线**：`submit_transactions`（build → sign → send 一条龙）
//! - **Fork 辅助**：`set_authorizer` / `add_delegate` / `get_owner` / `get_safe`
//!   — 本地 fork 初始化测试用

use alloy::{
    consensus::TxEip1559,
    network::{AnyNetwork, TransactionBuilder},
    primitives::{Address, Bytes, TxKind, B256},
    providers::{DynProvider, Provider},
    rpc::types::TransactionRequest,
    sol,
    sol_types::SolCall,
};
use eyre::Result;
use revm::context::TxEnv;

use crate::{CoboSafeBuilder, ForkSimulator, TxBuilder, TxRequest, TxSender, TxSigner};

sol! {
    function setAuthorizer(address _authorizer) external;
    function addDelegate(address _delegate) external;
    function owner() external view returns (address);
    function safe() external view returns (address);
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
