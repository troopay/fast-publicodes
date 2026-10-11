use fast_publicodes_engine::{
    Error as EngineError, Evaluation, Evaluations, Options, SituationOptions, Value,
};
use fast_publicodes_js::{
    Sent, Session, UnitKeyResult, error_json, expression, failure_json, parse_json, today, unit_key,
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
    /// JSON `{ name, message, dottedName, warnings }`.
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
        .map_err(|failure| Error::from_reason(failure_json(&failure)))?;
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

    /// Returns `{ nodeValue, index }`, `index` locating it in `evaluations`, with `unit` the index
    /// of its unit in the table the caller keeps, or `newUnit` when it is new and takes the next
    /// index, after `unitsReset` asks the caller to empty its table; or `{ error }`. `text` is
    /// JSON when `json`.
    #[napi]
    pub fn evaluate<'env>(
        &mut self,
        env: &'env Env,
        text: String,
        json: bool,
    ) -> Result<Object<'env>> {
        let value = expression(text, json).map_err(Error::from_reason)?;
        let evaluation = self.session.engine.evaluate(&value);
        self.evaluation(env, evaluation)
    }

    /// `evaluate`, for `getRule(rule).suggestions[name]`.
    #[napi]
    pub fn evaluate_suggestion<'env>(
        &mut self,
        env: &'env Env,
        rule: String,
        name: String,
    ) -> Result<Object<'env>> {
        let evaluation = self.session.engine.evaluate_suggestion(&rule, &name);
        self.evaluation(env, evaluation)
    }

    fn evaluation<'env>(
        &mut self,
        env: &'env Env,
        evaluation: std::result::Result<Evaluation, EngineError>,
    ) -> Result<Object<'env>> {
        let evaluation = match evaluation {
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
        result.set_named_property("index", evaluation.index)?;
        if let Some(unit) = evaluation.unit {
            let sent = self.session.send_unit(unit);
            if sent.reset {
                result.set_named_property("unitsReset", true)?;
            }
            match sent.unit {
                Sent::Index(index) => result.set_named_property("unit", index)?,
                Sent::New(unit) => {
                    let mut js_unit = Object::new(env)?;
                    js_unit.set_named_property("numerators", unit.numerators.clone())?;
                    js_unit.set_named_property("denominators", unit.denominators.clone())?;
                    result.set_named_property("newUnit", js_unit)?;
                }
            }
        }
        Ok(result)
    }

    /// A handle on the expressions evaluated since the cache was last reset, for
    /// `missingVariables`.
    #[napi(ts_return_type = "unknown")]
    pub fn evaluations(&self) -> External<Evaluations> {
        External::new(self.session.engine.evaluations())
    }

    /// The missing variables of the evaluation at `index` in `evaluations`.
    #[napi(ts_args_type = "evaluations: unknown, index: number")]
    pub fn missing_variables<'env>(
        &self,
        env: &'env Env,
        evaluations: &External<Evaluations>,
        index: u32,
    ) -> Result<Object<'env>> {
        let missing = evaluations.missing(index).unwrap_or_default();
        let mut result = Object::new(env)?;
        for (name, score) in self.session.engine.missing_variables(&missing) {
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
                Sent::Index(index) => Either::A(index),
                Sent::New(text) => Either::B(text),
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

    /// Returns `{ rule }`, the rule node with its inferred type and the names of its suggestions,
    /// as JSON, or `{ error }`.
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
    pub fn applicable_possibilities<'env>(
        &mut self,
        env: &'env Env,
        name: String,
    ) -> Result<Object<'env>> {
        let mut result = match self.session.engine.applicable_possibilities(&name) {
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
