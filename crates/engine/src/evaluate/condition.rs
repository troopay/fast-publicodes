use super::{Evaluated, Evaluator, UnitSlot};
use crate::ast::{NodeId, Value};
use crate::error::{Error, Result};
use crate::units::UnitRef;

/// A branch of a condition: a node, or `est non applicable` of a node.
#[derive(Debug, Clone, Copy)]
pub enum Branch {
    Node(NodeId),
    NotApplicable(NodeId),
}

impl Evaluator<'_> {
    fn branch(&mut self, branch: Branch) -> Result<Evaluated> {
        match branch {
            Branch::Node(id) => self.evaluate(id),
            Branch::NotApplicable(id) => self.est_non_applicable(id),
        }
    }

    /// The unit a branch carries before evaluation.
    fn static_unit(&self, branch: Branch) -> Option<UnitRef> {
        match branch {
            Branch::Node(id) => self.program.node(id).static_unit().cloned(),
            Branch::NotApplicable(_) => None,
        }
    }

    pub(super) fn condition(
        &mut self,
        si: NodeId,
        alors: Branch,
        sinon: Branch,
    ) -> Result<Evaluated> {
        let condition = self.evaluate(si)?;
        let not_boolean = || Error::evaluation("La condition doit être de type booléen");
        if !matches!(condition.unit, UnitSlot::Absent) {
            return Err(not_boolean());
        }
        let (evaluation, alors_unit) = match condition.value {
            Value::Bool(true) => {
                let alors = self.branch(alors)?;
                let unit = alors.unit.get().cloned();
                (alors, unit)
            }
            Value::Bool(false) => (self.branch(sinon)?, self.static_unit(alors)),
            Value::Null => (condition.clone(), self.static_unit(alors)),
            Value::Undefined => {
                let sinon = self.branch(sinon)?;
                let alors = self.branch(alors)?;
                let unit = alors.unit.get().cloned();
                let evaluation = Evaluated {
                    missing: sinon.missing.merge(&alors.missing),
                    ..condition.clone()
                };
                (evaluation, unit)
            }
            _ => return Err(not_boolean()),
        };
        let unit = evaluation.unit.get().cloned().or(alors_unit);
        Ok(Evaluated {
            value: evaluation.value,
            unit: UnitSlot::if_defined(unit),
            missing: condition.missing.bonus().merge(&evaluation.missing),
        })
    }
}
