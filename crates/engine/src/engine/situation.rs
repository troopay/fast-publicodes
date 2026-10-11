//! `setSituation`.

use serde_json::Value as Json;

use super::Engine;
use crate::ast::{Node, PossibilityKind, Value};
use crate::error::{Error, ErrorKind, Result, WarningKind};
use crate::evaluate::{Evaluated, Evaluator, to_js_string};
use crate::raw::Raw;

#[derive(Debug, Clone, Copy, Default)]
pub struct SituationOptions {
    pub keep_previous: bool,
    /// Forces strict checks of the situation; otherwise the `strict` option decides.
    pub strict: bool,
}

/// A failed `setSituation`, which leaves the situation `getSituation` lists either as it was,
/// or, when `situation_reset`, as the previous one if kept and empty otherwise; then adds
/// `accepted`, as a successful one would.
#[derive(Debug)]
pub struct SituationError {
    pub error: Error,
    pub situation_reset: bool,
    pub accepted: Vec<String>,
}

impl Engine {
    /// Sets the situation; returns the names `getSituation` gains.
    pub fn set_situation(
        &mut self,
        situation: &Json,
        options: SituationOptions,
    ) -> std::result::Result<Vec<String>, SituationError> {
        self.reset_cache();
        let strict = options.strict || self.program.options.strict.situation;
        let entries: Vec<(String, Raw)> = match situation {
            Json::Object(map) => map.iter().map(|(k, v)| (k.clone(), Raw::from(v))).collect(),
            _ => Vec::new(),
        };
        let previous = self.situation.clone();
        let previous_public = self.public_situation.clone();
        let fail = |error, situation_reset| SituationError {
            error,
            situation_reset,
            accepted: Vec::new(),
        };

        let mut accepted = Vec::with_capacity(entries.len());
        for entry in entries {
            match self.check_situation_rule(&entry.0) {
                Ok(()) => accepted.push(entry),
                Err(error) if strict => return Err(fail(error, false)),
                Err(error) => self.warn_situation(error),
            }
        }

        if !options.keep_previous {
            self.situation.clear();
            self.public_situation.clear();
            self.state.evaluated_rule = None;
            self.program.reset_overlay(self.base_len);
        }
        if strict {
            if let Err(error) = self.load_situation_entries(&accepted) {
                if error.kind == ErrorKind::Thrown {
                    return Err(fail(error, true));
                }
                self.restore_situation(previous);
                return Err(fail(situation_error(error), true));
            }
        } else {
            let mut loaded = Vec::with_capacity(accepted.len());
            for entry in accepted {
                match self.load_situation_entries(std::slice::from_ref(&entry)) {
                    Ok(()) => loaded.push(entry),
                    Err(error) if error.kind == ErrorKind::Thrown => {
                        return Err(fail(error, true));
                    }
                    Err(error) => self.warn_situation(situation_error(error)),
                }
            }
            accepted = loaded;
        }

        let mut valid = Vec::with_capacity(accepted.len());
        for (name, value) in accepted {
            match self.check_possible_value(&name, &value) {
                Ok(true) => valid.push(name),
                Ok(false) => {
                    let message = format!(
                        "La valeur \"{}\" ne fait pas partie des possibilités applicables listées pour cette règle.",
                        to_js_string(&self.evaluate_raw(&value).map_err(|e| fail(e, true))?.value)
                    );
                    let error = Error::new(ErrorKind::Situation, message).in_rule(&name);
                    if self.program.options.strict.check_possible_values {
                        if !options.keep_previous {
                            self.restore_situation(previous);
                        }
                        self.public_situation = previous_public;
                        return Err(fail(error, false));
                    }
                    self.warn_situation(error);
                }
                Err(error) => return Err(fail(error, true)),
            }
        }

        for name in &valid {
            if !self.public_situation.contains(name) {
                self.public_situation.push(name.clone());
            }
        }
        match self.warn_experimental_situation() {
            Ok(()) => Ok(valid),
            Err(error) => Err(SituationError {
                error,
                situation_reset: false,
                accepted: valid,
            }),
        }
    }

    fn warn_situation(&self, error: Error) {
        let rule = self
            .state
            .evaluated_rule
            .map(|name| self.program.names.name(name).to_owned());
        self.program
            .warn(WarningKind::SituationIssues, error.to_string(), rule, None);
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

    /// Adds `entries` to the situation the rules include.
    fn load_situation_entries(&mut self, entries: &[(String, Raw)]) -> Result<()> {
        let rules = Raw::map(
            entries
                .iter()
                .map(|(name, value)| (format!("[privé] {name} . $SITUATION"), value.clone())),
        );
        self.load_overlay(&rules)?;
        for (name, value) in entries {
            self.situation.retain(|(n, _)| n != name);
            self.situation.push((name.clone(), value.clone()));
        }
        Ok(())
    }

    fn restore_situation(&mut self, situation: Vec<(String, Raw)>) {
        self.program.reset_overlay(self.base_len);
        self.situation.clear();
        self.load_situation_entries(&situation)
            .expect("the previous situation loaded before");
    }

    /// `checkSituationRulePossibilities`: evaluates the value given to a rule with
    /// possibilities, which also warms the cache, and checks it is one of them.
    fn check_possible_value(&mut self, name: &str, value: &Raw) -> Result<bool> {
        let Some(possibilities) = self
            .program
            .names
            .get(name)
            .and_then(|id| self.program.rule(id))
            .and_then(|rule| rule.possibilities)
        else {
            return Ok(true);
        };
        let evaluation = self.evaluate_raw(value)?;
        let reference = matches!(
            self.program.node(possibilities),
            Node::UnePossibilite {
                kind: PossibilityKind::Reference,
                ..
            }
        );
        if reference && matches!(evaluation.value, Value::Null | Value::Undefined) {
            return Ok(true);
        }
        Evaluator::new(&self.program, &mut self.state).is_valid_option(possibilities, &evaluation)
    }

    fn evaluate_raw(&mut self, value: &Raw) -> Result<Evaluated> {
        self.evaluated(value).map(|(evaluated, _)| evaluated)
    }

    fn warn_experimental_situation(&self) -> Result<()> {
        if !self.program.options.warn.experimental_rules {
            return Ok(());
        }
        for name in &self.public_situation {
            let Some(id) = self.program.names.get(name) else {
                continue;
            };
            self.warn_if_experimental(id);
            let situation = format!("{name} . $SITUATION");
            if let Some(rule) = self
                .program
                .names
                .get(&situation)
                .and_then(|id| self.program.rule_node(id))
            {
                self.warn_experimental_references(rule)?;
            }
        }
        Ok(())
    }
}

/// `parseSituationRules` reports parsing errors as situation errors.
fn situation_error(error: Error) -> Error {
    Error::new(ErrorKind::Situation, error.to_string())
}
