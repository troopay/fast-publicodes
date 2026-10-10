use super::{Evaluated, Evaluator, Missing, UnitSlot};
use crate::ast::{Branch, Value};
use crate::error::Result;
use crate::units::UnitRef;

impl Evaluator<'_> {
    pub(super) fn variations(&mut self, branches: &[Branch]) -> Result<Evaluated> {
        let mut value = Value::Null;
        let mut unit: Option<UnitRef> = None;
        // `None` is JS `undefined`: a previous condition could not be evaluated.
        let mut previous = Some(false);
        let mut missing = Missing::default();
        for branch in branches {
            if previous == Some(true) {
                continue;
            }
            let condition = self.evaluate(branch.condition)?;
            let satisfied = !matches!(condition.value, Value::Bool(false) | Value::Null);
            let current = match (previous, &condition.value) {
                (None, _) | (_, Value::Undefined) => None,
                _ => Some(satisfied),
            };
            missing = missing.merge(&condition.missing.bonus());
            if current == Some(false) {
                continue;
            }
            let mut consequence = None;
            if satisfied {
                let mut evaluated = self.evaluate(branch.consequence)?;
                if let Some(unit) = &unit
                    && let Ok(converted) = self.convert_node(Some(unit), &evaluated)
                {
                    evaluated = converted;
                }
                missing = missing.merge(&evaluated.missing);
                consequence = Some(evaluated);
            }
            value = match (current, &consequence) {
                (Some(true), Some(consequence)) => consequence.value.clone(),
                _ => Value::Undefined,
            };
            if unit.is_none() {
                unit = consequence.as_ref().and_then(|c| c.unit.get().cloned());
            }
            previous = current;
        }
        Ok(Evaluated {
            value,
            unit: UnitSlot::if_defined(unit),
            missing,
        })
    }
}
