# Blutgang - the wd40 of alpha ethereum load balancers
![blutgang](https://github.com/rainshowerLabs/blutgang/assets/55022497/06fe1dd3-0bc4-4b5d-bfc8-5573d6f78db3)

Join the discussion on our [discord](https://discord.gg/92TfQWdjEh), [telegram](https://t.me/rainshower), or [matrix!](https://matrix.to/#/%23rainshower:matrix.org)

**Note: Blutgang is alpha software. Unexpected behavior and bugs are to be expected. Please report any issues you encounter.**   

Blutgang is a blazing fast, caching, minimalistic load balancer designed with Ethereum's json-rpc in mind.

Blutgang's main advantage compared to other load balancers is the speed of its cache. Historical RPC queries are cached in a local database, bypassing the need for slow, repeating calls to your node.

## Load Balancing

Blutgang is designed to be used in front of a cluster of Ethereum RPC nodes, distributing the load between them. By default, the load is balanced in a weighted round-robin configuration. Thanks to its modularity, the load balancing algorithm can be customized to support any load balancing schema.

### Local usage

If you are a developer or a power user, you can use blutgang as an intermediary between you and your RPC endpoint to cache historical queries. This speeds up usage dramatically, as well as increasing privacy.

## How to run 

For detailed instructions on how to use blutgang, please read the [wiki]().

### Tldr

Clone the repository, and find the `example_config.toml` file. Edit it to your liking, and run `cargo run --release -- -c example_config.toml`.   

If you want to use command line arguments instead, please run `cargo run --release -- --help` for more info. Keep in mind that the recommended way to run blutgang is via a config file.

### Docker

*Note: The overhead of running blutgang inside of docker reduces cache read performance by 10-20%.*   

The official docker image is available on [dockerhub](https://hub.docker.com/r/makemake1337/blutgang).  
You must provide a config file to the docker container, as well as expose the port specified. Example:   
```bash
docker run -v /full/path/to/config.toml:/app/config.toml --network host makemake1337/blutgang
```

## Benchmarks
*Benchmarks were performed with a Ryzen 7 2700X, NVME SSD, and default Ubuntu 23.04 kernel. Same RPC endpoints were used*
```bash
time sothis --source_rpc http://localhost:3000 --mode call_track --contract_address 0x1c479675ad559DC151F6Ec7ed3FbF8ceE79582B6 --origin_block 17885300 --terminal_block 17892269 --calldata 0x06f13056 --query_interval 20
```
![Figure_1](https://github.com/rainshowerLabs/blutgang/assets/55022497/8ce9a690-d2eb-4910-9a5d-807c2bdd4649)
![Figure_2](https://github.com/rainshowerLabs/blutgang/assets/55022497/50d78e5f-2209-488d-82fc-8018388a82e7)

## Notes on the license

Blutgang is libre software licensed under the AGPL-3.0 license. If you are using blutgang in a commercial enviroment, and do not get a commercial license from us, you are required by law to disclose the source code of blutgang, or any works based on or that use the licensed software in any way. This includes MEV bots. Blutgang respects the freedom of it's users and will *always* be free for non-commercial use.   

### Commercial use and support

If you want to use blutgang in a commercial setting without disclosing any source code, please [reach out](https://rainshower.cloud/).
