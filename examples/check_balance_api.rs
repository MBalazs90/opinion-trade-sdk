use opinion_trade_sdk::OpinionClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPINION_API_KEY").expect("Set OPINION_API_KEY env var");
    let client = OpinionClient::builder().api_key(&api_key).build()?;

    // === Balances ===
    println!("=== opinion.trade Account ===");
    let data = client.get_my_balances("56").await?;
    println!(
        "  Wallet:     {}",
        data.wallet_address.as_deref().unwrap_or("?")
    );
    println!(
        "  Multi-sig:  {}",
        data.multi_sign_address.as_deref().unwrap_or("?")
    );
    println!("  Chain:      {}", data.chain_id.as_deref().unwrap_or("?"));
    for b in &data.balances {
        println!("  Token:      {}", b.quote_token.as_deref().unwrap_or("?"));
        println!(
            "  Total:      {} | Available: {} | Frozen: {}",
            b.total_balance.as_deref().unwrap_or("0"),
            b.available_balance.as_deref().unwrap_or("0"),
            b.frozen_balance.as_deref().unwrap_or("0"),
        );
    }

    // === Open Orders ===
    println!("\n=== Open Orders ===");
    let orders = client
        .get_orders(&opinion_trade_sdk::OrderQuery {
            page: Some(1),
            limit: Some(20),
            ..Default::default()
        })
        .await?;
    println!("  Total: {}", orders.total);
    for o in &orders.list {
        println!(
            "  {} | market={} | status={} | price={}",
            o.order_id.as_deref().unwrap_or("?"),
            o.market_id.unwrap_or(-1),
            o.status_enum.as_deref().unwrap_or("?"),
            o.price.as_deref().unwrap_or("?"),
        );
    }
    if orders.list.is_empty() {
        println!("  No open orders");
    }

    Ok(())
}
