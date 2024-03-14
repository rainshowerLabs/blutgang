use crate::config::system::{
    encode,
    get_registry,
    get_storage_registry,
    MetricReceiver,
    MetricSender,
    RegistryChannel,
    RpcMetrics,
};
use crate::log_info;
use crate::rpc::error::RpcError;
use reqwest::Client;
use serde_json::{
    json,
    Value,
};
use url::Url;

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
    ma_length: f64,
    // ???
    // pub throughput: f64,
}

unsafe impl Sync for Status {}

#[derive(Debug, Clone)]
pub struct Rpc {
    pub name: String,           // sanitized name for appearing in logs
    url: String,                // url of the rpc we're forwarding requests to.
    client: Client,             // Reqwest client
    pub ws_url: Option<String>, // url of the websocket we're forwarding requests to.
    pub status: Status,         // stores stats related to the rpc.
    // For max_consecutive
    pub max_consecutive: u32, // max times we can call an rpc in a row
    pub consecutive: u32,
    // For max_per_second
    pub last_used: u128,      // last time we sent a querry to this node
    pub min_time_delta: u128, // microseconds
}

// Sanitizes URLs so secrets don't get outputed.
//
// For example, if we have a URL: https://eth-mainnet.g.alchemy.com/v2/api-key
// as input, we output: https://eth-mainnet.g.alchemy.com/
fn sanitize_url(url: &str) -> Result<String, url::ParseError> {
    let parsed_url = Url::parse(url)?;

    // Build a new URL with the scheme, host, and port (if any), but without the path or query
    let sanitized = Url::parse(&format!(
        "{}://{}{}",
        parsed_url.scheme(),
        parsed_url.host_str().unwrap_or_default(),
        match parsed_url.port() {
            Some(port) => format!(":{}", port),
            None => String::new(),
        }
    ))?;

    Ok(sanitized.to_string())
}

unsafe impl Sync for Rpc {}

impl Default for Rpc {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            url: "".to_string(),
            ws_url: None,
            client: Client::new(),
            status: Status::default(),
            max_consecutive: 0,
            consecutive: 0,
            last_used: 0,
            min_time_delta: 0,
        }
    }
}

// implement new for rpc
impl Rpc {
    pub fn new(
        url: String,
        ws_url: Option<String>,
        max_consecutive: u32,
        min_time_delta: u128,
        ma_length: f64,
    ) -> Self {
        Self {
            name: sanitize_url(&url).unwrap_or(url.clone()),
            url,
            client: Client::new(),
            ws_url,
            status: Status {
                ma_length,
                ..Default::default()
            },
            max_consecutive,
            consecutive: 0,
            last_used: 0,
            min_time_delta,
        }
    }

    // Explicitly get the url of the Rpc, potentially dangerous as it can expose basic auth
    #[cfg(test)]
    pub fn get_url(&self) -> String {
        self.url.clone()
    }

    // Generic fn to send rpc
    pub async fn send_request(&self, tx: Value) -> Result<String, crate::rpc::types::RpcError> {
        #[cfg(feature = "debug-verbose")]
        println!("Sending request: {}", tx.clone());
        let response = match self.client.post(&self.url).json(&tx).send().await {
            Ok(response) => response,
            Err(err) => {
                return Err(crate::rpc::types::RpcError::InvalidResponse(
                    err.to_string(),
                ))
            }
        };
        #[cfg(feature = "debug-verbose")]
        {
            let a = response.text().await.unwrap();
            println!("response: {}", a);
            return Ok(a);
        }

        #[cfg(not(feature = "debug-verbose"))]
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
        let return_number = extract_number(&number)?;

        Ok(return_number)
    }

    // Get the latest finalized block
    pub async fn get_finalized_block(&self) -> Result<u64, crate::rpc::types::RpcError> {
        let request = json!({
            "method": "eth_getBlockByNumber".to_string(),
            "params": ["finalized", false],
            "id": 1,
            "jsonrpc": "2.0".to_string(),
        });

        let number: Value =
            unsafe { simd_json::serde::from_str(&mut self.send_request(request).await?).unwrap() };
        let number = &number["result"]["number"];

        let number = match number.as_str() {
            Some(number) => number,
            None => {
                return Err(RpcError::InvalidResponse(
                    "error: Can't get finalized block!".to_string(),
                ))
            }
        };

        let return_number = match hex_to_decimal(number) {
            Ok(return_number) => return_number,
            Err(err) => return Err(RpcError::InvalidResponse(err.to_string())),
        };

        Ok(return_number)
    }

    // Update the latency of the last n calls.
    // We don't do it within send_request because we might kill it if it times out.
    pub fn update_latency(&mut self, latest: f64) {
        #[cfg(feature = "prometheusd")]
        let metric = RpcMetrics::init(get_storage_registry()).unwrap();
        #[cfg(feature = "prometheusd")]
        let metric_channel = RegistryChannel::new();
        #[cfg(feature = "prometheusd")]
        let (mut tx, mut rx) = RegistryChannel::channel("prometheus latency demo");
        // If we have data >= to ma_length, remove the first one in line
        if self.status.latency_data.len() >= self.status.ma_length as usize {
            self.status.latency_data.remove(0);
        }

        // Update latency
        self.status.latency_data.push(latest);
        let avg =
            self.status.latency_data.iter().sum::<f64>() / self.status.latency_data.len() as f64;
        self.status.latency = avg;
        #[cfg(feature = "prometheusd")]
        RegistryChannel::push_metrics(metric.clone(), &self.url, &self.name, avg, rx, tx);
        // Non-channel version
        // #[cfg(feature = "prometheusd")]
        // RpcMetrics::push_latency(&metric, &self.url, &self.name, avg);
        #[cfg(feature = "prometheusd")]
        let report = RegistryChannel::encode_channel(&metric_channel);
        #[cfg(feature = "prometheusd")]
        log_info!("Prometheus metrics: {}", report);
    }
}

// Take in the result of eth_getBlockByNumber, and extract the block number
fn extract_number(rx: &str) -> Result<u64, RpcError> {
    let mut rx = rx.to_string();

    let json: Value = unsafe { simd_json::serde::from_str(&mut rx)? };

    let number = match json["result"].as_str() {
        Some(number) => number,
        None => {
            return Err(RpcError::InvalidResponse(
                "error: Extracting response from request failed!".to_string(),
            ))
        }
    };

    let number = hex_to_decimal(number).unwrap();
    Ok(number)
}

pub fn hex_to_decimal(hex_string: &str) -> Result<u64, std::num::ParseIntError> {
    // TODO: theres a bizzare edge case where the last " isnt removed in the
    // previou step so check for that here and remove it if necessary
    let hex_string: &str = &hex_string.replace('\"', "");

    // Remove `0x` prefix if it exists
    let hex_string = hex_string.trim_start_matches("0x");

    u64::from_str_radix(hex_string, 16)
}
