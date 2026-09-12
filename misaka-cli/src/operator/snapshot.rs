//! **Everything the operator screens read, read once** — the node, the key, the bond's facts, the
//! wallet, the node's log and the works — so `status`, `doctor` and `work` render from one picture
//! of one moment instead of each asking the node its own questions at its own time.
//!
//! Every read is allowed to fail on its own. A node that does not answer still leaves the process
//! table and the log to describe it; a node without `--utxoindex` still answers everything but the
//! wallet. Each failure is kept with its reason, and a screen shows the reason where the value
//! would have been.

use crate::operator::nodelog::{self, NodeLog};
use crate::operator::profile::Profile;
use crate::operator::work::{self, Lane, RailVerdict};
use crate::palw_claim::{OutboxRow, OutboxState, Windows};
use crate::wallet::NodeView;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::network::{EndpointKind, NetworkId};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPalwFreePromptClaimResponse, GetPalwProducerFactsResponse, GetServerInfoResponse, RpcPeerInfo};
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

/// How much of the node's log the screens read from its end. The producer's hold and draw lines
/// repeat every five minutes; a tail this size spans hours on a busy node.
const LOG_TAIL_BYTES: u64 = 24 << 20;
/// How many of this node's produced blocks `status` follows to their claims.
const BLOCKS_FOLLOWED: usize = 40;

/// A connected node and what it said about itself.
pub(crate) struct NodeRead {
    pub(crate) nv: NodeView,
    pub(crate) url: String,
    pub(crate) server: GetServerInfoResponse,
    pub(crate) peers: Option<Vec<RpcPeerInfo>>,
    pub(crate) windows: Option<Windows>,
}

impl NodeRead {
    pub(crate) fn client(&self) -> &KaspaRpcClient {
        &self.nv.client
    }
    pub(crate) fn daa(&self) -> u64 {
        self.server.virtual_daa_score
    }
    pub(crate) fn outbound_peers(&self) -> Option<usize> {
        self.peers.as_ref().map(|p| p.iter().filter(|p| p.is_outbound).count())
    }
}

/// The public half of the producer key: what the bond is compared against. The seed is never kept.
#[derive(Clone, Debug)]
pub(crate) struct KeyFacts {
    pub(crate) pubkey_hex: String,
    pub(crate) address: String,
}

/// The pay address's UTXOs, split the way a wallet spends them.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub(crate) struct WalletFacts {
    pub(crate) address: String,
    pub(crate) spendable_sompi: u64,
    pub(crate) maturing_sompi: u64,
    pub(crate) maturing_outputs: usize,
    pub(crate) next_mature_daa: Option<u64>,
    pub(crate) bonded_sompi: u64,
    pub(crate) coinbase_outputs: usize,
}

/// One work, with everything read about it.
#[derive(Clone, Debug)]
pub(crate) struct WorkRow {
    pub(crate) lane: Lane,
    /// The claim id, 128 hex, once there is one.
    pub(crate) claim_id: Option<String>,
    /// The outbox stem (`fp-job-<16 hex>`) of a prompt-lane job.
    pub(crate) job: Option<String>,
    /// The block a block-lane work produced.
    pub(crate) block: Option<String>,
    /// When this host first recorded it (the log line, or the job file).
    pub(crate) seen_ts: Option<i64>,
    pub(crate) chain: Option<GetPalwFreePromptClaimResponse>,
    pub(crate) outbox: Option<OutboxRow>,
    pub(crate) reading: work::Reading,
    /// What the node's claim row adds (deadline, escrow, a queued payout), when the node served one.
    pub(crate) extra: Option<work::ClaimExtra>,
}

impl WorkRow {
    /// The id a person types: the claim's, else the job's.
    pub(crate) fn display_id(&self) -> String {
        match (&self.claim_id, &self.job) {
            (Some(id), _) => work::short_id(id).to_string(),
            (None, Some(job)) => format!("job:{}", work::short_id(job.trim_start_matches("fp-job-"))),
            (None, None) => "?".to_string(),
        }
    }
}

/// Everything, read once.
pub(crate) struct Snapshot {
    pub(crate) profile: Profile,
    pub(crate) log: Result<NodeLog, String>,
    pub(crate) node: Result<NodeRead, (String, String)>,
    /// The class the producer mines: the profile's, else the network's base class.
    pub(crate) class_id: Option<String>,
    pub(crate) class_is_base: bool,
    pub(crate) key: Option<Result<KeyFacts, String>>,
    pub(crate) facts: Option<Result<GetPalwProducerFactsResponse, String>>,
    pub(crate) wallet: Option<Result<WalletFacts, String>>,
    pub(crate) works: Vec<WorkRow>,
    /// Reads of works that failed (a block the node no longer has, an unreadable outbox).
    pub(crate) work_errors: Vec<String>,
    /// **Is the log this node's, now?** A log at the default path can be a previous run's — a node
    /// stopped last week, or another node's — and a hold or a fingerprint read from it would be a
    /// verdict about a process that is not running. Live means the running node has written to it
    /// since it started (or, for a node read over `--rpc` from elsewhere, that it moved in the last
    /// ten minutes).
    pub(crate) log_live: bool,
    /// The log's boot lines are this process's boot, not an earlier one's.
    pub(crate) boot_current: bool,
    /// `getPalwNodeStatus` (ADR-0122 §6.5), from a node that serves it; `None` from an older one,
    /// whose log is then the only account of its runtime.
    pub(crate) node_status: Option<kaspa_rpc_core::GetPalwNodeStatusResponse>,
    /// Where the works came from: `node` (`getPalwClaims`) or `log` (the produced blocks the node's
    /// log names, followed one by one).
    pub(crate) works_source: &'static str,
}

impl Snapshot {
    /// The log, when it describes the node as it runs now.
    pub(crate) fn live_log(&self) -> Option<&NodeLog> {
        self.log.as_ref().ok().filter(|_| self.log_live)
    }

    /// The log's boot lines, when they are the running node's boot.
    pub(crate) fn boot_log(&self) -> Option<&NodeLog> {
        self.live_log().filter(|_| self.boot_current)
    }
}

/// Liveness, from the log's own timestamps and the process's start.
fn log_liveness(log: &Result<NodeLog, String>, profile: &Profile, node_ok: bool) -> (bool, bool) {
    let Ok(log) = log else { return (false, false) };
    let now = crate::operator::procs::now_unix() as i64;
    match &profile.kaspad {
        Some((p, _)) => {
            let started = p.start_time as i64;
            let live = log.last_ts.is_some_and(|t| t >= started);
            // The boot lines are printed in the first seconds of a run; ones older than the
            // process are an earlier run's, read back from above the tail or from the archive.
            let current = live && log.boot_ts.is_some_and(|b| b >= started - 300);
            (live, current)
        }
        None => {
            let live = node_ok && log.last_ts.is_some_and(|t| now - t < 600);
            (live, live)
        }
    }
}

/// Open the node's wRPC Borsh connection for `profile`, and read its identity and peers.
pub(crate) async fn connect(profile: &Profile, timeout: Duration) -> Result<NodeRead, (String, String)> {
    let net = NetworkId::from_str(&profile.network).map_err(|e| (profile.network.clone(), format!("not a network id: {e}")))?;
    let registry = misaka_endpoints::EndpointRegistry::load(&profile.network);
    let hostport = misaka_endpoints::resolve(&net, EndpointKind::NodeWrpcBorsh, profile.rpc.as_deref(), registry.as_ref());
    let url = format!("ws://{hostport}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None).map_err(|e| (url.clone(), e.to_string()))?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(timeout),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    client.connect(Some(options)).await.map_err(|e| (url.clone(), e.to_string()))?;
    let server = client.get_server_info().await.map_err(|e| (url.clone(), format!("getServerInfo: {e}")))?;
    let peers = client.get_connected_peer_info().await.ok().map(|r| r.peer_info);
    let nv = NodeView::from_parts(client, &server);
    let windows = Windows::of(&nv.params);
    Ok(NodeRead { nv, url, server, peers, windows })
}

/// Read the key file's public half. The seed is dropped (and zeroized by the key type) here.
pub(crate) fn key_facts(path: &Path, prefix: kaspa_addresses::Prefix) -> Result<KeyFacts, String> {
    let source = crate::keys::KeySource { key_file: Some(path.display().to_string()), key_stdin: false };
    let key = source.load_key().map_err(|e| e.msg)?;
    Ok(KeyFacts { pubkey_hex: faster_hex::hex_string(key.public_key()), address: key.funding_address(prefix).to_string() })
}

impl Snapshot {
    pub(crate) async fn gather(profile: Profile, timeout: Duration, follow_works: bool) -> Snapshot {
        let log = nodelog::read(&profile.log_file, LOG_TAIL_BYTES);
        let node = connect(&profile, timeout).await;
        let params = kaspa_consensus_core::config::params::Params::from(
            NetworkId::from_str(&profile.network)
                .unwrap_or(NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 11)),
        );
        let base = match &params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.base_class_id.to_string()),
            _ => None,
        };
        let class_id = profile.class.clone().or_else(|| base.clone());
        let class_is_base = class_id.is_some() && class_id == base;

        let key = profile.key_path.as_ref().map(|p| key_facts(p, params.prefix()));
        let mut snap = Snapshot {
            profile,
            log,
            node,
            class_id,
            class_is_base,
            key,
            facts: None,
            wallet: None,
            works: Vec::new(),
            work_errors: Vec::new(),
            log_live: false,
            boot_current: false,
            node_status: None,
            works_source: "log",
        };
        (snap.log_live, snap.boot_current) = log_liveness(&snap.log, &snap.profile, snap.node.is_ok());
        let Ok(node) = snap.node.as_ref() else { return snap };
        // A node that predates the read answers "method not found": the log stays the account.
        snap.node_status = node.client().get_palw_node_status().await.ok();

        if let (Some(class), Some(bond)) = (snap.class_id.clone(), snap.profile.bond.clone()) {
            snap.facts = Some(match crate::bond::parse_outpoint(&bond) {
                Ok(op) => node
                    .client()
                    .get_palw_producer_facts(class, op.transaction_id.to_string(), op.index, true)
                    .await
                    .map_err(|e| format!("getPalwProducerFacts: {e}")),
                Err(e) => Err(e.msg),
            });
        }

        let pay =
            snap.profile.pay_address.clone().or_else(|| snap.key.as_ref().and_then(|k| k.as_ref().ok()).map(|k| k.address.clone()));
        if let Some(address) = pay {
            snap.wallet =
                Some(if node.server.has_utxo_index { wallet_facts(node, &address).await } else { Err("no --utxoindex".into()) });
        }

        if follow_works {
            let (works, errors, source) = gather_works(&snap, node).await;
            snap.works = works;
            snap.work_errors = errors;
            snap.works_source = source;
        }
        snap
    }
}

async fn wallet_facts(node: &NodeRead, address: &str) -> Result<WalletFacts, String> {
    let addr = kaspa_addresses::Address::try_from(address).map_err(|e| format!("{address}: {e}"))?;
    let all = crate::wallet::page_all(&node.nv, &addr).await.map_err(|e| e.msg)?;
    let after = node.nv.coinbase_spendable_after();
    let mut w = WalletFacts { address: address.to_string(), ..Default::default() };
    for u in &all {
        if u.bonded {
            w.bonded_sompi += u.amount;
        } else if u.mature {
            w.spendable_sompi += u.amount;
        } else {
            w.maturing_sompi += u.amount;
            w.maturing_outputs += 1;
            if u.entry.is_coinbase {
                let at = u.entry.block_daa_score.saturating_add(after);
                w.next_mature_daa = Some(w.next_mature_daa.map_or(at, |n| n.min(at)));
            }
        }
        if u.entry.is_coinbase {
            w.coinbase_outputs += 1;
        }
    }
    Ok(w)
}

/// The rail's `rail-watch-state.json`, read as the verdicts it records per job.
pub(crate) fn rail_state(outbox: &Path) -> std::collections::BTreeMap<String, RailVerdict> {
    let mut out = std::collections::BTreeMap::new();
    let Ok(bytes) = std::fs::read(outbox.join("rail-watch-state.json")) else { return out };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return out };
    if let Some(settled) = v.get("settled").and_then(|s| s.as_object()) {
        for (stem, how) in settled {
            // The rail keys its state by the stem's file name or path; the file name is what the
            // outbox scan calls it.
            let stem = stem.rsplit('/').next().unwrap_or(stem).to_string();
            out.entry(stem).or_insert_with(RailVerdict::default).settled = how.as_str().map(str::to_string);
        }
    }
    if let Some(a) = v.get("awaiting").filter(|a| !a.is_null()) {
        let stem = a.get("stem").and_then(|s| s.as_str()).unwrap_or_default();
        let stem = stem.rsplit('/').next().unwrap_or(stem).to_string();
        out.entry(stem).or_insert_with(RailVerdict::default).awaiting_since_daa = a.get("submitted_daa").and_then(|d| d.as_u64());
    }
    out
}

/// A claim row, as the claim read `work::classify` takes.
fn row_as_claim(r: &kaspa_rpc_core::RpcPalwClaimRow) -> GetPalwFreePromptClaimResponse {
    GetPalwFreePromptClaimResponse {
        found: true,
        claim_id: r.claim_id.clone(),
        is_free_prompt: r.is_free_prompt,
        class_id: r.class_id.clone(),
        executor_bond: r.executor_bond.clone(),
        work_leaves: r.work_leaves,
        quanta: r.quanta,
        quanta_spent: r.quanta_spent,
        phase: r.phase.clone(),
        void_reason: r.void_reason.clone(),
        phase_daa: r.phase_daa,
        accepted_block: r.accepted_block.clone(),
        accepted_daa: r.accepted_daa,
        ..Default::default()
    }
}

/// **This bond's works.** From a node that serves `getPalwClaims` (ADR-0122 §6.5): every claim the
/// state holds for the bond, both lanes, with the chain's own deadlines and escrows. From an older
/// node: the block lane's from the blocks the node's log says it produced, each followed to its
/// claim by the attempt id in the block's header — only as long as the log it read. Either way the
/// prompt lane's jobs that never reached the chain come from the outbox.
async fn gather_works(snap: &Snapshot, node: &NodeRead) -> (Vec<WorkRow>, Vec<String>, &'static str) {
    let mut works = Vec::new();
    let mut errors = Vec::new();
    let now = node.daa();
    let maturity = node.nv.coinbase_spendable_after();
    let windows = node.windows;
    let produced_at = |hash: &str| -> Option<i64> {
        snap.log
            .as_ref()
            .ok()
            .and_then(|l| l.produced.iter().chain(l.receipts.iter()).find(|(_, _, h)| h == hash).map(|(ts, _, _)| *ts))
    };

    let node_rows = match &snap.profile.bond {
        Some(bond) => node.client().get_palw_claims(bond.clone(), "executor".into(), true, 200).await.ok().filter(|r| r.available),
        None => None,
    };
    let source = if node_rows.is_some() { "node" } else { "log" };
    if let Some(resp) = &node_rows {
        for row in &resp.claims {
            let lane = if row.is_free_prompt { Lane::Prompt } else { Lane::Block };
            let chain = row_as_claim(row);
            let reading = work::classify(lane, None, None, Some(&chain), false, windows.as_ref(), maturity, now);
            let extra = work::ClaimExtra {
                deadline_daa: row.deadline_daa,
                escrow_sompi: row.escrow_sompi,
                payout_pending_sompi: row.payout_pending_sompi,
            };
            works.push(WorkRow {
                lane,
                claim_id: Some(row.claim_id.clone()),
                job: None,
                block: (lane == Lane::Block).then(|| row.accepted_block.clone()),
                seen_ts: produced_at(&row.accepted_block),
                chain: Some(chain),
                outbox: None,
                reading: work::refine(lane, reading, &extra),
                extra: Some(extra),
            });
        }
        if resp.truncated {
            errors.push(format!("the node listed the newest {} claims; older ones were left out", resp.claims.len()));
        }
    }

    if let (None, Ok(log)) = (&node_rows, &snap.log) {
        for (ts, _, hash) in log.produced.iter().rev().take(BLOCKS_FOLLOWED) {
            match claim_of_block(node, hash).await {
                Ok(Some(claim)) => {
                    let chain = ask_claim(node, &claim).await;
                    let reading = work::classify(Lane::Block, None, None, chain.as_ref(), false, windows.as_ref(), maturity, now);
                    works.push(WorkRow {
                        lane: Lane::Block,
                        claim_id: Some(claim),
                        job: None,
                        block: Some(hash.clone()),
                        seen_ts: Some(*ts),
                        chain,
                        outbox: None,
                        reading,
                        extra: None,
                    });
                }
                Ok(None) => errors.push(format!("block {} carries no attempt", work::short_id(hash))),
                Err(e) => errors.push(format!("block {}: {e}", work::short_id(hash))),
            }
        }
    }

    if let Some(outbox) = snap.profile.prompt.as_ref().and_then(|p| p.outbox.clone()) {
        match crate::palw_claim::scan_outbox(&outbox) {
            Ok(rows) => {
                let rail = rail_state(&outbox);
                for row in rows.into_iter().rev() {
                    // A job whose claim the node already listed is that claim: the files add its job
                    // name and when it was made, and the chain's reading stands.
                    if let Some(w) =
                        row.claim_id.as_deref().and_then(|id| works.iter_mut().find(|w| w.claim_id.as_deref() == Some(id)))
                    {
                        w.job = Some(row.stem.clone());
                        w.seen_ts = w
                            .seen_ts
                            .or(row.modified.and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64));
                        w.outbox = Some(row);
                        continue;
                    }
                    let verdict = rail.get(&row.stem).cloned();
                    let (chain, in_mempool) = if matches!(row.state, OutboxState::Submitted) {
                        match row.claim_id.as_deref() {
                            Some(id) => {
                                let chain = ask_claim(node, id).await;
                                let pooled = match (&chain, row.submitted_txid.as_deref()) {
                                    (Some(c), Some(txid)) if !c.found => in_mempool(node, txid).await,
                                    _ => false,
                                };
                                (chain, pooled)
                            }
                            None => (None, false),
                        }
                    } else {
                        (None, false)
                    };
                    let reading = work::classify(
                        Lane::Prompt,
                        Some(&row),
                        verdict.as_ref(),
                        chain.as_ref(),
                        in_mempool,
                        windows.as_ref(),
                        maturity,
                        now,
                    );
                    let seen_ts = row.modified.and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64);
                    works.push(WorkRow {
                        lane: Lane::Prompt,
                        claim_id: row.claim_id.clone(),
                        job: Some(row.stem.clone()),
                        block: None,
                        seen_ts,
                        chain,
                        outbox: Some(row),
                        reading,
                        extra: None,
                    });
                }
            }
            Err(e) => errors.push(e.msg),
        }
    }
    // Newest first: by when this host saw it, else by where the chain accepted it.
    works.sort_by(|a, b| {
        let key = |w: &WorkRow| (w.seen_ts, w.chain.as_ref().map(|c| c.accepted_daa));
        key(b).cmp(&key(a))
    });
    (works, errors, source)
}

/// The attempt id — the claim id — a block of this node's carries, from its header's envelope.
pub(crate) async fn claim_of_block(node: &NodeRead, hash: &str) -> Result<Option<String>, String> {
    let hash = kaspa_rpc_core::RpcHash::from_str(hash).map_err(|e| format!("not a block hash: {e}"))?;
    let block = node.client().get_block(hash, false).await.map_err(|e| format!("getBlock: {e}"))?;
    if !kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(block.header.pow_algo_id) {
        return Ok(None);
    }
    let envelope = kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&block.header.palw_commitment)
        .map_err(|e| format!("its attempt does not decode: {e}"))?;
    Ok(Some(kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&envelope.attempt).to_string()))
}

pub(crate) async fn ask_claim(node: &NodeRead, claim: &str) -> Option<GetPalwFreePromptClaimResponse> {
    let id = claim.parse::<Hash64>().ok()?;
    node.client().get_palw_free_prompt_claim(id.to_string()).await.ok()
}

async fn in_mempool(node: &NodeRead, txid: &str) -> bool {
    match txid.parse::<kaspa_consensus_core::tx::TransactionId>() {
        Ok(id) => node.client().get_mempool_entry(id, true, false).await.is_ok(),
        Err(_) => false,
    }
}
