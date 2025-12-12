# Signal Generation

## Overview

Signal generation is the first operation in the order lifecycle. This document covers signal generation across frequencies (HFT, MFT, LFT) and strategy types.

**Context:** Exotic crypto pairs, potentially no derivatives for hedging. Focus is HFT/MFT.

---

## Table of Contents

1. [Signal Types by Frequency](#1-signal-types-by-frequency)
2. [Mean Reversion Signals](#2-mean-reversion-signals)
3. [Momentum / Directional Signals](#3-momentum--directional-signals)
4. [Statistical Arbitrage](#4-statistical-arbitrage)
5. [Latency Arbitrage](#5-latency-arbitrage)
6. [Triangular & Multi-Leg Arbitrage](#6-triangular--multi-leg-arbitrage)
7. [Market Making](#7-market-making)
8. [Adverse Selection & Toxic Flow](#8-adverse-selection--toxic-flow)
9. [Machine Learning Features](#9-machine-learning-features)
10. [Unified Signal Output](#10-unified-signal-output)

---

# 1. Signal Types by Frequency

| Frequency | Timeframe | Data Granularity | What You're Modeling | Typical Holding Period |
|-----------|-----------|------------------|---------------------|----------------------|
| **HFT** | Microseconds to seconds | Tick-by-tick, L2/L3 order book | Order flow, microstructure | < 1 minute |
| **MFT** | Seconds to hours | Second/minute bars, L1/L2 | Price dynamics, short-term patterns | Minutes to hours |
| **LFT** | Hours to months | Minute/hour/daily bars | Fundamental value, macro factors | Days to months |

**Key insight:** As frequency increases, you shift from modeling *price* to modeling *order flow*.

---

# 2. Mean Reversion Signals

Mean reversion = price (or spread) deviates from "fair value" and is expected to revert.

## 2.1 HFT Mean Reversion

At HFT, mean reversion happens at the microstructure level.

### Bid-Ask Bounce
Trades alternate between bid and ask. A trade at the ask is likely followed by a trade at the bid.

```
Signal: Last trade was at ask → expect next trade closer to bid
Model: Autocorrelation of trade signs is negative at lag 1
```

### Microprice as Fair Value

The mid-price is a naive fair value. The **microprice** adjusts for order book imbalance:

```
imbalance = bid_qty / (bid_qty + ask_qty)
microprice = ask_price × imbalance + bid_price × (1 - imbalance)
```

Or the more general **Volume-Weighted Microprice** across multiple levels:

```
microprice = Σ(price_i × qty_i) / Σ(qty_i)  for top N levels
```

**Signal:** `mid_price - microprice` → if positive, mid is above fair value → expect down tick

### Order Book Imbalance Mean Reversion

Extreme imbalances tend to revert (either price moves or book refills):

```
OBI = (bid_qty - ask_qty) / (bid_qty + ask_qty)    ∈ [-1, 1]
```

If OBI is extremely positive but price hasn't moved, either:
- Price will move up (momentum), or
- Bid side will get hit/cancelled (mean reversion of imbalance)

### Cross-Venue Mean Reversion

Same asset trades on multiple venues at slightly different prices:

```
spread = price_venue_A - price_venue_B
signal = z_score(spread) over rolling window
```

In crypto: BTC-USDT on Binance vs BTC-USDT on OKX.

---

## 2.2 MFT Mean Reversion

### Ornstein-Uhlenbeck (OU) Process

Models price as mean-reverting stochastic process:

```
dX_t = θ(μ - X_t)dt + σdW_t

Where:
- μ = long-term mean (fair value)
- θ = speed of mean reversion (higher = faster reversion)
- σ = volatility
- W_t = Wiener process (Brownian motion)
```

**Half-life of mean reversion:**
```
half_life = ln(2) / θ
```

**Calibration:** Regress `ΔX_t` on `X_{t-1}`:
```
ΔX_t = a + b × X_{t-1} + ε_t

θ = -b / Δt
μ = a / θ
σ = std(ε_t) / sqrt(Δt)
```

**Signal:**
```
z_score = (X_t - μ) / (σ / sqrt(2θ))

if z_score > threshold → SELL
if z_score < -threshold → BUY
```

### Kalman Filter

State-space model for dynamic fair value estimation:

**State equation:**
```
x_t = A × x_{t-1} + w_t    (w_t ~ N(0, Q))
```

**Observation equation:**
```
y_t = H × x_t + v_t    (v_t ~ N(0, R))
```

The filter gives you:
- `x_t|t` = estimate of fair value given data up to t
- `P_t|t` = uncertainty in that estimate

**Signal:**
```
deviation = observed_price - kalman_fair_value
z_score = deviation / sqrt(P_t|t + R)
```

**Advantage:** Adapts to changing regimes, handles noise optimally under Gaussian assumptions.

### Rolling Z-Score

Simple, no model assumptions:

```
z_score = (price - rolling_mean(N)) / rolling_std(N)
```

**Signal:**
```
if z_score > +threshold → SELL
if z_score < -threshold → BUY
```

---

## 2.3 LFT Mean Reversion

### Cointegration (Pairs/Baskets)

Two (or more) non-stationary series that share a common stochastic trend. Their linear combination is stationary.

**For pair (X, Y):**
```
Regress: Y_t = β × X_t + ε_t
Spread: S_t = Y_t - β × X_t
```

**Test for stationarity:** Augmented Dickey-Fuller (ADF) test on spread:
```
H0: Spread has unit root (non-stationary)
H1: Spread is stationary (mean-reverting)

Reject H0 if ADF statistic < critical value
```

**For multiple assets:** Johansen test identifies cointegrating vectors.

**Signal:**
```
z_score = (spread - mean(spread)) / std(spread)

if z_score > +threshold → SELL Y, BUY X (or sell spread)
if z_score < -threshold → BUY Y, SELL X (or buy spread)
```

**Crypto application:** Cointegration between related tokens (e.g., ATOM/OSMO in Cosmos ecosystem), or between perpetual and spot.

---

# 3. Momentum / Directional Signals

Momentum = price movement will continue in the same direction.

## 3.1 HFT Momentum (Order Flow)

At HFT, momentum comes from **order flow**, not price patterns.

### Trade Flow Imbalance (TFI)

Classifies trades as buyer-initiated or seller-initiated and measures imbalance:

**Trade classification (Lee-Ready algorithm):**
```
if trade_price > mid_price → buyer-initiated
if trade_price < mid_price → seller-initiated
if trade_price == mid_price → use tick rule (compare to previous trade)
```

**Trade Flow Imbalance:**
```
TFI = (buy_volume - sell_volume) / (buy_volume + sell_volume)

Over window of N trades or T seconds
```

**Signal:** Positive TFI → expect price to move up (aggressive buyers)

### Order Flow Imbalance (OFI)

Measures changes in the order book, not just trades:

```
OFI_t = ΔBid_qty × I(bid_price ≥ bid_price_{t-1}) 
      - ΔAsk_qty × I(ask_price ≤ ask_price_{t-1})
```

Intuition: 
- Bid qty increases at same/better price → buying pressure
- Ask qty increases at same/better price → selling pressure

**Signal:** Cumulative OFI over window predicts short-term price direction.

### Sweep Detection

A large order "sweeps" multiple price levels:

```
sweep_detected = trade_size > top_of_book_qty AND multiple levels consumed

sweep_direction = BUY if swept through asks, SELL if swept through bids
```

**Signal:** Sweep indicates aggressive participant → expect continuation.

### Large Order Detection

Track unusual order sizes:

```
order_size_zscore = (order_size - mean(recent_sizes)) / std(recent_sizes)

if order_size_zscore > threshold → large order detected
```

**Signal:** Large orders on one side → momentum in that direction.

---

## 3.2 MFT Momentum

### Returns Over Lookback

Simple, academically validated (Jegadeesh & Titman, 1993):

```
momentum = (price_t - price_{t-N}) / price_{t-N}
```

Or log returns:
```
momentum = ln(price_t / price_{t-N})
```

**Signal:**
```
if momentum > threshold → BUY
if momentum < -threshold → SELL
```

### Linear Regression Slope

Fit OLS to recent prices, use slope as momentum measure:

```
price_t = α + β × t + ε_t

momentum = β (slope)
t_stat = β / std_error(β)
```

**Signal:**
```
if t_stat > +2 → statistically significant uptrend → BUY
if t_stat < -2 → statistically significant downtrend → SELL
```

### Hurst Exponent

Measures persistence of a time series:

```
H = 0.5 → random walk (no predictability)
H > 0.5 → persistent (trending / momentum)
H < 0.5 → anti-persistent (mean-reverting)
```

**Estimation (R/S method):**
```
For different lag sizes n:
1. Divide series into subseries of length n
2. Calculate range R and standard deviation S for each
3. E[R/S] ~ c × n^H

Regress log(R/S) on log(n), slope = H
```

**Signal:** H > 0.5 + buffer → momentum regime, use momentum signals.

### Variance Ratio

Tests random walk hypothesis:

```
VR(k) = Var(k-period returns) / (k × Var(1-period returns))

VR = 1 → random walk
VR > 1 → positive autocorrelation (momentum)
VR < 1 → negative autocorrelation (mean reversion)
```

**Statistical test (Lo-MacKinlay):**
```
z = (VR - 1) / std_error(VR)

if z > 1.96 → reject random walk, momentum present
```

---

## 3.3 LFT Momentum

### Time-Series Momentum

Moskowitz, Ooi, Pedersen (2012):

```
signal = sign(r_{t-12m, t-1m})  # Return over past 12 months, skip last month

position = signal × (target_vol / realized_vol)  # Vol-scaled
```

### Cross-Sectional Momentum

Rank assets by past returns, go long winners, short losers:

```
For each asset i:
    momentum_i = return over lookback period
    rank_i = percentile_rank(momentum_i)

Long: top decile
Short: bottom decile
```

---

# 4. Statistical Arbitrage

Stat arb = trading relative mispricings between related instruments.

## 4.1 Pairs Trading

Classic mean reversion on spread between two cointegrated assets.

**Setup:**
```
1. Find cointegrated pairs (ADF test on spread)
2. Calculate hedge ratio β (OLS or Kalman filter)
3. Monitor spread: S_t = Y_t - β × X_t
4. Trade when spread deviates significantly
```

**Entry:**
```
z_score = (S_t - μ_S) / σ_S

if z_score > +entry_threshold → SHORT spread (sell Y, buy β units of X)
if z_score < -entry_threshold → LONG spread (buy Y, sell β units of X)
```

**Exit:**
```
if z_score crosses 0 → close position
if z_score exceeds stop_loss → close position (spread diverged)
```

**Crypto application:** 
- Token pairs in same ecosystem (ATOM/OSMO, SOL/RAY)
- Same token on different venues (basis trade)
- Perpetual vs spot (funding rate arb)

## 4.2 Basket / Index Arbitrage

Trade deviations between an index/basket and its components:

```
fair_value_index = Σ(weight_i × price_i) for all components

spread = traded_index_price - fair_value_index
```

**Signal:**
```
if spread > transaction_costs → sell index, buy components
if spread < -transaction_costs → buy index, sell components
```

**Crypto application:** ETF-like tokens vs underlying basket.

## 4.3 Factor-Based Stat Arb

Model expected returns using factors, trade residuals:

```
E[r_i] = α_i + β_{i,1} × F_1 + β_{i,2} × F_2 + ... + ε_i

Where F_j are factors (market, momentum, value, etc.)
```

**Signal:**
```
residual_i = actual_return_i - predicted_return_i

if residual strongly negative → asset underperformed, expect reversion → BUY
if residual strongly positive → asset overperformed, expect reversion → SELL
```

---

# 5. Latency Arbitrage

Latency arb = exploiting speed advantage to trade on stale prices.

## 5.1 Classic Latency Arbitrage

**Setup:**
- Price updates on Exchange A before Exchange B
- You see A's new price before B updates
- Trade on B at stale price

```
if price_A > price_B + transaction_costs:
    BUY on B (stale low price)
    SELL on A (or wait for B to update)
```

**In practice:**
- Requires being faster than other participants
- Co-location, optimized network paths, FPGA
- Race to the bottom on latency

## 5.2 Stale Quote Arbitrage

Market makers don't update quotes fast enough after news/price move:

```
Detect: Large price move on correlated asset
Signal: Market maker quotes on target asset are now stale
Action: Hit stale quotes before they update
```

**Crypto application:**
- CEX updates before DEX
- Major pair (BTC/USDT) moves before exotic pair (ALT/BTC)
- Oracle latency in DeFi

## 5.3 Signals for Latency Arb

| Signal | Description |
|--------|-------------|
| **Lead-lag relationship** | Asset A leads asset B by N milliseconds |
| **Cross-venue price divergence** | Same asset, different prices |
| **Correlated asset move** | BTC moves, ALT quote is stale |
| **Order book staleness** | Quote timestamp vs current time |

**Detection:**
```
lead_lag = argmax(correlation(returns_A_{t}, returns_B_{t+lag}))

if lead_lag > 0 → A leads B
```

---

# 6. Triangular & Multi-Leg Arbitrage

## 6.1 Triangular Arbitrage

Three currency pairs form a triangle. Exploit mispricings.

**Example:** BTC, ETH, USDT

```
Path 1: USDT → BTC → ETH → USDT
Path 2: USDT → ETH → BTC → USDT
```

**Calculation:**
```
rate_BTC_USDT = 50000  (1 BTC = 50000 USDT)
rate_ETH_USDT = 3000   (1 ETH = 3000 USDT)
rate_ETH_BTC = 0.059   (1 ETH = 0.059 BTC)

Implied ETH/BTC from USD rates:
implied_ETH_BTC = rate_ETH_USDT / rate_BTC_USDT = 3000/50000 = 0.06

Discrepancy:
arb_signal = implied_ETH_BTC / actual_ETH_BTC - 1 
           = 0.06 / 0.059 - 1 = 0.017 = 1.7%
```

**Signal:**
```
if arb_signal > transaction_costs:
    Execute: USDT → BTC → ETH → USDT
    
if arb_signal < -transaction_costs:
    Execute: USDT → ETH → BTC → USDT
```

**Generalized formula:**
```
For triangle A → B → C → A:

implied_rate = rate_AB × rate_BC × rate_CA

if implied_rate > 1 + costs → arbitrage exists
```

## 6.2 Multi-Leg Arbitrage (N > 3)

**Bellman-Ford for Arbitrage Detection:**

Model exchange rates as graph edges with log weights:

```
weight(A → B) = -log(rate_AB)

Arbitrage exists if negative cycle exists in graph.

Use Bellman-Ford algorithm:
- If you can relax an edge after V-1 iterations → negative cycle exists
- The cycle is the arbitrage path
```

**Algorithm:**
```python
# Initialize distances
dist[source] = 0
dist[other] = infinity

# Relax edges V-1 times
for i in range(V - 1):
    for (u, v, rate) in edges:
        weight = -log(rate)
        if dist[u] + weight < dist[v]:
            dist[v] = dist[u] + weight
            predecessor[v] = u

# Check for negative cycle (arbitrage)
for (u, v, rate) in edges:
    weight = -log(rate)
    if dist[u] + weight < dist[v]:
        # Negative cycle found - trace back to find it
        return trace_cycle(v, predecessor)
```

## 6.3 Execution Challenges

| Challenge | Description | Mitigation |
|-----------|-------------|------------|
| **Leg risk** | Price moves between legs | Execute simultaneously, or accept risk |
| **Partial fills** | One leg fills, others don't | Use IOC orders, size conservatively |
| **Latency** | Others see same arb | Be faster, or find less competitive venues |
| **Inventory** | Need starting inventory in each asset | Pre-position, or use single starting asset |

---

# 7. Market Making

Market making = providing liquidity by quoting both bid and ask, profiting from spread.

## 7.1 Basic Market Making Model

**Quote placement:**
```
bid_price = fair_value - half_spread - skew
ask_price = fair_value + half_spread + skew
```

Where:
- `fair_value` = your estimate of true price (microprice, Kalman, etc.)
- `half_spread` = compensation for adverse selection + inventory risk
- `skew` = adjustment based on inventory (more on this below)

## 7.2 Fair Value Estimation

Critical for market making. If your fair value is wrong, you get picked off.

| Method | Formula | Use Case |
|--------|---------|----------|
| **Mid price** | `(bid + ask) / 2` | Naive, vulnerable to manipulation |
| **Microprice** | `ask × (bid_qty/(bid_qty+ask_qty)) + bid × (ask_qty/(bid_qty+ask_qty))` | Better, accounts for imbalance |
| **Weighted mid** | `(bid × ask_qty + ask × bid_qty) / (bid_qty + ask_qty)` | Same as microprice |
| **Multi-level microprice** | Volume-weighted across N levels | More robust |
| **Kalman filter** | State-space model | Adapts to changing dynamics |
| **Cross-asset model** | Regression on correlated assets | Uses external information |

## 7.3 Spread Determination

Spread must cover:
1. **Adverse selection cost** - informed traders pick you off
2. **Inventory risk** - holding inventory in volatile asset
3. **Operational costs** - fees, infrastructure

**Avellaneda-Stoikov model:**

```
optimal_spread = γ × σ² × T + (2/γ) × ln(1 + γ/k)

Where:
- γ = risk aversion parameter
- σ = volatility
- T = time to end of trading period
- k = order arrival rate parameter
```

**Simpler heuristic:**
```
min_spread = 2 × (adverse_selection_cost + volatility_buffer + fees)
```

## 7.4 Inventory Management (Skew)

Don't want to accumulate one-sided inventory. Skew quotes to mean-revert inventory.

**Linear skew:**
```
skew = -κ × inventory

bid_price = fair_value - half_spread - κ × inventory
ask_price = fair_value + half_spread - κ × inventory
```

If inventory is positive (long), both quotes shift down → more likely to sell.

**Avellaneda-Stoikov reservation price:**
```
reservation_price = fair_value - γ × σ² × inventory × T

Quote around reservation price instead of fair value
```

## 7.5 Market Making Signals

| Signal | Interpretation | Action |
|--------|----------------|--------|
| **Spread too wide** | Opportunity to provide liquidity | Quote tighter |
| **Spread too tight** | Not enough edge | Don't quote, or quote wider |
| **Inventory too large** | Risk exposure | Skew quotes aggressively, or hedge |
| **Toxic flow detected** | Adverse selection high | Widen spread or stop quoting |
| **Volatility spike** | Risk increased | Widen spread |

---

# 8. Adverse Selection & Toxic Flow

The enemy of market makers (and limit orders in general).

## 8.1 What is Adverse Selection?

When you trade with someone who knows more than you:
- You buy → price goes down (they sold because they knew)
- You sell → price goes up (they bought because they knew)

**Result:** Your fills are systematically on the wrong side.

## 8.2 Measuring Adverse Selection

### Realized Spread

Compare fill price to price N seconds later:

```
For a BUY fill at price P:
realized_spread = P - mid_price_{t+N}

For a SELL fill at price P:
realized_spread = mid_price_{t+N} - P

If realized_spread < 0 → adverse selection (price moved against you)
```

### Kyle's Lambda (Price Impact)

Measures how much price moves per unit of signed order flow:

```
ΔP_t = λ × SignedVolume_t + ε_t

λ = Cov(ΔP, SignedVolume) / Var(SignedVolume)
```

Higher λ → more informed trading → more adverse selection.

**Real-time estimation:**
```
Rolling regression of price changes on signed volume
Monitor λ over time
Spike in λ → informed traders active → widen spread or pull quotes
```

### VPIN (Volume-Synchronized Probability of Informed Trading)

Estimates probability that current volume is from informed traders:

```
1. Divide time into volume buckets (each bucket = V shares)
2. Classify trades as buy/sell (Lee-Ready or bulk classification)
3. For each bucket:
   |buy_volume - sell_volume| = order_imbalance

4. VPIN = mean(order_imbalance) / V over rolling N buckets
```

**Interpretation:**
```
VPIN ∈ [0, 1]
Higher VPIN → more one-sided flow → likely informed trading
```

### Toxic Flow Indicators

| Indicator | Calculation | Interpretation |
|-----------|-------------|----------------|
| **Fill rate asymmetry** | Fill rate on winning vs losing quotes | If losing quotes fill more → toxic |
| **Adverse selection per fill** | Average P&L at T+N per fill | Negative → adverse selection |
| **Large trade frequency** | Count of trades > X × average | More large trades → more informed flow |
| **Order-to-trade ratio** | Orders placed / orders filled | Informed traders have higher fill rates |
| **Time of day** | Track metrics by hour | Some periods more toxic |

## 8.3 Responding to Adverse Selection

| Situation | Response |
|-----------|----------|
| **High VPIN / Kyle's λ** | Widen spreads |
| **Consistent adverse selection** | Improve fair value model |
| **Toxic counterparty identified** | Avoid trading with them (if possible) |
| **News event expected** | Pull quotes or widen significantly |
| **Cross-asset signal detected** | Update fair value before getting picked off |

## 8.4 Identifying Informed Flow in Real-Time

```python
def is_flow_toxic(recent_trades, recent_price_moves):
    """
    Check if recent flow is toxic
    """
    # Calculate signed volume
    signed_vol = sum(t.volume * t.sign for t in recent_trades)
    
    # Calculate price move since first trade
    price_move = current_mid - recent_trades[0].mid_at_time
    
    # If signed volume predicted price move → informed flow
    if sign(signed_vol) == sign(price_move) and abs(price_move) > threshold:
        return True
    
    # Check VPIN
    if calculate_vpin(recent_trades) > vpin_threshold:
        return True
    
    return False
```

---

# 9. Machine Learning Features

Features that can be used in ML models for signal generation.

## 9.1 Order Book Features

| Feature | Calculation | Predictive Of |
|---------|-------------|---------------|
| **Bid-ask spread** | `ask - bid` | Volatility, liquidity |
| **Mid price** | `(bid + ask) / 2` | Reference price |
| **Microprice** | Imbalance-weighted mid | Fair value |
| **Imbalance L1** | `(bid_qty - ask_qty) / (bid_qty + ask_qty)` | Short-term direction |
| **Imbalance L1-L5** | Same, summed over 5 levels | Direction with more signal |
| **Depth imbalance** | Total bid depth vs ask depth | Supply/demand |
| **Spread volatility** | Rolling std of spread | Regime |
| **Book pressure** | Rate of change of imbalance | Momentum of flow |
| **Level slopes** | Price gap between levels | Book shape |
| **Queue position** | Your order's position in queue | Fill probability |

## 9.2 Trade Features

| Feature | Calculation | Predictive Of |
|---------|-------------|---------------|
| **Trade imbalance** | `(buy_vol - sell_vol) / total_vol` | Direction |
| **Trade intensity** | Trades per second | Volatility, activity |
| **Average trade size** | `total_volume / trade_count` | Participant type |
| **Large trade indicator** | `I(trade_size > k × avg)` | Informed flow |
| **VPIN** | Order imbalance in volume buckets | Toxicity |
| **Kyle's lambda** | Price impact per volume | Informed trading |
| **Trade arrival rate** | Exponential fit to inter-arrival times | Activity regime |
| **Signed volume momentum** | Cumulative signed volume | Order flow direction |

## 9.3 Price/Return Features

| Feature | Calculation | Predictive Of |
|---------|-------------|---------------|
| **Returns (multiple horizons)** | `r_1s, r_5s, r_30s, r_1m, r_5m, ...` | Momentum at different scales |
| **Realized volatility** | `std(returns) × sqrt(annualization)` | Risk, spread sizing |
| **Return autocorrelation** | `corr(r_t, r_{t-1})` | Mean reversion vs momentum |
| **High-low range** | `(high - low) / mid` | Intraday volatility |
| **Return skewness** | Third moment of returns | Tail risk |
| **Hurst exponent** | R/S analysis | Trending vs mean-reverting |
| **Variance ratio** | Multi-period variance comparison | Random walk test |

## 9.4 Cross-Asset Features

| Feature | Calculation | Predictive Of |
|---------|-------------|---------------|
| **Beta to BTC** | Rolling regression coefficient | Systematic risk |
| **Residual from BTC** | `return - beta × BTC_return` | Idiosyncratic move |
| **Correlation (rolling)** | Rolling correlation with majors | Regime |
| **Lead-lag** | Cross-correlation at different lags | Who leads |
| **Spread to correlated asset** | Deviation from historical relationship | Stat arb signal |
| **Sector momentum** | Average return of related tokens | Sector flow |

## 9.5 Time/Calendar Features

| Feature | Calculation | Predictive Of |
|---------|-------------|---------------|
| **Hour of day** | 0-23 | Liquidity, volatility patterns |
| **Day of week** | 0-6 | Weekend effects (crypto trades 24/7) |
| **Time since last trade** | Seconds | Activity level |
| **Time to funding** | Seconds until perp funding | Funding rate arb timing |
| **Distance from round number** | `price % round_number` | Psychological levels |

## 9.6 Inventory/Position Features (for execution)

| Feature | Calculation | Predictive Of |
|---------|-------------|---------------|
| **Current inventory** | Signed position | Skew needed |
| **Inventory as % of limit** | `inventory / max_inventory` | Risk level |
| **Time in position** | Seconds since entry | Urgency |
| **Unrealized P&L** | Current P&L on position | Exit signal |

## 9.7 Feature Engineering Tips

**Normalization:**
```
# Z-score normalization (rolling)
feature_normalized = (feature - rolling_mean) / rolling_std

# Percentile rank (rolling)
feature_percentile = percentile_rank(feature, lookback)
```

**Combining timeframes:**
```
# Multi-horizon features
features = [imbalance_1s, imbalance_5s, imbalance_30s, imbalance_1m]
```

**Interaction features:**
```
# Imbalance × volatility interaction
signal_strength = imbalance × (1 / volatility)

# Cross-asset residual momentum
residual_momentum = my_return - beta × market_return
```

## 9.8 Common ML Models for Signals

| Model | Use Case | Pros | Cons |
|-------|----------|------|------|
| **Linear/Ridge Regression** | Baseline, interpretable | Fast, interpretable | Limited capacity |
| **Logistic Regression** | Direction prediction | Fast, interpretable | Linear boundaries |
| **Random Forest** | Non-linear, feature importance | Handles interactions | Overfitting risk |
| **Gradient Boosting (XGBoost, LightGBM)** | Best tabular performance | High accuracy | Black box |
| **LSTM/GRU** | Sequence modeling | Captures temporal patterns | Slow, needs lots of data |
| **Transformer** | Sequence modeling | State of art for sequences | Very data hungry |
| **Online Learning (SGD)** | Adapts to new data | Real-time adaptation | Needs careful tuning |

---

# 10. Unified Signal Output

Regardless of strategy, all signals should output a consistent structure:

```python
Signal = {
    # Identification
    "timestamp": datetime,           # When signal was generated
    "signal_id": str,                # Unique identifier
    "strategy_id": str,              # Which strategy generated this
    "strategy_type": str,            # MEAN_REVERSION | MOMENTUM | STAT_ARB | 
                                     # LATENCY_ARB | TRIANGULAR_ARB | MARKET_MAKING
    
    # Core signal
    "symbol": str,                   # Primary instrument
    "direction": str,                # BUY | SELL | NONE
    "strength": float,               # Signal strength (for sizing)
    "confidence": float,             # Model confidence [0, 1]
    "urgency": float,                # How fast does edge decay [0, 1]
    
    # Prices
    "current_price": float,          # Observed price
    "fair_value": float,             # Model's fair value estimate
    "entry_price": float,            # Suggested entry (for limit orders)
    "target_price": float,           # Expected exit price
    "stop_price": float,             # Stop loss level
    
    # For multi-leg strategies
    "legs": [
        {
            "symbol": str,
            "direction": str,
            "ratio": float,          # Hedge ratio or weight
            "venue": str,            # Which exchange
        }
    ],
    
    # Risk metrics
    "expected_edge_bps": float,      # Expected profit in basis points
    "half_life_seconds": float,      # For mean reversion: expected time to revert
    "model_variance": float,         # Uncertainty in fair value
    
    # Metadata
    "features": dict,                # Key features that drove signal
    "model_version": str,            # Model version for tracking
}
```

---

## Summary: Signal Types at a Glance

| Strategy | Frequency | Signal Source | Edge Source |
|----------|-----------|---------------|-------------|
| **Mean Reversion** | HFT/MFT/LFT | Z-score, OU, Kalman, Cointegration | Price returns to fair value |
| **Momentum** | MFT/LFT | Returns, slope, Hurst | Trend continues |
| **Order Flow** | HFT | Imbalance, TFI, OFI | Flow predicts direction |
| **Stat Arb** | MFT/LFT | Spread z-score, residuals | Relative mispricing corrects |
| **Latency Arb** | HFT | Cross-venue divergence, lead-lag | Speed advantage |
| **Triangular Arb** | HFT/MFT | Rate inconsistency | Prices must be consistent |
| **Market Making** | HFT/MFT | Spread, fair value, inventory | Earn spread, manage inventory |

---
