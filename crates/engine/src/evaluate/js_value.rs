//! JavaScript operators on evaluated values.

use std::cmp::Ordering;

use crate::ast::Value;
use crate::js::number_to_string;

pub fn to_number(value: &Value) -> f64 {
    match value {
        Value::Undefined => f64::NAN,
        Value::Null => 0.,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Number(n) => *n,
        Value::String(s) => string_to_number(s),
    }
}

fn is_js_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// `StringToNumber`.
fn string_to_number(text: &str) -> f64 {
    let text = text.trim_matches(is_js_space);
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

pub fn to_js_string(value: &Value) -> String {
    match value {
        Value::Undefined => "undefined".to_owned(),
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => number_to_string(*n),
        Value::String(s) => s.to_string(),
    }
}

pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Undefined | Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => *n != 0. && !n.is_nan(),
        Value::String(s) => !s.is_empty(),
    }
}

/// `===`; `Value` equality already treats numbers as IEEE floats do.
pub fn strict_equals(a: &Value, b: &Value) -> bool {
    a == b
}

/// The abstract relational comparison; `None` when either side is `NaN`.
pub fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    if let (Value::String(a), Value::String(b)) = (a, b) {
        return Some(a.encode_utf16().cmp(b.encode_utf16()));
    }
    to_number(a).partial_cmp(&to_number(b))
}

/// `a ** b`.
pub fn power(a: f64, b: f64) -> f64 {
    if b.is_nan() || (a.abs() == 1. && b.is_infinite()) {
        return f64::NAN;
    }
    a.powf(b)
}
