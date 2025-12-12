//! Signal Generation Module
//!
//! This module provides the infrastructure for generating trading signals
//! from order book data and other market inputs.
//!
//! # Clean Architecture
//!
//! The signal module follows clean architecture principles:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Infrastructure Layer                          │
//! │  (adapters, extractors - concrete implementations)               │
//! │                                                                  │
//! │  ┌────────────────────────────────────────────────────────────┐ │
//! │  │                   Application Layer                         │ │
//! │  │  (ports/traits, services - orchestration logic)             │ │
//! │  │                                                             │ │
//! │  │  ┌───────────────────────────────────────────────────────┐ │ │
//! │  │  │                    Domain Layer                        │ │ │
//! │  │  │  (entities, value objects - pure business logic)       │ │ │
//! │  │  │  Signal, StrategyId, Calculations, Features            │ │ │
//! │  │  └───────────────────────────────────────────────────────┘ │ │
//! │  │                                                             │ │
//! │  │  Ports: MarketDataPort, SignalPublisher, FeatureExtractor  │ │
//! │  │  Services: EngineService                                    │ │
//! │  └────────────────────────────────────────────────────────────┘ │
//! │                                                                  │
//! │  Adapters: MarketDataAdapter, ChannelSignalPublisher            │
//! │  Extractors: OrderBookExtractor, PriceExtractor                 │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # SOLID Principles
//!
//! - **SRP**: Each layer has a single responsibility
//! - **OCP**: New strategies/extractors can be added without modifying existing code
//! - **LSP**: All implementations can be substituted for their abstractions
//! - **ISP**: Ports are segregated (OrderBookReader, SignalPublisher, etc.)
//! - **DIP**: High-level modules depend on abstractions, not concretions
//!
//! # Legacy Architecture (for backwards compatibility)
//!
//! The original engine and features modules are still available:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     SignalEngine                             │
//! │  (manages multiple strategies on dedicated OS threads)       │
//! │                                                              │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
//! │  │ Strategy 1  │  │ Strategy 2  │  │ Strategy N  │         │
//! │  │ (OS Thread) │  │ (OS Thread) │  │ (OS Thread) │         │
//! │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘         │
//! │         │                │                │                 │
//! │         ▼                ▼                ▼                 │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │              OrderBookManager (ArcSwap)              │   │
//! │  │                  Lock-free reads                      │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! │         │                │                │                 │
//! │         ▼                ▼                ▼                 │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │              Signal Output Channel                   │   │
//! │  │                  (mpsc::Sender)                       │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//! ```

// Clean Architecture Layers
pub mod application;
pub mod domain;
pub mod infrastructure;

// Legacy modules (for backwards compatibility)
mod engine;
pub mod features;
mod traits;
mod types;

// Re-export clean architecture types
pub use application::{
    ports::{
        BookLevel, FeatureExtractionPipeline, FeatureExtractorPort, GeneratorConfig,
        MarketDataPort, OrderBookReader, SignalGeneratorPort, SignalPublisher, SymbolKey,
    },
    services::{EngineService, EngineServiceConfig},
};
pub use domain::{
    Calculations, Features, Imbalance, Leg, Microprice, Signal, SignalBuilder, SignalDirection,
    SignalId, Spread, StrategyId, StrategyType, Urgency, Vwap,
};
pub use infrastructure::{
    adapters::{
        ChannelFactory, ChannelSignalPublisher, ChannelSignalSubscriber, MarketDataAdapter,
        OrderBookReaderAdapter, adapt_market_data, create_signal_channel,
    },
    extractors::{OrderBookExtractor, PriceExtractor},
};

// Legacy re-exports (for backwards compatibility)
pub use engine::{SignalEngine, SignalEngineConfig, StrategyHandle};
pub use features::{OrderBookFeatures, PriceFeatures};
pub use traits::{FeatureExtractor, SignalGenerator, SignalGeneratorConfig};
