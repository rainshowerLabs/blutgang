use std::convert::Infallible;

use tokio::net::TcpListener;

use http_body_util::Full;
use hyper::{
    body::Bytes,
    server::conn::http1,
    service::service_fn,
};
use hyper_util_blutgang::rt::TokioIo;

macro_rules! accept_health {
    ($io:expr) => {
        // Bind the incoming connection to our service
        // Just return 200 OK for everything here, no need for anything more

        if let Err(err) = http1::Builder::new()
            // `service_fn` converts our function in a `Service`
            .serve_connection(
                $io,
                service_fn(|_| {
                    let response = async {
                        let body = Full::new(Bytes::from("OK"));

                        Ok::<_, Infallible>(
                            hyper::Response::builder().status(200).body(body).unwrap(),
                        )
                    };
                    response
                }),
            )
            .await
        {
            println!("\x1b[31mErr:\x1b[0m Error serving connection: {:?}", err);
        }
    };
}

// Bind to address at /health and respond to everythong with ok 200
pub async fn listen_for_health(address: TcpListener) -> Result<(), Box<dyn std::error::Error>> {
    // Bind to the address/health
    //
    // the address arg is used for the load balancer so we need to modify it
    // to bind to /health
    let address = address.local_addr()?;
    let address = format!("{}{}", address, "/health");

    // Create a listener and bind to it
    let listener = TcpListener::bind(address.clone()).await?;
    println!("\x1b[35mInfo:\x1b[0m Bound health to: {}", address);

    // Accept all incoming connections
    loop {
        let (stream, socketaddr) = listener.accept().await?;
        println!("\x1b[35mInfo:\x1b[0m Connection from: {}", socketaddr);

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            accept_health!(io);
        });
    }
}
