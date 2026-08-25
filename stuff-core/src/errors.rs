use serde::Serialize;
use thiserror::Error;

#[derive(Clone, Debug, Serialize)]
pub struct AmbiguousCandidate {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
}

#[derive(Debug, Error)]
pub enum StuffError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid operation: {0}")]
    Invalid(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("state changed in another client; refresh and retry")]
    Conflict,
    #[error("selector '{selector}' is ambiguous")]
    Ambiguous {
        selector: String,
        candidates: Vec<AmbiguousCandidate>,
    },
    #[error(
        "contents in '{old_location}' cannot be assigned after splitting; choose a compartment"
    )]
    CompartmentSplitAmbiguous {
        old_location: String,
        candidates: Vec<String>,
    },
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("database migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, StuffError>;
