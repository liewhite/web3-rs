use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ops::Index,
};

use alloy::{
    hex::FromHex,
    signers::{
        k256::ecdsa::SigningKey,
        local::{coins_bip39::English, LocalSigner, MnemonicBuilder},
    },
};
use revm::primitives::Address;

pub struct MnemonicWallet {
    mnemonic: String,
    password: String,
    current_index: u32
}

impl MnemonicWallet {
    pub fn new(mn: &str, password: &str) -> MnemonicWallet {
        MnemonicWallet {
            mnemonic: mn.to_string(),
            password: password.to_string(),
            current_index: 0,
        }
    }
    pub fn index(&self, index: u32) -> LocalSigner<SigningKey> {
        Self::new_signer(&self.mnemonic, index, &self.password)
    }

    fn new_signer(mn: &str, index: u32, password: &str) -> LocalSigner<SigningKey> {
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
}

impl Iterator for MnemonicWallet {
    type Item = LocalSigner<SigningKey>;
    
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.index(self.current_index);
        self.current_index += 1;
        Some(item)
    }
}

#[test]
fn test_mn() {
    let mut s = MnemonicWallet::new("snack snack indicate legend glue siren acquire spread now forum sibling spawn blossom note merit word peace chat pole much bright member camp ready",  "");
    assert!(
        s.index(0).address() == Address::from_hex("0x5927ca8bf9807667b1e55f4c82eeB223AaE38775").unwrap()
    );
    assert!(
        s.next().unwrap().address() == Address::from_hex("0x5927ca8bf9807667b1e55f4c82eeB223AaE38775").unwrap()
    );
}
