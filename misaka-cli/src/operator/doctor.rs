//! **`misaka doctor`** — ADR-0122 §6.3 and §11: every check a miner needs, grouped the way an
//! operator looks for a fault (the node, who it mines as, what it mines, the host, the prompt
//! lane), each a `✓` / `!` / `✗` row, and every failure the five-field finding with its fix.
//!
//! The expected fingerprint and fence heights are computed from the consensus parameters this CLI
//! links, and compared with the lines the node printed at boot — the fleet's roll check, made a
//! command. "Is this node the release?" has an answer on the operator's own machine.
//!
//! The doctor reads and never changes anything. A check it cannot make on this host (a clock on
//! macOS, a key owned by another user) is a skipped row that says why, never a pass.

use crate::operator::catalog;
use crate::operator::finding::{self, Finding, Severity, paint};
use crate::operator::host;
use crate::operator::procs;
use crate::operator::snapshot::Snapshot;
use crate::operator::status::group;
use crate::{CliError, CliResult, OutputFormat};
use serde::Serialize;
use serde_json::json;
use std::io::{Read, Write};
use std::time::Duration;

/// The areas a check belongs to, in the order the screen prints them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Area {
    Node,
    Identity,
    Model,
    Host,
    Prompt,
}

impl Area {
    fn label(self) -> &'static str {
        match self {
            Area::Node => "NODE",
            Area::Identity => "IDENTITY",
            Area::Model => "MODEL",
            Area::Host => "HOST",
            Area::Prompt => "PROMPT",
        }
    }
}

/// One row of the doctor's table.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Check {
    pub(crate) area: Area,
    pub(crate) name: &'static str,
    pub(crate) severity: Severity,
    /// What was measured, in one line.
    pub(crate) value: String,
    /// For a warning or a failure: the five fields.
    pub(crate) finding: Option<Finding>,
}

impl Check {
    fn ok(area: Area, name: &'static str, value: impl Into<String>) -> Check {
        Check { area, name, severity: Severity::Ok, value: value.into(), finding: None }
    }
    fn skip(area: Area, name: &'static str, why: impl Into<String>) -> Check {
        Check { area, name, severity: Severity::Skip, value: why.into(), finding: None }
    }
    fn info(area: Area, name: &'static str, value: impl Into<String>) -> Check {
        Check { area, name, severity: Severity::Info, value: value.into(), finding: None }
    }
    fn found(area: Area, name: &'static str, value: impl Into<String>, finding: Finding) -> Check {
        Check { area, name, severity: finding.severity, value: value.into(), finding: Some(finding) }
    }
}

/// Run every check of `areas` (all of them when empty) against `snap`.
pub(crate) async fn checks(snap: &Snapshot, areas: &[Area]) -> Vec<Check> {
    let want = |a: Area| areas.is_empty() || areas.contains(&a);
    let mut out = Vec::new();
    if want(Area::Node) {
        node_checks(snap, &mut out);
    }
    if want(Area::Identity) {
        identity_checks(snap, &mut out).await;
    }
    if want(Area::Model) {
        model_checks(snap, &mut out);
    }
    if want(Area::Host) {
        host_checks(snap, &mut out);
    }
    if want(Area::Prompt) && snap.profile.prompt.is_some() {
        prompt_checks(snap, &mut out);
    }
    out
}

fn node_checks(snap: &Snapshot, out: &mut Vec<Check>) {
    let p = &snap.profile;
    let a = Area::Node;
    match (&p.kaspad, p.ambiguous_kaspads.as_slice()) {
        (_, pids) if !pids.is_empty() => {
            out.push(Check::found(a, "process", format!("{} nodes", pids.len()), catalog::several_nodes(&p.network, pids)))
        }
        (Some((proc_, args)), _) => {
            let role = match (args.produce, args.panel) {
                (true, _) => "producing",
                (false, true) => "panel only",
                _ => "not producing",
            };
            let exe = proc_.exe.as_ref().map(|e| format!(" · {}", host::tilde(e))).unwrap_or_default();
            let value =
                format!("kaspad pid {} · up {} · {role}{exe}", proc_.pid, crate::operator::status::ago(proc_.uptime_secs() as i64));
            match proc_.image_replaced {
                Some(true) => out.push(Check::found(a, "process", value, catalog::image_replaced(proc_.pid))),
                Some(false) => out.push(Check::ok(a, "process", format!("{value} · running image = binary on disk"))),
                None => out.push(Check::ok(a, "process", value)),
            }
        }
        (None, _) => {
            if snap.node.is_ok() {
                out.push(Check::info(a, "process", "no kaspad on this host — reading the node at --rpc"));
            } else if p.is_configured() {
                out.push(Check::found(a, "process", "not running", catalog::node_down(&p.network, &p.appdir.display().to_string())));
            } else {
                out.push(Check::found(a, "process", "nothing set up", catalog::not_set_up()));
            }
        }
    }
    // A devnet node started with a ruleset flag runs parameters of its own: this CLI's fingerprint
    // for the network is not the one it should print, and comparing the two would cry fork.
    let own_ruleset = p.kaspad.as_ref().and_then(|(_, a)| a.ruleset_flags.first().cloned());
    let expected = if own_ruleset.is_some() { None } else { crate::operator::status::expected(&p.network) };
    // Only the running node's own boot lines say what it runs: a log left by an earlier run, or by
    // another node at the default path, says nothing about the process that is up now.
    let log = snap.boot_log();
    match (log.and_then(|l| l.fingerprint.as_deref()), &expected) {
        (Some(node), Some((cli, _))) if node == cli => {
            out.push(Check::ok(a, "fork", format!("fingerprint {} = this CLI's {}", short(node), p.network)))
        }
        (Some(node), Some((cli, _))) => {
            let peers = snap.node.as_ref().ok().and_then(|n| n.peers.as_ref().map(|v| v.len()));
            out.push(Check::found(
                a,
                "fork",
                format!("node {} · this CLI {}", short(node), short(cli)),
                catalog::fingerprint_mismatch(node, cli, &p.network, peers),
            ))
        }
        (None, Some((cli, _))) => out.push(Check::skip(
            a,
            "fork",
            format!("this CLI expects {}; the node's boot line is not in its log ({})", short(cli), log_where(snap)),
        )),
        (node, None) => out.push(Check::skip(
            a,
            "fork",
            match &own_ruleset {
                Some(flag) => format!(
                    "fingerprint {} · the node runs its own ruleset ({flag}), not the shipped {}",
                    node.map(short).unwrap_or("?"),
                    p.network
                ),
                None => format!("'{}' is not a network this CLI knows", p.network),
            },
        )),
    }
    match (log.and_then(|l| l.schedule.as_ref()), &expected) {
        (Some(node), Some((_, cli))) if node == cli => out.push(Check::ok(a, "schedule", heights(node))),
        (Some(node), Some((_, cli))) => {
            out.push(Check::found(a, "schedule", format!("node {}", heights(node)), catalog::schedule_mismatch(node, cli)))
        }
        (None, Some((_, cli))) => out.push(Check::skip(a, "schedule", format!("this CLI expects {}", heights(cli)))),
        _ => {}
    }
    match &snap.node {
        Ok(n) => {
            let index = if n.server.has_utxo_index { "utxoindex on" } else { "utxoindex OFF" };
            out.push(Check::ok(a, "rpc", format!("wRPC Borsh {} · {index}", n.url.trim_start_matches("ws://"))));
            if !n.server.has_utxo_index {
                out.push(Check::found(a, "utxoindex", "off", catalog::utxoindex_off(p.prompt.is_some())));
            }
            let net = n.server.network_id.to_string();
            if net != p.network {
                out.push(Check::found(a, "network", net.clone(), catalog::network_mismatch(&net, &p.network)));
            }
            let unsynced_mining = p.kaspad.as_ref().is_some_and(|(_, a)| a.enable_unsynced_mining);
            let has_peers = n.peers.as_ref().is_some_and(|v| !v.is_empty());
            if n.server.is_synced {
                out.push(Check::ok(a, "sync", format!("synced · DAA {}", group(n.daa()))));
            } else if unsynced_mining && has_peers {
                // The producer's own gate: a stale tip is waived by the flag while peers are up.
                out.push(Check::info(
                    a,
                    "sync",
                    format!("not synced · DAA {} · --enable-unsynced-mining: the producer draws anyway", group(n.daa())),
                ));
            } else {
                out.push(Check::found(a, "sync", format!("syncing · DAA {}", group(n.daa())), catalog::not_synced(n.daa())));
            }
            match (n.peers.as_ref().map(|v| v.len()), n.outbound_peers()) {
                (Some(0), _) => out.push(Check::found(a, "peers", "0", catalog::no_peers())),
                (Some(all), Some(outbound)) => out.push(Check::ok(a, "peers", format!("{all} ({outbound} outbound)"))),
                _ => out.push(Check::skip(a, "peers", "getConnectedPeerInfo did not answer")),
            }
        }
        Err((url, why)) => {
            let borsh = p.kaspad.as_ref().and_then(|(_, a)| a.rpclisten_borsh.as_deref());
            out.push(Check::found(a, "rpc", format!("{url}: {why}"), catalog::rpc_unreachable(url, why, borsh)));
        }
    }
}

async fn identity_checks(snap: &Snapshot, out: &mut Vec<Check>) {
    let p = &snap.profile;
    let a = Area::Identity;
    match (&p.key_path, &snap.key) {
        (Some(path), Some(Ok(k))) => out.push(Check::ok(a, "key", format!("{} · {}", host::tilde(path), short_address(&k.address)))),
        (Some(path), Some(Err(e))) => {
            out.push(Check::found(a, "key", "unreadable", catalog::key_unreadable(&path.display().to_string(), e)))
        }
        _ => out.push(Check::skip(a, "key", "no key file named (--key-file, [mining] key, or the node's --palw-producer-key)")),
    }
    let facts = snap.facts.as_ref();
    match (&p.bond, facts) {
        (None, _) => out.push(Check::skip(a, "bond", "no bond named (--bond, [advanced] bond, or the node's --palw-producer-bond)")),
        (Some(bond), Some(Ok(f))) if f.bond_known => {
            let key = snap.key.as_ref().and_then(|k| k.as_ref().ok());
            match key {
                Some(k) if !f.bond_registered_pubkey.eq_ignore_ascii_case(&k.pubkey_hex) => out.push(Check::found(
                    a,
                    "bond",
                    format!("{bond} · registered to another key"),
                    catalog::key_mismatch(bond, Some(&k.pubkey_hex), Some(&f.bond_registered_pubkey)),
                )),
                Some(_) => out.push(Check::ok(
                    a,
                    "bond",
                    format!("{} · registered to this key · collateral {}", short_op(bond), catalog::msk(f.bond_collateral as u128)),
                )),
                None => out.push(Check::ok(
                    a,
                    "bond",
                    format!("{} · registered · collateral {}", short_op(bond), catalog::msk(f.bond_collateral as u128)),
                )),
            }
        }
        (Some(bond), Some(Ok(_))) => out.push(Check::found(
            a,
            "bond",
            format!("{} · the chain knows no bond there", short_op(bond)),
            catalog::not_ready(
                kaspa_consensus_core::palw_producer_v2::PALW_NOT_READY_BOND_UNKNOWN_V2,
                &Default::default(),
                Some(bond),
            ),
        )),
        (Some(_), Some(Err(e))) => out.push(Check::skip(a, "bond", format!("not read: {e}"))),
        (Some(_), None) => out.push(Check::skip(a, "bond", "not read: the node is not answering")),
    }
    // The pay address: an explicitly configured one must be ML-DSA-87 P2PKH on this network. The
    // key's own address is that by construction.
    if let Some(pay) = &p.pay_address {
        let prefix = kaspa_consensus_core::config::params::Params::from(
            p.network.parse::<kaspa_consensus_core::network::NetworkId>().unwrap_or(
                kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 11),
            ),
        )
        .prefix();
        match kaspa_addresses::Address::try_from(pay.as_str()) {
            Ok(addr) if addr.version != kaspa_addresses::Version::PubKeyHashMlDsa87 => out.push(Check::found(
                a,
                "pay address",
                pay.clone(),
                catalog::pay_address_invalid(pay, "not an ML-DSA-87 P2PKH address"),
            )),
            Ok(addr) if addr.prefix != prefix => out.push(Check::found(
                a,
                "pay address",
                pay.clone(),
                catalog::pay_address_invalid(pay, &format!("an address for {}, and this network is {prefix}", addr.prefix)),
            )),
            Ok(_) => out.push(Check::ok(a, "pay address", format!("{} · ML-DSA-87 P2PKH on {}", short_address(pay), p.network))),
            Err(e) => out.push(Check::found(a, "pay address", pay.clone(), catalog::pay_address_invalid(pay, &e.to_string()))),
        }
    }
    // The fee outpoint: named, or persisted by the panel once it has used one.
    if p.produce || p.panel {
        let persisted =
            std::fs::read_to_string(p.persisted_fee_outpoint()).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        match (&p.fee_outpoint, persisted) {
            (None, None) => out.push(Check::found(a, "fee outpoint", "none", catalog::fee_outpoint_missing())),
            (named, persisted) => {
                let op = persisted.clone().or(named.clone()).unwrap_or_default();
                let whence = if persisted.is_some() { "persisted by the panel" } else { "named" };
                let unspent = fee_outpoint_unspent(snap, &op).await;
                match unspent {
                    Some(true) => out.push(Check::ok(a, "fee outpoint", format!("{} ({whence}) · unspent", short_op(&op)))),
                    Some(false) => out.push(Check::found(
                        a,
                        "fee outpoint",
                        format!("{} ({whence})", short_op(&op)),
                        Finding::error(
                            "E-FUNDS-FEE-OUTPOINT-SPENT",
                            crate::exit::FUNDS,
                            "The panel's fee outpoint is not at the key's address",
                        )
                        .reason("the panel carries receipts and answers with it; spent or elsewhere, it carries nothing")
                        .current(format!("{op}: not among the key's unspent outputs"))
                        .required("a mature, unbonded UTXO at the producer key's address, ≥ 0.1 MSK")
                        .fix("misaka mining setup --fee-outpoint auto"),
                    )),
                    None => out.push(Check::skip(
                        a,
                        "fee outpoint",
                        format!("{} ({whence}) · not checked: needs the key and --utxoindex", short_op(&op)),
                    )),
                }
            }
        }
    }
}

/// Is `op` among the UTXOs at the key's address? `None` when that cannot be read.
async fn fee_outpoint_unspent(snap: &Snapshot, op: &str) -> Option<bool> {
    let node = snap.node.as_ref().ok()?;
    if !node.server.has_utxo_index {
        return None;
    }
    let key = snap.key.as_ref()?.as_ref().ok()?;
    let addr = kaspa_addresses::Address::try_from(key.address.as_str()).ok()?;
    let want = crate::bond::parse_outpoint(op).ok()?;
    let all = crate::wallet::page_all(&node.nv, &addr).await.ok()?;
    Some(all.iter().any(|u| u.outpoint == want))
}

fn model_checks(snap: &Snapshot, out: &mut Vec<Check>) {
    let a = Area::Model;
    match (&snap.class_id, snap.facts.as_ref().and_then(|f| f.as_ref().ok())) {
        (Some(class), Some(f)) if f.available => {
            let base = if snap.class_is_base { " (the base class)" } else { "" };
            let budget = if f.is_base_class {
                format!("epoch {} · the floor has no cap", f.epoch_index)
            } else {
                format!(
                    "epoch {} · the class has made {} of its {} blocks",
                    f.epoch_index, f.epoch_produced_blocks, f.epoch_budget_blocks
                )
            };
            out.push(Check::ok(a, "class", format!("{}…{base} · registered · {budget}", short(class))));
            if snap.profile.prompt.is_some() && !f.fp_certified {
                out.push(Check::found(
                    a,
                    "prompt lane",
                    "not certified",
                    Finding::error("E-PROMPT-CLASS-UNCERTIFIED", crate::exit::MODEL, "This class has no certified prompt lane")
                        .reason("a free-prompt claim on an uncertified class enters no state: the gateway refuses to commit it")
                        .current(format!("class {}… · fp_certified false", short(class)))
                        .fix("mine the prompt lane on a certified class, or certify this one (docs/palw-certify-a-new-model.md)"),
                ));
            }
        }
        (Some(class), Some(_)) => out.push(Check::found(a, "class", format!("{}…", short(class)), catalog::class_unknown(class))),
        (Some(class), None) => out.push(Check::skip(a, "class", format!("{}… · not read: needs the node and a bond", short(class)))),
        (None, _) => out.push(Check::skip(a, "class", "this network has no ConsensusV2 bundle in this build")),
    }
    for artifact in &snap.profile.artifacts {
        match std::fs::metadata(artifact) {
            Ok(m) => out.push(Check::ok(a, "artifact", format!("{} · {}", host::tilde(artifact), host::human_bytes(m.len())))),
            Err(_) => out.push(Check::found(
                a,
                "artifact",
                artifact.display().to_string(),
                catalog::artifact_missing(&artifact.display().to_string()),
            )),
        }
    }
}

fn host_checks(snap: &Snapshot, out: &mut Vec<Check>) {
    let a = Area::Host;
    let p = &snap.profile;
    let dir = p.retention_dir();
    match host::volume_space(&dir) {
        Some((free, total)) => {
            let floor = host::retention_floor_bytes(total);
            let usage =
                host::dir_usage(&dir).map(|(b, f)| format!(" · retention {} in {f} files", host::human_bytes(b))).unwrap_or_default();
            let janitor = snap
                .live_log()
                .and_then(|l| l.janitor_pruned.as_ref().map(|(ts, _)| *ts).or(l.janitor_started))
                .map(|ts| format!(" · janitor {} ago", crate::operator::status::ago(procs::now_unix() as i64 - ts)))
                .unwrap_or_default();
            let value = format!("{} free ≥ floor {}{usage}{janitor}", host::human_bytes(free), host::human_bytes(floor));
            match catalog::disk(free, floor, total, &dir.display().to_string()) {
                Some(f) => out.push(Check::found(a, "disk", value, f)),
                None => out.push(Check::ok(a, "disk", value)),
            }
        }
        None => out.push(Check::skip(a, "disk", format!("the volume holding {} could not be read", dir.display()))),
    }
    if let Some((_, line)) = snap.live_log().and_then(|l| l.janitor_short.as_ref()) {
        out.push(Check::found(a, "retention", "under its floor", catalog::retention_short(line)));
    }
    let weights: u64 = p.artifacts.iter().filter_map(|x| std::fs::metadata(x).ok()).map(|m| m.len()).sum();
    // A validating node's 8 GiB, and ADR-0112's default residency of a fifth of the weights.
    let needed = (8u64 << 30) + weights / 5;
    match host::mem_available() {
        Some(avail) if avail < needed => out.push(Check::found(a, "memory", host::human_bytes(avail), catalog::memory(avail, needed))),
        Some(avail) => {
            out.push(Check::ok(a, "memory", format!("{} available ≥ {} needed", host::human_bytes(avail), host::human_bytes(needed))))
        }
        None => out.push(Check::skip(a, "memory", "not readable on this host")),
    }
    match host::unattended_upgrades_enabled() {
        Some(true) => out.push(Check::found(a, "upgrades", "unattended-upgrades enabled", catalog::unattended_upgrades())),
        Some(false) => out.push(Check::ok(a, "upgrades", "unattended-upgrades off")),
        None => out.push(Check::skip(a, "upgrades", "not an apt host")),
    }
    match host::ntp_synchronized() {
        Some(true) => out.push(Check::ok(a, "clock", "NTP synchronised")),
        Some(false) => out.push(Check::found(a, "clock", "not synchronised", catalog::clock_unsynced())),
        None => out.push(Check::skip(a, "clock", "timedatectl is not available here")),
    }
}

fn prompt_checks(snap: &Snapshot, out: &mut Vec<Check>) {
    let a = Area::Prompt;
    let Some(lane) = &snap.profile.prompt else { return };
    match &lane.outbox {
        Some(dir) if dir.is_dir() => out.push(Check::ok(a, "outbox", dir.display().to_string())),
        Some(dir) => out.push(Check::found(
            a,
            "outbox",
            dir.display().to_string(),
            Finding::error("E-PROMPT-OUTBOX-MISSING", crate::exit::CONFIG, "The prompt lane's outbox does not exist")
                .current(dir.display().to_string())
                .fix("create it, or point [advanced.prompt] outbox at the gateway's --outbox"),
        )),
        None => out.push(Check::skip(a, "outbox", "no outbox named ([advanced.prompt] outbox or --outbox)")),
    }
    match http_get_status(&lane.gateway_listen, "/health", Duration::from_secs(3)) {
        Ok(200) => out.push(Check::ok(a, "gateway", format!("http://{}/health answers", lane.gateway_listen))),
        Ok(code) => out.push(Check::found(
            a,
            "gateway",
            format!("HTTP {code}"),
            catalog::gateway_down(&lane.gateway_listen, &format!("HTTP {code}")),
        )),
        Err(e) => out.push(Check::found(a, "gateway", "not answering", catalog::gateway_down(&lane.gateway_listen, &e))),
    }
    let rails = procs::find(procs::Component::Rail);
    let watching = lane.outbox.as_ref().is_some_and(|o| {
        let o = o.display().to_string();
        rails.iter().any(|r| {
            r.args.iter().any(|x| x == "--watch") && r.args.iter().any(|x| x.trim_end_matches('/') == o.trim_end_matches('/'))
        })
    });
    match (watching, &lane.outbox) {
        (true, _) => out.push(Check::ok(a, "rail", "misaka-palw-fp-rail --watch is running on this outbox")),
        (false, Some(o)) => out.push(Check::found(a, "rail", "not running", catalog::rail_down(&o.display().to_string()))),
        (false, None) => out.push(Check::skip(a, "rail", "no outbox to watch")),
    }
    if let Some(worker) = &lane.worker
        && !worker.is_absolute()
    {
        out.push(Check::found(
            a,
            "worker",
            worker.display().to_string(),
            Finding::error("E-PROMPT-WORKER-PATH", crate::exit::CONFIG, "The worker path is not absolute")
                .reason("the gateway spawns the worker from its own working directory; a relative path finds nothing")
                .current(worker.display().to_string())
                .fix("give the worker's absolute path"),
        ));
    }
}

/// `GET http://<hostport><path>` → the status code. A few lines of HTTP/1.1 over a socket, the way
/// `eth.rs` speaks to the EVM RPC: no HTTP client in this crate.
pub(crate) fn http_get_status(hostport: &str, path: &str, timeout: Duration) -> Result<u16, String> {
    use std::net::ToSocketAddrs;
    let addr = hostport.to_socket_addrs().map_err(|e| e.to_string())?.next().ok_or("no address")?;
    let mut stream = std::net::TcpStream::connect_timeout(&addr, timeout).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();
    write!(stream, "GET {path} HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\n\r\n").map_err(|e| e.to_string())?;
    let mut head = [0u8; 64];
    let n = stream.read(&mut head).map_err(|e| e.to_string())?;
    let line = String::from_utf8_lossy(&head[..n]);
    line.split_whitespace().nth(1).and_then(|c| c.parse().ok()).ok_or_else(|| format!("not an HTTP answer: {line}"))
}

fn short(id: &str) -> &str {
    crate::operator::work::short_id(id)
}

fn short_op(op: &str) -> String {
    match op.split_once(':') {
        Some((tx, i)) => format!("{}…:{i}", short(tx)),
        None => op.to_string(),
    }
}

/// `misakatest:qz8…4tq` — an address short enough for a table row; JSON carries it whole.
pub(crate) fn short_address(a: &str) -> String {
    match a.split_once(':') {
        Some((prefix, body)) if body.len() > 16 => format!("{prefix}:{}…{}", &body[..6], &body[body.len() - 6..]),
        _ => a.to_string(),
    }
}

fn heights(v: &[u64]) -> String {
    v.iter().map(|h| h.to_string()).collect::<Vec<_>>().join(", ")
}

fn log_where(snap: &Snapshot) -> String {
    match &snap.log {
        Ok(l) if !snap.log_live => {
            let last = l.last_ts.map(|t| format!(", last line {} ago", crate::operator::status::ago(procs::now_unix() as i64 - t)));
            format!("{} is an earlier run's{}", l.path.display(), last.unwrap_or_default())
        }
        Ok(l) if !snap.boot_current => format!("this run's boot has rolled out of {}", l.path.display()),
        Ok(l) => format!("read {}", l.path.display()),
        Err(e) => e.clone(),
    }
}

pub(crate) fn render(snap: &Snapshot, checks: &[Check]) -> String {
    let mut out = String::new();
    let profile = if snap.profile.prompt.is_some() { "mining + prompt lane" } else { "mining" };
    out.push_str(&paint::bold(&format!("misaka doctor · {} · {profile}\n", snap.profile.network)));
    let mut last: Option<Area> = None;
    for c in checks {
        let label = if last != Some(c.area) { c.area.label() } else { "" };
        last = Some(c.area);
        let mark = c.severity.paint(c.severity.mark());
        let code = c.finding.as_ref().map(|f| paint::dim(&format!("   [{}]", f.code))).unwrap_or_default();
        out.push_str(&format!(" {}  {mark} {:<12} {}{code}\n", paint::bold(&format!("{label:<8}")), c.name, c.value));
        if let Some(f) = &c.finding
            && f.severity >= Severity::Warning
        {
            for fix in f.fix.iter().take(2) {
                out.push_str(&format!("{:26}{}  {fix}\n", "", paint::dim("Fix")));
            }
        }
    }
    let n = |s: Severity| checks.iter().filter(|c| c.severity == s).count();
    out.push_str(&format!(
        " {} ok · {} · {} · {} skipped\n",
        n(Severity::Ok),
        paint::yellow(&format!("{} warning(s)", n(Severity::Warning))),
        paint::red(&format!("{} failure(s)", n(Severity::Error))),
        n(Severity::Skip)
    ));
    out
}

/// `misaka doctor [area…] [--strict]`.
pub(crate) async fn run(
    ctx: &crate::node::Ctx,
    profile: crate::operator::profile::Profile,
    areas: &[Area],
    strict: bool,
) -> CliResult {
    let timeout = Duration::from_secs(ctx.timeout_secs.clamp(2, 10));
    let snap = Snapshot::gather(profile, timeout, false).await;
    let rows = checks(&snap, areas).await;
    let findings: Vec<Finding> = rows.iter().filter_map(|c| c.finding.clone()).collect();
    let code = finding::exit_of(&findings, strict);
    if ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema": "misaka.doctor.v1",
                "network": snap.profile.network,
                "ok": code == crate::exit::SUCCESS,
                "exit": code,
                "checks": rows,
            }))
            .expect("serializable")
        );
    } else {
        print!("{}", render(&snap, &rows));
        // The worst failure, whole, so its Reason / Current / Required are on screen too.
        if let Some(f) = findings.iter().filter(|f| f.severity == Severity::Error).find(|f| f.exit == code) {
            println!();
            println!("{}", f.render());
        }
    }
    // Already printed in full; the empty message tells `main` not to print it again.
    if code == crate::exit::SUCCESS { Ok(()) } else { Err(CliError::new(code, String::new())) }
}
