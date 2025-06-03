# Environment Setup Guide

This project uses environment variables to securely manage configuration, including private keys for testing.

## Setting up your `.env` file

1. Create a `.env` file in the root directory of the project:
   ```bash
   touch .env
   ```

2. Add your configuration to the `.env` file:
   ```bash
   # Hyperliquid Rust SDK Test Configuration
   # WARNING: This should only contain TEST private keys, never production keys!
   
   # Test private key for Hyperliquid SDK testing (testnet only!)
   TEST_PRIVATE_KEY=0x846b1a9525bc36f4f
   ```

## Important Security Notes

⚠️ **NEVER commit your `.env` file to version control!**

- The `.env` file is already included in `.gitignore`
- Only use test private keys, never real ones with funds
- For production deployments, use proper secret management systems

## Running the tests

Once your `.env` file is set up, you can run the signed transaction tests:

```bash
# Run the signed transaction test binary
cargo run --bin signed_transaction_test

# Or run specific tests within the binary by modifying the main() function
```

## Environment Variable Reference

| Variable | Description | Required | Example |
|----------|-------------|----------|---------|
| `TEST_PRIVATE_KEY` | Private key for testing (testnet only) | Yes | `0x86f4f` |

## Troubleshooting

If you see an error like:
```
TEST_PRIVATE_KEY environment variable not found. Please set it in your .env file or environment.
```

Make sure:
1. Your `.env` file exists in the project root
2. The `TEST_PRIVATE_KEY` variable is properly set
3. There are no extra spaces around the `=` sign
4. The private key starts with `0x` 