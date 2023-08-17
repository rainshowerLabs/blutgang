# Blutgang - the wd40 of alpha ethereum load balancers
![blutgang](https://github.com/rainshowerLabs/blutgang/assets/55022497/06fe1dd3-0bc4-4b5d-bfc8-5573d6f78db3)

[Join the discussion on our discord!](https://discord.gg/92TfQWdjEh)

**Note: Blutgang is alpha software. Unexpected behavior and bugs are to be expected. Please report any issues you encounter.**   

Blutgang is a blazing fast, caching, minimalistic load balancer designed with Ethereum's json-rpc in mind.

Blutgang's main advantage compared to other load balancers is the speed of its cache. Historical RPC querries are cached in a local database, bypassing the need for slow, repeating calls to your node.

## Load Balancing

Blutgang is designed to be used in front of a cluster of Ethereum RPC nodes, distributing the load between them. By default, the load is balanced in a weighted round-robin configuration. Thanks to its modularity, the load balancing algorithm can be customized to support any load balancing schema.

It's lightweight, embedded design also allows it to be used with just one node. This way, the cache can be utilized to speed up the access of historic data. This can be very useful for developers and power users, especially if using remote nodes.

## Benchmarks

*tba*

## Notes on the license

Blutgang is libre software licensed under the AGPL-3.0 license. In practice, this means that if you use blutgang for non-personal use, you are required by law to disclose the source code of blutgang, or any works based on the licensed software. Blutgang respects the freedom of it's users will *always* be free for non-commerical use.   

### Commercial use and support

If you want to use blutgang in a comercial setting without disclosing any source code, please [reach out](https://rainshower.cloud/).
