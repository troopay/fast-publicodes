use std::fmt;

/// The kinds of `PublicodesError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Syntax,
    Evaluation,
    Situation,
    UnknownRule,
    PrivateRule,
    Engine,
    Internal,
    /// A mechanism fast-publicodes does not implement yet.
    Unsupported,
}

#[derive(Debug, Clone)]
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

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            ErrorKind::Syntax => "Erreur syntaxique",
            ErrorKind::Evaluation => "Erreur d'évaluation",
            ErrorKind::Situation => "Erreur lors de la mise à jour de la situation",
            ErrorKind::UnknownRule => "Règle inconnue",
            ErrorKind::PrivateRule => "Règle privée",
            ErrorKind::Engine => "EngineError",
            ErrorKind::Internal => "InternalError",
            ErrorKind::Unsupported => "Non supporté",
        };
        write!(f, "[ {kind} ]")?;
        if let Some(rule) = &self.rule {
            write!(f, "\nDans la règle \"{rule}\"")?;
        }
        write!(f, "\n{}", self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
