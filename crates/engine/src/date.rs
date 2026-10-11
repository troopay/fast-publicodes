//! `dd/mm/yyyy` dates, validated and converted with JS `Date` normalization.

use std::rc::Rc;

use crate::error::{Error, Result};
use crate::js::string_to_number;

/// Today as `dd/mm/yyyy`, asked each time a `durée` is parsed.
pub type Today = Rc<dyn Fn() -> Result<String>>;

/// Days since 1970-01-01 of a proleptic Gregorian date; `month` is 1-based.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The year and day of the month of a number of days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let year = year_of_era + era * 400 + i64::from(month_index >= 10);
    (year, day_of_year - (153 * month_index + 2) / 5 + 1)
}

fn civil_day_of(days: i64) -> i64 {
    civil_from_days(days).1
}

/// `new Date(year, month, day)` as days since epoch, `month` counted from 0, or `None` for an
/// invalid date.
fn js_local_date(year: f64, month: f64, day: f64) -> Option<i64> {
    if !(year.is_finite() && month.is_finite() && day.is_finite()) {
        return None;
    }
    let (year, month, day) = (
        year.trunc() as i64,
        month.trunc() as i64,
        day.trunc() as i64,
    );
    let year = if (0..=99).contains(&year) {
        1900 + year
    } else {
        year
    };
    let months = year * 12 + month;
    let first = days_from_civil(months.div_euclid(12), months.rem_euclid(12) + 1, 1);
    Some(first + day - 1)
}

fn pad(n: f64) -> String {
    let text = crate::js::number_to_string(n);
    if n < 10. { format!("0{text}") } else { text }
}

/// The year, month and day `normalizeDateString` reads in `text`, once checked valid.
fn valid_date(text: &str) -> Result<(f64, f64, f64)> {
    let mut parts = text.split('/');
    let mut part = || parts.next().map_or(f64::NAN, string_to_number);
    let (first, second) = (part(), part());
    let (year, month, day) = match text.split('/').nth(2).filter(|year| !year.is_empty()) {
        Some(year) => (string_to_number(year), second, first),
        None => (second, first, 1.),
    };
    match js_local_date(year, month - 1., day) {
        Some(days) if civil_day_of(days) as f64 == day => Ok((year, month, day)),
        _ => Err(Error::syntax(format!(
            "La date {}/{}/{} n'est pas valide",
            crate::js::number_to_string(day),
            crate::js::number_to_string(month),
            crate::js::number_to_string(year)
        ))),
    }
}

/// `normalizeDateString`: the date as `dd/mm/yyyy`, each part padded to two digits.
pub fn normalize_date_string(text: &str) -> Result<String> {
    let (year, month, day) = valid_date(text)?;
    Ok(format!("{}/{}/{}", pad(day), pad(month), pad(year)))
}

/// `convertToDate(value).getTime()`.
pub fn timestamp(text: &str) -> Result<f64> {
    let (year, month, day) = valid_date(text)?;
    // Each part as read back from its padding, which writes a negative number `0-…`.
    let padded = |n: f64| if n < 0. { f64::NAN } else { n };
    let days = js_local_date(padded(year), padded(month) - 1., padded(day));
    Ok(days.map_or(f64::NAN, |days| days as f64 * 86_400_000.))
}

fn parts(text: &str) -> Vec<f64> {
    text.split('/').map(string_to_number).collect()
}

fn part(parts: &[f64], index: usize) -> f64 {
    parts.get(index).copied().unwrap_or(f64::NAN)
}

/// `new Date(year, month, 0).getDate()`: the number of days of a month, 1-based.
fn days_in_month(year: f64, month: f64) -> f64 {
    js_local_date(year, month, 0.).map_or(f64::NAN, |last| civil_day_of(last) as f64)
}

pub fn difference_in_days(from: &str, to: &str) -> Result<f64> {
    Ok((timestamp(to)? - timestamp(from)?) / 86_400_000.)
}

pub fn difference_in_months(from: &str, to: &str) -> f64 {
    let (from, to) = (parts(from), parts(to));
    let (day_from, month_from, year_from) = (part(&from, 0), part(&from, 1), part(&from, 2));
    let (day_to, month_to, year_to) = (part(&to, 0), part(&to, 1), part(&to, 2));
    let full_months = month_to - month_from + 12. * (year_to - year_from);
    let prorata_from = (day_from - 1.) / days_in_month(year_from, month_from);
    let prorata_to = day_to / days_in_month(year_to, month_to);
    full_months - prorata_from + prorata_to
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Year of a date and whether it is on or after the 1st of March.
fn year_and_after_march(text: &str) -> Result<(i64, bool)> {
    let days = (timestamp(text)? / 86_400_000.) as i64;
    let (year, _) = civil_from_days(days);
    Ok((year, days >= days_from_civil(year, 3, 1)))
}

pub fn difference_in_years(from: &str, to: &str) -> Result<f64> {
    let days = difference_in_days(from, to)?;
    let (from_year, from_after_march) = year_and_after_march(from)?;
    let (to_year, _) = year_and_after_march(to)?;
    let first = from_year + i64::from(from_after_march);
    let last = to_year - i64::from(!from_after_march);
    let leap_years = (first..=last).filter(|&year| is_leap_year(year)).count() as f64;
    Ok((days - leap_years) / 365.)
}

fn trimestre_civil(text: &str) -> String {
    let segments: Vec<&str> = text.split('/').collect();
    let month: f64 = segments
        .get(1)
        .and_then(|m| {
            let digits: String = m
                .trim_start()
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse().ok()
        })
        .unwrap_or(f64::NAN);
    let start = 3. * ((month - 1.) / 3.).floor() + 1.;
    let start = if start.is_nan() {
        "NaN".to_owned()
    } else {
        format!("{:02}", start as i64)
    };
    format!(
        "01/{start}/{}",
        segments.get(2).copied().unwrap_or("undefined")
    )
}

pub fn difference_in_trimestres_civils(from: &str, to: &str) -> f64 {
    (difference_in_months(&trimestre_civil(from), &trimestre_civil(to)) / 3.).floor() + 1.
}

pub fn difference_in_civil_years(from: &str, to: &str) -> Result<f64> {
    let year = |text: &str| {
        let tail: String = text
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!(
            "01/{}",
            crate::js::number_to_string(string_to_number(&tail))
        )
    };
    Ok(difference_in_years(&year(from), &year(to))?.floor() + 1.)
}

/// Whether `text` matches `/^[\d]{2}\/[\d]{2}\/[\d]{4}$/`.
pub fn is_date_string(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes[2] == b'/'
        && bytes[5] == b'/'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 2 || i == 5 || b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_like_js_dates() {
        assert_eq!(normalize_date_string("1/3/2024").unwrap(), "01/03/2024");
        assert_eq!(normalize_date_string("03/2024").unwrap(), "01/03/2024");
        assert!(normalize_date_string("31/02/2024").is_err());
        assert_eq!(normalize_date_string("29/02/2024").unwrap(), "29/02/2024");
    }

    #[test]
    fn timestamps_count_days_from_epoch() {
        assert_eq!(timestamp("02/01/1970").unwrap(), 86_400_000.);
    }
}
