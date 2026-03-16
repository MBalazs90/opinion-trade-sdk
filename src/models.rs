use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct ApiEnvelope<T> {
    #[serde(alias = "code")]
    pub errno: i64,
    #[serde(alias = "msg")]
    pub errmsg: String,
    pub result: Option<T>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(bound(deserialize = "T: DeserializeOwned"))]
pub struct PagedList<T> {
    pub total: i64,
    #[serde(deserialize_with = "deserialize_null_as_empty")]
    pub list: Vec<T>,
}

fn deserialize_null_as_empty<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: DeserializeOwned,
{
    Option::<Vec<T>>::deserialize(deserializer).map(|v| v.unwrap_or_default())
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataResult<T> {
    pub data: T,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Market {
    #[serde(rename = "marketId")]
    pub market_id: Option<i64>,
    #[serde(rename = "marketTitle")]
    pub market_title: Option<String>,
    #[serde(rename = "statusEnum")]
    pub status_enum: Option<String>,
    #[serde(flatten)]
    pub extra: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuoteToken {
    pub id: Option<i64>,
    #[serde(rename = "quoteTokenName")]
    pub name: Option<String>,
    #[serde(rename = "quoteTokenAddress")]
    pub address: Option<String>,
    pub symbol: Option<String>,
    pub decimal: Option<u32>,
    #[serde(rename = "chainId")]
    pub chain_id: Option<String>,
    #[serde(flatten)]
    pub extra: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LatestPrice {
    #[serde(rename = "tokenId")]
    pub token_id: Option<String>,
    pub price: Option<String>,
    pub side: Option<String>,
    pub size: Option<String>,
    pub timestamp: Option<i64>,
    #[serde(flatten)]
    pub extra: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrderBookLevel {
    pub price: String,
    pub size: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrderBook {
    pub market: Option<String>,
    #[serde(rename = "tokenId")]
    pub token_id: Option<String>,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
    pub timestamp: Option<i64>,
    #[serde(flatten)]
    pub extra: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Order {
    #[serde(rename = "orderId")]
    pub order_id: Option<String>,
    #[serde(rename = "marketId")]
    pub market_id: Option<i64>,
    #[serde(rename = "statusEnum")]
    pub status_enum: Option<String>,
    pub price: Option<String>,
    #[serde(flatten)]
    pub extra: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Trade {
    #[serde(rename = "txHash")]
    pub tx_hash: Option<String>,
    #[serde(rename = "marketId")]
    pub market_id: Option<i64>,
    pub side: Option<String>,
    pub price: Option<String>,
    #[serde(flatten)]
    pub extra: Value,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct MarketQuery {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub status: Option<String>,
    #[serde(rename = "chainId")]
    pub chain_id: Option<String>,
    #[serde(rename = "sortBy")]
    pub sort_by: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct OrderQuery {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    #[serde(rename = "marketId")]
    pub market_id: Option<i64>,
    #[serde(rename = "chainId")]
    pub chain_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct UserTradesQuery {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    #[serde(rename = "marketId")]
    pub market_id: Option<i64>,
    #[serde(rename = "chainId")]
    pub chain_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PriceHistoryQuery {
    #[serde(rename = "token_id")]
    pub token_id: String,
    pub interval: Option<String>,
    #[serde(rename = "start_at")]
    pub start_at: Option<i64>,
    #[serde(rename = "end_at")]
    pub end_at: Option<i64>,
}

/// A price history response containing time-series data points.
#[derive(Debug, Clone, Deserialize)]
pub struct PriceHistory {
    pub history: Vec<PriceHistoryPoint>,
}

/// A single price point in a price history time-series.
#[derive(Debug, Clone, Deserialize)]
pub struct PriceHistoryPoint {
    /// Price as a string (e.g. "0.52").
    #[serde(rename = "p")]
    pub price: String,
    /// Unix timestamp.
    #[serde(rename = "t")]
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserialize_api_envelope_errno_format() {
        let json = json!({
            "errno": 0,
            "errmsg": "",
            "result": { "total": 1, "list": [] }
        });
        let envelope: ApiEnvelope<PagedList<Market>> = serde_json::from_value(json).unwrap();
        assert_eq!(envelope.errno, 0);
        assert_eq!(envelope.errmsg, "");
        let result = envelope.result.unwrap();
        assert_eq!(result.total, 1);
        assert!(result.list.is_empty());
    }

    #[test]
    fn deserialize_api_envelope_code_alias() {
        let json = json!({
            "code": 0,
            "msg": "success",
            "result": { "total": 0, "list": [] }
        });
        let envelope: ApiEnvelope<PagedList<Market>> = serde_json::from_value(json).unwrap();
        assert_eq!(envelope.errno, 0);
        assert_eq!(envelope.errmsg, "success");
        assert!(envelope.result.is_some());
    }

    #[test]
    fn deserialize_api_envelope_null_result() {
        let json = json!({
            "errno": 10607,
            "errmsg": "order not found",
            "result": null
        });
        let envelope: ApiEnvelope<DataResult<Order>> = serde_json::from_value(json).unwrap();
        assert_eq!(envelope.errno, 10607);
        assert!(envelope.result.is_none());
    }

    #[test]
    fn deserialize_paged_list_null_list() {
        let json = json!({ "total": 0, "list": null });
        let paged: PagedList<Order> = serde_json::from_value(json).unwrap();
        assert_eq!(paged.total, 0);
        assert!(paged.list.is_empty());
    }

    #[test]
    fn deserialize_market() {
        let json = json!({
            "marketId": 42,
            "marketTitle": "Will it rain?",
            "statusEnum": "activated",
            "someExtra": "field"
        });
        let market: Market = serde_json::from_value(json).unwrap();
        assert_eq!(market.market_id, Some(42));
        assert_eq!(market.market_title.as_deref(), Some("Will it rain?"));
        assert_eq!(market.status_enum.as_deref(), Some("activated"));
        assert_eq!(market.extra["someExtra"], "field");
    }

    #[test]
    fn deserialize_market_missing_optional_fields() {
        let json = json!({});
        let market: Market = serde_json::from_value(json).unwrap();
        assert_eq!(market.market_id, None);
        assert_eq!(market.market_title, None);
        assert_eq!(market.status_enum, None);
    }

    #[test]
    fn deserialize_quote_token() {
        let json = json!({
            "id": 4,
            "quoteTokenName": "USDT",
            "quoteTokenAddress": "0x55d3",
            "symbol": "USDT",
            "decimal": 18,
            "chainId": "56"
        });
        let qt: QuoteToken = serde_json::from_value(json).unwrap();
        assert_eq!(qt.id, Some(4));
        assert_eq!(qt.name.as_deref(), Some("USDT"));
        assert_eq!(qt.address.as_deref(), Some("0x55d3"));
        assert_eq!(qt.symbol.as_deref(), Some("USDT"));
        assert_eq!(qt.decimal, Some(18));
        assert_eq!(qt.chain_id.as_deref(), Some("56"));
    }

    #[test]
    fn deserialize_latest_price() {
        let json = json!({
            "tokenId": "tok_1",
            "price": "0.55",
            "side": "buy",
            "size": "100",
            "timestamp": 1700000000
        });
        let lp: LatestPrice = serde_json::from_value(json).unwrap();
        assert_eq!(lp.token_id.as_deref(), Some("tok_1"));
        assert_eq!(lp.price.as_deref(), Some("0.55"));
        assert_eq!(lp.side.as_deref(), Some("buy"));
        assert_eq!(lp.size.as_deref(), Some("100"));
        assert_eq!(lp.timestamp, Some(1700000000));
    }

    #[test]
    fn deserialize_orderbook() {
        let json = json!({
            "market": "abc123",
            "tokenId": "tok_1",
            "bids": [{ "price": "0.50", "size": "100" }],
            "asks": [{ "price": "0.55", "size": "200" }],
            "timestamp": 1700000000
        });
        let ob: OrderBook = serde_json::from_value(json).unwrap();
        assert_eq!(ob.market.as_deref(), Some("abc123"));
        assert_eq!(ob.token_id.as_deref(), Some("tok_1"));
        assert_eq!(ob.bids.len(), 1);
        assert_eq!(ob.bids[0].price, "0.50");
        assert_eq!(ob.bids[0].size, "100");
        assert_eq!(ob.asks.len(), 1);
        assert_eq!(ob.asks[0].price, "0.55");
        assert_eq!(ob.asks[0].size, "200");
    }

    #[test]
    fn deserialize_order() {
        let json = json!({
            "orderId": "ord_1",
            "marketId": 10,
            "statusEnum": "filled",
            "price": "0.75",
            "quantity": "50"
        });
        let order: Order = serde_json::from_value(json).unwrap();
        assert_eq!(order.order_id.as_deref(), Some("ord_1"));
        assert_eq!(order.market_id, Some(10));
        assert_eq!(order.status_enum.as_deref(), Some("filled"));
        assert_eq!(order.price.as_deref(), Some("0.75"));
        assert_eq!(order.extra["quantity"], "50");
    }

    #[test]
    fn deserialize_trade() {
        let json = json!({
            "txHash": "0xabc",
            "marketId": 5,
            "side": "sell",
            "price": "0.30",
            "amount": "10"
        });
        let trade: Trade = serde_json::from_value(json).unwrap();
        assert_eq!(trade.tx_hash.as_deref(), Some("0xabc"));
        assert_eq!(trade.market_id, Some(5));
        assert_eq!(trade.side.as_deref(), Some("sell"));
        assert_eq!(trade.price.as_deref(), Some("0.30"));
        assert_eq!(trade.extra["amount"], "10");
    }

    #[test]
    fn deserialize_data_result() {
        let json = json!({ "data": { "orderId": "ord_1" } });
        let dr: DataResult<Order> = serde_json::from_value(json).unwrap();
        assert_eq!(dr.data.order_id.as_deref(), Some("ord_1"));
    }

    #[test]
    fn serialize_market_query_defaults() {
        let q = MarketQuery::default();
        let v = serde_json::to_value(&q).unwrap();
        // All None fields should serialize as null
        assert!(v["page"].is_null());
        assert!(v["limit"].is_null());
    }

    #[test]
    fn serialize_market_query_with_values() {
        let q = MarketQuery {
            page: Some(2),
            limit: Some(25),
            status: Some("activated".into()),
            chain_id: Some("137".into()),
            sort_by: Some(1),
        };
        let v = serde_json::to_value(&q).unwrap();
        assert_eq!(v["page"], 2);
        assert_eq!(v["limit"], 25);
        assert_eq!(v["status"], "activated");
        assert_eq!(v["chainId"], "137");
        assert_eq!(v["sortBy"], 1);
    }

    #[test]
    fn serialize_order_query_renames() {
        let q = OrderQuery {
            market_id: Some(42),
            chain_id: Some("1".into()),
            ..Default::default()
        };
        let v = serde_json::to_value(&q).unwrap();
        assert_eq!(v["marketId"], 42);
        assert_eq!(v["chainId"], "1");
    }

    #[test]
    fn serialize_user_trades_query_renames() {
        let q = UserTradesQuery {
            market_id: Some(7),
            chain_id: Some("80001".into()),
            ..Default::default()
        };
        let v = serde_json::to_value(&q).unwrap();
        assert_eq!(v["marketId"], 7);
        assert_eq!(v["chainId"], "80001");
    }

    #[test]
    fn serialize_price_history_query_renames() {
        let q = PriceHistoryQuery {
            token_id: "tok_1".into(),
            interval: Some("1h".into()),
            start_at: Some(1000),
            end_at: Some(2000),
        };
        let v = serde_json::to_value(&q).unwrap();
        assert_eq!(v["token_id"], "tok_1");
        assert_eq!(v["interval"], "1h");
        assert_eq!(v["start_at"], 1000);
        assert_eq!(v["end_at"], 2000);
    }

    #[test]
    fn deserialize_price_history() {
        let json = json!({
            "history": [
                {"p": "0.52", "t": 1773658800},
                {"p": "0.55", "t": 1773655200}
            ]
        });
        let ph: PriceHistory = serde_json::from_value(json).unwrap();
        assert_eq!(ph.history.len(), 2);
        assert_eq!(ph.history[0].price, "0.52");
        assert_eq!(ph.history[0].timestamp, 1773658800);
        assert_eq!(ph.history[1].price, "0.55");
    }

    #[test]
    fn deserialize_price_history_empty() {
        let json = json!({"history": []});
        let ph: PriceHistory = serde_json::from_value(json).unwrap();
        assert!(ph.history.is_empty());
    }
}
