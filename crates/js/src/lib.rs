//! What the JavaScript bindings share: the engine as they expose it, JSON in and out, and the
//! functions JavaScript gives it.

use std::hash::Hash;
use std::rc::Rc;

use fast_publicodes_engine::{Engine, Error, ErrorKind, Failure, Today, UnitKey, UnitRef, Warning};
use rustc_hash::FxHashMap;
use serde_json::Value as Json;

pub fn parse_json(text: &str) -> Result<Json, String> {
    serde_json::from_str(text).map_err(|error| error.to_string())
}

/// An expression to evaluate: the string itself, or JSON when `json`.
pub fn expression(text: String, json: bool) -> Result<Json, String> {
    if json {
        parse_json(&text)
    } else {
        Ok(Json::String(text))
    }
}

/// `{ name, message, dottedName }`, from which the package builds a `PublicodesError`.
pub fn error_json(error: &Error) -> String {
    error_value(error).to_string()
}

/// The error as `error_json` writes it, with `warnings`: the texts of those raised before.
pub fn failure_json(failure: &Failure) -> String {
    let mut value = error_value(&failure.error);
    value["warnings"] = failure.warnings.iter().map(ToString::to_string).collect();
    value.to_string()
}

fn error_value(error: &Error) -> Json {
    serde_json::json!({
        "name": error.kind.name(),
        "message": error.to_string(),
        "dottedName": error.rule.clone().unwrap_or_default(),
    })
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

/// A value as it is sent: whole the first time, then by the index it took in the table the
/// caller keeps.
pub enum Sent<T> {
    Index(u32),
    New(T),
}

/// Values sent, then emptied with the caller's table when it would outgrow `CAPACITY`.
#[derive(Clone)]
struct SentTable<K> {
    indices: FxHashMap<K, u32>,
}

impl<K: Hash + Eq> SentTable<K> {
    const CAPACITY: usize = 4096;

    /// Makes room for `count` values: whether the caller empties its table first.
    fn reserve(&mut self, count: usize) -> bool {
        let reset = self.indices.len() + count > Self::CAPACITY;
        if reset {
            self.indices.clear();
        }
        reset
    }

    fn send<T>(&mut self, key: K, value: impl FnOnce(&K) -> T) -> Sent<T> {
        if let Some(&index) = self.indices.get(&key) {
            return Sent::Index(index);
        }
        let value = value(&key);
        let index = self.indices.len() as u32;
        self.indices.insert(key, index);
        Sent::New(value)
    }
}

impl<K> Default for SentTable<K> {
    fn default() -> Self {
        Self {
            indices: FxHashMap::default(),
        }
    }
}

pub struct SentWarnings {
    /// Whether the caller empties its table of texts first.
    pub reset: bool,
    pub warnings: Vec<Sent<String>>,
}

pub struct SentUnit {
    /// Whether the caller empties its table of units first.
    pub reset: bool,
    pub unit: Sent<UnitRef>,
}

/// An engine, with the warnings and units it sent.
pub struct Session {
    pub engine: Engine,
    warnings: SentTable<Warning>,
    units: SentTable<UnitRef>,
}

impl Session {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            warnings: SentTable::default(),
            units: SentTable::default(),
        }
    }

    /// `shallowCopy`, which shares the warnings and units already sent.
    pub fn shallow_copy(&self, copy_cache: bool) -> Self {
        Self {
            engine: self.engine.shallow_copy(copy_cache),
            warnings: self.warnings.clone(),
            units: self.units.clone(),
        }
    }

    /// The warnings raised since the last call, if any.
    pub fn take_warnings(&mut self) -> Option<SentWarnings> {
        let warnings = self.engine.take_warnings();
        if warnings.is_empty() {
            return None;
        }
        let reset = self.warnings.reserve(warnings.len());
        let warnings = warnings
            .into_iter()
            .map(|warning| self.warnings.send(warning, ToString::to_string))
            .collect();
        Some(SentWarnings { reset, warnings })
    }

    pub fn send_unit(&mut self, unit: UnitRef) -> SentUnit {
        let reset = self.units.reserve(1);
        SentUnit {
            reset,
            unit: self.units.send(unit, Rc::clone),
        }
    }
}
