//! Integration test: Market Maker with Exchange-Sim
//!
//! Tests the complete flow:
//! 1. Exchange provides liquidity (resting orders)
//! 2. Market maker receives OB updates
//! 3. MM quotes bid/ask
//! 4. Taker lifts MM quotes
//! 5. MM position updates, requotes with skew

use athena_core::{Order, OrderType, Side, TimeInForce};
use athena_gateway::messages::{
    market_data::{BookLevel, OrderBookUpdate},
    order::OrderSide,
};
use athena_strategy::{
    Action, BasicMarketMaker, LocalOrderBook, MarketMakerConfig, Position, Strategy,
    StrategyContext,
};
use exchange_sim::{Exchange, model::ExchangeMessage};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;

/// Helper to create strategy context
fn make_context<'a>(
    books: &'a HashMap<String, LocalOrderBook>,
    positions: &'a HashMap<String, Position>,
    open_orders: &'a HashMap<String, athena_strategy::OpenOrder>,
) -> StrategyContext<'a> {
    StrategyContext {
        books,
        positions,
        open_orders,
    }
}

/// Test basic quoting: MM receives book update, generates quotes
#[tokio::test]
async fn test_mm_generates_quotes_on_book_update() {
    let _ = env_logger::try_init();

    // Create market maker
    let config = MarketMakerConfig {
        instrument_id: "BTC-USD".to_string(),
        spread_bps: dec!(20),    // 20 bps spread
        quote_size: dec!(0.1),   // 0.1 BTC
        max_position: dec!(1.0), // Max 1 BTC
        skew_factor: dec!(10),   // 10 bps per unit
        tick_size: dec!(0.01),
        requote_threshold: dec!(5),
    };
    let mut mm = BasicMarketMaker::new(config);

    // Create local order book and apply snapshot
    let mut books = HashMap::new();
    let mut book = LocalOrderBook::new("BTC-USD");
    book.apply_update(&OrderBookUpdate::Snapshot {
        instrument_id: "BTC-USD".to_string(),
        bids: vec![
            BookLevel::new(dec!(50000), dec!(10.0)),
            BookLevel::new(dec!(49900), dec!(20.0)),
        ],
        asks: vec![
            BookLevel::new(dec!(50100), dec!(10.0)),
            BookLevel::new(dec!(50200), dec!(20.0)),
        ],
        sequence: 1,
        timestamp_ns: 1000000,
    });
    books.insert("BTC-USD".to_string(), book);

    // Create context with no position
    let positions = HashMap::new();
    let open_orders = HashMap::new();
    let ctx = make_context(&books, &positions, &open_orders);

    // Trigger strategy
    let update = OrderBookUpdate::Snapshot {
        instrument_id: "BTC-USD".to_string(),
        bids: vec![BookLevel::new(dec!(50000), dec!(10.0))],
        asks: vec![BookLevel::new(dec!(50100), dec!(10.0))],
        sequence: 1,
        timestamp_ns: 1000000,
    };

    let actions = mm.on_book_update(&update, &ctx).await;

    // Should generate 2 orders (bid + ask)
    let orders: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SubmitOrder(o) => Some(o),
            _ => None,
        })
        .collect();

    assert_eq!(orders.len(), 2, "Expected 2 orders (bid + ask)");

    // Find bid and ask
    let bid = orders.iter().find(|o| o.side == OrderSide::Buy).unwrap();
    let ask = orders.iter().find(|o| o.side == OrderSide::Sell).unwrap();

    // Verify prices are around mid (50050)
    let mid = dec!(50050);
    assert!(bid.price.unwrap() < mid, "Bid should be below mid");
    assert!(ask.price.unwrap() > mid, "Ask should be above mid");

    // Verify quantities
    assert_eq!(bid.quantity, dec!(0.1));
    assert_eq!(ask.quantity, dec!(0.1));

    println!("MM quotes: bid={:?} ask={:?}", bid.price, ask.price);
}

/// Test inventory skew: Long position -> lower bid, tighter ask
#[tokio::test]
async fn test_mm_skews_quotes_with_inventory() {
    let _ = env_logger::try_init();

    let config = MarketMakerConfig {
        instrument_id: "BTC-USD".to_string(),
        spread_bps: dec!(20),
        quote_size: dec!(0.1),
        max_position: dec!(1.0),
        skew_factor: dec!(20), // High skew for visible effect
        tick_size: dec!(0.01),
        requote_threshold: dec!(1),
    };

    // Create two MMs - one flat, one long
    let mut mm_flat = BasicMarketMaker::new(config.clone());
    let mut mm_long = BasicMarketMaker::new(config);

    // Same order book
    let mut books = HashMap::new();
    let mut book = LocalOrderBook::new("BTC-USD");
    book.apply_update(&OrderBookUpdate::Snapshot {
        instrument_id: "BTC-USD".to_string(),
        bids: vec![BookLevel::new(dec!(50000), dec!(10.0))],
        asks: vec![BookLevel::new(dec!(50100), dec!(10.0))],
        sequence: 1,
        timestamp_ns: 1000000,
    });
    books.insert("BTC-USD".to_string(), book);

    // Flat position
    let flat_positions = HashMap::new();
    let open_orders = HashMap::new();
    let flat_ctx = make_context(&books, &flat_positions, &open_orders);

    // Long position (0.5 BTC)
    let mut long_positions = HashMap::new();
    long_positions.insert(
        "BTC-USD".to_string(),
        Position {
            quantity: dec!(0.5),
            avg_price: dec!(50000),
            realized_pnl: Decimal::ZERO,
        },
    );
    let long_ctx = make_context(&books, &long_positions, &open_orders);

    let update = OrderBookUpdate::Delta {
        instrument_id: "BTC-USD".to_string(),
        bids: vec![],
        asks: vec![],
        sequence: 2,
        timestamp_ns: 2000000,
    };

    let flat_actions = mm_flat.on_book_update(&update, &flat_ctx).await;
    let long_actions = mm_long.on_book_update(&update, &long_ctx).await;

    // Extract bid prices
    let flat_bid = flat_actions
        .iter()
        .filter_map(|a| match a {
            Action::SubmitOrder(o) if o.side == OrderSide::Buy => o.price,
            _ => None,
        })
        .next()
        .unwrap();

    let long_bid = long_actions
        .iter()
        .filter_map(|a| match a {
            Action::SubmitOrder(o) if o.side == OrderSide::Buy => o.price,
            _ => None,
        })
        .next()
        .unwrap();

    // Long MM should have lower bid (trying to avoid more buys)
    assert!(
        long_bid < flat_bid,
        "Long MM bid ({}) should be lower than flat MM bid ({})",
        long_bid,
        flat_bid
    );

    println!("Flat bid: {}, Long bid: {}", flat_bid, long_bid);
}

/// Full integration: MM runs against exchange-sim
#[tokio::test]
async fn test_mm_full_integration_with_exchange() {
    let _ = env_logger::try_init();

    // === Setup Exchange ===
    let (client_tx, mut client_rx) = mpsc::channel::<ExchangeMessage>(100);
    let exchange = Exchange::new(
        vec!["BTC-USD".to_string()],
        client_tx,
        100, // heartbeat
        100, // capacity
        "price-time".to_string(),
    )
    .await
    .unwrap();

    // === Setup Market Maker ===
    let config = MarketMakerConfig {
        instrument_id: "BTC-USD".to_string(),
        spread_bps: dec!(10),
        quote_size: dec!(0.5),
        max_position: dec!(2.0),
        skew_factor: dec!(5),
        tick_size: dec!(1.0),
        requote_threshold: dec!(10),
    };
    let mut mm = BasicMarketMaker::new(config);

    // === Create Initial Liquidity (simulate existing book) ===
    // Place some resting orders to create a book
    let maker_sell = Order::new(
        "BTC-USD".to_string(),
        Side::Sell,
        OrderType::Limit,
        dec!(5.0),
        Some(dec!(51000)),
        None,
        TimeInForce::GTC,
    );
    let maker_buy = Order::new(
        "BTC-USD".to_string(),
        Side::Buy,
        OrderType::Limit,
        dec!(5.0),
        Some(dec!(49000)),
        None,
        TimeInForce::GTC,
    );
    exchange.submit_order(maker_sell).await.unwrap();
    exchange.submit_order(maker_buy).await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // === Build Local Book for MM ===
    let mut books = HashMap::new();
    let mut book = LocalOrderBook::new("BTC-USD");
    book.apply_update(&OrderBookUpdate::Snapshot {
        instrument_id: "BTC-USD".to_string(),
        bids: vec![BookLevel::new(dec!(49000), dec!(5.0))],
        asks: vec![BookLevel::new(dec!(51000), dec!(5.0))],
        sequence: 1,
        timestamp_ns: 1000000,
    });
    books.insert("BTC-USD".to_string(), book);

    // === MM Generates Quotes ===
    let positions = HashMap::new();
    let open_orders = HashMap::new();
    let ctx = make_context(&books, &positions, &open_orders);

    let update = OrderBookUpdate::Snapshot {
        instrument_id: "BTC-USD".to_string(),
        bids: vec![BookLevel::new(dec!(49000), dec!(5.0))],
        asks: vec![BookLevel::new(dec!(51000), dec!(5.0))],
        sequence: 1,
        timestamp_ns: 1000000,
    };

    let actions = mm.on_book_update(&update, &ctx).await;

    // Submit MM orders to exchange
    for action in &actions {
        if let Action::SubmitOrder(request) = action {
            let order = Order::new(
                request.instrument_id.clone(),
                match request.side {
                    OrderSide::Buy => Side::Buy,
                    OrderSide::Sell => Side::Sell,
                },
                OrderType::Limit,
                request.quantity,
                request.price,
                None,
                TimeInForce::GTC,
            );
            let id = exchange.submit_order(order).await.unwrap();
            println!(
                "MM submitted order: {} {:?} @ {:?}",
                id, request.side, request.price
            );
        }
    }

    tokio::time::sleep(Duration::from_millis(50)).await;

    // === Taker Lifts MM's Ask ===
    let taker_buy = Order::new(
        "BTC-USD".to_string(),
        Side::Buy,
        OrderType::Market,
        dec!(0.5),
        None,
        None,
        TimeInForce::IOC,
    );
    exchange.submit_order(taker_buy).await.unwrap();

    // === Collect Events ===
    let mut trades = vec![];
    let timeout = Duration::from_secs(1);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        match tokio::time::timeout(Duration::from_millis(100), client_rx.recv()).await {
            Ok(Some(msg)) => {
                if let ExchangeMessage::Trade(trade) = msg {
                    println!(
                        "Trade: {} @ {} qty={}",
                        trade.instrument_id, trade.price, trade.quantity
                    );
                    trades.push(trade);
                }
            }
            _ => break,
        }
    }

    // === Verify ===
    assert!(!trades.is_empty(), "Expected at least one trade");

    // The trade should be at MM's ask price (around 50050 + spread)
    let mm_trade = &trades[0];
    assert_eq!(mm_trade.quantity, dec!(0.5));

    println!("Integration test passed! Trades: {}", trades.len());
}
