use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{FutureContract, InstrumentSpec, OptionContract, PerpetualContract, SpotPair};
use crate::values::{Price, Quantity};

/// Unique identifier for an instrument
///
/// This provides a stable reference to an instrument that can be stored
/// in orders and used as map keys, without copying the full instrument spec.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstrumentId(pub String);

impl InstrumentId {
    /// Create a new instrument ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for InstrumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for InstrumentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for InstrumentId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Enumeration of all supported instrument types
///
/// Each variant contains the full specification for that instrument type,
/// allowing polymorphic access to instrument properties through the
/// `InstrumentSpec` trait.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Instrument {
    /// Spot trading pair (e.g., BTC/USD)
    Spot(SpotPair),
    /// Futures contract with expiry (e.g., BTC-DEC24)
    Future(FutureContract),
    /// Perpetual swap without expiry (e.g., BTC-PERP)
    Perpetual(PerpetualContract),
    /// Option contract (e.g., BTC-DEC24-50000-C)
    Option(OptionContract),
}

impl Instrument {
    /// Get the instrument ID
    pub fn id(&self) -> InstrumentId {
        InstrumentId::new(self.symbol().to_string())
    }

    /// Get the underlying asset
    pub fn underlying(&self) -> &str {
        match self {
            Instrument::Spot(s) => &s.base,
            Instrument::Future(f) => &f.underlying,
            Instrument::Perpetual(p) => &p.underlying,
            Instrument::Option(o) => &o.underlying,
        }
    }

    /// Check if this is a derivative (non-spot)
    pub fn is_derivative(&self) -> bool {
        !matches!(self, Instrument::Spot(_))
    }

    /// Check if this instrument has an expiry
    pub fn has_expiry(&self) -> bool {
        matches!(self, Instrument::Future(_) | Instrument::Option(_))
    }

    /// Get the contract multiplier (1.0 for spot)
    pub fn multiplier(&self) -> Decimal {
        match self {
            Instrument::Spot(_) => Decimal::ONE,
            Instrument::Future(f) => f.multiplier,
            Instrument::Perpetual(p) => p.multiplier,
            Instrument::Option(o) => o.multiplier,
        }
    }
}

// Implement InstrumentSpec for Instrument by delegating to the inner type
impl InstrumentSpec for Instrument {
    fn symbol(&self) -> &str {
        match self {
            Instrument::Spot(s) => s.symbol(),
            Instrument::Future(f) => f.symbol(),
            Instrument::Perpetual(p) => p.symbol(),
            Instrument::Option(o) => o.symbol(),
        }
    }

    fn tick_size(&self) -> Price {
        match self {
            Instrument::Spot(s) => s.tick_size(),
            Instrument::Future(f) => f.tick_size(),
            Instrument::Perpetual(p) => p.tick_size(),
            Instrument::Option(o) => o.tick_size(),
        }
    }

    fn lot_size(&self) -> Quantity {
        match self {
            Instrument::Spot(s) => s.lot_size(),
            Instrument::Future(f) => f.lot_size(),
            Instrument::Perpetual(p) => p.lot_size(),
            Instrument::Option(o) => o.lot_size(),
        }
    }

    fn margin_requirement(&self) -> Decimal {
        match self {
            Instrument::Spot(s) => s.margin_requirement(),
            Instrument::Future(f) => f.margin_requirement(),
            Instrument::Perpetual(p) => p.margin_requirement(),
            Instrument::Option(o) => o.margin_requirement(),
        }
    }

    fn is_shortable(&self) -> bool {
        match self {
            Instrument::Spot(s) => s.is_shortable(),
            Instrument::Future(f) => f.is_shortable(),
            Instrument::Perpetual(p) => p.is_shortable(),
            Instrument::Option(o) => o.is_shortable(),
        }
    }
}

impl std::fmt::Display for Instrument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Instrument::Spot(s) => write!(f, "{}", s),
            Instrument::Future(fut) => write!(f, "{}", fut),
            Instrument::Perpetual(p) => write!(f, "{}", p),
            Instrument::Option(o) => write!(f, "{}", o),
        }
    }
}

// Convenient From implementations
impl From<SpotPair> for Instrument {
    fn from(spot: SpotPair) -> Self {
        Instrument::Spot(spot)
    }
}

impl From<FutureContract> for Instrument {
    fn from(future: FutureContract) -> Self {
        Instrument::Future(future)
    }
}

impl From<PerpetualContract> for Instrument {
    fn from(perp: PerpetualContract) -> Self {
        Instrument::Perpetual(perp)
    }
}

impl From<OptionContract> for Instrument {
    fn from(option: OptionContract) -> Self {
        Instrument::Option(option)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use rust_decimal_macros::dec;

    #[test]
    fn test_instrument_id() {
        let id = InstrumentId::new("BTC-PERP");
        assert_eq!(id.as_str(), "BTC-PERP");
        assert_eq!(format!("{}", id), "BTC-PERP");
    }

    #[test]
    fn test_instrument_from_spot() {
        let spot = SpotPair::btc_usd();
        let instrument: Instrument = spot.into();

        assert!(!instrument.is_derivative());
        assert!(!instrument.has_expiry());
        assert_eq!(instrument.underlying(), "BTC");
        assert_eq!(instrument.multiplier(), Decimal::ONE);
    }

    #[test]
    fn test_instrument_from_perp() {
        let perp = PerpetualContract::btc_perp();
        let instrument: Instrument = perp.into();

        assert!(instrument.is_derivative());
        assert!(!instrument.has_expiry());
        assert_eq!(instrument.underlying(), "BTC");
    }

    #[test]
    fn test_instrument_from_future() {
        let expiry = Utc.with_ymd_and_hms(2024, 12, 27, 8, 0, 0).unwrap();
        let future = FutureContract::new("BTC", expiry, "BTC-DEC24");
        let instrument: Instrument = future.into();

        assert!(instrument.is_derivative());
        assert!(instrument.has_expiry());
    }

    #[test]
    fn test_instrument_spec_delegation() {
        let perp = PerpetualContract::btc_perp();
        let instrument: Instrument = perp.clone().into();

        // Verify delegation works correctly
        assert_eq!(instrument.tick_size(), perp.tick_size());
        assert_eq!(instrument.lot_size(), perp.lot_size());
        assert_eq!(instrument.margin_requirement(), perp.margin_requirement());
    }

    #[test]
    fn test_instrument_validate_price() {
        let spot = SpotPair::with_specs("BTC", "USD", dec!(0.01), dec!(0.001));
        let instrument: Instrument = spot.into();

        assert!(instrument.validate_price(dec!(50000.01)));
        assert!(!instrument.validate_price(dec!(50000.001)));
    }
}
