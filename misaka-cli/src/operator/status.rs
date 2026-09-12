//! **`misaka mining status`** — ADR-0122 Decisions 2 and 3: is the miner doing anything, and if
//! not, the one line that says why and the one command that fixes it; then the works, and what
//! they have paid.
//!
//! The miner's own state (STOPPED → STARTING → SYNCING → HOLDING ⇄ DRAWING) comes from
//! [`miner_state`], a pure function of what was read, so every hold can be pinned without a node.
//! "Drawing" is claimed only on evidence: a draw report or a produced block in the node's log. A
//! node that is ready and whose log this host cannot read is shown as `READY`, not as drawing.

use crate::operator::catalog;
use crate::operator::finding::{Finding, Severity, paint};
use crate::operator::host::human_bytes;
use crate::operator::nodelog::NodeLog;
use crate::operator::procs;
use crate::operator::snapshot::{Snapshot, WorkRow};
use crate::operator::work::{self, Outcome, WorkState};
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_rpc_core::GetPalwProducerFactsResponse;
use serde::Serialize;
use serde_json::json;

/// The miner's state (ADR-0122 Decision 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum MinerState {
    NotSetUp,
    Ambiguous,
    Stopped,
    NotProducing,
    Disabled,
    Unreachable,
    Starting,
    Syncing,
    Holding,
    Stopping,
    Ready,
    Drawing,
}

impl MinerState {
    fn mark(self) -> &'static str {
        match self {
            MinerState::Drawing => "●",
            MinerState::Ready | MinerState::Starting | MinerState::Syncing | MinerState::Holding | MinerState::Stopping => "◐",
            MinerState::NotProducing | MinerState::NotSetUp => "○",
            MinerState::Stopped | MinerState::Disabled | MinerState::Unreachable | MinerState::Ambiguous => "✗",
        }
    }

    fn word(self) -> &'static str {
        match self {
            MinerState::NotSetUp => "NOT SET UP",
            MinerState::Ambiguous => "WHICH NODE?",
            MinerState::Stopped => "STOPPED",
            MinerState::NotProducing => "NOT PRODUCING",
            MinerState::Disabled => "DISABLED",
            MinerState::Unreachable => "UNREACHABLE",
            MinerState::Starting => "STARTING",
            MinerState::Syncing => "SYNCING",
            MinerState::Holding => "HOLDING",
            MinerState::Stopping => "STOPPING",
            MinerState::Ready => "READY",
            MinerState::Drawing => "MINING",
        }
    }

    fn paint(self, s: &str) -> String {
        match self {
            MinerState::Drawing => paint::green(s),
            MinerState::Ready | MinerState::Starting | MinerState::Syncing | MinerState::Holding | MinerState::Stopping => {
                paint::yellow(s)
            }
            MinerState::NotProducing | MinerState::NotSetUp => paint::dim(s),
            _ => paint::red(s),
        }
    }
}

/// What the node answered, for [`miner_state`].
#[derive(Clone, Debug)]
pub(crate) enum NodeFacts {
    Reached { network_ok: bool, node_network: String, synced: bool, peers: Option<usize>, daa: u64 },
    Unreachable { url: String, why: String },
}

/// Everything [`miner_state`] decides on.
pub(crate) struct MinerInputs<'a> {
    pub(crate) configured: bool,
    pub(crate) ambiguous: &'a [u32],
    pub(crate) network: &'a str,
    pub(crate) appdir: &'a str,
    /// `Some(produce)` when this host runs the node; its uptime in seconds.
    pub(crate) process: Option<(bool, u64)>,
    pub(crate) borsh_flag: Option<&'a str>,
    pub(crate) node: NodeFacts,
    pub(crate) log: Option<&'a NodeLog>,
    pub(crate) facts: Option<&'a GetPalwProducerFactsResponse>,
    /// `getPalwNodeStatus` from a node that serves it: the producer's own state, which outranks
    /// the log and works from another host.
    pub(crate) runtime: Option<&'a kaspa_rpc_core::GetPalwNodeStatusResponse>,
    /// `(this key, the bond's key)` when they differ.
    pub(crate) key_mismatch: Option<(String, String)>,
    pub(crate) bond: Option<&'a str>,
    /// The node was started with `--enable-unsynced-mining`.
    pub(crate) unsynced_mining: bool,
    pub(crate) now_unix: i64,
}

/// The state, the headline and — when it is not mining — the finding that says why.
#[derive(Clone, Debug)]
pub(crate) struct MinerView {
    pub(crate) state: MinerState,
    pub(crate) headline: String,
    pub(crate) finding: Option<Finding>,
    /// Where the verdict came from: `rpc`, `log`, `process`.
    pub(crate) basis: &'static str,
}

/// How recent a draw report or a block must be to count as "drawing now": the producer reports
/// every five minutes while it draws, so six minutes without one means it is not.
const DRAWING_WITHIN_SECS: i64 = 6 * 60;

/// **The miner's state, as a pure function** — checked in the order a fault shadows the next: a
/// node that is not running cannot be syncing, a node that is syncing cannot be holding on its
/// bond, and a bond that is not ready cannot be drawing whatever the log last said.
pub(crate) fn miner_state(i: &MinerInputs<'_>) -> MinerView {
    let view = |state, headline: String, finding: Option<Finding>, basis| MinerView { state, headline, finding, basis };
    if !i.ambiguous.is_empty() {
        let f = catalog::several_nodes(i.network, i.ambiguous);
        return view(MinerState::Ambiguous, f.title.clone(), Some(f), "process");
    }
    let Some((produce, uptime)) = i.process else {
        if !i.configured {
            let f = catalog::not_set_up();
            return view(MinerState::NotSetUp, f.title.clone(), Some(f), "process");
        }
        // A node elsewhere (`--rpc` at another host) can still be read; a node that is simply not
        // running here cannot.
        if let NodeFacts::Unreachable { .. } = &i.node {
            let f = catalog::node_down(i.network, i.appdir);
            return view(MinerState::Stopped, f.title.clone(), Some(f), "process");
        }
        return from_node(i, true);
    };
    if !produce {
        let f = catalog::not_producing();
        return view(MinerState::NotProducing, "this node verifies and relays; it does not produce".to_string(), Some(f), "process");
    }
    if let Some(rt) = i.runtime.filter(|rt| rt.producer_state == "disabled") {
        let f = catalog::producer_disabled(&rt.producer_reason);
        return view(MinerState::Disabled, f.title.clone(), Some(f), "rpc");
    }
    if let Some((_, text)) = i.log.and_then(|l| l.producer_disabled.as_ref()) {
        let f = catalog::producer_disabled(text);
        return view(MinerState::Disabled, f.title.clone(), Some(f), "log");
    }
    if let NodeFacts::Unreachable { url, why } = &i.node {
        let loading = i.log.and_then(|l| l.last_loading.as_ref()).filter(|(ts, _)| i.now_unix - ts < 15 * 60);
        if uptime < 10 * 60 || loading.is_some() {
            let what = match loading {
                Some((_, line)) => line.trim_start_matches("loading class artifact ").to_string(),
                None => format!("the node started {} ago and is not answering yet", ago(uptime as i64)),
            };
            return view(MinerState::Starting, format!("starting — {what}"), None, "process");
        }
        let f = catalog::rpc_unreachable(url, why, i.borsh_flag);
        return view(MinerState::Unreachable, f.title.clone(), Some(f), "process");
    }
    from_node(i, false)
}

/// The half of [`miner_state`] that has a node to read.
fn from_node(i: &MinerInputs<'_>, remote: bool) -> MinerView {
    let view = |state, headline: String, finding: Option<Finding>, basis| MinerView { state, headline, finding, basis };
    let NodeFacts::Reached { network_ok, node_network, synced, peers, daa } = &i.node else {
        unreachable!("the caller handled an unreachable node")
    };
    if !network_ok {
        let f = catalog::network_mismatch(node_network, i.network);
        return view(MinerState::Unreachable, f.title.clone(), Some(f), "rpc");
    }
    // The producer's own gate is not `is_synced`: with --enable-unsynced-mining, peers and open
    // participation it draws on a chain whose tip is older than the window (a fresh chain's first
    // blocks). A log showing it drawing outranks the RPC's word for the chain.
    let drawing_now =
        i.runtime.is_some_and(|rt| rt.producer_state == "drawing" && i.now_unix - (rt.last_draw_unix as i64) < DRAWING_WITHIN_SECS)
            || i.log.is_some_and(|l| {
                l.last_draws.as_ref().is_some_and(|(ts, _)| i.now_unix - ts < DRAWING_WITHIN_SECS)
                    || l.produced.last().is_some_and(|(ts, _, _)| i.now_unix - ts < DRAWING_WITHIN_SECS)
            });
    let waived = drawing_now || (i.unsynced_mining && peers.is_some_and(|p| p > 0));
    if !(*synced || waived) {
        let f = catalog::not_synced(*daa);
        return view(MinerState::Syncing, format!("virtual DAA {}, not synced yet", group(*daa)), Some(f), "rpc");
    }
    if *peers == Some(0) {
        let f = catalog::no_peers();
        return view(MinerState::Holding, f.title.clone(), Some(f), "rpc");
    }
    if let Some((local, registered)) = &i.key_mismatch {
        let f = catalog::key_mismatch(i.bond.unwrap_or("the bond"), Some(local), Some(registered));
        return view(MinerState::Holding, f.title.clone(), Some(f), "rpc");
    }
    if let Some(facts) = i.facts
        && !facts.not_ready_reason.is_empty()
    {
        let n = catalog::HoldNumbers {
            epoch: Some(facts.epoch_index),
            produced: Some(facts.epoch_produced_blocks),
            budget: Some(facts.epoch_budget_blocks),
            reserved: facts.bond_reserved_exposure.parse().ok(),
            ceiling: facts.bond_exposure_ceiling.parse().ok(),
            per_claim: facts.bond_claim_exposure.parse().ok(),
        };
        let f = catalog::not_ready(&facts.not_ready_reason, &n, i.bond);
        return view(MinerState::Holding, f.title.clone(), Some(f), "rpc");
    }
    // The node's own account of its producer (ADR-0122 §6.5): the same sentences its log prints,
    // read over RPC, from this host or another.
    if let Some(rt) = i.runtime {
        return match rt.producer_state.as_str() {
            "holding" => {
                let f = catalog::hold_from_log(&rt.producer_reason, i.bond);
                view(MinerState::Holding, f.title.clone(), Some(f), "rpc")
            }
            "drawing" => view(MinerState::Drawing, runtime_headline(rt, i.facts, i.now_unix), None, "rpc"),
            "stopped" => view(MinerState::Stopping, "stopping — the producer loop has exited".to_string(), None, "rpc"),
            "off" => {
                let f = catalog::not_producing();
                view(MinerState::NotProducing, "this node verifies and relays; it does not produce".to_string(), Some(f), "rpc")
            }
            // `syncing` with the loop not yet round once, or a state this build cannot name.
            other => view(MinerState::Starting, format!("the producer is starting ({other}: {})", rt.producer_reason), None, "rpc"),
        };
    }
    let Some(log) = i.log else {
        let why = if remote { "this node runs on another host" } else { "this host cannot read the node's log" };
        return view(MinerState::Ready, format!("ready to produce — {why}, so its draws cannot be seen"), None, "rpc");
    };
    if let Some((_, detail)) = &log.last_hold {
        let f = catalog::hold_from_log(detail, i.bond);
        return view(MinerState::Holding, f.title.clone(), Some(f), "log");
    }
    if log.producer_stopped.is_some_and(|stopped| log.producer_started.is_none_or(|started| stopped >= started)) {
        return view(MinerState::Stopping, "stopping — the producer loop has exited".to_string(), None, "log");
    }
    let recent_draw = log.last_draws.as_ref().filter(|(ts, _)| i.now_unix - ts < DRAWING_WITHIN_SECS);
    let recent_block = log.produced.last().filter(|(ts, _, _)| i.now_unix - ts < DRAWING_WITHIN_SECS);
    if recent_draw.is_some() || recent_block.is_some() {
        return view(MinerState::Drawing, drawing_headline(log, i.now_unix), None, "log");
    }
    if let Some(started) = log.producer_started.filter(|ts| i.now_unix - ts < 30 * 60) {
        return view(
            MinerState::Starting,
            format!("the producer started {} ago; its first draw is still running", ago(i.now_unix - started)),
            None,
            "log",
        );
    }
    let quiet = log.last_ts.map(|ts| i.now_unix - ts);
    let headline = match quiet {
        Some(q) if q > 15 * 60 => format!("ready to produce, but the node's log has been silent for {}", ago(q)),
        _ => "ready to produce, but no draw report in the last few minutes".to_string(),
    };
    view(MinerState::Ready, headline, None, "rpc")
}

/// The drawing headline from the node's runtime, with the class ticket's odds from the facts.
fn runtime_headline(rt: &kaspa_rpc_core::GetPalwNodeStatusResponse, facts: Option<&GetPalwProducerFactsResponse>, now: i64) -> String {
    let mut parts = vec!["drawing".to_string()];
    if let Some(p) = facts.and_then(|f| f.class_target.parse::<u128>().ok()).map(|t| t as f64 / u128::MAX as f64).filter(|p| *p > 0.0)
    {
        parts.push(format!("1 in {} per draw", compact(1.0 / p)));
    }
    parts.push(format!("{} draw{} this run", group(rt.draws), if rt.draws == 1 { "" } else { "s" }));
    if rt.last_block_unix > 0 {
        parts.push(format!("last block {} ago", ago(now - rt.last_block_unix as i64)));
    } else {
        parts.push("no block yet this run".to_string());
    }
    parts.join(" · ")
}

fn drawing_headline(log: &NodeLog, now: i64) -> String {
    let mut parts = vec!["drawing".to_string()];
    if let Some((_, d)) = &log.last_draws {
        if let Some(p) = d.class_p.filter(|p| *p > 0.0) {
            parts.push(format!("1 in {} per draw", compact(1.0 / p)));
        }
        parts.push(format!("{} draw{} by the last report", group(d.draws), if d.draws == 1 { "" } else { "s" }));
    }
    match log.produced.last() {
        Some((ts, _, _)) => parts.push(format!("last block {} ago", ago(now - ts))),
        None => parts.push("no block yet in this log".to_string()),
    }
    parts.join(" · ")
}

/// `3.4 k`, `1.2 M` — an odds figure at a glance.
fn compact(v: f64) -> String {
    if v >= 1e9 {
        format!("{:.1} G", v / 1e9)
    } else if v >= 1e6 {
        format!("{:.1} M", v / 1e6)
    } else if v >= 1e3 {
        format!("{:.1} k", v / 1e3)
    } else {
        format!("{v:.0}")
    }
}

pub(crate) fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `12 s`, `12 m`, `2 h 13 m`, `3 d 4 h`.
pub(crate) fn ago(secs: i64) -> String {
    let s = secs.max(0);
    match s {
        0..60 => format!("{s} s"),
        60..3600 => format!("{} m", s / 60),
        3600..86_400 => format!("{} h {} m", s / 3600, (s % 3600) / 60),
        _ => format!("{} d {} h", s / 86_400, (s % 86_400) / 3600),
    }
}

/// The counts the header prints. Only `final` is mined (ADR-0122 §2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub(crate) struct Counts {
    pub(crate) computed: usize,
    pub(crate) submitted: usize,
    pub(crate) accepted: usize,
    pub(crate) failed: usize,
}

pub(crate) fn counts(works: &[WorkRow]) -> Counts {
    let mut c = Counts::default();
    for w in works {
        let s = w.reading.state;
        c.computed += usize::from(s.was_computed());
        c.submitted += usize::from(s.was_submitted());
        c.accepted += usize::from(s.outcome() == Outcome::Mined);
        c.failed += usize::from(s.outcome() == Outcome::Failed);
    }
    c
}

/// Build the inputs from a snapshot.
pub(crate) fn inputs<'a>(snap: &'a Snapshot, now_unix: i64) -> MinerInputs<'a> {
    let p = &snap.profile;
    let node = match &snap.node {
        Ok(n) => NodeFacts::Reached {
            network_ok: n.server.network_id.to_string() == p.network,
            node_network: n.server.network_id.to_string(),
            synced: n.server.is_synced,
            peers: n.peers.as_ref().map(|v| v.len()),
            daa: n.daa(),
        },
        Err((url, why)) => NodeFacts::Unreachable { url: url.clone(), why: why.clone() },
    };
    let facts = snap.facts.as_ref().and_then(|f| f.as_ref().ok());
    let key_mismatch = match (snap.key.as_ref().and_then(|k| k.as_ref().ok()), facts) {
        (Some(k), Some(f)) if f.bond_known && !f.bond_registered_pubkey.eq_ignore_ascii_case(&k.pubkey_hex) => {
            Some((k.pubkey_hex.clone(), f.bond_registered_pubkey.clone()))
        }
        _ => None,
    };
    MinerInputs {
        configured: p.is_configured(),
        ambiguous: &p.ambiguous_kaspads,
        network: &p.network,
        appdir: p.appdir.to_str().unwrap_or("?"),
        process: p.kaspad.as_ref().map(|(proc_, args)| (args.produce, proc_.uptime_secs())),
        borsh_flag: p.kaspad.as_ref().and_then(|(_, a)| a.rpclisten_borsh.as_deref()),
        node,
        // Only a log the running node is writing speaks for it: a previous run's hold is not this
        // run's, and a log that is not there is not a silent one.
        log: snap.live_log(),
        facts,
        runtime: snap.node_status.as_ref(),
        key_mismatch,
        bond: p.bond.as_deref(),
        unsynced_mining: p.kaspad.as_ref().is_some_and(|(_, a)| a.enable_unsynced_mining),
        now_unix,
    }
}

/// The expected fingerprint and heights for this CLI's build of `network`.
pub(crate) fn expected(network: &str) -> Option<(String, Vec<u64>)> {
    // (A node started with a ruleset flag runs its own parameters; callers check
    // `KaspadArgs::ruleset_flags` before comparing against this.)
    let net = network.parse::<kaspa_consensus_core::network::NetworkId>().ok()?;
    let params = kaspa_consensus_core::config::params::Params::from(net);
    Some((params.consensus_params_id().to_string(), params.fence_schedule_v1()))
}

/// `✓`, `✗ (node X)`, or `? (not in the log)` for the fingerprint and the schedule.
fn fork_marks(snap: &Snapshot) -> (String, String) {
    if let Some(flag) = snap.profile.kaspad.as_ref().and_then(|(_, a)| a.ruleset_flags.first()) {
        let node = snap.boot_log().and_then(|l| l.fingerprint.as_deref()).map(work::short_id).unwrap_or("?");
        return (format!("{node} (its own ruleset: {flag})"), "—".to_string());
    }
    let Some((fp, heights)) = expected(&snap.profile.network) else { return ("?".into(), "?".into()) };
    if let Some(rt) = &snap.node_status {
        let f = if rt.consensus_params_id == fp {
            format!("{} {}", work::short_id(&rt.consensus_params_id), paint::green("✓"))
        } else {
            format!("{} {} (this CLI {})", work::short_id(&rt.consensus_params_id), paint::red("✗"), work::short_id(&fp))
        };
        let s = if rt.fence_schedule == heights { paint::green("✓") } else { paint::red("✗") };
        return (f, s);
    }
    let log = snap.boot_log();
    let f = match log.and_then(|l| l.fingerprint.as_deref()) {
        Some(node) if node == fp => format!("{} {}", work::short_id(node), paint::green("✓")),
        Some(node) => format!("{} {} (this CLI {})", work::short_id(node), paint::red("✗"), work::short_id(&fp)),
        None => format!("{} (not in the node's log)", work::short_id(&fp)),
    };
    let s = match log.and_then(|l| l.schedule.as_ref()) {
        Some(node) if *node == heights => paint::green("✓"),
        Some(_) => paint::red("✗"),
        None => "?".to_string(),
    };
    (f, s)
}

/// **The whole screen**, as ADR-0122 §11 draws it.
pub(crate) fn render(snap: &Snapshot, view: &MinerView, now_unix: i64) -> String {
    let p = &snap.profile;
    let mut out = String::new();
    let version = snap.node.as_ref().map(|n| format!("kaspad {}", n.server.server_version)).unwrap_or_default();
    let pid = p.kaspad.as_ref().map(|(proc_, _)| format!("pid {}", proc_.pid)).unwrap_or_default();
    let right = [version, pid].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
    let whence = match p.network_source {
        crate::operator::profile::Source::Default => " (no network named: set [mining] network, or pass --network)",
        _ => "",
    };
    out.push_str(&format!(
        "{}{}  {}\n",
        paint::bold(&format!("MISAKA mining · {}", p.network)),
        paint::dim(whence),
        paint::dim(&right)
    ));
    out.push_str(&paint::dim(&"─".repeat(78)));
    out.push('\n');
    let head = format!("{} {} — {}", view.state.mark(), view.state.word(), view.headline);
    out.push_str(&view.state.paint(&head));
    out.push('\n');
    if let Some(f) = &view.finding {
        // The finding repeats the title; print the five fields under the state line instead.
        let block = f.render();
        for line in block.lines().skip(1) {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push('\n');

    // Node
    match &snap.node {
        Ok(n) => {
            let (fp, sched) = fork_marks(snap);
            let peers = match (n.peers.as_ref(), n.outbound_peers()) {
                (Some(all), Some(out)) => format!("{} peer{} ({} out)", all.len(), if all.len() == 1 { "" } else { "s" }, out),
                _ => "peers ?".to_string(),
            };
            let sync = if n.server.is_synced { "synced" } else { "syncing" };
            out.push_str(&format!(
                "  {}  {sync} · DAA {} · {peers} · fingerprint {fp} · schedule {sched}\n",
                paint::bold("Node   "),
                group(n.daa())
            ));
        }
        Err((url, why)) => out.push_str(&format!("  {}  not answering at {url} ({why})\n", paint::bold("Node   "))),
    }
    // Bond
    match (&p.bond, snap.facts.as_ref()) {
        (Some(bond), Some(Ok(f))) if f.bond_known => {
            let reserved = f.bond_reserved_exposure.parse::<u128>().unwrap_or(0);
            let ceiling = f.bond_exposure_ceiling.parse::<u128>().unwrap_or(0);
            out.push_str(&format!(
                "  {}  {} · collateral {} · exposure {} / {}\n",
                paint::bold("Bond   "),
                short_outpoint(bond),
                catalog::msk(f.bond_collateral as u128),
                catalog::msk(reserved),
                catalog::msk(ceiling)
            ));
        }
        (Some(bond), Some(Ok(_))) => {
            out.push_str(&format!("  {}  {} · not registered on this chain\n", paint::bold("Bond   "), short_outpoint(bond)))
        }
        (Some(bond), Some(Err(e))) => out.push_str(&format!("  {}  {} · {e}\n", paint::bold("Bond   "), short_outpoint(bond))),
        (Some(bond), None) => out.push_str(&format!("  {}  {}\n", paint::bold("Bond   "), short_outpoint(bond))),
        (None, _) => out.push_str(&format!("  {}  none named (--palw-producer-bond / [advanced] bond)\n", paint::bold("Bond   "))),
    }
    // Class
    if let Some(class) = &snap.class_id {
        let base = if snap.class_is_base { " (the base class)" } else { "" };
        let budget = match snap.facts.as_ref().and_then(|f| f.as_ref().ok()) {
            Some(f) if f.available && f.is_base_class => {
                format!(" · epoch {}: the class has made {} blocks (the floor has no cap)", f.epoch_index, f.epoch_produced_blocks)
            }
            Some(f) if f.available => {
                format!(
                    " · epoch {}: the class has made {} of its {} blocks",
                    f.epoch_index, f.epoch_produced_blocks, f.epoch_budget_blocks
                )
            }
            _ => String::new(),
        };
        out.push_str(&format!("  {}  {}…{base}{budget}\n", paint::bold("Class  "), work::short_id(class)));
    }
    // Pays to
    let key_address = snap.key.as_ref().and_then(|k| k.as_ref().ok()).map(|k| k.address.clone());
    match (&p.pay_address, &key_address) {
        (Some(pay), Some(k)) if pay == k => out.push_str(&format!(
            "  {}  {} (the key's own address)\n",
            paint::bold("Pays to"),
            crate::operator::doctor::short_address(pay)
        )),
        (Some(pay), _) => out.push_str(&format!("  {}  {}\n", paint::bold("Pays to"), crate::operator::doctor::short_address(pay))),
        (None, Some(k)) => out.push_str(&format!(
            "  {}  {} (the key's own address)\n",
            paint::bold("Pays to"),
            crate::operator::doctor::short_address(k)
        )),
        (None, None) => {}
    }
    if let Some(Err(e)) = &snap.key {
        out.push_str(&format!("  {}  {}\n", paint::bold("Key    "), paint::yellow(&format!("unreadable — {e}"))));
    }
    // Prompt lane
    match &p.prompt {
        None => out.push_str(&format!("  {}  off\n", paint::bold("Prompt "))),
        Some(lane) => {
            let gateways = procs::find(procs::Component::Gateway).len();
            let rails = procs::find(procs::Component::Rail).len();
            let outbox = lane.outbox.as_ref().map(|o| o.display().to_string()).unwrap_or_else(|| "no outbox named".into());
            let up = |n: usize| if n > 0 { paint::green("up") } else { paint::red("down") };
            out.push_str(&format!("  {}  outbox {outbox} · gateway {} · rail {}\n", paint::bold("Prompt "), up(gateways), up(rails)));
        }
    }
    out.push('\n');

    // Works
    let c = counts(&snap.works);
    out.push_str(&format!(
        "  {}   computed {} · submitted {} · accepted {} · failed {}\n",
        paint::bold(&format!("WORK · {} (from the {})", snap.works.len(), snap.works_source)),
        c.computed,
        c.submitted,
        c.accepted,
        c.failed
    ));
    if snap.works.is_empty() {
        let why = match (&snap.log, &snap.node) {
            (_, Err(_)) => "the node is not answering, so no work can be followed",
            (Err(_), _) => "the node's log cannot be read here, and no outbox is named",
            _ => "none yet: no produced block in the log, and no prompt-lane job in the outbox",
        };
        out.push_str(&format!("  {}\n", paint::dim(why)));
    } else {
        out.push_str(&paint::dim(&format!("  {:<12}{:<8}{:<19}{:<48}{}\n", "ID", "LANE", "STAGE", "DETAIL", "AGE")));
        for w in snap.works.iter().take(12) {
            let age = w.seen_ts.map(|t| ago(now_unix - t)).unwrap_or_default();
            let stage = w.reading.state.name();
            let stage = paint_state(w.reading.state, &format!("{stage:<19}"));
            let mut detail = w.reading.detail.clone();
            if detail.chars().count() > 46 {
                detail = detail.chars().take(45).collect::<String>() + "…";
            }
            out.push_str(&format!(
                "  {}{:<8}{stage}{detail:<48}{age}\n",
                paint::cyan(&format!("{:<12}", w.display_id())),
                w.lane.name()
            ));
        }
        if snap.works.len() > 12 {
            out.push_str(&paint::dim(&format!("  … {} more: misaka work list\n", snap.works.len() - 12)));
        }
    }
    for e in snap.work_errors.iter().take(3) {
        out.push_str(&paint::dim(&format!("  (could not follow {e})\n")));
    }
    out.push('\n');

    // Rewards
    match &snap.wallet {
        Some(Ok(w)) => {
            let next = w.next_mature_daa.map(|d| format!(" (next spendable at DAA {})", group(d))).unwrap_or_default();
            out.push_str(&format!(
                "  {}  spendable {} · maturing {}{next}\n",
                paint::bold("REWARDS"),
                catalog::msk(w.spendable_sompi as u128),
                catalog::msk(w.maturing_sompi as u128)
            ));
        }
        Some(Err(e)) => out.push_str(&format!("  {}  {}\n", paint::bold("REWARDS"), paint::dim(&format!("not read: {e}")))),
        None => {}
    }
    out.push_str(&format!("  {}     {}\n", paint::bold("NEXT"), next_step(view)));
    out
}

fn paint_state(s: WorkState, text: &str) -> String {
    match s.outcome() {
        Outcome::Mined => paint::green(text),
        Outcome::Failed => paint::red(text),
        Outcome::InFlight | Outcome::Paused => paint::yellow(text),
        Outcome::Unknown => paint::dim(text),
    }
}

fn short_outpoint(op: &str) -> String {
    match op.split_once(':') {
        Some((tx, i)) => format!("{}…:{i}", work::short_id(tx)),
        None => op.to_string(),
    }
}

fn next_step(view: &MinerView) -> String {
    match (&view.state, &view.finding) {
        (MinerState::Drawing, _) => "nothing to do — the miner is drawing.".to_string(),
        (MinerState::Starting, _) => "wait: the node is starting (run this again in a minute).".to_string(),
        (MinerState::Stopping, _) if view.basis == "supervisor" => {
            "nothing to do: the supervisor exits by itself when the last claim ends (misaka mining stop --force to stop now)"
                .to_string()
        }
        (MinerState::Syncing, _) => "wait: the node mines by itself once it is synced.".to_string(),
        (_, Some(f)) if f.severity == Severity::Error || f.severity == Severity::Warning => {
            f.fix.first().cloned().unwrap_or_else(|| "misaka doctor".to_string())
        }
        _ => "misaka doctor   (checks everything a miner needs)".to_string(),
    }
}

/// The JSON document (`misaka.mining.status.v1`) the dashboard and the Studio read.
pub(crate) fn document(snap: &Snapshot, view: &MinerView) -> serde_json::Value {
    let p = &snap.profile;
    let works: Vec<serde_json::Value> = snap
        .works
        .iter()
        .map(|w| {
            json!({
                "id": w.claim_id,
                "job": w.job,
                "lane": w.lane,
                "state": w.reading.state,
                "detail": w.reading.detail,
                "deadline_daa": w.reading.deadline_daa,
                "estimated": w.reading.estimated,
                "seen_unix": w.seen_ts,
                "block": w.block,
            })
        })
        .collect();
    let node = match &snap.node {
        Ok(n) => json!({
            "reached": true,
            "url": n.url,
            "network": n.server.network_id.to_string(),
            "version": n.server.server_version,
            "synced": n.server.is_synced,
            "daa": n.daa(),
            "peers": n.peers.as_ref().map(|v| v.len()),
            "outbound_peers": n.outbound_peers(),
            "utxoindex": n.server.has_utxo_index,
        }),
        Err((url, why)) => json!({ "reached": false, "url": url, "error": why }),
    };
    let log = snap.boot_log();
    json!({
        "schema": "misaka.mining.status.v1",
        "network": p.network,
        "miner": {
            "state": view.state,
            "headline": view.headline,
            "basis": view.basis,
            "finding": view.finding,
        },
        "node": node,
        "fork": {
            "node_fingerprint": snap.node_status.as_ref().map(|r| r.consensus_params_id.clone()).or_else(|| log.and_then(|l| l.fingerprint.clone())),
            "node_schedule": snap.node_status.as_ref().map(|r| r.fence_schedule.clone()).or_else(|| log.and_then(|l| l.schedule.clone())),
            "cli_fingerprint": expected(&p.network).map(|e| e.0),
            "cli_schedule": expected(&p.network).map(|e| e.1),
        },
        "bond": p.bond,
        "class_id": snap.class_id,
        "facts": snap.facts.as_ref().and_then(|f| f.as_ref().ok()).map(|f| json!({
            "bond_known": f.bond_known,
            "collateral_sompi": f.bond_collateral,
            "reserved_exposure_sompi": f.bond_reserved_exposure,
            "exposure_ceiling_sompi": f.bond_exposure_ceiling,
            "claim_exposure_sompi": f.bond_claim_exposure,
            "epoch_index": f.epoch_index,
            "epoch_budget_blocks": f.epoch_budget_blocks,
            "epoch_produced_blocks": f.epoch_produced_blocks,
            "not_ready_reason": f.not_ready_reason,
            "fp_certified": f.fp_certified,
        })),
        "pay_address": p.pay_address.clone().or_else(|| snap.key.as_ref().and_then(|k| k.as_ref().ok()).map(|k| k.address.clone())),
        "wallet": snap.wallet.as_ref().and_then(|w| w.as_ref().ok()),
        "counts": counts(&snap.works),
        "works_source": snap.works_source,
        "works": works,
        "runtime": snap.node_status.as_ref().map(|r| json!({
            "producer_state": r.producer_state,
            "producer_reason": r.producer_reason,
            "producer_since_unix": r.producer_since_unix,
            "draws": r.draws,
            "produced_blocks": r.produced_blocks,
            "receipt_blocks": r.receipt_blocks,
            "last_block": r.last_block,
            "last_block_unix": r.last_block_unix,
            "panel_running": r.panel_running,
            "panel_submitter": r.panel_submitter,
        })),
        "next": next_step(view),
        "sources": {
            "network": p.network_source,
            "bond": p.bond_source,
            "stop_grace_secs": p.stop_grace_secs,
            "config": p.config_path.as_ref().map(|c| c.display().to_string()),
            "kaspad_pid": p.kaspad.as_ref().map(|(proc_, _)| proc_.pid),
            "log": snap.log.as_ref().map(|l| l.path.display().to_string()).ok(),
        },
    })
}

/// `misaka mining status [--watch <secs>]`.
pub(crate) async fn run(ctx: &crate::node::Ctx, profile: crate::operator::profile::Profile, watch: Option<u64>) -> CliResult {
    let timeout = std::time::Duration::from_secs(ctx.timeout_secs.clamp(2, 10));
    loop {
        let snap = Snapshot::gather(profile.clone(), timeout, true).await;
        let now = procs::now_unix() as i64;
        let mut view = miner_state(&inputs(&snap, now));
        // A supervisor that is draining says so: the node runs as a panel only on purpose, and
        // "not producing" would read as a fault.
        if let Some(sup) = crate::operator::supervisor::running_supervisor(&snap.profile.network)
            && sup.phase == "draining"
        {
            view = MinerView {
                state: MinerState::Stopping,
                headline: format!("draining (supervisor pid {}) — {}", sup.supervisor_pid, sup.message),
                finding: None,
                basis: "supervisor",
            };
        }
        if ctx.output == OutputFormat::Json {
            println!("{}", serde_json::to_string_pretty(&document(&snap, &view)).expect("serializable"));
        } else {
            if watch.is_some() {
                // Home and clear, so the screen redraws in place.
                print!("\x1b[H\x1b[2J");
            }
            print!("{}", render(&snap, &view, now));
            if let Ok(log) = &snap.log
                && !log.whole_file
                && log.produced.is_empty()
                && view.state == MinerState::Drawing
            {
                println!("  {}", paint::dim(&format!("(read the last {} of {})", human_bytes(log.bytes_read), log.path.display())));
            }
        }
        match watch {
            Some(secs) => tokio::time::sleep(std::time::Duration::from_secs(secs.max(2))).await,
            None => {
                return match view.state {
                    MinerState::Drawing | MinerState::Ready | MinerState::NotProducing => Ok(()),
                    // The screen said why; the empty message tells `main` not to say it again.
                    _ => Err(CliError::new(view.finding.as_ref().map(|f| f.exit).unwrap_or(exit::NOT_READY), String::new())),
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::nodelog::{Draws, NodeLog};

    const NOW: i64 = 1_789_200_000;

    fn reached() -> NodeFacts {
        NodeFacts::Reached { network_ok: true, node_network: "testnet-11".into(), synced: true, peers: Some(8), daa: 7_412 }
    }

    fn base<'a>(log: Option<&'a NodeLog>, facts: Option<&'a GetPalwProducerFactsResponse>) -> MinerInputs<'a> {
        MinerInputs {
            configured: true,
            ambiguous: &[],
            network: "testnet-11",
            appdir: "/root/.t11",
            process: Some((true, 3600)),
            borsh_flag: Some("default"),
            node: reached(),
            log,
            facts,
            runtime: None,
            key_mismatch: None,
            bond: Some("aa:0"),
            unsynced_mining: false,
            now_unix: NOW,
        }
    }

    fn drawing_log() -> NodeLog {
        NodeLog {
            producer_started: Some(NOW - 3000),
            last_draws: Some((NOW - 60, Draws { draws: 41, produced: 0, network_lost: 0, class_p: Some(2.9e-4) })),
            last_ts: Some(NOW - 5),
            ..Default::default()
        }
    }

    /// Drawing is claimed on evidence — a recent draw report — and on nothing else.
    #[test]
    fn drawing_is_claimed_on_evidence() {
        let log = drawing_log();
        let v = miner_state(&base(Some(&log), None));
        assert_eq!(v.state, MinerState::Drawing, "{}", v.headline);
        assert!(v.headline.contains("1 in 3.4 k per draw"), "{}", v.headline);
        let stale = NodeLog { last_draws: Some((NOW - 3600, Draws { draws: 1, produced: 0, network_lost: 0, class_p: None })), ..log };
        let stale = NodeLog { producer_started: Some(NOW - 7200), ..stale };
        assert_eq!(miner_state(&base(Some(&stale), None)).state, MinerState::Ready, "an hour-old report is not drawing now");
        assert_eq!(miner_state(&base(None, None)).state, MinerState::Ready, "no log to read is not evidence either way");
    }

    /// The order a fault shadows the next: not running, then not answering, then not synced, then
    /// no peers, then the bond.
    #[test]
    fn a_fault_upstream_shadows_every_fault_below_it() {
        let log = drawing_log();
        let not_ready = GetPalwProducerFactsResponse {
            not_ready_reason: kaspa_consensus_core::palw_producer_v2::PALW_NOT_READY_EXPOSURE_FULL_V2.to_string(),
            ..Default::default()
        };
        let mut i = base(Some(&log), Some(&not_ready));
        assert_eq!(miner_state(&i).finding.unwrap().code, "E-BOND-EXPOSURE-FULL");
        i.node = NodeFacts::Reached { network_ok: true, node_network: "testnet-11".into(), synced: true, peers: Some(0), daa: 1 };
        assert_eq!(miner_state(&i).finding.unwrap().code, "E-NET-NO-PEERS");
        i.node = NodeFacts::Reached { network_ok: true, node_network: "testnet-11".into(), synced: false, peers: Some(0), daa: 1 };
        assert_eq!(
            miner_state(&i).finding.unwrap().code,
            "E-NET-NO-PEERS",
            "it drew minutes ago, unsynced; with no peer it holds now"
        );
        let quiet = NodeLog::default();
        let mut j = base(Some(&quiet), Some(&not_ready));
        j.node = NodeFacts::Reached { network_ok: true, node_network: "testnet-11".into(), synced: false, peers: Some(0), daa: 1 };
        assert_eq!(miner_state(&j).state, MinerState::Syncing, "no evidence of drawing: an unsynced node is syncing");
        i.node = NodeFacts::Unreachable { url: "ws://127.0.0.1:27210".into(), why: "refused".into() };
        assert_eq!(miner_state(&i).state, MinerState::Unreachable, "up an hour and not answering is not starting");
        i.process = Some((true, 30));
        assert_eq!(miner_state(&i).state, MinerState::Starting, "thirty seconds up and not answering is starting");
        i.process = None;
        assert_eq!(miner_state(&i).finding.unwrap().code, "E-PROC-NODE-DOWN");
        i.configured = false;
        assert_eq!(miner_state(&i).finding.unwrap().code, "E-CONFIG-NONE");
    }

    /// A fresh chain is "not synced" by the RPC's word, and a producer with --enable-unsynced-mining
    /// draws on it all the same: the log's evidence outranks `is_synced`.
    #[test]
    fn an_unsynced_chain_a_producer_draws_on_is_drawing() {
        let log = drawing_log();
        let mut i = base(Some(&log), None);
        i.node = NodeFacts::Reached { network_ok: true, node_network: "devnet".into(), synced: false, peers: Some(1), daa: 0 };
        assert_eq!(miner_state(&i).state, MinerState::Drawing);
        let quiet = NodeLog::default();
        let mut i = base(Some(&quiet), None);
        i.node = NodeFacts::Reached { network_ok: true, node_network: "devnet".into(), synced: false, peers: Some(1), daa: 0 };
        assert_eq!(miner_state(&i).state, MinerState::Syncing, "no evidence and no flag: it is syncing");
    }

    /// A key that is not the bond's is caught here, because the facts RPC cannot catch it: the node
    /// answers readiness for the bond's own key.
    #[test]
    fn a_foreign_key_is_caught_by_the_cli() {
        let log = drawing_log();
        let mut i = base(Some(&log), None);
        i.key_mismatch = Some(("aa".repeat(8), "bb".repeat(8)));
        assert_eq!(miner_state(&i).finding.unwrap().code, "E-BOND-KEY-MISMATCH");
    }

    /// The log's own hold, and the startup refusal, name themselves.
    #[test]
    fn a_hold_or_a_refusal_in_the_log_names_itself() {
        let hold = NodeLog {
            last_hold: Some((NOW - 30, "the mining rule engine says this node should not mine [enable_unsynced_mining=false peers=true participation_allowed=false]".into())),
            ..drawing_log()
        };
        assert_eq!(miner_state(&base(Some(&hold), None)).finding.unwrap().code, "E-NET-PARTICIPATION");
        let refused = NodeLog {
            producer_disabled: Some((NOW - 30, "pay address is not ML-DSA-87 P2PKH — production disabled".into())),
            ..drawing_log()
        };
        let v = miner_state(&base(Some(&refused), None));
        assert_eq!((v.state, v.finding.unwrap().code), (MinerState::Disabled, "E-CONFIG-PAY-ADDRESS"));
        let mut i = base(Some(&refused), None);
        i.process = Some((false, 3600));
        assert_eq!(miner_state(&i).state, MinerState::NotProducing, "a node without --palw-produce is not a disabled miner");
    }

    #[test]
    fn durations_read_the_way_people_say_them() {
        assert_eq!(ago(12), "12 s");
        assert_eq!(ago(12 * 60), "12 m");
        assert_eq!(ago(2 * 3600 + 13 * 60), "2 h 13 m");
        assert_eq!(ago(3 * 86_400 + 4 * 3600), "3 d 4 h");
        assert_eq!(group(7_412), "7,412");
        assert_eq!(group(1_234_567), "1,234,567");
        assert_eq!(compact(3408.0), "3.4 k");
    }
}
