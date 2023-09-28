use crate::balancer::cache_helper::get::get_block_number_from_request;
use serde_json::json;

#[test]
fn get_block_number_from_request_test() {
    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getBalance",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "latest"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getBalance",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x1"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "0x1"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getBalance",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1"]
    });

    assert_eq!(get_block_number_from_request(request).unwrap(), None);

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getStorageAt",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x0", "latest"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "latest"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getStorageAt",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x0", "0x1"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "0x1"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getStorageAt",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x0"]
    });

    assert_eq!(get_block_number_from_request(request).unwrap(), None);

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getTransactionCount",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "latest"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getTransactionCount",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x1"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "0x1"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getTransactionCount",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1"]
    });

    assert_eq!(get_block_number_from_request(request).unwrap(), None);

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getBlockTransactionCountByNumber",
        "params":["latest"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "latest"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getBlockTransactionCountByNumber",
        "params":["0x1"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "0x1"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getBlockTransactionCountByNumber",
        "params":[]
    });

    assert_eq!(get_block_number_from_request(request).unwrap(), None);
}
