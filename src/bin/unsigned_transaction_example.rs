use hl_ranger::prelude::Result;
use hl_ranger::{
    BaseUrl, ClientCancelRequest, ClientLimit, ClientOrder, ClientOrderRequest,
    UnsignedTransactionBuilder, UnsignedTransactionComponents,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the unsigned transaction builder for testnet
    let builder = UnsignedTransactionBuilder::new(
        None,                   // Use default HTTP client
        Some(BaseUrl::Testnet), // Use testnet
        None,                   // Fetch meta automatically
        None,                   // No vault address
    )
    .await?;

    println!("✅ UnsignedTransactionBuilder initialized successfully");

    // Example 1: Prepare an unsigned order
    println!("\n📝 Example 1: Preparing unsigned order");
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

    print_unsigned_components("Order", &unsigned_order);

    // Example 2: Prepare an unsigned USDC transfer
    println!("\n💸 Example 2: Preparing unsigned USDC transfer");
    let unsigned_transfer = builder
        .prepare_unsigned_usdc_transfer("100.0", "0x1234567890123456789012345678901234567890")
        .await?;

    print_unsigned_components("USDC Transfer", &unsigned_transfer);

    // Example 3: Prepare an unsigned cancel request
    println!("\n❌ Example 3: Preparing unsigned cancel");
    let cancel_request = ClientCancelRequest {
        asset: "ETH".to_string(),
        oid: 12345,
    };

    let unsigned_cancel = builder.prepare_unsigned_cancel(cancel_request).await?;

    print_unsigned_components("Cancel", &unsigned_cancel);

    // Example 4: Prepare an unsigned withdraw
    println!("\n🏦 Example 4: Preparing unsigned withdraw");
    let unsigned_withdraw = builder
        .prepare_unsigned_withdraw("50.0", "0x1234567890123456789012345678901234567890")
        .await?;

    print_unsigned_components("Withdraw", &unsigned_withdraw);

    println!("\n🎉 All examples completed successfully!");
    println!("\n📋 Next steps:");
    println!("1. Take the digest_to_sign and sign it with your private key");
    println!("2. Construct the final ExchangePayload with:");
    println!("   - action: action_payload_json");
    println!("   - signature: your_signature");
    println!("   - nonce: nonce");
    println!("   - vault_address: vault_address (if applicable)");
    println!("3. Send the signed payload to Hyperliquid");

    Ok(())
}

fn print_unsigned_components(action_type: &str, components: &UnsignedTransactionComponents) {
    println!("  Action Type: {action_type}");
    println!("  Nonce: {}", components.nonce);
    println!("  Digest to Sign: {:?}", components.digest_to_sign);
    println!(
        "  Is L1 Agent Signature: {}",
        components.is_l1_agent_signature
    );
    println!("  Vault Address: {:?}", components.vault_address);
    println!(
        "  EIP-712 Domain Chain ID: {:?}",
        components.eip712_domain_chain_id
    );
    println!(
        "  EIP-712 Hyperliquid Chain Name: {:?}",
        components.eip712_hyperliquid_chain_name
    );
    println!(
        "  Action Payload JSON: {}",
        serde_json::to_string_pretty(&components.action_payload_json)
            .unwrap_or_else(|_| "Failed to serialize".to_string())
    );
}
