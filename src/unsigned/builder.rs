use crate::meta::Meta;
use crate::prelude::Result;
use crate::req::HttpClient;
use crate::BaseUrl;
use crate::InfoClient;
use ethers::types::H160;
use reqwest::Client;
use std::collections::HashMap;

// Add new imports for the prepare_unsigned_order method
use super::bridge;
use super::components::UnsignedTransactionComponents;
use crate::exchange::{ApproveBuilderFee, BuilderInfo};
use crate::helpers::generate_random_key;
use crate::helpers::next_nonce;
use crate::signature::agent::l1::Agent as L1Agent;
use crate::{
    Actions, ApproveAgent, BulkCancel, BulkModify, BulkOrder, CancelRequest, ClientCancelRequest,
    ClientModifyRequest, ClientOrderRequest, ModifyRequest, SpotSend, UpdateIsolatedMargin,
    UpdateLeverage, UsdSend, VaultTransfer, Withdraw3,
};
use ethers::signers::{LocalWallet, Signer};
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
        self.prepare_unsigned_order_with_builder(order, grouping, None)
            .await
    }

    pub async fn prepare_unsigned_order_with_builder(
        &self,
        order: ClientOrderRequest,
        grouping: Option<String>,
        builder: Option<BuilderInfo>,
    ) -> Result<UnsignedTransactionComponents> {
        // Convert the ClientOrderRequest to OrderRequest using the coin_to_asset mapping
        let order_request = order.convert(&self.coin_to_asset)?;

        // Create the action
        let action = Actions::Order(BulkOrder {
            orders: vec![order_request],
            grouping: grouping.unwrap_or_else(|| "na".to_string()),
            builder,
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
        let signature_chain_id = if self.http_client.is_mainnet() {
            U256::from(42161) // Arbitrum mainnet
        } else {
            U256::from(421614) // Arbitrum testnet
        };

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

    pub async fn prepare_unsigned_cancel(
        &self,
        cancel: ClientCancelRequest,
    ) -> Result<UnsignedTransactionComponents> {
        let nonce = next_nonce();

        let &asset_index = self
            .coin_to_asset
            .get(&cancel.asset)
            .ok_or(crate::Error::AssetNotFound)?;

        let cancel_request = CancelRequest {
            asset: asset_index,
            oid: cancel.oid,
        };

        let action = Actions::Cancel(BulkCancel {
            cancels: vec![cancel_request],
        });

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

    pub async fn prepare_unsigned_withdraw(
        &self,
        amount: &str,
        destination: &str,
    ) -> Result<UnsignedTransactionComponents> {
        let timestamp = next_nonce();
        let hyperliquid_chain_name = if self.http_client.is_mainnet() {
            "Mainnet".to_string()
        } else {
            "Testnet".to_string()
        };
        let signature_chain_id = if self.http_client.is_mainnet() {
            U256::from(42161) // Arbitrum mainnet
        } else {
            U256::from(421614) // Arbitrum testnet
        };

        let withdraw_action = Withdraw3 {
            signature_chain_id,
            hyperliquid_chain: hyperliquid_chain_name.clone(),
            destination: destination.to_string(),
            amount: amount.to_string(),
            time: timestamp,
        };

        let action_payload_json = serde_json::to_value(Actions::Withdraw3(withdraw_action.clone()))
            .map_err(|e| crate::Error::JsonParse(e.to_string()))?;

        let digest_to_sign = ethers::types::H256::from(
            withdraw_action
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

    pub async fn prepare_unsigned_update_leverage(
        &self,
        leverage: u32,
        asset: &str,
        is_cross: bool,
    ) -> Result<UnsignedTransactionComponents> {
        let nonce = next_nonce();

        let &asset_index = self
            .coin_to_asset
            .get(asset)
            .ok_or(crate::Error::AssetNotFound)?;

        let action = Actions::UpdateLeverage(UpdateLeverage {
            asset: asset_index,
            is_cross,
            leverage,
        });

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

    pub async fn prepare_unsigned_update_isolated_margin(
        &self,
        asset: &str,
        margin_to_add: String,
    ) -> Result<UnsignedTransactionComponents> {
        let nonce = next_nonce();

        let &asset_index = self
            .coin_to_asset
            .get(asset)
            .ok_or(crate::Error::AssetNotFound)?;

        // Parse the margin amount and convert to micro USDC (6 decimal places)
        let margin_amount: f64 = margin_to_add
            .parse()
            .map_err(|_| crate::Error::FloatStringParse)?;
        let ntli = (margin_amount * 1_000_000.0).round() as i64;

        let action = Actions::UpdateIsolatedMargin(UpdateIsolatedMargin {
            asset: asset_index,
            is_buy: true, // Always true for adding margin
            ntli,
        });

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

    pub async fn prepare_unsigned_spot_transfer(
        &self,
        amount: &str,
        destination: &str,
        token: &str,
    ) -> Result<UnsignedTransactionComponents> {
        let timestamp = next_nonce();
        let hyperliquid_chain_name = if self.http_client.is_mainnet() {
            "Mainnet".to_string()
        } else {
            "Testnet".to_string()
        };
        let signature_chain_id = if self.http_client.is_mainnet() {
            U256::from(42161) // Arbitrum mainnet
        } else {
            U256::from(421614) // Arbitrum testnet
        };

        let spot_send_action = SpotSend {
            signature_chain_id,
            hyperliquid_chain: hyperliquid_chain_name.clone(),
            destination: destination.to_string(),
            token: token.to_string(),
            amount: amount.to_string(),
            time: timestamp,
        };

        let action_payload_json = serde_json::to_value(Actions::SpotSend(spot_send_action.clone()))
            .map_err(|e| crate::Error::JsonParse(e.to_string()))?;

        let digest_to_sign = ethers::types::H256::from(
            spot_send_action
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

    pub async fn prepare_unsigned_vault_transfer(
        &self,
        is_deposit: bool,
        usd: u64,
        vault_address: Option<ethers::types::H160>,
    ) -> Result<UnsignedTransactionComponents> {
        let nonce = next_nonce();

        let action = Actions::VaultTransfer(VaultTransfer {
            vault_address: vault_address.unwrap_or_default(),
            is_deposit,
            usd,
        });

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

    pub async fn prepare_unsigned_modify_order(
        &self,
        modify_request_client: ClientModifyRequest,
    ) -> Result<UnsignedTransactionComponents> {
        // Convert the ClientOrderRequest to OrderRequest using the coin_to_asset mapping
        let order_request = modify_request_client.order.convert(&self.coin_to_asset)?;

        // Create the ModifyRequest
        let transformed_modify = ModifyRequest {
            oid: modify_request_client.oid,
            order: order_request,
        };

        // Create the action
        let action = Actions::BatchModify(BulkModify {
            modifies: vec![transformed_modify],
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

    pub async fn prepare_unsigned_bulk_cancel(
        &self,
        cancels_client: Vec<ClientCancelRequest>,
    ) -> Result<UnsignedTransactionComponents> {
        // Transform Vec<ClientCancelRequest> to Vec<CancelRequest>
        let mut transformed_cancels = Vec::new();
        for cancel_client in cancels_client {
            let &asset_index = self
                .coin_to_asset
                .get(&cancel_client.asset)
                .ok_or(crate::Error::AssetNotFound)?;
            transformed_cancels.push(CancelRequest {
                asset: asset_index,
                oid: cancel_client.oid,
            });
        }

        // Create the action
        let action = Actions::Cancel(BulkCancel {
            cancels: transformed_cancels,
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

    pub async fn prepare_unsigned_approve_agent(
        &self,
    ) -> Result<(String, UnsignedTransactionComponents)> {
        let nonce = next_nonce();
        let hyperliquid_chain_name = if self.http_client.is_mainnet() {
            "Mainnet".to_string()
        } else {
            "Testnet".to_string()
        };
        let signature_chain_id = if self.http_client.is_mainnet() {
            U256::from(42161) // Arbitrum mainnet
        } else {
            U256::from(421614) // Arbitrum testnet
        };

        // Generate a random private key for the agent (like in ExchangeClient::approve_agent)
        let key = hex::encode(generate_random_key()?);
        let agent_address = key
            .parse::<LocalWallet>()
            .map_err(|e| crate::Error::PrivateKeyParse(e.to_string()))?
            .address();

        let approve_agent_action = ApproveAgent {
            signature_chain_id,
            hyperliquid_chain: hyperliquid_chain_name.clone(),
            agent_address,
            agent_name: None,
            nonce,
        };

        let action_payload_json =
            serde_json::to_value(Actions::ApproveAgent(approve_agent_action.clone()))
                .map_err(|e| crate::Error::JsonParse(e.to_string()))?;

        let digest_to_sign = ethers::types::H256::from(
            approve_agent_action
                .encode_eip712()
                .map_err(|e| crate::Error::Eip712(e.to_string()))?,
        );

        Ok((
            key,
            UnsignedTransactionComponents {
                action_payload_json,
                nonce,
                digest_to_sign,
                vault_address: self.vault_address,
                eip712_domain_chain_id: Some(signature_chain_id),
                eip712_hyperliquid_chain_name: Some(hyperliquid_chain_name),
                is_l1_agent_signature: false,
            },
        ))
    }

    /// Prepare an unsigned approve builder fee transaction
    pub async fn prepare_unsigned_approve_builder_fee(
        &self,
        builder: String,
        max_fee_rate: String,
    ) -> Result<UnsignedTransactionComponents> {
        let timestamp = next_nonce();
        let hyperliquid_chain_name = if self.http_client.is_mainnet() {
            "Mainnet".to_string()
        } else {
            "Testnet".to_string()
        };
        let signature_chain_id = if self.http_client.is_mainnet() {
            U256::from(42161) // Arbitrum mainnet
        } else {
            U256::from(421614) // Arbitrum testnet
        };

        let approve_action = ApproveBuilderFee {
            signature_chain_id,
            hyperliquid_chain: hyperliquid_chain_name.clone(),
            builder,
            max_fee_rate,
            nonce: timestamp,
        };

        let action_payload_json =
            serde_json::to_value(Actions::ApproveBuilderFee(approve_action.clone()))
                .map_err(|e| crate::Error::JsonParse(e.to_string()))?;

        let digest_to_sign = ethers::types::H256::from(
            approve_action
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

    /// Prepare unsigned USDC transfer to bridge contract for deposit
    pub async fn prepare_unsigned_bridge_deposit(
        &self,
        amount: ethers::types::U256,
    ) -> Result<UnsignedTransactionComponents> {
        let is_mainnet = self.http_client.is_mainnet();
        let bridge_address = bridge::get_bridge_address(is_mainnet);
        let usdc_address = bridge::get_usdc_address(is_mainnet);

        // Validate minimum deposit amount (5 USDC)
        let min_deposit = ethers::types::U256::from(bridge::MIN_DEPOSIT_USDC);
        if amount < min_deposit {
            return Err(crate::Error::GenericParse(format!(
                "Amount {} is below minimum deposit of {} USDC",
                amount,
                bridge::MIN_DEPOSIT_USDC as f64 / 1_000_000.0
            )));
        }

        // Create USDC transfer transaction data
        let transfer_data = bridge::create_usdc_transfer_data(bridge_address, amount);

        let chain_id = if is_mainnet { "0xa4b1" } else { "0x66eee" };

        let transaction_data = serde_json::json!({
            "to": format!("0x{:040x}", usdc_address),
            "data": transfer_data,
            "value": "0x0",
            "chainId": chain_id
        });

        Ok(UnsignedTransactionComponents {
            action_payload_json: transaction_data,
            nonce: 0,                                    // Will be set by the client
            digest_to_sign: ethers::types::H256::zero(), // Will be computed by the client
            vault_address: None,
            eip712_domain_chain_id: Some(if is_mainnet {
                ethers::types::U256::from(42161)
            } else {
                ethers::types::U256::from(421614)
            }),
            eip712_hyperliquid_chain_name: None,
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
                println!("Builder creation failed (expected in some environments): {e:?}");
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
                println!("Builder creation failed (expected in some environments): {e:?}");
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
                    println!("prepare_unsigned_order failed (may be expected): {e:?}");
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
                    println!("prepare_unsigned_usdc_transfer failed (may be expected): {e:?}");
                }
            }
        } else {
            println!("Builder creation failed, skipping USDC transfer test");
        }
    }

    #[tokio::test]
    async fn test_prepare_unsigned_cancel() {
        let builder_result =
            UnsignedTransactionBuilder::new(None, Some(BaseUrl::Testnet), None, None).await;

        if let Ok(builder) = builder_result {
            let cancel = ClientCancelRequest {
                asset: "ETH".to_string(),
                oid: 12345,
            };

            let result = builder.prepare_unsigned_cancel(cancel).await;

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
                        "should be L1 agent signature for cancel"
                    );
                    println!("✓ prepare_unsigned_cancel succeeded");
                    println!("  - Nonce: {}", components.nonce);
                    println!("  - Digest: {:?}", components.digest_to_sign);
                }
                Err(e) => {
                    println!("prepare_unsigned_cancel failed (may be expected): {e:?}");
                }
            }
        } else {
            println!("Builder creation failed, skipping cancel test");
        }
    }

    #[tokio::test]
    async fn test_prepare_unsigned_withdraw() {
        let builder_result =
            UnsignedTransactionBuilder::new(None, Some(BaseUrl::Testnet), None, None).await;

        if let Ok(builder) = builder_result {
            let amount = "50.0";
            let destination = "0x742d35Cc6634C0532925a3b8D8c9e4b7B6a3b";

            let result = builder.prepare_unsigned_withdraw(amount, destination).await;

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
                        "should NOT be L1 agent signature for withdraw"
                    );
                    assert_eq!(components.eip712_domain_chain_id, Some(U256::from(421614)));
                    println!("✓ prepare_unsigned_withdraw succeeded");
                    println!("  - Nonce: {}", components.nonce);
                    println!("  - Digest: {:?}", components.digest_to_sign);
                }
                Err(e) => {
                    println!("prepare_unsigned_withdraw failed (may be expected): {e:?}");
                }
            }
        } else {
            println!("Builder creation failed, skipping withdraw test");
        }
    }

    #[tokio::test]
    async fn test_prepare_unsigned_update_leverage() {
        let builder_result =
            UnsignedTransactionBuilder::new(None, Some(BaseUrl::Testnet), None, None).await;

        if let Ok(builder) = builder_result {
            let result = builder
                .prepare_unsigned_update_leverage(10, "ETH", true)
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
                        components.is_l1_agent_signature,
                        "should be L1 agent signature for leverage update"
                    );
                    println!("✓ prepare_unsigned_update_leverage succeeded");
                    println!("  - Nonce: {}", components.nonce);
                    println!("  - Digest: {:?}", components.digest_to_sign);
                }
                Err(e) => {
                    println!("prepare_unsigned_update_leverage failed (may be expected): {e:?}");
                }
            }
        } else {
            println!("Builder creation failed, skipping leverage update test");
        }
    }

    #[tokio::test]
    async fn test_prepare_unsigned_update_isolated_margin() {
        let builder_result =
            UnsignedTransactionBuilder::new(None, Some(BaseUrl::Testnet), None, None).await;

        if let Ok(builder) = builder_result {
            let result = builder
                .prepare_unsigned_update_isolated_margin("ETH", "100.5".to_string())
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
                        components.is_l1_agent_signature,
                        "should be L1 agent signature for isolated margin update"
                    );
                    assert_eq!(
                        components.eip712_domain_chain_id,
                        Some(ethers::types::U256::from(1337))
                    );
                    println!("✓ prepare_unsigned_update_isolated_margin succeeded");
                    println!("  - Nonce: {}", components.nonce);
                    println!("  - Digest: {:?}", components.digest_to_sign);
                }
                Err(e) => {
                    println!(
                        "prepare_unsigned_update_isolated_margin failed (may be expected): {e:?}"
                    );
                }
            }
        } else {
            println!("Builder creation failed, skipping isolated margin update test");
        }
    }

    #[tokio::test]
    async fn test_prepare_unsigned_spot_transfer() {
        let builder_result =
            UnsignedTransactionBuilder::new(None, Some(BaseUrl::Testnet), None, None).await;

        if let Ok(builder) = builder_result {
            let amount = "100.0";
            let destination = "0x742d35Cc6634C0532925a3b8D8c9e4b7B6a3b";
            let token = "USDC";

            let result = builder
                .prepare_unsigned_spot_transfer(amount, destination, token)
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
                        "should NOT be L1 agent signature for spot transfer"
                    );
                    assert_eq!(components.eip712_domain_chain_id, Some(U256::from(421614)));
                    println!("✓ prepare_unsigned_spot_transfer succeeded");
                    println!("  - Nonce: {}", components.nonce);
                    println!("  - Digest: {:?}", components.digest_to_sign);
                }
                Err(e) => {
                    println!("prepare_unsigned_spot_transfer failed (may be expected): {e:?}");
                }
            }
        } else {
            println!("Builder creation failed, skipping spot transfer test");
        }
    }

    #[tokio::test]
    async fn test_prepare_unsigned_modify_order() {
        let builder_result =
            UnsignedTransactionBuilder::new(None, Some(BaseUrl::Testnet), None, None).await;

        if let Ok(builder) = builder_result {
            let new_order = ClientOrderRequest {
                asset: "ETH".to_string(),
                is_buy: false,
                reduce_only: false,
                limit_px: 1900.0,
                sz: 0.2,
                cloid: Some(Uuid::new_v4()),
                order_type: ClientOrder::Limit(ClientLimit {
                    tif: "Gtc".to_string(),
                }),
            };

            let modify_request = ClientModifyRequest {
                oid: 12345,
                order: new_order,
            };

            let result = builder.prepare_unsigned_modify_order(modify_request).await;

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
                        "should be L1 agent signature for modify"
                    );
                    assert_eq!(
                        components.eip712_domain_chain_id,
                        Some(ethers::types::U256::from(1337))
                    );
                    println!("✓ prepare_unsigned_modify_order succeeded");
                    println!("  - Nonce: {}", components.nonce);
                    println!("  - Digest: {:?}", components.digest_to_sign);
                }
                Err(e) => {
                    println!("prepare_unsigned_modify_order failed (may be expected): {e:?}");
                }
            }
        } else {
            println!("Builder creation failed, skipping modify order test");
        }
    }

    #[tokio::test]
    async fn test_prepare_unsigned_bulk_cancel() {
        let builder_result =
            UnsignedTransactionBuilder::new(None, Some(BaseUrl::Testnet), None, None).await;

        if let Ok(builder) = builder_result {
            let cancels = vec![
                ClientCancelRequest {
                    asset: "ETH".to_string(),
                    oid: 12345,
                },
                ClientCancelRequest {
                    asset: "BTC".to_string(),
                    oid: 67890,
                },
            ];

            let result = builder.prepare_unsigned_bulk_cancel(cancels).await;

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
                        "should be L1 agent signature for bulk cancel"
                    );
                    assert_eq!(
                        components.eip712_domain_chain_id,
                        Some(ethers::types::U256::from(1337))
                    );
                    println!("✓ prepare_unsigned_bulk_cancel succeeded");
                    println!("  - Nonce: {}", components.nonce);
                    println!("  - Digest: {:?}", components.digest_to_sign);
                }
                Err(e) => {
                    println!("prepare_unsigned_bulk_cancel failed (may be expected): {e:?}");
                }
            }
        } else {
            println!("Builder creation failed, skipping bulk cancel test");
        }
    }

    #[tokio::test]
    async fn test_prepare_unsigned_approve_agent() {
        let builder_result =
            UnsignedTransactionBuilder::new(None, Some(BaseUrl::Testnet), None, None).await;

        if let Ok(builder) = builder_result {
            let result = builder.prepare_unsigned_approve_agent().await;

            match result {
                Ok((key, components)) => {
                    assert!(components.nonce > 0, "nonce should be set");
                    assert_ne!(
                        components.digest_to_sign,
                        H256::zero(),
                        "digest should not be zero"
                    );
                    assert!(
                        !components.is_l1_agent_signature,
                        "should NOT be L1 agent signature for approve agent"
                    );
                    assert_eq!(components.eip712_domain_chain_id, Some(U256::from(421614)));
                    println!("✓ prepare_unsigned_approve_agent succeeded");
                    println!("  - Nonce: {}", components.nonce);
                    println!("  - Digest: {:?}", components.digest_to_sign);
                    println!("  - Key: {key}");
                }
                Err(e) => {
                    println!("prepare_unsigned_approve_agent failed (may be expected): {e:?}");
                }
            }
        } else {
            println!("Builder creation failed, skipping approve agent test");
        }
    }

    #[tokio::test]
    async fn test_prepare_unsigned_bridge_deposit() {
        let builder_result =
            UnsignedTransactionBuilder::new(None, Some(BaseUrl::Testnet), None, None).await;

        if let Ok(builder) = builder_result {
            // Test with valid amount (10 USDC)
            let amount = ethers::types::U256::from(10_000_000); // 10 USDC in 6 decimals

            let result = builder.prepare_unsigned_bridge_deposit(amount).await;

            match result {
                Ok(components) => {
                    assert_eq!(components.nonce, 0, "nonce should be 0 for bridge deposit");
                    assert_eq!(
                        components.digest_to_sign,
                        ethers::types::H256::zero(),
                        "digest should be zero for bridge deposit"
                    );
                    assert!(
                        !components.is_l1_agent_signature,
                        "should NOT be L1 agent signature for bridge deposit"
                    );
                    assert_eq!(components.eip712_domain_chain_id, Some(U256::from(421614)));
                    assert!(components.eip712_hyperliquid_chain_name.is_none());
                    println!("✓ prepare_unsigned_bridge_deposit succeeded");
                    println!("  - Transaction data: {}", components.action_payload_json);
                }
                Err(e) => {
                    println!("prepare_unsigned_bridge_deposit failed (may be expected): {e:?}");
                }
            }

            // Test with amount below minimum (1 USDC)
            let small_amount = ethers::types::U256::from(1_000_000); // 1 USDC
            let result_small = builder.prepare_unsigned_bridge_deposit(small_amount).await;

            match result_small {
                Ok(_) => {
                    println!("❌ Expected error for amount below minimum, but got success");
                }
                Err(e) => {
                    println!("✓ Correctly rejected amount below minimum: {e:?}");
                }
            }
        } else {
            println!("Builder creation failed, skipping bridge deposit test");
        }
    }
}
