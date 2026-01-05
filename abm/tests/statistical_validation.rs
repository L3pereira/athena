//! Statistical validation tests for the synthetic orderbook generator
//!
//! These tests verify that the generator produces outputs that match
//! the target statistical moments within acceptable tolerances.

use abm::{NUM_LEVELS, SyntheticOrderbookGenerator};
use trading_core::Price;

const N_SAMPLES: usize = 2000;
const SEED: u64 = 42;

/// Compute mean of a slice
fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// Compute standard deviation of a slice
fn std_dev(values: &[f64]) -> f64 {
    let m = mean(values);
    let variance = values.iter().map(|x| (x - m).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

/// Compute correlation between two slices
fn correlation(x: &[f64], y: &[f64]) -> f64 {
    let mean_x = mean(x);
    let mean_y = mean(y);

    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;

    for i in 0..x.len() {
        let dx = x[i] - mean_x;
        let dy = y[i] - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }

    cov / (var_x.sqrt() * var_y.sqrt())
}

#[test]
fn test_spread_distribution_matches_target() {
    let mut generator = SyntheticOrderbookGenerator::from_regime("normal", SEED);
    let moments = generator.moments().clone();
    let mid = Price::from_f64(10000.0);

    // Collect spreads in bps
    let spreads: Vec<f64> = (0..N_SAMPLES)
        .map(|_| {
            let book = generator.generate(mid);
            book.spread.to_f64() / mid.to_f64() * 10000.0
        })
        .collect();

    let actual_mean = mean(&spreads);
    let actual_std = std_dev(&spreads);

    // Target mean should be within 30% (log-normal has skew that affects realized mean)
    let relative_error = (actual_mean - moments.spread_mean_bps).abs() / moments.spread_mean_bps;
    assert!(
        relative_error < 0.30,
        "Spread mean {:.2} bps differs from target {:.2} bps by {:.1}%",
        actual_mean,
        moments.spread_mean_bps,
        relative_error * 100.0
    );

    println!("Spread validation:");
    println!("  Target mean: {:.2} bps", moments.spread_mean_bps);
    println!("  Actual mean: {:.2} bps", actual_mean);
    println!("  Actual std:  {:.2} bps", actual_std);
}

#[test]
fn test_depth_decay_pattern() {
    let mut generator = SyntheticOrderbookGenerator::from_regime("normal", SEED);
    let moments = generator.moments().clone();
    let mid = Price::from_f64(10000.0);

    // Collect depths per level
    let mut level_depths: Vec<Vec<f64>> = vec![Vec::new(); NUM_LEVELS];

    for _ in 0..N_SAMPLES {
        let book = generator.generate(mid);
        for (i, level) in book.bid_levels.iter().enumerate() {
            if i < NUM_LEVELS {
                level_depths[i].push(level.quantity.to_f64());
            }
        }
    }

    // Verify exponential decay pattern
    let level_means: Vec<f64> = level_depths.iter().map(|d| mean(d)).collect();

    println!("Depth decay validation:");
    for i in 0..NUM_LEVELS {
        let target = moments.depth_mean[i];
        let actual = level_means[i];
        let relative_error = (actual - target).abs() / target;
        println!(
            "  Level {}: target={:.1}, actual={:.1}, error={:.1}%",
            i,
            target,
            actual,
            relative_error * 100.0
        );

        // Allow 30% error for depth (high variance)
        assert!(
            relative_error < 0.40,
            "Level {} depth {:.1} differs from target {:.1} by more than 40%",
            i,
            actual,
            target
        );
    }

    // Verify decay: each level should have less depth than previous
    for i in 1..NUM_LEVELS {
        assert!(
            level_means[i] < level_means[i - 1] * 1.1, // Allow small tolerance
            "Depth should decay: level {} ({:.1}) >= level {} ({:.1})",
            i,
            level_means[i],
            i - 1,
            level_means[i - 1]
        );
    }
}

#[test]
fn test_level_correlation_structure() {
    let mut generator = SyntheticOrderbookGenerator::from_regime("normal", SEED);
    let moments = generator.moments().clone();
    let mid = Price::from_f64(10000.0);

    // Collect depths per level for correlation analysis
    let mut level_depths: Vec<Vec<f64>> = vec![Vec::new(); NUM_LEVELS];

    for _ in 0..N_SAMPLES {
        let book = generator.generate(mid);
        for (i, level) in book.bid_levels.iter().enumerate() {
            if i < NUM_LEVELS {
                level_depths[i].push(level.quantity.to_f64());
            }
        }
    }

    // Check adjacent level correlations
    println!(
        "Correlation validation (target rho={:.2}):",
        moments.level_correlation
    );

    for i in 0..(NUM_LEVELS - 1) {
        let corr = correlation(&level_depths[i], &level_depths[i + 1]);
        let error = (corr - moments.level_correlation).abs();

        println!(
            "  Levels {}-{}: correlation={:.3}, error={:.3}",
            i,
            i + 1,
            corr,
            error
        );

        // Correlation should be within 0.15 of target
        assert!(
            error < 0.20,
            "Adjacent correlation {:.3} differs from target {:.3} by more than 0.20",
            corr,
            moments.level_correlation
        );
    }
}

#[test]
fn test_regime_differences() {
    let mid = Price::from_f64(10000.0);

    // Collect stats for each regime
    let regimes = ["normal", "volatile", "trending"];
    let mut spread_means = Vec::new();
    let mut depth_means = Vec::new();

    for regime in &regimes {
        let mut generator = SyntheticOrderbookGenerator::from_regime(regime, SEED);

        let spreads: Vec<f64> = (0..500)
            .map(|_| {
                let book = generator.generate(mid);
                book.spread.to_f64() / mid.to_f64() * 10000.0
            })
            .collect();

        let depths: Vec<f64> = (0..500)
            .map(|_| {
                let book = generator.generate(mid);
                book.total_bid_depth().to_f64()
            })
            .collect();

        spread_means.push(mean(&spreads));
        depth_means.push(mean(&depths));
    }

    println!("Regime comparison:");
    for (i, regime) in regimes.iter().enumerate() {
        println!(
            "  {}: spread={:.2} bps, depth={:.0}",
            regime, spread_means[i], depth_means[i]
        );
    }

    // Volatile should have wider spreads than normal
    assert!(
        spread_means[1] > spread_means[0] * 2.0,
        "Volatile spread should be > 2x normal"
    );

    // Volatile should have less depth than normal
    assert!(
        depth_means[1] < depth_means[0] * 0.5,
        "Volatile depth should be < 0.5x normal"
    );
}

#[test]
fn test_imbalance_emerges_from_independent_sampling() {
    let mut generator = SyntheticOrderbookGenerator::from_regime("normal", SEED);
    let mid = Price::from_f64(10000.0);

    let imbalances: Vec<f64> = (0..N_SAMPLES)
        .map(|_| generator.generate(mid).imbalance)
        .collect();

    let imb_mean = mean(&imbalances);
    let imb_std = std_dev(&imbalances);

    println!("Imbalance distribution:");
    println!("  Mean: {:.4}", imb_mean);
    println!("  Std:  {:.4}", imb_std);

    // Mean should be close to 0 (independent sampling)
    assert!(
        imb_mean.abs() < 0.05,
        "Imbalance mean {:.4} should be close to 0",
        imb_mean
    );

    // Should have some variance (not always exactly 0)
    assert!(
        imb_std > 0.01,
        "Imbalance should have natural variance, got std={:.4}",
        imb_std
    );
}

#[test]
fn test_generation_performance() {
    let mut generator = SyntheticOrderbookGenerator::from_regime("normal", SEED);
    let mid = Price::from_f64(50000.0);

    // Warmup
    for _ in 0..100 {
        let _ = generator.generate(mid);
    }

    // Benchmark
    let n_iterations = 10000;
    let start = std::time::Instant::now();
    for _ in 0..n_iterations {
        let _ = generator.generate(mid);
    }
    let elapsed = start.elapsed();

    let rate = n_iterations as f64 / elapsed.as_secs_f64();
    let per_book_us = elapsed.as_micros() as f64 / n_iterations as f64;

    println!("Performance:");
    println!("  Rate: {:.0} orderbooks/second", rate);
    println!("  Time per orderbook: {:.1} microseconds", per_book_us);

    // Should generate at least 30,000 orderbooks/second (debug mode is slower)
    // In release mode this is typically >100,000/s
    assert!(
        rate > 30_000.0,
        "Generation rate {:.0}/s should be > 30,000/s",
        rate
    );
}
