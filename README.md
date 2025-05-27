# hl_ranger (Hyperliquid Rust SDK Fork)

**This is a fork of the official [hyperliquid-rust-sdk](https://github.com/hyperliquid-dex/hyperliquid-rust-sdk) with added support for unsigned transaction generation.**

SDK for Hyperliquid API trading with Rust, enhanced with the ability to generate unsigned transactions for external signing.

## Key Features

- **All original hyperliquid-rust-sdk functionality** - Complete compatibility with the upstream SDK
- **Unsigned Transaction Support** - Generate transaction components without requiring a private key
- **External Signing** - Perfect for hardware wallets, multi-sig setups, or air-gapped signing
- **Zero Merge Conflicts** - Isolated implementation that won't conflict with upstream updates

## New: Unsigned Transaction Generation

This fork introduces the `UnsignedTransactionBuilder` which allows you to:

1. Generate all necessary transaction components (action payload, nonce, digest to sign)
2. Sign the digest externally with your preferred method
3. Construct and submit the final signed transaction

### Example Usage

```rust
use hl_ranger::prelude::Result;
use hl_ranger::{
    BaseUrl, ClientLimit, ClientOrder, ClientOrderRequest,
    UnsignedTransactionBuilder, UnsignedTransactionComponents,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the unsigned transaction builder
    let builder = UnsignedTransactionBuilder::new(
        None,                   // Use default HTTP client
        Some(BaseUrl::Testnet), // Use testnet
        None,                   // Fetch meta automatically
        None,                   // No vault address
    ).await?;

    // Prepare an unsigned order
    let order = ClientOrderRequest {
        asset: "ETH".to_string(),
        is_buy: true,
        limit_px: 2000.0,
        sz: 0.1,
        order_type: ClientOrder::Limit(ClientLimit {
            tif: "Gtc".to_string(),
        }),
        reduce_only: false,
        cloid: None,
    };

    let unsigned_order = builder
        .prepare_unsigned_order(order, Some("example_group".to_string()))
        .await?;

    // Now you have:
    // - unsigned_order.digest_to_sign: The hash to sign with your private key
    // - unsigned_order.action_payload_json: The action for the final payload
    // - unsigned_order.nonce: The nonce for the transaction
    
    // Sign the digest externally and construct the final ExchangePayload
    Ok(())
}
```

### Supported Unsigned Operations

- `prepare_unsigned_order` - Place orders
- `prepare_unsigned_cancel` - Cancel orders  
- `prepare_unsigned_usdc_transfer` - Transfer USDC
- `prepare_unsigned_withdraw` - Withdraw funds
- `prepare_unsigned_update_leverage` - Update leverage
- `prepare_unsigned_spot_transfer` - Spot transfers
- `prepare_unsigned_vault_transfer` - Vault transfers

## Usage Examples

See `src/bin` for examples. You can run any example with `cargo run --bin [EXAMPLE]`.

For unsigned transaction examples, see:
- `cargo run --bin unsigned_transaction_example`

## Installation

`cargo add hl_ranger`

## Original SDK Documentation

This fork maintains full compatibility with the original hyperliquid-rust-sdk. All original functionality remains unchanged.

## License

This project is licensed under the terms of the `MIT` license. See [LICENSE](LICENSE.md) for more details.

```bibtex
@misc{hl-ranger,
  author = {HL Ranger (Fork of Hyperliquid)},
  title = {Fork of Hyperliquid Rust SDK with unsigned transaction support},
  year = {2024},
  publisher = {GitHub},
  journal = {GitHub repository},
  howpublished = {\url{https://github.com/hl-ranger/hyperliquid-rust-sdk}}
}
```
