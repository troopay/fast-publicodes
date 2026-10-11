//! JavaScript number and string semantics the engine reproduces.

/// `String(value)` for a JS number.
pub fn number_to_string(value: f64) -> String {
    ryu_js::Buffer::new().format(value).to_owned()
}

/// `+value.toFixed(16)`: rounded to 16 decimals, halves away from zero.
pub fn round_to_16_decimals(value: f64) -> f64 {
    // From 0.5 on, doubles are more than 1e-16 apart: rounding to 16 decimals moves the value by
    // less than half that gap, so it reads back as the same double.
    if !value.is_finite() || value.abs() >= 0.5 {
        return value;
    }
    // |value| × 10¹⁶ = mantissa × 5¹⁶ × 2^(exponent + 16), exactly, in 128 bits.
    let bits = value.to_bits();
    let fraction = bits & ((1 << 52) - 1);
    let (mantissa, exponent) = match ((bits >> 52) & 0x7ff) as i32 {
        0 => (fraction, -1074),
        biased => (fraction | 1 << 52, biased - 1075),
    };
    let scaled = u128::from(mantissa) * 5u128.pow(16);
    let shift = exponent + 16;
    let units = if shift >= 0 {
        scaled << shift
    } else {
        match shift.unsigned_abs() {
            bits @ 1..=127 => (scaled + (1 << (bits - 1))) >> bits,
            _ => 0,
        }
    };
    // Both are exact doubles, so the division rounds once, as reading the decimals back does.
    let rounded = units as f64 / 1e16;
    if value < 0. { -rounded } else { rounded }
}

/// The high surrogate of `c`, astral, which a JavaScript string can hold alone but a Rust string
/// cannot: written as the private use character `U+F0000 + (surrogate - 0xD800)`.
pub fn lone_high_surrogate(c: char) -> char {
    let high = c.encode_utf16(&mut [0; 2])[0];
    char::from_u32(0xF0000 + u32::from(high - 0xD800)).expect("a private use character")
}

/// JavaScript `\s`, which `trim` also removes.
pub fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// `StringToNumber`.
pub fn string_to_number(text: &str) -> f64 {
    let text = text.trim_matches(is_js_whitespace);
    if text.is_empty() {
        return 0.;
    }
    match text {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = text.strip_prefix(prefix) {
            return u128::from_str_radix(digits, radix).map_or(f64::NAN, |n| n as f64);
        }
    }
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let valid = unsigned
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
        && unsigned
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '.');
    if !valid {
        return f64::NAN;
    }
    text.parse().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipping_the_rounding_from_one_half_keeps_the_same_value() {
        let mut value = 0.5f64;
        while value < 1e22 {
            for candidate in [
                value,
                value.next_up(),
                value * 1.1 + 1e-9,
                0.1 + 0.2 + value,
            ] {
                let formatted: f64 = format!("{candidate:.16}").parse().unwrap();
                assert_eq!(round_to_16_decimals(candidate), formatted, "{candidate}");
            }
            value *= 1.37;
        }
        assert_eq!(round_to_16_decimals(0.1 + 0.2), 0.3);
    }

    #[test]
    fn halves_round_away_from_zero() {
        assert_eq!(
            round_to_16_decimals(0.000_007_629_394_531_25),
            0.000_007_629_394_531_3
        );
        assert_eq!(
            round_to_16_decimals(-0.000_007_629_394_531_25),
            -0.000_007_629_394_531_3
        );
        assert_eq!(round_to_16_decimals(1e-20).to_bits(), 0f64.to_bits());
        assert_eq!(round_to_16_decimals(-1e-20).to_bits(), (-0f64).to_bits());
        assert_eq!(round_to_16_decimals(-0.).to_bits(), 0f64.to_bits());
        assert_eq!(round_to_16_decimals(0.1 / 3.), 0.033_333_333_333_333_3);
    }
}
