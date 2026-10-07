//! Due to how schizophrenic hyper is, we're defining our http errors like this.
//! ???

#[macro_export]
macro_rules! no_rpc_available {
    () => {
        Ok(hyper::Response::builder()
            .status(500)
            .body(Full::new(Bytes::from(
                "{code:-32002, message:\"error: No working RPC available! Try again later...\"}"
                    .to_string(),
            )))
            .unwrap())
    };
}

#[macro_export]
macro_rules! timed_out {
    () => {
        Ok(hyper::Response::builder()
            .status(408)
            .body(Full::new(Bytes::from(
                "{code:-32001, message:\"error: Request timed out! Try again later...\"}"
                    .to_string(),
            )))
            .unwrap())
    };
}

#[macro_export]
macro_rules! print_cache_error {
    () => {
        tracing::error!("!!! Cache error! Check the DB !!!");
        tracing::error!("To recover, please stop blutgang, delete your cache folder, and start blutgang again.");
        tracing::error!("If the error perists, please open up an issue: https://github.com/rainshowerLabs/blutgang/issues");
    };
}

#[macro_export]
macro_rules! cache_error {
    () => {
        Ok(hyper::Response::builder()
            .status(500)
            .body(Full::new(Bytes::from(
                "{code:-32003, message:\"error: Cache error! Try again later...\"}".to_string(),
            )))
            .unwrap())
    };
}

#[macro_export]
macro_rules! rpc_response {
    (
        $status:expr,
        $body:expr
    ) => {
        Ok(hyper::Response::builder()
            .status($status)
            .body($body)
            .unwrap())
    };
}

/// JSON-RPC error for a request that isn't a single request object, e.g. a batch.
#[macro_export]
macro_rules! invalid_request_body {
    () => {
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid request: expected a single JSON-RPC request object"}}"#
    };
}

#[macro_export]
macro_rules! invalid_request {
    () => {
        Ok(hyper::Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .body(Full::new(Bytes::from($crate::invalid_request_body!())))
            .unwrap())
    };
}
