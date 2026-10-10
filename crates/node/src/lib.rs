use fast_publicodes_engine::{Engine, Missing, Value};
use napi::bindgen_prelude::{External, JsObjectValue, Null, Object, Undefined};
use napi::{Env, Error, Result};
use napi_derive::napi;

fn parse_json(text: &str) -> Result<serde_json::Value> {
    serde_json::from_str(text).map_err(|error| Error::from_reason(error.to_string()))
}

/// The engine, taking rules and situations as JSON.
#[napi(js_name = "Engine")]
pub struct NativeEngine {
    engine: Engine,
}

#[napi]
impl NativeEngine {
    #[napi(constructor)]
    pub fn new(rules: String) -> Result<Self> {
        let engine = Engine::new(&parse_json(&rules)?)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(Self { engine })
    }

    /// Returns the error message, if any.
    #[napi]
    pub fn set_situation(&mut self, situation: String) -> Result<Option<String>> {
        let situation = parse_json(&situation)?;
        Ok(self
            .engine
            .set_situation(&situation)
            .err()
            .map(|e| e.to_string()))
    }

    /// Returns `{ nodeValue, unit?, missing }`, where `missing` is a handle for
    /// `missingVariables`, or `{ error }`.
    #[napi]
    pub fn evaluate<'env>(&mut self, env: &'env Env, expression: String) -> Result<Object<'env>> {
        let mut result = Object::new(env)?;
        let evaluation = match self.engine.evaluate(&expression) {
            Ok(evaluation) => evaluation,
            Err(error) => {
                result.set_named_property("error", error.to_string())?;
                return Ok(result);
            }
        };
        match evaluation.value {
            Value::Undefined => result.set_named_property("nodeValue", Undefined::default())?,
            Value::Null => result.set_named_property("nodeValue", Null)?,
            Value::Bool(b) => result.set_named_property("nodeValue", b)?,
            Value::Number(n) => result.set_named_property("nodeValue", n)?,
            Value::String(s) => result.set_named_property("nodeValue", s.as_ref())?,
        }
        if let Some(unit) = evaluation.unit {
            let mut js_unit = Object::new(env)?;
            js_unit.set_named_property("numerators", unit.numerators)?;
            js_unit.set_named_property("denominators", unit.denominators)?;
            result.set_named_property("unit", js_unit)?;
        }
        result.set_named_property("missing", External::new(evaluation.missing))?;
        Ok(result)
    }

    #[napi(ts_args_type = "missing: unknown")]
    pub fn missing_variables<'env>(
        &self,
        env: &'env Env,
        missing: &External<Missing>,
    ) -> Result<Object<'env>> {
        let mut result = Object::new(env)?;
        for (name, score) in self.engine.missing_variables(missing) {
            result.set_named_property(name, score)?;
        }
        Ok(result)
    }
}
