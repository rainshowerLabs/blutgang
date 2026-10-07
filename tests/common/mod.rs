//! Harness for end-to-end tests: spawns `anvil` nodes and a `blutgang` process
//! in front of them, and talks to both over JSON-RPC.
//!
//! `anvil` is looked up in `$ANVIL_BIN`, then `$PATH`, then `~/.foundry/bin`.
//! Tests are skipped when it can't be found, unless `BLUTGANG_REQUIRE_ANVIL`
//! is set, in which case they fail instead.

#![allow(dead_code)]

use std::{
    fs::File,
    net::TcpListener,
    path::{
        Path,
        PathBuf,
    },
    process::{
        Child,
        Command,
        Stdio,
    },
    time::{
        Duration,
        Instant,
    },
};

use serde_json::{
    json,
    Value,
};
use tempfile::TempDir;

/// How long a process gets to start answering requests.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

/// The first account anvil funds with 10,000 ETH and unlocks.
pub const DEV_ACCOUNT_0: &str = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";
/// The second account anvil funds with 10,000 ETH and unlocks.
pub const DEV_ACCOUNT_1: &str = "0x70997970c51812dc3a010c7d01b50e0d17dc79c8";

/// Returns early from the calling test if `anvil` isn't installed.
#[macro_export]
macro_rules! require_anvil {
    () => {
        if $crate::common::anvil_bin().is_none() {
            if std::env::var_os("BLUTGANG_REQUIRE_ANVIL").is_some() {
                panic!("anvil not found and BLUTGANG_REQUIRE_ANVIL is set");
            }
            eprintln!("skipping: anvil not found, install foundry or set ANVIL_BIN");
            return;
        }
    };
}

/// Finds the `anvil` binary.
pub fn anvil_bin() -> Option<PathBuf> {
    if let Some(bin) = std::env::var_os("ANVIL_BIN") {
        return Some(bin.into());
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    let foundry = std::env::var_os("HOME").map(|home| Path::new(&home).join(".foundry/bin"));
    std::env::split_paths(&path)
        .chain(foundry)
        .map(|dir| dir.join("anvil"))
        .find(|bin| bin.is_file())
}

/// Asks the OS for a port that is free right now.
pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn http_client() -> reqwest::Client {
    // Everything is on localhost, so keep any configured proxy out of the way.
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

/// Posts `body` as-is and returns the HTTP status and response body.
pub async fn post_raw(url: &str, content_type: Option<&str>, body: &str) -> (u16, String) {
    let mut request = http_client().post(url).body(body.to_string());
    if let Some(content_type) = content_type {
        request = request.header("content-type", content_type);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.text().await.unwrap())
}

/// Sends a JSON-RPC request with the given id and returns the whole response.
pub async fn request_with_id(url: &str, id: u64, method: &str, params: Value) -> Value {
    let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    let response = http_client()
        .post(url)
        .json(&request)
        .send()
        .await
        .unwrap_or_else(|err| panic!("{method} to {url} failed: {err}"));
    let body = response.text().await.unwrap();
    serde_json::from_str(&body).unwrap_or_else(|err| panic!("{method}: bad JSON {body:?}: {err}"))
}

/// Sends a JSON-RPC request and returns its `result`, panicking on an error response.
pub async fn call(url: &str, method: &str, params: Value) -> Value {
    let response = request_with_id(url, 1, method, params).await;
    match response.get("result") {
        Some(result) if response.get("error").is_none() => result.clone(),
        _ => panic!("{method} returned an error: {response}"),
    }
}

/// Parses a `0x`-prefixed hex quantity.
pub fn hex_u128(value: &Value) -> u128 {
    let hex = value
        .as_str()
        .unwrap_or_else(|| panic!("not a string: {value}"));
    u128::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap()
}

/// Polls `check` until it returns `Some`, panicking after `timeout`.
pub async fn wait_for<T, F, Fut>(what: &str, timeout: Duration, mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let start = Instant::now();
    loop {
        if let Some(value) = check().await {
            return value;
        }
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Returns true once `url` answers `eth_chainId`.
async fn answers(url: &str) -> bool {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "eth_chainId", "params": [] });
    let Ok(response) = http_client().post(url).json(&request).send().await else {
        return false;
    };
    matches!(response.json::<Value>().await, Ok(body) if body.get("result").is_some())
}

/// Kills a child process and reaps it.
fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// An `anvil` node, killed on drop.
pub struct Anvil {
    child: Option<Child>,
    pub port: u16,
    pub chain_id: u64,
}

impl Anvil {
    /// Starts an anvil node with the default chain id and waits for it to answer.
    pub async fn spawn() -> Self {
        Self::with_chain_id(31337).await
    }

    /// Starts an anvil node with `chain_id` and waits for it to answer.
    pub async fn with_chain_id(chain_id: u64) -> Self {
        Self::start(free_port(), chain_id).await
    }

    async fn start(port: u16, chain_id: u64) -> Self {
        let mut child = Command::new(anvil_bin().expect("anvil not found"))
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["--chain-id", &chain_id.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to start anvil");

        let url = format!("http://127.0.0.1:{port}");
        let start = Instant::now();
        while !answers(&url).await {
            if let Ok(Some(status)) = child.try_wait() {
                panic!("anvil exited during startup: {status}");
            }
            if start.elapsed() > STARTUP_TIMEOUT {
                kill(&mut child);
                panic!("anvil did not start answering on port {port}");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Self {
            child: Some(child),
            port,
            chain_id,
        }
    }

    pub fn http_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.port)
    }

    /// Calls the node directly, bypassing blutgang.
    pub async fn call(&self, method: &str, params: Value) -> Value {
        call(&self.http_url(), method, params).await
    }

    /// Stops the node, simulating it going down.
    pub fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            kill(&mut child);
        }
    }

    /// Starts a fresh node on the same port, simulating it coming back up.
    pub async fn restart(&mut self) {
        self.kill();
        let restarted = Self::start(self.port, self.chain_id).await;
        *self = restarted;
    }
}

impl Drop for Anvil {
    fn drop(&mut self) {
        self.kill();
    }
}

/// An upstream RPC as listed in blutgang's config.
#[derive(Clone)]
struct Upstream {
    url: String,
    ws_url: Option<String>,
}

/// Builder for the config file a [`Blutgang`] is started with.
#[derive(Clone)]
pub struct Config {
    upstreams: Vec<Upstream>,
    db: &'static str,
    ttl: u64,
    max_retries: u32,
    max_consecutive: u32,
    expected_block_time: u64,
    health_check: bool,
    health_check_ttl: u64,
    sort_on_startup: bool,
    admin: bool,
}

impl Config {
    /// A config balancing over `anvils`, with health checks and the admin namespace off.
    pub fn new(anvils: &[&Anvil]) -> Self {
        Self {
            upstreams: anvils
                .iter()
                .map(|anvil| {
                    Upstream {
                        url: anvil.http_url(),
                        ws_url: Some(anvil.ws_url()),
                    }
                })
                .collect(),
            db: "sled",
            ttl: 1000,
            max_retries: 4,
            max_consecutive: 150,
            expected_block_time: 1000,
            health_check: false,
            health_check_ttl: 200,
            sort_on_startup: false,
            admin: false,
        }
    }

    /// Selects the cache backend, `sled` or `rocksdb`.
    pub fn db(mut self, db: &'static str) -> Self {
        self.db = db;
        self
    }

    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// How many requests in a row may go to the same RPC.
    pub fn max_consecutive(mut self, max_consecutive: u32) -> Self {
        self.max_consecutive = max_consecutive;
        self
    }

    pub fn health_check(mut self, enabled: bool) -> Self {
        self.health_check = enabled;
        self
    }

    pub fn sort_on_startup(mut self, enabled: bool) -> Self {
        self.sort_on_startup = enabled;
        self
    }

    pub fn admin(mut self, enabled: bool) -> Self {
        self.admin = enabled;
        self
    }

    /// Lists the RPCs without WS endpoints, which turns blutgang's WS support off.
    pub fn without_ws(mut self) -> Self {
        for upstream in &mut self.upstreams {
            upstream.ws_url = None;
        }
        self
    }

    fn render(&self, dir: &Path, port: u16, admin_port: u16) -> String {
        let mut toml = format!(
            r#"[blutgang]
address = "127.0.0.1"
port = {port}
ma_length = 10
sort_on_startup = {sort_on_startup}
health_check = {health_check}
header_check = true
ttl = {ttl}
max_retries = {max_retries}
expected_block_time = {expected_block_time}
health_check_ttl = {health_check_ttl}
supress_rpc_check = true
db = "{db}"

[blutgang.admin]
enable = {admin}
address = "127.0.0.1"
port = {admin_port}
readonly = false
jwt = false

[blutgang.sled]
path = {sled_path:?}

[blutgang.rocksdb]
create_if_missing = true
"#,
            sort_on_startup = self.sort_on_startup,
            health_check = self.health_check,
            ttl = self.ttl,
            max_retries = self.max_retries,
            expected_block_time = self.expected_block_time,
            health_check_ttl = self.health_check_ttl,
            db = self.db,
            admin = self.admin,
            sled_path = dir.join("cache-sled").display().to_string(),
        );
        for upstream in &self.upstreams {
            toml.push_str(&format!(
                "\n[[rpc]]\nurl = {:?}\nmax_consecutive = {}\nmax_per_second = 0\n",
                upstream.url, self.max_consecutive
            ));
            if let Some(ws_url) = &upstream.ws_url {
                toml.push_str(&format!("ws_url = {ws_url:?}\n"));
            }
        }
        toml
    }
}

/// A `blutgang` process, killed on drop.
///
/// It runs in its own temporary directory, where its config, cache and log live.
/// The log is printed if the test fails.
pub struct Blutgang {
    child: Child,
    dir: TempDir,
    pub port: u16,
    pub admin_port: u16,
    pub metrics_port: u16,
}

impl Blutgang {
    /// Starts blutgang with `config` and waits for it to proxy requests.
    pub async fn spawn(config: Config) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let port = free_port();
        let admin_port = free_port();
        let metrics_port = free_port();
        let config_path = dir.path().join("config.toml");
        std::fs::write(&config_path, config.render(dir.path(), port, admin_port)).unwrap();

        let log = File::create(dir.path().join("blutgang.log")).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_blutgang"))
            .arg("--config")
            .arg(&config_path)
            // The rocksdb cache is created in the working directory.
            .current_dir(dir.path())
            // The metrics exporter binds 9000 by default, which parallel tests would fight over.
            .env("TRACING_METRICS_PORT", metrics_port.to_string())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("failed to start blutgang");

        let mut blutgang = Self {
            child,
            dir,
            port,
            admin_port,
            metrics_port,
        };

        let url = blutgang.http_url();
        let start = Instant::now();
        while !answers(&url).await {
            if let Ok(Some(status)) = blutgang.child.try_wait() {
                panic!(
                    "blutgang exited during startup: {status}\n{}",
                    blutgang.log()
                );
            }
            if start.elapsed() > STARTUP_TIMEOUT {
                panic!("blutgang did not start answering\n{}", blutgang.log());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        blutgang
    }

    pub fn http_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.port)
    }

    pub fn admin_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.admin_port)
    }

    pub fn metrics_url(&self) -> String {
        format!("http://127.0.0.1:{}/metrics", self.metrics_port)
    }

    /// Calls a method through blutgang, returning its `result`.
    pub async fn call(&self, method: &str, params: Value) -> Value {
        call(&self.http_url(), method, params).await
    }

    /// Calls a method through blutgang, returning the whole response.
    pub async fn request(&self, id: u64, method: &str, params: Value) -> Value {
        request_with_id(&self.http_url(), id, method, params).await
    }

    /// Calls a method on the admin namespace, returning its `result`.
    pub async fn admin_call(&self, method: &str, params: Value) -> Value {
        call(&self.admin_url(), method, params).await
    }

    /// Everything blutgang has written to stdout and stderr so far.
    pub fn log(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("blutgang.log")).unwrap_or_default()
    }
}

impl Drop for Blutgang {
    fn drop(&mut self) {
        kill(&mut self.child);
        if std::thread::panicking() {
            eprintln!("---- blutgang log ----\n{}", self.log());
        }
    }
}
