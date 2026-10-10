//! What the JavaScript bindings share: the engine as they expose it, JSON in and out, and the
//! functions JavaScript gives it.

use std::rc::Rc;

use fast_publicodes_engine::{Engine, Error, ErrorKind, Today, UnitKey, Warning};
use rustc_hash::FxHashMap;
use serde_json::Value as Json;

pub fn parse_json(text: &str) -> Result<Json, String> {
    serde_json::from_str(text).map_err(|error| error.to_string())
}

/// `{ name, message, dottedName }`, from which the package builds a `PublicodesError`.
pub fn error_json(error: &Error) -> String {
    serde_json::json!({
        "name": error.kind.name(),
        "message": error.to_string(),
        "dottedName": error.rule.clone().unwrap_or_default(),
    })
    .to_string()
}

/// What a call to `getUnitKey` gave.
pub enum UnitKeyResult {
    Key(String),
    /// The message of the `Error` it threw.
    Error(String),
    /// It threw another value, which the caller keeps.
    Thrown,
}

/// `getUnitKey`, from a call that fails with a message when JavaScript cannot be called.
pub fn unit_key(call: impl Fn(&str) -> Result<UnitKeyResult, String> + 'static) -> UnitKey {
    Rc::new(move |unit| match call(unit) {
        Ok(UnitKeyResult::Key(key)) => Ok(key),
        Ok(UnitKeyResult::Error(message)) => Err(Error::new(ErrorKind::Plain, message)),
        Ok(UnitKeyResult::Thrown) => Err(Error::new(
            ErrorKind::Thrown,
            "getUnitKey a levé une valeur",
        )),
        Err(reason) => Err(Error::new(ErrorKind::Internal, reason)),
    })
}

/// The current date, from a call that fails with a message when JavaScript cannot be called.
pub fn today(call: impl Fn() -> Result<String, String> + 'static) -> Today {
    Rc::new(move || call().map_err(|reason| Error::new(ErrorKind::Internal, reason)))
}

/// A warning as it is sent: its text the first time, then the index it took.
pub enum SentWarning {
    Index(u32),
    Text(String),
}

pub struct SentWarnings {
    /// Whether the caller empties its table of texts first.
    pub reset: bool,
    pub warnings: Vec<SentWarning>,
}

/// How many distinct warnings a session remembers having sent.
const SENT_WARNINGS: usize = 4096;

/// An engine and the warnings it sent.
pub struct Session {
    pub engine: Engine,
    sent_warnings: FxHashMap<Warning, u32>,
}

impl Session {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            sent_warnings: FxHashMap::default(),
        }
    }

    /// `shallowCopy`, which shares the texts already sent.
    pub fn shallow_copy(&self, copy_cache: bool) -> Self {
        Self {
            engine: self.engine.shallow_copy(copy_cache),
            sent_warnings: self.sent_warnings.clone(),
        }
    }

    /// The warnings raised since the last call, if any.
    pub fn take_warnings(&mut self) -> Option<SentWarnings> {
        let warnings = self.engine.take_warnings();
        if warnings.is_empty() {
            return None;
        }
        let reset = self.sent_warnings.len() + warnings.len() > SENT_WARNINGS;
        if reset {
            self.sent_warnings.clear();
        }
        let warnings = warnings
            .into_iter()
            .map(|warning| match self.sent_warnings.get(&warning) {
                Some(&index) => SentWarning::Index(index),
                None => {
                    let text = warning.to_string();
                    let index = self.sent_warnings.len() as u32;
                    self.sent_warnings.insert(warning, index);
                    SentWarning::Text(text)
                }
            })
            .collect();
        Some(SentWarnings { reset, warnings })
    }
}
