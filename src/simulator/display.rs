use alloy::primitives::U256;
use revm::context::TxEnv;

use super::decoder::AbiDecoder;
use super::fork::SimulationResult;

/// 格式化输出模拟结果
pub fn display_result(result: &SimulationResult, tx: &TxEnv, decoder: Option<&AbiDecoder>) {
    println!("\n{}", "═".repeat(40));
    println!("  Transaction Simulation");
    println!("{}", "═".repeat(40));

    // Status
    if result.success {
        println!("Status:    SUCCESS");
    } else {
        println!("Status:    REVERTED");
    }

    // From / To
    println!("From:      {:?}", tx.caller);
    match &tx.kind {
        alloy::primitives::TxKind::Call(addr) => println!("To:        {:?}", addr),
        alloy::primitives::TxKind::Create => println!("To:        CREATE"),
    }

    // Value
    if tx.value > U256::ZERO {
        println!("Value:     {} wei", tx.value);
    }

    // Calldata
    let calldata = &tx.data;
    if !calldata.is_empty() {
        println!("\n── Calldata ──");
        println!("Raw:       0x{}", alloy::hex::encode(calldata));
        if let Some(dec) = decoder {
            if let alloy::primitives::TxKind::Call(to) = &tx.kind {
                if let Some(decoded) = dec.decode_calldata(to, calldata) {
                    println!("Decoded:   {}(", decoded.name);
                    for (name, val) in &decoded.params {
                        println!("             {name}: {val}");
                    }
                    println!("           )");
                }
            }
        }
    }

    // Result
    println!("\n── Result ──");
    println!("Gas Used:  {}", result.gas_used);
    if result.gas_refunded > 0 {
        println!("Refunded:  {}", result.gas_refunded);
    }
    if let Some(ref output) = result.output {
        if !output.is_empty() {
            println!("Output:    0x{}", alloy::hex::encode(output));
        }
    }
    if let Some(ref addr) = result.created_address {
        println!("Created:   {:?}", addr);
    }

    // Events
    if !result.logs.is_empty() {
        println!("\n── Events ({}) ──", result.logs.len());
        for (i, log) in result.logs.iter().enumerate() {
            let decoded = decoder.and_then(|d| d.decode_log(log));
            if let Some(decoded) = decoded {
                let params: Vec<String> = decoded
                    .params
                    .iter()
                    .map(|(name, val, _)| format!("{name}: {val}"))
                    .collect();
                println!(
                    "[{i}] {}({})  @ {:?}",
                    decoded.name,
                    params.join(", "),
                    log.address
                );
            } else {
                println!("[{i}] {:?}  @ {:?}", log.topics(), log.address);
            }
        }
    }

    // Revert reason
    if let Some(ref reason) = result.revert_reason {
        println!("\n── Revert Reason ──");
        println!("Error: {reason}");
    }

    println!("{}\n", "═".repeat(40));
}
