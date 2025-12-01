use athena_ports::MatchingError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExchangeError {
    #[error("Invalid order: {0}")]
    InvalidOrder(String),

    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    #[error("Order not found: {0}")]
    OrderNotFound(String),

    #[error("Insufficient liquidity")]
    InsufficientLiquidity,

    #[error("Time-in-force violation: {0}")]
    TimeInForceViolation(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Insufficient margin: {0}")]
    InsufficientMargin(String),

    #[error("Channel send error: {0}")]
    ChannelSendError(String),

    #[error("Channel receive error: {0}")]
    ChannelReceiveError(String),

    #[error("Time error: {0}")]
    TimeError(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ExchangeError>;

impl From<MatchingError> for ExchangeError {
    fn from(err: MatchingError) -> Self {
        match err {
            MatchingError::CannotMatch(msg) => ExchangeError::InternalError(msg),
            MatchingError::NoQuantity => {
                ExchangeError::InternalError("No quantity to match".to_string())
            }
            MatchingError::TimeInForceViolation(msg) => ExchangeError::TimeInForceViolation(msg),
            MatchingError::NoPriceAvailable => ExchangeError::InsufficientLiquidity,
        }
    }
}
