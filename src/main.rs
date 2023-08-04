mod balancer;

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
        .get_matches();

    Ok(())
}
