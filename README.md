# Blutgang - the wd40 of alpha ethereum load balancers
![blutgang](https://github.com/rainshowerLabs/blutgang/assets/55022497/06fe1dd3-0bc4-4b5d-bfc8-5573d6f78db3)

[Join the discussion on our discord!](https://discord.gg/92TfQWdjEh)

**Note: Blutgang is alpha software. Unexpected behavior and bugs are to be expected. Please report any issues you encounter.**   

Blutgang is a blazing fast, caching, minimalistic load balancer designed with Ethereum's json-rpc in mind.

Blutgang's main advantage compared to other load balancers is the speed of its cache. Historical RPC querries are cached in a local database, bypassing the need for slow, repeating calls to your node.

## Load Balancing

Blutgang is designed to be used in front of a cluster of Ethereum RPC nodes, distributing the load between them. By default, the load is balanced in a weighted round-robin configuration. Thanks to its modularity, the load balancing algorithm can be customized to support any load balancing schema.

### Local usage

If you are a developer or a power user, you can use blutgang as an intermediary between you and your RPC endpoint to cache historical querries. This speeds up usage dramatically, as well as increasing privacy.

## How to run 

For detailed instructions on how to use blutgang, please read the [wiki]().

### Tldr

Clone the repository, and fint the `example_config.toml` file. Edit it to your liking, and run `cargo run --release -- -c example_config.toml`.   

If you want to use command line arguments instead, please run `cargo run --release -- --help` for more info. Keep in mind that the recommended way to run blutgang is via a config file.

## Benchmarks
*Benchmarks were performed with a Ryzen 7 2700X, NVME SSD, and default Ubuntu 23.04 kernel. Same RPC endpoints were used*
```bash
time sothis --source_rpc http://localhost:3000 --mode call_track --contract_address 0x1c479675ad559DC151F6Ec7ed3FbF8ceE79582B6 --origin_block 17885300 --terminal_block 17892269 --calldata 0x06f13056 --query_interval 20
```
![Figure_1](https://github.com/rainshowerLabs/blutgang/assets/55022497/9d6727de-7001-464d-8d48-b7659c2644c8)
![Figure_2](https://github.com/rainshowerLabs/blutgang/assets/55022497/1e631a01-2772-49dc-af1f-8609b504544f)

## Notes on the license

Blutgang is libre software licensed under the AGPL-3.0 license. In practice, this means that if you use blutgang for non-personal use, you are required by law to disclose the source code of blutgang, or any works based on or that use the licensed software in any way. This includes MEV bots. Blutgang respects the freedom of it's users will *always* be free for non-commerical use.   

### Commercial use and support

If you want to use blutgang in a comercial setting without disclosing any source code, please [reach out](https://rainshower.cloud/).
