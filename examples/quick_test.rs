use opinion_trade_sdk::{MarketQuery, OpinionClient, OpinionWsClient, PriceHistoryQuery};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPINION_API_KEY")?;
    let client = OpinionClient::builder().api_key(&api_key).build()?;

    // === REST: Markets ===
    println!("=== REST: Fetching markets ===");
    let markets = client
        .get_markets(&MarketQuery {
            page: Some(1),
            limit: Some(3),
            status: Some("activated".into()),
            ..Default::default()
        })
        .await?;

    println!("Total markets: {}", markets.total);
    for m in &markets.list {
        println!(
            "  [{}] {}",
            m.market_id.unwrap_or(-1),
            m.market_title.as_deref().unwrap_or("(no title)")
        );
    }

    // === REST: Quote tokens ===
    println!("\n=== REST: Quote tokens ===");
    let tokens = client.get_quote_tokens().await?;
    for t in &tokens.list {
        println!(
            "  {} ({}) decimal={}",
            t.symbol.as_deref().unwrap_or("?"),
            t.address.as_deref().unwrap_or("?"),
            t.decimal.unwrap_or(0)
        );
    }

    // === REST: Single market detail ===
    let first_market = &markets.list[0];
    let market_id = first_market.market_id.unwrap();
    println!("\n=== REST: Market {} detail ===", market_id);
    let detail = client.get_market(market_id).await?;
    let m = &detail.data;
    println!("  Title: {}", m.market_title.as_deref().unwrap_or("?"));
    println!("  Status: {}", m.status_enum.as_deref().unwrap_or("?"));

    // Extract token IDs from the extra fields
    let yes_token = m.extra["yesTokenId"].as_str().unwrap_or("");
    let no_token = m.extra["noTokenId"].as_str().unwrap_or("");
    println!("  YES token: {}...", &yes_token[..20.min(yes_token.len())]);
    println!("  NO  token: {}...", &no_token[..20.min(no_token.len())]);

    // === REST: Latest price ===
    if !yes_token.is_empty() {
        println!("\n=== REST: Latest price (YES token) ===");
        let price = client.get_latest_price(yes_token).await?;
        println!(
            "  Price: {} | Side: {} | Size: {}",
            price.price.as_deref().unwrap_or("?"),
            price.side.as_deref().unwrap_or("?"),
            price.size.as_deref().unwrap_or("?"),
        );

        println!("\n=== REST: Orderbook (YES token) ===");
        let ob = client.get_orderbook(yes_token).await?;
        println!(
            "  Market: {} | Bids: {} | Asks: {}",
            ob.market.as_deref().unwrap_or("?"),
            ob.bids.len(),
            ob.asks.len()
        );
        for bid in ob.bids.iter().take(3) {
            println!("    BID {} @ {}", bid.size, bid.price);
        }
        for ask in ob.asks.iter().take(3) {
            println!("    ASK {} @ {}", ask.size, ask.price);
        }

        println!("\n=== REST: Price history ===");
        let history = client
            .get_price_history(&PriceHistoryQuery {
                token_id: yes_token.to_string(),
                interval: Some("1h".into()),
                start_at: None,
                end_at: None,
            })
            .await?;
        println!("  {} data points", history.history.len());
        for p in history.history.iter().take(3) {
            println!("    price={} time={}", p.price, p.timestamp);
        }
    }

    // === WebSocket ===
    println!("\n=== WebSocket: Connecting ===");
    let mut ws = OpinionWsClient::connect(&api_key).await?;
    println!("  Connected!");

    println!("  Sending heartbeat...");
    ws.heartbeat().await?;
    println!("  Heartbeat sent.");

    println!("  Subscribing to market {} orderbook...", market_id);
    ws.subscribe_market("orderbook", market_id).await?;
    println!("  Subscribed. Waiting for messages (5s timeout)...");

    let mut msg_count = 0;
    loop {
        let result = tokio::time::timeout(Duration::from_secs(5), ws.next_json()).await;

        match result {
            Ok(Ok(Some(msg))) => {
                msg_count += 1;
                let channel = msg["channel"].as_str().unwrap_or("?");
                let action = msg["action"].as_str().unwrap_or("?");
                println!("  MSG {}: channel={} action={}", msg_count, channel, action);

                // Print some detail for orderbook messages
                if let Some(data) = msg.get("data") {
                    if let Some(bids) = data.get("bids") {
                        println!("    bids: {}", bids);
                    }
                    if let Some(asks) = data.get("asks") {
                        println!("    asks: {}", asks);
                    }
                }

                // Also print raw for first message
                if msg_count == 1 {
                    let raw = serde_json::to_string_pretty(&msg)?;
                    let truncated: String = raw.chars().take(500).collect();
                    println!("  [raw] {}", truncated);
                }

                if msg_count >= 5 {
                    println!("  (stopping after 5 messages)");
                    break;
                }
            }
            Ok(Ok(None)) => {
                println!("  Stream closed by server.");
                break;
            }
            Ok(Err(e)) => {
                println!("  Error: {}", e);
                break;
            }
            Err(_) => {
                println!("  Timeout (no message in 5s).");
                break;
            }
        }
    }

    println!("  Unsubscribing...");
    ws.unsubscribe_market("orderbook", market_id).await?;

    println!("  Subscribing to market channel...");
    ws.subscribe_market("market", market_id).await?;

    let result = tokio::time::timeout(Duration::from_secs(5), ws.next_json()).await;
    match result {
        Ok(Ok(Some(msg))) => {
            let channel = msg["channel"].as_str().unwrap_or("?");
            println!("  Market channel msg: channel={}", channel);
        }
        Ok(Ok(None)) => println!("  Stream closed."),
        Ok(Err(e)) => println!("  Error: {}", e),
        Err(_) => println!("  Timeout (no market msg in 5s)."),
    }

    println!("  Closing WebSocket...");
    ws.close().await?;
    println!("  Closed.");

    println!("\n=== All tests passed! ===");
    Ok(())
}
