use ethers::types::{H160, H256, U256};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct UnsignedTransactionComponents {
    pub action_payload_json: Value, // The "action" field for the final ExchangePayload
    pub nonce: u64,                 // The nonce (timestamp) used
    pub digest_to_sign: H256,       // The actual H256 hash to be signed

    // Optional context helpful for reconstructing the EIP-712 typed data or understanding the signature type
    pub vault_address: Option<H160>, // Vault address if applicable
    pub eip712_domain_chain_id: Option<U256>, // e.g., 421614 for Arbitrum or 1337 for L1 agent
    pub eip712_hyperliquid_chain_name: Option<String>, // "Mainnet" or "Testnet" for some EIP-712 structs
    pub is_l1_agent_signature: bool, // True if digest is for l1::Agent, false for direct EIP-712 on action
} 