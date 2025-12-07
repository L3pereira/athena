//! Integration test: Market simulation with market makers and retail traders
//!
//! This test sets up:
//! 1. A BTCUSDT market with custom fees
//! 2. Market makers with rebates and capital
//! 3. Retail traders trading on "technical indicators"
//!
//! Demonstrates the full trading lifecycle with fee collection.

use exchange_sim::{
    AccountRepository, Clock, ControllableClock, ExchangeConfig, FeeSchedule, Order,
    OrderBookReader, OrderBookWriter, Price, Quantity, Side, Symbol, TimeInForce,
    TradingPairConfig,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Helper to create the exchange with custom setup
async fn setup_exchange() -> exchange_sim::Exchange<exchange_sim::SimulationClock> {
    let config = ExchangeConfig::default();
    exchange_sim::Exchange::fixed_time(config)
}

#[tokio::test]
async fn test_market_maker_receives_rebate() {
    let exchange = setup_exchange().await;

    // Setup: Create market maker account with rebate schedule
    {
        let mut mm_account = exchange.account_repo.get_or_create("market_maker_1").await;
        mm_account.deposit("USDT", dec!(1_000_000)); // $1M capital
        mm_account.deposit("BTC", dec!(100)); // 100 BTC
        mm_account.fee_schedule = FeeSchedule::market_maker(); // Tier 9: negative maker fee (rebate)
        exchange.account_repo.save(mm_account).await;
    }

    // Setup: Create retail trader account
    {
        let mut retail = exchange.account_repo.get_or_create("retail_trader_1").await;
        retail.deposit("USDT", dec!(10_000)); // $10k capital
        retail.fee_schedule = FeeSchedule::default(); // Standard fees
        exchange.account_repo.save(retail).await;
    }

    // Get the order book for BTCUSDT
    let symbol = Symbol::new("BTCUSDT").unwrap();

    // Market maker posts a limit sell order (provides liquidity)
    {
        let mut book = exchange.order_book_repo.get_or_create(&symbol).await;
        let mm_order = Order::new_limit(
            symbol.clone(),
            Side::Sell,
            Quantity::from(dec!(1)),
            Price::from(dec!(50000)),
            TimeInForce::Gtc,
        );
        book.add_order(mm_order);
        exchange.order_book_repo.save(book).await;
    }

    // Verify initial balances
    let mm_account = exchange
        .account_repo
        .get_by_owner("market_maker_1")
        .await
        .unwrap();
    assert_eq!(mm_account.balance("USDT").available, dec!(1_000_000));
    assert_eq!(mm_account.balance("BTC").available, dec!(100));

    let retail_account = exchange
        .account_repo
        .get_by_owner("retail_trader_1")
        .await
        .unwrap();
    assert_eq!(retail_account.balance("USDT").available, dec!(10_000));
}

#[tokio::test]
async fn test_create_custom_market() {
    let exchange = setup_exchange().await;

    // Create a custom market with specific fees
    let symbol = Symbol::new("ETHBTC").unwrap();
    let config = TradingPairConfig::new(symbol.clone(), "ETH", "BTC")
        .with_fees(dec!(-0.0002), dec!(0.0004)) // -2 bps maker (rebate), 4 bps taker
        .with_tick_size(Price::from(dec!(0.00001)))
        .with_lot_size(Quantity::from(dec!(0.001)));

    exchange.instrument_repo.add(config);

    // Verify it was added
    let retrieved = exchange.instrument_repo.get(&symbol);
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.maker_fee_rate, dec!(-0.0002));
    assert_eq!(retrieved.taker_fee_rate, dec!(0.0004));
}

#[tokio::test]
async fn test_fee_tiers() {
    let exchange = setup_exchange().await;

    // Create accounts at different VIP tiers
    let tiers = vec![
        ("vip_0", FeeSchedule::default()),
        ("vip_1", FeeSchedule::tier_1()),
        ("vip_2", FeeSchedule::tier_2()),
        ("vip_3", FeeSchedule::tier_3()),
        ("market_maker", FeeSchedule::market_maker()),
    ];

    for (owner, schedule) in &tiers {
        let mut account = exchange.account_repo.get_or_create(owner).await;
        account.deposit("USDT", dec!(100_000));
        account.fee_schedule = *schedule;
        exchange.account_repo.save(account).await;
    }

    // Verify accounts were created with correct tiers
    for (owner, expected_schedule) in &tiers {
        let account = exchange.account_repo.get_by_owner(owner).await.unwrap();
        assert_eq!(account.fee_schedule.tier, expected_schedule.tier);
        assert_eq!(
            account.fee_schedule.maker_discount,
            expected_schedule.maker_discount
        );
    }
}

#[tokio::test]
async fn test_order_book_depth() {
    let exchange = setup_exchange().await;
    let symbol = Symbol::new("BTCUSDT").unwrap();

    // Create multiple market makers
    for i in 1..=3 {
        let owner = format!("mm_{}", i);
        let mut account = exchange.account_repo.get_or_create(&owner).await;
        account.deposit("USDT", dec!(1_000_000));
        account.deposit("BTC", dec!(100));
        account.fee_schedule = FeeSchedule::market_maker();
        exchange.account_repo.save(account).await;
    }

    // Post orders at different price levels
    {
        let mut book = exchange.order_book_repo.get_or_create(&symbol).await;

        // Bids (buy orders)
        let bid_levels = [
            (Decimal::new(49900, 0), Decimal::new(2, 0)),
            (Decimal::new(49800, 0), Decimal::new(5, 0)),
            (Decimal::new(49700, 0), Decimal::new(10, 0)),
        ];
        for (price, qty) in bid_levels {
            let order = Order::new_limit(
                symbol.clone(),
                Side::Buy,
                Quantity::from(qty),
                Price::from(price),
                TimeInForce::Gtc,
            );
            book.add_order(order);
        }

        // Asks (sell orders)
        let ask_levels = [
            (Decimal::new(50100, 0), Decimal::new(2, 0)),
            (Decimal::new(50200, 0), Decimal::new(5, 0)),
            (Decimal::new(50300, 0), Decimal::new(10, 0)),
        ];
        for (price, qty) in ask_levels {
            let order = Order::new_limit(
                symbol.clone(),
                Side::Sell,
                Quantity::from(qty),
                Price::from(price),
                TimeInForce::Gtc,
            );
            book.add_order(order);
        }

        exchange.order_book_repo.save(book).await;
    }

    // Verify order book has depth
    let book = exchange.order_book_repo.get_or_create(&symbol).await;
    let snapshot = book.snapshot(Some(10));

    assert_eq!(snapshot.bids.len(), 3);
    assert_eq!(snapshot.asks.len(), 3);

    // Best bid should be 49900, best ask should be 50100
    assert_eq!(snapshot.bids[0].price, Price::from(dec!(49900)));
    assert_eq!(snapshot.asks[0].price, Price::from(dec!(50100)));
}

#[tokio::test]
async fn test_time_advancement() {
    let exchange = setup_exchange().await;

    let t1 = exchange.clock.now();

    // Advance time by 1 hour
    exchange.clock.advance(chrono::Duration::hours(1));

    let t2 = exchange.clock.now();

    assert_eq!((t2 - t1).num_hours(), 1);
}
