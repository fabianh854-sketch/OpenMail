//! Fehlertyp der Kernschicht. Nach außen (Frontend) geht nur [`AppError::public_message`].

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AppError {
    #[error("ungültige Eingabe: {0}")]
    InvalidInput(&'static str),
    #[error("nicht gefunden")]
    NotFound,
    #[error("keine Berechtigung")]
    Forbidden,
    #[error("Grenze erreicht: {0}")]
    LimitReached(&'static str),
    /// Interner Fehler. Der Kontext ist nur für Logs gedacht und wird nie ausgegeben.
    #[error("interner Fehler")]
    Internal(&'static str),
}

impl AppError {
    /// Meldung, die an das Frontend weitergegeben werden darf.
    #[must_use]
    pub fn public_message(&self) -> String {
        match self {
            Self::Internal(_) => "Es ist ein interner Fehler aufgetreten.".to_owned(),
            other => other.to_string(),
        }
    }
}
