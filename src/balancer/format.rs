use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::Request;
use regex::Regex;
use serde_json::Value;
use std::str::from_utf8;

use crate::rpc::error::RpcError;

pub async fn incoming_to_value(tx: Request<Incoming>) -> Result<Value, Box<dyn std::error::Error>> {
    let tx = tx.collect().await?.to_bytes().clone();
    let tx = from_utf8(&tx).unwrap().clone();
    let tx: Value = serde_json::from_str(tx).unwrap_or(tx.into());
    Ok(tx)
}

pub fn extract_id(request: &str) -> Option<String> {
    // Define the regular expression pattern to capture the "id" field
    let re = Regex::new(r#""id"\s*:\s*("([^"]*)"|(\d+))"#).unwrap();

    if let Some(captures) = re.captures(request) {
        if let Some(id) = captures.get(2).or(captures.get(3)) {
            return Some(id.as_str().to_string());
        }
    }

    None
}

pub fn replace_id(tx: &str, id: &str) -> Result<String, RpcError> {
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
    let tx = replace_id(tx, "2").unwrap();
    assert_eq!(
        tx,
        r#"{"id":2,"jsonrpc":"2.0","method":"eth_call","params":...}"#
    );

    let tx = r#"{"jsonrpc":"2.0","method":"eth_call", "id":1, "params":...}"#;
    let tx = replace_id(tx, "2").unwrap();
    assert_eq!(
        tx,
        r#"{"jsonrpc":"2.0","method":"eth_call", "id":2, "params":...}"#
    );

    let tx = r#"{"jsonrpc":"2.0","method":"eth_call", "id": "1", "params":...}"#;
    let tx = replace_id(tx, "2").unwrap();
    assert_eq!(
        tx,
        r#"{"jsonrpc":"2.0","method":"eth_call", "id": 2, "params":...}"#
    );

    let tx = r#"{"jsonrpc":"2.0","method":"eth_call", "id": 1, "params":...}"#;
    let tx = replace_id(tx, "2").unwrap();
    assert_eq!(
        tx,
        r#"{"jsonrpc":"2.0","method":"eth_call", "id": 2, "params":...}"#
    );
}

#[test]
fn extract_id_test() {
    let tx = r#"{"id":1,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
    let tx = extract_id(tx).unwrap();
    assert_eq!(tx, r#"1"#);

    let tx = r#"{"id": 1,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
    let tx = extract_id(tx).unwrap();
    assert_eq!(tx, r#"1"#);

    let tx = r#"{"id":1 ,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
    let tx = extract_id(tx).unwrap();
    assert_eq!(tx, r#"1"#);

    let tx = r#"{"id":"1","jsonrpc":"2.0","method":"eth_call","params":...}"#;
    let tx = extract_id(tx).unwrap();
    assert_eq!(tx, r#"1"#);

    let tx = r#"{"id":"1","jsonrpc":"2.0","method":"eth_call","params":...}"#;
    let tx = extract_id(tx).unwrap();
    assert_eq!(tx, r#"1"#);

    let tx = r#"{,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
    let tx = extract_id(tx);
    assert_eq!(tx, None);
}
