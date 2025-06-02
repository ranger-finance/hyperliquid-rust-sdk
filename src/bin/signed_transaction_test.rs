use ethers::signers::{LocalWallet, Signer};
use ethers::types::{Signature, H256};
use hl_ranger::exchange::ExchangeClient;
use hl_ranger::prelude::Result;
use hl_ranger::{
    BaseUrl, ClientCancelRequest, ClientLimit, ClientModifyRequest, ClientOrder,
    ClientOrderRequest, ExchangeDataStatus, ExchangeResponseStatus, UnsignedTransactionBuilder,
    UnsignedTransactionComponents,
};
use log::{error, info};
use std::str::FromStr;
use std::{env, thread, time};
use uuid::Uuid;

// Load private key from environment variable
fn get_test_private_key() -> Result<String> {
    // Load .env file if it exists (for development)
    dotenv::dotenv().ok();

    env::var("TEST_PRIVATE_KEY")
        .map_err(|_| hl_ranger::Error::GenericRequest(
            "TEST_PRIVATE_KEY environment variable not found. Please set it in your .env file or environment.".to_string()
        ))
}

// Helper function to sign a pre-computed hash (digest)
// This function is a simplified version of what's in the SDK's signature module
// It's exposed here for direct use in testing the signing of UnsignedTransactionComponents.
fn sign_digest(hash: H256, wallet: &LocalWallet) -> Result<Signature> {
    // The Hyperliquid SDK uses Sha256Proxy for signing, which effectively means it signs the H256 directly.
    // ethers::signers::Signer::sign_hash can be used if the hash is treated as a message hash.
    // However, Hyperliquid's EIP-712 signing process involves specific structures.
    // For L1 agent actions, the structure is `hl_ranger::signature::agent::l1::Agent`.
    // For other actions (like USDC transfer), it's the action struct itself (e.g., `hl_ranger::UsdSend`).
    // The `UnsignedTransactionBuilder` already provides the final `digest_to_sign`.
    // We just need to sign this H256 digest.
    // The `wallet.sign_hash(hash)` method is appropriate here.
    wallet
        .sign_hash(hash)
        .map_err(|e| hl_ranger::Error::SignatureFailure(e.to_string()))
}

async fn sign_and_post_transaction(
    components: UnsignedTransactionComponents,
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
    agent_key: Option<&str>,
) -> Result<ExchangeResponseStatus> {
    info!("Digest to sign: {:?}", components.digest_to_sign);

    // Choose the correct wallet for signing based on transaction type
    let signing_wallet = if components.is_l1_agent_signature {
        if let Some(key) = agent_key {
            info!("Using agent key for L1 agent signature");
            LocalWallet::from_str(key)
                .map_err(|e| hl_ranger::Error::PrivateKeyParse(e.to_string()))?
        } else {
            return Err(hl_ranger::Error::GenericRequest(
                "L1 agent signature required but no agent key provided".to_string(),
            ));
        }
    } else {
        info!("Using main wallet for EIP-712 direct signature");
        wallet.clone()
    };

    let signature = sign_digest(components.digest_to_sign, &signing_wallet)?;
    info!(
        "Generated Signature: r: {}, s: {}, v: {}",
        signature.r, signature.s, signature.v
    );

    // The ExchangeClient's internal post method is not public.
    // We need to replicate its functionality or use a similar public method if available.
    // For now, let's use the existing `exchange_client.post` by making it pub(crate) or creating a helper.
    // Looking at `ExchangeClient`, the `post` method is private.
    // We will call it directly here.
    // To do this, we need to construct the `ExchangePayload` manually and serialize it.

    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ExchangePayloadInternal<'a> {
        action: &'a serde_json::Value,
        signature: Signature,
        nonce: u64,
        vault_address: Option<ethers::types::H160>,
    }

    let exchange_payload_internal = ExchangePayloadInternal {
        action: &components.action_payload_json,
        signature,
        nonce: components.nonce,
        vault_address: components.vault_address,
    };

    let payload_str = serde_json::to_string(&exchange_payload_internal)
        .map_err(|e| hl_ranger::Error::JsonParse(e.to_string()))?;

    info!("Sending payload: {}", payload_str);

    let response_str = exchange_client
        .http_client
        .post("/exchange", payload_str)
        .await
        .map_err(|e| {
            error!("HTTP request failed: {}", e);
            hl_ranger::Error::GenericRequest(e.to_string())
        })?;

    info!("Received response string: '{}'", response_str);
    info!("Response length: {} bytes", response_str.len());
    info!(
        "Response bytes (first 200): {:?}",
        response_str.chars().take(200).collect::<String>()
    );

    if response_str.is_empty() {
        error!("Server returned empty response!");
        return Err(hl_ranger::Error::GenericRequest(
            "Server returned empty response".to_string(),
        ));
    }

    // Try to parse the response, but provide more detailed error info if it fails
    serde_json::from_str(&response_str).map_err(|e| {
        error!("Failed to parse response JSON: {}", e);
        error!("Raw response was: '{}'", response_str);
        error!("Response as bytes: {:?}", response_str.as_bytes());
        error!("Is valid UTF-8: {}", response_str.is_ascii());

        // Try to identify what type of response this might be
        if response_str.starts_with('<') {
            error!("Response appears to be HTML (possibly an error page)");
        } else if response_str.starts_with('{') || response_str.starts_with('[') {
            error!("Response appears to be JSON but failed to parse");
        } else {
            error!("Response format is unknown");
        }

        hl_ranger::Error::JsonParse(format!(
            "Failed to parse response: {}. Raw response: '{}'",
            e, response_str
        ))
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    info!(
        "Starting signed transaction tests with key: {}",
        get_test_private_key()?
    );

    let wallet = LocalWallet::from_str(&get_test_private_key()?)
        .map_err(|e| hl_ranger::Error::Wallet(e.to_string()))?;
    info!("Wallet address: {:?}", wallet.address());

    let unsigned_builder = UnsignedTransactionBuilder::new(
        None,
        Some(BaseUrl::Testnet),
        None,
        None, // No vault address for these general tests unless specified
    )
    .await?;
    info!("UnsignedTransactionBuilder initialized for Testnet.");

    let exchange_client =
        ExchangeClient::new(None, wallet.clone(), Some(BaseUrl::Testnet), None, None).await?;
    info!("ExchangeClient initialized for Testnet.");

    // Track all opened orders for cleanup
    let mut opened_orders: Vec<u64> = Vec::new();

    // Test 1: Main wallet direct orders (using ExchangeClient)
    info!("\n🔹 TEST 1: Main Wallet Direct Orders");
    match test_main_wallet_orders(&exchange_client, &wallet).await {
        Ok(oids) => {
            opened_orders.extend(oids);
            info!("✅ Main wallet test completed successfully");
        }
        Err(e) => {
            error!("❌ Main wallet test failed: {:?}", e);
        }
    }

    // Test 2: Agent wallet orders (using approved agent)
    info!("\n🔹 TEST 2: Agent Wallet Orders");
    match test_agent_wallet_orders(&exchange_client, &wallet).await {
        Ok(oids) => {
            opened_orders.extend(oids);
            info!("✅ Agent wallet test completed successfully");
        }
        Err(e) => {
            error!("❌ Agent wallet test failed: {:?}", e);
        }
    }

    // Clean up all opened orders
    if !opened_orders.is_empty() {
        info!(
            "\n🧹 CLEANUP: Canceling {} opened orders",
            opened_orders.len()
        );
        cleanup_orders(&exchange_client, &wallet, opened_orders).await;
    } else {
        info!("No orders to clean up.");
    }

    info!("\n✅ All tests completed!");
    Ok(())
}

async fn test_main_wallet_orders(
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<Vec<u64>> {
    info!("📋 Testing main wallet direct orders (no agent approval needed)");
    let mut opened_orders = Vec::new();

    // Place a limit order using main wallet directly
    let cloid = Uuid::new_v4();
    let order_request = ClientOrderRequest {
        asset: "ETH".to_string(),
        is_buy: true,
        reduce_only: false,
        limit_px: 2500.0, // Below market price so it rests
        sz: 0.1,          // $250+ value, well above $10 minimum
        cloid: Some(cloid),
        order_type: ClientOrder::Limit(ClientLimit {
            tif: "Gtc".to_string(),
        }),
    };

    info!(
        "📤 Placing limit order with main wallet: {:?}",
        order_request
    );

    match exchange_client.order(order_request, Some(wallet)).await {
        Ok(response) => {
            info!("📥 Order response: {:?}", response);
            if let hl_ranger::ExchangeResponseStatus::Ok(response_data) = response {
                if let Some(data) = response_data.data {
                    if !data.statuses.is_empty() {
                        match &data.statuses[0] {
                            hl_ranger::ExchangeDataStatus::Resting(resting_order) => {
                                info!("✅ Order resting with OID: {}", resting_order.oid);
                                opened_orders.push(resting_order.oid);
                            }
                            hl_ranger::ExchangeDataStatus::Filled(filled_order) => {
                                info!("✅ Order filled with OID: {}", filled_order.oid);
                                // Filled orders don't need cleanup
                            }
                            hl_ranger::ExchangeDataStatus::Error(e) => {
                                error!("❌ Order error: {}", e);
                            }
                            other => {
                                info!("ℹ️ Unexpected order status: {:?}", other);
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("❌ Failed to place order with main wallet: {:?}", e);
            return Err(e);
        }
    }

    Ok(opened_orders)
}

async fn test_agent_wallet_orders(
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<Vec<u64>> {
    info!("📋 Testing agent wallet orders (with agent approval)");
    let mut opened_orders = Vec::new();

    // Step 1: Approve agent
    info!("🔐 Approving L1 agent...");
    let (agent_key, approve_response) = exchange_client.approve_agent(Some(wallet)).await?;
    info!("🔑 Generated agent key: {}", agent_key);
    info!("📥 Agent approval response: {:?}", approve_response);

    // Wait for approval to be processed
    info!("⏳ Waiting 10 seconds for agent approval to be processed...");
    thread::sleep(time::Duration::from_secs(10));

    // Step 2: Place order using the approved agent
    let cloid = Uuid::new_v4();
    let order_request = ClientOrderRequest {
        asset: "ETH".to_string(),
        is_buy: true,
        reduce_only: false,
        limit_px: 2501.0, // Slightly different price from main wallet test
        sz: 0.1,
        cloid: Some(cloid),
        order_type: ClientOrder::Limit(ClientLimit {
            tif: "Gtc".to_string(),
        }),
    };

    info!(
        "📤 Placing limit order with agent wallet: {:?}",
        order_request
    );

    match exchange_client.order(order_request, Some(wallet)).await {
        Ok(response) => {
            info!("📥 Agent order response: {:?}", response);
            if let hl_ranger::ExchangeResponseStatus::Ok(response_data) = response {
                if let Some(data) = response_data.data {
                    if !data.statuses.is_empty() {
                        match &data.statuses[0] {
                            hl_ranger::ExchangeDataStatus::Resting(resting_order) => {
                                info!("✅ Agent order resting with OID: {}", resting_order.oid);
                                opened_orders.push(resting_order.oid);
                            }
                            hl_ranger::ExchangeDataStatus::Filled(filled_order) => {
                                info!("✅ Agent order filled with OID: {}", filled_order.oid);
                                // Filled orders don't need cleanup
                            }
                            hl_ranger::ExchangeDataStatus::Error(e) => {
                                error!("❌ Agent order error: {}", e);
                            }
                            other => {
                                info!("ℹ️ Unexpected agent order status: {:?}", other);
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("❌ Failed to place order with agent wallet: {:?}", e);
            return Err(e);
        }
    }

    Ok(opened_orders)
}

async fn cleanup_orders(exchange_client: &ExchangeClient, wallet: &LocalWallet, oids: Vec<u64>) {
    for oid in oids {
        info!("Canceling order OID: {}", oid);
        let cancel_request = ClientCancelRequest {
            asset: "ETH".to_string(),
            oid,
        };

        match exchange_client.cancel(cancel_request, Some(wallet)).await {
            Ok(response) => {
                info!(
                    "✅ Successfully canceled order OID: {} - {:?}",
                    oid, response
                );
            }
            Err(e) => {
                error!("❌ Failed to cancel order OID: {} - {:?}", oid, e);
            }
        }

        // Small delay between cancellations
        thread::sleep(time::Duration::from_millis(500));
    }
}

async fn test_signed_order_and_cancel(
    builder: &UnsignedTransactionBuilder,
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<()> {
    info!("--- Testing Signed Order and Cancel ---");

    // 1. Prepare and post an order
    let cloid = Uuid::new_v4();
    let order_request = ClientOrderRequest {
        asset: "ETH".to_string(),
        is_buy: true,
        reduce_only: false,
        limit_px: 2500.0, // More realistic price close to current market ($2525)
        sz: 0.1, // Increased from 0.01 to 0.1 ETH (~$252 vs $25) to be well above $10 minimum
        cloid: Some(cloid),
        order_type: ClientOrder::Limit(ClientLimit {
            tif: "Gtc".to_string(),
        }),
    };
    info!("Preparing unsigned order: {:?}", order_request);
    let unsigned_order_components = builder
        .prepare_unsigned_order(
            order_request.clone(),
            Some("na".to_string()), // Use same grouping as ExchangeClient
        )
        .await?;

    info!(
        "Action for order: {}",
        serde_json::to_string_pretty(&unsigned_order_components.action_payload_json).unwrap()
    );

    let order_response =
        sign_and_post_transaction(unsigned_order_components, exchange_client, wallet, None).await;

    let mut oid_to_cancel: Option<u64> = None;
    match order_response {
        Ok(response_status) => {
            info!("Order Post Response: {:?}", response_status);
            if let ExchangeResponseStatus::Ok(response_data) = response_status {
                if let Some(data) = response_data.data {
                    if !data.statuses.is_empty() {
                        match &data.statuses[0] {
                            ExchangeDataStatus::Resting(resting_order) => {
                                info!("Order Resting with OID: {}", resting_order.oid);
                                oid_to_cancel = Some(resting_order.oid);
                            }
                            ExchangeDataStatus::Filled(filled_order) => {
                                info!(
                                    "Order Filled with OID: {}. Cannot cancel.",
                                    filled_order.oid
                                );
                            }
                            ExchangeDataStatus::Error(e) => {
                                error!("Order Post Error in status: {}", e);
                            }
                            other => {
                                info!("Order Post - Unexpected status: {:?}", other);
                            }
                        }
                    } else {
                        info!("Order Post - No statuses in response data.");
                    }
                } else {
                    info!("Order Post - No data in response.");
                }
            } else if let ExchangeResponseStatus::Err(e) = response_status {
                error!("Order Post Error: {}", e);
            }
        }
        Err(e) => {
            error!("Failed to post order: {:?}", e);
        }
    }

    if let Some(oid) = oid_to_cancel {
        info!(
            "Waiting a few seconds before attempting to cancel order OID: {}",
            oid
        );
        thread::sleep(time::Duration::from_secs(5));

        // 2. Prepare and post a cancel for that order
        let cancel_request = ClientCancelRequest {
            asset: "ETH".to_string(),
            oid,
        };
        info!(
            "Preparing unsigned cancel for OID {}: {:?}",
            oid, cancel_request
        );
        let unsigned_cancel_components = builder.prepare_unsigned_cancel(cancel_request).await?;
        info!(
            "Action for cancel: {}",
            serde_json::to_string_pretty(&unsigned_cancel_components.action_payload_json).unwrap()
        );

        let cancel_response =
            sign_and_post_transaction(unsigned_cancel_components, exchange_client, wallet, None)
                .await;

        match cancel_response {
            Ok(response_status) => {
                info!("Cancel Post Response: {:?}", response_status);
                if let ExchangeResponseStatus::Ok(response_data) = response_status {
                    if let Some(data) = response_data.data {
                        if !data.statuses.is_empty() {
                            match &data.statuses[0] {
                                ExchangeDataStatus::Success => {
                                    info!("Successfully cancelled OID: {}", oid);
                                }
                                ExchangeDataStatus::Error(e) => {
                                    error!("Cancel Post Error in status for OID {}: {}", oid, e);
                                }
                                other => {
                                    info!(
                                        "Cancel Post - Unexpected status for OID {}: {:?}",
                                        oid, other
                                    );
                                }
                            }
                        }
                    }
                } else if let ExchangeResponseStatus::Err(e) = response_status {
                    error!("Cancel Post Error for OID {}: {}", oid, e);
                }
            }
            Err(e) => {
                error!("Failed to post cancel for OID {}: {:?}", oid, e);
            }
        }
    } else {
        info!("No resting order OID obtained, skipping cancel test.");
    }

    info!("--- Finished Signed Order and Cancel Test ---");
    Ok(())
}

async fn test_signed_usdc_transfer(
    builder: &UnsignedTransactionBuilder,
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<()> {
    info!("--- Testing Signed USDC Transfer ---");
    let amount = "1.0"; // 1 USDC
                        // Using a common test address, replace if needed, but for testnet this might not matter beyond format.
    let destination = "0x0D1d9635D0640821d15e323ac8AdADfA9c111414";

    info!(
        "Preparing unsigned USDC transfer: amount {}, destination {}",
        amount, destination
    );
    let components = builder
        .prepare_unsigned_usdc_transfer(amount, destination)
        .await?;

    info!(
        "Action for USDC transfer: {}",
        serde_json::to_string_pretty(&components.action_payload_json).unwrap()
    );

    let response = sign_and_post_transaction(components, exchange_client, wallet, None).await;

    match response {
        Ok(response_status) => {
            info!("USDC Transfer Post Response: {:?}", response_status);
            // Further checks can be added based on expected success/error messages
        }
        Err(e) => {
            error!("Failed to post USDC transfer: {:?}", e);
        }
    }

    info!("--- Finished Signed USDC Transfer Test ---");
    Ok(())
}

async fn test_signed_withdraw(
    builder: &UnsignedTransactionBuilder,
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<()> {
    info!("--- Testing Signed Withdraw ---");
    let amount = "0.5"; // 0.5 USD
    let destination = "0x0D1d9635D0640821d15e323ac8AdADfA9c111414";

    info!(
        "Preparing unsigned withdraw: amount {}, destination {}",
        amount, destination
    );
    let components = builder
        .prepare_unsigned_withdraw(amount, destination)
        .await?;

    info!(
        "Action for withdraw: {}",
        serde_json::to_string_pretty(&components.action_payload_json).unwrap()
    );

    let response = sign_and_post_transaction(components, exchange_client, wallet, None).await;

    match response {
        Ok(response_status) => {
            info!("Withdraw Post Response: {:?}", response_status);
            // Further checks can be added based on expected success/error messages
        }
        Err(e) => {
            error!("Failed to post withdraw: {:?}", e);
        }
    }

    info!("--- Finished Signed Withdraw Test ---");
    Ok(())
}

async fn test_signed_modify_order(
    builder: &UnsignedTransactionBuilder,
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<()> {
    info!("--- Testing Signed Modify Order ---");

    // 1. Place an order to get an OID
    let initial_cloid = Uuid::new_v4();
    let initial_order_request = ClientOrderRequest {
        asset: "ETH".to_string(),
        is_buy: true,
        reduce_only: false,
        limit_px: 1000.0, // Low price to ensure it rests
        sz: 0.01,
        cloid: Some(initial_cloid),
        order_type: ClientOrder::Limit(ClientLimit {
            tif: "Gtc".to_string(),
        }),
    };
    info!(
        "Preparing initial unsigned order for modification test: {:?}",
        initial_order_request
    );
    let unsigned_initial_order_components = builder
        .prepare_unsigned_order(
            initial_order_request.clone(),
            Some("test_modify_group".to_string()),
        )
        .await?;

    let initial_order_response = sign_and_post_transaction(
        unsigned_initial_order_components,
        exchange_client,
        wallet,
        None,
    )
    .await;

    let mut oid_to_modify: Option<u64> = None;
    match initial_order_response {
        Ok(response_status) => {
            info!(
                "Initial Order Post Response (for modify test): {:?}",
                response_status
            );
            if let ExchangeResponseStatus::Ok(response_data) = response_status {
                if let Some(data) = response_data.data {
                    if !data.statuses.is_empty() {
                        match &data.statuses[0] {
                            ExchangeDataStatus::Resting(resting_order) => {
                                info!("Initial Order Resting with OID: {}", resting_order.oid);
                                oid_to_modify = Some(resting_order.oid);
                            }
                            ExchangeDataStatus::Filled(filled_order) => {
                                info!("Initial Order Filled with OID: {}. Cannot modify if fully filled immediately.", filled_order.oid);
                            }
                            ExchangeDataStatus::Error(e) => {
                                error!("Initial Order Post Error in status: {}", e);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("Failed to post initial order for modify test: {:?}", e);
        }
    }

    if let Some(oid) = oid_to_modify {
        info!(
            "Waiting a few seconds before attempting to modify order OID: {}",
            oid
        );
        thread::sleep(time::Duration::from_secs(3));

        // 2. Prepare and post a modify request for that order
        let modified_cloid = Uuid::new_v4();
        let modify_order_details = ClientOrderRequest {
            asset: "ETH".to_string(), // Must match original asset
            is_buy: true,             // Usually matches original, unless flipping side (complex)
            reduce_only: false,
            limit_px: 1001.0, // New price
            sz: 0.012,        // New size
            cloid: Some(modified_cloid),
            order_type: ClientOrder::Limit(ClientLimit {
                tif: "Gtc".to_string(),
            }),
        };
        let modify_request = ClientModifyRequest {
            oid,
            order: modify_order_details,
        };

        info!(
            "Preparing unsigned modify for OID {}: {:?}",
            oid, modify_request
        );
        let unsigned_modify_components = builder
            .prepare_unsigned_modify_order(modify_request)
            .await?;
        info!(
            "Action for modify: {}",
            serde_json::to_string_pretty(&unsigned_modify_components.action_payload_json).unwrap()
        );

        let modify_response =
            sign_and_post_transaction(unsigned_modify_components, exchange_client, wallet, None)
                .await;

        match modify_response {
            Ok(response_status) => {
                info!(
                    "Modify Post Response for OID {}: {:?}",
                    oid, response_status
                );
                // Similar status checking as order/cancel
            }
            Err(e) => {
                error!("Failed to post modify for OID {}: {:?}", oid, e);
            }
        }
        // Optional: Attempt to cancel the modified order to clean up
        info!(
            "Attempting to cancel modified order OID: {} (best effort)",
            oid
        );
        let cancel_req = ClientCancelRequest {
            asset: "ETH".to_string(),
            oid,
        };
        if let Ok(cancel_comps) = builder.prepare_unsigned_cancel(cancel_req).await {
            let _ = sign_and_post_transaction(cancel_comps, exchange_client, wallet, None).await;
            info!("Sent cancel for modified order OID: {}", oid);
        }
    } else {
        info!("No resting order OID obtained, skipping modify test.");
    }

    info!("--- Finished Signed Modify Order Test ---");
    Ok(())
}

async fn test_signed_update_leverage(
    builder: &UnsignedTransactionBuilder,
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<()> {
    info!("--- Testing Signed Update Leverage ---");
    let asset = "ETH";
    let leverage = 5;
    let is_cross = false; // Or true, depending on what you want to test

    info!(
        "Preparing unsigned update leverage: asset {}, leverage {}, is_cross {}",
        asset, leverage, is_cross
    );
    let components = builder
        .prepare_unsigned_update_leverage(leverage, asset, is_cross)
        .await?;

    info!(
        "Action for update leverage: {}",
        serde_json::to_string_pretty(&components.action_payload_json).unwrap()
    );

    let response = sign_and_post_transaction(components, exchange_client, wallet, None).await;

    match response {
        Ok(response_status) => {
            info!("Update Leverage Post Response: {:?}", response_status);
            // Check for success or specific errors
        }
        Err(e) => {
            error!("Failed to post update leverage: {:?}", e);
        }
    }

    info!("--- Finished Signed Update Leverage Test ---");
    Ok(())
}

async fn test_signed_spot_transfer(
    builder: &UnsignedTransactionBuilder,
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<()> {
    info!("--- Testing Signed Spot Transfer ---");
    let amount = "1"; // Amount of the token
    let destination = "0x0D1d9635D0640821d15e323ac8AdADfA9c111414";
    // Use a valid spot token for testnet, e.g., from spot_order.rs example or docs
    // This is an example token format. Ensure it exists on testnet.
    let token = "PURR:0xc4bf3f870c0e9465323c0b6ed28096c2";

    info!(
        "Preparing unsigned spot transfer: amount {}, destination {}, token {}",
        amount, destination, token
    );
    let components = builder
        .prepare_unsigned_spot_transfer(amount, destination, token)
        .await?;
    info!(
        "Action for spot transfer: {}",
        serde_json::to_string_pretty(&components.action_payload_json).unwrap()
    );

    let response = sign_and_post_transaction(components, exchange_client, wallet, None).await;

    match response {
        Ok(response_status) => {
            info!("Spot Transfer Post Response: {:?}", response_status);
            // Check for success or specific errors.
            // This might fail if the account doesn't hold the spot asset.
        }
        Err(e) => {
            error!("Failed to post spot transfer: {:?}", e);
        }
    }

    info!("--- Finished Signed Spot Transfer Test ---");
    Ok(())
}

async fn test_signed_vault_transfer(
    builder: &UnsignedTransactionBuilder,
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<()> {
    info!("--- Testing Signed Vault Transfer ---");
    let is_deposit = true;
    let usd_amount = 5_000_000; // Example: 5 USD (represented as u64, check SDK for exact unit)
                                // The UnsignedTransactionBuilder expects u64 for usd.
                                // Using the vault address from the `vault_transfer.rs` example bin.
    let vault_address_str = "0x1962905b0a2d0ce7907ae1a0d17f3e4a1f63dfb7";
    let vault_h160 = ethers::types::H160::from_str(vault_address_str)
        .map_err(|e| hl_ranger::Error::GenericParse(format!("Invalid vault address: {}", e)))?;

    // Note: The UnsignedTransactionBuilder has vault_address as an Option<H160> in its own state.
    // The prepare_unsigned_vault_transfer also takes an Option<H160>.
    // If the builder was initialized with a vault_address, that one is used by default for L1 agent hashing.
    // The vault_address parameter in prepare_unsigned_vault_transfer is specifically for the VaultTransfer action payload.

    info!(
        "Preparing unsigned vault transfer: is_deposit {}, usd_amount {}, vault_address {}",
        is_deposit, usd_amount, vault_address_str
    );
    let components = builder
        .prepare_unsigned_vault_transfer(is_deposit, usd_amount, Some(vault_h160))
        .await?;
    info!(
        "Action for vault transfer: {}",
        serde_json::to_string_pretty(&components.action_payload_json).unwrap()
    );

    let response = sign_and_post_transaction(components, exchange_client, wallet, None).await;

    match response {
        Ok(response_status) => {
            info!("Vault Transfer Post Response: {:?}", response_status);
            // Check for success. This might fail if the user is not whitelisted or vault doesn't exist.
        }
        Err(e) => {
            error!("Failed to post vault transfer: {:?}", e);
        }
    }

    info!("--- Finished Signed Vault Transfer Test ---");
    Ok(())
}

async fn test_signed_bulk_cancel(
    builder: &UnsignedTransactionBuilder,
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<()> {
    info!("--- Testing Signed Bulk Cancel ---");

    // 1. Place a couple of orders to get OIDs
    let mut oids_to_cancel: Vec<ClientCancelRequest> = Vec::new();
    info!("Placing orders for bulk cancel test...");

    for i in 0..2 {
        let cloid = Uuid::new_v4();
        let order_request = ClientOrderRequest {
            asset: "ETH".to_string(),
            is_buy: true,
            reduce_only: false,
            limit_px: 1000.0 + i as f64, // Slightly different prices
            sz: 0.01,
            cloid: Some(cloid),
            order_type: ClientOrder::Limit(ClientLimit {
                tif: "Gtc".to_string(),
            }),
        };
        let unsigned_order_components = builder
            .prepare_unsigned_order(
                order_request.clone(),
                Some(format!("bulk_cancel_setup_{}", i)),
            )
            .await?;

        let order_response =
            sign_and_post_transaction(unsigned_order_components, exchange_client, wallet, None)
                .await;
        match order_response {
            Ok(ExchangeResponseStatus::Ok(res_data)) => {
                if let Some(data) = res_data.data {
                    if !data.statuses.is_empty() {
                        if let ExchangeDataStatus::Resting(resting) = &data.statuses[0] {
                            info!("Order {} placed for bulk cancel, OID: {}", i, resting.oid);
                            oids_to_cancel.push(ClientCancelRequest {
                                asset: "ETH".to_string(),
                                oid: resting.oid,
                            });
                        } else {
                            info!("Order {} did not rest: {:?}", i, data.statuses[0]);
                        }
                    }
                }
            }
            Ok(ExchangeResponseStatus::Err(e)) => error!("Error placing order {}: {}", i, e),
            Err(e) => error!("Failed to post order {}: {:?}", i, e),
        }
        thread::sleep(time::Duration::from_millis(500)); // Small delay between posts
    }

    if oids_to_cancel.len() < 2 {
        error!(
            "Could not place enough orders for bulk cancel test. Placed: {}. Skipping.",
            oids_to_cancel.len()
        );
        return Ok(());
    }

    info!(
        "Waiting a few seconds before attempting bulk cancel for OIDs: {:?}",
        oids_to_cancel.iter().map(|c| c.oid).collect::<Vec<_>>()
    );
    thread::sleep(time::Duration::from_secs(5));

    // 2. Prepare and post a bulk cancel for these orders
    info!(
        "Preparing unsigned bulk cancel for {} orders",
        oids_to_cancel.len()
    );
    let components = builder.prepare_unsigned_bulk_cancel(oids_to_cancel).await?;
    info!(
        "Action for bulk cancel: {}",
        serde_json::to_string_pretty(&components.action_payload_json).unwrap()
    );

    let response = sign_and_post_transaction(components, exchange_client, wallet, None).await;

    match response {
        Ok(response_status) => {
            info!("Bulk Cancel Post Response: {:?}", response_status);
            // Check statuses for each cancelled order if needed
        }
        Err(e) => {
            error!("Failed to post bulk cancel: {:?}", e);
        }
    }

    info!("--- Finished Signed Bulk Cancel Test ---");
    Ok(())
}

async fn test_exchange_client_order_with_agent(
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<()> {
    info!("--- Testing ExchangeClient Order Method Directly ---");

    // 1. Approve the wallet as an L1 agent using ExchangeClient directly
    info!("Approving L1 agent using ExchangeClient...");
    let (agent_key, approve_response) = exchange_client.approve_agent(Some(wallet)).await?;
    info!("Generated agent key: {}", agent_key);
    info!("L1 Agent Approval Response: {:?}", approve_response);

    // Wait a moment for the approval to be processed
    info!("Waiting for agent approval to be processed...");
    thread::sleep(time::Duration::from_secs(10));

    // 2. Now try to place an order using ExchangeClient's order method directly
    let cloid = Uuid::new_v4();
    let order_request = ClientOrderRequest {
        asset: "ETH".to_string(),
        is_buy: true,
        reduce_only: false,
        limit_px: 2500.0, // More realistic price close to current market ($2525)
        sz: 0.1, // Increased from 0.01 to 0.1 ETH (~$252 vs $25) to be well above $10 minimum
        cloid: Some(cloid),
        order_type: ClientOrder::Limit(ClientLimit {
            tif: "Gtc".to_string(),
        }),
    };
    info!(
        "Placing order using ExchangeClient.order() method: {:?}",
        order_request
    );

    // Use ExchangeClient's order method directly
    let order_response = exchange_client
        .order(order_request.clone(), Some(wallet))
        .await;

    match order_response {
        Ok(response_status) => {
            info!("ExchangeClient Order Response: {:?}", response_status);
            if let ExchangeResponseStatus::Ok(response_data) = response_status {
                if let Some(data) = response_data.data {
                    if !data.statuses.is_empty() {
                        match &data.statuses[0] {
                            ExchangeDataStatus::Resting(resting_order) => {
                                info!("Order Resting with OID: {}", resting_order.oid);
                                // Try to cancel it
                                let cancel_request = ClientCancelRequest {
                                    asset: "ETH".to_string(),
                                    oid: resting_order.oid,
                                };
                                thread::sleep(time::Duration::from_secs(3));
                                let cancel_response =
                                    exchange_client.cancel(cancel_request, Some(wallet)).await;
                                info!("Cancel Response: {:?}", cancel_response);
                            }
                            ExchangeDataStatus::Filled(filled_order) => {
                                info!(
                                    "Order Filled with OID: {}. Position opened!",
                                    filled_order.oid
                                );
                            }
                            ExchangeDataStatus::Error(e) => {
                                error!("Order Post Error in status: {}", e);
                            }
                            other => {
                                info!("Order Post - Unexpected status: {:?}", other);
                            }
                        }
                    } else {
                        info!("Order Post - No statuses in response data.");
                    }
                } else {
                    info!("Order Post - No data in response.");
                }
            } else if let ExchangeResponseStatus::Err(e) = response_status {
                error!("Order Post Error: {}", e);
            }
        }
        Err(e) => {
            error!("Failed to post order using ExchangeClient: {:?}", e);
        }
    }

    info!("--- Finished ExchangeClient Order Method Test ---");
    Ok(())
}

async fn test_compare_payloads(
    builder: &UnsignedTransactionBuilder,
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<()> {
    info!("--- Comparing Our Payload vs ExchangeClient Payload ---");

    // Test with USDC transfer since it was working before
    let amount = "1.0";
    let destination = "0x0D1d9635D0640821d15e323ac8AdADfA9c111414";

    // 1. Our implementation
    info!("Testing our UnsignedTransactionBuilder implementation...");
    let components = builder
        .prepare_unsigned_usdc_transfer(amount, destination)
        .await?;

    let response = sign_and_post_transaction(components, exchange_client, wallet, None).await;
    info!("Our implementation result: {:?}", response);

    // 2. ExchangeClient implementation
    info!("Testing ExchangeClient implementation...");
    let exchange_response = exchange_client
        .usdc_transfer(amount, destination, Some(wallet))
        .await;
    info!("ExchangeClient result: {:?}", exchange_response);

    info!("--- Finished Payload Comparison ---");
    Ok(())
}

async fn test_open_and_close_position(
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<()> {
    info!("--- Testing Open and Close Position with Market Orders ---");

    // 1. Approve the wallet as an L1 agent using ExchangeClient directly
    info!("Approving L1 agent using ExchangeClient...");
    let (agent_key, approve_response) = exchange_client.approve_agent(Some(wallet)).await?;
    info!("Generated agent key: {}", agent_key);
    info!("L1 Agent Approval Response: {:?}", approve_response);

    // Wait a moment for the approval to be processed
    info!("Waiting for agent approval to be processed...");
    thread::sleep(time::Duration::from_secs(10));

    // 2. Open position with market order (should fill immediately)
    let cloid = Uuid::new_v4();
    let market_order_request = ClientOrderRequest {
        asset: "ETH".to_string(),
        is_buy: true,
        reduce_only: false,
        limit_px: 2600.0, // Above market price to ensure immediate fill
        sz: 0.1,          // $250+ value, well above $10 minimum
        cloid: Some(cloid),
        order_type: ClientOrder::Limit(ClientLimit {
            tif: "Ioc".to_string(), // Immediate or Cancel - acts like market order
        }),
    };
    info!(
        "Opening position with market-like order: {:?}",
        market_order_request
    );

    let open_response = exchange_client
        .order(market_order_request.clone(), Some(wallet))
        .await;

    let mut position_opened = false;
    match open_response {
        Ok(response_status) => {
            info!("Open Position Response: {:?}", response_status);
            if let ExchangeResponseStatus::Ok(response_data) = response_status {
                if let Some(data) = response_data.data {
                    if !data.statuses.is_empty() {
                        match &data.statuses[0] {
                            ExchangeDataStatus::Filled(filled_order) => {
                                info!(
                                    "🎉 POSITION OPENED! Order Filled with OID: {}",
                                    filled_order.oid
                                );
                                position_opened = true;
                            }
                            ExchangeDataStatus::Resting(resting_order) => {
                                info!(
                                    "Order Resting with OID: {} (not filled yet)",
                                    resting_order.oid
                                );
                                // Cancel the resting order
                                let cancel_request = ClientCancelRequest {
                                    asset: "ETH".to_string(),
                                    oid: resting_order.oid,
                                };
                                let _ = exchange_client.cancel(cancel_request, Some(wallet)).await;
                                info!("Canceled resting order");
                            }
                            ExchangeDataStatus::Error(e) => {
                                error!("Order Error: {}", e);
                            }
                            other => {
                                info!("Unexpected order status: {:?}", other);
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("Failed to place market order: {:?}", e);
        }
    }

    if position_opened {
        info!("Position opened successfully! Waiting 5 seconds before closing...");
        thread::sleep(time::Duration::from_secs(5));

        // 3. Close position with market order
        let close_cloid = Uuid::new_v4();
        let close_order_request = ClientOrderRequest {
            asset: "ETH".to_string(),
            is_buy: false,     // Sell to close long position
            reduce_only: true, // This ensures we're closing, not opening new position
            limit_px: 2400.0,  // Below market price to ensure immediate fill
            sz: 0.1,           // Same size as opened position
            cloid: Some(close_cloid),
            order_type: ClientOrder::Limit(ClientLimit {
                tif: "Ioc".to_string(), // Immediate or Cancel
            }),
        };
        info!(
            "Closing position with market-like order: {:?}",
            close_order_request
        );

        let close_response = exchange_client
            .order(close_order_request, Some(wallet))
            .await;

        match close_response {
            Ok(response_status) => {
                info!("Close Position Response: {:?}", response_status);
                if let ExchangeResponseStatus::Ok(response_data) = response_status {
                    if let Some(data) = response_data.data {
                        if !data.statuses.is_empty() {
                            match &data.statuses[0] {
                                ExchangeDataStatus::Filled(filled_order) => {
                                    info!(
                                        "🎉 POSITION CLOSED! Order Filled with OID: {}",
                                        filled_order.oid
                                    );
                                }
                                ExchangeDataStatus::Error(e) => {
                                    error!("Close Order Error: {}", e);
                                }
                                other => {
                                    info!("Unexpected close status: {:?}", other);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to close position: {:?}", e);
            }
        }
    } else {
        info!("No position was opened, skipping close test.");
    }

    info!("--- Finished Open and Close Position Test ---");
    Ok(())
}

async fn test_simple_limit_orders_with_exchange_client(
    exchange_client: &ExchangeClient,
    wallet: &LocalWallet,
) -> Result<()> {
    info!("--- Testing Simple Limit Orders with ExchangeClient ---");

    // 1. Approve agent (required for orders)
    info!("Approving L1 agent...");
    let (agent_key, approve_response) = exchange_client.approve_agent(Some(wallet)).await?;
    info!("Generated agent key: {}", agent_key);
    info!("L1 Agent Approval Response: {:?}", approve_response);

    // Wait for approval to be processed
    info!("Waiting for agent approval to be processed...");
    thread::sleep(time::Duration::from_secs(10));

    // 2. Place a limit order using ExchangeClient directly
    let cloid = Uuid::new_v4();
    let order_request = ClientOrderRequest {
        asset: "ETH".to_string(),
        is_buy: true,
        reduce_only: false,
        limit_px: 2500.0, // Below market price so it rests
        sz: 0.1,          // $250+ value, well above $10 minimum
        cloid: Some(cloid),
        order_type: ClientOrder::Limit(ClientLimit {
            tif: "Gtc".to_string(),
        }),
    };
    info!(
        "Placing limit order using ExchangeClient: {:?}",
        order_request
    );

    let order_response = exchange_client.order(order_request, Some(wallet)).await;

    let mut oid_to_cancel: Option<u64> = None;
    match order_response {
        Ok(response_status) => {
            info!("Limit Order Response: {:?}", response_status);
            if let ExchangeResponseStatus::Ok(response_data) = response_status {
                if let Some(data) = response_data.data {
                    if !data.statuses.is_empty() {
                        match &data.statuses[0] {
                            ExchangeDataStatus::Resting(resting_order) => {
                                info!(
                                    "🎉 SUCCESS! Limit order resting with OID: {}",
                                    resting_order.oid
                                );
                                oid_to_cancel = Some(resting_order.oid);
                            }
                            ExchangeDataStatus::Filled(filled_order) => {
                                info!(
                                    "🎉 SUCCESS! Limit order filled with OID: {}",
                                    filled_order.oid
                                );
                            }
                            ExchangeDataStatus::Error(e) => {
                                error!("Limit order error: {}", e);
                            }
                            other => {
                                info!("Limit order unexpected status: {:?}", other);
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("Failed to place limit order: {:?}", e);
        }
    }

    // 3. Cancel the order if it's resting
    if let Some(oid) = oid_to_cancel {
        info!("Waiting 3 seconds before canceling order OID: {}", oid);
        thread::sleep(time::Duration::from_secs(3));

        let cancel_request = ClientCancelRequest {
            asset: "ETH".to_string(),
            oid,
        };
        info!("Canceling order OID: {}", oid);

        let cancel_response = exchange_client.cancel(cancel_request, Some(wallet)).await;

        match cancel_response {
            Ok(response_status) => {
                info!("Cancel Response: {:?}", response_status);
                if let ExchangeResponseStatus::Ok(response_data) = response_status {
                    if let Some(data) = response_data.data {
                        if !data.statuses.is_empty() {
                            match &data.statuses[0] {
                                ExchangeDataStatus::Success => {
                                    info!("✅ Successfully cancelled OID: {}", oid);
                                }
                                ExchangeDataStatus::Error(e) => {
                                    error!("Cancel error for OID {}: {}", oid, e);
                                }
                                other => {
                                    info!("Cancel unexpected status for OID {}: {:?}", oid, other);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to cancel OID {}: {:?}", oid, e);
            }
        }
    } else {
        info!("No resting order to cancel.");
    }

    info!("--- Finished Simple Limit Orders with ExchangeClient Test ---");
    Ok(())
}
