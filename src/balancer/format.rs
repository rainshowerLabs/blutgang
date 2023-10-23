use http_body_util::BodyExt;
use hyper::{
    body::Incoming,
    Request,
};
use memchr::memmem;
use regex::Regex;
use simd_json;
use serde_json::Value;
use std::str::from_utf8;

use crate::rpc::error::RpcError;

struct BlocknumIndex<'a> {
    pub method: &'a [u8],
    pub position: usize,
}

// Return the blocknumber from a json-rpc request as a Option<String>, returning None if it cant find anything
pub fn get_block_number_from_request(tx: Value) -> Option<u64> {
    // If the `params` field does not exist return None
    if !tx["params"].is_array() {
        return None;
    }

    // Return None immediately if params == 0
    if tx["params"].as_array().unwrap().is_empty() {
        return None;
    }

    // The JSON-RPC standard is all over the place so depending on the method, we need to look at
    // different param indexes. Why? Has i ever???
    let methods = [
        BlocknumIndex {
            method: b"eth_getBalance",
            position: 1,
        },
        BlocknumIndex {
            method: b"eth_getStorageAt",
            position: 2,
        },
        BlocknumIndex {
            method: b"eth_getTransactionCount",
            position: 1,
        },
        BlocknumIndex {
            method: b"eth_getBlockTransactionCountByNumber",
            position: 0,
        },
        BlocknumIndex {
            method: b"eth_getUncleCountByBlockNumber",
            position: 0,
        },
        BlocknumIndex {
            method: b"eth_getCode",
            position: 1,
        },
        BlocknumIndex {
            method: b"eth_call",
            position: 1,
        },
        BlocknumIndex {
            method: b"eth_getBlockByNumber",
            position: 0,
        },
        BlocknumIndex {
            method: b"eth_getTransactionByBlockNumberAndIndex",
            position: 0,
        },
        BlocknumIndex {
            method: b"eth_getUncleByBlockNumberAndIndex",
            position: 0,
        },
    ];

    // Iterate through the array and return the position, return None if not present
    for item in methods.iter() {
        if memmem::find(tx["method"].to_string().as_bytes(), item.method).is_some() {
            let block_number = tx["params"][item.position].to_string().replace('\"', "");

            // If `null` return None
            if block_number == "null" {
                return None;
            }

            // Convert to decimal
            let block_number = match u64::from_str_radix(&block_number[2..], 16) {
                Ok(block_number) => block_number,
                Err(_) => return None,
            };

            return Some(block_number);
        }
    }

    None
}

pub async fn incoming_to_value(tx: Request<Incoming>) -> Result<Value, Box<dyn std::error::Error>> {
    let tx = tx.collect().await?.to_bytes().clone();
    let mut tx = from_utf8(&tx).unwrap().to_owned();
    let ret: Value;
    unsafe {
        ret = simd_json::serde::from_str(&mut tx).unwrap();
    }
    Ok(ret)
}

pub fn _extract_id(request: &str) -> Option<String> {
    // Define the regular expression pattern to capture the "id" field
    let re = Regex::new(r#""id"\s*:\s*("([^"]*)"|(\d+))"#).unwrap();

    if let Some(captures) = re.captures(request) {
        if let Some(id) = captures.get(2).or(captures.get(3)) {
            return Some(id.as_str().to_string());
        }
    }

    None
}

pub fn _replace_id(tx: &str, id: &str) -> Result<String, RpcError> {
    // Use regex to find and capture the "id" field with optional double quotes
    //
    // We could be using memmem and making it much more optimized but this is fine
    let re = match Regex::new(r#"("id"\s*:\s*)("[^"]*"|\d+)"#) {
        Ok(re) => re,
        Err(err) => return Err(RpcError::InvalidResponse(err.to_string())),
    };

    // Replace "id" with the new id using the captured group
    let replaced = re.replace(tx, |caps: &regex::Captures| format!("{}{}", &caps[1], id));

    Ok(replaced.to_string())
}

#[test]
fn replace_id_test() {
    let tx = r#"{"id":1,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
    let tx = _replace_id(tx, "2").unwrap();
    assert_eq!(
        tx,
        r#"{"id":2,"jsonrpc":"2.0","method":"eth_call","params":...}"#
    );

    let tx = r#"{"jsonrpc":"2.0","method":"eth_call", "id":1, "params":...}"#;
    let tx = _replace_id(tx, "2").unwrap();
    assert_eq!(
        tx,
        r#"{"jsonrpc":"2.0","method":"eth_call", "id":2, "params":...}"#
    );

    let tx = r#"{"jsonrpc":"2.0","method":"eth_call", "id": "1", "params":...}"#;
    let tx = _replace_id(tx, "2").unwrap();
    assert_eq!(
        tx,
        r#"{"jsonrpc":"2.0","method":"eth_call", "id": 2, "params":...}"#
    );

    let tx = r#"{"jsonrpc":"2.0","method":"eth_call", "id": 1, "params":...}"#;
    let tx = _replace_id(tx, "2").unwrap();
    assert_eq!(
        tx,
        r#"{"jsonrpc":"2.0","method":"eth_call", "id": 2, "params":...}"#
    );
}

#[test]
fn extract_id_test() {
    let tx = r#"{"id":1,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
    let tx = _extract_id(tx).unwrap();
    assert_eq!(tx, r#"1"#);

    let tx = r#"{"id": 1,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
    let tx = _extract_id(tx).unwrap();
    assert_eq!(tx, r#"1"#);

    let tx = r#"{"id":1 ,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
    let tx = _extract_id(tx).unwrap();
    assert_eq!(tx, r#"1"#);

    let tx = r#"{"id":"1","jsonrpc":"2.0","method":"eth_call","params":...}"#;
    let tx = _extract_id(tx).unwrap();
    assert_eq!(tx, r#"1"#);

    let tx = r#"{"id":"1","jsonrpc":"2.0","method":"eth_call","params":...}"#;
    let tx = _extract_id(tx).unwrap();
    assert_eq!(tx, r#"1"#);

    let tx = r#"{,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
    let tx = _extract_id(tx);
    assert_eq!(tx, None);
}

#[cfg(test)]
mod tests {
    use crate::balancer::format::get_block_number_from_request;
    use serde_json::json;

    #[test]
    fn get_block_number_from_request_test() {
        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBalance",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"]
        });

        assert_eq!(get_block_number_from_request(request), None);

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBalance",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x1"]
        });

        assert_eq!(get_block_number_from_request(request), Some(1));

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBalance",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1"]
        });

        assert_eq!(get_block_number_from_request(request), None);

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getStorageAt",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x0", "latest"]
        });

        assert_eq!(get_block_number_from_request(request), None);

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getStorageAt",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x0", "0x1"]
        });

        assert_eq!(get_block_number_from_request(request).unwrap(), 1);

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getStorageAt",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x0"]
        });

        assert_eq!(get_block_number_from_request(request), None);

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getTransactionCount",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"]
        });

        assert_eq!(get_block_number_from_request(request), None);

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getTransactionCount",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x1"]
        });

        assert_eq!(get_block_number_from_request(request).unwrap(), 1);

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getTransactionCount",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1"]
        });

        assert_eq!(get_block_number_from_request(request), None);

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBlockTransactionCountByNumber",
            "params":["latest"]
        });

        assert_eq!(get_block_number_from_request(request), None);

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBlockTransactionCountByNumber",
            "params":["0x1"]
        });

        assert_eq!(get_block_number_from_request(request).unwrap(), 1);

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBlockTransactionCountByNumber",
            "params":[]
        });

        assert_eq!(get_block_number_from_request(request), None);
    }
}
