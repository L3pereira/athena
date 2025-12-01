use serde::{Deserialize, Serialize};

/// Order types supported by the exchange
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderType {
    /// Execute at current market price
    Market,
    /// Execute at specified price or better
    Limit,
    /// Market order triggered when price reaches stop price
    StopLoss,
    /// Limit order triggered when price reaches stop price
    StopLimit,
}
