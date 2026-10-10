//! `formatValue` with the default `fr` language, as `Intl.NumberFormat('fr')` writes numbers.

use crate::ast::Value;
use crate::error::Result;
use crate::js::number_to_string;
use crate::units::{Unit, convert_unit, serialize_unit, simplify_unit};

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Decimal,
    Currency,
    Percent,
}

/// The digits of a finite number, from its shortest representation.
fn decimal_digits(value: f64) -> (Vec<u8>, i32) {
    let text = number_to_string(value.abs());
    let (mantissa, exponent) = match text.split_once('e') {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().unwrap_or(0)),
        None => (text.as_str(), 0),
    };
    let (integer, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits: Vec<u8> = integer
        .bytes()
        .chain(fraction.bytes())
        .map(|b| b - b'0')
        .collect();
    (digits, integer.len() as i32 + exponent)
}

/// `Intl.NumberFormat('fr', { style, maximumFractionDigits, minimumFractionDigits })`.
fn format_number(value: f64, style: Style, max_fraction: usize, min_fraction: usize) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let suffix = match style {
        Style::Decimal => "",
        Style::Currency => "\u{a0}€",
        Style::Percent => "\u{a0}%",
    };
    if value.is_infinite() {
        return format!("{sign}∞{suffix}");
    }
    let (digits, mut point) = decimal_digits(value);
    if style == Style::Percent {
        point += 2;
    }
    let digit = |i: i32| {
        if i >= 0 {
            digits.get(i as usize).copied().unwrap_or(0)
        } else {
            0
        }
    };
    let integer_len = point.max(1);
    let mut kept: Vec<u8> = (point - integer_len..point + max_fraction as i32)
        .map(digit)
        .collect();
    if digit(point + max_fraction as i32) >= 5 {
        let mut i = kept.len();
        loop {
            if i == 0 {
                kept.insert(0, 1);
                break;
            }
            i -= 1;
            if kept[i] == 9 {
                kept[i] = 0;
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let fraction_start = kept.len() - max_fraction;
    let (integer, fraction) = kept.split_at(fraction_start);
    let mut fraction = fraction.to_vec();
    while fraction.len() > min_fraction && fraction.last() == Some(&0) {
        fraction.pop();
    }
    let integer: String = integer.iter().map(|d| char::from(b'0' + d)).collect();
    let integer = integer.trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };
    let mut grouped = String::new();
    for (i, c) in integer.chars().enumerate() {
        if i > 0 && (integer.len() - i) % 3 == 0 {
            grouped.push('\u{202f}');
        }
        grouped.push(c);
    }
    if !fraction.is_empty() {
        grouped.push(',');
        grouped.extend(fraction.iter().map(|d| char::from(b'0' + d)));
    }
    format!("{sign}{grouped}{suffix}")
}

/// `formatValue(evaluation)`, the text a `texte` interpolation shows.
pub fn format_value(value: &Value, unit: Option<&Unit>) -> Result<String> {
    Ok(match value {
        Value::Number(n) if n.is_nan() => "Erreur dans le calcul du nombre".to_owned(),
        Value::Undefined => "Pas encore défini".to_owned(),
        Value::Null => "Non applicable".to_owned(),
        Value::String(s) => s.replacen("\\n", "\n", 1),
        Value::Bool(true) => "oui".to_owned(),
        Value::Bool(false) => "non".to_owned(),
        Value::Number(n) => {
            let (n, unit) = match unit {
                Some(unit) => {
                    let simplified = simplify_unit(unit);
                    (
                        convert_unit(Some(unit), Some(&simplified), *n)?,
                        Some(simplified),
                    )
                }
                None => (*n, None),
            };
            let text = match serialize_unit(unit.as_ref()).as_deref() {
                Some("€") => {
                    let min = if n.fract() != 0. && n.is_finite() {
                        2
                    } else {
                        0
                    };
                    format_number(n, Style::Currency, 2, min)
                }
                Some("%") => format_number(n / 100., Style::Percent, 2, 0),
                Some(unit) => format!("{}\u{a0}{unit}", format_number(n, Style::Decimal, 2, 0)),
                None => format_number(n, Style::Decimal, 2, 0),
            };
            text.trim().to_owned()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_numbers_like_intl_fr() {
        assert_eq!(
            format_number(1234567.891, Style::Decimal, 2, 0),
            "1\u{202f}234\u{202f}567,89"
        );
        assert_eq!(format_number(1.005, Style::Decimal, 2, 0), "1,01");
        assert_eq!(
            format_number(-1234.5, Style::Currency, 2, 2),
            "-1\u{202f}234,50\u{a0}€"
        );
        assert_eq!(
            format_number(1.005 / 100., Style::Percent, 2, 0),
            "1\u{a0}%"
        );
        assert_eq!(format_number(-0.004, Style::Decimal, 2, 0), "-0");
    }
}
