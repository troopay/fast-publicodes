use fast_publicodes_engine::{
    Engine, Error as EngineError, Missing, Options, SituationOptions, Value, Warning,
};
use napi::bindgen_prelude::{Either, External, JsObjectValue, Null, Object, Undefined};
use napi::{Env, Error, Result};
use napi_derive::napi;
use rustc_hash::FxHashMap;

fn parse_json(text: &str) -> Result<serde_json::Value> {
    serde_json::from_str(text).map_err(|error| Error::from_reason(error.to_string()))
}

fn error_json(error: &EngineError) -> serde_json::Value {
    serde_json::json!({
        "name": error.kind.name(),
        "message": error.to_string(),
        "dottedName": error.rule.clone().unwrap_or_default(),
    })
}

/// `{ error: { name, message, dottedName } }`, from which the package builds a
/// `PublicodesError`.
fn failure<'env>(env: &'env Env, error: &EngineError) -> Result<Object<'env>> {
    let mut result = Object::new(env)?;
    result.set_named_property("error", error_json(error).to_string())?;
    Ok(result)
}

/// How many distinct warnings an engine remembers having sent.
const SENT_WARNINGS: usize = 4096;

/// The engine, taking rules, options and situations as JSON.
///
/// Calls report the warnings they raised as `warnings`: the text of a warning not sent before,
/// which takes the next index in the table the caller keeps, or the index of one already sent.
/// `warningsReset` asks the caller to empty its table first.
#[napi(js_name = "Engine")]
pub struct NativeEngine {
    engine: Engine,
    sent_warnings: FxHashMap<Warning, u32>,
}

#[napi]
impl NativeEngine {
    /// `today` is the date `durée` defaults to, as `dd/mm/yyyy`. Fails with the error as
    /// JSON `{ name, message, dottedName }`.
    #[napi(constructor)]
    pub fn new(rules: String, today: String, options: String) -> Result<Self> {
        let options = Options::from_json(&parse_json(&options)?);
        let engine = Engine::new(&parse_json(&rules)?, &today, options)
            .map_err(|error| Error::from_reason(error_json(&error).to_string()))?;
        Ok(Self {
            engine,
            sent_warnings: FxHashMap::default(),
        })
    }

    /// Returns `{ accepted }`, the names the situation gains, or `{ error, situationReset,
    /// accepted }`.
    #[napi]
    pub fn set_situation<'env>(
        &mut self,
        env: &'env Env,
        situation: String,
        keep_previous: bool,
        strict: bool,
    ) -> Result<Object<'env>> {
        let options = SituationOptions {
            keep_previous,
            strict,
        };
        let mut result = match self.engine.set_situation(&parse_json(&situation)?, options) {
            Ok(accepted) => {
                let mut result = Object::new(env)?;
                result.set_named_property("accepted", accepted)?;
                result
            }
            Err(failed) => {
                let mut result = failure(env, &failed.error)?;
                result.set_named_property("situationReset", failed.situation_reset)?;
                result.set_named_property("accepted", failed.accepted)?;
                result
            }
        };
        self.attach_warnings(&mut result)?;
        Ok(result)
    }

    /// Returns `{ nodeValue, unit?, missing }`, where `missing` is a handle for
    /// `missingVariables`, or `{ error }`. `expression` is JSON when `json`.
    #[napi]
    pub fn evaluate<'env>(
        &mut self,
        env: &'env Env,
        expression: String,
        json: bool,
    ) -> Result<Object<'env>> {
        let value = if json {
            parse_json(&expression)?
        } else {
            serde_json::Value::String(expression)
        };
        let evaluation = match self.engine.evaluate(&value) {
            Ok(evaluation) => evaluation,
            Err(error) => {
                let mut result = failure(env, &error)?;
                self.attach_warnings(&mut result)?;
                return Ok(result);
            }
        };
        let mut result = Object::new(env)?;
        self.attach_warnings(&mut result)?;
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

    /// Returns the warnings raised by building the engine.
    #[napi]
    pub fn take_warnings<'env>(&mut self, env: &'env Env) -> Result<Object<'env>> {
        let mut result = Object::new(env)?;
        self.attach_warnings(&mut result)?;
        Ok(result)
    }

    fn attach_warnings(&mut self, result: &mut Object) -> Result<()> {
        let warnings = self.engine.take_warnings();
        if warnings.is_empty() {
            return Ok(());
        }
        if self.sent_warnings.len() + warnings.len() > SENT_WARNINGS {
            self.sent_warnings.clear();
            result.set_named_property("warningsReset", true)?;
        }
        let mut sent: Vec<Either<u32, String>> = Vec::with_capacity(warnings.len());
        for warning in warnings {
            if let Some(&index) = self.sent_warnings.get(&warning) {
                sent.push(Either::A(index));
            } else {
                sent.push(Either::B(warning.to_string()));
                let index = self.sent_warnings.len() as u32;
                self.sent_warnings.insert(warning, index);
            }
        }
        result.set_named_property("warnings", sent)?;
        Ok(())
    }

    #[napi]
    pub fn inversion_fail(&self) -> bool {
        self.engine.inversion_fail()
    }

    #[napi]
    pub fn reset_cache(&mut self) {
        self.engine.reset_cache();
    }

    #[napi]
    pub fn shallow_copy(&self, copy_cache: bool) -> NativeEngine {
        NativeEngine {
            engine: self.engine.shallow_copy(copy_cache),
            sent_warnings: self.sent_warnings.clone(),
        }
    }

    /// The names of the public rules, in definition order.
    #[napi]
    pub fn rule_names(&self) -> Vec<String> {
        self.engine.rule_names().map(str::to_owned).collect()
    }

    /// Returns `{ rule }`, the rule node as JSON, or `{ error }`.
    #[napi]
    pub fn rule<'env>(&self, env: &'env Env, name: String) -> Result<Object<'env>> {
        match self.engine.rule(&name) {
            Ok(rule) => {
                let mut result = Object::new(env)?;
                result.set_named_property("rule", rule.to_string())?;
                Ok(result)
            }
            Err(error) => failure(env, &error),
        }
    }

    /// Returns `{ possibilities }`, as JSON, or `{ error }`.
    #[napi]
    pub fn possibilities<'env>(
        &mut self,
        env: &'env Env,
        name: String,
        filter_not_applicable: bool,
    ) -> Result<Object<'env>> {
        let mut result = match self.engine.possibilities(&name, filter_not_applicable) {
            Ok(possibilities) => {
                let mut result = Object::new(env)?;
                result.set_named_property("possibilities", possibilities.to_string())?;
                result
            }
            Err(error) => failure(env, &error)?,
        };
        self.attach_warnings(&mut result)?;
        Ok(result)
    }
}

/// `parseExpression`: returns `{ expression }`, as JSON, or `{ error }`.
#[napi]
pub fn parse_expression<'env>(
    env: &'env Env,
    expression: String,
    dotted_name: String,
) -> Result<Object<'env>> {
    match fast_publicodes_engine::parse_expression(&expression, &dotted_name) {
        Ok(tree) => {
            let mut result = Object::new(env)?;
            result.set_named_property("expression", tree.to_string())?;
            Ok(result)
        }
        Err(error) => failure(env, &error),
    }
}
