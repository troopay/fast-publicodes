use napi::{Error, Result};
use napi_derive::napi;

/// Returns the evaluation of `expression` serialized as JSON.
#[napi]
pub fn evaluate(expression: String) -> Result<String> {
    fast_publicodes_engine::evaluate(&expression)
        .map(|evaluation| evaluation.to_string())
        .map_err(|error| Error::from_reason(error.to_string()))
}
