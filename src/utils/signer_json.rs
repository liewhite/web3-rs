//! 把 Rust `TxEip1559` 转成 `cs-signer` 规定的 `JsStruct` 格式。
//!
//! cs-signer 的 rule.js 以 `check(dataStr: string) -> boolean` 为契约，
//! 其中 `dataStr` 是 `JSON.stringify(jsStruct)`。对 EIP-1559 交易，
//! `jsStruct` 结构：
//!
//! ```json
//! {
//!   "type": "transaction",
//!   "content": {
//!     "chain_id": 1,
//!     "account": "0x...",
//!     "transaction": {
//!       "chainId": "0x1",
//!       "type": "0x02",
//!       "hash": "0x00...00",
//!       "nonce": "0x05",
//!       "from": "0x...",
//!       "to": "0x...",
//!       "value": "0x00",
//!       "gas": "0x5208",
//!       "gasPrice": "",
//!       "maxPriorityFeePerGas": "0x...",
//!       "maxFeePerGas": "0x...",
//!       "input": "0x...",
//!       "access_list": null,
//!       "v": "", "r": "", "s": ""
//!     }
//!   }
//! }
//! ```
//!
//! 典型用途：在 Rust fork 测试里构造一堆 tx，dump 成这种 JSON 交给
//! `node test_rule.js` 去验证 rule.js 是否符合预期（rule.js 与 ACL 的一致性
//! 校验）。
//!
//! 当前只覆盖 EIP-1559（`type=0x02`）。legacy / EIP-2930 / EIP-4844 待需再加。

use alloy::{
    consensus::TxEip1559,
    primitives::{Address, TxKind},
};
use serde_json::{json, Value};

/// 构造 rule.js 一次 check 所需的顶层 `jsStruct`。
///
/// `account` = 签名者地址；tx 本身没有 from 字段（EIP-1559 未签名 tx），
/// 由调用方显式传入。
pub fn tx_to_signer_json(tx: &TxEip1559, account: Address, chain_id: u64) -> Value {
    json!({
        "type": "transaction",
        "content": {
            "chain_id": chain_id,
            "account": format!("{account:#x}"),
            "transaction": tx_inner(tx, account),
        }
    })
}

/// EIP-1559 tx 的内层 JSON。与 cs-signer `Transaction` struct 字段对齐。
fn tx_inner(tx: &TxEip1559, account: Address) -> Value {
    let to = match tx.to {
        TxKind::Call(a) => format!("{a:#x}"),
        TxKind::Create => String::new(),
    };
    json!({
        "chainId": format!("0x{:x}", tx.chain_id),
        "type": "0x02",
        "hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "nonce": format!("0x{:x}", tx.nonce),
        "from": format!("{account:#x}"),
        "to": to,
        "value": format!("0x{:x}", tx.value),
        "gas": format!("0x{:x}", tx.gas_limit),
        "gasPrice": "",
        "maxPriorityFeePerGas": format!("0x{:x}", tx.max_priority_fee_per_gas),
        "maxFeePerGas": format!("0x{:x}", tx.max_fee_per_gas),
        "input": format!("0x{}", alloy::hex::encode(&tx.input)),
        "access_list": Value::Null,
        "v": "", "r": "", "s": ""
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{address, Bytes, U256};

    #[test]
    fn eip1559_tx_serializes_per_cs_signer_spec() {
        let account = address!("1111111111111111111111111111111111111111");
        let target = address!("2222222222222222222222222222222222222222");
        let tx = TxEip1559 {
            chain_id: 1,
            nonce: 5,
            max_priority_fee_per_gas: 2_000_000_000,
            max_fee_per_gas: 20_000_000_000,
            gas_limit: 21_000,
            to: TxKind::Call(target),
            value: U256::from(1_000_000_000_000_000_000u128),
            input: Bytes::from(vec![0xab, 0xcd]),
            access_list: Default::default(),
        };
        let v = tx_to_signer_json(&tx, account, 1);

        assert_eq!(v["type"], "transaction");
        assert_eq!(v["content"]["chain_id"], 1);
        assert_eq!(v["content"]["account"], "0x1111111111111111111111111111111111111111");

        let t = &v["content"]["transaction"];
        assert_eq!(t["chainId"], "0x1");
        assert_eq!(t["type"], "0x02");
        assert_eq!(t["nonce"], "0x5");
        assert_eq!(t["from"], "0x1111111111111111111111111111111111111111");
        assert_eq!(t["to"], "0x2222222222222222222222222222222222222222");
        assert_eq!(t["value"], "0xde0b6b3a7640000"); // 1e18
        assert_eq!(t["gas"], "0x5208"); // 21000
        assert_eq!(t["maxPriorityFeePerGas"], "0x77359400"); // 2 gwei
        assert_eq!(t["maxFeePerGas"], "0x4a817c800"); // 20 gwei
        assert_eq!(t["input"], "0xabcd");
        assert!(t["access_list"].is_null());
    }

    #[test]
    fn contract_creation_tx_has_empty_to() {
        let account = address!("1111111111111111111111111111111111111111");
        let tx = TxEip1559 {
            chain_id: 1,
            nonce: 0,
            max_priority_fee_per_gas: 1,
            max_fee_per_gas: 1,
            gas_limit: 100_000,
            to: TxKind::Create,
            value: U256::ZERO,
            input: Bytes::new(),
            access_list: Default::default(),
        };
        let v = tx_to_signer_json(&tx, account, 1);
        assert_eq!(v["content"]["transaction"]["to"], "");
    }
}
