use reqwest::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPINION_API_KEY").expect("Set OPINION_API_KEY env var");
    let http = Client::new();
    let base = "https://openapi.opinion.trade/openapi";

    // Balance with chain_id
    println!("=== /user/balance with chain_id ===");
    let resp = http
        .get(format!("{}/user/balance", base))
        .header("apikey", &api_key)
        .query(&[("chain_id", "56")])
        .send()
        .await?;
    println!("  Status: {}", resp.status());
    let body = resp.text().await?;
    println!("  Body: {}", &body[..500.min(body.len())]);

    // Try chainId variant
    println!("\n=== /user/balance with chainId ===");
    let resp = http
        .get(format!("{}/user/balance", base))
        .header("apikey", &api_key)
        .query(&[("chainId", "56")])
        .send()
        .await?;
    println!("  Status: {}", resp.status());
    let body = resp.text().await?;
    println!("  Body: {}", &body[..500.min(body.len())]);

    // User position paths
    println!("\n=== Position paths with chain_id ===");
    for path in ["/user/position", "/user/positions", "/position/user"] {
        let resp = http
            .get(format!("{}{}", base, path))
            .header("apikey", &api_key)
            .query(&[("chain_id", "56"), ("page", "1"), ("limit", "10")])
            .send()
            .await?;
        let status = resp.status();
        if status.is_success() {
            let body = resp.text().await?;
            println!(
                "  {} -> {} ✓ {}",
                path,
                status,
                &body[..300.min(body.len())]
            );
        } else {
            println!("  {} -> {}", path, status);
        }
    }

    // User trades
    println!("\n=== Trade paths with chain_id ===");
    for path in ["/trade/my", "/user/trade", "/user/trades", "/trade/user"] {
        let resp = http
            .get(format!("{}{}", base, path))
            .header("apikey", &api_key)
            .query(&[("chain_id", "56"), ("page", "1"), ("limit", "3")])
            .send()
            .await?;
        let status = resp.status();
        if status.is_success() {
            let body = resp.text().await?;
            println!(
                "  {} -> {} ✓ {}",
                path,
                status,
                &body[..300.min(body.len())]
            );
        } else {
            println!("  {} -> {}", path, status);
        }
    }

    Ok(())
}
