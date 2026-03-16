use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Side of an order or trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
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
#[derive(Debug, Clone, Serialize)]
pub struct CreateOrderRequest {
    #[serde(rename = "tokenId")]
    pub token_id: String,
    pub side: Side,
    pub price: String,
    pub size: String,
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
    #[serde(rename = "chainId", skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<String>,
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
    fn serialize_create_order_request() {
        let req = CreateOrderRequest {
            token_id: "tok_1".into(),
            side: Side::Buy,
            price: "0.55".into(),
            size: "100".into(),
            chain_id: Some("137".into()),
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["tokenId"], "tok_1");
        assert_eq!(v["side"], "buy");
        assert_eq!(v["price"], "0.55");
        assert_eq!(v["size"], "100");
        assert_eq!(v["chainId"], "137");
    }

    #[test]
    fn serialize_create_order_no_chain() {
        let req = CreateOrderRequest {
            token_id: "tok_1".into(),
            side: Side::Sell,
            price: "0.45".into(),
            size: "50".into(),
            chain_id: None,
        };
        let v = serde_json::to_value(&req).unwrap();
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
            chain_id: None,
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["marketId"], 42);
        assert!(v.get("chainId").is_none());
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
