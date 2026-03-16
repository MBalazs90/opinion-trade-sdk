use opinion_trade_sdk::{MarketQuery, OpinionClient, OrderQuery, PriceHistoryQuery};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPINION_API_KEY").expect("Set OPINION_API_KEY env var");
    let client = OpinionClient::builder().api_key(&api_key).build()?;

    println!("=== ANOMALY 1: Global trades 404 ===");
    println!("Testing various trade endpoint paths...\n");

    // Try different paths
    let paths_to_try = [
        "/trade/global",
        "/trade",
        "/trades",
        "/trade/list",
        "/trades/global",
        "/trade/all",
    ];

    for path in paths_to_try {
        let url = format!("{}{}", client.base_url(), path);
        let result = reqwest::Client::new()
            .get(&url)
            .header("apikey", &api_key)
            .query(&[("page", "1"), ("limit", "3")])
            .send()
            .await?;
        println!(
            "  {} -> {} {}",
            path,
            result.status(),
            result.status().as_str()
        );
        if result.status().is_success() {
            let body: String = result.text().await?;
            let truncated: String = body.chars().take(300).collect();
            println!("    Body: {}", truncated);
        }
    }

    println!("\n=== ANOMALY 2: Floating point sizes in orderbook ===");
    println!("Fetching orderbooks to inspect raw size values...\n");

    let markets = client
        .get_markets(&MarketQuery {
            page: Some(1),
            limit: Some(10),
            status: Some("activated".into()),
            ..Default::default()
        })
        .await?;

    let mut inspected = 0;
    for m in &markets.list {
        let mid = m.market_id.unwrap();
        let detail = client.get_market(mid).await?;
        let yt = detail.data.extra["yesTokenId"].as_str().unwrap_or("");
        let nt = detail.data.extra["noTokenId"].as_str().unwrap_or("");
        if yt.is_empty() {
            continue;
        }

        let ob = client.get_orderbook(yt).await?;
        if ob.bids.is_empty() && ob.asks.is_empty() {
            continue;
        }

        println!(
            "  Market {} ({}):",
            mid,
            m.market_title.as_deref().unwrap_or("?")
        );
        println!("    YES token: {}", yt);
        for (i, bid) in ob.bids.iter().enumerate().take(3) {
            println!(
                "    BID[{}] price=\"{}\" size=\"{}\"",
                i, bid.price, bid.size
            );
        }
        for (i, ask) in ob.asks.iter().enumerate().take(3) {
            println!(
                "    ASK[{}] price=\"{}\" size=\"{}\"",
                i, ask.price, ask.size
            );
        }

        // Also check NO token if present
        if !nt.is_empty() {
            let ob_no = client.get_orderbook(nt).await?;
            if !ob_no.bids.is_empty() || !ob_no.asks.is_empty() {
                println!("    NO token: {}", nt);
                for (i, bid) in ob_no.bids.iter().enumerate().take(2) {
                    println!(
                        "    NO BID[{}] price=\"{}\" size=\"{}\"",
                        i, bid.price, bid.size
                    );
                }
                for (i, ask) in ob_no.asks.iter().enumerate().take(2) {
                    println!(
                        "    NO ASK[{}] price=\"{}\" size=\"{}\"",
                        i, ask.price, ask.size
                    );
                }
            }
        }

        inspected += 1;
        if inspected >= 4 {
            break;
        }
    }

    println!("\n=== ANOMALY 3: Market total = page limit? ===");
    println!("Testing pagination...\n");

    let page1 = client
        .get_markets(&MarketQuery {
            page: Some(1),
            limit: Some(5),
            status: Some("activated".into()),
            ..Default::default()
        })
        .await?;
    println!(
        "  page=1 limit=5: total={} returned={}",
        page1.total,
        page1.list.len()
    );

    let page2 = client
        .get_markets(&MarketQuery {
            page: Some(2),
            limit: Some(5),
            status: Some("activated".into()),
            ..Default::default()
        })
        .await?;
    println!(
        "  page=2 limit=5: total={} returned={}",
        page2.total,
        page2.list.len()
    );

    let big = client
        .get_markets(&MarketQuery {
            page: Some(1),
            limit: Some(100),
            status: Some("activated".into()),
            ..Default::default()
        })
        .await?;
    println!(
        "  page=1 limit=100: total={} returned={}",
        big.total,
        big.list.len()
    );

    let all_statuses = client
        .get_markets(&MarketQuery {
            page: Some(1),
            limit: Some(5),
            ..Default::default()
        })
        .await?;
    println!(
        "  no status filter: total={} returned={}",
        all_statuses.total,
        all_statuses.list.len()
    );

    println!("\n=== ANOMALY 4: User trades endpoint ===");
    println!("Testing user trades with a dummy address...\n");

    let user_trades = client
        .get_user_trades(
            "0x0000000000000000000000000000000000000000",
            &opinion_trade_sdk::UserTradesQuery {
                page: Some(1),
                limit: Some(3),
                ..Default::default()
            },
        )
        .await;
    match user_trades {
        Ok(t) => println!("  User trades: total={} returned={}", t.total, t.list.len()),
        Err(e) => println!("  User trades error: {}", e),
    }

    println!("\n=== ANOMALY 5: Price history format ===");
    // Find a token with data
    if let Some(m) = markets.list.first() {
        let mid = m.market_id.unwrap();
        let detail = client.get_market(mid).await?;
        let yt = detail.data.extra["yesTokenId"].as_str().unwrap_or("");
        if !yt.is_empty() {
            let history = client
                .get_price_history(&PriceHistoryQuery {
                    token_id: yt.to_string(),
                    interval: Some("1h".into()),
                    start_at: None,
                    end_at: None,
                })
                .await?;
            println!("  Price history: {} data points", history.history.len());
            for p in history.history.iter().take(3) {
                println!("    price={} time={}", p.price, p.timestamp);
            }
        }
    }

    println!("\n=== ANOMALY 6: Orders endpoint (authenticated) ===");
    let orders = client
        .get_orders(&OrderQuery {
            page: Some(1),
            limit: Some(3),
            ..Default::default()
        })
        .await;
    match orders {
        Ok(o) => println!("  Orders: total={} returned={}", o.total, o.list.len()),
        Err(e) => println!("  Orders error: {}", e),
    }

    println!("\n=== ANOMALY 7: Raw API envelope inspection ===");
    // Make a raw request to see the exact envelope format
    let raw_resp = reqwest::Client::new()
        .get(format!("{}/market", client.base_url()))
        .header("apikey", &api_key)
        .query(&[("page", "1"), ("limit", "2"), ("status", "activated")])
        .send()
        .await?;
    let raw_body = raw_resp.text().await?;
    let truncated: String = raw_body.chars().take(500).collect();
    println!("  Raw /market response:\n{}", truncated);

    println!("\n=== Investigation complete ===");
    Ok(())
}
