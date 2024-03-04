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

#[derive(PartialEq)]
enum ReadinessState {
    Ready,
    Setup,
}

#[derive(PartialEq, Clone, Copy)]
enum HealthState {
    Healthy, // Everything nominal
    MissingRpcs, // Some RPCs are not following the head but otherwise ok
    Unhealhy, // Nothing works
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
    readiness_rx: Arc<watch::Receiver<ReadinessState>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
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

pub async fn accept_health_request(
    health_endpoint_rx: Arc<watch::Receiver<HealthState>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    let state = *health_endpoint_rx.borrow();

    match state {
        HealthState::Healthy => {
            return Ok(hyper::Response::builder()
                .status(200)
                .body(Full::new(Bytes::from("OK")))
                .unwrap());
        },
        // todo: ermmmmm?
        HealthState::MissingRpcs => {
            return Ok(hyper::Response::builder()
                .status(200)
                .body(Full::new(Bytes::from("NOK")))
                .unwrap());
        },
        _ => {
            return Ok(hyper::Response::builder()
                .status(503)
                .body(Full::new(Bytes::from("NOK")))
                .unwrap());
        }
    }
}
