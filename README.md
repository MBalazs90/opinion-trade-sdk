# opinion_trade_sdk

Unofficial Rust SDK for `opinion.trade`.

It currently provides:

- OpenAPI REST client (`https://openapi.opinion.trade/openapi`)
- WebSocket client (`wss://ws.opinion.trade`)

The design borrows from common Polymarket Rust SDK patterns:

- small `ClientBuilder`
- typed models for common fields with `extra` JSON passthrough
- explicit auth-gated methods
- simple WebSocket subscribe and heartbeat helpers

## Install

```toml
[dependencies]
opinion_trade_sdk = { path = "." }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## REST usage

```rust,no_run
use opinion_trade_sdk::{MarketQuery, OpinionClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = OpinionClient::builder().api_key("YOUR_API_KEY").build()?;

    let markets = client
        .get_markets(&MarketQuery {
            page: Some(1),
            limit: Some(10),
            status: Some("activated".into()),
            ..Default::default()
        })
        .await?;

    println!("markets: {}", markets.list.len());
    Ok(())
}
```

## WebSocket usage

```rust,no_run
use opinion_trade_sdk::OpinionWsClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ws = OpinionWsClient::connect("YOUR_API_KEY").await?;
    ws.subscribe_market("orderbook", 123).await?;
    ws.heartbeat().await?;

    if let Some(msg) = ws.next_json().await? {
        println!("event: {msg}");
    }

    ws.close().await?;
    Ok(())
}
```

## Notes

- `OpenAPI` docs describe market data + account data endpoints.
- Trading order placement/signing is handled in Opinion's CLOB SDKs (Python/TypeScript).
- This crate does not yet implement CLOB signing flows.
