use reqwest::{Client as HttpClient, Method, RequestBuilder};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{Result, SdkError};
use crate::models::{
    ApiEnvelope, DataResult, LatestPrice, Market, MarketQuery, Order, OrderBook, OrderQuery,
    PagedList, PriceHistoryQuery, QuoteToken, Trade, UserTradesQuery,
};

const DEFAULT_OPENAPI_BASE: &str = "https://openapi.opinion.trade/openapi";

#[derive(Debug, Clone)]
pub struct OpinionClientBuilder {
    base_url: String,
    api_key: Option<String>,
    timeout_secs: u64,
}

impl Default for OpinionClientBuilder {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_OPENAPI_BASE.to_string(),
            api_key: None,
            timeout_secs: 15,
        }
    }
}

impl OpinionClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    pub fn build(self) -> Result<OpinionClient> {
        let http = HttpClient::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .build()?;

        Ok(OpinionClient {
            http,
            base_url: self.base_url.trim_end_matches('/').to_string(),
            api_key: self.api_key,
        })
    }
}

#[derive(Debug, Clone)]
pub struct OpinionClient {
    http: HttpClient,
    base_url: String,
    api_key: Option<String>,
}

impl OpinionClient {
    pub fn builder() -> OpinionClientBuilder {
        OpinionClientBuilder::default()
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn get_markets(&self, query: &MarketQuery) -> Result<PagedList<Market>> {
        self.get("/market", Some(query)).await
    }

    pub async fn get_market(&self, market_id: i64) -> Result<Market> {
        self.get(&format!("/market/{market_id}"), Option::<&()>::None)
            .await
    }

    pub async fn get_quote_tokens(&self) -> Result<PagedList<QuoteToken>> {
        self.get("/quoteToken", Option::<&()>::None).await
    }

    pub async fn get_latest_price(&self, token_id: &str) -> Result<LatestPrice> {
        #[derive(Serialize)]
        struct Query<'a> {
            token_id: &'a str,
        }
        self.get("/token/latest-price", Some(&Query { token_id }))
            .await
    }

    pub async fn get_orderbook(&self, token_id: &str) -> Result<OrderBook> {
        #[derive(Serialize)]
        struct Query<'a> {
            token_id: &'a str,
        }
        self.get("/token/orderbook", Some(&Query { token_id }))
            .await
    }

    pub async fn get_price_history(&self, query: &PriceHistoryQuery) -> Result<Value> {
        self.get("/token/price-history", Some(query)).await
    }

    pub async fn get_user_trades(
        &self,
        wallet_address: &str,
        query: &UserTradesQuery,
    ) -> Result<PagedList<Trade>> {
        self.get(&format!("/trade/user/{wallet_address}"), Some(query))
            .await
    }

    pub async fn get_orders(&self, query: &OrderQuery) -> Result<PagedList<Order>> {
        self.get_auth("/order", Some(query)).await
    }

    pub async fn get_order_detail(&self, order_id: &str) -> Result<DataResult<Order>> {
        self.get_auth(&format!("/order/{order_id}"), Option::<&()>::None)
            .await
    }

    async fn get<T, Q>(&self, path: &str, query: Option<&Q>) -> Result<T>
    where
        T: DeserializeOwned,
        Q: Serialize + ?Sized,
    {
        let request = self.build_request(Method::GET, path, query, false)?;
        self.send_api_envelope(request).await
    }

    async fn get_auth<T, Q>(&self, path: &str, query: Option<&Q>) -> Result<T>
    where
        T: DeserializeOwned,
        Q: Serialize + ?Sized,
    {
        let request = self.build_request(Method::GET, path, query, true)?;
        self.send_api_envelope(request).await
    }

    fn build_request<Q>(
        &self,
        method: Method,
        path: &str,
        query: Option<&Q>,
        requires_api_key: bool,
    ) -> Result<reqwest::Request>
    where
        Q: Serialize + ?Sized,
    {
        let mut rb: RequestBuilder = self
            .http
            .request(method, format!("{}{}", self.base_url, path));

        if let Some(query) = query {
            rb = rb.query(query);
        }

        if let Some(api_key) = &self.api_key {
            rb = rb.header("apikey", api_key);
        } else if requires_api_key {
            return Err(SdkError::MissingApiKey);
        }

        Ok(rb.build()?)
    }

    async fn send_api_envelope<T>(&self, request: reqwest::Request) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let response = self.http.execute(request).await?;
        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(SdkError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let envelope: ApiEnvelope<T> = serde_json::from_str(&body)?;

        if envelope.code == 0 {
            Ok(envelope.result)
        } else {
            Err(SdkError::Api {
                code: envelope.code,
                msg: envelope.msg,
            })
        }
    }
}
