//! Evaluation of nodes, cached for the current situation.

mod arrondi;
mod cache;
mod condition;
mod dates;
mod js_value;
mod missing;
mod operation;
mod rule;
mod sub_engines;
mod tranches;
mod units;
mod variations;

use rustc_hash::FxHashMap;

pub use condition::Branch;
pub use js_value::{to_js_string, to_number};
pub use missing::Missing;
pub use operation::Operand;

use cache::Cache;

use crate::ast::{NameId, Node, NodeId, PossibilityKind, Program, TextPart, Value};
use crate::error::{Error, ErrorKind, Result, WarningKind};
use crate::format::format_value;
use crate::units::{Unit, UnitRef};

/// The `unit` key of an evaluated node: absent, present but undefined, or set.
#[derive(Debug, Clone, Default)]
pub enum UnitSlot {
    #[default]
    Absent,
    Undefined,
    Set(UnitRef),
}

impl UnitSlot {
    pub fn unit(&self) -> Option<&Unit> {
        self.get().map(|unit| &**unit)
    }

    pub fn get(&self) -> Option<&UnitRef> {
        match self {
            UnitSlot::Set(unit) => Some(unit),
            _ => None,
        }
    }

    /// `{ unit: value }`: the key is present even when the value is undefined.
    pub fn present(unit: Option<UnitRef>) -> Self {
        unit.map_or(UnitSlot::Undefined, UnitSlot::Set)
    }

    /// `...(unit ? { unit } : {})`.
    pub fn if_defined(unit: Option<UnitRef>) -> Self {
        unit.map_or(UnitSlot::Absent, UnitSlot::Set)
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

/// The state of an engine for its situation: caches, stacks and sub-engines.
#[derive(Debug, Default, Clone)]
pub struct EvalState {
    cache: Cache,
    rule_stack: Vec<NameId>,
    parent_stack: Vec<NameId>,
    /// Rules the situations of enclosing sub-engines define, innermost last.
    layers: Vec<FxHashMap<NameId, NodeId>>,
    /// The values sub-engines give their `SituationInput` nodes.
    inputs: FxHashMap<NodeId, Evaluated>,
    /// The sub-engine of each `contexte`, by mechanism.
    sub_engines: FxHashMap<u64, EvalState>,
    /// The amended situation this sub-engine evaluates a `contexte` in.
    contexte_situation: Option<Vec<(NameId, u64)>>,
    inversion_fail: bool,
    /// The last rule evaluated, which warnings name.
    pub evaluated_rule: Option<NameId>,
}

impl EvalState {
    /// Clears everything a situation determines, keeping the rules defined by sub-engines.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.reset_stacks();
        self.sub_engines.clear();
        self.contexte_situation = None;
        self.inversion_fail = false;
    }

    pub fn reset_stacks(&mut self) {
        self.rule_stack.clear();
        self.parent_stack.clear();
    }

    /// Whether an inversion failed in the current situation.
    pub fn inversion_fail(&self) -> bool {
        self.inversion_fail
    }

    /// `shallowCopy`: a sub-engine seeing the same rules, with a copy of this cache if
    /// `copy_cache`.
    fn fork(&self, copy_cache: bool) -> EvalState {
        EvalState {
            cache: if copy_cache {
                self.cache.snapshot()
            } else {
                Cache::default()
            },
            layers: self.layers.clone(),
            inputs: self.inputs.clone(),
            evaluated_rule: self.evaluated_rule,
            ..EvalState::default()
        }
    }
}

pub struct Evaluator<'a> {
    pub program: &'a Program,
    pub state: &'a mut EvalState,
}

impl<'a> Evaluator<'a> {
    pub fn new(program: &'a Program, state: &'a mut EvalState) -> Self {
        Self { program, state }
    }

    pub fn warn(&self, kind: WarningKind, message: String, cause: Option<Error>) {
        let rule = self
            .state
            .evaluated_rule
            .map(|name| self.program.names.name(name).to_owned());
        self.program.warn(kind, message, rule, cause);
    }

    /// The rule node defining `name` for this engine.
    fn rule_node(&self, name: NameId) -> Option<NodeId> {
        self.state
            .layers
            .iter()
            .rev()
            .find_map(|layer| layer.get(&name).copied())
            .or_else(|| self.program.rule_node(name))
    }

    fn cached(&self, id: NodeId) -> Option<&Evaluated> {
        self.state.cache.get(id)
    }

    pub fn evaluate(&mut self, id: NodeId) -> Result<Evaluated> {
        if let Some(evaluated) = self.cached(id) {
            return Ok(evaluated.clone());
        }
        let evaluated = self.compute(id)?;
        self.state.cache.insert(id, evaluated.clone());
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
                    .and_then(|target| self.rule_node(target))
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
            Node::Arrondi { valeur, arrondi } => self.arrondi(*valeur, *arrondi),
            Node::Duree {
                depuis,
                jusqua,
                unit,
            } => self.duree(*depuis, *jusqua, unit),
            Node::Logarithme(valeur) => {
                let valeur = self.evaluate(*valeur)?;
                let valeur = self.simplify_node_unit(valeur)?;
                let value = match valeur.value {
                    Value::Null | Value::Undefined => valeur.value,
                    Value::Number(n) => Value::Number(n.ln()),
                    other => {
                        return Err(Error::evaluation(format!(
                            "Impossible de calculer le logarithme de {}",
                            js_value::to_js_string(&other)
                        )));
                    }
                };
                Ok(Evaluated::new(value, valeur.missing))
            }
            Node::Texte(parts) => {
                let mut text = String::new();
                let mut missing = Missing::default();
                for part in parts {
                    match part {
                        TextPart::Text(literal) => text.push_str(literal),
                        TextPart::Node(node) => {
                            let evaluated = self.evaluate(*node)?;
                            text.push_str(&format_value(&evaluated.value, evaluated.unit.unit())?);
                            missing = missing.merge(&evaluated.missing);
                        }
                    }
                }
                Ok(Evaluated::new(Value::String(text.into()), missing))
            }
            Node::Tranches {
                mechanism,
                assiette,
                multiplicateur,
                tranches,
            } => self.tranches(*mechanism, *assiette, *multiplicateur, tranches),
            Node::SimplifierUnite(valeur) => self.simplifier_unite(*valeur),
            Node::UnePossibilite {
                kind,
                unit,
                options,
                not_applicable,
                ..
            } => {
                if *kind != PossibilityKind::Reference {
                    return Ok(Evaluated {
                        value: Value::Undefined,
                        unit: UnitSlot::if_defined(unit.clone()),
                        missing: Missing::default(),
                    });
                }
                let mut missing = Missing::default();
                let mut applicable = Vec::new();
                for (&option, not_applicable) in options.iter().zip(not_applicable) {
                    let excluded = match not_applicable {
                        Some(node) => {
                            let evaluated = self.evaluate(*node)?;
                            missing = missing.merge(&evaluated.missing);
                            evaluated.value == Value::Bool(true)
                        }
                        None => false,
                    };
                    if !excluded {
                        applicable.push(option);
                    }
                }
                let value = match applicable.as_slice() {
                    [] => Value::Null,
                    [only] => match program.node(*only) {
                        Node::Reference(reference) => Value::String(reference.name.as_str().into()),
                        _ => Value::Undefined,
                    },
                    _ => Value::Undefined,
                };
                Ok(Evaluated {
                    value,
                    unit: UnitSlot::if_defined(unit.clone()),
                    missing,
                })
            }
            Node::Contexte { valeur, pairs, id } => self.contexte(*valeur, pairs, *id),
            Node::Inversion {
                rule,
                candidates,
                min,
                max,
                tolerance,
                rules,
            } => {
                let rules = rules.as_ref().ok_or_else(unprepared)?;
                self.inversion(*rule, candidates, (*min, *max, *tolerance), rules)
            }
            Node::ResoudreReferenceCirculaire {
                rule,
                valeur,
                rules,
            } => {
                let rules = rules.ok_or_else(unprepared)?;
                self.resoudre_reference_circulaire(*rule, *valeur, rules)
            }
            Node::SituationInput => Ok(self
                .state
                .inputs
                .get(&id)
                .cloned()
                .unwrap_or_else(|| Evaluated::new(Value::Undefined, Missing::default()))),
        }
    }
}

fn unprepared() -> Error {
    Error::new(ErrorKind::Internal, "Mécanisme évalué avant sa préparation")
}
