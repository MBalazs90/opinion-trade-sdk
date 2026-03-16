use opinion_trade_sdk::{
    FixedOrderBook, LocalOrderBook, MarketQuery, OpinionClient, OrderBuilder, Side,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPINION_API_KEY").expect("Set OPINION_API_KEY env var");
    let client = OpinionClient::builder().api_key(&api_key).build()?;

    // === Warm up connection ===
    println!("=== Warming up connection ===");
    client.warm_up().await?;
    println!("  Connection pool ready.");

    // === Markets ===
    println!("\n=== Markets ===");
    let markets = client
        .get_markets(&MarketQuery {
            page: Some(1),
            limit: Some(5),
            status: Some("activated".into()),
            ..Default::default()
        })
        .await?;
    println!("  Total: {}", markets.total);
    for m in &markets.list {
        println!(
            "  [{}] {}",
            m.market_id.unwrap_or(-1),
            m.market_title.as_deref().unwrap_or("(no title)")
        );
    }

    // === Quote tokens ===
    println!("\n=== Quote Tokens ===");
    let tokens = client.get_quote_tokens().await?;
    println!("  Count: {}", tokens.list.len());
    for t in &tokens.list {
        println!(
            "  {} ({}) chain={}",
            t.symbol.as_deref().unwrap_or("?"),
            t.address.as_deref().unwrap_or("?")[..16.min(t.address.as_deref().unwrap_or("").len())]
                .to_string()
                + "...",
            t.chain_id.as_deref().unwrap_or("?")
        );
    }

    // === Try markets to find one with a decent orderbook ===
    let mut market_id = markets.list[0].market_id.unwrap();
    let mut yes_token_found = String::new();
    let mut ob_found = None;

    for m in &markets.list {
        let mid = m.market_id.unwrap();
        let det = client.get_market(mid).await?;
        let yt = det.data.extra["yesTokenId"]
            .as_str()
            .unwrap_or("")
            .to_string();
        if yt.is_empty() {
            continue;
        }
        let ob_result = client.get_orderbook(&yt).await?;
        if !ob_result.bids.is_empty() && !ob_result.asks.is_empty() {
            market_id = mid;
            yes_token_found = yt;
            ob_found = Some(ob_result);
            break;
        }
        // Even a one-sided book is OK as fallback
        if yes_token_found.is_empty() {
            market_id = mid;
            yes_token_found = yt;
            ob_found = Some(ob_result);
        }
    }

    println!("\n=== Market {} Detail ===", market_id);
    let detail = client.get_market(market_id).await?;
    let m = &detail.data;
    println!("  Title: {}", m.market_title.as_deref().unwrap_or("?"));
    println!("  Status: {}", m.status_enum.as_deref().unwrap_or("?"));

    let yes_token = &yes_token_found;
    if yes_token.is_empty() {
        println!("  No YES token found, skipping orderbook tests.");
        return Ok(());
    }
    println!("  YES token: {}", &yes_token[..20.min(yes_token.len())]);

    // === Latest Price ===
    println!("\n=== Latest Price ===");
    let price = client.get_latest_price(yes_token).await?;
    println!(
        "  Price={} Side={} Size={}",
        price.price.as_deref().unwrap_or("?"),
        price.side.as_deref().unwrap_or("?"),
        price.size.as_deref().unwrap_or("?"),
    );

    // === Orderbook ===
    println!("\n=== Orderbook ===");
    let ob = ob_found.unwrap();
    println!(
        "  Bids: {} levels | Asks: {} levels",
        ob.bids.len(),
        ob.asks.len()
    );
    for bid in ob.bids.iter().take(3) {
        println!("    BID {} @ {}", bid.size, bid.price);
    }
    for ask in ob.asks.iter().take(3) {
        println!("    ASK {} @ {}", ask.size, ask.price);
    }

    // === LocalOrderBook (f64) ===
    println!("\n=== LocalOrderBook (f64) ===");
    let local = LocalOrderBook::from_rest(&ob);
    println!("  Best bid: {:?}", local.best_bid());
    println!("  Best ask: {:?}", local.best_ask());
    println!("  Spread:   {:?}", local.spread());
    println!("  Mid:      {:?}", local.mid_price());
    println!("  Wt. mid:  {:?}", local.weighted_mid_price());
    println!(
        "  Bid depth: {} ({:.2} total)",
        local.bid_depth(),
        local.total_bid_size()
    );
    println!(
        "  Ask depth: {} ({:.2} total)",
        local.ask_depth(),
        local.total_ask_size()
    );

    // === FixedOrderBook (u32/i64) ===
    println!("\n=== FixedOrderBook (u32/i64) ===");
    let fixed = FixedOrderBook::from_rest(&ob);
    println!(
        "  Best bid: {:?} (raw: {:?})",
        fixed.best_bid(),
        fixed.best_bid_fixed()
    );
    println!(
        "  Best ask: {:?} (raw: {:?})",
        fixed.best_ask(),
        fixed.best_ask_fixed()
    );
    println!(
        "  Spread:   {:?} (raw: {:?})",
        fixed.spread(),
        fixed.spread_fixed()
    );
    println!(
        "  Bid depth: {} ({:.2} total)",
        fixed.bid_depth(),
        fixed.total_bid_size()
    );
    println!(
        "  Ask depth: {} ({:.2} total)",
        fixed.ask_depth(),
        fixed.total_ask_size()
    );

    // === Market price calculation ===
    if local.ask_depth() > 0 {
        println!("\n=== Market Price (buy 10 units) ===");
        if let Some(p) = local.calculate_market_price(Side::Buy, 10.0) {
            println!("  LocalOrderBook:  {:.6}", p);
        }
        if let Some(p) = fixed.calculate_market_price(Side::Buy, 10.0) {
            println!("  FixedOrderBook:  {:.6}", p);
        }

        // === Market impact ===
        println!("\n=== Market Impact (buy 100 units) ===");
        if let Some(impact) = local.calculate_market_impact(Side::Buy, 100.0) {
            println!("  Avg price: {:.6}", impact.avg_price);
            println!("  Reference: {:.6}", impact.reference_price);
            println!("  Impact:    {:.4}%", impact.impact_pct);
            println!("  Total cost: {:.2}", impact.total_cost);
        } else {
            println!("  Insufficient liquidity for 100 units");
        }

        // === Fill simulation ===
        println!("\n=== Fill Simulation (buy 50 units, 5% slippage) ===");
        match local.simulate_fill(Side::Buy, 50.0, Some(0.05)) {
            opinion_trade_sdk::FillResult::Filled(s) => {
                println!(
                    "  FILLED: avg={:.6} slippage={:.4}% fills={}",
                    s.avg_price,
                    s.slippage * 100.0,
                    s.fills.len()
                );
            }
            opinion_trade_sdk::FillResult::SlippageExceeded {
                actual_slippage, ..
            } => {
                println!("  SLIPPAGE EXCEEDED: {:.4}%", actual_slippage * 100.0);
            }
            opinion_trade_sdk::FillResult::InsufficientLiquidity => {
                println!("  INSUFFICIENT LIQUIDITY");
            }
        }

        // === Order builder ===
        println!("\n=== Order Builder ===");
        if let Some(best_ask) = local.best_ask() {
            let order = OrderBuilder::new(market_id, yes_token, Side::Buy)
                .price(best_ask)
                .amount_in_quote_token(10.0)
                .build()?;
            println!(
                "  Built limit order: price={} side={:?}",
                order.price, order.side
            );
        }

        // Market order from book
        let market_order = OrderBuilder::new(market_id, yes_token, Side::Buy)
            .amount_in_base_token(10.0)
            .max_slippage(0.02)
            .build_market_order(&local)?;
        println!("  Built market order: price={}", market_order.price);
    }

    // === Trades (requires market_id) ===
    println!("\n=== Trades (market {}) ===", market_id);
    match client
        .get_trades(&opinion_trade_sdk::GlobalTradesQuery {
            page: Some(1),
            limit: Some(3),
            market_id: Some(market_id),
            ..Default::default()
        })
        .await
    {
        Ok(trades) => {
            println!("  Total: {} (server caps limit at 20)", trades.total);
            for t in &trades.list {
                println!(
                    "  {} {} @ {} (market {})",
                    t.side.as_deref().unwrap_or("?"),
                    t.extra.get("size").and_then(|v| v.as_str()).unwrap_or("?"),
                    t.price.as_deref().unwrap_or("?"),
                    t.market_id.unwrap_or(-1),
                );
            }
        }
        Err(e) => println!("  Error: {}", e),
    }

    // === Price History ===
    println!("\n=== Price History ===");
    let history = client
        .get_price_history(&opinion_trade_sdk::PriceHistoryQuery {
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

    println!("\n=== All live tests passed! ===");
    Ok(())
}
