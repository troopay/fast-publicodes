use super::{Evaluated, Evaluator, UnitSlot};
use crate::ast::{NodeId, Value};
use crate::date::{
    difference_in_civil_years, difference_in_days, difference_in_months,
    difference_in_trimestres_civils, difference_in_years,
};
use crate::error::{Error, ErrorKind, Result};
use crate::units::UnitRef;

impl Evaluator<'_> {
    pub(super) fn duree(
        &mut self,
        depuis: NodeId,
        jusqua: NodeId,
        unit: &UnitRef,
    ) -> Result<Evaluated> {
        let from = self.evaluate(depuis)?;
        let to = self.evaluate(jusqua)?;
        let value = match (&from.value, &to.value) {
            (Value::Null, _) | (_, Value::Null) => Value::Null,
            (Value::Undefined, _) | (_, Value::Undefined) => Value::Undefined,
            (Value::String(from), Value::String(to)) => {
                let difference = match unit.numerators[0].as_str() {
                    "jour" => difference_in_days(from, to)?,
                    "mois" => difference_in_months(from, to),
                    "an" => difference_in_years(from, to)?,
                    "trimestre" => difference_in_months(from, to) / 3.,
                    "trimestre civil" => difference_in_trimestres_civils(from, to),
                    _ => difference_in_civil_years(from, to)?,
                };
                Value::Number(if difference.is_nan() {
                    difference
                } else {
                    difference.max(0.)
                })
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::Type,
                    "dateString.split is not a function or its return value is not iterable",
                ));
            }
        };
        Ok(Evaluated {
            value,
            unit: UnitSlot::present(Some(unit.clone())),
            missing: from.missing.merge(&to.missing),
        })
    }
}
