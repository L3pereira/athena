//! Simulator Gateway Out - converts wire format to exchange-sim format
//!
//! Receives order requests from internal components via channels
//! and submits them to the exchange-sim.

use crate::error::GatewayError;
use crate::messages::order::{
    CancelRequest, OrderRequest, OrderResponse, OrderSide, OrderStatusWire, OrderTypeWire,
    TimeInForceWire,
};
use crate::transport::channel::ChannelResponder;
use athena_core::{Order, OrderType, Side, TimeInForce};
use chrono::Utc;
use log::{debug, error, info, warn};
use rust_decimal::Decimal;

/// Gateway Out for the simulator
///
/// Receives order requests from channels and submits them
/// to the exchange simulator.
pub struct SimulatorGatewayOut {
    /// Channel responder for order submissions
    order_responder: ChannelResponder<OrderRequest, OrderResponse>,
    /// Channel responder for cancel requests
    cancel_responder: ChannelResponder<CancelRequest, OrderResponse>,
}

impl SimulatorGatewayOut {
    /// Create a new simulator gateway out with channel responders
    pub fn new(
        order_responder: ChannelResponder<OrderRequest, OrderResponse>,
        cancel_responder: ChannelResponder<CancelRequest, OrderResponse>,
    ) -> Self {
        Self {
            order_responder,
            cancel_responder,
        }
    }

    /// Run the gateway out loop, processing order requests
    ///
    /// This method requires an order submission function that takes an Order
    /// and returns a Result with the exchange order ID or an error.
    pub async fn run<F, Fut>(&mut self, mut submit_order: F) -> Result<(), GatewayError>
    where
        F: FnMut(Order) -> Fut,
        Fut: std::future::Future<Output = Result<String, String>>,
    {
        info!("SimulatorGatewayOut started, listening for orders");

        loop {
            tokio::select! {
                // Handle order submissions
                result = self.order_responder.next() => {
                    match result {
                        Some((request, reply_tx)) => {
                            debug!("Received order request: {:?}", request.client_order_id);

                            // Convert wire format to internal Order
                            let order = self.denormalize_order(&request);

                            // Submit to exchange
                            let response = match submit_order(order).await {
                                Ok(exchange_id) => {
                                    info!("Order accepted: {} -> {}", request.client_order_id, exchange_id);
                                    OrderResponse::accepted(
                                        &request.client_order_id,
                                        exchange_id,
                                        Utc::now().timestamp_nanos_opt().unwrap_or(0),
                                    )
                                }
                                Err(reason) => {
                                    warn!("Order rejected: {} - {}", request.client_order_id, reason);
                                    OrderResponse::rejected(
                                        &request.client_order_id,
                                        reason,
                                        Utc::now().timestamp_nanos_opt().unwrap_or(0),
                                    )
                                }
                            };

                            // Send reply
                            if reply_tx.send(response).is_err() {
                                error!("Failed to send order response: receiver dropped");
                            }
                        }
                        None => {
                            // Channel closed
                            break;
                        }
                    }
                }

                // Handle cancel requests
                result = self.cancel_responder.next() => {
                    match result {
                        Some((request, reply_tx)) => {
                            debug!("Received cancel request: {:?}", request.client_order_id);

                            // For now, just acknowledge - full implementation would cancel the order
                            let response = OrderResponse {
                                client_order_id: request.client_order_id.clone(),
                                exchange_order_id: request.exchange_order_id.clone(),
                                status: OrderStatusWire::Cancelled,
                                filled_qty: Decimal::ZERO,
                                avg_price: None,
                                reject_reason: None,
                                timestamp_ns: Utc::now().timestamp_nanos_opt().unwrap_or(0),
                            };

                            if reply_tx.send(response).is_err() {
                                error!("Failed to send cancel response: receiver dropped");
                            }
                        }
                        None => break,
                    }
                }
            }
        }

        Ok(())
    }

    /// Convert wire format OrderRequest to internal Order
    fn denormalize_order(&self, request: &OrderRequest) -> Order {
        let side = match request.side {
            OrderSide::Buy => Side::Buy,
            OrderSide::Sell => Side::Sell,
        };

        let order_type = match request.order_type {
            OrderTypeWire::Limit => OrderType::Limit,
            OrderTypeWire::Market => OrderType::Market,
        };

        let time_in_force = match request.time_in_force {
            TimeInForceWire::Gtc => TimeInForce::GTC,
            TimeInForceWire::Ioc => TimeInForce::IOC,
            TimeInForceWire::Fok => TimeInForce::FOK,
        };

        Order::new(
            request.instrument_id.clone(),
            side,
            order_type,
            request.quantity,
            request.price,
            None, // stop_price - not supported in wire format yet
            time_in_force,
        )
    }
}
