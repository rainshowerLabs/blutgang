use crate::rpc::error::RpcError;
use reqwest::Client;
use serde_json::{
    json,
    Value,
};

// All as floats so we have an easier time getting averages, stats and terminology copied from flood.
#[derive(Debug, Clone, Default)]
pub struct Status {
    // Set this to true in case the RPC becomes unavailable
    // Also set the last time it was called, so we can check again later
    pub is_erroring: bool,
    pub last_error: u64,

    // The latency is a moving average of the last n calls
    pub latency: f64,
    pub latency_data: Vec<f64>,
    // ???
    // pub throughput: f64,
}

unsafe impl Sync for Status {}

#[derive(Debug, Clone)]
pub struct Rpc {
    pub url: String,    // url of the rpc we're forwarding requests to.
    client: Client,     // Reqwest client
    pub status: Status, // stores stats related to the rpc.
    pub max_consecutive: u32,
    pub consecutive: u32,
}

unsafe impl Sync for Rpc {}

// implement new for rpc
impl Rpc {
    pub fn new(url: String, max_consecutive: u32) -> Self {
        Self {
            url: url,
            client: Client::new(),
            status: Status::default(),
            max_consecutive: max_consecutive,
            consecutive: 0,
        }
    }

    // Generic fn to send rpc
    pub async fn send_request(&self, tx: Value) -> Result<String, crate::rpc::types::RpcError> {
        // #[cfg(debug_assertions)] {
        //     println!("Sending request: {}", request.clone());
        // }

        let response = match self.client.post(&self.url).json(&tx).send().await {
            Ok(response) => response,
            Err(err) => {
                return Err(crate::rpc::types::RpcError::InvalidResponse(
                    err.to_string(),
                ))
            }
        };

        Ok(response.text().await.unwrap())
    }

    // Request blocknumber and return its value
    pub async fn block_number(&self) -> Result<u64, crate::rpc::types::RpcError> {
        let request = json!({
            "method": "eth_blockNumber".to_string(),
            "params": serde_json::Value::Null,
            "id": 1,
            "jsonrpc": "2.0".to_string(),
        });

        let number = self.send_request(request).await?;
        let return_number = format_hex(&number);
        let return_number = hex_to_decimal(return_number).unwrap();

        Ok(return_number)
    }

    // Update the latency of the last n calls
    pub fn update_latency(&mut self, latest: f64, ma_length: f64) {
        // If we have data >= to ma_length, remove the first one in line
        if self.status.latency_data.len() >= ma_length as usize {
            self.status.latency_data.remove(0);
        }

        // Update latency
        self.status.latency_data.push(latest);
        self.status.latency =
            self.status.latency_data.iter().sum::<f64>() / self.status.latency_data.len() as f64;
    }
}

fn format_hex(hex: &str) -> &str {
    // if `hex` is "\"0x8a165b\"" only return 0x8a165b
    // if `hex` is "0x8a165b" only return 0x8a165b
    if hex.starts_with("\"") {
        &hex[1..hex.len() - 1]
    } else {
        hex
    }
}

fn hex_to_decimal(hex_string: &str) -> Result<u64, std::num::ParseIntError> {
    // remove 0x prefix if it exists
    let hex_string = if hex_string.starts_with("0x") {
        &hex_string[2..]
    } else {
        hex_string
    };

    u64::from_str_radix(hex_string, 16)
}
