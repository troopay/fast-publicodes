//! Replacements: each reference to a replaced rule becomes a `variations` node that picks the
//! first applicable replacing rule, then the reference itself.

use rustc_hash::FxHashSet;
use std::cmp::Ordering;
use std::rc::Rc;

use crate::ast::{
    Branch, Constant, ContextePair, NameId, Node, NodeId, Program, Replacement, Replacements,
    SourceArgs, Value,
};
use crate::collation::{Weights, weights};
use crate::error::{Error, ErrorKind, Result};

/// Inlines replacements into `rules`, and returns the replacements the program has once they
/// are defined, if they change.
///
/// The base rules get their own replacements. Rules added later get the previous ones, and their
/// own apply to every rule that references what they replace, added to `rules` when defined
/// before.
pub fn inline_replacements(
    program: &mut Program,
    rules: &mut Vec<(NameId, NodeId)>,
    uses: &[(NameId, NameId)],
    overlay: bool,
) -> Result<Option<Replacements>> {
    if !overlay {
        let replacements = rule_replacements(program, rules)?;
        let affected = users(uses, &replacements);
        inline(program, rules, &affected, &replacements);
        return Ok(Some(replacements));
    }
    let previous = std::mem::take(&mut program.replacements);
    let affected = users(uses, &previous);
    inline(program, rules, &affected, &previous);
    let added = rule_replacements(program, rules);
    program.replacements = previous;
    let added = added?;
    if added.is_empty() {
        return Ok(None);
    }
    let affected = rules_referencing(program, rules, &added);
    for &name in &affected {
        if !rules.iter().any(|&(n, _)| n == name)
            && let Some(node) = program.rule_node(name)
        {
            rules.push((name, node));
        }
    }
    inline(program, rules, &affected, &added);
    let mut merged = program.replacements.clone();
    for (replaced, mut list) in added {
        let previous = merged.entry(replaced).or_default();
        list.append(previous);
        sort_by_precedence(program, &mut list);
        *previous = list;
    }
    Ok(Some(merged))
}

/// The replacements `rules` define, by replaced rule, in the order they are tried.
fn rule_replacements(program: &Program, rules: &[(NameId, NodeId)]) -> Result<Replacements> {
    let mut replacements = Replacements::default();
    for &(_, node) in rules {
        let Node::Rule(rule) = program.node(node) else {
            continue;
        };
        for replacement in &rule.replacements {
            let replaced = program.target(replacement.replaced).ok_or_else(|| {
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
        candidates.reverse();
        sort_by_precedence(program, candidates);
    }
    Ok(replacements)
}

/// The rules among `uses` that reference a replaced rule.
fn users(uses: &[(NameId, NameId)], replacements: &Replacements) -> FxHashSet<NameId> {
    uses.iter()
        .filter(|(_, used)| replacements.contains_key(used))
        .map(|&(rule, _)| rule)
        .collect()
}

/// The rules, defined or among `rules`, whose tree references a replaced rule.
fn rules_referencing(
    program: &Program,
    rules: &[(NameId, NodeId)],
    replacements: &Replacements,
) -> FxHashSet<NameId> {
    program
        .names
        .ids()
        .filter_map(|name| {
            let node = rules
                .iter()
                .find(|&&(n, _)| n == name)
                .map(|&(_, node)| node)
                .or_else(|| program.rule_node(name))?;
            references_any(program, node, replacements).then_some(name)
        })
        .collect()
}

fn references_any(program: &Program, root: NodeId, replacements: &Replacements) -> bool {
    let mut visited = FxHashSet::default();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        match program.node(id) {
            Node::Reference(reference) => {
                if reference
                    .target
                    .is_some_and(|target| replacements.contains_key(&target))
                {
                    return true;
                }
            }
            node => stack.extend(node.children()),
        }
    }
    false
}

fn inline(
    program: &mut Program,
    rules: &mut [(NameId, NodeId)],
    affected: &FxHashSet<NameId>,
    replacements: &Replacements,
) {
    if affected.is_empty() {
        return;
    }
    let mut inliner = Inliner {
        program,
        replacements,
    };
    for (name, node) in rules.iter_mut() {
        if affected.contains(name) {
            *node = inliner.inline(*node);
        }
    }
}

fn name_of(program: &Program, reference: NodeId) -> &str {
    program
        .target(reference)
        .map_or("", |name| program.names.name(name))
}

/// The replacements of `replaced` that apply to a reference written in `context`, in the
/// order they are tried.
pub fn applicable_replacements<'a>(
    program: &Program,
    replacements: &'a Replacements,
    replaced: NameId,
    context: NameId,
) -> Vec<&'a Replacement> {
    let Some(candidates) = replacements.get(&replaced) else {
        return Vec::new();
    };
    let context_name = program.names.name(context);
    candidates
        .iter()
        .filter(|r| program.target(r.definition) != Some(context))
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

/// Orders replacements as they are tried: by decreasing priority, then by decreasing name of
/// the rule defining them, keeping their order among equals. The last defined come first, and
/// filtering keeps this order, so it is computed once.
fn sort_by_precedence(program: &Program, replacements: &mut [Replacement]) {
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
    replacements: &'a Replacements,
}

impl Inliner<'_> {
    /// Copies the tree under `id` with replacements inlined, once for each place it appears.
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
                self.program.push_copy(id, copy)
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
        let source: Rc<[NodeId]> = applicable
            .iter()
            .flat_map(|r| r.references())
            .chain([id])
            .collect();
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
        self.program
            .source_args
            .insert(node, SourceArgs::Replacement(source));
        self.program.replacement_nodes.insert(key, node);
        node
    }

    fn constant(&mut self, value: Value) -> NodeId {
        let constant = match value {
            Value::Null => Constant::not_applicable(),
            value => Constant::default_of(value),
        };
        self.program.push(Node::Constant(constant))
    }
}
