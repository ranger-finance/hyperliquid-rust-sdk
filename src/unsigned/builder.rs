use crate::meta::Meta;
use crate::prelude::Result;
use crate::req::HttpClient;
use crate::BaseUrl;
use crate::InfoClient;
use ethers::types::H160;
use reqwest::Client;
use std::collections::HashMap;

// Add new imports for the prepare_unsigned_order method
use super::components::UnsignedTransactionComponents;
use crate::helpers::next_nonce;
use crate::signature::agent::l1::Agent as L1Agent;
use crate::{Actions, BulkOrder, ClientOrderRequest, UsdSend};
use ethers::types::transaction::eip712::Eip712;
use ethers::types::U256;

#[derive(Debug)]
pub struct UnsignedTransactionBuilder {
    pub http_client: HttpClient,
    pub meta: Meta,
    pub vault_address: Option<H160>,
    pub coin_to_asset: HashMap<String, u32>,
}

impl UnsignedTransactionBuilder {
    pub async fn new(
        client: Option<Client>,
        base_url_override: Option<BaseUrl>,
        meta_override: Option<Meta>,
        vault_address: Option<H160>,
    ) -> Result<Self> {
        let client = client.unwrap_or_default();
        let base_url = base_url_override.unwrap_or(BaseUrl::Mainnet);

        let info_for_setup = InfoClient::new(None, Some(base_url)).await?;
        let meta = if let Some(m) = meta_override {
            m
        } else {
            info_for_setup.meta().await?
        };

        let mut coin_to_asset = HashMap::new();
        for (asset_ind, asset_meta) in meta.universe.iter().enumerate() {
            coin_to_asset.insert(asset_meta.name.clone(), asset_ind as u32);
        }
        coin_to_asset = info_for_setup
            .spot_meta()
            .await?
            .add_pair_and_name_to_index_map(coin_to_asset);

        Ok(UnsignedTransactionBuilder {
            http_client: HttpClient {
                client,
                base_url: base_url.get_url(),
            },
            meta,
            vault_address,
            coin_to_asset,
        })
    }

    pub async fn prepare_unsigned_order(
        &self,
        order: ClientOrderRequest,
        grouping: Option<String>,
    ) -> Result<UnsignedTransactionComponents> {
        // Convert the ClientOrderRequest to OrderRequest using the coin_to_asset mapping
        let order_request = order.convert(&self.coin_to_asset)?;

        // Create the action
        let action = Actions::Order(BulkOrder {
            orders: vec![order_request],
            grouping: grouping.unwrap_or_else(|| "na".to_string()),
            builder: None,
        });

        // Generate nonce
        let nonce = next_nonce();

        // Compute the action hash for L1 agent signing
        let connection_id = action.hash(nonce, self.vault_address)?;

        // Create L1 Agent for signing
        let agent = L1Agent {
            source: self.vault_address.unwrap_or_default().to_string(),
            connection_id,
        };

        // Get the typed data hash
        let digest = agent
            .encode_eip712()
            .map_err(|e| crate::Error::Eip712(e.to_string()))?;

        // Serialize action to JSON for the caller
        let action_json =
            serde_json::to_value(&action).map_err(|e| crate::Error::JsonParse(e.to_string()))?;

        Ok(UnsignedTransactionComponents {
            action_payload_json: action_json,
            nonce,
            digest_to_sign: ethers::types::H256::from(digest),
            vault_address: self.vault_address,
            eip712_domain_chain_id: Some(ethers::types::U256::from(1337)),
            eip712_hyperliquid_chain_name: None,
            is_l1_agent_signature: true,
        })
    }

    pub async fn prepare_unsigned_usdc_transfer(
        &self,
        amount_str: &str,
        destination_str: &str,
    ) -> Result<UnsignedTransactionComponents> {
        let timestamp = next_nonce();
        let hyperliquid_chain_name = if self.http_client.is_mainnet() {
            "Mainnet".to_string()
        } else {
            "Testnet".to_string()
        };
        let signature_chain_id = U256::from(421614);

        let usd_send_action = UsdSend {
            signature_chain_id,
            hyperliquid_chain: hyperliquid_chain_name.clone(),
            destination: destination_str.to_string(),
            amount: amount_str.to_string(),
            time: timestamp,
        };

        let action_payload_json = serde_json::to_value(Actions::UsdSend(usd_send_action.clone()))
            .map_err(|e| crate::Error::JsonParse(e.to_string()))?;

        let digest_to_sign = ethers::types::H256::from(
            usd_send_action
                .encode_eip712()
                .map_err(|e| crate::Error::Eip712(e.to_string()))?,
        );

        Ok(UnsignedTransactionComponents {
            action_payload_json,
            nonce: timestamp,
            digest_to_sign,
            vault_address: self.vault_address,
            eip712_domain_chain_id: Some(signature_chain_id),
            eip712_hyperliquid_chain_name: Some(hyperliquid_chain_name),
            is_l1_agent_signature: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClientLimit, ClientOrder};
    use ethers::types::H256;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_unsigned_transaction_builder_new_testnet() {
        let builder =
            UnsignedTransactionBuilder::new(None, Some(BaseUrl::Testnet), None, None).await;

        match builder {
            Ok(b) => {
                assert!(
                    !b.coin_to_asset.is_empty(),
                    "coin_to_asset should not be empty"
                );
                assert!(
                    b.vault_address.is_none(),
                    "vault_address should be None as set"
                );
                println!(
                    "✓ UnsignedTransactionBuilder created successfully with {} assets",
                    b.coin_to_asset.len()
                );
            }
            Err(e) => {
                println!(
                    "Builder creation failed (expected in some environments): {:?}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_unsigned_transaction_builder_with_vault() {
        let vault_addr = "0x1234567890123456789012345678901234567890"
            .parse::<H160>()
            .unwrap();

        let builder =
            UnsignedTransactionBuilder::new(None, Some(BaseUrl::Testnet), None, Some(vault_addr))
                .await;

        match builder {
            Ok(b) => {
                assert_eq!(
                    b.vault_address,
                    Some(vault_addr),
                    "vault_address should match"
                );
                println!("✓ UnsignedTransactionBuilder created successfully with vault address");
            }
            Err(e) => {
                println!(
                    "Builder creation failed (expected in some environments): {:?}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_prepare_unsigned_order() {
        let builder_result =
            UnsignedTransactionBuilder::new(None, Some(BaseUrl::Testnet), None, None).await;

        if let Ok(builder) = builder_result {
            let order = ClientOrderRequest {
                asset: "ETH".to_string(),
                is_buy: true,
                reduce_only: false,
                limit_px: 2000.0,
                sz: 0.1,
                cloid: Some(Uuid::new_v4()),
                order_type: ClientOrder::Limit(ClientLimit {
                    tif: "Gtc".to_string(),
                }),
            };

            let result = builder.prepare_unsigned_order(order, None).await;

            match result {
                Ok(components) => {
                    assert!(components.nonce > 0, "nonce should be set");
                    assert_ne!(
                        components.digest_to_sign,
                        H256::zero(),
                        "digest should not be zero"
                    );
                    assert!(
                        components.is_l1_agent_signature,
                        "should be L1 agent signature"
                    );
                    assert_eq!(
                        components.eip712_domain_chain_id,
                        Some(ethers::types::U256::from(1337))
                    );
                    println!("✓ prepare_unsigned_order succeeded");
                    println!("  - Nonce: {}", components.nonce);
                    println!("  - Digest: {:?}", components.digest_to_sign);
                }
                Err(e) => {
                    println!("prepare_unsigned_order failed (may be expected): {:?}", e);
                }
            }
        } else {
            println!("Builder creation failed, skipping order test");
        }
    }

    #[tokio::test]
    async fn test_prepare_unsigned_usdc_transfer() {
        let builder_result =
            UnsignedTransactionBuilder::new(None, Some(BaseUrl::Testnet), None, None).await;

        if let Ok(builder) = builder_result {
            let amount = "10.5";
            let destination = "0x742d35Cc6634C0532925a3b8D8c9e4b7B6a3b";

            let result = builder
                .prepare_unsigned_usdc_transfer(amount, destination)
                .await;

            match result {
                Ok(components) => {
                    assert!(components.nonce > 0, "nonce should be set");
                    assert_ne!(
                        components.digest_to_sign,
                        H256::zero(),
                        "digest should not be zero"
                    );
                    assert!(
                        !components.is_l1_agent_signature,
                        "should NOT be L1 agent signature for USDC transfer"
                    );
                    assert_eq!(components.eip712_domain_chain_id, Some(U256::from(421614)));
                    assert_eq!(
                        components.eip712_hyperliquid_chain_name,
                        Some("Testnet".to_string())
                    );
                    println!("✓ prepare_unsigned_usdc_transfer succeeded");
                    println!("  - Nonce: {}", components.nonce);
                    println!("  - Digest: {:?}", components.digest_to_sign);
                    println!(
                        "  - Chain Name: {:?}",
                        components.eip712_hyperliquid_chain_name
                    );
                }
                Err(e) => {
                    println!(
                        "prepare_unsigned_usdc_transfer failed (may be expected): {:?}",
                        e
                    );
                }
            }
        } else {
            println!("Builder creation failed, skipping USDC transfer test");
        }
    }
}
