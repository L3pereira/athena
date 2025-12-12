//! Presentation Layer - Outbound interfaces to downstream consumers
//!
//! This layer contains adapters for systems that consume from us:
//! - MarketDataPublisher: Publishes order book data to strategy processes
//!
//! Follows Hexagonal Architecture:
//! - Infrastructure = inbound (exchanges → gateway)
//! - Presentation = outbound (gateway → consumers)

mod publisher;

pub use publisher::MarketDataPublisher;
