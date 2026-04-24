#!/usr/bin/env bash
# skill 启动时检查前置工具。缺工具 exit 1，打印清晰消息。
set -euo pipefail

missing=()

check() {
    local tool="$1"
    local install_hint="$2"
    if ! command -v "$tool" >/dev/null 2>&1; then
        missing+=("$tool — $install_hint")
    fi
}

check forge "install foundry (https://getfoundry.sh)"
check cargo "install rustup + stable toolchain"
check cs-evm-signer "build from https://github.com/CoinSummer/cs-signer (go build)"
check node "install Node.js 18+"
check curl "系统自带"

if [ ${#missing[@]} -ne 0 ]; then
    echo "缺少工具："
    for m in "${missing[@]}"; do echo "  - $m"; done
    exit 1
fi

echo "all tools ok"
