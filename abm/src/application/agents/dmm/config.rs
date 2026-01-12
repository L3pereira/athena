//! DMM Configuration
//!
//! Configuration for the Designated Market Maker agent.

use serde::{Deserialize, Serialize};

/// Configuration for the DMM agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DMMConfig {
    /// Risk aversion parameter (gamma in A-S model)
    /// Higher = wider spreads, more conservative
    pub gamma: f64,

    /// Maximum inventory position (in base asset raw units)
    pub max_inventory: i64,

    /// Quote size (in base asset raw units)
    pub quote_size: i64,

    /// Minimum spread in bps to quote (below this, not profitable)
    pub min_spread_bps: f64,

    /// Inventory ratio threshold to start skewing quotes
    pub skew_threshold: f64,

    /// Price change threshold to trigger requote (as fraction)
    pub requote_threshold: f64,

    /// Time horizon for A-S model (hours)
    pub time_horizon_hours: u32,

    /// Pause quoting when reference feed indicates stress
    pub pause_on_stress: bool,
}

impl Default for DMMConfig {
    fn default() -> Self {
        Self {
            gamma: 0.1,
            max_inventory: 10_00000000, // 10 units
            quote_size: 1_00000000,     // 1 unit
            min_spread_bps: 5.0,
            skew_threshold: 0.5,      // Start skewing at 50% inventory
            requote_threshold: 0.001, // 0.1% price change
            time_horizon_hours: 8,
            pause_on_stress: true,
        }
    }
}

impl DMMConfig {
    /// Create conservative config (wider spreads)
    pub fn conservative() -> Self {
        Self {
            gamma: 0.2,
            min_spread_bps: 10.0,
            ..Default::default()
        }
    }

    /// Create aggressive config (tighter spreads)
    pub fn aggressive() -> Self {
        Self {
            gamma: 0.05,
            min_spread_bps: 2.0,
            ..Default::default()
        }
    }

    /// Set max inventory (builder pattern)
    pub fn with_max_inventory(mut self, max: i64) -> Self {
        self.max_inventory = max;
        self
    }

    /// Set quote size (builder pattern)
    pub fn with_quote_size(mut self, size: i64) -> Self {
        self.quote_size = size;
        self
    }

    /// Set gamma (builder pattern)
    pub fn with_gamma(mut self, gamma: f64) -> Self {
        self.gamma = gamma;
        self
    }
}
