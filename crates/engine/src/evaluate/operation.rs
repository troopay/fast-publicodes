//! Operations between two values.

use super::js_value::{compare, power, strict_equals, to_js_string, to_number, truthy};
use super::{Evaluated, Evaluator, UnitSlot};
use crate::ast::{NodeId, Operator, Value};
use crate::date::{is_date_string, timestamp};
use crate::error::{Error, Result};
use crate::units::{Unit, UnitOperator, infer_unit, serialize_unit};

/// An operand: a node, or a value built by the engine.
pub enum Operand {
    Node(NodeId),
    Constant(Evaluated),
}

fn is_zero(value: &Value) -> bool {
    matches!(value, Value::Number(n) if *n == 0.)
}

fn unit_operator(operator: Operator) -> Option<UnitOperator> {
    match operator {
        Operator::Times => Some(UnitOperator::Times),
        Operator::Divide => Some(UnitOperator::Divide),
        Operator::IntegerDivide => Some(UnitOperator::IntegerDivide),
        Operator::Plus => Some(UnitOperator::Plus),
        Operator::Minus => Some(UnitOperator::Minus),
        _ => None,
    }
}

fn per_percent() -> Unit {
    Unit {
        numerators: Vec::new(),
        denominators: vec!["%".to_owned()],
    }
}

/// The JS operator applied to two defined values.
fn apply(operator: Operator, a: &Value, b: &Value) -> Value {
    use Operator::*;
    let nullish = |v: &Value| match v {
        Value::Null | Value::Undefined => Value::Bool(false),
        v => v.clone(),
    };
    match operator {
        Times => Value::Number(to_number(a) * to_number(b)),
        Divide => Value::Number(to_number(a) / to_number(b)),
        IntegerDivide => {
            let (a, b) = (to_number(a), to_number(b));
            Value::Number((a - a % b) / b)
        }
        Power => Value::Number(power(to_number(a), to_number(b))),
        Plus => match (a, b) {
            (Value::String(_), _) | (_, Value::String(_)) => {
                Value::String(format!("{}{}", to_js_string(a), to_js_string(b)).into())
            }
            _ => Value::Number(to_number(a) + to_number(b)),
        },
        Minus => Value::Number(to_number(a) - to_number(b)),
        Less => Value::Bool(compare(a, b).is_some_and(|o| o.is_lt())),
        LessOrEqual => Value::Bool(compare(a, b).is_some_and(|o| o.is_le())),
        Greater => Value::Bool(compare(a, b).is_some_and(|o| o.is_gt())),
        GreaterOrEqual => Value::Bool(compare(a, b).is_some_and(|o| o.is_ge())),
        Equal => Value::Bool(strict_equals(&nullish(a), &nullish(b))),
        NotEqual => Value::Bool(!strict_equals(&nullish(a), &nullish(b))),
        And => {
            let a = nullish(a);
            if truthy(&a) { nullish(b) } else { a }
        }
        Or => {
            let a = nullish(a);
            if truthy(&a) { a } else { nullish(b) }
        }
    }
}

impl Evaluator<'_> {
    fn operand(&mut self, operand: Operand) -> Result<Evaluated> {
        match operand {
            Operand::Node(id) => self.evaluate(id),
            Operand::Constant(evaluated) => Ok(evaluated),
        }
    }

    pub fn operation(
        &mut self,
        operator: Operator,
        left: Operand,
        right: Operand,
    ) -> Result<Evaluated> {
        use Operator::*;
        let mut left = self.operand(left)?;
        let short_circuits =
            |value: &Value, null_operators: &[Operator], zero_operators: &[Operator]| {
                (*value == Value::Null && null_operators.contains(&operator))
                    || (is_zero(value) && zero_operators.contains(&operator))
                    || (*value == Value::Bool(false) && operator == And)
                    || (*value == Value::Bool(true) && operator == Or)
            };
        let early = |evaluated: Evaluated| {
            let value = if operator == And {
                Value::Bool(false)
            } else {
                evaluated.value
            };
            Ok(Evaluated::new(value, evaluated.missing))
        };
        if short_circuits(
            &left.value,
            &[
                Less,
                Greater,
                LessOrEqual,
                GreaterOrEqual,
                Divide,
                Times,
                IntegerDivide,
                Minus,
                And,
            ],
            &[Divide, Times],
        ) {
            return early(left);
        }
        let mut right = self.operand(right)?;
        if operator == Divide && is_zero(&right.value) {
            return Err(Error::evaluation("Division by zero"));
        }
        if short_circuits(
            &right.value,
            &[
                Less,
                Greater,
                LessOrEqual,
                GreaterOrEqual,
                Divide,
                Times,
                IntegerDivide,
                And,
            ],
            &[Times],
        ) {
            return early(right);
        }
        let missing = left.missing.merge(&right.missing);
        let undefined = left.value == Value::Undefined || right.value == Value::Undefined;
        let with_percentage = matches!(operator, Plus | Minus)
            && serialize_unit(right.unit.get()).as_deref() == Some("%")
            && serialize_unit(left.unit.get()).as_deref() != Some("%");
        if !undefined && !matches!(operator, Divide | Times | IntegerDivide) && !with_percentage {
            if let Some(unit) = left.unit.get().cloned() {
                if !matches!(right.unit, UnitSlot::Absent)
                    && let Ok(converted) = self.convert_node(Some(&unit), &right)
                {
                    right = converted;
                }
            } else if let Some(unit) = right.unit.get().cloned()
                && let Ok(converted) = self.convert_node(Some(&unit), &left)
            {
                left = converted;
            }
        }
        let mut value = if undefined {
            Value::Undefined
        } else if let (Value::String(a), Value::String(b)) = (&left.value, &right.value)
            && is_date_string(a)
            && is_date_string(b)
        {
            apply(
                operator,
                &Value::Number(timestamp(a)?),
                &Value::Number(timestamp(b)?),
            )
        } else {
            apply(operator, &left.value, &right.value)
        };
        let units = [left.unit.get(), right.unit.get()];
        if operator == Times
            && let Some(unit) = infer_unit(UnitOperator::Times, units)
                .filter(|u| u.numerators.iter().any(|n| n == "%"))
        {
            if let Value::Number(n) = value {
                value = Value::Number(n / 100.);
            }
            let unit = infer_unit(UnitOperator::Times, [Some(&unit), Some(&per_percent())]);
            return Ok(Evaluated {
                value,
                unit: UnitSlot::present(unit),
                missing,
            });
        }
        if with_percentage {
            let unit = infer_unit(UnitOperator::Times, units);
            if let (Value::Number(a), Value::Number(b)) = (&left.value, &right.value) {
                let sign = if operator == Minus { -1. } else { 1. };
                value = Value::Number(a * (1. + (b / 100.) * sign));
            }
            let unit = infer_unit(UnitOperator::Times, [unit.as_ref(), Some(&per_percent())]);
            return Ok(Evaluated {
                value,
                unit: UnitSlot::present(unit),
                missing,
            });
        }
        let unit = match unit_operator(operator) {
            Some(unit_operator) => UnitSlot::present(infer_unit(unit_operator, units)),
            None => UnitSlot::Absent,
        };
        Ok(Evaluated {
            value,
            unit,
            missing,
        })
    }
}
