//! The engine for WebAssembly, with the interface of the Node addon.

use fast_publicodes_engine::{
    Engine, Error as EngineError, Evaluations, Options, SituationOptions, Value,
};
use fast_publicodes_js::{
    Sent, Session, UnitKeyResult, error_json, expression, failure_json, parse_json, today, unit_key,
};
use js_sys::{Array, Function, Object, Reflect};
use wasm_bindgen::prelude::*;

type Result<T> = std::result::Result<T, JsError>;

fn json(text: &str) -> Result<serde_json::Value> {
    parse_json(text).map_err(|reason| JsError::new(&reason))
}

fn set(object: &Object, key: &str, value: impl Into<JsValue>) {
    Reflect::set(object, &key.into(), &value.into()).expect("objects accept properties");
}

/// `{ error }`, the error as JSON `{ name, message, dottedName }`.
fn failure(error: &EngineError) -> Object {
    let result = Object::new();
    set(&result, "error", error_json(error));
    result
}

fn strings(values: Vec<String>) -> Array {
    values.into_iter().map(JsValue::from).collect()
}

fn reason(thrown: JsValue) -> String {
    thrown
        .as_string()
        .or_else(|| js_sys::Error::from(thrown).message().as_string())
        .unwrap_or_default()
}

/// The expressions evaluated since the cache was last reset, for `missingVariables`.
#[wasm_bindgen]
pub struct EvaluationsHandle(Evaluations);

/// The engine, taking rules, options and situations as JSON.
///
/// Calls report the warnings they raised as `warnings`: the text of a warning not sent before,
/// which takes the next index in the table the caller keeps, or the index of one already sent.
/// `warningsReset` asks the caller to empty its table first.
#[wasm_bindgen(js_name = Engine)]
pub struct WasmEngine {
    session: Session,
}

#[wasm_bindgen(js_class = Engine)]
impl WasmEngine {
    /// `get_today` gives the date `durée` defaults to, as `dd/mm/yyyy`. Fails with the error as
    /// JSON `{ name, message, dottedName, warnings }`.
    #[wasm_bindgen(constructor)]
    pub fn new(
        rules: &str,
        options: &str,
        get_today: Function,
        get_unit_key: Option<Function>,
    ) -> Result<WasmEngine> {
        let options = Options::from_json(&json(options)?);
        let unit_key = get_unit_key.map(|function| {
            unit_key(move |unit| {
                let returned = function
                    .call1(&JsValue::NULL, &unit.into())
                    .map_err(reason)?;
                Ok(match returned.as_string() {
                    Some(key) => UnitKeyResult::Key(key),
                    None => match Reflect::get(&returned, &"message".into())
                        .ok()
                        .and_then(|message| message.as_string())
                    {
                        Some(message) => UnitKeyResult::Error(message),
                        None => UnitKeyResult::Thrown,
                    },
                })
            })
        });
        let today = today(move || {
            let date = get_today.call0(&JsValue::NULL).map_err(reason)?;
            date.as_string()
                .ok_or_else(|| "La date du jour n'est pas une chaîne".to_owned())
        });
        let engine = Engine::new(&json(rules)?, options, today, unit_key)
            .map_err(|failure| JsError::new(&failure_json(&failure)))?;
        Ok(WasmEngine {
            session: Session::new(engine),
        })
    }

    /// Returns `{ accepted }`, the names the situation gains, or `{ error, situationReset,
    /// accepted }`.
    #[wasm_bindgen(js_name = setSituation)]
    pub fn set_situation(
        &mut self,
        situation: &str,
        keep_previous: bool,
        strict: bool,
    ) -> Result<Object> {
        let options = SituationOptions {
            keep_previous,
            strict,
        };
        let result = match self
            .session
            .engine
            .set_situation(&json(situation)?, options)
        {
            Ok(accepted) => {
                let result = Object::new();
                set(&result, "accepted", strings(accepted));
                result
            }
            Err(failed) => {
                let result = failure(&failed.error);
                set(&result, "situationReset", failed.situation_reset);
                set(&result, "accepted", strings(failed.accepted));
                result
            }
        };
        self.attach_warnings(&result);
        Ok(result)
    }

    /// Returns `{ nodeValue, index }`, `index` locating it in `evaluations`, with `unit` the index
    /// of its unit in the table the caller keeps, or `newUnit` when it is new and takes the next
    /// index, after `unitsReset` asks the caller to empty its table; or `{ error }`. `text` is
    /// JSON when `json`.
    pub fn evaluate(&mut self, text: String, json: bool) -> Result<Object> {
        let value = expression(text, json).map_err(|reason| JsError::new(&reason))?;
        let evaluation = match self.session.engine.evaluate(&value) {
            Ok(evaluation) => evaluation,
            Err(error) => {
                let result = failure(&error);
                self.attach_warnings(&result);
                return Ok(result);
            }
        };
        let result = Object::new();
        self.attach_warnings(&result);
        let node_value = match evaluation.value {
            Value::Undefined => JsValue::UNDEFINED,
            Value::Null => JsValue::NULL,
            Value::Bool(b) => b.into(),
            Value::Number(n) => n.into(),
            Value::String(s) => JsValue::from_str(&s),
        };
        set(&result, "nodeValue", node_value);
        set(&result, "index", evaluation.index);
        if let Some(unit) = evaluation.unit {
            let sent = self.session.send_unit(unit);
            if sent.reset {
                set(&result, "unitsReset", true);
            }
            match sent.unit {
                Sent::Index(index) => set(&result, "unit", index),
                Sent::New(unit) => {
                    let js_unit = Object::new();
                    set(&js_unit, "numerators", strings(unit.numerators.clone()));
                    set(&js_unit, "denominators", strings(unit.denominators.clone()));
                    set(&result, "newUnit", js_unit);
                }
            }
        }
        Ok(result)
    }

    /// A handle on the expressions evaluated since the cache was last reset, for
    /// `missingVariables`.
    pub fn evaluations(&self) -> EvaluationsHandle {
        EvaluationsHandle(self.session.engine.evaluations())
    }

    /// The missing variables of the evaluation at `index` in `evaluations`.
    #[wasm_bindgen(js_name = missingVariables)]
    pub fn missing_variables(&self, evaluations: &EvaluationsHandle, index: u32) -> Object {
        let missing = evaluations.0.missing(index).unwrap_or_default();
        let result = Object::new();
        for (name, score) in self.session.engine.missing_variables(&missing) {
            set(&result, name, score);
        }
        result
    }

    /// Returns the warnings raised by building the engine.
    #[wasm_bindgen(js_name = takeWarnings)]
    pub fn take_warnings(&mut self) -> Object {
        let result = Object::new();
        self.attach_warnings(&result);
        result
    }

    fn attach_warnings(&mut self, result: &Object) {
        let Some(sent) = self.session.take_warnings() else {
            return;
        };
        if sent.reset {
            set(result, "warningsReset", true);
        }
        let warnings: Array = sent
            .warnings
            .into_iter()
            .map(|warning| match warning {
                Sent::Index(index) => JsValue::from(index),
                Sent::New(text) => JsValue::from(text),
            })
            .collect();
        set(result, "warnings", warnings);
    }

    #[wasm_bindgen(js_name = inversionFail)]
    pub fn inversion_fail(&self) -> bool {
        self.session.engine.inversion_fail()
    }

    #[wasm_bindgen(js_name = resetCache)]
    pub fn reset_cache(&mut self) {
        self.session.engine.reset_cache();
    }

    #[wasm_bindgen(js_name = shallowCopy)]
    pub fn shallow_copy(&self, copy_cache: bool) -> WasmEngine {
        WasmEngine {
            session: self.session.shallow_copy(copy_cache),
        }
    }

    /// The names of the public rules, in definition order.
    #[wasm_bindgen(js_name = ruleNames)]
    pub fn rule_names(&self) -> Vec<String> {
        self.session
            .engine
            .rule_names()
            .map(str::to_owned)
            .collect()
    }

    /// Returns `{ rule }`, the rule node with its inferred type and the names of its suggestions,
    /// as JSON, or `{ error }`.
    pub fn rule(&self, name: &str) -> Object {
        match self.session.engine.rule(name) {
            Ok(rule) => {
                let result = Object::new();
                set(&result, "rule", rule.to_string());
                result
            }
            Err(error) => failure(&error),
        }
    }

    /// Returns `{ possibilities }`, as JSON, or `{ error }`.
    #[wasm_bindgen(js_name = applicablePossibilities)]
    pub fn applicable_possibilities(&mut self, name: &str) -> Object {
        let result = match self.session.engine.applicable_possibilities(name) {
            Ok(possibilities) => {
                let result = Object::new();
                set(&result, "possibilities", possibilities.to_string());
                result
            }
            Err(error) => failure(&error),
        };
        self.attach_warnings(&result);
        result
    }
}

/// `parseExpression`: returns `{ expression }`, as JSON, or `{ error }`.
#[wasm_bindgen(js_name = parseExpression)]
pub fn parse_expression(expression: &str, dotted_name: &str) -> Object {
    match fast_publicodes_engine::parse_expression(expression, dotted_name) {
        Ok(tree) => {
            let result = Object::new();
            set(&result, "expression", tree.to_string());
            result
        }
        Err(error) => failure(&error),
    }
}
