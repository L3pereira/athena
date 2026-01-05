//! Python bindings for the ABM library via PyO3
//!
//! Exposes the synthetic orderbook generator to Python/Jupyter notebooks.

use crate::application::generators::{GeneratedOrderbook, SyntheticOrderbookGenerator};
use crate::domain::{MarketStructureState, NUM_LEVELS, OrderbookMoments};
use pyo3::prelude::*;
use trading_core::{Price, PriceLevel};

/// Python wrapper for a price level
#[pyclass(name = "PriceLevel")]
#[derive(Clone)]
pub struct PyPriceLevel {
    #[pyo3(get)]
    pub price: f64,
    #[pyo3(get)]
    pub quantity: f64,
}

#[pymethods]
impl PyPriceLevel {
    fn __repr__(&self) -> String {
        format!(
            "PriceLevel(price={:.8}, quantity={:.8})",
            self.price, self.quantity
        )
    }
}

impl From<&PriceLevel> for PyPriceLevel {
    fn from(level: &PriceLevel) -> Self {
        PyPriceLevel {
            price: level.price.to_f64(),
            quantity: level.quantity.to_f64(),
        }
    }
}

/// Python wrapper for generated orderbook
#[pyclass(name = "GeneratedOrderbook")]
pub struct PyGeneratedOrderbook {
    #[pyo3(get)]
    pub mid_price: f64,
    #[pyo3(get)]
    pub best_bid: f64,
    #[pyo3(get)]
    pub best_ask: f64,
    #[pyo3(get)]
    pub spread: f64,
    #[pyo3(get)]
    pub imbalance: f64,
    #[pyo3(get)]
    pub bid_levels: Vec<PyPriceLevel>,
    #[pyo3(get)]
    pub ask_levels: Vec<PyPriceLevel>,
}

#[pymethods]
impl PyGeneratedOrderbook {
    /// Actual spread in basis points of mid price
    #[getter]
    fn spread_bps(&self) -> f64 {
        self.spread / self.mid_price * 10000.0
    }

    /// Total bid depth
    #[getter]
    fn total_bid_depth(&self) -> f64 {
        self.bid_levels.iter().map(|l| l.quantity).sum()
    }

    /// Total ask depth
    #[getter]
    fn total_ask_depth(&self) -> f64 {
        self.ask_levels.iter().map(|l| l.quantity).sum()
    }

    fn __repr__(&self) -> String {
        format!(
            "GeneratedOrderbook(mid={:.2}, spread={:.4} ({:.1} bps), imbalance={:.3})",
            self.mid_price,
            self.spread,
            self.spread_bps(),
            self.imbalance
        )
    }
}

impl From<GeneratedOrderbook> for PyGeneratedOrderbook {
    fn from(book: GeneratedOrderbook) -> Self {
        PyGeneratedOrderbook {
            mid_price: book.mid_price.to_f64(),
            best_bid: book.best_bid().to_f64(),
            best_ask: book.best_ask().to_f64(),
            spread: book.spread.to_f64(),
            imbalance: book.imbalance,
            bid_levels: book.bid_levels.iter().map(PyPriceLevel::from).collect(),
            ask_levels: book.ask_levels.iter().map(PyPriceLevel::from).collect(),
        }
    }
}

/// Python wrapper for orderbook moments
#[pyclass(name = "OrderbookMoments")]
#[derive(Clone)]
pub struct PyOrderbookMoments {
    inner: OrderbookMoments,
}

#[pymethods]
impl PyOrderbookMoments {
    /// Create moments for a normal/calm market regime
    #[staticmethod]
    fn default_normal() -> Self {
        PyOrderbookMoments {
            inner: OrderbookMoments::default_normal(),
        }
    }

    /// Create moments for a volatile market regime
    #[staticmethod]
    fn default_volatile() -> Self {
        PyOrderbookMoments {
            inner: OrderbookMoments::default_volatile(),
        }
    }

    /// Create moments for a trending market regime
    #[staticmethod]
    fn default_trending() -> Self {
        PyOrderbookMoments {
            inner: OrderbookMoments::default_trending(),
        }
    }

    #[getter]
    fn spread_mean_bps(&self) -> f64 {
        self.inner.spread_mean_bps
    }

    #[getter]
    fn spread_var_bps(&self) -> f64 {
        self.inner.spread_var_bps
    }

    #[getter]
    fn depth_mean(&self) -> Vec<f64> {
        self.inner.depth_mean.to_vec()
    }

    #[getter]
    fn imbalance_mean(&self) -> f64 {
        self.inner.imbalance_mean
    }

    #[getter]
    fn level_correlation(&self) -> f64 {
        self.inner.level_correlation
    }

    #[getter]
    fn decay_rate(&self) -> f64 {
        self.inner.decay_rate
    }

    fn __repr__(&self) -> String {
        format!(
            "OrderbookMoments(spread={:.1}bps, depth_0={:.0}, corr={:.2})",
            self.inner.spread_mean_bps, self.inner.depth_mean[0], self.inner.level_correlation
        )
    }
}

/// Python wrapper for market structure state
#[pyclass(name = "MarketStructureState")]
pub struct PyMarketStructureState {
    #[pyo3(get)]
    pub liquidity_score: f64,
    #[pyo3(get)]
    pub stress_level: f64,
    #[pyo3(get)]
    pub is_stressed: bool,
    #[pyo3(get)]
    pub regime_index: u8,
}

#[pymethods]
impl PyMarketStructureState {
    /// Compute market structure from moments
    #[staticmethod]
    #[pyo3(signature = (moments, regime_index, vpin=None))]
    fn from_orderbook(moments: &PyOrderbookMoments, regime_index: u8, vpin: Option<f64>) -> Self {
        let state = MarketStructureState::from_orderbook(&moments.inner, vpin, regime_index);
        PyMarketStructureState {
            liquidity_score: state.liquidity_score,
            stress_level: state.stress_level,
            is_stressed: state.is_stressed,
            regime_index: state.regime_index,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "MarketStructureState(liquidity={:.3}, stress={:.3}, stressed={})",
            self.liquidity_score, self.stress_level, self.is_stressed
        )
    }
}

/// Python wrapper for synthetic orderbook generator
#[pyclass(name = "SyntheticOrderbookGenerator")]
pub struct PySyntheticOrderbookGenerator {
    inner: SyntheticOrderbookGenerator,
}

#[pymethods]
impl PySyntheticOrderbookGenerator {
    /// Create a new generator with given moments and seed
    #[new]
    fn new(moments: &PyOrderbookMoments, seed: u64) -> Self {
        PySyntheticOrderbookGenerator {
            inner: SyntheticOrderbookGenerator::new(moments.inner.clone(), seed),
        }
    }

    /// Create generator from regime name ("normal", "volatile", "trending")
    #[staticmethod]
    fn from_regime(regime: &str, seed: u64) -> Self {
        PySyntheticOrderbookGenerator {
            inner: SyntheticOrderbookGenerator::from_regime(regime, seed),
        }
    }

    /// Update moments (for regime switching)
    fn update_moments(&mut self, moments: &PyOrderbookMoments) {
        self.inner.update_moments(moments.inner.clone());
    }

    /// Get current moments
    #[getter]
    fn moments(&self) -> PyOrderbookMoments {
        PyOrderbookMoments {
            inner: self.inner.moments().clone(),
        }
    }

    /// Generate a synthetic orderbook at the given mid price
    fn generate(&mut self, mid_price: f64) -> PyGeneratedOrderbook {
        let price = Price::from_f64(mid_price);
        self.inner.generate(price).into()
    }

    fn __repr__(&self) -> String {
        format!(
            "SyntheticOrderbookGenerator(spread_mean={:.1}bps)",
            self.inner.moments().spread_mean_bps
        )
    }
}

/// ABM Python module
#[pymodule]
fn abm_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyPriceLevel>()?;
    m.add_class::<PyGeneratedOrderbook>()?;
    m.add_class::<PyOrderbookMoments>()?;
    m.add_class::<PyMarketStructureState>()?;
    m.add_class::<PySyntheticOrderbookGenerator>()?;
    m.add("NUM_LEVELS", NUM_LEVELS)?;
    Ok(())
}
