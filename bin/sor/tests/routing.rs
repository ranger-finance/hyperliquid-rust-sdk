use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use sor::routing::strategies::{BestPriceStrategy, RoutingStrategy};
use sor_models::{
    Quote, QuoteRequestParams, TradeSide,
};
use sor_models::market::{InstrumentType, FeeAsset};

/// Test data builders
mod test_data {
    use super::*;
    use chrono::Utc;

    pub fn sample_quote_request() -> QuoteRequestParams {
        QuoteRequestParams {
            symbol: "BTC-USD".to_string(),
            side: TradeSide::Buy,
            quantity: dec!(1.0),
            price_limit: None,
        }
    }

    pub fn sample_quote(venue_name: &str, price: Decimal, fee: Decimal) -> Quote {
        Quote {
            symbol: "BTC-USD".to_string(),
            side: TradeSide::Buy,
            size: dec!(1.0),
            price,
            venue_name: venue_name.to_string(),
            fees: fee,
            total_cost: price * dec!(1.0) + fee,
            timestamp: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
        }
    }
}

/// Core routing strategy tests
mod routing_strategy_tests {
    use super::*;

    #[tokio::test]
    async fn test_best_price_strategy_selects_cheapest_quote() {
        let strategy = BestPriceStrategy::new();
        let quote_params = test_data::sample_quote_request();

        let quotes = vec![
            (
                "venue1".to_string(),
                test_data::sample_quote("venue1", dec!(50000.0), dec!(10.0)),
            ),
            (
                "venue2".to_string(),
                test_data::sample_quote("venue2", dec!(49900.0), dec!(15.0)),
            ), // Better price
            (
                "venue3".to_string(),
                test_data::sample_quote("venue3", dec!(50100.0), dec!(5.0)),
            ),
        ];

        let result = strategy.select_best_quote(quotes, &quote_params).await.unwrap();

        assert!(result.is_some());
        let (venue_id, quote) = result.unwrap();
        assert_eq!(venue_id, "venue2");
        assert_eq!(quote.price, dec!(49900.0));
    }

    #[tokio::test]
    async fn test_best_price_strategy_handles_empty_quotes() {
        let strategy = BestPriceStrategy::new();
        let quote_params = test_data::sample_quote_request();
        let quotes = vec![];

        let result = strategy.select_best_quote(quotes, &quote_params).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_best_price_strategy_considers_fees() {
        let strategy = BestPriceStrategy::new();
        let quote_params = test_data::sample_quote_request();

        let quotes = vec![
            (
                "venue1".to_string(),
                test_data::sample_quote("venue1", dec!(50000.0), dec!(5.0)),
            ), // Lower price, higher fee
            (
                "venue2".to_string(),
                test_data::sample_quote("venue2", dec!(50010.0), dec!(1.0)),
            ), // Higher price, lower fee
        ];

        let result = strategy.select_best_quote(quotes, &quote_params).await.unwrap();

        assert!(result.is_some());
        let (venue_id, quote) = result.unwrap();
        // Should select venue1 because total cost is lower (50005 vs 50011)
        assert_eq!(venue_id, "venue1");
        assert_eq!(quote.total_cost, dec!(50005.0));
    }

    #[tokio::test]
    async fn test_strategy_name() {
        let strategy = BestPriceStrategy::new();
        assert_eq!(strategy.name(), "BestPrice");
    }

    #[tokio::test]
    async fn test_best_price_strategy_total_cost_calculation() {
        let strategy = BestPriceStrategy::new();
        let quote_params = test_data::sample_quote_request();

        let quotes = vec![
            (
                "venue1".to_string(),
                test_data::sample_quote("venue1", dec!(50000.0), dec!(100.0)),
            ),
        ];

        let result = strategy.select_best_quote(quotes, &quote_params).await.unwrap();

        assert!(result.is_some());
        let (_, quote) = result.unwrap();
        assert_eq!(quote.total_cost, dec!(50100.0)); // price + fees
    }

    #[tokio::test]
    async fn test_quote_score_calculation() {
        let strategy = BestPriceStrategy::new();
        let quote_params = test_data::sample_quote_request();

        // Test that lower total cost gets better score
        let quotes = vec![
            (
                "expensive".to_string(),
                test_data::sample_quote("expensive", dec!(51000.0), dec!(50.0)),
            ),
            (
                "cheap".to_string(),
                test_data::sample_quote("cheap", dec!(49000.0), dec!(25.0)),
            ),
        ];

        let result = strategy.select_best_quote(quotes, &quote_params).await.unwrap();
        assert!(result.is_some());
        let (venue_id, _) = result.unwrap();
        assert_eq!(venue_id, "cheap");
    }
}

mod quote_tests {
    use super::*;

    #[tokio::test]
    async fn test_quote_creation() {
        let quote = test_data::sample_quote("test_venue", dec!(50000.0), dec!(10.0));
        
        assert_eq!(quote.symbol, "BTC-USD");
        assert_eq!(quote.side, TradeSide::Buy);
        assert_eq!(quote.size, dec!(1.0));
        assert_eq!(quote.price, dec!(50000.0));
        assert_eq!(quote.venue_name, "test_venue");
        assert_eq!(quote.fees, dec!(10.0));
        assert_eq!(quote.total_cost, dec!(50010.0));
    }

    #[tokio::test]
    async fn test_quote_is_expired() {
        let mut quote = test_data::sample_quote("test_venue", dec!(50000.0), dec!(10.0));
        
        // Set expiry to past
        quote.expires_at = chrono::Utc::now() - chrono::Duration::minutes(1);
        
        // Note: We'd need an is_expired method on Quote to test this properly
        // For now, just verify the expiry time is in the past
        assert!(quote.expires_at < chrono::Utc::now());
    }

    #[tokio::test]
    async fn test_quote_effective_price() {
        let quote = test_data::sample_quote("test_venue", dec!(50000.0), dec!(10.0));
        
        // Effective price should include fees
        let effective_price = quote.total_cost / quote.size;
        assert_eq!(effective_price, dec!(50010.0));
    }

    #[tokio::test]
    async fn test_quote_with_different_sides() {
        let mut buy_quote = test_data::sample_quote("test_venue", dec!(50000.0), dec!(10.0));
        buy_quote.side = TradeSide::Buy;
        
        let mut sell_quote = test_data::sample_quote("test_venue", dec!(50000.0), dec!(10.0));
        sell_quote.side = TradeSide::Sell;
        
        assert_eq!(buy_quote.side, TradeSide::Buy);
        assert_eq!(sell_quote.side, TradeSide::Sell);
    }
}

mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_quote_request_validation() {
        let request = test_data::sample_quote_request();
        
        assert!(!request.symbol.is_empty());
        assert!(request.quantity > Decimal::ZERO);
        // Add more validation as needed
    }

    #[tokio::test]
    async fn test_quote_request_with_price_limit() {
        let mut request = test_data::sample_quote_request();
        request.price_limit = Some(dec!(50000.0));
        
        assert!(request.price_limit.is_some());
        assert_eq!(request.price_limit.unwrap(), dec!(50000.0));
    }

    #[tokio::test]
    async fn test_strategy_quote_selection_integration() {
        let strategy = BestPriceStrategy::new();
        let quote_params = test_data::sample_quote_request();

        // Create quotes with different total costs
        let quotes = vec![
            (
                "venue_a".to_string(),
                test_data::sample_quote("venue_a", dec!(50000.0), dec!(20.0)), // Total: 50020
            ),
            (
                "venue_b".to_string(),
                test_data::sample_quote("venue_b", dec!(49990.0), dec!(25.0)), // Total: 50015
            ),
            (
                "venue_c".to_string(),
                test_data::sample_quote("venue_c", dec!(50010.0), dec!(5.0)),  // Total: 50015
            ),
        ];

        let result = strategy.select_best_quote(quotes, &quote_params).await.unwrap();
        
        assert!(result.is_some());
        let (venue_id, quote) = result.unwrap();
        
        // Should select one of the venues with total cost 50015
        assert!(venue_id == "venue_b" || venue_id == "venue_c");
        assert_eq!(quote.total_cost, dec!(50015.0));
    }

    #[tokio::test]
    async fn test_trade_side_enum() {
        let buy_side = TradeSide::Buy;
        let sell_side = TradeSide::Sell;
        
        assert_ne!(buy_side, sell_side);
        
        // Test serialization/deserialization would go here if needed
        let buy_request = QuoteRequestParams {
            symbol: "BTC-USD".to_string(),
            side: buy_side,
            quantity: dec!(1.0),
            price_limit: None,
        };
        
        assert_eq!(buy_request.side, TradeSide::Buy);
    }

    #[tokio::test]
    async fn test_instrument_types() {
        // Test other instrument types
        assert_ne!(InstrumentType::Spot, InstrumentType::Perpetual);
        assert_ne!(InstrumentType::Perpetual, InstrumentType::Future);
        assert_ne!(InstrumentType::Future, InstrumentType::Option);
    }

    #[tokio::test]
    async fn test_fee_asset_enum() {
        assert_ne!(FeeAsset::Base, FeeAsset::Quote);
        assert_ne!(FeeAsset::Quote, FeeAsset::Native);
        assert_ne!(FeeAsset::Native, FeeAsset::Base);
    }
} 