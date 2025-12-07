pub mod rest;
pub mod websocket;

pub use rest::{ApiError, AppState, create_router};
pub use websocket::{StreamManager, WsState, ws_handler};
