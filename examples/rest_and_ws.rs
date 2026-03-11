use opinion_trade_sdk::{MarketQuery, OpinionClient, OpinionWsClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPINION_API_KEY")?;

    let client = OpinionClient::builder().api_key(&api_key).build()?;
    let markets = client
        .get_markets(&MarketQuery {
            page: Some(1),
            limit: Some(5),
            status: Some("activated".into()),
            ..Default::default()
        })
        .await?;

    println!("top markets: {}", markets.list.len());

    let mut ws = OpinionWsClient::connect(&api_key).await?;
    ws.heartbeat().await?;

    if let Some(first_market) = markets.list.first().and_then(|m| m.market_id) {
        ws.subscribe_market("market", first_market).await?;
        if let Some(msg) = ws.next_json().await? {
            println!("first ws message: {msg}");
        }
    }

    ws.close().await?;
    Ok(())
}
