//! Impact Model Calibration
//!
//! Fits impact model parameters from historical execution data.
//!
//! # Calibration Methods
//!
//! - **Least Squares**: Minimize squared error between predicted and actual impact
//! - **Robust Regression**: Downweight outliers
//! - **Cross-Validation**: Evaluate out-of-sample performance
//!
//! # Usage
//!
//! ```rust,ignore
//! let calibrator = ImpactCalibrator::new(CalibrationConfig::default());
//! let result = calibrator.calibrate_square_root(&historical_data);
//! let model = result.to_model();
//! ```

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::models::{AlmgrenChrissParams, SquareRootParams, sqrt_decimal};

/// Configuration for calibration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationConfig {
    /// Minimum data points required for calibration
    pub min_data_points: usize,
    /// Whether to use robust regression (downweight outliers)
    pub robust_regression: bool,
    /// Outlier threshold (in standard deviations)
    pub outlier_threshold: Decimal,
    /// Cross-validation folds
    pub cv_folds: usize,
    /// L2 regularization strength
    pub regularization: Decimal,
    /// Whether to fit intercept
    pub fit_intercept: bool,
}

impl Default for CalibrationConfig {
    fn default() -> Self {
        Self {
            min_data_points: 30,
            robust_regression: true,
            outlier_threshold: dec!(3),
            cv_folds: 5,
            regularization: dec!(0.01),
            fit_intercept: false,
        }
    }
}

/// Historical execution data point for calibration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionDataPoint {
    /// Instrument
    pub instrument_id: String,
    /// Order quantity
    pub quantity: Decimal,
    /// Average daily volume at time of execution
    pub adv: Decimal,
    /// Volatility at time of execution
    pub volatility: Decimal,
    /// Spread at time of execution (bps)
    pub spread_bps: Decimal,
    /// Realized market impact (bps)
    pub realized_impact_bps: Decimal,
    /// Execution duration (seconds)
    pub duration_secs: u64,
    /// Is buy order
    pub is_buy: bool,
}

impl ExecutionDataPoint {
    /// Calculate participation rate
    pub fn participation_rate(&self) -> Decimal {
        if self.adv > Decimal::ZERO {
            self.quantity / self.adv
        } else {
            Decimal::ZERO
        }
    }

    /// Calculate sqrt(participation)
    pub fn sqrt_participation(&self) -> Decimal {
        sqrt_decimal(self.participation_rate())
    }
}

/// Result of model calibration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationResult {
    /// Calibrated model type
    pub model_type: CalibratedModelType,
    /// Calibrated parameters
    pub parameters: HashMap<String, Decimal>,
    /// In-sample R-squared
    pub r_squared: Decimal,
    /// Root mean squared error (bps)
    pub rmse_bps: Decimal,
    /// Mean absolute error (bps)
    pub mae_bps: Decimal,
    /// Number of data points used
    pub n_samples: usize,
    /// Cross-validation score (if computed)
    pub cv_score: Option<Decimal>,
    /// Parameter standard errors
    pub std_errors: HashMap<String, Decimal>,
}

impl CalibrationResult {
    /// Convert to Square Root model parameters
    pub fn to_square_root_params(&self) -> Option<SquareRootParams> {
        if self.model_type != CalibratedModelType::SquareRoot {
            return None;
        }

        Some(SquareRootParams {
            y_coefficient: *self.parameters.get("y_coefficient")?,
            temporary_fraction: self
                .parameters
                .get("temporary_fraction")
                .copied()
                .unwrap_or(dec!(0.7)),
            decay_half_life_secs: 300,
        })
    }

    /// Convert to Almgren-Chriss parameters
    pub fn to_almgren_chriss_params(&self) -> Option<AlmgrenChrissParams> {
        if self.model_type != CalibratedModelType::AlmgrenChriss {
            return None;
        }

        Some(AlmgrenChrissParams {
            gamma: *self.parameters.get("gamma")?,
            eta: *self.parameters.get("eta")?,
            eta_exponent: self
                .parameters
                .get("eta_exponent")
                .copied()
                .unwrap_or(dec!(0.6)),
            volatility_scale: dec!(1),
            adv_exponent: dec!(0.5),
        })
    }

    /// Get calibration quality grade
    pub fn quality_grade(&self) -> CalibrationQuality {
        if self.r_squared > dec!(0.7) && self.n_samples >= 100 {
            CalibrationQuality::Excellent
        } else if self.r_squared > dec!(0.5) && self.n_samples >= 50 {
            CalibrationQuality::Good
        } else if self.r_squared > dec!(0.3) && self.n_samples >= 30 {
            CalibrationQuality::Fair
        } else {
            CalibrationQuality::Poor
        }
    }

    /// Generate calibration report
    pub fn report(&self) -> String {
        format!(
            "Calibration Results ({:?})\n\
             Parameters: {:?}\n\
             R²: {:.3}\n\
             RMSE: {:.2} bps\n\
             MAE: {:.2} bps\n\
             Samples: {}\n\
             CV Score: {}\n\
             Quality: {:?}",
            self.model_type,
            self.parameters,
            self.r_squared,
            self.rmse_bps,
            self.mae_bps,
            self.n_samples,
            self.cv_score
                .map(|s| format!("{:.3}", s))
                .unwrap_or_else(|| "N/A".to_string()),
            self.quality_grade(),
        )
    }
}

/// Type of calibrated model
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalibratedModelType {
    SquareRoot,
    AlmgrenChriss,
    Kyle,
}

/// Quality of calibration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationQuality {
    Excellent,
    Good,
    Fair,
    Poor,
}

/// Impact model calibrator
pub struct ImpactCalibrator {
    config: CalibrationConfig,
}

impl ImpactCalibrator {
    /// Create new calibrator
    pub fn new(config: CalibrationConfig) -> Self {
        Self { config }
    }

    /// Create with default configuration
    pub fn default_calibrator() -> Self {
        Self::new(CalibrationConfig::default())
    }

    /// Calibrate Square Root model
    ///
    /// Fits: Impact = Y × σ × √(Q/V)
    /// Returns calibrated Y coefficient
    pub fn calibrate_square_root(&self, data: &[ExecutionDataPoint]) -> CalibrationResult {
        if data.len() < self.config.min_data_points {
            return self.insufficient_data_result(CalibratedModelType::SquareRoot, data.len());
        }

        // Prepare features: σ × √(Q/V)
        let features: Vec<Decimal> = data
            .iter()
            .map(|d| d.volatility * d.sqrt_participation())
            .collect();

        let targets: Vec<Decimal> = data.iter().map(|d| d.realized_impact_bps).collect();

        // Apply robust regression if configured
        let weights = if self.config.robust_regression {
            self.calculate_robust_weights(&targets)
        } else {
            vec![Decimal::ONE; data.len()]
        };

        // Weighted least squares: Y = Σ(w × y × x) / Σ(w × x²)
        let (y_coefficient, stats) =
            self.weighted_least_squares_single(&features, &targets, &weights);

        let mut parameters = HashMap::new();
        parameters.insert("y_coefficient".to_string(), y_coefficient);

        // Estimate temporary fraction (simplified - would need more data)
        parameters.insert("temporary_fraction".to_string(), dec!(0.7));

        CalibrationResult {
            model_type: CalibratedModelType::SquareRoot,
            parameters,
            r_squared: stats.r_squared,
            rmse_bps: stats.rmse,
            mae_bps: stats.mae,
            n_samples: data.len(),
            cv_score: self.cross_validate_square_root(data),
            std_errors: {
                let mut se = HashMap::new();
                se.insert("y_coefficient".to_string(), stats.std_error);
                se
            },
        }
    }

    /// Calibrate Almgren-Chriss model
    ///
    /// Fits: Impact = γ × Q/√V + η × (Q/V)^α × σ
    /// More complex multivariate regression
    pub fn calibrate_almgren_chriss(&self, data: &[ExecutionDataPoint]) -> CalibrationResult {
        if data.len() < self.config.min_data_points {
            return self.insufficient_data_result(CalibratedModelType::AlmgrenChriss, data.len());
        }

        // For Almgren-Chriss, we need to separate temporary and permanent impact
        // This requires data on how impact decays, which we don't have in simple execution data
        // So we use a simplified approach: fit total impact as function of participation and vol

        // Fit: Impact = (gamma + eta × vol) × √participation × vol_scale
        let features: Vec<(Decimal, Decimal)> = data
            .iter()
            .map(|d| {
                let sqrt_part = d.sqrt_participation();
                let vol_scaled = sqrt_part * d.volatility;
                (sqrt_part, vol_scaled)
            })
            .collect();

        let targets: Vec<Decimal> = data.iter().map(|d| d.realized_impact_bps).collect();

        let weights = if self.config.robust_regression {
            self.calculate_robust_weights(&targets)
        } else {
            vec![Decimal::ONE; data.len()]
        };

        // Two-variable regression
        let (gamma, eta, stats) = self.weighted_least_squares_two(&features, &targets, &weights);

        let mut parameters = HashMap::new();
        parameters.insert("gamma".to_string(), gamma / dec!(10000)); // Scale down
        parameters.insert("eta".to_string(), eta / dec!(10000));
        parameters.insert("eta_exponent".to_string(), dec!(0.6));

        CalibrationResult {
            model_type: CalibratedModelType::AlmgrenChriss,
            parameters,
            r_squared: stats.r_squared,
            rmse_bps: stats.rmse,
            mae_bps: stats.mae,
            n_samples: data.len(),
            cv_score: None, // Would need separate CV implementation
            std_errors: HashMap::new(),
        }
    }

    /// Calibrate Kyle (linear) model
    ///
    /// Fits: Impact = λ × (Q/V)
    pub fn calibrate_kyle(&self, data: &[ExecutionDataPoint]) -> CalibrationResult {
        if data.len() < self.config.min_data_points {
            return self.insufficient_data_result(CalibratedModelType::Kyle, data.len());
        }

        // Features: Q/V (participation rate)
        let features: Vec<Decimal> = data.iter().map(|d| d.participation_rate()).collect();

        let targets: Vec<Decimal> = data.iter().map(|d| d.realized_impact_bps).collect();

        let weights = if self.config.robust_regression {
            self.calculate_robust_weights(&targets)
        } else {
            vec![Decimal::ONE; data.len()]
        };

        // λ in bps per 100% participation
        let (lambda, stats) = self.weighted_least_squares_single(&features, &targets, &weights);

        let mut parameters = HashMap::new();
        parameters.insert("lambda".to_string(), lambda);

        CalibrationResult {
            model_type: CalibratedModelType::Kyle,
            parameters,
            r_squared: stats.r_squared,
            rmse_bps: stats.rmse,
            mae_bps: stats.mae,
            n_samples: data.len(),
            cv_score: None,
            std_errors: {
                let mut se = HashMap::new();
                se.insert("lambda".to_string(), stats.std_error);
                se
            },
        }
    }

    /// Calculate robust regression weights (Huber-like)
    fn calculate_robust_weights(&self, targets: &[Decimal]) -> Vec<Decimal> {
        if targets.is_empty() {
            return vec![];
        }

        // Calculate median and MAD
        let mut sorted: Vec<Decimal> = targets.to_vec();
        sorted.sort();
        let median = sorted[sorted.len() / 2];

        let deviations: Vec<Decimal> = targets.iter().map(|t| (*t - median).abs()).collect();
        let mut sorted_devs = deviations.clone();
        sorted_devs.sort();
        let mad = sorted_devs[sorted_devs.len() / 2];

        // Scale MAD to estimate standard deviation
        let scale = mad * dec!(1.4826); // For normal distribution

        // Calculate weights
        targets
            .iter()
            .map(|t| {
                if scale.is_zero() {
                    return Decimal::ONE;
                }
                let z = (*t - median).abs() / scale;
                if z <= self.config.outlier_threshold {
                    Decimal::ONE
                } else {
                    // Downweight outliers
                    self.config.outlier_threshold / z
                }
            })
            .collect()
    }

    /// Weighted least squares for single variable
    fn weighted_least_squares_single(
        &self,
        features: &[Decimal],
        targets: &[Decimal],
        weights: &[Decimal],
    ) -> (Decimal, RegressionStats) {
        let n = features.len();
        if n == 0 {
            return (Decimal::ZERO, RegressionStats::default());
        }

        // Weighted sums
        let mut sum_wx2 = Decimal::ZERO;
        let mut sum_wxy = Decimal::ZERO;
        let mut sum_wy = Decimal::ZERO;
        let mut sum_w = Decimal::ZERO;

        for i in 0..n {
            let w = weights[i];
            let x = features[i];
            let y = targets[i];

            sum_wx2 += w * x * x;
            sum_wxy += w * x * y;
            sum_wy += w * y;
            sum_w += w;
        }

        // Add regularization
        sum_wx2 += self.config.regularization;

        // Coefficient
        let coef = if sum_wx2 > Decimal::ZERO {
            sum_wxy / sum_wx2
        } else {
            Decimal::ZERO
        };

        // Calculate statistics
        let y_mean = if sum_w > Decimal::ZERO {
            sum_wy / sum_w
        } else {
            Decimal::ZERO
        };

        let mut ss_res = Decimal::ZERO;
        let mut ss_tot = Decimal::ZERO;
        let mut abs_errors = Decimal::ZERO;

        for i in 0..n {
            let pred = features[i] * coef;
            let resid = targets[i] - pred;
            ss_res += weights[i] * resid * resid;
            ss_tot += weights[i] * (targets[i] - y_mean) * (targets[i] - y_mean);
            abs_errors += weights[i] * resid.abs();
        }

        let r_squared = if ss_tot > Decimal::ZERO {
            Decimal::ONE - ss_res / ss_tot
        } else {
            Decimal::ZERO
        };

        let rmse = sqrt_decimal(ss_res / sum_w.max(dec!(1)));
        let mae = abs_errors / sum_w.max(dec!(1));

        // Standard error of coefficient
        let mse = ss_res / Decimal::from((n.max(2) - 1) as u64);
        let std_error = sqrt_decimal(mse / sum_wx2.max(dec!(0.0001)));

        (
            coef,
            RegressionStats {
                r_squared,
                rmse,
                mae,
                std_error,
            },
        )
    }

    /// Weighted least squares for two variables
    fn weighted_least_squares_two(
        &self,
        features: &[(Decimal, Decimal)],
        targets: &[Decimal],
        weights: &[Decimal],
    ) -> (Decimal, Decimal, RegressionStats) {
        let n = features.len();
        if n == 0 {
            return (Decimal::ZERO, Decimal::ZERO, RegressionStats::default());
        }

        // Normal equations for 2 variables
        let mut sum_wx1x1 = Decimal::ZERO;
        let mut sum_wx2x2 = Decimal::ZERO;
        let mut sum_wx1x2 = Decimal::ZERO;
        let mut sum_wx1y = Decimal::ZERO;
        let mut sum_wx2y = Decimal::ZERO;
        let mut sum_wy = Decimal::ZERO;
        let mut sum_w = Decimal::ZERO;

        for i in 0..n {
            let w = weights[i];
            let (x1, x2) = features[i];
            let y = targets[i];

            sum_wx1x1 += w * x1 * x1;
            sum_wx2x2 += w * x2 * x2;
            sum_wx1x2 += w * x1 * x2;
            sum_wx1y += w * x1 * y;
            sum_wx2y += w * x2 * y;
            sum_wy += w * y;
            sum_w += w;
        }

        // Add regularization
        sum_wx1x1 += self.config.regularization;
        sum_wx2x2 += self.config.regularization;

        // Solve 2x2 system via Cramer's rule
        let det = sum_wx1x1 * sum_wx2x2 - sum_wx1x2 * sum_wx1x2;

        let (coef1, coef2) = if det.abs() > dec!(0.0001) {
            let c1 = (sum_wx1y * sum_wx2x2 - sum_wx2y * sum_wx1x2) / det;
            let c2 = (sum_wx1x1 * sum_wx2y - sum_wx1x2 * sum_wx1y) / det;
            (c1, c2)
        } else {
            (Decimal::ZERO, Decimal::ZERO)
        };

        // Calculate R² and errors
        let y_mean = if sum_w > Decimal::ZERO {
            sum_wy / sum_w
        } else {
            Decimal::ZERO
        };

        let mut ss_res = Decimal::ZERO;
        let mut ss_tot = Decimal::ZERO;
        let mut abs_errors = Decimal::ZERO;

        for i in 0..n {
            let (x1, x2) = features[i];
            let pred = x1 * coef1 + x2 * coef2;
            let resid = targets[i] - pred;
            ss_res += weights[i] * resid * resid;
            ss_tot += weights[i] * (targets[i] - y_mean) * (targets[i] - y_mean);
            abs_errors += weights[i] * resid.abs();
        }

        let r_squared = if ss_tot > Decimal::ZERO {
            Decimal::ONE - ss_res / ss_tot
        } else {
            Decimal::ZERO
        };

        let rmse = sqrt_decimal(ss_res / sum_w.max(dec!(1)));
        let mae = abs_errors / sum_w.max(dec!(1));

        (
            coef1,
            coef2,
            RegressionStats {
                r_squared,
                rmse,
                mae,
                std_error: Decimal::ZERO, // Complex to compute for multivariate
            },
        )
    }

    /// Cross-validation for square root model
    fn cross_validate_square_root(&self, data: &[ExecutionDataPoint]) -> Option<Decimal> {
        if data.len() < self.config.cv_folds * 5 {
            return None;
        }

        let fold_size = data.len() / self.config.cv_folds;
        let mut cv_scores = Vec::new();

        for fold in 0..self.config.cv_folds {
            let start = fold * fold_size;
            let end = if fold == self.config.cv_folds - 1 {
                data.len()
            } else {
                start + fold_size
            };

            // Train on all except this fold
            let train: Vec<_> = data[..start]
                .iter()
                .chain(data[end..].iter())
                .cloned()
                .collect();

            let test: Vec<_> = data[start..end].to_vec();

            if train.len() < self.config.min_data_points {
                continue;
            }

            // Fit on training data (simplified)
            let result = self.calibrate_square_root(&train);

            // Evaluate on test data
            let y_coef = *result.parameters.get("y_coefficient").unwrap_or(&dec!(0.3));
            let mut sse = Decimal::ZERO;

            for point in &test {
                let pred = y_coef * point.volatility * point.sqrt_participation() * dec!(10000);
                let error = point.realized_impact_bps - pred;
                sse += error * error;
            }

            let mse = sse / Decimal::from(test.len() as u64);
            cv_scores.push(Decimal::ONE - mse / dec!(1000)); // Normalize
        }

        if cv_scores.is_empty() {
            None
        } else {
            let mean_score: Decimal =
                cv_scores.iter().sum::<Decimal>() / Decimal::from(cv_scores.len() as u64);
            Some(mean_score)
        }
    }

    /// Create result for insufficient data
    fn insufficient_data_result(
        &self,
        model_type: CalibratedModelType,
        n_samples: usize,
    ) -> CalibrationResult {
        CalibrationResult {
            model_type,
            parameters: HashMap::new(),
            r_squared: Decimal::ZERO,
            rmse_bps: Decimal::ZERO,
            mae_bps: Decimal::ZERO,
            n_samples,
            cv_score: None,
            std_errors: HashMap::new(),
        }
    }
}

impl Default for ImpactCalibrator {
    fn default() -> Self {
        Self::default_calibrator()
    }
}

/// Regression statistics
#[derive(Debug, Clone, Default)]
struct RegressionStats {
    r_squared: Decimal,
    rmse: Decimal,
    mae: Decimal,
    std_error: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_test_data(n: usize, true_y: Decimal) -> Vec<ExecutionDataPoint> {
        // Generate synthetic data following square root model
        // Impact = Y × σ × √(Q/V) expressed in bps
        // The regression fits: impact_bps = Y_coef × (σ × √participation)
        (0..n)
            .map(|i| {
                // Vary participation from 0.1% to 5%
                let participation =
                    dec!(0.001) + dec!(0.049) * Decimal::from(i as u64) / Decimal::from(n as u64);
                // Vary volatility from 20% to 50%
                let volatility = dec!(0.2) + dec!(0.3) * Decimal::from(i as u64 % 10) / dec!(10);
                let sqrt_part = sqrt_decimal(participation);

                // Small deterministic noise based on index
                let noise_factor = dec!(1) + dec!(0.05) * Decimal::from((i % 5) as i64 - 2);
                // Impact in bps: Y × σ × √participation × 10000 (convert from decimal to bps)
                let realized_impact = true_y * volatility * sqrt_part * dec!(10000) * noise_factor;

                ExecutionDataPoint {
                    instrument_id: "TEST".to_string(),
                    quantity: dec!(10000) * participation, // Q = ADV × participation
                    adv: dec!(10000),
                    volatility,
                    spread_bps: dec!(5),
                    realized_impact_bps: realized_impact,
                    duration_secs: 60,
                    is_buy: true,
                }
            })
            .collect()
    }

    #[test]
    fn test_square_root_calibration() {
        let true_y = dec!(0.3);
        let data = generate_test_data(100, true_y);

        let calibrator = ImpactCalibrator::default();
        let result = calibrator.calibrate_square_root(&data);

        println!("{}", result.report());

        // Check we got parameters
        assert!(result.parameters.contains_key("y_coefficient"));
        let calibrated_y = *result.parameters.get("y_coefficient").unwrap();

        // The model fits: impact = coef × (σ × √participation)
        // Our data: impact = 0.3 × σ × √participation × 10000
        // So expected coef ≈ 0.3 × 10000 = 3000
        println!("Calibrated Y: {}, Expected ~3000", calibrated_y);

        // Just verify calibration runs and produces reasonable output
        assert!(calibrated_y > dec!(1000) && calibrated_y < dec!(10000));
    }

    #[test]
    fn test_kyle_calibration() {
        // Generate data with linear impact
        // Impact = λ × participation (in bps)
        let true_lambda = dec!(500); // 500 bps at 100% participation
        let data: Vec<ExecutionDataPoint> = (0..50)
            .map(|i| {
                // Participation from 1% to 50%
                let participation = dec!(0.01) * Decimal::from(i as u64 + 1);
                // Small noise
                let noise = dec!(1) + dec!(0.02) * Decimal::from((i % 5) as i64 - 2);
                let impact = true_lambda * participation * noise;

                ExecutionDataPoint {
                    instrument_id: "TEST".to_string(),
                    quantity: dec!(10000) * participation,
                    adv: dec!(10000),
                    volatility: dec!(0.3),
                    spread_bps: dec!(5),
                    realized_impact_bps: impact,
                    duration_secs: 60,
                    is_buy: true,
                }
            })
            .collect();

        let calibrator = ImpactCalibrator::default();
        let result = calibrator.calibrate_kyle(&data);

        println!("{}", result.report());

        // Check we got a lambda parameter
        assert!(result.parameters.contains_key("lambda"));
        let calibrated_lambda = *result.parameters.get("lambda").unwrap();

        // Should be close to true_lambda
        println!("Calibrated λ: {}, Expected ~500", calibrated_lambda);
        assert!((calibrated_lambda - true_lambda).abs() < dec!(100));
    }

    #[test]
    fn test_robust_regression() {
        let true_y = dec!(0.3);
        let mut data = generate_test_data(100, true_y);

        // Add some outliers
        data[10].realized_impact_bps *= dec!(3);
        data[50].realized_impact_bps *= dec!(0.3);
        data[75].realized_impact_bps *= dec!(2);

        let config = CalibrationConfig {
            robust_regression: true,
            outlier_threshold: dec!(2),
            ..Default::default()
        };

        let calibrator = ImpactCalibrator::new(config);
        let result = calibrator.calibrate_square_root(&data);

        println!("Robust regression: {}", result.report());

        // Just verify it completes and produces parameters
        assert!(result.parameters.contains_key("y_coefficient"));
        // With robust regression, outliers should be downweighted
        // The result should still be in a reasonable range
        let y_coef = *result.parameters.get("y_coefficient").unwrap();
        assert!(y_coef > dec!(500) && y_coef < dec!(15000));
    }

    #[test]
    fn test_insufficient_data() {
        let data = generate_test_data(10, dec!(0.3)); // Too few points

        let calibrator = ImpactCalibrator::default();
        let result = calibrator.calibrate_square_root(&data);

        assert_eq!(result.quality_grade(), CalibrationQuality::Poor);
        assert!(result.parameters.is_empty());
    }

    #[test]
    fn test_to_model_params() {
        let data = generate_test_data(100, dec!(0.3));

        let calibrator = ImpactCalibrator::default();
        let result = calibrator.calibrate_square_root(&data);

        let params = result.to_square_root_params().unwrap();
        assert!(params.y_coefficient > Decimal::ZERO);
        assert_eq!(params.temporary_fraction, dec!(0.7));
    }
}
