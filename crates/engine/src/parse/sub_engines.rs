//! Mechanisms that evaluate in a sub-engine: `contexte` and `inversion numérique`.

use std::hash::{DefaultHasher, Hash, Hasher};

use super::Parser;
use crate::ast::{ContextePair, Node, NodeId};
use crate::collation::locale_compare;
use crate::error::Result;
use crate::evaluate::to_number;
use crate::raw::Raw;

fn fingerprint(context: &str, parts: &[(&str, &Raw)]) -> u64 {
    let mut hasher = DefaultHasher::new();
    context.hash(&mut hasher);
    for (name, raw) in parts {
        name.hash(&mut hasher);
        format!("{raw:?}").hash(&mut hasher);
    }
    hasher.finish()
}

impl Parser<'_> {
    pub(super) fn parse_contexte(&mut self, value: &Raw) -> Result<NodeId> {
        let valeur = self.parse_field(value, "valeur")?;
        let entries = match value.get("contexte") {
            Some(Raw::Map(entries)) => entries.clone(),
            _ => Vec::new(),
        };
        let mut pairs = Vec::with_capacity(entries.len());
        for (name, replacement) in &entries {
            let origin = self.parse(&Raw::str(name))?;
            let parsed = self.parse(replacement)?;
            pairs.push((
                name.as_str(),
                replacement,
                ContextePair {
                    origin,
                    replacement: parsed,
                    replacement_key: fingerprint(&self.context, &[(name, replacement)]),
                    situation: None,
                },
            ));
        }
        pairs.sort_by(|a, b| locale_compare(a.0, b.0));
        let parts: Vec<(&str, &Raw)> = pairs.iter().map(|(name, raw, _)| (*name, *raw)).collect();
        let id = fingerprint(&self.context, &parts);
        let pairs = pairs.into_iter().map(|(_, _, pair)| pair).collect();
        Ok(self.push(Node::Contexte { valeur, pairs, id }))
    }

    pub(super) fn parse_inversion(&mut self, value: &Raw) -> Result<NodeId> {
        if matches!(value, Raw::Null) {
            return Err(self.syntax(
                "Il manque les règles avec laquelle effectuer le calcul d'inversion dans le mécanisme `inversion numérique`",
            ));
        }
        let number = |key: &str, default: f64| match value {
            Raw::Map(_) => value.get(key).map_or(default, |raw| match raw {
                Raw::Number(n) => *n,
                Raw::String(s) => to_number(&crate::ast::Value::String(s.as_str().into())),
                _ => f64::NAN,
            }),
            _ => default,
        };
        let (min, max, tolerance) = (
            number("min", -1_000_000.),
            number("max", 100_000_000.),
            number("tolérance d'erreur", 0.1),
        );
        let avec = match value {
            Raw::Map(_) => value.get("avec").cloned().unwrap_or_else(|| value.clone()),
            other => other.clone(),
        };
        let items = match avec {
            Raw::List(items) => items,
            single => vec![single],
        };
        let mut candidates = Vec::with_capacity(items.len());
        for item in &items {
            let node = self.parse(item)?;
            let copy = self.program.node(node).clone();
            candidates.push(self.push(copy));
        }
        let rule = self.program.names.intern(&self.context);
        Ok(self.push(Node::Inversion {
            rule,
            candidates,
            min,
            max,
            tolerance,
            rules: None,
        }))
    }
}
