use std::path::Path;

use alloy::{
    hex::FromHex,
    signers::{k256::ecdsa::SigningKey, local::LocalSigner},
};
use revm::primitives::Address;

pub fn new_signer(keystore_path: &str, password: &str) -> LocalSigner<SigningKey> {
    let keystore_file_path = Path::new(keystore_path);
    println!("{:?}", keystore_file_path.exists());
    println!("{:?}", keystore_file_path);
    let signer = LocalSigner::decrypt_keystore(keystore_file_path, password).unwrap();
    signer
}

#[test]
fn test_keystore() {
    let s = new_signer("./ks.json", "123");
    assert!(
        s.address() == Address::from_hex("0x5927ca8bf9807667b1e55f4c82eeB223AaE38775").unwrap()
    );
}
