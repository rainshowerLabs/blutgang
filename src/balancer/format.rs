use crate::NamedBlocknumbers;
use http_body_util::BodyExt;
use hyper::{
    body::Incoming,
    Request,
};
use memchr::memmem;
use serde_json::{
    json,
    Value,
    Value::Null,
};
use simd_json::serde::from_str;
use std::{
    str::from_utf8,
    sync::{
        Arc,
        RwLock,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NamedNumber {
    Latest,
    Earliest,
    Safe,
    Finalized,
    Pending,
    Null,
}

// Returns the corresponding NamedNumber enum value for the named number
// Null if n/a
fn has_named_number(param: &str) -> NamedNumber {
    let named_list = ["latest", "earliest", "safe", "finalized", "pending"];

    for (index, item) in named_list.iter().enumerate() {
        if memmem::find(param.as_bytes(), item.as_bytes()).is_some() {
            match index {
                0 => return NamedNumber::Latest,
                1 => return NamedNumber::Earliest,
                2 => return NamedNumber::Safe,
                3 => return NamedNumber::Finalized,
                4 => return NamedNumber::Pending,
                _ => NamedNumber::Null,
            };
        }
    }

    NamedNumber::Null
}

// Return the blocknumber from a json-rpc request as a Option<String>, returning None if it cant find anything
pub fn get_block_number_from_request(
    tx: Value,
    named_blocknumbers: &Arc<RwLock<NamedBlocknumbers>>,
) -> Option<u64> {
    // Return none if `params` is not a thing
    let params = tx["params"].as_array();
    if let Some(params) = params {
        if params.is_empty() {
            return None;
        }
    } else {
        return None;
    }

    // The JSON-RPC standard is all over the place so depending on the method, we need to look at
    // different param indexes. Why? Has i ever???
    let position = match tx["method"].as_str() {
        Some("eth_getBalance") => 1,
        Some("eth_getStorageAt") => 2,
        Some("eth_getTransactionCount") => 1,
        Some("eth_getBlockTransactionCountByNumber") => 0,
        Some("eth_getUncleCountByBlockNumber") => 0,
        Some("eth_getCode") => 1,
        Some("eth_call") => 1,
        Some("eth_getBlockByNumber") => 0,
        Some("eth_getTransactionByBlockNumberAndIndex") => 0,
        Some("eth_getUncleByBlockNumberAndIndex") => 0,
        _ => return None,
    };

    // Get the corresponding blockbumber from the params
    let block_number = tx["params"][position].to_string().replace('\"', "");

    // Return the corresponding named parameter from the RwLock is present
    let nn = has_named_number(&block_number);
    if nn != NamedNumber::Null {
        let rwlock_guard = named_blocknumbers.read().unwrap();

        match nn {
            NamedNumber::Latest => return Some(rwlock_guard.latest),
            NamedNumber::Earliest => return Some(rwlock_guard.earliest),
            NamedNumber::Safe => return Some(rwlock_guard.safe),
            NamedNumber::Finalized => return Some(rwlock_guard.finalized),
            NamedNumber::Pending => return Some(rwlock_guard.pending),
            NamedNumber::Null => return None,
        }
    }

    // Convert to decimal
    let block_number = match u64::from_str_radix(&block_number[2..], 16) {
        Ok(block_number) => block_number,
        Err(_) => return None,
    };

    Some(block_number)
}

// Replaces block tags with a hex number and return the request
pub fn replace_block_tags(
    tx: &mut Value,
    named_blocknumbers: &Arc<RwLock<NamedBlocknumbers>>,
) -> Value {
    // Return if `params` is not a thing
    let params = tx["params"].as_array();
    if params.map_or(true, |p| p.is_empty()) {
        return tx.to_owned();
    }

    // Determine the correct parameter index based on the method
    let position = match tx["method"].as_str() {
        Some("eth_getBalance")
        | Some("eth_getTransactionCount")
        | Some("eth_getCode")
        | Some("eth_call") => 1,
        Some("eth_getStorageAt") => 2, // Corrected index for eth_getStorageAt
        Some("eth_getBlockTransactionCountByNumber")
        | Some("eth_getUncleCountByBlockNumber")
        | Some("eth_getBlockByNumber")
        | Some("eth_getTransactionByBlockNumberAndIndex")
        | Some("eth_getUncleByBlockNumberAndIndex") => 0,
        _ => return tx.to_owned(),
    };

    // Extract the block number parameter
    let block_number = tx["params"][position].to_string().replace('\"', "");

    // Check if the block number is a named tag
    let nn = has_named_number(&block_number);
    if nn != NamedNumber::Null {
        let rwlock_guard = named_blocknumbers.read().unwrap();

        // Replace the named block tag with its corresponding hex value
        match nn {
            NamedNumber::Latest => {
                if rwlock_guard.latest != 0 {
                    tx["params"][position] = json!(format!("0x{:x}", rwlock_guard.latest));
                }
            }
            NamedNumber::Finalized => {
                if rwlock_guard.finalized != 0 {
                    tx["params"][position] = json!(format!("0x{:x}", rwlock_guard.finalized));
                }
            }
            _ => (),
        }
    }

    tx.to_owned()
}

pub async fn incoming_to_value(tx: Request<Incoming>) -> Result<Value, hyper::Error> {
    #[cfg(feature = "debug-verbose")]
    println!("Incoming request: {:?}", tx);

    let tx = tx.collect().await?.to_bytes().clone();
    let mut tx = from_utf8(&tx).unwrap().to_owned();

    let ret = match unsafe { from_str(&mut tx) } {
        Ok(ret) => ret,
        Err(_) => {
            // Insane error handling
            let ret = json!({
                "id": Null,
                "jsonrpc": "2.0",
                "result": "Invalid JSON",
            });

            return Ok(ret);
        }
    };

    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balancer::format::get_block_number_from_request;
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::RwLock;

    // Dummy NamedBlocknumbers for testing
    fn dummy_named_blocknumbers() -> Arc<RwLock<NamedBlocknumbers>> {
        Arc::new(RwLock::new(NamedBlocknumbers {
            latest: 10,
            earliest: 2,
            safe: 3,
            finalized: 4,
            pending: 5,
            number: 6,
        }))
    }

    #[test]
    fn has_named_number_test() {
        assert_eq!(has_named_number("latest"), NamedNumber::Latest);
        assert_eq!(has_named_number("earliest"), NamedNumber::Earliest);
        assert_eq!(has_named_number("safe"), NamedNumber::Safe);
        assert_eq!(has_named_number("finalized"), NamedNumber::Finalized);
        assert_eq!(has_named_number("pending"), NamedNumber::Pending);
        assert_eq!(has_named_number("0x1"), NamedNumber::Null);
        assert_eq!(has_named_number("0x"), NamedNumber::Null);
        assert_eq!(has_named_number("0"), NamedNumber::Null);
    }

    #[test]
    fn get_block_number_from_request_test() {
        // Set up a fake NamedBlocknumbers
        let named_blocknumbers = dummy_named_blocknumbers();

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBalance",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            Some(10)
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBalance",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "adiuasiudagbdiad"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            None
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBalance",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x1"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            Some(1)
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBalance",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            None
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getStorageAt",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x0", "latest"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            Some(10)
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getStorageAt",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x0", "0x1"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            Some(1)
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getStorageAt",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x0"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            None
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getTransactionCount",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            Some(10)
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getTransactionCount",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "safe"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            Some(3)
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getTransactionCount",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "finalized"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            Some(4)
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getTransactionCount",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "pending"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            Some(5)
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getTransactionCount",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x1"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            Some(1)
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getTransactionCount",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            None
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBlockTransactionCountByNumber",
            "params":["latest"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            Some(10)
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBlockTransactionCountByNumber",
            "params":["0x1"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            Some(1)
        );

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBlockTransactionCountByNumber",
            "params":[]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            None
        );
    }

    #[test]
    fn replace_named_block_number_test() {
        let named_blocknumbers = dummy_named_blocknumbers();
        let mut tx = json!({
            "method": "eth_getBalance",
            "params": ["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"]
        });

        let expected = json!({
            "method": "eth_getBalance",
            "params": ["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0xa"]
        });

        assert_eq!(replace_block_tags(&mut tx, &named_blocknumbers), expected);
    }

    #[test]
    fn keep_hex_block_number_test() {
        let named_blocknumbers = dummy_named_blocknumbers();
        let mut tx = json!({
            "method": "eth_getBalance",
            "params": ["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x1"]
        });

        assert_eq!(replace_block_tags(&mut tx, &named_blocknumbers), tx);
    }

    #[test]
    fn handle_invalid_block_number_test() {
        let named_blocknumbers = dummy_named_blocknumbers();
        let mut tx = json!({
            "method": "eth_getBalance",
            "params": ["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "invalid"]
        });

        assert_eq!(replace_block_tags(&mut tx, &named_blocknumbers), tx);
    }

    #[test]
    fn replace_block_number_different_methods_test() {
        let named_blocknumbers = dummy_named_blocknumbers();
        let methods = vec!["eth_getBalance", "eth_getTransactionCount"];

        for method in methods {
            let mut tx = json!({
                "method": method,
                "params": ["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"]
            });

            let expected = json!({
                "method": method,
                "params": ["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0xa"]
            });

            let a = replace_block_tags(&mut tx, &named_blocknumbers);

            assert_eq!(a, expected);
        }
    }

    #[test]
    fn handle_missing_or_empty_params_test() {
        let named_blocknumbers = dummy_named_blocknumbers();
        let mut tx_no_params = json!({
            "method": "eth_getBalance"
        });

        let mut tx_empty_params = json!({
            "method": "eth_getBalance",
            "params": []
        });

        assert_eq!(
            replace_block_tags(&mut tx_no_params, &named_blocknumbers),
            tx_no_params
        );
        assert_eq!(
            replace_block_tags(&mut tx_empty_params, &named_blocknumbers),
            tx_empty_params
        );
    }

    #[test]
    fn handle_zero_blocknumber() {
        let named_blocknumbers = dummy_named_blocknumbers();
        named_blocknumbers.write().unwrap().latest = 0;
        let mut tx = json!({
            "method": "eth_getTransactionCount",
            "params": ["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"]
        });

        let expected = json!({
            "method": "eth_getTransactionCount",
            "params": ["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"]
        });

        assert_eq!(replace_block_tags(&mut tx, &named_blocknumbers), expected);
    }

    #[test]
    fn handle_non_string_block_number_test() {
        let named_blocknumbers = dummy_named_blocknumbers();
        let mut tx = json!({
            "method": "eth_getBalance",
            "params": ["0x407d73d8a49eeb85d32cf465507dd71d507100c1", 100]
        });

        assert_eq!(replace_block_tags(&mut tx, &named_blocknumbers), tx);
    }
}
