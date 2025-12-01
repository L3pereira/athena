use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

/// Price value - uses Decimal for precision
/// Future: could become a newtype with validation (non-negative, tick size)
pub type Price = Decimal;

/// Quantity value - uses Decimal for precision
/// Future: could become a newtype with validation (non-negative, lot size)
pub type Quantity = Decimal;

/// Timestamp in UTC
pub type Timestamp = DateTime<Utc>;

/// Symbol identifier for a tradeable instrument
pub type Symbol = String;
