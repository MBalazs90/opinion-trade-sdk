use crate::error::{Result, SdkError};
use crate::orderbook::LocalOrderBook;
use crate::types::{CreateOrderRequest, OrderType, Side};

/// Supported tick sizes for price rounding.
///
/// opinion.trade prices have a maximum of 4 decimal places (0.0001 tick size).
/// Default is `Hundredths` (0.01).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TickSize {
    /// 0.1 — one decimal place
    Tenths,
    /// 0.01 — two decimal places (default for prediction markets)
    #[default]
    Hundredths,
    /// 0.001 — three decimal places
    Thousandths,
    /// 0.0001 — four decimal places (maximum precision per API docs)
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

/// Valid price range for opinion.trade prediction markets.
const MIN_PRICE: f64 = 0.01;
const MAX_PRICE: f64 = 0.99;

/// Builder for constructing validated `CreateOrderRequest`s.
///
/// Handles price rounding, validation (price in [0.01, 0.99] range per API docs),
/// and optional market order price calculation from a local order book.
///
/// Amounts can be specified as quote token (USDT budget) or base token (outcome token quantity).
#[derive(Debug, Clone)]
pub struct OrderBuilder {
    market_id: i64,
    token_id: String,
    side: Side,
    price: Option<f64>,
    amount_quote: Option<f64>,
    amount_base: Option<f64>,
    tick_size: TickSize,
    chain_id: Option<String>,
    max_slippage: Option<f64>,
}

impl OrderBuilder {
    /// Create a new order builder.
    ///
    /// Specify the amount via `.amount_in_quote_token()` (USDT budget)
    /// or `.amount_in_base_token()` (outcome token quantity).
    pub fn new(market_id: i64, token_id: impl Into<String>, side: Side) -> Self {
        Self {
            market_id,
            token_id: token_id.into(),
            side,
            price: None,
            amount_quote: None,
            amount_base: None,
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

    /// Set the amount in quote token (USDT).
    /// The number of outcome tokens received = amount / price.
    pub fn amount_in_quote_token(mut self, amount: f64) -> Self {
        self.amount_quote = Some(amount);
        self.amount_base = None;
        self
    }

    /// Set the amount in base token (outcome token quantity).
    /// The USDT cost = amount * price.
    pub fn amount_in_base_token(mut self, amount: f64) -> Self {
        self.amount_base = Some(amount);
        self.amount_quote = None;
        self
    }

    /// Set the tick size (default: Hundredths / 0.01).
    pub fn tick_size(mut self, tick_size: TickSize) -> Self {
        self.tick_size = tick_size;
        self
    }

    /// Set the chain ID (default: 56 for BNB Chain).
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

        self.validate_amount()?;
        self.validate_price(raw_price)?;

        let rounded = round_price(raw_price, self.tick_size, self.side);
        self.validate_price(rounded)?;

        Ok(CreateOrderRequest {
            market_id: self.market_id,
            token_id: self.token_id,
            side: self.side,
            order_type: OrderType::Limit,
            price: format_price(rounded, self.tick_size),
            maker_amount_in_quote_token: self.amount_quote.map(|a| format!("{a}")),
            maker_amount_in_base_token: self.amount_base.map(|a| format!("{a}")),
            chain_id: self.chain_id,
        })
    }

    /// Build a market order by walking the order book to determine execution price.
    ///
    /// For market orders the price is set to "0" (ignored by server) and the order
    /// type is `Market`. The amount must be specified.
    pub fn build_market_order(self, book: &LocalOrderBook) -> Result<CreateOrderRequest> {
        self.validate_amount()?;
        let size = self.effective_size(0.5)?; // dummy price for validation

        let market_price = book
            .calculate_market_price(self.side, size)
            .ok_or_else(|| SdkError::Validation("insufficient liquidity in order book".into()))?;

        // Apply slippage buffer for the limit price ceiling/floor
        let price_with_slippage = if let Some(slippage) = self.max_slippage {
            match self.side {
                Side::Buy => market_price * (1.0 + slippage),
                Side::Sell => market_price * (1.0 - slippage),
            }
        } else {
            market_price
        };

        let rounded = round_price(price_with_slippage, self.tick_size, self.side);
        let clamped = rounded.clamp(MIN_PRICE, MAX_PRICE);

        Ok(CreateOrderRequest {
            market_id: self.market_id,
            token_id: self.token_id,
            side: self.side,
            order_type: OrderType::Market,
            price: format_price(clamped, self.tick_size),
            maker_amount_in_quote_token: self.amount_quote.map(|a| format!("{a}")),
            maker_amount_in_base_token: self.amount_base.map(|a| format!("{a}")),
            chain_id: self.chain_id,
        })
    }

    fn validate_amount(&self) -> Result<()> {
        if self.amount_quote.is_none() && self.amount_base.is_none() {
            return Err(SdkError::Validation(
                "must specify amount via amount_in_quote_token() or amount_in_base_token()".into(),
            ));
        }
        if let Some(a) = self.amount_quote
            && a <= 0.0
        {
            return Err(SdkError::Validation("quote amount must be positive".into()));
        }
        if let Some(a) = self.amount_base
            && a <= 0.0
        {
            return Err(SdkError::Validation("base amount must be positive".into()));
        }
        Ok(())
    }

    fn validate_price(&self, price: f64) -> Result<()> {
        if !(MIN_PRICE..=MAX_PRICE).contains(&price) {
            return Err(SdkError::Validation(format!(
                "price {price} must be between {MIN_PRICE} and {MAX_PRICE} (inclusive)"
            )));
        }
        Ok(())
    }

    /// Get effective size in base tokens for market price calculation.
    fn effective_size(&self, price_estimate: f64) -> Result<f64> {
        if let Some(base) = self.amount_base {
            Ok(base)
        } else if let Some(quote) = self.amount_quote {
            if price_estimate <= 0.0 {
                return Err(SdkError::Validation(
                    "cannot calculate size without a valid price".into(),
                ));
            }
            Ok(quote / price_estimate)
        } else {
            Err(SdkError::Validation("no amount specified".into()))
        }
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
    fn order_builder_limit_order_quote() {
        let req = OrderBuilder::new(42, "tok_1", Side::Buy)
            .price(0.556)
            .amount_in_quote_token(100.0)
            .build()
            .unwrap();
        assert_eq!(req.market_id, 42);
        assert_eq!(req.token_id, "tok_1");
        assert_eq!(req.side, Side::Buy);
        assert_eq!(req.order_type, OrderType::Limit);
        assert_eq!(req.price, "0.55"); // rounded down for buy
        assert_eq!(req.maker_amount_in_quote_token.as_deref(), Some("100"));
        assert!(req.maker_amount_in_base_token.is_none());
    }

    #[test]
    fn order_builder_limit_order_base() {
        let req = OrderBuilder::new(42, "tok_1", Side::Sell)
            .price(0.551)
            .amount_in_base_token(50.0)
            .build()
            .unwrap();
        assert_eq!(req.price, "0.56"); // rounded up for sell
        assert_eq!(req.order_type, OrderType::Limit);
        assert!(req.maker_amount_in_quote_token.is_none());
        assert_eq!(req.maker_amount_in_base_token.as_deref(), Some("50"));
    }

    #[test]
    fn order_builder_custom_tick_size() {
        let req = OrderBuilder::new(42, "tok_1", Side::Buy)
            .price(0.5556)
            .amount_in_quote_token(100.0)
            .tick_size(TickSize::Thousandths)
            .build()
            .unwrap();
        assert_eq!(req.price, "0.555");
    }

    #[test]
    fn order_builder_with_chain_id() {
        let req = OrderBuilder::new(42, "tok_1", Side::Buy)
            .price(0.55)
            .amount_in_quote_token(100.0)
            .chain_id("56")
            .build()
            .unwrap();
        assert_eq!(req.chain_id.as_deref(), Some("56"));
    }

    #[test]
    fn order_builder_rejects_no_amount() {
        let result = OrderBuilder::new(42, "tok_1", Side::Buy)
            .price(0.55)
            .build();
        assert!(matches!(result.unwrap_err(), SdkError::Validation(_)));
    }

    #[test]
    fn order_builder_rejects_zero_amount() {
        let result = OrderBuilder::new(42, "tok_1", Side::Buy)
            .price(0.55)
            .amount_in_quote_token(0.0)
            .build();
        assert!(matches!(result.unwrap_err(), SdkError::Validation(_)));
    }

    #[test]
    fn order_builder_rejects_price_below_001() {
        let result = OrderBuilder::new(42, "tok_1", Side::Buy)
            .price(0.005)
            .amount_in_quote_token(100.0)
            .build();
        assert!(matches!(result.unwrap_err(), SdkError::Validation(_)));
    }

    #[test]
    fn order_builder_rejects_price_above_099() {
        let result = OrderBuilder::new(42, "tok_1", Side::Buy)
            .price(0.995)
            .amount_in_quote_token(100.0)
            .build();
        assert!(matches!(result.unwrap_err(), SdkError::Validation(_)));
    }

    #[test]
    fn order_builder_accepts_price_001() {
        let req = OrderBuilder::new(42, "tok_1", Side::Buy)
            .price(0.01)
            .amount_in_quote_token(100.0)
            .build()
            .unwrap();
        assert_eq!(req.price, "0.01");
    }

    #[test]
    fn order_builder_accepts_price_099() {
        let req = OrderBuilder::new(42, "tok_1", Side::Sell)
            .price(0.99)
            .amount_in_base_token(100.0)
            .build()
            .unwrap();
        assert_eq!(req.price, "0.99");
    }

    #[test]
    fn order_builder_no_price_errors() {
        let result = OrderBuilder::new(42, "tok_1", Side::Buy)
            .amount_in_quote_token(100.0)
            .build();
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
        let req = OrderBuilder::new(42, "tok_1", Side::Buy)
            .amount_in_base_token(50.0)
            .build_market_order(&book)
            .unwrap();
        assert_eq!(req.side, Side::Buy);
        assert_eq!(req.order_type, OrderType::Market);
        assert_eq!(req.price, "0.55");
    }

    #[test]
    fn order_builder_market_buy_crossing_levels() {
        let book = make_book();
        let req = OrderBuilder::new(42, "tok_1", Side::Buy)
            .amount_in_base_token(150.0)
            .build_market_order(&book)
            .unwrap();
        // avg = (100*0.55 + 50*0.58) / 150 = 0.56
        assert_eq!(req.price, "0.56");
    }

    #[test]
    fn order_builder_market_sell() {
        let book = make_book();
        let req = OrderBuilder::new(42, "tok_1", Side::Sell)
            .amount_in_base_token(50.0)
            .build_market_order(&book)
            .unwrap();
        assert_eq!(req.side, Side::Sell);
        assert_eq!(req.price, "0.50");
    }

    #[test]
    fn order_builder_market_order_with_slippage() {
        let book = make_book();
        let req = OrderBuilder::new(42, "tok_1", Side::Buy)
            .amount_in_base_token(50.0)
            .max_slippage(0.02)
            .build_market_order(&book)
            .unwrap();
        // 0.55 * 1.02 = 0.561 -> rounded down to 0.56
        assert_eq!(req.price, "0.56");
    }

    #[test]
    fn order_builder_market_order_insufficient_liquidity() {
        let book = make_book();
        let result = OrderBuilder::new(42, "tok_1", Side::Buy)
            .amount_in_base_token(10000.0)
            .build_market_order(&book);
        assert!(matches!(result.unwrap_err(), SdkError::Validation(_)));
    }
}
