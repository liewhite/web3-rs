//! fork 模拟结果的 log 断言 helpers。
//!
//! 两种用法：
//!
//! 1. **按 (address, topic0) 精确匹配** — 不依赖 ABI，适合只关心事件是否 emit：
//!    ```ignore
//!    use flashseal_rs::simulator::assertions::{find_log, count_logs};
//!    assert!(find_log(&result.logs, WETH, DEPOSIT_TOPIC).is_some());
//!    ```
//!
//! 2. **按事件名 + 参数约束** — 依赖已注册 ABI 的 `AbiDecoder`，
//!    在断言业务流程时更直观：
//!    ```ignore
//!    use flashseal_rs::simulator::assertions::{assert_events_in_order, ExpectedEvent};
//!    assert_events_in_order(&result.logs, &decoder, &[
//!        ExpectedEvent {
//!            address: WETH, event: "Deposit",
//!            params: vec![("dst", format!("{safe:?}"))],
//!        },
//!    ])?;
//!    ```

use alloy::primitives::{Address, Log, B256};
use eyre::Result;

use super::decoder::{AbiDecoder, DecodedEvent};

/// 找第一条匹配 `address` + `topic0` 的 log。
pub fn find_log<'a>(logs: &'a [Log], address: Address, topic0: B256) -> Option<&'a Log> {
    logs.iter()
        .find(|l| l.address == address && l.topics().first() == Some(&topic0))
}

/// 统计匹配 `address` + `topic0` 的 log 条数。
pub fn count_logs(logs: &[Log], address: Address, topic0: B256) -> usize {
    logs.iter()
        .filter(|l| l.address == address && l.topics().first() == Some(&topic0))
        .count()
}

/// 找第一条通过 `decoder` 成功解码且事件名匹配的 log，返回 `(log, decoded)`。
pub fn find_decoded_log<'a>(
    logs: &'a [Log],
    decoder: &AbiDecoder,
    event_name: &str,
) -> Option<(&'a Log, DecodedEvent)> {
    logs.iter().find_map(|l| {
        decoder
            .decode_log(l)
            .filter(|d| d.name == event_name)
            .map(|d| (l, d))
    })
}

/// 期望事件：address + 事件名 + 参数子集约束。
///
/// `params` 的每一项 `(name, value)` 必须在解码后的参数里找到同名参数，
/// 且值（`DecodedEvent::params` 里 `format!("{v:?}")` 生成的字符串）相等。
/// 不在 `params` 里的参数不校验 —— 支持"部分匹配"。
#[derive(Debug, Clone)]
pub struct ExpectedEvent<'a> {
    pub address: Address,
    pub event: &'a str,
    pub params: Vec<(&'a str, String)>,
}

/// 在 `logs` 里按**顺序**查找 `expected` 列表（允许中间混有不相关的 log）。
///
/// 对每条 expected：从 cursor 位置向后扫描，找到第一条满足 (address, event_name,
/// params subset) 全部匹配的。找不到 bail 并指明是第几条 expected 丢失。
///
/// 返回 `Ok(())` 表示所有 expected 都按序出现；`Err` 说明某条未找到。
pub fn assert_events_in_order(
    logs: &[Log],
    decoder: &AbiDecoder,
    expected: &[ExpectedEvent<'_>],
) -> Result<()> {
    let mut cursor = 0;
    for (i, exp) in expected.iter().enumerate() {
        let found = logs[cursor..].iter().enumerate().find_map(|(j, log)| {
            if log.address != exp.address {
                return None;
            }
            let decoded = decoder.decode_log(log)?;
            if decoded.name != exp.event {
                return None;
            }
            for (pname, pexpected) in &exp.params {
                let got = decoded.params.iter().find(|(n, _, _)| n == pname)?;
                if got.1 != *pexpected {
                    return None;
                }
            }
            Some(j)
        });
        match found {
            Some(j) => cursor += j + 1,
            None => {
                eyre::bail!(
                    "expected event #{i} `{}@{:?}` not found in logs from position {cursor}",
                    exp.event,
                    exp.address
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{address, b256, Bytes, LogData};

    fn mk_log(addr: Address, topics: Vec<B256>) -> Log {
        Log {
            address: addr,
            data: LogData::new(topics, Bytes::new()).expect("valid topics"),
        }
    }

    #[test]
    fn find_log_matches_address_and_topic0() {
        let a = address!("1111111111111111111111111111111111111111");
        let b = address!("2222222222222222222222222222222222222222");
        let t1 = b256!("0000000000000000000000000000000000000000000000000000000000000001");
        let t2 = b256!("0000000000000000000000000000000000000000000000000000000000000002");

        let logs = vec![mk_log(b, vec![t1]), mk_log(a, vec![t1]), mk_log(a, vec![t2])];

        assert!(find_log(&logs, a, t1).is_some());
        assert_eq!(count_logs(&logs, a, t1), 1);
        assert_eq!(count_logs(&logs, a, t2), 1);
        assert!(find_log(&logs, a, b256!("00000000000000000000000000000000000000000000000000000000000000ff")).is_none());
    }
}
