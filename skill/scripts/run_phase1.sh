#!/usr/bin/env bash
# Phase 1: Safe 直发 POC
# 用法: bash skill/scripts/run_phase1.sh <project_dir>
set -euo pipefail

PROJECT_DIR="${1:?用法: $0 <project_dir>}"
: "${RPC_URL:?RPC_URL env required}"

cd "$PROJECT_DIR/rust"
RPC_URL="$RPC_URL" cargo test --test phase1_safe_direct -- --ignored --nocapture
