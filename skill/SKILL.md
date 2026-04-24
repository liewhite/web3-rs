---
name: cobosafe
description: 从自然语言需求生成经 Cobo Argus (CoboSafe) + cs-signer 链路的生产级 bot 项目。产出含 ACL 合约、Rust bin、rule.js、Safe 部署 JSON，每个环节有 fork 集成测试强制门禁。仅在用户明确要求"写 CoboSafe / Cobo Argus 交易 bot"时触发。
---

# cobosafe — Cobo Argus 交易 bot 生成器

## 触发条件

用户要写"通过 CoboSafe / Cobo Argus 发交易的 bot"，且明确到：
- 和什么协议交互（有合约地址、有业务目标）
- 要发什么交易
- 通过 cs-signer (remote signer) 签名 + rule.js 校验

**不触发**：用户只是想部署 ACL / 查询 Safe / 发单笔 direct transfer。

## 正确性核心

两道防线 + 业务逻辑 **同一份代码跑三遍**：

```
build_inner_calls(params, safe) ─ 纯函数，无 IO ─ 三遍复用：
  ① Phase 1: caller=safe 直调（证明业务可做，绕过 ACL + rule.js）
  ② Phase 2: delegate 签名 → simulate_raw_tx 走 ACL 链上校验
  ③ Phase 3: 本地 cs-signer 签 → 走 rule.js 链下 + ACL 链上 双校验
```

三遍的**业务断言必须一致**（logs / balance delta）。任何一遍不一致 → 停。

## 前置要求（skill 启动时检查，失败直接停）

| 工具 | 用途 | 验证命令 |
|---|---|---|
| `forge` (Foundry) | ACL 合约编译 / 测试 | `forge --version` |
| `cargo` (stable+) | Rust bin + fork 测试 | `cargo --version` |
| `cs-evm-signer` | 本地 signer（Phase 3） | `which cs-evm-signer` |
| `node` (18+) | 未来可能用 | `node --version` |
| `curl` / `jq` | 脚本工具 | 系统自带 |

有 `RPC_URL` 可访问的主网 RPC（fork 需要）。

## 依赖版本锁

- web3-rs：`https://github.com/liewhite/web3-rs`，skill 启动时执行 `skill/scripts/fetch_web3rs_sha.sh` 拿 main 最新 commit sha，写进生成项目的 `Cargo.toml`
- cobosafe 合约：`ssh://github.com/CoinSummer/cobosafe` tag `v1.0.3`
- cs-evm-signer：用户本地版本

## 4 Phase 流程

### Phase 0: 输入收集 + 业务理解

**问用户**（逐项；缺哪项让用户补）：
1. 项目名字（Rust crate 名 + Foundry 项目目录名，例：`aave_eth_withdraw`）
2. 项目工作目录（默认 `./<project>/`）
3. RPC URL（主网；用于 fork 基础）
4. CoboSafe 地址（已部署的 CoboSafeAccount / Argus）
5. **你想做什么**？（自然语言，例："从 Aave V3 把 Safe 里所有 aWETH 换回 ETH"）
6. **链上环境有什么要求**？（自然语言，例："需要时间推移 14 天后"、"需要 admin 把合约 X 的暂停开关打开"）
   - 若涉及"开关/状态切换"：skill 追问"哪个合约、哪个方法、admin 地址、参数"
   - admin 若是多签：skill 在 fork 上用 admin 地址作为 caller 直调（`disable_balance/nonce_check` 已开）

**Claude 工作**（不问用户，自己做）：
- `flashseal_rs::utils::cobosafe::query_safe(provider, cobosafe)` 查 Safe 地址
- 从业务描述推断 `target` / `method` / `params`：
  - 允许调用 `contract-analyzer` skill 查 Etherscan 源码
  - 允许 `WebFetch` 查协议文档
  - 不确定时问用户澄清（**优先问而不是猜**）
- 推断 FlatRoleManager 地址：从 CoboSafeAccount 查（`cobosafe.roleManager()` 若存在）；没有就问用户

**产出**（写到工作目录的 `notes.md`）：
```markdown
# 执行规格
- 项目: aave_eth_withdraw
- chain_id: 1
- cobosafe: 0x...
- safe: 0x...（从 query_safe 查到）
- role_manager: 0x...
- 业务：inner calls 列表
  1. (target=0xAavePool, method=withdraw, params=...)
- 环境 mock:
  - set_timestamp(...)
  - admin=0x... → 调 X.setSwitch(true) on fork
```

### Phase 1: Safe 直发 POC

**目的**：证明"如果 Safe 能直接发 tx，业务可执行"。绕过 ACL + rule.js + delegate。

**生成文件**：
- `<project>/rust/Cargo.toml`（web3-rs pinned sha）
- `<project>/rust/src/lib.rs` — `build_inner_calls` 函数（参考 `skill/templates/rust/src/lib.rs.template`）
- `<project>/rust/tests/phase1_safe_direct.rs`（参考 `skill/templates/rust/tests/phase1_safe_direct.rs.template`）

**测试内容**：
```rust
fork_for_simulation → 环境 mock → 对 build_inner_calls 的每个 TxRequest
  执行 caller=safe 的 simulate_and_commit → 断言业务成功 + logs
```

**跑**：`bash skill/scripts/run_phase1.sh <project>`

**失败处理**：
- 环境 mock 不对 → Claude 调整 `tests/phase1_safe_direct.rs`
- `build_inner_calls` 参数算错 → Claude 调整 `lib.rs`
- 业务根本做不到 → 停，告诉用户

通过 → 进 Phase 2。**Phase 1 测试不进最终产出**（生产没意义，绕过权限）。

### Phase 2: ACL 编写 + 全权限链测试

**目的**：链上防线（ACL）生效。delegate → CoboSafe → ACL 拦截 → Safe → target，全链路通过。

**生成文件**：
- `<project>/contracts/foundry.toml`（参考 `skill/templates/contracts/foundry.toml.template`）
- `<project>/contracts/soldeer.lock`（cobosafe v1.0.3 pin）
- `<project>/contracts/src/<Name>ACL.sol`（继承 `BaseACL`，参考 `skill/templates/contracts/src/ACL.sol.template`）
- `<project>/contracts/test/<Name>ACL.t.sol`（positive + negative，**对每个 `require()` 至少 1 个 negative case**，参考 `skill/templates/contracts/test/ACL.t.sol.template`）
- `<project>/rust/tests/phase2_full_chain.rs`（参考 `skill/templates/rust/tests/phase2_full_chain.rs.template`）

**ACL 编写要点**（Claude 遵循）：
- 继承 `BaseACL`（cobosafe v1.0.3 `contracts/base/BaseACL.sol`）
- `contracts()` 返回允许的 target 地址数组
- 每个业务方法实现同名检查函数，用 `require(cond, "why")` 表达约束
- 约束来自 Phase 0 的执行规格（recipient == safe、amount <= limit、in whitelist、等）

**测试内容（Rust 侧）**：
```rust
fork_for_simulation → 环境 mock → 
foundry::load_artifact_by_name("./contracts", "<Name>ACL") →
sim.set_code(ACL_ADDR, artifact.deployed_bytecode) →
cobosafe::setup_fork_test_env(&mut sim, cobosafe, ACL_ADDR, delegate) →
cobosafe::add_roles / grant_roles (用 testing_delegate 的地址) →
let (signer, _) = testing_delegate() →
构造 TxEip1559 → signer.sign(tx) → sim.simulate_raw_tx(&raw.0) → 
assert result.success + assert_events_in_order (断言和 Phase 1 一致的业务事件)
```

**跑**：`bash skill/scripts/run_phase2.sh <project>`
（内部先 `forge test` 再 `cargo test --test phase2_full_chain`）

**失败处理**：
- `forge test` 失败 → Claude 修 ACL 或 ACL 测试
- `cargo test` 的 positive fail → ACL 太严（拒绝了合法 tx）
- `cargo test` 的 negative fail → ACL 太宽（放过了非法 tx）
- Claude 分析输出，改代码，重跑。3 次失败停，告诉用户。

### Phase 3: rule.js + 本地 cs-signer 全路径

**目的**：链下防线（rule.js 在 cs-signer 内）+ 链上防线（ACL）同时生效。双防线对**同一 tx** 必须判断一致。

**生成文件**：
- `<project>/signer/config.yaml`（cs-signer 主配置，参考 `skill/templates/signer/config.yaml.template`）
- `<project>/signer/projects/TEST/config.yaml`（TEST 项目配置，参考 `skill/templates/signer/projects/TEST/config.yaml.template`）
  - `public_keys`：`testing_signer_auth_pubkey_hex()` 的值 `8139770ea87d175f56a35466c34c7ecccb8d8a91b4ee37a25df60f5b8fc9b394`
  - `accounts[0].key`：testing delegate 的 secp256k1 私钥 `0x0101...01`
  - `ip_whitelist`：`["127.0.0.1", "::1"]`
- `<project>/signer/projects/TEST/rule.js`（参考 `skill/templates/signer/rule.js.template`）
- `<project>/rust/tests/phase3_rule_chain.rs`（参考 `skill/templates/rust/tests/phase3_rule_chain.rs.template`）

**rule.js 编写要点**：
- 细粒度：解码 CoboSafe `execTransactions(TransactionData[])` 的 input 拿 inner calls
- 对每个 inner call 按 Phase 0 执行规格做等价于 ACL 的检查
- 不通过直接 `return false`

**测试内容**：
```rust
// Phase 3 启动 cs-signer subprocess（scripts/start_local_signer.sh）
// Rust 测试连本地 signer @ 127.0.0.1:<port>
fork + 环境 mock + 部署 ACL + setup_fork_test_env + grant_roles →
let signer = RemoteSigner::new("http://127.0.0.1:<port>".into(), "TEST".into(),
                               TESTING_SIGNER_AUTH_SEED, 0).await? →
构造 unsigned tx → signer.sign(tx).await?  // rule.js 在 cs-signer 内跑
  .expect("rule.js 拒绝了合法 tx") →
sim.simulate_raw_tx(&raw.0) → assert result.success

// Negative：构造应被 rule.js 拒绝的 tx
let bad_tx = ...;
let err = signer.sign(bad_tx).await;
assert!(err.is_err(), "rule.js 放过了非法 tx");
```

**跑**：`bash skill/scripts/run_phase3.sh <project>`
（内部启动 cs-signer → 等 /ping ready → cargo test → kill signer）

**失败处理**：
- Phase 3 positive fail 但 Phase 2 positive pass → rule.js 太严，Claude 放松
- Phase 3 negative fail 但 Phase 2 negative pass → rule.js 太宽（危险！），Claude 加严
- Phase 3 和 Phase 2 判断一致即对齐

### Phase 4: 生产产出

**让用户提供**（**不再用 testing_delegate**）：
- 真实 delegate 地址（已生成的 operator EOA 的 address）
- 真实 delegate 的 ed25519 **公钥**（signer 侧用）
- 生产 cs-signer URL（如 `https://signer.csiodev.com`）
- 生产 cs-signer project 名
- 生产 ed25519 **私钥 seed**（**硬编码进 bin**；用户接受这个选择）

**生成文件**（写到 `<project>/production/`）：
- `src/bin/<name>.rs`（参考 `skill/templates/rust/src/bin/main.rs.template`）
  - 硬编码：RPC_URL / COBOSAFE / SAFE / DELEGATE / SIGNER_URL / SIGNER_PROJECT / ED25519_SEED_HEX
  - clap CLI：业务参数（amount / 等）+ `--gas-price-gwei <N>` + `--dry-run`
- `src/lib.rs`：**直接复制** Phase 1-3 用的 `build_inner_calls`（保持同一份，无修改）
- `Cargo.toml`：web3-rs 同一 sha pin
- `safe-txs/deploy.json`（由 `flashseal_rs::utils::safe_tx_builder` 在 skill 的小辅助脚本里生成）
  - 内容：`enable_module`（若需）+ `set_authorizer(ACL)` + `add_roles` + `grant_roles(roles, [真实 delegate])` + `add_delegate(真实 delegate)`
- `safe-txs/rollback.json`（反向：revoke_roles + remove_delegate + remove_authorizer）
- `safe-txs/deploy_fork_verify.rs`（fork 重放 `deploy.json`，断言后置条件）
- `signer/rule.js`（Phase 3 过的那份原样）
- `README.md`（部署 runbook：forge build → deploy ACL → Safe 导入 deploy.json → 配 cs-signer → 启动 bin）

**跑**：`bash skill/scripts/run_phase4.sh <project>`
（内部 `cargo test --test deploy_fork_verify` 验 `deploy.json`）

## 失败 / 重跑规则

- 每个 Phase 内，Claude 有 **3 次** 自动修复尝试（读测试失败输出 → 改代码 → 重跑）
- 3 次都失败 → 停，汇总失败信息给用户，等用户指示
- 用户调整 Phase 0 需求（比如改金额上限）→ skill 从该 Phase 重新生成（下游也要重跑）

## 工作目录结构（最终）

```
<project>/
├── notes.md                # Phase 0 执行规格，Claude 自用
├── contracts/              # Phase 2 产出
│   ├── foundry.toml
│   ├── src/<Name>ACL.sol
│   └── test/<Name>ACL.t.sol
├── rust/                   # Phase 1-3 测试项目（测试不进最终产出）
│   ├── Cargo.toml
│   ├── src/lib.rs
│   └── tests/
│       ├── phase1_safe_direct.rs    # 不进最终产出
│       ├── phase2_full_chain.rs
│       └── phase3_rule_chain.rs
├── signer/                 # Phase 3 本地 signer 配置
│   ├── config.yaml
│   └── projects/TEST/
│       ├── config.yaml
│       └── rule.js
└── production/             # Phase 4 最终产出
    ├── Cargo.toml
    ├── src/lib.rs          # 和 rust/src/lib.rs 的 build_inner_calls 完全相同
    ├── src/bin/<name>.rs
    ├── safe-txs/
    │   ├── deploy.json
    │   ├── rollback.json
    │   └── (skill/templates/rust/tests/deploy_fork_verify.rs.template 复制来)
    ├── signer/
    │   └── rule.js
    └── README.md
```

## 产物清单（交付给用户的）

- `<project>/production/` 里所有内容
- 简版状态：
  - ACL artifact 产物在 `contracts/out/<Name>ACL.sol/<Name>ACL.json`
  - production/README.md 是部署流程
  - production 目录可以独立 `cargo build --release` 出 bin 二进制

## 绝对不做的事

- 不自动发真 tx 到主网（没有 `--execute` 开关）
- 不读用户的真 private_key 文件
- 不改用户的 Safe 配置（skill 只生成 JSON，是否导入 Safe UI 由用户决定）
- 不 push 任何 git repo
- 不依赖 warden CLI（参考 cs-argus-agent 的流程，但不调它）
