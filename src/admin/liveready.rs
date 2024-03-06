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

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum ReadinessState {
    Ready,
    #[default]
    Setup,
}

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum HealthState {
    #[default]
    Healthy, // Everything nominal
    MissingRpcs, // Some RPCs are not following the head but otherwise ok
    Unhealhy,    // Nothing works
}

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub struct LiveReady {
    readiness: ReadinessState,
    health: HealthState,
}

#[derive(PartialEq, Clone, Copy)]
pub enum LiveReadyUpdate {
    Readiness(ReadinessState),
    Health(HealthState),
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
    while let Some(update) = liveness_receiver.recv().await {
        match update {
            LiveReadyUpdate::Readiness(state) => {
                let mut liveness = liveness_status.write().unwrap();
                liveness.readiness = state;
            }
            LiveReadyUpdate::Health(state) => {
                let mut liveness = liveness_status.write().unwrap();
                liveness.health = state;
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
            let current_status = *liveness_status.read().unwrap();
            let _ = incoming.send(current_status);
        }
    }
}

// Monitor for new liveness updates and update the statuses accordingly.
//
// Also handles incoming requests about the current status.
pub(in crate::r#admin) async fn liveness_monitor(
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

    let _ = liveness_request_sender.send(tx).await;

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

    let _ = liveness_request_sender.send(tx).await;

    let rax = match rx.await {
        Ok(v) => v,
        Err(_) => {
            return nok!();
        }
    };

    match rax.health {
        HealthState::Healthy => ok!(),
        HealthState::MissingRpcs => partial_ok!(),
        HealthState::Unhealhy => nok!(),
    }
}

// Just a sink used to immediately discard request in cases where admin is disabled
pub async fn liveness_update_sink(mut liveness_rx: LiveReadyUpdateRecv) {
    loop {
        while (liveness_rx.recv().await).is_some() {
            continue;
        }
    }
}

#[cfg(test)]
mod tests {
   use super::*;
   use tokio::sync::{mpsc, oneshot};

   #[tokio::test]
   async fn test_liveness_listener() {
       let (tx, rx) = mpsc::channel(100);
       let liveness_status = Arc::new(RwLock::new(LiveReady::default()));

       // Spawn the listener task
       let liveness_status_clone = liveness_status.clone();
       let listener_task = tokio::spawn(liveness_listener(rx, liveness_status_clone));

       // Send updates and verify the changes
       let mut updates = vec![
           LiveReadyUpdate::Readiness(ReadinessState::Ready),
           LiveReadyUpdate::Health(HealthState::MissingRpcs),
           LiveReadyUpdate::Health(HealthState::Unhealhy),
       ];

       for update in updates.drain(..) {
           tx.send(update).await.unwrap();
           let liveness = liveness_status.read().unwrap();
           match update {
               LiveReadyUpdate::Readiness(state) => assert_eq!(liveness.readiness, state),
               LiveReadyUpdate::Health(state) => assert_eq!(liveness.health, state),
           }
       }

       // Drop the sender to signal the listener task to stop
       drop(tx);
       listener_task.await.unwrap();
   }

   #[tokio::test]
   async fn test_liveness_request_processor() {
       let (tx, rx) = mpsc::channel(100);
       let liveness_status = Arc::new(RwLock::new(LiveReady::default()));

       // Spawn the request processor task
       let liveness_status_clone = liveness_status.clone();
       let processor_task = tokio::spawn(liveness_request_processor(rx, liveness_status_clone));

       // Send requests and verify the responses
       let (tx1, rx1) = oneshot::channel();
       tx.send(tx1).await.unwrap();
       let response = rx1.await.unwrap();
       assert_eq!(response, LiveReady::default());

       *liveness_status.write().unwrap() = LiveReady {
           readiness: ReadinessState::Ready,
           health: HealthState::Healthy,
       };

       let (tx2, rx2) = oneshot::channel();
       tx.send(tx2).await.unwrap();
       let response = rx2.await.unwrap();
       assert_eq!(
           response,
           LiveReady {
               readiness: ReadinessState::Ready,
               health: HealthState::Healthy
           }
       );

       // Drop the sender to signal the processor task to stop
       drop(tx);
       processor_task.await.unwrap();
   }

   #[tokio::test]
   async fn test_accept_readiness_request() {
       let (tx, rx) = mpsc::channel(100);

       // Ready state
       let liveness_status = Arc::new(RwLock::new(LiveReady {
           readiness: ReadinessState::Ready,
           health: HealthState::Healthy,
       }));

       let liveness_status_clone = liveness_status.clone();
       let request_processor_task =
           tokio::spawn(liveness_request_processor(rx, liveness_status_clone));

       let response = accept_readiness_request(tx.clone()).await.unwrap();
       assert_eq!(response.status(), 200);

       // Setup state
       *liveness_status.write().unwrap() = LiveReady {
           readiness: ReadinessState::Setup,
           health: HealthState::Healthy,
       };

       let response = accept_readiness_request(tx.clone()).await.unwrap();
       assert_eq!(response.status(), 503);

       drop(tx);
       request_processor_task.await.unwrap();
   }

   #[tokio::test]
   async fn test_accept_health_request() {
       let (tx, rx) = mpsc::channel(100);

       // Healthy state
       let liveness_status = Arc::new(RwLock::new(LiveReady {
           readiness: ReadinessState::Ready,
           health: HealthState::Healthy,
       }));

       let liveness_status_clone = liveness_status.clone();
       let request_processor_task =
           tokio::spawn(liveness_request_processor(rx, liveness_status_clone));

       let response = accept_health_request(tx.clone()).await.unwrap();
       assert_eq!(response.status(), 200);

       // MissingRpcs state
       *liveness_status.write().unwrap() = LiveReady {
           readiness: ReadinessState::Ready,
           health: HealthState::MissingRpcs,
       };

       let response = accept_health_request(tx.clone()).await.unwrap();
       assert_eq!(response.status(), 202);

       // Unhealhy state
       *liveness_status.write().unwrap() = LiveReady {
           readiness: ReadinessState::Ready,
           health: HealthState::Unhealhy,
       };

       let response = accept_health_request(tx.clone()).await.unwrap();
       assert_eq!(response.status(), 503);

       drop(tx);
       request_processor_task.await.unwrap();
   }
}
