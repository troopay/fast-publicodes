//! The public engine: rules, situation and evaluation.

use rustc_hash::FxHashMap;

use serde_json::Value as Json;

use crate::ast::{NameId, Node, NodeId, PossibilityKind, Program, Value};
use crate::error::{Error, ErrorKind, Result};
use crate::evaluate::{EvalState, Evaluated, Evaluator, Missing};
use crate::expression::is_reference;
use crate::js::number_to_string;
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

pub struct Engine {
    program: Program,
    templates: Templates,
    /// Number of nodes of the base rules; later nodes belong to the situation.
    base_len: usize,
    situation: Vec<(String, Raw)>,
    state: EvalState,
    expressions: FxHashMap<String, Evaluated>,
    /// What each expression evaluated so far references directly, if anything.
    referenced_rules: FxHashMap<Box<str>, Option<Reference>>,
}

impl Engine {
    /// `today` is the date `durée` defaults to, as `dd/mm/yyyy`.
    pub fn new(rules: &Json, today: &str) -> Result<Self> {
        if !rules.is_object() {
            return Err(Error::new(
                ErrorKind::Engine,
                "Publicodes does not parse yaml rule sets itself anymore. Please provide a parsed js object.",
            ));
        }
        let mut program = Program::default();
        program.today = today.to_owned();
        let templates = Templates::new(&mut program)?;
        let mut parser = Parser::new(&mut program, &templates, "");
        parser.parse_rules(&Raw::from(rules))?;
        let batch = parser.batch;
        finalize(&mut program, batch, false)?;
        Ok(Self {
            base_len: program.nodes.len(),
            program,
            templates,
            situation: Vec::new(),
            state: EvalState::default(),
            expressions: FxHashMap::default(),
            referenced_rules: FxHashMap::default(),
        })
    }

    pub fn set_situation(&mut self, situation: &Json) -> Result<()> {
        self.state.clear();
        self.expressions.clear();
        let entries: Vec<(String, Raw)> = match situation {
            Json::Object(map) => map.iter().map(|(k, v)| (k.clone(), Raw::from(v))).collect(),
            _ => Vec::new(),
        };
        for (name, _) in &entries {
            self.check_situation_rule(name)?;
        }
        let previous = std::mem::take(&mut self.situation);
        match self.load_situation(&entries) {
            Ok(()) => {
                self.situation = entries;
                self.evaluate_possible_values()
            }
            Err(error) => {
                self.load_situation(&previous)
                    .expect("the previous situation loaded before");
                self.situation = previous;
                Err(Error {
                    kind: ErrorKind::Situation,
                    ..error
                })
            }
        }
    }

    /// Evaluates the values given to rules with possibilities, which warms the cache for the
    /// situation.
    fn evaluate_possible_values(&mut self) -> Result<()> {
        let entries = self.situation.clone();
        for (name, value) in &entries {
            let Some(possibilities) = self
                .program
                .names
                .get(name)
                .and_then(|id| self.program.rule(id))
                .and_then(|rule| rule.possibilities)
            else {
                continue;
            };
            let expression = match value {
                Raw::String(text) => text.to_string(),
                Raw::Number(n) => number_to_string(*n),
                _ => continue,
            };
            let evaluation = self.evaluate(&expression)?;
            let is_reference = matches!(
                self.program.node(possibilities),
                Node::UnePossibilite {
                    kind: PossibilityKind::Reference,
                    ..
                }
            );
            if is_reference && !matches!(evaluation.value, Value::Null | Value::Undefined) {
                Evaluator::new(&self.program, &mut self.state).evaluate(possibilities)?;
            }
        }
        Ok(())
    }

    fn check_situation_rule(&self, name: &str) -> Result<()> {
        let situation_error =
            |message: String| Error::new(ErrorKind::Situation, message).in_rule(name);
        let rule = self
            .program
            .names
            .get(name)
            .and_then(|id| self.program.rule(id))
            .ok_or_else(|| {
                situation_error(format!("'{name}' n'existe pas dans la base de règle."))
            })?;
        if rule.private {
            return Err(situation_error(format!(
                "La règle {name} est une règle privée."
            )));
        }
        Ok(())
    }

    fn load_situation(&mut self, entries: &[(String, Raw)]) -> Result<()> {
        self.program.reset_overlay(self.base_len);
        let rules = Raw::map(
            entries
                .iter()
                .map(|(name, value)| (format!("[privé] {name} . $SITUATION"), value.clone())),
        );
        self.load_overlay(&rules)
    }

    fn load_overlay(&mut self, rules: &Raw) -> Result<()> {
        let len = self.program.nodes.len();
        let mut parser = Parser::new(&mut self.program, &self.templates, "");
        let parsed = parser.parse_rules(rules);
        let batch = parser.batch;
        let result = parsed.and_then(|()| finalize(&mut self.program, batch, true));
        if result.is_err() {
            self.program.truncate(len);
        }
        result
    }

    pub fn evaluate(&mut self, expression: &str) -> Result<Evaluation> {
        let evaluated = match self.expressions.get(expression) {
            Some(evaluated) => evaluated.clone(),
            None => {
                let evaluated = self.evaluate_expression(expression)?;
                self.expressions
                    .insert(expression.to_owned(), evaluated.clone());
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

    fn evaluate_expression(&mut self, expression: &str) -> Result<Evaluated> {
        if let Some(rule) = self.referenced_rule(expression) {
            self.state.reset_stacks();
            return Evaluator::new(&self.program, &mut self.state).evaluate(rule);
        }
        let rules = Raw::map([("[privé] $EVALUATION", Raw::str(expression))]);
        self.load_overlay(&rules)?;
        let name = self.program.names.intern("$EVALUATION");
        let valeur = match self.program.rule_node(name).map(|id| self.program.node(id)) {
            Some(Node::Rule(rule)) => rule.valeur,
            _ => {
                return Err(Error::new(
                    ErrorKind::Internal,
                    "$EVALUATION n'a pas été défini",
                ));
            }
        };
        self.state.reset_stacks();
        Evaluator::new(&self.program, &mut self.state).evaluate(valeur)
    }

    /// The node `expression` evaluates to when it only references a rule; evaluating it
    /// directly skips parsing the expression.
    fn referenced_rule(&mut self, expression: &str) -> Option<NodeId> {
        let target = match self.referenced_rules.get(expression) {
            Some(target) => *target,
            None => {
                let target = self.resolve_reference(expression);
                self.referenced_rules.insert(expression.into(), target);
                target
            }
        };
        match target? {
            Reference::Rule(name) => self.program.rule_node(name),
            Reference::Replaced(node) => Some(node),
        }
    }

    /// Only the base rules decide what an expression references, so the result never changes.
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
}
