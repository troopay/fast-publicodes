//! `barème`, `grille` and `taux progressif`, which share how tranches are bounded.

use super::js_value::{compare, to_number};
use super::{Evaluated, Evaluator, Missing, UnitSlot};
use std::rc::Rc;

use crate::ast::{NodeId, Tranche, TrancheMechanism, Value};
use crate::error::{Error, Result, WarningKind};
use crate::units::{Unit, UnitOperator, check_convertible, convert_unit, infer_unit, parse_unit};

/// A tranche after its bounds were evaluated. `None` stands for JS `undefined`.
struct Bounded {
    plafond: Option<Evaluated>,
    plafond_value: Option<f64>,
    plancher_value: Option<f64>,
    is_after_active: Option<bool>,
    is_active: Option<bool>,
    missing: Missing,
}

fn defined(value: Option<f64>) -> Value {
    value.map_or(Value::Undefined, Value::Number)
}

fn percent() -> Unit {
    Unit {
        numerators: vec!["%".to_owned()],
        denominators: Vec::new(),
    }
}

impl Evaluator<'_> {
    /// `evaluatePlafondUntilActiveTranche`: bounds every tranche up to the active one.
    fn bound_tranches(
        &mut self,
        tranches: &[Tranche],
        assiette: &Evaluated,
        multiplicateur: &Evaluated,
    ) -> Result<Vec<Bounded>> {
        let mut bounded: Vec<Bounded> = Vec::with_capacity(tranches.len());
        let mut active_found = false;
        for (i, tranche) in tranches.iter().enumerate() {
            if active_found {
                bounded.push(Bounded {
                    plafond: None,
                    plafond_value: None,
                    plancher_value: None,
                    is_after_active: Some(true),
                    is_active: None,
                    missing: Missing::default(),
                });
                continue;
            }
            let plafond = self.evaluate(tranche.plafond)?;
            let previous = i.checked_sub(1).map(|j| &bounded[j]);
            let mut plafond_value = match (&plafond.value, &multiplicateur.value) {
                (Value::Undefined, _) | (_, Value::Undefined) => None,
                (p, m) => Some(to_number(p) * to_number(m)),
            };
            if !matches!(plafond_value, Some(v) if v == f64::INFINITY || v == 0.) {
                let from = infer_unit(
                    UnitOperator::Times,
                    [plafond.unit.get(), multiplicateur.unit.get()],
                );
                let converted = match plafond_value {
                    Some(value) => {
                        convert_unit(from.as_deref(), assiette.unit.unit(), value).map(Some)
                    }
                    None => check_convertible(from.as_deref(), assiette.unit.unit()).map(|()| None),
                };
                match converted {
                    Ok(converted) => plafond_value = converted,
                    Err(error) => self.warn(
                        WarningKind::UnitConversion,
                        format!(
                            "L'unité du plafond de la tranche n°{}  n'est pas compatible avec celle l'assiette",
                            i + 1
                        ),
                        Some(error),
                    ),
                }
            }
            let plancher_value = match previous {
                Some(previous) => previous.plafond_value,
                None => Some(0.),
            };
            let is_after_active = match (plancher_value, &assiette.value) {
                (None, _) | (_, Value::Undefined) => None,
                (Some(plancher), assiette) => {
                    Some(compare(&Value::Number(plancher), assiette).is_some_and(|o| o.is_gt()))
                }
            };
            let (plancher_node_value, plancher_missing) = match previous {
                Some(previous) => previous
                    .plafond
                    .as_ref()
                    .map_or((Value::Undefined, Missing::default()), |p| {
                        (p.value.clone(), p.missing.clone())
                    }),
                None => (Value::Number(0.), Missing::default()),
            };
            let calculation = [
                &plafond.value,
                &assiette.value,
                &multiplicateur.value,
                &plancher_node_value,
            ];
            if calculation.iter().any(|v| **v == Value::Undefined) {
                let missing = plafond
                    .missing
                    .merge(&assiette.missing)
                    .merge(&multiplicateur.missing)
                    .merge(&plancher_missing);
                bounded.push(Bounded {
                    plafond: Some(plafond),
                    plafond_value,
                    plancher_value,
                    is_after_active,
                    is_active: None,
                    missing,
                });
                active_found = false;
                continue;
            }
            if let (Some(_), Some(plancher)) = (previous, plancher_value)
                && plancher != 0.
                && !plancher.is_nan()
                && plafond_value.is_some_and(|p| p <= plancher)
            {
                return Err(self.error(format!(
                    "Le plafond de la tranche n°{} a une valeur inférieure à celui de la tranche précédente",
                    i + 1
                )));
            }
            let is_active = compare(&assiette.value, &defined(plancher_value))
                .is_some_and(|o| o.is_ge())
                && compare(&assiette.value, &defined(plafond_value)).is_some_and(|o| o.is_lt());
            bounded.push(Bounded {
                plafond: Some(plafond),
                plafond_value,
                plancher_value,
                is_after_active,
                is_active: Some(is_active),
                missing: Missing::default(),
            });
            active_found = is_active;
        }
        Ok(bounded)
    }

    pub(super) fn tranches(
        &mut self,
        mechanism: TrancheMechanism,
        assiette: NodeId,
        multiplicateur: NodeId,
        tranches: &[Tranche],
    ) -> Result<Evaluated> {
        let assiette = self.evaluate(assiette)?;
        let multiplicateur = self.evaluate(multiplicateur)?;
        if multiplicateur.value == Value::Number(0.) {
            return Err(match mechanism {
                TrancheMechanism::TauxProgressif => Error::evaluation("Division by zero"),
                _ => self.error("Le multiplicateur ne peut pas être nul"),
            });
        }
        match mechanism {
            TrancheMechanism::Bareme => self.bareme(tranches, assiette, multiplicateur),
            TrancheMechanism::Grille => self.grille(tranches, assiette, multiplicateur),
            TrancheMechanism::TauxProgressif => {
                self.taux_progressif(tranches, assiette, multiplicateur)
            }
        }
    }

    fn bareme(
        &mut self,
        tranches: &[Tranche],
        assiette: Evaluated,
        multiplicateur: Evaluated,
    ) -> Result<Evaluated> {
        let mut missing = assiette.missing.merge(&multiplicateur.missing);
        let mut value = assiette.value.clone();
        if !matches!(assiette.value, Value::Number(n) if n == 0.)
            && !matches!(assiette.value, Value::Undefined | Value::Null)
        {
            let bounded = self.bound_tranches(tranches, &assiette, &multiplicateur)?;
            let mut total = Value::Number(0.);
            for (tranche, bounds) in tranches.iter().zip(&bounded) {
                let (tranche_value, tranche_missing) = if bounds.is_after_active == Some(true) {
                    (Value::Number(0.), bounds.missing.clone())
                } else {
                    let taux = self.evaluate(tranche.taux.ok_or_else(|| {
                        Error::evaluation("Une tranche de barème doit avoir un taux")
                    })?)?;
                    let tranche_missing = taux.missing.merge(&bounds.missing);
                    let inputs = [
                        assiette.value.clone(),
                        taux.value.clone(),
                        defined(bounds.plafond_value),
                        defined(bounds.plancher_value),
                    ];
                    if inputs.contains(&Value::Undefined) {
                        (Value::Undefined, tranche_missing)
                    } else {
                        let rate = match &taux.value {
                            Value::Number(n) => {
                                convert_unit(taux.unit.unit(), Some(&parse_unit("")?), *n)?
                            }
                            other => to_number(other),
                        };
                        let assiette = to_number(&assiette.value);
                        let plafond = bounds.plafond_value.unwrap_or(f64::NAN);
                        let minimum = if assiette.is_nan() || plafond.is_nan() {
                            f64::NAN
                        } else {
                            assiette.min(plafond)
                        };
                        let plancher = bounds.plancher_value.unwrap_or(f64::NAN);
                        (Value::Number((minimum - plancher) * rate), tranche_missing)
                    }
                };
                missing = missing.merge(&tranche_missing);
                total = match (total, tranche_value) {
                    (_, Value::Undefined | Value::Null) => Value::Undefined,
                    (Value::Number(acc), value) => Value::Number(acc + to_number(&value)),
                    (_, _) => Value::Number(f64::NAN),
                };
            }
            value = total;
        }
        Ok(Evaluated {
            value,
            unit: UnitSlot::present(assiette.unit.get().cloned()),
            missing,
        })
    }

    fn grille(
        &mut self,
        tranches: &[Tranche],
        assiette: Evaluated,
        multiplicateur: Evaluated,
    ) -> Result<Evaluated> {
        let bounded = self.bound_tranches(tranches, &assiette, &multiplicateur)?;
        // The montant of every tranche not known to be inactive.
        let mut montants = Vec::with_capacity(bounded.len());
        for (tranche, bounds) in tranches.iter().zip(&bounded) {
            montants.push(if bounds.is_active == Some(false) {
                None
            } else {
                let montant = self.evaluate(tranche.montant.ok_or_else(|| {
                    Error::evaluation("Une tranche de grille doit avoir un montant")
                })?)?;
                let missing = montant.missing.merge(&bounds.missing);
                Some(Evaluated { missing, ..montant })
            });
        }
        let mut missing = assiette.missing.merge(&multiplicateur.missing);
        let (value, unit) =
            if let Some(index) = bounded.iter().position(|b| b.is_active == Some(true)) {
                let montant = montants[index]
                    .as_ref()
                    .expect("active tranches are evaluated");
                missing = missing.merge(&montant.missing);
                (montant.value.clone(), montant.unit.get().cloned())
            } else if bounded
                .last()
                .is_some_and(|b| b.is_after_active == Some(false))
            {
                (Value::Undefined, None)
            } else {
                let pending: Vec<&Evaluated> = bounded
                    .iter()
                    .zip(&montants)
                    .filter(|(bounds, _)| bounds.is_active.is_none())
                    .filter_map(|(_, montant)| montant.as_ref())
                    .collect();
                for montant in &pending {
                    missing = missing.merge(&montant.missing);
                }
                match pending.first() {
                    None => (Value::Bool(false), None),
                    Some(montant) => (Value::Undefined, montant.unit.get().cloned()),
                }
            };
        Ok(Evaluated {
            value,
            unit: UnitSlot::present(unit),
            missing,
        })
    }

    fn rate_in_percent(&mut self, taux: Option<NodeId>) -> Result<Evaluated> {
        let taux = taux.ok_or_else(|| {
            Error::evaluation("Une tranche de taux progressif doit avoir un taux")
        })?;
        let taux = self.evaluate(taux)?;
        self.convert_node(Some(&Rc::new(percent())), &taux)
    }

    fn taux_progressif(
        &mut self,
        tranches: &[Tranche],
        assiette: Evaluated,
        multiplicateur: Evaluated,
    ) -> Result<Evaluated> {
        let bounded = self.bound_tranches(tranches, &assiette, &multiplicateur)?;
        let unit = UnitSlot::present(Some(Rc::new(percent())));
        let result = |value, missing| Evaluated {
            value,
            unit: unit.clone(),
            missing,
        };
        let (Some(last_bounds), Some(last)) = (bounded.last(), tranches.last()) else {
            return Ok(result(Value::Undefined, Missing::default()));
        };
        let last_plafond_infinite = match &last_bounds.plafond {
            Some(plafond) => plafond.value == Value::Number(f64::INFINITY),
            None => matches!(
                self.program.node(last.plafond),
                crate::ast::Node::Constant(c) if c.value == Value::Number(f64::INFINITY)
            ),
        };
        if bounded.iter().all(|b| b.is_active == Some(false))
            || (last_bounds.is_active == Some(true) && last_plafond_infinite)
        {
            let taux = self.rate_in_percent(last.taux)?;
            return Ok(result(taux.value, taux.missing));
        }
        if bounded.iter().all(|b| b.is_active != Some(true))
            || !matches!(assiette.value, Value::Number(_))
        {
            let missing = bounded
                .iter()
                .fold(Missing::default(), |acc, b| acc.merge(&b.missing));
            return Ok(result(Value::Undefined, missing));
        }
        let index = bounded
            .iter()
            .position(|b| b.is_active == Some(true))
            .unwrap_or_default();
        let active = self.rate_in_percent(tranches[index].taux)?;
        let previous = match index.checked_sub(1) {
            Some(previous) => Some(self.rate_in_percent(tranches[previous].taux)?),
            None => None,
        };
        let previous = previous.as_ref().unwrap_or(&active);
        if previous.value == Value::Undefined || active.value == Value::Undefined {
            return Ok(result(
                Value::Undefined,
                previous.missing.merge(&active.missing),
            ));
        }
        let lower = to_number(&previous.value);
        let upper = to_number(&active.value);
        let plancher = bounded[index].plancher_value.unwrap_or(f64::NAN);
        let plafond = bounded[index].plafond_value.unwrap_or(f64::NAN);
        let coefficient = (upper - lower) / (plafond - plancher);
        let value = lower + (to_number(&assiette.value) - plancher) * coefficient;
        Ok(result(Value::Number(value), Missing::default()))
    }
}
