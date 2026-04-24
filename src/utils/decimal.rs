//! decimal 字符串 <-> raw U256 转换。

use alloy::primitives::U256;
use eyre::{Result, WrapErr};

/// `"1000.5"` + `decimals=18` → `1000.5 * 10^18` raw。
///
/// 不允许：空字符串、多个小数点、小数位超过 `decimals`。
pub fn parse_decimal_units(s: &str, decimals: u8) -> Result<U256> {
    eyre::ensure!(!s.is_empty(), "empty decimal string");
    let (int_part, dec_part) = match s.split_once('.') {
        Some((i, d)) => (i, d),
        None => (s, ""),
    };
    eyre::ensure!(
        !dec_part.contains('.'),
        "invalid decimal format (multiple dots): {s}"
    );
    eyre::ensure!(
        dec_part.len() <= decimals as usize,
        "{s} has more decimals than allowed ({decimals})"
    );
    let mut combined = String::new();
    combined.push_str(int_part);
    combined.push_str(dec_part);
    eyre::ensure!(!combined.is_empty(), "invalid decimal format: {s}");
    let value: U256 = combined
        .parse()
        .wrap_err_with(|| format!("failed to parse {s} as integer"))?;
    let pad = decimals as usize - dec_part.len();
    let mult = U256::from(10u64).pow(U256::from(pad));
    Ok(value * mult)
}

/// raw U256 → human f64。**仅用于打印**；极大 raw 值会丢失精度（f64 仅 53 位尾数）。
pub fn raw_to_human(raw: U256, decimals: u8) -> f64 {
    let as_u128: u128 = raw.try_into().unwrap_or(u128::MAX);
    as_u128 as f64 / 10f64.powi(decimals as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_decimal_units_integer() {
        let v = parse_decimal_units("1000", 18).unwrap();
        assert_eq!(v, U256::from(1000u64) * U256::from(10u64).pow(U256::from(18u64)));
    }

    #[test]
    fn test_parse_decimal_units_fraction() {
        let v = parse_decimal_units("1.5", 6).unwrap();
        assert_eq!(v, U256::from(1_500_000u64));
    }

    #[test]
    fn test_parse_decimal_units_too_many_decimals() {
        assert!(parse_decimal_units("1.123456789", 6).is_err());
    }
}
