//! The publicodes expression grammar.
//!
//! Each token reproduces a JavaScript regular expression of `publicodes`, matched against the
//! rest of the input; the cursor advances by the length of the match even when the match
//! starts after the cursor.

use crate::date::normalize_date_string;
use crate::error::{Error, ErrorKind, Result};
use crate::js::{is_js_whitespace, lone_high_surrogate};
use crate::raw::Raw;

fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// Characters JavaScript `.` does not match.
fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn is_letter(c: char) -> bool {
    c.is_ascii_alphabetic() || ('\u{C0}'..='\u{17F}').contains(&c) || c == '$'
}

fn is_any(c: char) -> bool {
    is_letter(c)
        || c.is_ascii_digit()
        || matches!(
            c,
            '\'' | ',' | '°' | '€' | '%' | '²' | '_' | '"' | '«' | '»'
        )
}

fn is_any_or_special(c: char) -> bool {
    is_any(c) || matches!(c, '-' | '+')
}

/// `publicodes` writes `\p{Sc}` without the `u` flag, so `p`, `{`, `S`, `c` and `}` are literal.
fn starts_unit(c: char) -> bool {
    is_letter(c) || matches!(c, '°' | '%' | 'p' | '{' | 'S' | 'c' | '}' | '€')
}

/// Length of the leading spaces.
fn spaces(text: &str) -> usize {
    text.len() - text.trim_start_matches(is_space).len()
}

fn first(text: &str) -> Option<char> {
    text.chars().next()
}

/// `phrase_starting_with`: a word starting with `starts`, then words after single spaces.
fn phrase(text: &str, starts: fn(char) -> bool) -> Option<usize> {
    let head = first(text).filter(|&c| starts(c))?;
    let word =
        |from: usize| text[from..].len() - text[from..].trim_start_matches(is_any_or_special).len();
    let mut length = head.len_utf8();
    length += word(length);
    loop {
        let mut chars = text[length..].chars();
        match (chars.next(), chars.next()) {
            (Some(space), Some(next)) if is_space(space) && is_any(next) => {
                length += space.len_utf8() + next.len_utf8();
                length += word(length);
            }
            _ => return Some(length),
        }
    }
}

fn rule_name(text: &str) -> Option<usize> {
    phrase(text, is_letter)
}

fn dotted_name(text: &str) -> Option<usize> {
    let mut length = rule_name(text).or_else(|| text.starts_with('^').then_some(1))?;
    while let Some(rest) = text[length..].strip_prefix(" . ")
        && let Some(name) = rule_name(rest)
    {
        length += 3 + name;
    }
    Some(length)
}

fn digits(text: &str) -> usize {
    text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len()
}

fn decimal(text: &str) -> Option<usize> {
    let integer = digits(text);
    if integer == 0 {
        return None;
    }
    match text[integer..].strip_prefix('.').map(digits) {
        Some(decimals) if decimals > 0 => Some(integer + 1 + decimals),
        _ => Some(integer),
    }
}

/// `(?:(?:0?[1-9]|[12][0-9]|3[01])\/)?(?:0?[1-9]|1[012])\/\d{4}`, trying its alternatives in
/// the order a backtracking engine does.
fn date(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let at = |i: usize| bytes.get(i).copied().unwrap_or(0);
    let nonzero = |i: usize| (b'1'..=b'9').contains(&at(i));
    let digit = |i: usize| at(i).is_ascii_digit();
    let day = |i: usize| {
        [
            (at(i) == b'0' && nonzero(i + 1)).then_some(2),
            nonzero(i).then_some(1),
            (matches!(at(i), b'1' | b'2') && digit(i + 1)).then_some(2),
            (at(i) == b'3' && matches!(at(i + 1), b'0' | b'1')).then_some(2),
        ]
    };
    let month = |i: usize| {
        [
            (at(i) == b'0' && nonzero(i + 1)).then_some(2),
            nonzero(i).then_some(1),
            (at(i) == b'1' && matches!(at(i + 1), b'0'..=b'2')).then_some(2),
        ]
    };
    let year = |i: usize| (at(i) == b'/' && (1..=4).all(|k| digit(i + k))).then_some(i + 5);
    let with_month = |i: usize| month(i).into_iter().flatten().find_map(|m| year(i + m));
    day(0)
        .into_iter()
        .flatten()
        .filter(|&d| at(d) == b'/')
        .find_map(|d| with_month(d + 1))
        .or_else(|| with_month(0))
}

/// `^SPACE*(\/)?unit(SPACE*(\.|\/)unit)*`.
fn unit(text: &str) -> Option<usize> {
    let mut length = spaces(text);
    if text[length..].starts_with('/') {
        length += 1;
    }
    length += phrase(&text[length..], starts_unit)?;
    loop {
        let separator = length + spaces(&text[length..]);
        if !matches!(first(&text[separator..]), Some('.' | '/')) {
            return Some(length);
        }
        match phrase(&text[separator + 1..], starts_unit) {
            Some(identifier) => length = separator + 1 + identifier,
            None => return Some(length),
        }
    }
}

/// `'.*'|".*"`, anywhere in `text`: the start and end of the match.
fn string(text: &str) -> Option<(usize, usize)> {
    text.char_indices().find_map(|(start, quote)| {
        if !matches!(quote, '\'' | '"') {
            return None;
        }
        let rest = &text[start + 1..];
        let line = &rest[..rest.find(is_line_terminator).unwrap_or(rest.len())];
        line.rfind(quote).map(|end| (start, start + 1 + end + 1))
    })
}

/// `^SPACE+(operator|…)SPACE+`: the operator and the length of the match.
fn between_spaces(text: &str, operators: &[&'static str]) -> Option<(&'static str, usize)> {
    let before = spaces(text);
    if before == 0 {
        return None;
    }
    operators.iter().find_map(|&operator| {
        let after = text[before..].strip_prefix(operator).map(spaces)?;
        (after > 0).then_some((operator, before + operator.len() + after))
    })
}

const EXPONENTIATION: &[&str] = &["**"];
const MULTIPLICATION: &[&str] = &["*", "/", "//"];
const ADDITION: &[&str] = &["-", "+"];
const COMPARISON: &[&str] = &[">", "<", ">=", "<=", "=", "!="];

struct Parser<'a> {
    text: &'a str,
    current: usize,
}

type Parse<T> = std::result::Result<T, Failure>;

enum Failure {
    Grammar {
        position: usize,
        expected: String,
        found: String,
        custom: Option<&'static str>,
    },
    /// A date literal that matches the grammar but is not a valid date.
    Date(Error),
    /// The `TypeError` `publicodes` raises when a term is expected at the end.
    Missing,
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        self.text.get(self.current..).unwrap_or("")
    }

    fn peek(&self) -> Option<char> {
        first(self.rest())
    }

    fn is_eol(&self) -> bool {
        self.current >= self.text.len()
    }

    fn error(&self, expected: &str, found: String, custom: Option<&'static str>) -> Failure {
        Failure::Grammar {
            position: self.current,
            expected: expected.to_owned(),
            found,
            custom,
        }
    }

    /// The character JavaScript reads at the cursor: its first UTF-16 unit.
    fn found(&self) -> String {
        utf16_prefix(self.rest(), 1)
    }

    /// Advances past a match of `length` and returns its text.
    fn advance(&mut self, length: usize) -> &'a str {
        let text = &self.rest()[..length];
        self.current += length;
        text
    }

    /// Advances past a match, or fails.
    fn expect(&mut self, length: Option<usize>, description: &str) -> Parse<&'a str> {
        match length {
            Some(length) => Ok(self.advance(length)),
            None => Err(self.unmatched(description)),
        }
    }

    /// A failed match, showing the next five UTF-16 units.
    fn unmatched(&self, description: &str) -> Failure {
        self.error(description, utf16_prefix(self.rest(), 5), None)
    }

    /// Advances by `units` UTF-16 units, as JavaScript moves its cursor.
    fn advance_units(&mut self, units: usize) {
        let mut count = 0;
        for c in self.rest().chars() {
            if count >= units {
                break;
            }
            count += c.len_utf16();
            self.current += c.len_utf8();
        }
    }

    fn skip_spaces(&mut self) {
        self.current += spaces(self.rest());
    }

    fn expect_char(&mut self, expected: char) -> Parse<()> {
        match self.peek() {
            Some(c) if c == expected => {
                self.current += c.len_utf8();
                Ok(())
            }
            _ => Err(self.error(&format!("'{expected}'"), self.found(), None)),
        }
    }

    fn binary_level(
        &mut self,
        operators: &[&'static str],
        next: fn(&mut Self) -> Parse<Raw>,
        right_recursive: bool,
    ) -> Parse<Raw> {
        let mut left = next(self)?;
        while !self.is_eol()
            && let Some((operator, length)) = between_spaces(self.rest(), operators)
        {
            self.current += length;
            let right = if right_recursive {
                self.exponentiation()?
            } else {
                next(self)?
            };
            left = binary(operator, left, right);
        }
        Ok(left)
    }

    fn comparison(&mut self) -> Parse<Raw> {
        self.binary_level(COMPARISON, Self::addition, false)
    }

    fn addition(&mut self) -> Parse<Raw> {
        self.binary_level(ADDITION, Self::multiplication, false)
    }

    fn multiplication(&mut self) -> Parse<Raw> {
        self.binary_level(MULTIPLICATION, Self::exponentiation, false)
    }

    fn exponentiation(&mut self) -> Parse<Raw> {
        self.binary_level(EXPONENTIATION, Self::primary, true)
    }

    fn primary(&mut self) -> Parse<Raw> {
        match self.peek() {
            Some('(') => {
                self.current += 1;
                self.skip_spaces();
                let expression = self.comparison()?;
                self.skip_spaces();
                self.expect_char(')')?;
                Ok(expression)
            }
            Some('-') => {
                self.current += 1;
                self.skip_spaces();
                let expression = self.primary()?;
                Ok(binary("-", number(0.), expression))
            }
            _ if date(self.rest()).is_some() => {
                let text = self.expect(date(self.rest()), "Une date")?;
                Ok(constant("date", Raw::String(date_value(text)?.into())))
            }
            None => Err(Failure::Missing),
            Some(c) if c.is_ascii_digit() => {
                let text = self.expect(decimal(self.rest()), "Un nombre")?;
                let value = Raw::Number(text.parse().unwrap_or(f64::NAN));
                let Some(length) = unit(self.rest()) else {
                    return Ok(constant("number", value));
                };
                let unit = self.advance(length);
                Ok(Raw::map([(
                    "constant",
                    Raw::map([
                        ("type", Raw::str("number")),
                        ("nodeValue", value),
                        ("rawUnit", Raw::str(unit.trim_matches(is_js_whitespace))),
                    ]),
                )]))
            }
            Some('\'' | '"') => {
                let Some((start, end)) = string(self.rest()) else {
                    return Err(self.unmatched("Une chaîne de caractères"));
                };
                let text = &self.rest()[start..end];
                self.advance_units(text.encode_utf16().count());
                Ok(constant("string", Raw::str(&text[1..text.len() - 1])))
            }
            Some(c) if c == '^' || is_letter(c) => {
                let name = self.expect(dotted_name(self.rest()), "Un nom de règle")?;
                Ok(match name {
                    "oui" => constant("boolean", Raw::Bool(true)),
                    "non" => constant("boolean", Raw::Bool(false)),
                    _ => Raw::map([("variable", Raw::str(name))]),
                })
            }
            _ => Err(self.error(
                "Une constante, une référence ou une expression entre parenthèses",
                self.found(),
                None,
            )),
        }
    }
}

/// The first `units` UTF-16 units of `text`, as JavaScript slices it.
fn utf16_prefix(text: &str, units: usize) -> String {
    let mut prefix = String::new();
    let mut count = 0;
    for c in text.chars() {
        if count + c.len_utf16() > units {
            if count < units {
                prefix.push(lone_high_surrogate(c));
            }
            break;
        }
        prefix.push(c);
        count += c.len_utf16();
    }
    prefix
}

fn date_value(text: &str) -> Parse<String> {
    normalize_date_string(text).map_err(Failure::Date)
}

fn constant(kind: &str, value: Raw) -> Raw {
    Raw::map([(
        "constant",
        Raw::map([("type", Raw::str(kind)), ("nodeValue", value)]),
    )])
}

pub fn number(value: f64) -> Raw {
    constant("number", Raw::Number(value))
}

fn binary(operator: &str, left: Raw, right: Raw) -> Raw {
    Raw::map([(operator, Raw::list(vec![left, right]))])
}

/// Whether `expression` parses to a single reference, written as `name`.
pub fn is_reference(expression: &str) -> bool {
    expression.starts_with(is_letter)
        && !matches!(expression, "oui" | "non")
        && dotted_name(expression) == Some(expression.len())
        && collapse_lines(expression) == expression
}

/// Parses an expression into the raw object `parse` turns into nodes.
pub fn parse_expression(expression: &str, rule: &str) -> Result<Raw> {
    let single_line = collapse_lines(expression);
    let mut parser = Parser {
        text: &single_line,
        current: 0,
    };
    let result = parser.comparison().and_then(|tree| {
        if parser.is_eol() {
            return Ok(tree);
        }
        if parser.rest().contains(['+', '-', '*', '/', '>', '<', '=']) {
            return Err(parser.error(
                "un opérateur entouré d’espaces",
                parser.found(),
                Some("Les opérateurs doivent être entourés d’espaces (\"2 + 2\" et non \"2+2\")"),
            ));
        }
        Err(parser.error("La fin d’expression", parser.found(), None))
    });
    result.map_err(|failure| {
        let (position, expected, found, custom) = match failure {
            Failure::Date(error) => return error,
            Failure::Missing => {
                return Error::new(
                    ErrorKind::Type,
                    "Cannot read properties of undefined (reading 'match')",
                );
            }
            Failure::Grammar {
                position,
                expected,
                found,
                custom,
            } => (position, expected, found, custom),
        };
        let pointer = " ".repeat(single_line[..position].encode_utf16().count());
        let message = custom.map(str::to_owned).unwrap_or_else(|| {
            if found.is_empty() {
                format!("{expected} est attendu, hors l’expression se termine prématurément")
            } else {
                format!("{expected} est attendu, mais \"{found}\" a été trouvé")
            }
        });
        Error::syntax(format!(
            "L'expression suivante n'est pas valide :\n   \n   {single_line}\n   {pointer}^\n   {message}\n"
        ))
        .in_rule(rule)
    })
}

/// `.replace(/\s*\n\s*/g, ' ').trim()`.
fn collapse_lines(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut pending = String::new();
    let mut newline = false;
    for c in text.chars() {
        if c == '\n' {
            newline = true;
            pending.clear();
        } else if is_js_whitespace(c) {
            pending.push(c);
        } else {
            if newline {
                result.push(' ');
            } else {
                result.push_str(&pending);
            }
            pending.clear();
            newline = false;
            result.push(c);
        }
    }
    if newline {
        result.push(' ');
    } else {
        result.push_str(&pending);
    }
    result.trim_matches(is_js_whitespace).to_owned()
}
