use std::path::Path;
use std::sync::Arc;

use crate::abi;
use crate::argus::build_transaction;
use crate::simulator::SimulateTxMsg;
use alloy::hex::FromHex;
use alloy::network::TransactionBuilder;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::TransactionRequest;
use alloy::sol_types::sol_data::FixedBytes;
use alloy::sol_types::SolCall;
use alloy::{providers::RootProvider, transports::BoxTransport};
use eyre::Result;
use eyre::*;
use revm::primitives::{fixed_bytes, Address, Bytes, TxKind, U256};
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
        let argus_instance = abi::argus::Argus::new(argus_addr, cli.clone());

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
        let role = alloy::primitives::fixed_bytes!(
            "111636480de784ff000000000000000000000000000000000000000000000000"
        );
        let roles = vec![role.clone()];
        let add_role = self
            .role_manager_instance
            .addRoles(roles.clone())
            .calldata()
            .clone();
        simulator.exec_transaction(SimulateTxMsg {
            from: self.safe_instance.address().clone(),
            to: Some(self.role_manager_instance.address().clone()).into(),
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
            to: Some(self.role_manager_instance.address().clone()).into(),
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
            to: Some(self.argus_instance.address().clone()).into(),
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
            to: Some(self.authorizer_instance.address().clone()).into(),
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
    let rs_eth = Address::from_hex("0xA1290d69c65A6Fe4DF752f95823fae25cB99e5A7").unwrap();
    let argus_addr = Address::from_hex("").unwrap();
    let bs = read_bytecode_from_json("").await;

    // 初始化rpc和argus
    let cli = Arc::new(ProviderBuilder::new().on_builtin(rpc).await.unwrap());
    let argus = Argus::new(cli, argus_addr).await.unwrap();

    let cli = ProviderBuilder::new().on_builtin(rpc).await.unwrap();
    let mut simulator = Simulator::builder(cli).build();

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

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn test_rs_eth_once() {
    structured_logger::Builder::with_level("info").init();

    // 填写基本信息
    let rpc = "https://eth-mainnet.g.alchemy.com/v2/jP0h5UEZoR7Wpww9tnNKPGihmNwEkECH";
    let bot = Address::from_hex("0x1108691fAd7cE639fd465e870936f13161741530").unwrap();
    let argus_addr = Address::from_hex("0x80b1ADF81A6a7B7a8E1f587Abf29DD0445b5Eb5E").unwrap();
    let rs_eth = Address::from_hex("0xA1290d69c65A6Fe4DF752f95823fae25cB99e5A7").unwrap();
    let wst_eth = Address::from_hex("0x7f39C581F595B53c5cb19bD0b3f8dA6c935E2Ca0").unwrap();
    let st_eth = Address::from_hex("0xae7ab96520de3a18e5e111b5eaab095312d7fe84").unwrap();
    let pool_addr = Address::from_hex("0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2").unwrap();
    let rs_deposit = Address::from_hex("0x036676389e48133B63a802f8635AD39E752D375D").unwrap();
    let bs = read_bytecode_from_json(
        "/Users/lee/repos/sol/cobosafe/out/AaveRsETHACL.sol/AaveRsETHACL.json",
    )
    .await;

    // 初始化rpc和argus
    let cli = Arc::new(ProviderBuilder::new().on_builtin(rpc).await.unwrap());
    let argus = Argus::new(cli.clone(), argus_addr).await.unwrap();
    println!("safe: {}", argus.safe_instance.address());
    let safe_addr = argus.safe_instance.address().clone();

    let cli = ProviderBuilder::new().on_builtin(rpc).await.unwrap();
    let mut simulator = Simulator::builder(cli)
        .height(21393635)
        ._timestamp(1734266881)
        .build();

    // 部署acl并添加bot权限
    let acl_addr = simulator.deploy_contract(bot.clone(), bs).unwrap();
    argus.add_acl_to_bot(&mut simulator, acl_addr, bot).unwrap();

    // approve rseth to pool
    simulator.exec_transaction(SimulateTxMsg {
        from: safe_addr,
        to: Some(rs_eth.clone()).into(),
        value: U256::ZERO,
        data: abi::erc20::IERC20::approveCall {
            spender: pool_addr,
            amount: U256::from_str_radix("10000000000000000000000", 10).unwrap(),
        }
        .abi_encode()
        .into(),
    });

    // approve steth to deposit
    simulator.exec_transaction(SimulateTxMsg {
        from: safe_addr,
        to: Some(st_eth.clone()).into(),
        value: U256::ZERO,
        data: abi::erc20::IERC20::approveCall {
            spender: rs_deposit,
            amount: U256::from_str_radix("10000000000000000000000", 10).unwrap(),
        }
        .abi_encode()
        .into(),
    });

    // set e mode
    simulator.exec_transaction(SimulateTxMsg {
        from: safe_addr,
        to: Some(pool_addr.clone()).into(),
        value: U256::ZERO,
        data: abi::aave::Pool::setUserEModeCall { categoryId: 3 }
            .abi_encode()
            .into(),
    });

    // 开始业务逻辑
    // supply
    let supply_call = abi::aave::Pool::supplyCall {
        asset: rs_eth,
        amount: U256::from_str_radix("1000000000000000000", 10).unwrap(),
        onBehalfOf: safe_addr,
        referralCode: 0,
    };
    let tx = build_transaction(
        argus_addr,
        TransactionRequest::default()
            .with_from(bot)
            .with_to(pool_addr)
            .with_call(&supply_call),
    );
    simulator.exec_transaction(tx);

    let borrow_call = abi::aave::Pool::borrowCall {
        asset: wst_eth,
        amount: U256::from_str_radix("500000000000000000", 10).unwrap(),
        interestRateMode: U256::from(2_u32),
        referralCode: 0,
        onBehalfOf: safe_addr,
    };
    let tx = build_transaction(
        argus_addr,
        TransactionRequest::default()
            .with_from(bot)
            .with_to(pool_addr)
            .with_call(&borrow_call),
    );
    simulator.exec_transaction(tx);

    let unwrap_call = abi::wsteth::WstEth::unwrapCall {
        _wstETHAmount: U256::from_str_radix("500000000000000000", 10).unwrap(),
    };
    let tx = build_transaction(
        argus_addr,
        TransactionRequest::default()
            .with_from(bot)
            .with_to(wst_eth)
            .with_call(&unwrap_call),
    );
    simulator.exec_transaction(tx);

    let mint_rs_eth_call = abi::rseth::RsEth::depositAssetCall {
        asset: st_eth,
        depositAmount: U256::from_str_radix("100000000000000000", 10).unwrap(),
        minRSETHAmountExpected: U256::ZERO,
        referralId: "0xd05723c7b17b4e4c722ca4fb95e64ffc54a70131c75e2b2548a456c51ed7cdaf"
            .to_string(),
    };
    let tx = build_transaction(
        argus_addr,
        TransactionRequest::default()
            .with_from(bot)
            .with_to(rs_deposit)
            .with_call(&mint_rs_eth_call),
    );
    simulator.exec_transaction(tx);
}
