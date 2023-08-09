use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::Request;
use serde_json::Value;
use std::str::from_utf8;

pub async fn incoming_to_value(tx: Request<Incoming>) -> Result<Value, Box<dyn std::error::Error>> {
    let tx = tx.collect().await?.to_bytes().clone();
    let tx = from_utf8(&tx).unwrap().clone();
    let tx: Value = serde_json::from_str(tx).unwrap();
    Ok(tx)
}
