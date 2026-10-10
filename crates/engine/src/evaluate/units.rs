//! The `unité` and `simplifier l'unité` mechanisms, and unit conversion of evaluated nodes.

use super::js_value::{to_number, truthy};
use super::{Evaluated, Evaluator, UnitSlot};
use crate::ast::{NodeId, Value};
use crate::error::Result;
use crate::units::{Unit, check_convertible, convert_unit, simplify_unit};

/// `convertUnit` for any value.
fn convert_value(from: Option<&Unit>, to: Option<&Unit>, value: &Value) -> Result<Value> {
    check_convertible(from, to)?;
    if !truthy(value) || from.is_none() {
        return Ok(value.clone());
    }
    Ok(Value::Number(convert_unit(from, to, to_number(value))?))
}

impl Evaluator<'_> {
    /// `convertNodeToUnit`.
    pub(super) fn convert_node(&self, to: Option<&Unit>, node: &Evaluated) -> Result<Evaluated> {
        let value = match (&node.value, node.unit.get()) {
            (Value::Number(n), Some(from)) => Value::Number(convert_unit(Some(from), to, *n)?),
            (value, _) => value.clone(),
        };
        Ok(Evaluated {
            value,
            unit: UnitSlot::present(to.cloned()),
            missing: node.missing.clone(),
        })
    }

    pub(super) fn unite(&mut self, unit: &Unit, valeur: NodeId) -> Result<Evaluated> {
        let valeur = self.evaluate(valeur)?;
        let mut value = valeur.value.clone();
        if value != Value::Null
            && let Ok(converted) = convert_value(valeur.unit.get(), Some(unit), &value)
        {
            value = converted;
        }
        Ok(Evaluated {
            value,
            unit: UnitSlot::present(Some(unit.clone())),
            missing: valeur.missing,
        })
    }

    pub(super) fn simplifier_unite(&mut self, valeur: NodeId) -> Result<Evaluated> {
        let valeur = self.evaluate(valeur)?;
        if matches!(valeur.value, Value::Null | Value::Undefined) {
            return Ok(valeur);
        }
        let Some(from) = valeur.unit.get().cloned() else {
            return Ok(Evaluated {
                unit: UnitSlot::Undefined,
                ..valeur
            });
        };
        let unit = simplify_unit(&from);
        let value = match valeur.value {
            Value::Number(n) => Value::Number(convert_unit(Some(&from), Some(&unit), n)?),
            value => value,
        };
        Ok(Evaluated {
            value,
            unit: UnitSlot::present(Some(unit)),
            missing: valeur.missing,
        })
    }
}
