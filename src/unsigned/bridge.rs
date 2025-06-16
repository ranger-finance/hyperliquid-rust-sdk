//! Bridge-specific functionality for Arbitrum <> Hyperliquid transfers

use ethers::types::Address;

/// Bridge contract addresses
pub const BRIDGE_MAINNET: &str = "0x2df1c51e09aecf9cacb7bc98cb1742757f163df7";
pub const BRIDGE_TESTNET: &str = "0x08cfc1B6b2dCF36A1480b99353A354AA8AC56f89";

/// USDC contract addresses
pub const USDC_MAINNET: &str = "0xaf88d065e77c8cC2239327C5EDb3A432268e5831";
pub const USDC_TESTNET: &str = "0x1baAbB04529D43a73232B713C0FE471f7c7334d5";

/// Minimum deposit amount in USDC (5 USDC)
pub const MIN_DEPOSIT_USDC: u64 = 5_000_000; // 5 USDC in 6 decimal places

/// Get bridge contract address for the given network
pub fn get_bridge_address(is_mainnet: bool) -> Address {
    if is_mainnet {
        BRIDGE_MAINNET.parse().unwrap()
    } else {
        BRIDGE_TESTNET.parse().unwrap()
    }
}

/// Get USDC contract address for the given network
pub fn get_usdc_address(is_mainnet: bool) -> Address {
    if is_mainnet {
        USDC_MAINNET.parse().unwrap()
    } else {
        USDC_TESTNET.parse().unwrap()
    }
}

/// Create USDC transfer transaction data for ERC-20 transfer
pub fn create_usdc_transfer_data(to: Address, amount: ethers::types::U256) -> String {
    // ERC-20 transfer function selector: transfer(address,uint256)
    let selector = "a9059cbb";
    let to_padded = format!("{:064x}", to);
    let amount_padded = format!("{:064x}", amount);
    
    format!("0x{}{}{}", selector, to_padded, amount_padded)
}
