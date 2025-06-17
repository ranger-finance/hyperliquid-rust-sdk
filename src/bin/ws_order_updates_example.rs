use hl_ranger::info::InfoClient;
use hl_ranger::ws::{Message, Subscription};
use hl_ranger::BaseUrl;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::init();

    // --- WebSocket Subscription ---
    // 1. Create an InfoClient
    let mut info_client = InfoClient::new(None, Some(BaseUrl::Testnet)).await?;

    // 2. Create a channel to receive messages
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // 3. Subscribe to L2Book updates for a coin (e.g., ETH)
    let subscription = Subscription::L2Book {
        coin: "ETH".to_string(),
    };

    let subscription_id = info_client.subscribe(subscription, tx).await?;
    println!(
        "Subscribed to L2 book updates with subscription id: {}",
        subscription_id
    );

    // 4. Listen for messages
    println!("Listening for L2 book updates...");
    while let Some(message) = rx.recv().await {
        match message {
            Message::L2Book(l2_book) => {
                println!("Received L2 book update: {:?}", l2_book);
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
