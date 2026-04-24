#!/usr/bin/env bash
# Phase 4: 生产产出的 fork 重放验证
# (deploy.json 生成由 skill 用 web3-rs::utils::safe_tx_builder 做，脚本不负责)
set -euo pipefail

PROJECT_DIR="${1:?用法: $0 <project_dir>}"
: "${RPC_URL:?RPC_URL env required}"

cd "$PROJECT_DIR/production"

# 验 Cargo 项目本身能 build
cargo check

# 跑 deploy_fork_verify 测试
RPC_URL="$RPC_URL" cargo test --test deploy_fork_verify -- --ignored --nocapture

echo "Phase 4 fork 验证通过。最终产物路径:"
echo "  contracts/out/       (ACL 编译产物)"
echo "  production/          (Rust bin + safe-txs + signer/rule.js + README)"
