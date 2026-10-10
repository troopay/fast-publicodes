//! JavaScript number semantics the engine reproduces.

/// `String(value)` for a JS number.
pub fn number_to_string(value: f64) -> String {
    ryu_js::Buffer::new().format(value).to_owned()
}

/// `+value.toFixed(16)`.
pub fn round_to_16_decimals(value: f64) -> f64 {
    // From 0.5 on, doubles are more than 1e-16 apart: rounding to 16 decimals moves the value by
    // less than half that gap, so it reads back as the same double.
    if !value.is_finite() || value.abs() >= 0.5 {
        return value;
    }
    format!("{value:.16}").parse().unwrap_or(value)
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
}
