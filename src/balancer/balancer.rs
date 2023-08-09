use crate::balancer::format::incoming_to_value;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::Request;

use std::convert::Infallible;
use crate::rpc::types::Rpc;

// TODO: Since we're not ranking RPCs properly, just pick the next one in line for now
pub fn pick(
	list: &Vec<Rpc>,
	last: usize,
) -> (Rpc, usize) {
	println!("{:?}", last);
	println!("{:?}", list.len());
	let now = last + 1;
	if now >= list.len() {
		return (list[last].clone(), 0)
	}
	(list[last].clone(), now)
}

pub async fn forward(
	tx: Request<hyper::body::Incoming>,
	rpc: Rpc,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    
	println!("Forwarding to: {}", rpc.url);
	// Convert incoming body to serde value
	let tx = incoming_to_value(tx).await.unwrap();
	let rx: reqwest::Response = rpc.send_request(tx).await.unwrap();

	// Convert rx to bytes and but it in a Buf
	let body = hyper::body::Bytes::from(rx.bytes().await.unwrap());

	// Put it in a http_body_util::Full
	let body = Full::new(body);

	//Build the response
    let res = hyper::Response::builder()
        .status(200)
        .body(body)
        .unwrap();
	println!("Response: {:?}", res);

	Ok(res)
}
