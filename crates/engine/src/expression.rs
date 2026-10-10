//! The publicodes expression grammar.
//!
//! Each regular expression is matched against the rest of the input, and the cursor advances by
//! the length of the match even when the match starts after the cursor.

use std::sync::LazyLock;

use regex::Regex;

use crate::date::normalize_date_string;
use crate::error::{Error, Result};
use crate::raw::Raw;

const SPACE: &str =
    r"[\t\x{20}\x{a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}\x{feff}]";
const LETTER: &str = r"[a-zA-Z\x{C0}-\x{17F}$]";
const SYMBOL: &str = r#"[',°€$%²_"«»]"#;
const DIGIT: &str = "[0-9]";
/// `p`, `{`, `S`, `c` and `}` are literal characters here, not a Unicode class.
const UNIT_SYMBOL: &str = r"[°%p\{Sc\}€$]";

fn any_char() -> String {
    format!("({LETTER}|{SYMBOL}|{DIGIT})")
}

fn phrase_starting_with(first: &str) -> String {
    let any = any_char();
    let any_or_special = format!(r"({any}|\-|\+|')");
    format!("({first}{any_or_special}*)({SPACE}{any}{any_or_special}*)*")
}

fn between_spaces(operators: &[&str]) -> Regex {
    regex(&format!("^{SPACE}+({}){SPACE}+", operators.join("|")))
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid expression grammar")
}

struct Grammar {
    spaces: Regex,
    string: Regex,
    number: Regex,
    date: Regex,
    dotted_name: Regex,
    unit: Regex,
    exponentiation: Regex,
    multiplication: Regex,
    addition: Regex,
    comparison: Regex,
    operator: Regex,
}

static GRAMMAR: LazyLock<Grammar> = LazyLock::new(|| {
    let rule_name = phrase_starting_with(LETTER);
    let unit_identifier = phrase_starting_with(&format!("({UNIT_SYMBOL}|{LETTER})"));
    Grammar {
        spaces: regex(&format!("^{SPACE}*")),
        string: regex(r#"'.*'|".*""#),
        number: regex(r"[0-9]+(\.[0-9]+)?"),
        date: regex(r"^(?:(?:0?[1-9]|[12][0-9]|3[01])/)?(?:0?[1-9]|1[012])/[0-9]{4}"),
        dotted_name: regex(&format!(r"^({rule_name}|\^)( \. {rule_name})*")),
        unit: regex(&format!(
            r"^{SPACE}*(/)?{unit_identifier}({SPACE}*(\.|/){unit_identifier})*"
        )),
        exponentiation: between_spaces(&[r"\*\*"]),
        multiplication: between_spaces(&[r"\*", "/", "//"]),
        addition: between_spaces(&["-", r"\+"]),
        comparison: between_spaces(&[">", "<", ">=", "<=", "=", "!="]),
        operator: regex(r"(\+|-|\*|/|\*\*|>|<|>=|<=|=|!=)"),
    }
});

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
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        self.text.get(self.current..).unwrap_or("")
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
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

    fn found(&self) -> String {
        self.peek().map(String::from).unwrap_or_default()
    }

    fn is_next(&self, regex: &Regex) -> bool {
        regex.is_match(self.rest())
    }

    /// Matches `regex` anywhere in the rest and advances by the match length.
    fn expect_regex(&mut self, regex: &Regex, description: &str) -> Parse<&'a str> {
        let rest = self.rest();
        let Some(found) = regex.find(rest) else {
            let preview: String = rest.chars().take(5).collect();
            return Err(self.error(description, preview, None));
        };
        self.current += found.len();
        Ok(found.as_str())
    }

    fn expect_operator(&mut self, regex: &Regex) -> &'a str {
        let rest = self.rest();
        let captures = regex.captures(rest).expect("operator was peeked");
        self.current += captures[0].len();
        captures.get(1).map_or("", |m| m.as_str())
    }

    fn expect_char(&mut self, expected: char) -> Parse<()> {
        match self.peek() {
            Some(c) if c == expected => {
                self.current += c.len_utf8();
                Ok(())
            }
            other => Err(self.error(
                &format!("'{expected}'"),
                other.map(String::from).unwrap_or_default(),
                None,
            )),
        }
    }

    fn binary_level(
        &mut self,
        operators: &Regex,
        next: fn(&mut Self) -> Parse<Raw>,
        right_recursive: bool,
    ) -> Parse<Raw> {
        let mut left = next(self)?;
        while !self.is_eol() && self.is_next(operators) {
            let operator = self.expect_operator(operators);
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
        self.binary_level(&GRAMMAR.comparison, Self::addition, false)
    }

    fn addition(&mut self) -> Parse<Raw> {
        self.binary_level(&GRAMMAR.addition, Self::multiplication, false)
    }

    fn multiplication(&mut self) -> Parse<Raw> {
        self.binary_level(&GRAMMAR.multiplication, Self::exponentiation, false)
    }

    fn exponentiation(&mut self) -> Parse<Raw> {
        self.binary_level(&GRAMMAR.exponentiation, Self::primary, true)
    }

    fn primary(&mut self) -> Parse<Raw> {
        match self.peek() {
            Some('(') => {
                self.current += 1;
                self.expect_regex(&GRAMMAR.spaces, "spaces")?;
                let expression = self.comparison()?;
                self.expect_regex(&GRAMMAR.spaces, "spaces")?;
                self.expect_char(')')?;
                Ok(expression)
            }
            Some('-') => {
                self.current += 1;
                self.expect_regex(&GRAMMAR.spaces, "spaces")?;
                let expression = self.primary()?;
                Ok(binary("-", number(0.), expression))
            }
            _ if self.is_next(&GRAMMAR.date) => {
                let text = self.expect_regex(&GRAMMAR.date, "Une date")?;
                Ok(constant("date", Raw::String(date_value(text)?)))
            }
            Some(c) if c.is_ascii_digit() => {
                let text = self.expect_regex(&GRAMMAR.number, "Un nombre")?;
                let value = Raw::Number(text.parse().unwrap_or(f64::NAN));
                if !self.is_next(&GRAMMAR.unit) {
                    return Ok(constant("number", value));
                }
                let unit = self.expect_regex(&GRAMMAR.unit, "Une unité")?;
                Ok(Raw::map([(
                    "constant",
                    Raw::map([
                        ("type", Raw::str("number")),
                        ("nodeValue", value),
                        ("rawUnit", Raw::str(unit.trim())),
                    ]),
                )]))
            }
            Some('\'' | '"') => {
                let text = self.expect_regex(&GRAMMAR.string, "Une chaîne de caractères")?;
                let inner = text.get(1..text.len().saturating_sub(1)).unwrap_or("");
                Ok(constant("string", Raw::str(inner)))
            }
            Some(c) if c == '^' || is_letter(c) => {
                let name = self.expect_regex(&GRAMMAR.dotted_name, "Un nom de règle")?;
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

fn date_value(text: &str) -> Parse<String> {
    normalize_date_string(text).map_err(Failure::Date)
}

fn is_letter(c: char) -> bool {
    c.is_ascii_alphabetic() || ('\u{C0}'..='\u{17F}').contains(&c) || c == '$'
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
    Raw::map([(operator, Raw::List(vec![left, right]))])
}

/// Whether `expression` parses to a single reference, written as `name`.
pub fn is_reference(expression: &str) -> bool {
    expression.starts_with(is_letter)
        && !matches!(expression, "oui" | "non")
        && GRAMMAR
            .dotted_name
            .find(expression)
            .is_some_and(|m| m.len() == expression.len())
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
        if parser.is_next(&GRAMMAR.operator) {
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
            Failure::Grammar {
                position,
                expected,
                found,
                custom,
            } => (position, expected, found, custom),
        };
        let pointer = " ".repeat(single_line[..position.min(single_line.len())].chars().count());
        let message = custom.map(str::to_owned).unwrap_or_else(|| {
            if found.is_empty() {
                format!("{expected} est attendu, hors l’expression se termine prématurément")
            } else {
                format!("{expected} est attendu, mais \"{found}\" a été trouvé")
            }
        });
        Error::syntax(format!(
            "L'expression suivante n'est pas valide :\n   {single_line}\n   {pointer}^\n   {message}\n"
        ))
        .in_rule(rule)
    })
}

/// `.replace(/\s*\n\s*/g, ' ').trim()`.
fn collapse_lines(text: &str) -> String {
    let is_space = |c: char| c.is_whitespace() || c == '\u{feff}';
    let mut result = String::with_capacity(text.len());
    let mut pending = String::new();
    let mut newline = false;
    for c in text.chars() {
        if c == '\n' {
            newline = true;
            pending.clear();
        } else if is_space(c) {
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
    result.trim_matches(is_space).to_owned()
}
