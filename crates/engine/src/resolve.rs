//! Reference resolution after parsing.
//!
//! Resolution copies every node except references and constants, so after it rules share only
//! those nodes.

use std::collections::HashMap;

use crate::ast::{NameId, Node, NodeId, Program, rule_parents};
use crate::error::{Error, Result};
use crate::parse::Batch;
use crate::types::infer_rule_types;

/// The rules references resolve against: the defined ones, shadowed by those being added.
struct Scope<'a> {
    program: &'a Program,
    batch: &'a HashMap<NameId, NodeId>,
}

impl Scope<'_> {
    fn rule_private(&self, name: &str) -> Option<bool> {
        let id = self.program.names.get(name)?;
        let node = self
            .batch
            .get(&id)
            .copied()
            .or_else(|| self.program.rule_node(id))?;
        match self.program.node(node) {
            Node::Rule(rule) => Some(rule.private),
            _ => None,
        }
    }

    fn is_rule(&self, name: &str) -> bool {
        self.rule_private(name).is_some()
    }

    fn is_accessible(&self, context: &str, name: &str) -> bool {
        let common = common_ancestor(context, name);
        let mut parents = vec![name];
        parents.extend(rule_parents(name));
        parents.push("");
        let end = parents
            .iter()
            .position(|p| *p == common)
            .map_or(0, |index| index.saturating_sub(1));
        parents[..end]
            .iter()
            .all(|p| self.rule_private(p) != Some(true))
    }

    fn disambiguate(&self, from: &str, written: &str) -> Result<String> {
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
        let candidate = |context: &str| {
            if context.is_empty() {
                partial.to_owned()
            } else {
                format!("{context} . {partial}")
            }
        };
        for context in &contexts {
            let name = candidate(context);
            if self.is_rule(&name) && name != from && self.is_accessible(from, &name) {
                return Ok(name);
            }
        }
        if from.ends_with(partial) {
            return Ok(from.to_owned());
        }
        let rule = from.strip_suffix(" . $SITUATION").unwrap_or(from);
        match contexts.iter().map(|c| candidate(c)).find(|n| self.is_rule(n)) {
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
    batch: HashMap<NameId, NodeId>,
}

impl Resolver<'_> {
    fn scope(&self) -> Scope<'_> {
        Scope {
            program: self.program,
            batch: &self.batch,
        }
    }

    fn resolve(&mut self, id: NodeId) -> Result<NodeId> {
        let node = self.program.node(id).clone();
        match node {
            Node::Reference(reference) => {
                if reference.target.is_none() {
                    let from = self.program.names.name(reference.context).to_owned();
                    let target = self.scope().disambiguate(&from, &reference.name)?;
                    let target = self.program.names.intern(&target);
                    if let Node::Reference(r) = self.program.node_mut(id) {
                        r.target = Some(target);
                    }
                }
                Ok(id)
            }
            Node::Constant(_) => Ok(id),
            other => {
                let copy = other.map_children(|child| self.resolve(child))?;
                Ok(self.program.push(copy))
            }
        }
    }
}

fn common_ancestor(a: &str, b: &str) -> String {
    let a_parts: Vec<&str> = a.split(" . ").collect();
    let b_parts: Vec<&str> = b.split(" . ").collect();
    match a_parts
        .iter()
        .enumerate()
        .position(|(i, part)| b_parts.get(i) != Some(part))
    {
        None => a.to_owned(),
        Some(index) => a_parts[..index].join(" . "),
    }
}

/// The rule a reference written `written` in rule `from` points to.
pub fn disambiguate(program: &Program, from: &str, written: &str) -> Result<String> {
    Scope {
        program,
        batch: &HashMap::new(),
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
    for &(name, node) in &resolved {
        program.define(name, node, overlay);
    }
    infer_rule_types(program, resolved.iter().map(|&(_, node)| node));
    Ok(())
}
