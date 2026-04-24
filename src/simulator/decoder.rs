use std::{collections::HashMap, path::Path};

use alloy::{
    dyn_abi::{EventExt, JsonAbiExt},
    json_abi::{Event, Function, JsonAbi},
    primitives::{Address, FixedBytes, Log, B256},
};
use eyre::Result;

/// 解码后的函数调用
pub struct DecodedCall {
    pub name: String,
    pub signature: String,
    pub params: Vec<(String, String)>,
}

/// 解码后的事件
pub struct DecodedEvent {
    pub name: String,
    pub signature: String,
    pub params: Vec<(String, String, bool)>, // (name, value, indexed)
}

/// ABI 动态解码器
pub struct AbiDecoder {
    abis: HashMap<Address, JsonAbi>,
    fn_selectors: HashMap<FixedBytes<4>, Vec<(Address, Function)>>,
    event_selectors: HashMap<B256, Vec<(Address, Event)>>,
}

impl AbiDecoder {
    pub fn new() -> Self {
        Self {
            abis: HashMap::new(),
            fn_selectors: HashMap::new(),
            event_selectors: HashMap::new(),
        }
    }

    /// 注册合约 ABI
    pub fn register_abi(&mut self, address: Address, abi: JsonAbi) {
        for func in abi.functions() {
            let selector = func.selector();
            self.fn_selectors
                .entry(selector)
                .or_default()
                .push((address, func.clone()));
        }
        for event in abi.events() {
            let selector = event.selector();
            self.event_selectors
                .entry(selector)
                .or_default()
                .push((address, event.clone()));
        }
        self.abis.insert(address, abi);
    }

    /// 从 JSON 文件加载 ABI
    pub fn load_abi_file(&mut self, address: Address, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        let abi: JsonAbi = serde_json::from_str(&content)?;
        self.register_abi(address, abi);
        Ok(())
    }

    /// 解码 calldata
    pub fn decode_calldata(&self, to: &Address, data: &[u8]) -> Option<DecodedCall> {
        if data.len() < 4 {
            return None;
        }
        let selector = FixedBytes::<4>::from_slice(&data[..4]);

        let candidates = self.fn_selectors.get(&selector)?;
        let func = candidates
            .iter()
            .find(|(addr, _)| addr == to)
            .or_else(|| candidates.first())
            .map(|(_, f)| f)?;

        let decoded = func.abi_decode_input(&data[4..]).ok()?;
        let params = func
            .inputs
            .iter()
            .zip(decoded)
            .map(|(input, val)| (input.name.clone(), format!("{val:?}")))
            .collect();

        Some(DecodedCall {
            name: func.name.clone(),
            signature: func.signature(),
            params,
        })
    }

    /// 解码事件日志
    pub fn decode_log(&self, log: &Log) -> Option<DecodedEvent> {
        let topic0 = log.topics().first()?;
        let candidates = self.event_selectors.get(topic0)?;

        let event = candidates
            .iter()
            .find(|(addr, _)| *addr == log.address)
            .or_else(|| candidates.first())
            .map(|(_, e)| e)?;

        let decoded = event.decode_log(&log.data).ok()?;

        let mut indexed_iter = decoded.indexed.into_iter();
        let mut body_iter = decoded.body.into_iter();
        let params = event
            .inputs
            .iter()
            .map(|input| {
                let val = if input.indexed {
                    indexed_iter
                        .next()
                        .map(|v| format!("{v:?}"))
                        .unwrap_or_default()
                } else {
                    body_iter
                        .next()
                        .map(|v| format!("{v:?}"))
                        .unwrap_or_default()
                };
                (input.name.clone(), val, input.indexed)
            })
            .collect();

        Some(DecodedEvent {
            name: event.name.clone(),
            signature: event.signature(),
            params,
        })
    }

    /// 解码 revert 数据
    pub fn decode_revert(data: &[u8]) -> Option<String> {
        if data.len() < 4 {
            return None;
        }
        // Error(string) selector: 0x08c379a0
        if data[..4] == [0x08, 0xc3, 0x79, 0xa0] {
            if let Ok(s) =
                <alloy::sol_types::sol_data::String as alloy::sol_types::SolType>::abi_decode(
                    &data[4..],
                )
            {
                return Some(s);
            }
        }
        // Panic(uint256) selector: 0x4e487b71
        if data[..4] == [0x4e, 0x48, 0x7b, 0x71] {
            if let Ok(code) =
                <alloy::sol_types::sol_data::Uint<256> as alloy::sol_types::SolType>::abi_decode(
                    &data[4..],
                )
            {
                return Some(format!("Panic(0x{code:x})"));
            }
        }
        Some(format!("0x{}", alloy::hex::encode(data)))
    }
}

impl Default for AbiDecoder {
    fn default() -> Self {
        Self::new()
    }
}
