//! Raw publicodes values: rules as written, and the objects mechanisms build before parsing.

use std::rc::Rc;

use serde_json::Value as Json;

use crate::ast::NodeId;
use crate::js::number_to_string;

pub type Entries = Vec<(Rc<str>, Raw)>;

/// Cloning shares the strings, lists and maps.
#[derive(Debug, Clone)]
pub enum Raw {
    /// Given as the value of a rule or of the situation, or as the value to evaluate.
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Rc<str>),
    List(Rc<Vec<Raw>>),
    Map(Rc<Entries>),
    /// An already parsed node.
    Node(NodeId),
}

impl Raw {
    pub fn map<K: Into<Rc<str>>>(entries: impl IntoIterator<Item = (K, Raw)>) -> Self {
        Raw::Map(Rc::new(
            entries.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        ))
    }

    pub fn list(items: Vec<Raw>) -> Self {
        Raw::List(Rc::new(items))
    }

    pub fn str(text: &str) -> Self {
        Raw::String(text.into())
    }

    pub fn get(&self, key: &str) -> Option<&Raw> {
        match self {
            Raw::Map(entries) => entries.iter().find(|(k, _)| &**k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Raw> {
        match self {
            Raw::Map(entries) => Rc::make_mut(entries)
                .iter_mut()
                .find(|(k, _)| &**k == key)
                .map(|(_, v)| v),
            _ => None,
        }
    }

    /// JS truthiness.
    pub fn is_truthy(&self) -> bool {
        match self {
            Raw::Null | Raw::Bool(false) => false,
            Raw::Number(n) => *n != 0. && !n.is_nan(),
            Raw::String(s) => !s.is_empty(),
            _ => true,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Raw::String(text) => Some(text),
            _ => None,
        }
    }

    /// `value === 'oui'`.
    pub fn is_oui(&self) -> bool {
        self.as_str() == Some("oui")
    }

    /// `String(value)`, a parsed node being an object.
    pub fn to_js_string(&self) -> String {
        match self {
            Raw::Undefined => "undefined".to_owned(),
            Raw::Null => "null".to_owned(),
            Raw::Bool(b) => b.to_string(),
            Raw::Number(n) => number_to_string(*n),
            Raw::String(s) => s.to_string(),
            Raw::List(items) => items
                .iter()
                .map(|item| match item {
                    Raw::Undefined | Raw::Null => String::new(),
                    item => item.to_js_string(),
                })
                .collect::<Vec<_>>()
                .join(","),
            Raw::Map(_) | Raw::Node(_) => "[object Object]".to_owned(),
        }
    }

    pub fn to_json(&self) -> Json {
        match self {
            Raw::Undefined | Raw::Null | Raw::Node(_) => Json::Null,
            Raw::Bool(b) => Json::Bool(*b),
            Raw::Number(n) => serde_json::Number::from_f64(*n).map_or(Json::Null, Json::Number),
            Raw::String(s) => Json::String(s.to_string()),
            Raw::List(items) => Json::Array(items.iter().map(Raw::to_json).collect()),
            Raw::Map(entries) => Json::Object(
                entries
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_json()))
                    .collect(),
            ),
        }
    }
}

/// Reads JSON in which an object with the single key `"\u{0}"` stands for what JSON cannot
/// write: `"undefined"`, `"NaN"`, `"Infinity"` or `"-Infinity"`.
impl From<&Json> for Raw {
    fn from(value: &Json) -> Self {
        match value {
            Json::Object(entries)
                if entries.len() == 1
                    && let Some(Json::String(special)) = entries.get("\u{0}") =>
            {
                match special.as_str() {
                    "undefined" => Raw::Undefined,
                    "Infinity" => Raw::Number(f64::INFINITY),
                    "-Infinity" => Raw::Number(f64::NEG_INFINITY),
                    _ => Raw::Number(f64::NAN),
                }
            }
            Json::Null => Raw::Null,
            Json::Bool(b) => Raw::Bool(*b),
            Json::Number(n) => Raw::Number(n.as_f64().unwrap_or(f64::NAN)),
            Json::String(s) => Raw::String(s.as_str().into()),
            Json::Array(items) => Raw::list(items.iter().map(Raw::from).collect()),
            Json::Object(entries) => {
                Raw::map(entries.iter().map(|(k, v)| (k.as_str(), Raw::from(v))))
            }
        }
    }
}
