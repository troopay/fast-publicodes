//! `String.prototype.localeCompare` with the default collation: letters compare without accents
//! or case first, then by accents, then lowercase before uppercase.

use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Space,
    Punctuation,
    Symbol,
    Digit,
    Letter,
}

/// The base letters, accent and case of a character.
fn decompose(c: char) -> (&'static str, u8, bool) {
    const TABLE: &[(&str, &str, u8)] = &[
        ("àáâãäå", "a", 0),
        ("ç", "c", 6),
        ("èéêë", "e", 0),
        ("ìíîï", "i", 0),
        ("ñ", "n", 7),
        ("òóôõö", "o", 0),
        ("ùúûü", "u", 0),
        ("ýÿ", "y", 0),
    ];
    const ACCENTS: [&str; 6] = ["àèìòù", "áéíóúý", "âêîôû", "ãõ", "äëïöüÿ", "å"];
    let upper = c.is_uppercase();
    let lower = c.to_lowercase().next().unwrap_or(c);
    match lower {
        'œ' => return ("oe", 0, upper),
        'æ' => return ("ae", 0, upper),
        _ => {}
    }
    for (letters, base, accent) in TABLE {
        if letters.contains(lower) {
            let accent = match accent {
                0 => ACCENTS
                    .iter()
                    .position(|group| group.contains(lower))
                    .map_or(0, |i| i as u8 + 1),
                other => *other,
            };
            return (base, accent, upper);
        }
    }
    ("", 0, upper)
}

fn class(c: char) -> Class {
    if c.is_whitespace() {
        Class::Space
    } else if c.is_alphabetic() {
        Class::Letter
    } else if c.is_numeric() {
        Class::Digit
    } else if c.is_ascii_punctuation() && !"$+<=>^`|~".contains(c) {
        Class::Punctuation
    } else {
        Class::Symbol
    }
}

type Weights = (Vec<(Class, char)>, Vec<u8>, Vec<bool>);

fn weights(text: &str) -> Weights {
    let (mut primary, mut secondary, mut tertiary) = (Vec::new(), Vec::new(), Vec::new());
    for c in text.chars() {
        let (base, accent, upper) = decompose(c);
        let class = class(c);
        if base.is_empty() {
            primary.push((class, c.to_lowercase().next().unwrap_or(c)));
            secondary.push(0);
            tertiary.push(upper);
        } else {
            for letter in base.chars() {
                primary.push((class, letter));
                secondary.push(accent);
                tertiary.push(upper);
            }
        }
    }
    (primary, secondary, tertiary)
}

pub fn locale_compare(a: &str, b: &str) -> Ordering {
    let (a, b) = (weights(a), weights(b));
    a.0.cmp(&b.0)
        .then_with(|| a.1.cmp(&b.1))
        .then_with(|| a.2.cmp(&b.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_letters_before_accents_and_case() {
        assert_eq!(locale_compare("a", "B"), Ordering::Less);
        assert_eq!(locale_compare("é", "f"), Ordering::Less);
        assert_eq!(locale_compare("e", "é"), Ordering::Less);
        assert_eq!(locale_compare("a", "A"), Ordering::Less);
        assert_eq!(locale_compare("a b", "ab"), Ordering::Less);
    }
}
