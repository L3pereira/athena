//! Multi-exchange integration tests
//!
//! Tests the agent's order book manager ability to maintain
//! separate order books for the same symbol on different exchanges.

use trading_core::DepthSnapshotEvent;

use agent::gateway_in::{ExchangeId, OrderBookWriter, QualifiedSymbol, StreamData};
use agent::order_book::OrderBookManager;

// ============================================================================
// Order Book Manager Tests
// ============================================================================

#[test]
fn test_same_symbol_different_exchanges() {
    let order_books = OrderBookManager::new();

    // Create depth snapshots with different prices for same symbol
    let binance_depth = DepthSnapshotEvent {
        last_update_id: 100,
        bids: vec![["50000".to_string(), "1.0".to_string()]],
        asks: vec![["50010".to_string(), "1.5".to_string()]],
    };

    let kraken_depth = DepthSnapshotEvent {
        last_update_id: 200,
        bids: vec![["50020".to_string(), "2.0".to_string()]], // Higher bid!
        asks: vec![["50030".to_string(), "2.5".to_string()]],
    };

    // Apply snapshots with different exchange IDs
    let binance = ExchangeId::binance();
    let kraken = ExchangeId::kraken();

    OrderBookWriter::apply_snapshot(
        &order_books,
        &QualifiedSymbol::new(binance.clone(), "BTCUSDT"),
        &binance_depth,
    );
    OrderBookWriter::apply_snapshot(
        &order_books,
        &QualifiedSymbol::new(kraken.clone(), "BTCUSDT"),
        &kraken_depth,
    );

    // Verify both books exist and have different data
    let binance_book = order_books.book("binance", "BTCUSDT");
    let kraken_book = order_books.book("kraken", "BTCUSDT");

    assert!(binance_book.is_initialized());
    assert!(kraken_book.is_initialized());

    // Verify prices are different (proving they're separate books)
    assert_eq!(binance_book.best_bid().unwrap().price.to_string(), "50000");
    assert_eq!(kraken_book.best_bid().unwrap().price.to_string(), "50020");

    assert_eq!(binance_book.best_ask().unwrap().price.to_string(), "50010");
    assert_eq!(kraken_book.best_ask().unwrap().price.to_string(), "50030");

    // Verify symbols() returns both qualified symbols
    assert_eq!(order_books.symbols().len(), 2);

    // Verify exchange-specific queries
    assert_eq!(order_books.symbols_for_exchange(&binance), vec!["BTCUSDT"]);
    assert_eq!(order_books.symbols_for_exchange(&kraken), vec!["BTCUSDT"]);
}

#[test]
fn test_multiple_symbols_across_exchanges() {
    let order_books = OrderBookManager::new();

    let btc_depth = DepthSnapshotEvent {
        last_update_id: 100,
        bids: vec![["50000".to_string(), "1.0".to_string()]],
        asks: vec![["50100".to_string(), "1.5".to_string()]],
    };

    let eth_depth = DepthSnapshotEvent {
        last_update_id: 200,
        bids: vec![["3000".to_string(), "10.0".to_string()]],
        asks: vec![["3010".to_string(), "15.0".to_string()]],
    };

    let binance = ExchangeId::binance();
    let kraken = ExchangeId::kraken();

    // BTC on Binance, ETH on Kraken
    OrderBookWriter::apply_snapshot(
        &order_books,
        &QualifiedSymbol::new(binance.clone(), "BTCUSDT"),
        &btc_depth,
    );
    OrderBookWriter::apply_snapshot(
        &order_books,
        &QualifiedSymbol::new(kraken.clone(), "ETHUSDT"),
        &eth_depth,
    );

    // Verify
    let btc_book = order_books.book("binance", "BTCUSDT");
    let eth_book = order_books.book("kraken", "ETHUSDT");

    assert!(btc_book.is_initialized());
    assert!(eth_book.is_initialized());

    assert_eq!(btc_book.best_bid().unwrap().price.to_string(), "50000");
    assert_eq!(eth_book.best_bid().unwrap().price.to_string(), "3000");

    // Verify exchange-specific queries
    assert_eq!(order_books.symbols_for_exchange(&binance), vec!["BTCUSDT"]);
    assert_eq!(order_books.symbols_for_exchange(&kraken), vec!["ETHUSDT"]);

    // Total symbols
    assert_eq!(order_books.symbols().len(), 2);
}

#[test]
fn test_apply_update_with_exchange_id() {
    let order_books = OrderBookManager::new();

    let snapshot = DepthSnapshotEvent {
        last_update_id: 100,
        bids: vec![["50000".to_string(), "1.0".to_string()]],
        asks: vec![["50100".to_string(), "1.5".to_string()]],
    };

    let exchange = ExchangeId::binance();
    let key = QualifiedSymbol::new(exchange.clone(), "BTCUSDT");

    // Apply snapshot
    OrderBookWriter::apply_snapshot(&order_books, &key, &snapshot);

    // Apply update
    let update = StreamData::DepthUpdate {
        symbol: "BTCUSDT".to_string(),
        event_time: 0,
        first_update_id: 101,
        final_update_id: 102,
        bids: vec![["50000".to_string(), "2.0".to_string()]], // Updated quantity
        asks: vec![],
    };

    let applied = OrderBookWriter::apply_update(&order_books, &exchange, &update);
    assert!(applied);

    // Verify update was applied
    let book = order_books.book("binance", "BTCUSDT");
    assert_eq!(book.best_bid().unwrap().quantity.to_string(), "2.0");
    assert_eq!(book.last_update_id(), 102);
}

#[test]
fn test_update_wrong_exchange_fails() {
    let order_books = OrderBookManager::new();

    // Apply snapshot for Binance
    let snapshot = DepthSnapshotEvent {
        last_update_id: 100,
        bids: vec![["50000".to_string(), "1.0".to_string()]],
        asks: vec![],
    };

    OrderBookWriter::apply_snapshot(
        &order_books,
        &QualifiedSymbol::new("binance", "BTCUSDT"),
        &snapshot,
    );

    // Try to apply update for Kraken (book doesn't exist)
    let update = StreamData::DepthUpdate {
        symbol: "BTCUSDT".to_string(),
        event_time: 0,
        first_update_id: 101,
        final_update_id: 102,
        bids: vec![["50000".to_string(), "2.0".to_string()]],
        asks: vec![],
    };

    let applied = OrderBookWriter::apply_update(&order_books, &ExchangeId::kraken(), &update);
    assert!(!applied); // Should fail because kraken:BTCUSDT doesn't exist

    // Binance book should be unchanged
    let binance_book = order_books.book("binance", "BTCUSDT");
    assert_eq!(binance_book.best_bid().unwrap().quantity.to_string(), "1.0");
}

#[test]
fn test_arbitrage_detection_across_exchanges() {
    let order_books = OrderBookManager::new();

    // Binance: ask at 50010
    let binance_depth = DepthSnapshotEvent {
        last_update_id: 100,
        bids: vec![["50000".to_string(), "1.0".to_string()]],
        asks: vec![["50010".to_string(), "1.5".to_string()]],
    };

    // Kraken: bid at 50020 (higher than Binance ask!)
    let kraken_depth = DepthSnapshotEvent {
        last_update_id: 200,
        bids: vec![["50020".to_string(), "2.0".to_string()]],
        asks: vec![["50030".to_string(), "2.5".to_string()]],
    };

    OrderBookWriter::apply_snapshot(
        &order_books,
        &QualifiedSymbol::new("binance", "BTCUSDT"),
        &binance_depth,
    );
    OrderBookWriter::apply_snapshot(
        &order_books,
        &QualifiedSymbol::new("kraken", "BTCUSDT"),
        &kraken_depth,
    );

    // Detect arbitrage: Binance ask < Kraken bid
    let binance_book = order_books.book("binance", "BTCUSDT");
    let kraken_book = order_books.book("kraken", "BTCUSDT");

    let binance_ask = binance_book.best_ask().unwrap().price;
    let kraken_bid = kraken_book.best_bid().unwrap().price;

    // Arbitrage exists: buy on Binance at 50010, sell on Kraken at 50020 = $10 profit
    let spread = kraken_bid.inner() - binance_ask.inner();
    assert!(spread > rust_decimal::Decimal::ZERO);
    assert_eq!(spread.to_string(), "10");
}

#[test]
fn test_qualified_symbol_display_and_equality() {
    let binance_btc = QualifiedSymbol::new("binance", "BTCUSDT");
    let kraken_btc = QualifiedSymbol::new("kraken", "BTCUSDT");
    let binance_btc2 = QualifiedSymbol::new("binance", "BTCUSDT");

    // Check Display implementation
    assert_eq!(binance_btc.to_string(), "binance:BTCUSDT");
    assert_eq!(kraken_btc.to_string(), "kraken:BTCUSDT");

    // Different exchanges = not equal
    assert_ne!(binance_btc, kraken_btc);

    // Same exchange + symbol = equal
    assert_eq!(binance_btc, binance_btc2);

    // Case normalization: exchange lowercase, symbol uppercase
    let mixed_case = QualifiedSymbol::new("BINANCE", "btcusdt");
    assert_eq!(mixed_case.to_string(), "binance:BTCUSDT");
    assert_eq!(binance_btc, mixed_case);
}
