use ethers::signers::LocalWallet;
use log::info;

use hl_ranger::{
    BaseUrl, BuilderInfo, ClientCancelRequest, ClientLimit, ClientOrder, ClientOrderRequest,
    ExchangeClient, ExchangeDataStatus, ExchangeResponseStatus,
};
use std::{thread::sleep, time::Duration};

#[tokio::main]
async fn main() {
    env_logger::init();
    // Key was randomly generated for testing and shouldn't be used with any real funds
    let wallet: LocalWallet = "e908f86dbb4d55ac876378565aafeabc187f6690f046459397b17d9b9a19688e"
        .parse()
        .unwrap();

    let exchange_client = ExchangeClient::new(None, wallet, Some(BaseUrl::Testnet), None, None)
        .await
        .unwrap();

    let order = ClientOrderRequest {
        asset: "ETH".to_string(),
        is_buy: true,
        reduce_only: false,
        limit_px: 1800.0,
        sz: 0.01,
        cloid: None,
        order_type: ClientOrder::Limit(ClientLimit {
            tif: "Gtc".to_string(),
        }),
    };

    let fee = 1u64;
    let builder = "0xF5Bc9107916B91A3Ea5966cd2e51655D21B7Eb02";

    let response = exchange_client
        .order_with_builder(
            order,
            None,
            BuilderInfo {
                builder: builder.to_string(),
                fee,
            },
        )
        .await
        .unwrap();
    info!("Order placed: {response:?}");

    let response = match response {
        ExchangeResponseStatus::Ok(exchange_response) => exchange_response,
        ExchangeResponseStatus::Err(e) => panic!("error with exchange response: {e}"),
    };
    let status = response.data.unwrap().statuses[0].clone();
    let oid = match status {
        ExchangeDataStatus::Filled(order) => order.oid,
        ExchangeDataStatus::Resting(order) => order.oid,
        _ => panic!("Error: {status:?}"),
    };

    // So you can see the order before it's cancelled
    sleep(Duration::from_secs(10));

    let cancel = ClientCancelRequest {
        asset: "ETH".to_string(),
        oid,
    };

    // This response will return an error if order was filled (since you can't cancel a filled order), otherwise it will cancel the order
    let response = exchange_client.cancel(cancel, None).await.unwrap();
    info!("Order potentially cancelled: {response:?}");
}
