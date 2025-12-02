//! Integration test: Gateway <-> Exchange-Sim
//!
//! Tests the full round-trip:
//! Strategy -> Gateway Out -> Exchange -> Gateway In -> Strategy

use athena_core::{Order, OrderType, Side, TimeInForce};
use athena_gateway::{
    adapters::simulator::{OrderResponseParams, SimulatorGatewayIn},
    messages::{
        market_data::{OrderBookUpdate, TradeMessage},
        order::{OrderRequest, OrderResponse, OrderSide, TimeInForceWire},
    },
    transport::{
        Requester, Subscriber,
        channel::{ChannelPublisher, ChannelRequester},
    },
};
use exchange_sim::{Exchange, model::ExchangeMessage};
use rust_decimal_macros::dec;
use std::time::Duration;
use tokio::sync::mpsc;

/// Test the full round-trip flow through gateway and exchange
#[tokio::test]
async fn test_gateway_exchange_round_trip() {
    let _ = env_logger::try_init();

    // === Setup Exchange ===
    let (client_tx, mut client_rx) = mpsc::channel::<ExchangeMessage>(100);
    let exchange = Exchange::new(
        vec!["BTC-USD".to_string()],
        client_tx,
        50,  // heartbeat interval ms
        100, // channel capacity
        "price-time".to_string(),
    )
    .await
    .expect("Failed to create exchange");

    // === Setup Gateway Channels ===
    // Market data channel (order book updates)
    let (md_pub, mut _md_sub) = ChannelPublisher::<OrderBookUpdate>::pair(100);
    // Trade channel
    let (trade_pub, mut trade_sub) = ChannelPublisher::<TradeMessage>::pair(100);
    // Order response channel
    let (order_pub, mut order_sub) = ChannelPublisher::<OrderResponse>::pair(100);

    // === Create Gateway In ===
    let gateway_in =
        SimulatorGatewayIn::new(Box::new(md_pub), Box::new(trade_pub), Box::new(order_pub));

    // === Wire Exchange Events -> Gateway In ===
    let gateway_in_handle = tokio::spawn(async move {
        while let Some(msg) = client_rx.recv().await {
            match msg {
                ExchangeMessage::Trade(trade) => {
                    gateway_in.publish_trade(&trade).await.ok();
                }
                ExchangeMessage::OrderUpdate {
                    order_id,
                    status,
                    filled_qty,
                    symbol: _,
                } => {
                    let order_id_str = order_id.to_string();
                    gateway_in
                        .publish_order_response(OrderResponseParams {
                            client_order_id: &order_id_str,
                            exchange_order_id: Some(&order_id_str),
                            status,
                            filled_qty,
                            avg_price: None,
                            reject_reason: None,
                            timestamp_ns: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                        })
                        .await
                        .ok();
                }
                ExchangeMessage::Heartbeat(_) => {
                    // Ignore heartbeats for now
                }
                _ => {}
            }
        }
    });

    // Give exchange time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // === Submit Orders Directly to Exchange (simulating Gateway Out) ===
    // First, place a sell order (maker)
    let sell_order = Order::new(
        "BTC-USD".to_string(),
        Side::Sell,
        OrderType::Limit,
        dec!(1.0),
        Some(dec!(50000)),
        None,
        TimeInForce::GTC,
    );
    let sell_id = exchange
        .submit_order(sell_order)
        .await
        .expect("Failed to submit sell order");
    println!("Submitted sell order: {}", sell_id);

    // Wait for order to be processed
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Now place a buy order that will match (taker)
    let buy_order = Order::new(
        "BTC-USD".to_string(),
        Side::Buy,
        OrderType::Limit,
        dec!(1.0),
        Some(dec!(50000)),
        None,
        TimeInForce::GTC,
    );
    let buy_id = exchange
        .submit_order(buy_order)
        .await
        .expect("Failed to submit buy order");
    println!("Submitted buy order: {}", buy_id);

    // === Verify Events Flow Through Gateway ===
    // Wait for trade to come through gateway
    let timeout = Duration::from_secs(2);
    let trade_result = tokio::time::timeout(timeout, trade_sub.next()).await;

    match trade_result {
        Ok(Ok(trade)) => {
            println!("Received trade through gateway: {:?}", trade);
            assert_eq!(trade.instrument_id, "BTC-USD");
            assert_eq!(trade.price, dec!(50000));
            assert_eq!(trade.quantity, dec!(1.0));
        }
        Ok(Err(e)) => panic!("Channel error: {:?}", e),
        Err(_) => panic!("Timeout waiting for trade"),
    }

    // Check for order updates
    let mut updates_received = 0;
    while let Ok(Ok(update)) =
        tokio::time::timeout(Duration::from_millis(200), order_sub.next()).await
    {
        println!("Order update: {:?}", update);
        updates_received += 1;
        if updates_received >= 2 {
            break;
        }
    }
    assert!(updates_received >= 1, "Expected at least 1 order update");

    // Cleanup
    gateway_in_handle.abort();
}

/// Test order request flow via ChannelRequester
#[tokio::test]
async fn test_order_request_via_channel() {
    let _ = env_logger::try_init();

    // Create requester/responder pair for orders
    let (order_requester, mut order_responder) =
        ChannelRequester::<OrderRequest, OrderResponse>::pair(10);

    // Spawn a mock "gateway out" that handles requests
    let responder_handle = tokio::spawn(async move {
        while let Some((request, reply_tx)) = order_responder.next().await {
            // Simulate order acceptance
            let response = OrderResponse::accepted(
                &request.client_order_id,
                format!("exch-{}", request.client_order_id),
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            );
            let _ = reply_tx.send(response);
        }
    });

    // Submit an order via the requester
    let request = OrderRequest::limit(
        "client-001",
        "BTC-USD",
        OrderSide::Buy,
        dec!(0.5),
        dec!(48000),
        TimeInForceWire::Gtc,
    );

    let response = order_requester
        .request(&request)
        .await
        .expect("Request failed");

    assert_eq!(response.client_order_id, "client-001");
    assert!(response.exchange_order_id.is_some());
    assert!(matches!(
        response.status,
        athena_gateway::messages::order::OrderStatusWire::Accepted
    ));

    responder_handle.abort();
}

/// Test market data flow
#[tokio::test]
async fn test_market_data_publishing() {
    let _ = env_logger::try_init();

    // Create channel for order book updates
    let (md_pub, mut md_sub) = ChannelPublisher::<OrderBookUpdate>::pair(10);
    let (trade_pub, _trade_sub) = ChannelPublisher::<TradeMessage>::pair(10);
    let (order_pub, _order_sub) = ChannelPublisher::<OrderResponse>::pair(10);

    let gateway_in =
        SimulatorGatewayIn::new(Box::new(md_pub), Box::new(trade_pub), Box::new(order_pub));

    // Publish a snapshot
    gateway_in
        .publish_snapshot(
            "ETH-USD",
            vec![(dec!(3000), dec!(10.0)), (dec!(2999), dec!(5.0))], // bids
            vec![(dec!(3001), dec!(8.0)), (dec!(3002), dec!(12.0))], // asks
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        )
        .await
        .expect("Failed to publish snapshot");

    // Receive and verify
    let update = tokio::time::timeout(Duration::from_secs(1), md_sub.next())
        .await
        .expect("Timeout")
        .expect("Channel error");

    match update {
        OrderBookUpdate::Snapshot {
            instrument_id,
            bids,
            asks,
            ..
        } => {
            assert_eq!(instrument_id, "ETH-USD");
            assert_eq!(bids.len(), 2);
            assert_eq!(asks.len(), 2);
            assert_eq!(bids[0].price, dec!(3000));
            assert_eq!(asks[0].price, dec!(3001));
        }
        _ => panic!("Expected snapshot"),
    }
}
