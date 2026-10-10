use std::fmt;

use serde_json::Value;

#[derive(Debug)]
pub enum Error {
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(expression) => write!(f, "unsupported expression: {expression}"),
        }
    }
}

impl std::error::Error for Error {}

/// Evaluates `expression`, returning the evaluation as the JSON object the package exposes.
pub fn evaluate(expression: &str) -> Result<Value, Error> {
    Err(Error::Unsupported(expression.to_owned()))
}
