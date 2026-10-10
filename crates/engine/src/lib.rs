mod ast;
mod collation;
mod date;
mod engine;
mod error;
mod evaluate;
mod expression;
mod format;
mod js;
mod options;
mod parse;
mod raw;
mod replace;
mod resolve;
mod sub_engines;
mod types;
mod units;

pub use ast::Value;
pub use engine::{Engine, Evaluation, SituationError, SituationOptions};
pub use error::{Error, ErrorKind, Warning, WarningKind};
pub use evaluate::Missing;
pub use options::Options;
pub use units::Unit;

/// `parseExpression`: the expression as the objects `publicodes` builds.
pub fn parse_expression(expression: &str, rule: &str) -> Result<serde_json::Value, Error> {
    expression::parse_expression(expression, rule).map(|raw| raw.to_json())
}
