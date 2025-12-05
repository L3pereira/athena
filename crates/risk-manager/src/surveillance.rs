//! Market Surveillance
//!
//! Monitors order books for:
//! - Spoofing (large orders that get pulled)
//! - Layering (stacked orders creating false depth)
//! - Unusual spread behavior
//! - Book imbalances
//!
//! Both Risk Manager and Strategies can use this:
//! - Risk: Hard stops on manipulation detection
//! - Strategy: Soft adjustments (wider spreads, smaller size)

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Trait for market surveillance - can be used by Risk and Strategy
pub trait MarketSurveillance: Send + Sync {
    /// Get overall market quality for an instrument
    fn market_quality(&self, instrument_id: &str) -> MarketQuality;

    /// Get detailed book quality metrics
    fn book_quality(&self, instrument_id: &str) -> Option<BookQualityMetrics>;

    /// Check for active alerts
    fn active_alerts(&self, instrument_id: &str) -> Vec<SurveillanceAlert>;

    /// Is it safe to trade this instrument?
    fn is_safe_to_trade(&self, instrument_id: &str) -> bool {
        let quality = self.market_quality(instrument_id);
        quality.score >= dec!(0.5) && !quality.manipulation_suspected
    }
}

/// Overall market quality assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketQuality {
    /// Instrument
    pub instrument_id: String,
    /// Overall quality score (0.0 = bad, 1.0 = excellent)
    pub score: Decimal,
    /// Is manipulation suspected?
    pub manipulation_suspected: bool,
    /// Spread quality (relative to historical)
    pub spread_quality: SpreadQuality,
    /// Depth quality
    pub depth_quality: DepthQuality,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

impl Default for MarketQuality {
    fn default() -> Self {
        Self {
            instrument_id: String::new(),
            score: Decimal::ONE,
            manipulation_suspected: false,
            spread_quality: SpreadQuality::Normal,
            depth_quality: DepthQuality::Normal,
            timestamp: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpreadQuality {
    /// Spread is tighter than normal
    Tight,
    /// Spread is normal
    Normal,
    /// Spread is wider than normal
    Wide,
    /// Spread is extremely wide (warning)
    VeryWide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DepthQuality {
    /// More depth than normal
    Deep,
    /// Normal depth
    Normal,
    /// Less depth than normal
    Shallow,
    /// Very little depth (warning)
    VeryShallow,
}

/// Detailed book quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookQualityMetrics {
    /// Instrument
    pub instrument_id: String,
    /// Current spread in bps
    pub spread_bps: Decimal,
    /// Average spread (rolling)
    pub avg_spread_bps: Decimal,
    /// Spread percentile (0-100, higher = wider than usual)
    pub spread_percentile: Decimal,
    /// Top of book size (bid + ask)
    pub tob_size: Decimal,
    /// Average TOB size
    pub avg_tob_size: Decimal,
    /// Book imbalance (-1.0 = all bids, 1.0 = all asks)
    pub imbalance: Decimal,
    /// Number of price levels with significant size
    pub significant_levels: usize,
    /// Estimated market impact for a standard size (in bps)
    pub est_impact_bps: Decimal,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Surveillance alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurveillanceAlert {
    /// Alert type
    pub alert_type: AlertType,
    /// Instrument
    pub instrument_id: String,
    /// Severity
    pub severity: AlertSeverity,
    /// Description
    pub description: String,
    /// When detected
    pub timestamp: DateTime<Utc>,
    /// When it expires (if temporary)
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertType {
    /// Large orders appearing and disappearing
    SpoofingDetected,
    /// Stacked orders creating false depth
    LayeringDetected,
    /// Abnormal spread behavior
    AbnormalSpread,
    /// Sudden loss of liquidity
    LiquidityDrain,
    /// Unusual price movement
    AbnormalPriceMove,
    /// Book crossed or locked
    BookCrossed,
    /// General market stress
    MarketStress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AlertSeverity {
    /// Informational only
    Info,
    /// Warning - consider reducing exposure
    Warning,
    /// Critical - should stop trading
    Critical,
}

/// Basic surveillance implementation
pub struct BasicSurveillance {
    /// Spread history by instrument
    spread_history: std::collections::HashMap<String, VecDeque<SpreadSample>>,
    /// Active alerts
    alerts: std::collections::HashMap<String, Vec<SurveillanceAlert>>,
    /// Configuration
    config: SurveillanceConfig,
}

struct SpreadSample {
    spread_bps: Decimal,
    tob_size: Decimal,
    timestamp: DateTime<Utc>,
}

/// Configuration for surveillance
#[derive(Debug, Clone)]
pub struct SurveillanceConfig {
    /// How many samples to keep for averaging
    pub history_samples: usize,
    /// Spread percentile above which is "wide"
    pub wide_spread_percentile: Decimal,
    /// Spread percentile above which is "very wide"
    pub very_wide_spread_percentile: Decimal,
    /// TOB size below which is "shallow"
    pub shallow_depth_ratio: Decimal,
    /// Alert expiry duration
    pub alert_expiry: Duration,
    /// Spoof detection: size threshold
    pub spoof_size_threshold: Decimal,
    /// Spoof detection: time window
    pub spoof_time_window: Duration,
}

impl Default for SurveillanceConfig {
    fn default() -> Self {
        Self {
            history_samples: 1000,
            wide_spread_percentile: dec!(75),
            very_wide_spread_percentile: dec!(95),
            shallow_depth_ratio: dec!(0.3),
            alert_expiry: Duration::minutes(5),
            spoof_size_threshold: dec!(10), // 10x normal size
            spoof_time_window: Duration::seconds(30),
        }
    }
}

impl BasicSurveillance {
    pub fn new(config: SurveillanceConfig) -> Self {
        Self {
            spread_history: std::collections::HashMap::new(),
            alerts: std::collections::HashMap::new(),
            config,
        }
    }

    /// Update with new book data
    pub fn update_book(&mut self, instrument_id: &str, spread_bps: Decimal, tob_size: Decimal) {
        let history = self
            .spread_history
            .entry(instrument_id.to_string())
            .or_default();

        history.push_back(SpreadSample {
            spread_bps,
            tob_size,
            timestamp: Utc::now(),
        });

        // Keep only recent samples
        while history.len() > self.config.history_samples {
            history.pop_front();
        }

        // Check for alerts
        self.check_alerts(instrument_id);
    }

    /// Check for surveillance alerts
    fn check_alerts(&mut self, instrument_id: &str) {
        let now = Utc::now();

        // Remove expired alerts
        if let Some(alerts) = self.alerts.get_mut(instrument_id) {
            alerts.retain(|a| a.expires_at.map(|exp| now < exp).unwrap_or(true));
        }

        // Check for new alerts based on book data
        // Collect alerts first to avoid borrow issues
        let mut new_alerts = Vec::new();

        if let Some(history) = self.spread_history.get(instrument_id)
            && let Some(latest) = history.back()
        {
            // Check for very wide spread
            let avg_spread = self.calculate_avg_spread(history);
            if latest.spread_bps > avg_spread * dec!(3) {
                new_alerts.push(SurveillanceAlert {
                    alert_type: AlertType::AbnormalSpread,
                    instrument_id: instrument_id.to_string(),
                    severity: AlertSeverity::Warning,
                    description: format!(
                        "Spread {:.2} bps is 3x normal ({:.2} bps)",
                        latest.spread_bps, avg_spread
                    ),
                    timestamp: now,
                    expires_at: Some(now + self.config.alert_expiry),
                });
            }

            // Check for liquidity drain
            let avg_size = self.calculate_avg_tob_size(history);
            if latest.tob_size < avg_size * self.config.shallow_depth_ratio {
                new_alerts.push(SurveillanceAlert {
                    alert_type: AlertType::LiquidityDrain,
                    instrument_id: instrument_id.to_string(),
                    severity: AlertSeverity::Warning,
                    description: format!(
                        "TOB size {:.4} is {:.0}% of normal",
                        latest.tob_size,
                        (latest.tob_size / avg_size * dec!(100))
                    ),
                    timestamp: now,
                    expires_at: Some(now + self.config.alert_expiry),
                });
            }
        }

        // Now add alerts
        for alert in new_alerts {
            self.add_alert(alert);
        }
    }

    fn calculate_avg_spread(&self, history: &VecDeque<SpreadSample>) -> Decimal {
        if history.is_empty() {
            return Decimal::ZERO;
        }
        let sum: Decimal = history.iter().map(|s| s.spread_bps).sum();
        sum / Decimal::from(history.len())
    }

    fn calculate_avg_tob_size(&self, history: &VecDeque<SpreadSample>) -> Decimal {
        if history.is_empty() {
            return Decimal::ONE;
        }
        let sum: Decimal = history.iter().map(|s| s.tob_size).sum();
        sum / Decimal::from(history.len())
    }

    fn add_alert(&mut self, alert: SurveillanceAlert) {
        let alerts = self.alerts.entry(alert.instrument_id.clone()).or_default();

        // Don't duplicate same alert type
        if !alerts.iter().any(|a| a.alert_type == alert.alert_type) {
            alerts.push(alert);
        }
    }

    /// Calculate spread percentile
    fn spread_percentile(&self, instrument_id: &str, current_spread: Decimal) -> Decimal {
        let Some(history) = self.spread_history.get(instrument_id) else {
            return dec!(50); // Default to median
        };

        if history.is_empty() {
            return dec!(50);
        }

        let count_below = history
            .iter()
            .filter(|s| s.spread_bps < current_spread)
            .count();

        Decimal::from(count_below) / Decimal::from(history.len()) * dec!(100)
    }
}

impl MarketSurveillance for BasicSurveillance {
    fn market_quality(&self, instrument_id: &str) -> MarketQuality {
        let alerts = self.active_alerts(instrument_id);
        let has_critical = alerts.iter().any(|a| a.severity == AlertSeverity::Critical);
        let has_warning = alerts.iter().any(|a| a.severity == AlertSeverity::Warning);

        let manipulation_suspected = alerts.iter().any(|a| {
            matches!(
                a.alert_type,
                AlertType::SpoofingDetected | AlertType::LayeringDetected
            )
        });

        // Calculate score
        let base_score = if has_critical {
            dec!(0.2)
        } else if has_warning {
            dec!(0.6)
        } else {
            dec!(1.0)
        };

        // Get spread quality
        let spread_quality = self
            .spread_history
            .get(instrument_id)
            .and_then(|h| h.back())
            .map(|latest| {
                let percentile = self.spread_percentile(instrument_id, latest.spread_bps);
                if percentile > self.config.very_wide_spread_percentile {
                    SpreadQuality::VeryWide
                } else if percentile > self.config.wide_spread_percentile {
                    SpreadQuality::Wide
                } else if percentile < dec!(25) {
                    SpreadQuality::Tight
                } else {
                    SpreadQuality::Normal
                }
            })
            .unwrap_or(SpreadQuality::Normal);

        // Get depth quality
        let depth_quality = self
            .spread_history
            .get(instrument_id)
            .and_then(|h| {
                let avg = self.calculate_avg_tob_size(h);
                h.back().map(|latest| {
                    let ratio = latest.tob_size / avg;
                    if ratio < self.config.shallow_depth_ratio {
                        DepthQuality::VeryShallow
                    } else if ratio < dec!(0.7) {
                        DepthQuality::Shallow
                    } else if ratio > dec!(1.5) {
                        DepthQuality::Deep
                    } else {
                        DepthQuality::Normal
                    }
                })
            })
            .unwrap_or(DepthQuality::Normal);

        MarketQuality {
            instrument_id: instrument_id.to_string(),
            score: base_score,
            manipulation_suspected,
            spread_quality,
            depth_quality,
            timestamp: Utc::now(),
        }
    }

    fn book_quality(&self, instrument_id: &str) -> Option<BookQualityMetrics> {
        let history = self.spread_history.get(instrument_id)?;
        let latest = history.back()?;

        Some(BookQualityMetrics {
            instrument_id: instrument_id.to_string(),
            spread_bps: latest.spread_bps,
            avg_spread_bps: self.calculate_avg_spread(history),
            spread_percentile: self.spread_percentile(instrument_id, latest.spread_bps),
            tob_size: latest.tob_size,
            avg_tob_size: self.calculate_avg_tob_size(history),
            imbalance: Decimal::ZERO, // Would need bid/ask breakdown
            significant_levels: 0,    // Would need full book
            est_impact_bps: latest.spread_bps / dec!(2), // Simplified estimate
            timestamp: latest.timestamp,
        })
    }

    fn active_alerts(&self, instrument_id: &str) -> Vec<SurveillanceAlert> {
        let now = Utc::now();
        self.alerts
            .get(instrument_id)
            .map(|alerts| {
                alerts
                    .iter()
                    .filter(|a| a.expires_at.map(|exp| now < exp).unwrap_or(true))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_surveillance() {
        let config = SurveillanceConfig::default();
        let mut surveillance = BasicSurveillance::new(config);

        // Normal conditions
        for _ in 0..100 {
            surveillance.update_book("BTC-USD", dec!(5), dec!(10));
        }

        let quality = surveillance.market_quality("BTC-USD");
        assert_eq!(quality.score, Decimal::ONE);
        assert!(!quality.manipulation_suspected);
        assert!(surveillance.is_safe_to_trade("BTC-USD"));
    }

    #[test]
    fn test_wide_spread_alert() {
        let config = SurveillanceConfig::default();
        let mut surveillance = BasicSurveillance::new(config);

        // Build baseline
        for _ in 0..100 {
            surveillance.update_book("BTC-USD", dec!(5), dec!(10));
        }

        // Sudden wide spread
        surveillance.update_book("BTC-USD", dec!(20), dec!(10));

        let alerts = surveillance.active_alerts("BTC-USD");
        assert!(!alerts.is_empty());
        assert!(
            alerts
                .iter()
                .any(|a| a.alert_type == AlertType::AbnormalSpread)
        );
    }

    #[test]
    fn test_liquidity_drain_alert() {
        let config = SurveillanceConfig::default();
        let mut surveillance = BasicSurveillance::new(config);

        // Build baseline
        for _ in 0..100 {
            surveillance.update_book("BTC-USD", dec!(5), dec!(10));
        }

        // Sudden loss of liquidity
        surveillance.update_book("BTC-USD", dec!(5), dec!(1));

        let alerts = surveillance.active_alerts("BTC-USD");
        assert!(!alerts.is_empty());
        assert!(
            alerts
                .iter()
                .any(|a| a.alert_type == AlertType::LiquidityDrain)
        );
    }

    #[test]
    fn test_market_quality_degrades() {
        let config = SurveillanceConfig::default();
        let mut surveillance = BasicSurveillance::new(config);

        // Build baseline
        for _ in 0..100 {
            surveillance.update_book("BTC-USD", dec!(5), dec!(10));
        }

        // Abnormal conditions
        surveillance.update_book("BTC-USD", dec!(50), dec!(1));

        let quality = surveillance.market_quality("BTC-USD");
        assert!(quality.score < Decimal::ONE);
    }
}
