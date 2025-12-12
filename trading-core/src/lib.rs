pub mod entities;
pub mod events;
pub mod stats;
pub mod value_objects;

// Re-export value objects at crate root for convenience
pub use value_objects::{
    OrderId, OrderType, PRICE_DECIMALS, PRICE_SCALE, Price, QUANTITY_DECIMALS, QUANTITY_SCALE,
    Quantity, Side, Symbol, TimeInForce, Timestamp, TradeId, Value,
};

// Re-export entities at crate root
pub use entities::{Order, OrderStatus, PriceLevel, Trade};

// Re-export events at crate root
pub use events::{
    DepthSnapshotEvent, DepthUpdateEvent, OrderAcceptedEvent, OrderCanceledEvent,
    OrderExpiredEvent, OrderFilledEvent, OrderRejectedEvent, TradeExecutedEvent,
};

// Re-export stats at crate root
pub use stats::{Ema, RollingStats};
