//! Market Making Models
//!
//! Implementations of market making strategies.
//!
//! From docs Section 8: The Market Maker Perspective
//! - Avellaneda-Stoikov: Optimal quoting with reservation price
//! - Inventory skew: Position-based quote adjustment
//! - Toxic flow detection: VPIN, OFI signals

mod avellaneda_stoikov;
mod inventory_skew;
mod protocol;
mod toxic_flow;

pub use avellaneda_stoikov::AvellanedaStoikov;
pub use inventory_skew::InventorySkew;
pub use protocol::QuotingModel;
pub use toxic_flow::ToxicFlowDetector;
