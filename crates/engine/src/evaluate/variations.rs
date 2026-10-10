use super::{Evaluated, Evaluator, Missing, UnitSlot};
use crate::ast::{Branch, Value};
use crate::error::{Result, WarningKind};
use crate::units::UnitRef;

impl Evaluator<'_> {
    pub(super) fn variations(&mut self, branches: &[Branch]) -> Result<Evaluated> {
        let mut value = Value::Null;
        let mut unit: Option<UnitRef> = None;
        // `None` is JS `undefined`: a previous condition could not be evaluated.
        let mut previous = Some(false);
        let mut missing = Missing::default();
        for (i, branch) in branches.iter().enumerate() {
            if previous == Some(true) {
                continue;
            }
            let condition = self.evaluate(branch.condition)?;
            let satisfied = !matches!(condition.value, Value::Bool(false) | Value::Null);
            let current = match (previous, &condition.value) {
                (None, _) | (_, Value::Undefined) => None,
                _ => Some(satisfied),
            };
            let mut branch_missing = condition.missing.bonus();
            if current == Some(false) {
                missing = missing.merge(&branch_missing);
                continue;
            }
            let mut consequence = None;
            if satisfied {
                let mut evaluated = self.evaluate(branch.consequence)?;
                if let Some(unit) = &unit {
                    match self.convert_node(Some(unit), &evaluated) {
                        Ok(converted) => evaluated = converted,
                        Err(error) => self.warn(
                            WarningKind::UnitConversion,
                            format!(
                                "L'unité de la branche n° {} du mécanisme 'variations' n'est pas compatible avec celle d'une branche précédente",
                                i + 1
                            ),
                            Some(error),
                        ),
                    }
                }
                branch_missing = branch_missing.merge(&evaluated.missing);
                consequence = Some(evaluated);
            }
            missing = missing.merge(&branch_missing);
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
