# Trading Strategies

Core trading algorithms for market makers and directional traders.

See [Glossary](trading_glossary.md) for symbol definitions.

---

## Table of Contents

1. [Agent Taxonomy](#agent-taxonomy)
2. [Quoting Strategies (Market Making)](#quoting-strategies-market-making)
3. [Execution Algorithms](#execution-algorithms)
4. [Signal Strategies](#signal-strategies)
5. [Regime Detection](#regime-detection)
6. [Order Flow Analysis](#order-flow-analysis)
7. [Connecting Signal to Execution to Risk](#connecting-signal-to-execution-to-risk)
8. [Calibration Guidance](#calibration-guidance)

---

## Agent Taxonomy

| Agent Type | FV Access | Role | Primary Strategy |
|------------|-----------|------|------------------|
| **Fund** | Estimates (better models) | Directional (taker) | Model signal + Almgren-Chriss execution |
| **DMM** | Estimates (order flow edge) | Liquidity provider (maker) | A-S / GLFT optimal quoting |
| **Prop MM** | Estimates (speed edge) | Liquidity provider (maker) | A-S / GLFT + flow analysis |
| **Momentum Trader** | Infers from price | Directional (taker) | Trend following |
| **Mean Reversion Trader** | Infers from price | Directional (taker) | Statistical arbitrage |
| **Retail/Noise** | None | Random (taker) | Random/behavioral |

### In the ABM Simulation

For simulation purposes, we model information asymmetry by giving some agents direct FV access:

| Agent (Simulation) | Sees FV? | Purpose |
|--------------------|----------|---------|
| Fund | Yes | Model institutional price discovery |
| DMM | Yes | Model informed market making |
| Others | No | Model uninformed participants |

This is a **modeling simplification** - in reality, "informed" means "better estimates."

---

## Quoting Strategies (Market Making)

### A-S and GLFT: Universal Market Making Framework

**All market makers** use variants of Avellaneda-Stoikov or GLFT. The framework is:

```
Reference price: s = YOUR BEST ESTIMATE of fair value
Reservation price: r = s - γ_inv·q·σ²·τ  (skew for inventory)
Optimal spread: δ = f(γ_inv, σ, k, fees)  (compensation for risk)
```

**Note on δ**: Throughout this document, δ refers to the **half-spread** (distance from mid to one side).
- Full spread = 2δ
- Bid = mid - δ
- Ask = mid + δ

**What differs between MMs is the reference price `s`:**

| MM Type | How They Estimate Reference Price |
|---------|-----------------------------------|
| Quant MM | Fundamental model + order flow |
| HFT MM | Microstructure signals, cross-venue |
| DMM | Order flow + inventory position |
| Retail MM | Mid price (no edge) |

### Avellaneda-Stoikov (2008)

The original optimal market making model.

**Formulas:**
```
Reservation price: r = s - γ_inv·q·σ²·τ
Optimal half-spread: δ = γ_inv·σ²·τ + (2/γ_inv)·ln(1 + γ_inv/k)
```

**Parameters:**
| Symbol | Name | Description |
|--------|------|-------------|
| s | Reference price | Mid price or fair value estimate |
| γ_inv | Risk aversion | Inventory risk penalty (0.01-1.0) |
| q | Inventory | Current position (+long, -short) |
| σ | Volatility | Standard deviation of returns |
| τ | Time remaining | Normalized time to session end |
| k | Arrival intensity | Order arrival rate |

**Spread Components:**
1. `γ_inv·σ²·τ` = Volatility risk premium
2. `(2/γ_inv)·ln(1+γ_inv/k)` = Adverse selection component

### GLFT (2013)

Extension of A-S with explicit fee handling.

**Formula:**
```
Half-spread: δ = (1/γ_inv)·ln(1 + γ_inv/k) + fee_adjustment
Fee adjustment = (-maker_rebate + taker_fee × unwind_prob) / 2
```

GLFT adds explicit fee handling for real-world exchange economics.

### When to Use Each

| Situation | Model | Reason |
|-----------|-------|--------|
| Academic study | A-S | Cleaner formulation |
| Real trading | GLFT | Accounts for fee structure |
| High inventory | Both | Increase γ_inv for faster mean-reversion |
| High volatility | Both | Spread widens automatically via σ² term |

---

## Execution Algorithms

### Almgren-Chriss: Universal Execution Framework

**All institutional traders** use Almgren-Chriss or variants when executing large orders.

**The Problem:** Execute X shares over horizon T, minimizing:

```
min E[Cost] + λ_risk·Var[Cost]
```

**Impact Model:**
- **Temporary impact**: g(v) = η·v (linear in trade rate v)
- **Permanent impact**: h(v) = γ_perm·v (linear in trade rate v)

**Parameters:**
| Symbol | Name | Description |
|--------|------|-------------|
| X | Total quantity | Shares to execute |
| T | Horizon | Execution time window |
| λ_risk | Risk aversion | Variance penalty (10⁻⁵ - 10⁻²) |
| η | Temp impact | Transient price impact coefficient |
| γ_perm | Perm impact | Permanent price impact coefficient |
| σ | Volatility | Price volatility during execution |

**Note:** λ_risk here is different from λ_kyle (Kyle's Lambda) used in market microstructure. See [Glossary](trading_glossary.md).

**Optimal Trading Trajectory:**
```
x(t) = X₀ · sinh(κ(T-t)) / sinh(κT)

where κ = √(λ_risk·σ²/η)
```

**Intuition:**
- High λ_risk (risk averse) → trade faster (front-loaded)
- Low λ_risk (risk neutral) → trade slower (minimize impact)
- High σ (volatile) → trade faster (timing risk)
- High η (high impact) → trade slower (impact cost)

### Other Execution Algorithms

| Algorithm | Key Feature | Best For |
|-----------|-------------|----------|
| **TWAP** | Equal slices over time | Simple, predictable |
| **VWAP** | Volume-weighted slices | Track volume profile |
| **POV** | Percentage of volume | Minimize market footprint |
| **IS** | Minimize vs arrival price | Benchmark-sensitive |

---

## Signal Strategies

Signal strategies determine **when and which direction to trade**.

### Types of Alpha Signals

| Signal Type | Information Source | Typical User |
|-------------|-------------------|--------------|
| Fundamental | Financials, alternative data | Long-term funds |
| Order Flow | Trade tape, book imbalance | MMs, prop traders |
| Technical (Momentum) | Price history | All traders |
| Technical (Mean Reversion) | Price history | All traders |
| Cross-Asset | Related instruments | Stat arb funds |

### Momentum Strategy

**Premise**: Price trends persist (proxy for: informed traders pushing price to FV)

**Signal (Percentile-Based):**
```python
def momentum_percentile(returns, window=100):
    """
    Where does current return rank historically?
    No distributional assumptions needed.
    """
    recent_returns = returns[-window:]
    current = returns[-1]
    rank = np.sum(recent_returns < current) / len(recent_returns)
    return rank  # 0.95 = stronger than 95% of recent moves

# Trading signal
rank = momentum_percentile(returns, window=100)

if rank > 0.80:  # Top 20% of moves
    signal = BUY
elif rank < 0.20:  # Bottom 20% of moves
    signal = SELL
```

**Why Percentile > Z-Score:**
```
Markets have FAT TAILS (not normal distribution):
  Normal:     P(z > 3) ≈ 0.13%  (rare)
  Reality:    P(z > 3) ≈ 1-5%   (happens often!)

Z-score of 3 is NOT as extreme as it suggests.
Percentile rank makes no distributional assumption.
```

**When It Works:**
- Trending regime (low θ in underlying FV process)
- When informed traders are actively pushing price to FV
- Early in a price discovery cycle

**When It Fails:**
- Mean-reverting regime (high θ)
- Random noise in low-information environments
- Late in price discovery (FV already reached)

### Mean Reversion Strategy

**Premise**: Prices revert to a mean (proxy for: price will return to FV)

**Signal (Percentile-Based):**
```python
def price_percentile(prices, window=100):
    """
    Where does current price rank in recent history?
    Extreme ranks suggest reversion opportunity.
    """
    recent = prices[-window:]
    current = prices[-1]
    rank = np.sum(recent < current) / len(recent)
    return rank

# Trading signal
rank = price_percentile(prices, window=100)

if rank > 0.95:  # Price at top 5% - expect reversion down
    signal = SELL
elif rank < 0.05:  # Price at bottom 5% - expect reversion up
    signal = BUY
```

**Alternative Methods:**
```python
# MAD-based (outlier resistant)
def mad_deviation(prices, window=100):
    """
    Median Absolute Deviation - robust to outliers.
    Better than std for fat-tailed data.
    """
    recent = prices[-window:]
    median = np.median(recent)
    mad = np.median(np.abs(recent - median))
    return (prices[-1] - median) / (1.4826 * mad)

# Quantile bands (replaces Bollinger)
def quantile_bands(prices, window=100, lower_q=0.05, upper_q=0.95):
    """
    Use actual quantiles, not assumed normal std.
    """
    recent = prices[-window:]
    lower = np.percentile(recent, lower_q * 100)
    upper = np.percentile(recent, upper_q * 100)
    median = np.median(recent)
    return lower, median, upper
```

**Method Comparison:**
| Method | Assumes Normal? | Robust to Outliers? | Fat-Tail Safe? |
|--------|-----------------|---------------------|----------------|
| Z-score | Yes | No | No |
| Percentile | No | Yes | Yes |
| MAD | No | Yes | Yes |
| Quantile bands | No | Yes | Yes |

**When It Works:**
- Mean-reverting regime (high θ)
- Price overshoots FV due to noise/momentum traders
- After large price moves

**When It Fails:**
- Trending regime (FV is moving, not price reverting)
- Regime changes (FV shift)
- Low liquidity (can't exit at mean)

---

## Regime Detection

Before applying technical strategies, assess the market regime.

### Regime Characteristics

| Regime | Characteristics | θ Value | Strategy Preference |
|--------|-----------------|---------|---------------------|
| **Trending** | Persistent direction | θ < 0.1 | Momentum |
| **Mean-Reverting** | Quick reversals | θ > 0.5 | Mean Reversion |
| **Volatile** | Large swings | Any | Reduce size |
| **Normal** | Balanced | θ ≈ 0.3 | Mixed |

Where θ is the mean reversion speed in the Ornstein-Uhlenbeck process. See [Glossary](trading_glossary.md) for full definition.

### Detection Methods

```python
# Volatility regime
vol = np.std(returns[-100:])
vol_percentile = percentile_rank(vol, historical_vols)

# Trend strength (autocorrelation)
trend = np.corrcoef(returns[:-1], returns[1:])[0, 1]

# Hurst exponent (persistence)
H = hurst_exponent(prices)  # H > 0.5 = trending, H < 0.5 = mean-reverting

# Variance ratio test
def variance_ratio(returns, q=5):
    """
    VR(q) = Var(q-period returns) / (q × Var(1-period returns))

    VR > 1 suggests trending (returns positively autocorrelated)
    VR < 1 suggests mean-reverting (returns negatively autocorrelated)
    VR = 1 suggests random walk
    """
    # q-period returns
    returns_q = np.array([
        np.sum(returns[i:i+q]) for i in range(len(returns)-q+1)
    ])

    var_1 = np.var(returns)
    var_q = np.var(returns_q)

    return var_q / (q * var_1)
```

### Regime-Dependent Parameters

```
TRENDING regime:   → higher momentum weight, shorter lookback
MEAN_REVERTING:    → higher mean reversion weight, wider threshold
VOLATILE:          → reduce size, wider stops, wider threshold
NORMAL:            → balanced weights, moderate parameters
```

---

## Order Flow Analysis

**All market makers** face the **adverse selection problem**: traders with better information will pick them off.

### Order Flow Imbalance

```python
# Track buy vs sell aggressor volume
buy_volume = sum(trade.qty for trade in trades if trade.aggressor == BUY)
sell_volume = sum(trade.qty for trade in trades if trade.aggressor == SELL)

imbalance = (buy_volume - sell_volume) / (buy_volume + sell_volume)

if imbalance > threshold:
    # Heavy buying → informed traders think price should be higher
    # Widen ask, tighten bid (lean with the flow)
    skew = +imbalance * skew_sensitivity
```

### Trade Size Analysis

```python
# Large trades often indicate informed trading
avg_size = np.mean(trade_sizes)
large_trade_threshold = avg_size * 3

large_buy_volume = sum(t.qty for t in trades if t.qty > large_trade_threshold and t.side == BUY)
large_sell_volume = sum(t.qty for t in trades if t.qty > large_trade_threshold and t.side == SELL)

# If large trades are directional, lean away
if large_buy_volume > large_sell_volume * 2:  # 2x imbalance threshold
    widen_ask()  # Protect against informed buyers
```

### Toxicity Metrics (VPIN)

Volume-Synchronized Probability of Informed Trading:

```python
# Bucket trades by volume, not time
bucket_size = ADV / n_buckets

for each bucket:
    buy_volume = classify_as_buy(trades_in_bucket)
    sell_volume = classify_as_sell(trades_in_bucket)
    imbalance[bucket] = abs(buy_volume - sell_volume) / bucket_size

VPIN = np.mean(imbalance[-n_buckets:])

if VPIN > threshold:
    widen_spreads()  # High toxicity environment
```

### Combining A-S/GLFT with Order Flow

```python
class AdaptiveMMStrategy:
    def compute_quotes(self, state, inventory, order_flow_stats):
        # Start with A-S/GLFT optimal spread
        base_spread = avellaneda_stoikov_spread(gamma_inv, sigma, tau, k)

        # Adjust reference price based on order flow
        flow_adjusted_reference = mid_price + order_imbalance * flow_sensitivity

        # Widen spread when detecting informed flow
        toxicity_adjustment = VPIN * toxicity_sensitivity

        # Inventory skew (from A-S formula)
        inventory_skew = gamma_inv * inventory * sigma**2 * tau

        half_spread = base_spread + toxicity_adjustment
        skew = inventory_skew

        return QuotingSignal(half_spread, skew, reference=flow_adjusted_reference)
```

---

## Connecting Signal to Execution to Risk

### The Complete Flow

```
Signal Generation          Execution Planning         Risk Management
      │                          │                         │
      ▼                          ▼                         ▼
  Momentum/MR signal    →   Almgren-Chriss      →    Position limits
  (direction + strength)     (optimal trajectory)      (real-time checks)
      │                          │                         │
      ▼                          ▼                         ▼
  Signal strength        →   Urgency (λ_risk)    →    Stop losses
  determines size             determines speed          (tail risk)
```

### How Components Connect

| Signal Output | Execution Input | Risk Check |
|---------------|-----------------|------------|
| Direction (buy/sell) | Sign of X | Position limit check |
| Strength (0-1) | Size scaling | Exposure limit check |
| Confidence | λ_risk adjustment | VaR impact |
| Regime | Execution urgency | Volatility adjustment |

### Worked Example

**Scenario**: Momentum percentile = 0.92 (strong uptrend)

1. **Signal Generation**:
   - Percentile rank: 0.92 (top 8% of moves)
   - Direction: BUY
   - Strength: High confidence

2. **Size Decision**:
   - Strong signal → larger position
   - Target: 5000 shares (50% of max allowed)

3. **Execution Planning**:
   - Horizon: 30 minutes
   - λ_risk = 0.001 (moderate urgency given strong signal)
   - κ = √(λ_risk·σ²/η) → front-loaded trajectory
   - Result: Execute 60% in first 10 minutes

4. **Risk Management**:
   - Max position check: 5000 < 10000 limit → OK
   - Intraday VaR impact: Within limits → OK
   - Stop loss: Set at 2% below VWAP entry
   - Trailing stop: Activate after 1% profit

### Decision Matrix

| Condition | Signal Action | Execution Style | Risk Action |
|-----------|---------------|-----------------|-------------|
| Strong momentum + trending | Full size | Aggressive (high λ_risk) | Normal stops |
| Weak momentum + volatile | Reduce size | Patient (low λ_risk) | Tight stops |
| Mean reversion + extreme | Full size | Patient | Wide stops |
| Regime unclear | Half size | TWAP | Very tight stops |

---

## Calibration Guidance

### Conceptual Framework

Calibration means estimating parameters from data. This is a conceptual guide; actual calibration requires historical data and statistical methods.

### A-S / GLFT Parameters

| Parameter | Estimation Approach |
|-----------|---------------------|
| **γ_inv** | Target inventory half-life: γ_inv ≈ ln(2) / (desired_halflife × σ²) |
| **k** | Order arrival rate: Count fills per unit time at each price level |
| **σ** | Realized volatility from recent returns (use appropriate window) |

**γ_inv Calibration Intuition:**
- Higher γ_inv = faster inventory mean-reversion = wider spreads
- Start with γ_inv = 0.1, adjust based on inventory dynamics
- If inventory swings too much, increase γ_inv
- If spreads are too wide (no fills), decrease γ_inv

### Almgren-Chriss Parameters

| Parameter | Estimation Approach |
|-----------|---------------------|
| **η** (temp impact) | Regression of short-term price change on trade size |
| **γ_perm** (perm impact) | Regression of permanent price change on cumulative volume |
| **λ_risk** | Set based on risk tolerance; higher = faster execution |

**Impact Estimation (Conceptual):**
```
1. Collect historical executions
2. Measure price at trade start (arrival price)
3. Measure price during execution (temporary)
4. Measure price after execution (permanent)
5. Regress price changes on trade sizes
```

### Regime Detection Thresholds

| Metric | Trending | Normal | Mean-Reverting |
|--------|----------|--------|----------------|
| Variance Ratio | VR > 1.2 | 0.8-1.2 | VR < 0.8 |
| Hurst Exponent | H > 0.55 | 0.45-0.55 | H < 0.45 |
| Autocorrelation | AC > 0.1 | -0.1 to 0.1 | AC < -0.1 |

**Note**: These thresholds are starting points. Optimal values depend on your specific market and timeframe.

---

## See Also

- [Glossary](trading_glossary.md) - Symbol definitions
- [Microstructure](trading_microstructure.md) - Vol clustering, price clustering
- [Risk](trading_risk.md) - VaR, CVaR, risk metrics
- [Philosophy](trading_philosophy.md) - Edge vs risk premium
