# Risk Management and Metrics

Risk metrics and optimization objectives for trading strategies.

See [Glossary](trading_glossary.md) for symbol definitions.

---

## Table of Contents

1. [Market Impact Models](#market-impact-models)
2. [Optimization Objectives](#optimization-objectives)
3. [Risk Parameters](#risk-parameters)
4. [Volatility Types](#volatility-types)
5. [Risk Metrics](#risk-metrics)
6. [Practical Implementation](#practical-implementation)

---

## Market Impact Models

Market impact is the price change caused by trading. Institutions use these models for execution planning.

### 1. Square-Root Model (Most Common)

**Used by**: Most institutional traders, transaction cost analysis (TCA)

```
Impact = σ · √(Q/V) · η

Where:
- σ = daily volatility
- Q = order size
- V = average daily volume (ADV)
- η = impact coefficient (typically 0.1-0.5)
```

**Empirical Basis**: Extensive empirical evidence (Almgren et al., Kissell, etc.)

**Intuition**: Impact grows with square root of participation rate, not linearly.

### 2. Linear Model (Almgren-Chriss)

**Used by**: Optimal execution frameworks, risk models

```
Temporary impact: g(v) = η · v
Permanent impact: h(v) = γ_perm · v

Where:
- v = trading rate (shares/time)
- η = temporary impact coefficient
- γ_perm = permanent impact coefficient
```

**Temporary vs Permanent:**
- **Temporary**: Price bounce-back after trade (market resilience)
- **Permanent**: Information content of trade (price discovery)

### 3. Kyle's Lambda

**From**: Kyle (1985) - "Continuous Auctions and Insider Trading"

```
ΔP = λ_kyle · (signed_order_flow)

Where:
- λ_kyle = Kyle's lambda (price impact per unit of order flow)
- λ_kyle = σ_v / (2·σ_u) in equilibrium
```

**Interpretation**: λ_kyle measures market depth. Higher λ_kyle = less liquid market.

**Note**: λ_kyle is different from λ_risk (risk aversion parameter). See [Glossary](trading_glossary.md).

### 4. Power-Law Model

**Used by**: High-frequency research, market microstructure

```
Impact = η · (Q/V)^δ

Where:
- δ = power law exponent (typically 0.4-0.6)
- Reduces to square-root when δ = 0.5
```

**Advantages**: Flexible, fits various market conditions.

### 5. Transient Impact Model (Obizhaeva-Wang)

**From**: Obizhaeva & Wang (2013)

```
Impact at time t:
I(t) = ∫₀ᵗ G(t-s) · dQ(s)

Where:
- G(τ) = decay kernel (how impact fades)
- G(τ) = e^(-ρτ) for exponential decay
- ρ = resilience parameter
```

**Key Insight**: Impact is not instantaneous; it builds up and decays.

### Summary of Impact Models

| Model | Formula | Best For |
|-------|---------|----------|
| Square-Root | σ·√(Q/V)·η | TCA, pre-trade estimates |
| Linear | η·v + γ_perm·v | Almgren-Chriss optimization |
| Kyle Lambda | λ_kyle·order_flow | Market depth analysis |
| Power-Law | η·(Q/V)^δ | Flexible fitting |
| Transient | ∫G(t-s)dQ(s) | Intraday execution |

### Typical Coefficient Values

| Market | η (temp) | γ_perm | λ_kyle |
|--------|----------|--------|--------|
| Large-cap equity | 0.1-0.3 | 0.05-0.1 | 10⁻⁵ - 10⁻⁴ |
| Mid-cap equity | 0.3-0.5 | 0.1-0.2 | 10⁻⁴ - 10⁻³ |
| Small-cap equity | 0.5-1.0 | 0.2-0.4 | 10⁻³ - 10⁻² |
| Crypto | 0.5-2.0 | 0.3-0.8 | Variable |

---

## Optimization Objectives

Different agents have different objective functions.

### For Funds (Directional Traders)

**Primary Objective: Maximize Risk-Adjusted PnL**

```
max E[PnL] - λ_risk · Var[PnL]
```

This is equivalent to maximizing **Certainty Equivalent**:

```
CE = E[PnL] - (λ_risk/2) · Var[PnL]
```

**Expanded:**
```
E[PnL] = E[α · position] - E[execution_cost] - E[fees]

Where:
- α = alpha (expected return from signal)
- execution_cost = market impact + spread crossing
- fees = exchange fees

Var[PnL] = position² · σ² + execution_variance
```

**The Tradeoff:**
- Trade fast → capture alpha before decay, but high impact cost
- Trade slow → low impact, but alpha decays, more timing risk

### For Execution Algorithms (Almgren-Chriss)

**Primary Objective: Minimize Implementation Shortfall**

```
min E[Cost] + λ_risk · Var[Cost]

Where:
Cost = Σ (execution_price - arrival_price) · quantity
```

**Components:**
```
E[Cost] = permanent_impact + temporary_impact + fees
        = γ_perm·X²/2T + η·X²/T + spread·X

Var[Cost] = σ² · ∫₀ᵀ x(t)² dt
          = timing risk from price volatility during execution
```

### For Market Makers

**Primary Objective: Maximize Spread Capture Net of Adverse Selection**

```
max E[spread_earned] - E[adverse_selection_loss] - λ_risk · Var[inventory_PnL]
```

**Components:**
```
E[spread_earned] = (ask - bid) · E[fills] · fill_rate
E[adverse_selection] = E[|price_move_against_MM|] · E[inventory]
Var[inventory_PnL] = inventory² · σ²
```

**The Tradeoff:**
- Tight spread → more fills, more rebates, but more adverse selection
- Wide spread → fewer fills, less adverse selection, but less revenue

### Unified Framework

All objectives can be written as:

```
max  E[Revenue] - E[Cost] - λ_risk · Risk
```

| Agent | Revenue | Cost | Risk |
|-------|---------|------|------|
| Fund | Alpha capture | Impact + fees | Var[PnL] |
| Execution | - | Impact + fees | Var[shortfall] |
| MM | Spread + rebates | Adverse selection | Var[inventory] |

---

## Risk Parameters

### Directional Traders (Funds, Prop Traders)

| Parameter | Symbol | Description | Optimization Target |
|-----------|--------|-------------|---------------------|
| **Risk aversion** | λ_risk | Penalty on variance | Calibrate to max Sharpe |
| **Urgency** | u | Trade rate / total size | Balance alpha decay vs impact |
| **Aggression** | a | Market order % | Balance fill certainty vs cost |
| **Position scale** | s | Max position as % of limit | Balance profit vs drawdown |
| **Threshold** | θ | Min signal to trade | Filter noise vs miss opportunities |

**Regime-Dependent Adjustment:**
```
High volatility:  → higher threshold (more noise)
Mean-reverting:   → lower urgency (price comes to you)
Trending:         → higher urgency (capture momentum)
```

### Market Makers (All Types)

All MMs use A-S/GLFT framework. The parameters are:

| Parameter | Symbol | Description | Optimization Target |
|-----------|--------|-------------|---------------------|
| **Risk aversion** | γ_inv | Inventory risk penalty | Calibrate to target inventory |
| **Liquidity param** | k | Order arrival intensity | Estimate from order flow |
| **Min spread** | δ_min | Floor on spread | Cover adverse selection |
| **Skew sensitivity** | κ | Inventory skew rate | Mean-revert inventory faster/slower |
| **Max inventory** | q_max | Position limit | Bound worst-case loss |
| **Toxicity threshold** | VPIN_thresh | When to widen | Minimize adverse selection |
| **Flow sensitivity** | φ | How much to lean with flow | Balance following vs fading |

**What differs is the quality of reference price estimation:**
```
Better reference price estimate → tighter spreads → more volume → more profit
Worse reference price estimate → wider spreads → less volume → lower profit
```

### Technical Traders (Momentum / Mean Reversion)

| Parameter | Symbol | Description | Optimization Target |
|-----------|--------|-------------|---------------------|
| **Lookback** | n | Window for signal calculation | Match market time scale |
| **Threshold** | z | Signal strength to trade | Filter noise vs opportunity |
| **Position size** | s | Trade size | Balance profit vs impact |
| **Stop loss** | SL | Max loss before exit | Bound worst case |
| **Regime weights** | w_mom, w_mr | Mix of momentum vs MR | Adapt to detected regime |

**Regime-Dependent Optimization:**
```
TRENDING regime:   → higher w_mom, lower w_mr, shorter lookback
MEAN_REVERTING:    → lower w_mom, higher w_mr, wider threshold
VOLATILE regime:   → reduce size, wider stops, wider threshold
NORMAL regime:     → balanced weights, moderate parameters
```

---

## Volatility Types

### Realized Volatility (Historical)

**What it is**: Backward-looking volatility computed from actual price movements.

```python
def realized_vol(returns, window=20):
    """Annualized realized volatility from returns."""
    return np.std(returns[-window:]) * np.sqrt(252)

def realized_vol_hf(prices_5min):
    """Realized vol from intraday data - more accurate."""
    returns = np.diff(np.log(prices_5min))
    return np.sqrt(np.sum(returns**2)) * np.sqrt(252)
```

**Used by**: All traders for spread sizing, position sizing, risk limits.

### Implied Volatility (Forward-Looking)

**What it is**: Market's expectation of future volatility, derived from options prices.

```
Given: Option price, Strike, Expiry, Spot, Rate
Solve: Black-Scholes formula for σ (implied vol)

C = S·N(d₁) - K·e^(-rT)·N(d₂)

where d₁ = [ln(S/K) + (r + σ²/2)T] / (σ√T)

Invert numerically to find σ that matches observed price
```

### Key Implied Volatility Concepts

| Concept | Description | Trading Signal |
|---------|-------------|----------------|
| **IV vs RV** | Implied minus Realized | IV > RV → options "expensive" (sell vol) |
| **IV Term Structure** | IV across expirations | Contango (normal) vs Backwardation (fear) |
| **IV Skew** | IV across strikes | High put skew → crash fear priced in |
| **IV Smile** | U-shaped IV curve | Both tails have higher IV than ATM |
| **VIX** | S&P 500 30-day IV | "Fear gauge" - regime indicator |

### IV Term Structure

```
              IV
               │
    Backwardation    Contango (normal)
    (fear/stress)    (calm markets)
               │    ╱
               │   ╱
               │  ╱
               │ ╱
               │╱_______________
               └────────────────► Expiration
              Near            Far
```

### IV Skew (Equities)

```
              IV
               │
               │    ╲
               │     ╲      (put skew)
               │      ╲____╱
               │
               └────────────────► Strike
              OTM Put    ATM    OTM Call
```

### Who Uses IV

| Participant | IV Usage |
|-------------|----------|
| **Options MM** | Core input for pricing, hedging vega |
| **Vol Traders** | Trade IV vs RV spread (variance swaps) |
| **Directional** | VIX as regime indicator |
| **Risk Managers** | Stress testing, scenario analysis |

### IV Proxy for ABM (No Options)

```python
def order_flow_fear_index(orderbook, trades, avg_spread, std_spread):
    """
    Proxy for IV using microstructure signals.
    High values = stressed market (like high VIX).
    """
    # Spread widening
    current_spread = orderbook.ask - orderbook.bid
    spread_z = (current_spread - avg_spread) / std_spread

    # Order imbalance
    buy_volume = sum(t.qty for t in trades if t.side == "BUY")
    sell_volume = sum(t.qty for t in trades if t.side == "SELL")
    total_volume = buy_volume + sell_volume
    imbalance = abs(buy_volume - sell_volume) / total_volume if total_volume > 0 else 0

    # Large trade frequency
    large_threshold = np.mean([t.qty for t in trades]) * 3
    large_trades = sum(1 for t in trades if t.qty > large_threshold)
    large_trade_ratio = large_trades / len(trades) if trades else 0

    # Combine into fear index
    fear_index = 0.4 * spread_z + 0.3 * imbalance + 0.3 * large_trade_ratio
    return fear_index
```

---

## Risk Metrics

### Value at Risk (VaR)

**What it is**: Maximum loss at a given confidence level over a time horizon.

```
VaR_α = quantile of loss distribution at α

Example: 95% 1-day VaR = $1M means:
  "There's a 5% chance of losing more than $1M tomorrow"
```

**VaR Methods:**

| Method | Description | Pros/Cons |
|--------|-------------|-----------|
| **Historical** | Use actual return distribution | Simple; assumes past = future |
| **Parametric** | Assume normal distribution | Fast; misses fat tails |
| **Monte Carlo** | Simulate many scenarios | Flexible; computationally heavy |

```python
def historical_var(returns, confidence=0.95):
    """Historical VaR at given confidence level."""
    return -np.percentile(returns, (1 - confidence) * 100)

def parametric_var(position, volatility, confidence=0.95, horizon_days=1):
    """Parametric (normal) VaR."""
    from scipy.stats import norm
    z = norm.ppf(confidence)
    return position * volatility * z * np.sqrt(horizon_days)
```

**VaR Limitations:**
- Doesn't tell you HOW BAD losses can get beyond VaR
- Not subadditive (portfolio VaR can exceed sum of parts)
- Can be gamed (hide risk just beyond threshold)

### Conditional VaR (CVaR) / Expected Shortfall

**What it is**: Average loss in the worst cases (beyond VaR).

```
CVaR_α = E[Loss | Loss > VaR_α]

Example: 95% CVaR = $1.5M means:
  "In the worst 5% of cases, average loss is $1.5M"
```

**Why CVaR > VaR:**

| Property | VaR | CVaR |
|----------|-----|------|
| Tail risk | Ignores severity | Captures average tail loss |
| Subadditivity | No (not coherent) | Yes (coherent risk measure) |
| Optimization | Non-convex | Convex (easier to optimize) |
| Gaming | Can hide risk beyond threshold | Harder to game |
| Regulation | Basel II/III | Increasingly required |

```python
def cvar(returns, confidence=0.95):
    """Conditional VaR (Expected Shortfall)."""
    var = historical_var(returns, confidence)
    # Average of losses worse than VaR
    tail_losses = returns[returns < -var]
    return -tail_losses.mean() if len(tail_losses) > 0 else var

def cvar_parametric(position, volatility, confidence=0.95):
    """Parametric CVaR assuming normal distribution."""
    from scipy.stats import norm
    alpha = 1 - confidence
    z = norm.ppf(alpha)
    # Expected shortfall for normal distribution
    es_factor = norm.pdf(z) / alpha
    return position * volatility * es_factor
```

**CVaR in Optimization:**

```python
# Standard Almgren-Chriss (variance penalty)
def almgren_chriss_objective(trajectory, params):
    expected_cost = compute_expected_cost(trajectory, params)
    variance_cost = compute_variance(trajectory, params)
    return expected_cost + params.lambda_risk * variance_cost

# CVaR-based execution (better for fat tails)
def cvar_execution_objective(trajectory, params, scenarios):
    costs = [compute_cost(trajectory, scenario) for scenario in scenarios]
    expected_cost = np.mean(costs)
    cvar_cost = cvar(np.array(costs), confidence=0.95)
    return expected_cost + params.lambda_risk * cvar_cost
```

### Maximum Drawdown

**What it is**: Largest peak-to-trough decline.

```python
def max_drawdown(equity_curve):
    """Maximum drawdown from equity curve."""
    peak = np.maximum.accumulate(equity_curve)
    drawdown = (equity_curve - peak) / peak
    return drawdown.min()

def calmar_ratio(returns, equity_curve):
    """Return / Max Drawdown - risk-adjusted performance."""
    annual_return = np.mean(returns) * 252
    mdd = abs(max_drawdown(equity_curve))
    return annual_return / mdd if mdd > 0 else np.inf
```

**Used by**: Fund managers, performance evaluation, strategy selection.

### Sharpe Ratio

**What it is**: Risk-adjusted return measuring excess return per unit of volatility.

```
Sharpe = (R - Rf) / σ

Where:
- R = strategy return
- Rf = risk-free rate
- σ = standard deviation of returns
```

**HFT Note**: For high-frequency strategies, the risk-free rate is effectively zero over microsecond/second holding periods. HFTs typically use:

```
Sharpe_HFT = R / σ   (no risk-free subtraction)
```

```python
def sharpe_ratio(returns, risk_free_rate=0.0, annualize=True):
    """
    Sharpe ratio - risk-adjusted return.

    For HFT: set risk_free_rate=0 (holding period too short for Rf to matter)
    """
    excess_returns = returns - risk_free_rate / 252  # Daily Rf adjustment
    if annualize:
        return np.mean(excess_returns) / np.std(excess_returns) * np.sqrt(252)
    return np.mean(excess_returns) / np.std(excess_returns)

def sharpe_hft(returns):
    """HFT Sharpe - no risk-free rate, can use any time scale."""
    return np.mean(returns) / np.std(returns)
```

**Interpretation**:
| Sharpe | Quality |
|--------|---------|
| < 0 | Losing money |
| 0 - 1 | Below average |
| 1 - 2 | Good |
| 2 - 3 | Very good |
| > 3 | Excellent (or overfitting / too short sample) |

### Sortino Ratio

**What it is**: Like Sharpe, but only penalizes downside volatility. Better for asymmetric return distributions.

```
Sortino = (R - Rf) / σ_downside

Where:
- σ_downside = std of negative returns only
```

**Why Sortino > Sharpe for some strategies**:
- Sharpe penalizes upside volatility equally with downside
- A strategy with occasional large gains gets punished by Sharpe
- Sortino only cares about losses

```python
def sortino_ratio(returns, risk_free_rate=0.0, annualize=True):
    """
    Sortino ratio - only penalizes downside deviation.
    Better for strategies with positive skew.
    """
    excess_returns = returns - risk_free_rate / 252

    # Only negative returns for downside deviation
    downside_returns = excess_returns[excess_returns < 0]
    downside_std = np.std(downside_returns) if len(downside_returns) > 0 else 1e-10

    if annualize:
        return np.mean(excess_returns) / downside_std * np.sqrt(252)
    return np.mean(excess_returns) / downside_std
```

**When to use which**:
| Metric | Best For |
|--------|----------|
| **Sharpe** | Symmetric return distributions, comparing strategies |
| **Sortino** | Strategies with positive skew, options-like payoffs |
| **Calmar** | Focus on worst-case drawdown risk |

### Risk Metrics by Time Horizon

**All participants use all risk metrics** - they differ in the time horizon at which they measure them:

| Metric | HFT Horizon | MM Horizon | Prop Trader | Fund/Institution |
|--------|-------------|------------|-------------|------------------|
| **Position limits** | Per-tick/ms | Per-second | Per-minute | Daily |
| **Drawdown limits** | Per-second/minute | Hourly | Intraday | Weekly/Monthly |
| **VaR/CVaR** | Rolling seconds | Rolling minutes | Daily | Daily/Weekly |
| **P&L limits** | Per-minute | Per-hour | Daily | Monthly |
| **Stress testing** | Flash crash (ms) | Volatility spikes | Regime change | Tail scenarios |

**Key point**: HFTs compute risk metrics in real-time at sub-second granularity. A rolling 10-second CVaR that exceeds threshold can trigger automatic strategy shutdown in microseconds. The same math applies whether you're measuring over 10 seconds or 10 days - just different input windows.

### Connecting Risk Metrics to Strategies

| Participant | Strategy | Primary Risk Metric | Secondary |
|-------------|----------|---------------------|-----------|
| **Market Makers** | A-S/GLFT | Inventory limits, γ_inv | Intraday VaR, Sharpe |
| **Directional (Technical)** | Momentum | Max Drawdown, CVaR | Sortino, Position limits |
| **Directional (Technical)** | Mean Reversion | CVaR (tail risk) | Time stops, Sortino |
| **Directional (Quant)** | Stat Arb | Correlation breakdown VaR | Gross exposure, Sharpe |
| **All Large Traders** | Execution (A-C) | Implementation shortfall | CVaR of slippage |

**Note**: Technical traders (momentum, mean reversion) and quant funds (stat arb) are all **directional traders** - they take positions based on price forecasts. They differ in signal source (price patterns vs cross-asset relationships) but share similar risk management needs.

---

## Practical Implementation

### Multi-Horizon Risk Manager

```python
class RiskManager:
    """
    Multi-horizon risk management.

    Microstructure (real-time):
      - Position limits
      - Inventory skew

    Intraday:
      - P&L limits
      - Drawdown limits

    Daily:
      - VaR/CVaR reporting
      - Exposure limits
    """

    def __init__(self, config):
        # Real-time limits
        self.max_position = config.max_position
        self.max_inventory_seconds = config.max_hold_time

        # Intraday limits
        self.max_daily_loss = config.max_daily_loss
        self.max_drawdown_intraday = config.max_dd_intraday

        # Risk metrics
        self.var_confidence = 0.95
        self.cvar_confidence = 0.95

    def check_real_time(self, position, inventory_age):
        """Real-time risk checks (every tick)."""
        if abs(position) > self.max_position:
            return "REDUCE_POSITION"
        if inventory_age > self.max_inventory_seconds:
            return "FLATTEN"
        return "OK"

    def check_intraday(self, pnl, peak_pnl):
        """Intraday risk checks (periodic)."""
        if pnl < -self.max_daily_loss:
            return "STOP_TRADING"
        drawdown = (pnl - peak_pnl) / abs(peak_pnl) if peak_pnl != 0 else 0
        if drawdown < -self.max_drawdown_intraday:
            return "REDUCE_SIZE"
        return "OK"

    def compute_risk_report(self, returns):
        """End-of-day risk report."""
        equity = np.cumprod(1 + returns)
        return {
            'var_95': historical_var(returns, 0.95),
            'cvar_95': cvar(returns, 0.95),
            'volatility': np.std(returns) * np.sqrt(252),
            'max_drawdown': max_drawdown(equity),
        }
```

### Position Sizing with Risk Constraints

```python
def position_size_with_var(
    signal_strength,
    max_position,
    current_volatility,
    var_limit,
    confidence=0.95
):
    """
    Size position respecting VaR limit.

    Args:
        signal_strength: 0-1 strength of trading signal
        max_position: Maximum allowed position
        current_volatility: Current realized volatility
        var_limit: Maximum acceptable VaR
        confidence: VaR confidence level
    """
    from scipy.stats import norm
    z = norm.ppf(confidence)

    # Max position from VaR limit
    # VaR = position * vol * z
    # position = VaR / (vol * z)
    var_constrained_position = var_limit / (current_volatility * z)

    # Signal-based position
    signal_position = max_position * signal_strength

    # Take minimum of constraints
    final_position = min(signal_position, var_constrained_position, max_position)

    return final_position
```

---

## What to Actually Use

**Stop reading tables. Here's what you need:**

### If You're a Market Maker

1. **Real-time**: Position limits (hard cap), inventory age limits
2. **Per-minute**: Rolling P&L, inventory skew (γ_inv adjustment)
3. **End of day**: Sharpe ratio, max drawdown
4. **Use CVaR** not VaR (you have tail risk from adverse selection)

### If You're a Directional Trader (Momentum/MR)

1. **Real-time**: Position limits
2. **Per-trade**: Stop loss (fixed % or ATR-based)
3. **Daily**: Drawdown check, P&L limit
4. **Performance**: **Sortino** (not Sharpe) - you want upside volatility
5. **Use CVaR** for position sizing (fat tails will kill you)

### If You're Running Execution (Almgren-Chriss)

1. **Per-execution**: Implementation shortfall vs arrival price
2. **Use CVaR** for trajectory optimization (variance understates risk)

### Minimum Viable Risk System

```python
# This is all you need to start:
class MinimalRisk:
    def __init__(self, max_position, max_daily_loss, max_drawdown_pct):
        self.max_position = max_position
        self.max_daily_loss = max_daily_loss
        self.max_drawdown_pct = max_drawdown_pct

    def can_trade(self, current_position, daily_pnl, peak_pnl):
        # Hard limits - non-negotiable
        if abs(current_position) >= self.max_position:
            return False, "position_limit"
        if daily_pnl <= -self.max_daily_loss:
            return False, "daily_loss_limit"
        if peak_pnl > 0 and (daily_pnl - peak_pnl) / peak_pnl <= -self.max_drawdown_pct:
            return False, "drawdown_limit"
        return True, "ok"
```

**Start with this. Add complexity only when you understand why you need it.**

---

## See Also

- [Glossary](trading_glossary.md) - Symbol definitions
- [Strategies](trading_strategies.md) - A-S, GLFT, Almgren-Chriss
- [Microstructure](trading_microstructure.md) - Vol clustering, price clustering
- [Philosophy](trading_philosophy.md) - Edge vs risk premium
