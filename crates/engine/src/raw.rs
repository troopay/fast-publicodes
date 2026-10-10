//! Raw publicodes values: rules as written, and the objects mechanisms build before parsing.

use serde_json::Value as Json;

use crate::ast::NodeId;

#[derive(Debug, Clone)]
pub enum Raw {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    List(Vec<Raw>),
    Map(Vec<(String, Raw)>),
    /// An already parsed node.
    Node(NodeId),
}

impl Raw {
    pub fn map<K: Into<String>>(entries: impl IntoIterator<Item = (K, Raw)>) -> Self {
        Raw::Map(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    pub fn str(text: &str) -> Self {
        Raw::String(text.to_owned())
    }

    pub fn get(&self, key: &str) -> Option<&Raw> {
        match self {
            Raw::Map(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Raw> {
        match self {
            Raw::Map(entries) => entries.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
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
}

impl From<&Json> for Raw {
    fn from(value: &Json) -> Self {
        match value {
            Json::Null => Raw::Null,
            Json::Bool(b) => Raw::Bool(*b),
            Json::Number(n) => Raw::Number(n.as_f64().unwrap_or(f64::NAN)),
            Json::String(s) => Raw::String(s.clone()),
            Json::Array(items) => Raw::List(items.iter().map(Raw::from).collect()),
            Json::Object(entries) => Raw::Map(
                entries
                    .iter()
                    .map(|(k, v)| (k.clone(), Raw::from(v)))
                    .collect(),
            ),
        }
    }
}
