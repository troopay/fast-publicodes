//! `getRule`, `getParsedRules` and `getPossibilitiesFor`, without explanation trees.

use serde_json::{Value as Json, json};

use super::Engine;
use crate::ast::{Node, NodeId, PossibilityKind, Rule, Value};
use crate::error::{Error, ErrorKind, Result};
use crate::evaluate::Evaluator;
use crate::units::Unit;

fn value_json(value: &Value) -> Json {
    match value {
        Value::Undefined | Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Number(n) => serde_json::Number::from_f64(*n).map_or(Json::Null, Json::Number),
        Value::String(s) => Json::String(s.to_string()),
    }
}

fn unit_json(unit: &Unit) -> Json {
    json!({ "numerators": unit.numerators, "denominators": unit.denominators })
}

fn kind_name(kind: PossibilityKind) -> &'static str {
    match kind {
        PossibilityKind::Number => "number",
        PossibilityKind::String => "string",
        PossibilityKind::Reference => "reference",
    }
}

impl Engine {
    /// The rules `getParsedRules` lists, in definition order.
    pub fn rule_names(&self) -> impl Iterator<Item = &str> {
        self.program
            .rule_order
            .iter()
            .filter(|&&name| {
                self.program
                    .rule(name)
                    .is_some_and(|rule| rule.reports_missing)
            })
            .map(|&name| self.program.names.name(name))
    }

    fn public_rule(&self, name: &str) -> Result<&Rule> {
        let rule = self
            .program
            .names
            .get(name)
            .and_then(|id| self.program.base_rule(id))
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::UnknownRule,
                    format!("La règle '{name}' n'existe pas"),
                )
                .in_rule(name)
            })?;
        if !rule.reports_missing {
            return Err(Error::new(
                ErrorKind::PrivateRule,
                format!("La règle {name} est une règle privée."),
            )
            .in_rule(name));
        }
        Ok(rule)
    }

    /// `getRule`: the rule node, without its explanation.
    pub fn rule(&self, name: &str) -> Result<Json> {
        let rule = self.public_rule(name)?;
        let possibilities = match rule.possibilities {
            Some(possibilities) => self.possibilities_json(possibilities, None),
            None => Json::Null,
        };
        Ok(json!({
            "dottedName": name,
            "title": rule.title,
            "nodeKind": "rule",
            "private": rule.private,
            "virtualRule": rule.private,
            "rawNode": rule.raw.to_json(),
            "possibilities": possibilities,
        }))
    }

    /// `getPossibilitiesFor`.
    pub fn possibilities(&mut self, name: &str, filter_not_applicable: bool) -> Result<Json> {
        if filter_not_applicable && !self.program.options.filter_not_applicable_possibilities {
            return Err(Error::new(
                ErrorKind::Engine,
                "You must enable `flag.filterNotApplicablePossibilities` in engine instantiation to use the filterNotApplicable option",
            ));
        }
        let Some(possibilities) = self.public_rule(name)?.possibilities else {
            return Ok(Json::Null);
        };
        let not_applicable = if filter_not_applicable {
            Some(self.evaluate_not_applicable(possibilities)?)
        } else {
            None
        };
        let mut options = match self.possibilities_json(possibilities, not_applicable.as_deref()) {
            Json::Object(mut node) => node.remove("explanation").unwrap_or(Json::Null),
            _ => Json::Null,
        };
        if let (Json::Array(items), Some(excluded)) = (&mut options, &not_applicable) {
            let mut index = 0;
            items.retain(|_| {
                let keep = !excluded[index];
                index += 1;
                keep
            });
        }
        Ok(options)
    }

    /// Whether each option is not applicable, from the evaluation of the possibilities.
    fn evaluate_not_applicable(&mut self, possibilities: NodeId) -> Result<Vec<bool>> {
        let Node::UnePossibilite { not_applicable, .. } = self.program.node(possibilities) else {
            return Ok(Vec::new());
        };
        let not_applicable = not_applicable.clone();
        let mut evaluator = Evaluator::new(&self.program, &mut self.state);
        evaluator.evaluate(possibilities)?;
        not_applicable
            .into_iter()
            .map(|node| match node {
                Some(node) => Ok(evaluator.evaluate(node)?.value == Value::Bool(true)),
                None => Ok(false),
            })
            .collect()
    }

    fn possibilities_json(&self, possibilities: NodeId, not_applicable: Option<&[bool]>) -> Json {
        let program = &self.program;
        let Node::UnePossibilite {
            kind,
            unit,
            options,
            values,
            ..
        } = program.node(possibilities)
        else {
            return Json::Null;
        };
        let explanation: Vec<Json> = options
            .iter()
            .zip(values)
            .enumerate()
            .map(|(index, (&option, written))| {
                let excluded = not_applicable.is_some_and(|n| n[index]);
                let not_applicable = json!({
                    "nodeKind": "constant",
                    "type": "boolean",
                    "nodeValue": excluded,
                });
                match program.node(option) {
                    Node::Reference(reference) => {
                        let target = reference.target.map(|t| program.names.name(t));
                        let title = reference
                            .target
                            .and_then(|t| program.rule(t))
                            .map(|rule| rule.title.clone());
                        json!({
                            "nodeKind": "reference",
                            "type": "reference",
                            "name": reference.name,
                            "contextDottedName": program.names.name(reference.context),
                            "dottedName": target,
                            "title": title,
                            "nodeValue": reference.name,
                            "publicodesValue": written,
                            "notApplicable": not_applicable,
                        })
                    }
                    Node::Constant(constant) => {
                        let mut node = json!({
                            "nodeKind": "constant",
                            "type": kind_name(*kind),
                            "nodeValue": value_json(&constant.value),
                            "publicodesValue": written,
                            "notApplicable": not_applicable,
                        });
                        if let Some(unit) = &constant.unit {
                            node["unit"] = unit_json(unit);
                        }
                        node
                    }
                    _ => Json::Null,
                }
            })
            .collect();
        let mut node = json!({
            "nodeKind": "une possibilité",
            "type": kind_name(*kind),
            "explanation": explanation,
        });
        if let Some(unit) = unit {
            node["unit"] = unit_json(unit);
        }
        node
    }
}
