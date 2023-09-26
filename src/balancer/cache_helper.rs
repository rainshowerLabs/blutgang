use serde_json::{
    Error,
    Value,
};

// Return the blocknumber from a json-rpc request as a Option<u64>, returning None if it cant find anything
// fn get_block_number_from_request(tx: Value) -> Result<u64, Error> {
//     // The JSON-RPC standard is all over the place so depending on the method, we need to look at
//     // different param indexes. Why? Has i ever???
//     let params = &tx["params"];
//     let method = &tx["method"];

//     match method {
//         "eth_getBlockByNumber" => {}
//     }
// }
