use std::convert::Infallible;
use http_body_util::Full;
use hyper::{
	Request,
	Response,
	body::Bytes,
};

use crate::{
	rpc::{
		types::{
			Rpc,
		},
	},
};

// TODO: Since we're not ranking RPCs properly, just pick the next one in line for now
fn pick(
	list: Vec<Rpc>,
	last: i32,
) -> i32 {
	last+1
}

pub async fn forward(
	tx: Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    
    Ok(Response::new(Full::new(Bytes::from("Hello, World!"))))
}

