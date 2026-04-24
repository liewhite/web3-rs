//! Gnosis Safe [Transaction Builder] v1 JSON 生成。
//!
//! Safe 官方 Transaction Builder app 能导入本 JSON 作为 batch 交易，让多签
//! 一次性审批执行。典型用途：把 CoboSafe 的 `setAuthorizer` / `addDelegate`、
//! FlatRoleManager 的 `addRoles` / `grantRoles`、Safe 自己的 `enableModule`
//! 等"权限配置"动作打包成一个 JSON 让 Safe owner 审批上链。
//!
//! # 用法
//!
//! ```ignore
//! use flashseal_rs::utils::safe_tx_builder as stb;
//!
//! let txs = vec![
//!     stb::set_authorizer(cobosafe, acl_address),
//!     stb::add_delegate(cobosafe, operator_eoa),
//!     stb::grant_roles(role_manager, vec![role_name_to_bytes32("swap_bot")], vec![operator_eoa]),
//!     stb::enable_module(safe, cobosafe), // 若 CoboSafe 还没挂上 Safe module
//! ];
//! let json = stb::build(
//!     /*chain_id=*/ 1,
//!     /*safe=*/ safe_address,
//!     /*name=*/ "Setup MyBot ACL",
//!     /*description=*/ "Attach ACL + grant role + register delegate",
//!     &txs,
//! );
//! std::fs::write("safe-tx-deploy.json", serde_json::to_string_pretty(&json)?)?;
//! ```
//!
//! 把 `safe-tx-deploy.json` 导入 Safe Web UI → Apps → Transaction Builder → Load。
//!
//! [Transaction Builder]: https://help.safe.global/en/articles/40841-transaction-builder

use alloy::{
    primitives::{Address, Bytes, B256, U256},
    sol,
    sol_types::SolCall,
};
use serde_json::{json, Value};

sol! {
    // 本模块独立声明一份 selectors，不强依赖 utils::cobosafe
    function setAuthorizer(address _authorizer) external;
    function addDelegate(address _delegate) external;
    function addRoles(bytes32[] _roles) external;
    function grantRoles(bytes32[] _roles, address[] _delegates) external;
    function revokeRoles(bytes32[] _roles, address[] _delegates) external;
    function enableModule(address module) external;
    function disableModule(address prevModule, address module) external;
    function removeAuthorizer() external;
    function removeDelegate(address _delegate) external;
}

/// Safe 要批量执行的一笔 sub-tx。
#[derive(Debug, Clone)]
pub struct TxItem {
    pub to: Address,
    pub value: U256,
    pub data: Bytes,
}

impl TxItem {
    pub fn to_json(&self) -> Value {
        json!({
            "to": format!("{:#x}", self.to),
            "value": self.value.to_string(),
            "data": format!("0x{}", alloy::hex::encode(&self.data)),
        })
    }
}

// ── CoboSafe (CoboSafeAccount / Argus) 常见 ops ──

pub fn set_authorizer(cobosafe: Address, authorizer: Address) -> TxItem {
    TxItem {
        to: cobosafe,
        value: U256::ZERO,
        data: Bytes::from(setAuthorizerCall { _authorizer: authorizer }.abi_encode()),
    }
}

pub fn remove_authorizer(cobosafe: Address) -> TxItem {
    TxItem {
        to: cobosafe,
        value: U256::ZERO,
        data: Bytes::from(removeAuthorizerCall {}.abi_encode()),
    }
}

pub fn add_delegate(cobosafe: Address, delegate: Address) -> TxItem {
    TxItem {
        to: cobosafe,
        value: U256::ZERO,
        data: Bytes::from(addDelegateCall { _delegate: delegate }.abi_encode()),
    }
}

pub fn remove_delegate(cobosafe: Address, delegate: Address) -> TxItem {
    TxItem {
        to: cobosafe,
        value: U256::ZERO,
        data: Bytes::from(removeDelegateCall { _delegate: delegate }.abi_encode()),
    }
}

// ── FlatRoleManager 常见 ops ──

pub fn add_roles(role_manager: Address, roles: Vec<B256>) -> TxItem {
    TxItem {
        to: role_manager,
        value: U256::ZERO,
        data: Bytes::from(addRolesCall { _roles: roles }.abi_encode()),
    }
}

pub fn grant_roles(role_manager: Address, roles: Vec<B256>, delegates: Vec<Address>) -> TxItem {
    TxItem {
        to: role_manager,
        value: U256::ZERO,
        data: Bytes::from(
            grantRolesCall { _roles: roles, _delegates: delegates }.abi_encode(),
        ),
    }
}

pub fn revoke_roles(role_manager: Address, roles: Vec<B256>, delegates: Vec<Address>) -> TxItem {
    TxItem {
        to: role_manager,
        value: U256::ZERO,
        data: Bytes::from(
            revokeRolesCall { _roles: roles, _delegates: delegates }.abi_encode(),
        ),
    }
}

// ── Gnosis Safe module 管理 ──

pub fn enable_module(safe: Address, module: Address) -> TxItem {
    TxItem {
        to: safe,
        value: U256::ZERO,
        data: Bytes::from(enableModuleCall { module }.abi_encode()),
    }
}

/// `prev_module` 是 Safe module 链表里 `module` 的前驱（查 `getModulesPaginated`）。
pub fn disable_module(safe: Address, prev_module: Address, module: Address) -> TxItem {
    TxItem {
        to: safe,
        value: U256::ZERO,
        data: Bytes::from(
            disableModuleCall { prevModule: prev_module, module }.abi_encode(),
        ),
    }
}

// ── 自定义 ──

/// 任意合约调用（不在上述常见操作里时用）。
pub fn custom(to: Address, value: U256, data: Bytes) -> TxItem {
    TxItem { to, value, data }
}

/// 构造 Safe Transaction Builder v1 JSON。
///
/// - `chain_id`：1 / 42161 / ...
/// - `safe`：多签 Safe 地址（仅作 metadata，实际执行由导入者决定）
/// - `name` / `description`：Safe UI 里显示
/// - `txs`：按顺序执行的 sub-tx 列表
pub fn build(
    chain_id: u64,
    safe: Address,
    name: &str,
    description: &str,
    txs: &[TxItem],
) -> Value {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    json!({
        "version": "1.0",
        "chainId": chain_id.to_string(),
        "createdAt": now_ms,
        "meta": {
            "name": name,
            "description": description,
            "txBuilderVersion": "1.17.0",
            "createdFromSafeAddress": format!("{safe:#x}"),
            "createdFromOwnerAddress": "",
        },
        "transactions": txs.iter().map(TxItem::to_json).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{address, b256};

    #[test]
    fn set_authorizer_calldata_roundtrip() {
        let cobosafe = address!("1111111111111111111111111111111111111111");
        let authorizer = address!("2222222222222222222222222222222222222222");
        let tx = set_authorizer(cobosafe, authorizer);
        assert_eq!(tx.to, cobosafe);
        assert_eq!(tx.value, U256::ZERO);
        // selector + 1 word (abi-encoded address)
        assert_eq!(tx.data.len(), 4 + 32);
        // 反解回来必须匹配
        let decoded = setAuthorizerCall::abi_decode(&tx.data).unwrap();
        assert_eq!(decoded._authorizer, authorizer);
    }

    #[test]
    fn grant_roles_encodes_arrays() {
        let tx = grant_roles(
            address!("3333333333333333333333333333333333333333"),
            vec![b256!("0000000000000000000000000000000000000000000000000000000000000001")],
            vec![address!("4444444444444444444444444444444444444444")],
        );
        assert!(tx.data.len() > 4);
        // ABI decode verifies structure
        let decoded = grantRolesCall::abi_decode(&tx.data).unwrap();
        assert_eq!(decoded._roles.len(), 1);
        assert_eq!(decoded._delegates.len(), 1);
    }

    #[test]
    fn build_produces_v1_schema() {
        let safe = address!("5555555555555555555555555555555555555555");
        let cobosafe = address!("6666666666666666666666666666666666666666");
        let acl = address!("7777777777777777777777777777777777777777");
        let json = build(
            1,
            safe,
            "Test",
            "Test desc",
            &[set_authorizer(cobosafe, acl)],
        );
        assert_eq!(json["version"], "1.0");
        assert_eq!(json["chainId"], "1");
        assert_eq!(json["meta"]["name"], "Test");
        assert_eq!(json["meta"]["createdFromSafeAddress"], "0x5555555555555555555555555555555555555555");
        let txs = json["transactions"].as_array().unwrap();
        assert_eq!(txs.len(), 1);
        assert_eq!(txs[0]["to"], "0x6666666666666666666666666666666666666666");
        assert_eq!(txs[0]["value"], "0");
    }
}
