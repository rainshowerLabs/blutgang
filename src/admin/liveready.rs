use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::Arc,
};

use crate::log_info;

use http_body_util::Full;
use hyper_util_blutgang::rt::TokioIo;
use tokio::{
    net::TcpListener,
    sync::watch,
};

use hyper::{
    body::Bytes,
    server::conn::http1,
    service::service_fn,
    Request,
};

#[derive(Debug, PartialEq)]
enum ReadinessState {
    Ready,
    Setup,
}

#[macro_use]
macro_rules! readiness {
    (
        $io:expr,
        $readiness_rx:expr,
    ) => {
        // Bind the incoming connection to our service
        if let Err(err) = http1::Builder::new()
            // `service_fn` converts our function in a `Service`
            .serve_connection(
                $io,
                service_fn(|req| {
                    let response = accept_readiness_request(req, Arc::clone($readiness_rx));
                    response
                }),
            )
            .await
        {
            println!("error serving admin connection: {:?}", err);
        }
    };
}

pub async fn accept_readiness_request(
    tx: Request<hyper::body::Incoming>,
    readiness_rx: Arc<watch::Receiver<ReadinessState>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // if tx.uri().path() != "/ready" {
    //     return Ok(hyper::Response::builder()
    //         .status(404)
    //         .body(Full::new(Bytes::from("Not found")))
    //         .unwrap());
    // }

    if *readiness_rx.borrow() == ReadinessState::Ready {
        Ok(hyper::Response::builder()
            .status(200)
            .body(Full::new(Bytes::from("OK")))
            .unwrap())
    } else {
        Ok(hyper::Response::builder()
            .status(503)
            .body(Full::new(Bytes::from("NOK")))
            .unwrap())
    }
}
