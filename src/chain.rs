/// On-chain operations for opinion.trade (BNB Chain).
///
/// Provides `enable_trading`, `split`, `merge`, and `redeem` — the four smart contract
/// operations that require a private key for transaction signing.
///
/// Enabled via the `chain` cargo feature:
/// ```toml
/// opinion_trade_sdk = { path = ".", features = ["chain"] }
/// ```
use alloy::{
    network::EthereumWallet,
    primitives::{Address, FixedBytes, U256, address},
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
};

use crate::chain_abi::{IConditionalTokens, IERC20, IERC1155};
use crate::error::{Result, SdkError};

/// Default BSC RPC endpoint.
const DEFAULT_BSC_RPC: &str = "https://bsc-dataseed.binance.org/";

/// ConditionalTokens contract address on BNB Chain.
pub const CONDITIONAL_TOKENS_ADDR: Address = address!("AD1a38cEc043e70E83a3eC30443dB285ED10D774");

/// Default USDT address on BNB Chain (BSC).
pub const USDT_ADDR: Address = address!("55d398326f99059fF775485246999027B3197955");

/// BNB Chain ID.
pub const BSC_CHAIN_ID: u64 = 56;

/// Binary market partition: YES = index 0 (bitmask 1), NO = index 1 (bitmask 2).
const BINARY_PARTITION: [U256; 2] = [
    U256::from_limbs([1, 0, 0, 0]),
    U256::from_limbs([2, 0, 0, 0]),
];

/// Zero parent collection ID (for top-level conditions).
const ZERO_PARENT: FixedBytes<32> = FixedBytes::ZERO;

/// Result of an on-chain transaction.
#[derive(Debug, Clone)]
pub struct TxResult {
    /// Transaction hash.
    pub tx_hash: String,
}

/// Builder for `OnChainClient`.
#[derive(Debug, Clone)]
pub struct OnChainClientBuilder {
    rpc_url: String,
    private_key: Option<String>,
    usdt_address: Address,
    conditional_tokens_address: Address,
}

impl Default for OnChainClientBuilder {
    fn default() -> Self {
        Self {
            rpc_url: DEFAULT_BSC_RPC.to_string(),
            private_key: None,
            usdt_address: USDT_ADDR,
            conditional_tokens_address: CONDITIONAL_TOKENS_ADDR,
        }
    }
}

impl OnChainClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the JSON-RPC URL (default: BSC mainnet public endpoint).
    pub fn rpc_url(mut self, url: impl Into<String>) -> Self {
        self.rpc_url = url.into();
        self
    }

    /// Set the private key (hex string, with or without 0x prefix).
    pub fn private_key(mut self, key: impl Into<String>) -> Self {
        self.private_key = Some(key.into());
        self
    }

    /// Read the private key from an environment variable.
    pub fn private_key_from_env(mut self, var_name: &str) -> Self {
        self.private_key = std::env::var(var_name).ok();
        self
    }

    /// Override the USDT contract address.
    pub fn usdt_address(mut self, addr: Address) -> Self {
        self.usdt_address = addr;
        self
    }

    /// Override the ConditionalTokens contract address.
    pub fn conditional_tokens_address(mut self, addr: Address) -> Self {
        self.conditional_tokens_address = addr;
        self
    }

    /// Build the `OnChainClient`.
    pub async fn build(self) -> Result<OnChainClient> {
        let key_hex = self.private_key.ok_or(SdkError::MissingPrivateKey)?;
        let key_hex = key_hex.strip_prefix("0x").unwrap_or(&key_hex);

        let signer: PrivateKeySigner = key_hex
            .parse()
            .map_err(|e| SdkError::Chain(format!("invalid private key: {e}")))?;

        let wallet_address = signer.address();
        let wallet = EthereumWallet::from(signer);

        let rpc_url: url::Url = self
            .rpc_url
            .parse()
            .map_err(|e| SdkError::Chain(format!("invalid RPC URL: {e}")))?;

        let provider = ProviderBuilder::new().wallet(wallet).connect_http(rpc_url);

        Ok(OnChainClient {
            provider,
            wallet_address,
            usdt_address: self.usdt_address,
            conditional_tokens_address: self.conditional_tokens_address,
        })
    }
}

type BoxedProvider = alloy::providers::fillers::FillProvider<
    alloy::providers::fillers::JoinFill<
        alloy::providers::fillers::JoinFill<
            alloy::providers::Identity,
            alloy::providers::fillers::JoinFill<
                alloy::providers::fillers::GasFiller,
                alloy::providers::fillers::JoinFill<
                    alloy::providers::fillers::BlobGasFiller,
                    alloy::providers::fillers::JoinFill<
                        alloy::providers::fillers::NonceFiller,
                        alloy::providers::fillers::ChainIdFiller,
                    >,
                >,
            >,
        >,
        alloy::providers::fillers::WalletFiller<EthereumWallet>,
    >,
    alloy::providers::RootProvider,
    alloy::network::Ethereum,
>;

/// Client for on-chain operations on BNB Chain.
///
/// Handles `enable_trading`, `split`, `merge`, and `redeem` via direct
/// smart contract calls signed with a local private key.
impl std::fmt::Debug for OnChainClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnChainClient")
            .field("wallet_address", &self.wallet_address)
            .field("usdt_address", &self.usdt_address)
            .field(
                "conditional_tokens_address",
                &self.conditional_tokens_address,
            )
            .finish()
    }
}

pub struct OnChainClient {
    provider: BoxedProvider,
    wallet_address: Address,
    usdt_address: Address,
    conditional_tokens_address: Address,
}

impl OnChainClient {
    pub fn builder() -> OnChainClientBuilder {
        OnChainClientBuilder::default()
    }

    /// The wallet address derived from the private key.
    pub fn wallet_address(&self) -> Address {
        self.wallet_address
    }

    /// Grant ERC20 (USDT) and ERC1155 (ConditionalTokens) approvals to the exchange.
    ///
    /// This is a one-time setup operation. Approves `U256::MAX` for USDT and
    /// sets `isApprovedForAll` for outcome tokens.
    ///
    /// Returns two transaction results: (USDT approval, ConditionalTokens approval).
    pub async fn enable_trading(&self) -> Result<(TxResult, TxResult)> {
        // 1. Approve USDT
        let usdt = IERC20::new(self.usdt_address, &self.provider);
        let tx1 = usdt
            .approve(self.conditional_tokens_address, U256::MAX)
            .send()
            .await
            .map_err(|e| SdkError::Chain(format!("USDT approve failed: {e}")))?
            .watch()
            .await
            .map_err(|e| SdkError::Chain(format!("USDT approve watch failed: {e}")))?;

        // 2. Approve ConditionalTokens for outcome token transfers
        let ct_as_1155 = IERC1155::new(self.conditional_tokens_address, &self.provider);
        let tx2 = ct_as_1155
            .setApprovalForAll(self.conditional_tokens_address, true)
            .send()
            .await
            .map_err(|e| SdkError::Chain(format!("ERC1155 approval failed: {e}")))?
            .watch()
            .await
            .map_err(|e| SdkError::Chain(format!("ERC1155 approval watch failed: {e}")))?;

        Ok((
            TxResult {
                tx_hash: format!("{tx1:#x}"),
            },
            TxResult {
                tx_hash: format!("{tx2:#x}"),
            },
        ))
    }

    /// Check if trading is already enabled (approvals are set).
    pub async fn check_trading_enabled(&self) -> Result<TradingStatus> {
        let usdt = IERC20::new(self.usdt_address, &self.provider);
        let allowance = usdt
            .allowance(self.wallet_address, self.conditional_tokens_address)
            .call()
            .await
            .map_err(|e| SdkError::Chain(format!("allowance check failed: {e}")))?;

        let ct_as_1155 = IERC1155::new(self.conditional_tokens_address, &self.provider);
        let approved = ct_as_1155
            .isApprovedForAll(self.wallet_address, self.conditional_tokens_address)
            .call()
            .await
            .map_err(|e| SdkError::Chain(format!("approval check failed: {e}")))?;

        Ok(TradingStatus {
            usdt_approved: allowance > U256::ZERO,
            conditional_tokens_approved: approved,
        })
    }

    /// Split USDT collateral into YES + NO outcome tokens.
    ///
    /// `condition_id`: The condition ID for the market (from `Market.extra["conditionId"]`).
    /// `amount`: Amount in human-readable form (e.g., "100.5" for 100.5 USDT).
    pub async fn split(&self, condition_id: FixedBytes<32>, amount: &str) -> Result<TxResult> {
        let amount_wei = parse_amount_18(amount)?;
        let ct = IConditionalTokens::new(self.conditional_tokens_address, &self.provider);

        let tx = ct
            .splitPosition(
                self.usdt_address,
                ZERO_PARENT,
                condition_id,
                BINARY_PARTITION.to_vec(),
                amount_wei,
            )
            .send()
            .await
            .map_err(|e| SdkError::Chain(format!("split failed: {e}")))?
            .watch()
            .await
            .map_err(|e| SdkError::Chain(format!("split watch failed: {e}")))?;

        Ok(TxResult {
            tx_hash: format!("{tx:#x}"),
        })
    }

    /// Split with raw U256 amount (in wei, 18 decimals).
    pub async fn split_raw(
        &self,
        condition_id: FixedBytes<32>,
        amount_wei: U256,
    ) -> Result<TxResult> {
        let ct = IConditionalTokens::new(self.conditional_tokens_address, &self.provider);

        let tx = ct
            .splitPosition(
                self.usdt_address,
                ZERO_PARENT,
                condition_id,
                BINARY_PARTITION.to_vec(),
                amount_wei,
            )
            .send()
            .await
            .map_err(|e| SdkError::Chain(format!("split failed: {e}")))?
            .watch()
            .await
            .map_err(|e| SdkError::Chain(format!("split watch failed: {e}")))?;

        Ok(TxResult {
            tx_hash: format!("{tx:#x}"),
        })
    }

    /// Merge YES + NO outcome tokens back into USDT collateral.
    ///
    /// `amount`: Amount in human-readable form (e.g., "50" for 50 token pairs).
    pub async fn merge(&self, condition_id: FixedBytes<32>, amount: &str) -> Result<TxResult> {
        let amount_wei = parse_amount_18(amount)?;
        let ct = IConditionalTokens::new(self.conditional_tokens_address, &self.provider);

        let tx = ct
            .mergePositions(
                self.usdt_address,
                ZERO_PARENT,
                condition_id,
                BINARY_PARTITION.to_vec(),
                amount_wei,
            )
            .send()
            .await
            .map_err(|e| SdkError::Chain(format!("merge failed: {e}")))?
            .watch()
            .await
            .map_err(|e| SdkError::Chain(format!("merge watch failed: {e}")))?;

        Ok(TxResult {
            tx_hash: format!("{tx:#x}"),
        })
    }

    /// Merge with raw U256 amount (in wei, 18 decimals).
    pub async fn merge_raw(
        &self,
        condition_id: FixedBytes<32>,
        amount_wei: U256,
    ) -> Result<TxResult> {
        let ct = IConditionalTokens::new(self.conditional_tokens_address, &self.provider);

        let tx = ct
            .mergePositions(
                self.usdt_address,
                ZERO_PARENT,
                condition_id,
                BINARY_PARTITION.to_vec(),
                amount_wei,
            )
            .send()
            .await
            .map_err(|e| SdkError::Chain(format!("merge failed: {e}")))?
            .watch()
            .await
            .map_err(|e| SdkError::Chain(format!("merge watch failed: {e}")))?;

        Ok(TxResult {
            tx_hash: format!("{tx:#x}"),
        })
    }

    /// Redeem winning positions from a resolved market.
    ///
    /// Converts winning outcome tokens into USDT. Redeems all available tokens.
    pub async fn redeem(&self, condition_id: FixedBytes<32>) -> Result<TxResult> {
        let ct = IConditionalTokens::new(self.conditional_tokens_address, &self.provider);

        let tx = ct
            .redeemPositions(
                self.usdt_address,
                ZERO_PARENT,
                condition_id,
                BINARY_PARTITION.to_vec(),
            )
            .send()
            .await
            .map_err(|e| SdkError::Chain(format!("redeem failed: {e}")))?
            .watch()
            .await
            .map_err(|e| SdkError::Chain(format!("redeem watch failed: {e}")))?;

        Ok(TxResult {
            tx_hash: format!("{tx:#x}"),
        })
    }

    /// Get the USDT balance of the wallet.
    pub async fn usdt_balance(&self) -> Result<U256> {
        let usdt = IERC20::new(self.usdt_address, &self.provider);
        let bal = usdt
            .balanceOf(self.wallet_address)
            .call()
            .await
            .map_err(|e| SdkError::Chain(format!("balance check failed: {e}")))?;
        Ok(bal)
    }

    /// Get the native BNB balance of the wallet (for gas).
    pub async fn bnb_balance(&self) -> Result<U256> {
        let bal = self
            .provider
            .get_balance(self.wallet_address)
            .await
            .map_err(|e| SdkError::Chain(format!("BNB balance check failed: {e}")))?;
        Ok(bal)
    }
}

/// Status of trading approvals.
#[derive(Debug, Clone)]
pub struct TradingStatus {
    /// Whether USDT is approved for the exchange.
    pub usdt_approved: bool,
    /// Whether ConditionalTokens (outcome tokens) are approved.
    pub conditional_tokens_approved: bool,
}

impl TradingStatus {
    /// Returns true if both approvals are set.
    pub fn is_enabled(&self) -> bool {
        self.usdt_approved && self.conditional_tokens_approved
    }
}

/// Parse a human-readable amount string (e.g., "100.5") to U256 with 18 decimals.
pub fn parse_amount_18(amount: &str) -> Result<U256> {
    let amount = amount.trim();
    if amount.is_empty() {
        return Err(SdkError::Validation("amount cannot be empty".into()));
    }

    let parts: Vec<&str> = amount.split('.').collect();
    if parts.len() > 2 {
        return Err(SdkError::Validation("invalid amount format".into()));
    }

    let integer_str = parts[0];
    let frac_str = if parts.len() == 2 { parts[1] } else { "" };

    if frac_str.len() > 18 {
        return Err(SdkError::Validation(
            "amount has more than 18 decimal places".into(),
        ));
    }

    // Pad fractional part to 18 digits
    let padded = format!("{frac_str:0<18}");

    let integer: U256 = integer_str
        .parse()
        .map_err(|_| SdkError::Validation(format!("invalid integer part: {integer_str}")))?;
    let fractional: U256 = padded
        .parse()
        .map_err(|_| SdkError::Validation(format!("invalid fractional part: {frac_str}")))?;

    let decimals = U256::from(10u64).pow(U256::from(18u64));
    Ok(integer * decimals + fractional)
}

/// Format a U256 wei amount (18 decimals) to a human-readable string.
pub fn format_amount_18(amount: U256) -> String {
    let decimals = U256::from(10u64).pow(U256::from(18u64));
    let integer = amount / decimals;
    let fractional = amount % decimals;
    if fractional.is_zero() {
        format!("{integer}")
    } else {
        let frac_str = format!("{fractional:0>18}");
        let trimmed = frac_str.trim_end_matches('0');
        format!("{integer}.{trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_amount_integer() {
        let r = parse_amount_18("100").unwrap();
        let expected = U256::from(100u64) * U256::from(10u64).pow(U256::from(18u64));
        assert_eq!(r, expected);
    }

    #[test]
    fn parse_amount_with_decimals() {
        let r = parse_amount_18("100.5").unwrap();
        let expected = U256::from(100u64) * U256::from(10u64).pow(U256::from(18u64))
            + U256::from(5u64) * U256::from(10u64).pow(U256::from(17u64));
        assert_eq!(r, expected);
    }

    #[test]
    fn parse_amount_small() {
        let r = parse_amount_18("0.000000000000000001").unwrap();
        assert_eq!(r, U256::from(1u64));
    }

    #[test]
    fn parse_amount_zero() {
        let r = parse_amount_18("0").unwrap();
        assert_eq!(r, U256::ZERO);
    }

    #[test]
    fn parse_amount_empty_errors() {
        assert!(parse_amount_18("").is_err());
    }

    #[test]
    fn parse_amount_too_many_decimals() {
        assert!(parse_amount_18("1.0000000000000000001").is_err());
    }

    #[test]
    fn format_amount_integer() {
        let v = U256::from(100u64) * U256::from(10u64).pow(U256::from(18u64));
        assert_eq!(format_amount_18(v), "100");
    }

    #[test]
    fn format_amount_with_decimals() {
        let v = U256::from(100u64) * U256::from(10u64).pow(U256::from(18u64))
            + U256::from(5u64) * U256::from(10u64).pow(U256::from(17u64));
        assert_eq!(format_amount_18(v), "100.5");
    }

    #[test]
    fn format_amount_zero() {
        assert_eq!(format_amount_18(U256::ZERO), "0");
    }

    #[test]
    fn format_amount_1_wei() {
        assert_eq!(format_amount_18(U256::from(1u64)), "0.000000000000000001");
    }

    #[test]
    fn roundtrip_amount() {
        for s in ["0", "1", "100.5", "999.123456789012345678"] {
            let parsed = parse_amount_18(s).unwrap();
            let formatted = format_amount_18(parsed);
            assert_eq!(formatted, s, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn builder_missing_key_errors() {
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(OnChainClientBuilder::new().build());
        assert!(matches!(result.unwrap_err(), SdkError::MissingPrivateKey));
    }

    #[test]
    fn builder_invalid_key_errors() {
        let result = tokio::runtime::Runtime::new().unwrap().block_on(
            OnChainClientBuilder::new()
                .private_key("not_a_valid_hex_key")
                .build(),
        );
        assert!(matches!(result.unwrap_err(), SdkError::Chain(_)));
    }

    #[test]
    fn trading_status_is_enabled() {
        let status = TradingStatus {
            usdt_approved: true,
            conditional_tokens_approved: true,
        };
        assert!(status.is_enabled());

        let status2 = TradingStatus {
            usdt_approved: true,
            conditional_tokens_approved: false,
        };
        assert!(!status2.is_enabled());
    }

    #[test]
    fn binary_partition_values() {
        assert_eq!(BINARY_PARTITION[0], U256::from(1u64));
        assert_eq!(BINARY_PARTITION[1], U256::from(2u64));
    }
}
