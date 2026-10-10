use std::fmt;

/// The kinds of `PublicodesError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    Syntax,
    Evaluation,
    Situation,
    UnknownRule,
    PrivateRule,
    Engine,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
    pub rule: Option<String>,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            rule: None,
        }
    }

    pub fn in_rule(mut self, rule: impl Into<String>) -> Self {
        self.rule.get_or_insert_with(|| rule.into());
        self
    }

    pub fn syntax(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Syntax, message)
    }

    pub fn evaluation(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Evaluation, message)
    }
}

impl ErrorKind {
    /// The `name` of the matching `PublicodesError`.
    pub fn name(self) -> &'static str {
        match self {
            ErrorKind::Syntax => "SyntaxError",
            ErrorKind::Evaluation => "EvaluationError",
            ErrorKind::Situation => "SituationError",
            ErrorKind::UnknownRule => "UnknownRule",
            ErrorKind::PrivateRule => "PrivateRule",
            ErrorKind::Engine => "EngineError",
            ErrorKind::Internal => "InternalError",
        }
    }
}

/// `buildMessage`: how `publicodes` writes errors and warnings.
fn build_message(name: &str, message: &str, rule: Option<&str>, cause: Option<&Error>) -> String {
    let title = match name {
        "SyntaxError" => "Erreur syntaxique",
        "EvaluationError" => "Erreur d'évaluation",
        "SituationError" => "Erreur lors de la mise à jour de la situation",
        "UnknownRule" => "Règle inconnue",
        "PrivateRule" => "Règle privée",
        other => other,
    };
    let is_error = name.to_lowercase().contains("error");
    let mut text = format!("\n[ {title} ]");
    if let Some(rule) = rule.filter(|r| !r.is_empty()) {
        text.push_str(&format!("\n➡️  Dans la règle \"{rule}\""));
    }
    text.push_str(&format!(
        "\n{}  {message}",
        if is_error { "✖️" } else { "⚠️" }
    ));
    if let Some(cause) = cause {
        text.push_str(if is_error { "\n    " } else { "\nℹ️  " });
        text.push_str(&cause.to_string());
    }
    text
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&build_message(
            self.kind.name(),
            &self.message,
            self.rule.as_deref(),
            None,
        ))
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&build_message(
            "Avertissement",
            &self.message,
            self.rule.as_deref(),
            self.cause.as_ref(),
        ))
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// The `warn` option each warning depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WarningKind {
    ExperimentalRules,
    UnitConversion,
    CyclicReferences,
    DeprecatedSyntax,
    SituationIssues,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Warning {
    pub kind: WarningKind,
    pub message: String,
    /// The rule the warning is about, if any.
    pub rule: Option<String>,
    /// The error the warning reports, appended to its message.
    pub cause: Option<Error>,
}
