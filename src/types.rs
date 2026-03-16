use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Side of an order or trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

/// Type of order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OrderType {
    Market = 1,
    Limit = 2,
}

impl Serialize for OrderType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for OrderType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = u8::deserialize(deserializer)?;
        match v {
            1 => Ok(Self::Market),
            2 => Ok(Self::Limit),
            _ => Err(serde::de::Error::custom(format!("unknown order type: {v}"))),
        }
    }
}

/// Type of market/topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TopicType {
    Binary = 0,
    Categorical = 1,
}

impl Serialize for TopicType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for TopicType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = u8::deserialize(deserializer)?;
        match v {
            0 => Ok(Self::Binary),
            1 => Ok(Self::Categorical),
            _ => Err(serde::de::Error::custom(format!("unknown topic type: {v}"))),
        }
    }
}

/// Status of a market.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketStatus {
    Activated,
    Paused,
    Closed,
    Settled,
}

/// Status of an order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Open,
    Filled,
    Cancelled,
    #[serde(rename = "partial_filled")]
    PartialFilled,
}

/// A user's position in a market token.
#[derive(Debug, Clone, Deserialize)]
pub struct Position {
    #[serde(rename = "tokenId")]
    pub token_id: Option<String>,
    #[serde(rename = "marketId")]
    pub market_id: Option<i64>,
    pub size: Option<String>,
    #[serde(rename = "avgPrice")]
    pub avg_price: Option<String>,
    #[serde(flatten)]
    pub extra: Value,
}

/// Request body for creating an order.
///
/// Use either `maker_amount_in_quote_token` (USDT budget) or
/// `maker_amount_in_base_token` (outcome token quantity), not both.
#[derive(Debug, Clone, Serialize)]
pub struct CreateOrderRequest {
    #[serde(rename = "marketId")]
    pub market_id: i64,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    pub side: Side,
    #[serde(rename = "orderType")]
    pub order_type: OrderType,
    pub price: String,
    #[serde(
        rename = "makerAmountInQuoteToken",
        skip_serializing_if = "Option::is_none"
    )]
    pub maker_amount_in_quote_token: Option<String>,
    #[serde(
        rename = "makerAmountInBaseToken",
        skip_serializing_if = "Option::is_none"
    )]
    pub maker_amount_in_base_token: Option<String>,
    #[serde(rename = "chainId", skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<String>,
}

/// Request body for cancelling a single order.
#[derive(Debug, Clone, Serialize)]
pub struct CancelOrderRequest {
    #[serde(rename = "orderId")]
    pub order_id: String,
}

/// Request body for cancelling all orders.
#[derive(Debug, Clone, Serialize)]
pub struct CancelAllOrdersRequest {
    #[serde(rename = "marketId", skip_serializing_if = "Option::is_none")]
    pub market_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    #[serde(rename = "chainId", skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<String>,
}

/// Request body for batch cancelling orders by IDs.
#[derive(Debug, Clone, Serialize)]
pub struct CancelOrdersBatchRequest {
    #[serde(rename = "orderIds")]
    pub order_ids: Vec<String>,
}

/// Query parameters for fetching global trades.
#[derive(Debug, Clone, Serialize, Default)]
pub struct GlobalTradesQuery {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    #[serde(rename = "marketId")]
    pub market_id: Option<i64>,
    #[serde(rename = "chainId")]
    pub chain_id: Option<String>,
}

/// Query parameters for fetching user positions.
#[derive(Debug, Clone, Serialize, Default)]
pub struct PositionsQuery {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    #[serde(rename = "marketId")]
    pub market_id: Option<i64>,
    #[serde(rename = "chainId")]
    pub chain_id: Option<String>,
}

/// Query parameters for fetching the authenticated user's trades.
#[derive(Debug, Clone, Serialize, Default)]
pub struct MyTradesQuery {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    #[serde(rename = "marketId")]
    pub market_id: Option<i64>,
}

/// User balance information returned by `/user/balance`.
#[derive(Debug, Clone, Deserialize)]
pub struct Balances {
    #[serde(rename = "walletAddress")]
    pub wallet_address: Option<String>,
    #[serde(rename = "multiSignAddress")]
    pub multi_sign_address: Option<String>,
    #[serde(rename = "chainId")]
    pub chain_id: Option<String>,
    #[serde(default)]
    pub balances: Vec<Balance>,
}

/// A single token balance entry.
#[derive(Debug, Clone, Deserialize)]
pub struct Balance {
    #[serde(rename = "quoteToken")]
    pub quote_token: Option<String>,
    #[serde(rename = "tokenDecimals")]
    pub token_decimals: Option<u32>,
    #[serde(rename = "totalBalance")]
    pub total_balance: Option<String>,
    #[serde(rename = "availableBalance")]
    pub available_balance: Option<String>,
    #[serde(rename = "frozenBalance")]
    pub frozen_balance: Option<String>,
    #[serde(flatten)]
    pub extra: Value,
}

/// Fee rate information for a token.
#[derive(Debug, Clone, Deserialize)]
pub struct FeeRates {
    #[serde(flatten)]
    pub extra: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn side_serialize_buy() {
        assert_eq!(serde_json::to_value(Side::Buy).unwrap(), json!("buy"));
    }

    #[test]
    fn side_serialize_sell() {
        assert_eq!(serde_json::to_value(Side::Sell).unwrap(), json!("sell"));
    }

    #[test]
    fn side_deserialize() {
        let buy: Side = serde_json::from_value(json!("buy")).unwrap();
        assert_eq!(buy, Side::Buy);
        let sell: Side = serde_json::from_value(json!("sell")).unwrap();
        assert_eq!(sell, Side::Sell);
    }

    #[test]
    fn market_status_roundtrip() {
        for (status, expected) in [
            (MarketStatus::Activated, "activated"),
            (MarketStatus::Paused, "paused"),
            (MarketStatus::Closed, "closed"),
            (MarketStatus::Settled, "settled"),
        ] {
            let v = serde_json::to_value(status).unwrap();
            assert_eq!(v, json!(expected));
            let back: MarketStatus = serde_json::from_value(v).unwrap();
            assert_eq!(back, status);
        }
    }

    #[test]
    fn order_status_roundtrip() {
        for (status, expected) in [
            (OrderStatus::Open, "open"),
            (OrderStatus::Filled, "filled"),
            (OrderStatus::Cancelled, "cancelled"),
            (OrderStatus::PartialFilled, "partial_filled"),
        ] {
            let v = serde_json::to_value(status).unwrap();
            assert_eq!(v, json!(expected));
            let back: OrderStatus = serde_json::from_value(v).unwrap();
            assert_eq!(back, status);
        }
    }

    #[test]
    fn deserialize_position() {
        let json = json!({
            "tokenId": "tok_1",
            "marketId": 42,
            "size": "100.5",
            "avgPrice": "0.65",
            "unrealizedPnl": "5.00"
        });
        let pos: Position = serde_json::from_value(json).unwrap();
        assert_eq!(pos.token_id.as_deref(), Some("tok_1"));
        assert_eq!(pos.market_id, Some(42));
        assert_eq!(pos.size.as_deref(), Some("100.5"));
        assert_eq!(pos.avg_price.as_deref(), Some("0.65"));
        assert_eq!(pos.extra["unrealizedPnl"], "5.00");
    }

    #[test]
    fn deserialize_position_missing_fields() {
        let json = json!({});
        let pos: Position = serde_json::from_value(json).unwrap();
        assert!(pos.token_id.is_none());
        assert!(pos.market_id.is_none());
    }

    #[test]
    fn serialize_create_order_request_quote() {
        let req = CreateOrderRequest {
            market_id: 42,
            token_id: "tok_1".into(),
            side: Side::Buy,
            order_type: OrderType::Limit,
            price: "0.55".into(),
            maker_amount_in_quote_token: Some("100".into()),
            maker_amount_in_base_token: None,
            chain_id: Some("56".into()),
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["marketId"], 42);
        assert_eq!(v["tokenId"], "tok_1");
        assert_eq!(v["side"], "buy");
        assert_eq!(v["orderType"], 2);
        assert_eq!(v["price"], "0.55");
        assert_eq!(v["makerAmountInQuoteToken"], "100");
        assert!(v.get("makerAmountInBaseToken").is_none());
        assert_eq!(v["chainId"], "56");
    }

    #[test]
    fn serialize_create_order_request_base() {
        let req = CreateOrderRequest {
            market_id: 42,
            token_id: "tok_1".into(),
            side: Side::Sell,
            order_type: OrderType::Market,
            price: "0".into(),
            maker_amount_in_quote_token: None,
            maker_amount_in_base_token: Some("50".into()),
            chain_id: None,
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["orderType"], 1);
        assert_eq!(v["makerAmountInBaseToken"], "50");
        assert!(v.get("makerAmountInQuoteToken").is_none());
        assert!(v.get("chainId").is_none());
    }

    #[test]
    fn serialize_cancel_order_request() {
        let req = CancelOrderRequest {
            order_id: "ord_123".into(),
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["orderId"], "ord_123");
    }

    #[test]
    fn serialize_cancel_all_orders_request() {
        let req = CancelAllOrdersRequest {
            market_id: Some(42),
            side: Some(Side::Buy),
            chain_id: None,
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["marketId"], 42);
        assert_eq!(v["side"], "buy");
        assert!(v.get("chainId").is_none());
    }

    #[test]
    fn serialize_cancel_orders_batch() {
        let req = CancelOrdersBatchRequest {
            order_ids: vec!["ord_1".into(), "ord_2".into()],
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["orderIds"], json!(["ord_1", "ord_2"]));
    }

    #[test]
    fn order_type_serde() {
        assert_eq!(serde_json::to_value(OrderType::Market).unwrap(), json!(1));
        assert_eq!(serde_json::to_value(OrderType::Limit).unwrap(), json!(2));
        let m: OrderType = serde_json::from_value(json!(1)).unwrap();
        assert_eq!(m, OrderType::Market);
        let l: OrderType = serde_json::from_value(json!(2)).unwrap();
        assert_eq!(l, OrderType::Limit);
    }

    #[test]
    fn topic_type_serde() {
        assert_eq!(serde_json::to_value(TopicType::Binary).unwrap(), json!(0));
        assert_eq!(
            serde_json::to_value(TopicType::Categorical).unwrap(),
            json!(1)
        );
        let b: TopicType = serde_json::from_value(json!(0)).unwrap();
        assert_eq!(b, TopicType::Binary);
    }

    #[test]
    fn serialize_global_trades_query() {
        let q = GlobalTradesQuery {
            page: Some(1),
            limit: Some(20),
            market_id: Some(5),
            chain_id: Some("56".into()),
        };
        let v = serde_json::to_value(&q).unwrap();
        assert_eq!(v["page"], 1);
        assert_eq!(v["limit"], 20);
        assert_eq!(v["marketId"], 5);
        assert_eq!(v["chainId"], "56");
    }

    #[test]
    fn serialize_positions_query() {
        let q = PositionsQuery {
            page: Some(1),
            limit: Some(10),
            market_id: None,
            chain_id: None,
        };
        let v = serde_json::to_value(&q).unwrap();
        assert_eq!(v["page"], 1);
        assert_eq!(v["limit"], 10);
    }
}
