//! fork 场景下的 ERC20 通用查询。
//!
//! 与 [`crate::utils::erc20::balance`]（RPC 版）区别：这里的输入是 `ForkSimulator`
//! 快照，不走 RPC，适合二分查找 / gas 试探 / 后置断言等连续仿真。

use alloy::{
    primitives::{Address, Bytes, TxKind, U256},
    sol,
    sol_types::SolCall,
};
use eyre::Result;
use revm::context::TxEnv;

use super::ForkSimulator;

sol! {
    function balanceOf(address owner) external view returns (uint256);
}

/// 在 fork 上查 `token.balanceOf(owner)`。caller 用 `Address::ZERO`，无状态。
pub fn balance(sim: &ForkSimulator, token: Address, owner: Address) -> Result<U256> {
    let data = balanceOfCall { owner }.abi_encode();
    let tx = TxEnv {
        caller: Address::ZERO,
        kind: TxKind::Call(token),
        data: Bytes::from(data),
        gas_limit: 100_000,
        ..Default::default()
    };
    let result = sim.simulate(tx)?;
    let output = result
        .output
        .ok_or_else(|| eyre::eyre!("{token}.balanceOf({owner}) returned no output"))?;
    Ok(balanceOfCall::abi_decode_returns(&output)?)
}
