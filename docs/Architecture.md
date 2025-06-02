## Project Architecture

This document outlines the architecture of the `hyperliquid-rust-sdk` project.

### Core Library (`src/`)

Contains the main SDK logic, including:
- `client.rs`: HTTP client functionalities.
- `exchange/`: Modules for interacting with the exchange (placing orders, cancelling, etc.).
  - `exchange_client.rs`: Client for sending signed transactions to the exchange.
  - `actions.rs`: Defines EIP-712 typed data structures for various exchange actions.
  - `order.rs`, `cancel.rs`, `modify.rs`: Request structures for order operations.
- `info/`: Modules for fetching read-only data from the info API.
  - `info_client.rs`: Client for info API requests and WebSocket subscriptions.
- `unsigned/`: Modules for generating unsigned transaction components.
  - `builder.rs`: `UnsignedTransactionBuilder` for creating transaction components that can be signed externally.
  - `components.rs`: `UnsignedTransactionComponents` struct holding the parts of an unsigned transaction.
- `signature/`: Helper functions for cryptographic signing of transactions.
- `ws/`: WebSocket client for streaming data.
- `meta.rs`: Structures for metadata.
- `errors.rs`: Custom error types.
- `helpers.rs`: Utility functions.
- `lib.rs`: Main library entry point, re-exporting key modules and types.

### Binaries (`src/bin/`)

Executable examples and test utilities showcasing SDK usage.

- `agent.rs`: Example of creating and using an agent wallet.
- `approve_builder_fee.rs`: Example of approving a builder fee.
- `bridge_withdraw.rs`: Example of withdrawing from the L1 bridge.
- `class_transfer.rs`: Example of transferring USDC between spot and perp accounts.
- `info.rs`: Example demonstrating various `InfoClient` functionalities.
- `leverage.rs`: Example of updating leverage and isolated margin.
- `market_maker.rs`: Basic market making bot example.
- `market_order_and_cancel.rs`: Example of placing and cancelling a market order.
- `market_order_with_builder_and_cancel.rs`: Example of a market order with a builder fee.
- `order_and_cancel.rs`: Example of placing and cancelling a limit order by OID.
- `order_and_cancel_cloid.rs`: Example of placing and cancelling a limit order by CLOID.
- `order_with_builder_and_cancel.rs`: Example of a limit order with a builder fee.
- `set_referrer.rs`: Example of setting a referrer code.
- `spot_order.rs`: Example of placing a spot order.
- `spot_transfer.rs`: Example of transferring spot assets.
- `unsigned_transaction_example.rs`: Demonstrates how to use `UnsignedTransactionBuilder` to prepare various transaction types for external signing.
- `signed_transaction_test.rs`: **NEW** - Test binary for creating unsigned transactions via `UnsignedTransactionBuilder`, signing them with a test private key, and posting them to the Hyperliquid testnet. Covers orders, cancels, USDC transfers, withdrawals, leverage updates, spot transfers, vault transfers, and bulk cancels.
- `usdc_transfer.rs`: Example of transferring USDC.
- `vault_transfer.rs`: Example of depositing/withdrawing from a vault.
- `ws_*.rs`: Various examples demonstrating WebSocket subscriptions (allMids, candles, l2Book, orders, trades, userEvents, etc.).

### Configuration & CI

- `Cargo.toml`: Project manifest, dependencies, and binary definitions.
- `ci.sh`: Continuous integration script (build, fmt, clippy, test).
- `.github/workflows/master.yml`: GitHub Actions workflow for CI.

### Documentation

- `README.md`: Overview, features, installation, and usage examples.
- `LICENSE.md`: MIT License.
- `docs/Architecture.md`: (This file) Overview of the project structure. 