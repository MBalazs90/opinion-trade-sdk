use opinion_trade_sdk::{OnChainClient, format_amount_18};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = OnChainClient::builder()
        .private_key_from_env("OPINION_PRIVATE_KEY")
        .build()
        .await?;

    println!("Wallet: {}", client.wallet_address());

    let bnb = client.bnb_balance().await?;
    println!("BNB:    {}", format_amount_18(bnb));

    let usdt = client.usdt_balance().await?;
    println!("USDT:   {}", format_amount_18(usdt));

    let status = client.check_trading_enabled().await?;
    println!(
        "Trading enabled: {} (USDT={}, CT={})",
        status.is_enabled(),
        status.usdt_approved,
        status.conditional_tokens_approved
    );

    Ok(())
}
