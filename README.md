# opinion_trade_sdk

Unofficial Rust SDK for `opinion.trade`.

## Features

- **REST client** — 14 endpoints covering markets, tokens, orderbooks, trades, orders, and positions
- **WebSocket client** — typed events, subscribe/unsubscribe, heartbeat, raw JSON access
- **Managed WebSocket** — background task with auto-reconnect, auto-heartbeat, subscription tracking, stream stats
- **Order builder** — tick size handling, price rounding, limit and market order construction with validation
- **Local order book** — seeded from REST, updated via WS deltas, with analytics (spread, mid, weighted mid, liquidity queries, market impact, fill simulation with slippage protection)
- **Rate limiter** — token bucket with configurable RPS and burst, thread-safe
- **Retry** — exponential backoff with error classification (retryable vs permanent)
- **Mock stream** — for testing WS consumers without a live connection

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
    let client = OpinionClient::builder()
        .api_key("YOUR_API_KEY")
        .build()?;

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

## Order builder

```rust,no_run
use opinion_trade_sdk::{OrderBuilder, Side, TickSize, OpinionClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = OpinionClient::builder()
        .api_key("YOUR_API_KEY")
        .build()?;

    // Limit order with automatic price rounding
    let order = OrderBuilder::new("token_id_here", Side::Buy, 100.0)
        .price(0.556)                     // rounded down to 0.55 for buys
        .tick_size(TickSize::Hundredths)   // 0.01 increments (default)
        .chain_id("137")
        .build()?;

    let result = client.create_order(&order).await?;
    println!("order: {:?}", result.data.order_id);
    Ok(())
}
```

## Market order from order book

```rust,no_run
use opinion_trade_sdk::{OrderBuilder, Side, OpinionClient, LocalOrderBook};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = OpinionClient::builder()
        .api_key("YOUR_API_KEY")
        .build()?;

    // Fetch order book and build a market order
    let ob = client.get_orderbook("token_id_here").await?;
    let book = LocalOrderBook::from_rest(&ob);

    let order = OrderBuilder::new("token_id_here", Side::Buy, 50.0)
        .max_slippage(0.02)  // 2% max slippage
        .build_market_order(&book)?;

    let result = client.create_order(&order).await?;
    println!("filled at price: {}", order.price);
    Ok(())
}
```

## Fill simulation

```rust,no_run
use opinion_trade_sdk::{LocalOrderBook, FillResult, Side, OpinionClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = OpinionClient::builder().build()?;
    let ob = client.get_orderbook("token_id_here").await?;
    let book = LocalOrderBook::from_rest(&ob);

    // Simulate a fill before placing the order
    match book.simulate_fill(Side::Buy, 200.0, Some(0.01)) {
        FillResult::Filled(summary) => {
            println!("avg price: {:.4}, slippage: {:.2}%",
                summary.avg_price, summary.slippage * 100.0);
        }
        FillResult::SlippageExceeded { actual_slippage, .. } => {
            println!("slippage too high: {:.2}%", actual_slippage * 100.0);
        }
        FillResult::InsufficientLiquidity => {
            println!("not enough liquidity");
        }
    }

    // Market impact analysis
    if let Some(impact) = book.calculate_market_impact(Side::Buy, 200.0) {
        println!("impact: {:.2}%, total cost: {:.2}",
            impact.impact_pct, impact.total_cost);
    }

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

    // Typed events
    while let Some(event) = ws.next_event().await? {
        println!("event: {:?}", event);
    }

    ws.close().await?;
    Ok(())
}
```

## Managed WebSocket with book applier

```rust,no_run
use opinion_trade_sdk::{
    ManagedWsClient, BookApplier, LocalOrderBook, OpinionClient, WsEvent,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = OpinionClient::builder().build()?;
    let ob = client.get_orderbook("token_id_here").await?;

    // Seed local book from REST snapshot
    let book = LocalOrderBook::from_rest(&ob);
    let mut applier = BookApplier::new(book);

    // Managed WS with auto-reconnect and heartbeat
    let mut ws = ManagedWsClient::connect("YOUR_API_KEY").await?;
    ws.subscribe_market("orderbook", 123).await?;

    while let Some(event) = ws.next_event().await {
        // Auto-apply order book deltas
        applier.apply_event(&event);

        if let WsEvent::OrderBook { .. } = &event {
            let book = applier.book();
            println!("bid: {:?} ask: {:?} spread: {:?}",
                book.best_bid(), book.best_ask(), book.spread());
        }
    }

    println!("stats: {} msgs, {} reconnects",
        ws.stats().messages_received(), ws.stats().reconnects());
    Ok(())
}
```

## Rate limiting

```rust,no_run
use opinion_trade_sdk::RateLimiter;

#[tokio::main]
async fn main() {
    let limiter = RateLimiter::new(10.0, 5); // 10 req/s, burst of 5

    for _ in 0..20 {
        limiter.acquire().await;  // waits if rate exceeded
        // make API call...
    }
}
```

## REST endpoints

| Method | Endpoint | Auth |
|--------|----------|------|
| `get_markets` | GET /market | No |
| `get_market` | GET /market/{id} | No |
| `get_quote_tokens` | GET /quoteToken | No |
| `get_latest_price` | GET /token/latest-price | No |
| `get_orderbook` | GET /token/orderbook | No |
| `get_price_history` | GET /token/price-history | No |
| `get_user_trades` | GET /trade/user/{addr} | No |
| `get_trades` | GET /trade | No |
| `get_orders` | GET /order | API key |
| `get_order_detail` | GET /order/{id} | API key |
| `get_positions` | GET /position | API key |
| `create_order` | POST /order | API key |
| `cancel_order` | POST /order/cancel | API key |
| `cancel_all_orders` | POST /order/cancel-all | API key |

## License

MIT
