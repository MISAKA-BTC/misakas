use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const WIRE_VERSION_V1: u16 = 1;
pub const CLASS_ROLE_V1: u8 = 1;
pub const OUTSIDER_ROLE_V1: u8 = 2;
pub const MAX_CANDIDATES_V1: u32 = 4096;
pub const MAX_PENDING_V1: u32 = 1024;
pub const MAX_TRACKED_V1: u32 = 65_536;
pub const MAX_BEACON_PROOF_BYTES_V1: u32 = 65_536;
/// RFC-0006 × RFC-0010: at most this many strata (a plan's `S_L` is at most 64) …
pub const MAX_STRATA_V1: u16 = 64;
/// … at most this many class seats a stratum …
pub const MAX_STRATUM_CLASS_SEATS_V1: u16 = 15;
/// … and at most this many seats a stratified binding (64 shards × `[outsider] ++ 3 class seats`).
pub const MAX_STRATIFIED_SEATS_V1: u32 = 256;

/// **A stratified draw** (RFC-0006's per-shard Panel under RFC-0010's permissionless rule): the claim's Panel is `count` strata,
/// each `[outsider?] ++ class_seats` seats, stored stratum-major. Generic — the engine knows nothing of shards: stratum `s`'s class
/// seats come from the candidates the host marked as members of `s` (`PanelSnapshotV1::strata_members`), its outsider from the
/// OUTSIDER-role population. Frozen at admission; `None` on a record is the flat draw, byte for byte the old rules.
///
/// The rules (`stage_assign`, re-checked by the carriage validation):
/// * stratum `s` is drawn from its own seed `H("misaka-palw/panel-v3/stratum-seed" ‖ seed ‖ s)` with the flat draw's tickets;
/// * inside a stratum, one seat per operator, and the stratum's outsider is none of its class seats;
/// * an operator holds at most one outsider seat over the claim's whole life (every round, every stratum);
/// * a retry never reuses, in stratum `s`, an operator that sat in `s` in an earlier round (class or outsider) — alternates are
///   never reused per stratum; an operator may sit in several strata (distinct duties);
/// * a bond seated in several strata of one round reserves the claim's per-seat exposure ONCE (the one ledger's duty row holds a
///   bond once) and needs headroom for it once;
/// * a stratum that cannot be filled ends the claim `NoCapablePanel` — a stratified claim never falls back to a flat Panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelStrataV1 {
    pub count: u16,
    pub class_seats: u16,
    pub outsider: bool,
}

impl PanelStrataV1 {
    /// `2 ≤ count ≤ 64`, `1 ≤ class_seats ≤ 15`, at most [`MAX_STRATIFIED_SEATS_V1`] seats in all.
    pub fn validate(&self) -> Result<(), PanelErrorV1> {
        if !(2..=MAX_STRATA_V1).contains(&self.count)
            || !(1..=MAX_STRATUM_CLASS_SEATS_V1).contains(&self.class_seats)
            || self.seat_count() > MAX_STRATIFIED_SEATS_V1 as usize
        {
            return Err(PanelErrorV1::InvalidSnapshot);
        }
        Ok(())
    }
    /// Seats a stratum: `[outsider?] ++ class_seats`.
    pub fn stride(&self) -> u16 {
        self.class_seats + u16::from(self.outsider)
    }
    /// Seats a binding: `count × stride`.
    pub fn seat_count(&self) -> usize {
        usize::from(self.count) * usize::from(self.stride())
    }
    /// The stratum bits a member may carry (`count` low bits).
    pub fn member_mask(&self) -> u64 {
        if self.count >= 64 { u64::MAX } else { (1u64 << self.count) - 1 }
    }
}

/// All numbers are experimental configuration, not network defaults. The whole value is committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelPolicyV1 {
    pub seal_depth_blocks: u64,
    pub seal_wait_daa: u64,
    pub bond_maturity_daa: u64,
    pub beacon_period_daa: u64,
    pub beacon_wait_daa: u64,
    pub assignment_delay_daa: u64,
    pub receipt_window_daa: u64,
    pub seat_count: u16,
    pub outsider_seats: u16,
    pub max_retries: u16,
    pub min_collateral: u64,
    pub max_candidates: u32,
    pub max_pending: u32,
    pub max_pending_per_bond: u32,
    pub max_assignments_per_block: u32,
    pub max_admissions_per_block: u32,
    pub max_tracked_claims: u32,
    pub max_beacons_per_block: u32,
    pub max_beacon_proof_bytes: u32,
    /// Exact verifier/transcript specification identity. Zero never denotes a usable source.
    pub beacon_scheme: Hash64,
}

impl PanelPolicyV1 {
    pub fn validate(&self) -> Result<(), PanelErrorV1> {
        if self.seal_depth_blocks == 0
            || self.seal_wait_daa == 0
            || self.beacon_period_daa == 0
            || self.beacon_wait_daa == 0
            || self.beacon_wait_daa >= self.beacon_period_daa
            || self.assignment_delay_daa == 0
            || self.receipt_window_daa == 0
            || self.seat_count == 0
            || self.outsider_seats > 1
            || self.outsider_seats >= self.seat_count
            || self.min_collateral == 0
            || self.max_retries > 16
            || self.max_candidates < self.seat_count as u32
            || self.max_candidates > MAX_CANDIDATES_V1
            || self.max_pending == 0
            || self.max_pending > MAX_PENDING_V1
            || self.max_pending_per_bond == 0
            || self.max_pending_per_bond > self.max_pending
            || self.max_assignments_per_block == 0
            || self.max_assignments_per_block > self.max_pending
            || self.max_admissions_per_block == 0
            || self.max_admissions_per_block > self.max_pending
            || self.max_tracked_claims < self.max_pending
            || self.max_tracked_claims > MAX_TRACKED_V1
            || self.max_beacons_per_block == 0
            || self.max_beacons_per_block > self.max_pending
            || self.max_beacon_proof_bytes == 0
            || self.max_beacon_proof_bytes > MAX_BEACON_PROOF_BYTES_V1
            || self.beacon_scheme == Hash64::default()
        {
            return Err(PanelErrorV1::InvalidPolicy);
        }
        Ok(())
    }

    pub fn id(&self) -> Hash64 {
        digest("misaka-palw/panel-v3/policy", self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BondIdV1 {
    pub transaction: Hash64,
    pub index: u32,
}

/// Already admitted under execution, fee, bond/exposure and identity rules; not a raw submission.
/// `immutable_fields` commits the complete canonical claim payload and payout, excluding signatures
/// and mutable phases. `claim_id` is lookup only: resigning cannot change the draw identity.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdmittedClaimV1 {
    pub claim_id: Hash64,
    pub work_id: Hash64,
    pub class_id: Hash64,
    pub producer: BondIdV1,
    pub producer_operator: Hash64,
    pub producer_key: Hash64,
    pub immutable_fields: Hash64,
    pub required_exposure: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatCandidateV1 {
    pub bond: BondIdV1,
    pub operator: Hash64,
    pub key: Hash64,
    pub collateral: u64,
    pub registered_daa: u64,
    /// Public, authenticated capability/readiness record commitments, not local artifacts.
    pub capability_root: Hash64,
    pub readiness_root: Hash64,
    pub roles: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimSealV1 {
    pub id: Hash64,
    pub accepted_block: Hash64,
    pub accepted_daa: u64,
    pub accepted_height: u64,
    pub acceptance_order: u64,
    pub occurrence_index: u32,
    pub checkpoint: Hash64,
    pub checkpoint_daa: u64,
    pub checkpoint_height: u64,
    pub anchor_slot: u64,
    pub beacon_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelSnapshotV1 {
    pub root: Hash64,
    pub checkpoint: Hash64,
    pub checkpoint_daa: u64,
    pub checkpoint_height: u64,
    pub policy_id: Hash64,
    pub class_id: Hash64,
    pub excluded_bond: BondIdV1,
    pub excluded_operator: Hash64,
    pub excluded_key: Hash64,
    pub candidates: Vec<SeatCandidateV1>,
    /// **The strata populations** of a stratified claim, frozen with everything else at the pre-entropy checkpoint (the root covers
    /// them): one bitmap per candidate, aligned with `candidates` — bit `s` set iff the candidate may hold a class seat of stratum
    /// `s`, and the CLASS role set iff some bit is. Empty for a flat claim.
    pub strata_members: Vec<u64>,
}

impl PanelSnapshotV1 {
    pub fn computed_root(&self) -> Hash64 {
        let mut copy = self.clone();
        copy.root = Hash64::default();
        digest("misaka-palw/panel-v3/snapshot", &copy)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeaconProofV1 {
    pub epoch: u64,
    pub output: Hash64,
    pub proof: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeaconRequestV1 {
    pub network: Hash64,
    pub ruleset: Hash64,
    pub scheme: Hash64,
    pub epoch: u64,
    pub release_daa: u64,
    pub deadline_daa: u64,
}

/// How the caller's own record of a bound claim stands for the receipt clock. The engine's built-in clock is
/// `bound_daa + receipt_window_daa` from its own binding; a host that pauses or re-bases the receipt window (a data-availability
/// session, a verification-horizon credit) reports it here so the engine never redraws a Panel whose window the host stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptClockV1 {
    /// The window runs from this bound DAA (the host's, possibly credited later than the engine's own binding).
    Running { bound_daa: u64 },
    /// The window is stopped (a pause): no timeout is due and no retry is drawn.
    Paused,
}

/// This interface must read one validated selected-chain base, before this block's own objects.
/// There is deliberately no built-in production beacon verifier, mock VRF or block-hash fallback.
/// Certificate verification must bind the entire request, enforce unique epoch output, canonical
/// inclusion before the deadline, fork/domain rules and a bounded verification cost.
pub trait ConsensusViewV1 {
    fn candidates(&self, claim: &AdmittedClaimV1) -> Result<Vec<SeatCandidateV1>, PanelErrorV1>;
    fn available_collateral(&self, bond: &BondIdV1) -> u128;
    fn verify_beacon(&self, request: &BeaconRequestV1, proof: &BeaconProofV1) -> Result<(), PanelErrorV1>;
    /// Validate a terminal claim outcome under the existing receipt/court/payout rules.
    fn terminal_claim(&self, claim: &Hash64) -> bool;
    /// The host's receipt clock for a currently bound claim; `None` (the default) is the engine's own
    /// `bound_daa + receipt_window_daa`.
    fn receipt_clock(&self, _claim: &Hash64) -> Option<ReceiptClockV1> {
        None
    }
    /// **The population of a stratified claim** ([`PanelStrataV1`]), at the same checkpoint and under the same rules as
    /// [`Self::candidates`]: the candidates and, aligned with them, each one's stratum bits (bit `s`: may hold a class seat of
    /// stratum `s`; the CLASS role iff some bit, the OUTSIDER role for the outsider population). The default — a host that cannot
    /// answer for strata — is an empty population: such a claim seals nothing it can use and ends `NoCapablePanel`, never a
    /// failed fold.
    fn stratified_candidates(
        &self,
        _claim: &AdmittedClaimV1,
        _strata: &PanelStrataV1,
    ) -> Result<(Vec<SeatCandidateV1>, Vec<u64>), PanelErrorV1> {
        Ok((Vec::new(), Vec::new()))
    }
}

/// The scalar header of the engine state: the unit a production state store journals when the selected chain advances.
/// Everything else in the state is keyed (claims, retained work identities, certified epoch outputs), so a block's change to the
/// state is exactly a cursor change plus keyed row changes; reservations are derived from the claims and never journalled.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelCursorV1 {
    pub version: u16,
    pub network: Hash64,
    pub ruleset: Hash64,
    pub policy: PanelPolicyV1,
    pub tip: Hash64,
    pub height: u64,
    pub daa: u64,
    pub next_order: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelBoundV3 {
    pub claim_seal_id: Hash64,
    pub panel_snapshot_root: Hash64,
    pub beacon_id: Hash64,
    pub panel_seed_v3: Hash64,
    pub entropy_ready_daa: u64,
    pub assignment_point: u64,
    /// Inclusion witness ONLY; no derivation below uses it as entropy.
    pub binding_block: Hash64,
    pub bound_daa: u64,
    pub retry_index: u16,
    pub seats: Vec<BondIdV1>,
    pub exposure: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub enum NonFraudReasonV1 {
    SealUnavailable,
    BeaconUnavailable,
    NoCapablePanel,
    PanelUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub enum ClaimPhaseV3 {
    PendingSeal,
    Sealed,
    EntropyReady {
        ready_daa: u64,
        assignment_point: u64,
        seed: Hash64,
        beacon_id: Hash64,
    },
    Bound(PanelBoundV3),
    /// Reservation handed back only after the caller validates a terminal legacy-lattice outcome.
    Released {
        daa: u64,
    },
    Voided {
        daa: u64,
        reason: NonFraudReasonV1,
    },
}

impl ClaimPhaseV3 {
    pub fn terminal(&self) -> bool {
        matches!(self, Self::Released { .. } | Self::Voided { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRecordV3 {
    pub claim: AdmittedClaimV1,
    pub accepted_block: Hash64,
    pub accepted_daa: u64,
    pub accepted_height: u64,
    pub acceptance_order: u64,
    pub occurrence_index: u32,
    pub seal: Option<ClaimSealV1>,
    pub snapshot: Option<PanelSnapshotV1>,
    pub used_operators: Vec<Hash64>,
    pub binding_history: Vec<PanelBoundV3>,
    pub phase: ClaimPhaseV3,
    /// The claim's strata, frozen at admission; `None` is the flat draw.
    pub strata: Option<PanelStrataV1>,
}

/// Ordered *accepted* objects, not a producer-chosen subset. Fold assignments precede admissions.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedChainStepV1 {
    pub block: Hash64,
    pub parent: Hash64,
    pub height: u64,
    pub daa: u64,
    pub admissions: Vec<AdmittedClaimV1>,
    pub beacons: Vec<BeaconProofV1>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PanelFoldEventsV1 {
    pub bindings: BTreeMap<Hash64, PanelBoundV3>,
    /// No slash, strike, reward or refund instruction is attached to these events.
    pub non_fraud_voids: BTreeMap<Hash64, NonFraudReasonV1>,
    pub released: Vec<Hash64>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PanelErrorV1 {
    #[error("invalid permissionless Panel policy")]
    InvalidPolicy,
    #[error("noncanonical selected-chain step")]
    NoncanonicalStep,
    #[error("duplicate claim/work identity")]
    DuplicateClaim,
    #[error("permissionless Panel resource bound exceeded")]
    ResourceLimit,
    #[error("invalid pre-entropy snapshot")]
    InvalidSnapshot,
    #[error("beacon source is unimplemented or certificate invalid")]
    InvalidBeacon,
    #[error("conflicting certified beacon outputs")]
    BeaconEquivocation,
    #[error("arithmetic overflow")]
    Overflow,
    #[error("invalid or incompatible Panel state carriage")]
    InvalidCarriage,
}

pub(crate) fn digest<T: BorshSerialize>(domain: &str, value: &T) -> Hash64 {
    let bytes = borsh::to_vec(value).expect("in-memory consensus serialization");
    digest_bytes(domain, &bytes)
}

pub(crate) fn digest_bytes(domain: &str, bytes: &[u8]) -> Hash64 {
    // Domain-as-message framing supports domains longer than BLAKE2b's 64-byte key limit.
    let mut h = blake2b_simd::Params::new().hash_length(64).to_state();
    h.update(&(domain.len() as u32).to_le_bytes());
    h.update(domain.as_bytes());
    h.update(bytes);
    Hash64::from_bytes(h.finalize().as_bytes().try_into().expect("BLAKE2b-512"))
}

pub(crate) fn add(a: u64, b: u64) -> Result<u64, PanelErrorV1> {
    a.checked_add(b).ok_or(PanelErrorV1::Overflow)
}
