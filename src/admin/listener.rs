use crate::admin::accept::accept_admin_request;
use tokio::net::TcpListener;
use hyper::{
    server::conn::http1,
    service::service_fn,
};
use hyper_util_blutgang::rt::TokioIo;

macro_rules! accept_admin {
    (
        $io:expr,
    ) => {
        // Bind the incoming connection to our service
        if let Err(err) = http1::Builder::new()
            // `service_fn` converts our function in a `Service`
            .serve_connection(
                $io,
                service_fn(|req| {
                    let response = accept_admin_request(
                        req,
                    );
                    response
                }),
            )
            .await
        {
            println!("error serving admin connection: {:?}", err);
        }
    };
}

pub async fn listen_for_admin_requests() -> Result<(), Box<dyn std::error::Error>>{
    // Create a listener and bind to it
    // TODO: dont hardcode
    let listener = TcpListener::bind("127.0.0.1:5975").await?;

    loop {
        let (stream, socketaddr) = listener.accept().await?;
        println!("\x1b[35mInfo:\x1b[0m Admin connection from: {}", socketaddr);

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            accept_admin!(
                io,
            );
        });
    }
}

