mod balancer;
mod rpc;

use crate::{
    rpc::types::Rpc,
    balancer::balancer::forward,
};
use std::convert::Infallible;
use std::net::SocketAddr;

use http_body_util::Full;
use hyper::{
    Request,
    Response,
    body::Bytes,
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use clap::{Command, Arg};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("blutgang")
        .version("0.1.0")
        .author("makemake <vukasin@gostovic.me>")
        .about("Tool for replaying historical transactions. Designed to be used with anvil or hardhat.")
        .arg(Arg::new("rpc_list")
            .long("rpc_list")
            .short('r')
            .num_args(1..)
            .default_value("")
            .help("CSV list of rpcs"))
        .arg(Arg::new("port")
            .long("port")
            .short('p')
            .num_args(1..)
            .default_value("3000")
            .help("port to listen to"))
    .get_matches();

    let rpc_list: String = matches.get_one::<String>("rpc_list").expect("Invalid rpc_list").to_string();
    // turn the rpc_list into a csv vec
    let rpc_list: Vec<&str> = rpc_list.split(",").collect();
    let rpc_list: Vec<String> = rpc_list.iter().map(|rpc| rpc.to_string()).collect();
    // Make a list of Rpc structs
    let rpc_list: Vec<Rpc> = rpc_list.iter().map(|rpc| Rpc::new(rpc.to_string())).collect();

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(addr).await?;

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(io, service_fn(move |req| forward(req,rpc_list)))
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}
