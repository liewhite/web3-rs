#!/usr/bin/env bash
# Phase 2: ACL 编写 + 全权限链测试
#   1. forge soldeer install（第一次拉依赖）
#   2. forge test（ACL 合约单元测试，positive + negative）
#   3. cargo test phase2_full_chain（Rust fork 集成测试）
set -euo pipefail

PROJECT_DIR="${1:?用法: $0 <project_dir>}"
: "${RPC_URL:?RPC_URL env required}"

cd "$PROJECT_DIR/contracts"
if [ ! -d "dependencies/cobosafe-1.0.3" ]; then
    echo "首次拉 cobosafe v1.0.3 + forge-std + openzeppelin-contracts..."
    forge soldeer install
fi
forge test -vv

cd "../rust"
RPC_URL="$RPC_URL" cargo test --test phase2_full_chain -- --ignored --nocapture
