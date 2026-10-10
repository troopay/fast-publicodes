//! Units: parsing, serialization, inference and conversion.

use std::sync::LazyLock;

use crate::error::{Error, ErrorKind, Result};
use crate::js::round_to_16_decimals;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Unit {
    pub numerators: Vec<String>,
    pub denominators: Vec<String>,
}

pub fn parse_unit(text: &str) -> Result<Unit> {
    if text.contains("/ ") {
        return Err(Error::syntax(format!(
            "L'unité \"{text}\" ne doit pas contenir d'espace après \"/\""
        )));
    }
    let mut parts = text.split('/');
    let split =
        |part: &str| decompose_power(part.trim().split('.').filter(|unit| !unit.is_empty()));
    let numerators = split(parts.next().unwrap_or(""));
    let denominators = parts.flat_map(split).collect();
    Ok(Unit {
        numerators,
        denominators,
    })
}

/// Power suffix of a base unit: the first digit run not followed by any ASCII letter.
fn power_of(unit: &str) -> Option<(&str, &str)> {
    let bytes = unit.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() {
            start += 1;
            continue;
        }
        let end = start
            + bytes[start..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
        if !bytes[end..].iter().any(u8::is_ascii_alphabetic) {
            let power = &unit[start..end];
            let primary = unit.split(power).next().unwrap_or("");
            return Some((primary, power));
        }
        start = end;
    }
    None
}

fn unit_counts<'a>(units: impl IntoIterator<Item = &'a str>) -> Vec<(String, u32)> {
    let mut counts: Vec<(String, u32)> = Vec::new();
    for unit in units {
        let (primary, power) = match power_of(unit) {
            Some((primary, power)) => (primary, power.parse().unwrap_or(u32::MAX)),
            None => (unit, 1),
        };
        match counts.iter_mut().find(|(name, _)| name == primary) {
            Some((_, count)) => *count = count.saturating_add(power),
            None => counts.push((primary.to_owned(), power)),
        }
    }
    counts
}

fn decompose_power<'a>(units: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    unit_counts(units)
        .into_iter()
        .flat_map(|(unit, power)| std::iter::repeat_n(unit, power as usize))
        .collect()
}

fn print_units(units: &[String]) -> String {
    unit_counts(units.iter().map(String::as_str))
        .into_iter()
        .map(|(unit, power)| {
            if power > 1 {
                format!("{unit}{power}")
            } else {
                unit
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

pub fn serialize_unit(unit: Option<&Unit>) -> Option<String> {
    let unit = simplify(unit?, |a, b| a == b);
    let (n, d) = (&unit.numerators, &unit.denominators);
    Some(match (n.is_empty(), d.is_empty()) {
        (true, true) => String::new(),
        (false, true) => print_units(n),
        (true, false) => format!("/{}", print_units(d)),
        (false, false) => format!("{}/{}", print_units(n), print_units(d)),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitOperator {
    Times,
    Divide,
    IntegerDivide,
    Plus,
    Minus,
}

pub fn infer_unit(operator: UnitOperator, units: [Option<&Unit>; 2]) -> Option<Unit> {
    if operator == UnitOperator::Divide {
        let no_unit = Unit::default();
        let left = units[0].unwrap_or(&no_unit);
        let right = units[1].unwrap_or(&no_unit);
        let inverted = Unit {
            numerators: right.denominators.clone(),
            denominators: right.numerators.clone(),
        };
        return infer_unit(UnitOperator::Times, [Some(left), Some(&inverted)]);
    }
    let defined: Vec<&Unit> = units.into_iter().flatten().collect();
    if defined.len() <= 1 {
        return defined.first().map(|unit| (*unit).clone());
    }
    match operator {
        UnitOperator::Times => Some(simplify(
            &Unit {
                numerators: defined.iter().flat_map(|u| u.numerators.clone()).collect(),
                denominators: defined
                    .iter()
                    .flat_map(|u| u.denominators.clone())
                    .collect(),
            },
            |a, b| a == b,
        )),
        UnitOperator::Plus | UnitOperator::Minus => Some(defined[0].clone()),
        UnitOperator::Divide | UnitOperator::IntegerDivide => None,
    }
}

fn remove_once(list: &mut Vec<String>, element: &str, eq: impl Fn(&str, &str) -> bool) {
    if let Some(index) = list.iter().position(|e| eq(e, element)) {
        list.remove(index);
    }
}

fn simplify(unit: &Unit, eq: impl Fn(&str, &str) -> bool + Copy) -> Unit {
    let mut result = unit.clone();
    for next in unit.numerators.iter().chain(&unit.denominators) {
        if result.numerators.iter().any(|u| eq(next, u))
            && result.denominators.iter().any(|u| eq(next, u))
        {
            remove_once(&mut result.numerators, next, eq);
            remove_once(&mut result.denominators, next, eq);
        }
    }
    result
}

const CONVERT_TABLE: &[(&str, &str, f64)] = &[
    ("mois", "an", 12.),
    ("jour", "an", 365.),
    ("jour", "mois", 365. / 12.),
    ("trimestre", "an", 4.),
    ("mois", "trimestre", 3.),
    ("jour", "trimestre", (365. / 12.) * 3.),
    ("€", "k€", 1e3),
    ("g", "kg", 1e3),
    ("mg", "g", 1e3),
    ("mg", "kg", 1e6),
    ("m", "km", 1e3),
    ("cm", "m", 1e2),
    ("mm", "cm", 1e1),
    ("mm", "m", 1e3),
    ("cm", "km", 1e5),
    ("mm", "km", 1e6),
];

static UNIT_CLASSES: LazyLock<Vec<Vec<&'static str>>> = LazyLock::new(|| {
    let mut classes: Vec<Vec<&str>> = Vec::new();
    for &(a, b, _) in CONVERT_TABLE {
        let ia = classes.iter().position(|c| c.contains(&a));
        let ib = classes.iter().position(|c| c.contains(&b));
        match (ia, ib) {
            (None, None) => classes.push(vec![a, b]),
            (Some(i), None) => classes[i].push(b),
            (None, Some(i)) => classes[i].push(a),
            (Some(_), Some(_)) => {}
        }
    }
    classes
});

fn table_ratio(numerator: &str, denominator: &str) -> Option<f64> {
    CONVERT_TABLE
        .iter()
        .find(|(a, b, _)| *a == numerator && *b == denominator)
        .map(|(_, _, ratio)| *ratio)
}

fn single_unit_conversion_factor(from: &str, to: &str) -> Option<f64> {
    table_ratio(to, from).or_else(|| table_ratio(from, to).map(|ratio| 1. / ratio))
}

fn units_conversion_factor(from: &[String], to: &[String]) -> f64 {
    let percents = |units: &[String]| units.iter().filter(|u| *u == "%").count() as i32;
    let mut factor = 100f64.powi(percents(to) - percents(from));
    for from_unit in from {
        factor *= to
            .iter()
            .find_map(|to_unit| single_unit_conversion_factor(from_unit, to_unit))
            .unwrap_or(1.);
    }
    factor
}

fn are_equivalent_serialized(from: Option<&str>, to: Option<&str>) -> bool {
    const EQUIVALENTS: [(&str, &str); 2] = [("kW.h", "kWh"), ("mn/h", "noeud")];
    let (Some(from), Some(to)) = (from, to) else {
        return false;
    };
    if from.is_empty() || to.is_empty() {
        return false;
    }
    from == to
        || EQUIVALENTS
            .iter()
            .any(|&(a, b)| (to == a && from == b) || (from == a && to == b))
}

fn are_same_class(a: &str, b: &str) -> bool {
    a == b
        || UNIT_CLASSES
            .iter()
            .any(|c| c.contains(&a) && c.contains(&b))
}

fn remove_percentages(unit: &Unit) -> Unit {
    Unit {
        numerators: unit
            .numerators
            .iter()
            .filter(|u| *u != "%")
            .cloned()
            .collect(),
        denominators: unit
            .denominators
            .iter()
            .filter(|u| *u != "%")
            .cloned()
            .collect(),
    }
}

pub fn simplify_unit(unit: &Unit) -> Unit {
    let simplified = simplify(unit, are_same_class);
    if !simplified.numerators.is_empty() && simplified.numerators.iter().all(|u| u == "%") {
        return Unit {
            numerators: vec!["%".to_owned()],
            denominators: simplified.denominators,
        };
    }
    remove_percentages(&simplified)
}

fn simplify_unit_with_factor(unit: &Unit) -> (Unit, f64) {
    let factor = units_conversion_factor(&unit.numerators, &unit.denominators);
    (
        simplify(&remove_percentages(unit), are_same_class),
        round_to_16_decimals(factor),
    )
}

fn are_unit_convertible(a: Option<&Unit>, b: Option<&Unit>) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return true;
    };
    let class_of = |unit: &str| {
        UNIT_CLASSES
            .iter()
            .position(|c| c.contains(&unit))
            .map_or_else(|| unit.to_owned(), |index| index.to_string())
    };
    let count = |units: &[String]| {
        let mut counts: Vec<(String, i64)> = Vec::new();
        for unit in units {
            let class = class_of(unit);
            match counts.iter_mut().find(|(c, _)| *c == class) {
                Some((_, n)) => *n += 1,
                None => counts.push((class, 1)),
            }
        }
        counts
    };
    let lists = [
        count(&a.numerators),
        count(&a.denominators),
        count(&b.numerators),
        count(&b.denominators),
    ];
    let get = |list: &[(String, i64)], class: &str| {
        list.iter().find(|(c, _)| c == class).map_or(0, |(_, n)| *n)
    };
    lists.iter().flatten().all(|(class, _)| {
        class == "%"
            || get(&lists[0], class) - get(&lists[1], class)
                == get(&lists[2], class) - get(&lists[3], class)
    })
}

/// Fails when `from` cannot be converted to `to`.
pub fn check_convertible(from: Option<&Unit>, to: Option<&Unit>) -> Result<()> {
    let serialized_from = serialize_unit(from);
    let serialized_to = serialize_unit(to);
    if are_equivalent_serialized(serialized_from.as_deref(), serialized_to.as_deref())
        || are_unit_convertible(from, to)
    {
        return Ok(());
    }
    Err(Error::new(
        ErrorKind::Engine,
        format!(
            "Impossible de convertir l'unité '{}' en '{}'",
            serialized_from.unwrap_or_default(),
            serialized_to.unwrap_or_default()
        ),
    ))
}

/// `convertUnit` for a number.
pub fn convert_unit(from: Option<&Unit>, to: Option<&Unit>, value: f64) -> Result<f64> {
    check_convertible(from, to)?;
    let Some(from) = from else {
        return Ok(value);
    };
    if value == 0. || value.is_nan() {
        return Ok(value);
    }
    let no_unit = Unit::default();
    let (from_simplified, factor_to) = simplify_unit_with_factor(from);
    let (to_simplified, factor_from) = simplify_unit_with_factor(to.unwrap_or(&no_unit));
    let numerators =
        units_conversion_factor(&from_simplified.numerators, &to_simplified.numerators);
    let denominators =
        units_conversion_factor(&to_simplified.denominators, &from_simplified.denominators);
    Ok(round_to_16_decimals(
        value * factor_to / factor_from * numerators * denominators,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(text: &str) -> Unit {
        parse_unit(text).unwrap()
    }

    #[test]
    fn parses_powers_and_serializes_them_back() {
        assert_eq!(unit("m2/s").numerators, ["m", "m"]);
        assert_eq!(serialize_unit(Some(&unit("m.m/s"))).unwrap(), "m2/s");
        assert_eq!(serialize_unit(Some(&unit("€/mois.€"))).unwrap(), "/mois");
    }

    #[test]
    fn converts_through_the_table() {
        let monthly = unit("€/mois");
        let yearly = unit("€/an");
        assert_eq!(
            convert_unit(Some(&monthly), Some(&yearly), 100.).unwrap(),
            1200.
        );
        assert!(convert_unit(Some(&unit("minute")), Some(&unit("heure")), 90.).is_err());
        assert_eq!(convert_unit(None, Some(&yearly), 7.).unwrap(), 7.);
    }
}
