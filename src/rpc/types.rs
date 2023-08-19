use std::println;

use reqwest::Client;
use serde_json::Value;

// All as floats so we have an easier time getting averages, stats and terminology copied from flood.
#[derive(Debug, Clone, Default)]
pub struct Status {
    // Set this to true in case the RPC becomes unavailable
    // Also set the last time it was called, so we can check again later
    pub is_erroring: bool,
    pub last_error: u64,

    // The latency is a moving average of the last 200 calls
    pub latency: f64,
    pub latency_data: Vec<f64>,

    // ???
    pub throughput: f64,
}

#[derive(Debug, Clone)]
pub struct Rpc {
    pub url: String,    // url of the rpc we're forwarding requests to.
    client: Client,     // Reqwest client
    pub status: Status, // stores stats related to the rpc.
    pub max_consecutive: u32,
    pub consecutive: u32,
}

// implement new for rpc
impl Rpc {
    pub fn new(url: String) -> Self {
        Self {
            url: url,
            client: Client::new(),
            status: Status::default(),
            max_consecutive: 6,
            consecutive: 0,
        }
    }

    // Generic fn to send rpc
    pub async fn send_request(&self, tx: Value) -> Result<String, Box<dyn std::error::Error>> {
        // #[cfg(debug_assertions)] {
        //     println!("Sending request: {}", request.clone());
        // }

        let response = match self.client.post(&self.url).json(&tx).send().await {
            Ok(response) => response,
            Err(err) => return Err(err.to_string().into()),
        };

        Ok(response.text().await?)
    }

    pub fn update_latency(&mut self, latest: f64, ma_lenght: f64) {
        // If we have data >= to ma_lenght, remove the first one in line
        if self.status.latency_data.len() >= ma_lenght as usize {
            self.status.latency_data.remove(0);
        }

        // Update latency
        self.status.latency_data.push(latest);
        self.status.latency =
            self.status.latency_data.iter().sum::<f64>() / self.status.latency_data.len() as f64;
    }
}
