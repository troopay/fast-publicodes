//! Replacements: each reference to a replaced rule becomes a `variations` node that picks the
//! first applicable replacing rule, then the reference itself.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::ast::{Branch, ContextePair, NameId, Node, NodeId, Program, Replacement, Value};
use crate::collation::locale_compare;
use crate::error::{Error, ErrorKind, Result};
use crate::parse::default_constant;

fn target(program: &Program, reference: NodeId) -> Option<NameId> {
    match program.node(reference) {
        Node::Reference(reference) => reference.target,
        _ => None,
    }
}

/// Inlines replacements into `rules`. The base rules get their own replacements; rules added
/// later get the base ones.
pub fn inline_replacements(
    program: &mut Program,
    rules: &mut [(NameId, NodeId)],
    uses: &[(NameId, NameId)],
    overlay: bool,
) -> Result<()> {
    let (replacements, affected): (HashMap<NameId, Vec<Replacement>>, HashSet<NameId>) = if overlay
    {
        let replacements = std::mem::take(&mut program.replacements);
        let affected = uses
            .iter()
            .filter(|(_, used)| replacements.contains_key(used))
            .map(|&(rule, _)| rule)
            .collect();
        (replacements, affected)
    } else {
        let mut replacements: HashMap<NameId, Vec<Replacement>> = HashMap::new();
        for &(_, node) in rules.iter() {
            let Node::Rule(rule) = program.node(node) else {
                continue;
            };
            for replacement in &rule.replacements {
                let replaced = target(program, replacement.replaced).ok_or_else(|| {
                    Error::new(
                        ErrorKind::Internal,
                        "Le remplacement ne référence pas une règle",
                    )
                })?;
                replacements
                    .entry(replaced)
                    .or_default()
                    .push(replacement.clone());
            }
        }
        let affected = uses
            .iter()
            .filter(|(_, used)| replacements.contains_key(used))
            .map(|&(rule, _)| rule)
            .collect();
        (replacements, affected)
    };
    if !affected.is_empty() {
        let mut inliner = Inliner {
            program,
            replacements: &replacements,
        };
        for (name, node) in rules.iter_mut() {
            if affected.contains(name) {
                *node = inliner.inline(*node);
            }
        }
    }
    program.replacements = replacements;
    Ok(())
}

struct Inliner<'a> {
    program: &'a mut Program,
    replacements: &'a HashMap<NameId, Vec<Replacement>>,
}

impl Inliner<'_> {
    fn inline(&mut self, id: NodeId) -> NodeId {
        match self.program.node(id).clone() {
            Node::Reference(_) => self.replace(id),
            Node::Constant(_)
            | Node::UnePossibilite { .. }
            | Node::Inversion { .. }
            | Node::SituationInput => id,
            Node::Contexte {
                valeur,
                pairs,
                id: key,
            } => {
                let valeur = self.inline(valeur);
                let pairs = pairs
                    .into_iter()
                    .map(|pair| ContextePair {
                        replacement: self.inline(pair.replacement),
                        ..pair
                    })
                    .collect();
                self.program.push(Node::Contexte {
                    valeur,
                    pairs,
                    id: key,
                })
            }
            other => {
                let copy = other
                    .map_children(|child| Ok::<_, std::convert::Infallible>(self.inline(child)))
                    .unwrap_or_else(|never| match never {});
                self.program.push(copy)
            }
        }
    }

    fn name_of(&self, reference: NodeId) -> &str {
        target(self.program, reference).map_or("", |name| self.program.names.name(name))
    }

    fn replace(&mut self, id: NodeId) -> NodeId {
        let Node::Reference(reference) = self.program.node(id) else {
            return id;
        };
        let (Some(replaced), context) = (reference.target, reference.context) else {
            return id;
        };
        let Some(candidates) = self.replacements.get(&replaced) else {
            return id;
        };
        let context_name = self.program.names.name(context);
        let mut applicable: Vec<&Replacement> = candidates
            .iter()
            .filter(|r| target(self.program, r.definition) != Some(context))
            .filter(|r| {
                r.whitelist.is_empty()
                    || r.whitelist
                        .iter()
                        .any(|&w| context_name.starts_with(self.name_of(w)))
            })
            .filter(|r| {
                r.blacklist
                    .iter()
                    .all(|&b| !context_name.starts_with(self.name_of(b)))
            })
            .collect();
        if applicable.is_empty() {
            return id;
        }
        applicable.reverse();
        applicable.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    locale_compare(self.name_of(b.definition), self.name_of(a.definition))
                })
        });
        let key: Vec<u32> = applicable.iter().map(|r| r.id).collect();
        if let Some(&node) = self.program.replacement_nodes.get(&key) {
            return node;
        }
        let mut branches = Vec::with_capacity(applicable.len() + 1);
        for replacement in applicable {
            let definition = replacement.definition;
            branches.push(if replacement.non_applicable {
                Branch {
                    condition: definition,
                    consequence: self.constant(Value::Null),
                }
            } else {
                let si = self.program.push(Node::EstNonApplicable(definition));
                let alors = self.constant(Value::Bool(false));
                let sinon = self.constant(Value::Bool(true));
                Branch {
                    condition: self.program.push(Node::Condition { si, alors, sinon }),
                    consequence: definition,
                }
            });
        }
        branches.push(Branch {
            condition: self.constant(Value::Bool(true)),
            consequence: id,
        });
        let node = self.program.push(Node::Variations(branches));
        self.program.replacement_nodes.insert(key, node);
        node
    }

    fn constant(&mut self, value: Value) -> NodeId {
        let constant = match value {
            Value::Null => crate::ast::Constant {
                nullable: Some(true),
                ..crate::ast::Constant::new(Value::Null)
            },
            value => default_constant(value),
        };
        self.program.push(Node::Constant(constant))
    }
}
