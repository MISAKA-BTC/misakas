//! **The catalog of operator findings** — ADR-0122 Decision 4, sourced from the code's own refusals.
//!
//! Each constructor turns one condition the node, the host or a file reports into the five-field
//! finding, with the code that names it. The node's sentences are matched by the constants the node
//! itself returns (`PALW_NOT_READY_*_V2`), never by a copy of the words: the day one is reworded,
//! this file stops compiling instead of quietly turning a known hold into an unrecognised one.

use crate::exit;
use crate::operator::finding::Finding;
use kaspa_consensus_core::palw_producer_v2::{
    PALW_NOT_READY_BOND_UNKNOWN_V2, PALW_NOT_READY_EPOCH_BUDGET_V2, PALW_NOT_READY_EXPOSURE_FULL_V2, PALW_NOT_READY_KEY_MISMATCH_V2,
};

const JOIN: &str = "docs/testnet11-join-mining.md";
const DOCS_KEY: &str = "docs/testnet11-join-mining.md#2-key-address-funds";
const DOCS_BOND: &str = "docs/testnet11-join-mining.md#3-register-a-bond";
const DOCS_PRODUCE: &str = "docs/testnet11-join-mining.md#4-produce";
const DOCS_CLASS: &str = "docs/testnet11-join-mining.md#5-which-class-you-are-mining";
const DOCS_COST: &str = "docs/testnet11-join-mining.md#6-what-a-bond-costs-you";
const DOCS_STOP: &str = "docs/testnet11-join-mining.md#6b-do-not-stop-your-node-with-claims-in-flight";
const DOCS_RUN: &str = "docs/testnet11-node-operator.md#3-running-the-node";
const DOCS_QUARANTINE: &str = "docs/testnet11-node-operator.md#7-if-your-node-says-quarantined";
const DOCS_FP_ARTIFACT: &str = "docs/testnet11-free-prompt-mining.md#2-the-artifact-bound-and-the-same-file-everywhere";
const DOCS_FP_RUN: &str = "docs/testnet11-free-prompt-mining.md#4-run-it-node-gateway-watcher";

/// Nothing on this host mines: no `mining.toml`, no running `kaspad`, no bond named.
pub(crate) fn not_set_up() -> Finding {
    Finding::error("E-CONFIG-NONE", exit::CONFIG, "Not mining: nothing on this host is set up to mine")
        .reason("there is no ~/.misaka/mining.toml, no kaspad running here, and no --bond on this command")
        .fix("misaka mining setup")
        .docs(JOIN)
}

/// Several nodes of this network run here and nothing said which one.
pub(crate) fn several_nodes(network: &str, pids: &[u32]) -> Finding {
    Finding::error("E-CONFIG-AMBIGUOUS-NODE", exit::CONFIG, format!("{} kaspad processes run {network} on this host", pids.len()))
        .reason("the status of one miner cannot be read off several nodes")
        .current(format!("pids {}", pids.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")))
        .fix("name one: --appdir <the node's --appdir>, or set [advanced] appdir in ~/.misaka/mining.toml")
        .docs("docs/testnet11-node-operator.md#6-running-more-than-one-node-on-a-host")
}

/// A mining configuration exists and its node is not running.
pub(crate) fn node_down(network: &str, appdir: &str) -> Finding {
    Finding::error("E-PROC-NODE-DOWN", exit::COMPONENT_DOWN, "Not mining: the node is not running")
        .reason("no kaspad process for this network and appdir is running on this host")
        .current(format!("network {network} · appdir {appdir}"))
        .fix("misaka mining start")
        .docs(DOCS_RUN)
}

/// The process runs and its RPC does not answer.
pub(crate) fn rpc_unreachable(url: &str, why: &str, borsh_flag: Option<&str>) -> Finding {
    let listener = match borsh_flag {
        Some(v) => format!("the node was started with --rpclisten-borsh={v}"),
        None => "the node was started without --rpclisten-borsh, so it serves no wRPC Borsh at all".to_string(),
    };
    Finding::error("E-NODE-RPC-UNREACHABLE", exit::CONNECTION, "The node is running but its RPC does not answer")
        .reason("every reading of the chain goes through the node's wRPC Borsh port")
        .current(format!("{url}: {why}"))
        .current(listener)
        .required("kaspad with --rpclisten-borsh (the default is 127.0.0.1:27210 on a testnet)")
        .fix("pass --rpc <host:port> if the node listens elsewhere, or restart it with --rpclisten-borsh=default")
        .docs(DOCS_RUN)
}

pub(crate) fn network_mismatch(node: &str, want: &str) -> Finding {
    Finding::error("E-NODE-NETWORK", exit::NETWORK_MISMATCH, format!("The node is on {node}, not {want}"))
        .reason("a mining profile names one network, and this node runs another")
        .current(format!("node {node} · profile {want}"))
        .fix(format!("point --rpc at the {want} node, or set network = \"{node}\" in ~/.misaka/mining.toml"))
}

pub(crate) fn not_synced(daa: u64) -> Finding {
    Finding::error("E-NODE-NOT-SYNCED", exit::NOT_READY, "Not mining yet: the node is still syncing")
        .reason("a producer only draws on a synced chain — a block on a stale tip is a fork of one")
        .current(format!("virtual DAA {daa}, not synced"))
        .required("synced")
        .fix("wait: the node mines by itself once it is synced (misaka mining status shows when)")
        .docs("docs/testnet11-node-operator.md#5-how-long-the-first-sync-takes--measured-on-the-algo-4-lane")
}

pub(crate) fn no_peers() -> Finding {
    Finding::error("E-NET-NO-PEERS", exit::NOT_READY, "Not mining: no peer is connected")
        .reason("the producer never mines alone: a block with no peer to relay it is a fork of one")
        .current("0 peers")
        .required("at least 1 connected peer")
        .fix("misaka doctor node   (checks the P2P port, --addpeer and the fork fingerprint)")
        .docs(DOCS_RUN)
}

pub(crate) fn participation_closed() -> Finding {
    Finding::error("E-NET-PARTICIPATION", exit::NOT_READY, "Not mining: this node's chain participation is closed")
        .reason("the node has closed its participation gate — quarantined, or its view of the chain is not trusted yet")
        .current("participation_allowed=false")
        .fix("read the node log for 'quarantined'; if it is, restart once with --clear-quarantine and then without it")
        .docs(DOCS_QUARANTINE)
}

/// `should_mine` is false only because the sink is older than the sync window.
pub(crate) fn sink_stale() -> Finding {
    Finding::error("E-NODE-SINK-STALE", exit::NOT_READY, "Not mining: the chain tip is older than the sync window")
        .reason("the mining rule engine holds a producer whose sink is older than the window, and this node has peers")
        .current("enable_unsynced_mining=false · peers=true · participation_allowed=true")
        .fix("wait for a block from the network; a brand-new chain's first block alone needs --enable-unsynced-mining")
        .docs(DOCS_PRODUCE)
}

pub(crate) fn class_unknown(class: &str) -> Finding {
    Finding::error("E-MODEL-CLASS-UNKNOWN", exit::MODEL, "Not mining: this network has no such class")
        .reason("the class the producer is told to mine is not registered on this chain")
        .current(format!("class {class}"))
        .fix("mine the base class (omit --palw-producer-class), or name a registered class id")
        .docs(DOCS_CLASS)
}

/// The numbers the producer prints after a hold (`[class=… epoch=… produced=… budget=… exposure=a/b
/// per_claim=c]`) or that the facts RPC returns, for `Current`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HoldNumbers {
    pub(crate) epoch: Option<u64>,
    pub(crate) produced: Option<u64>,
    pub(crate) budget: Option<u64>,
    pub(crate) reserved: Option<u128>,
    pub(crate) ceiling: Option<u128>,
    pub(crate) per_claim: Option<u128>,
}

impl HoldNumbers {
    /// Read the bracket the producer appends to a `holding:` line.
    pub(crate) fn from_bracket(detail: &str) -> HoldNumbers {
        let mut n = HoldNumbers::default();
        let Some(open) = detail.rfind(" [") else { return n };
        for part in detail[open + 2..].trim_end_matches(']').split_whitespace() {
            let Some((k, v)) = part.split_once('=') else { continue };
            match k {
                "epoch" => n.epoch = v.parse().ok(),
                "produced" => n.produced = v.parse().ok(),
                "budget" => n.budget = v.parse().ok(),
                "exposure" => {
                    if let Some((a, b)) = v.split_once('/') {
                        n.reserved = a.parse().ok();
                        n.ceiling = b.parse().ok();
                    }
                }
                "per_claim" => n.per_claim = v.parse().ok(),
                _ => {}
            }
        }
        n
    }
}

/// `2,150.00 MSK` from sompi.
pub(crate) fn msk(sompi: u128) -> String {
    let whole = sompi / 100_000_000;
    let frac = (sompi % 100_000_000) / 1_000_000;
    let mut digits = whole.to_string();
    let mut grouped = String::new();
    while digits.len() > 3 {
        let tail = digits.split_off(digits.len() - 3);
        grouped = format!(",{tail}{grouped}");
    }
    format!("{digits}{grouped}.{frac:02} MSK")
}

/// **`ready_to_produce`'s verdict, as the operator should read it.** `reason` is the node's
/// sentence: the RPC's `not_ready_reason`, or the text a `holding:` line carries before its bracket.
pub(crate) fn not_ready(reason: &str, n: &HoldNumbers, bond: Option<&str>) -> Finding {
    let reason = reason.trim();
    let bond = bond.unwrap_or("the configured bond");
    if reason.starts_with(PALW_NOT_READY_BOND_UNKNOWN_V2) {
        return Finding::error("E-BOND-NOT-REGISTERED", exit::IDENTITY, "Not mining: the bond is not registered on this chain")
            .reason("a producer draws under a registered bond; the chain knows no bond at this outpoint")
            .current(format!("bond {bond}"))
            .required("a PALW bond registered to this node's key")
            .fix("misaka mining setup   (registers one, and records its outpoint for you)")
            .fix("a bond registered under another key: run with that key")
            .docs(DOCS_BOND);
    }
    if reason.starts_with(PALW_NOT_READY_KEY_MISMATCH_V2) {
        return key_mismatch(bond, None, None);
    }
    if reason.starts_with(PALW_NOT_READY_EPOCH_BUDGET_V2) {
        let mut f = Finding::error("E-MODEL-EPOCH-BUDGET", exit::NOT_READY, "Not mining: this class's epoch budget is spent")
            .reason("each class may produce a set number of blocks per epoch; this epoch's are all produced")
            .docs(DOCS_CLASS);
        match (n.epoch, n.produced, n.budget) {
            (Some(e), Some(p), Some(0)) => {
                f = f.current(format!("epoch {e}: budget 0, produced {p}")).fix(
                    "a budget of 0 is either a class that holds no share at all, or one registered mid-epoch that gets a budget \
                     at the next boundary: kaspad --palw-dump-classes prints its share (NONE is the first case)",
                )
            }
            (Some(e), Some(p), Some(b)) => {
                f = f
                    .current(format!("epoch {e}: {p} of {b} blocks produced"))
                    .fix("wait: the budget resets at the next epoch boundary")
            }
            _ => f = f.fix("wait for the next epoch boundary; kaspad --palw-dump-classes prints the class's share and budget"),
        }
        return f;
    }
    if reason.starts_with(PALW_NOT_READY_EXPOSURE_FULL_V2) {
        let mut f = Finding::error("E-BOND-EXPOSURE-FULL", exit::NOT_READY, "Not mining: the bond's exposure ceiling leaves no room")
            .reason("every claim reserves collateral until it is final; this bond's collateral is all reserved")
            .docs(DOCS_COST);
        if let (Some(r), Some(c)) = (n.reserved, n.ceiling) {
            f = f.current(format!("reserved {} of {}", msk(r), msk(c)));
        }
        if let Some(p) = n.per_claim {
            f = f.required(format!("{} of room for one more claim", msk(p)));
        }
        return f
            .fix("wait: room returns as this bond's claims turn final (misaka work list)")
            .fix("more room needs a bond with more collateral — a bond's collateral is fixed at registration");
    }
    Finding::error("E-NODE-NOT-READY", exit::NOT_READY, "Not mining: the node says this bond is not ready")
        .reason("the node gave a reason this build does not know")
        .current(reason.to_string())
        .fix("read it with a misaka CLI as new as the node")
}

/// The local key is not the key the bond registered — checked here because the facts RPC cannot:
/// the node never holds the caller's key, so it answers readiness for the bond's own.
pub(crate) fn key_mismatch(bond: &str, local: Option<&str>, registered: Option<&str>) -> Finding {
    let mut f = Finding::error("E-BOND-KEY-MISMATCH", exit::IDENTITY, "Not mining: this key is not the bond's key")
        .reason("a bond signs with the key it registered; the producer holds another")
        .current(format!("bond {bond}"));
    if let (Some(l), Some(r)) = (local, registered) {
        f = f.current(format!("this key {}… · the bond's {}…", &l[..l.len().min(16)], &r[..r.len().min(16)]));
    }
    f.fix("run with the key file that registered this bond (--palw-producer-key / [mining] key)")
        .fix("or register a bond for this key: misaka mining setup")
        .docs(DOCS_BOND)
}

/// **A `holding:` line from the node's log**, to the finding it means — the producer's three hold
/// sites, by the sentences `kaspad/src/palw_producer.rs` prints.
pub(crate) fn hold_from_log(detail: &str, bond: Option<&str>) -> Finding {
    if let Some(flags) = detail.strip_prefix("the mining rule engine says this node should not mine") {
        if flags.contains("peers=false") {
            return no_peers();
        }
        if flags.contains("participation_allowed=false") {
            return participation_closed();
        }
        return sink_stale();
    }
    if let Some(rest) = detail.strip_prefix("this network has no ConsensusV2 facts for class ") {
        return class_unknown(rest.split_whitespace().next().unwrap_or(rest));
    }
    let sentence = detail.rfind(" [").map(|i| &detail[..i]).unwrap_or(detail);
    not_ready(sentence, &HoldNumbers::from_bracket(detail), bond)
}

/// A startup refusal the producer printed (`… — production disabled`, `not producing (…)`).
pub(crate) fn producer_disabled(text: &str) -> Finding {
    let (code, exit_code, what, fix) = if text.contains("pay address") {
        ("E-CONFIG-PAY-ADDRESS", exit::IDENTITY, "the pay address", "set [mining] wallet to an ML-DSA-87 address of this network")
    } else if text.contains("key") || text.contains("seed") {
        (
            "E-CONFIG-KEY",
            exit::CONFIG,
            "the producer key",
            "point --palw-producer-key / [mining] key at the bond's 32-byte hex seed file",
        )
    } else if text.contains("outpoint") || text.contains("bond") {
        (
            "E-CONFIG-BOND",
            exit::CONFIG,
            "the bond outpoint",
            "set [advanced] bond to <txid>:<index> (misaka bond status --class-id finds it)",
        )
    } else {
        ("E-CONFIG-PRODUCER", exit::CONFIG, "the producer's configuration", "read the startup warning in the node log")
    };
    Finding::error(code, exit_code, format!("Not mining: the producer refused {what} at startup"))
        .reason("kaspad checks the producer's key, bond and pay address once, at startup, and stays a plain node if one is wrong")
        .current(text.to_string())
        .fix(fix)
        .fix("then restart the node")
        .docs(DOCS_PRODUCE)
}

pub(crate) fn key_unreadable(path: &str, why: &str) -> Finding {
    Finding::error("E-CONFIG-KEY", exit::CONFIG, "The producer key cannot be read")
        .reason("the key file proves this node is the bond's; without it nothing can be checked or signed")
        .current(format!("{path}: {why}"))
        .required("a 32-byte hex ML-DSA-87 seed file, mode 0600, readable by this user")
        .fix("run as the user the node runs as, or pass --key-file <path>")
        .docs(DOCS_KEY)
}

pub(crate) fn pay_address_invalid(address: &str, why: &str) -> Finding {
    Finding::error("E-CONFIG-PAY-ADDRESS", exit::IDENTITY, "The pay address cannot receive block rewards")
        .reason("a coinbase must pay an ML-DSA-87 P2PKH address of this network, or the block is dead on arrival")
        .current(format!("{address}: {why}"))
        .fix("set [mining] wallet to your ML-DSA-87 address (misaka key address --key-file <seed>)")
        .docs(DOCS_KEY)
}

/// The node booted a different consensus than this CLI knows for the network.
pub(crate) fn fingerprint_mismatch(node: &str, cli: &str, network: &str, peers: Option<usize>) -> Finding {
    let who = match peers {
        Some(0) => "the node has no peers, which is what a node on the wrong side of a fence looks like",
        Some(_) => "the node has peers, so this CLI is the likelier odd one out — rebuild it",
        None => "one of the two is not the release",
    };
    Finding::error("E-NODE-FORK-MISMATCH", exit::NETWORK_MISMATCH, "The node and this CLI disagree about the network's consensus")
        .reason(format!("two builds of {network} with different consensus parameters are different chains; {who}"))
        .current(format!("node {node} · this CLI {cli}"))
        .required("one release: the same fingerprint on the node, this CLI and the network")
        .fix("rebuild kaspad and misaka from the release commit, then restart the node")
        .docs("docs/testnet11-node-operator.md")
}

pub(crate) fn schedule_mismatch(node: &[u64], cli: &[u64]) -> Finding {
    let show = |v: &[u64]| v.iter().map(|h| h.to_string()).collect::<Vec<_>>().join(", ");
    Finding::error("E-NODE-SCHEDULE-MISMATCH", exit::NETWORK_MISMATCH, "The node schedules different fence heights than this CLI")
        .reason("a fence scheduled at another height splits the network at that height")
        .current(format!("node {}", show(node)))
        .current(format!("this CLI {}", show(cli)))
        .fix("rebuild kaspad and misaka from the release commit, then restart the node")
}

pub(crate) fn image_replaced(pid: u32) -> Finding {
    Finding::warning("W-PROC-NODE-IMAGE-REPLACED", "The running node is not the binary on disk")
        .reason("the kaspad file was replaced after this process started — a rebuild that was not restarted")
        .current(format!("pid {pid}"))
        .fix("restart the node when it has no claims to defend (misaka mining stop --drain, then start)")
        .docs(DOCS_STOP)
}

pub(crate) fn fee_outpoint_missing() -> Finding {
    Finding::error("E-FUNDS-FEE-OUTPOINT-MISSING", exit::FUNDS, "The panel has no fee outpoint")
        .reason("a producer on a ConsensusV2 network must be able to carry receipts and answers; kaspad panics without one")
        .current("no --palw-fee-outpoint and none persisted in palw-panel/palw-fee-outpoint")
        .required("a mature, unbonded UTXO at the producer's key, ≥ 0.1 MSK")
        .fix("misaka mining setup --fee-outpoint auto")
        .docs(DOCS_PRODUCE)
}

pub(crate) fn utxoindex_off(prompt_lane: bool) -> Finding {
    let f = if prompt_lane {
        Finding::error("E-NODE-NO-UTXOINDEX", exit::CONFIG, "The node has no --utxoindex, which the prompt lane needs")
    } else {
        Finding::warning("W-NODE-NO-UTXOINDEX", "The node has no --utxoindex: rewards and funds cannot be read")
    };
    f.reason("balances, maturity and the rail's funding are read from the node's UTXO index")
        .fix("restart the node with --utxoindex (a one-time reindex)")
        .docs(DOCS_RUN)
}

pub(crate) fn disk(free: u64, floor: u64, total: u64, volume_of: &str) -> Option<Finding> {
    use crate::operator::host::human_bytes;
    if free < floor {
        Some(
            Finding::error("E-HOST-DISK-FLOOR", exit::HOST, "The retention volume is under its free-space floor")
                .reason(
                    "the janitor deletes re-makeable captures first and then cannot free more; a full disk stops the node's database",
                )
                .current(format!("{} free of {} on the volume holding {volume_of}", human_bytes(free), human_bytes(total)))
                .required(format!("{} free: max(8 GiB, 5 %)", human_bytes(floor)))
                .fix("free space on that volume, or move the appdir to a larger one"),
        )
    } else if free < floor.saturating_mul(2) {
        Some(
            Finding::warning("W-HOST-DISK-LOW", "The retention volume is close to its free-space floor")
                .current(format!("{} free, the floor is {}", human_bytes(free), human_bytes(floor)))
                .fix("free space before the janitor has to start dropping captures"),
        )
    } else {
        None
    }
}

pub(crate) fn memory(available: u64, needed: u64) -> Finding {
    use crate::operator::host::human_bytes;
    Finding::warning("W-HOST-MEMORY", "Memory is tight for this node")
        .reason("a validating node uses 8–11 GiB before any model weights; an OOM kill stops the node with its claims")
        .current(format!("{} available", human_bytes(available)))
        .required(format!("about {}", human_bytes(needed)))
        .fix("set MemoryMax= on the unit, lower --palw-class-resident-bytes, or run fewer nodes on this host")
}

pub(crate) fn unattended_upgrades() -> Finding {
    Finding::warning("W-HOST-UNATTENDED-UPGRADES", "unattended-upgrades is enabled")
        .reason("a background upgrade has restarted and OOM-killed fleet nodes, taking their claims with them")
        .fix("disable it (sudo systemctl disable --now unattended-upgrades) or bound the node with MemoryMax=")
}

pub(crate) fn clock_unsynced() -> Finding {
    Finding::warning("W-HOST-CLOCK", "The clock is not NTP-synchronised")
        .reason("block timestamps and peers' time offsets are checked against this clock")
        .fix("enable time sync (sudo timedatectl set-ntp true)")
}

pub(crate) fn retention_short(line: &str) -> Finding {
    Finding::warning("W-HOST-RETENTION-FLOOR", "The janitor could not keep the retention volume above its floor")
        .reason("what is left is free-prompt captures the chain can still ask about, or other data on the volume")
        .current(line.to_string())
        .fix("free space on the retention volume")
}

pub(crate) fn artifact_missing(path: &str) -> Finding {
    Finding::error("E-MODEL-ARTIFACT-MISSING", exit::MODEL, "A class artifact the node is told to hold is not there")
        .reason("kaspad refuses a --palw-class-artifact it cannot read, and the class is then not served")
        .current(path.to_string())
        .fix("put the file back, or drop the flag / the [advanced] artifact entry")
        .docs(DOCS_FP_ARTIFACT)
}

pub(crate) fn gateway_down(listen: &str, why: &str) -> Finding {
    Finding::error("E-PROMPT-GATEWAY-DOWN", exit::COMPONENT_DOWN, "The prompt lane's gateway does not answer")
        .reason("prompts reach the worker only through the gateway")
        .current(format!("GET http://{listen}/health: {why}"))
        .fix("start the prompt lane: misaka mining start (with prompt = true)")
        .docs(DOCS_FP_RUN)
}

pub(crate) fn rail_down(outbox: &str) -> Finding {
    Finding::error("E-PROMPT-RAIL-DOWN", exit::COMPONENT_DOWN, "Nothing submits the prompt lane's work")
        .reason("the gateway commits answers to its outbox and NEVER submits them: without the rail no prompt claim reaches the chain")
        .current(format!("no misaka-palw-fp-rail --watch {outbox} is running"))
        .fix(format!("misaka-palw-fp-rail --watch {outbox} --bond-key-seed <file> --rpc <host:port>"))
        .docs(DOCS_FP_RUN)
}

pub(crate) fn not_producing() -> Finding {
    let mut f = Finding::warning("I-NODE-NOT-PRODUCING", "This node does not produce")
        .reason("kaspad runs without --palw-produce: it verifies and relays, and draws nothing")
        .fix("to mine, run it with --palw-produce (misaka mining start does)");
    f.severity = crate::operator::finding::Severity::Info;
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_producer_v2::PALW_NOT_READY_REASONS_V2;

    /// **Every sentence `ready_to_produce` can return maps to a code of its own** — the test the
    /// catalog's module comment promises. A fifth reason added to the node fails here until it is
    /// given a code.
    #[test]
    fn every_readiness_verdict_the_node_can_return_has_a_code() {
        let mut codes: Vec<&str> =
            PALW_NOT_READY_REASONS_V2.iter().map(|reason| not_ready(reason, &HoldNumbers::default(), None).code).collect();
        assert!(!codes.contains(&"E-NODE-NOT-READY"), "a node verdict fell through to the unknown arm: {codes:?}");
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), PALW_NOT_READY_REASONS_V2.len(), "two verdicts share a code");
    }

    /// The producer's log line, read back into its finding with the numbers it carried.
    #[test]
    fn a_holding_line_is_read_into_its_finding_and_numbers() {
        let line = format!(
            "{PALW_NOT_READY_EXPOSURE_FULL_V2} [class=ab epoch=3 produced=1 budget=12 exposure=215000000000/220000000000 \
             per_claim=74000000000]"
        );
        let f = hold_from_log(&line, Some("aa:0"));
        assert_eq!(f.code, "E-BOND-EXPOSURE-FULL");
        assert_eq!(f.current, vec!["reserved 2,150.00 MSK of 2,200.00 MSK".to_string()]);
        assert_eq!(f.required, "740.00 MSK of room for one more claim");

        let f = hold_from_log(&format!("{PALW_NOT_READY_EPOCH_BUDGET_V2} [class=ab epoch=9 produced=0 budget=0]"), None);
        assert_eq!(f.code, "E-MODEL-EPOCH-BUDGET");
        assert!(f.fix[0].contains("holds no share"), "a zero budget names both of its causes: {:?}", f.fix);

        let peers = "the mining rule engine says this node should not mine [enable_unsynced_mining=false peers=false \
                     participation_allowed=true]";
        assert_eq!(hold_from_log(peers, None).code, "E-NET-NO-PEERS");
        let gate = "the mining rule engine says this node should not mine [enable_unsynced_mining=false peers=true \
                    participation_allowed=false]";
        assert_eq!(hold_from_log(gate, None).code, "E-NET-PARTICIPATION");
        let stale = "the mining rule engine says this node should not mine [enable_unsynced_mining=false peers=true \
                     participation_allowed=true]";
        assert_eq!(hold_from_log(stale, None).code, "E-NODE-SINK-STALE");
        assert_eq!(
            hold_from_log("this network has no ConsensusV2 facts for class 4277d84f — nothing to produce", None).code,
            "E-MODEL-CLASS-UNKNOWN"
        );
    }

    #[test]
    fn msk_reads_like_an_amount() {
        assert_eq!(msk(0), "0.00 MSK");
        assert_eq!(msk(1_110_106_160), "11.10 MSK");
        assert_eq!(msk(215_000_000_000), "2,150.00 MSK");
        assert_eq!(msk(123_456_789_000_000), "1,234,567.89 MSK");
    }
}
