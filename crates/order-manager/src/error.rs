//! Order Manager errors

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Risk check failed: {reason}")]
    RiskCheckFailed { reason: String },

    #[error(
        "Position limit exceeded for {instrument_id}: current={current}, requested={requested}, limit={limit}"
    )]
    PositionLimitExceeded {
        instrument_id: String,
        current: String,
        requested: String,
        limit: String,
    },

    #[error("Exposure limit exceeded: current={current}, limit={limit}")]
    ExposureLimitExceeded { current: String, limit: String },

    #[error("Unknown instrument: {0}")]
    UnknownInstrument(String),

    #[error("Unknown strategy: {0}")]
    UnknownStrategy(String),

    #[error("Invalid signal: {0}")]
    InvalidSignal(String),
}

pub type Result<T> = std::result::Result<T, Error>;
