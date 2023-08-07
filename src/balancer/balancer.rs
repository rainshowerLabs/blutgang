use hyper::Request;
use reqwest::Response;
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
) -> Result<Response, hyper::Error> {
    
	println!("Forwarding to: {}", rpc.url);
    Ok(rpc.send_request(tx).await.unwrap())
}
