use crate::error::{Result, SdkError};
use crate::orderbook::LocalOrderBook;
use crate::types::{CreateOrderRequest, Side};

/// Supported tick sizes for price rounding.
///
/// opinion.trade uses string-based prices (e.g. "0.55"). The tick size determines
/// the minimum price increment. Default is `Hundredths` (0.01) which is standard
/// for prediction markets with prices between 0 and 1.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TickSize {
    /// 0.1 — one decimal place
    Tenths,
    /// 0.01 — two decimal places (default for prediction markets)
    #[default]
    Hundredths,
    /// 0.001 — three decimal places
    Thousandths,
    /// 0.0001 — four decimal places
    TenThousandths,
}

impl TickSize {
    /// The tick size as an f64 value.
    pub fn value(self) -> f64 {
        match self {
            Self::Tenths => 0.1,
            Self::Hundredths => 0.01,
            Self::Thousandths => 0.001,
            Self::TenThousandths => 0.0001,
        }
    }

    /// Number of decimal places for this tick size.
    pub fn decimals(self) -> u32 {
        match self {
            Self::Tenths => 1,
            Self::Hundredths => 2,
            Self::Thousandths => 3,
            Self::TenThousandths => 4,
        }
    }
}

/// Round a price to the nearest tick, biased in the favorable direction for the given side.
///
/// - Buy orders round **down** (you don't want to pay more than intended)
/// - Sell orders round **up** (you don't want to receive less than intended)
pub fn round_price(price: f64, tick_size: TickSize, side: Side) -> f64 {
    let tick = tick_size.value();
    let factor = 1.0 / tick;
    // Round to nearest tick first to avoid floating point drift,
    // then apply directional bias only if not already on a tick boundary.
    let scaled = price * factor;
    let nearest = scaled.round();
    // If already on a tick (within epsilon), return that
    if (scaled - nearest).abs() < 1e-9 {
        return nearest / factor;
    }
    match side {
        Side::Buy => scaled.floor() / factor,
        Side::Sell => scaled.ceil() / factor,
    }
}

/// Format a price to the correct number of decimal places for a tick size.
pub fn format_price(price: f64, tick_size: TickSize) -> String {
    format!("{:.prec$}", price, prec = tick_size.decimals() as usize)
}

/// Builder for constructing validated `CreateOrderRequest`s.
///
/// Handles price rounding, validation (price in 0..1 range, positive size),
/// and optional market order price calculation from a local order book.
#[derive(Debug, Clone)]
pub struct OrderBuilder {
    token_id: String,
    side: Side,
    price: Option<f64>,
    size: f64,
    tick_size: TickSize,
    chain_id: Option<String>,
    max_slippage: Option<f64>,
}

impl OrderBuilder {
    pub fn new(token_id: impl Into<String>, side: Side, size: f64) -> Self {
        Self {
            token_id: token_id.into(),
            side,
            price: None,
            size,
            tick_size: TickSize::default(),
            chain_id: None,
            max_slippage: None,
        }
    }

    /// Set the limit price. Will be rounded to the tick size.
    pub fn price(mut self, price: f64) -> Self {
        self.price = Some(price);
        self
    }

    /// Set the tick size (default: Hundredths / 0.01).
    pub fn tick_size(mut self, tick_size: TickSize) -> Self {
        self.tick_size = tick_size;
        self
    }

    /// Set the chain ID.
    pub fn chain_id(mut self, chain_id: impl Into<String>) -> Self {
        self.chain_id = Some(chain_id.into());
        self
    }

    /// Set maximum slippage tolerance (as a fraction, e.g., 0.02 = 2%).
    /// Only used with `build_market_order`.
    pub fn max_slippage(mut self, slippage: f64) -> Self {
        self.max_slippage = Some(slippage);
        self
    }

    /// Build a limit order request. Price must be set.
    pub fn build(self) -> Result<CreateOrderRequest> {
        let raw_price = self
            .price
            .ok_or_else(|| SdkError::Validation("price is required for limit orders".into()))?;

        self.validate_common(raw_price)?;

        let rounded = round_price(raw_price, self.tick_size, self.side);
        self.validate_common(rounded)?;

        Ok(CreateOrderRequest {
            token_id: self.token_id,
            side: self.side,
            price: format_price(rounded, self.tick_size),
            size: format!("{}", self.size),
            chain_id: self.chain_id,
        })
    }

    /// Build a market order by walking the order book to determine execution price.
    ///
    /// Calculates the price needed to fill `size` from the book, applies slippage
    /// tolerance, rounds to tick, and returns a limit order at that price.
    pub fn build_market_order(self, book: &LocalOrderBook) -> Result<CreateOrderRequest> {
        if self.size <= 0.0 {
            return Err(SdkError::Validation("size must be positive".into()));
        }

        let market_price = book
            .calculate_market_price(self.side, self.size)
            .ok_or_else(|| SdkError::Validation("insufficient liquidity in order book".into()))?;

        // Apply slippage buffer
        let price_with_slippage = if let Some(slippage) = self.max_slippage {
            match self.side {
                Side::Buy => market_price * (1.0 + slippage),
                Side::Sell => market_price * (1.0 - slippage),
            }
        } else {
            market_price
        };

        let rounded = round_price(price_with_slippage, self.tick_size, self.side);

        // Clamp to valid prediction market range
        let clamped = rounded.clamp(self.tick_size.value(), 1.0 - self.tick_size.value());

        Ok(CreateOrderRequest {
            token_id: self.token_id,
            side: self.side,
            price: format_price(clamped, self.tick_size),
            size: format!("{}", self.size),
            chain_id: self.chain_id,
        })
    }

    fn validate_common(&self, price: f64) -> Result<()> {
        if self.size <= 0.0 {
            return Err(SdkError::Validation("size must be positive".into()));
        }
        if price <= 0.0 || price >= 1.0 {
            return Err(SdkError::Validation(format!(
                "price {price} must be between 0 and 1 (exclusive) for prediction markets"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{OrderBook, OrderBookLevel};
    use serde_json::json;

    #[test]
    fn tick_size_values() {
        assert!((TickSize::Tenths.value() - 0.1).abs() < f64::EPSILON);
        assert!((TickSize::Hundredths.value() - 0.01).abs() < f64::EPSILON);
        assert!((TickSize::Thousandths.value() - 0.001).abs() < f64::EPSILON);
        assert!((TickSize::TenThousandths.value() - 0.0001).abs() < f64::EPSILON);
    }

    #[test]
    fn tick_size_decimals() {
        assert_eq!(TickSize::Tenths.decimals(), 1);
        assert_eq!(TickSize::Hundredths.decimals(), 2);
        assert_eq!(TickSize::Thousandths.decimals(), 3);
        assert_eq!(TickSize::TenThousandths.decimals(), 4);
    }

    #[test]
    fn tick_size_default_is_hundredths() {
        assert_eq!(TickSize::default(), TickSize::Hundredths);
    }

    #[test]
    fn round_price_buy_rounds_down() {
        assert!((round_price(0.556, TickSize::Hundredths, Side::Buy) - 0.55).abs() < 1e-10);
        assert!((round_price(0.559, TickSize::Hundredths, Side::Buy) - 0.55).abs() < 1e-10);
    }

    #[test]
    fn round_price_sell_rounds_up() {
        assert!((round_price(0.551, TickSize::Hundredths, Side::Sell) - 0.56).abs() < 1e-10);
        assert!((round_price(0.554, TickSize::Hundredths, Side::Sell) - 0.56).abs() < 1e-10);
    }

    #[test]
    fn round_price_exact_tick_unchanged() {
        assert!((round_price(0.55, TickSize::Hundredths, Side::Buy) - 0.55).abs() < 1e-10);
        assert!((round_price(0.55, TickSize::Hundredths, Side::Sell) - 0.55).abs() < 1e-10);
    }

    #[test]
    fn round_price_tenths() {
        assert!((round_price(0.56, TickSize::Tenths, Side::Buy) - 0.5).abs() < 1e-10);
        assert!((round_price(0.51, TickSize::Tenths, Side::Sell) - 0.6).abs() < 1e-10);
    }

    #[test]
    fn round_price_thousandths() {
        assert!((round_price(0.5556, TickSize::Thousandths, Side::Buy) - 0.555).abs() < 1e-10);
        assert!((round_price(0.5551, TickSize::Thousandths, Side::Sell) - 0.556).abs() < 1e-10);
    }

    #[test]
    fn format_price_decimals() {
        assert_eq!(format_price(0.5, TickSize::Tenths), "0.5");
        assert_eq!(format_price(0.5, TickSize::Hundredths), "0.50");
        assert_eq!(format_price(0.5, TickSize::Thousandths), "0.500");
        assert_eq!(format_price(0.5, TickSize::TenThousandths), "0.5000");
    }

    #[test]
    fn order_builder_limit_order() {
        let req = OrderBuilder::new("tok_1", Side::Buy, 100.0)
            .price(0.556)
            .build()
            .unwrap();
        assert_eq!(req.token_id, "tok_1");
        assert_eq!(req.side, Side::Buy);
        assert_eq!(req.price, "0.55"); // rounded down for buy
        assert_eq!(req.size, "100");
    }

    #[test]
    fn order_builder_sell_rounds_up() {
        let req = OrderBuilder::new("tok_1", Side::Sell, 50.0)
            .price(0.551)
            .build()
            .unwrap();
        assert_eq!(req.price, "0.56");
    }

    #[test]
    fn order_builder_custom_tick_size() {
        let req = OrderBuilder::new("tok_1", Side::Buy, 100.0)
            .price(0.5556)
            .tick_size(TickSize::Thousandths)
            .build()
            .unwrap();
        assert_eq!(req.price, "0.555");
    }

    #[test]
    fn order_builder_with_chain_id() {
        let req = OrderBuilder::new("tok_1", Side::Buy, 100.0)
            .price(0.55)
            .chain_id("137")
            .build()
            .unwrap();
        assert_eq!(req.chain_id.as_deref(), Some("137"));
    }

    #[test]
    fn order_builder_rejects_zero_size() {
        let result = OrderBuilder::new("tok_1", Side::Buy, 0.0)
            .price(0.55)
            .build();
        assert!(matches!(result.unwrap_err(), SdkError::Validation(_)));
    }

    #[test]
    fn order_builder_rejects_negative_size() {
        let result = OrderBuilder::new("tok_1", Side::Buy, -10.0)
            .price(0.55)
            .build();
        assert!(matches!(result.unwrap_err(), SdkError::Validation(_)));
    }

    #[test]
    fn order_builder_rejects_price_at_zero() {
        let result = OrderBuilder::new("tok_1", Side::Buy, 100.0)
            .price(0.0)
            .build();
        assert!(matches!(result.unwrap_err(), SdkError::Validation(_)));
    }

    #[test]
    fn order_builder_rejects_price_at_one() {
        let result = OrderBuilder::new("tok_1", Side::Buy, 100.0)
            .price(1.0)
            .build();
        assert!(matches!(result.unwrap_err(), SdkError::Validation(_)));
    }

    #[test]
    fn order_builder_rejects_price_above_one() {
        let result = OrderBuilder::new("tok_1", Side::Buy, 100.0)
            .price(1.5)
            .build();
        assert!(matches!(result.unwrap_err(), SdkError::Validation(_)));
    }

    #[test]
    fn order_builder_no_price_errors() {
        let result = OrderBuilder::new("tok_1", Side::Buy, 100.0).build();
        assert!(matches!(result.unwrap_err(), SdkError::Validation(_)));
    }

    fn make_book() -> LocalOrderBook {
        let ob = OrderBook {
            market: None,
            token_id: Some("tok_1".into()),
            bids: vec![
                OrderBookLevel {
                    price: "0.50".into(),
                    size: "100".into(),
                },
                OrderBookLevel {
                    price: "0.48".into(),
                    size: "200".into(),
                },
            ],
            asks: vec![
                OrderBookLevel {
                    price: "0.55".into(),
                    size: "100".into(),
                },
                OrderBookLevel {
                    price: "0.58".into(),
                    size: "200".into(),
                },
            ],
            timestamp: None,
            extra: json!({}),
        };
        LocalOrderBook::from_rest(&ob)
    }

    #[test]
    fn order_builder_market_buy() {
        let book = make_book();
        let req = OrderBuilder::new("tok_1", Side::Buy, 50.0)
            .build_market_order(&book)
            .unwrap();
        assert_eq!(req.side, Side::Buy);
        // Should buy from asks, 50 units at 0.55 -> price 0.55
        assert_eq!(req.price, "0.55");
    }

    #[test]
    fn order_builder_market_buy_crossing_levels() {
        let book = make_book();
        // 150 units: 100 at 0.55 + 50 at 0.58 -> weighted avg
        let req = OrderBuilder::new("tok_1", Side::Buy, 150.0)
            .build_market_order(&book)
            .unwrap();
        // avg = (100*0.55 + 50*0.58) / 150 = (55 + 29) / 150 = 0.56
        assert_eq!(req.price, "0.56");
    }

    #[test]
    fn order_builder_market_sell() {
        let book = make_book();
        let req = OrderBuilder::new("tok_1", Side::Sell, 50.0)
            .build_market_order(&book)
            .unwrap();
        assert_eq!(req.side, Side::Sell);
        // Should sell into bids, 50 units at 0.50 -> price 0.50
        assert_eq!(req.price, "0.50");
    }

    #[test]
    fn order_builder_market_order_with_slippage() {
        let book = make_book();
        let req = OrderBuilder::new("tok_1", Side::Buy, 50.0)
            .max_slippage(0.02)
            .build_market_order(&book)
            .unwrap();
        // 0.55 * 1.02 = 0.561 -> rounded down to 0.56
        assert_eq!(req.price, "0.56");
    }

    #[test]
    fn order_builder_market_order_insufficient_liquidity() {
        let book = make_book();
        let result = OrderBuilder::new("tok_1", Side::Buy, 10000.0).build_market_order(&book);
        assert!(matches!(result.unwrap_err(), SdkError::Validation(_)));
    }
}
