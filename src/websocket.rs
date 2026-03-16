use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async, tungstenite::protocol::Message,
};

use crate::error::{Result, SdkError};
use crate::orderbook::LocalOrderBook;

const DEFAULT_WS_URL: &str = "wss://ws.opinion.trade";

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// A typed WebSocket event received from the server.
#[derive(Debug, Clone)]
pub enum WsEvent {
    /// Heartbeat acknowledgement.
    Heartbeat,
    /// Subscription confirmed.
    Subscribed {
        channel: String,
        market_id: Option<i64>,
    },
    /// Unsubscription confirmed.
    Unsubscribed {
        channel: String,
        market_id: Option<i64>,
    },
    /// Order book snapshot or delta.
    OrderBook { market_id: Option<i64>, data: Value },
    /// Trade event.
    Trade { market_id: Option<i64>, data: Value },
    /// Price update event.
    Price { market_id: Option<i64>, data: Value },
    /// Any event that doesn't match known types.
    Raw(Value),
    /// Connection was closed.
    Closed,
}

/// A WebSocket message that can be sent to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsMessage {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(rename = "marketId", skip_serializing_if = "Option::is_none")]
    pub market_id: Option<i64>,
    #[serde(rename = "rootMarketId", skip_serializing_if = "Option::is_none")]
    pub root_market_id: Option<i64>,
    #[serde(flatten)]
    pub extra: Value,
}

/// Tracks active subscriptions for reconnection.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Subscription {
    channel: String,
    market_id: Option<i64>,
    root_market_id: Option<i64>,
}

fn parse_ws_event(value: Value) -> WsEvent {
    let action = value
        .get("action")
        .or_else(|| value.get("event"))
        .or_else(|| value.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_uppercase();

    let channel = value
        .get("channel")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();

    let market_id = value.get("marketId").and_then(|v| v.as_i64());

    match action.as_str() {
        "HEARTBEAT" => WsEvent::Heartbeat,
        "SUBSCRIBED" | "SUBSCRIBE_OK" => WsEvent::Subscribed {
            channel: channel.clone(),
            market_id,
        },
        "UNSUBSCRIBED" | "UNSUBSCRIBE_OK" => WsEvent::Unsubscribed {
            channel: channel.clone(),
            market_id,
        },
        _ => match channel.as_str() {
            "orderbook" | "order_book" => WsEvent::OrderBook {
                market_id,
                data: value,
            },
            "trade" | "trades" => WsEvent::Trade {
                market_id,
                data: value,
            },
            "price" | "ticker" => WsEvent::Price {
                market_id,
                data: value,
            },
            _ => WsEvent::Raw(value),
        },
    }
}

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

    /// Read the next message and parse it into a typed WsEvent.
    pub async fn next_event(&mut self) -> Result<Option<WsEvent>> {
        match self.next_json().await? {
            Some(value) => Ok(Some(parse_ws_event(value))),
            None => Ok(Some(WsEvent::Closed)),
        }
    }

    pub async fn close(&mut self) -> Result<()> {
        self.stream.close(None).await.map_err(SdkError::from)
    }
}

/// A managed WebSocket client with auto-reconnect, auto-heartbeat, and subscription tracking.
///
/// Runs a background task that owns the WS stream, sends heartbeats, handles reconnection
/// with exponential backoff, re-subscribes after reconnect, and forwards events through a channel.
pub struct ManagedWsClient {
    event_rx: tokio::sync::mpsc::Receiver<WsEvent>,
    cmd_tx: tokio::sync::mpsc::Sender<ManagedCmd>,
    stats: Arc<StreamStats>,
    _task: tokio::task::JoinHandle<()>,
}

/// Statistics about the WebSocket stream.
#[derive(Debug)]
pub struct StreamStats {
    messages_received: AtomicU64,
    errors: AtomicU64,
    reconnects: AtomicU64,
    connected_since: tokio::time::Instant,
}

impl StreamStats {
    fn new() -> Self {
        Self {
            messages_received: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            reconnects: AtomicU64::new(0),
            connected_since: tokio::time::Instant::now(),
        }
    }

    /// Total messages received since connection.
    pub fn messages_received(&self) -> u64 {
        self.messages_received.load(Ordering::Relaxed)
    }

    /// Total errors encountered.
    pub fn errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }

    /// Number of reconnections performed.
    pub fn reconnects(&self) -> u64 {
        self.reconnects.load(Ordering::Relaxed)
    }

    /// Duration since the initial connection.
    pub fn uptime(&self) -> std::time::Duration {
        self.connected_since.elapsed()
    }
}

enum ManagedCmd {
    Subscribe {
        channel: String,
        market_id: Option<i64>,
        root_market_id: Option<i64>,
    },
    Unsubscribe {
        channel: String,
        market_id: Option<i64>,
    },
    Shutdown,
}

impl ManagedWsClient {
    /// Connect and start the background management task.
    pub async fn connect(api_key: impl Into<String>) -> Result<Self> {
        Self::connect_with_url(DEFAULT_WS_URL, api_key).await
    }

    pub async fn connect_with_url(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self> {
        let base_url = base_url.into();
        let api_key = api_key.into();

        // Verify initial connection works.
        let client = OpinionWsClient::connect_with_url(&base_url, &api_key).await?;

        let (event_tx, event_rx) = tokio::sync::mpsc::channel(256);
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(64);
        let stats = Arc::new(StreamStats::new());

        let task = tokio::spawn(managed_task(
            client,
            base_url,
            api_key,
            event_tx,
            cmd_rx,
            Arc::clone(&stats),
        ));

        Ok(Self {
            event_rx,
            cmd_tx,
            stats,
            _task: task,
        })
    }

    /// Receive the next event. Returns `None` if the background task has ended.
    pub async fn next_event(&mut self) -> Option<WsEvent> {
        self.event_rx.recv().await
    }

    /// Subscribe to a channel for a market.
    pub async fn subscribe_market(&self, channel: impl Into<String>, market_id: i64) -> Result<()> {
        self.cmd_tx
            .send(ManagedCmd::Subscribe {
                channel: channel.into(),
                market_id: Some(market_id),
                root_market_id: None,
            })
            .await
            .map_err(|_| SdkError::ConnectionClosed)
    }

    /// Subscribe to a channel for a root market.
    pub async fn subscribe_root_market(
        &self,
        channel: impl Into<String>,
        root_market_id: i64,
    ) -> Result<()> {
        self.cmd_tx
            .send(ManagedCmd::Subscribe {
                channel: channel.into(),
                market_id: None,
                root_market_id: Some(root_market_id),
            })
            .await
            .map_err(|_| SdkError::ConnectionClosed)
    }

    /// Unsubscribe from a channel for a market.
    pub async fn unsubscribe_market(
        &self,
        channel: impl Into<String>,
        market_id: i64,
    ) -> Result<()> {
        self.cmd_tx
            .send(ManagedCmd::Unsubscribe {
                channel: channel.into(),
                market_id: Some(market_id),
            })
            .await
            .map_err(|_| SdkError::ConnectionClosed)
    }

    /// Get stream statistics (messages, errors, reconnects, uptime).
    pub fn stats(&self) -> &StreamStats {
        &self.stats
    }

    /// Shut down the background task and close the connection.
    pub async fn shutdown(&self) -> Result<()> {
        let _ = self.cmd_tx.send(ManagedCmd::Shutdown).await;
        Ok(())
    }
}

impl futures_util::Stream for ManagedWsClient {
    type Item = WsEvent;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.event_rx.poll_recv(cx)
    }
}

async fn managed_task(
    mut client: OpinionWsClient,
    base_url: String,
    api_key: String,
    event_tx: tokio::sync::mpsc::Sender<WsEvent>,
    mut cmd_rx: tokio::sync::mpsc::Receiver<ManagedCmd>,
    stats: Arc<StreamStats>,
) {
    let mut subscriptions: Vec<Subscription> = Vec::new();
    let heartbeat_interval = tokio::time::Duration::from_secs(15);
    let mut heartbeat_timer = tokio::time::interval(heartbeat_interval);
    heartbeat_timer.tick().await; // consume initial tick

    loop {
        tokio::select! {
            // Process commands
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(ManagedCmd::Subscribe { channel, market_id, root_market_id }) => {
                        let mut msg = json!({
                            "action": "SUBSCRIBE",
                            "channel": &channel,
                        });
                        if let Some(mid) = market_id {
                            msg["marketId"] = json!(mid);
                        }
                        if let Some(rmid) = root_market_id {
                            msg["rootMarketId"] = json!(rmid);
                        }
                        let _ = client.send_action(msg).await;
                        subscriptions.push(Subscription { channel, market_id, root_market_id });
                    }
                    Some(ManagedCmd::Unsubscribe { channel, market_id }) => {
                        let mut msg = json!({
                            "action": "UNSUBSCRIBE",
                            "channel": &channel,
                        });
                        if let Some(mid) = market_id {
                            msg["marketId"] = json!(mid);
                        }
                        let _ = client.send_action(msg).await;
                        subscriptions.retain(|s| !(s.channel == channel && s.market_id == market_id));
                    }
                    Some(ManagedCmd::Shutdown) | None => {
                        let _ = client.close().await;
                        return;
                    }
                }
            }
            // Heartbeat
            _ = heartbeat_timer.tick() => {
                if client.heartbeat().await.is_err() {
                    stats.errors.fetch_add(1, Ordering::Relaxed);
                    if !reconnect_with_stats(&mut client, &base_url, &api_key, &subscriptions, &stats).await {
                        return;
                    }
                    heartbeat_timer.reset();
                }
            }
            // Read events
            event = client.next_event() => {
                match event {
                    Ok(Some(WsEvent::Closed)) | Err(_) => {
                        if event.is_err() {
                            stats.errors.fetch_add(1, Ordering::Relaxed);
                        }
                        if !reconnect_with_stats(&mut client, &base_url, &api_key, &subscriptions, &stats).await {
                            return;
                        }
                        heartbeat_timer.reset();
                    }
                    Ok(Some(evt)) => {
                        stats.messages_received.fetch_add(1, Ordering::Relaxed);
                        if event_tx.send(evt).await.is_err() {
                            let _ = client.close().await;
                            return;
                        }
                    }
                    Ok(None) => {
                        if !reconnect_with_stats(&mut client, &base_url, &api_key, &subscriptions, &stats).await {
                            return;
                        }
                        heartbeat_timer.reset();
                    }
                }
            }
        }
    }
}

async fn reconnect_with_stats(
    client: &mut OpinionWsClient,
    base_url: &str,
    api_key: &str,
    subscriptions: &[Subscription],
    stats: &StreamStats,
) -> bool {
    let result = reconnect(client, base_url, api_key, subscriptions).await;
    if result {
        stats.reconnects.fetch_add(1, Ordering::Relaxed);
    }
    result
}

async fn reconnect(
    client: &mut OpinionWsClient,
    base_url: &str,
    api_key: &str,
    subscriptions: &[Subscription],
) -> bool {
    let mut delay = tokio::time::Duration::from_millis(500);
    let max_delay = tokio::time::Duration::from_secs(30);
    let max_attempts = 10;

    for _ in 0..max_attempts {
        tokio::time::sleep(delay).await;
        match OpinionWsClient::connect_with_url(base_url, api_key).await {
            Ok(new_client) => {
                *client = new_client;
                // Re-subscribe
                for sub in subscriptions {
                    let mut msg = json!({
                        "action": "SUBSCRIBE",
                        "channel": &sub.channel,
                    });
                    if let Some(mid) = sub.market_id {
                        msg["marketId"] = json!(mid);
                    }
                    if let Some(rmid) = sub.root_market_id {
                        msg["rootMarketId"] = json!(rmid);
                    }
                    let _ = client.send_action(msg).await;
                }
                return true;
            }
            Err(_) => {
                delay = (delay * 2).min(max_delay);
            }
        }
    }
    false
}

/// Automatically applies WebSocket order book events to a `LocalOrderBook`.
///
/// Wraps any `WsEvent` source and maintains the book in sync.
pub struct BookApplier {
    book: LocalOrderBook,
}

impl BookApplier {
    /// Create a new book applier with the given initial order book.
    pub fn new(book: LocalOrderBook) -> Self {
        Self { book }
    }

    /// Get a reference to the current order book.
    pub fn book(&self) -> &LocalOrderBook {
        &self.book
    }

    /// Get a mutable reference to the order book.
    pub fn book_mut(&mut self) -> &mut LocalOrderBook {
        &mut self.book
    }

    /// Process a WsEvent and apply any order book deltas.
    ///
    /// Returns `true` if the event was an order book update that was applied.
    pub fn apply_event(&mut self, event: &WsEvent) -> bool {
        match event {
            WsEvent::OrderBook { data, .. } => {
                self.apply_orderbook_data(data);
                true
            }
            _ => false,
        }
    }

    fn apply_orderbook_data(&mut self, data: &Value) {
        if let Some(bids) = data.get("bids").and_then(|v| v.as_array()) {
            for bid in bids {
                if let Some((price, size)) = extract_price_size(bid) {
                    self.book.apply_delta("buy", price, size);
                }
            }
        }
        if let Some(asks) = data.get("asks").and_then(|v| v.as_array()) {
            for ask in asks {
                if let Some((price, size)) = extract_price_size(ask) {
                    self.book.apply_delta("sell", price, size);
                }
            }
        }
    }
}

/// Extract price and size from a JSON order book level entry.
/// Handles both string ("0.55") and numeric (0.55) formats,
/// and both object ({"price": ..., "size": ...}) and array ([price, size]) layouts.
#[inline]
fn extract_price_size(entry: &Value) -> Option<(f64, f64)> {
    let price = entry.get("price").or_else(|| entry.get(0)).and_then(|v| {
        v.as_f64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })?;
    let size = entry.get("size").or_else(|| entry.get(1)).and_then(|v| {
        v.as_f64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })?;
    Some((price, size))
}

/// High-performance book applier using `FixedOrderBook`.
///
/// Uses fixed-point integer parsing to avoid f64 conversions on the hot path.
pub struct FastBookApplier {
    book: crate::fixed_book::FixedOrderBook,
}

impl FastBookApplier {
    pub fn new(book: crate::fixed_book::FixedOrderBook) -> Self {
        Self { book }
    }

    pub fn book(&self) -> &crate::fixed_book::FixedOrderBook {
        &self.book
    }

    pub fn book_mut(&mut self) -> &mut crate::fixed_book::FixedOrderBook {
        &mut self.book
    }

    /// Apply a WsEvent using fixed-point parsing. Returns true if applied.
    pub fn apply_event(&mut self, event: &WsEvent) -> bool {
        match event {
            WsEvent::OrderBook { data, .. } => {
                self.apply_orderbook_data(data);
                true
            }
            _ => false,
        }
    }

    fn apply_orderbook_data(&mut self, data: &Value) {
        use crate::fixed_book::{parse_price_fixed, parse_size_fixed};

        if let Some(bids) = data.get("bids").and_then(|v| v.as_array()) {
            for bid in bids {
                if let Some((price, size)) = extract_price_size_fixed(bid) {
                    self.book.apply_delta_fixed(0, price, size);
                } else if let Some((price, size)) = extract_price_size(bid) {
                    // Fallback for numeric-only formats
                    self.book.apply_delta_fixed(
                        0,
                        parse_price_fixed(&format!("{price}")).unwrap_or(0),
                        parse_size_fixed(&format!("{size}")).unwrap_or(0),
                    );
                }
            }
        }
        if let Some(asks) = data.get("asks").and_then(|v| v.as_array()) {
            for ask in asks {
                if let Some((price, size)) = extract_price_size_fixed(ask) {
                    self.book.apply_delta_fixed(1, price, size);
                } else if let Some((price, size)) = extract_price_size(ask) {
                    self.book.apply_delta_fixed(
                        1,
                        parse_price_fixed(&format!("{price}")).unwrap_or(0),
                        parse_size_fixed(&format!("{size}")).unwrap_or(0),
                    );
                }
            }
        }
    }
}

/// Extract price and size as fixed-point directly from string values (zero f64 conversion).
#[inline]
fn extract_price_size_fixed(entry: &Value) -> Option<(u32, i64)> {
    use crate::fixed_book::{parse_price_fixed, parse_size_fixed};

    let price_str = entry
        .get("price")
        .or_else(|| entry.get(0))
        .and_then(|v| v.as_str())?;
    let size_str = entry
        .get("size")
        .or_else(|| entry.get(1))
        .and_then(|v| v.as_str())?;

    let price = parse_price_fixed(price_str)?;
    let size = parse_size_fixed(size_str)?;
    Some((price, size))
}

/// A mock WebSocket event source for testing.
///
/// Push events into the mock, then consume them via `next_event()`.
pub struct MockWsStream {
    events: std::collections::VecDeque<WsEvent>,
}

impl MockWsStream {
    pub fn new() -> Self {
        Self {
            events: std::collections::VecDeque::new(),
        }
    }

    /// Push an event to be returned by `next_event()`.
    pub fn push_event(&mut self, event: WsEvent) {
        self.events.push_back(event);
    }

    /// Push multiple events.
    pub fn push_events(&mut self, events: impl IntoIterator<Item = WsEvent>) {
        self.events.extend(events);
    }

    /// Get the next event, or `None` if empty.
    pub fn next_event(&mut self) -> Option<WsEvent> {
        self.events.pop_front()
    }

    /// Number of remaining events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether there are no remaining events.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl Default for MockWsStream {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_heartbeat_event() {
        let v = json!({"action": "HEARTBEAT"});
        let evt = parse_ws_event(v);
        assert!(matches!(evt, WsEvent::Heartbeat));
    }

    #[test]
    fn parse_subscribed_event() {
        let v = json!({"action": "SUBSCRIBED", "channel": "orderbook", "marketId": 42});
        let evt = parse_ws_event(v);
        match evt {
            WsEvent::Subscribed { channel, market_id } => {
                assert_eq!(channel, "orderbook");
                assert_eq!(market_id, Some(42));
            }
            _ => panic!("expected Subscribed"),
        }
    }

    #[test]
    fn parse_unsubscribed_event() {
        let v = json!({"action": "UNSUBSCRIBED", "channel": "trade", "marketId": 5});
        let evt = parse_ws_event(v);
        match evt {
            WsEvent::Unsubscribed { channel, market_id } => {
                assert_eq!(channel, "trade");
                assert_eq!(market_id, Some(5));
            }
            _ => panic!("expected Unsubscribed"),
        }
    }

    #[test]
    fn parse_orderbook_event() {
        let v = json!({"channel": "orderbook", "marketId": 42, "bids": [], "asks": []});
        let evt = parse_ws_event(v);
        match evt {
            WsEvent::OrderBook { market_id, data } => {
                assert_eq!(market_id, Some(42));
                assert!(data.get("bids").is_some());
            }
            _ => panic!("expected OrderBook"),
        }
    }

    #[test]
    fn parse_trade_event() {
        let v = json!({"channel": "trade", "marketId": 10, "price": "0.55"});
        let evt = parse_ws_event(v);
        assert!(matches!(evt, WsEvent::Trade { .. }));
    }

    #[test]
    fn parse_price_event() {
        let v = json!({"channel": "ticker", "marketId": 7, "price": "0.60"});
        let evt = parse_ws_event(v);
        assert!(matches!(evt, WsEvent::Price { .. }));
    }

    #[test]
    fn parse_unknown_falls_back_to_raw() {
        let v = json!({"something": "unknown"});
        let evt = parse_ws_event(v);
        assert!(matches!(evt, WsEvent::Raw(_)));
    }

    #[test]
    fn parse_subscribe_ok_variant() {
        let v = json!({"action": "SUBSCRIBE_OK", "channel": "price", "marketId": 1});
        let evt = parse_ws_event(v);
        assert!(matches!(evt, WsEvent::Subscribed { .. }));
    }

    #[test]
    fn ws_message_serialize() {
        let msg = WsMessage {
            action: "SUBSCRIBE".into(),
            channel: Some("orderbook".into()),
            market_id: Some(42),
            root_market_id: None,
            extra: json!({}),
        };
        let v = serde_json::to_value(&msg).unwrap();
        assert_eq!(v["action"], "SUBSCRIBE");
        assert_eq!(v["channel"], "orderbook");
        assert_eq!(v["marketId"], 42);
        assert!(v.get("rootMarketId").is_none());
    }

    #[test]
    fn ws_message_deserialize() {
        let v = json!({
            "action": "SUBSCRIBE",
            "channel": "trade",
            "marketId": 5,
            "extra_field": true
        });
        let msg: WsMessage = serde_json::from_value(v).unwrap();
        assert_eq!(msg.action, "SUBSCRIBE");
        assert_eq!(msg.channel.as_deref(), Some("trade"));
        assert_eq!(msg.market_id, Some(5));
        assert_eq!(msg.extra["extra_field"], true);
    }

    #[test]
    fn stream_stats_initial() {
        let stats = StreamStats::new();
        assert_eq!(stats.messages_received(), 0);
        assert_eq!(stats.errors(), 0);
        assert_eq!(stats.reconnects(), 0);
    }

    #[test]
    fn stream_stats_increment() {
        let stats = StreamStats::new();
        stats.messages_received.fetch_add(5, Ordering::Relaxed);
        stats.errors.fetch_add(2, Ordering::Relaxed);
        stats.reconnects.fetch_add(1, Ordering::Relaxed);
        assert_eq!(stats.messages_received(), 5);
        assert_eq!(stats.errors(), 2);
        assert_eq!(stats.reconnects(), 1);
    }

    #[test]
    fn book_applier_applies_orderbook_event() {
        let book = LocalOrderBook::new("tok_1");
        let mut applier = BookApplier::new(book);

        let event = WsEvent::OrderBook {
            market_id: Some(42),
            data: json!({
                "bids": [{"price": "0.50", "size": "100"}],
                "asks": [{"price": "0.55", "size": "200"}]
            }),
        };

        assert!(applier.apply_event(&event));
        assert!((applier.book().best_bid().unwrap() - 0.50).abs() < f64::EPSILON);
        assert!((applier.book().best_ask().unwrap() - 0.55).abs() < f64::EPSILON);
    }

    #[test]
    fn book_applier_ignores_non_orderbook_events() {
        let book = LocalOrderBook::new("tok_1");
        let mut applier = BookApplier::new(book);

        assert!(!applier.apply_event(&WsEvent::Heartbeat));
        assert!(!applier.apply_event(&WsEvent::Trade {
            market_id: Some(1),
            data: json!({}),
        }));
        assert_eq!(applier.book().bid_depth(), 0);
    }

    #[test]
    fn book_applier_removes_zero_size() {
        let book = LocalOrderBook::new("tok_1");
        let mut applier = BookApplier::new(book);

        // Add a level
        applier.apply_event(&WsEvent::OrderBook {
            market_id: None,
            data: json!({"bids": [{"price": "0.50", "size": "100"}], "asks": []}),
        });
        assert_eq!(applier.book().bid_depth(), 1);

        // Remove it with size 0
        applier.apply_event(&WsEvent::OrderBook {
            market_id: None,
            data: json!({"bids": [{"price": "0.50", "size": "0"}], "asks": []}),
        });
        assert_eq!(applier.book().bid_depth(), 0);
    }

    #[test]
    fn book_applier_numeric_prices() {
        let book = LocalOrderBook::new("tok_1");
        let mut applier = BookApplier::new(book);

        // Some APIs send numeric prices instead of strings
        applier.apply_event(&WsEvent::OrderBook {
            market_id: None,
            data: json!({
                "bids": [{"price": 0.50, "size": 100.0}],
                "asks": [{"price": 0.55, "size": 200.0}]
            }),
        });
        assert!((applier.book().best_bid().unwrap() - 0.50).abs() < f64::EPSILON);
        assert!((applier.book().best_ask().unwrap() - 0.55).abs() < f64::EPSILON);
    }

    #[test]
    fn mock_ws_stream_basic() {
        let mut mock = MockWsStream::new();
        assert!(mock.is_empty());
        assert_eq!(mock.len(), 0);

        mock.push_event(WsEvent::Heartbeat);
        mock.push_event(WsEvent::Closed);
        assert_eq!(mock.len(), 2);
        assert!(!mock.is_empty());

        assert!(matches!(mock.next_event(), Some(WsEvent::Heartbeat)));
        assert!(matches!(mock.next_event(), Some(WsEvent::Closed)));
        assert!(mock.next_event().is_none());
    }

    #[test]
    fn mock_ws_stream_push_events() {
        let mut mock = MockWsStream::new();
        mock.push_events(vec![
            WsEvent::Heartbeat,
            WsEvent::Subscribed {
                channel: "trade".into(),
                market_id: Some(1),
            },
        ]);
        assert_eq!(mock.len(), 2);
    }

    #[test]
    fn mock_ws_stream_with_book_applier() {
        let mut mock = MockWsStream::new();
        mock.push_event(WsEvent::OrderBook {
            market_id: Some(42),
            data: json!({
                "bids": [{"price": "0.50", "size": "100"}],
                "asks": [{"price": "0.55", "size": "200"}]
            }),
        });

        let book = LocalOrderBook::new("tok_1");
        let mut applier = BookApplier::new(book);

        while let Some(event) = mock.next_event() {
            applier.apply_event(&event);
        }

        assert_eq!(applier.book().bid_depth(), 1);
        assert_eq!(applier.book().ask_depth(), 1);
    }

    #[test]
    fn fast_book_applier_applies_string_prices() {
        let book = crate::fixed_book::FixedOrderBook::new("tok_1");
        let mut applier = FastBookApplier::new(book);

        let event = WsEvent::OrderBook {
            market_id: Some(42),
            data: json!({
                "bids": [{"price": "0.50", "size": "100"}],
                "asks": [{"price": "0.55", "size": "200"}]
            }),
        };

        assert!(applier.apply_event(&event));
        assert_eq!(applier.book().best_bid_fixed(), Some(5000));
        assert_eq!(applier.book().best_ask_fixed(), Some(5500));
    }

    #[test]
    fn fast_book_applier_applies_numeric_prices() {
        let book = crate::fixed_book::FixedOrderBook::new("tok_1");
        let mut applier = FastBookApplier::new(book);

        let event = WsEvent::OrderBook {
            market_id: None,
            data: json!({
                "bids": [{"price": 0.50, "size": 100.0}],
                "asks": [{"price": 0.55, "size": 200.0}]
            }),
        };

        assert!(applier.apply_event(&event));
        assert!((applier.book().best_bid().unwrap() - 0.50).abs() < 1e-4);
    }

    #[test]
    fn fast_book_applier_removes_zero_size() {
        let book = crate::fixed_book::FixedOrderBook::new("tok_1");
        let mut applier = FastBookApplier::new(book);

        applier.apply_event(&WsEvent::OrderBook {
            market_id: None,
            data: json!({"bids": [{"price": "0.50", "size": "100"}], "asks": []}),
        });
        assert_eq!(applier.book().bid_depth(), 1);

        applier.apply_event(&WsEvent::OrderBook {
            market_id: None,
            data: json!({"bids": [{"price": "0.50", "size": "0"}], "asks": []}),
        });
        assert_eq!(applier.book().bid_depth(), 0);
    }

    #[test]
    fn extract_price_size_helper() {
        let entry = json!({"price": "0.55", "size": "100"});
        let (p, s) = extract_price_size(&entry).unwrap();
        assert!((p - 0.55).abs() < f64::EPSILON);
        assert!((s - 100.0).abs() < f64::EPSILON);

        let entry = json!({"price": 0.55, "size": 100.0});
        let (p, _s) = extract_price_size(&entry).unwrap();
        assert!((p - 0.55).abs() < f64::EPSILON);
    }

    #[test]
    fn extract_price_size_fixed_helper() {
        let entry = json!({"price": "0.55", "size": "100"});
        let (p, s) = extract_price_size_fixed(&entry).unwrap();
        assert_eq!(p, 5500);
        assert_eq!(s, 100_000_000);
    }

    #[test]
    fn subscription_tracking_add_remove() {
        let mut subs: Vec<Subscription> = Vec::new();
        subs.push(Subscription {
            channel: "orderbook".into(),
            market_id: Some(42),
            root_market_id: None,
        });
        subs.push(Subscription {
            channel: "trade".into(),
            market_id: Some(10),
            root_market_id: None,
        });
        assert_eq!(subs.len(), 2);

        let channel = "orderbook";
        let market_id = Some(42);
        subs.retain(|s| !(s.channel == channel && s.market_id == market_id));
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].channel, "trade");
    }
}
