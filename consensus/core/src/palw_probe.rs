//! Bounded, read-only PALW state exposed to operator tooling.
//!
//! The probe deliberately accepts at most one batch id and one provider-bond outpoint. It reports
//! state at a named sink and never enumerates the provider registry or a batch's leaves.

use kaspa_hashes::Hash64;

use crate::{
    BlockHash,
    palw::{PalwBatchLifecycleV1, PalwBatchManifestV1, PalwProviderBondRecord, PalwProviderBondStatus},
    tx::TransactionOutpoint,
};

/// Fork-relative carried state for one requested batch at `PalwStateProbe::sink`.
///
/// The lifecycle view is built from raw blue-mergeset carriers before acceptance filtering. Manifest,
/// leaf, and certificate bytes live in global content-addressed stores. This is therefore a bounded
/// diagnostic of the exact surfaces ticket resolution reads, not proof that a carrier was accepted on
/// the selected chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBatchProbe {
    pub batch_id: Hash64,
    pub lifecycle: PalwBatchLifecycleV1,
    /// The content-addressed manifest resolved for this fork-local lifecycle entry, if the blob exists.
    pub manifest: Option<PalwBatchManifestV1>,
    /// Number of leaf blobs present, scanned only up to the activated network's bounded batch limit.
    pub leaf_blobs_present: u32,
    /// False only if a corrupt/legacy lifecycle claims more leaves than the bounded scan limit.
    pub leaf_scan_complete: bool,
    /// Whether the fork-local `cert_hash`, when present, resolves to a certificate blob. This is a
    /// presence check, not fork-scoped proof that the certificate was verified at this sink: certificate
    /// blobs remain globally content-addressed until the attestation-provenance blocker is closed.
    pub certificate_blob_present: bool,
}

/// Selected-chain registry state for one requested provider-bond outpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwProviderBondProbe {
    pub record: PalwProviderBondRecord,
    pub effective_status: PalwProviderBondStatus,
    pub release_daa_score: Option<u64>,
}

/// One open data-availability challenge on the requested provider bond, at `PalwStateProbe::sink`.
///
/// Reported only when the request names a provider bond and only for `Open` challenges — enough for
/// an off-node, owner-key-holding responder to build and submit a deadline-aware 0x3b response. The
/// node itself never holds owner keys or signs; this read-only surface is how the responder discovers
/// what needs answering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwDaChallengeProbe {
    pub challenge_id: Hash64,
    pub provider_bond: TransactionOutpoint,
    pub object_root: Hash64,
    pub chunk_index: u16,
    pub opened_daa_score: u64,
    pub response_deadline_daa_score: u64,
}

/// One OPEN search-availability challenge whose obligation is anchored by the requested provider
/// bond (the scheduler's), at `PalwStateProbe::sink`.
///
/// Same contract as [`PalwDaChallengeProbe`]: reported only when the request names a provider bond
/// and only for `Challenged` obligations — enough for an off-node responder holding the snapshot
/// bytes to build and submit a deadline-aware 0x3e chunk-proof response (which is unsigned by
/// design; the proof itself is the authorization).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSearchChallengeProbe {
    pub object_root: Hash64,
    pub scheduler_bond: TransactionOutpoint,
    pub chunk_index: u16,
    pub response_deadline_daa_score: u64,
    pub availability_deadline_daa_score: u64,
}

/// The lagged beacon-activation signal, derived at `PalwStateProbe::sink` with the EXACT helpers the
/// virtual processor's `commit_palw_overlay_effects` gate uses (`resolve_palw_lagged_anchor` →
/// `resolve_palw_buried_epoch_seeds` → `palw_lagged_activation_open`).
///
/// Honest-coordinate caveat: the gate evaluates at `selected_parent(current)` while committing chain
/// block `current`. This probe runs the identical walk at the sink S, so it reports exactly the
/// samples `advance_epoch_gated` will consume for the NEXT chain block whose selected parent is S —
/// a forward-looking derivation with the same function and inputs, not a replay of the last commit.
/// Consensus consults `activation_open` only when a Certified batch is epoch-eligible; this field is
/// the pure beacon-signal half of that conjunction, exposed so operators can gate a mock lifecycle on
/// the REAL activation predicate instead of a `dns_health` poll proxy (review §6.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwActivationProbe {
    /// `palw_lagged_activation_open(samples)`: the two newest buried per-epoch beacon seeds differ.
    /// Fail-closed `false` when fewer than 2 samples resolve (including "no buried anchor yet").
    pub activation_open: bool,
    /// Newest buried `(palw_epoch, beacon_seed)` sample, if any.
    pub newest_sample: Option<(u64, Hash64)>,
    /// Second-newest buried sample — the one `activation_open` compares the newest against.
    pub previous_sample: Option<(u64, Hash64)>,
    /// How many buried per-epoch samples resolved (walk cap: `palw_beacon_grace_epochs + 2`).
    pub buried_sample_count: u64,
    /// `palw_seed_carry_run(samples)` — consecutive newest epochs carrying the SAME seed. The mint
    /// lane is open iff this is `<= grace_epochs` (clause 10).
    pub buried_carry_run: u64,
    /// The finality-buried DNS anchor the walk started from (`None` ⇒ no anchor ⇒ fail-closed).
    pub anchor_hash: Option<BlockHash>,
    /// `sink_daa_score / palw_epoch_length_daa`.
    pub current_epoch: u64,
    /// `Params::palw_beacon_grace_epochs`, echoed so a client can evaluate the lane predicate.
    pub grace_epochs: u64,
    /// The sink's own persisted `PalwBeaconStateV1.mode` (0 Healthy / 1 DegradedGrace / 2 Halted),
    /// when present. This is the exact per-block derived mode — distinct from the LAGGED buried
    /// signal above; do not conflate them.
    pub derived_mode: Option<u8>,
    /// The sink's persisted `PalwBeaconStateV1.degraded_epochs`, when present.
    pub derived_degraded_epochs: Option<u64>,
}

/// One bounded operator probe pinned to a named virtual sink.
///
/// The sink, its immutable carried view, and the selected-chain provider registry are chosen under the
/// virtual-state read lock. Direct global blob writes do not take that lock, so manifest/leaf/certificate
/// availability is diagnostic and is not part of an atomic or fork-scoped acceptance snapshot. A
/// missing requested lifecycle/provider object is represented by `None`; global blobs are never used to
/// invent a lifecycle entry absent from the sink view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStateProbe {
    pub enabled: bool,
    pub sink: BlockHash,
    pub sink_daa_score: u64,
    pub overlay_view_available: bool,
    pub batch: Option<PalwBatchProbe>,
    pub provider_bond: Option<PalwProviderBondProbe>,
    /// Open DA challenges on the requested provider bond. Empty unless a provider bond was requested.
    pub da_challenges: Vec<PalwDaChallengeProbe>,
    /// Open search-availability challenges anchored by the requested provider bond (as scheduler).
    /// Empty unless a provider bond was requested.
    pub search_challenges: Vec<PalwSearchChallengeProbe>,
    /// The lagged activation signal at the sink. `None` when PALW is disabled or the preset has no
    /// `dns_params` (the walk is undefined there).
    pub activation: Option<PalwActivationProbe>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwStateProbeError {
    #[error("PALW state-store read failed: {0}")]
    Store(String),
}
