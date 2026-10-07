//! End-to-end tests that run blutgang in front of real `anvil` nodes.
//!
//! Each test starts its own anvil node(s) and blutgang process on free ports,
//! so they can run in parallel. See `common` for how anvil is found.

mod common;

use std::time::Duration;

use common::{
    hex_u128,
    post_raw,
    wait_for,
    Anvil,
    Blutgang,
    Config,
    DEV_ACCOUNT_0,
    DEV_ACCOUNT_1,
};
use futures::{
    SinkExt,
    StreamExt,
};
use serde_json::{
    json,
    Value,
};
use tokio_tungstenite::tungstenite::Message;

const ONE_ETH: u128 = 1_000_000_000_000_000_000;

#[tokio::test]
async fn proxies_requests_to_anvil() {
    require_anvil!();
    let anvil = Anvil::spawn().await;
    anvil.call("anvil_mine", json!(["0x5"])).await;
    let blutgang = Blutgang::spawn(Config::new(&[&anvil])).await;

    assert_eq!(
        blutgang.call("eth_chainId", json!([])).await,
        json!("0x7a69")
    );
    assert_eq!(
        blutgang.call("eth_blockNumber", json!([])).await,
        anvil.call("eth_blockNumber", json!([])).await
    );
    assert_eq!(
        hex_u128(
            &blutgang
                .call("eth_getBalance", json!([DEV_ACCOUNT_0, "latest"]))
                .await
        ),
        10_000 * ONE_ETH
    );

    let block = blutgang
        .call("eth_getBlockByNumber", json!(["0x3", false]))
        .await;
    assert_eq!(
        block,
        anvil
            .call("eth_getBlockByNumber", json!(["0x3", false]))
            .await
    );
}

#[tokio::test]
async fn preserves_request_ids() {
    require_anvil!();
    let anvil = Anvil::spawn().await;
    let blutgang = Blutgang::spawn(Config::new(&[&anvil])).await;

    for id in [0, 1, 42, 1337] {
        let response = blutgang.request(id, "eth_chainId", json!([])).await;
        assert_eq!(response["id"], json!(id), "{response}");
        assert_eq!(response["jsonrpc"], json!("2.0"));
    }

    // A cache hit must answer with the id of the request, not the one that filled the cache.
    anvil.call("evm_mine", json!([])).await;
    for id in [7, 8] {
        let response = blutgang
            .request(id, "eth_getBlockByNumber", json!(["0x1", false]))
            .await;
        assert_eq!(response["id"], json!(id), "{response}");
    }
}

#[tokio::test]
async fn sends_transactions() {
    require_anvil!();
    let anvil = Anvil::spawn().await;
    let blutgang = Blutgang::spawn(Config::new(&[&anvil])).await;

    let before = hex_u128(
        &blutgang
            .call("eth_getBalance", json!([DEV_ACCOUNT_1, "latest"]))
            .await,
    );

    let tx_hash = blutgang
        .call(
            "eth_sendTransaction",
            json!([{
                "from": DEV_ACCOUNT_0,
                "to": DEV_ACCOUNT_1,
                "value": format!("0x{ONE_ETH:x}"),
            }]),
        )
        .await;

    let receipt = blutgang
        .call("eth_getTransactionReceipt", json!([tx_hash]))
        .await;
    assert_eq!(receipt["status"], json!("0x1"), "{receipt}");
    assert_eq!(receipt["to"], json!(DEV_ACCOUNT_1));

    let tx = blutgang
        .call("eth_getTransactionByHash", json!([tx_hash]))
        .await;
    assert_eq!(tx["hash"], tx_hash);

    // `latest` must not be answered from the cache, or the balance would be stale.
    let after = hex_u128(
        &blutgang
            .call("eth_getBalance", json!([DEV_ACCOUNT_1, "latest"]))
            .await,
    );
    assert_eq!(after, before + ONE_ETH);
}

#[tokio::test]
async fn latest_follows_the_chain_head() {
    require_anvil!();
    let anvil = Anvil::spawn().await;
    let blutgang = Blutgang::spawn(Config::new(&[&anvil])).await;

    for expected in 0..3u128 {
        let head = blutgang
            .call("eth_getBlockByNumber", json!(["latest", false]))
            .await;
        assert_eq!(hex_u128(&head["number"]), expected);
        assert_eq!(
            hex_u128(&blutgang.call("eth_blockNumber", json!([])).await),
            expected
        );
        blutgang.call("evm_mine", json!([])).await;
    }
}

#[tokio::test]
async fn latest_follows_new_heads_with_health_checks() {
    require_anvil!();
    let anvil = Anvil::spawn().await;
    // With health checks on, blutgang tracks the head over a newHeads subscription
    // and rewrites `latest` to that block number before forwarding.
    let blutgang = Blutgang::spawn(Config::new(&[&anvil]).health_check(true)).await;

    for target in 1..=3u128 {
        anvil.call("evm_mine", json!([])).await;
        wait_for("latest to advance", Duration::from_secs(10), || {
            async {
                let head = blutgang
                    .call("eth_getBlockByNumber", json!(["latest", false]))
                    .await;
                (hex_u128(&head["number"]) == target).then_some(())
            }
        })
        .await;
    }
}

async fn serves_cached_results_when_upstream_is_down(db: &'static str) {
    let mut anvil = Anvil::spawn().await;
    anvil.call("anvil_mine", json!(["0x3"])).await;
    let blutgang = Blutgang::spawn(Config::new(&[&anvil]).db(db).max_retries(2)).await;

    let historical = [
        ("eth_getBlockByNumber", json!(["0x2", false])),
        ("eth_getBalance", json!([DEV_ACCOUNT_0, "0x1"])),
    ];
    let mut expected = Vec::new();
    for (method, params) in &historical {
        expected.push(blutgang.call(method, params.clone()).await);
    }

    anvil.kill();

    // Queries pinned to a block are answered from the cache...
    for ((method, params), expected) in historical.iter().zip(&expected) {
        assert_eq!(&blutgang.call(method, params.clone()).await, expected);
    }

    // ...while anything that needs the node fails.
    let (status, body) = post_raw(
        &blutgang.http_url(),
        Some("application/json"),
        r#"{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}"#,
    )
    .await;
    assert_eq!(status, 408, "{body}");
    assert!(body.contains("timed out"), "{body}");
}

#[tokio::test]
async fn serves_cached_results_when_upstream_is_down_sled() {
    require_anvil!();
    serves_cached_results_when_upstream_is_down("sled").await;
}

#[tokio::test]
async fn serves_cached_results_when_upstream_is_down_rocksdb() {
    require_anvil!();
    serves_cached_results_when_upstream_is_down("rocksdb").await;
}

#[tokio::test]
async fn rejects_invalid_requests() {
    require_anvil!();
    let anvil = Anvil::spawn().await;
    let blutgang = Blutgang::spawn(Config::new(&[&anvil])).await;
    let url = blutgang.http_url();
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}"#;

    let (status, response) = post_raw(&url, None, body).await;
    assert_eq!(status, 400, "{response}");
    let (status, response) = post_raw(&url, Some("text/plain"), body).await;
    assert_eq!(status, 400, "{response}");

    // Batches aren't supported.
    let (status, response) = post_raw(&url, Some("application/json"), &format!("[{body}]")).await;
    assert_eq!(status, 400, "{response}");
    let response: Value = serde_json::from_str(&response).unwrap();
    assert_eq!(response["error"]["code"], json!(-32600), "{response}");

    // Blutgang should still be serving after all of that.
    assert_eq!(
        blutgang.call("eth_chainId", json!([])).await,
        json!("0x7a69")
    );
}

#[tokio::test]
async fn balances_requests_across_rpcs() {
    require_anvil!();
    let first = Anvil::with_chain_id(1001).await;
    let second = Anvil::with_chain_id(1002).await;
    // Allowing only one request in a row per RPC makes blutgang alternate between them.
    let blutgang = Blutgang::spawn(
        Config::new(&[&first, &second])
            .max_consecutive(1)
            .sort_on_startup(true),
    )
    .await;

    let mut seen = Vec::new();
    for _ in 0..10 {
        seen.push(hex_u128(&blutgang.call("eth_chainId", json!([])).await) as u64);
    }
    assert!(seen.contains(&first.chain_id), "{seen:?}");
    assert!(seen.contains(&second.chain_id), "{seen:?}");
}

#[tokio::test]
async fn fails_over_to_a_live_rpc() {
    require_anvil!();
    let mut dead = Anvil::with_chain_id(1001).await;
    let alive = Anvil::with_chain_id(1002).await;
    let blutgang = Blutgang::spawn(Config::new(&[&dead, &alive]).max_consecutive(1)).await;

    dead.kill();

    for _ in 0..10 {
        assert_eq!(
            hex_u128(&blutgang.call("eth_chainId", json!([])).await) as u64,
            alive.chain_id
        );
    }
}

/// Waits for the admin `/health` endpoint to answer with `status`.
async fn wait_for_health(blutgang: &Blutgang, status: u16) {
    let url = format!("{}/health", blutgang.admin_url());
    wait_for(
        &format!("/health to be {status}"),
        Duration::from_secs(10),
        || {
            let url = url.clone();
            async move { (post_raw(&url, None, "").await.0 == status).then_some(()) }
        },
    )
    .await;
}

/// Kills one of two RPCs and checks that the health check parks it in the poverty
/// list, that traffic keeps flowing, and that it rejoins once it's back up.
async fn health_check_handles_a_dead_rpc(config: impl FnOnce(Config) -> Config) {
    let mut flaky = Anvil::with_chain_id(1001).await;
    let stable = Anvil::with_chain_id(1002).await;
    let blutgang = Blutgang::spawn(config(
        Config::new(&[&flaky, &stable])
            .health_check(true)
            .admin(true),
    ))
    .await;
    let flaky_name = format!("127.0.0.1:{}", flaky.port);
    let in_list = |list: Value| list.as_str().unwrap().contains(&flaky_name);

    assert!(in_list(
        blutgang.admin_call("blutgang_rpc_list", json!([])).await
    ));

    flaky.kill();

    wait_for(
        "the dead RPC to be dropped",
        Duration::from_secs(15),
        || {
            async {
                let poverty = blutgang
                    .admin_call("blutgang_poverty_list", json!([]))
                    .await;
                in_list(poverty).then_some(())
            }
        },
    )
    .await;
    assert!(!in_list(
        blutgang.admin_call("blutgang_rpc_list", json!([])).await
    ));

    // Missing an RPC degrades health, but requests keep working.
    wait_for_health(&blutgang, 202).await;
    for _ in 0..5 {
        assert_eq!(
            hex_u128(&blutgang.call("eth_chainId", json!([])).await) as u64,
            stable.chain_id
        );
    }

    flaky.restart().await;

    wait_for("the RPC to rejoin", Duration::from_secs(15), || {
        async {
            let rpc_list = blutgang.admin_call("blutgang_rpc_list", json!([])).await;
            in_list(rpc_list).then_some(())
        }
    })
    .await;
    wait_for_health(&blutgang, 200).await;
}

#[tokio::test]
async fn health_check_handles_a_dead_http_rpc() {
    require_anvil!();
    health_check_handles_a_dead_rpc(Config::without_ws).await;
}

#[tokio::test]
#[ignore = "bug: when an RPC's WS connection drops, the health check loop stalls, so \
            the RPC isn't reliably kept in the poverty list, health isn't updated \
            and the RPC never rejoins"]
async fn health_check_handles_a_dead_ws_rpc() {
    require_anvil!();
    health_check_handles_a_dead_rpc(|config| config).await;
}

#[tokio::test]
async fn admin_namespace() {
    require_anvil!();
    let anvil = Anvil::spawn().await;
    let blutgang = Blutgang::spawn(Config::new(&[&anvil]).admin(true)).await;

    let (status, body) = post_raw(&format!("{}/ready", blutgang.admin_url()), None, "").await;
    assert_eq!((status, body.as_str()), (200, "OK"));

    let rpc_list = blutgang.admin_call("blutgang_rpc_list", json!([])).await;
    assert!(
        rpc_list
            .as_str()
            .unwrap()
            .contains(&format!("127.0.0.1:{}", anvil.port)),
        "{rpc_list}"
    );

    assert_eq!(
        blutgang.admin_call("blutgang_ttl", json!([])).await,
        json!(1000)
    );
    blutgang
        .admin_call("blutgang_set_ttl", json!(["2500"]))
        .await;
    assert_eq!(
        blutgang.admin_call("blutgang_ttl", json!([])).await,
        json!(2500)
    );

    // The admin namespace isn't exposed on the public port.
    let response = blutgang.request(1, "blutgang_ttl", json!([])).await;
    assert!(response.get("error").is_some(), "{response}");
}

/// Reads WS messages until one satisfies `matches`.
async fn next_matching<S>(ws: &mut S, matches: impl Fn(&Value) -> bool) -> Value
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let message = ws.next().await.expect("WS closed").expect("WS error");
            if let Message::Text(text) = message {
                let value: Value = serde_json::from_str(&text).unwrap();
                if matches(&value) {
                    return value;
                }
            }
        }
    })
    .await
    .expect("timed out waiting for a WS message")
}

#[tokio::test]
async fn websocket_calls_and_subscriptions() {
    require_anvil!();
    let anvil = Anvil::spawn().await;
    let blutgang = Blutgang::spawn(Config::new(&[&anvil])).await;

    let (mut ws, _) = tokio_tungstenite::connect_async(blutgang.ws_url())
        .await
        .expect("failed to connect over WS");

    ws.send(Message::Text(
        json!({"jsonrpc": "2.0", "id": 1, "method": "eth_chainId", "params": []}).to_string(),
    ))
    .await
    .unwrap();
    let response = next_matching(&mut ws, |msg| msg["id"] == json!(1)).await;
    assert_eq!(response["result"], json!("0x7a69"), "{response}");

    ws.send(Message::Text(
        json!({"jsonrpc": "2.0", "id": 2, "method": "eth_subscribe", "params": ["newHeads"]})
            .to_string(),
    ))
    .await
    .unwrap();
    let response = next_matching(&mut ws, |msg| msg["id"] == json!(2)).await;
    let subscription = response["result"].clone();
    assert!(subscription.is_string(), "{response}");

    anvil.call("evm_mine", json!([])).await;

    let notification = next_matching(&mut ws, |msg| msg["method"] == "eth_subscription").await;
    assert_eq!(notification["params"]["subscription"], subscription);
    assert_eq!(
        hex_u128(&notification["params"]["result"]["number"]),
        1,
        "{notification}"
    );

    ws.send(Message::Text(
        json!({"jsonrpc": "2.0", "id": 3, "method": "eth_unsubscribe", "params": [subscription]})
            .to_string(),
    ))
    .await
    .unwrap();
    let response = next_matching(&mut ws, |msg| msg["id"] == json!(3)).await;
    assert_eq!(response["result"], json!(true), "{response}");
}

#[tokio::test]
async fn websocket_is_disabled_without_ws_upstreams() {
    require_anvil!();
    let anvil = Anvil::spawn().await;
    let blutgang = Blutgang::spawn(Config::new(&[&anvil]).without_ws()).await;

    let error = tokio_tungstenite::connect_async(blutgang.ws_url())
        .await
        .expect_err("WS upgrade should be refused");
    match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), 500);
        }
        other => panic!("unexpected WS error: {other}"),
    }

    // HTTP still works.
    assert_eq!(
        blutgang.call("eth_chainId", json!([])).await,
        json!("0x7a69")
    );
}
