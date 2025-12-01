use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Time-in-force instructions for order validity
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForce {
    /// Immediate or Cancel: execute immediately (partially or fully) and cancel unfilled portion
    IOC,

    /// Fill or Kill: execute immediately and completely, or cancel entire order
    FOK,

    /// Good Till Canceled: order remains active until explicitly canceled
    GTC,

    /// Good Till Date: order remains active until the specified datetime
    GTD(DateTime<Utc>),

    /// Day order: automatically canceled at end of trading day
    DAY,
}

impl TimeInForce {
    /// Check if the order has expired based on current time
    pub fn is_expired(&self, current_time: DateTime<Utc>, day_end: Option<DateTime<Utc>>) -> bool {
        match self {
            TimeInForce::GTD(expiry) => current_time >= *expiry,
            TimeInForce::DAY => day_end.is_some_and(|end| current_time >= end),
            _ => false,
        }
    }

    /// Returns true if partial fills are allowed
    pub fn allows_partial_fill(&self) -> bool {
        !matches!(self, TimeInForce::FOK)
    }
}
