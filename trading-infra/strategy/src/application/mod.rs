//! Application Layer - Use cases and ports
//!
//! The application layer defines:
//! - Ports: Interfaces (traits) that define boundaries
//! - Services: Use cases that orchestrate domain logic
//!
//! This layer depends only on the domain layer.
//! Infrastructure layer implements the ports.

pub mod ports;
pub mod services;

pub use ports::*;
pub use services::*;
