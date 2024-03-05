use std::{
    convert::Infallible,
    sync::Arc,
};

use http_body_util::Full;

use tokio::{
    sync::mpsc,
};

use hyper::{
    body::Bytes,
};

#[derive(PartialEq, Clone, Copy)]
pub enum ReadinessState {
    Ready,
    Setup,
}

#[derive(PartialEq, Clone, Copy)]
pub enum HealthState {
    Healthy, // Everything nominal
    MissingRpcs, // Some RPCs are not following the head but otherwise ok
    Unhealhy, // Nothing works
}

#[derive(PartialEq, Clone, Copy)]
pub struct LiveReady {
    readiness: ReadinessState,
    health: HealthState,
}

pub type LivenessReceiver = mpsc::Receiver<LiveReady>;
pub type LivenessSender = mpsc::Sender<LiveReady>;

pub async fn accept_readiness_request(
    liveness_receiver: Arc<LivenessReceiver>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    let readiness = liveness_receiver.recv();
    if readiness.readiness == ReadinessState::Ready {
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
    liveness_receiver: Arc<LivenessReceiver>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    let health = *liveness_receiver.borrow();
    let health = health.health; // schizo code

    match health {
        HealthState::Healthy => {
            return Ok(hyper::Response::builder()
                .status(200)
                .body(Full::new(Bytes::from("OK")))
                .unwrap());
        },
        // todo: ermmmmm?
        HealthState::MissingRpcs => {
            return Ok(hyper::Response::builder()
                .status(202)
                .body(Full::new(Bytes::from("RPC")))
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

async fn update_liveness(
    liveness_receiver: Arc<LivenessReceiver>,
    liveness_status: Arc<RwLock<>>
) {
    loop {
         while let Some(incoming) = liveness_receiver.recv().await {
            incoming
        }
    }
}
