pub async fn listen_for_admin_requests() {
    loop {
        let (stream, socketaddr) = listener.accept().await?;
        println!("\x1b[35mInfo:\x1b[0m Admin connection from: {}", socketaddr);

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            accept!(
                io,
                &rpc_list_rwlock_clone,
                config.ma_length,
                &cache_clone,
                &finalized_rx_clone,
                &named_blocknumbers_clone,
                &head_cache_clone,
                &config_clone
            );
        });
    }
}