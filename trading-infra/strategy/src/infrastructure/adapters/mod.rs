//! Infrastructure Adapters
//!
//! Adapters implement the application layer ports using concrete infrastructure.
//! These bridge the abstract domain/application layers with real implementations.

mod market_data_adapter;
mod signal_channel_adapter;

pub use market_data_adapter::{MarketDataAdapter, OrderBookReaderAdapter, adapt_market_data};
pub use signal_channel_adapter::{
    BoundedChannelFactory, BoundedSignalPublisher, BoundedSignalSubscriber, ChannelFactory,
    ChannelSignalPublisher, ChannelSignalSubscriber, create_signal_channel,
};
