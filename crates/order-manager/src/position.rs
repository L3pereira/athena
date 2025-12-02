//! Position Tracking and PnL Attribution
//!
//! Tracks positions at two levels:
//! 1. Per-strategy positions (for PnL attribution)
//! 2. Net portfolio positions (what we actually hold)
//!
//! When a fill comes in, we attribute it to the strategy that generated the order,
//! update both the strategy and portfolio positions, and calculate realized PnL.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::Signed;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A fill from the exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    /// Order ID that was filled
    pub order_id: String,
    /// Client order ID (links back to strategy)
    pub client_order_id: String,
    /// Instrument
    pub instrument_id: String,
    /// Fill side
    pub side: FillSide,
    /// Quantity filled
    pub quantity: Decimal,
    /// Fill price
    pub price: Decimal,
    /// Exchange fee (positive = cost)
    pub fee: Decimal,
    /// Fee currency
    pub fee_currency: String,
    /// When the fill occurred
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillSide {
    Buy,
    Sell,
}

/// Position for a single strategy-instrument pair
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategyPosition {
    /// Current position quantity (positive=long, negative=short)
    pub quantity: Decimal,
    /// Average entry price (FIFO)
    pub avg_price: Decimal,
    /// Total realized PnL
    pub realized_pnl: Decimal,
    /// Total fees paid
    pub total_fees: Decimal,
    /// Number of fills
    pub fill_count: u64,
    /// Total volume traded (absolute)
    pub volume: Decimal,
}

impl StrategyPosition {
    /// Apply a fill to this position, returning realized PnL from this fill
    pub fn apply_fill(
        &mut self,
        side: FillSide,
        quantity: Decimal,
        price: Decimal,
        fee: Decimal,
    ) -> Decimal {
        let signed_qty = match side {
            FillSide::Buy => quantity,
            FillSide::Sell => -quantity,
        };

        let mut realized_pnl = Decimal::ZERO;

        // Calculate realized PnL if reducing position
        if (self.quantity > Decimal::ZERO && signed_qty < Decimal::ZERO)
            || (self.quantity < Decimal::ZERO && signed_qty > Decimal::ZERO)
        {
            // Closing (partially or fully)
            let close_qty = signed_qty.abs().min(self.quantity.abs());
            realized_pnl = if self.quantity > Decimal::ZERO {
                // Was long, selling
                close_qty * (price - self.avg_price)
            } else {
                // Was short, buying
                close_qty * (self.avg_price - price)
            };
        }

        // Update position
        let new_quantity = self.quantity + signed_qty;

        // Update average price
        if new_quantity.is_zero() {
            // Flat, reset avg price
            self.avg_price = Decimal::ZERO;
        } else if (self.quantity >= Decimal::ZERO && signed_qty > Decimal::ZERO)
            || (self.quantity <= Decimal::ZERO && signed_qty < Decimal::ZERO)
        {
            // Adding to position - weighted average
            let total_cost = self.quantity.abs() * self.avg_price + quantity * price;
            self.avg_price = total_cost / new_quantity.abs();
        } else if new_quantity.abs() > Decimal::ZERO
            && new_quantity.signum() != self.quantity.signum()
        {
            // Flipped sides - new avg price is fill price
            self.avg_price = price;
        }
        // If reducing but not flipping, avg_price stays same

        self.quantity = new_quantity;
        self.realized_pnl += realized_pnl;
        self.total_fees += fee;
        self.fill_count += 1;
        self.volume += quantity;

        realized_pnl
    }

    /// Calculate unrealized PnL at a given mark price
    pub fn unrealized_pnl(&self, mark_price: Decimal) -> Decimal {
        if self.quantity.is_zero() {
            Decimal::ZERO
        } else if self.quantity > Decimal::ZERO {
            self.quantity * (mark_price - self.avg_price)
        } else {
            self.quantity.abs() * (self.avg_price - mark_price)
        }
    }

    /// Total PnL (realized + unrealized) minus fees
    pub fn total_pnl(&self, mark_price: Decimal) -> Decimal {
        self.realized_pnl + self.unrealized_pnl(mark_price) - self.total_fees
    }
}

/// Net portfolio position for an instrument (aggregated across all strategies)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortfolioPosition {
    /// Net position quantity
    pub quantity: Decimal,
    /// Total notional exposure (for risk)
    pub notional: Decimal,
}

/// Tracks all positions and attributes PnL to strategies
#[derive(Debug, Default)]
pub struct PositionTracker {
    /// Position per (strategy_id, instrument_id)
    strategy_positions: HashMap<(String, String), StrategyPosition>,
    /// Net position per instrument
    portfolio_positions: HashMap<String, PortfolioPosition>,
    /// Maps client_order_id -> strategy_id
    order_to_strategy: HashMap<String, String>,
}

impl PositionTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an order so we can attribute fills to the right strategy
    pub fn register_order(&mut self, client_order_id: &str, strategy_id: &str) {
        self.order_to_strategy
            .insert(client_order_id.to_string(), strategy_id.to_string());
    }

    /// Process a fill, updating positions and returning the strategy attribution
    pub fn process_fill(&mut self, fill: &Fill) -> Option<FillAttribution> {
        // Look up which strategy this fill belongs to
        let strategy_id = self.order_to_strategy.get(&fill.client_order_id)?.clone();

        // Update strategy position
        let key = (strategy_id.clone(), fill.instrument_id.clone());
        let strategy_pos = self.strategy_positions.entry(key).or_default();
        let realized_pnl = strategy_pos.apply_fill(fill.side, fill.quantity, fill.price, fill.fee);

        // Update portfolio position
        let portfolio_pos = self
            .portfolio_positions
            .entry(fill.instrument_id.clone())
            .or_default();

        let signed_qty = match fill.side {
            FillSide::Buy => fill.quantity,
            FillSide::Sell => -fill.quantity,
        };
        portfolio_pos.quantity += signed_qty;
        portfolio_pos.notional = portfolio_pos.quantity.abs() * fill.price;

        Some(FillAttribution {
            strategy_id,
            instrument_id: fill.instrument_id.clone(),
            realized_pnl,
            fee: fill.fee,
            new_position: strategy_pos.quantity,
            timestamp: fill.timestamp,
        })
    }

    /// Get position for a specific strategy and instrument
    pub fn strategy_position(
        &self,
        strategy_id: &str,
        instrument_id: &str,
    ) -> Option<&StrategyPosition> {
        self.strategy_positions
            .get(&(strategy_id.to_string(), instrument_id.to_string()))
    }

    /// Get net portfolio position for an instrument
    pub fn portfolio_position(&self, instrument_id: &str) -> Option<&PortfolioPosition> {
        self.portfolio_positions.get(instrument_id)
    }

    /// Get all strategy positions
    pub fn all_strategy_positions(&self) -> &HashMap<(String, String), StrategyPosition> {
        &self.strategy_positions
    }

    /// Get all portfolio positions
    pub fn all_portfolio_positions(&self) -> &HashMap<String, PortfolioPosition> {
        &self.portfolio_positions
    }

    /// Calculate total portfolio PnL across all strategies
    pub fn total_realized_pnl(&self) -> Decimal {
        self.strategy_positions
            .values()
            .map(|p| p.realized_pnl)
            .sum()
    }

    /// Calculate total fees paid
    pub fn total_fees(&self) -> Decimal {
        self.strategy_positions.values().map(|p| p.total_fees).sum()
    }

    /// Get PnL breakdown by strategy
    pub fn pnl_by_strategy(
        &self,
        mark_prices: &HashMap<String, Decimal>,
    ) -> HashMap<String, StrategyPnL> {
        let mut result: HashMap<String, StrategyPnL> = HashMap::new();

        for ((strategy_id, instrument_id), pos) in &self.strategy_positions {
            let mark = mark_prices
                .get(instrument_id)
                .copied()
                .unwrap_or(pos.avg_price);

            let entry = result.entry(strategy_id.clone()).or_default();
            entry.realized_pnl += pos.realized_pnl;
            entry.unrealized_pnl += pos.unrealized_pnl(mark);
            entry.total_fees += pos.total_fees;
            entry.volume += pos.volume;
            entry.fill_count += pos.fill_count;
        }

        result
    }
}

/// Attribution of a fill back to a strategy
#[derive(Debug, Clone)]
pub struct FillAttribution {
    pub strategy_id: String,
    pub instrument_id: String,
    pub realized_pnl: Decimal,
    pub fee: Decimal,
    pub new_position: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// PnL summary for a strategy
#[derive(Debug, Clone, Default)]
pub struct StrategyPnL {
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub total_fees: Decimal,
    pub volume: Decimal,
    pub fill_count: u64,
}

impl StrategyPnL {
    pub fn net_pnl(&self) -> Decimal {
        self.realized_pnl + self.unrealized_pnl - self.total_fees
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_strategy_position_long() {
        let mut pos = StrategyPosition::default();

        // Buy 1 @ 100
        let pnl = pos.apply_fill(FillSide::Buy, dec!(1), dec!(100), dec!(0.1));
        assert_eq!(pnl, dec!(0)); // No realized PnL
        assert_eq!(pos.quantity, dec!(1));
        assert_eq!(pos.avg_price, dec!(100));

        // Buy 1 @ 110 (avg now 105)
        let pnl = pos.apply_fill(FillSide::Buy, dec!(1), dec!(110), dec!(0.1));
        assert_eq!(pnl, dec!(0));
        assert_eq!(pos.quantity, dec!(2));
        assert_eq!(pos.avg_price, dec!(105));

        // Sell 1 @ 120 (realized PnL = 1 * (120 - 105) = 15)
        let pnl = pos.apply_fill(FillSide::Sell, dec!(1), dec!(120), dec!(0.1));
        assert_eq!(pnl, dec!(15));
        assert_eq!(pos.quantity, dec!(1));
        assert_eq!(pos.avg_price, dec!(105)); // Avg stays same

        // Check unrealized
        let unrealized = pos.unrealized_pnl(dec!(130));
        assert_eq!(unrealized, dec!(25)); // 1 * (130 - 105)
    }

    #[test]
    fn test_strategy_position_short() {
        let mut pos = StrategyPosition::default();

        // Sell 1 @ 100 (go short)
        let pnl = pos.apply_fill(FillSide::Sell, dec!(1), dec!(100), dec!(0));
        assert_eq!(pnl, dec!(0));
        assert_eq!(pos.quantity, dec!(-1));
        assert_eq!(pos.avg_price, dec!(100));

        // Buy back @ 90 (profit = 100 - 90 = 10)
        let pnl = pos.apply_fill(FillSide::Buy, dec!(1), dec!(90), dec!(0));
        assert_eq!(pnl, dec!(10));
        assert_eq!(pos.quantity, dec!(0));
    }

    #[test]
    fn test_position_tracker() {
        let mut tracker = PositionTracker::new();

        // Register orders
        tracker.register_order("order-1", "strategy-a");
        tracker.register_order("order-2", "strategy-b");

        // Fill for strategy A
        let fill_a = Fill {
            order_id: "exch-1".to_string(),
            client_order_id: "order-1".to_string(),
            instrument_id: "BTC-USD".to_string(),
            side: FillSide::Buy,
            quantity: dec!(1),
            price: dec!(50000),
            fee: dec!(5),
            fee_currency: "USD".to_string(),
            timestamp: Utc::now(),
        };
        let attr = tracker.process_fill(&fill_a).unwrap();
        assert_eq!(attr.strategy_id, "strategy-a");
        assert_eq!(attr.new_position, dec!(1));

        // Fill for strategy B
        let fill_b = Fill {
            order_id: "exch-2".to_string(),
            client_order_id: "order-2".to_string(),
            instrument_id: "BTC-USD".to_string(),
            side: FillSide::Sell,
            quantity: dec!(0.5),
            price: dec!(50100),
            fee: dec!(2.5),
            fee_currency: "USD".to_string(),
            timestamp: Utc::now(),
        };
        let attr = tracker.process_fill(&fill_b).unwrap();
        assert_eq!(attr.strategy_id, "strategy-b");
        assert_eq!(attr.new_position, dec!(-0.5));

        // Check net portfolio
        let net = tracker.portfolio_position("BTC-USD").unwrap();
        assert_eq!(net.quantity, dec!(0.5)); // 1 - 0.5

        // Check individual strategies
        let pos_a = tracker.strategy_position("strategy-a", "BTC-USD").unwrap();
        assert_eq!(pos_a.quantity, dec!(1));

        let pos_b = tracker.strategy_position("strategy-b", "BTC-USD").unwrap();
        assert_eq!(pos_b.quantity, dec!(-0.5));
    }

    #[test]
    fn test_pnl_by_strategy() {
        let mut tracker = PositionTracker::new();

        tracker.register_order("order-1", "strategy-a");
        tracker.register_order("order-2", "strategy-a");

        // Buy 1 @ 100
        tracker.process_fill(&Fill {
            order_id: "exch-1".to_string(),
            client_order_id: "order-1".to_string(),
            instrument_id: "BTC-USD".to_string(),
            side: FillSide::Buy,
            quantity: dec!(1),
            price: dec!(100),
            fee: dec!(1),
            fee_currency: "USD".to_string(),
            timestamp: Utc::now(),
        });

        // Sell 1 @ 120 (profit 20)
        tracker.process_fill(&Fill {
            order_id: "exch-2".to_string(),
            client_order_id: "order-2".to_string(),
            instrument_id: "BTC-USD".to_string(),
            side: FillSide::Sell,
            quantity: dec!(1),
            price: dec!(120),
            fee: dec!(1),
            fee_currency: "USD".to_string(),
            timestamp: Utc::now(),
        });

        let mark_prices: HashMap<String, Decimal> = HashMap::new();
        let pnl = tracker.pnl_by_strategy(&mark_prices);

        let strategy_pnl = pnl.get("strategy-a").unwrap();
        assert_eq!(strategy_pnl.realized_pnl, dec!(20));
        assert_eq!(strategy_pnl.total_fees, dec!(2));
        assert_eq!(strategy_pnl.net_pnl(), dec!(18)); // 20 - 2
    }
}
