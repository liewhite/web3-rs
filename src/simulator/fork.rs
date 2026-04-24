use std::sync::Arc;

use alloy::{
    consensus::BlockHeader,
    eips::BlockId,
    network::{AnyNetwork, AnyRpcBlock},
    primitives::{keccak256, Address, Bytes, U256},
    providers::{Provider, ProviderBuilder},
};
use alloy_evm::{eth::EthEvmBuilder, Evm, EvmEnv};
use eyre::Result;
use foundry_fork_db::{cache::BlockchainDbMeta, BlockchainDb, SharedBackend};
use revm::{
    context::{BlockEnv, CfgEnv, TxEnv},
    context_interface::{
        block::BlobExcessGasAndPrice,
        result::{ExecutionResult, HaltReason, Output, ResultAndState},
    },
    database::WrapDatabaseRef,
    primitives::hardfork::SpecId,
    state::{Account, EvmState, EvmStorageSlot},
    DatabaseRef,
};

use super::decoder::AbiDecoder;

/// 交易模拟结果
pub struct SimulationResult {
    pub success: bool,
    pub gas_used: u64,
    pub gas_refunded: u64,
    pub output: Option<Bytes>,
    pub logs: Vec<alloy::primitives::Log>,
    pub revert_reason: Option<String>,
    pub state_changes: EvmState,
    pub created_address: Option<Address>,
}

/// EVM Fork 模拟器
pub struct ForkSimulator {
    shared: SharedBackend,
    block_env: BlockEnv,
    cfg_env: CfgEnv,
}

impl ForkSimulator {
    /// 从 RPC fork 创建模拟器。自动获取 chain_id，默认使用 PRAGUE 规范。
    pub async fn fork(rpc_url: &str, block_id: Option<BlockId>) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .network::<AnyNetwork>()
            .connect_http(rpc_url.parse()?);

        let bid = block_id.unwrap_or(BlockId::latest());
        let block = provider
            .get_block(bid)
            .await?
            .ok_or_else(|| eyre::eyre!("block not found"))?;
        let chain_id = provider.get_chain_id().await?;

        let block_env = block_env_from_rpc(&block);

        let meta = BlockchainDbMeta::default()
            .with_block(&block.inner)
            .with_url(rpc_url);
        let db = BlockchainDb::new(meta, None);

        let shared = SharedBackend::spawn_backend(Arc::new(provider), db, Some(bid)).await;

        let mut cfg_env = CfgEnv::default();
        cfg_env.spec = SpecId::PRAGUE;
        cfg_env.chain_id = chain_id;
        cfg_env.disable_eip3607 = true;

        Ok(Self {
            shared,
            block_env,
            cfg_env,
        })
    }

    /// 像 [`fork`](Self::fork) 一样从 RPC 创建 fork，但把 `disable_balance_check`
    /// 与 `disable_nonce_check` 一并打开。适用于大多数 bin 的模拟场景
    /// （直接复用链上 operator / safe 地址时不希望因为本地状态错配而 revert）。
    pub async fn fork_for_simulation(
        rpc_url: &str,
        block_id: Option<BlockId>,
    ) -> Result<Self> {
        let mut sim = Self::fork(rpc_url, block_id).await?;
        sim.set_disable_balance_check(true);
        sim.set_disable_nonce_check(true);
        Ok(sim)
    }

    pub fn block_env(&self) -> &BlockEnv {
        &self.block_env
    }

    pub fn set_block_number(&mut self, n: u64) {
        self.block_env.number = U256::from(n);
    }

    pub fn set_timestamp(&mut self, ts: u64) {
        self.block_env.timestamp = U256::from(ts);
    }

    pub fn set_basefee(&mut self, fee: u64) {
        self.block_env.basefee = fee;
    }

    pub fn set_block_env(&mut self, env: BlockEnv) {
        self.block_env = env;
    }

    pub fn set_disable_balance_check(&mut self, disable: bool) {
        self.cfg_env.disable_balance_check = disable;
    }

    pub fn set_disable_nonce_check(&mut self, disable: bool) {
        self.cfg_env.disable_nonce_check = disable;
    }

    /// 模拟执行交易（不 commit 状态）
    pub fn simulate(&self, tx: TxEnv) -> Result<SimulationResult> {
        let tx = self.fill_tx_defaults(tx)?;
        let mut evm = self.build_evm();
        let res = evm.transact(tx).map_err(|e| eyre::eyre!("{e:?}"))?;
        Ok(into_simulation_result(res))
    }

    /// 模拟执行并 commit 状态变更到 fork DB（用于连续交易模拟）
    pub fn simulate_and_commit(&mut self, tx: TxEnv) -> Result<SimulationResult> {
        let tx = self.fill_tx_defaults(tx)?;
        let mut evm = self.build_evm();
        let res = evm.transact(tx).map_err(|e| eyre::eyre!("{e:?}"))?;
        let result = into_simulation_result(res);
        self.commit_state(&result.state_changes);
        Ok(result)
    }

    /// 提交状态变更到 fork DB，修复 foundry-fork-db 的零值 slot 问题。
    ///
    /// foundry-fork-db 的 do_commit 会把 present_value=0 的 slot 从缓存中删除，
    /// 导致后续读取回退到 RPC 获取旧值。此方法在 do_commit 后重新插入零值 slot。
    fn commit_state(&self, state: &EvmState) {
        let db = self.shared.data();
        db.do_commit(state.clone());

        let mut storage = db.storage.write();
        for (addr, account) in state {
            for (slot, value) in &account.storage {
                if value.present_value().is_zero() {
                    storage.entry(*addr).or_default().insert(*slot, U256::ZERO);
                }
            }
        }
    }

    /// 获取账户当前 nonce
    pub fn get_nonce(&self, addr: Address) -> Result<u64> {
        let info = self
            .shared
            .basic_ref(addr)
            .map_err(|e| eyre::eyre!("{e:?}"))?;
        Ok(info.map(|a| a.nonce).unwrap_or_default())
    }

    /// 自动补全未指定的 gas_price（basefee）
    fn fill_tx_defaults(&self, mut tx: TxEnv) -> Result<TxEnv> {
        if tx.gas_price == 0 {
            tx.gas_price = self.block_env.basefee as u128;
        }
        Ok(tx)
    }

    /// 为账户设置 ETH 余额
    pub fn set_eth_balance(&mut self, addr: Address, balance: U256) -> Result<()> {
        let info = self
            .shared
            .basic_ref(addr)
            .map_err(|e| eyre::eyre!("{e:?}"))?
            .unwrap_or_default();

        let mut account = Account::default()
            .with_info(info)
            .with_touched_mark();
        account.info.balance = balance;

        let mut state = EvmState::default();
        state.insert(addr, account);
        self.shared.data().do_commit(state);
        Ok(())
    }

    /// 为账户设置 ERC20 代币余额（自动探测 storage layout）
    ///
    /// 通过写入测试值 + 调用 `balanceOf` 验证的方式自动探测 balance mapping 的 storage slot，
    /// 兼容 Solidity（`keccak256(key, slot)`）和 Vyper（`keccak256(slot, key)`）两种布局。
    pub fn set_erc20_balance(
        &mut self,
        token: Address,
        owner: Address,
        balance: U256,
    ) -> Result<()> {
        let test_value = U256::from(0xDEAD_BEEF_CAFE_BABE_u64);

        for slot_index in 0u64..20 {
            let mapping_slot = U256::from(slot_index);

            for storage_key in [
                solidity_mapping_key(owner, mapping_slot),
                vyper_mapping_key(owner, mapping_slot),
            ] {
                let original = self
                    .shared
                    .storage_ref(token, storage_key)
                    .map_err(|e| eyre::eyre!("{e:?}"))?;

                self.commit_storage(token, storage_key, original, test_value)?;
                let probed = self.probe_balance_of(token, owner)?;

                if probed == test_value {
                    self.commit_storage(token, storage_key, original, balance)?;
                    return Ok(());
                }
                self.commit_storage(token, storage_key, original, original)?;
            }
        }

        Err(eyre::eyre!(
            "could not detect ERC20 balance storage slot for token {:?}",
            token
        ))
    }

    /// 为地址设置合约字节码
    pub fn set_code(&mut self, addr: Address, code: Bytes) -> Result<()> {
        let info = self
            .shared
            .basic_ref(addr)
            .map_err(|e| eyre::eyre!("{e:?}"))?
            .unwrap_or_default();

        let code_hash = keccak256(&code);
        let bytecode = revm::bytecode::Bytecode::new_raw(code);

        let mut new_info = info;
        new_info.code_hash = code_hash;
        new_info.code = Some(bytecode);

        let account = Account::default()
            .with_info(new_info)
            .with_touched_mark();

        let mut state = EvmState::default();
        state.insert(addr, account);
        self.shared.data().do_commit(state);
        Ok(())
    }

    /// 读取账户余额
    pub fn get_balance(&self, addr: Address) -> Result<U256> {
        let info = self
            .shared
            .basic_ref(addr)
            .map_err(|e| eyre::eyre!("{e:?}"))?;
        Ok(info.map(|a| a.balance).unwrap_or_default())
    }

    /// 写入 storage 并 commit 到 fork DB
    fn commit_storage(
        &self,
        addr: Address,
        key: U256,
        original: U256,
        value: U256,
    ) -> Result<()> {
        let info = self
            .shared
            .basic_ref(addr)
            .map_err(|e| eyre::eyre!("{e:?}"))?
            .unwrap_or_default();
        let slot = EvmStorageSlot::new_changed(original, value, 0);
        let account = Account::default()
            .with_info(info)
            .with_touched_mark()
            .with_storage([(key, slot)].into_iter());
        let mut state = EvmState::default();
        state.insert(addr, account);
        self.commit_state(&state);
        Ok(())
    }

    /// 通过 EVM 调用 balanceOf 探测余额（内部禁用 balance check）
    fn probe_balance_of(&self, token: Address, owner: Address) -> Result<U256> {
        // balanceOf(address) selector: 0x70a08231
        let mut calldata = [0u8; 36];
        calldata[0..4].copy_from_slice(&[0x70, 0xa0, 0x82, 0x31]);
        calldata[16..36].copy_from_slice(owner.as_slice());

        let tx = TxEnv {
            caller: Address::ZERO,
            kind: alloy::primitives::TxKind::Call(token),
            data: Bytes::from(calldata.to_vec()),
            gas_limit: 100_000,
            gas_price: self.block_env.basefee as u128,
            ..Default::default()
        };

        let mut evm = self.build_probe_evm();
        let res = evm.transact(tx).map_err(|e| eyre::eyre!("{e:?}"))?;
        let result = into_simulation_result(res);

        if let Some(output) = &result.output {
            if output.len() >= 32 {
                return Ok(U256::from_be_slice(&output[..32]));
            }
        }
        Ok(U256::ZERO)
    }

    /// 构建禁用 balance check 的 EVM（用于内部 probe 调用）
    fn build_probe_evm(
        &self,
    ) -> impl Evm<Tx = TxEnv, HaltReason = HaltReason, DB = WrapDatabaseRef<SharedBackend>> + '_
    {
        let mut cfg = self.cfg_env.clone();
        cfg.disable_balance_check = true;
        let env = EvmEnv {
            block_env: self.block_env.clone(),
            cfg_env: cfg,
        };
        EthEvmBuilder::new(WrapDatabaseRef(self.shared.clone()), env).build()
    }

    fn build_evm(
        &self,
    ) -> impl Evm<Tx = TxEnv, HaltReason = HaltReason, DB = WrapDatabaseRef<SharedBackend>> + '_
    {
        let env = EvmEnv {
            block_env: self.block_env.clone(),
            cfg_env: self.cfg_env.clone(),
        };
        EthEvmBuilder::new(WrapDatabaseRef(self.shared.clone()), env).build()
    }
}

fn block_env_from_rpc(block: &AnyRpcBlock) -> BlockEnv {
    BlockEnv {
        number: U256::from(block.header.number()),
        beneficiary: block.header.beneficiary(),
        timestamp: U256::from(block.header.timestamp()),
        gas_limit: block.header.gas_limit(),
        basefee: block
            .header
            .base_fee_per_gas()
            .expect("post-London block must have base_fee"),
        prevrandao: block.header.mix_hash(),
        difficulty: block.header.difficulty(),
        blob_excess_gas_and_price: block.header.excess_blob_gas().map(|gas| {
            BlobExcessGasAndPrice::new_with_spec(gas, SpecId::PRAGUE)
        }),
    }
}

/// Solidity mapping storage key: keccak256(abi.encode(key, slot))
fn solidity_mapping_key(key: Address, mapping_slot: U256) -> U256 {
    let mut buf = [0u8; 64];
    buf[12..32].copy_from_slice(key.as_slice());
    buf[32..64].copy_from_slice(&mapping_slot.to_be_bytes::<32>());
    U256::from_be_bytes(keccak256(buf).0)
}

/// Vyper mapping storage key: keccak256(abi.encode(slot, key))
fn vyper_mapping_key(key: Address, mapping_slot: U256) -> U256 {
    let mut buf = [0u8; 64];
    buf[0..32].copy_from_slice(&mapping_slot.to_be_bytes::<32>());
    buf[44..64].copy_from_slice(key.as_slice());
    U256::from_be_bytes(keccak256(buf).0)
}

fn into_simulation_result(res: ResultAndState<HaltReason>) -> SimulationResult {
    let state_changes = res.state;
    let result = res.result;

    match result {
        ExecutionResult::Success {
            gas_used,
            gas_refunded,
            output,
            logs,
            ..
        } => {
            let (out_bytes, created_addr) = match &output {
                Output::Call(b) => (Some(b.clone()), None),
                Output::Create(b, addr) => (Some(b.clone()), *addr),
            };
            SimulationResult {
                success: true,
                gas_used,
                gas_refunded,
                output: out_bytes,
                logs: logs.into_iter().map(|l| l.into()).collect(),
                revert_reason: None,
                state_changes,
                created_address: created_addr,
            }
        }
        ExecutionResult::Revert { gas_used, output } => {
            let revert_reason = AbiDecoder::decode_revert(&output);
            SimulationResult {
                success: false,
                gas_used,
                gas_refunded: 0,
                output: Some(output),
                logs: vec![],
                revert_reason,
                state_changes,
                created_address: None,
            }
        }
        ExecutionResult::Halt {
            gas_used, reason, ..
        } => SimulationResult {
            success: false,
            gas_used,
            gas_refunded: 0,
            output: None,
            logs: vec![],
            revert_reason: Some(format!("HALT: {reason:?}")),
            state_changes,
            created_address: None,
        },
    }
}
