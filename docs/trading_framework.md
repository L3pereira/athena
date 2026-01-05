# Trading Strategy Framework

A comprehensive framework for understanding and implementing trading strategies in agent-based market simulations.

---

## Documentation Index

| Document | Description | Key Topics |
|----------|-------------|------------|
| **[Glossary](trading_glossary.md)** | Symbol definitions and formulas | γ, λ, δ, θ, σ, η, τ, κ - disambiguated |
| **[Strategies](trading_strategies.md)** | Core trading algorithms | A-S, GLFT, Almgren-Chriss, Momentum, Mean Reversion |
| **[Microstructure](trading_microstructure.md)** | Market microstructure patterns | Vol clustering, price clustering, queue dynamics |
| **[Risk](trading_risk.md)** | Risk metrics and management | VaR, CVaR, impact models, optimization |
| **[Philosophy](trading_philosophy.md)** | Conceptual foundations | Edge vs Risk Premium, Reality vs Simulation |

---

## Quick Reference

### "I want to..."

| Goal | See |
|------|-----|
| Understand what symbols mean | [Glossary](trading_glossary.md) |
| Implement market making | [Strategies: A-S/GLFT](trading_strategies.md#quoting-strategies) |
| Execute large orders optimally | [Strategies: Almgren-Chriss](trading_strategies.md#execution-algorithms) |
| Build momentum/mean reversion signals | [Strategies: Signal Strategies](trading_strategies.md#signal-strategies) |
| Understand volatility clustering | [Microstructure: Vol Clustering](trading_microstructure.md#volatility-clustering) |
| Avoid stop hunting | [Microstructure: Price Clustering](trading_microstructure.md#order-price-clustering) |
| Measure risk properly | [Risk: VaR/CVaR](trading_risk.md#risk-metrics) |
| Understand why strategies work | [Philosophy: Edge vs Risk Premium](trading_philosophy.md#edge-vs-risk-premium) |

---

## Framework Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                        TRADING FRAMEWORK                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐              │
│  │   SIGNAL    │ →  │  EXECUTION  │ →  │    RISK     │              │
│  │ Generation  │    │  Algorithm  │    │ Management  │              │
│  └─────────────┘    └─────────────┘    └─────────────┘              │
│        │                  │                  │                       │
│        ▼                  ▼                  ▼                       │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐              │
│  │ Momentum    │    │ Almgren-    │    │ Position    │              │
│  │ Mean Rev    │    │ Chriss      │    │ Limits      │              │
│  │ Regime Det. │    │ TWAP/VWAP   │    │ VaR/CVaR    │              │
│  └─────────────┘    └─────────────┘    └─────────────┘              │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                    MARKET MAKING                              │    │
│  │  ┌───────────┐  ┌───────────┐  ┌───────────┐                 │    │
│  │  │   A-S     │  │   GLFT    │  │ Order Flow│                 │    │
│  │  │  Quoting  │  │  Quoting  │  │ Analysis  │                 │    │
│  │  └───────────┘  └───────────┘  └───────────┘                 │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                    MICROSTRUCTURE                             │    │
│  │  Vol Clustering │ Price Clustering │ Queue Dynamics          │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Key Principles

### 1. Everyone Uses the Same Algorithms
- A-S/GLFT used by ALL market makers
- Almgren-Chriss used by ALL institutional traders
- The difference is **input quality**, not algorithm choice

### 2. Returns = Risk Premium (Mostly)
- Known strategies work because they compensate for RISK
- True edge is rare and temporary
- See [Philosophy: Edge vs Risk Premium](trading_philosophy.md#edge-vs-risk-premium)

### 3. Markets Have Fat Tails
- Don't use z-scores (assumes normality)
- Use percentile ranks and quantile bands
- See [Strategies: Signal Strategies](trading_strategies.md#signal-strategies)

### 4. Information is a Spectrum
- Reality: Everyone estimates FV with varying quality
- ABM Simulation: Binary "sees FV" vs "doesn't" is a simplification
- See [Philosophy: Reality vs Simulation](trading_philosophy.md#reality-vs-simulation)

---

## References

1. **Almgren, R., & Chriss, N.** (2000). "Optimal execution of portfolio transactions." *Journal of Risk*
2. **Avellaneda, M., & Stoikov, S.** (2008). "High-frequency trading in a limit order book." *Quantitative Finance*
3. **Guéant, O., Lehalle, C.A., & Fernandez-Tapia, J.** (2013). "Dealing with the inventory risk." *Mathematics and Financial Economics*
4. **Kyle, A.S.** (1985). "Continuous auctions and insider trading." *Econometrica*
5. **Obizhaeva, A., & Wang, J.** (2013). "Optimal trading strategy and supply/demand dynamics." *Journal of Financial Markets*
6. **Easley, D., López de Prado, M., & O'Hara, M.** (2012). "Flow toxicity and liquidity." *Review of Financial Studies*
7. **Bollerslev, T.** (1986). "Generalized autoregressive conditional heteroskedasticity." *Journal of Econometrics*
8. **Harris, L.** (1991). "Stock price clustering and discreteness." *Review of Financial Studies*
9. **Hasbrouck, J.** (2007). *Empirical Market Microstructure*. Oxford University Press
10. **Foucault, T., Pagano, M., & Röell, A.** (2013). *Market Liquidity*. Oxford University Press
11. **Rockafellar, R.T., & Uryasev, S.** (2000). "Optimization of Conditional Value-at-Risk." *Journal of Risk*
12. **Black, F., & Scholes, M.** (1973). "The Pricing of Options." *Journal of Political Economy*
13. **Jorion, P.** (2006). *Value at Risk*. McGraw-Hill
14. **Gatheral, J.** (2006). *The Volatility Surface*. Wiley
