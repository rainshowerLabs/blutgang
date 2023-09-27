use memchr::memmem;
use serde_json::{
    Error,
    Value,
};

struct RequestPos<'a> {
    pub method: &'a [u8],
    pub position: Option<usize>,
}

// Return the blocknumber from a json-rpc request as a Option<String>, returning None if it cant find anything
fn get_block_number_from_request(tx: Value) -> Result<Option<String>, Error> {
    // Return None immediately if params == 0
    if tx["params"].as_array().unwrap().len() == 0 {
        return Ok(None);
    }

    // The JSON-RPC standard is all over the place so depending on the method, we need to look at
    // different param indexes. Why? Has i ever???
    let methods = [
        RequestPos {
            method: b"eth_getBalance",
            position: Some(1),
        },
        RequestPos {
            method: b"eth_getStorageAt",
            position: Some(2),
        },
        RequestPos {
            method: b"eth_getTransactionCount",
            position: Some(1),
        },
        RequestPos {
            method: b"eth_getBlockTransactionCountByNumber",
            position: Some(0),
        },
        RequestPos {
            method: b"eth_getUncleCountByBlockNumber",
            position: Some(0),
        },
        RequestPos {
            method: b"eth_getCode",
            position: Some(1),
        },
        RequestPos {
            method: b"eth_call",
            position: Some(1),
        },
        RequestPos {
            method: b"eth_getBlockByNumber",
            position: Some(0),
        },
        RequestPos {
            method: b"eth_getTransactionByBlockNumberAndIndex",
            position: Some(0),
        },
        RequestPos {
            method: b"eth_getUncleByBlockNumberAndIndex",
            position: Some(0),
        },
    ];

    // Iterate through the array and return the position, return None if not present
    for item in methods.iter() {
        if memmem::find(tx["method"].to_string().as_bytes(), item.method).is_some() {
            let pos = item.position.unwrap();
            let block_number = tx["params"][pos].to_string();
            return Ok(Some(block_number));
        }
    }

    Ok(None)
}

// Tests
#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

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

        assert_eq!(
            get_block_number_from_request(request).unwrap(),
            None
        );

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

        assert_eq!(
            get_block_number_from_request(request).unwrap(),
            None
        );

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

        assert_eq!(
            get_block_number_from_request(request).unwrap(),
            None
        );

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

        assert_eq!(
            get_block_number_from_request(request).unwrap(),
            None
        );
    }
}

