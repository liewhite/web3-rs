//! ERC20 通用接口与 RPC 查询。
//!
//! - sol! 接口：`balanceOfCall` / `decimalsCall` / `approveCall` / `transferCall`
//! - RPC 查询 helper：`balance` / `decimals`
//!
//! fork 场景下的 `balanceOf` 查询见 [`crate::simulator::erc20::balance`]。

use alloy::{
    network::{AnyNetwork, TransactionBuilder},
    primitives::{Address, Bytes, U256},
    providers::{DynProvider, Provider},
    rpc::types::TransactionRequest,
    sol,
    sol_types::SolCall,
};
use eyre::Result;

sol! {
    function approve(address spender, uint256 amount) external returns (bool);
    function balanceOf(address owner) external view returns (uint256);
    function decimals() external view returns (uint8);
    function transfer(address to, uint256 amount) external returns (bool);
}

/// 通过 RPC 查询 `token.balanceOf(owner)`。
pub async fn balance(
    provider: &DynProvider<AnyNetwork>,
    token: Address,
    owner: Address,
) -> Result<U256> {
    let data = balanceOfCall { owner }.abi_encode();
    let req = TransactionRequest::default()
        .with_to(token)
        .with_input(Bytes::from(data));
    let result = provider.call(req.into()).await?;
    Ok(balanceOfCall::abi_decode_returns(&result)?)
}

/// 通过 RPC 查询 `token.decimals()`。
pub async fn decimals(provider: &DynProvider<AnyNetwork>, token: Address) -> Result<u8> {
    let data = decimalsCall {}.abi_encode();
    let req = TransactionRequest::default()
        .with_to(token)
        .with_input(Bytes::from(data));
    let result = provider.call(req.into()).await?;
    Ok(decimalsCall::abi_decode_returns(&result)?)
}
