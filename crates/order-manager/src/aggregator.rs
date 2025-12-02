//! Signal Aggregation
//!
//! Aggregates signals from multiple strategies into a target portfolio.
//!
//! ## Aggregation Methods
//!
//! 1. **Simple Average**: Equal weight to all signals
//! 2. **Alpha-Weighted**: Weight by expected return (alpha)
//! 3. **Confidence-Weighted**: Weight by strategy confidence
//! 4. **Combined**: alpha * confidence weighting
//!
//! ## Position Netting
//!
//! Multiple strategies may have opposing views:
//! - Strategy A: long 2 BTC
//! - Strategy B: short 1 BTC
//! - Net target: long 1 BTC

use crate::signal::{Signal, Urgency};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;

/// Configuration for signal aggregation
#[derive(Debug, Clone)]
pub struct AggregatorConfig {
    /// How to weight signals
    pub weighting: WeightingMethod,
    /// Strategy weight overrides (e.g., give more weight to trusted strategies)
    pub strategy_weights: HashMap<String, Decimal>,
    /// Maximum position per instrument (applied post-aggregation)
    pub max_positions: HashMap<String, Decimal>,
    /// Default max position if not specified
    pub default_max_position: Decimal,
}

impl Default for AggregatorConfig {
    fn default() -> Self {
        Self {
            weighting: WeightingMethod::Combined,
            strategy_weights: HashMap::new(),
            max_positions: HashMap::new(),
            default_max_position: dec!(100), // Default to 100 units max
        }
    }
}

/// How to weight multiple signals
#[derive(Debug, Clone, Copy, Default)]
pub enum WeightingMethod {
    /// Simple average of all signals
    Average,
    /// Weight by alpha (expected return)
    AlphaWeighted,
    /// Weight by confidence
    ConfidenceWeighted,
    /// Weight by alpha * confidence (default)
    #[default]
    Combined,
}

/// Aggregated target position for one instrument
#[derive(Debug, Clone)]
pub struct PortfolioTarget {
    /// Instrument
    pub instrument_id: String,
    /// Target position
    pub target_position: Decimal,
    /// Combined alpha (weighted avg)
    pub combined_alpha: Option<Decimal>,
    /// Combined confidence
    pub combined_confidence: Decimal,
    /// Most urgent signal's urgency
    pub urgency: Urgency,
    /// Most restrictive stop loss
    pub stop_loss: Option<Decimal>,
    /// Most restrictive take profit
    pub take_profit: Option<Decimal>,
    /// Signals that contributed
    pub contributing_signals: Vec<SignalContribution>,
    /// When this target was computed
    pub timestamp: DateTime<Utc>,
}

/// How one signal contributed to the target
#[derive(Debug, Clone)]
pub struct SignalContribution {
    pub strategy_id: String,
    pub signal_position: Decimal,
    pub weight: Decimal,
    pub weighted_contribution: Decimal,
}

/// Aggregates signals into portfolio targets
pub struct SignalAggregator {
    config: AggregatorConfig,
    /// Active signals by (strategy_id, instrument_id)
    active_signals: HashMap<(String, String), Signal>,
}

impl SignalAggregator {
    pub fn new(config: AggregatorConfig) -> Self {
        Self {
            config,
            active_signals: HashMap::new(),
        }
    }

    /// Update or add a signal from a strategy
    pub fn update_signal(&mut self, signal: Signal) {
        // Remove expired signals first
        self.cleanup_expired();

        let key = (signal.strategy_id.clone(), signal.instrument_id.clone());
        self.active_signals.insert(key, signal);
    }

    /// Remove all signals from a strategy
    pub fn remove_strategy(&mut self, strategy_id: &str) {
        self.active_signals.retain(|(sid, _), _| sid != strategy_id);
    }

    /// Remove all signals for an instrument
    pub fn remove_instrument(&mut self, instrument_id: &str) {
        self.active_signals
            .retain(|(_, iid), _| iid != instrument_id);
    }

    /// Remove expired signals
    pub fn cleanup_expired(&mut self) {
        let now = Utc::now();
        self.active_signals
            .retain(|_, signal| !signal.is_expired_at(now));
    }

    /// Get all current portfolio targets
    pub fn compute_targets(&self) -> Vec<PortfolioTarget> {
        // Group signals by instrument
        let mut by_instrument: HashMap<String, Vec<&Signal>> = HashMap::new();
        for signal in self.active_signals.values() {
            by_instrument
                .entry(signal.instrument_id.clone())
                .or_default()
                .push(signal);
        }

        // Aggregate each instrument
        let mut targets = Vec::new();
        for (instrument_id, signals) in by_instrument {
            if let Some(target) = self.aggregate_signals(&instrument_id, &signals) {
                targets.push(target);
            }
        }

        targets
    }

    /// Aggregate signals for one instrument
    fn aggregate_signals(
        &self,
        instrument_id: &str,
        signals: &[&Signal],
    ) -> Option<PortfolioTarget> {
        if signals.is_empty() {
            return None;
        }

        // Calculate weights for each signal
        let weights: Vec<Decimal> = signals.iter().map(|s| self.calculate_weight(s)).collect();

        let total_weight: Decimal = weights.iter().sum();
        if total_weight.is_zero() {
            return None;
        }

        // Normalize weights
        let norm_weights: Vec<Decimal> = weights.iter().map(|w| *w / total_weight).collect();

        // Calculate weighted position
        let mut weighted_position = Decimal::ZERO;
        let mut contributions = Vec::new();

        for (i, signal) in signals.iter().enumerate() {
            let contribution = signal.target_position * norm_weights[i];
            weighted_position += contribution;

            contributions.push(SignalContribution {
                strategy_id: signal.strategy_id.clone(),
                signal_position: signal.target_position,
                weight: norm_weights[i],
                weighted_contribution: contribution,
            });
        }

        // Apply max position limit
        let max_pos = self
            .config
            .max_positions
            .get(instrument_id)
            .copied()
            .unwrap_or(self.config.default_max_position);
        weighted_position = weighted_position.clamp(-max_pos, max_pos);

        // Combine alpha (weighted average)
        let combined_alpha = self.combine_alpha(signals, &norm_weights);

        // Combined confidence
        let combined_confidence: Decimal = signals
            .iter()
            .zip(norm_weights.iter())
            .map(|(s, w)| s.confidence * w)
            .sum();

        // Most urgent signal determines urgency
        let urgency = signals
            .iter()
            .map(|s| s.urgency)
            .max_by_key(|u| match u {
                Urgency::Passive => 0,
                Urgency::Normal => 1,
                Urgency::Aggressive => 2,
                Urgency::Immediate => 3,
            })
            .unwrap_or(Urgency::Normal);

        // Most restrictive stop loss (closest to current position direction)
        let stop_loss = self.combine_stop_loss(signals, weighted_position);

        // Most restrictive take profit
        let take_profit = self.combine_take_profit(signals, weighted_position);

        Some(PortfolioTarget {
            instrument_id: instrument_id.to_string(),
            target_position: weighted_position,
            combined_alpha,
            combined_confidence,
            urgency,
            stop_loss,
            take_profit,
            contributing_signals: contributions,
            timestamp: Utc::now(),
        })
    }

    /// Calculate weight for a single signal
    fn calculate_weight(&self, signal: &Signal) -> Decimal {
        // Base weight from strategy override
        let strategy_weight = self
            .config
            .strategy_weights
            .get(&signal.strategy_id)
            .copied()
            .unwrap_or(Decimal::ONE);

        // Weight based on configured method
        let method_weight = match self.config.weighting {
            WeightingMethod::Average => Decimal::ONE,
            WeightingMethod::AlphaWeighted => signal.alpha.unwrap_or(Decimal::ONE).abs(),
            WeightingMethod::ConfidenceWeighted => signal.confidence,
            WeightingMethod::Combined => {
                signal.alpha.unwrap_or(Decimal::ONE).abs() * signal.confidence
            }
        };

        strategy_weight * method_weight
    }

    /// Combine alpha values
    fn combine_alpha(&self, signals: &[&Signal], weights: &[Decimal]) -> Option<Decimal> {
        let mut sum = Decimal::ZERO;
        let mut count = 0;

        for (signal, weight) in signals.iter().zip(weights.iter()) {
            if let Some(alpha) = signal.alpha {
                sum += alpha * weight;
                count += 1;
            }
        }

        if count > 0 { Some(sum) } else { None }
    }

    /// Combine stop losses - pick most conservative
    fn combine_stop_loss(&self, signals: &[&Signal], target: Decimal) -> Option<Decimal> {
        let stops: Vec<Decimal> = signals.iter().filter_map(|s| s.stop_loss).collect();

        if stops.is_empty() {
            return None;
        }

        // For long positions, stop loss is below current - want highest stop (most conservative)
        // For short positions, stop loss is above current - want lowest stop
        if target > Decimal::ZERO {
            stops.into_iter().max()
        } else {
            stops.into_iter().min()
        }
    }

    /// Combine take profits - pick most conservative
    fn combine_take_profit(&self, signals: &[&Signal], target: Decimal) -> Option<Decimal> {
        let tps: Vec<Decimal> = signals.iter().filter_map(|s| s.take_profit).collect();

        if tps.is_empty() {
            return None;
        }

        // For long positions, take profit is above - want lowest (most conservative)
        // For short positions, take profit is below - want highest
        if target > Decimal::ZERO {
            tps.into_iter().min()
        } else {
            tps.into_iter().max()
        }
    }

    /// Get active signals count
    pub fn active_signal_count(&self) -> usize {
        self.active_signals.len()
    }

    /// Get signals for a specific instrument
    pub fn signals_for_instrument(&self, instrument_id: &str) -> Vec<&Signal> {
        self.active_signals
            .values()
            .filter(|s| s.instrument_id == instrument_id)
            .collect()
    }
}

// Helper extension for Signal
impl Signal {
    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.map(|exp| now > exp).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn make_signal(strategy: &str, instrument: &str, position: Decimal) -> Signal {
        Signal::new(strategy, instrument, position)
    }

    #[test]
    fn test_simple_aggregation() {
        let config = AggregatorConfig {
            weighting: WeightingMethod::Average,
            ..Default::default()
        };
        let mut agg = SignalAggregator::new(config);

        // Two strategies, same view
        agg.update_signal(make_signal("a", "BTC-USD", dec!(10)));
        agg.update_signal(make_signal("b", "BTC-USD", dec!(10)));

        let targets = agg.compute_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_position, dec!(10));
    }

    #[test]
    fn test_opposing_signals_net_out() {
        let config = AggregatorConfig {
            weighting: WeightingMethod::Average,
            ..Default::default()
        };
        let mut agg = SignalAggregator::new(config);

        // Opposing views should net
        agg.update_signal(make_signal("long-strategy", "BTC-USD", dec!(10)));
        agg.update_signal(make_signal("short-strategy", "BTC-USD", dec!(-10)));

        let targets = agg.compute_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_position, dec!(0)); // Net flat
    }

    #[test]
    fn test_confidence_weighting() {
        let config = AggregatorConfig {
            weighting: WeightingMethod::ConfidenceWeighted,
            ..Default::default()
        };
        let mut agg = SignalAggregator::new(config);

        // Strategy A: long 10 with 90% confidence
        // Strategy B: short 10 with 10% confidence
        // Weighted: 10*0.9 + (-10)*0.1 = 9 - 1 = 8
        agg.update_signal(Signal::new("a", "BTC-USD", dec!(10)).with_confidence(dec!(0.9)));
        agg.update_signal(Signal::new("b", "BTC-USD", dec!(-10)).with_confidence(dec!(0.1)));

        let targets = agg.compute_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_position, dec!(8));
    }

    #[test]
    fn test_max_position_limit() {
        let mut max_pos = HashMap::new();
        max_pos.insert("BTC-USD".to_string(), dec!(5));

        let config = AggregatorConfig {
            weighting: WeightingMethod::Average,
            max_positions: max_pos,
            ..Default::default()
        };
        let mut agg = SignalAggregator::new(config);

        // Signal wants 100, but max is 5
        agg.update_signal(make_signal("a", "BTC-USD", dec!(100)));

        let targets = agg.compute_targets();
        assert_eq!(targets[0].target_position, dec!(5));
    }

    #[test]
    fn test_urgency_highest_wins() {
        let config = AggregatorConfig::default();
        let mut agg = SignalAggregator::new(config);

        agg.update_signal(Signal::new("a", "BTC-USD", dec!(1)).with_urgency(Urgency::Passive));
        agg.update_signal(Signal::new("b", "BTC-USD", dec!(1)).with_urgency(Urgency::Immediate));

        let targets = agg.compute_targets();
        assert_eq!(targets[0].urgency, Urgency::Immediate);
    }

    #[test]
    fn test_multiple_instruments() {
        let config = AggregatorConfig::default();
        let mut agg = SignalAggregator::new(config);

        agg.update_signal(make_signal("a", "BTC-USD", dec!(10)));
        agg.update_signal(make_signal("a", "ETH-USD", dec!(100)));

        let targets = agg.compute_targets();
        assert_eq!(targets.len(), 2);
    }

    #[test]
    fn test_signal_update_replaces() {
        let config = AggregatorConfig::default();
        let mut agg = SignalAggregator::new(config);

        // First signal
        agg.update_signal(make_signal("a", "BTC-USD", dec!(10)));

        // Same strategy updates signal
        agg.update_signal(make_signal("a", "BTC-USD", dec!(20)));

        let targets = agg.compute_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_position, dec!(20));
    }

    #[test]
    fn test_contributions_tracked() {
        let config = AggregatorConfig {
            weighting: WeightingMethod::Average,
            ..Default::default()
        };
        let mut agg = SignalAggregator::new(config);

        agg.update_signal(make_signal("a", "BTC-USD", dec!(10)));
        agg.update_signal(make_signal("b", "BTC-USD", dec!(20)));

        let targets = agg.compute_targets();
        assert_eq!(targets[0].contributing_signals.len(), 2);

        // Equal weights
        for contrib in &targets[0].contributing_signals {
            assert_eq!(contrib.weight, dec!(0.5));
        }
    }
}
