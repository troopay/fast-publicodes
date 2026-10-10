use fast_publicodes_engine::{Error as EngineError, Missing, Options, SituationOptions, Value};
use fast_publicodes_js::{
    SentWarning, Session, UnitKeyResult, error_json, parse_json, today, unit_key,
};
use napi::bindgen_prelude::{Either, External, Function, JsObjectValue, Null, Object, Undefined};
use napi::{Env, Error, Result};
use napi_derive::napi;

fn json(text: &str) -> Result<serde_json::Value> {
    parse_json(text).map_err(Error::from_reason)
}

/// `{ error }`, the error as JSON `{ name, message, dottedName }`.
fn failure<'env>(env: &'env Env, error: &EngineError) -> Result<Object<'env>> {
    let mut result = Object::new(env)?;
    result.set_named_property("error", error_json(error))?;
    Ok(result)
}

/// What `getUnitKey` returns: a key, or what it threw.
type UnitKeyReturn = Either<String, Thrown>;

/// A thrown value: an `Error` and its message, or another value, which the caller keeps.
#[napi(object)]
pub struct Thrown {
    pub message: Option<String>,
}

/// A JavaScript function the engine calls from within calls that come from JavaScript.
fn callback<Args, Return>(
    env: &Env,
    function: Function<Args, Return>,
) -> Result<impl Fn(Args) -> std::result::Result<Return, String> + 'static>
where
    Args: napi::bindgen_prelude::JsValuesTupleIntoVec + 'static,
    Return: napi::bindgen_prelude::FromNapiValue + 'static,
{
    let function = function.create_ref()?;
    let raw_env = env.raw();
    Ok(move |args| {
        let env = Env::from_raw(raw_env);
        function
            .borrow_back(&env)
            .and_then(|function| function.call(args))
            .map_err(|error| error.reason)
    })
}

/// The engine, taking rules, options and situations as JSON.
///
/// Calls report the warnings they raised as `warnings`: the text of a warning not sent before,
/// which takes the next index in the table the caller keeps, or the index of one already sent.
/// `warningsReset` asks the caller to empty its table first.
#[napi(js_name = "Engine")]
pub struct NativeEngine {
    session: Session,
}

#[napi]
impl NativeEngine {
    /// `get_today` gives the date `durée` defaults to, as `dd/mm/yyyy`. Fails with the error as
    /// JSON `{ name, message, dottedName }`.
    #[napi(constructor)]
    pub fn new(
        env: &Env,
        rules: String,
        options: String,
        get_today: Function<(), String>,
        get_unit_key: Option<Function<String, UnitKeyReturn>>,
    ) -> Result<Self> {
        let options = Options::from_json(&json(&options)?);
        let unit_key = match get_unit_key {
            Some(function) => {
                let call = callback(env, function)?;
                Some(unit_key(move |unit| {
                    call(unit.to_owned()).map(|returned| match returned {
                        Either::A(key) => UnitKeyResult::Key(key),
                        Either::B(Thrown {
                            message: Some(message),
                        }) => UnitKeyResult::Error(message),
                        Either::B(Thrown { message: None }) => UnitKeyResult::Thrown,
                    })
                }))
            }
            None => None,
        };
        let get_today = callback(env, get_today)?;
        let engine = fast_publicodes_engine::Engine::new(
            &json(&rules)?,
            options,
            today(move || get_today(())),
            unit_key,
        )
        .map_err(|error| Error::from_reason(error_json(&error)))?;
        Ok(Self {
            session: Session::new(engine),
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
        let mut result = match self
            .session
            .engine
            .set_situation(&json(&situation)?, options)
        {
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
        json_expression: bool,
    ) -> Result<Object<'env>> {
        let value = if json_expression {
            json(&expression)?
        } else {
            serde_json::Value::String(expression)
        };
        let evaluation = match self.session.engine.evaluate(&value) {
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
        for (name, score) in self.session.engine.missing_variables(missing) {
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
        let Some(sent) = self.session.take_warnings() else {
            return Ok(());
        };
        if sent.reset {
            result.set_named_property("warningsReset", true)?;
        }
        let warnings: Vec<Either<u32, String>> = sent
            .warnings
            .into_iter()
            .map(|warning| match warning {
                SentWarning::Index(index) => Either::A(index),
                SentWarning::Text(text) => Either::B(text),
            })
            .collect();
        result.set_named_property("warnings", warnings)
    }

    #[napi]
    pub fn inversion_fail(&self) -> bool {
        self.session.engine.inversion_fail()
    }

    #[napi]
    pub fn reset_cache(&mut self) {
        self.session.engine.reset_cache();
    }

    #[napi]
    pub fn shallow_copy(&self, copy_cache: bool) -> NativeEngine {
        NativeEngine {
            session: self.session.shallow_copy(copy_cache),
        }
    }

    /// The names of the public rules, in definition order.
    #[napi]
    pub fn rule_names(&self) -> Vec<String> {
        self.session
            .engine
            .rule_names()
            .map(str::to_owned)
            .collect()
    }

    /// Returns `{ rule }`, the rule node as JSON, or `{ error }`.
    #[napi]
    pub fn rule<'env>(&self, env: &'env Env, name: String) -> Result<Object<'env>> {
        match self.session.engine.rule(&name) {
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
        let mut result = match self
            .session
            .engine
            .possibilities(&name, filter_not_applicable)
        {
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
