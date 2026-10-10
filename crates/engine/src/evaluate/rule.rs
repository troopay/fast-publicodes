//! Rules, their disabling by a parent, and `est non applicable`.

use super::{Branch, Evaluated, Evaluator, Missing, Operand, UnitSlot};
use crate::ast::{Node, NodeId, Operator, Rule, Type, Value};
use crate::error::Result;

impl Evaluator<'_> {
    pub(super) fn evaluate_rule(&mut self, rule: &Rule) -> Result<Evaluated> {
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
            if let Some(cached) = self.state.cache[rule.valeur.0 as usize].as_mut() {
                cached.missing = valeur_missing.clone();
            }
        }
        let possibilities_unit = possibilities.as_ref().and_then(|p| p.unit.get().cloned());
        let possibilities_missing = possibilities.map(|p| p.missing).unwrap_or_default();
        Ok(Evaluated {
            value: valeur.value,
            unit: match possibilities_unit {
                Some(unit) => UnitSlot::Set(unit.into()),
                None => valeur.unit,
            },
            missing: possibilities_missing
                .merge(&valeur_missing)
                .merge(&parent_missing),
        })
    }

    /// `evaluateDisablingParent`: whether a parent makes the rule not applicable.
    fn disabling_parent(&mut self, rule: &Rule) -> Result<(bool, Missing)> {
        let program = self.program;
        if program.names.name(rule.name).ends_with("$SITUATION") {
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
        let program = self.program;
        let name = format!("{} . $SITUATION", program.names.name(rule.name));
        program
            .names
            .get(&name)
            .and_then(|id| self.rule_node(id))
            .and_then(|node| program.ty(node))
            .is_some_and(|t| t.nullable == Some(true))
    }
}
