//! Reflexive Market Architecture
//!
//! Implements Soros's reflexivity: trades can shift market structure permanently.
//!
//! The reflexive loop:
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                                                             │
//! │  Market State → Agents Trade → Impact → Structure Change    │
//! │       ↑                                      │              │
//! │       └────── Regime Shift Detection ←───────┘              │
//! │                                                             │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! Key insight: Large trades don't just move prices temporarily—they can
//! permanently alter market microstructure (liquidity, volatility regime).

mod circuit_breakers;
mod reflexive_loop;

pub use circuit_breakers::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
pub use reflexive_loop::{ReflexiveEvent, ReflexiveLoop, ReflexiveLoopConfig};
