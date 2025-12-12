//! Application Services - Use cases for signal generation
//!
//! Services orchestrate the flow of data through the domain,
//! coordinating ports and domain entities.

mod engine_service;

pub use engine_service::{EngineService, EngineServiceConfig};
