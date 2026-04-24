//! 测试专用的稳定值，让 Phase 2 / Phase 3 fork 测试有可重现的输入。
//!
//! **仅限本地 fork 模拟使用**。任何常量都是公开可见的固定私钥，不可用于主网。

use alloy::{
    primitives::{address, Address, B256},
    signers::local::PrivateKeySigner,
};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testing_delegate_address_is_stable() {
        let (_, addr) = testing_delegate();
        assert_eq!(addr, TESTING_DELEGATE_ADDRESS);
    }
}
