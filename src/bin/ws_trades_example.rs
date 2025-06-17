use hl_ranger::info::InfoClient;
use hl_ranger::ws::{Message, Subscription};
use hl_ranger::BaseUrl;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::init();

    let coin = "ETH".to_string();

    // --- REST Request & WebSocket Subscription ---
    // 1. Create an InfoClient
    let mut info_client = InfoClient::new(None, Some(BaseUrl::Testnet)).await?;

    // 2. Fetch recent trades
    println!("Fetching recent trades for {}...", coin);
    let recent_trades = info_client.recent_trades(coin.clone()).await?;
    println!("Recent trades: {:#?}\n", recent_trades);

    // 3. Create a channel to receive messages
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // 4. Subscribe to trade updates for a coin
    let subscription = Subscription::Trades { coin: coin.clone() };

    let subscription_id = info_client.subscribe(subscription, tx).await?;
    println!(
        "Subscribed to live trade updates for {} with subscription id: {}",
        coin, subscription_id
    );

    // 5. Listen for messages
    println!("Listening for live trade updates...");
    while let Some(message) = rx.recv().await {
        match message {
            Message::Trades(trades) => {
                println!("Received live trades update: {:?}", trades);
            }
            Message::NoData => {
                println!("Websocket disconnected. The client will attempt to reconnect.");
            }
            _ => {
                println!("Received other message: {:?}", message);
            }
        }
    }

    Ok(())
}
