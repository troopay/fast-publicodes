//! Reference resolution after parsing.
//!
//! Resolution copies every node except references and constants, so after it rules share only
//! those nodes.

use rustc_hash::FxHashMap;

use crate::ast::{NameId, Node, NodeId, Program, rule_parents};
use crate::error::{Error, Result};
use crate::parse::Batch;
use crate::replace::inline_replacements;
use crate::sub_engines::prepare_sub_engines;
use crate::types::{infer_rule_types, infer_types};

/// The rules references resolve against: the defined ones, shadowed by those being added.
struct Scope<'a> {
    program: &'a Program,
    batch: &'a FxHashMap<NameId, NodeId>,
}

impl Scope<'_> {
    fn rule_private(&self, name: NameId) -> Option<bool> {
        let node = self
            .batch
            .get(&name)
            .copied()
            .or_else(|| self.program.rule_node(name))?;
        match self.program.node(node) {
            Node::Rule(rule) => Some(rule.private),
            _ => None,
        }
    }

    fn named_rule_private(&self, name: &str) -> Option<bool> {
        self.rule_private(self.program.names.get(name)?)
    }

    fn is_rule(&self, name: &str) -> bool {
        self.named_rule_private(name).is_some()
    }

    fn is_accessible(&self, context: &str, name: &str) -> bool {
        let common = common_ancestor(context, name);
        // `name`, its ancestors nearest first, then the root.
        let mut parents = vec![name];
        parents.extend(rule_parents(name));
        parents.push("");
        let end = parents
            .iter()
            .position(|p| *p == common)
            .map_or(0, |index| index.saturating_sub(1));
        parents[..end]
            .iter()
            .all(|p| self.named_rule_private(p) != Some(true))
    }

    fn disambiguate(&self, from: &str, written: &str) -> Result<NameId> {
        let mut contexts: Vec<&str> = rule_parents(from);
        contexts.push(from);
        let mut partial = written;
        if partial.starts_with("^ . ") {
            let mut count = 0;
            while let Some(rest) = partial.strip_prefix("^ . ") {
                partial = rest;
                count += 1;
            }
            contexts.truncate(contexts.len().saturating_sub(count));
        }
        let root = contexts.pop().unwrap_or("");
        contexts.insert(0, root);
        contexts.push("");
        let mut candidate = String::with_capacity(from.len() + partial.len() + 3);
        for context in &contexts {
            candidate.clear();
            if !context.is_empty() {
                candidate.push_str(context);
                candidate.push_str(" . ");
            }
            candidate.push_str(partial);
            if let Some(id) = self.program.names.get(&candidate)
                && self.rule_private(id).is_some()
                && candidate != from
                && self.is_accessible(from, &candidate)
            {
                return Ok(id);
            }
        }
        if from.ends_with(partial)
            && let Some(id) = self.program.names.get(from)
        {
            return Ok(id);
        }
        let rule = from.strip_suffix(" . $SITUATION").unwrap_or(from);
        let existing = contexts
            .iter()
            .map(|c| {
                if c.is_empty() {
                    partial.to_owned()
                } else {
                    format!("{c} . {partial}")
                }
            })
            .find(|n| self.is_rule(n));
        match existing {
            None => Err(Error::syntax(format!(
                "La référence \"{partial}\" est introuvable.\nVérifiez que l'orthographe et l'espace de nom sont corrects"
            ))
            .in_rule(rule)),
            Some(name) => Err(Error::syntax(format!(
                "La règle \"{name}\" n'est pas accessible depuis \"{from}\".\nCela vient du fait qu'elle est privée ou qu'un de ses parent est privé"
            ))
            .in_rule(rule)),
        }
    }
}

struct Resolver<'a> {
    program: &'a mut Program,
    batch: FxHashMap<NameId, NodeId>,
    /// `(rule, referenced rule)` for every reference resolved outside replacements.
    uses: Vec<(NameId, NameId)>,
}

impl Resolver<'_> {
    fn scope(&self) -> Scope<'_> {
        Scope {
            program: self.program,
            batch: &self.batch,
        }
    }

    fn resolve_reference(&mut self, id: NodeId) -> Result<Option<NameId>> {
        let Node::Reference(reference) = self.program.node(id) else {
            return Ok(None);
        };
        if let Some(target) = reference.target {
            return Ok(Some(target));
        }
        let scope = self.scope();
        let target =
            scope.disambiguate(scope.program.names.name(reference.context), &reference.name)?;
        if let Node::Reference(r) = self.program.node_mut(id) {
            r.target = Some(target);
        }
        Ok(Some(target))
    }

    fn resolve(&mut self, id: NodeId) -> Result<NodeId> {
        match self.program.node(id) {
            Node::Reference(reference) => {
                let context = reference.context;
                if let Some(target) = self.resolve_reference(id)? {
                    self.uses.push((context, target));
                }
                Ok(id)
            }
            Node::Constant(_) => Ok(id),
            node => {
                if let Node::Rule(rule) = node {
                    let references: Vec<NodeId> = rule
                        .replacements
                        .iter()
                        .flat_map(|r| r.references())
                        .collect();
                    for reference in references {
                        self.resolve_reference(reference)?;
                    }
                }
                let children = self.program.node(id).children();
                let resolved = children
                    .into_iter()
                    .map(|child| self.resolve(child))
                    .collect::<Result<Vec<_>>>()?;
                let copy = self.program.node(id).with_children(resolved);
                Ok(self.program.push(copy))
            }
        }
    }
}

/// The segments `a` starts with that `b` also starts with.
fn common_ancestor<'a>(a: &'a str, b: &str) -> &'a str {
    let mut b_parts = b.split(" . ");
    let mut end = None;
    let mut offset = 0;
    for part in a.split(" . ") {
        if b_parts.next() != Some(part) {
            return &a[..end.unwrap_or(0)];
        }
        offset += part.len();
        end = Some(offset);
        offset += 3;
    }
    a
}

/// The rule a reference written `written` in rule `from` points to.
pub fn disambiguate(program: &Program, from: &str, written: &str) -> Result<NameId> {
    Scope {
        program,
        batch: &FxHashMap::default(),
    }
    .disambiguate(from, written)
}

/// Resolves the rules of `batch`, defines them, and infers their types.
pub fn finalize(program: &mut Program, batch: Batch, overlay: bool) -> Result<()> {
    let rules: Vec<(NameId, NodeId)> = batch
        .rules
        .into_iter()
        .filter_map(|(name, node)| Some((name, node?)))
        .collect();
    let mut resolver = Resolver {
        batch: rules.iter().copied().collect(),
        program,
        uses: Vec::new(),
    };
    let mut resolved = Vec::with_capacity(rules.len());
    for &(name, draft) in &rules {
        let dotted = resolver.program.names.name(name).to_owned();
        if let Some(parent) = rule_parents(&dotted)
            .into_iter()
            .find(|p| !resolver.scope().is_rule(p))
        {
            return Err(
                Error::syntax(format!("La règle parente \"{parent}\" n'existe pas"))
                    .in_rule(&dotted),
            );
        }
        let node = resolver.resolve(draft)?;
        let reports_missing = match resolver.program.node(node) {
            Node::Rule(rule) => !rule.private && resolver.scope().is_accessible("", &dotted),
            _ => false,
        };
        if let Node::Rule(rule) = resolver.program.node_mut(node) {
            rule.reports_missing = reports_missing;
        }
        resolved.push((name, node));
    }
    let uses = std::mem::take(&mut resolver.uses);
    inline_replacements(program, &mut resolved, &uses, overlay)?;
    for &(name, node) in &resolved {
        program.define(name, node, overlay);
    }
    infer_rule_types(program, resolved.iter().map(|&(_, node)| node));
    let prepared = prepare_sub_engines(program, resolved.iter().map(|&(_, node)| node));
    infer_types(program, prepared);
    Ok(())
}
