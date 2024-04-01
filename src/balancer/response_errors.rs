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
        println!("\x1b[31m!!! Cache error! Check the DB !!!\x1b[0m");
        println!("To recover, please stop blutgang, delete your cache folder, and start blutgang again.");
        println!("If the error perists, please open up an issue: https://github.com/rainshowerLabs/blutgang/issues");
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
