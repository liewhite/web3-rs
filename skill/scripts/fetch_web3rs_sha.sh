#!/usr/bin/env bash
# 拿 liewhite/web3-rs main 最新 commit sha（7 字符）。
# Claude 把输出填进生成项目的 Cargo.toml `rev` 字段。
set -euo pipefail

git ls-remote https://github.com/liewhite/web3-rs main | cut -c1-7
