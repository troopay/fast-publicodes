//! `contexte`, `inversion numérique` and `résoudre la référence circulaire`, which evaluate in
//! sub-engines with a modified situation.

use rustc_hash::FxHashMap;

use super::js_value::{compare, to_number};
use super::{EvalState, Evaluated, Evaluator, Missing, UnitSlot};
use crate::ast::{
    ContextePair, InversionRules, NameId, Node, NodeId, PossibilityKind, Program, Value,
};
use crate::error::{Error, ErrorKind, Result};
use crate::units::serialize_unit;

fn target(program: &Program, reference: NodeId) -> Result<NameId> {
    program.target(reference).ok_or_else(unresolved)
}

fn unresolved() -> Error {
    Error::new(
        ErrorKind::Internal,
        "Le mécanisme attend une référence à une règle",
    )
}

fn situation_name(program: &Program, rule: NameId) -> Option<NameId> {
    program
        .names
        .get(&format!("{} . $SITUATION", program.names.name(rule)))
}

fn greater(a: &Value, b: &Value) -> bool {
    compare(a, b).is_some_and(|o| o.is_gt())
}

fn less(a: &Value, b: &Value) -> bool {
    compare(a, b).is_some_and(|o| o.is_lt())
}

/// A sub-engine evaluating `goal` with a rule's situation set to successive values.
struct Iteration<'p> {
    program: &'p Program,
    state: EvalState,
    situation: NameId,
    input_rule: NodeId,
    input: NodeId,
    unit: UnitSlot,
    goal: NodeId,
    last: Option<Evaluated>,
}

impl Iteration<'_> {
    fn evaluate(&mut self, n: f64) -> Result<Evaluated> {
        self.state.clear();
        if let Some(layer) = self.state.layers.last_mut() {
            layer.insert(self.situation, self.input_rule);
        }
        let input = Evaluated {
            value: Value::Number(n),
            unit: self.unit.clone(),
            missing: Missing::default(),
        };
        self.state.inputs.insert(self.input, input);
        let evaluated = Evaluator::new(self.program, &mut self.state).evaluate(self.goal)?;
        self.last = Some(evaluated.clone());
        Ok(evaluated)
    }
}

/// Brent's method: a root of `f` between `lower` and `upper`.
fn uniroot(
    f: &mut dyn FnMut(f64) -> Result<f64>,
    lower: f64,
    upper: f64,
    error_tolerance: f64,
    mut max_iterations: f64,
    acceptable_tolerance: f64,
) -> Result<Option<f64>> {
    let (mut a, mut b) = (lower, upper);
    let mut c = a;
    let mut fa = f(a)?;
    let mut fb = f(b)?;
    let mut fc = fa;
    let mut fallback = None;
    while max_iterations > 0. {
        max_iterations -= 1.;
        let previous_step = b - a;
        if fc.abs() < fb.abs() {
            (a, b, c) = (b, c, b);
            (fa, fb, fc) = (fb, fc, fb);
        }
        let tolerance = 1e-15 * b.abs() + error_tolerance / 2.;
        let mut step = (c - b) / 2.;
        if step.abs() <= tolerance || fb == 0. {
            return Ok(Some(b));
        }
        if previous_step.abs() >= tolerance && fa.abs() > fb.abs() {
            let cb = c - b;
            let (mut p, mut q);
            if a == c {
                let t1 = fb / fa;
                p = cb * t1;
                q = 1. - t1;
            } else {
                let (q0, t1, t2) = (fa / fc, fb / fc, fb / fa);
                p = t2 * (cb * q0 * (q0 - t1) - (b - a) * (t1 - 1.));
                q = (q0 - 1.) * (t1 - 1.) * (t2 - 1.);
            }
            if p > 0. {
                q = -q;
            } else {
                p = -p;
            }
            if p < 0.75 * cb * q - (tolerance * q).abs() / 2. && p < (previous_step * q / 2.).abs()
            {
                step = p / q;
            }
        }
        if step.abs() < tolerance {
            step = if step > 0. { tolerance } else { -tolerance };
        }
        (a, fa) = (b, fb);
        b += step;
        fb = f(b)?;
        if (fb > 0. && fc > 0.) || (fb < 0. && fc < 0.) {
            (c, fc) = (a, fa);
        }
        if fb.abs() < error_tolerance {
            return Ok(Some(b));
        }
        if fb.abs() < acceptable_tolerance {
            fallback = Some(b);
        }
    }
    Ok(fallback)
}

impl Evaluator<'_> {
    fn rule_on_stack_below_top(&self, rule: NameId) -> bool {
        let stack = &self.state.rule_stack;
        stack[..stack.len().saturating_sub(1)].contains(&rule)
    }

    /// A sub-engine of this one; it copies the evaluation stacks when `with_stacks`.
    fn fork(&self, copy_cache: bool, with_stacks: bool) -> EvalState {
        let mut state = self.state.fork(copy_cache);
        if with_stacks {
            state.rule_stack = self.state.rule_stack.clone();
            state.parent_stack = self.state.parent_stack.clone();
        }
        state
    }

    pub(super) fn contexte(
        &mut self,
        valeur: NodeId,
        pairs: &[ContextePair],
        id: u64,
    ) -> Result<Evaluated> {
        let program = self.program;
        let mut undefined_origins = Vec::new();
        let mut amended = Vec::new();
        for pair in pairs {
            let origin = self.evaluate(pair.origin)?;
            let replacement = self.evaluate(pair.replacement)?;
            let name = target(program, pair.origin)?;
            if replacement.value == Value::Undefined {
                undefined_origins.push(name);
            }
            if origin.value != replacement.value
                || serialize_unit(origin.unit.unit()) != serialize_unit(replacement.unit.unit())
            {
                amended.push((name, pair));
            }
        }
        let situation: Vec<(NameId, u64)> = amended
            .iter()
            .map(|(name, pair)| (*name, pair.replacement_key))
            .collect();
        if self.state.contexte_situation.as_ref() == Some(&situation) {
            return Ok(Evaluated::new(Value::Null, Missing::default()));
        }
        let mut sub = match self.state.sub_engines.remove(&id) {
            Some(sub) => sub,
            None => self.contexte_engine(&amended)?,
        };
        sub.contexte_situation = Some(situation);
        let evaluated = Evaluator::new(program, &mut sub).evaluate(valeur);
        self.state.sub_engines.insert(id, sub);
        let evaluated = evaluated?;
        let origins: Vec<NameId> = pairs
            .iter()
            .filter_map(|p| target(program, p.origin).ok())
            .collect();
        let missing = evaluated
            .missing
            .retain(|name| !origins.contains(&name) || undefined_origins.contains(&name));
        Ok(Evaluated {
            missing,
            ..evaluated
        })
    }

    fn contexte_engine(&mut self, amended: &[(NameId, &ContextePair)]) -> Result<EvalState> {
        let program = self.program;
        let mut sub = self.fork(amended.is_empty(), false);
        if amended.is_empty() {
            return Ok(sub);
        }
        let mut layer = FxHashMap::default();
        for (name, pair) in amended {
            let situation = situation_name(program, *name).ok_or_else(unresolved)?;
            layer.insert(situation, pair.situation.ok_or_else(unresolved)?.0);
        }
        sub.layers.push(layer);
        for (name, pair) in amended {
            let Some(possibilities) = program.rule(*name).and_then(|rule| rule.possibilities)
            else {
                continue;
            };
            let Some((_, Some(checked))) = pair.situation else {
                continue;
            };
            let value = Evaluator::new(program, &mut sub).evaluate(checked)?;
            sub.cache.insert(pair.replacement, value.clone());
            let reference = matches!(
                program.node(possibilities),
                Node::UnePossibilite {
                    kind: PossibilityKind::Reference,
                    ..
                }
            );
            if reference && !matches!(value.value, Value::Null | Value::Undefined) {
                Evaluator::new(program, &mut sub).evaluate(possibilities)?;
            }
        }
        for (_, pair) in amended {
            let Some((rule, _)) = pair.situation else {
                continue;
            };
            if let (Node::Rule(rule), Some(evaluated)) =
                (program.node(rule), self.cached(pair.replacement))
            {
                sub.cache.insert(rule.valeur, evaluated.clone());
            }
        }
        Ok(sub)
    }

    /// The first candidate known as a number in the situation, with its evaluation.
    fn inversion_goal(
        &self,
        state: &mut EvalState,
        candidates: &[NodeId],
    ) -> Result<Option<(usize, NodeId, NameId, Evaluated)>> {
        let program = self.program;
        for (index, &candidate) in candidates.iter().enumerate() {
            let name = target(program, candidate)?;
            if self.state.rule_stack.contains(&name) {
                continue;
            }
            let mut evaluator = Evaluator::new(program, state);
            let situation = situation_name(program, name)
                .and_then(|situation| evaluator.rule_node(situation))
                .ok_or_else(unresolved)?;
            let evaluation = evaluator.evaluate(situation)?;
            if matches!(evaluation.value, Value::Number(_)) && !evaluation.missing.contains(name) {
                let evaluated_goal = evaluator.evaluate(candidate)?;
                return Ok(Some((index, candidate, name, evaluated_goal)));
            }
        }
        Ok(None)
    }

    pub(super) fn inversion(
        &mut self,
        rule: NameId,
        candidates: &[NodeId],
        (min, max, tolerance): (f64, f64, f64),
        rules: &InversionRules,
    ) -> Result<Evaluated> {
        let program = self.program;
        if self.rule_on_stack_below_top(rule) {
            return Ok(Evaluated::new(Value::Undefined, Missing::default()));
        }
        // Like a copy of this cache, but cheaper: this engine waits while the goal is sought.
        let mut state = self.fork(false, true);
        state.cache = self.state.cache.lend();
        let goal = self.inversion_goal(&mut state, candidates);
        let mut lent = std::mem::take(&mut state.cache);
        self.state.cache.restore(&mut lent);
        let Some((index, goal, goal_name, evaluated_goal)) = goal? else {
            let mut missing = Missing::default();
            for &candidate in candidates {
                missing = missing.with(target(program, candidate)?, 1.);
            }
            return Ok(Evaluated::new(Value::Undefined, missing.with(rule, 1.)));
        };
        state.clear();
        let goal_situation = situation_name(program, goal_name).ok_or_else(unresolved)?;
        let goal_rule = rules.goal_rules[index].ok_or_else(unresolved)?;
        state
            .layers
            .push(FxHashMap::from_iter([(goal_situation, goal_rule)]));
        let mut iteration = Iteration {
            program,
            state,
            situation: situation_name(program, rule).ok_or_else(unresolved)?,
            input_rule: rules.input_rule,
            input: rules.input,
            unit: UnitSlot::present(evaluated_goal.unit.get().cloned()),
            goal,
            last: None,
        };
        let goal_value = Value::Number(to_number(&evaluated_goal.value));
        let goal = to_number(&evaluated_goal.value);
        let x1 = goal;
        let y1 = iteration.evaluate(x1)?.value;
        let coefficient = if greater(&y1, &goal_value) { 0.9 } else { 1.2 };
        let x2 = if y1 == Value::Undefined {
            2000.
        } else {
            x1 * goal * coefficient / to_number(&y1)
        };
        let y2 = iteration.evaluate(x2)?.value;
        let mut value = None;
        if y1 != Value::Undefined || y2 != Value::Undefined {
            let below = if y2 != Value::Undefined
                && less(&y2, &goal_value)
                && (greater(&y2, &y1) || greater(&y1, &goal_value))
            {
                x2
            } else if y1 != Value::Undefined
                && less(&y1, &goal_value)
                && (greater(&y1, &y2) || greater(&y2, &goal_value))
            {
                x1
            } else {
                min
            };
            let above = if y2 != Value::Undefined
                && greater(&y2, &goal_value)
                && (less(&y2, &y1) || less(&y1, &goal_value))
            {
                x2
            } else if y1 != Value::Undefined
                && greater(&y1, &goal_value)
                && (less(&y1, &y2) || less(&y2, &goal_value))
            {
                x1
            } else {
                max
            };
            let mut test = |x: f64| -> Result<f64> {
                let y = if x == x1 {
                    y1.clone()
                } else if x == x2 {
                    y2.clone()
                } else {
                    iteration.evaluate(x)?.value
                };
                Ok(to_number(&y) - goal)
            };
            let iterations = program.options.inversion_max_iterations.unwrap_or(10.);
            value = uniroot(&mut test, below, above, tolerance, iterations, 1.)?;
            if let Some(v) = value
                && v != 0.
                && !v.is_nan()
                && (v < min || v > max)
            {
                value = None;
            }
        }
        if value.is_none() {
            self.state.inversion_fail = true;
        }
        Ok(Evaluated {
            value: value.map_or(Value::Undefined, Value::Number),
            unit: UnitSlot::present(evaluated_goal.unit.get().cloned()),
            missing: iteration.last.map(|last| last.missing).unwrap_or_default(),
        })
    }

    pub(super) fn resoudre_reference_circulaire(
        &mut self,
        rule: NameId,
        valeur: NodeId,
        (input_rule, input): (NodeId, NodeId),
    ) -> Result<Evaluated> {
        let program = self.program;
        if self.rule_on_stack_below_top(rule) {
            return Ok(Evaluated::new(Value::Undefined, Missing::default()));
        }
        let mut state = self.fork(false, true);
        state.layers.push(FxHashMap::default());
        let mut iteration = Iteration {
            program,
            state,
            situation: situation_name(program, rule).ok_or_else(unresolved)?,
            input_rule,
            input,
            unit: UnitSlot::Absent,
            goal: valeur,
            last: None,
        };
        let first = iteration.evaluate(1.)?;
        let (y0, unit) = (first.value, first.unit);
        let value = if y0 == Value::Undefined {
            self.state.inversion_fail = true;
            None
        } else {
            let mut test = |x: f64| -> Result<f64> {
                if x == 1. {
                    return Ok(to_number(&y0) - 1.);
                }
                Ok(to_number(&iteration.evaluate(x)?.value) - x)
            };
            let iterations = program.options.inversion_max_iterations.unwrap_or(25.);
            uniroot(&mut test, -1_000_000., 100_000_000., 0.5, iterations, 2.)?
        };
        if let Some(value) = value {
            iteration.evaluate(value)?;
        }
        Ok(Evaluated {
            value: value.map_or(Value::Undefined, Value::Number),
            unit: UnitSlot::present(unit.get().cloned()),
            missing: iteration.last.map(|last| last.missing).unwrap_or_default(),
        })
    }
}
