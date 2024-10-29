use alloy::{hex::FromHex, signers::{
    k256::ecdsa::SigningKey,
    local::{coins_bip39::English, LocalSigner, MnemonicBuilder},
}};
use revm::primitives::Address;

pub fn new_signer(mn: &str, index: u32, password: &str) -> LocalSigner<SigningKey> {
    let wallet: LocalSigner<SigningKey> = MnemonicBuilder::<English>::default()
        .phrase(mn)
        .index(index)
        .unwrap()
        // Use this if your mnemonic is encrypted.
        .password(password)
        .build()
        .unwrap();
    wallet
}

#[test]
fn test_mn(){
    let s = new_signer("snack snack indicate legend glue siren acquire spread now forum sibling spawn blossom note merit word peace chat pole much bright member camp ready", 0, "");
    assert!(s.address() == Address::from_hex("0x5927ca8bf9807667b1e55f4c82eeB223AaE38775").unwrap());
}