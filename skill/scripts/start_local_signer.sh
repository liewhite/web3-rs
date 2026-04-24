#!/usr/bin/env bash
# 启动本地 cs-evm-signer，监听随机可用端口。
# 输出：
#   CS_SIGNER_PORT=<port>   （可 eval）
#   CS_SIGNER_PID=<pid>
#
# 调用方（run_phase3.sh）在结束时 kill $CS_SIGNER_PID（SIGTERM，cs-signer 优雅关停）。
set -euo pipefail

PROJECT_DIR="${1:?用法: $0 <project_dir>}"
SIGNER_CONF="$PROJECT_DIR/signer"

[ -f "$SIGNER_CONF/config.yaml" ] || { echo "signer config 缺失: $SIGNER_CONF/config.yaml"; exit 1; }
[ -f "$SIGNER_CONF/projects/TEST/rule.js" ] || { echo "rule.js 缺失"; exit 1; }

# 找可用端口
find_free_port() {
    python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'
}

PORT=$(find_free_port)

# 后台启动 signer
cs-evm-signer start --port "$PORT" --config-dir "$SIGNER_CONF" --dev \
    > "$SIGNER_CONF/signer.log" 2>&1 &
PID=$!

# 等 /ping 就绪（最多 20s）
for i in $(seq 1 40); do
    if curl -s "http://127.0.0.1:$PORT/ping" >/dev/null 2>&1; then
        break
    fi
    if ! kill -0 "$PID" 2>/dev/null; then
        echo "signer 启动后立即退出，log:"
        tail -50 "$SIGNER_CONF/signer.log"
        exit 1
    fi
    sleep 0.5
done

if ! curl -s "http://127.0.0.1:$PORT/ping" >/dev/null 2>&1; then
    echo "signer 20s 未就绪，kill 并 log 如下："
    kill "$PID" || true
    tail -50 "$SIGNER_CONF/signer.log"
    exit 1
fi

echo "CS_SIGNER_PORT=$PORT"
echo "CS_SIGNER_PID=$PID"
