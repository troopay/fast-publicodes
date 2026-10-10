//! JavaScript number semantics the engine reproduces.

/// `String(value)` for a JS number.
pub fn number_to_string(value: f64) -> String {
    ryu_js::Buffer::new().format(value).to_owned()
}

/// `+value.toFixed(16)`.
pub fn round_to_16_decimals(value: f64) -> f64 {
    if !value.is_finite() || value.abs() >= 1e21 {
        return value;
    }
    format!("{value:.16}").parse().unwrap_or(value)
}
