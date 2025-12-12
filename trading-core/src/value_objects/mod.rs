mod order_type;
mod price;
mod quantity;
mod side;
mod symbol;
mod time_in_force;

pub use order_type::OrderType;
pub use price::{PRICE_DECIMALS, PRICE_SCALE, Price, Value};
pub use quantity::{QUANTITY_DECIMALS, QUANTITY_SCALE, Quantity};
pub use side::Side;
pub use symbol::Symbol;
pub use time_in_force::TimeInForce;

pub type OrderId = uuid::Uuid;
pub type TradeId = uuid::Uuid;
pub type Timestamp = chrono::DateTime<chrono::Utc>;
