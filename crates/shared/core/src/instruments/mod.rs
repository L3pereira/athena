//! Instrument definitions for tradeable assets
//!
//! This module provides abstractions for different financial instruments:
//! - Spot pairs (BTC/USD)
//! - Futures (BTC-DEC24)
//! - Perpetuals (BTC-PERP)
//! - Options (BTC-DEC24-50000-C)
//! - Swaps, Bonds, Forex (future extensions)

mod future;
mod instrument;
mod option;
mod perpetual;
mod spec;
mod spot;

pub use future::FutureContract;
pub use instrument::{Instrument, InstrumentId};
pub use option::{OptionContract, OptionType};
pub use perpetual::PerpetualContract;
pub use spec::InstrumentSpec;
pub use spot::SpotPair;
