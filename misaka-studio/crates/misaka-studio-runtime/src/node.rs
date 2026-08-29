//! The MISAKA node, watched or supervised — how the Studio joins the network.
//!
//! Participation on this chain is a ladder, and every rung is the same binary (`kaspad`, the
//! misakas node) run with more at stake:
//!
//! * **Observer** — read someone's node over RPC. Nothing to run.
//! * **Verifier** — run a full node. Syncing IS verifying on this chain: every block's PALW claim
//!   is re-derived by the nodes that accept it, so an unbonded node is already doing the work the
//!   network pays bonded panels for.
//! * **Producer** — the same node with `--palw-produce`: a bonded key, a pay address, and (for a
//!   model class) the class artifact. There is no external miner on this network — the thing
//!   that runs the model is the thing that makes the block — so "mining software" means
//!   supervising `kaspad` well.
//!
//! # The RPC dialect, exactly
//!
//! kaspad's JSON RPC is a **bare websocket** (any path, no handshake) speaking the
//! `workflow-rpc` JSON envelope — which is *not* JSON-RPC 2.0, and the differences are load
//! bearing:
//!
//! * request: `{"id":1,"method":"getInfo","params":{}}` — `id` must be a **number** and present,
//!   `params` must be present (`{}` when empty), method names are lowerCamelCase;
//! * reply: the result arrives in **`params`** (there is no `result` field), errors as
//!   `{"error":{"code":0,"message":…}}`;
//! * a malformed envelope or unknown method string **drops the socket** without a reply, so this
//!   client never interpolates method names and always round-trips ids;
//! * server-pushed notifications are the same envelope with no `id` — responses are matched by
//!   `id` presence first.
//!
//! The endpoint only exists when the node was started with `--rpclisten-json`; the supervisor
//! below always passes it, and the attach path says exactly that when a connection is refused.
//!
//! # Why one-shot connections
//!
//! Each status poll opens a fresh websocket, performs its calls, and closes. A held connection
//! with a demux map would be faster per call — and would need reconnect logic, id routing, and a
//! liveness story for a node that restarts (which, on a chain where operators are told to restart
//! with new flags to change role, is normal). Polling a loopback socket once a second costs
//! microseconds; the complexity is the thing worth not paying.

use crate::{Error, Result};
use futures_util::{SinkExt, StreamExt};
use misaka_studio_core::settings::{NetworkRole, NodeNetwork, NodeSettings};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

/// Default wRPC-JSON ports, from the node's `network.rs` (upstream Kaspa's + 10000).
pub fn default_json_rpc_port(network: NodeNetwork) -> u16 {
    match network {
        NodeNetwork::Testnet11 => 28210,
        NodeNetwork::Devnet => 28610,
        NodeNetwork::Simnet => 28510,
    }
}

/// P2P entry nodes for testnet-11, from the join runbook — used only as `--addpeer` fallbacks
/// when DNS is unavailable, which is exactly the situation the runbook names them for.
pub const TESTNET11_FALLBACK_PEERS: &[&str] = &["169.58.232.113:26311", "169.58.232.114:26311", "169.58.39.220:26311"];

/// Turn what a user types into a websocket URL. Accepts `host:port`, `ws://…`, or a bare host.
pub fn normalize_rpc_url(input: &str, network: NodeNetwork) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return format!("ws://127.0.0.1:{}", default_json_rpc_port(network));
    }
    if trimmed.starts_with("ws://") || trimmed.starts_with("wss://") {
        return trimmed.to_string();
    }
    if trimmed.contains(':') {
        return format!("ws://{trimmed}");
    }
    format!("ws://{trimmed}:{}", default_json_rpc_port(network))
}

/// One call against a node's JSON endpoint.
///
/// Waits for the frame whose `id` matches ours; notification frames (no `id`) that arrive in
/// between are skipped, because a node with an active subscription interleaves them freely.
pub async fn wrpc_call(url: &str, method: &str, params: Value, timeout: Duration) -> Result<Value> {
    let attempt = async {
        let (mut socket, _) =
            tokio_tungstenite::connect_async(url).await.map_err(|e| Error::Node { message: format!("{url}: {e}") })?;

        // A fixed id per connection is enough: the connection carries exactly one request.
        let id = 1u64;
        let frame = json!({ "id": id, "method": method, "params": params }).to_string();
        socket.send(Message::Text(frame)).await.map_err(|e| Error::Node { message: format!("{url}: send: {e}") })?;

        while let Some(message) = socket.next().await {
            let message = message.map_err(|e| Error::Node { message: format!("{url}: {e}") })?;
            let Message::Text(text) = message else { continue };
            let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
            // No `id` = a notification, not our reply.
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error")
                && !error.is_null()
            {
                let message = error.get("message").and_then(Value::as_str).unwrap_or("unknown node error");
                return Err(Error::Node { message: format!("{method}: {message}") });
            }
            let _ = socket.close(None).await;
            return Ok(value.get("params").cloned().unwrap_or(Value::Null));
        }
        Err(Error::Node {
            message: format!("{url}: the connection closed before a reply — a malformed frame or an unknown method drops the socket"),
        })
    };

    tokio::time::timeout(timeout, attempt).await.map_err(|_| Error::Node { message: format!("{url}: no reply within {timeout:?}") })?
}

/// What the Studio shows about a node, assembled from `getInfo` + `getBlockDagInfo` +
/// `getConnectedPeerInfo`.
#[derive(Clone, Debug, Default, Serialize)]
pub struct NodeStatus {
    pub reachable: bool,
    pub rpc_url: String,
    /// `supervised` when this Studio launched the process, `attached` otherwise.
    pub source: String,
    pub server_version: Option<String>,
    pub network: Option<String>,
    pub is_synced: Option<bool>,
    pub virtual_daa_score: Option<u64>,
    pub block_count: Option<u64>,
    pub header_count: Option<u64>,
    pub difficulty: Option<f64>,
    pub peer_count: Option<usize>,
    pub mempool_size: Option<u64>,
    pub sink: Option<String>,
    /// Why the node is unreachable, when it is.
    pub error: Option<String>,
}

/// Read a node's status. Partial answers are kept: a node that answers `getInfo` but not the DAG
/// call is still reachable, and half a picture beats an error page.
pub async fn query_status(url: &str) -> NodeStatus {
    let timeout = Duration::from_secs(4);
    let mut status = NodeStatus { rpc_url: url.to_string(), ..Default::default() };

    match wrpc_call(url, "getInfo", json!({}), timeout).await {
        Ok(info) => {
            status.reachable = true;
            status.server_version = info.get("serverVersion").and_then(Value::as_str).map(str::to_string);
            status.is_synced = info.get("isSynced").and_then(Value::as_bool);
            status.mempool_size = info.get("mempoolSize").and_then(Value::as_u64);
        }
        Err(e) => {
            status.error = Some(format!(
                "{e}. The node's JSON RPC only exists when it was started with --rpclisten-json; a supervised node gets the flag automatically."
            ));
            return status;
        }
    }

    if let Ok(dag) = wrpc_call(url, "getBlockDagInfo", json!({}), timeout).await {
        status.network = dag.get("network").and_then(Value::as_str).map(str::to_string);
        status.virtual_daa_score = dag.get("virtualDaaScore").and_then(Value::as_u64);
        status.block_count = dag.get("blockCount").and_then(Value::as_u64);
        status.header_count = dag.get("headerCount").and_then(Value::as_u64);
        status.difficulty = dag.get("difficulty").and_then(Value::as_f64);
        status.sink = dag.get("sink").and_then(Value::as_str).map(str::to_string);
    }
    if let Ok(peers) = wrpc_call(url, "getConnectedPeerInfo", json!({}), timeout).await {
        status.peer_count = peers.get("peerInfo").and_then(Value::as_array).map(Vec::len);
    }
    status
}

/// One row of the node's own class table, parsed from its `[palw-dump]` log line —
/// `class=<id> base=<bool> status=<status> share=<permille|NONE> budget=<blocks>`.
///
/// Log-scraped because the node exposes no class-enumeration RPC (its `palw_dump.rs` says as
/// much); the dump flag exists for exactly this consumer. Only a supervised node's table is
/// visible — for an attached node the Studio shows the built-in registry snapshot instead.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NodeClassRow {
    pub class_id: String,
    pub base: bool,
    pub status: String,
    pub share_permille: Option<u16>,
    pub budget_blocks: Option<u64>,
}

pub(crate) fn parse_class_row(line: &str) -> Option<NodeClassRow> {
    if !line.contains("[palw-dump]") || !line.contains("class=") {
        return None;
    }
    let field = |key: &str| line.split_whitespace().find_map(|word| word.strip_prefix(key)).map(str::to_string);
    Some(NodeClassRow {
        class_id: field("class=")?,
        base: field("base=").is_some_and(|v| v == "true"),
        status: field("status=").unwrap_or_default(),
        share_permille: field("share=").and_then(|v| v.parse().ok()),
        budget_blocks: field("budget=").and_then(|v| v.parse().ok()),
    })
}

/// A line worth surfacing in the activity feed: production, panel work, holds, and the identity
/// lines an operator is told to check.
pub(crate) fn is_activity_line(line: &str) -> bool {
    [
        "[palw-producer]",
        "[palw-panel]",
        "[palw-dump]",
        "Consensus params fingerprint",
        "Genesis mismatch",
        "accepted block",
        "Accepted block",
    ]
    .iter()
    .any(|needle| line.contains(needle))
}

/// How many log lines the supervisor keeps, and how many activity lines.
const LOG_CAPACITY: usize = 600;
const ACTIVITY_CAPACITY: usize = 120;

#[derive(Default)]
struct NodeLogState {
    log: VecDeque<String>,
    activity: VecDeque<String>,
    classes: Vec<NodeClassRow>,
}

struct SupervisedNode {
    child: tokio::process::Child,
    rpc_url: String,
    role: NetworkRole,
    args_shown: Vec<String>,
}

/// The node this Studio watches or runs.
pub struct NodeManager {
    supervised: RwLock<Option<SupervisedNode>>,
    logs: Arc<Mutex<NodeLogState>>,
}

/// What `/api/v1/network/node` returns.
#[derive(Clone, Debug, Serialize)]
pub struct NodeView {
    pub status: NodeStatus,
    pub role: NetworkRole,
    /// Present when supervised: the exact command line, because an operator must be able to see
    /// — and reproduce without the Studio — what is running under their key.
    pub command_line: Option<Vec<String>>,
    pub classes_from_node: Vec<NodeClassRow>,
    pub activity: Vec<String>,
}

impl NodeManager {
    pub fn new() -> Self {
        NodeManager { supervised: RwLock::new(None), logs: Arc::new(Mutex::new(NodeLogState::default())) }
    }

    /// Where the node binary is: the configured path, beside the Studio, or PATH.
    pub fn resolve_kaspad(configured: Option<&PathBuf>) -> PathBuf {
        let name = if cfg!(windows) { "kaspad.exe" } else { "kaspad" };
        if let Some(path) = configured {
            return path.clone();
        }
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            let beside = dir.join(name);
            if beside.is_file() {
                return beside;
            }
        }
        std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path).map(|dir| dir.join(name)).find(|c| c.is_file()))
            .unwrap_or(None)
            .unwrap_or_else(|| PathBuf::from(name))
    }

    /// The command line for a node in `settings`' network and role.
    ///
    /// Built as data first so the UI can show it verbatim: a person putting a bonded key on the
    /// line gets to read the exact flags before anything runs, and can run them without the
    /// Studio afterwards.
    pub fn build_args(settings: &NodeSettings, rpc_port: u16) -> Result<Vec<String>> {
        let mut args: Vec<String> = Vec::new();
        match settings.network {
            NodeNetwork::Testnet11 => {
                args.push("--testnet".into());
                args.push("--netsuffix=11".into());
            }
            NodeNetwork::Devnet => args.push("--devnet".into()),
            NodeNetwork::Simnet => args.push("--simnet".into()),
        }
        if let Some(appdir) = &settings.appdir {
            args.push(format!("--appdir={}", appdir.display()));
        }
        // The Studio's whole view of the node runs over this endpoint. Loopback, always — the
        // node's JSON RPC has no authentication, so exposing it is an operator's deliberate act
        // via extra_args, not a default.
        args.push(format!("--rpclisten-json=127.0.0.1:{rpc_port}"));
        args.push("--utxoindex".into());
        // One-shot class-table dump after sync: the only place the node reports per-class share
        // and budget, and the source of the class ids the UI shows.
        args.push("--palw-dump-classes".into());

        if settings.role == NetworkRole::Producer {
            let key = settings
                .producer_key_path
                .as_ref()
                .ok_or_else(|| Error::bad_request("producing needs node.producer_key_path — generate one with `misaka key gen`"))?;
            let pay = settings.mining_address.as_ref().ok_or_else(|| {
                Error::bad_request("producing needs node.mining_address — the ML-DSA-87 address rewards are paid to")
            })?;
            args.push("--palw-produce".into());
            args.push("--palw-panel".into());
            args.push(format!("--palw-producer-key={}", key.display()));
            args.push(format!("--palw-producer-pay-address={pay}"));
            match &settings.producer_bond {
                Some(bond) => args.push(format!("--palw-producer-bond={bond}")),
                // On the public network a first run registers the bond; the node prints the
                // outpoint to keep. On devnet the genesis registry seats the initial set, so a
                // bond outpoint may legitimately be absent.
                None => args.push("--palw-register-bond".into()),
            }
            if let Some(outpoint) = &settings.fee_outpoint {
                args.push(format!("--palw-fee-outpoint={outpoint}"));
            }
            if let Some(class) = &settings.producer_class {
                args.push(format!("--palw-producer-class={class}"));
            }
            if let Some(artifact) = &settings.class_artifact {
                args.push(format!("--palw-class-artifact={}", artifact.display()));
            }
        }

        args.extend(settings.extra_args.iter().cloned());
        Ok(args)
    }

    /// Launch a supervised node. Refuses when one is already running — two nodes sharing an
    /// appdir corrupt its database, and "start" must never be the thing that does that.
    pub async fn start(&self, settings: &NodeSettings) -> Result<NodeView> {
        {
            let mut guard = self.supervised.write().await;
            if let Some(node) = guard.as_mut() {
                match node.child.try_wait() {
                    Ok(None) => return Err(Error::bad_request("a supervised node is already running; stop it first")),
                    _ => *guard = None, // it exited on its own; forget it
                }
            }
        }

        let binary = Self::resolve_kaspad(settings.kaspad_path.as_ref());
        let rpc_port = default_json_rpc_port(settings.network);
        let args = Self::build_args(settings, rpc_port)?;
        let rpc_url = format!("ws://127.0.0.1:{rpc_port}");

        {
            let mut logs = self.logs.lock().expect("log lock");
            *logs = NodeLogState::default();
        }

        tracing::info!(binary = %binary.display(), ?args, "starting MISAKA node");
        let mut child = tokio::process::Command::new(&binary)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| Error::Node {
                message: format!(
                    "could not start {}: {e}. Build the node with `cargo build --release -p kaspad` in the misakas \
                     repository, or set node.kaspad_path.",
                    binary.display()
                ),
            })?;

        for stream in [child.stdout.take().map(NodePipe::Out), child.stderr.take().map(NodePipe::Err)].into_iter().flatten() {
            let logs = self.logs.clone();
            tokio::spawn(async move {
                match stream {
                    NodePipe::Out(s) => drain_node(s, logs).await,
                    NodePipe::Err(s) => drain_node(s, logs).await,
                }
            });
        }

        let view_args = std::iter::once(binary.display().to_string()).chain(args.iter().cloned()).collect();
        *self.supervised.write().await = Some(SupervisedNode { child, rpc_url, role: settings.role, args_shown: view_args });
        self.view(settings).await
    }

    /// Stop the supervised node.
    ///
    /// The producing caveat is real and stated where the button is (the UI), not silently
    /// enforced here: on this chain a producer's in-flight claims are its responsibility to
    /// serve, and a node stopped with claims open defaults them against its bond. Stopping is
    /// still the operator's call — the Studio's job is that they make it knowingly.
    pub async fn stop(&self) -> Result<()> {
        if let Some(mut node) = self.supervised.write().await.take() {
            tracing::info!("stopping the supervised node");
            #[cfg(unix)]
            {
                // SIGTERM first: the node flushes its database on a graceful shutdown, and a
                // RocksDB that was kill -9'd replays its WAL for minutes on the next start.
                if let Some(pid) = node.child.id() {
                    let _ = std::process::Command::new("kill").args(["-TERM", &pid.to_string()]).status();
                    for _ in 0..50 {
                        if matches!(node.child.try_wait(), Ok(Some(_))) {
                            return Ok(());
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
            let _ = node.child.kill().await;
            let _ = node.child.wait().await;
        }
        Ok(())
    }

    /// The current picture: supervised child if any, else the configured attach URL.
    pub async fn view(&self, settings: &NodeSettings) -> Result<NodeView> {
        let (rpc_url, source, command_line, role) = {
            let mut guard = self.supervised.write().await;
            let exited = match guard.as_mut() {
                Some(node) => matches!(node.child.try_wait(), Ok(Some(_))),
                None => false,
            };
            if exited {
                // Keep the log (it holds the reason); drop the dead handle so start works again.
                *guard = None;
            }
            match guard.as_ref() {
                Some(node) => (node.rpc_url.clone(), "supervised".to_string(), Some(node.args_shown.clone()), node.role),
                None => {
                    let url = normalize_rpc_url(settings.rpc_url.as_deref().unwrap_or(""), settings.network);
                    (url, "attached".to_string(), None, settings.role)
                }
            }
        };

        let mut status = query_status(&rpc_url).await;
        status.source = source;
        let (classes_from_node, activity) = {
            let logs = self.logs.lock().expect("log lock");
            (logs.classes.clone(), logs.activity.iter().cloned().collect())
        };
        Ok(NodeView { status, role, command_line, classes_from_node, activity })
    }

    pub async fn is_supervising(&self) -> bool {
        self.supervised.read().await.is_some()
    }

    pub fn recent_log(&self, limit: usize) -> Vec<String> {
        let logs = self.logs.lock().expect("log lock");
        logs.log.iter().rev().take(limit).cloned().collect::<Vec<_>>().into_iter().rev().collect()
    }
}

impl Default for NodeManager {
    fn default() -> Self {
        Self::new()
    }
}

enum NodePipe {
    Out(tokio::process::ChildStdout),
    Err(tokio::process::ChildStderr),
}

async fn drain_node<R: tokio::io::AsyncRead + Unpin>(stream: R, logs: Arc<Mutex<NodeLogState>>) {
    let mut lines = BufReader::new(stream).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::debug!(target: "node", "{line}");
        let mut state = logs.lock().expect("log lock");
        if state.log.len() == LOG_CAPACITY {
            state.log.pop_front();
        }
        state.log.push_back(line.clone());
        if let Some(row) = parse_class_row(&line) {
            state.classes.retain(|existing| existing.class_id != row.class_id);
            state.classes.push(row);
        }
        if is_activity_line(&line) {
            if state.activity.len() == ACTIVITY_CAPACITY {
                state.activity.pop_front();
            }
            state.activity.push_back(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_studio_core::settings::NodeSettings;

    #[test]
    fn urls_normalize_to_the_network_default_port() {
        assert_eq!(normalize_rpc_url("", NodeNetwork::Devnet), "ws://127.0.0.1:28610");
        assert_eq!(normalize_rpc_url("", NodeNetwork::Testnet11), "ws://127.0.0.1:28210");
        assert_eq!(normalize_rpc_url("10.0.0.5:28210", NodeNetwork::Testnet11), "ws://10.0.0.5:28210");
        assert_eq!(normalize_rpc_url("ws://x:1/", NodeNetwork::Devnet), "ws://x:1/");
        assert_eq!(normalize_rpc_url("myhost", NodeNetwork::Devnet), "ws://myhost:28610");
    }

    /// The dump line as `palw_dump.rs` writes it, share both present and NONE.
    #[test]
    fn class_rows_parse_from_the_dump_lines() {
        let row =
            parse_class_row("[palw-dump]   class=c185df95388739dc base=true  status=Active share=999  budget=1000").expect("parses");
        assert_eq!(row.class_id, "c185df95388739dc");
        assert!(row.base);
        assert_eq!(row.status, "Active");
        assert_eq!(row.share_permille, Some(999));
        assert_eq!(row.budget_blocks, Some(1000));

        let none = parse_class_row("[palw-dump]   class=ec7bbcbf base=false status=Pending share=NONE budget=0").expect("parses");
        assert_eq!(none.share_permille, None);
        assert_eq!(none.budget_blocks, Some(0));

        assert!(parse_class_row("[palw-dump] class table follows").is_none());
        assert!(parse_class_row("unrelated line").is_none());
    }

    #[test]
    fn a_verifier_gets_a_plain_full_node_command() {
        let settings = NodeSettings { network: NodeNetwork::Testnet11, role: NetworkRole::Verifier, ..Default::default() };
        let args = NodeManager::build_args(&settings, 28210).expect("builds");
        let joined = args.join(" ");
        assert!(joined.contains("--testnet --netsuffix=11"));
        assert!(joined.contains("--rpclisten-json=127.0.0.1:28210"));
        assert!(joined.contains("--palw-dump-classes"));
        assert!(!joined.contains("--palw-produce"), "a verifier does not produce: {joined}");
    }

    /// The producer command is the runbook's §4, assembled — and refused while its named
    /// prerequisites are missing, with the remedy in the message.
    #[test]
    fn a_producer_without_a_key_is_refused_with_the_remedy() {
        let settings = NodeSettings { role: NetworkRole::Producer, ..Default::default() };
        let err = NodeManager::build_args(&settings, 28210).unwrap_err();
        assert!(err.to_string().contains("misaka key gen"), "{err}");
    }

    #[test]
    fn a_producer_with_a_bond_mines_and_without_one_registers() {
        let mut settings = NodeSettings {
            role: NetworkRole::Producer,
            producer_key_path: Some("/keys/miner.seed".into()),
            mining_address: Some("misakatest:qqq".into()),
            ..Default::default()
        };
        let register = NodeManager::build_args(&settings, 28210).expect("builds").join(" ");
        assert!(register.contains("--palw-register-bond"));
        assert!(!register.contains("--palw-producer-bond="));

        settings.producer_bond = Some("abc123:0".into());
        settings.fee_outpoint = Some("abc123:1".into());
        let produce = NodeManager::build_args(&settings, 28210).expect("builds").join(" ");
        assert!(produce.contains("--palw-produce"));
        assert!(produce.contains("--palw-panel"));
        assert!(produce.contains("--palw-producer-bond=abc123:0"));
        assert!(produce.contains("--palw-fee-outpoint=abc123:1"));
        assert!(!produce.contains("--palw-register-bond"));
    }

    #[test]
    fn activity_lines_are_the_palw_ones() {
        assert!(is_activity_line("[palw-producer] holding: budget spent [class=… budget=0]"));
        assert!(is_activity_line("[palw-panel] registered bond abc:0"));
        assert!(is_activity_line("Consensus params fingerprint: 15bab795… (network testnet-11)"));
        assert!(!is_activity_line("2026-08-29 mempool size 3"));
    }
}
