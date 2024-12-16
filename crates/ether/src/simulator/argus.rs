use std::path::Path;
use std::sync::Arc;

use crate::abi;
use crate::argus::build_transaction;
use crate::simulator::SimulateTxMsg;
use alloy::hex::FromHex;
use alloy::network::TransactionBuilder;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::TransactionRequest;
use alloy::sol_types::SolCall;
use alloy::{providers::RootProvider, transports::BoxTransport};
use eyre::Result;
use eyre::*;
use revm::primitives::{Address, Bytes, U256};
use serde_json::Value;
use tokio::fs;

use super::Simulator;

/**
 * argus相关的逻辑
 * * 添加ROLE
 * * grantRole
 * * delegate
 * * 添加ACL
 */
pub struct Argus {
    cli: Arc<RootProvider<BoxTransport>>,
    argus_instance: abi::argus::Argus::ArgusInstance<BoxTransport, Arc<RootProvider<BoxTransport>>>,
    safe_instance: abi::argus::Safe::SafeInstance<BoxTransport, Arc<RootProvider<BoxTransport>>>,
    role_manager_instance:
        abi::argus::RoleManager::RoleManagerInstance<BoxTransport, Arc<RootProvider<BoxTransport>>>,
    authorizer_instance:
        abi::argus::Authorizer::AuthorizerInstance<BoxTransport, Arc<RootProvider<BoxTransport>>>,
}

impl Argus {
    async fn new(cli: Arc<RootProvider<BoxTransport>>, argus_addr: Address) -> Result<Argus> {
        let argus_instance: abi::argus::Argus::ArgusInstance<
            BoxTransport,
            Arc<RootProvider<BoxTransport>>,
        > = abi::argus::Argus::new(argus_addr, cli.clone());

        let role_manager_addr = argus_instance.roleManager().call().await.map(|x| x._0)?;

        let role_manager_instance = abi::argus::RoleManager::new(role_manager_addr, cli.clone());

        let authorizer = argus_instance.authorizer().call().await.map(|x| x._0)?;
        let authorizer_instance = abi::argus::Authorizer::new(authorizer, cli.clone());
        let safe = argus_instance.safe().call().await.map(|x| x._0)?;
        let safe_instance = abi::argus::Safe::new(safe, cli.clone());
        return Result::Ok(Argus {
            cli: cli,
            argus_instance,
            role_manager_instance,
            safe_instance,
            authorizer_instance,
        });
    }

    pub fn add_acl_to_bot(
        &self,
        simulator: &mut Simulator,
        acl: Address,
        bot: Address,
    ) -> Result<()> {
        let role = alloy::primitives::FixedBytes::from_hex(
            "0x111636480de784ff000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        let roles = vec![role.clone()];
        let add_role = self
            .role_manager_instance
            .addRoles(roles.clone())
            .calldata()
            .clone();
        simulator.exec_transaction(SimulateTxMsg {
            from: self.safe_instance.address().clone(),
            to: self.role_manager_instance.address().clone(),
            value: U256::ZERO,
            data: add_role,
        })?;
        let grant_role = self
            .role_manager_instance
            .grantRoles(roles, vec![bot])
            .calldata()
            .clone();
        simulator.exec_transaction(SimulateTxMsg {
            from: self.safe_instance.address().clone(),
            to: self.role_manager_instance.address().clone(),
            value: U256::ZERO,
            data: grant_role,
        })?;
        let add_delegate = self
            .argus_instance
            .addDelegate(bot.clone())
            .calldata()
            .clone();
        simulator.exec_transaction(SimulateTxMsg {
            from: self.safe_instance.address().clone(),
            to: self.argus_instance.address().clone(),
            value: U256::ZERO,
            data: add_delegate,
        })?;
        let add_authorizer = self
            .authorizer_instance
            .addAuthorizer(false, role, acl)
            .calldata()
            .clone();
        simulator.exec_transaction(SimulateTxMsg {
            from: self.safe_instance.address().clone(),
            to: self.authorizer_instance.address().clone(),
            value: U256::ZERO,
            data: add_authorizer,
        })?;
        return Result::Ok(());
    }
}

async fn read_bytecode_from_json(f: impl AsRef<Path>) -> Bytes {
    let json_str = fs::read_to_string(f).await.unwrap();
    let json: Value = serde_json::from_str(&json_str).unwrap();

    // 获取字节码
    let bytecode = Bytes::from_hex(
        json["bytecode"]["object"]
            .as_str()
            .ok_or("Failed to get bytecode")
            .unwrap(),
    )
    .unwrap();
    return bytecode;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn test_argus_info() {
    structured_logger::Builder::with_level("info").init();

    // 填写基本信息
    let rpc = "";
    let bot = Address::from_hex("").unwrap();
    let transfer_to = Address::from_hex("").unwrap();
    let rs_eth = Address::from_hex("").unwrap();
    let argus_addr = Address::from_hex("").unwrap();
    let bs = read_bytecode_from_json("").await;

    // 初始化rpc和argus
    let cli = Arc::new(ProviderBuilder::new().on_builtin(rpc).await.unwrap());
    let argus = Argus::new(cli, argus_addr).await.unwrap();

    let mut simulator = Simulator::builder().rpc(rpc).build();

    // 部署acl并添加bot权限
    let acl_addr = simulator.deploy_contract(bot.clone(), bs).unwrap();
    argus.add_acl_to_bot(&mut simulator, acl_addr, bot).unwrap();

    // transfer
    let data = abi::erc20::IERC20::transferCall {
        to: transfer_to,
        amount: U256::from(100000),
    };
    let tx = TransactionRequest::default()
        .with_from(bot)
        .with_to(rs_eth)
        .with_call(&data);

    let argus_tx = build_transaction(argus_addr, tx);
    simulator.exec_transaction(argus_tx);
}
