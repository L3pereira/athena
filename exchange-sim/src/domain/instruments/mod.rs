//! Instrument definitions for tradeable assets
//!
//! Supports multiple instrument types:
//! - Spot pairs (BTC/USDT)
//! - Perpetual swaps (BTC-PERP)
//! - Futures (BTC-DEC24)
//! - Options (BTC-27DEC24-50000-C)

mod future;
mod option;
mod perpetual;
mod spec;
mod spot;

pub use future::{FutureContract, SettlementType};
pub use option::{ExerciseStyle, OptionContract, OptionType};
pub use perpetual::PerpetualContract;
pub use spec::InstrumentSpec;
pub use spot::SpotPair;

use serde::{Deserialize, Serialize};

/// Unified instrument enum for polymorphic handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Instrument {
    Spot(SpotPair),
    Perpetual(PerpetualContract),
    Future(FutureContract),
    Option(OptionContract),
}

impl Instrument {
    /// Get underlying spec for polymorphic operations
    pub fn as_spec(&self) -> &dyn InstrumentSpec {
        match self {
            Instrument::Spot(i) => i,
            Instrument::Perpetual(i) => i,
            Instrument::Future(i) => i,
            Instrument::Option(i) => i,
        }
    }

    // Delegate all common operations to the trait
    pub fn symbol(&self) -> &str {
        self.as_spec().symbol()
    }

    pub fn base_asset(&self) -> &str {
        self.as_spec().base_asset()
    }

    pub fn quote_asset(&self) -> &str {
        self.as_spec().quote_asset()
    }

    pub fn is_derivative(&self) -> bool {
        self.as_spec().is_derivative()
    }

    pub fn is_shortable(&self) -> bool {
        self.as_spec().is_shortable()
    }
}

impl std::fmt::Display for Instrument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.symbol())
    }
}
