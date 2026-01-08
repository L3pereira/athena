//! Impact Models
//!
//! Implementations of market impact estimation models.
//!
//! From docs Section 2: Price Impact Models
//! - Linear: Kyle's lambda, Impact = λ × Q
//! - Square-root: Impact = σ × (Q/V)^0.5
//! - Obizhaeva-Wang: Transient + permanent with resilience decay
//! - Full L2: Multi-dimensional impact on spread, depth, volatility, regime

mod full_impact;
mod linear;
mod obizhaeva_wang;
mod protocol;
mod square_root;

pub use full_impact::FullImpactModel;
pub use linear::LinearImpact;
pub use obizhaeva_wang::ObizhaevaWangImpact;
pub use protocol::ImpactModel;
pub use square_root::SquareRootImpact;
