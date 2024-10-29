use std::str::FromStr;

use alloy::{hex::FromHex, signers::{
    k256::ecdsa::SigningKey,
    local::{LocalSigner, PrivateKeySigner},
}};
use eyre::Result;
use revm::primitives::Address;

pub fn new_signer(private_key: &str) -> LocalSigner<SigningKey> {
    let signer = PrivateKeySigner::from_str(private_key).unwrap();
    signer
}


#[test]
fn test_pk(){
    let s = new_signer("0xfa031edc02812621d5a90c72a04e637c0160ed5f81cf0f8cb26505d3a1b80a82");
    assert!(s.address() == Address::from_hex("0x5927ca8bf9807667b1e55f4c82eeB223AaE38775").unwrap());
}