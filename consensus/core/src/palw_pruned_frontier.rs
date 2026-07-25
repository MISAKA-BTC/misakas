//! Versioned PALW pruning-point frontier transport.
//!
//! [`crate::palw::PalwPrunedFrontierV1`] is only the block-keyed execution frontier. A usable
//! pruning boundary also needs the fork-local beacon accumulator, the selected-chain provider-bond
//! registry, and the below-boundary part of the paid-work window. This module keeps that complete
//! transport object separate from the header and from the legacy frontier type, so adding it does not
//! change the encoding of an already-live PALW consensus object. Header-v3 keeps its pinned commitment
//! bytes; a re-genesis-only Header-v4 folds the live subset into a new, disjoint v2 commitment.

use crate::{
    BlockHash,
    dns_finality::{STAKE_VALIDATOR_PUBKEY_LEN, validator_id_from_pubkey},
    palw::{
        PALW_MAX_PROVIDER_CAPACITY_ENTRIES_V1, PALW_MAX_PROVIDER_RUNTIME_CLASSES_V1, PalwBatchCertificateV2, PalwBatchManifestV1,
        PalwBatchViewV1, PalwBeaconEpochAccumV1, PalwProviderBondRecord, PalwPrunedFrontierV1, PalwPublicLeafV1,
        da::PalwDaPruningSnapshotV1, palw_leaf_merkle_root,
        search_snapshot::{PalwSearchAvailabilityStateV1, PalwSearchPruningSnapshotV1},
    },
    tx::TransactionOutpoint,
};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash64, blake2b_512_keyed};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::BTreeMap,
    fmt::{Display, Formatter},
    io::Write,
    str::FromStr,
};
use thiserror::Error;

pub const PALW_PRUNING_SNAPSHOT_VERSION: u16 = 2;
pub const PALW_PRUNING_SNAPSHOT_DIGEST_DOMAIN: &[u8] = b"MISAKA_PALW_PRUNED_FRONTIER_V2";
pub const PALW_ACTIVE_BATCH_REF_DIGEST_DOMAIN: &[u8] = b"MISAKA_PALW_ACTIVE_BATCH_REFS_V1";
/// Version/domain for the complete selected-parent PALW state digest carried indirectly by the
/// Header-v4 `overlay_commitment_root`. This is intentionally disjoint from the pruning-sidecar
/// digest: the latter transports recovery rows, while this digest commits exactly the live state
/// that can affect a child transition.
pub const PALW_SELECTED_PARENT_STATE_VERSION: u16 = 2;
pub const PALW_SELECTED_PARENT_STATE_DIGEST_DOMAIN: &[u8] = b"MISAKA_PALW_SELECTED_PARENT_STATE_V2";

/// Hard decode-independent cardinality ceilings. P2P additionally limits the encoded byte length
/// before Borsh deserialization. These bounds keep non-P2P import callers from bypassing that limit.
pub const MAX_PALW_PRUNING_PROVIDER_BONDS: usize = 32_768;
pub const MAX_PALW_PRUNING_BEACON_EPOCHS: usize = 4;
pub const MAX_PALW_PRUNING_BEACON_ROWS: usize = 32_768;
pub const MAX_PALW_PRUNING_PAID_BLOCKS: usize = 1_000_001;
pub const MAX_PALW_PRUNING_PAID_IDS: usize = 1_000_000;
pub const MAX_PALW_PRUNING_ACTIVE_NULLIFIERS: usize = 1_000_000;
pub const MAX_PALW_PRUNING_ACTIVE_BATCHES: usize = 1_024;
pub const MAX_PALW_PRUNING_ACTIVE_LEAVES: usize = 262_144;
/// Leaves and quorum certificates dominate the sidecar. Keep their canonical Borsh payload below
/// the 128-MiB P2P envelope with room for DA/provider/spam state and framing.
pub const MAX_PALW_PRUNING_ACTIVE_BLOB_BYTES: usize = 96 << 20;
/// Exact outer Borsh envelope limit shared by core validation and P2P. Component cardinality
/// ceilings are defense in depth; no combination of individually valid components may produce a
/// snapshot which honest peers are unable to serve.
pub const MAX_PALW_PRUNING_SNAPSHOT_BYTES: usize = 128 << 20;
/// Header-v4 transports exactly one power-of-two selected-parent checkpoint. The active parameter
/// ceiling fixes that checkpoint, and therefore the support-vector ceiling, at 65,536 rows.
pub const MAX_PALW_PRUNING_SPAM_SUPPORT_ROWS: usize = 65_536;

/// An operator-authenticated pruning boundary. The value is deliberately the digest of the complete
/// canonical snapshot payload, rather than a root of selected fields, so it also authenticates every
/// Header-v4 anti-spam support row transported by the sidecar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPruningSnapshotCheckpoint {
    pub pruning_point: BlockHash,
    pub payload_digest: Hash64,
}

impl Display for PalwPruningSnapshotCheckpoint {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.pruning_point, self.payload_digest)
    }
}

impl FromStr for PalwPruningSnapshotCheckpoint {
    type Err = PalwPruningSnapshotCheckpointParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((pruning_point, payload_digest)) = value.split_once(':') else {
            return Err(PalwPruningSnapshotCheckpointParseError::InvalidShape);
        };
        if pruning_point.is_empty() || payload_digest.is_empty() || payload_digest.contains(':') {
            return Err(PalwPruningSnapshotCheckpointParseError::InvalidShape);
        }
        let pruning_point =
            BlockHash::from_str(pruning_point).map_err(|_| PalwPruningSnapshotCheckpointParseError::InvalidPruningPointHash)?;
        let payload_digest =
            Hash64::from_str(payload_digest).map_err(|_| PalwPruningSnapshotCheckpointParseError::InvalidPayloadDigest)?;
        Ok(Self { pruning_point, payload_digest })
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum PalwPruningSnapshotCheckpointParseError {
    #[error("expected exactly <128-hex-pruning-point>:<128-hex-snapshot-payload-digest>")]
    InvalidShape,
    #[error("pruning-point hash must be exactly 128 hexadecimal characters")]
    InvalidPruningPointHash,
    #[error("snapshot payload digest must be exactly 128 hexadecimal characters")]
    InvalidPayloadDigest,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum PalwPruningSnapshotCheckpointSetError {
    #[error("duplicate PALW pruning snapshot checkpoint for {0}")]
    Duplicate(BlockHash),
    #[error("conflicting PALW pruning snapshot checkpoints for {pruning_point}: {first_digest} versus {second_digest}")]
    Conflict { pruning_point: BlockHash, first_digest: Hash64, second_digest: Hash64 },
}

/// Reject both repeated identical values and two operator claims for the same pruning point. Silently
/// taking the first or last value would make command-line/config ordering security-sensitive.
pub fn validate_palw_pruning_snapshot_checkpoints(
    checkpoints: &[PalwPruningSnapshotCheckpoint],
) -> Result<(), PalwPruningSnapshotCheckpointSetError> {
    let mut seen = BTreeMap::new();
    for checkpoint in checkpoints {
        if let Some(previous_digest) = seen.insert(checkpoint.pruning_point, checkpoint.payload_digest) {
            return if previous_digest == checkpoint.payload_digest {
                Err(PalwPruningSnapshotCheckpointSetError::Duplicate(checkpoint.pruning_point))
            } else {
                Err(PalwPruningSnapshotCheckpointSetError::Conflict {
                    pruning_point: checkpoint.pruning_point,
                    first_digest: previous_digest,
                    second_digest: checkpoint.payload_digest,
                })
            };
        }
    }
    Ok(())
}

/// The provenance carried across the P2P/consensus API boundary. Header-v3 preserves its historical
/// closed-network path. Header-v4 is a separate capability and cannot be enabled by merely relabeling
/// a peer-advertised trusted-data digest as operator trust.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwPruningSnapshotImportProvenance {
    LegacyHeaderV3,
    OperatorPinnedCheckpoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPruningSnapshotImportAuth {
    pub checkpoint: PalwPruningSnapshotCheckpoint,
    pub provenance: PalwPruningSnapshotImportProvenance,
}

impl PalwPruningSnapshotImportAuth {
    pub fn legacy_header_v3(pruning_point: BlockHash, payload_digest: Hash64) -> Self {
        Self {
            checkpoint: PalwPruningSnapshotCheckpoint { pruning_point, payload_digest },
            provenance: PalwPruningSnapshotImportProvenance::LegacyHeaderV3,
        }
    }

    pub fn operator_pinned(checkpoint: PalwPruningSnapshotCheckpoint) -> Self {
        Self { checkpoint, provenance: PalwPruningSnapshotImportProvenance::OperatorPinnedCheckpoint }
    }
}

/// Exact-version admission policy for peer snapshot import. Future header versions remain closed
/// until they receive a distinct reviewed authentication policy.
pub fn palw_pruned_ibd_snapshot_import_allowed(header_version: u16, auth: &PalwPruningSnapshotImportAuth) -> bool {
    matches!(
        (header_version, auth.provenance),
        (crate::constants::PALW_HEADER_VERSION, PalwPruningSnapshotImportProvenance::LegacyHeaderV3)
            | (crate::constants::PALW_ANTISPAM_HEADER_VERSION, PalwPruningSnapshotImportProvenance::OperatorPinnedCheckpoint)
    )
}

// ---------------------------------------------------------------------------
// Permissionless (chain-derived) snapshot authentication — foundational primitives.
//
// The operator-pinned checkpoint above authenticates an imported boundary by a single
// out-of-band digest of the whole payload. A permissionless network cannot assume that
// trust anchor. Removing it requires reproducing, from the chain itself, the two things
// the operator digest currently supplies transitively:
//
//   1. the imported live state must be authenticated *before* the durable write, by
//      reconstructing the selected-parent PALW state from the transported payload and
//      checking that its `state_root()` folds into a proof-of-work authenticated
//      descendant header's `overlay_commitment_root` (the `c == v` check, moved ahead
//      of installation); and
//   2. every transported Header-v4 anti-spam support row must be bound to a transported
//      header commitment, not merely shape/monotonicity/closure-checked.
//
// The functions below are the pure, reviewable core of (1) and (2). They deliberately
// do NOT change any live import path, do NOT relax `palw_pruned_ibd_snapshot_import_allowed`
// (Header-v4 peer import stays fenced), and do NOT themselves verify proof-of-work.
// Authenticating the transported header bundle (PoW + selected-chain linkage) and moving
// the durable write are the wiring steps specified in
// `docs/adr-permissionless-snapshot-authentication.md`; they remain a reviewed StopShip.
// Landing the cryptographic binding first keeps it independently testable before it is
// trusted by consensus.
// ---------------------------------------------------------------------------

/// A single transported Header-v4 fact: a block hash and the anti-spam accumulator
/// commitment carried in that block's `palw_spam_accumulator_commitment` header field.
///
/// The wiring layer must independently authenticate the originating header (proof of work
/// and selected-chain linkage) before these facts are trusted; this type carries only the
/// committed value a support row is bound against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct TransportedSpamHeaderCommitmentV1 {
    pub block_hash: BlockHash,
    pub spam_accumulator_commitment: Hash64,
}

/// Bind every transported anti-spam row to a transported header commitment (gap 2).
///
/// `PalwPruningPointSnapshotV1::validate_canonical` only checks the support rows' shape,
/// monotonicity, and (via the consensus-crate closure validator) selected-parent/skip
/// completeness. It does not check that each row's accumulator value equals the value the
/// *header* of that block committed. Under an operator pin the whole-payload digest covers
/// the rows transitively; without a pin they must be bound to headers directly.
///
/// This function requires the transported header set to be exactly the pruning-point row
/// plus every support row — no missing, no extra, canonical strictly-increasing order — and
/// each header commitment to equal the corresponding row's `commitment()`. It authenticates
/// the *binding*, not the headers themselves (see the module note above).
pub fn verify_support_rows_against_transported_headers(
    spam: &PalwPrunedSpamFrontierV1,
    pruning_point: BlockHash,
    headers: &[TransportedSpamHeaderCommitmentV1],
) -> Result<(), PalwPruningSnapshotError> {
    // Defense in depth: the header set can never exceed the pruning-point row plus the
    // bounded support vector. This also short-circuits a pathological oversized input.
    if headers.len() > MAX_PALW_PRUNING_SPAM_SUPPORT_ROWS.saturating_add(1) {
        return Err(PalwPruningSnapshotError::TooMany("transported spam headers", headers.len()));
    }
    // Expected (block, commitment) facts: the pruning-point row plus every support row.
    let mut expected: Vec<(BlockHash, Hash64)> = Vec::with_capacity(spam.support_rows.len().saturating_add(1));
    expected.push((pruning_point, spam.pruning_point_state.commitment()));
    for row in &spam.support_rows {
        expected.push((row.block_hash, row.state.commitment()));
    }
    expected.sort_by(|a, b| a.0.as_bytes().cmp(&b.0.as_bytes()));
    // Strictly increasing => no duplicate row, and the pruning point is not aliased by one.
    if !expected.windows(2).all(|w| w[0].0.as_bytes() < w[1].0.as_bytes()) {
        return Err(PalwPruningSnapshotError::Incoherent("support rows alias the pruning point or duplicate a block hash"));
    }
    if headers.len() != expected.len() {
        return Err(PalwPruningSnapshotError::Incoherent("transported spam header set size does not match the support rows"));
    }
    if !headers.windows(2).all(|w| w[0].block_hash.as_bytes() < w[1].block_hash.as_bytes()) {
        return Err(PalwPruningSnapshotError::NonCanonical("transported spam headers"));
    }
    for (header, (block_hash, commitment)) in headers.iter().zip(expected.iter()) {
        if header.block_hash != *block_hash {
            return Err(PalwPruningSnapshotError::Incoherent("transported spam header has no matching support row"));
        }
        if header.spam_accumulator_commitment != *commitment {
            return Err(PalwPruningSnapshotError::Incoherent("transported spam header commitment does not bind its row"));
        }
    }
    Ok(())
}

/// Reconstruct the selected-parent PALW state that a first post-pruning-point Header-v4
/// child authenticates (`c == v`), sourced from the transported pruning payload instead of
/// installed stores (gap 1).
///
/// `paid_work_nullifiers` and `da_state_root` are the two derived values the authoritative
/// overlay-commitment path computes from the paid-work window and the DA snapshot; the
/// caller supplies them so this stays a pure, store-free bridge. Soundness does not rely on
/// the caller deriving them correctly: the resulting `state_root()` is compared against a
/// proof-of-work authenticated descendant header, so any wrong field — derived or
/// transported — fails that comparison. The returned value is canonical-validated by
/// [`PalwSelectedParentStateV2::try_new`].
pub fn reconstruct_selected_parent_state_from_pruning_payload(
    payload: &PalwPruningPointSnapshotPayloadV1,
    paid_work_nullifiers: Vec<Hash64>,
    da_state_root: Hash64,
) -> Result<PalwSelectedParentStateV2, PalwPruningSnapshotError> {
    let active_batch_ref_root = palw_active_batch_ref_root(payload.frontier.overlay_view.as_ref());
    let search_availability_state_root = palw_pruning_payload_search_availability_state_root(payload);
    PalwSelectedParentStateV2::try_new(
        payload.pruning_point,
        payload.pruning_point_daa_score,
        payload.frontier.clone(),
        payload.beacon_accumulator.clone(),
        payload.provider_bonds.clone(),
        paid_work_nullifiers,
        da_state_root,
        search_availability_state_root,
        active_batch_ref_root,
    )
}

/// Compose the full chain-derived pre-install authentication of a transported pruning boundary.
///
/// This is the single check the importer runs, for the `ChainDerivedHeaderBundle` provenance, BEFORE
/// staging any durable row — replacing the operator-pin trust anchor with chain-derived facts:
///
/// 1. Reconstruct the selected-parent PALW state from the transported payload and fold it exactly as
///    Header-v4 does (`palw_overlay_commitment_root_v2(legacy_overlay_root, palw_state_root)`), then
///    require it to equal the descendant child's committed `overlay_commitment_root` — the `c == v`
///    check, moved ahead of installation (gap 1).
/// 2. Bind every transported anti-spam support row to a transported header commitment (gap 2).
///
/// Soundness rests on two facts the *wiring* layer must supply and authenticate independently (this
/// pure function does neither): `descendant_overlay_commitment_root` must come from a proof-of-work
/// authenticated first-post-pruning-point Header-v4 child, and `legacy_overlay_root` must be the
/// `commitment_root()` of the transported, authenticated DNS/EVM `OverlaySnapshot`. Given those, any
/// tampering of the transported payload — in any field the selected-parent `state_root` covers, or in
/// the derived paid-work / DA values — changes the fold and fails the comparison against the honest
/// child's commitment. `paid_work_nullifiers` and `da_state_root` are the two window/DA-derived values
/// the caller supplies from the transported payload; the caller need not derive them trustfully because
/// a wrong value simply fails the comparison.
#[allow(clippy::too_many_arguments)]
pub fn verify_chain_derived_pruning_boundary(
    payload: &PalwPruningPointSnapshotPayloadV1,
    paid_work_nullifiers: Vec<Hash64>,
    da_state_root: Hash64,
    legacy_overlay_root: Hash64,
    descendant_overlay_commitment_root: Hash64,
    support_row_headers: &[TransportedSpamHeaderCommitmentV1],
) -> Result<(), PalwPruningSnapshotError> {
    let state = reconstruct_selected_parent_state_from_pruning_payload(payload, paid_work_nullifiers, da_state_root)?;
    let expected = crate::dns_finality::palw_overlay_commitment_root_v2(&legacy_overlay_root, &state.state_root());
    if expected != descendant_overlay_commitment_root {
        return Err(PalwPruningSnapshotError::Incoherent(
            "descendant overlay commitment does not authenticate the transported PALW state",
        ));
    }
    match &payload.spam_accumulator {
        Some(spam) => verify_support_rows_against_transported_headers(spam, payload.pruning_point, support_row_headers)?,
        None if !support_row_headers.is_empty() => {
            return Err(PalwPruningSnapshotError::Incoherent("support-row headers supplied for a snapshot without an anti-spam frontier"));
        }
        None => {}
    }
    Ok(())
}

/// Derive the active paid-work nullifier set the first post-pruning-point child folds into its
/// selected-parent state, sourced from the transported payload.
///
/// Provably equal to `palw_paid_work_window(pruning_point, pp_daa)` at the pruning anchor: that walk
/// unions the persisted snapshot's below-boundary window with a backward selected-chain walk from the
/// anchor, but the backward walk stops on its first element because its `stop_at` IS the anchor. So only
/// the transported window within `walk_bound` contributes. `walk_bound_daa` is the network's
/// `PalwBatchAdmission::paid_work_walk_bound_daa(palw_epoch_length_daa)`. The result is deduplicated and
/// sorted by `Hash64::as_bytes()`, matching `PalwSelectedParentStateV2`'s canonical order.
pub fn palw_pruning_payload_paid_work_nullifiers(payload: &PalwPruningPointSnapshotPayloadV1, walk_bound_daa: u64) -> Vec<Hash64> {
    let mut nullifiers: Vec<Hash64> = Vec::new();
    for row in &payload.paid_work {
        if payload.pruning_point_daa_score.saturating_sub(row.block_daa_score) <= walk_bound_daa {
            nullifiers.extend(row.job_nullifiers.iter().copied());
        }
    }
    nullifiers.sort_by(|a, b| a.as_bytes().cmp(&b.as_bytes()));
    nullifiers.dedup();
    nullifiers
}

/// Derive the DA state root the first post-pruning-point child folds into its selected-parent state,
/// sourced from the transported payload.
///
/// Provably equal to `palw_da_parent_state(pruning_point, pp_daa).state_root()`: that method returns the
/// stored DA state at the selected parent verbatim (`state(pp).clone()`), which is exactly the state the
/// pruning snapshot transports in `da_snapshot` (canonical validation ties `da_snapshot.pruning_point`
/// to the payload's pruning point). Absent a DA snapshot (pre-activation / empty), both sides are the
/// default DA state's root.
pub fn palw_pruning_payload_da_state_root(payload: &PalwPruningPointSnapshotPayloadV1) -> Hash64 {
    match &payload.da_snapshot {
        Some(snapshot) => snapshot.state.state_root(),
        None => crate::palw::da::PalwDaStateV1::default().state_root(),
    }
}

/// Derive the search-availability state root the first post-pruning-point child folds into its
/// selected-parent state, sourced from the transported payload. Same contract as
/// [`palw_pruning_payload_da_state_root`]: absent a snapshot (pre-activation / empty), both sides
/// are the default state's root, and a wrong transported value simply fails the fold comparison.
pub fn palw_pruning_payload_search_availability_state_root(payload: &PalwPruningPointSnapshotPayloadV1) -> Hash64 {
    match &payload.search_availability_snapshot {
        Some(snapshot) => snapshot.state.state_root(),
        None => PalwSearchAvailabilityStateV1::default().state_root(),
    }
}

/// The bounded, chain-derived authentication bundle transported alongside a permissionless Header-v4
/// pruning snapshot. It carries exactly the facts [`verify_chain_derived_pruning_boundary`] consumes.
///
/// The wiring (P2P) layer produces these facts only after PoW/target-validating the descendant
/// Header-v4 header(s) — from which `descendant_overlay_commitment_root` is taken — and authenticating
/// the transported DNS/EVM overlay snapshot — whose `commitment_root()` is `legacy_overlay_root`.
/// Transporting and PoW-validating the full headers is the P2P layer's job; this type carries the
/// authenticated results the pure verifier compares against.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct PalwChainDerivedAuthBundleV1 {
    pub descendant_overlay_commitment_root: Hash64,
    pub legacy_overlay_root: Hash64,
    pub support_row_headers: Vec<TransportedSpamHeaderCommitmentV1>,
}

/// Whether chain-derived (permissionless) Header-v4 snapshot import is admitted. Fail-closed: only when
/// the node-local lever (`Config::palw_permissionless_snapshot_auth`) is set AND the header version is
/// exactly Header-v4. Otherwise peer import stays fenced to the closed-network v3 and operator-pinned v4
/// paths regardless of what a peer advertises. Admission is a necessary precondition, not sufficient:
/// the importer must additionally pass [`verify_chain_derived_pruning_boundary`] before any durable
/// write.
pub fn palw_pruned_ibd_chain_derived_import_allowed(permissionless_enabled: bool, header_version: u16) -> bool {
    permissionless_enabled && header_version == crate::constants::PALW_ANTISPAM_HEADER_VERSION
}

/// The single chain-derived pre-install verification the importer runs before staging any durable row:
/// derive the two window/DA values from the transported payload, then authenticate the boundary against
/// the PoW-authenticated facts in `bundle`. `walk_bound_daa` is the receiving network's
/// `PalwBatchAdmission::paid_work_walk_bound_daa(palw_epoch_length_daa)`.
///
/// The caller must have already checked [`palw_pruned_ibd_chain_derived_import_allowed`] and
/// authenticated the bundle (PoW of the descendant header, authenticity of the DNS overlay snapshot
/// whose `commitment_root()` is `bundle.legacy_overlay_root`). This composition is fail-closed: any
/// tampering of the transported payload changes a derived value and fails the fold comparison.
pub fn verify_chain_derived_pruning_boundary_from_payload(
    payload: &PalwPruningPointSnapshotPayloadV1,
    walk_bound_daa: u64,
    bundle: &PalwChainDerivedAuthBundleV1,
) -> Result<(), PalwPruningSnapshotError> {
    let paid_work_nullifiers = palw_pruning_payload_paid_work_nullifiers(payload, walk_bound_daa);
    let da_state_root = palw_pruning_payload_da_state_root(payload);
    verify_chain_derived_pruning_boundary(
        payload,
        paid_work_nullifiers,
        da_state_root,
        bundle.legacy_overlay_root,
        bundle.descendant_overlay_commitment_root,
        &bundle.support_row_headers,
    )
}

/// The P2P-transported header bundle for permissionless Header-v4 snapshot import. Unlike the
/// authenticated [`PalwChainDerivedAuthBundleV1`] (extracted roots only), this carries the full
/// descendant Header-v4 header(s), the below-boundary support-row headers, and the DNS/EVM overlay
/// snapshot, so the receiver can PoW/target-validate the headers and authenticate the overlay snapshot
/// before extracting the trusted roots.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct PalwChainDerivedHeaderBundleWireV1 {
    pub descendant_header: crate::header::Header,
    pub support_headers: Vec<crate::header::Header>,
    pub dns_overlay_snapshot: crate::dns_finality::OverlaySnapshot,
}

impl PalwChainDerivedHeaderBundleWireV1 {
    /// Project the already-authenticated headers + overlay snapshot into the roots the pure verifier
    /// consumes.
    ///
    /// TRUST BOUNDARY: the caller MUST have PoW/target-validated `descendant_header` and every
    /// `support_headers` entry, and authenticated `dns_overlay_snapshot`, before calling this. This
    /// function performs NO authentication — it only enforces structural bounds and projects header
    /// fields. The proof-of-work authentication that makes these facts trustworthy is the security
    /// anchor and is deliberately kept out of this pure projection (task 1d, needs independent review).
    pub fn extract_authenticated_bundle(&self) -> Result<PalwChainDerivedAuthBundleV1, PalwPruningSnapshotError> {
        if self.support_headers.len() > MAX_PALW_PRUNING_SPAM_SUPPORT_ROWS {
            return Err(PalwPruningSnapshotError::TooMany("chain-derived support headers", self.support_headers.len()));
        }
        if self.descendant_header.version != crate::constants::PALW_ANTISPAM_HEADER_VERSION {
            return Err(PalwPruningSnapshotError::Incoherent("chain-derived descendant header is not Header-v4"));
        }
        let mut support_row_headers: Vec<TransportedSpamHeaderCommitmentV1> = self
            .support_headers
            .iter()
            .map(|header| TransportedSpamHeaderCommitmentV1 {
                block_hash: header.hash,
                spam_accumulator_commitment: header.palw_spam_accumulator_commitment,
            })
            .collect();
        support_row_headers.sort_by(|a, b| a.block_hash.as_bytes().cmp(&b.block_hash.as_bytes()));
        Ok(PalwChainDerivedAuthBundleV1 {
            descendant_overlay_commitment_root: self.descendant_header.overlay_commitment_root,
            legacy_overlay_root: self.dns_overlay_snapshot.commitment_root(),
            support_row_headers,
        })
    }
}

struct BoundedSizeWriter {
    written: usize,
    limit: usize,
}

impl Write for BoundedSizeWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let next = self.written.checked_add(bytes.len()).ok_or_else(|| std::io::Error::other("PALW snapshot size overflow"))?;
        if next > self.limit {
            self.written = next;
            return Err(std::io::Error::other("PALW snapshot exceeds its encoded limit"));
        }
        self.written = next;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn validate_borsh_encoded_size<T: BorshSerialize>(value: &T, limit: usize) -> Result<usize, PalwPruningSnapshotError> {
    let mut writer = BoundedSizeWriter { written: 0, limit };
    if BorshSerialize::serialize(value, &mut writer).is_err() {
        return Err(PalwPruningSnapshotError::TooMany("encoded snapshot bytes", writer.written.max(limit.saturating_add(1))));
    }
    Ok(writer.written)
}

fn cmp_outpoint(a: &TransactionOutpoint, b: &TransactionOutpoint) -> Ordering {
    a.transaction_id.as_bytes().cmp(&b.transaction_id.as_bytes()).then(a.index.cmp(&b.index))
}

fn outpoints_are_strictly_sorted<T>(rows: &[(TransactionOutpoint, T)]) -> bool {
    rows.windows(2).all(|w| cmp_outpoint(&w[0].0, &w[1].0).is_lt())
}

/// Core representation of the consensus-crate fork-local beacon accumulator at the pruning point.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct PalwPrunedBeaconAccumulatorV1 {
    pub version: u16,
    pub epochs: BTreeMap<u64, PalwBeaconEpochAccumV1>,
    pub stake_by_epoch: BTreeMap<u64, Vec<(TransactionOutpoint, u64)>>,
}

impl PalwPrunedBeaconAccumulatorV1 {
    pub fn new() -> Self {
        Self { version: 1, epochs: BTreeMap::new(), stake_by_epoch: BTreeMap::new() }
    }

    fn canonicalize(&mut self) {
        for accum in self.epochs.values_mut() {
            accum.commits.sort_by(|a, b| cmp_outpoint(&a.0, &b.0));
            accum.valid_reveals.sort_by(|a, b| cmp_outpoint(&a.0, &b.0));
        }
        for rows in self.stake_by_epoch.values_mut() {
            rows.sort_by(|a, b| cmp_outpoint(&a.0, &b.0));
        }
    }
}

/// Complete PALW state as of a Header-v4 block's selected parent.
///
/// The digest of this value is folded into Header-v4's versioned overlay commitment. Consequently,
/// the first child of an imported pruning point authenticates every transported state component
/// before accepting a transition derived from it (`c == v`). `paid_work_nullifiers` is the sorted
/// set returned by the active paid-work window, rather than its transport-only per-block rows: that
/// set is exactly the state consumed by duplicate-payment validation.
///
/// Header-v4 anti-spam support rows are deliberately absent. The current selected-parent row is
/// already committed directly by that parent's Header-v4 `palw_spam_accumulator_commitment`.
/// Support rows are validation witnesses, not independently mutable child state, so folding the same
/// current row into this digest would be a duplicate commitment. Canonical shape/monotonicity alone
/// does not authenticate every historical support row, however; Header-v4 peer import remains fenced
/// until those witnesses are recursively committed or checked against transported headers before
/// durable installation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct PalwSelectedParentStateV2 {
    pub version: u16,
    pub selected_parent: BlockHash,
    pub selected_parent_daa_score: u64,
    pub frontier: PalwPrunedFrontierV1,
    pub beacon_accumulator: Option<PalwPrunedBeaconAccumulatorV1>,
    pub provider_bonds: Vec<PalwProviderBondRecord>,
    pub paid_work_nullifiers: Vec<Hash64>,
    pub da_state_root: Hash64,
    /// Canonical digest of the search-availability state at the selected parent
    /// (`PalwSearchAvailabilityStateV1::state_root`). Bound exactly like `da_state_root`: the first
    /// post-pruning-point Header-v4 child authenticates the transported search obligations before
    /// any transition derived from them is accepted.
    pub search_availability_state_root: Hash64,
    /// Canonical immutable content references from `frontier.overlay_view`. This intentionally does
    /// not hash locally available leaf bytes, so a late blob cannot mutate an accepted parent's root.
    /// Snapshot validation separately requires complete root-matching bytes for certified entries.
    pub active_batch_ref_root: Hash64,
}

impl PalwSelectedParentStateV2 {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        selected_parent: BlockHash,
        selected_parent_daa_score: u64,
        frontier: PalwPrunedFrontierV1,
        beacon_accumulator: Option<PalwPrunedBeaconAccumulatorV1>,
        provider_bonds: Vec<PalwProviderBondRecord>,
        paid_work_nullifiers: Vec<Hash64>,
        da_state_root: Hash64,
        search_availability_state_root: Hash64,
        active_batch_ref_root: Hash64,
    ) -> Result<Self, PalwPruningSnapshotError> {
        let mut state = Self {
            version: PALW_SELECTED_PARENT_STATE_VERSION,
            selected_parent,
            selected_parent_daa_score,
            frontier,
            beacon_accumulator,
            provider_bonds,
            paid_work_nullifiers,
            da_state_root,
            search_availability_state_root,
            active_batch_ref_root,
        };
        state.canonicalize();
        state.validate_canonical()?;
        Ok(state)
    }

    pub fn canonicalize(&mut self) {
        if let Some(accumulator) = self.beacon_accumulator.as_mut() {
            accumulator.canonicalize();
        }
        self.provider_bonds.sort_by(|a, b| cmp_outpoint(&a.bond_outpoint, &b.bond_outpoint));
        self.paid_work_nullifiers.sort_by_key(|hash| hash.as_bytes());
    }

    pub fn validate_canonical(&self) -> Result<(), PalwPruningSnapshotError> {
        if self.version != PALW_SELECTED_PARENT_STATE_VERSION {
            return Err(PalwPruningSnapshotError::UnsupportedVersion(self.version));
        }
        let mut canonical = self.clone();
        canonical.canonicalize();
        if canonical != *self {
            return Err(PalwPruningSnapshotError::NonCanonical("selected-parent PALW state ordering"));
        }
        if self.provider_bonds.len() > MAX_PALW_PRUNING_PROVIDER_BONDS {
            return Err(PalwPruningSnapshotError::TooMany("provider bonds", self.provider_bonds.len()));
        }
        if !self.provider_bonds.windows(2).all(|w| cmp_outpoint(&w[0].bond_outpoint, &w[1].bond_outpoint).is_lt()) {
            return Err(PalwPruningSnapshotError::NonCanonical("provider bonds"));
        }
        if self.paid_work_nullifiers.len() > MAX_PALW_PRUNING_PAID_IDS {
            return Err(PalwPruningSnapshotError::TooMany("paid-work nullifiers", self.paid_work_nullifiers.len()));
        }
        if !self.paid_work_nullifiers.windows(2).all(|w| w[0].as_bytes() < w[1].as_bytes()) {
            return Err(PalwPruningSnapshotError::NonCanonical("paid-work nullifiers"));
        }
        if self.frontier.active_nullifiers.len() > MAX_PALW_PRUNING_ACTIVE_NULLIFIERS {
            return Err(PalwPruningSnapshotError::TooMany("active nullifiers", self.frontier.active_nullifiers.len()));
        }
        if self.frontier.active_nullifiers.iter_sorted().any(|(_, daa)| *daa > self.selected_parent_daa_score) {
            return Err(PalwPruningSnapshotError::Incoherent("active nullifier first-seen DAA is after selected parent"));
        }
        if let Some(state) = &self.frontier.beacon_state
            && (state.version != 1 || state.anchor_daa_score > self.selected_parent_daa_score || state.mode > 2)
        {
            return Err(PalwPruningSnapshotError::Incoherent("beacon state"));
        }
        if self.frontier.overlay_view.as_ref().is_some_and(|view| view.version != 1) {
            return Err(PalwPruningSnapshotError::Incoherent("overlay-view version"));
        }
        if self.active_batch_ref_root != palw_active_batch_ref_root(self.frontier.overlay_view.as_ref()) {
            return Err(PalwPruningSnapshotError::Incoherent("active-batch reference root"));
        }
        validate_beacon_accumulator(self.beacon_accumulator.as_ref())?;
        Ok(())
    }

    /// Canonical PALW selected-parent digest folded into Header-v4's outer overlay commitment.
    pub fn state_root(&self) -> Hash64 {
        let mut canonical = self.clone();
        canonical.canonicalize();
        blake2b_512_keyed(
            PALW_SELECTED_PARENT_STATE_DIGEST_DOMAIN,
            &borsh::to_vec(&canonical).expect("PALW selected-parent state has an infallible Borsh encoding"),
        )
    }
}

fn validate_beacon_accumulator(view: Option<&PalwPrunedBeaconAccumulatorV1>) -> Result<(), PalwPruningSnapshotError> {
    let Some(view) = view else {
        return Ok(());
    };
    if view.version != 1 {
        return Err(PalwPruningSnapshotError::Incoherent("beacon-accumulator version"));
    }
    if view.epochs.len() > MAX_PALW_PRUNING_BEACON_EPOCHS || view.stake_by_epoch.len() > MAX_PALW_PRUNING_BEACON_EPOCHS {
        return Err(PalwPruningSnapshotError::TooMany("beacon epochs", view.epochs.len().max(view.stake_by_epoch.len())));
    }
    if view.epochs.keys().ne(view.stake_by_epoch.keys()) {
        return Err(PalwPruningSnapshotError::Incoherent("beacon epoch/stake key sets"));
    }
    for (epoch, accum) in &view.epochs {
        if accum.version != 1
            || accum.commits.len() > MAX_PALW_PRUNING_BEACON_ROWS
            || accum.valid_reveals.len() > MAX_PALW_PRUNING_BEACON_ROWS
            || !outpoints_are_strictly_sorted(&accum.commits)
            || !outpoints_are_strictly_sorted(&accum.valid_reveals)
        {
            return Err(PalwPruningSnapshotError::Incoherent("beacon accumulator rows"));
        }
        let stakes = &view.stake_by_epoch[epoch];
        if stakes.len() != accum.commits.len()
            || stakes.len() > MAX_PALW_PRUNING_BEACON_ROWS
            || !outpoints_are_strictly_sorted(stakes)
            || accum.commits.iter().map(|x| x.0).ne(stakes.iter().map(|x| x.0))
            || accum.valid_reveals.iter().any(|(op, _)| accum.commits.binary_search_by(|x| cmp_outpoint(&x.0, op)).is_err())
        {
            return Err(PalwPruningSnapshotError::Incoherent("beacon stake/reveal rows"));
        }
    }
    Ok(())
}

/// One selected-chain block in the below-pruning-point paid-work window. Empty rows are retained:
/// they make the captured chain segment explicit and let recovery distinguish a complete window from
/// a best-effort collection of only the surviving non-empty store rows.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct PalwPrunedPaidWorkBlockV1 {
    pub block_hash: BlockHash,
    pub block_daa_score: u64,
    pub job_nullifiers: Vec<Hash64>,
}

/// Self-contained content-addressed data for one batch retained by the pruning-point lifecycle view.
/// Every view entry has exactly one row and its manifest is mandatory. A certified entry carries the
/// exact lifecycle certificate plus the complete leaf set. An uncertified entry carries no leaves:
/// pre-pruning partial chunks are availability data, not fork-local consensus state, and are
/// re-announced after the pruning boundary with their manifest-membership proofs.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct PalwPrunedActiveBatchV1 {
    pub batch_id: Hash64,
    pub manifest: PalwBatchManifestV1,
    pub leaves: Vec<PalwPublicLeafV1>,
    pub certificate: Option<PalwBatchCertificateV2>,
}

/// The immutable, block-keyed content references committed by Header-v4. Deliberately excludes the
/// locally available blob bytes: a leaf arriving after a selected parent was accepted must not change
/// that parent's commitment. The complete bytes are validated against these roots during pruning
/// snapshot import, while uncertified partial leaves are canonicalized away.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
struct PalwActiveBatchRefV1 {
    batch_id: Hash64,
    manifest_hash: Hash64,
    leaf_root: Hash64,
    leaf_count: u32,
    certificate_hash: Option<Hash64>,
}

pub fn palw_active_batch_ref_root(view: Option<&PalwBatchViewV1>) -> Hash64 {
    let refs: Vec<_> = view
        .into_iter()
        .flat_map(|view| &view.batches)
        .map(|(batch_id, lifecycle)| PalwActiveBatchRefV1 {
            batch_id: *batch_id,
            // A PALW manifest is content-addressed, so its canonical content hash is the batch id.
            manifest_hash: *batch_id,
            leaf_root: lifecycle.leaf_root,
            leaf_count: lifecycle.leaf_count,
            certificate_hash: lifecycle.cert_hash,
        })
        .collect();
    blake2b_512_keyed(
        PALW_ACTIVE_BATCH_REF_DIGEST_DOMAIN,
        &borsh::to_vec(&refs).expect("PALW active-batch references have an infallible Borsh encoding"),
    )
}

fn validate_active_batch_blob_limits(
    batch_count: usize,
    leaf_count: usize,
    encoded_len: usize,
) -> Result<(), PalwPruningSnapshotError> {
    if batch_count > MAX_PALW_PRUNING_ACTIVE_BATCHES {
        return Err(PalwPruningSnapshotError::TooMany("active batch blobs", batch_count));
    }
    if leaf_count > MAX_PALW_PRUNING_ACTIVE_LEAVES {
        return Err(PalwPruningSnapshotError::TooMany("active batch leaves", leaf_count));
    }
    if encoded_len > MAX_PALW_PRUNING_ACTIVE_BLOB_BYTES {
        return Err(PalwPruningSnapshotError::TooMany("active batch blob bytes", encoded_len));
    }
    Ok(())
}

/// Cross-check the content-addressed bundle against the compact lifecycle view. This rejects missing,
/// duplicate, orphan and non-canonical rows before any import write is staged.
pub fn validate_palw_active_batch_bundles(
    view: Option<&PalwBatchViewV1>,
    rows: &[PalwPrunedActiveBatchV1],
) -> Result<(), PalwPruningSnapshotError> {
    let empty = PalwBatchViewV1::new();
    let view = view.unwrap_or(&empty);
    validate_active_batch_blob_limits(rows.len(), 0, 0)?;
    if rows.len() != view.batches.len() {
        return Err(PalwPruningSnapshotError::Incoherent("active batch blob/view key set"));
    }
    if !rows.windows(2).all(|w| w[0].batch_id.as_bytes() < w[1].batch_id.as_bytes()) {
        return Err(PalwPruningSnapshotError::NonCanonical("active batch blobs"));
    }
    let encoded_len = borsh::to_vec(rows).expect("PALW active-batch bundle has an infallible Borsh encoding").len();
    validate_active_batch_blob_limits(rows.len(), 0, encoded_len)?;

    let mut total_leaves = 0usize;
    for row in rows {
        let Some(lifecycle) = view.batches.get(&row.batch_id) else {
            return Err(PalwPruningSnapshotError::Incoherent("orphan active batch blob"));
        };
        let manifest = &row.manifest;
        if manifest.batch_id != row.batch_id
            || !manifest.batch_id_is_content_derived()
            || manifest.registration_epoch != lifecycle.registration_epoch
            || manifest.activation_not_before_epoch != lifecycle.activation_not_before_epoch
            || manifest.expiry_epoch != lifecycle.expiry_epoch
            || manifest.leaf_count != lifecycle.leaf_count
            || manifest.chunk_count != lifecycle.chunk_count
            || manifest.leaf_root != lifecycle.leaf_root
        {
            return Err(PalwPruningSnapshotError::Incoherent("active batch manifest/lifecycle binding"));
        }
        total_leaves = total_leaves.saturating_add(row.leaves.len());
        validate_active_batch_blob_limits(rows.len(), total_leaves, encoded_len)?;
        if !row.leaves.windows(2).all(|w| w[0].leaf_index < w[1].leaf_index)
            || row.leaves.iter().any(|leaf| leaf.batch_id != row.batch_id || leaf.leaf_index >= manifest.leaf_count)
        {
            return Err(PalwPruningSnapshotError::NonCanonical("active batch leaves"));
        }

        match (lifecycle.cert_hash, row.certificate.as_ref()) {
            (None, None) => {
                if !row.leaves.is_empty() {
                    return Err(PalwPruningSnapshotError::NonCanonical("uncertified active batch leaves"));
                }
            }
            (Some(expected), Some(certificate)) => {
                if row.leaves.len() != manifest.leaf_count as usize
                    || certificate.hash() != expected
                    || certificate.batch_id != row.batch_id
                    || certificate.manifest_hash != manifest.content_id()
                    || certificate.leaf_root != manifest.leaf_root
                {
                    return Err(PalwPruningSnapshotError::Incoherent("active batch certificate binding"));
                }
                if !row.leaves.iter().enumerate().all(|(index, leaf)| leaf.leaf_index == index as u32) {
                    return Err(PalwPruningSnapshotError::Incoherent("active batch complete leaf index set"));
                }
                let projected_hashes: Vec<Hash64> = row
                    .leaves
                    .iter()
                    .cloned()
                    .map(|mut leaf| {
                        leaf.batch_id = Hash64::default();
                        leaf.leaf_hash()
                    })
                    .collect();
                if palw_leaf_merkle_root(&projected_hashes) != manifest.leaf_root {
                    return Err(PalwPruningSnapshotError::Incoherent("active batch leaf root"));
                }
            }
            _ => return Err(PalwPruningSnapshotError::Incoherent("active batch certificate presence")),
        }
    }
    Ok(())
}

/// Header-v4 fork-local anti-spam row at the pruning point. The live store type belongs to the
/// consensus crate; this consensus-core mirror is the stable pruning transport representation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct PalwPrunedSpamAccumulatorV1 {
    pub version: u16,
    pub daa_score: u64,
    pub selected_height: u64,
    pub total_hash_blues: u64,
    pub total_replica_blues: u64,
    pub selected_parent: Option<BlockHash>,
    pub skip: Option<BlockHash>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct PalwPrunedSpamSupportRowV1 {
    pub block_hash: BlockHash,
    pub state: PalwPrunedSpamAccumulatorV1,
}

/// The PP row plus the bounded selected-chain/skip-link closure it needs until the anti-spam DAA
/// horizon has moved completely above the pruning boundary.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct PalwPrunedSpamFrontierV1 {
    pub pruning_point_state: PalwPrunedSpamAccumulatorV1,
    pub support_rows: Vec<PalwPrunedSpamSupportRowV1>,
}

impl PalwPrunedSpamAccumulatorV1 {
    pub fn commitment(&self) -> Hash64 {
        crate::palw_antispam::palw_spam_accumulator_commitment(
            self.daa_score,
            self.selected_height,
            self.total_hash_blues,
            self.total_replica_blues,
            self.selected_parent,
            self.skip,
        )
    }

    fn shape_is_valid(&self) -> bool {
        self.version == 1
            && match self.selected_height {
                0 => {
                    self.selected_parent.is_none()
                        && self.skip.is_none()
                        && self.total_hash_blues == 0
                        && self.total_replica_blues == 0
                }
                1 => self.selected_parent.is_some() && self.skip.is_none(),
                _ => self.selected_parent.is_some() && self.skip.is_some(),
            }
    }
}

/// The digest preimage. Keeping the digest outside this struct avoids a self-referential encoding.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct PalwPruningPointSnapshotPayloadV1 {
    pub version: u16,
    pub pruning_point: BlockHash,
    pub pruning_point_daa_score: u64,
    /// The locally-derived paid-work bound. Import requires equality with the receiving network's
    /// parameters; a snapshot from another parameterization is not silently truncated or extended.
    pub paid_work_window_daa: u64,
    pub frontier: PalwPrunedFrontierV1,
    pub beacon_accumulator: Option<PalwPrunedBeaconAccumulatorV1>,
    pub spam_accumulator: Option<PalwPrunedSpamFrontierV1>,
    /// Fork-local data-availability obligations/challenges at the pruning boundary. The outer
    /// digest binds this component together with the execution, provider and anti-spam frontiers.
    pub da_snapshot: Option<PalwDaPruningSnapshotV1>,
    /// Fork-local search-availability obligations at the pruning boundary (subnet bytes
    /// 0x3d-0x3f). Same rationale as `da_snapshot`: omission would silently forget open
    /// obligations and challenge deadlines after pruned IBD.
    pub search_availability_snapshot: Option<PalwSearchPruningSnapshotV1>,
    /// Complete content-addressed PALW store projection for every batch retained by `frontier.overlay_view`.
    pub active_batches: Vec<PalwPrunedActiveBatchV1>,
    pub provider_bonds: Vec<PalwProviderBondRecord>,
    pub paid_work: Vec<PalwPrunedPaidWorkBlockV1>,
}

impl PalwPruningPointSnapshotPayloadV1 {
    pub fn canonicalize(&mut self) {
        if let Some(accum) = self.beacon_accumulator.as_mut() {
            accum.canonicalize();
        }
        if let Some(spam) = self.spam_accumulator.as_mut() {
            spam.support_rows.sort_by(|a, b| a.block_hash.as_bytes().cmp(&b.block_hash.as_bytes()));
        }
        self.active_batches.sort_by(|a, b| a.batch_id.as_bytes().cmp(&b.batch_id.as_bytes()));
        for batch in &mut self.active_batches {
            batch.leaves.sort_by_key(|leaf| leaf.leaf_index);
        }
        self.provider_bonds.sort_by(|a, b| cmp_outpoint(&a.bond_outpoint, &b.bond_outpoint));
        self.paid_work.sort_by(|a, b| {
            a.block_daa_score.cmp(&b.block_daa_score).then_with(|| a.block_hash.as_bytes().cmp(&b.block_hash.as_bytes()))
        });
        for row in &mut self.paid_work {
            row.job_nullifiers.sort_by_key(|hash| hash.as_bytes());
        }
    }

    pub fn digest(&self) -> Hash64 {
        blake2b_512_keyed(
            PALW_PRUNING_SNAPSHOT_DIGEST_DOMAIN,
            &borsh::to_vec(self).expect("PALW pruning snapshot payload has an infallible Borsh encoding"),
        )
    }
}

/// Complete, checksummed pruning-point sidecar. `payload_digest` is also copied into trusted-data
/// transport so the sidecar received later in IBD is bound to the earlier package.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct PalwPruningPointSnapshotV1 {
    pub payload: PalwPruningPointSnapshotPayloadV1,
    pub payload_digest: Hash64,
}

impl PalwPruningPointSnapshotV1 {
    pub fn new(mut payload: PalwPruningPointSnapshotPayloadV1) -> Self {
        payload.canonicalize();
        let payload_digest = payload.digest();
        Self { payload, payload_digest }
    }

    /// Canonical writer used by production capture paths. Unlike [`Self::new`], which remains a
    /// convenient constructor for corruption/ordering tests, this refuses to materialize a snapshot
    /// whose component cardinalities or shapes exceed the transport contract.
    pub fn try_new(payload: PalwPruningPointSnapshotPayloadV1) -> Result<Self, PalwPruningSnapshotError> {
        let snapshot = Self::new(payload);
        snapshot.validate_canonical()?;
        Ok(snapshot)
    }

    /// Strict structural and canonical validation. Chain-context checks (requested pruning point,
    /// header DAA, local paid-work bound and provider UTXOs) are deliberately performed by the
    /// consensus importer, which has those stores and network parameters.
    pub fn validate_canonical(&self) -> Result<(), PalwPruningSnapshotError> {
        // Run the exact outer-envelope fence before hashing or cloning attacker-controlled
        // collections. This is the same constant the P2P decoder applies before Borsh allocation.
        validate_borsh_encoded_size(self, MAX_PALW_PRUNING_SNAPSHOT_BYTES)?;
        let p = &self.payload;
        if p.version != PALW_PRUNING_SNAPSHOT_VERSION {
            return Err(PalwPruningSnapshotError::UnsupportedVersion(p.version));
        }
        if self.payload_digest != p.digest() {
            return Err(PalwPruningSnapshotError::DigestMismatch);
        }
        let mut canonical = p.clone();
        canonical.canonicalize();
        if canonical != *p {
            return Err(PalwPruningSnapshotError::NonCanonical("payload ordering"));
        }
        if p.provider_bonds.len() > MAX_PALW_PRUNING_PROVIDER_BONDS {
            return Err(PalwPruningSnapshotError::TooMany("provider bonds", p.provider_bonds.len()));
        }
        if p.frontier.active_nullifiers.len() > MAX_PALW_PRUNING_ACTIVE_NULLIFIERS {
            return Err(PalwPruningSnapshotError::TooMany("active nullifiers", p.frontier.active_nullifiers.len()));
        }
        if p.frontier.active_nullifiers.iter_sorted().any(|(_, daa)| *daa > p.pruning_point_daa_score) {
            return Err(PalwPruningSnapshotError::Incoherent("active nullifier first-seen DAA is after pruning point"));
        }
        if let Some(state) = &p.frontier.beacon_state
            && (state.version != 1 || state.anchor_daa_score > p.pruning_point_daa_score || state.mode > 2)
        {
            return Err(PalwPruningSnapshotError::Incoherent("beacon state"));
        }
        if p.frontier.overlay_view.as_ref().is_some_and(|v| v.version != 1) {
            return Err(PalwPruningSnapshotError::Incoherent("overlay-view version"));
        }
        self.validate_beacon_accumulator()?;
        if let Some(spam) = &self.payload.spam_accumulator
            && (!spam.pruning_point_state.shape_is_valid()
                || spam.pruning_point_state.daa_score != self.payload.pruning_point_daa_score
                || spam.support_rows.len() > MAX_PALW_PRUNING_SPAM_SUPPORT_ROWS
                || !spam.support_rows.windows(2).all(|w| w[0].block_hash.as_bytes() < w[1].block_hash.as_bytes())
                || spam.support_rows.iter().any(|row| {
                    row.block_hash == self.payload.pruning_point
                        || !row.state.shape_is_valid()
                        || row.state.daa_score > self.payload.pruning_point_daa_score
                }))
        {
            return Err(PalwPruningSnapshotError::Incoherent("Header-v4 spam accumulator"));
        }
        if let Some(da) = &p.da_snapshot
            && (da.pruning_point != p.pruning_point || !da.validate())
        {
            return Err(PalwPruningSnapshotError::Incoherent("DA pruning snapshot"));
        }
        if let Some(search) = &p.search_availability_snapshot
            && (search.pruning_point != p.pruning_point || !search.validate())
        {
            return Err(PalwPruningSnapshotError::Incoherent("search-availability pruning snapshot"));
        }
        validate_palw_active_batch_bundles(p.frontier.overlay_view.as_ref(), &p.active_batches)?;
        self.validate_provider_bonds()?;
        self.validate_paid_work()?;
        Ok(())
    }

    fn validate_beacon_accumulator(&self) -> Result<(), PalwPruningSnapshotError> {
        validate_beacon_accumulator(self.payload.beacon_accumulator.as_ref())
    }

    fn validate_provider_bonds(&self) -> Result<(), PalwPruningSnapshotError> {
        if !self.payload.provider_bonds.windows(2).all(|w| cmp_outpoint(&w[0].bond_outpoint, &w[1].bond_outpoint).is_lt()) {
            return Err(PalwPruningSnapshotError::NonCanonical("provider bonds"));
        }
        for rec in &self.payload.provider_bonds {
            if rec.version != 1
                || rec.bond_outpoint.index != 0
                || rec.owner_public_key.len() != STAKE_VALIDATOR_PUBKEY_LEN
                || rec.owner_pubkey_hash != validator_id_from_pubkey(&rec.owner_public_key)
                || rec.runtime_classes.is_empty()
                || rec.runtime_classes.len() > PALW_MAX_PROVIDER_RUNTIME_CLASSES_V1
                || !rec.runtime_classes.windows(2).all(|w| w[0].as_bytes() < w[1].as_bytes())
                || rec.capacity_by_shape.is_empty()
                || rec.capacity_by_shape.len() > PALW_MAX_PROVIDER_CAPACITY_ENTRIES_V1
                || !rec.capacity_by_shape.windows(2).all(|w| w[0].0 < w[1].0)
                || rec.capacity_by_shape.iter().any(|(_, capacity)| *capacity == 0)
                || rec.amount_sompi == 0
                || rec.unbond_delay_epochs == 0
                || rec.created_daa_score != rec.activation_daa_score
                || rec.created_daa_score > self.payload.pruning_point_daa_score
                || rec
                    .unbond_request_daa_score
                    .is_some_and(|daa| daa < rec.created_daa_score || daa > self.payload.pruning_point_daa_score)
                || rec
                    .slashed_at_daa_score
                    .is_some_and(|daa| daa < rec.created_daa_score || daa > self.payload.pruning_point_daa_score)
            {
                return Err(PalwPruningSnapshotError::Incoherent("provider bond"));
            }
        }
        Ok(())
    }

    fn validate_paid_work(&self) -> Result<(), PalwPruningSnapshotError> {
        let rows = &self.payload.paid_work;
        if rows.len() > MAX_PALW_PRUNING_PAID_BLOCKS {
            return Err(PalwPruningSnapshotError::TooMany("paid-work blocks", rows.len()));
        }
        let mut ids = 0usize;
        let mut all_ids = std::collections::HashSet::new();
        let mut block_hashes = std::collections::HashSet::new();
        for row in rows {
            ids = ids.saturating_add(row.job_nullifiers.len());
            if row.block_daa_score > self.payload.pruning_point_daa_score
                || self.payload.pruning_point_daa_score.saturating_sub(row.block_daa_score) > self.payload.paid_work_window_daa
                || !row.job_nullifiers.windows(2).all(|w| w[0].as_bytes() < w[1].as_bytes())
                || !block_hashes.insert(row.block_hash)
                || row.job_nullifiers.iter().any(|id| !all_ids.insert(*id))
            {
                return Err(PalwPruningSnapshotError::Incoherent("paid-work window"));
            }
        }
        if ids > MAX_PALW_PRUNING_PAID_IDS {
            return Err(PalwPruningSnapshotError::TooMany("paid-work nullifiers", ids));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PalwPruningSnapshotError {
    #[error("unsupported PALW pruning snapshot version {0}")]
    UnsupportedVersion(u16),
    #[error("PALW pruning snapshot digest does not match its canonical payload")]
    DigestMismatch,
    #[error("PALW pruning snapshot field is not canonical: {0}")]
    NonCanonical(&'static str),
    #[error("PALW pruning snapshot has too many {0}: {1}")]
    TooMany(&'static str, usize),
    #[error("PALW pruning snapshot fields are incoherent: {0}")]
    Incoherent(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dns_finality::{OverlaySnapshot, palw_overlay_commitment_root_v2, validator_id_from_pubkey},
        palw::{
            PALW_BATCH_CERTIFICATE_VERSION_V2, PalwBatchLifecycleV1, PalwBatchStatus, PalwBatchViewV1, PalwBeaconStateV1,
            PalwLaneBitsV1,
        },
        tx::{ScriptPublicKey, ScriptVec},
    };

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn provider(byte: u8) -> PalwProviderBondRecord {
        let pk = vec![byte; STAKE_VALIDATOR_PUBKEY_LEN];
        let op = TransactionOutpoint::new(h(byte), 0);
        PalwProviderBondRecord {
            version: 1,
            bond_outpoint: op,
            owner_pubkey_hash: validator_id_from_pubkey(&pk),
            owner_public_key: pk,
            operator_group_id: h(byte.wrapping_add(1)),
            runtime_classes: vec![h(1), h(2)],
            capacity_by_shape: vec![(1, 10), (2, 20)],
            reward_key_root: h(3),
            amount_sompi: 100,
            activation_daa_score: 10,
            created_daa_score: 10,
            unbond_delay_epochs: 4,
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
        }
    }

    fn active_batch() -> (PalwBatchViewV1, PalwPrunedActiveBatchV1) {
        let reward_script = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0x51]));
        let mut leaf = PalwPublicLeafV1 {
            version: 1,
            batch_id: Hash64::default(),
            leaf_index: 0,
            job_nullifier: h(0x21),
            ticket_nullifier_commitment: h(0x22),
            model_profile_id: h(0x23),
            runtime_class_id: h(0x24),
            shape_id: 1,
            quantum_count: 1,
            proof_type: 1,
            provider_a_bond: TransactionOutpoint::new(h(0x25), 0),
            provider_b_bond: TransactionOutpoint::new(h(0x26), 0),
            provider_a_reward_script: reward_script.clone(),
            provider_b_reward_script: reward_script,
            ticket_authority_pk_hash: h(0x27),
            private_match_commitment: h(0x28),
            receipt_da_object_version: 1,
            receipt_da_root: h(0x29),
            receipt_da_object_len: 1,
            receipt_da_chunk_count: 1,
            receipt_v3_compute_set_id: Hash64::default(),
            receipt_v3_job_challenge: Hash64::default(),
            receipt_v3_issued_epoch: 0,
            receipt_v3_expires_epoch: 0,
            registered_epoch: 1,
            activation_epoch: 2,
            expiry_epoch: 20,
            leaf_bond_sompi: 1,
        };
        let leaf_root = palw_leaf_merkle_root(&[leaf.leaf_hash()]);
        let mut manifest = PalwBatchManifestV1 {
            version: 1,
            batch_id: Hash64::default(),
            registration_epoch: 1,
            model_profile_id: leaf.model_profile_id,
            runtime_class_id: leaf.runtime_class_id,
            leaf_count: 1,
            chunk_count: 1,
            leaf_root,
            descriptor_root: h(0x2a),
            total_leaf_bond_sompi: 1,
            audit_policy_id: h(0x2b),
            activation_not_before_epoch: 2,
            expiry_epoch: 20,
        };
        let batch_id = manifest.content_id();
        manifest.batch_id = batch_id;
        leaf.batch_id = batch_id;
        let certificate = PalwBatchCertificateV2 {
            version: PALW_BATCH_CERTIFICATE_VERSION_V2,
            batch_id,
            manifest_hash: manifest.content_id(),
            leaf_root,
            audit_beacon_epoch: 1,
            audit_sample_root: h(0x2c),
            passed_leaf_count: 1,
            rejected_leaf_bitmap_root: h(0x2d),
            certificate_epoch: 1,
            activation_epoch: 2,
            expiry_epoch: 20,
            auditor_set_commitment: h(0x2e),
            approving_stake: 1,
            votes: vec![],
        };
        let cert_hash = certificate.hash();
        let lifecycle = PalwBatchLifecycleV1 {
            status: PalwBatchStatus::Active,
            registration_epoch: 1,
            activation_not_before_epoch: 2,
            expiry_epoch: 20,
            leaf_count: 1,
            chunk_count: 1,
            chunks_present: [1, 0, 0, 0],
            leaf_root,
            cert_hash: Some(cert_hash),
            cert_activation_epoch: 0,
            cert_expiry_epoch: 0,
            cert_approving_stake: 0,
            first_cert_daa: Some(10),
            revoked_from_daa: None,
        };
        let mut view = PalwBatchViewV1::new();
        view.batches.insert(batch_id, lifecycle);
        (view, PalwPrunedActiveBatchV1 { batch_id, manifest, leaves: vec![leaf], certificate: Some(certificate) })
    }

    fn snapshot() -> PalwPruningPointSnapshotV1 {
        PalwPruningPointSnapshotV1::new(PalwPruningPointSnapshotPayloadV1 {
            version: PALW_PRUNING_SNAPSHOT_VERSION,
            pruning_point: h(9),
            pruning_point_daa_score: 100,
            paid_work_window_daa: 20,
            frontier: PalwPrunedFrontierV1 {
                beacon_state: None,
                overlay_view: None,
                lane_bits: Some(PalwLaneBitsV1 { hash_bits: 1, replica_bits: 2 }),
                active_nullifiers: Default::default(),
            },
            beacon_accumulator: Some(PalwPrunedBeaconAccumulatorV1::new()),
            spam_accumulator: None,
            da_snapshot: None,
            search_availability_snapshot: None,
            active_batches: vec![],
            provider_bonds: vec![provider(2), provider(1)],
            paid_work: vec![
                PalwPrunedPaidWorkBlockV1 { block_hash: h(8), block_daa_score: 90, job_nullifiers: vec![h(5), h(4)] },
                PalwPrunedPaidWorkBlockV1 { block_hash: h(9), block_daa_score: 100, job_nullifiers: vec![] },
            ],
        })
    }

    fn selected_parent_state() -> PalwSelectedParentStateV2 {
        let mut active_nullifiers = crate::palw::PalwActiveNullifierSet::new();
        active_nullifiers.insert(h(0x31), 90);
        let frontier = PalwPrunedFrontierV1 {
            beacon_state: Some(PalwBeaconStateV1 {
                version: 1,
                epoch: 2,
                seed: h(0x11),
                dns_anchor: h(0x12),
                anchor_blue_score: 80,
                anchor_daa_score: 80,
                anchor_overlay_root: h(0x13),
                valid_reveals_root: h(0x14),
                missing_commitments_root: h(0x15),
                mode: 0,
                degraded_epochs: 0,
                valid_reveal_count: 1,
                missing_commit_count: 0,
            }),
            overlay_view: Some(PalwBatchViewV1::new()),
            lane_bits: Some(PalwLaneBitsV1 { hash_bits: 0x1d00ffff, replica_bits: 0x1c00ffff }),
            active_nullifiers,
        };
        let active_batch_ref_root = palw_active_batch_ref_root(frontier.overlay_view.as_ref());
        PalwSelectedParentStateV2::try_new(
            h(0x90),
            100,
            frontier,
            Some(PalwPrunedBeaconAccumulatorV1::new()),
            vec![provider(1)],
            vec![h(0x41)],
            h(0x51),
            h(0x52),
            active_batch_ref_root,
        )
        .unwrap()
    }

    #[test]
    fn writer_is_deterministic_and_canonical() {
        let a = snapshot();
        let b = snapshot();
        assert_eq!(a, b);
        assert_eq!(borsh::to_vec(&a).unwrap(), borsh::to_vec(&b).unwrap());
        assert!(a.validate_canonical().is_ok());
        assert_eq!(a.payload.provider_bonds[0].bond_outpoint.transaction_id, h(1));
        assert_eq!(a.payload.paid_work[0].job_nullifiers, vec![h(4), h(5)]);
    }

    #[test]
    fn exact_snapshot_byte_cap_and_versioned_import_auth_are_pinned() {
        let value = snapshot();
        let encoded_len = borsh::to_vec(&value).unwrap().len();
        assert_eq!(validate_borsh_encoded_size(&value, encoded_len), Ok(encoded_len));
        assert!(matches!(
            validate_borsh_encoded_size(&value, encoded_len - 1),
            Err(PalwPruningSnapshotError::TooMany("encoded snapshot bytes", _))
        ));
        assert_eq!(MAX_PALW_PRUNING_SNAPSHOT_BYTES, 128 << 20);
        let legacy = PalwPruningSnapshotImportAuth::legacy_header_v3(value.payload.pruning_point, value.payload_digest);
        let pinned = PalwPruningSnapshotImportAuth::operator_pinned(PalwPruningSnapshotCheckpoint {
            pruning_point: value.payload.pruning_point,
            payload_digest: value.payload_digest,
        });
        assert!(!palw_pruned_ibd_snapshot_import_allowed(crate::constants::EVM_HEADER_VERSION, &legacy));
        assert!(palw_pruned_ibd_snapshot_import_allowed(crate::constants::PALW_HEADER_VERSION, &legacy));
        assert!(!palw_pruned_ibd_snapshot_import_allowed(crate::constants::PALW_HEADER_VERSION, &pinned));
        assert!(!palw_pruned_ibd_snapshot_import_allowed(crate::constants::PALW_ANTISPAM_HEADER_VERSION, &legacy));
        assert!(palw_pruned_ibd_snapshot_import_allowed(crate::constants::PALW_ANTISPAM_HEADER_VERSION, &pinned));
        assert!(!palw_pruned_ibd_snapshot_import_allowed(crate::constants::PALW_ANTISPAM_HEADER_VERSION + 1, &pinned));
    }

    #[test]
    fn operator_checkpoint_parser_and_set_validation_are_strict() {
        let encoded = format!("{}:{}", h(1), h(2));
        let parsed = encoded.parse::<PalwPruningSnapshotCheckpoint>().unwrap();
        assert_eq!(parsed, PalwPruningSnapshotCheckpoint { pruning_point: h(1), payload_digest: h(2) });
        assert_eq!(parsed.to_string(), encoded);

        for malformed in
            ["", "00", "00:11", &format!("{}:{}:extra", h(1), h(2)), &format!("0x{}:{}", h(1), h(2)), &format!(" {}:{}", h(1), h(2))]
        {
            assert!(malformed.parse::<PalwPruningSnapshotCheckpoint>().is_err(), "accepted malformed checkpoint {malformed:?}");
        }

        let same = parsed;
        assert_eq!(
            validate_palw_pruning_snapshot_checkpoints(&[parsed, same]),
            Err(PalwPruningSnapshotCheckpointSetError::Duplicate(h(1)))
        );
        let conflict = PalwPruningSnapshotCheckpoint { pruning_point: h(1), payload_digest: h(3) };
        assert_eq!(
            validate_palw_pruning_snapshot_checkpoints(&[parsed, conflict]),
            Err(PalwPruningSnapshotCheckpointSetError::Conflict { pruning_point: h(1), first_digest: h(2), second_digest: h(3) })
        );
        assert!(
            validate_palw_pruning_snapshot_checkpoints(&[
                parsed,
                PalwPruningSnapshotCheckpoint { pruning_point: h(4), payload_digest: h(2) },
            ])
            .is_ok()
        );
    }

    #[test]
    fn operator_checkpoint_digest_authenticates_header_v4_spam_support_rows() {
        let mut payload = snapshot().payload;
        payload.spam_accumulator = Some(PalwPrunedSpamFrontierV1 {
            pruning_point_state: PalwPrunedSpamAccumulatorV1 {
                version: 1,
                daa_score: payload.pruning_point_daa_score,
                selected_height: 1,
                total_hash_blues: 1,
                total_replica_blues: 0,
                selected_parent: Some(h(8)),
                skip: None,
            },
            support_rows: vec![PalwPrunedSpamSupportRowV1 {
                block_hash: h(8),
                state: PalwPrunedSpamAccumulatorV1 {
                    version: 1,
                    daa_score: 90,
                    selected_height: 0,
                    total_hash_blues: 0,
                    total_replica_blues: 0,
                    selected_parent: None,
                    skip: None,
                },
            }],
        });
        let snapshot = PalwPruningPointSnapshotV1::try_new(payload).unwrap();
        let checkpoint =
            PalwPruningSnapshotCheckpoint { pruning_point: snapshot.payload.pruning_point, payload_digest: snapshot.payload_digest };

        let mut tampered_payload = snapshot.payload.clone();
        tampered_payload.spam_accumulator.as_mut().unwrap().support_rows[0].state.daa_score -= 1;
        let tampered = PalwPruningPointSnapshotV1::try_new(tampered_payload).unwrap();
        assert_ne!(tampered.payload_digest, checkpoint.payload_digest);

        let mut stale_envelope = tampered;
        stale_envelope.payload_digest = checkpoint.payload_digest;
        assert_eq!(stale_envelope.validate_canonical(), Err(PalwPruningSnapshotError::DigestMismatch));
    }

    #[test]
    fn permissionless_support_row_binding_accepts_matched_headers_and_rejects_tampering() {
        let pruning_point = h(20);
        let spam = PalwPrunedSpamFrontierV1 {
            pruning_point_state: PalwPrunedSpamAccumulatorV1 {
                version: 1,
                daa_score: 100,
                selected_height: 2,
                total_hash_blues: 2,
                total_replica_blues: 1,
                selected_parent: Some(h(12)),
                skip: Some(h(8)),
            },
            support_rows: vec![
                PalwPrunedSpamSupportRowV1 {
                    block_hash: h(8),
                    state: PalwPrunedSpamAccumulatorV1 {
                        version: 1,
                        daa_score: 80,
                        selected_height: 0,
                        total_hash_blues: 0,
                        total_replica_blues: 0,
                        selected_parent: None,
                        skip: None,
                    },
                },
                PalwPrunedSpamSupportRowV1 {
                    block_hash: h(12),
                    state: PalwPrunedSpamAccumulatorV1 {
                        version: 1,
                        daa_score: 90,
                        selected_height: 1,
                        total_hash_blues: 1,
                        total_replica_blues: 0,
                        selected_parent: Some(h(8)),
                        skip: None,
                    },
                },
            ],
        };
        // The header facts an honest wiring layer would present: the PP row plus each support row.
        let mut headers = vec![
            TransportedSpamHeaderCommitmentV1 {
                block_hash: pruning_point,
                spam_accumulator_commitment: spam.pruning_point_state.commitment(),
            },
            TransportedSpamHeaderCommitmentV1 { block_hash: h(8), spam_accumulator_commitment: spam.support_rows[0].state.commitment() },
            TransportedSpamHeaderCommitmentV1 { block_hash: h(12), spam_accumulator_commitment: spam.support_rows[1].state.commitment() },
        ];
        headers.sort_by(|a, b| a.block_hash.as_bytes().cmp(&b.block_hash.as_bytes()));
        assert!(verify_support_rows_against_transported_headers(&spam, pruning_point, &headers).is_ok());

        // A header that commits a different value than its row.
        let mut wrong = headers.clone();
        let idx = wrong.iter().position(|entry| entry.block_hash == h(8)).unwrap();
        wrong[idx].spam_accumulator_commitment = h(0xff);
        assert_eq!(
            verify_support_rows_against_transported_headers(&spam, pruning_point, &wrong),
            Err(PalwPruningSnapshotError::Incoherent("transported spam header commitment does not bind its row"))
        );

        // A missing header for one row.
        let missing: Vec<_> = headers.iter().cloned().filter(|entry| entry.block_hash != h(8)).collect();
        assert_eq!(
            verify_support_rows_against_transported_headers(&spam, pruning_point, &missing),
            Err(PalwPruningSnapshotError::Incoherent("transported spam header set size does not match the support rows"))
        );

        // An extra header nobody asked for.
        let mut extra = headers.clone();
        extra.push(TransportedSpamHeaderCommitmentV1 { block_hash: h(30), spam_accumulator_commitment: h(1) });
        extra.sort_by(|a, b| a.block_hash.as_bytes().cmp(&b.block_hash.as_bytes()));
        assert_eq!(
            verify_support_rows_against_transported_headers(&spam, pruning_point, &extra),
            Err(PalwPruningSnapshotError::Incoherent("transported spam header set size does not match the support rows"))
        );

        // A substituted header (right count, wrong block) is caught by the set comparison.
        let mut substituted = headers.clone();
        let pp_idx = substituted.iter().position(|entry| entry.block_hash == pruning_point).unwrap();
        substituted[pp_idx] = TransportedSpamHeaderCommitmentV1 {
            block_hash: h(30),
            spam_accumulator_commitment: spam.pruning_point_state.commitment(),
        };
        substituted.sort_by(|a, b| a.block_hash.as_bytes().cmp(&b.block_hash.as_bytes()));
        assert_eq!(
            verify_support_rows_against_transported_headers(&spam, pruning_point, &substituted),
            Err(PalwPruningSnapshotError::Incoherent("transported spam header has no matching support row"))
        );

        // Non-canonical (unsorted) headers.
        let mut unsorted = headers.clone();
        unsorted.reverse();
        assert_eq!(
            verify_support_rows_against_transported_headers(&spam, pruning_point, &unsorted),
            Err(PalwPruningSnapshotError::NonCanonical("transported spam headers"))
        );

        // A support row that aliases the pruning point.
        let mut aliased = spam.clone();
        aliased.support_rows[0].block_hash = pruning_point;
        assert_eq!(
            verify_support_rows_against_transported_headers(&aliased, pruning_point, &headers),
            Err(PalwPruningSnapshotError::Incoherent("support rows alias the pruning point or duplicate a block hash"))
        );
    }

    #[test]
    fn reconstruct_selected_parent_state_binds_every_transported_field() {
        let payload = snapshot().payload;
        let nullifiers = vec![h(0x41), h(0x42)];
        let da_root = h(0x55);
        let state = reconstruct_selected_parent_state_from_pruning_payload(&payload, nullifiers.clone(), da_root).unwrap();
        // The reconstruction is deterministic and canonical-validated.
        let again = reconstruct_selected_parent_state_from_pruning_payload(&payload, nullifiers.clone(), da_root).unwrap();
        assert_eq!(state.state_root(), again.state_root());
        // It is anchored to the pruning point as the child's selected parent.
        assert_eq!(state.selected_parent, payload.pruning_point);
        assert_eq!(state.selected_parent_daa_score, payload.pruning_point_daa_score);
        // Every folded field moves the authenticated root: a different DA root or paid-work set
        // yields a different state_root, so a tampered payload cannot match an honest header.
        let other_da = reconstruct_selected_parent_state_from_pruning_payload(&payload, nullifiers.clone(), h(0x56)).unwrap();
        assert_ne!(state.state_root(), other_da.state_root());
        let other_nullifiers = reconstruct_selected_parent_state_from_pruning_payload(&payload, vec![h(0x41)], da_root).unwrap();
        assert_ne!(state.state_root(), other_nullifiers.state_root());
    }

    #[test]
    fn chain_derived_pre_install_verifier_binds_state_and_support_rows() {
        let mut payload = snapshot().payload;
        let pruning_point = payload.pruning_point;
        payload.spam_accumulator = Some(PalwPrunedSpamFrontierV1 {
            pruning_point_state: PalwPrunedSpamAccumulatorV1 {
                version: 1,
                daa_score: payload.pruning_point_daa_score,
                selected_height: 1,
                total_hash_blues: 1,
                total_replica_blues: 0,
                selected_parent: Some(h(0xf7)),
                skip: None,
            },
            support_rows: vec![PalwPrunedSpamSupportRowV1 {
                block_hash: h(0xf7),
                state: PalwPrunedSpamAccumulatorV1 {
                    version: 1,
                    daa_score: 1,
                    selected_height: 0,
                    total_hash_blues: 0,
                    total_replica_blues: 0,
                    selected_parent: None,
                    skip: None,
                },
            }],
        });
        let nullifiers = vec![h(0x41), h(0x42)];
        let da_root = h(0x55);
        let legacy_root = h(0x33);

        // The honest first-post-pruning-point child commits exactly this folded root.
        let state = reconstruct_selected_parent_state_from_pruning_payload(&payload, nullifiers.clone(), da_root).unwrap();
        let descendant = palw_overlay_commitment_root_v2(&legacy_root, &state.state_root());

        let spam = payload.spam_accumulator.clone().unwrap();
        let mut headers = vec![TransportedSpamHeaderCommitmentV1 {
            block_hash: pruning_point,
            spam_accumulator_commitment: spam.pruning_point_state.commitment(),
        }];
        for row in &spam.support_rows {
            headers.push(TransportedSpamHeaderCommitmentV1 {
                block_hash: row.block_hash,
                spam_accumulator_commitment: row.state.commitment(),
            });
        }
        headers.sort_by(|a, b| a.block_hash.as_bytes().cmp(&b.block_hash.as_bytes()));

        // Honest bundle authenticates before any durable install.
        assert!(verify_chain_derived_pruning_boundary(&payload, nullifiers.clone(), da_root, legacy_root, descendant, &headers).is_ok());

        // A tampered payload field (here the DA-state input) changes the folded state and is rejected.
        assert_eq!(
            verify_chain_derived_pruning_boundary(&payload, nullifiers.clone(), h(0x56), legacy_root, descendant, &headers),
            Err(PalwPruningSnapshotError::Incoherent("descendant overlay commitment does not authenticate the transported PALW state"))
        );
        // A peer-substituted legacy (DNS/EVM) overlay root is rejected.
        assert_eq!(
            verify_chain_derived_pruning_boundary(&payload, nullifiers.clone(), da_root, h(0x34), descendant, &headers),
            Err(PalwPruningSnapshotError::Incoherent("descendant overlay commitment does not authenticate the transported PALW state"))
        );
        // A wrong descendant commitment (unauthenticated header) is rejected.
        assert_eq!(
            verify_chain_derived_pruning_boundary(&payload, nullifiers.clone(), da_root, legacy_root, h(0xaa), &headers),
            Err(PalwPruningSnapshotError::Incoherent("descendant overlay commitment does not authenticate the transported PALW state"))
        );
        // With the state binding correct, a tampered support-row header is still caught.
        let mut bad_headers = headers.clone();
        let idx = bad_headers.iter().position(|entry| entry.block_hash == h(0xf7)).unwrap();
        bad_headers[idx].spam_accumulator_commitment = h(0xee);
        assert_eq!(
            verify_chain_derived_pruning_boundary(&payload, nullifiers, da_root, legacy_root, descendant, &bad_headers),
            Err(PalwPruningSnapshotError::Incoherent("transported spam header commitment does not bind its row"))
        );
    }

    #[test]
    fn header_v4_peer_import_admits_no_permissionless_provenance() {
        // The permissionless primitives above add no admitted provenance. Peer import still admits
        // exactly the closed-network v3 path and the operator-pinned v4 path, and nothing else.
        let v3 = PalwPruningSnapshotImportAuth::legacy_header_v3(h(1), h(2));
        let v4 = PalwPruningSnapshotImportAuth::operator_pinned(PalwPruningSnapshotCheckpoint {
            pruning_point: h(1),
            payload_digest: h(2),
        });
        assert!(palw_pruned_ibd_snapshot_import_allowed(crate::constants::PALW_HEADER_VERSION, &v3));
        assert!(palw_pruned_ibd_snapshot_import_allowed(crate::constants::PALW_ANTISPAM_HEADER_VERSION, &v4));
        // No cross-pairing and no future version is admitted.
        assert!(!palw_pruned_ibd_snapshot_import_allowed(crate::constants::PALW_ANTISPAM_HEADER_VERSION, &v3));
        assert!(!palw_pruned_ibd_snapshot_import_allowed(crate::constants::PALW_HEADER_VERSION, &v4));
        assert!(!palw_pruned_ibd_snapshot_import_allowed(crate::constants::PALW_ANTISPAM_HEADER_VERSION + 1, &v4));
    }

    #[test]
    fn payload_derivations_reproduce_the_store_anchored_values() {
        let mut payload = snapshot().payload;
        payload.pruning_point_daa_score = 1_000;
        payload.paid_work = vec![
            PalwPrunedPaidWorkBlockV1 { block_hash: h(1), block_daa_score: 995, job_nullifiers: vec![h(0x20), h(0x21)] }, // in (5)
            PalwPrunedPaidWorkBlockV1 { block_hash: h(2), block_daa_score: 900, job_nullifiers: vec![h(0x22)] },          // out (100)
            PalwPrunedPaidWorkBlockV1 { block_hash: h(3), block_daa_score: 990, job_nullifiers: vec![h(0x20)] },          // in (10), dup
        ];
        // Only rows within walk_bound 10 of DAA 1000 (block_daa >= 990) contribute, deduped + byte-sorted.
        assert_eq!(palw_pruning_payload_paid_work_nullifiers(&payload, 10), vec![h(0x20), h(0x21)]);
        // A tighter bound drops the boundary row.
        assert_eq!(palw_pruning_payload_paid_work_nullifiers(&payload, 4), Vec::<Hash64>::new());

        // DA state root: present uses the transported state; absent uses the default DA state root.
        let default_root = crate::palw::da::PalwDaStateV1::default().state_root();
        let mut no_da = payload.clone();
        no_da.da_snapshot = None;
        assert_eq!(palw_pruning_payload_da_state_root(&no_da), default_root);
        if let Some(snapshot) = &payload.da_snapshot {
            assert_eq!(palw_pruning_payload_da_state_root(&payload), snapshot.state.state_root());
        }
    }

    #[test]
    fn chain_derived_from_payload_composes_derivations_and_verifier() {
        let mut payload = snapshot().payload;
        payload.spam_accumulator = None;
        let walk_bound = 10;
        let legacy_root = h(0x33);

        // The honest first child commits fold(legacy_root, reconstructed state_root), where the state is
        // built from the SAME payload-derived window/DA values the composition recomputes internally.
        let paid_work = palw_pruning_payload_paid_work_nullifiers(&payload, walk_bound);
        let da_root = palw_pruning_payload_da_state_root(&payload);
        let state = reconstruct_selected_parent_state_from_pruning_payload(&payload, paid_work, da_root).unwrap();
        let descendant = palw_overlay_commitment_root_v2(&legacy_root, &state.state_root());
        let bundle = PalwChainDerivedAuthBundleV1 {
            descendant_overlay_commitment_root: descendant,
            legacy_overlay_root: legacy_root,
            support_row_headers: vec![],
        };
        assert!(verify_chain_derived_pruning_boundary_from_payload(&payload, walk_bound, &bundle).is_ok());

        // Tampering the transported below-boundary paid-work window changes a derived value and is
        // rejected against the honest child's commitment — before any durable install.
        let mut tampered = payload.clone();
        tampered.paid_work.push(PalwPrunedPaidWorkBlockV1 {
            block_hash: h(0x77),
            block_daa_score: payload.pruning_point_daa_score,
            job_nullifiers: vec![h(0x99)],
        });
        assert!(verify_chain_derived_pruning_boundary_from_payload(&tampered, walk_bound, &bundle).is_err());
    }

    #[test]
    fn header_bundle_extracts_authenticated_roots_and_rejects_non_v4() {
        use crate::header::Header;
        let mk = |nonce: u64| {
            Header::new_finalized(
                crate::constants::PALW_ANTISPAM_HEADER_VERSION,
                vec![vec![1.into()]].try_into().unwrap(),
                Default::default(),
                Default::default(),
                Default::default(),
                234,
                23,
                nonce,
                crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH,
                0,
                0.into(),
                0,
                Default::default(),
            )
        };
        let mut descendant = mk(1);
        descendant.overlay_commitment_root = h(0xcc);
        let mut s1 = mk(2);
        s1.palw_spam_accumulator_commitment = h(0x11);
        let mut s2 = mk(3);
        s2.palw_spam_accumulator_commitment = h(0x22);
        let overlay = crate::dns_finality::OverlaySnapshot { bonds: vec![], reserve_balance: 0, window: vec![] };
        let wire = PalwChainDerivedHeaderBundleWireV1 {
            descendant_header: descendant,
            support_headers: vec![s1.clone(), s2.clone()],
            dns_overlay_snapshot: overlay.clone(),
        };

        let bundle = wire.extract_authenticated_bundle().unwrap();
        assert_eq!(bundle.descendant_overlay_commitment_root, h(0xcc));
        assert_eq!(bundle.legacy_overlay_root, overlay.commitment_root());
        let mut expected = vec![
            TransportedSpamHeaderCommitmentV1 { block_hash: s1.hash, spam_accumulator_commitment: h(0x11) },
            TransportedSpamHeaderCommitmentV1 { block_hash: s2.hash, spam_accumulator_commitment: h(0x22) },
        ];
        expected.sort_by(|a, b| a.block_hash.as_bytes().cmp(&b.block_hash.as_bytes()));
        assert_eq!(bundle.support_row_headers, expected);

        // A non-v4 descendant is rejected by the projection.
        let mut v3 = wire.clone();
        v3.descendant_header.version = crate::constants::PALW_HEADER_VERSION;
        assert_eq!(
            v3.extract_authenticated_bundle(),
            Err(PalwPruningSnapshotError::Incoherent("chain-derived descendant header is not Header-v4"))
        );
    }

    #[test]
    fn chain_derived_import_gate_is_fail_closed_and_v4_only() {
        // Lever off => never admitted, regardless of version.
        assert!(!palw_pruned_ibd_chain_derived_import_allowed(false, crate::constants::PALW_ANTISPAM_HEADER_VERSION));
        assert!(!palw_pruned_ibd_chain_derived_import_allowed(false, crate::constants::PALW_HEADER_VERSION));
        // Lever on => admitted for exactly Header-v4, nothing else.
        assert!(!palw_pruned_ibd_chain_derived_import_allowed(true, crate::constants::PALW_HEADER_VERSION));
        assert!(!palw_pruned_ibd_chain_derived_import_allowed(true, crate::constants::PALW_ANTISPAM_HEADER_VERSION + 1));
        assert!(palw_pruned_ibd_chain_derived_import_allowed(true, crate::constants::PALW_ANTISPAM_HEADER_VERSION));

        // The transported bundle round-trips through Borsh.
        let bundle = PalwChainDerivedAuthBundleV1 {
            descendant_overlay_commitment_root: h(1),
            legacy_overlay_root: h(2),
            support_row_headers: vec![TransportedSpamHeaderCommitmentV1 { block_hash: h(3), spam_accumulator_commitment: h(4) }],
        };
        let bytes = borsh::to_vec(&bundle).unwrap();
        assert_eq!(borsh::from_slice::<PalwChainDerivedAuthBundleV1>(&bytes).unwrap(), bundle);
    }

    #[test]
    fn active_batch_bundle_rejects_missing_tamper_duplicate_orphan_and_oversize() {
        let (view, row) = active_batch();
        assert!(validate_palw_active_batch_bundles(Some(&view), std::slice::from_ref(&row)).is_ok());

        assert_eq!(
            validate_palw_active_batch_bundles(Some(&view), &[]),
            Err(PalwPruningSnapshotError::Incoherent("active batch blob/view key set"))
        );

        let mut tampered = row.clone();
        tampered.leaves[0].job_nullifier = h(0xf1);
        assert_eq!(
            validate_palw_active_batch_bundles(Some(&view), &[tampered]),
            Err(PalwPruningSnapshotError::Incoherent("active batch leaf root"))
        );

        assert!(validate_palw_active_batch_bundles(Some(&view), &[row.clone(), row.clone()]).is_err());

        let mut orphan = row.clone();
        orphan.batch_id = h(0xf2);
        assert_eq!(
            validate_palw_active_batch_bundles(Some(&view), &[orphan]),
            Err(PalwPruningSnapshotError::Incoherent("orphan active batch blob"))
        );

        assert_eq!(
            validate_active_batch_blob_limits(1, 1, MAX_PALW_PRUNING_ACTIVE_BLOB_BYTES + 1),
            Err(PalwPruningSnapshotError::TooMany("active batch blob bytes", MAX_PALW_PRUNING_ACTIVE_BLOB_BYTES + 1))
        );
    }

    #[test]
    fn certified_active_batch_snapshot_rejects_a_missing_leaf() {
        let (view, mut row) = active_batch();
        row.leaves.clear();
        assert_eq!(
            validate_palw_active_batch_bundles(Some(&view), &[row]),
            Err(PalwPruningSnapshotError::Incoherent("active batch certificate binding"))
        );
    }

    #[test]
    fn uncertified_active_batch_canonicalizes_partial_leaf_availability_to_manifest_only() {
        let (mut view, mut row) = active_batch();
        let lifecycle = view.batches.get_mut(&row.batch_id).unwrap();
        lifecycle.status = PalwBatchStatus::Registering;
        lifecycle.cert_hash = None;
        lifecycle.first_cert_daa = None;
        row.certificate = None;

        let mut manifest_only = row.clone();
        manifest_only.leaves.clear();
        assert!(validate_palw_active_batch_bundles(Some(&view), &[manifest_only]).is_ok());
        assert_eq!(
            validate_palw_active_batch_bundles(Some(&view), &[row]),
            Err(PalwPruningSnapshotError::NonCanonical("uncertified active batch leaves"))
        );
    }

    #[test]
    fn header_v4_selected_parent_ref_root_is_unchanged_by_late_uncertified_leaf_availability() {
        let (mut view, mut available_row) = active_batch();
        let lifecycle = view.batches.get_mut(&available_row.batch_id).unwrap();
        lifecycle.status = PalwBatchStatus::Registering;
        lifecycle.cert_hash = None;
        lifecycle.first_cert_daa = None;
        available_row.certificate = None;

        let before = palw_active_batch_ref_root(Some(&view));
        // The global blob store may gain this authenticated leaf later, but the selected-parent
        // reference root has no availability input and therefore remains byte-for-byte fixed.
        assert!(!available_row.leaves.is_empty());
        let after = palw_active_batch_ref_root(Some(&view));
        assert_eq!(before, after);
    }

    #[test]
    fn rejects_corrupt_stale_and_noncanonical_payloads() {
        let mut corrupt = snapshot();
        corrupt.payload_digest = h(0xff);
        assert_eq!(corrupt.validate_canonical(), Err(PalwPruningSnapshotError::DigestMismatch));

        let mut noncanonical = snapshot();
        noncanonical.payload.provider_bonds.swap(0, 1);
        noncanonical.payload_digest = noncanonical.payload.digest();
        assert_eq!(noncanonical.validate_canonical(), Err(PalwPruningSnapshotError::NonCanonical("payload ordering")));

        let mut stale = snapshot();
        stale.payload.provider_bonds[0].created_daa_score = 101;
        stale.payload.provider_bonds[0].activation_daa_score = 101;
        stale.payload_digest = stale.payload.digest();
        assert_eq!(stale.validate_canonical(), Err(PalwPruningSnapshotError::Incoherent("provider bond")));
    }

    #[test]
    fn writer_fails_closed_when_provider_cardinality_exceeds_transport_cap() {
        // Shape validation comes after the hard cardinality fence. Keep this fixture tiny so the
        // boundary test does not allocate 32k ML-DSA public keys.
        let mut tiny = provider(1);
        tiny.owner_public_key = vec![1];
        let mut payload = snapshot().payload;
        payload.provider_bonds = vec![tiny; MAX_PALW_PRUNING_PROVIDER_BONDS + 1];
        assert_eq!(
            PalwPruningPointSnapshotV1::try_new(payload),
            Err(PalwPruningSnapshotError::TooMany("provider bonds", MAX_PALW_PRUNING_PROVIDER_BONDS + 1))
        );
    }

    #[test]
    fn writer_fails_closed_when_da_cardinality_exceeds_transport_cap() {
        let mut state = crate::palw::da::PalwDaStateV1::default();
        for index in 0..=crate::palw::da::PALW_DA_MAX_TIMEOUT_EVIDENCE {
            state.timeout_evidence.insert(Hash64::from_u64_word(index as u64));
        }
        let mut payload = snapshot().payload;
        payload.da_snapshot = Some(PalwDaPruningSnapshotV1 { version: 1, pruning_point: payload.pruning_point, state });
        assert_eq!(PalwPruningPointSnapshotV1::try_new(payload), Err(PalwPruningSnapshotError::Incoherent("DA pruning snapshot")));
    }

    /// The first post-pruning-point Header-v4 child compares its header's frozen v2 root against a
    /// root recomputed from the imported selected-parent state. Every transported component must
    /// therefore change that outer root independently; otherwise a peer could tamper that component
    /// while preserving `c == v`.
    #[test]
    fn first_post_pp_header_v4_rejects_each_tampered_selected_parent_component() {
        let base = selected_parent_state();
        let legacy = OverlaySnapshot::default().commitment_root();
        let expected = palw_overlay_commitment_root_v2(&legacy, &base.state_root());

        macro_rules! assert_tamper_rejected {
            ($name:literal, $mutation:expr) => {{
                let mut tampered = base.clone();
                $mutation(&mut tampered);
                let actual = palw_overlay_commitment_root_v2(&legacy, &tampered.state_root());
                assert_ne!(expected, actual, "{} tampering must fail Header-v4 c == v", $name);
            }};
        }

        assert_tamper_rejected!("selected parent", |state: &mut PalwSelectedParentStateV2| { state.selected_parent = h(0x91) });
        assert_tamper_rejected!("selected-parent DAA", |state: &mut PalwSelectedParentStateV2| {
            state.selected_parent_daa_score += 1
        });
        assert_tamper_rejected!("beacon frontier", |state: &mut PalwSelectedParentStateV2| {
            state.frontier.beacon_state.as_mut().unwrap().seed = h(0x61)
        });
        assert_tamper_rejected!("batch frontier", |state: &mut PalwSelectedParentStateV2| { state.frontier.overlay_view = None });
        assert_tamper_rejected!("lane frontier", |state: &mut PalwSelectedParentStateV2| {
            state.frontier.lane_bits.as_mut().unwrap().replica_bits ^= 1
        });
        assert_tamper_rejected!("nullifier frontier", |state: &mut PalwSelectedParentStateV2| {
            state.frontier.active_nullifiers.insert(h(0x62), 99);
        });
        assert_tamper_rejected!("beacon accumulator", |state: &mut PalwSelectedParentStateV2| { state.beacon_accumulator = None });
        assert_tamper_rejected!("provider view", |state: &mut PalwSelectedParentStateV2| {
            state.provider_bonds[0].amount_sompi += 1
        });
        assert_tamper_rejected!("paid-work window", |state: &mut PalwSelectedParentStateV2| {
            state.paid_work_nullifiers.push(h(0x63))
        });
        assert_tamper_rejected!("DA state", |state: &mut PalwSelectedParentStateV2| { state.da_state_root = h(0x64) });
        assert_tamper_rejected!("active batch blobs", |state: &mut PalwSelectedParentStateV2| {
            state.active_batch_ref_root = h(0x65)
        });
    }

    #[test]
    fn selected_parent_state_writer_is_canonical_and_bounded() {
        let base = selected_parent_state();
        assert!(base.validate_canonical().is_ok());

        let mut reordered = base.clone();
        reordered.provider_bonds.push(provider(2));
        reordered.provider_bonds.reverse();
        reordered.paid_work_nullifiers = vec![h(0x43), h(0x42), h(0x41)];
        let canonical = PalwSelectedParentStateV2::try_new(
            reordered.selected_parent,
            reordered.selected_parent_daa_score,
            reordered.frontier,
            reordered.beacon_accumulator,
            reordered.provider_bonds,
            reordered.paid_work_nullifiers,
            reordered.da_state_root,
            reordered.search_availability_state_root,
            reordered.active_batch_ref_root,
        )
        .unwrap();
        assert!(canonical.provider_bonds.windows(2).all(|w| cmp_outpoint(&w[0].bond_outpoint, &w[1].bond_outpoint).is_lt()));
        assert!(canonical.paid_work_nullifiers.windows(2).all(|w| w[0].as_bytes() < w[1].as_bytes()));

        let too_many_paid = vec![h(1); MAX_PALW_PRUNING_PAID_IDS + 1];
        assert_eq!(
            PalwSelectedParentStateV2::try_new(
                base.selected_parent,
                base.selected_parent_daa_score,
                base.frontier,
                base.beacon_accumulator,
                base.provider_bonds,
                too_many_paid,
                base.da_state_root,
                base.search_availability_state_root,
                base.active_batch_ref_root,
            ),
            Err(PalwPruningSnapshotError::TooMany("paid-work nullifiers", MAX_PALW_PRUNING_PAID_IDS + 1))
        );
    }
}
