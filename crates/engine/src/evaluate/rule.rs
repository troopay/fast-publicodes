//! Rules, their disabling by a parent, and `est non applicable`.

use super::js_value::to_js_string;
use super::{Branch, Evaluated, Evaluator, Missing, Operand, UnitSlot};
use crate::ast::{Node, NodeId, Operator, PossibilityKind, Rule, Type, Value};
use crate::error::{Error, Result, WarningKind};

impl Evaluator<'_> {
    pub(super) fn evaluate_rule(&mut self, rule: &Rule) -> Result<Evaluated> {
        let program = self.program;
        let name = program.names.name(rule.name);
        if rule.names_evaluation {
            self.state.evaluated_rule = Some(rule.name);
        }
        let (disabled, parent_missing) = self.disabling_parent(rule)?;
        if disabled {
            return Ok(Evaluated::new(Value::Null, parent_missing));
        }
        let occurrences = self
            .state
            .rule_stack
            .iter()
            .filter(|&&n| n == rule.name)
            .count();
        if occurrences > 1 {
            let parents = &self.state.parent_stack;
            if parents.is_empty()
                || parents
                    .iter()
                    .any(|&p| !program.names.name(p).starts_with(name))
            {
                let message = self.cycle_message(rule);
                if program.options.strict.no_cycle_runtime {
                    return Err(Error::evaluation(message).in_rule(name));
                }
                self.warn(WarningKind::CyclicReferences, message, None);
            }
            return Ok(Evaluated::new(Value::Undefined, parent_missing));
        }
        self.state.rule_stack.push(rule.name);
        let possibilities = match rule.possibilities {
            Some(possibilities) => Some(self.evaluate(possibilities)?),
            None => None,
        };
        if let Some(possibilities) = &possibilities
            && possibilities.value != Value::Undefined
        {
            // This path deliberately keeps the rule on the stack.
            return Ok(Evaluated {
                value: possibilities.value.clone(),
                unit: UnitSlot::if_defined(possibilities.unit.get().cloned()),
                missing: possibilities.missing.merge(&parent_missing),
            });
        }
        let valeur = self.evaluate(rule.valeur)?;
        self.state.rule_stack.pop();

        let mut valeur_missing = valeur.missing.clone();
        if rule.reports_missing && valeur.value == Value::Undefined && valeur_missing.is_empty() {
            valeur_missing = Missing::single(rule.name, 1.);
            if let Some(cached) = self.state.cache.get_own_mut(rule.valeur) {
                cached.missing = valeur_missing.clone();
            }
        }
        if let Some(node) = rule.possibilities
            && program.options.strict.check_possible_values
            && !self.is_valid_option(node, &valeur)?
        {
            return Err(Error::evaluation(format!(
                "La valeur \"{}\" de la règle {name} n'est pas une des possibilités attendues",
                to_js_string(&valeur.value)
            ))
            .in_rule(name));
        }
        let possibilities_unit = possibilities.as_ref().and_then(|p| p.unit.get().cloned());
        let possibilities_missing = possibilities.map(|p| p.missing).unwrap_or_default();
        Ok(Evaluated {
            value: valeur.value,
            unit: match possibilities_unit {
                Some(unit) => UnitSlot::Set(unit),
                None => valeur.unit,
            },
            missing: possibilities_missing
                .merge(&valeur_missing)
                .merge(&parent_missing),
        })
    }

    fn cycle_message(&self, rule: &Rule) -> String {
        let names = &self.program.names;
        let stack = &self.state.rule_stack;
        let start = stack.iter().rposition(|&n| n == rule.name).unwrap_or(0);
        let cycle: Vec<&str> = stack[start..]
            .iter()
            .chain([&rule.name])
            .map(|&n| names.name(n))
            .collect();
        let separator = if cycle.len() > 5 { "\n → " } else { " → " };
        let name = names.name(rule.name);
        format!(
            "\nUn cycle a été détecté lors de l'évaluation de cette règle:\n{}\n\nCela vient probablement d'une erreur dans votre modèle.\n\nSi le cycle est voulu, vous pouvez indiquer au moteur de résoudre la référence circulaire en trouvant le point fixe de la fonction. Pour cela, ajoutez l'attribut suivant niveau de la règle :\n\n\t{name}:\n\t\trésoudre la référence circulaire: oui\"\n\t\t...\n",
            cycle.join(separator)
        )
    }

    /// `isAValidOption`: whether `value` is one of the possibilities.
    pub fn is_valid_option(&mut self, possibilities: NodeId, value: &Evaluated) -> Result<bool> {
        let program = self.program;
        if matches!(value.value, Value::Null | Value::Undefined) {
            return Ok(true);
        }
        let Node::UnePossibilite {
            kind,
            options,
            values,
            not_applicable,
            ..
        } = program.node(possibilities)
        else {
            return Ok(true);
        };
        let written = format!("'{}'", to_js_string(&value.value));
        match kind {
            PossibilityKind::Number => {
                for &option in options {
                    let Node::Constant(constant) = program.node(option) else {
                        continue;
                    };
                    let matches = match &constant.unit {
                        Some(unit) => self.convert_node(Some(unit), value)?.value == constant.value,
                        None => constant.value == value.value,
                    };
                    if matches {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            PossibilityKind::String => Ok(values.contains(&written)),
            PossibilityKind::Reference => {
                self.evaluate(possibilities)?;
                for (v, not_applicable) in values.iter().zip(not_applicable) {
                    let excluded = match not_applicable {
                        Some(node) => self.evaluate(*node)?.value == Value::Bool(true),
                        None => false,
                    };
                    if *v == written && !excluded {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    /// `evaluateDisablingParent`: whether a parent makes the rule not applicable.
    fn disabling_parent(&mut self, rule: &Rule) -> Result<(bool, Missing)> {
        let program = self.program;
        if rule.is_situation {
            return Ok((false, Missing::default()));
        }
        let Some(parent) = rule.parents.iter().copied().find(|&parent| {
            program
                .ty(parent)
                .is_some_and(|t| t.nullable == Some(true) || t.ty == Some(Type::Boolean))
        }) else {
            return Ok((false, Missing::default()));
        };
        let parent_type = program.ty(parent).unwrap_or_default();
        let boolean = parent_type.ty == Some(Type::Boolean);
        if !self.state.parent_stack.contains(&rule.name) {
            self.state.parent_stack.push(rule.name);
            let mut not_applicable = Evaluated::new(Value::Bool(false), Missing::default());
            if parent_type.nullable == Some(true) {
                not_applicable = self.est_non_applicable(parent)?;
            }
            if not_applicable.value != Value::Bool(true) && boolean {
                let no = Operand::Constant(Evaluated::new(Value::Bool(false), Missing::default()));
                not_applicable = self.operation(Operator::Equal, Operand::Node(parent), no)?;
            }
            self.state.parent_stack.pop();
            if not_applicable.value == Value::Bool(true) {
                return Ok((true, not_applicable.missing));
            }
        }
        if boolean {
            let evaluated = self.evaluate(parent)?;
            return Ok((evaluated.value == Value::Bool(false), evaluated.missing));
        }
        Ok((false, Missing::default()))
    }

    /// `est non applicable` of `valeur`, evaluated without caching its result.
    pub(super) fn est_non_applicable(&mut self, valeur: NodeId) -> Result<Evaluated> {
        let program = self.program;
        let node = program.node(valeur);
        if program
            .ty(valeur)
            .is_some_and(|t| t.nullable == Some(false))
            && !matches!(node, Node::Rule(_) | Node::Reference(_))
        {
            return Ok(Evaluated::new(Value::Bool(false), Missing::default()));
        }
        if let Some(cached) = self.cached(valeur) {
            return Ok(Evaluated::new(
                Value::Bool(cached.value == Value::Null),
                cached.missing.clone(),
            ));
        }
        match node {
            Node::Rule(rule) => {
                let (disabled, parent_missing) = self.disabling_parent(rule)?;
                if disabled {
                    return Ok(Evaluated::new(Value::Bool(true), parent_missing));
                }
                let inner = self.est_non_applicable(rule.valeur)?;
                let mut missing = parent_missing.merge(&inner.missing);
                if inner.value == Value::Bool(false)
                    && inner.missing.is_empty()
                    && self.situation_is_nullable(rule)
                {
                    missing = missing.with(rule.name, 1.);
                }
                Ok(Evaluated::new(inner.value, missing))
            }
            Node::Reference(reference) => match reference.target.and_then(|t| self.rule_node(t)) {
                Some(rule) => self.est_non_applicable(rule),
                None => Ok(Evaluated::new(Value::Undefined, Missing::default())),
            },
            Node::Condition { si, alors, sinon } => self.condition(
                *si,
                Branch::NotApplicable(*alors),
                Branch::NotApplicable(*sinon),
            ),
            _ => {
                let evaluated = self.evaluate(valeur)?;
                let value = match evaluated.value {
                    Value::Undefined => Value::Undefined,
                    value => Value::Bool(value == Value::Null),
                };
                Ok(Evaluated::new(value, evaluated.missing))
            }
        }
    }

    fn situation_is_nullable(&self, rule: &Rule) -> bool {
        rule.situation
            .and_then(|name| self.rule_node(name))
            .and_then(|node| self.program.ty(node))
            .is_some_and(|t| t.nullable == Some(true))
    }
}
