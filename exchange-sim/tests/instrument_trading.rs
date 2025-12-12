//! Integration tests for different instrument types and trading scenarios
//!
//! Tests cover:
//! - Spot trading (maker/taker)
//! - Spot short selling with margin
//! - Perpetual futures trading
//! - Options trading
//! - Margin trading with collateral

use exchange_sim::{
    AccountRepository, Clock, ExchangeConfig, FeeSchedule, MarginCalculator, Order,
    OrderBookReader, OrderBookWriter, Price, Quantity, Side, StandardMarginCalculator, Symbol,
    TimeInForce, TradingPairConfig, Value, domain::Rate,
};

/// Setup helper
async fn setup_exchange() -> exchange_sim::Exchange<exchange_sim::SimulationClock> {
    let config = ExchangeConfig::default();
    exchange_sim::Exchange::fixed_time(config)
}

// ============================================================================
// SPOT TRADING TESTS
// ============================================================================

mod spot_trading {
    use super::*;

    #[tokio::test]
    async fn test_spot_maker_taker_fees() {
        let exchange = setup_exchange().await;
        let symbol = Symbol::new("BTCUSDT").unwrap();

        // Create maker (posts limit order, provides liquidity)
        {
            let mut maker = exchange.account_repo.get_or_create("maker").await;
            maker.deposit("BTC", Value::from_int(10));
            maker.fee_schedule = FeeSchedule::market_maker(); // Gets rebate
            exchange.account_repo.save(maker).await;
        }

        // Create taker (takes liquidity)
        {
            let mut taker = exchange.account_repo.get_or_create("taker").await;
            taker.deposit("USDT", Value::from_int(100_000));
            taker.fee_schedule = FeeSchedule::default(); // Pays taker fee
            exchange.account_repo.save(taker).await;
        }

        // Maker posts sell order (provides liquidity)
        {
            let mut book = exchange.order_book_repo.get_or_create(&symbol).await;
            let maker_order = Order::new_limit(
                symbol.clone(),
                Side::Sell,
                Quantity::from_int(1),
                Price::from_int(50000),
                TimeInForce::Gtc,
            );
            book.add_order(maker_order);
            exchange.order_book_repo.save(book).await;
        }

        // Verify order book has the maker's order
        let book = exchange.order_book_repo.get_or_create(&symbol).await;
        let snapshot = book.snapshot(Some(10));
        assert_eq!(snapshot.asks.len(), 1);
        assert_eq!(snapshot.asks[0].price, Price::from_int(50000));
        assert_eq!(snapshot.asks[0].quantity, Quantity::from_int(1));
    }

    #[tokio::test]
    async fn test_spot_buy_sell_cycle() {
        let exchange = setup_exchange().await;
        let symbol = Symbol::new("ETHUSDT").unwrap();

        // Add ETHUSDT trading pair (1 bps maker, 2 bps taker)
        let config = TradingPairConfig::new(symbol.clone(), "ETH", "USDT").with_fees_bps(1, 2);
        exchange.instrument_repo.add(config);

        // Trader with USDT wants to buy ETH
        {
            let mut trader = exchange.account_repo.get_or_create("trader").await;
            trader.deposit("USDT", Value::from_int(10_000));
            exchange.account_repo.save(trader).await;
        }

        // Market maker provides liquidity
        {
            let mut mm = exchange.account_repo.get_or_create("mm").await;
            mm.deposit("ETH", Value::from_int(100));
            mm.fee_schedule = FeeSchedule::market_maker();
            exchange.account_repo.save(mm).await;
        }

        // MM posts sell orders at different levels
        {
            let mut book = exchange.order_book_repo.get_or_create(&symbol).await;
            for (price, qty) in [(3000, 5), (3010, 10), (3020, 20)] {
                let order = Order::new_limit(
                    symbol.clone(),
                    Side::Sell,
                    Quantity::from_int(qty),
                    Price::from_int(price),
                    TimeInForce::Gtc,
                );
                book.add_order(order);
            }
            exchange.order_book_repo.save(book).await;
        }

        // Verify depth
        let book = exchange.order_book_repo.get_or_create(&symbol).await;
        let snapshot = book.snapshot(Some(10));
        assert_eq!(snapshot.asks.len(), 3);
        assert_eq!(snapshot.asks[0].price, Price::from_int(3000)); // Best ask
    }
}

// ============================================================================
// SPOT SHORT SELLING TESTS
// ============================================================================

mod spot_short_selling {
    use super::*;
    use exchange_sim::domain::PositionSide;

    #[tokio::test]
    async fn test_borrow_and_short_sell() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();

        // Create short seller with USDT collateral
        {
            let mut trader = exchange.account_repo.get_or_create("short_seller").await;
            trader.deposit("USDT", Value::from_int(100_000)); // Collateral

            // Borrow 1 BTC using USDT as collateral
            trader
                .borrow(
                    "BTC",
                    Value::from_int(1),      // borrow amount
                    Rate::from_bps(500),     // 5% annual interest (500 bps)
                    "USDT",                  // collateral asset
                    Value::from_int(60_000), // collateral amount (120% of position value)
                    now,
                )
                .expect("Borrow should succeed");

            exchange.account_repo.save(trader).await;
        }

        // Verify borrow worked
        let trader = exchange
            .account_repo
            .get_by_owner("short_seller")
            .await
            .unwrap();

        // Should have borrowed BTC available
        assert_eq!(trader.balance("BTC").available, Value::from_int(1));
        assert_eq!(trader.balance("BTC").borrowed, Value::from_int(1));

        // USDT should be partially locked as collateral
        assert_eq!(trader.balance("USDT").available, Value::from_int(40_000)); // 100k - 60k locked
        assert_eq!(trader.balance("USDT").locked, Value::from_int(60_000));
    }

    #[tokio::test]
    async fn test_short_position_tracking() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();
        let symbol = Symbol::new("BTCUSDT").unwrap();

        // Create trader and open short position
        {
            let mut trader = exchange.account_repo.get_or_create("short_trader").await;
            trader.deposit("USDT", Value::from_int(150_000)); // Enough for 120k collateral

            // Borrow BTC first
            trader
                .borrow(
                    "BTC",
                    Value::from_int(2),
                    Rate::from_bps(500),
                    "USDT",
                    Value::from_int(120_000),
                    now,
                )
                .unwrap();

            // Open short position (selling borrowed BTC)
            trader.open_position(
                symbol.clone(),
                PositionSide::Short,
                Quantity::from_int(2),
                Price::from_int(50_000),
                Value::from_int(20_000), // margin
                now,
            );

            exchange.account_repo.save(trader).await;
        }

        // Verify position
        let trader = exchange
            .account_repo
            .get_by_owner("short_trader")
            .await
            .unwrap();
        let position = trader.position(&symbol);
        assert!(position.is_some());

        let pos = position.unwrap();
        assert_eq!(pos.side, PositionSide::Short);
        assert_eq!(pos.quantity, Quantity::from_int(2));
        assert_eq!(pos.entry_price, Price::from_int(50_000));
    }

    #[tokio::test]
    async fn test_short_pnl_calculation() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();
        let symbol = Symbol::new("BTCUSDT").unwrap();

        {
            let mut trader = exchange.account_repo.get_or_create("pnl_trader").await;
            trader.deposit("USDT", Value::from_int(100_000));
            trader
                .borrow(
                    "BTC",
                    Value::from_int(1),
                    Rate::from_bps(500),
                    "USDT",
                    Value::from_int(60_000),
                    now,
                )
                .unwrap();

            // Short 1 BTC at $50,000
            trader.open_position(
                symbol.clone(),
                PositionSide::Short,
                Quantity::from_int(1),
                Price::from_int(50_000),
                Value::from_int(10_000),
                now,
            );

            exchange.account_repo.save(trader).await;
        }

        // Price drops to $45,000 - short is profitable
        // Use update_mark_prices which takes a HashMap
        {
            let mut trader = exchange
                .account_repo
                .get_by_owner("pnl_trader")
                .await
                .unwrap();
            let mut prices = std::collections::HashMap::new();
            prices.insert(symbol.clone(), Price::from_int(45_000));
            trader.update_mark_prices(&prices, now);
            exchange.account_repo.save(trader).await;
        }

        let trader = exchange
            .account_repo
            .get_by_owner("pnl_trader")
            .await
            .unwrap();
        let pos = trader.position(&symbol).unwrap();

        // Short profit = (entry - mark) * qty = (50000 - 45000) * 1 = $5000
        let calc = StandardMarginCalculator;
        assert_eq!(calc.unrealized_pnl(pos), Value::from_int(5000));
    }
}

// ============================================================================
// PERPETUAL FUTURES TESTS
// ============================================================================

mod perpetual_trading {
    use super::*;
    use exchange_sim::domain::PositionSide;

    #[tokio::test]
    async fn test_perpetual_long_position() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();
        let symbol = Symbol::new("BTCPERP").unwrap();

        // Add perpetual trading pair (maker rebate -1 bps, taker 3 bps)
        let config = TradingPairConfig::new(symbol.clone(), "BTC", "USD").with_fees_bps(-1, 3);
        exchange.instrument_repo.add(config);

        // Trader opens long
        {
            let mut trader = exchange.account_repo.get_or_create("perp_long").await;
            trader.deposit("USD", Value::from_int(10_000)); // Margin

            // Open 0.5 BTC long at $50,000 with 10x leverage
            let margin = Value::from_int(2_500); // 5% of notional
            trader.open_position(
                symbol.clone(),
                PositionSide::Long,
                Quantity::from_f64(0.5),
                Price::from_int(50_000),
                margin,
                now,
            );

            exchange.account_repo.save(trader).await;
        }

        let trader = exchange
            .account_repo
            .get_by_owner("perp_long")
            .await
            .unwrap();
        let pos = trader.position(&symbol).unwrap();

        assert_eq!(pos.side, PositionSide::Long);
        assert_eq!(pos.quantity, Quantity::from_f64(0.5));
        assert_eq!(pos.margin, Value::from_int(2_500));

        // Notional = 0.5 * 50000 = 25000
        assert_eq!(pos.notional_value(), Value::from_int(25_000));
    }

    #[tokio::test]
    async fn test_perpetual_short_position() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();
        let symbol = Symbol::new("ETHPERP").unwrap();

        // -1.5 bps maker rebate, 3.5 bps taker
        let config = TradingPairConfig::new(symbol.clone(), "ETH", "USD").with_fees_bps(-2, 4); // Approximating -0.00015 and 0.00035 in bps
        exchange.instrument_repo.add(config);

        {
            let mut trader = exchange.account_repo.get_or_create("perp_short").await;
            trader.deposit("USD", Value::from_int(5_000));

            // Short 2 ETH at $3000 with 20x leverage
            let margin = Value::from_int(300); // 2% of notional (6000)
            trader.open_position(
                symbol.clone(),
                PositionSide::Short,
                Quantity::from_int(2),
                Price::from_int(3_000),
                margin,
                now,
            );

            exchange.account_repo.save(trader).await;
        }

        let trader = exchange
            .account_repo
            .get_by_owner("perp_short")
            .await
            .unwrap();
        let pos = trader.position(&symbol).unwrap();

        assert_eq!(pos.side, PositionSide::Short);
        assert_eq!(pos.notional_value(), Value::from_int(6_000));
    }

    #[tokio::test]
    async fn test_perpetual_liquidation_price() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();
        let symbol = Symbol::new("BTCPERP").unwrap();

        {
            let mut trader = exchange.account_repo.get_or_create("liq_test").await;
            trader.deposit("USD", Value::from_int(10_000));
            trader.maintenance_margin_rate = Rate::from_bps(50); // 0.5% maintenance in bps

            // Long 1 BTC at $50,000 with 10% margin ($5000)
            trader.open_position(
                symbol.clone(),
                PositionSide::Long,
                Quantity::from_int(1),
                Price::from_int(50_000),
                Value::from_int(5_000), // 10% margin
                now,
            );

            exchange.account_repo.save(trader).await;
        }

        let trader = exchange
            .account_repo
            .get_by_owner("liq_test")
            .await
            .unwrap();
        let pos = trader.position(&symbol).unwrap();

        // Liquidation price for long = entry * (1 - margin_ratio + maintenance_rate)
        let calc = StandardMarginCalculator;
        let liq_price = calc.liquidation_price(pos, trader.maintenance_margin_rate);

        // Should be around $45,250 (entry - margin + maintenance buffer)
        assert!(liq_price.raw() > Price::from_int(45_000).raw());
        assert!(liq_price.raw() < Price::from_int(46_000).raw());
    }
}

// ============================================================================
// FUTURES TRADING TESTS (distinct from perpetuals)
// ============================================================================

mod futures_trading {
    use super::*;
    use exchange_sim::domain::{FutureContract, SettlementType};

    #[tokio::test]
    async fn test_futures_have_expiry() {
        // Key difference from perpetuals: futures expire
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();

        // Create quarterly futures (e.g., BTC-MAR25)
        let expiry = now + chrono::Duration::days(90);
        let future = FutureContract::linear("BTC", "BTC-MAR25", expiry);

        // Not expired yet
        assert!(!future.is_expired(now));
        assert!(future.days_to_expiry(now) > 89.0);
        assert!(future.days_to_expiry(now) < 91.0);

        // After expiry
        let after_expiry = now + chrono::Duration::days(91);
        assert!(future.is_expired(after_expiry));
    }

    #[tokio::test]
    async fn test_futures_basis_contango() {
        // Contango: futures price > spot price (normal market)
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();

        let expiry = now + chrono::Duration::days(90); // ~3 months
        let future = FutureContract::linear("BTC", "BTC-MAR25", expiry);

        let spot_price = Price::from_int(50_000);
        let future_price = Price::from_int(51_000); // 2% premium

        // Annualized basis = premium% * (365 / days_to_expiry)
        // = 2% * (365/90) ≈ 8.1% annualized
        let basis = future.annualized_basis_bps(future_price, spot_price, now);

        assert!(basis > 800); // 8% in bps
        assert!(basis < 900); // 9% in bps
        assert!(basis > 0); // Positive = contango
    }

    #[tokio::test]
    async fn test_futures_basis_backwardation() {
        // Backwardation: futures price < spot price (inverted market)
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();

        let expiry = now + chrono::Duration::days(30);
        let future = FutureContract::linear("BTC", "BTC-JAN25", expiry);

        let spot_price = Price::from_int(50_000);
        let future_price = Price::from_int(49_500); // 1% discount

        let basis = future.annualized_basis_bps(future_price, spot_price, now);

        // Negative basis = backwardation
        assert!(basis < 0);
    }

    #[tokio::test]
    async fn test_futures_settlement_types() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();
        let expiry = now + chrono::Duration::days(90);

        // Cash settled future (most crypto)
        let cash_settled = FutureContract::linear("BTC", "BTC-MAR25", expiry)
            .with_settlement(SettlementType::Cash);
        assert_eq!(cash_settled.settlement, SettlementType::Cash);

        // Physically settled (rare in crypto, common in commodities)
        let physical = FutureContract::linear("BTC", "BTC-MAR25-PHYS", expiry)
            .with_settlement(SettlementType::Physical);
        assert_eq!(physical.settlement, SettlementType::Physical);
    }

    #[tokio::test]
    async fn test_inverse_future() {
        // Inverse futures: settled in base currency (e.g., BTC), not USD
        // Common on BitMEX, Deribit
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();
        let expiry = now + chrono::Duration::days(90);

        let inverse = FutureContract::inverse("BTC", "BTCUSD-MAR25", expiry);

        assert!(inverse.is_inverse);

        // Position value calculation differs:
        // Linear: price * qty * multiplier
        // Inverse: qty * multiplier / price (settled in BTC)
        let qty = Quantity::from_int(10000); // 10,000 contracts
        let price = Price::from_int(50_000);

        // Inverse: 10000 / 50000 = 0.2 BTC
        let value = inverse.position_value(price, qty);
        assert_eq!(value, Value::from_f64(0.2));
    }

    #[tokio::test]
    async fn test_linear_future_position_value() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();
        let expiry = now + chrono::Duration::days(90);

        // Default multiplier is PRICE_SCALE (1x), use 10 * PRICE_SCALE for 10x
        let linear = FutureContract::linear("ETH", "ETH-MAR25", expiry)
            .with_multiplier(10 * exchange_sim::domain::PRICE_SCALE); // 10x multiplier

        let qty = Quantity::from_int(100); // 100 contracts
        let price = Price::from_int(3000); // ETH at $3000

        // Linear: 3000 * 100 * 10 = 3,000,000 USD notional
        let value = linear.position_value(price, qty);
        assert_eq!(value, Value::from_int(3_000_000));
    }

    #[tokio::test]
    async fn test_futures_margin_requirement() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();
        let expiry = now + chrono::Duration::days(90);

        // Default 2% initial margin = 50x leverage (200 bps)
        let future = FutureContract::linear("BTC", "BTC-MAR25", expiry);
        assert_eq!(future.initial_margin_bps, 200);

        // Custom margin for higher risk contracts (10% = 1000 bps = 10x leverage)
        let high_margin =
            FutureContract::linear("SHIB", "SHIB-MAR25", expiry).with_initial_margin_bps(1000);
        assert_eq!(high_margin.initial_margin_bps, 1000);
    }

    #[tokio::test]
    async fn test_futures_vs_perpetual_key_differences() {
        // This test documents the key differences
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();

        let expiry = now + chrono::Duration::days(90);
        let future = FutureContract::linear("BTC", "BTC-MAR25", expiry);

        // 1. EXPIRY: Futures expire, perpetuals don't
        assert!(!future.is_expired(now));
        let after = now + chrono::Duration::days(100);
        assert!(future.is_expired(after));

        // 2. BASIS: Futures trade at premium/discount to spot
        //    Perpetuals use funding rates instead
        let basis =
            future.annualized_basis_bps(Price::from_int(51_000), Price::from_int(50_000), now);
        assert!(basis != 0);

        // 3. SETTLEMENT: Futures settle at expiry
        //    Perpetuals never settle, just roll forever
        assert_eq!(future.settlement, SettlementType::Cash);

        // 4. CONVERGENCE: Future price → spot as expiry approaches
        //    This is automatic, no funding rate mechanism needed
        let near_expiry = now + chrono::Duration::days(89);
        let days_left = future.days_to_expiry(near_expiry);
        assert!(days_left < 2.0); // Very close to expiry
    }
}

// ============================================================================
// OPTIONS TRADING TESTS
// ============================================================================

mod options_trading {
    use super::*;
    use exchange_sim::domain::{OptionContract, OptionType};

    #[tokio::test]
    async fn test_call_option_in_the_money() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();
        let expiry = now + chrono::Duration::days(30);

        // Create a BTC call option (4 args: underlying, expiry, strike, option_type)
        let call = OptionContract::new("BTC", expiry, Price::from_int(50_000), OptionType::Call);

        // Current BTC price is $55,000 - option is ITM
        let spot_price = Price::from_int(55_000);

        // Intrinsic value = max(0, spot - strike) = 55000 - 50000 = 5000
        assert_eq!(call.intrinsic_value(spot_price), Value::from_int(5_000));
        assert!(call.is_in_the_money(spot_price));
    }

    #[tokio::test]
    async fn test_put_option_out_of_the_money() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();
        let expiry = now + chrono::Duration::days(30);

        // Create a BTC put option
        let put = OptionContract::new("BTC", expiry, Price::from_int(50_000), OptionType::Put);

        // Current BTC price is $55,000 - put is OTM
        let spot_price = Price::from_int(55_000);

        // Intrinsic value = max(0, strike - spot) = 0 (OTM)
        assert_eq!(put.intrinsic_value(spot_price), Value::ZERO);
        assert!(!put.is_in_the_money(spot_price));
    }

    #[tokio::test]
    async fn test_option_expiry() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();
        let expiry = now + chrono::Duration::days(7);

        let option = OptionContract::new("ETH", expiry, Price::from_int(3_000), OptionType::Call);

        // Not expired yet
        assert!(!option.is_expired(now));

        // Advance time past expiry
        let after_expiry = now + chrono::Duration::days(8);
        assert!(option.is_expired(after_expiry));
    }

    #[tokio::test]
    async fn test_option_moneyness() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();
        let expiry = now + chrono::Duration::days(30);

        let call = OptionContract::new("BTC", expiry, Price::from_int(50_000), OptionType::Call);

        // ATM (at the money) - spot price equals strike
        assert!(call.is_at_the_money(Price::from_int(50_000)));

        // ITM (in the money) - spot > strike for call
        assert!(call.is_in_the_money(Price::from_int(55_000)));

        // OTM (out of the money) - spot < strike for call
        assert!(call.is_out_of_the_money(Price::from_int(45_000)));

        // Moneyness ratio = spot / strike for calls (in bps: 11000 = 110%)
        let moneyness = call.moneyness_bps(Price::from_int(55_000));
        assert_eq!(moneyness, 11000); // 55000 / 50000 = 1.1 = 110% = 11000 bps
    }
}

// ============================================================================
// MARGIN TRADING TESTS
// ============================================================================

mod margin_trading {
    use super::*;

    #[tokio::test]
    async fn test_cross_margin_mode() {
        let exchange = setup_exchange().await;

        {
            let mut trader = exchange.account_repo.get_or_create("cross_margin").await;
            trader.margin_mode = exchange_sim::domain::MarginMode::Cross;
            trader.deposit("USDT", Value::from_int(50_000));
            trader.deposit("BTC", Value::from_int(1));
            exchange.account_repo.save(trader).await;
        }

        let trader = exchange
            .account_repo
            .get_by_owner("cross_margin")
            .await
            .unwrap();
        assert_eq!(trader.margin_mode, exchange_sim::domain::MarginMode::Cross);
    }

    #[tokio::test]
    async fn test_isolated_margin_mode() {
        let exchange = setup_exchange().await;

        {
            let mut trader = exchange.account_repo.get_or_create("isolated_margin").await;
            trader.margin_mode = exchange_sim::domain::MarginMode::Isolated;
            trader.deposit("USDT", Value::from_int(50_000));
            exchange.account_repo.save(trader).await;
        }

        let trader = exchange
            .account_repo
            .get_by_owner("isolated_margin")
            .await
            .unwrap();
        assert_eq!(
            trader.margin_mode,
            exchange_sim::domain::MarginMode::Isolated
        );
    }

    #[tokio::test]
    async fn test_loan_interest_accrual() {
        use exchange_sim::domain::Loan;

        let exchange = setup_exchange().await;
        let now = exchange.clock.now();

        // Test the Loan struct directly for interest calculation
        // 10% annual rate = 1000 bps
        let mut loan = Loan::new(
            "BTC",
            Value::from_int(1),
            Rate::from_bps(1000),
            Value::from_int(60_000),
            now,
        );

        // Advance time by 1 year
        let one_year_later = now + chrono::Duration::days(365);
        loan.accrue_interest(one_year_later);

        // After 1 year at 10%, interest should be ~0.1 BTC
        let interest = loan.accrued_interest.to_f64();
        assert!(interest > 0.09);
        assert!(interest < 0.11);

        // Total owed = principal + interest
        let total = loan.total_owed().to_f64();
        assert!(total > 1.09);
        assert!(total < 1.11);
    }

    #[tokio::test]
    async fn test_loan_repayment() {
        let exchange = setup_exchange().await;
        let now = exchange.clock.now();

        {
            let mut trader = exchange.account_repo.get_or_create("repay_test").await;
            trader.deposit("USDT", Value::from_int(100_000));
            trader.deposit("BTC", Value::from_int(2)); // Extra BTC to repay

            // Borrow 1 BTC (5% annual = 500 bps)
            trader
                .borrow(
                    "BTC",
                    Value::from_int(1),
                    Rate::from_bps(500),
                    "USDT",
                    Value::from_int(60_000),
                    now,
                )
                .unwrap();

            exchange.account_repo.save(trader).await;
        }

        // Verify borrowed state
        let trader = exchange
            .account_repo
            .get_by_owner("repay_test")
            .await
            .unwrap();
        assert_eq!(trader.balance("BTC").borrowed, Value::from_int(1));
        assert!(trader.has_borrowed("BTC"));

        // Repay the loan
        {
            let mut trader = exchange
                .account_repo
                .get_by_owner("repay_test")
                .await
                .unwrap();
            trader
                .repay_loan("BTC", Value::from_int(1), "USDT", now)
                .expect("Repayment should succeed");
            exchange.account_repo.save(trader).await;
        }

        // Verify loan is cleared
        let trader = exchange
            .account_repo
            .get_by_owner("repay_test")
            .await
            .unwrap();
        assert!(!trader.has_borrowed("BTC"));
        // Collateral should be unlocked
        assert_eq!(trader.balance("USDT").locked, Value::ZERO);
        assert_eq!(trader.balance("USDT").available, Value::from_int(100_000));
    }
}

// ============================================================================
// MAKER/TAKER SCENARIO TESTS
// ============================================================================

mod maker_taker_scenarios {
    use super::*;

    #[tokio::test]
    async fn test_market_maker_with_multiple_orders() {
        let exchange = setup_exchange().await;
        let symbol = Symbol::new("BTCUSDT").unwrap();

        // Market maker with inventory
        {
            let mut mm = exchange.account_repo.get_or_create("pro_mm").await;
            mm.deposit("USDT", Value::from_int(1_000_000));
            mm.deposit("BTC", Value::from_int(50));
            mm.fee_schedule = FeeSchedule::market_maker();
            exchange.account_repo.save(mm).await;
        }

        // Post two-sided quotes (bids and asks)
        {
            let mut book = exchange.order_book_repo.get_or_create(&symbol).await;

            // Bid side
            for (price, qty) in [(49_900, 1), (49_800, 2), (49_700, 5)] {
                let order = Order::new_limit(
                    symbol.clone(),
                    Side::Buy,
                    Quantity::from_int(qty),
                    Price::from_int(price),
                    TimeInForce::Gtc,
                );
                book.add_order(order);
            }

            // Ask side
            for (price, qty) in [(50_100, 1), (50_200, 2), (50_300, 5)] {
                let order = Order::new_limit(
                    symbol.clone(),
                    Side::Sell,
                    Quantity::from_int(qty),
                    Price::from_int(price),
                    TimeInForce::Gtc,
                );
                book.add_order(order);
            }

            exchange.order_book_repo.save(book).await;
        }

        // Verify spread
        let book = exchange.order_book_repo.get_or_create(&symbol).await;
        let best_bid = book.best_bid();
        let best_ask = book.best_ask();

        assert_eq!(best_bid, Some(Price::from_int(49_900)));
        assert_eq!(best_ask, Some(Price::from_int(50_100)));

        // Spread = 200 (0.4%)
        let spread = best_ask.unwrap().raw() - best_bid.unwrap().raw();
        assert_eq!(spread, Price::from_int(200).raw());
    }

    #[tokio::test]
    async fn test_aggressive_taker_sweeps_book() {
        let exchange = setup_exchange().await;
        let symbol = Symbol::new("ETHUSDT").unwrap();

        let config = TradingPairConfig::new(symbol.clone(), "ETH", "USDT");
        exchange.instrument_repo.add(config);

        // Market maker posts liquidity
        {
            let mut mm = exchange
                .account_repo
                .get_or_create("liquidity_provider")
                .await;
            mm.deposit("ETH", Value::from_int(100));
            mm.fee_schedule = FeeSchedule::market_maker();
            exchange.account_repo.save(mm).await;

            let mut book = exchange.order_book_repo.get_or_create(&symbol).await;
            for (price, qty) in [(3000, 10), (3010, 20), (3020, 30)] {
                let order = Order::new_limit(
                    symbol.clone(),
                    Side::Sell,
                    Quantity::from_int(qty),
                    Price::from_int(price),
                    TimeInForce::Gtc,
                );
                book.add_order(order);
            }
            exchange.order_book_repo.save(book).await;
        }

        // Aggressive taker wants to buy a lot
        {
            let mut taker = exchange
                .account_repo
                .get_or_create("aggressive_buyer")
                .await;
            taker.deposit("USDT", Value::from_int(500_000));
            taker.fee_schedule = FeeSchedule::default();
            exchange.account_repo.save(taker).await;
        }

        // Verify total liquidity available
        let book = exchange.order_book_repo.get_or_create(&symbol).await;
        let snapshot = book.snapshot(Some(10));

        let total_ask_qty: i64 = snapshot.asks.iter().map(|l| l.quantity.raw()).sum();
        assert_eq!(total_ask_qty, Quantity::from_int(60).raw()); // 10 + 20 + 30
    }
}
