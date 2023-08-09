use crate::balancer::format::incoming_to_Value;
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
	let tx = incoming_to_Value(tx).await.unwrap();

	// Convert tx which is hyper::Request to a reqwest::Response

	let rx: reqwest::Response = rpc.send_request(tx).await.unwrap();
	println!("Response: {:?}", rx);

	// Convert rx which is reqwest::Response to a hyper::Response
	let body = hyper::body::to_bytes(rx.into_body()).await.unwrap();
    let res = hyper::Response::builder()
        .status(200)
        .body(body)
        .unwrap();
	Ok(res)


    // Ok(hyper::Response::new(Full::new(Bytes::new())))
}
