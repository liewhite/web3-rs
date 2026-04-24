//! 测试专用的稳定值，让 Phase 2 / Phase 3 fork 测试有可重现的输入。
//!
//! **仅限本地 fork / 本地 cs-signer 使用**。所有常量都是公开可见的固定私钥，
//! 不可用于主网。

use alloy::{
    primitives::{address, Address, B256},
    signers::local::PrivateKeySigner,
};
use ed25519_dalek::SigningKey;

use crate::LocalSigner;

/// 测试 delegate 的固定私钥：`0x0101...01`。
pub const TESTING_DELEGATE_PRIVKEY: B256 = B256::new([0x01; 32]);

/// 对应地址：`0x1a642f0e3c3af545e7acbd38b07251b3990914f1`（privkey = 0x01*32 派生）
pub const TESTING_DELEGATE_ADDRESS: Address =
    address!("1a642f0e3c3af545e7acbd38b07251b3990914f1");

/// 把固定私钥包装成 `LocalSigner`，返回 `(signer, address)`。
///
/// 用法：
/// ```ignore
/// use flashseal_rs::utils::testing::testing_delegate;
/// let (signer, delegate) = testing_delegate();
/// cobosafe::grant_roles(&mut sim, safe, role_manager, &[role], &[delegate])?;
/// let raw = signer.sign(unsigned_tx).await?;
/// sim.simulate_raw_tx(&raw.0)?;
/// ```
///
/// 业务逻辑和 delegate 地址无关（ACL 通过 FlatRoleManager 判断角色），
/// 所以 skill / 测试用这个固定 delegate，生产切换真实 delegate，无需改 ACL。
pub fn testing_delegate() -> (LocalSigner, Address) {
    let pk = PrivateKeySigner::from_slice(TESTING_DELEGATE_PRIVKEY.as_slice())
        .expect("constant privkey is valid secp256k1 scalar");
    let addr = pk.address();
    (LocalSigner::new(pk), addr)
}

/// 测试客户端认证 cs-signer 用的 ed25519 seed：`0x0202...02`。
///
/// cs-signer 的客户端（[`crate::RemoteSigner`]）需要一对 ed25519 keypair 做
/// 请求认证。测试场景用这个稳定值，skill 的 signer `projects/<name>/config.yaml`
/// 里 `public_keys` 直接写 [`testing_signer_auth_pubkey_hex`] 的返回值。
pub const TESTING_SIGNER_AUTH_SEED: [u8; 32] = [0x02; 32];

/// 返回 [`TESTING_SIGNER_AUTH_SEED`] 对应的 ed25519 public key hex（32 字节，无 0x 前缀）。
///
/// skill 启动本地 cs-signer 时把它写进项目配置 `public_keys` map。
pub fn testing_signer_auth_pubkey_hex() -> String {
    let sk = SigningKey::from_bytes(&TESTING_SIGNER_AUTH_SEED);
    alloy::hex::encode(sk.verifying_key().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testing_delegate_address_is_stable() {
        let (_, addr) = testing_delegate();
        assert_eq!(addr, TESTING_DELEGATE_ADDRESS);
    }

    #[test]
    fn testing_signer_auth_pubkey_is_stable() {
        // seed = [0x02; 32] → ed25519 公钥始终相同。
        // 这个值被 skill 模板的 signer config 引用，不能变。
        let pk = testing_signer_auth_pubkey_hex();
        assert_eq!(
            pk, "8139770ea87d175f56a35466c34c7ecccb8d8a91b4ee37a25df60f5b8fc9b394",
            "testing_signer_auth_pubkey changed; update skill/templates/signer/*.yaml"
        );
    }
}
