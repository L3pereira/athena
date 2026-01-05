# Market Microstructure Patterns

Market microstructure reveals patterns that sophisticated traders exploit. Understanding these patterns is essential for both market makers and directional traders.

See [Glossary](trading_glossary.md) for symbol definitions.

---

## Table of Contents

1. [Volatility Clustering](#volatility-clustering)
2. [Order Price Clustering](#order-price-clustering)
3. [Queue Position and Priority](#queue-position-and-priority)
4. [Latency Arbitrage](#latency-arbitrage)
5. [Market Making Queue Dynamics](#market-making-queue-dynamics)
6. [Practical Implications](#practical-implications)

---

## Volatility Clustering

**Definition**: High-volatility periods tend to be followed by high-volatility periods, and low-volatility periods tend to be followed by low-volatility periods.

### GARCH Model

```
σ²_t = ω + α·ε²_{t-1} + β·σ²_{t-1}

Where:
- σ²_t = conditional variance at time t
- ε²_{t-1} = squared shock from previous period
- α = shock sensitivity (how much recent news affects vol)
- β = persistence (how long vol regimes last)
- α + β close to 1 = high persistence
```

**Typical GARCH(1,1) Values:**
| Parameter | Typical Range | Interpretation |
|-----------|---------------|----------------|
| ω | 0.0001 | Long-term variance floor |
| α | 0.05-0.15 | News impact |
| β | 0.80-0.90 | Persistence |
| α + β | 0.90-0.99 | Total persistence |

### Key Properties

| Property | Implication |
|----------|-------------|
| **Persistence** | Vol regimes last multiple periods |
| **Asymmetry** | Bad news increases vol more than good news decreases it |
| **Mean reversion** | Extreme vol eventually normalizes |
| **Clustering** | Large moves cluster together |

### How HFT Exploits Volatility Clustering

#### 1. Spread Adjustment

```python
# Widen spreads when vol is spiking
if realized_vol > predicted_vol * 1.5:
    spread_multiplier = 1.5  # Wider spreads
elif realized_vol < predicted_vol * 0.5:
    spread_multiplier = 0.8  # Tighter spreads (more aggressive)
```

#### 2. Position Sizing

```python
# Reduce size in high vol regimes
vol_regime = "HIGH" if vol > vol_threshold else "NORMAL"
if vol_regime == "HIGH":
    max_position = base_position * 0.5
    quote_size = base_size * 0.5
```

#### 3. Mean Reversion Timing

```python
# Vol mean reverts, so:
# - In low vol: expect vol expansion → trade smaller, wider stops
# - In high vol: expect vol contraction → larger mean reversion bets
if vol_percentile > 90:
    mean_reversion_size *= 1.5  # Vol likely to contract
```

### Volatility Regime Detection

```python
def detect_vol_regime(returns, short_window=20, long_window=100):
    """Detect if we're in high/low volatility regime."""
    short_vol = np.std(returns[-short_window:])
    long_vol = np.std(returns[-long_window:])

    vol_ratio = short_vol / long_vol

    if vol_ratio > 1.5:
        return "HIGH_VOL"  # Vol expanding
    elif vol_ratio < 0.7:
        return "LOW_VOL"   # Vol contracting
    else:
        return "NORMAL_VOL"
```

### GARCH Implementation for ABM

```python
class GARCHVolatility:
    """GARCH(1,1) volatility model."""

    def __init__(self, omega=0.0001, alpha=0.1, beta=0.85):
        self.omega = omega
        self.alpha = alpha
        self.beta = beta
        # Initialize at unconditional variance
        self.current_var = omega / (1 - alpha - beta)

    def update(self, return_t):
        """Update variance given new return observation."""
        self.current_var = (
            self.omega +
            self.alpha * return_t**2 +
            self.beta * self.current_var
        )
        return np.sqrt(self.current_var)

    @property
    def half_life(self):
        """Half-life of volatility shocks."""
        return np.log(2) / np.log(1 / (self.alpha + self.beta))
```

---

## Order Price Clustering

**Definition**: Orders cluster at "round" or psychologically significant price levels (whole numbers, half-points, etc.).

### Empirical Patterns

```
Price endings distribution (typical):
  .00 → 25% of orders (round numbers)
  .50 → 15% of orders (half-points)
  .25, .75 → 10% each
  Other → 40% remaining

Stop-loss clustering:
  Below recent lows (support levels)
  At round numbers (100.00, 50.00)
  At moving average levels
  At fibonacci retracements
```

### Why Clustering Occurs

| Reason | Example |
|--------|---------|
| **Cognitive ease** | "$100" easier to think about than "$99.73" |
| **Technical analysis** | Everyone uses same support/resistance |
| **Algorithm design** | Many algos use round price targets |
| **Stop placement** | Retail stops at obvious levels |

### How HFT Exploits Price Clustering

#### 1. Stop Hunting / Stop Running

```python
class StopHunter:
    """
    Detect stop clusters and position for the cascade.

    When stops trigger, they become market orders →
    price moves further → more stops trigger → cascade
    """

    def detect_stop_cluster(self, orderbook, recent_low, recent_high):
        # Stops likely just below recent lows (for longs)
        # and just above recent highs (for shorts)

        long_stop_zone = (recent_low * 0.995, recent_low)
        short_stop_zone = (recent_high, recent_high * 1.005)

        return long_stop_zone, short_stop_zone

    def position_for_cascade(self, price, stop_zone, direction):
        """
        If price approaching stop zone:
        1. Position in direction of expected cascade
        2. Profit from the acceleration
        """
        if direction == "DOWN" and price < stop_zone[1]:
            # Stops will trigger selling → go short ahead
            return "SHORT"
        elif direction == "UP" and price > stop_zone[0]:
            # Stops will trigger buying → go long ahead
            return "LONG"
        return None
```

#### 2. Round Number Exploitation

```python
def round_number_strategy(price, orderbook, tick_size):
    """
    Exploit clustering at round numbers.

    Price behavior at round numbers:
    - Often acts as support/resistance
    - Large orders cluster there
    - Breakouts through round numbers tend to accelerate
    """

    nearest_round = round(price, -1)  # Nearest $10
    distance_to_round = abs(price - nearest_round)

    if distance_to_round < tick_size * 5:
        # Near round number - expect increased activity

        # Check book imbalance at this level
        bids_at_round = orderbook.bids_at(nearest_round)
        asks_at_round = orderbook.asks_at(nearest_round)

        if bids_at_round > asks_at_round * 2:
            # Strong support - fade breaks below
            return "FADE_BREAKS_DOWN"
        elif asks_at_round > bids_at_round * 2:
            # Strong resistance - fade breaks above
            return "FADE_BREAKS_UP"
        else:
            # Balanced - look for breakout
            return "TRADE_BREAKOUT"

    return None
```

#### 3. Liquidity Clustering Exploitation

```python
def exploit_liquidity_clusters(orderbook, current_mid):
    """
    Large orders often sit at round numbers.
    This creates predictable price dynamics.
    """

    # Find price levels with abnormally high liquidity
    liquidity_by_level = orderbook.get_depth_profile()
    mean_liquidity = np.mean(list(liquidity_by_level.values()))

    clusters = {
        price: qty for price, qty in liquidity_by_level.items()
        if qty > mean_liquidity * 3
    }

    support_levels = []
    resistance_levels = []

    # These clusters act as magnets and barriers
    for price, qty in clusters.items():
        if price > current_mid:
            # Resistance - price may struggle to break through
            # But if it does break, expect acceleration (stops above)
            resistance_levels.append(price)
        else:
            # Support - price may bounce
            support_levels.append(price)

    return support_levels, resistance_levels
```

### Modeling Price Clustering in ABM

```python
class ClusteredOrderGenerator:
    """Generate orders with realistic price clustering."""

    def generate_price(self, fair_value, tick_size, round_prob=0.30):
        """
        Generate order price with clustering behavior.

        Args:
            fair_value: Current fair value estimate
            tick_size: Minimum price increment
            round_prob: Probability of choosing a round number
        """
        if random.random() < round_prob:
            return round(fair_value, -1)  # Round to nearest 10
        else:
            return round(fair_value / tick_size) * tick_size
```

---

## Queue Position and Priority

**Definition**: In price-time priority markets, being first in queue at a price level is valuable.

### Queue Priority Value

```
Expected fill value at position i in queue:
  E[fill] = P(price reaches level) × P(queue position fills | price reaches)

Queue position matters because:
  - First in queue fills first
  - Later positions may not fill even if price touches
  - Queue position = "free option" to trade at that price
```

### HFT Queue Strategies

#### 1. Queue Jumping (Penny Jumping)

```python
def queue_jump_opportunity(orderbook, our_position, tick_size, threshold=1000):
    """
    If someone has a large order in queue ahead of us,
    consider jumping ahead by 1 tick.

    Tradeoff:
    - Pay 1 tick to jump queue
    - Get filled sooner
    - Worth it if queue is long and fills are valuable
    """

    best_bid = orderbook.best_bid
    qty_ahead_of_us = orderbook.qty_ahead(our_position)

    if qty_ahead_of_us > threshold:
        # Long queue - consider jumping
        cost_to_jump = tick_size  # We bid 1 tick higher
        benefit_of_earlier_fill = estimate_queue_wait_cost(qty_ahead_of_us)

        if benefit_of_earlier_fill > cost_to_jump:
            return best_bid + tick_size  # Jump the queue

    return best_bid  # Stay in queue
```

#### 2. Queue Position Management

```python
class QueueManager:
    """
    Manage queue positions across multiple price levels.

    Strategy:
    - Maintain queue position at multiple levels
    - Cancel and replace to stay near front
    - Balance queue priority vs price level
    """

    def optimize_queue_positions(self, orderbook, target_inventory, tick_size):
        positions = []

        for level in range(1, 5):  # Top 4 price levels
            price = orderbook.best_bid - (level - 1) * tick_size

            # Queue position value decreases with distance from touch
            queue_value = self.estimate_queue_value(price, level, orderbook)

            # Optimal size at each level
            size = self.calculate_optimal_size(queue_value, target_inventory)
            positions.append((price, size))

        return positions

    def estimate_queue_value(self, price, level, orderbook):
        """Value of queue position at given price level."""
        # Further from touch = lower fill probability = lower value
        distance_penalty = 0.8 ** level
        queue_depth = orderbook.qty_at(price)
        return distance_penalty / (1 + queue_depth / 1000)

    def calculate_optimal_size(self, queue_value, target_inventory):
        """Size to quote given queue value."""
        return int(target_inventory * queue_value)
```

---

## Latency Arbitrage

**Definition**: Exploiting the speed difference between venues or data feeds.

### How It Works

```
Venue A price updates → 1ms later → Venue B price updates

Fast trader:
1. Sees price change on Venue A
2. Trades on Venue B before B's price updates
3. Captures the stale price

Example:
- Stock trades at $100.00 on both venues
- Large buy on Venue A pushes price to $100.05
- Fast trader buys on Venue B at $100.00 (stale)
- Sells on Venue A at $100.05
- Profit: $0.05 per share, risk-free
```

### Speed Requirements

```
Typical latencies:
- Colocation: ~1-10 microseconds
- Direct market access: ~100 microseconds
- Retail broker: ~10-100 milliseconds

Latency arb window: ~1-10 milliseconds
→ Only accessible to colocated HFT
```

### Cross-Venue Arbitrage

```python
class LatencyArbitrageur:
    """
    Cross-venue latency arbitrage.

    Note: Requires sub-millisecond execution capability.
    Not feasible for most market participants.
    """

    def __init__(self, venues, latencies):
        self.venues = venues
        self.latencies = latencies  # Dict of venue -> latency in ms

    def find_opportunity(self):
        """
        Find stale prices across venues.
        """
        prices = {v: v.get_mid_price() for v in self.venues}

        # Find max price difference
        max_price = max(prices.values())
        min_price = min(prices.values())

        if max_price - min_price > self.min_spread:
            sell_venue = [v for v, p in prices.items() if p == max_price][0]
            buy_venue = [v for v, p in prices.items() if p == min_price][0]
            return buy_venue, sell_venue, max_price - min_price

        return None
```

### Why Most Can't Do This

| Requirement | Cost | Barrier |
|-------------|------|---------|
| Colocation | $10K-100K/month per venue | Capital |
| Direct feeds | $10K-50K/month | Capital |
| Custom hardware | $1M+ | Capital + expertise |
| Low-latency code | PhD-level engineers | Human capital |

---

## Market Making Queue Dynamics

**Understanding queue dynamics is critical for market makers.**

### Queue-Aware Market Making

```python
class QueueAwareMarketMaker:
    """
    Market maker that understands queue position affects expected PnL.

    Key insight:
    - Being first in queue = higher fill probability
    - But posting first = more information revealed
    - Tradeoff: queue priority vs information leakage
    """

    def decide_quote_timing(self, orderbook, volatility, vol_threshold=0.02):
        """
        When to post quotes:
        - Early: Better queue position, but reveal intentions
        - Late: Worse queue position, but see more information
        """

        if volatility > vol_threshold:
            # High vol: post late (information more valuable)
            return "POST_LATE"
        else:
            # Low vol: post early (queue position more valuable)
            return "POST_EARLY"

    def estimate_fill_probability(self, price, queue_position, orderbook):
        """
        P(fill) depends on:
        1. P(price touches our level)
        2. P(we get filled given price touches) - queue dependent
        """

        p_touch = self.estimate_touch_probability(price, orderbook)

        qty_at_level = orderbook.qty_at(price)
        if qty_at_level == 0:
            p_fill_given_touch = 1.0
        else:
            p_fill_given_touch = min(1.0, queue_position / qty_at_level)

        return p_touch * p_fill_given_touch

    def estimate_touch_probability(self, price, orderbook):
        """Estimate probability price reaches this level."""
        distance_from_mid = abs(price - orderbook.mid_price)
        # Simplified: exponential decay with distance
        return np.exp(-distance_from_mid / orderbook.volatility)
```

### Information Leakage vs Queue Priority

| Action | Queue Benefit | Information Cost |
|--------|---------------|------------------|
| Post early | First in queue | Reveal direction |
| Post late | Back of queue | See flow first |
| Large size | Fill more | Signal large interest |
| Small size | Fill less | Hide intentions |

---

## Practical Implications

### For Market Makers

| Pattern | Implication | Action |
|---------|-------------|--------|
| Vol clustering | Vol regimes persist | Adjust spreads dynamically |
| Price clustering | Liquidity at round numbers | Be aware of magnet/barrier effects |
| Queue dynamics | Position matters | Manage queue actively |

**Spread Adjustment Rule:**
```python
def adjust_spread_for_vol(base_spread, current_vol, historical_vol):
    vol_ratio = current_vol / historical_vol
    if vol_ratio > 1.5:
        return base_spread * 1.5  # Widen in high vol
    elif vol_ratio < 0.7:
        return base_spread * 0.85  # Tighten in low vol
    return base_spread
```

### For Directional Traders

| Pattern | Implication | Action |
|---------|-------------|--------|
| Vol clustering | High vol = high vol continues | Size down in vol spikes |
| Price clustering | Stops cluster at obvious levels | Place stops at non-obvious levels |
| Stop hunting | HFTs know where stops are | Avoid round number stops |

**Stop Placement Strategy:**
```python
def smart_stop_placement(entry_price, stop_distance, tick_size):
    """
    Place stops at non-obvious levels to avoid stop hunting.
    """
    naive_stop = entry_price - stop_distance

    # Avoid round numbers
    nearest_round = round(naive_stop, -1)
    if abs(naive_stop - nearest_round) < tick_size * 3:
        # Too close to round number - adjust
        naive_stop = nearest_round - tick_size * 5

    return naive_stop
```

### For ABM Simulation

Include these patterns for realistic market dynamics:

```python
# Complete microstructure model
class MicrostructureModel:
    def __init__(self):
        self.garch = GARCHVolatility(omega=0.0001, alpha=0.1, beta=0.85)
        self.clustering = ClusteredOrderGenerator()

    def generate_dynamics(self, fair_value, tick_size):
        # Update volatility with GARCH
        current_vol = self.garch.update(self.last_return)

        # Generate clustered order prices
        order_price = self.clustering.generate_price(fair_value, tick_size)

        return current_vol, order_price
```

---

## See Also

- [Glossary](trading_glossary.md) - Symbol definitions
- [Strategies](trading_strategies.md) - A-S, GLFT, Almgren-Chriss
- [Risk](trading_risk.md) - VaR, CVaR, risk metrics
- [Philosophy](trading_philosophy.md) - Edge vs risk premium
