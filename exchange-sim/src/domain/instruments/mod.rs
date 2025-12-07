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
    pub fn symbol(&self) -> &str {
        match self {
            Instrument::Spot(i) => i.symbol(),
            Instrument::Perpetual(i) => i.symbol(),
            Instrument::Future(i) => i.symbol(),
            Instrument::Option(i) => i.symbol(),
        }
    }

    pub fn as_spec(&self) -> &dyn InstrumentSpec {
        match self {
            Instrument::Spot(i) => i,
            Instrument::Perpetual(i) => i,
            Instrument::Future(i) => i,
            Instrument::Option(i) => i,
        }
    }

    pub fn is_derivative(&self) -> bool {
        !matches!(self, Instrument::Spot(_))
    }

    pub fn is_shortable(&self) -> bool {
        self.as_spec().is_shortable()
    }

    /// Get the base asset (e.g., BTC in BTCUSDT)
    pub fn base_asset(&self) -> &str {
        match self {
            Instrument::Spot(i) => &i.base,
            Instrument::Perpetual(i) => &i.underlying,
            Instrument::Future(i) => &i.underlying,
            Instrument::Option(i) => &i.underlying,
        }
    }

    /// Get the quote/settlement asset (e.g., USDT in BTCUSDT)
    /// For derivatives, returns "USDT" for linear contracts and the underlying for inverse
    pub fn quote_asset(&self) -> &str {
        match self {
            Instrument::Spot(i) => &i.quote,
            Instrument::Perpetual(i) => {
                if i.is_inverse {
                    &i.underlying
                } else {
                    "USDT"
                }
            }
            Instrument::Future(i) => {
                if i.is_inverse {
                    &i.underlying
                } else {
                    "USDT"
                }
            }
            Instrument::Option(_) => "USDT", // Options typically settle in USDT
        }
    }
}

impl std::fmt::Display for Instrument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.symbol())
    }
}
