//! Evaluation of nodes, cached for the current situation.

mod condition;
mod js_value;
mod missing;
mod operation;
mod rule;
mod units;
mod variations;

use std::rc::Rc;

pub use condition::Branch;
pub use missing::Missing;
pub use operation::Operand;

use crate::ast::{NameId, Node, NodeId, PossibilityKind, Program, Value};
use crate::error::{Error, ErrorKind, Result};
use crate::units::Unit;

/// The `unit` key of an evaluated node: absent, present but undefined, or set.
#[derive(Debug, Clone, Default)]
pub enum UnitSlot {
    #[default]
    Absent,
    Undefined,
    Set(Rc<Unit>),
}

impl UnitSlot {
    pub fn get(&self) -> Option<&Unit> {
        match self {
            UnitSlot::Set(unit) => Some(unit),
            _ => None,
        }
    }

    /// `{ unit: value }`: the key is present even when the value is undefined.
    pub fn present(unit: Option<Unit>) -> Self {
        unit.map_or(UnitSlot::Undefined, |unit| UnitSlot::Set(Rc::new(unit)))
    }

    /// `...(unit ? { unit } : {})`.
    pub fn if_defined(unit: Option<Unit>) -> Self {
        unit.map_or(UnitSlot::Absent, |unit| UnitSlot::Set(Rc::new(unit)))
    }
}

#[derive(Debug, Clone)]
pub struct Evaluated {
    pub value: Value,
    pub unit: UnitSlot,
    pub missing: Missing,
}

impl Evaluated {
    pub fn new(value: Value, missing: Missing) -> Self {
        Self {
            value,
            unit: UnitSlot::Absent,
            missing,
        }
    }
}

/// Caches and stacks that live as long as a situation.
#[derive(Debug, Default)]
pub struct EvalState {
    cache: Vec<Option<Evaluated>>,
    rule_stack: Vec<NameId>,
    parent_stack: Vec<NameId>,
}

impl EvalState {
    pub fn clear(&mut self) {
        self.cache.clear();
        self.reset_stacks();
    }

    pub fn reset_stacks(&mut self) {
        self.rule_stack.clear();
        self.parent_stack.clear();
    }
}

pub struct Evaluator<'a> {
    pub program: &'a Program,
    pub state: &'a mut EvalState,
}

impl<'a> Evaluator<'a> {
    pub fn new(program: &'a Program, state: &'a mut EvalState) -> Self {
        if state.cache.len() < program.nodes.len() {
            state.cache.resize(program.nodes.len(), None);
        }
        Self { program, state }
    }

    fn cached(&self, id: NodeId) -> Option<&Evaluated> {
        self.state.cache[id.0 as usize].as_ref()
    }

    pub fn evaluate(&mut self, id: NodeId) -> Result<Evaluated> {
        if let Some(evaluated) = self.cached(id) {
            return Ok(evaluated.clone());
        }
        let evaluated = self.compute(id)?;
        self.state.cache[id.0 as usize] = Some(evaluated.clone());
        Ok(evaluated)
    }

    fn compute(&mut self, id: NodeId) -> Result<Evaluated> {
        let program = self.program;
        match program.node(id) {
            Node::Constant(constant) => Ok(Evaluated {
                value: constant.value.clone(),
                unit: UnitSlot::if_defined(constant.unit.clone()),
                missing: Missing::default(),
            }),
            Node::Reference(reference) => {
                let rule = reference
                    .target
                    .and_then(|target| program.rule_node(target))
                    .ok_or_else(|| {
                        Error::new(
                            ErrorKind::Internal,
                            format!("Référence non résolue : {}", reference.name),
                        )
                    })?;
                self.evaluate(rule)
            }
            Node::Rule(rule) => self.evaluate_rule(rule),
            Node::Operation { operator, operands } => self.operation(
                *operator,
                Operand::Node(operands[0]),
                Operand::Node(operands[1]),
            ),
            Node::Condition { si, alors, sinon } => {
                self.condition(*si, Branch::Node(*alors), Branch::Node(*sinon))
            }
            Node::Variations(branches) => self.variations(branches),
            Node::EstNonDefini(valeur) => {
                let valeur = self.evaluate(*valeur)?;
                Ok(Evaluated::new(
                    Value::Bool(valeur.value == Value::Undefined),
                    valeur.missing,
                ))
            }
            Node::EstNonApplicable(valeur) => self.est_non_applicable(*valeur),
            Node::VariableManquante { variable, valeur } => {
                let valeur = self.evaluate(*valeur)?;
                let score = valeur.missing.max_score() + 1.;
                Ok(Evaluated {
                    missing: valeur.missing.merge(&Missing::single(*variable, score)),
                    unit: UnitSlot::present(valeur.unit.get().cloned()),
                    value: valeur.value,
                })
            }
            Node::Unite { unit, valeur } => self.unite(unit, *valeur),
            Node::SimplifierUnite(valeur) => self.simplifier_unite(*valeur),
            Node::UnePossibilite {
                kind,
                unit,
                options,
            } => {
                let value = match (kind, options.as_slice()) {
                    (PossibilityKind::Reference, []) => Value::Null,
                    (PossibilityKind::Reference, [only]) => match program.node(*only) {
                        Node::Reference(reference) => Value::String(reference.name.as_str().into()),
                        _ => Value::Undefined,
                    },
                    _ => Value::Undefined,
                };
                Ok(Evaluated {
                    value,
                    unit: UnitSlot::if_defined(unit.clone()),
                    missing: Missing::default(),
                })
            }
            Node::Unsupported(mechanism) => Err(Error::new(
                ErrorKind::Unsupported,
                format!("Le mécanisme \"{mechanism}\" n'est pas encore supporté"),
            )),
        }
    }
}
