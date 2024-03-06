use std::{
    convert::Infallible,
    sync::{
        Arc,
        RwLock,
    },
};

use http_body_util::Full;

use tokio::sync::{
    mpsc,
    oneshot,
};

use hyper::body::Bytes;

#[derive(PartialEq, Clone, Copy, Default)]
pub enum ReadinessState {
    #[default]
    Ready,
    Setup,
}

#[derive(PartialEq, Clone, Copy, Default)]
pub enum HealthState {
    #[default]
    Healthy, // Everything nominal
    MissingRpcs, // Some RPCs are not following the head but otherwise ok
    Unhealhy,    // Nothing works
}

#[derive(PartialEq, Clone, Copy, Default)]
pub struct LiveReady {
    readiness: ReadinessState,
    health: HealthState,
}

#[derive(PartialEq, Clone, Copy)]
pub struct LiveReadyUpdate {
    readiness: Option<ReadinessState>,
    health: Option<HealthState>,
}

// These 2 are used to send and receive updates related to the current
// health of blutgang.
pub type LiveReadyUpdateRecv = mpsc::Receiver<LiveReadyUpdate>;
pub type LiveReadyUpdateSnd = mpsc::Sender<LiveReadyUpdate>;

// These are used to request/return updates about health
pub type LiveReadyRecv = oneshot::Receiver<LiveReady>;
pub type LiveReadySnd = oneshot::Sender<LiveReady>;

pub type LiveReadyRequestRecv = mpsc::Receiver<LiveReadySnd>;
pub type LiveReadyRequestSnd = mpsc::Sender<LiveReadySnd>;

// Macros to make returning statuses less ugly in code
macro_rules! ok {
    () => {
        Ok(hyper::Response::builder()
            .status(200)
            .body(Full::new(Bytes::from("OK")))
            .unwrap())
    };
}

macro_rules! partial_ok {
    () => {
        Ok(hyper::Response::builder()
            .status(202)
            .body(Full::new(Bytes::from("RPC")))
            .unwrap())
    };
}

macro_rules! nok {
    () => {
        Ok(hyper::Response::builder()
            .status(503)
            .body(Full::new(Bytes::from("NOK")))
            .unwrap())
    };
}

// Listen for liveness update messages and update the current status accordingly
async fn liveness_listener(
    mut liveness_receiver: LiveReadyUpdateRecv,
    liveness_status: Arc<RwLock<LiveReady>>,
) {
    loop {
        while let Some(incoming) = liveness_receiver.recv().await {
            if incoming.readiness != None {
                liveness_status.write().unwrap().readiness = incoming.readiness.unwrap();
            }
            if incoming.health != None {
                liveness_status.write().unwrap().health = incoming.health.unwrap();
            }
        }
    }
}

// Receives requests about current status updates and returns the current liveness
async fn liveness_request_processor(
    mut liveness_request_receiver: LiveReadyRequestRecv,
    liveness_status: Arc<RwLock<LiveReady>>,
) {
    loop {
        while let Some(incoming) = liveness_request_receiver.recv().await {
            let current_status = liveness_status.read().unwrap().clone();
            incoming.send(current_status);
        }
    }
}

// Monitor for new liveness updates and update the statuses accordingly.
//
// Also handles incoming requests about the current status.
async fn liveness_monitor(
    liveness_receiver: LiveReadyUpdateRecv,
    liveness_request_receiver: LiveReadyRequestRecv,
) {
    let liveness_status = Arc::new(RwLock::new(LiveReady::default()));

    // Spawn thread for listening and updating the current liveness status
    let liveness_status_listener = liveness_status.clone();
    tokio::spawn(liveness_listener(
        liveness_receiver,
        liveness_status_listener,
    ));

    // Listens to incoming requests about the current liveness
    liveness_request_processor(liveness_request_receiver, liveness_status).await;
}

pub async fn accept_readiness_request(
    liveness_request_sender: LiveReadyRequestSnd,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    let (tx, rx) = oneshot::channel();

    liveness_request_sender.send(tx);

    let rax = match rx.await {
        Ok(v) => v,
        Err(_) => {
            return nok!();
        }
    };

    if rax.readiness == ReadinessState::Ready {
        return ok!();
    }

    nok!()
}

pub async fn accept_health_request(
    liveness_request_sender: LiveReadyRequestSnd,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    let (tx, rx) = oneshot::channel();

    liveness_request_sender.send(tx);

    let rax = match rx.await {
        Ok(v) => v,
        Err(_) => {
            return nok!();
        }
    };

    match rax.health {
        HealthState::Healthy => return ok!(),
        HealthState::MissingRpcs => return partial_ok!(),
        HealthState::Unhealhy => return nok!(),
    }
}
