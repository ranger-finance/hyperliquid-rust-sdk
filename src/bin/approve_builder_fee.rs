use ethers::signers::LocalWallet;
use hl_ranger::{BaseUrl, ExchangeClient};
use log::info;

#[tokio::main]
async fn main() {
    env_logger::init();
    // Key was randomly generated for testing and shouldn't be used with any real funds
    let wallet: LocalWallet = "e908f86dbb4d55ac876378565aafeabc187f6690f046459397b17d9b9a19688e"
        .parse()
        .unwrap();

    let exchange_client =
        ExchangeClient::new(None, wallet.clone(), Some(BaseUrl::Testnet), None, None)
            .await
            .unwrap();

    let max_fee_rate = "0.1%";
    let builder = "0xF5Bc9107916B91A3Ea5966cd2e51655D21B7Eb02".to_lowercase();

    let resp = exchange_client
        .approve_builder_fee(builder.to_string(), max_fee_rate.to_string(), Some(&wallet))
        .await;
    info!("resp: {resp:#?}");
}
