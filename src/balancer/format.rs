use crate::NamedBlocknumbers;
use http_body_util::BodyExt;
use hyper::{
    body::Incoming,
    Request,
};
use memchr::memmem;
use regex::Regex;
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

use crate::rpc::error::RpcError;

struct BlocknumIndex<'a> {
    pub method: &'a [u8],
    pub position: usize,
}

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
    let methods = [
        BlocknumIndex {
            method: b"eth_getBalance",
            position: 1,
        },
        BlocknumIndex {
            method: b"eth_getStorageAt",
            position: 2,
        },
        BlocknumIndex {
            method: b"eth_getTransactionCount",
            position: 1,
        },
        BlocknumIndex {
            method: b"eth_getBlockTransactionCountByNumber",
            position: 0,
        },
        BlocknumIndex {
            method: b"eth_getUncleCountByBlockNumber",
            position: 0,
        },
        BlocknumIndex {
            method: b"eth_getCode",
            position: 1,
        },
        BlocknumIndex {
            method: b"eth_call",
            position: 1,
        },
        BlocknumIndex {
            method: b"eth_getBlockByNumber",
            position: 0,
        },
        BlocknumIndex {
            method: b"eth_getTransactionByBlockNumberAndIndex",
            position: 0,
        },
        BlocknumIndex {
            method: b"eth_getUncleByBlockNumberAndIndex",
            position: 0,
        },
    ];

    // Iterate through the array and return the position, return None if not present
    for item in methods.iter() {
        if memmem::find(tx["method"].to_string().as_bytes(), item.method).is_some() {
            let block_number = tx["params"][item.position].to_string().replace('\"', "");

            // If `null` return None
            if block_number == "null" {
                return None;
            }

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
                    _ => continue, // continue and try to decode as decimal just in case
                }
            }

            // Convert to decimal
            let block_number = match u64::from_str_radix(&block_number[2..], 16) {
                Ok(block_number) => block_number,
                Err(_) => return None,
            };

            return Some(block_number);
        }
    }

    None
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

pub fn _extract_id(request: &str) -> Option<String> {
    // Define the regular expression pattern to capture the "id" field
    let re = Regex::new(r#""id"\s*:\s*("([^"]*)"|(\d+))"#).unwrap();

    if let Some(captures) = re.captures(request) {
        if let Some(id) = captures.get(2).or(captures.get(3)) {
            return Some(id.as_str().to_string());
        }
    }

    None
}

pub fn _replace_id(tx: &str, id: &str) -> Result<String, RpcError> {
    // Use regex to find and capture the "id" field with optional double quotes
    //
    // We could be using memmem and making it much more optimized but this is fine
    let re = match Regex::new(r#"("id"\s*:\s*)("[^"]*"|\d+)"#) {
        Ok(re) => re,
        Err(err) => return Err(RpcError::InvalidResponse(err.to_string())),
    };

    // Replace "id" with the new id using the captured group
    let replaced = re.replace(tx, |caps: &regex::Captures| format!("{}{}", &caps[1], id));

    Ok(replaced.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balancer::format::get_block_number_from_request;
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::RwLock;

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
        let named_blocknumbers = Arc::new(RwLock::new(NamedBlocknumbers {
            latest: 1,
            earliest: 2,
            safe: 3,
            finalized: 4,
            pending: 5,
            number: 6,
        }));

        let request = json!({
            "id":1,
            "jsonrpc":"2.0",
            "method":"eth_getBalance",
            "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"]
        });

        assert_eq!(
            get_block_number_from_request(request, &named_blocknumbers),
            Some(1)
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
            Some(1)
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
            Some(1)
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
            Some(1)
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
    fn replace_id_test() {
        let tx = r#"{"id":1,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
        let tx = _replace_id(tx, "2").unwrap();
        assert_eq!(
            tx,
            r#"{"id":2,"jsonrpc":"2.0","method":"eth_call","params":...}"#
        );

        let tx = r#"{"jsonrpc":"2.0","method":"eth_call", "id":1, "params":...}"#;
        let tx = _replace_id(tx, "2").unwrap();
        assert_eq!(
            tx,
            r#"{"jsonrpc":"2.0","method":"eth_call", "id":2, "params":...}"#
        );

        let tx = r#"{"jsonrpc":"2.0","method":"eth_call", "id": "1", "params":...}"#;
        let tx = _replace_id(tx, "2").unwrap();
        assert_eq!(
            tx,
            r#"{"jsonrpc":"2.0","method":"eth_call", "id": 2, "params":...}"#
        );

        let tx = r#"{"jsonrpc":"2.0","method":"eth_call", "id": 1, "params":...}"#;
        let tx = _replace_id(tx, "2").unwrap();
        assert_eq!(
            tx,
            r#"{"jsonrpc":"2.0","method":"eth_call", "id": 2, "params":...}"#
        );
    }

    #[test]
    fn extract_id_test() {
        let tx = r#"{"id":1,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
        let tx = _extract_id(tx).unwrap();
        assert_eq!(tx, r#"1"#);

        let tx = r#"{"id": 1,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
        let tx = _extract_id(tx).unwrap();
        assert_eq!(tx, r#"1"#);

        let tx = r#"{"id":1 ,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
        let tx = _extract_id(tx).unwrap();
        assert_eq!(tx, r#"1"#);

        let tx = r#"{"id":"1","jsonrpc":"2.0","method":"eth_call","params":...}"#;
        let tx = _extract_id(tx).unwrap();
        assert_eq!(tx, r#"1"#);

        let tx = r#"{"id":"1","jsonrpc":"2.0","method":"eth_call","params":...}"#;
        let tx = _extract_id(tx).unwrap();
        assert_eq!(tx, r#"1"#);

        let tx = r#"{,"jsonrpc":"2.0","method":"eth_call","params":...}"#;
        let tx = _extract_id(tx);
        assert_eq!(tx, None);
    }
}
