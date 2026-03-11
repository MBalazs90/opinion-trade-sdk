use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct ApiEnvelope<T> {
    pub code: i64,
    pub msg: String,
    pub result: T,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PagedList<T> {
    pub total: i64,
    pub list: Vec<T>,
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
    #[serde(rename = "quoteToken")]
    pub quote_token: Option<String>,
    #[serde(flatten)]
    pub extra: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LatestPrice {
    #[serde(rename = "tokenId")]
    pub token_id: Option<String>,
    pub price: Option<String>,
    pub side: Option<String>,
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
