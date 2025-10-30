use crate::rpc::error::RpcError;
use reqwest::Client;
use rust_tracing::deps::metrics;
use url::Url;

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
    ma_length: f64,
    // ???
    // pub throughput: f64,
}

#[derive(Debug, Clone)]
pub struct Rpc {
    pub name: String,             // sanitized name for appearing in logs
    url: url::Url,                // url of the rpc we're forwarding requests to.
    client: Client,               // Reqwest client
    pub ws_url: Option<url::Url>, // url of the websocket we're forwarding requests to.
    pub status: Status,           // stores stats related to the rpc.
    // For max_consecutive
    pub max_consecutive: u32, // max times we can call an rpc in a row
    pub consecutive: u32,
    // For max_per_second
    pub last_used: u128,      // last time we sent a query to this node
    pub min_time_delta: u128, // microseconds
}

/// Sanitizes URLs so secrets don't get outputed.
///
/// For example, if we have a URL: https://eth-mainnet.g.alchemy.com/v2/api-key
// as input, we output: https://eth-mainnet.g.alchemy.com/
fn sanitize_url(url: &url::Url) -> Result<String, url::ParseError> {
    // Build a new URL with the scheme, host, and port (if any), but without the path or query
    let sanitized = Url::parse(&format!(
        "{}://{}{}",
        url.scheme(),
        url.host_str().unwrap_or_default(),
        match url.port() {
            Some(port) => format!(":{}", port),
            None => String::new(),
        }
    ))?;

    Ok(sanitized.to_string())
}

impl Default for Rpc {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            url: "https://eth.merkle.io".parse().unwrap(),
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
        url: url::Url,
        ws_url: Option<url::Url>,
        max_consecutive: u32,
        min_time_delta: u128,
        ma_length: f64,
    ) -> Self {
        Self {
            name: sanitize_url(&url).unwrap_or(url.to_string()),
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

    /// Explicitly get the url of the Rpc, potentially dangerous as it can expose basic auth
    #[cfg(test)]
    pub fn get_url(&self) -> Url {
        self.url.clone()
    }

    /// Generic fn to send rpc
    pub async fn send_request(&self, tx: Value) -> Result<String, crate::rpc::types::RpcError> {
        tracing::debug!("Sending request: {}", tx.clone());

        let response = match self.client.post(self.url.clone()).json(&tx).send().await {
            Ok(response) => response,
            Err(err) => return Err(RpcError::InvalidResponse(err.to_string())),
        };

        let resp_text = response.text().await;
        tracing::debug!("response: {:?}", resp_text);

        resp_text.map_err(From::from)
    }

    /// Request blocknumber and return its value
    pub async fn block_number(&self) -> Result<u64, crate::rpc::types::RpcError> {
        let request = json!({
            "method": "eth_blockNumber".to_string(),
            "params": serde_json::Value::Null,
            "id": 1,
            "jsonrpc": "2.0".to_string(),
        });

        metrics::gauge!("rpc_requests_active", "method" => "eth_blockNumber").increment(1);
        metrics::counter!("rpc_requests_total", "method" => "eth_blockNumber").increment(1);

        let req_start = std::time::Instant::now();
        let number = self.send_request(request).await?;

        metrics::histogram!("rpc_response_time_secs", "method" => "eth_blockNumber")
            .record(req_start.elapsed().as_secs_f64());
        metrics::gauge!("rpc_requests_active", "method" => "eth_blockNumber").decrement(1);

        let return_number = extract_number(&number)?;

        Ok(return_number)
    }

    /// Returns the sync status. False if we're synced and following the head.
    pub async fn syncing(&self) -> Result<bool, crate::rpc::types::RpcError> {
        let request = json!({
            "method": "eth_syncing".to_string(),
            "params": serde_json::Value::Null,
            "id": 1,
            "jsonrpc": "2.0".to_string(),
        });

        metrics::gauge!("rpc_requests_active", "method" => "eth_syncing").increment(1);
        metrics::counter!("rpc_requests_total", "method" => "eth_syncing").increment(1);

        let req_start = std::time::Instant::now();
        let sync = self.send_request(request).await?;

        metrics::histogram!("rpc_response_time_secs", "method" => "eth_syncing")
            .record(req_start.elapsed().as_secs_f64());
        metrics::gauge!("rpc_requests_active", "method" => "eth_syncing").decrement(1);

        let status = extract_sync(&sync)?;

        Ok(status)
    }

    /// Get the latest finalized block
    pub async fn get_finalized_block(&self) -> Result<u64, crate::rpc::types::RpcError> {
        let request = json!({
            "method": "eth_getBlockByNumber".to_string(),
            "params": ["finalized", false],
            "id": 1,
            "jsonrpc": "2.0".to_string(),
        });

        metrics::gauge!("rpc_requests_active", "method" => "eth_getBlockByNumber").increment(1);
        metrics::counter!("rpc_requests_total", "method" => "eth_getBlockByNumber").increment(1);

        let req_start = std::time::Instant::now();
        let mut resp = self.send_request(request).await?;

        metrics::histogram!("rpc_response_time_secs", "method" => "eth_getBlockByNumber")
            .record(req_start.elapsed().as_secs_f64());
        metrics::gauge!("rpc_requests_active", "method" => "eth_getBlockByNumber").decrement(1);

        let number: Value = unsafe { simd_json::serde::from_str(&mut resp)? };
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

    /// Update the latency of the last n calls.
    /// We don't do it within send_request because we might kill it if it times out.
    pub fn update_latency(&mut self, latest: f64) {
        // If we have data >= to ma_length, remove the first one in line
        if self.status.latency_data.len() >= self.status.ma_length as usize {
            self.status.latency_data.remove(0);
        }

        // Update latency
        self.status.latency_data.push(latest);
        self.status.latency =
            self.status.latency_data.iter().sum::<f64>() / self.status.latency_data.len() as f64;
    }
}

/// Parses the result of `eth_syncing` and returns the status as a bool.
fn extract_sync(rx: &str) -> Result<bool, RpcError> {
    let mut rx = rx.to_string();

    let json: Value = unsafe { simd_json::serde::from_str(&mut rx)? };

    let result = &json["result"];

    if result.is_boolean() {
        result
            .as_bool()
            .ok_or_else(|| RpcError::InvalidResponse("Boolean expected".to_string()))
    } else if result.is_object() {
        Ok(true) // Syncing information present, thus syncing is in progress
    } else {
        Err(RpcError::InvalidResponse(
            "Unexpected result format".to_string(),
        ))
    }
}

/// Take in the result of `eth_getBlockByNumber`, and extract the block number
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use simd_json::serde::to_string;

    #[test]
    fn test_extract_sync_syncing() {
        let input = json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "startingBlock": "0x384",
                "currentBlock": "0x386",
                "highestBlock": "0x454"
            }
        });
        let input_str = to_string(&input).unwrap();
        let result = extract_sync(&input_str);
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn test_extract_sync_not_syncing() {
        let input = json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": false
        });
        let input_str = to_string(&input).unwrap();
        let result = extract_sync(&input_str);
        assert_eq!(result.unwrap(), false);
    }

    #[test]
    fn test_extract_sync_invalid() {
        let input = json!({
            "id": 1,
            "jsonrpc": "2.0",
            "wrong_key": {
                "startingBlock": "0x384",
                "currentBlock": "0x386",
                "highestBlock": "0x454"
            }
        });
        let input_str = to_string(&input).unwrap();
        let result = extract_sync(&input_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_number_success() {
        let input = json!({
            "result": "0x1b4"
        });
        let input_str = to_string(&input).unwrap();
        let result = extract_number(&input_str);
        assert_eq!(result.unwrap(), 436); // 0x1b4 in decimal
    }

    #[test]
    fn test_extract_number_failure() {
        let input = json!({
            "wrong_field": "0x1b4"
        });
        let input_str = to_string(&input).unwrap();
        let result = extract_number(&input_str);
        assert!(result.is_err());
    }
}
