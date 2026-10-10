//! Mechanisms defined as macros over other mechanisms.

use std::collections::HashMap;

use super::{Parser, default_constant};
use crate::ast::{Constant, Node, NodeId, Program, Value};
use crate::error::{Error, Result};
use crate::raw::Raw;

/// A template is parsed once; each use copies it with its arguments substituted.
struct Template {
    body: NodeId,
    args: &'static [&'static str],
}

pub struct Templates {
    not_applicable: NodeId,
    templates: HashMap<&'static str, Template>,
}

type Body = fn(NodeId) -> Raw;

fn s(text: &str) -> Raw {
    Raw::str(text)
}

fn condition(si: Raw, alors: Raw, sinon: Raw) -> Raw {
    Raw::map([(
        "condition",
        Raw::map([("si", si), ("alors", alors), ("sinon", sinon)]),
    )])
}

/// Templates in dependency order: a body may use the templates before it.
const TEMPLATES: [(&str, &[&str], Body); 9] = [
    ("applicable si", &["applicable si", "valeur"], |na| {
        condition(s("applicable si != non"), s("valeur"), Raw::Node(na))
    }),
    (
        "non applicable si",
        &["non applicable si", "valeur"],
        |na| condition(s("non applicable si = non"), s("valeur"), Raw::Node(na)),
    ),
    ("par défaut", &["par défaut", "valeur"], |_| {
        condition(
            Raw::map([("est non défini", s("valeur"))]),
            s("par défaut"),
            s("valeur"),
        )
    }),
    (
        "dans la situation",
        &["valeur", "dans la situation"],
        |_| {
            condition(
                Raw::map([("est non défini", s("dans la situation"))]),
                s("valeur"),
                s("dans la situation"),
            )
        },
    ),
    ("est défini", &["valeur"], |_| {
        Raw::map([(
            "=",
            Raw::List(vec![Raw::map([("est non défini", s("valeur"))]), s("non")]),
        )])
    }),
    ("est applicable", &["valeur"], |_| {
        Raw::map([(
            "=",
            Raw::List(vec![
                Raw::map([("est non applicable", s("valeur"))]),
                s("non"),
            ]),
        )])
    }),
    ("plafond", &["plafond", "valeur"], |_| {
        condition(
            Raw::map([(
                "et",
                Raw::List(vec![s("plafond != non"), s("valeur > plafond")]),
            )]),
            s("plafond"),
            s("valeur"),
        )
    }),
    ("plancher", &["plancher", "valeur"], |_| {
        condition(
            Raw::map([(
                "et",
                Raw::List(vec![s("plancher != non"), s("valeur < plancher")]),
            )]),
            s("plancher"),
            s("valeur"),
        )
    }),
    ("abattement", &["abattement", "valeur"], |_| {
        Raw::map([
            ("-", Raw::List(vec![s("valeur"), s("abattement")])),
            ("plancher", Raw::Number(0.)),
        ])
    }),
];

const INLINE_CONTEXT: &str = "INLINE_MECANISM";

impl Templates {
    pub fn new(program: &mut Program) -> Result<Self> {
        let not_applicable = program.push(Node::Constant(Constant {
            value: Value::Null,
            ty: None,
            nullable: Some(true),
            unit: None,
        }));
        let mut templates = Templates {
            not_applicable,
            templates: HashMap::new(),
        };
        for (name, args, body) in TEMPLATES {
            let body =
                Parser::new(program, &templates, INLINE_CONTEXT).parse(&body(not_applicable))?;
            templates.templates.insert(name, Template { body, args });
        }
        Ok(templates)
    }
}

impl Parser<'_> {
    /// Parses `key` when it names an inline mechanism.
    pub(super) fn parse_inline(&mut self, key: &str, value: &Raw) -> Result<Option<NodeId>> {
        if let Some(template) = self.templates.templates.get(key) {
            let provided = if template.args == ["valeur"] {
                Raw::map([("valeur", value.clone())])
            } else {
                value.clone()
            };
            let mut args = HashMap::new();
            if let Raw::Map(entries) = &provided {
                for (name, raw) in entries {
                    let node = self.parse(raw)?;
                    args.insert(name.clone(), node);
                }
            }
            let (body, declared) = (template.body, template.args);
            return self.substitute(key, body, declared, &args).map(Some);
        }
        let list = match key {
            "somme"
            | "produit"
            | "multiplication"
            | "moyenne"
            | "toutes ces conditions"
            | "une de ces conditions"
            | "le maximum de"
            | "le minimum de" => self.parse_list(value)?,
            _ => return Ok(None),
        };
        let na = self.templates.not_applicable;
        let body = match key {
            "somme" => sum(list, na),
            "produit" | "multiplication" => Raw::map([
                (
                    "valeur",
                    product(
                        list,
                        self.push(Node::Constant(default_constant(Value::Number(1.)))),
                    ),
                ),
                ("simplifier l'unité", s("oui")),
            ]),
            "moyenne" => {
                let mut values = list;
                values.reverse();
                let total = sum_in_order(values.clone(), na);
                let counts = values.into_iter().map(one_if_applicable).collect();
                Raw::map([("/", Raw::List(vec![total, sum(counts, na)]))])
            }
            "toutes ces conditions" => list.into_iter().fold(s("oui"), |acc, value| {
                Raw::map([("et", Raw::List(vec![acc, value]))])
            }),
            "une de ces conditions" => list.into_iter().fold(s("non"), |acc, value| {
                Raw::map([("ou", Raw::List(vec![acc, value]))])
            }),
            _ => extremum(list, na, if key == "le maximum de" { ">" } else { "<" }),
        };
        self.parse(&body).map(Some)
    }

    fn parse_list(&mut self, value: &Raw) -> Result<Vec<Raw>> {
        let Raw::List(items) = value else {
            return Err(Error::syntax("Ce mécanisme attend une liste").in_rule(&self.context));
        };
        items
            .iter()
            .map(|item| self.parse(item).map(Raw::Node))
            .collect()
    }

    /// Copies `node`, replacing references to the template arguments with `args`.
    fn substitute(
        &mut self,
        mechanism: &str,
        node: NodeId,
        declared: &[&str],
        args: &HashMap<String, NodeId>,
    ) -> Result<NodeId> {
        let current = self.program.node(node).clone();
        match current {
            Node::Reference(reference) if declared.contains(&reference.name.as_str()) => {
                args.get(&reference.name).copied().ok_or_else(|| {
                    Error::syntax(format!(
                        "Il manque la clé '{} dans le mécanisme {mechanism}",
                        reference.name
                    ))
                    .in_rule(&reference.name)
                })
            }
            Node::Reference(_) | Node::Constant(_) | Node::Unsupported(_) => Ok(node),
            other => {
                let copy = other
                    .map_children(|child| self.substitute(mechanism, child, declared, args))?;
                Ok(self.program.push(copy))
            }
        }
    }
}

/// `reduceToSumNodes`: `a + (b + (c + non applicable))`.
fn sum(mut values: Vec<Raw>, na: NodeId) -> Raw {
    values.reverse();
    sum_in_order(values, na)
}

fn sum_in_order(reversed: Vec<Raw>, na: NodeId) -> Raw {
    reversed.into_iter().fold(Raw::Node(na), |acc, value| {
        Raw::map([("+", Raw::List(vec![value, acc]))])
    })
}

/// `reduceToProduitNodes`: `c * (b * (a * 1))`.
fn product(values: Vec<Raw>, one: NodeId) -> Raw {
    values.into_iter().fold(Raw::Node(one), |acc, value| {
        Raw::map([("*", Raw::List(vec![value, acc]))])
    })
}

fn one_if_applicable(value: Raw) -> Raw {
    Raw::map([
        ("applicable si", Raw::map([("est applicable", value)])),
        ("valeur", Raw::Number(1.)),
    ])
}

fn extremum(values: Vec<Raw>, na: NodeId, comparison: &str) -> Raw {
    values.into_iter().fold(Raw::Node(na), |acc, value| {
        let outer = condition(
            Raw::map([("est non applicable", s("$INTERNAL valeur"))]),
            s("$INTERNAL acc"),
            condition(
                Raw::map([(
                    "ou",
                    Raw::List(vec![
                        Raw::map([("est non applicable", s("$INTERNAL acc"))]),
                        Raw::map([(
                            comparison,
                            Raw::List(vec![s("$INTERNAL valeur"), s("$INTERNAL acc")]),
                        )]),
                    ]),
                )]),
                s("$INTERNAL valeur"),
                s("$INTERNAL acc"),
            ),
        );
        let Raw::Map(mut entries) = outer else {
            unreachable!("condition builds a map")
        };
        entries.push((
            "avec".to_owned(),
            Raw::map([
                ("[privé] $INTERNAL valeur", Raw::map([("valeur", value)])),
                ("[privé] $INTERNAL acc", Raw::map([("valeur", acc)])),
            ]),
        ));
        Raw::Map(entries)
    })
}
