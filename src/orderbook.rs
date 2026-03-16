use std::collections::BTreeMap;
use std::fmt;

use crate::models::OrderBook;
use crate::types::Side;

/// A float wrapper that implements `Ord` for use in BTreeMap keys.
/// Panics on NaN — safe for API-sourced price strings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderedFloat(pub f64);

impl Eq for OrderedFloat {}

impl PartialOrd for OrderedFloat {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedFloat {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.partial_cmp(&other.0).expect("NaN in OrderedFloat")
    }
}

impl fmt::Display for OrderedFloat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<f64> for OrderedFloat {
    fn from(v: f64) -> Self {
        assert!(!v.is_nan(), "NaN not allowed in OrderedFloat");
        Self(v)
    }
}

/// A local in-memory order book, seeded from REST and updated via WS deltas.
///
/// Bids and asks are stored as BTreeMap<OrderedFloat, f64> where key=price, value=size.
/// Bids are stored with negated keys so that the highest bid comes first in iteration order.
#[derive(Debug, Clone)]
pub struct LocalOrderBook {
    pub token_id: String,
    /// Bids stored with negated price keys for descending order.
    bids: BTreeMap<OrderedFloat, f64>,
    /// Asks stored with normal price keys for ascending order.
    asks: BTreeMap<OrderedFloat, f64>,
}

impl LocalOrderBook {
    /// Create an empty local order book.
    pub fn new(token_id: impl Into<String>) -> Self {
        Self {
            token_id: token_id.into(),
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    /// Seed from a REST OrderBook snapshot.
    pub fn from_rest(ob: &OrderBook) -> Self {
        let token_id = ob.token_id.clone().unwrap_or_default();
        let mut book = Self::new(token_id);

        for level in &ob.bids {
            if let (Ok(price), Ok(size)) = (level.price.parse::<f64>(), level.size.parse::<f64>())
                && size > 0.0
            {
                book.bids.insert(OrderedFloat(-price), size);
            }
        }

        for level in &ob.asks {
            if let (Ok(price), Ok(size)) = (level.price.parse::<f64>(), level.size.parse::<f64>())
                && size > 0.0
            {
                book.asks.insert(OrderedFloat(price), size);
            }
        }

        book
    }

    /// Apply a delta update. If size is 0, remove the level.
    pub fn apply_delta(&mut self, side: &str, price: f64, size: f64) {
        match side {
            "buy" | "bid" => {
                let key = OrderedFloat(-price);
                if size <= 0.0 {
                    self.bids.remove(&key);
                } else {
                    self.bids.insert(key, size);
                }
            }
            "sell" | "ask" => {
                let key = OrderedFloat(price);
                if size <= 0.0 {
                    self.asks.remove(&key);
                } else {
                    self.asks.insert(key, size);
                }
            }
            _ => {}
        }
    }

    /// Best bid price (highest).
    pub fn best_bid(&self) -> Option<f64> {
        self.bids.keys().next().map(|k| -k.0)
    }

    /// Best ask price (lowest).
    pub fn best_ask(&self) -> Option<f64> {
        self.asks.keys().next().map(|k| k.0)
    }

    /// Spread between best ask and best bid.
    pub fn spread(&self) -> Option<f64> {
        match (self.best_ask(), self.best_bid()) {
            (Some(ask), Some(bid)) => Some(ask - bid),
            _ => None,
        }
    }

    /// Mid price: average of best bid and best ask.
    pub fn mid_price(&self) -> Option<f64> {
        match (self.best_ask(), self.best_bid()) {
            (Some(ask), Some(bid)) => Some((ask + bid) / 2.0),
            _ => None,
        }
    }

    /// Number of bid levels.
    pub fn bid_depth(&self) -> usize {
        self.bids.len()
    }

    /// Number of ask levels.
    pub fn ask_depth(&self) -> usize {
        self.asks.len()
    }

    /// Bids as (price, size) pairs in descending price order.
    pub fn bids(&self) -> Vec<(f64, f64)> {
        self.bids.iter().map(|(k, &v)| (-k.0, v)).collect()
    }

    /// Asks as (price, size) pairs in ascending price order.
    pub fn asks(&self) -> Vec<(f64, f64)> {
        self.asks.iter().map(|(k, &v)| (k.0, v)).collect()
    }

    /// Zero-alloc bid iterator (descending price order).
    pub fn bids_iter(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        self.bids.iter().map(|(k, &v)| (-k.0, v))
    }

    /// Zero-alloc ask iterator (ascending price order).
    pub fn asks_iter(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        self.asks.iter().map(|(k, &v)| (k.0, v))
    }

    /// Total size across all bid levels.
    pub fn total_bid_size(&self) -> f64 {
        self.bids.values().sum()
    }

    /// Total size across all ask levels.
    pub fn total_ask_size(&self) -> f64 {
        self.asks.values().sum()
    }

    /// Volume-weighted mid price (weighted by size at best bid/ask).
    pub fn weighted_mid_price(&self) -> Option<f64> {
        let (best_bid, bid_size) = self.bids.iter().next().map(|(k, &v)| (-k.0, v))?;
        let (best_ask, ask_size) = self.asks.iter().next().map(|(k, &v)| (k.0, v))?;
        let total = bid_size + ask_size;
        if total == 0.0 {
            return Some((best_bid + best_ask) / 2.0);
        }
        Some((best_bid * ask_size + best_ask * bid_size) / total)
    }

    /// Liquidity (total size) available at a specific price on a given side.
    pub fn liquidity_at_price(&self, side: Side, price: f64) -> f64 {
        match side {
            Side::Buy => self.bids.get(&OrderedFloat(-price)).copied().unwrap_or(0.0),
            Side::Sell => self.asks.get(&OrderedFloat(price)).copied().unwrap_or(0.0),
        }
    }

    /// Total liquidity within a price range (inclusive) on a given side.
    pub fn liquidity_in_range(&self, side: Side, min_price: f64, max_price: f64) -> f64 {
        match side {
            Side::Buy => {
                // Bids are stored with negated keys, so range is [-max_price, -min_price]
                let from = OrderedFloat(-max_price);
                let to = OrderedFloat(-min_price);
                self.bids.range(from..=to).map(|(_, &size)| size).sum()
            }
            Side::Sell => {
                let from = OrderedFloat(min_price);
                let to = OrderedFloat(max_price);
                self.asks.range(from..=to).map(|(_, &size)| size).sum()
            }
        }
    }

    /// Calculate the average execution price for a market order of given size.
    ///
    /// For Buy: walks the ask side (lowest first).
    /// For Sell: walks the bid side (highest first).
    /// Returns `None` if insufficient liquidity.
    pub fn calculate_market_price(&self, side: Side, size: f64) -> Option<f64> {
        let mut remaining = size;
        let mut total_cost = 0.0;

        let iter: Box<dyn Iterator<Item = (f64, f64)> + '_> = match side {
            Side::Buy => Box::new(self.asks_iter()),
            Side::Sell => Box::new(self.bids_iter()),
        };

        for (price, level_size) in iter {
            let fill = remaining.min(level_size);
            total_cost += fill * price;
            remaining -= fill;
            if remaining <= 0.0 {
                return Some(total_cost / size);
            }
        }

        None // insufficient liquidity
    }

    /// Calculate the market impact of executing a given size.
    pub fn calculate_market_impact(&self, side: Side, size: f64) -> Option<MarketImpact> {
        let reference_price = match side {
            Side::Buy => self.best_ask()?,
            Side::Sell => self.best_bid()?,
        };

        let avg_price = self.calculate_market_price(side, size)?;

        let impact_pct = ((avg_price - reference_price) / reference_price).abs() * 100.0;

        Some(MarketImpact {
            avg_price,
            reference_price,
            impact_pct,
            total_cost: avg_price * size,
            size_filled: size,
        })
    }

    /// Simulate filling an order, checking slippage tolerance.
    ///
    /// `max_slippage` is a fraction (e.g. 0.02 = 2%).
    /// Returns `Err` reason if slippage is exceeded or liquidity insufficient.
    pub fn simulate_fill(&self, side: Side, size: f64, max_slippage: Option<f64>) -> FillResult {
        let reference_price = match side {
            Side::Buy => match self.best_ask() {
                Some(p) => p,
                None => return FillResult::InsufficientLiquidity,
            },
            Side::Sell => match self.best_bid() {
                Some(p) => p,
                None => return FillResult::InsufficientLiquidity,
            },
        };

        let iter: Box<dyn Iterator<Item = (f64, f64)> + '_> = match side {
            Side::Buy => Box::new(self.asks_iter()),
            Side::Sell => Box::new(self.bids_iter()),
        };

        let mut remaining = size;
        let mut total_cost = 0.0;
        let mut fills = Vec::new();

        for (price, level_size) in iter {
            let fill = remaining.min(level_size);
            total_cost += fill * price;
            fills.push(Fill { price, size: fill });
            remaining -= fill;
            if remaining <= 0.0 {
                break;
            }
        }

        if remaining > 0.0 {
            return FillResult::InsufficientLiquidity;
        }

        let avg_price = total_cost / size;
        let slippage = ((avg_price - reference_price) / reference_price).abs();

        if let Some(max) = max_slippage
            && slippage > max
        {
            return FillResult::SlippageExceeded {
                actual_slippage: slippage,
                max_slippage: max,
            };
        }

        FillResult::Filled(FillSummary {
            avg_price,
            total_cost,
            size_filled: size,
            slippage,
            fills,
        })
    }
}

/// Market impact analysis result.
#[derive(Debug, Clone)]
pub struct MarketImpact {
    /// Volume-weighted average execution price.
    pub avg_price: f64,
    /// Best price before execution (best ask for buys, best bid for sells).
    pub reference_price: f64,
    /// Price impact as a percentage.
    pub impact_pct: f64,
    /// Total cost (avg_price * size).
    pub total_cost: f64,
    /// Amount that would be filled.
    pub size_filled: f64,
}

/// Result of a fill simulation.
#[derive(Debug, Clone)]
pub enum FillResult {
    /// Order would be fully filled.
    Filled(FillSummary),
    /// Not enough liquidity in the book.
    InsufficientLiquidity,
    /// Slippage exceeded tolerance.
    SlippageExceeded {
        actual_slippage: f64,
        max_slippage: f64,
    },
}

/// Summary of a simulated fill.
#[derive(Debug, Clone)]
pub struct FillSummary {
    pub avg_price: f64,
    pub total_cost: f64,
    pub size_filled: f64,
    /// Slippage as a fraction (0.02 = 2%).
    pub slippage: f64,
    pub fills: Vec<Fill>,
}

/// A single fill at one price level.
#[derive(Debug, Clone)]
pub struct Fill {
    pub price: f64,
    pub size: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{OrderBook, OrderBookLevel};
    use serde_json::json;

    fn make_level(price: &str, size: &str) -> OrderBookLevel {
        OrderBookLevel {
            price: price.into(),
            size: size.into(),
        }
    }

    fn sample_orderbook() -> OrderBook {
        OrderBook {
            market: Some("test".into()),
            token_id: Some("tok_1".into()),
            bids: vec![
                make_level("0.50", "100"),
                make_level("0.48", "200"),
                make_level("0.45", "50"),
            ],
            asks: vec![
                make_level("0.55", "150"),
                make_level("0.58", "300"),
                make_level("0.60", "75"),
            ],
            timestamp: Some(1700000000),
            extra: json!({}),
        }
    }

    #[test]
    fn from_rest_basic() {
        let ob = sample_orderbook();
        let local = LocalOrderBook::from_rest(&ob);
        assert_eq!(local.token_id, "tok_1");
        assert_eq!(local.bid_depth(), 3);
        assert_eq!(local.ask_depth(), 3);
    }

    #[test]
    fn best_bid_ask() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        assert!((local.best_bid().unwrap() - 0.50).abs() < f64::EPSILON);
        assert!((local.best_ask().unwrap() - 0.55).abs() < f64::EPSILON);
    }

    #[test]
    fn spread_and_mid() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        assert!((local.spread().unwrap() - 0.05).abs() < 1e-10);
        assert!((local.mid_price().unwrap() - 0.525).abs() < 1e-10);
    }

    #[test]
    fn empty_book_returns_none() {
        let local = LocalOrderBook::new("tok_empty");
        assert!(local.best_bid().is_none());
        assert!(local.best_ask().is_none());
        assert!(local.spread().is_none());
        assert!(local.mid_price().is_none());
    }

    #[test]
    fn apply_delta_add_level() {
        let mut local = LocalOrderBook::new("tok_1");
        local.apply_delta("buy", 0.50, 100.0);
        local.apply_delta("sell", 0.55, 200.0);
        assert!((local.best_bid().unwrap() - 0.50).abs() < f64::EPSILON);
        assert!((local.best_ask().unwrap() - 0.55).abs() < f64::EPSILON);
    }

    #[test]
    fn apply_delta_remove_level() {
        let mut local = LocalOrderBook::from_rest(&sample_orderbook());
        assert_eq!(local.bid_depth(), 3);
        local.apply_delta("buy", 0.50, 0.0);
        assert_eq!(local.bid_depth(), 2);
        assert!((local.best_bid().unwrap() - 0.48).abs() < f64::EPSILON);
    }

    #[test]
    fn apply_delta_update_size() {
        let mut local = LocalOrderBook::from_rest(&sample_orderbook());
        local.apply_delta("buy", 0.50, 999.0);
        let bids = local.bids();
        assert!((bids[0].0 - 0.50).abs() < f64::EPSILON);
        assert!((bids[0].1 - 999.0).abs() < f64::EPSILON);
    }

    #[test]
    fn bids_descending_order() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        let bids = local.bids();
        assert!(bids[0].0 > bids[1].0);
        assert!(bids[1].0 > bids[2].0);
    }

    #[test]
    fn asks_ascending_order() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        let asks = local.asks();
        assert!(asks[0].0 < asks[1].0);
        assert!(asks[1].0 < asks[2].0);
    }

    #[test]
    fn from_rest_skips_zero_size() {
        let ob = OrderBook {
            market: None,
            token_id: Some("tok_1".into()),
            bids: vec![make_level("0.50", "0"), make_level("0.48", "100")],
            asks: vec![make_level("0.55", "0")],
            timestamp: None,
            extra: json!({}),
        };
        let local = LocalOrderBook::from_rest(&ob);
        assert_eq!(local.bid_depth(), 1);
        assert_eq!(local.ask_depth(), 0);
    }

    #[test]
    fn ordered_float_ordering() {
        let a = OrderedFloat(1.0);
        let b = OrderedFloat(2.0);
        assert!(a < b);
        assert_eq!(OrderedFloat(1.0), OrderedFloat(1.0));
    }

    #[test]
    #[should_panic(expected = "NaN")]
    fn ordered_float_rejects_nan() {
        let _ = OrderedFloat::from(f64::NAN);
    }

    #[test]
    fn apply_delta_unknown_side_ignored() {
        let mut local = LocalOrderBook::new("tok_1");
        local.apply_delta("unknown", 0.50, 100.0);
        assert_eq!(local.bid_depth(), 0);
        assert_eq!(local.ask_depth(), 0);
    }

    #[test]
    fn total_bid_ask_size() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        assert!((local.total_bid_size() - 350.0).abs() < f64::EPSILON); // 100+200+50
        assert!((local.total_ask_size() - 525.0).abs() < f64::EPSILON); // 150+300+75
    }

    #[test]
    fn total_size_empty_book() {
        let local = LocalOrderBook::new("tok_empty");
        assert!((local.total_bid_size()).abs() < f64::EPSILON);
        assert!((local.total_ask_size()).abs() < f64::EPSILON);
    }

    #[test]
    fn weighted_mid_price_basic() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        let wmp = local.weighted_mid_price().unwrap();
        // best_bid=0.50 size=100, best_ask=0.55 size=150
        // wmp = (0.50*150 + 0.55*100) / 250 = (75+55)/250 = 0.52
        assert!((wmp - 0.52).abs() < 1e-10);
    }

    #[test]
    fn weighted_mid_price_empty() {
        let local = LocalOrderBook::new("tok_empty");
        assert!(local.weighted_mid_price().is_none());
    }

    #[test]
    fn liquidity_at_price_exists() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        assert!((local.liquidity_at_price(Side::Buy, 0.50) - 100.0).abs() < f64::EPSILON);
        assert!((local.liquidity_at_price(Side::Sell, 0.55) - 150.0).abs() < f64::EPSILON);
    }

    #[test]
    fn liquidity_at_price_missing() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        assert!((local.liquidity_at_price(Side::Buy, 0.99)).abs() < f64::EPSILON);
        assert!((local.liquidity_at_price(Side::Sell, 0.01)).abs() < f64::EPSILON);
    }

    #[test]
    fn liquidity_in_range_bids() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        // Bids: 0.50=100, 0.48=200, 0.45=50
        let liq = local.liquidity_in_range(Side::Buy, 0.47, 0.51);
        assert!((liq - 300.0).abs() < f64::EPSILON); // 0.50 + 0.48
    }

    #[test]
    fn liquidity_in_range_asks() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        // Asks: 0.55=150, 0.58=300, 0.60=75
        let liq = local.liquidity_in_range(Side::Sell, 0.55, 0.59);
        assert!((liq - 450.0).abs() < f64::EPSILON); // 0.55 + 0.58
    }

    #[test]
    fn liquidity_in_range_empty() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        let liq = local.liquidity_in_range(Side::Buy, 0.90, 0.99);
        assert!((liq).abs() < f64::EPSILON);
    }

    #[test]
    fn calculate_market_price_buy_single_level() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        let price = local.calculate_market_price(Side::Buy, 50.0).unwrap();
        assert!((price - 0.55).abs() < 1e-10); // fills entirely at best ask
    }

    #[test]
    fn calculate_market_price_buy_multi_level() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        // 200 units: 150 at 0.55 + 50 at 0.58
        let price = local.calculate_market_price(Side::Buy, 200.0).unwrap();
        let expected = (150.0 * 0.55 + 50.0 * 0.58) / 200.0;
        assert!((price - expected).abs() < 1e-10);
    }

    #[test]
    fn calculate_market_price_sell_single_level() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        let price = local.calculate_market_price(Side::Sell, 50.0).unwrap();
        assert!((price - 0.50).abs() < 1e-10); // fills at best bid
    }

    #[test]
    fn calculate_market_price_insufficient_liquidity() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        assert!(local.calculate_market_price(Side::Buy, 10000.0).is_none());
    }

    #[test]
    fn market_impact_basic() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        let impact = local.calculate_market_impact(Side::Buy, 200.0).unwrap();
        assert!((impact.reference_price - 0.55).abs() < 1e-10);
        assert!(impact.impact_pct > 0.0);
        assert!((impact.size_filled - 200.0).abs() < f64::EPSILON);
        assert!((impact.total_cost - impact.avg_price * 200.0).abs() < 1e-10);
    }

    #[test]
    fn market_impact_zero_for_small_order() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        let impact = local.calculate_market_impact(Side::Buy, 10.0).unwrap();
        assert!((impact.impact_pct).abs() < 1e-10); // fills entirely at best ask
    }

    #[test]
    fn market_impact_insufficient_liquidity() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        assert!(local.calculate_market_impact(Side::Buy, 10000.0).is_none());
    }

    #[test]
    fn simulate_fill_success() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        match local.simulate_fill(Side::Buy, 50.0, None) {
            FillResult::Filled(summary) => {
                assert!((summary.avg_price - 0.55).abs() < 1e-10);
                assert!((summary.size_filled - 50.0).abs() < f64::EPSILON);
                assert_eq!(summary.fills.len(), 1);
                assert!((summary.slippage).abs() < 1e-10);
            }
            other => panic!("expected Filled, got {:?}", other),
        }
    }

    #[test]
    fn simulate_fill_multi_level() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        match local.simulate_fill(Side::Buy, 200.0, None) {
            FillResult::Filled(summary) => {
                assert_eq!(summary.fills.len(), 2);
                assert!((summary.fills[0].price - 0.55).abs() < 1e-10);
                assert!((summary.fills[0].size - 150.0).abs() < f64::EPSILON);
                assert!((summary.fills[1].price - 0.58).abs() < 1e-10);
                assert!((summary.fills[1].size - 50.0).abs() < f64::EPSILON);
            }
            other => panic!("expected Filled, got {:?}", other),
        }
    }

    #[test]
    fn simulate_fill_insufficient_liquidity() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        assert!(matches!(
            local.simulate_fill(Side::Buy, 10000.0, None),
            FillResult::InsufficientLiquidity
        ));
    }

    #[test]
    fn simulate_fill_slippage_exceeded() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        // Buy 200: crosses levels, slippage > 0
        match local.simulate_fill(Side::Buy, 200.0, Some(0.001)) {
            FillResult::SlippageExceeded {
                actual_slippage,
                max_slippage,
            } => {
                assert!(actual_slippage > 0.001);
                assert!((max_slippage - 0.001).abs() < f64::EPSILON);
            }
            other => panic!("expected SlippageExceeded, got {:?}", other),
        }
    }

    #[test]
    fn simulate_fill_slippage_within_tolerance() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        // Buy 200 with generous slippage
        assert!(matches!(
            local.simulate_fill(Side::Buy, 200.0, Some(0.5)),
            FillResult::Filled(_)
        ));
    }

    #[test]
    fn simulate_fill_sell() {
        let local = LocalOrderBook::from_rest(&sample_orderbook());
        match local.simulate_fill(Side::Sell, 50.0, None) {
            FillResult::Filled(summary) => {
                assert!((summary.avg_price - 0.50).abs() < 1e-10);
                assert!((summary.size_filled - 50.0).abs() < f64::EPSILON);
            }
            other => panic!("expected Filled, got {:?}", other),
        }
    }

    #[test]
    fn simulate_fill_empty_book() {
        let local = LocalOrderBook::new("tok_empty");
        assert!(matches!(
            local.simulate_fill(Side::Buy, 10.0, None),
            FillResult::InsufficientLiquidity
        ));
    }
}
