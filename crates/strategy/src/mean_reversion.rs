//! Mean Reversion Taker Strategy
//!
//! An informed trading strategy that:
//! - Receives fair value events from an event feed
//! - Trades when market price deviates from fair value
//! - Buys when market is below fair value (expects price to rise)
//! - Sells when market is above fair value (expects price to fall)
//! - Closes positions when price reverts to fair value

use crate::{
    events::MarketEvent,
    orderbook::LocalOrderBook,
    strategy::{Action, Strategy, StrategyContext},
};
use async_trait::async_trait;
use athena_gateway::messages::{
    market_data::OrderBookUpdate,
    order::{OrderRequest, OrderSide},
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Configuration for mean reversion taker
#[derive(Debug, Clone)]
pub struct MeanReversionConfig {
    /// Instrument to trade
    pub instrument_id: String,
    /// Threshold deviation to enter position (in basis points)
    /// e.g., 50 = enter when price is 0.5% away from fair value
    pub entry_threshold_bps: Decimal,
    /// Threshold to exit position (in basis points)
    /// e.g., 10 = exit when price is within 0.1% of fair value
    pub exit_threshold_bps: Decimal,
    /// Size to trade on each signal
    pub trade_size: Decimal,
    /// Maximum position size (absolute value)
    pub max_position: Decimal,
}

impl Default for MeanReversionConfig {
    fn default() -> Self {
        Self {
            instrument_id: "BTC-USD".to_string(),
            entry_threshold_bps: dec!(50), // 0.5% deviation to enter
            exit_threshold_bps: dec!(10),  // 0.1% to exit
            trade_size: dec!(0.1),         // Trade 0.1 units at a time
            max_position: dec!(1),         // Max 1 unit position
        }
    }
}

/// Mean reversion taker strategy
///
/// Trades when market price deviates from fair value (from event feed)
pub struct MeanReversionTaker {
    /// Strategy configuration
    config: MeanReversionConfig,
    /// Current fair value (from event feed)
    fair_value: Option<Decimal>,
    /// Local order book
    order_book: LocalOrderBook,
    /// Current position (tracked locally)
    position: Decimal,
    /// Order counter for unique IDs
    order_counter: u64,
}

impl MeanReversionTaker {
    /// Create a new mean reversion taker
    pub fn new(config: MeanReversionConfig) -> Self {
        let order_book = LocalOrderBook::new(&config.instrument_id);
        Self {
            config,
            fair_value: None,
            order_book,
            position: Decimal::ZERO,
            order_counter: 0,
        }
    }

    /// Generate next order ID
    fn next_order_id(&mut self) -> String {
        self.order_counter += 1;
        format!("mr-{}", self.order_counter)
    }

    /// Calculate deviation from fair value in basis points
    fn calculate_deviation(&self) -> Option<Decimal> {
        let fair = self.fair_value?;
        let mid = self.order_book.mid_price()?;

        if fair.is_zero() {
            return None;
        }

        // Deviation = (mid - fair) / fair * 10000
        Some((mid - fair) / fair * dec!(10000))
    }

    /// Generate trading signal based on deviation
    fn generate_signal(&mut self) -> Option<Action> {
        let deviation_bps = self.calculate_deviation()?;
        let mid = self.order_book.mid_price()?;

        // Entry logic: enter when deviation exceeds threshold
        if deviation_bps > self.config.entry_threshold_bps {
            // Market is ABOVE fair value -> SELL (expect reversion down)
            if self.position > -self.config.max_position {
                let qty = self
                    .config
                    .trade_size
                    .min(self.config.max_position + self.position);
                if qty > Decimal::ZERO {
                    log::info!(
                        "[MeanReversion] SELL signal: deviation={:.2}bps, mid={}, fair={:?}",
                        deviation_bps,
                        mid,
                        self.fair_value
                    );
                    return Some(self.create_market_order(OrderSide::Sell, qty));
                }
            }
        } else if deviation_bps < -self.config.entry_threshold_bps {
            // Market is BELOW fair value -> BUY (expect reversion up)
            if self.position < self.config.max_position {
                let qty = self
                    .config
                    .trade_size
                    .min(self.config.max_position - self.position);
                if qty > Decimal::ZERO {
                    log::info!(
                        "[MeanReversion] BUY signal: deviation={:.2}bps, mid={}, fair={:?}",
                        deviation_bps,
                        mid,
                        self.fair_value
                    );
                    return Some(self.create_market_order(OrderSide::Buy, qty));
                }
            }
        }

        // Exit logic: close position when price reverts to fair value
        if self.position != Decimal::ZERO && deviation_bps.abs() < self.config.exit_threshold_bps {
            let (side, qty) = if self.position > Decimal::ZERO {
                (OrderSide::Sell, self.position)
            } else {
                (OrderSide::Buy, -self.position)
            };

            log::info!(
                "[MeanReversion] EXIT signal: closing position {}, deviation={:.2}bps",
                self.position,
                deviation_bps
            );
            return Some(self.create_market_order(side, qty));
        }

        None
    }

    /// Create a market order action
    fn create_market_order(&mut self, side: OrderSide, quantity: Decimal) -> Action {
        let order_id = self.next_order_id();
        Action::SubmitOrder(OrderRequest::market(
            order_id,
            &self.config.instrument_id,
            side,
            quantity,
        ))
    }
}

#[async_trait]
impl Strategy for MeanReversionTaker {
    fn name(&self) -> &str {
        "MeanReversionTaker"
    }

    async fn on_book_update(
        &mut self,
        update: &OrderBookUpdate,
        _ctx: &StrategyContext<'_>,
    ) -> Vec<Action> {
        // Update local order book
        if update.instrument_id() == self.config.instrument_id {
            self.order_book.apply_update(update);

            // Generate signal if we have fair value
            if self.fair_value.is_some()
                && let Some(action) = self.generate_signal()
            {
                return vec![action];
            }
        }

        Vec::new()
    }

    async fn on_event(&mut self, event: &MarketEvent, _ctx: &StrategyContext<'_>) -> Vec<Action> {
        match event {
            MarketEvent::FairValue {
                instrument_id,
                price,
            } => {
                if instrument_id == &self.config.instrument_id {
                    self.fair_value = Some(*price);
                    log::debug!(
                        "[MeanReversion] Fair value updated: {} = {}",
                        instrument_id,
                        price
                    );

                    // Check for trading opportunity with new fair value
                    if let Some(action) = self.generate_signal() {
                        return vec![action];
                    }
                }
            }
            MarketEvent::Sentiment {
                instrument_id,
                score,
            } => {
                // Could incorporate sentiment into trading logic
                if instrument_id == &self.config.instrument_id {
                    log::debug!(
                        "[MeanReversion] Sentiment update: {} = {}",
                        instrument_id,
                        score
                    );
                }
            }
            _ => {}
        }

        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use athena_gateway::messages::market_data::BookLevel;

    #[test]
    fn test_config_defaults() {
        let config = MeanReversionConfig::default();
        assert_eq!(config.entry_threshold_bps, dec!(50));
        assert_eq!(config.exit_threshold_bps, dec!(10));
    }

    #[test]
    fn test_strategy_creation() {
        let config = MeanReversionConfig {
            instrument_id: "ETH-USD".to_string(),
            ..Default::default()
        };
        let strategy = MeanReversionTaker::new(config);
        assert_eq!(strategy.name(), "MeanReversionTaker");
        assert!(strategy.fair_value.is_none());
    }

    #[tokio::test]
    async fn test_no_signal_without_fair_value() {
        let mut strategy = MeanReversionTaker::new(MeanReversionConfig::default());

        // Create a book update
        let update = OrderBookUpdate::snapshot(
            "BTC-USD",
            vec![BookLevel::new(dec!(49000), dec!(1))],
            vec![BookLevel::new(dec!(51000), dec!(1))],
            1,
            0,
        );

        let ctx = StrategyContext {
            books: &Default::default(),
            positions: &Default::default(),
            open_orders: &Default::default(),
        };

        let actions = strategy.on_book_update(&update, &ctx).await;
        assert!(actions.is_empty(), "Should not signal without fair value");
    }

    #[tokio::test]
    async fn test_buy_signal_when_below_fair() {
        let mut strategy = MeanReversionTaker::new(MeanReversionConfig {
            instrument_id: "BTC-USD".to_string(),
            entry_threshold_bps: dec!(50), // 0.5%
            trade_size: dec!(0.1),
            ..Default::default()
        });

        // Set fair value at 50000
        strategy.fair_value = Some(dec!(50000));

        // Market mid at 49500 (1% below fair value)
        let update = OrderBookUpdate::snapshot(
            "BTC-USD",
            vec![BookLevel::new(dec!(49400), dec!(1))],
            vec![BookLevel::new(dec!(49600), dec!(1))],
            1,
            0,
        );

        let ctx = StrategyContext {
            books: &Default::default(),
            positions: &Default::default(),
            open_orders: &Default::default(),
        };

        let actions = strategy.on_book_update(&update, &ctx).await;
        assert_eq!(actions.len(), 1);

        if let Action::SubmitOrder(order) = &actions[0] {
            assert_eq!(order.side, OrderSide::Buy);
            assert_eq!(order.quantity, dec!(0.1));
        } else {
            panic!("Expected SubmitOrder action");
        }
    }

    #[tokio::test]
    async fn test_sell_signal_when_above_fair() {
        let mut strategy = MeanReversionTaker::new(MeanReversionConfig {
            instrument_id: "BTC-USD".to_string(),
            entry_threshold_bps: dec!(50), // 0.5%
            trade_size: dec!(0.1),
            ..Default::default()
        });

        // Set fair value at 50000
        strategy.fair_value = Some(dec!(50000));

        // Market mid at 50500 (1% above fair value)
        let update = OrderBookUpdate::snapshot(
            "BTC-USD",
            vec![BookLevel::new(dec!(50400), dec!(1))],
            vec![BookLevel::new(dec!(50600), dec!(1))],
            1,
            0,
        );

        let ctx = StrategyContext {
            books: &Default::default(),
            positions: &Default::default(),
            open_orders: &Default::default(),
        };

        let actions = strategy.on_book_update(&update, &ctx).await;
        assert_eq!(actions.len(), 1);

        if let Action::SubmitOrder(order) = &actions[0] {
            assert_eq!(order.side, OrderSide::Sell);
            assert_eq!(order.quantity, dec!(0.1));
        } else {
            panic!("Expected SubmitOrder action");
        }
    }
}
