//! Foundry 编译产物（`out/<file>.sol/<contract>.json`）读取 helpers。
//!
//! `forge build` 输出的 artifact JSON 含：
//! - `abi` — 合约 JSON ABI（注册到 [`crate::AbiDecoder`] 即可解码 tx/events）
//! - `bytecode.object` — **带 constructor** 的 create bytecode（部署 tx 用）
//! - `deployedBytecode.object` — 合约部署后的 **runtime** bytecode，也就是
//!   [`crate::ForkSimulator::set_code`] 要写入的内容
//!
//! 两种加载入口：
//!
//! - [`load_artifact`]：直接给 artifact JSON 文件路径
//! - [`load_artifact_by_name`]：给 Foundry 项目目录 + 合约名，自动拼
//!   `<project>/out/<Name>.sol/<Name>.json`

use std::path::{Path, PathBuf};

use alloy::json_abi::JsonAbi;
use eyre::{Result, WrapErr};
use serde_json::Value;

/// Foundry artifact 里 skill / 测试 / 生产 都会用到的字段。
#[derive(Debug, Clone)]
pub struct Artifact {
    pub abi: JsonAbi,
    /// 带 constructor 的 create bytecode —— 用于 deploy tx (`TxKind::Create`)。
    pub bytecode: Vec<u8>,
    /// 合约部署后的 runtime bytecode —— `set_code` 写入 fork 的就是它。
    pub deployed_bytecode: Vec<u8>,
}

/// 从 Foundry artifact JSON 文件读取 ABI + bytecode + deployedBytecode。
pub fn load_artifact<P: AsRef<Path>>(path: P) -> Result<Artifact> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("failed to read foundry artifact: {}", path.display()))?;
    let v: Value = serde_json::from_str(&content)
        .wrap_err_with(|| format!("foundry artifact is not valid JSON: {}", path.display()))?;

    let abi_value = v
        .get("abi")
        .ok_or_else(|| eyre::eyre!("no `abi` in artifact: {}", path.display()))?;
    let abi: JsonAbi =
        serde_json::from_value(abi_value.clone()).wrap_err("parse `abi` field as JsonAbi")?;

    let bytecode = read_hex_field(&v, "bytecode").wrap_err_with(|| {
        format!("read `bytecode.object` in {}", path.display())
    })?;
    let deployed_bytecode = read_hex_field(&v, "deployedBytecode").wrap_err_with(|| {
        format!("read `deployedBytecode.object` in {}", path.display())
    })?;

    Ok(Artifact {
        abi,
        bytecode,
        deployed_bytecode,
    })
}

/// 按 Foundry 默认路径 `<project_dir>/out/<Name>.sol/<Name>.json` 加载。
///
/// `contract_name` 不含 `.sol` 后缀。例如：
/// ```ignore
/// load_artifact_by_name("./mock-acl", "MockAuthorizer")
/// // 读 ./mock-acl/out/MockAuthorizer.sol/MockAuthorizer.json
/// ```
pub fn load_artifact_by_name<P: AsRef<Path>>(
    project_dir: P,
    contract_name: &str,
) -> Result<Artifact> {
    let path: PathBuf = project_dir
        .as_ref()
        .join("out")
        .join(format!("{contract_name}.sol"))
        .join(format!("{contract_name}.json"));
    load_artifact(&path)
}

fn read_hex_field(v: &Value, field: &str) -> Result<Vec<u8>> {
    let hex_str = v
        .get(field)
        .and_then(|o| o.get("object"))
        .and_then(|s| s.as_str())
        .ok_or_else(|| eyre::eyre!("no `{field}.object`"))?;
    let hex = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    let bytes = alloy::hex::decode(hex)
        .wrap_err_with(|| format!("{field}.object hex decode"))?;
    eyre::ensure!(!bytes.is_empty(), "{field}.object is empty");
    Ok(bytes)
}
