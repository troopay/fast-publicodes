//! The public engine: rules, situation and evaluation.

mod rules;
mod situation;

use std::cell::RefCell;
use std::rc::Rc;

use rustc_hash::FxHashMap;
use serde_json::Value as Json;

pub use situation::{SituationError, SituationOptions};

use crate::ast::{NameId, Node, NodeId, Program, Replacement, SourceArgs, Value};
use crate::date::Today;
use crate::error::{Error, ErrorKind, Result, Warning, WarningKind};
use crate::evaluate::{EvalState, Evaluated, Evaluator, Missing};
use crate::expression::is_reference;
use crate::options::Options;
use crate::parse::{Parser, Templates};
use crate::raw::Raw;
use crate::replace::applicable_replacements;
use crate::resolve::{disambiguate, finalize};
use crate::units::{UnitKey, UnitRef};

/// The result of `evaluate`: its value and unit, and its index in `Engine::evaluations`.
#[derive(Debug, Clone)]
pub struct Evaluation {
    pub value: Value,
    pub unit: Option<UnitRef>,
    pub index: u32,
}

/// The expressions evaluated since the cache was reset, which stay readable after it is.
#[derive(Clone, Default)]
pub struct Evaluations(Rc<RefCell<EvaluationList>>);

#[derive(Clone, Default)]
struct EvaluationList {
    evaluated: Vec<Evaluated>,
    /// The index of each string expression evaluated.
    strings: FxHashMap<String, u32>,
}

impl Evaluations {
    pub fn missing(&self, index: u32) -> Option<Missing> {
        let list = self.0.borrow();
        list.evaluated
            .get(index as usize)
            .map(|evaluated| evaluated.missing.clone())
    }

    fn push(&self, evaluated: Evaluated) -> u32 {
        let mut list = self.0.borrow_mut();
        list.evaluated.push(evaluated);
        list.evaluated.len() as u32 - 1
    }
}

/// What an expression made of a single reference evaluates.
#[derive(Debug, Clone, Copy)]
enum Reference {
    Rule(NameId),
    /// The node replacements turn the reference into, shared with the base rules.
    Replaced(NodeId),
}

pub struct Engine {
    program: Program,
    templates: Templates,
    /// Number of nodes of the base rules; later nodes belong to the situation.
    base_len: usize,
    /// The situation the rules include, in the order it was set.
    situation: Vec<(String, Raw)>,
    /// The names `getSituation` lists.
    public_situation: Vec<String>,
    state: EvalState,
    expressions: Evaluations,
    /// What each expression evaluated so far references directly, if anything.
    referenced_rules: FxHashMap<Box<str>, Option<Reference>>,
}

/// Why the rules could not be loaded, with the warnings raised before.
#[derive(Debug)]
pub struct Failure {
    pub error: Error,
    pub warnings: Vec<Warning>,
}

/// Parses, resolves and defines `rules`.
fn load(program: &mut Program, rules: &Raw) -> Result<Templates> {
    let templates = Templates::new(program)?;
    let mut parser = Parser::new(program, &templates, "");
    parser.parse_rules(rules)?;
    let batch = parser.batch;
    let order = batch
        .rules
        .iter()
        .filter_map(|&(name, node)| node.map(|_| name))
        .collect();
    finalize(program, batch, false)?;
    program.rule_order = order;
    inherit_experimental(program);
    program.freeze();
    Ok(templates)
}

impl Engine {
    pub fn new(
        rules: &Json,
        options: Options,
        today: Today,
        unit_key: Option<UnitKey>,
    ) -> std::result::Result<Self, Failure> {
        if !rules.is_object() {
            return Err(Failure {
                error: Error::new(
                    ErrorKind::Engine,
                    "Publicodes does not parse yaml rule sets itself anymore. Please provide a parsed js object. E.g. the `eemeli/yaml` package.",
                ),
                warnings: Vec::new(),
            });
        }
        let mut program = Program::new(options, today, unit_key);
        let templates = match load(&mut program, &Raw::from(rules)) {
            Ok(templates) => templates,
            Err(error) => {
                let warnings = std::mem::take(&mut *program.warnings.borrow_mut());
                return Err(Failure { error, warnings });
            }
        };
        Ok(Self {
            base_len: program.len(),
            program,
            templates,
            situation: Vec::new(),
            public_situation: Vec::new(),
            state: EvalState::default(),
            expressions: Evaluations::default(),
            referenced_rules: FxHashMap::default(),
        })
    }

    /// `shallowCopy`: an engine with the same rules and situation, and this cache if
    /// `copy_cache`, but no inversion failure.
    pub fn shallow_copy(&self, copy_cache: bool) -> Engine {
        let (mut state, expressions) = if copy_cache {
            let list = self.expressions.0.borrow().clone();
            (self.state.clone(), Evaluations(Rc::new(RefCell::new(list))))
        } else {
            (self.state.fork(false), Evaluations::default())
        };
        state.forget_inversion_fail();
        let copy = Engine {
            program: self.program.clone(),
            templates: self.templates.clone(),
            base_len: self.base_len,
            situation: self.situation.clone(),
            public_situation: self.public_situation.clone(),
            state,
            expressions,
            referenced_rules: self.referenced_rules.clone(),
        };
        copy.program.warnings.borrow_mut().clear();
        copy
    }

    pub fn reset_cache(&mut self) {
        self.state.clear();
        self.expressions = Evaluations::default();
    }

    /// The expressions evaluated since the cache was last reset.
    pub fn evaluations(&self) -> Evaluations {
        self.expressions.clone()
    }

    /// The warnings raised since the last call.
    pub fn take_warnings(&self) -> Vec<Warning> {
        std::mem::take(&mut *self.program.warnings.borrow_mut())
    }

    /// Evaluates a publicodes expression: a string, a number, or an object written as a rule,
    /// which may hold suggestions. Strings evaluated since the cache was reset are not evaluated
    /// again.
    pub fn evaluate(&mut self, value: &Json) -> Result<Evaluation> {
        let value = self.with_suggestions(&Raw::from(value))?;
        let (evaluated, index) = self.evaluated(&value)?;
        Ok(Evaluation {
            value: evaluated.value,
            unit: evaluated.unit.get().cloned(),
            index,
        })
    }

    /// `value` with each suggestion, written `{ "\u{0}": { rule, name } }`, replaced by its node;
    /// a suggestion alone becomes `{ valeur: node }`, as `publicodes` wraps a node.
    fn with_suggestions(&self, value: &Raw) -> Result<Raw> {
        Ok(match self.suggestion(value)? {
            Some(node) => Raw::map([("valeur", Raw::Node(node))]),
            None => self.nested_suggestions(value)?,
        })
    }

    fn nested_suggestions(&self, value: &Raw) -> Result<Raw> {
        if let Some(node) = self.suggestion(value)? {
            return Ok(Raw::Node(node));
        }
        Ok(match value {
            Raw::List(items) => Raw::list(
                items
                    .iter()
                    .map(|item| self.nested_suggestions(item))
                    .collect::<Result<_>>()?,
            ),
            Raw::Map(entries) => Raw::map(
                entries
                    .iter()
                    .map(|(key, item)| Ok((key.clone(), self.nested_suggestions(item)?)))
                    .collect::<Result<Vec<_>>>()?,
            ),
            other => other.clone(),
        })
    }

    fn suggestion(&self, value: &Raw) -> Result<Option<NodeId>> {
        let Raw::Map(entries) = value else {
            return Ok(None);
        };
        let [(key, reference)] = entries.as_slice() else {
            return Ok(None);
        };
        let (Some(rule), Some(name)) = (
            reference.get("rule").and_then(Raw::as_str),
            reference.get("name").and_then(Raw::as_str),
        ) else {
            return Ok(None);
        };
        if &**key != "\u{0}" {
            return Ok(None);
        }
        self.public_rule(rule)?
            .1
            .suggestions
            .iter()
            .find(|(n, _)| &**n == name)
            .map(|&(_, node)| Some(node))
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::Internal,
                    format!("La règle {rule} n'a pas de suggestion {name}"),
                )
            })
    }

    fn evaluated(&mut self, value: &Raw) -> Result<(Evaluated, u32)> {
        let Raw::String(expression) = value else {
            let evaluated = self.evaluate_value(value)?;
            let index = self.expressions.push(evaluated.clone());
            return Ok((evaluated, index));
        };
        let cached = self
            .expressions
            .0
            .borrow()
            .strings
            .get(&**expression)
            .copied();
        if let Some(index) = cached {
            let list = self.expressions.0.borrow();
            return Ok((list.evaluated[index as usize].clone(), index));
        }
        let evaluated = self.evaluate_value(value)?;
        let index = self.expressions.push(evaluated.clone());
        self.expressions
            .0
            .borrow_mut()
            .strings
            .insert(expression.to_string(), index);
        Ok((evaluated, index))
    }

    /// Whether an `inversion numérique` failed since the situation was set.
    pub fn inversion_fail(&self) -> bool {
        self.state.inversion_fail()
    }

    /// The missing variables of an evaluation with their scores.
    pub fn missing_variables<'a>(
        &'a self,
        missing: &'a Missing,
    ) -> impl Iterator<Item = (&'a str, f64)> {
        missing
            .iter()
            .map(|&(name, score)| (self.program.names.name(name), score))
    }

    fn load_overlay(&mut self, rules: &Raw) -> Result<()> {
        let len = self.program.len();
        let mut parser = Parser::new(&mut self.program, &self.templates, "");
        parser.source_args = true;
        let parsed = parser.parse_rules(rules);
        let batch = parser.batch;
        let result = parsed.and_then(|()| finalize(&mut self.program, batch, true));
        if result.is_err() {
            self.program.truncate(len);
        }
        result
    }

    fn evaluate_value(&mut self, value: &Raw) -> Result<Evaluated> {
        let node = match value {
            Raw::String(expression) => self.referenced(expression),
            _ => None,
        };
        let node = match node {
            Some(Reference::Rule(name)) => {
                self.warn_if_experimental(name);
                self.program
                    .rule_node(name)
                    .ok_or_else(unknown_evaluation)?
            }
            Some(Reference::Replaced(node)) => {
                self.warn_experimental_references(node)?;
                node
            }
            None => {
                let rules = Raw::map([("[privé] $EVALUATION", value.clone())]);
                self.load_overlay(&rules)?;
                let name = self.program.names.intern("$EVALUATION");
                let rule = self
                    .program
                    .rule_node(name)
                    .ok_or_else(unknown_evaluation)?;
                self.warn_experimental_references(rule)?;
                match self.program.node(rule) {
                    Node::Rule(rule) => rule.valeur,
                    _ => return Err(unknown_evaluation()),
                }
            }
        };
        self.state.reset_stacks();
        Evaluator::new(&self.program, &mut self.state).evaluate(node)
    }

    fn referenced(&mut self, expression: &str) -> Option<Reference> {
        if self.program.has_overlay_replacements() {
            return None;
        }
        if let Some(reference) = self.referenced_rules.get(expression) {
            return *reference;
        }
        let reference = self.resolve_reference(expression);
        self.referenced_rules.insert(expression.into(), reference);
        reference
    }

    /// What an expression made of a single reference evaluates; only the base rules decide it,
    /// so the result never changes.
    fn resolve_reference(&mut self, expression: &str) -> Option<Reference> {
        if !is_reference(expression) {
            return None;
        }
        let id = disambiguate(&self.program, "$EVALUATION", expression).ok()?;
        if self.program.names.name(id) == "$EVALUATION" {
            return None;
        }
        if !self.program.replacements.contains_key(&id) {
            return Some(Reference::Rule(id));
        }
        let context = self.program.names.intern("$EVALUATION");
        let applicable =
            applicable_replacements(&self.program, &self.program.replacements, id, context);
        if applicable.is_empty() {
            return Some(Reference::Rule(id));
        }
        let key: Vec<u32> = applicable.iter().map(|r| r.id).collect();
        let node = *self.program.replacement_nodes.get(&key)?;
        ((node.0 as usize) < self.base_len).then_some(Reference::Replaced(node))
    }

    fn warn_if_experimental(&self, name: NameId) {
        if self
            .program
            .rule(name)
            .is_some_and(|rule| rule.experimental)
        {
            self.program.warn(
                WarningKind::ExperimentalRules,
                "Cette règle est tagguée comme experimentale. \n\nCela veut dire qu'elle peut être modifiée, renommée, ou supprimée sans qu'il n'y ait de changement de version majeure dans l'API.\n".to_owned(),
                Some(self.program.names.name(name).to_owned()),
                None,
            );
        }
    }

    /// `checkExperimentalRule`: warns about each reference to an experimental rule in the tree
    /// under `root`, as often and in the order `publicodes` visits them.
    fn warn_experimental_references(&self, root: NodeId) -> Result<()> {
        if self.program.options.warn.experimental_rules {
            self.visit_experimental(root)?;
        }
        Ok(())
    }

    fn visit_experimental(&self, id: NodeId) -> Result<()> {
        let program = &self.program;
        let node = program.node(id);
        if let Node::Reference(reference) = node {
            if let Some(target) = reference.target {
                if program.rule(target).is_none() {
                    let name = program.names.name(target);
                    return Err(Error::new(
                        ErrorKind::Internal,
                        format!("La règle \"{name}\" n'existe pas"),
                    )
                    .in_rule(name));
                }
                self.warn_if_experimental(target);
            }
            return Ok(());
        }
        match program.source_args.get(&id) {
            Some(SourceArgs::Inline(paths)) => {
                for path in paths.iter() {
                    let arg = path.iter().fold(id, |node, &index| {
                        program.node(node).children()[index as usize]
                    });
                    self.visit_experimental(arg)?;
                }
            }
            Some(SourceArgs::Replacement(references)) => {
                for &reference in references.iter() {
                    self.visit_experimental(reference)?;
                }
            }
            None => {}
        }
        let children = match node {
            Node::Rule(rule) => rule
                .suggestions
                .iter()
                .map(|&(_, suggestion)| suggestion)
                .chain(rule.possibilities)
                .chain(rule.replacements.iter().flat_map(Replacement::references))
                .chain(rule.parents.iter().copied())
                .chain([rule.valeur])
                .collect(),
            node => node.children(),
        };
        for child in children {
            self.visit_experimental(child)?;
        }
        Ok(())
    }
}

/// Marks as experimental every rule with an experimental ancestor.
fn inherit_experimental(program: &mut Program) {
    let prefixes: Vec<String> = program
        .rule_order
        .iter()
        .filter(|&&name| program.rule(name).is_some_and(|rule| rule.experimental))
        .map(|&name| format!("{} . ", program.names.name(name)))
        .collect();
    if prefixes.is_empty() {
        return;
    }
    let inheriting: Vec<NodeId> = program
        .rule_order
        .iter()
        .filter(|&&name| {
            let dotted = program.names.name(name);
            prefixes
                .iter()
                .any(|prefix| dotted.starts_with(prefix.as_str()))
        })
        .filter_map(|&name| program.rule_node(name))
        .collect();
    for node in inheriting {
        if let Node::Rule(rule) = program.node_mut(node) {
            rule.experimental = true;
        }
    }
}

fn unknown_evaluation() -> Error {
    Error::new(
        ErrorKind::Internal,
        "L'expression évaluée n'a pas été définie",
    )
}
