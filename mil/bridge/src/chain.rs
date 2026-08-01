//! Chain facts — the bridge's window onto a live node.
//!
//! Everything the consensus seams need from the chain arrives through [`ChainFacts`]:
//! the buried PALW beacon (for challenge derivation, DA chunk sampling, and auditor draws) and
//! provider-bond records (for authorization, stake weights, and slash targeting). Two
//! implementations:
//!
//! * [`RpcChainFacts`] — wRPC to a real node, exactly the transport `kaspa-pq-validator` uses
//!   (`ws://host:port`, Borsh, retrying connect). Beacon comes from `getPalwState`'s
//!   `activation` block (the FINALITY-BURIED sample pair — which is what the DA sampler wants:
//!   `palw_da_provider_sample_indices` refuses a beacon that is not buried deep enough). Bond
//!   records come from `getPalwState { provider_bond_outpoint }`.
//! * [`PinnedChainFacts`] — an operator-supplied JSON snapshot. For tests and air-gapped dev.
//!   It is NOT live: a pinned beacon is a frozen number, so anything derived from it is
//!   reproducible but not fresh. `/palw/v1/status` reports which source is in use.
//!
//! **Registry enumeration.** There is no RPC that lists provider bonds (`palw_probe.rs` says so
//! outright: it "never enumerates the provider registry"), and `getPalwAuditFacts` only exposes
//! the full record set for an existing on-chain batch. So the bridge draws its auditor committee
//! from the providers REGISTERED WITH THIS BRIDGE, each independently verified against the chain
//! by outpoint. That is a strictly smaller set than the chain registry; it is the honest maximum
//! an off-chain coordinator can see today, and it is stated in the README rather than papered
//! over. When a node-side enumerate RPC lands, only [`ChainFacts::bond_record`]'s caller changes.
//!
//! One field cannot be recovered over RPC: `PalwProviderBondRecord::created_daa_score` (the DTO
//! omits it). We set it to `activation_daa_score`, which is sound because no consensus function
//! the bridge calls reads it — `effective_provider_bond_status`, the weighted sampler, and
//! `auditor_set_commitment` use activation / unbond / slash / amount / outpoint only.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use kaspa_consensus_core::palw::da::PalwBuriedBeaconV1;
use kaspa_consensus_core::palw::{PalwProviderBondRecord, PalwProviderBondStatus, effective_provider_bond_status};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use serde::{Deserialize, Serialize};

use crate::match_key::{decode_hex, hash64_hex};

/// A beacon sample plus the sink observation that dates it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BeaconFacts {
    pub epoch: u64,
    pub seed_hex: String,
    pub anchor_hash_hex: String,
    /// DAA score of the beacon's anchor (the burial reference).
    pub anchor_daa_score: u64,
    /// The node's sink DAA score when this was read (the "now" that burial is measured against).
    pub observed_daa_score: u64,
    /// Sink epoch — jobs are leased against this, not against the (older) buried sample epoch.
    pub current_epoch: u64,
}

impl BeaconFacts {
    /// The consensus-side buried-beacon struct the DA sampler consumes.
    pub fn to_buried(&self) -> Result<PalwBuriedBeaconV1, String> {
        Ok(PalwBuriedBeaconV1 {
            epoch: self.epoch,
            seed: parse_hash64(&self.seed_hex).map_err(|e| format!("beacon seed: {e}"))?,
            anchor_hash: parse_hash64(&self.anchor_hash_hex).map_err(|e| format!("beacon anchor: {e}"))?,
            anchor_daa_score: self.anchor_daa_score,
            observed_daa_score: self.observed_daa_score,
        })
    }

    pub fn seed(&self) -> Result<Hash64, String> {
        parse_hash64(&self.seed_hex)
    }
}

pub fn parse_hash64(hex: &str) -> Result<Hash64, String> {
    let bytes = decode_hex(hex)?;
    let arr: [u8; 64] = bytes.as_slice().try_into().map_err(|_| format!("expected 64 bytes, got {}", bytes.len()))?;
    Ok(Hash64::from_bytes(arr))
}

pub fn parse_outpoint(text: &str) -> Result<TransactionOutpoint, String> {
    let (txid, index) = text.rsplit_once(':').ok_or_else(|| format!("outpoint {text:?} is not txid:index"))?;
    let hash = parse_hash64(txid).map_err(|e| format!("outpoint txid: {e}"))?;
    let index: u32 = index.parse().map_err(|e| format!("outpoint index: {e}"))?;
    Ok(TransactionOutpoint::new(hash, index))
}

pub fn format_outpoint(outpoint: &TransactionOutpoint) -> String {
    format!("{}:{}", outpoint.transaction_id, outpoint.index)
}

/// A bond as the chain currently sees it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BondFacts {
    pub bond_outpoint: String,
    pub owner_pubkey_hash_hex: String,
    pub operator_group_id_hex: String,
    pub amount_sompi: u64,
    pub activation_daa_score: u64,
    /// "pending" | "active" | "unbonding" | "slashed" — the node's own derivation.
    pub effective_status: String,
    pub unbond_request_daa_score: Option<u64>,
    pub slashed_at_daa_score: Option<u64>,
    pub unbond_delay_epochs: u64,
    pub reward_key_root_hex: String,
    pub runtime_classes_hex: Vec<String>,
    pub capacity_by_shape: Vec<(u16, u32)>,
}

impl BondFacts {
    pub fn is_active(&self) -> bool {
        self.effective_status == "active"
    }

    /// Rebuild the consensus record so the REAL selectors/predicates can run on it.
    /// `owner_public_key` is supplied by the provider at registration and is cross-checked
    /// against the chain's `owner_pubkey_hash` by the caller before this is used.
    pub fn to_record(&self, owner_public_key: Vec<u8>) -> Result<PalwProviderBondRecord, String> {
        let mut runtime_classes = Vec::with_capacity(self.runtime_classes_hex.len());
        for class in &self.runtime_classes_hex {
            runtime_classes.push(parse_hash64(class).map_err(|e| format!("runtime class: {e}"))?);
        }
        Ok(PalwProviderBondRecord {
            version: 1,
            bond_outpoint: parse_outpoint(&self.bond_outpoint)?,
            owner_pubkey_hash: parse_hash64(&self.owner_pubkey_hash_hex).map_err(|e| format!("owner hash: {e}"))?,
            owner_public_key,
            operator_group_id: parse_hash64(&self.operator_group_id_hex).map_err(|e| format!("operator group: {e}"))?,
            runtime_classes,
            capacity_by_shape: self.capacity_by_shape.clone(),
            reward_key_root: parse_hash64(&self.reward_key_root_hex).map_err(|e| format!("reward key root: {e}"))?,
            amount_sompi: self.amount_sompi,
            activation_daa_score: self.activation_daa_score,
            // Not exposed by the RPC DTO; unread by every consensus function the bridge calls.
            created_daa_score: self.activation_daa_score,
            unbond_delay_epochs: self.unbond_delay_epochs,
            unbond_request_daa_score: self.unbond_request_daa_score,
            slashed_at_daa_score: self.slashed_at_daa_score,
        })
    }
}

pub trait ChainFacts: Send + Sync {
    /// The freshest FINALITY-BURIED beacon sample plus the sink's current epoch.
    fn beacon(&self) -> Result<BeaconFacts, String>;
    /// The chain's view of one bond, by `txid:index`.
    fn bond_record(&self, bond_outpoint: &str) -> Result<BondFacts, String>;
    /// ADR-0045 D3-b (Seam 5) — the VALIDATED PCPB production context for one anchor epoch, plus
    /// (when `a_commit` is named) that anchor's registration epoch. `Ok((None, _))` is a real
    /// answer: outside the retained window, or the draw beacon has not closed — the self-serial
    /// flow turns it into a wait, never into evidence built on substituted values.
    fn pcpb_context(
        &self,
        anchor_epoch: u64,
        a_commit: Option<Hash64>,
    ) -> Result<(Option<crate::pcpb::PcpbContext>, Option<u64>), String>;
    /// Human label for `/palw/v1/status` — operators must be able to see whether verdicts are
    /// backed by a live node or by pinned numbers.
    fn source_label(&self) -> String;
    fn is_live(&self) -> bool;
}

/// Cross-check that a consensus record agrees with the status the node derived, so a bug in our
/// reconstruction cannot silently promote a slashed bond.
pub fn record_status_agrees(record: &PalwProviderBondRecord, facts: &BondFacts, pov_daa_score: u64) -> bool {
    let derived = match effective_provider_bond_status(record, pov_daa_score) {
        PalwProviderBondStatus::Pending => "pending",
        PalwProviderBondStatus::Active => "active",
        PalwProviderBondStatus::Unbonding => "unbonding",
        PalwProviderBondStatus::Slashed => "slashed",
    };
    derived == facts.effective_status
}

// ---- pinned ------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PinnedFactsFile {
    pub beacon: BeaconFacts,
    #[serde(default)]
    pub bonds: BTreeMap<String, BondFacts>,
    /// ADR-0045 D3-b — pinned PCPB production contexts, keyed by anchor epoch. Like the beacon,
    /// these are frozen numbers: reproducible, not fresh.
    #[serde(default)]
    pub pcpb_anchors: BTreeMap<u64, PinnedPcpbAnchor>,
    /// Pinned A-commit registry rows (`a_commit hex → accept epoch`).
    #[serde(default)]
    pub pcpb_acommits: BTreeMap<String, u64>,
}

/// A pinned PCPB context. The commitment is REBUILT from `entries` through the same consensus
/// canonicalization the live path validates against, so a pinned file cannot describe an
/// entry-set/commitment pair that could never exist on a chain.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PinnedPcpbAnchor {
    pub snapshot_epoch: u64,
    pub draw_epoch: u64,
    pub entries: Vec<PinnedPcpbEntry>,
    pub anchor_seed_hex: String,
    /// `None` models "the draw beacon has not closed yet" — the state a self-serial flow waits in.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub draw_seed_hex: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PinnedPcpbEntry {
    pub provider_id_hex: String,
    pub ml_dsa_pk_hash_hex: String,
    pub bond_sompi: u64,
    pub reward_script_commitment_hex: String,
}

impl PinnedPcpbAnchor {
    fn to_context(&self, anchor_epoch: u64) -> Result<Option<crate::pcpb::PcpbContext>, String> {
        let Some(draw_seed_hex) = &self.draw_seed_hex else { return Ok(None) };
        let entries = self
            .entries
            .iter()
            .map(|e| {
                Ok(kaspa_consensus_core::palw::PalwProviderSnapshotEntry {
                    provider_id: parse_hash64(&e.provider_id_hex).map_err(|err| format!("provider_id: {err}"))?,
                    ml_dsa_pk_hash: parse_hash64(&e.ml_dsa_pk_hash_hex).map_err(|err| format!("ml_dsa_pk_hash: {err}"))?,
                    bond_sompi: e.bond_sompi,
                    reward_script_commitment: parse_hash64(&e.reward_script_commitment_hex)
                        .map_err(|err| format!("reward_script_commitment: {err}"))?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let commitment = kaspa_consensus_core::palw::palw_build_snapshot_witnesses(&entries).commitment;
        crate::pcpb::PcpbContext::new(
            anchor_epoch,
            self.snapshot_epoch,
            self.draw_epoch,
            commitment,
            &entries,
            parse_hash64(&self.anchor_seed_hex).map_err(|e| format!("anchor_seed: {e}"))?,
            parse_hash64(draw_seed_hex).map_err(|e| format!("draw_seed: {e}"))?,
        )
        .map(Some)
        .map_err(|e| e.to_string())
    }
}

/// Facts from a JSON file. Explicitly NOT live — see module docs.
pub struct PinnedChainFacts {
    path: String,
    facts: PinnedFactsFile,
}

impl PinnedChainFacts {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let facts: PinnedFactsFile = serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))?;
        Ok(Self { path: path.display().to_string(), facts })
    }

    pub fn from_parts(beacon: BeaconFacts, bonds: BTreeMap<String, BondFacts>) -> Self {
        Self::from_facts(PinnedFactsFile { beacon, bonds, pcpb_anchors: BTreeMap::new(), pcpb_acommits: BTreeMap::new() })
    }

    pub fn from_facts(facts: PinnedFactsFile) -> Self {
        Self { path: "<in-memory>".into(), facts }
    }
}

impl ChainFacts for PinnedChainFacts {
    fn beacon(&self) -> Result<BeaconFacts, String> {
        Ok(self.facts.beacon.clone())
    }
    fn bond_record(&self, bond_outpoint: &str) -> Result<BondFacts, String> {
        self.facts.bonds.get(bond_outpoint).cloned().ok_or_else(|| format!("bond {bond_outpoint} not in pinned facts"))
    }
    fn pcpb_context(
        &self,
        anchor_epoch: u64,
        a_commit: Option<Hash64>,
    ) -> Result<(Option<crate::pcpb::PcpbContext>, Option<u64>), String> {
        let acommit_epoch = a_commit.and_then(|c| self.facts.pcpb_acommits.get(&crate::match_key::hash64_hex(&c)).copied());
        let ctx = match self.facts.pcpb_anchors.get(&anchor_epoch) {
            Some(pinned) => pinned.to_context(anchor_epoch)?,
            // Outside the pinned window — same shape a live node answers with.
            None => None,
        };
        Ok((ctx, acommit_epoch))
    }
    fn source_label(&self) -> String {
        format!("pinned:{} (NOT live)", self.path)
    }
    fn is_live(&self) -> bool {
        false
    }
}

// ---- live wRPC ---------------------------------------------------------------------------

/// Live node facts over wRPC.
///
/// The client runs on its OWN thread with its OWN tokio runtime, and requests cross by channel.
/// The obvious alternative — `block_in_place` + `block_on` on the server's runtime — deadlocks
/// here: the bridge's HTTP dispatch is synchronous and runs inside a runtime worker, so nesting
/// a `block_on` of the same runtime wedges the connection (observed against a live node: the
/// status route never answered). A dedicated runtime also keeps the WebSocket driven while the
/// bridge is idle, because the request loop lives inside `block_on` and yields to the reactor
/// between calls.
///
/// Every call is bounded by [`CALL_TIMEOUT`]: a wedged or unreachable node must surface as an
/// error on the route that needed it, never as a hung bridge.
pub struct RpcChainFacts {
    url: String,
    requests: tokio::sync::mpsc::UnboundedSender<ChainRequest>,
    beacon_cache: Mutex<Option<(std::time::Instant, BeaconFacts)>>,
    cache_ttl: std::time::Duration,
}

/// Upper bound on one node round-trip.
const CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

struct ChainRequest {
    bond: Option<String>,
    /// ADR-0045 D3-b: `(anchor_epoch, a_commit_hex?)` — ask the node for the PCPB production context
    /// of one anchor epoch, and (optionally) that anchor's registration epoch, at ONE point of view.
    pcpb: Option<(u64, Option<String>)>,
    reply: std::sync::mpsc::Sender<Result<kaspa_rpc_core::GetPalwStateResponse, String>>,
}

impl RpcChainFacts {
    /// `node_rpc` is `host:port` (Borsh wRPC), matching `kaspa-pq-validator --node-wrpc-borsh`.
    /// Blocks until the FIRST connection succeeds, so a missing node fails loudly at startup.
    pub fn connect(node_rpc: &str) -> Result<Self, String> {
        use kaspa_rpc_core::api::rpc::RpcApi;
        use kaspa_wrpc_client::client::{ConnectOptions, ConnectStrategy};
        use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};

        let url = format!("ws://{node_rpc}");
        let (requests, mut rx) = tokio::sync::mpsc::unbounded_channel::<ChainRequest>();
        let (started_tx, started_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let thread_url = url.clone();

        std::thread::Builder::new()
            .name("palw-bridge-chain".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = started_tx.send(Err(format!("chain runtime: {e}")));
                        return;
                    }
                };
                runtime.block_on(async move {
                    let client = match KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&thread_url), None, None, None) {
                        Ok(c) => c,
                        Err(e) => {
                            let _ = started_tx.send(Err(format!("build wRPC client: {e}")));
                            return;
                        }
                    };
                    // Retry keeps the reconnect loop alive across node bounces (the validator
                    // sidecar's reasoning); block on the first connect so startup is honest.
                    let options = ConnectOptions {
                        block_async_connect: true,
                        connect_timeout: Some(std::time::Duration::from_millis(5_000)),
                        strategy: ConnectStrategy::Retry,
                        ..Default::default()
                    };
                    if let Err(e) = client.connect(Some(options)).await {
                        let _ = started_tx.send(Err(format!("connect {thread_url}: {e}")));
                        return;
                    }
                    let _ = started_tx.send(Ok(()));

                    while let Some(request) = rx.recv().await {
                        let (pcpb_anchor_epoch, pcpb_a_commit) = match request.pcpb {
                            Some((epoch, a_commit)) => (Some(epoch), a_commit),
                            None => (None, None),
                        };
                        let call = kaspa_rpc_core::GetPalwStateRequest {
                            batch_id: None,
                            provider_bond_outpoint: request.bond,
                            pcpb_anchor_epoch,
                            pcpb_a_commit,
                        };
                        let result = client.get_palw_state_call(None, call).await.map_err(|e| format!("getPalwState: {e}"));
                        let _ = request.reply.send(result);
                    }
                });
            })
            .map_err(|e| format!("spawn chain thread: {e}"))?;

        started_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .map_err(|_| format!("chain thread did not report readiness for {url}"))??;

        Ok(Self { url, requests, beacon_cache: Mutex::new(None), cache_ttl: std::time::Duration::from_secs(5) })
    }

    fn palw_state(&self, bond: Option<String>) -> Result<kaspa_rpc_core::GetPalwStateResponse, String> {
        self.palw_state_with_pcpb(bond, None)
    }

    fn palw_state_with_pcpb(
        &self,
        bond: Option<String>,
        pcpb: Option<(u64, Option<String>)>,
    ) -> Result<kaspa_rpc_core::GetPalwStateResponse, String> {
        let (reply, wait) = std::sync::mpsc::channel();
        self.requests.send(ChainRequest { bond, pcpb, reply }).map_err(|_| "chain thread has stopped".to_string())?;
        wait.recv_timeout(CALL_TIMEOUT).map_err(|_| format!("node {} did not answer within {CALL_TIMEOUT:?}", self.url))?
    }
}

impl ChainFacts for RpcChainFacts {
    fn beacon(&self) -> Result<BeaconFacts, String> {
        if let Some((at, cached)) = self.beacon_cache.lock().unwrap().as_ref()
            && at.elapsed() < self.cache_ttl
        {
            return Ok(cached.clone());
        }
        let response = self.palw_state(None)?;
        if !response.enabled {
            return Err("node reports the PALW lane disabled (below palw_activation_daa_score)".into());
        }
        let activation =
            response.activation.ok_or("node returned no activation block (wire version < 3?) — cannot read the beacon")?;
        let epoch =
            activation.newest_sample_epoch.ok_or("no buried beacon sample yet — the lane has not produced a finality-buried epoch")?;
        if activation.newest_sample_seed.is_empty() {
            return Err("buried beacon sample carries an empty seed".into());
        }
        if activation.derived_mode != "healthy" {
            // Halted/degraded means the seed is being CARRIED, not advanced: derivations would
            // silently reuse an old epoch's randomness. Refuse rather than mint against it.
            return Err(format!(
                "beacon mode is {:?} (not healthy) — refusing to derive challenges from a carried seed",
                activation.derived_mode
            ));
        }
        let facts = BeaconFacts {
            epoch,
            seed_hex: activation.newest_sample_seed,
            anchor_hash_hex: activation.anchor_hash,
            // The buried sample's anchor sits at the end of its epoch; the sampler only needs
            // `observed - anchor >= min_burial`, and the node already applied its own burial
            // rule to publish this pair.
            anchor_daa_score: epoch.saturating_mul(100),
            observed_daa_score: response.sink_daa_score,
            current_epoch: activation.current_epoch,
        };
        *self.beacon_cache.lock().unwrap() = Some((std::time::Instant::now(), facts.clone()));
        Ok(facts)
    }

    fn bond_record(&self, bond_outpoint: &str) -> Result<BondFacts, String> {
        let response = self.palw_state(Some(bond_outpoint.to_string()))?;
        let bond = response.provider_bond.ok_or_else(|| format!("bond {bond_outpoint} not found on chain"))?;
        Ok(BondFacts {
            bond_outpoint: bond.bond_outpoint,
            owner_pubkey_hash_hex: bond.owner_pubkey_hash,
            operator_group_id_hex: bond.operator_group_id,
            amount_sompi: bond.amount_sompi,
            activation_daa_score: bond.activation_daa_score,
            effective_status: bond.effective_status,
            unbond_request_daa_score: bond.unbond_request_daa_score,
            slashed_at_daa_score: bond.slashed_at_daa_score,
            unbond_delay_epochs: bond.unbond_delay_epochs,
            reward_key_root_hex: bond.reward_key_root,
            runtime_classes_hex: bond.runtime_classes,
            capacity_by_shape: bond.capacity_by_shape,
        })
    }

    /// ADR-0045 D3-b — fetch and VALIDATE the PCPB production context for `anchor_epoch`.
    ///
    /// The validation is [`crate::pcpb::PcpbContext::new`]'s: the served entry set is rebuilt with
    /// the same canonicalization consensus used and its roots must match the served commitment. So a
    /// node that serves a stale or doctored entry set is caught here, by the producer, rather than
    /// at the acceptance arm where the rejection is silent.
    ///
    /// Returns `(context, acommit_epoch)`. A missing draw seed is NOT an error: on a fresh anchor it
    /// is the normal state and means "the ordering guarantee has not matured yet" — the caller's
    /// `SelfSerialFlow::step` turns it into `AwaitDrawBeacon`.
    fn pcpb_context(
        &self,
        anchor_epoch: u64,
        a_commit: Option<Hash64>,
    ) -> Result<(Option<crate::pcpb::PcpbContext>, Option<u64>), String> {
        let response = self.palw_state_with_pcpb(None, Some((anchor_epoch, a_commit.as_ref().map(hash64_to_hex))))?;
        let served = response
            .pcpb
            .ok_or_else(|| format!("node {} did not return a PCPB context — it predates ADR-0045 D3-b (wire v6)", self.url))?;
        crate::pcpb::PcpbContext::from_rpc(&served).map_err(|e| format!("PCPB context at anchor {anchor_epoch}: {e}"))
    }

    fn source_label(&self) -> String {
        format!("live:{}", self.url)
    }

    fn is_live(&self) -> bool {
        true
    }
}

pub fn hash64_to_hex(h: &Hash64) -> String {
    hash64_hex(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beacon() -> BeaconFacts {
        BeaconFacts {
            epoch: 42,
            seed_hex: "ab".repeat(64),
            anchor_hash_hex: "cd".repeat(64),
            anchor_daa_score: 4_200,
            observed_daa_score: 4_500,
            current_epoch: 45,
        }
    }

    #[test]
    fn beacon_converts_to_the_consensus_buried_struct() {
        let buried = beacon().to_buried().unwrap();
        assert_eq!(buried.epoch, 42);
        assert_eq!(buried.observed_daa_score - buried.anchor_daa_score, 300);
        assert_eq!(hash64_hex(&buried.seed), "ab".repeat(64));
    }

    #[test]
    fn outpoint_roundtrips() {
        let text = format!("{}:7", "11".repeat(64));
        let outpoint = parse_outpoint(&text).unwrap();
        assert_eq!(outpoint.index, 7);
        assert_eq!(format_outpoint(&outpoint), text);
        assert!(parse_outpoint("nope").is_err());
        assert!(parse_outpoint("zz:1").is_err());
    }

    #[test]
    fn pinned_facts_are_labelled_not_live() {
        let mut bonds = BTreeMap::new();
        let outpoint = format!("{}:0", "22".repeat(64));
        bonds.insert(
            outpoint.clone(),
            BondFacts {
                bond_outpoint: outpoint.clone(),
                owner_pubkey_hash_hex: "33".repeat(64),
                operator_group_id_hex: "44".repeat(64),
                amount_sompi: 1_000_000_000,
                activation_daa_score: 10,
                effective_status: "active".into(),
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                unbond_delay_epochs: 6,
                reward_key_root_hex: "55".repeat(64),
                runtime_classes_hex: vec!["66".repeat(64)],
                capacity_by_shape: vec![(1, 4)],
            },
        );
        let facts = PinnedChainFacts::from_parts(beacon(), bonds);
        assert!(!facts.is_live());
        assert!(facts.source_label().contains("NOT live"));
        let bond = facts.bond_record(&outpoint).unwrap();
        assert!(bond.is_active());
        // Reconstruction agrees with the node's own status derivation.
        let record = bond.to_record(vec![0u8; 2592]).unwrap();
        assert!(record_status_agrees(&record, &bond, 4_500));
        assert!(facts.bond_record("missing:0").is_err());
    }

    #[test]
    fn slashed_bond_reconstructs_as_slashed() {
        let outpoint = format!("{}:0", "22".repeat(64));
        let bond = BondFacts {
            bond_outpoint: outpoint,
            owner_pubkey_hash_hex: "33".repeat(64),
            operator_group_id_hex: "44".repeat(64),
            amount_sompi: 1_000_000_000,
            activation_daa_score: 10,
            effective_status: "slashed".into(),
            unbond_request_daa_score: None,
            slashed_at_daa_score: Some(500),
            unbond_delay_epochs: 6,
            reward_key_root_hex: "55".repeat(64),
            runtime_classes_hex: vec![],
            capacity_by_shape: vec![],
        };
        let record = bond.to_record(vec![0u8; 2592]).unwrap();
        assert!(record_status_agrees(&record, &bond, 4_500), "a slashed bond must not reconstruct as active");
        assert!(!bond.is_active());
    }
}
