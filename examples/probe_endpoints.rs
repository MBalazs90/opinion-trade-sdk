use reqwest::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPINION_API_KEY").expect("Set OPINION_API_KEY env var");
    let http = Client::new();

    let bases = [
        "https://openapi.opinion.trade/openapi",
        "https://proxy.opinion.trade:8443",
    ];

    let paths = [
        "/balance",
        "/balance/my",
        "/user/balance",
        "/position",
        "/position/my",
        "/user/position",
        "/trade/my",
        "/trade/user",
        "/user/trades",
        "/order",
        "/order/my",
    ];

    for base in &bases {
        println!("=== {} ===", base);
        for path in &paths {
            let url = format!("{}{}", base, path);
            match http
                .get(&url)
                .header("apikey", &api_key)
                .query(&[("page", "1"), ("limit", "3")])
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let body = resp.text().await?;
                        let trunc: String = body.chars().take(200).collect();
                        println!("  {} -> {} ✓ {}", path, status, trunc);
                    } else {
                        println!("  {} -> {}", path, status);
                    }
                }
                Err(e) => println!("  {} -> ERR: {}", path, e),
            }
        }
        println!();
    }

    Ok(())
}
