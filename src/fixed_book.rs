use std::cmp::Reverse;
use std::collections::BTreeMap;

use crate::models::OrderBook;
use crate::types::Side;

/// Price scale: 10,000 = 1.0000 (4 decimal places).
const PRICE_SCALE: u32 = 10_000;
/// Size scale: 1,000,000 = 1.000000 (6 decimal places).
const SIZE_SCALE: i64 = 1_000_000;

/// Convert a float price to fixed-point u32 (0..10000 for 0.0..1.0).
#[inline(always)]
pub fn price_to_fixed(price: f64) -> u32 {
    (price * PRICE_SCALE as f64).round() as u32
}

/// Convert a fixed-point u32 price back to f64.
#[inline(always)]
pub fn fixed_to_price(fixed: u32) -> f64 {
    fixed as f64 / PRICE_SCALE as f64
}

/// Convert a float size to fixed-point i64.
#[inline(always)]
pub fn size_to_fixed(size: f64) -> i64 {
    (size * SIZE_SCALE as f64).round() as i64
}

/// Convert a fixed-point i64 size back to f64.
#[inline(always)]
pub fn fixed_to_size(fixed: i64) -> f64 {
    fixed as f64 / SIZE_SCALE as f64
}

/// Parse a price string directly to fixed-point u32, avoiding f64 intermediary.
///
/// Handles strings like "0.55", "0.5", "1", "0.5500". Returns None for invalid input.
#[inline]
pub fn parse_price_fixed(s: &str) -> Option<u32> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let mut integer_part: u32 = 0;
    let mut fractional: u32 = 0;
    let mut frac_digits: u32 = 0;
    let mut in_fraction = false;

    for &b in bytes {
        if b == b'.' {
            if in_fraction {
                return None; // double dot
            }
            in_fraction = true;
            continue;
        }
        if !b.is_ascii_digit() {
            return None;
        }
        let digit = (b - b'0') as u32;
        if in_fraction {
            if frac_digits < 4 {
                fractional = fractional * 10 + digit;
                frac_digits += 1;
            }
            // ignore digits beyond 4 decimal places
        } else {
            integer_part = integer_part * 10 + digit;
        }
    }

    // Scale fractional part to 4 decimal places
    while frac_digits < 4 {
        fractional *= 10;
        frac_digits += 1;
    }

    Some(integer_part * PRICE_SCALE + fractional)
}

/// Parse a size string directly to fixed-point i64.
#[inline]
pub fn parse_size_fixed(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let mut integer_part: i64 = 0;
    let mut fractional: i64 = 0;
    let mut frac_digits: u32 = 0;
    let mut in_fraction = false;

    for &b in bytes {
        if b == b'.' {
            if in_fraction {
                return None;
            }
            in_fraction = true;
            continue;
        }
        if !b.is_ascii_digit() {
            return None;
        }
        let digit = (b - b'0') as i64;
        if in_fraction {
            if frac_digits < 6 {
                fractional = fractional * 10 + digit;
                frac_digits += 1;
            }
        } else {
            integer_part = integer_part * 10 + digit;
        }
    }

    while frac_digits < 6 {
        fractional *= 10;
        frac_digits += 1;
    }

    Some(integer_part * SIZE_SCALE + fractional)
}

/// High-performance fixed-point order book.
///
/// Uses `u32` prices (scaled by 10,000) and `i64` sizes (scaled by 1,000,000).
/// BTreeMap key comparison on `u32` is a single CPU instruction — no NaN checks,
/// no floating-point comparison overhead.
///
/// Bids use `Reverse<u32>` for natural descending order without the negation hack.
#[derive(Debug, Clone)]
pub struct FixedOrderBook {
    pub token_id: String,
    bids: BTreeMap<Reverse<u32>, i64>,
    asks: BTreeMap<u32, i64>,
}

impl FixedOrderBook {
    pub fn new(token_id: impl Into<String>) -> Self {
        Self {
            token_id: token_id.into(),
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    /// Seed from a REST OrderBook snapshot using zero-alloc string parsing.
    pub fn from_rest(ob: &OrderBook) -> Self {
        let token_id = ob.token_id.clone().unwrap_or_default();
        let mut book = Self::new(token_id);

        for level in &ob.bids {
            if let (Some(price), Some(size)) = (
                parse_price_fixed(&level.price),
                parse_size_fixed(&level.size),
            ) && size > 0
            {
                book.bids.insert(Reverse(price), size);
            }
        }

        for level in &ob.asks {
            if let (Some(price), Some(size)) = (
                parse_price_fixed(&level.price),
                parse_size_fixed(&level.size),
            ) && size > 0
            {
                book.asks.insert(price, size);
            }
        }

        book
    }

    /// Hot-path delta application using pre-converted fixed-point values.
    ///
    /// `side`: 0 = bid, 1 = ask. Avoids string matching entirely.
    #[inline(always)]
    pub fn apply_delta_fixed(&mut self, side: u8, price: u32, size: i64) {
        if side == 0 {
            if size <= 0 {
                self.bids.remove(&Reverse(price));
            } else {
                self.bids.insert(Reverse(price), size);
            }
        } else if size <= 0 {
            self.asks.remove(&price);
        } else {
            self.asks.insert(price, size);
        }
    }

    /// Convenience delta with string side and f64 values (converts to fixed-point).
    #[inline]
    pub fn apply_delta(&mut self, side: &str, price: f64, size: f64) {
        let fp = price_to_fixed(price);
        let fs = size_to_fixed(size);
        match side {
            "buy" | "bid" => self.apply_delta_fixed(0, fp, fs),
            "sell" | "ask" => self.apply_delta_fixed(1, fp, fs),
            _ => {}
        }
    }

    /// Apply delta with typed Side enum.
    #[inline]
    pub fn apply_delta_typed(&mut self, side: Side, price: f64, size: f64) {
        let fp = price_to_fixed(price);
        let fs = size_to_fixed(size);
        match side {
            Side::Buy => self.apply_delta_fixed(0, fp, fs),
            Side::Sell => self.apply_delta_fixed(1, fp, fs),
        }
    }

    pub fn best_bid(&self) -> Option<f64> {
        self.bids.keys().next().map(|k| fixed_to_price(k.0))
    }

    pub fn best_ask(&self) -> Option<f64> {
        self.asks.keys().next().map(|k| fixed_to_price(*k))
    }

    /// Best bid as raw fixed-point u32 (no conversion overhead).
    #[inline]
    pub fn best_bid_fixed(&self) -> Option<u32> {
        self.bids.keys().next().map(|k| k.0)
    }

    /// Best ask as raw fixed-point u32 (no conversion overhead).
    #[inline]
    pub fn best_ask_fixed(&self) -> Option<u32> {
        self.asks.keys().next().copied()
    }

    pub fn spread(&self) -> Option<f64> {
        match (self.best_ask(), self.best_bid()) {
            (Some(ask), Some(bid)) => Some(ask - bid),
            _ => None,
        }
    }

    /// Spread in fixed-point units (1 unit = 0.0001).
    #[inline]
    pub fn spread_fixed(&self) -> Option<u32> {
        match (self.best_ask_fixed(), self.best_bid_fixed()) {
            (Some(ask), Some(bid)) if ask >= bid => Some(ask - bid),
            _ => None,
        }
    }

    pub fn mid_price(&self) -> Option<f64> {
        match (self.best_ask(), self.best_bid()) {
            (Some(ask), Some(bid)) => Some((ask + bid) / 2.0),
            _ => None,
        }
    }

    pub fn bid_depth(&self) -> usize {
        self.bids.len()
    }

    pub fn ask_depth(&self) -> usize {
        self.asks.len()
    }

    /// Bids as (price, size) f64 pairs in descending price order.
    pub fn bids(&self) -> Vec<(f64, f64)> {
        self.bids
            .iter()
            .map(|(k, &v)| (fixed_to_price(k.0), fixed_to_size(v)))
            .collect()
    }

    /// Asks as (price, size) f64 pairs in ascending price order.
    pub fn asks(&self) -> Vec<(f64, f64)> {
        self.asks
            .iter()
            .map(|(k, &v)| (fixed_to_price(*k), fixed_to_size(v)))
            .collect()
    }

    /// Zero-alloc bid iterator.
    pub fn bids_iter(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        self.bids
            .iter()
            .map(|(k, &v)| (fixed_to_price(k.0), fixed_to_size(v)))
    }

    /// Zero-alloc ask iterator.
    pub fn asks_iter(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        self.asks
            .iter()
            .map(|(k, &v)| (fixed_to_price(*k), fixed_to_size(v)))
    }

    /// Raw fixed-point bid iterator (zero conversion overhead).
    pub fn bids_fixed_iter(&self) -> impl Iterator<Item = (u32, i64)> + '_ {
        self.bids.iter().map(|(k, &v)| (k.0, v))
    }

    /// Raw fixed-point ask iterator (zero conversion overhead).
    pub fn asks_fixed_iter(&self) -> impl Iterator<Item = (u32, i64)> + '_ {
        self.asks.iter().map(|(k, &v)| (*k, v))
    }

    pub fn total_bid_size(&self) -> f64 {
        fixed_to_size(self.bids.values().sum())
    }

    pub fn total_ask_size(&self) -> f64 {
        fixed_to_size(self.asks.values().sum())
    }

    pub fn weighted_mid_price(&self) -> Option<f64> {
        let (&Reverse(bid_p), &bid_s) = self.bids.iter().next()?;
        let (&ask_p, &ask_s) = self.asks.iter().next()?;
        let total = bid_s + ask_s;
        if total == 0 {
            return Some((fixed_to_price(bid_p) + fixed_to_price(ask_p)) / 2.0);
        }
        // Fixed-point weighted calculation
        let bid_f = fixed_to_price(bid_p);
        let ask_f = fixed_to_price(ask_p);
        let bid_sz = fixed_to_size(bid_s);
        let ask_sz = fixed_to_size(ask_s);
        Some((bid_f * ask_sz + ask_f * bid_sz) / (bid_sz + ask_sz))
    }

    pub fn liquidity_at_price(&self, side: Side, price: f64) -> f64 {
        let fp = price_to_fixed(price);
        match side {
            Side::Buy => self
                .bids
                .get(&Reverse(fp))
                .map(|&s| fixed_to_size(s))
                .unwrap_or(0.0),
            Side::Sell => self.asks.get(&fp).map(|&s| fixed_to_size(s)).unwrap_or(0.0),
        }
    }

    pub fn liquidity_in_range(&self, side: Side, min_price: f64, max_price: f64) -> f64 {
        let min_fp = price_to_fixed(min_price);
        let max_fp = price_to_fixed(max_price);
        match side {
            Side::Buy => {
                let total: i64 = self
                    .bids
                    .range(Reverse(max_fp)..=Reverse(min_fp))
                    .map(|(_, &s)| s)
                    .sum();
                fixed_to_size(total)
            }
            Side::Sell => {
                let total: i64 = self.asks.range(min_fp..=max_fp).map(|(_, &s)| s).sum();
                fixed_to_size(total)
            }
        }
    }

    /// Calculate average execution price using iterators (zero allocation).
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

        None
    }

    pub fn calculate_market_impact(
        &self,
        side: Side,
        size: f64,
    ) -> Option<super::orderbook::MarketImpact> {
        let reference_price = match side {
            Side::Buy => self.best_ask()?,
            Side::Sell => self.best_bid()?,
        };

        let avg_price = self.calculate_market_price(side, size)?;
        let impact_pct = ((avg_price - reference_price) / reference_price).abs() * 100.0;

        Some(super::orderbook::MarketImpact {
            avg_price,
            reference_price,
            impact_pct,
            total_cost: avg_price * size,
            size_filled: size,
        })
    }

    pub fn simulate_fill(
        &self,
        side: Side,
        size: f64,
        max_slippage: Option<f64>,
    ) -> super::orderbook::FillResult {
        use super::orderbook::{Fill, FillResult, FillSummary};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{OrderBook, OrderBookLevel};
    use crate::orderbook::FillResult;
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

    // --- Fixed-point conversion tests ---

    #[test]
    fn price_roundtrip() {
        for price in [0.0, 0.01, 0.5, 0.55, 0.999, 1.0] {
            let fixed = price_to_fixed(price);
            let back = fixed_to_price(fixed);
            assert!(
                (back - price).abs() < 1e-4,
                "roundtrip failed for {price}: got {back}"
            );
        }
    }

    #[test]
    fn size_roundtrip() {
        for size in [0.0, 1.0, 100.5, 999999.123456] {
            let fixed = size_to_fixed(size);
            let back = fixed_to_size(fixed);
            assert!(
                (back - size).abs() < 1e-6,
                "roundtrip failed for {size}: got {back}"
            );
        }
    }

    #[test]
    fn parse_price_fixed_various() {
        assert_eq!(parse_price_fixed("0.55"), Some(5500));
        assert_eq!(parse_price_fixed("0.5"), Some(5000));
        assert_eq!(parse_price_fixed("0.5500"), Some(5500));
        assert_eq!(parse_price_fixed("1"), Some(10000));
        assert_eq!(parse_price_fixed("0"), Some(0));
        assert_eq!(parse_price_fixed("0.0001"), Some(1));
        assert_eq!(parse_price_fixed("0.50"), Some(5000));
    }

    #[test]
    fn parse_price_fixed_invalid() {
        assert_eq!(parse_price_fixed(""), None);
        assert_eq!(parse_price_fixed("abc"), None);
        assert_eq!(parse_price_fixed("0.1.2"), None);
    }

    #[test]
    fn parse_size_fixed_various() {
        assert_eq!(parse_size_fixed("100"), Some(100_000_000));
        assert_eq!(parse_size_fixed("0.5"), Some(500_000));
        assert_eq!(parse_size_fixed("0"), Some(0));
        assert_eq!(parse_size_fixed("99.123456"), Some(99_123_456));
    }

    // --- FixedOrderBook tests ---

    #[test]
    fn from_rest_basic() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        assert_eq!(book.token_id, "tok_1");
        assert_eq!(book.bid_depth(), 3);
        assert_eq!(book.ask_depth(), 3);
    }

    #[test]
    fn best_bid_ask() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        assert!((book.best_bid().unwrap() - 0.50).abs() < 1e-4);
        assert!((book.best_ask().unwrap() - 0.55).abs() < 1e-4);
    }

    #[test]
    fn best_bid_ask_fixed() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        assert_eq!(book.best_bid_fixed(), Some(5000));
        assert_eq!(book.best_ask_fixed(), Some(5500));
    }

    #[test]
    fn spread_and_mid() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        assert!((book.spread().unwrap() - 0.05).abs() < 1e-4);
        assert!((book.mid_price().unwrap() - 0.525).abs() < 1e-4);
    }

    #[test]
    fn spread_fixed() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        assert_eq!(book.spread_fixed(), Some(500)); // 0.05 * 10000
    }

    #[test]
    fn empty_book() {
        let book = FixedOrderBook::new("tok_empty");
        assert!(book.best_bid().is_none());
        assert!(book.best_ask().is_none());
        assert!(book.spread().is_none());
    }

    #[test]
    fn apply_delta_fixed_add_remove() {
        let mut book = FixedOrderBook::new("tok_1");
        book.apply_delta_fixed(0, 5000, 100_000_000); // bid 0.50, size 100
        book.apply_delta_fixed(1, 5500, 200_000_000); // ask 0.55, size 200
        assert_eq!(book.best_bid_fixed(), Some(5000));
        assert_eq!(book.best_ask_fixed(), Some(5500));

        // Remove bid
        book.apply_delta_fixed(0, 5000, 0);
        assert!(book.best_bid().is_none());
    }

    #[test]
    fn apply_delta_string_convenience() {
        let mut book = FixedOrderBook::new("tok_1");
        book.apply_delta("buy", 0.50, 100.0);
        book.apply_delta("sell", 0.55, 200.0);
        assert!((book.best_bid().unwrap() - 0.50).abs() < 1e-4);
        assert!((book.best_ask().unwrap() - 0.55).abs() < 1e-4);
    }

    #[test]
    fn apply_delta_typed() {
        let mut book = FixedOrderBook::new("tok_1");
        book.apply_delta_typed(Side::Buy, 0.50, 100.0);
        book.apply_delta_typed(Side::Sell, 0.55, 200.0);
        assert!((book.best_bid().unwrap() - 0.50).abs() < 1e-4);
    }

    #[test]
    fn bids_descending_order() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        let bids = book.bids();
        assert!(bids[0].0 > bids[1].0);
        assert!(bids[1].0 > bids[2].0);
    }

    #[test]
    fn asks_ascending_order() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        let asks = book.asks();
        assert!(asks[0].0 < asks[1].0);
        assert!(asks[1].0 < asks[2].0);
    }

    #[test]
    fn total_sizes() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        assert!((book.total_bid_size() - 350.0).abs() < 1e-4);
        assert!((book.total_ask_size() - 525.0).abs() < 1e-4);
    }

    #[test]
    fn weighted_mid_price() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        let wmp = book.weighted_mid_price().unwrap();
        assert!((wmp - 0.52).abs() < 1e-4);
    }

    #[test]
    fn liquidity_at_price() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        assert!((book.liquidity_at_price(Side::Buy, 0.50) - 100.0).abs() < 1e-4);
        assert!((book.liquidity_at_price(Side::Sell, 0.55) - 150.0).abs() < 1e-4);
        assert!((book.liquidity_at_price(Side::Buy, 0.99)).abs() < 1e-4);
    }

    #[test]
    fn liquidity_in_range() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        let liq = book.liquidity_in_range(Side::Buy, 0.47, 0.51);
        assert!((liq - 300.0).abs() < 1e-4);
        let liq = book.liquidity_in_range(Side::Sell, 0.55, 0.59);
        assert!((liq - 450.0).abs() < 1e-4);
    }

    #[test]
    fn calculate_market_price_single_level() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        let price = book.calculate_market_price(Side::Buy, 50.0).unwrap();
        assert!((price - 0.55).abs() < 1e-4);
    }

    #[test]
    fn calculate_market_price_multi_level() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        let price = book.calculate_market_price(Side::Buy, 200.0).unwrap();
        let expected = (150.0 * 0.55 + 50.0 * 0.58) / 200.0;
        assert!((price - expected).abs() < 1e-4);
    }

    #[test]
    fn calculate_market_price_insufficient() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        assert!(book.calculate_market_price(Side::Buy, 10000.0).is_none());
    }

    #[test]
    fn market_impact() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        let impact = book.calculate_market_impact(Side::Buy, 200.0).unwrap();
        assert!((impact.reference_price - 0.55).abs() < 1e-4);
        assert!(impact.impact_pct > 0.0);
    }

    #[test]
    fn simulate_fill_success() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        match book.simulate_fill(Side::Buy, 50.0, None) {
            FillResult::Filled(s) => {
                assert!((s.avg_price - 0.55).abs() < 1e-4);
                assert_eq!(s.fills.len(), 1);
            }
            other => panic!("expected Filled, got {:?}", other),
        }
    }

    #[test]
    fn simulate_fill_slippage() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        assert!(matches!(
            book.simulate_fill(Side::Buy, 200.0, Some(0.001)),
            FillResult::SlippageExceeded { .. }
        ));
    }

    #[test]
    fn iterator_matches_vec() {
        let book = FixedOrderBook::from_rest(&sample_orderbook());
        let bids_vec = book.bids();
        let bids_iter: Vec<_> = book.bids_iter().collect();
        assert_eq!(bids_vec.len(), bids_iter.len());
        for (a, b) in bids_vec.iter().zip(bids_iter.iter()) {
            assert!((a.0 - b.0).abs() < 1e-10);
            assert!((a.1 - b.1).abs() < 1e-4);
        }
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
        let book = FixedOrderBook::from_rest(&ob);
        assert_eq!(book.bid_depth(), 1);
        assert_eq!(book.ask_depth(), 0);
    }
}
