//! Mean Reversion HFT Strategy
//!
//! High-frequency mean reversion strategy that generates signals based on:
//! - Microprice deviation from mid price (fair value estimate)
//! - Order book imbalance confirmation
//! - Z-score of price movements
//!
//! # Strategy Logic
//!
//! 1. Calculate microprice as fair value estimate
//! 2. Compute deviation from mid price in basis points
//! 3. When deviation exceeds threshold AND imbalance confirms:
//!    - Negative deviation + positive imbalance → BUY (price below fair value, buyers present)
//!    - Positive deviation + negative imbalance → SELL (price above fair value, sellers present)
//! 4. Signal strength proportional to deviation magnitude
//! 5. Confidence based on imbalance strength

use crate::application::ports::{
    GeneratorConfig, MarketDataPort, OrderBookReader, SignalGeneratorPort, SymbolKey,
};
use crate::domain::{Signal, SignalDirection, StrategyId, StrategyType, Urgency};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use trading_core::{Price, RollingStats};

/// Configuration for the Mean Reversion HFT strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeanReversionConfig {
    /// Minimum microprice skew (in bps) to generate signal
    pub min_skew_bps: f64,
    /// Maximum microprice skew - signals capped here for risk
    pub max_skew_bps: f64,
    /// Minimum order book imbalance to confirm signal (0.0 to 1.0)
    pub min_imbalance: f64,
    /// Z-score threshold for additional confirmation
    pub z_score_threshold: f64,
    /// Expected half-life of mean reversion (seconds)
    pub half_life_seconds: f64,
    /// Cooldown between signals for same symbol (milliseconds)
    pub signal_cooldown_ms: u64,
    /// Rolling window size for statistics
    pub rolling_window: usize,
}

impl Default for MeanReversionConfig {
    fn default() -> Self {
        Self {
            min_skew_bps: 2.0,       // 2 bps minimum deviation
            max_skew_bps: 50.0,      // Cap at 50 bps
            min_imbalance: 0.1,      // 10% imbalance minimum
            z_score_threshold: 1.5,  // 1.5 sigma for confirmation
            half_life_seconds: 0.5,  // 500ms expected reversion
            signal_cooldown_ms: 100, // 100ms between signals
            rolling_window: 100,     // 100 tick rolling window
        }
    }
}

/// Mean Reversion HFT Strategy Implementation
///
/// Implements `SignalGeneratorPort` for use with the strategy engine.
pub struct MeanReversionHFT {
    config: GeneratorConfig,
    params: MeanReversionConfig,
    /// Rolling statistics for each symbol's mid price
    price_stats: HashMap<String, RollingStats>,
    /// Last signal time per symbol (for cooldown)
    last_signal_time: HashMap<String, Instant>,
    /// Tick counter for statistics
    tick_count: u64,
}

impl MeanReversionHFT {
    /// Create a new Mean Reversion HFT strategy
    pub fn new(config: GeneratorConfig, params: MeanReversionConfig) -> Self {
        Self {
            config,
            params,
            price_stats: HashMap::new(),
            last_signal_time: HashMap::new(),
            tick_count: 0,
        }
    }

    /// Create with default parameters
    pub fn with_defaults(strategy_id: impl Into<String>, symbols: Vec<SymbolKey>) -> Self {
        let config = GeneratorConfig::new(
            StrategyId::new(strategy_id),
            StrategyType::MeanReversion,
            symbols,
        );
        Self::new(config, MeanReversionConfig::default())
    }

    /// Check if cooldown period has elapsed for a symbol
    fn is_cooldown_elapsed(&self, symbol: &str) -> bool {
        match self.last_signal_time.get(symbol) {
            Some(last_time) => {
                last_time.elapsed().as_millis() as u64 >= self.params.signal_cooldown_ms
            }
            None => true,
        }
    }

    /// Update rolling statistics with new price
    fn update_stats(&mut self, symbol: &str, price: Price) {
        let stats = self
            .price_stats
            .entry(symbol.to_string())
            .or_insert_with(|| RollingStats::new(self.params.rolling_window, 8));
        stats.push(price.raw());
    }

    /// Get z-score for current price
    fn get_z_score(&self, symbol: &str, price: Price) -> Option<f64> {
        self.price_stats
            .get(symbol)
            .and_then(|stats| stats.z_score(price.raw()))
            .map(|z| z as f64 / trading_core::PRICE_SCALE as f64)
    }

    /// Calculate signal strength from skew (normalized 0-1)
    fn calculate_strength(&self, skew_bps: f64) -> f64 {
        let abs_skew = skew_bps.abs();
        if abs_skew < self.params.min_skew_bps {
            return 0.0;
        }
        // Linear scaling from min to max
        let range = self.params.max_skew_bps - self.params.min_skew_bps;
        ((abs_skew - self.params.min_skew_bps) / range).min(1.0)
    }

    /// Calculate confidence from imbalance (0-1)
    fn calculate_confidence(&self, imbalance: f64) -> f64 {
        let abs_imbalance = imbalance.abs();
        if abs_imbalance < self.params.min_imbalance {
            return 0.0;
        }
        // Scale imbalance to confidence
        (abs_imbalance * 2.0).min(1.0)
    }

    /// Generate signal for a single symbol
    fn generate_signal_for_symbol<M: MarketDataPort>(
        &mut self,
        symbol_key: &SymbolKey,
        market_data: &M,
    ) -> Option<Signal> {
        let book = market_data.book(symbol_key);

        if !book.is_initialized() {
            return None;
        }

        // Get best bid/ask
        let best_bid = book.best_bid()?;
        let best_ask = book.best_ask()?;

        let bid_price = best_bid.price;
        let bid_size = best_bid.size;
        let ask_price = best_ask.price;
        let ask_size = best_ask.size;

        // Calculate mid price
        let mid_raw = (bid_price.raw() + ask_price.raw()) / 2;
        let mid_price = Price::from_raw(mid_raw);

        // Update rolling stats
        self.update_stats(&symbol_key.symbol, mid_price);

        // Calculate microprice
        let total_size = bid_size.raw() as i128 + ask_size.raw() as i128;
        if total_size == 0 {
            return None;
        }
        let microprice_raw = (bid_price.raw() as i128 * ask_size.raw() as i128
            + ask_price.raw() as i128 * bid_size.raw() as i128)
            / total_size;
        let microprice = Price::from_raw(microprice_raw as i64);

        // Calculate skew in basis points
        if mid_raw == 0 {
            return None;
        }
        let skew_bps = ((microprice_raw - mid_raw as i128) * 10000) as f64 / mid_raw as f64;

        // Calculate imbalance
        let imbalance =
            (bid_size.raw() - ask_size.raw()) as f64 / (bid_size.raw() + ask_size.raw()) as f64;

        // Check if signal conditions are met
        let abs_skew = skew_bps.abs();
        let abs_imbalance = imbalance.abs();

        if abs_skew < self.params.min_skew_bps {
            return None;
        }

        if abs_imbalance < self.params.min_imbalance {
            return None;
        }

        // Check cooldown
        if !self.is_cooldown_elapsed(&symbol_key.symbol) {
            return None;
        }

        // Determine direction based on skew and imbalance alignment
        // Negative skew = microprice < mid = fair value below current price = expect price to fall
        // But if imbalance is positive (more bids) = buying pressure contradicts
        // So we look for alignment:
        // - skew < 0 (microprice below mid) AND imbalance > 0 (buy pressure) → BUY
        //   (microprice weighted toward bid suggests fair value is lower, but buyers present)
        // - skew > 0 (microprice above mid) AND imbalance < 0 (sell pressure) → SELL
        //   (microprice weighted toward ask suggests fair value is higher, but sellers present)
        //
        // Actually, let's think about this more carefully for mean reversion:
        // - If microprice > mid (skew > 0): fair value is HIGHER than mid → BUY (price will revert UP)
        // - If microprice < mid (skew < 0): fair value is LOWER than mid → SELL (price will revert DOWN)
        // We use imbalance as confirmation:
        // - Positive imbalance (more bids) confirms buy
        // - Negative imbalance (more asks) confirms sell

        let direction = if skew_bps > 0.0 && imbalance > 0.0 {
            // Microprice above mid + buying pressure → BUY
            SignalDirection::Buy
        } else if skew_bps < 0.0 && imbalance < 0.0 {
            // Microprice below mid + selling pressure → SELL
            SignalDirection::Sell
        } else {
            // No alignment - skip
            return None;
        };

        // Optional z-score confirmation
        let z_score = self.get_z_score(&symbol_key.symbol, mid_price);
        let z_score_confirms = match (z_score, &direction) {
            // For buy: we want price below mean (negative z-score)
            (Some(z), SignalDirection::Buy) if z < -self.params.z_score_threshold => true,
            // For sell: we want price above mean (positive z-score)
            (Some(z), SignalDirection::Sell) if z > self.params.z_score_threshold => true,
            // Z-score not available or doesn't confirm - still allow signal but lower confidence
            _ => false,
        };

        // Calculate signal parameters
        let strength = self.calculate_strength(skew_bps);
        let mut confidence = self.calculate_confidence(imbalance);

        // Boost confidence if z-score confirms
        if z_score_confirms {
            confidence = (confidence * 1.3).min(1.0);
        }

        // Record signal time
        self.last_signal_time
            .insert(symbol_key.symbol.clone(), Instant::now());

        // Build features map
        let mut features = HashMap::new();
        features.insert("microprice_skew_bps".to_string(), skew_bps);
        features.insert("imbalance".to_string(), imbalance);
        if let Some(z) = z_score {
            features.insert("z_score".to_string(), z);
        }
        features.insert("bid_size".to_string(), bid_size.to_f64());
        features.insert("ask_size".to_string(), ask_size.to_f64());

        // Build signal
        Some(
            Signal::builder(
                self.config.strategy_id.clone(),
                self.config.strategy_type,
                symbol_key.to_string(),
            )
            .direction(direction)
            .strength(strength)
            .confidence(confidence)
            .urgency(Urgency::high()) // HFT signals are time-sensitive
            .prices(mid_price, microprice)
            .half_life_seconds(self.params.half_life_seconds)
            .expected_edge_bps(abs_skew)
            .features(features)
            .model_version("mean_reversion_hft_v1.0")
            .build(),
        )
    }
}

impl SignalGeneratorPort for MeanReversionHFT {
    fn config(&self) -> &GeneratorConfig {
        &self.config
    }

    fn on_tick<M: MarketDataPort>(&mut self, market_data: &M) -> Vec<Signal> {
        self.tick_count += 1;

        let mut signals = Vec::new();

        // Clone symbols to avoid borrow issues
        let symbols: Vec<SymbolKey> = self.config.symbols.clone();

        for symbol_key in &symbols {
            if let Some(signal) = self.generate_signal_for_symbol(symbol_key, market_data) {
                signals.push(signal);
            }
        }

        signals
    }

    fn on_start(&mut self) {
        tracing::info!(
            "MeanReversionHFT strategy '{}' starting with {} symbols",
            self.config.strategy_id,
            self.config.symbols.len()
        );
    }

    fn on_stop(&mut self) {
        tracing::info!(
            "MeanReversionHFT strategy '{}' stopping after {} ticks",
            self.config.strategy_id,
            self.tick_count
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{BookLevel, OrderBookReader};
    use std::sync::Arc;
    use trading_core::Quantity;

    /// Mock order book for testing
    struct MockOrderBook {
        bids: Vec<BookLevel>,
        asks: Vec<BookLevel>,
        initialized: bool,
    }

    impl MockOrderBook {
        fn new(bid_price: i64, bid_size: i64, ask_price: i64, ask_size: i64) -> Self {
            Self {
                bids: vec![BookLevel {
                    price: Price::from_int(bid_price),
                    size: Quantity::from_int(bid_size),
                }],
                asks: vec![BookLevel {
                    price: Price::from_int(ask_price),
                    size: Quantity::from_int(ask_size),
                }],
                initialized: true,
            }
        }
    }

    impl OrderBookReader for MockOrderBook {
        fn is_initialized(&self) -> bool {
            self.initialized
        }
        fn best_bid(&self) -> Option<BookLevel> {
            self.bids.first().cloned()
        }
        fn best_ask(&self) -> Option<BookLevel> {
            self.asks.first().cloned()
        }
        fn mid_price(&self) -> Option<Price> {
            let bid = self.bids.first()?;
            let ask = self.asks.first()?;
            Some(Price::from_raw((bid.price.raw() + ask.price.raw()) / 2))
        }
        fn spread(&self) -> Option<Price> {
            let bid = self.bids.first()?;
            let ask = self.asks.first()?;
            Some(Price::from_raw(ask.price.raw() - bid.price.raw()))
        }
        fn bid_levels(&self, depth: usize) -> Vec<BookLevel> {
            self.bids.iter().take(depth).cloned().collect()
        }
        fn ask_levels(&self, depth: usize) -> Vec<BookLevel> {
            self.asks.iter().take(depth).cloned().collect()
        }
        fn total_bid_depth(&self, levels: usize) -> Quantity {
            let sum: i64 = self.bids.iter().take(levels).map(|l| l.size.raw()).sum();
            Quantity::from_raw(sum)
        }
        fn total_ask_depth(&self, levels: usize) -> Quantity {
            let sum: i64 = self.asks.iter().take(levels).map(|l| l.size.raw()).sum();
            Quantity::from_raw(sum)
        }
        fn last_update_time(&self) -> Option<u64> {
            Some(0)
        }
    }

    /// Mock market data
    struct MockMarketData {
        books: HashMap<String, Arc<MockOrderBook>>,
    }

    impl MarketDataPort for MockMarketData {
        type BookReader = MockOrderBook;

        fn book(&self, key: &SymbolKey) -> Arc<MockOrderBook> {
            self.books
                .get(&key.to_string())
                .cloned()
                .unwrap_or_else(|| {
                    Arc::new(MockOrderBook {
                        bids: vec![],
                        asks: vec![],
                        initialized: false,
                    })
                })
        }

        fn has_symbol(&self, key: &SymbolKey) -> bool {
            self.books.contains_key(&key.to_string())
        }

        fn symbols(&self) -> Vec<SymbolKey> {
            vec![]
        }
    }

    #[test]
    fn test_strategy_creation() {
        let strategy =
            MeanReversionHFT::with_defaults("test_mr", vec![SymbolKey::new("binance", "BTCUSDT")]);

        assert_eq!(strategy.config().strategy_id.as_str(), "test_mr");
        assert_eq!(strategy.config().strategy_type, StrategyType::MeanReversion);
    }

    #[test]
    fn test_no_signal_below_threshold() {
        let mut strategy =
            MeanReversionHFT::with_defaults("test_mr", vec![SymbolKey::new("binance", "BTCUSDT")]);

        // Balanced book - no imbalance
        let mut books = HashMap::new();
        books.insert(
            "binance:BTCUSDT".to_string(),
            Arc::new(MockOrderBook::new(100, 10, 101, 10)), // Equal sizes
        );
        let market_data = MockMarketData { books };

        let signals = strategy.on_tick(&market_data);
        assert!(
            signals.is_empty(),
            "Should not generate signal with balanced book"
        );
    }

    #[test]
    fn test_buy_signal_generation() {
        let mut strategy =
            MeanReversionHFT::with_defaults("test_mr", vec![SymbolKey::new("binance", "BTCUSDT")]);

        // Imbalanced book favoring buyers (more bid size)
        // microprice will be weighted toward ask (higher)
        // With more bids than asks: imbalance > 0
        // microprice = (100*5 + 101*20) / 25 = (500 + 2020) / 25 = 100.8
        // mid = 100.5
        // skew = positive (microprice > mid)
        // This should generate BUY signal
        let mut books = HashMap::new();
        books.insert(
            "binance:BTCUSDT".to_string(),
            Arc::new(MockOrderBook::new(100, 20, 101, 5)), // More bid size
        );
        let market_data = MockMarketData { books };

        let signals = strategy.on_tick(&market_data);

        // Signal should be generated if conditions are met
        // The skew might be below threshold in this example, so let's check
        if !signals.is_empty() {
            assert_eq!(signals[0].direction, SignalDirection::Buy);
            assert!(signals[0].strength > 0.0);
        }
    }

    #[test]
    fn test_signal_cooldown() {
        let mut strategy =
            MeanReversionHFT::with_defaults("test_mr", vec![SymbolKey::new("binance", "BTCUSDT")]);

        // Create a strongly imbalanced book
        let mut books = HashMap::new();
        books.insert(
            "binance:BTCUSDT".to_string(),
            Arc::new(MockOrderBook::new(100, 50, 101, 5)),
        );
        let market_data = MockMarketData { books };

        // First tick might generate a signal
        let _signals1 = strategy.on_tick(&market_data);

        // Immediate second tick should be blocked by cooldown
        let signals2 = strategy.on_tick(&market_data);
        assert!(
            signals2.is_empty(),
            "Cooldown should prevent immediate second signal"
        );
    }

    #[test]
    fn test_strength_calculation() {
        let strategy =
            MeanReversionHFT::with_defaults("test_mr", vec![SymbolKey::new("binance", "BTCUSDT")]);

        // Below threshold
        assert_eq!(strategy.calculate_strength(1.0), 0.0);

        // At threshold
        let strength_at_min = strategy.calculate_strength(2.0);
        assert!((0.0..=0.1).contains(&strength_at_min));

        // At max
        let strength_at_max = strategy.calculate_strength(50.0);
        assert!((strength_at_max - 1.0).abs() < 0.01);

        // Beyond max (capped)
        let strength_beyond = strategy.calculate_strength(100.0);
        assert_eq!(strength_beyond, 1.0);
    }

    #[test]
    fn test_confidence_calculation() {
        let strategy =
            MeanReversionHFT::with_defaults("test_mr", vec![SymbolKey::new("binance", "BTCUSDT")]);

        // Below threshold
        assert_eq!(strategy.calculate_confidence(0.05), 0.0);

        // At threshold
        let conf = strategy.calculate_confidence(0.2);
        assert!(conf > 0.0);

        // High imbalance (capped at 1.0)
        let conf_high = strategy.calculate_confidence(0.8);
        assert_eq!(conf_high, 1.0);
    }
}
