use std::{
    fmt::{self, format},
    str::FromStr,
    time::{self, SystemTime, UNIX_EPOCH},
};

use alloy::{
    consensus::TxEnvelope, network::{EthereumWallet, TransactionBuilder}, providers::{Provider, ProviderBuilder}, rpc::types::TransactionRequest, signers::{local::PrivateKeySigner, Signer}
};
use alloy::eips::{eip2718::Encodable2718};
use alloy::primitives::{hex::ToHexExt, keccak256, Bytes,U256};
use eyre::Result;
use rand::prelude::*;
use serde::Serialize;

#[derive(Serialize)]
pub struct FlashBotRequest {
    jsonrpc: String,
    id: u32,
    method: String,
    params: Vec<FlashBotRequestParams>,
}
#[derive(Serialize)]
pub struct FlashBotRequestParams {
    txs: Vec<String>,
    blockNumber: String,
    minTimestamp: u64,
    maxTimestamp: u64,
    revertingTxHashes: Vec<String>,
    replacementUuid: String,
    builders: Vec<String>,
}

pub struct Flashbot {
    client: reqwest::Client,
}

impl Flashbot {
    pub fn new() -> Flashbot {
        Flashbot {
            client: reqwest::Client::new(),
        }
    }

    pub async fn send_bundle(&self, bundle: Vec<TxEnvelope>, block: u64) -> Result<String> {
        let mut rng = rand::thread_rng();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let txs: Vec<String> = bundle.iter().map(|x| x.encoded_2718().encode_hex_with_prefix()).collect();
        let block_number = format!("0x{:x}", block);
        let body = FlashBotRequest {
            jsonrpc: "2.0".to_string(),
            id: rng.gen(),
            method: "eth_sendBundle".to_string(),
            params: vec![FlashBotRequestParams {
                txs: txs,
                blockNumber: block_number,
                minTimestamp: 0,
                maxTimestamp: ts,
                revertingTxHashes: vec![],
                replacementUuid: "".to_string(),
                builders: vec![
                    "flashbots",
                    "f1b.io",
                    "rsync",
                    "beaverbuild.org",
                    "builder0x69",
                    "Titan",
                    "EigenPhi",
                    "boba-builder",
                    "Gambit Labs",
                    "payload",
                    "Loki",
                    "BuildAI",
                    "JetBuilder",
                    "tbuilder",
                    "penguinbuild",
                    "bobthebuilder",
                    "BTCS",
                    "bloXroute",
                ]
                .iter()
                .map(|x| x.to_string())
                .collect(),
            }],
        };
        let data = serde_json::to_string(&body).unwrap();
        let signer = PrivateKeySigner::random();
        let signer = signer.with_chain_id(Some(1));
        let msg_hash = keccak256(data.as_bytes())
            .as_slice()
            .encode_hex_with_prefix();
        let sig = signer.sign_message(msg_hash.as_bytes()).await.unwrap();
        let header = format!(
            "{}:{}",
            signer.address(),
            sig.as_bytes().encode_hex_with_prefix()
        );

        self.client
            .post("https://relay.flashbots.net")
            .json(&body)
            .header("X-Flashbots-Signature", header)
            .send()
            .await?
            .text()
            .await
            .map_err(|e| e.into())
    }

}

#[tokio::test]
async fn test_bundle() {
    let signer = PrivateKeySigner::from_str("").unwrap();
    let wallet = EthereumWallet::from(signer);

    let provider = ProviderBuilder::new().on_builtin("").await.unwrap();
    let nonce = provider
        .get_transaction_count(wallet.default_signer().address())
        .await
        .unwrap();
    let tx = TransactionRequest::default()
        .with_from(wallet.default_signer().address())
        .with_to(wallet.default_signer().address())
        .with_gas_price(u128::from_str("50000000000").unwrap())
        .with_nonce(nonce)
        .with_gas_limit(21000)
        .with_value(U256::from(0));
    let signed = tx.build(&wallet).await.unwrap();
    let bn = provider.get_block_number().await.unwrap();

    let mev = Flashbot::new();
    let result = mev
        .send_bundle(vec![signed], bn + 1)
        .await
        .unwrap();
    println!("{:?}", result)
}
