//! Mechanisms defined as macros over other mechanisms.

use rustc_hash::FxHashMap;

use super::{Parser, default_constant};
use crate::ast::{Constant, Node, NodeId, Program, SourceArgs, Value};
use crate::error::{Error, Result};
use crate::raw::Raw;

/// A template is parsed once; each use copies it with its arguments substituted.
#[derive(Clone)]
struct Template {
    body: NodeId,
    args: &'static [&'static str],
    /// The path from the body to a use of each argument.
    paths: Vec<Box<[u16]>>,
}

#[derive(Clone)]
pub struct Templates {
    not_applicable: NodeId,
    templates: FxHashMap<&'static str, Template>,
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
            Raw::list(vec![Raw::map([("est non défini", s("valeur"))]), s("non")]),
        )])
    }),
    ("est applicable", &["valeur"], |_| {
        Raw::map([(
            "=",
            Raw::list(vec![
                Raw::map([("est non applicable", s("valeur"))]),
                s("non"),
            ]),
        )])
    }),
    ("plafond", &["plafond", "valeur"], |_| {
        condition(
            Raw::map([(
                "et",
                Raw::list(vec![s("plafond != non"), s("valeur > plafond")]),
            )]),
            s("plafond"),
            s("valeur"),
        )
    }),
    ("plancher", &["plancher", "valeur"], |_| {
        condition(
            Raw::map([(
                "et",
                Raw::list(vec![s("plancher != non"), s("valeur < plancher")]),
            )]),
            s("plancher"),
            s("valeur"),
        )
    }),
    ("abattement", &["abattement", "valeur"], |_| {
        Raw::map([
            ("-", Raw::list(vec![s("valeur"), s("abattement")])),
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
            templates: FxHashMap::default(),
        };
        for (name, args, body) in TEMPLATES {
            let body =
                Parser::new(program, &templates, INLINE_CONTEXT).parse(&body(not_applicable))?;
            let paths = args
                .iter()
                .map(|arg| {
                    path_to(
                        program,
                        body,
                        |_, node| matches!(node, Node::Reference(r) if r.name == *arg),
                    )
                    .expect("templates use each argument")
                })
                .collect();
            templates
                .templates
                .insert(name, Template { body, args, paths });
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
            let mut args = FxHashMap::default();
            let mut paths = Vec::new();
            if let Raw::Map(entries) = &provided {
                for (name, raw) in entries.iter() {
                    let node = self.parse(raw)?;
                    args.insert(name.to_string(), node);
                    if self.source_args
                        && let Some(index) = template.args.iter().position(|arg| **arg == **name)
                    {
                        paths.push(template.paths[index].clone());
                    }
                }
            }
            let (body, declared) = (template.body, template.args);
            let node = self.substitute(key, body, declared, &args)?;
            if self.source_args {
                self.program
                    .source_args
                    .insert(node, SourceArgs::Inline(paths.into()));
            }
            return Ok(Some(node));
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
        let items: Vec<NodeId> = list
            .iter()
            .filter_map(|item| match item {
                Raw::Node(id) => Some(*id),
                _ => None,
            })
            .collect();
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
                Raw::map([("/", Raw::list(vec![total, sum(counts, na)]))])
            }
            "toutes ces conditions" => list.into_iter().fold(s("oui"), |acc, value| {
                Raw::map([("et", Raw::list(vec![acc, value]))])
            }),
            "une de ces conditions" => list.into_iter().fold(s("non"), |acc, value| {
                Raw::map([("ou", Raw::list(vec![acc, value]))])
            }),
            _ => extremum(list, na, if key == "le maximum de" { ">" } else { "<" }),
        };
        let node = self.parse(&body)?;
        if self.source_args {
            let paths = items
                .into_iter()
                .filter_map(|item| path_to(self.program, node, |id, _| id == item))
                .collect::<Vec<_>>();
            self.program
                .source_args
                .insert(node, SourceArgs::Inline(paths.into()));
        }
        Ok(Some(node))
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
        args: &FxHashMap<String, NodeId>,
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
            Node::Reference(_) | Node::Constant(_) => Ok(node),
            other => {
                let copy = other
                    .map_children(|child| self.substitute(mechanism, child, declared, args))?;
                Ok(self.program.push(copy))
            }
        }
    }
}

/// The child indices leading from `root` to the first node, depth first, that `is_target`
/// accepts.
fn path_to(
    program: &Program,
    root: NodeId,
    is_target: impl Fn(NodeId, &Node) -> bool,
) -> Option<Box<[u16]>> {
    fn walk(
        program: &Program,
        id: NodeId,
        is_target: &impl Fn(NodeId, &Node) -> bool,
        path: &mut Vec<u16>,
    ) -> bool {
        if is_target(id, program.node(id)) {
            return true;
        }
        for (index, child) in program.node(id).children().into_iter().enumerate() {
            path.push(index as u16);
            if walk(program, child, is_target, path) {
                return true;
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    walk(program, root, &is_target, &mut path).then(|| path.into_boxed_slice())
}

/// `reduceToSumNodes`: `a + (b + (c + non applicable))`.
fn sum(mut values: Vec<Raw>, na: NodeId) -> Raw {
    values.reverse();
    sum_in_order(values, na)
}

fn sum_in_order(reversed: Vec<Raw>, na: NodeId) -> Raw {
    reversed.into_iter().fold(Raw::Node(na), |acc, value| {
        Raw::map([("+", Raw::list(vec![value, acc]))])
    })
}

/// `reduceToProduitNodes`: `c * (b * (a * 1))`.
fn product(values: Vec<Raw>, one: NodeId) -> Raw {
    values.into_iter().fold(Raw::Node(one), |acc, value| {
        Raw::map([("*", Raw::list(vec![value, acc]))])
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
                    Raw::list(vec![
                        Raw::map([("est non applicable", s("$INTERNAL acc"))]),
                        Raw::map([(
                            comparison,
                            Raw::list(vec![s("$INTERNAL valeur"), s("$INTERNAL acc")]),
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
        std::rc::Rc::make_mut(&mut entries).push((
            "avec".into(),
            Raw::map([
                ("[privé] $INTERNAL valeur", Raw::map([("valeur", value)])),
                ("[privé] $INTERNAL acc", Raw::map([("valeur", acc)])),
            ]),
        ));
        Raw::Map(entries)
    })
}
