# cobosafe skill 架构

## 正确性核心

```
同一份 build_inner_calls(params, safe) 纯函数，跑三遍：

Phase 1   caller=safe 直调 target       证明"业务可做"（绕过 ACL）
Phase 2   delegate 签 → simulate_raw_tx → ACL 链上校验
Phase 3   本地 cs-signer 签 → rule.js 校验 + ACL 链上校验 双防线

断言同一组业务事件 & 状态变化，不一致就停。
```

## Phase 流水线

```
Phase 0: 输入
   用户: rpc_url, cobosafe, 业务自然语言, 环境要求
   Claude: query_safe + 推断 target/method/params + 翻译环境 mock
   产出: <project>/notes.md (skill 自用)

Phase 1: Safe 直发 POC
   代码: <project>/rust/{Cargo.toml, src/lib.rs, tests/phase1_safe_direct.rs}
   gate: cargo test phase1_safe_direct

Phase 2: ACL + 全权限链
   代码: <project>/contracts/{foundry.toml, src/*.sol, test/*.t.sol}
        <project>/rust/tests/phase2_full_chain.rs
   gate: forge test + cargo test phase2_full_chain
   Claude 强制: ACL 每个 require() 至少对应 1 个 negative Rust case

Phase 3: rule.js + 本地 cs-signer
   代码: <project>/signer/{config.yaml, projects/TEST/{config.yaml, rule.js}}
        <project>/rust/tests/phase3_rule_chain.rs
   gate: run_phase3.sh（起 signer + cargo test + 停 signer）

Phase 4: 生产产出
   代码: <project>/production/{Cargo.toml, src/{lib.rs, bin/*.rs}, safe-txs/*.json, signer/rule.js, README.md}
   gate: cargo test deploy_fork_verify（fork 重放 deploy.json 断言后置条件）
```

## 关键数据流

```
build_inner_calls(params, safe)
      ├─ Phase 1: 每 TxRequest → TxEnv { caller: safe } → simulate_and_commit
      ├─ Phase 2: CoboSafeBuilder::build_txs → testing_delegate.sign → simulate_raw_tx
      ├─ Phase 3: CoboSafeBuilder::build_txs → RemoteSigner(localhost).sign → simulate_raw_tx
      └─ Phase 4: 同 Phase 3 路径，但 bin 里用生产 RemoteSigner + RpcSender::send_txs

cobosafe v1.0.3 合约接口:
      TransactionData { flag, to, value, data, hint, extra }
      AuthorizerReturnData { result: FAILED|SUCCESS, message, data }
      CoboSafe.execTransactions(TransactionData[])
      FlatRoleManager.addRoles([bytes32]) / grantRoles([bytes32], [address])
```

## skill / web3-rs / cs-signer / cobosafe 版本锁

- **web3-rs**: skill/scripts/fetch_web3rs_sha.sh 每次生成项目时拿 main 最新 sha，写进 Cargo.toml rev
- **cobosafe**: foundry.toml.template 固定 `v1.0.3`（ssh://github.com/CoinSummer/cobosafe）
- **cs-signer**: 依赖用户本地 `cs-evm-signer` binary（skill/scripts/ensure_tools.sh 检查）
- **solc**: foundry.toml 固定 `0.8.26`，via_ir on

## 失败处理

每 Phase 自动 gate，失败 Claude 读 stderr 分析改代码，**最多 3 次**，都失败则停给用户。

Claude 的 diagnostic 优先级：
1. 看 cargo / forge 的 revert_reason（通常匹配 ACL require message）
2. 看 logs（事件没 emit 或多 emit）
3. 对比 Phase 1 / Phase 2 / Phase 3 差异（漂移定位）
