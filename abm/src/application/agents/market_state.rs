//! Market State
//!
//! State information provided to agents each tick.

use crate::application::generators::GeneratedOrderbook;
use risk_management::OrderbookMoments;
use trading_core::{Price, Quantity};

/// Best bid and offer
#[derive(Debug, Clone, Copy, Default)]
pub struct BBO {
    pub bid_price: Price,
    pub bid_size: Quantity,
    pub ask_price: Price,
    pub ask_size: Quantity,
}

impl BBO {
    /// Calculate mid price
    pub fn mid_price(&self) -> Price {
        Price::from_raw((self.bid_price.raw() + self.ask_price.raw()) / 2)
    }

    /// Calculate spread in raw units
    pub fn spread(&self) -> i64 {
        self.ask_price.raw() - self.bid_price.raw()
    }

    /// Calculate spread in basis points
    pub fn spread_bps(&self) -> f64 {
        let mid = self.mid_price().raw() as f64;
        if mid == 0.0 {
            return 0.0;
        }
        (self.spread() as f64 / mid) * 10_000.0
    }

    /// Check if the market is crossed (invalid state)
    pub fn is_crossed(&self) -> bool {
        self.bid_price.raw() >= self.ask_price.raw()
    }
}

/// Order book depth level
#[derive(Debug, Clone, Copy)]
pub struct DepthLevel {
    pub price: Price,
    pub quantity: Quantity,
}

/// Market state provided to agents each tick
#[derive(Debug, Clone)]
pub struct MarketState {
    /// Current timestamp in milliseconds
    pub timestamp_ms: u64,

    /// Symbol being traded
    pub symbol: String,

    /// Best bid and offer
    pub bbo: BBO,

    /// Orderbook moments (spread, imbalance, depth ratio, volatility)
    pub moments: OrderbookMoments,

    /// Top N bid levels
    pub bids: Vec<DepthLevel>,

    /// Top N ask levels
    pub asks: Vec<DepthLevel>,

    /// Last trade price (if any)
    pub last_trade_price: Option<Price>,

    /// Volume in last period
    pub recent_volume: Quantity,
}

impl MarketState {
    /// Create empty market state
    pub fn empty(symbol: impl Into<String>) -> Self {
        Self {
            timestamp_ms: 0,
            symbol: symbol.into(),
            bbo: BBO::default(),
            moments: OrderbookMoments::default(),
            bids: Vec::new(),
            asks: Vec::new(),
            last_trade_price: None,
            recent_volume: Quantity::from_raw(0),
        }
    }

    /// Get mid price
    pub fn mid_price(&self) -> Price {
        self.bbo.mid_price()
    }

    /// Get spread in bps
    pub fn spread_bps(&self) -> f64 {
        self.bbo.spread_bps()
    }

    /// Check if market has valid quotes
    pub fn has_quotes(&self) -> bool {
        self.bbo.bid_price.raw() > 0 && self.bbo.ask_price.raw() > 0 && !self.bbo.is_crossed()
    }

    /// Total bid depth
    pub fn total_bid_depth(&self) -> Quantity {
        Quantity::from_raw(self.bids.iter().map(|l| l.quantity.raw()).sum())
    }

    /// Total ask depth
    pub fn total_ask_depth(&self) -> Quantity {
        Quantity::from_raw(self.asks.iter().map(|l| l.quantity.raw()).sum())
    }

    /// Order book imbalance (-1 to 1)
    pub fn imbalance(&self) -> f64 {
        let bid_depth = self.total_bid_depth().raw() as f64;
        let ask_depth = self.total_ask_depth().raw() as f64;
        let total = bid_depth + ask_depth;

        if total == 0.0 {
            return 0.0;
        }

        (bid_depth - ask_depth) / total
    }

    /// Update market state from generated orderbook
    pub fn update_from_orderbook(&mut self, orderbook: &GeneratedOrderbook) {
        // Update BBO
        let best_bid = orderbook.best_bid();
        let best_ask = orderbook.best_ask();

        self.bbo = BBO {
            bid_price: best_bid,
            bid_size: orderbook
                .bid_levels
                .first()
                .map(|l| l.quantity)
                .unwrap_or(Quantity::ZERO),
            ask_price: best_ask,
            ask_size: orderbook
                .ask_levels
                .first()
                .map(|l| l.quantity)
                .unwrap_or(Quantity::ZERO),
        };

        // Update depth levels
        self.bids = orderbook
            .bid_levels
            .iter()
            .map(|l| DepthLevel {
                price: l.price,
                quantity: l.quantity,
            })
            .collect();

        self.asks = orderbook
            .ask_levels
            .iter()
            .map(|l| DepthLevel {
                price: l.price,
                quantity: l.quantity,
            })
            .collect();

        // Update moments from computed values
        let mid = self.mid_price().raw() as f64;
        if mid > 0.0 {
            self.moments.spread_bps = self.spread_bps();
            self.moments.imbalance = orderbook.imbalance;
        }
    }
}

impl Default for MarketState {
    fn default() -> Self {
        Self::empty("UNKNOWN")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bbo_mid_price() {
        let bbo = BBO {
            bid_price: Price::from_int(99),
            bid_size: Quantity::from_raw(100),
            ask_price: Price::from_int(101),
            ask_size: Quantity::from_raw(100),
        };

        assert_eq!(bbo.mid_price(), Price::from_int(100));
    }

    #[test]
    fn test_bbo_spread_bps() {
        let bbo = BBO {
            bid_price: Price::from_int(9990),
            bid_size: Quantity::from_raw(100),
            ask_price: Price::from_int(10010),
            ask_size: Quantity::from_raw(100),
        };

        // Spread = 20, mid = 10000, bps = 20/10000 * 10000 = 20
        let spread_bps = bbo.spread_bps();
        assert!((spread_bps - 20.0).abs() < 0.1);
    }

    #[test]
    fn test_market_state_imbalance() {
        let mut state = MarketState::empty("BTC-USDT");
        state.bids = vec![DepthLevel {
            price: Price::from_int(100),
            quantity: Quantity::from_raw(100),
        }];
        state.asks = vec![DepthLevel {
            price: Price::from_int(101),
            quantity: Quantity::from_raw(50),
        }];

        // bid = 100, ask = 50, imbalance = (100-50)/(100+50) = 50/150 = 0.333
        let imb = state.imbalance();
        assert!((imb - 0.333).abs() < 0.01);
    }
}
