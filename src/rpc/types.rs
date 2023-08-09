use reqwest::{
	Client,
	Response,
};

use serde_json::Value;

// All as floats so we have an easier time getting averages, stats and terminology copied from flood.
#[derive(Debug, Clone, Default, Copy)]
pub struct Status {
	pub is_erroring: bool,
	pub latency: f64,
	pub throughput: f64,
}

#[derive(Debug, Clone)]
pub struct Rpc {
	pub url: String, // url of the rpc we're forwarding requests to.
	client: Client, // Reqwest client
	pub rank: i32, // rank of the rpc, higer is better.
	pub status: Status, // stores stats related to the rpc.
}

// implement new for rpc
impl Rpc {
	pub fn new(url: String) -> Self {
		Self{
			url: url,
			client: Client::new(),
			rank: 0,
			status: Status::default(),
		}
	}

    // Generic fn to send rpc
    pub async fn send_request(
        &self,
        tx: Value,
    ) -> Result<Response, Box<dyn std::error::Error>> {

        // #[cfg(debug_assertions)] {
        //     println!("Sending request: {}", request.clone());
        // }

        let response = match self.client.post(&self.url).json(&tx).send().await {
            Ok(response) => response,
            Err(err) => return Err(err.to_string().into()),
        };

        Ok(response)
    }
}
