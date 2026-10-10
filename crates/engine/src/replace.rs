//! Replacements: each reference to a replaced rule becomes a `variations` node that picks the
//! first applicable replacing rule, then the reference itself.

use rustc_hash::{FxHashMap, FxHashSet};
use std::cmp::Ordering;

use crate::ast::{Branch, ContextePair, NameId, Node, NodeId, Program, Replacement, Value};
use crate::collation::{Weights, weights};
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
    let (replacements, affected): (FxHashMap<NameId, Vec<Replacement>>, FxHashSet<NameId>) =
        if overlay {
            let replacements = std::mem::take(&mut program.replacements);
            let affected = uses
                .iter()
                .filter(|(_, used)| replacements.contains_key(used))
                .map(|&(rule, _)| rule)
                .collect();
            (replacements, affected)
        } else {
            let mut replacements: FxHashMap<NameId, Vec<Replacement>> = FxHashMap::default();
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
            for candidates in replacements.values_mut() {
                sort_replacements(program, candidates);
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

fn name_of(program: &Program, reference: NodeId) -> &str {
    target(program, reference).map_or("", |name| program.names.name(name))
}

/// The replacements of `replaced` that apply to a reference written in `context`, in the
/// order they are tried.
pub fn applicable_replacements<'a>(
    program: &Program,
    replacements: &'a FxHashMap<NameId, Vec<Replacement>>,
    replaced: NameId,
    context: NameId,
) -> Vec<&'a Replacement> {
    let Some(candidates) = replacements.get(&replaced) else {
        return Vec::new();
    };
    let context_name = program.names.name(context);
    candidates
        .iter()
        .filter(|r| target(program, r.definition) != Some(context))
        .filter(|r| {
            r.whitelist.is_empty()
                || r.whitelist
                    .iter()
                    .any(|&w| context_name.starts_with(name_of(program, w)))
        })
        .filter(|r| {
            r.blacklist
                .iter()
                .all(|&b| !context_name.starts_with(name_of(program, b)))
        })
        .collect()
}

/// Orders the replacements of a rule as they are tried: by decreasing priority, then by
/// decreasing name of the rule defining them, the last defined first among equals. Filtering
/// keeps this order, so it is computed once.
fn sort_replacements(program: &Program, replacements: &mut [Replacement]) {
    replacements.reverse();
    let mut keyed: Vec<(f64, Weights, Replacement)> = replacements
        .iter()
        .map(|r| {
            (
                r.priority,
                weights(name_of(program, r.definition)),
                r.clone(),
            )
        })
        .collect();
    keyed.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(Ordering::Equal)
            .then_with(|| b.1.cmp(&a.1))
    });
    for (slot, (_, _, replacement)) in replacements.iter_mut().zip(keyed) {
        *slot = replacement;
    }
}

struct Inliner<'a> {
    program: &'a mut Program,
    replacements: &'a FxHashMap<NameId, Vec<Replacement>>,
}

impl Inliner<'_> {
    fn inline(&mut self, id: NodeId) -> NodeId {
        match self.program.node(id) {
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
                let (valeur, pairs, key) = (*valeur, pairs.clone(), *key);
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
            node => {
                let children = node.children();
                let inlined = children
                    .into_iter()
                    .map(|child| self.inline(child))
                    .collect();
                let copy = self.program.node(id).with_children(inlined);
                self.program.push(copy)
            }
        }
    }

    fn replace(&mut self, id: NodeId) -> NodeId {
        let Node::Reference(reference) = self.program.node(id) else {
            return id;
        };
        let (Some(replaced), context) = (reference.target, reference.context) else {
            return id;
        };
        let applicable =
            applicable_replacements(self.program, self.replacements, replaced, context);
        if applicable.is_empty() {
            return id;
        }
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
