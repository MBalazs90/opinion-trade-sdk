use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async, tungstenite::protocol::Message,
};

use crate::error::{Result, SdkError};

const DEFAULT_WS_URL: &str = "wss://ws.opinion.trade";

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug)]
pub struct OpinionWsClient {
    stream: WsStream,
}

impl OpinionWsClient {
    pub async fn connect(api_key: &str) -> Result<Self> {
        Self::connect_with_url(DEFAULT_WS_URL, api_key).await
    }

    pub async fn connect_with_url(base_url: &str, api_key: &str) -> Result<Self> {
        let mut url = url::Url::parse(base_url)?;
        url.query_pairs_mut().append_pair("apikey", api_key);

        let (stream, _) = connect_async(url.as_str()).await?;
        Ok(Self { stream })
    }

    pub async fn heartbeat(&mut self) -> Result<()> {
        self.send_action(json!({ "action": "HEARTBEAT" })).await
    }

    pub async fn subscribe_market(&mut self, channel: &str, market_id: i64) -> Result<()> {
        self.send_action(json!({
            "action": "SUBSCRIBE",
            "channel": channel,
            "marketId": market_id
        }))
        .await
    }

    pub async fn subscribe_root_market(
        &mut self,
        channel: &str,
        root_market_id: i64,
    ) -> Result<()> {
        self.send_action(json!({
            "action": "SUBSCRIBE",
            "channel": channel,
            "rootMarketId": root_market_id
        }))
        .await
    }

    pub async fn unsubscribe_market(&mut self, channel: &str, market_id: i64) -> Result<()> {
        self.send_action(json!({
            "action": "UNSUBSCRIBE",
            "channel": channel,
            "marketId": market_id
        }))
        .await
    }

    pub async fn send_action(&mut self, value: Value) -> Result<()> {
        let payload = serde_json::to_string(&value)?;
        self.stream.send(Message::Text(payload)).await?;
        Ok(())
    }

    pub async fn next_json(&mut self) -> Result<Option<Value>> {
        while let Some(msg) = self.stream.next().await {
            match msg? {
                Message::Text(text) => {
                    let value = serde_json::from_str::<Value>(&text)?;
                    return Ok(Some(value));
                }
                Message::Binary(data) => {
                    let value = serde_json::from_slice::<Value>(&data)?;
                    return Ok(Some(value));
                }
                Message::Ping(payload) => {
                    self.stream.send(Message::Pong(payload)).await?;
                }
                Message::Pong(_) => {}
                Message::Close(_) => return Ok(None),
                Message::Frame(_) => {}
            }
        }

        Ok(None)
    }

    pub async fn close(&mut self) -> Result<()> {
        self.stream.close(None).await.map_err(SdkError::from)
    }
}
