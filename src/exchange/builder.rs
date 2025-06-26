use serde::{Deserialize, Serialize};

/// Builder information for Hyperliquid Builder Codes
///
/// Builder codes allow builders (DeFi application developers) to receive a fee on fills
/// that they send on behalf of a user. This enables developers to monetize their trading
/// applications, bots, and interfaces built on top of Hyperliquid.
///
/// # Integration Flow
/// 1. User must first approve a maximum builder fee for the builder address via `ApproveBuilderFee` action
/// 2. Orders can then include this `BuilderInfo` to charge the specified fee
/// 3. Builder fees are collected in USDC and can be claimed through the referral reward system
///
/// # Fee Limits
/// - Perpetuals: Maximum 0.1% (10 basis points)
/// - Spot: Maximum 1.0% (100 basis points)
///
/// # Example
/// ```rust
/// use hl_ranger::BuilderInfo;
///
/// // Builder charging 0.05% (5 basis points) fee
/// let builder_info = BuilderInfo {
///     builder: "0xF5Bc9107916B91A3Ea5966cd2e51655D21B7Eb02".to_string(),
///     fee: 5, // 5 tenths of basis points = 0.5 basis points = 0.05%
/// };
/// ```
#[derive(Default, Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BuilderInfo {
    /// Builder wallet address in 42-character hexadecimal format
    ///
    /// This is the Ethereum-style address (0x...) of the builder who will receive the fee.
    /// The builder must have at least 100 USDC in perps account value to be eligible.
    ///
    /// # Format
    /// - Must be a valid 42-character hexadecimal string
    /// - Starts with "0x" followed by 40 hexadecimal characters
    /// - Example: "0xF5Bc9107916B91A3Ea5966cd2e51655D21B7Eb02"
    #[serde(rename = "b")]
    pub builder: String,

    /// Builder fee in tenths of basis points
    ///
    /// This represents the fee that will be charged to the user and sent to the builder.
    /// The fee is specified in tenths of basis points, where:
    /// - 1 basis point = 0.01% = 100 tenths of basis points
    /// - 1 tenth of basis point = 0.001%
    ///
    /// # Fee Calculation
    /// - `fee = 1` → 0.1 tenths of basis points → 0.001%
    /// - `fee = 10` → 1.0 basis point → 0.01%
    /// - `fee = 50` → 5.0 basis points → 0.05%
    /// - `fee = 100` → 10.0 basis points → 0.1%
    ///
    /// # Maximum Limits
    /// - Perpetuals: `fee <= 100` (0.1% maximum)
    /// - Spot: `fee <= 1000` (1.0% maximum)
    ///
    /// # Example Values
    /// ```rust
    /// // Examples of fee values:
    /// let fee_5 = 5;   // 0.5 basis points = 0.005%
    /// let fee_10 = 10; // 1.0 basis point = 0.01%
    /// let fee_25 = 25; // 2.5 basis points = 0.025%
    /// let fee_50 = 50; // 5.0 basis points = 0.05%
    /// ```
    #[serde(rename = "f")]
    pub fee: u64,
}
