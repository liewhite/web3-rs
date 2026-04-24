//! 生成生产部署用的 Safe Transaction Builder JSON —— 把 CoboSafe 权限
//! 配置动作打包让 Safe owner 多签一次性审批。
//!
//! 覆盖的操作：
//!
//! 1. `Safe.enableModule(CoboSafe)` —— 仅当 CoboSafe 还没挂上 Safe 时需要
//! 2. `CoboSafe.setAuthorizer(ACL)` —— 把 ACL 挂到 CoboSafe
//! 3. `FlatRoleManager.addRoles([role])` —— 注册角色
//! 4. `FlatRoleManager.grantRoles([role], [delegate])` —— 把 delegate 授予角色
//! 5. `CoboSafe.addDelegate(delegate)` —— 在 CoboSafe 侧登记 delegate
//!
//! 产出：`./safe_tx_deploy.json`（pretty printed）。Safe UI → Apps →
//! **Transaction Builder** → Load → 选此文件 → 审批签名。
//!
//! 运行：
//!   SAFE=0x... COBOSAFE=0x... ACL=0x... ROLE_MANAGER=0x... \
//!   DELEGATE=0x... ROLE_NAME=swap_bot \
//!   [MODULE_NOT_ENABLED=1] \
//!     cargo run --example safe_tx_deploy_json

use alloy::primitives::Address;
use eyre::Result;

use flashseal_rs::utils::{
    cobosafe::role_name_to_bytes32,
    safe_tx_builder as stb,
};

fn get_addr(name: &str) -> Result<Address> {
    std::env::var(name)
        .map_err(|_| eyre::eyre!("{name} env required"))?
        .parse()
        .map_err(|e| eyre::eyre!("{name} parse: {e}"))
}

fn main() -> Result<()> {
    let safe = get_addr("SAFE")?;
    let cobosafe = get_addr("COBOSAFE")?;
    let acl = get_addr("ACL")?;
    let role_manager = get_addr("ROLE_MANAGER")?;
    let delegate = get_addr("DELEGATE")?;
    let role_name = std::env::var("ROLE_NAME").unwrap_or_else(|_| "bot".into());
    let chain_id: u64 = std::env::var("CHAIN_ID")
        .unwrap_or_else(|_| "1".into())
        .parse()?;
    let module_not_enabled = std::env::var("MODULE_NOT_ENABLED").is_ok();

    let role = role_name_to_bytes32(&role_name);
    eprintln!("role `{role_name}` -> 0x{}", alloy::hex::encode(role));

    let mut txs = Vec::new();
    if module_not_enabled {
        txs.push(stb::enable_module(safe, cobosafe));
    }
    txs.push(stb::set_authorizer(cobosafe, acl));
    txs.push(stb::add_roles(role_manager, vec![role]));
    txs.push(stb::grant_roles(role_manager, vec![role], vec![delegate]));
    txs.push(stb::add_delegate(cobosafe, delegate));

    let json = stb::build(
        chain_id,
        safe,
        &format!("Deploy ACL + grant `{role_name}` to {delegate:#x}"),
        "自动生成：setAuthorizer + addRoles + grantRoles + addDelegate [+ enableModule]",
        &txs,
    );

    let out = "./safe_tx_deploy.json";
    std::fs::write(out, serde_json::to_string_pretty(&json)?)?;
    println!("Wrote {out} with {} txs", txs.len());
    println!("Safe UI → Apps → Transaction Builder → Load → select {out}");
    Ok(())
}
