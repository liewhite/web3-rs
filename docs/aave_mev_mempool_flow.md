# aave_mev_mempool 执行流程

对应 `src/bin/aave_mev_mempool.rs`。用 mermaid 画，在支持 mermaid 的 Markdown viewer（VS Code、GitHub、Obsidian 等）里能直接渲染。

## 1. 整体主循环

启动 → 初始化依赖 → tokio::select 同时消费 pending tx 流和新 block 流。

```mermaid
flowchart TD
    Start([启动]) --> Init["加载 config.json<br/>初始化 tracing<br/>RemoteSigner → operator<br/>HTTP/WS provider<br/>query_safe → Safe 地址<br/>FlashbotsSender / CoboSafeBuilder<br/>min_wei"]
    Init --> Sub["订阅 pending_stream<br/>+ block_stream"]
    Sub --> Loop{tokio::select}

    Loop -->|pending tx| A1[tx_count++<br/>每 100 笔打日志]
    A1 --> A2{合约创建?<br/>is_noise?}
    A2 -->|是| Loop
    A2 -->|否| A3["fork_and_filter<br/>fork latest + replay<br/>扫 Supply/Repay WETH useATokens=false"]
    A3 -->|revert 或无事件| Loop
    A3 -->|命中事件| A4["log 目标 tx<br/>tx_hash/from/to/nonce/value/gas_limit"]
    A4 --> A5[max_withdraw_on_fork]
    A5 -->|None| A5s["log skipped<br/>aWETH 不足"] --> Loop
    A5 -->|Some amount,gas,aweth| A6["dispatch_bundles<br/>candidate_prefix=Some"]
    A6 --> Loop

    Loop -->|新 block| B1["fork latest<br/>disable balance/nonce check"]
    B1 --> B2["query WETH.balanceOf aEthWETH<br/>= pool_idle"]
    B2 -->|pool_idle < min_wei| Loop
    B2 --> B3[max_withdraw_on_fork]
    B3 -->|None| Loop
    B3 -->|Some| B4["log pool idle"]
    B4 --> B5["dispatch_bundles<br/>candidate_prefix=None"]
    B5 --> Loop
```

## 2. max_withdraw_on_fork

在已 filter 通过的 fork 上做上边界二分，找出最大能 withdraw 的额度 + 对应 gas_used。

```mermaid
flowchart TD
    MW0([sim, builder, operator, safe, min_wei]) --> MW1["query aWETH.balanceOf safe"]
    MW1 --> MW2["max_wei = min(MAX_ETH=5000, aWETH)"]
    MW2 -->|max_wei < min_wei| MWN([return None])
    MW2 --> MW3[try_withdraw min_wei]
    MW3 -->|revert| MWN
    MW3 -->|Some initial_gas| MW4["lo=min_wei, hi=max_wei,<br/>best_gas=initial_gas"]
    MW4 --> MW5{iter < 20<br/>且 lo < hi?}
    MW5 -->|否| MWD(["return Some<br/>lo, best_gas, aweth_balance"])
    MW5 -->|是| MW6["mid = lo + (hi-lo+1)/2"]
    MW6 --> MW7[try_withdraw mid]
    MW7 -->|Some gas| MW8["lo=mid<br/>best_gas=gas"]
    MW7 -->|None| MW9["hi = mid - 1"]
    MW8 --> MW5
    MW9 --> MW5
```

### try_withdraw 单次模拟

```mermaid
flowchart LR
    T1["withdraw_request(amount, safe, gas_limit=0)<br/>= TxRequest to=WETH_GATEWAY,<br/>data=withdrawETH(pool,amount,safe)"] --> T2["CoboSafeBuilder.build_txs<br/>→ 外层 execTransaction CallData"]
    T2 --> T3["sim.simulate TxEnv<br/>caller=operator<br/>to=CoboSafe<br/>gas_limit=2M"]
    T3 --> T4{success?}
    T4 -->|是| T5[return Some gas_used]
    T4 -->|否| T6[return None]
```

## 3. dispatch_bundles

统一的 bundle 分发流程。拿到 max amount 后，生成 4 个 plan（100% + 70/50/30%），每个单独签名并发 bundle。

```mermaid
flowchart TD
    DB0([入口:<br/>sim, amount, sim_gas_used, aweth_balance,<br/>base_gas_price_gwei, gas_price_factor,<br/>candidate_prefix: Option RawTx]) --> DB1["log aWETH, max withdraw, sim gas"]
    DB1 --> DB2["op_nonce = provider.get_transaction_count operator"]
    DB2 --> DB3["plans = 100%, 70%, 50%, 30% of amount<br/>（共用 op_nonce）"]
    DB3 --> DB4{遍历 plans}
    DB4 -->|label, amt| DB5[simulate_build_sign]
    DB5 -->|Err 或 None| DB4
    DB5 -->|Ok Some| DB6["log 该 tier 的 amt / sim_gas / gas_limit / gas_price"]
    DB6 --> DB7{label == 100%<br/>且 candidate_prefix = Some?}
    DB7 -->|是| DB8["bundle = candidate, withdraw<br/>log 两个 tx 的 hex+JSON"]
    DB7 -->|否| DB9["bundle = withdraw<br/>log 一个 tx 的 hex+JSON"]
    DB8 --> DB10[send_bundle → FlashbotsSender.send_txs]
    DB9 --> DB10
    DB10 --> DB11["log 每个 block+N 的 bundle hash"]
    DB11 --> DB4
```

### simulate_build_sign

一个 plan（label, amt）内部：模拟 → 算 gas → 构建 → 远程签名。

```mermaid
flowchart LR
    S1[try_withdraw amt → sim_gas_used] --> S2["gas_limit = sim_gas × 1.3"]
    S2 --> S3["gas_price_wei = compute_gas_price_wei amt, base, factor"]
    S3 --> S4["CoboSafeBuilder.build_txs<br/>max_fee = max_priority = gas_price_wei<br/>→ TxEip1559"]
    S4 --> S5["RemoteSigner.sign tx<br/>远程签名服务<br/>type=0x2"]
    S5 --> S6([return raw, sim_gas, gas_limit, gas_price_wei])
```

### compute_gas_price_wei

```mermaid
flowchart LR
    G0([amount_wei, base_gwei, factor_gwei]) --> G1["base_wei = base × 1e9"]
    G1 --> G2["incr_wei = amount × factor × 1e9 / 100ETH_wei"]
    G2 --> G3["total = base_wei + incr_wei"]
    G3 --> G4{amount > 500 ETH?}
    G4 -->|是| G5["total × 10"]
    G4 -->|否| G6[pass]
    G5 --> G7["max 1 gwei"]
    G6 --> G7
    G7 --> G8([return total_wei])
```

## 4. FlashbotsSender.send_txs

对 3 个 target block × 21 个 builder 两层 fan-out，并发 POST `eth_sendBundle`。

```mermaid
flowchart TD
    F1[block = get_block_number] --> F2["并发 offset ∈ 1..=3"]
    F2 --> F3[send_bundle txs, block+offset]
    F3 --> F4["body = eth_sendBundle<br/>params: txs, blockNumber<br/>无 builders 字段"]
    F4 --> F5["Flashbots 签名 header<br/>EIP-191 personal_sign"]
    F5 --> F6["并发 POST 到 21 个硬编码 builder URL"]
    F6 --> F7["每个 builder 的响应独立 log<br/>builder:name block=.. status=.. body=.."]
    F7 --> F8[聚合：第一个拿到 bundleHash 返回]
```

## 关键特性

| 特性 | 细节 |
|---|---|
| **nonce 共用** | 4 个 bundle 共享同一 `op_nonce`，链上至多 1 条 include，其余因 nonce 自动作废——这是"候选被夹走时抢剩余流动性"的机制 |
| **两路并行** | `tokio::select!` 多路复用 pending 流和 block 流；block 路径独立触发，不依赖 mempool 信号 |
| **filter 两层信号强度** | L1 selector 黑名单（ns 级）排明显噪音；L2 事件检测（百 ms 级 fork）精准过滤 |
| **gas 独立** | 每个 plan 单独跑 sim 取 gas_used ×1.3；gas_price 也按该 plan 的 amount 独立梯度缩放 |
| **Bundle fan-out 规模** | 1 次触发 = 4 plans × 3 blocks × 21 builders = **252** 次 HTTP POST |
| **Tx 类型** | EIP-1559 (type 0x2)，`max_fee = max_priority = gas_price_wei` |
| **资金路径** | Operator EOA → CoboSafe.execTransaction → Safe → WETHGateway.withdrawETH → Aave Pool |
