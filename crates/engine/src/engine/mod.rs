//! The public engine: rules, situation and evaluation.

mod rules;
mod situation;

use rustc_hash::FxHashMap;
use serde_json::Value as Json;

pub use situation::{SituationError, SituationOptions};

use crate::ast::{NameId, Node, NodeId, Program, Replacement, SourceArgs, Value};
use crate::error::{Error, ErrorKind, Result, Warning, WarningKind};
use crate::evaluate::{EvalState, Evaluated, Evaluator, Missing};
use crate::expression::is_reference;
use crate::options::Options;
use crate::parse::{Parser, Templates};
use crate::raw::Raw;
use crate::replace::applicable_replacements;
use crate::resolve::{disambiguate, finalize};
use crate::units::Unit;

/// The result of `evaluate`; `Engine::missing_variables` names its missing variables.
#[derive(Debug, Clone)]
pub struct Evaluation {
    pub value: Value,
    pub unit: Option<Unit>,
    pub missing: Missing,
}

/// What an expression made of a single reference evaluates.
#[derive(Debug, Clone, Copy)]
enum Reference {
    Rule(NameId),
    /// The node replacements turn the reference into, shared with the base rules.
    Replaced(NodeId),
}

#[derive(Clone)]
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
    expressions: FxHashMap<String, Evaluated>,
    /// What each expression evaluated so far references directly, if anything.
    referenced_rules: FxHashMap<Box<str>, Option<Reference>>,
}

impl Engine {
    /// `today` is the date `durée` defaults to, as `dd/mm/yyyy`.
    pub fn new(rules: &Json, today: &str, options: Options) -> Result<Self> {
        if !rules.is_object() {
            return Err(Error::new(
                ErrorKind::Engine,
                "Publicodes does not parse yaml rule sets itself anymore. Please provide a parsed js object. E.g. the `eemeli/yaml` package.",
            ));
        }
        let mut program = Program::default();
        program.options = options;
        program.today = today.to_owned();
        let templates = Templates::new(&mut program)?;
        let mut parser = Parser::new(&mut program, &templates, "");
        parser.parse_rules(&Raw::from(rules))?;
        let batch = parser.batch;
        let order = batch
            .rules
            .iter()
            .filter_map(|&(name, node)| node.map(|_| name))
            .collect();
        finalize(&mut program, batch, false)?;
        program.rule_order = order;
        inherit_experimental(&mut program);
        Ok(Self {
            base_len: program.nodes.len(),
            program,
            templates,
            situation: Vec::new(),
            public_situation: Vec::new(),
            state: EvalState::default(),
            expressions: FxHashMap::default(),
            referenced_rules: FxHashMap::default(),
        })
    }

    /// `shallowCopy`: an engine with the same rules and situation, and this cache if
    /// `copy_cache`.
    pub fn shallow_copy(&self, copy_cache: bool) -> Engine {
        let mut copy = self.clone();
        copy.program.warnings.borrow_mut().clear();
        if !copy_cache {
            copy.reset_cache();
        }
        copy
    }

    pub fn reset_cache(&mut self) {
        self.state.clear();
        self.expressions.clear();
    }

    /// The warnings raised since the last call.
    pub fn take_warnings(&self) -> Vec<Warning> {
        std::mem::take(&mut *self.program.warnings.borrow_mut())
    }

    /// Evaluates a publicodes expression: a string, a number, or an object written as a rule.
    pub fn evaluate(&mut self, value: &Json) -> Result<Evaluation> {
        let json_key;
        let key = match value {
            Json::String(expression) => expression.as_str(),
            other => {
                json_key = format!("\u{0}{other}");
                json_key.as_str()
            }
        };
        let evaluated = match self.expressions.get(key) {
            Some(evaluated) => evaluated.clone(),
            None => {
                let evaluated = self.evaluate_value(value)?;
                self.expressions.insert(key.to_owned(), evaluated.clone());
                evaluated
            }
        };
        Ok(Evaluation {
            value: evaluated.value,
            unit: evaluated.unit.unit().cloned(),
            missing: evaluated.missing,
        })
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
        let len = self.program.nodes.len();
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

    fn evaluate_value(&mut self, value: &Json) -> Result<Evaluated> {
        let node = match value {
            Json::String(expression) => self.referenced(expression),
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
                let rules = Raw::map([("[privé] $EVALUATION", Raw::from(value))]);
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
                .copied()
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
