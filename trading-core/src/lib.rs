pub mod entities;
pub mod events;
pub mod value_objects;

// Re-export value objects at crate root for convenience
pub use value_objects::{
    OrderId, OrderType, Price, Quantity, Side, Symbol, TimeInForce, Timestamp, TradeId,
};

// Re-export entities at crate root
pub use entities::{Order, OrderStatus, PriceLevel, Trade};

// Re-export events at crate root
pub use events::{
    DepthSnapshotEvent, DepthUpdateEvent, OrderAcceptedEvent, OrderCanceledEvent,
    OrderExpiredEvent, OrderFilledEvent, OrderRejectedEvent, TradeExecutedEvent,
};
