use super::js_value::power;
use super::{Evaluated, Evaluator, UnitSlot};
use crate::ast::{NodeId, Value};
use crate::error::{Error, Result};
use crate::units::{serialize_unit, simplify_unit};

/// `Math.round`: halves round towards positive infinity.
fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let floor = x.floor();
    if x - floor >= 0.5 { floor + 1. } else { floor }
}

fn round_with_precision(n: f64, digits: f64) -> f64 {
    let scale = power(10., digits);
    js_round((n + f64::EPSILON) * scale) / scale
}

impl Evaluator<'_> {
    /// `simplifyNodeUnit`.
    pub(super) fn simplify_node_unit(&self, node: Evaluated) -> Result<Evaluated> {
        match node.unit.get() {
            None => Ok(node),
            Some(unit) => {
                let simplified = simplify_unit(unit);
                self.convert_node(Some(&simplified), &node)
            }
        }
    }

    pub(super) fn arrondi(&mut self, valeur: NodeId, arrondi: NodeId) -> Result<Evaluated> {
        let valeur = self.evaluate(valeur)?;
        let valeur = self.simplify_node_unit(valeur)?;
        let arrondi = if valeur.value == Value::Bool(false) {
            None
        } else {
            let arrondi = self.evaluate(arrondi)?;
            if let Value::Number(_) = arrondi.value
                && !serialize_unit(arrondi.unit.get()).is_some_and(|u| u.contains("décimale"))
            {
                return Err(Error::evaluation(format!(
                    "L'unité {} de l'arrondi est inconnu. Vous devez utiliser l'unité “décimales”",
                    serialize_unit(arrondi.unit.get()).unwrap_or_default()
                )));
            }
            Some(arrondi)
        };
        let value = match (&valeur.value, arrondi.as_ref().map(|a| &a.value)) {
            (Value::Number(n), Some(Value::Number(digits))) => {
                Value::Number(round_with_precision(*n, *digits))
            }
            (Value::Number(n), Some(Value::Bool(true))) => {
                Value::Number(round_with_precision(*n, 0.))
            }
            (Value::Number(_), Some(Value::Undefined)) => Value::Undefined,
            (value, _) => value.clone(),
        };
        let missing = match &arrondi {
            Some(arrondi) => valeur.missing.merge(&arrondi.missing),
            None => valeur.missing.clone(),
        };
        Ok(Evaluated {
            value,
            unit: UnitSlot::present(valeur.unit.get().cloned()),
            missing,
        })
    }
}
