use thiserror::Error;

/// Domain-level errors for matching operations
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MatchingError {
    #[error("Orders cannot match: {0}")]
    CannotMatch(String),

    #[error("No quantity to match")]
    NoQuantity,

    #[error("Time-in-force violation: {0}")]
    TimeInForceViolation(String),

    #[error("Price unavailable for market order")]
    NoPriceAvailable,
}

pub type MatchingResult<T> = std::result::Result<T, MatchingError>;
