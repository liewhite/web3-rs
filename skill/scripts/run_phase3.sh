#!/usr/bin/env bash
# Phase 3: rule.js + 本地 cs-signer 全路径
# 启停 cs-signer + 跑 Rust fork 测试
set -euo pipefail

PROJECT_DIR="${1:?用法: $0 <project_dir>}"
: "${RPC_URL:?RPC_URL env required}"

# 启 signer
output=$(bash "$(dirname "$0")/start_local_signer.sh" "$PROJECT_DIR")
eval "$output"     # 导入 CS_SIGNER_PORT / CS_SIGNER_PID
echo "signer running: pid=$CS_SIGNER_PID port=$CS_SIGNER_PORT"

# 保证退出时 kill signer
trap 'kill "$CS_SIGNER_PID" 2>/dev/null || true; wait "$CS_SIGNER_PID" 2>/dev/null || true' EXIT

# 跑测试
cd "$PROJECT_DIR/rust"
RPC_URL="$RPC_URL" CS_SIGNER_PORT="$CS_SIGNER_PORT" \
    cargo test --test phase3_rule_chain -- --ignored --nocapture
