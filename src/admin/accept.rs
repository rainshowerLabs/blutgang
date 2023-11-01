use std::convert::Infallible;
use http_body_util::Full;
use hyper::{
    body::Bytes,
    Request,
};

pub async fn accept_admin_request(
    tx: Request<hyper::body::Incoming>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
	let response: Result<hyper::Response<Full<Bytes>>, Infallible>;
    // Convert rx to bytes and but it in a Buf
    let body = hyper::body::Bytes::from("A");

    // Put it in a http_body_util::Full
    let body = Full::new(body);
	response = Ok(hyper::Response::builder().status(200).body(body).unwrap());
	response
}