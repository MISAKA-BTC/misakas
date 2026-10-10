//! **G14 lane D, phase 3: model onboarding on the real node** — the consensus objects and rows that tie a V2 model-registry class to
//! the kernel route (RFC-0011, RFC-0013, RFC-0015), all dormant behind `Params::palw_probabilistic_constraints_v1` (which no network
//! can arm). Tags 104–108; the rows ride the kernel route's aux tables 36–38 (so the delta entries `KernelRouteRow`, the carriage tail
//! `0xEC` and the `kernel-route/v1` root block already carry, revert and root them — no new delta, tail or root block).
//!
//! ```text
//! V2 class (Registered) ── 104 ArtifactBound ─► Pending ─(window)─► Matured ─(liability)─► Final        105 refutes (slash)
//!                                                  │ Matured/Final: the kernel route may attest `kernel_param_root`
//! kernel RegisterClass over that root ───────────────────────────────────────────────────────────────► kernel class
//! 106 KernelBound: V2 class ↔ kernel class (same program bytes, same artifact, the network's challenge policy)
//! 107 ConformanceCommitted: the RFC-0013 statement per (class, artifact root) — an ATTEMPT (counted, `OnboardingRecordV1`)
//! 109 ConformanceEvidence: Post (the registrant, against the beacon FUTURE OPV Finals locked) / Refute (anyone else, in the window)
//! activation gate: a kernel-bound class leaves Registered only when the binding is Final, the kernel class stands (the route
//! registered it only after `PUBLIC_PROSECUTION_COMPLETE`) and its record is past CONFORMANCE_PASSED — a commitment alone never is
//! ```
//!
//! # Conformance evidence (P0, tag 109; tables 39 and 40)
//!
//! Table **39** holds one [`ConformanceAttemptRowV1`] per class: the contract's [`OnboardingRecordV1`] (state, last failure, counted
//! attempts against the policy's limit), the current attempt's full commitment, the beacon context frozen at it, and the posted
//! evidence's summary. Table **40** holds the posted evidence's material ([`ConformanceEvidencePostV1`]) for a fresh verifier and a
//! refuter. The judgement is [`crate::palw_conformance_evidence_v1`]'s; the fold is `palw_onboarding_fold_v1`.
//!
//! # The artifact attestation (least trust)
//!
//! A kernel class registers only over an artifact root the consumer attests public. The V2 registry's `artifact_root` is a Merkle
//! root over `(tensor name, layer, byte offset, bytes)` leaves; the kernel's `ParamCommitmentsV1` root is over per-tensor
//! row/column commitments. They are roots of the SAME bytes in two unrelated hash structures, so no function of the two roots
//! shows they agree, and the fold has no bytes to compare — "same bytes ⇒ both roots" cannot be CHECKED at registration without
//! RFC-0014 §16 availability. What the chain can do, and does here, is make the statement **bonded and refutable**:
//!
//! * `ArtifactBoundV1` (104) is the registrant of an existing V2 class stating that its artifact has kernel root `K`. It reserves a
//!   slice of the registrant's free collateral until a liability horizon ends; `K` is attested to the kernel route only after a
//!   challenge window, and never once refuted.
//! * `ArtifactBindingChallengedV1` (105) is the fraud proof: two openings of the same coordinates — the V2 class's inventory leaf
//!   (a Merkle path to the registered `artifact_root`) and the kernel's tensor row (a path to the bound commitment) — that disagree
//!   in dtype, shape or bytes, or an instance set that differs from the program's declaration. Everything it needs is on chain
//!   (the program, both roots) or in the proof. It slashes the registrant's reservation (half to the challenger).
//!
//! The residual assumption is exactly the OPV one: at least one capable honest party can obtain the artifact bytes within the
//! window. A binding nobody can challenge is a declaration; the fence stays dormant until RFC-0014 §16 closes that.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::merkle::{AXIS_ROW, LayoutV1, TensorOpeningV1};
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_tir::TirProgramV1;
use std::collections::BTreeSet;

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::constants::SOMPI_PER_KASPA;
use crate::palw_artifact::{PalwArtifactOpeningV1, verify_artifact_opening_v1};
use crate::palw_conformance_evidence_v1::ConformanceEvidencePostV1;
use crate::palw_kernel_route_v1::PalwKernelRouteStateV1;
use crate::palw_mode_v2::PalwModeV2Error;
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_tir_artifact_v1::{palw_tir_inventory_leaf_count_v1, palw_tir_leaf_index_v1, palw_tir_param_instances_v1};
use misaka_palw_challenge::{BeaconContextV1, ConformanceCommitmentV1, OnboardingFailureV1, OnboardingRecordV1, OnboardingStateV1};

/// The consensus tables of the route's aux rows that hold onboarding state (the route's own are 32–35).
pub const PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1: u8 = 36;
pub const PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1: u8 = 37;
pub const PALW_ONBOARDING_TABLE_CONFORMANCE_V1: u8 = 38;
/// P0 (the lead's allocation): one [`ConformanceAttemptRowV1`] per class, keyed by the class id.
pub const PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1: u8 = 39;
/// P0: the posted evidence's material ([`ConformanceEvidencePostV1`]) of a class's current attempt, keyed by the class id.
pub const PALW_ONBOARDING_TABLE_CONFORMANCE_EVIDENCE_V1: u8 = 40;

/// The ML-DSA-87 context of every onboarding object's signature.
pub const PALW_ONBOARDING_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/onboarding/object/v1";
const PALW_ONBOARDING_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/onboarding/object-message/v1";
/// The envelope's own message domain and context (RFC-0009 G-EXPIRY / G-RULESET).
pub const PALW_SIGNED_REGISTRATION_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/onboarding/signed-registration/v1";
/// v2 (F-C4R3-01(b)): the preimage names the fork-id fired digest and the validity window's start, never `consensus_params_id`.
const PALW_SIGNED_REGISTRATION_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/onboarding/signed-registration-message/v2";

/// **INTERIM onboarding terms** — consensus constants of the (never-armed) route fence, written once here; a real activation would
/// revisit every one. The reservation is what a refuted binding costs; the window is how long a binding is Pending (not yet
/// attested); the liability is how long it stays refutable (and reserved) — it equals the kernel ledger's own liability horizon.
pub const PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1: u64 = 100 * SOMPI_PER_KASPA;
pub const PALW_ONBOARDING_BINDING_WINDOW_DAA_V1: u64 = 40;
pub const PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1: u64 = 200;
/// The challenger's share of a refuted binding's slash (permille); the rest is burned (the slash's burn at release). The PALW reporter
/// share of ADR-0032 (49 %), the route's one rate (`PALW_KERNEL_REPORTER_SHARE_PERMILLE_V1`).
pub const PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1: u64 =
    crate::palw_kernel_route_v1::PALW_KERNEL_REPORTER_SHARE_PERMILLE_V1 as u64;

/// **The message an onboarding object's signer signs**: `H(domain; network ‖ kind ‖ signer ‖ len ‖ payload)`. `payload` is the
/// object's Borsh with its signature field left out, so the signature covers every other field.
pub fn palw_onboarding_message_v1(network_domain: Hash64, kind: u8, signer: &PalwBondKeyV2, payload: &[u8]) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_ONBOARDING_MESSAGE_DOMAIN_V1).to_state();
    s.update(network_domain.as_byte_slice());
    s.update(&[kind]);
    s.update(signer.0.transaction_id.as_byte_slice());
    s.update(&signer.0.index.to_le_bytes());
    s.update(&(payload.len() as u64).to_le_bytes());
    s.update(payload);
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The message the envelope's signer signs** (RFC-0009 G-EXPIRY, G-RULESET): the network, the fork-id fired digest the signer
/// computed at `valid_from_daa` from its own compiled params ([`palw_envelope_fork_digest_v1`]), the validity window, the signer and
/// the wrapped registration's Borsh. A leaked signed bundle therefore dies at `valid_until_daa` (or at the next fence that fires),
/// and one signed under another fork schedule is not valid on this one.
pub fn palw_signed_registration_message_v1(
    network_domain: Hash64,
    fork_digest: crate::Hash,
    valid_from_daa: u64,
    valid_until_daa: u64,
    signer: &PalwBondKeyV2,
    registration_bytes: &[u8],
) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_SIGNED_REGISTRATION_MESSAGE_DOMAIN_V1).to_state();
    s.update(network_domain.as_byte_slice());
    s.update(fork_digest.as_bytes().as_slice());
    s.update(&valid_from_daa.to_le_bytes());
    s.update(&valid_until_daa.to_le_bytes());
    s.update(signer.0.transaction_id.as_byte_slice());
    s.update(&signer.0.index.to_le_bytes());
    s.update(&(registration_bytes.len() as u64).to_le_bytes());
    s.update(registration_bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The fork-id fired digest at `daa`** from a fence schedule already computed (`Params::fence_schedule_v1`) — exactly
/// `fork_id_v1(params, daa).fired` (genesis and the fences at or below `daa`), without cloning the params per envelope. What tag 108
/// names and is checked against (F-C4R3-01(b)): two builds that differ only in a fence not yet fired compute the same digest.
pub fn palw_envelope_fork_digest_v1(genesis: Hash64, schedule: &[u64], daa: u64) -> crate::Hash {
    let crossed = schedule.partition_point(|&fence| fence <= daa);
    crate::fork_id_v1::fired_fences_digest_v1(genesis, &schedule[..crossed])
}

// ---- the envelope's fence (RFC-0009 G-EXPIRY / G-RULESET) ------------------------------------------------------------------

impl Params {
    /// Whether the signed-registration envelope (tag 108) may be carried at `daa_score` — never, in this binary.
    pub fn palw_signed_registration_active_at(&self, daa_score: u64) -> bool {
        self.palw_signed_registration_v1.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa_score))
    }

    /// **The fence's refusal**: it ships beside the kernel route, which no network can arm, so any armed height is refused.
    pub fn validate_palw_signed_registration_v1(&self) -> Result<(), PalwModeV2Error> {
        match self.palw_signed_registration_v1 {
            Some(f) if f != ForkActivation::never() => Err(PalwModeV2Error::Invalid(
                "palw_signed_registration_v1 cannot be armed: it is part of the kernel route's onboarding surface (G14, RFC-0014 §16), which no network can arm",
            )),
            _ => Ok(()),
        }
    }
}

// ---- rows ---------------------------------------------------------------------------------------------------------------

/// One artifact binding `(V2 class, kernel param root)`. Its state is a function of the DAA (see [`Self::state_at`]); only the
/// refutation and the release of the reservation are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ArtifactBindingRowV1 {
    pub binder: PalwBondKeyV2,
    pub bound_daa: u64,
    /// From this DAA the kernel route may attest the root.
    pub matures_daa: u64,
    /// From this DAA the binding can no longer be refuted and its reservation is released.
    pub final_daa: u64,
    /// What is reserved against the binder's bond now (0 once released or slashed).
    pub reserved: u64,
    pub refuted: bool,
}

/// Where a binding stands at a DAA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactBindingStateV1 {
    /// Within the challenge window: not attested yet.
    Pending,
    /// Attested to the kernel route; still refutable.
    Matured,
    /// Past the liability horizon: attested and no longer refutable.
    Final,
    Refuted,
}

impl ArtifactBindingRowV1 {
    pub fn state_at(&self, daa: u64) -> ArtifactBindingStateV1 {
        if self.refuted {
            ArtifactBindingStateV1::Refuted
        } else if daa < self.matures_daa {
            ArtifactBindingStateV1::Pending
        } else if daa < self.final_daa {
            ArtifactBindingStateV1::Matured
        } else {
            ArtifactBindingStateV1::Final
        }
    }
}

/// The link between a V2 class and its kernel class (`KernelBoundV1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct KernelBindingRowV1 {
    pub kernel_class: Hash64,
    /// The artifact binding this link rests on (the kernel class's `param_commitments.root()`).
    pub kernel_param_root: Hash64,
    /// The kernel class's `VerificationPlanV1::root()`.
    pub plan_root: Hash64,
    /// The network's challenge policy id the class is bound to (equal to the route's policy).
    pub challenge_policy_id: Hash64,
    pub binder: PalwBondKeyV2,
    pub bound_daa: u64,
}

/// A conformance commitment accepted on chain for `(class, artifact root)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ConformanceRowV1 {
    /// `ConformanceCommitmentV1::statement_root` (provenance excluded).
    pub statement_root: Hash64,
    pub signer: PalwBondKeyV2,
    pub committed_daa: u64,
}

/// **How a conformance attempt ended** — the detail under the record's `last_failure` code (the codes are the contract's).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ConformanceAttemptEndV1 {
    /// `BEACON_UNAVAILABLE`: the collection window closed with fewer than `k` qualifying Finals (pending, not fraud; counted).
    BeaconUnavailable = 1,
    /// `BEACON_UNAVAILABLE`: the beacon re-derived differently when the evidence's window closed (a source lost its Final); the
    /// evidence was about a beacon this branch no longer has (counted, like any retry).
    BeaconChanged = 2,
    /// `CONFORMANCE_FAILED`: evidence bound to the attempt that is not a pass — honestly reported, forged, or past the chain's bound.
    EvidenceFailed = 3,
    /// `CONFORMANCE_FAILED`: a refutation proved the posted evidence false.
    Refuted = 4,
    /// `CONFORMANCE_FAILED`: no evidence by the deadline after the lock — a default, never a pass.
    Withheld = 5,
    /// `BEACON_VETOED` (v3): a mixed seal was withheld past the reveal window, or a mixed source ended without a standing Final — the
    /// beacon never locks without it (counted, like any beacon retry).
    BeaconVetoed = 6,
}

impl ConformanceAttemptEndV1 {
    pub const fn name(self) -> &'static str {
        match self {
            Self::BeaconUnavailable => "BEACON_UNAVAILABLE",
            Self::BeaconChanged => "BEACON_CHANGED",
            Self::EvidenceFailed => "EVIDENCE_FAILED",
            Self::Refuted => "REFUTED",
            Self::Withheld => "EVIDENCE_WITHHELD",
            Self::BeaconVetoed => "BEACON_VETOED",
        }
    }
}

/// The evidence posted for the current attempt (its material is table 40's row).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PostedEvidenceRowV1 {
    /// `BeaconConformanceEvidenceV1::id`.
    pub evidence_id: Hash64,
    /// The seed the chain derived (the selection a refutation is judged against).
    pub seed: Hash64,
    pub beacon_output: Hash64,
    pub lock_position: u64,
    pub posted_daa: u64,
    /// Refutable while `daa < window_end_daa`; it can pass only from then on.
    pub window_end_daa: u64,
}

/// **A class's conformance record** (table 39): the contract's lifecycle record and the current (or last) attempt.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ConformanceAttemptRowV1 {
    /// State, last failure, counted attempts (`beacon_retries + conformance_failures`) and the policy's limit.
    pub record: OnboardingRecordV1,
    /// The attempt's full statement: every root of its challenge subject (the commitment row of table 38 holds its root).
    pub commitment: ConformanceCommitmentV1,
    /// The accepted position of the commitment (the beacon's `commitment_position`).
    pub committed_daa: u64,
    /// The attempt's ordinal, 0-based: the beacon context's `challenge_epoch`, so a re-committed statement draws a new seed.
    pub challenge_epoch: u64,
    /// Frozen at the commitment: the route's OPV kernel classes (Panel-independent Finals; registered only after
    /// `PUBLIC_PROSECUTION_COMPLETE`) other than the excluded ones.
    pub eligible_profiles: Vec<Hash64>,
    /// The candidate (the V2 class) and its bound kernel class: never a source of its own conformance.
    pub excluded_profiles: Vec<Hash64>,
    pub evidence: Option<PostedEvidenceRowV1>,
    /// How the last closed attempt ended, and at which DAA.
    pub last_end: Option<(ConformanceAttemptEndV1, u64)>,
    /// **C4 F-C4R4-11: the refuter bonds already judged against this attempt's evidence** (sorted, unique). One judged refutation
    /// per (attempt, refuter bond) per evidence window — a repeat is refused before the charge, at no cost — so junk from a set of
    /// bonds can hold a valid refutation off for at most `|bonds| / runs` blocks, each of them paying `dismissed_proof_fee`. At most
    /// the block's runs × the window's blocks entries (each spent a run).
    pub refuters_judged: Vec<PalwBondKeyV2>,
}

impl ConformanceAttemptRowV1 {
    /// The beacon context of the current attempt under the network's policy (RFC-0007 §VI.3: frozen at the commitment).
    pub fn beacon_context(&self, policy: &misaka_palw_challenge::PostCommitChallengePolicyV1) -> BeaconContextV1 {
        BeaconContextV1 {
            chain_genesis: self.commitment.chain_genesis,
            ruleset_id: self.commitment.ruleset_id,
            policy: policy.clone(),
            subject_kind: self.commitment.subject_kind,
            commitment_root: self.commitment.statement_root(),
            commitment_position: self.committed_daa,
            challenge_epoch: self.challenge_epoch,
            eligible_profiles: self.eligible_profiles.iter().map(|h| h.as_bytes()).collect(),
            excluded_profiles: self.excluded_profiles.iter().map(|h| h.as_bytes()).collect(),
            candidate_profile_id: misaka_palw_challenge::RootV1::Present(self.commitment.candidate_id),
        }
    }

    /// The attempt is open (committed, not yet passed or ended).
    pub fn open(&self) -> bool {
        self.record.state == OnboardingStateV1::ChallengePending
    }

    /// Whether the attempt was committed under the network's complete-check policy (no beacon: the bootstrap path).
    pub fn is_complete_check(&self) -> bool {
        self.commitment.challenge_policy_id == crate::palw_opv_bootstrap_v1::palw_onboarding_complete_check_policy_v1().id()
    }

    /// Whether the attempt was committed under the network's sealed-source (v3) policy.
    pub fn is_sealed_source(&self) -> bool {
        self.commitment.challenge_policy_id == crate::palw_conformance_evidence_v1::palw_onboarding_sealed_policy_v1().id()
    }

    /// The network policy the attempt was committed under: the complete-check one, the sealed-source (v3) one, or the sampled (v2)
    /// one.
    pub fn policy(&self) -> misaka_palw_challenge::PostCommitChallengePolicyV1 {
        if self.is_complete_check() {
            crate::palw_opv_bootstrap_v1::palw_onboarding_complete_check_policy_v1()
        } else if self.is_sealed_source() {
            crate::palw_conformance_evidence_v1::palw_onboarding_sealed_policy_v1()
        } else {
            crate::palw_conformance_evidence_v1::palw_onboarding_challenge_policy_v1()
        }
    }
}

/// What stops a class from leaving `Registered`, or that nothing does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwOnboardingGateV1 {
    /// The class has no onboarding row (no artifact binding, no kernel binding): the legacy path, unchanged.
    NotKernelBound,
    Ready,
    /// Kernel-bound but not yet eligible; `code` is the RFC-0011 §17 failure / state that names the wait.
    Held {
        code: &'static str,
        why: &'static str,
    },
}

// ---- reads (the route's aux rows) -----------------------------------------------------------------------------------------

fn key2(a: &Hash64, b: &Hash64) -> Vec<u8> {
    borsh::to_vec(&(*a, *b)).expect("two digests serialize")
}

impl PalwKernelRouteStateV1 {
    /// The artifact binding of `(class, kernel root)`.
    pub fn artifact_binding_v1(&self, class: &Hash64, kernel_param_root: &Hash64) -> Option<ArtifactBindingRowV1> {
        self.aux_row(PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, &key2(class, kernel_param_root))
    }

    /// Every artifact binding of a V2 class, `(kernel root, row)`, in key order.
    pub fn artifact_bindings_of_v1(&self, class: &Hash64) -> Vec<(Hash64, ArtifactBindingRowV1)> {
        let lo = (PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, key2(class, &Hash64::from_bytes([0; 64])));
        let hi = (PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, key2(class, &Hash64::from_bytes([0xFF; 64])));
        self.aux
            .range(lo..=hi)
            .filter_map(|((_, key), row)| {
                let (_, root) = borsh::from_slice::<(Hash64, Hash64)>(key).ok()?;
                Some((root, borsh::from_slice::<ArtifactBindingRowV1>(row).ok()?))
            })
            .collect()
    }

    pub fn kernel_binding_v1(&self, class: &Hash64) -> Option<KernelBindingRowV1> {
        self.aux_row(PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1, &borsh::to_vec(class).expect("a digest serializes"))
    }

    pub fn conformance_v1(&self, class: &Hash64, artifact_root: &Hash64) -> Option<ConformanceRowV1> {
        self.aux_row(PALW_ONBOARDING_TABLE_CONFORMANCE_V1, &key2(class, artifact_root))
    }

    /// The class's conformance record and current attempt (table 39).
    pub fn conformance_attempt_v1(&self, class: &Hash64) -> Option<ConformanceAttemptRowV1> {
        self.aux_row(PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1, &borsh::to_vec(class).expect("a digest serializes"))
    }

    /// The posted evidence's material of the class's current attempt (table 40).
    pub fn conformance_evidence_post_v1(&self, class: &Hash64) -> Option<ConformanceEvidencePostV1> {
        self.aux_row(PALW_ONBOARDING_TABLE_CONFORMANCE_EVIDENCE_V1, &borsh::to_vec(class).expect("a digest serializes"))
    }

    /// **The artifact roots the kernel route may attest at `daa`**: the kernel root of every binding that is Matured or Final — never a
    /// Pending or refuted one. Derived from the rows, so every node reads the same list; this is the route's only attestation source.
    pub fn onboarding_attested_roots_v1(&self, daa: u64) -> Vec<Hash64> {
        let mut roots: Vec<Hash64> = self
            .aux
            .range(
                (PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, Vec::new())..(PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1 + 1, Vec::new()),
            )
            .filter_map(|((_, key), row)| {
                let (_, root) = borsh::from_slice::<(Hash64, Hash64)>(key).ok()?;
                let row = borsh::from_slice::<ArtifactBindingRowV1>(row).ok()?;
                matches!(row.state_at(daa), ArtifactBindingStateV1::Matured | ArtifactBindingStateV1::Final).then_some(root)
            })
            .collect();
        roots.sort();
        roots.dedup();
        roots
    }

    /// What the onboarding objects hold against `bond`'s collateral (summed over its bindings): the term V2's committed-collateral
    /// ledger and both withdrawal gates add beside the kernel ledger's own reservations.
    pub fn onboarding_reserved_v1(&self, bond: &PalwBondKeyV2) -> u64 {
        self.aux
            .range(
                (PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, Vec::new())..(PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1 + 1, Vec::new()),
            )
            .filter_map(|(_, row)| borsh::from_slice::<ArtifactBindingRowV1>(row).ok())
            .filter(|row| row.binder == *bond)
            .map(|row| row.reserved)
            .fold(0u64, |acc, r| acc.saturating_add(r))
    }

    /// Whether the route's ledger holds the kernel class (it registered only after `PUBLIC_PROSECUTION_COMPLETE`).
    pub fn kernel_class_record_v1(&self, kernel_class: &Hash64) -> Option<misaka_palw_kernel::state::ClassRecordV1> {
        let key = borsh::to_vec(&kernel_class.as_bytes()).expect("a digest serializes");
        let row = self.rows.get(&(misaka_palw_kernel::rows::TABLE_CLASSES_V1, key))?;
        borsh::from_slice(row).ok()
    }

    /// **The activation gate of a V2 class** (`activate_due_classes`): a class with no kernel binding follows the legacy path; a
    /// kernel-bound one is held until its artifact binding is Final (past the refutation horizon), the kernel class stands, and its
    /// conformance record is past `CONFORMANCE_PASSED` (verified-and-unrefuted evidence of a committed attempt, then the
    /// public-prosecution step). **A commitment alone never passes**: it waits for randomness, then for evidence, then for the window.
    ///
    /// It is [`Self::onboarding_identity_gate_v1`] with DA16's artifact-lapse hold in front (a kernel-bound, standing class whose pair
    /// lapsed after it was bound). ADR-0177 withdraws that hold (DA16b re-scopes it), so the G14-for-rewards gate never asks it.
    pub fn onboarding_gate_v1(&self, class: &Hash64, artifact_root: &Hash64, daa: u64) -> PalwOnboardingGateV1 {
        if let Some(binding) = self.kernel_binding_v1(class)
            && self.kernel_class_record_v1(&binding.kernel_class).is_some()
            && self
                .artifact_binding_v1(class, &binding.kernel_param_root)
                .is_some_and(|row| self.provider_pair_lapsed_since_v1(class, &binding.kernel_param_root, row.bound_daa))
        {
            return PalwOnboardingGateV1::Held {
                code: "AVAILABILITY_REQUIRED",
                why: "the artifact's bytes stopped being publicly obtainable inside the binding's horizon (every provider was charged): re-bind over live leases",
            };
        }
        self.onboarding_identity_gate_v1(class, artifact_root, daa)
    }

    /// **The onboarding gate's IDENTITY part** — everything [`Self::onboarding_gate_v1`] asks except DA16's artifact-lapse hold: kernel
    /// bound, the kernel class standing, the binding past its refutation horizon and unrefuted (a clock and proofs, never whether the
    /// bytes are served — the historical code `AVAILABILITY_REQUIRED` of a binding inside its horizon means only that), and the
    /// conformance record. What the G14-for-rewards gate reads (ADR-0177; `docs/design/palw/opv-beacon-bootstrap.md` §14.1).
    pub fn onboarding_identity_gate_v1(&self, class: &Hash64, artifact_root: &Hash64, daa: u64) -> PalwOnboardingGateV1 {
        let Some(binding) = self.kernel_binding_v1(class) else {
            // A class with an artifact binding (live or refuted) has begun onboarding: it is held until it is kernel-bound. Only a class
            // with no onboarding row at all follows the legacy path.
            return if self.artifact_bindings_of_v1(class).is_empty() {
                PalwOnboardingGateV1::NotKernelBound
            } else {
                PalwOnboardingGateV1::Held {
                    code: "REGISTERED_DORMANT",
                    why: "the class has an artifact binding but no kernel binding (tag 106) yet",
                }
            };
        };
        if self.kernel_class_record_v1(&binding.kernel_class).is_none() {
            return PalwOnboardingGateV1::Held {
                code: "PUBLIC_PROSECUTION_INCOMPLETE",
                why: "the kernel class the V2 class is bound to is not registered in the route",
            };
        }
        match self.artifact_binding_v1(class, &binding.kernel_param_root).map(|row| row.state_at(daa)) {
            Some(ArtifactBindingStateV1::Final) => {}
            Some(ArtifactBindingStateV1::Refuted) | None => {
                return PalwOnboardingGateV1::Held {
                    code: "AVAILABILITY_REQUIRED",
                    why: "the artifact binding is refuted or missing",
                };
            }
            Some(_) => {
                return PalwOnboardingGateV1::Held {
                    code: "AVAILABILITY_REQUIRED",
                    why: "the artifact binding is inside its refutation horizon",
                };
            }
        }
        if self.conformance_v1(class, artifact_root).is_none() {
            return PalwOnboardingGateV1::Held {
                code: "REGISTERED_DORMANT",
                why: "no conformance commitment is on chain for this (class, artifact root)",
            };
        }
        let Some(attempt) = self.conformance_attempt_v1(class) else {
            return PalwOnboardingGateV1::Held { code: "REGISTERED_DORMANT", why: "no conformance record for this class" };
        };
        // The commitment must be of THIS artifact (a new artifact is a new class).
        if attempt.commitment.artifact_root != artifact_root.as_bytes() {
            return PalwOnboardingGateV1::Held { code: "REGISTERED_DORMANT", why: "the conformance record is of another artifact" };
        }
        conformance_gate_v1(&attempt)
    }
}

/// **Where a class's conformance record holds it** (the codes are the contract's state and failure codes).
pub fn conformance_gate_v1(attempt: &ConformanceAttemptRowV1) -> PalwOnboardingGateV1 {
    use OnboardingStateV1 as S;
    match attempt.record.state {
        S::G14Eligible | S::ActiveRewardable => PalwOnboardingGateV1::Ready,
        S::ConformancePassed => PalwOnboardingGateV1::Held {
            code: OnboardingFailureV1::PublicProsecutionIncomplete.code(),
            why: "conformance passed; the bound kernel class's public-prosecution step has not",
        },
        S::ChallengePending if attempt.is_complete_check() => PalwOnboardingGateV1::Held {
            code: S::ChallengePending.code(),
            why: "a complete-check commitment awaits its PostComplete (no randomness: the fold checks every input and leaf)",
        },
        S::ChallengePending if attempt.evidence.is_some() => PalwOnboardingGateV1::Held {
            code: S::ChallengePending.code(),
            why: "evidence posted and verified in the fold; its challenge window is open (it passes only unrefuted)",
        },
        S::ChallengePending => PalwOnboardingGateV1::Held {
            code: OnboardingFailureV1::BeaconUnavailable.code(),
            why: "waiting for randomness: k future Panel-independent Finals, then the attempt's evidence",
        },
        _ if attempt.record.attempts() >= attempt.record.attempt_limit => PalwOnboardingGateV1::Held {
            code: OnboardingFailureV1::ConformanceFailed.code(),
            why: "the policy's conformance attempts are exhausted",
        },
        _ => PalwOnboardingGateV1::Held {
            code: attempt.record.last_failure.map(OnboardingFailureV1::code).unwrap_or(S::RegisteredDormant.code()),
            why: "the last conformance attempt ended without a pass: commit again (counted)",
        },
    }
}

// ---- the read model (RPC op 230) -------------------------------------------------------------------------------------------

/// **Where a V2 class stands on the onboarding path** — every row the route holds for it and the gate's verdict with its reason, so a
/// registrant, a verifier and an explorer read one answer. Nothing here is private: it is all chain state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnboardingReadV1 {
    pub class_id: Hash64,
    /// The V2 class's status (`Registered { .. }`, `Active`, ...).
    pub status: String,
    pub artifact_root: Hash64,
    pub registrant: Option<PalwBondKeyV2>,
    /// `(kernel param root, row, state at the tip)`.
    pub artifact_bindings: Vec<(Hash64, ArtifactBindingRowV1, &'static str)>,
    pub kernel_binding: Option<KernelBindingRowV1>,
    pub conformance: Option<ConformanceRowV1>,
    /// The class's conformance record and current attempt (table 39).
    pub conformance_attempt: Option<ConformanceAttemptRowV1>,
    pub gate: PalwOnboardingGateV1,
    /// The route's committed ledger root (the anchor for the rows).
    pub ledger_root: Hash64,
    pub aux_root: Hash64,
}

impl crate::palw_state_v2::PalwChainStateV2 {
    /// **The onboarding read of a V2 class** at `daa` (`None`: no such class).
    pub fn onboarding_read_v1(&self, class: &Hash64, daa: u64) -> Option<OnboardingReadV1> {
        let state = self.class(class)?;
        let route = self.kernel_route();
        let bindings = route
            .map(|r| {
                r.artifact_bindings_of_v1(class)
                    .into_iter()
                    .map(|(root, row)| {
                        let name = match row.state_at(daa) {
                            ArtifactBindingStateV1::Pending => "Pending",
                            ArtifactBindingStateV1::Matured => "Matured",
                            ArtifactBindingStateV1::Final => "Final",
                            ArtifactBindingStateV1::Refuted => "Refuted",
                        };
                        (root, row, name)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(OnboardingReadV1 {
            class_id: *class,
            status: format!("{:?}", state.status),
            artifact_root: state.artifact_root,
            registrant: state.registrant_bond,
            artifact_bindings: bindings,
            kernel_binding: route.and_then(|r| r.kernel_binding_v1(class)),
            conformance: route.and_then(|r| r.conformance_v1(class, &state.artifact_root)),
            conformance_attempt: route.and_then(|r| r.conformance_attempt_v1(class)),
            gate: route
                .map(|r| r.onboarding_gate_v1(class, &state.artifact_root, daa))
                .unwrap_or(PalwOnboardingGateV1::NotKernelBound),
            ledger_root: route.map(|r| r.ledger_root()).unwrap_or_default(),
            aux_root: route.map(|r| r.aux_root()).unwrap_or_default(),
        })
    }
}

// ---- the beacon's facts and the conformance read (RPC op 231) ---------------------------------------------------------------

impl PalwKernelRouteStateV1 {
    /// **The beacon's facts**: every Final the route serves (op 212) that carries an attributed `WorkFinalEventV1` — an OPV Final,
    /// `FinalPathV1::PanelIndependent`, with its producer; a Panel-licensed Final exports none. The fold and every reader derive the
    /// beacon from exactly these (the contract filters, sorts, de-duplicates and applies the policy's distinct source rule). `Err`
    /// only if the stored rows do not rebuild (corruption).
    pub fn beacon_events_v1(&self) -> Result<Vec<misaka_palw_challenge::AttributedWorkV1>, String> {
        let mut out = Vec::new();
        for f in self.finals_read_v1()? {
            if let Some(bytes) = f.event {
                out.push(borsh::from_slice(&bytes).map_err(|e| e.to_string())?);
            }
        }
        Ok(out)
    }
}

/// **An attempt's beacon, whichever the policy** (v2's accumulator or v3's sealed sources), as the fold, the tick, the chunk lane and
/// op 231 read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttemptBeaconV1 {
    /// No beacon yet; `state` names the phase (`COLLECTING`, `CANDIDATE`, `SEALING`, `REVEALING`, `SETTLING`).
    Waiting {
        state: &'static str,
        have: u32,
        lock_position: Option<u64>,
    },
    Locked(misaka_palw_challenge::beacon::VerifiedWorkBeaconV1),
    /// `BEACON_UNAVAILABLE`: too few qualifying sources (counted).
    Unavailable {
        have: u32,
        need: u32,
    },
    /// v3 only, `BEACON_VETOED`: a mixed seal withheld or a mixed source abandoned (counted).
    Vetoed {
        withheld: u32,
        failed: u32,
    },
}

impl PalwKernelRouteStateV1 {
    /// **The v3 beacon's facts**: every claim seal the kernel ledger holds — live, revealed with its salt, or forfeited (G14-R4's
    /// tables 25–26, `claim_beacon_seals_v1`) — as `SealedSourceV3`: the profile is the sealed job's class (job rows are permanent),
    /// the producer the sealing bond, and a reveal's fate is its claim's (Final, with its op-212 facts; failed when convicted, timed
    /// out or unavailable; live otherwise). The consumer is `Absent` until the ledger keeps the job's poster (F-C4R4-08, G14-R4's
    /// table 18). `Err` only if the stored rows do not rebuild.
    pub fn beacon_sealed_sources_v1(&self) -> Result<Vec<misaka_palw_challenge::SealedSourceV3>, String> {
        use misaka_palw_challenge::{RootV1, SealRevealV3, SealedSourceV3, SourceAttributionV1, SourceFateV3};
        let ledger = self.ledger()?;
        let finals: std::collections::BTreeMap<misaka_palw_kernel::hash::Digest, misaka_palw_challenge::WorkFinalEventV1> = self
            .finals_read_v1()?
            .into_iter()
            .filter_map(|f| {
                let bytes = f.event?;
                let w: misaka_palw_challenge::AttributedWorkV1 = borsh::from_slice(&bytes).ok()?;
                Some((f.receipt.claim, w.event))
            })
            .collect();
        let mut out = Vec::new();
        for s in ledger.claim_beacon_seals_v1() {
            let Some(profile) = ledger
                .jobs
                .get(&s.job)
                .map(|j| j.class_binding_id)
                .or_else(|| ledger.pipeline_jobs.get(&s.job).map(|j| j.class_binding_id))
            else {
                continue;
            };
            let reveal = s.revealed.map(|(claim, revealed_daa, salt)| {
                use misaka_palw_kernel::lifecycle::ClaimStateV1 as C;
                let fate = match (ledger.claims.get(&claim), finals.get(&claim)) {
                    (_, Some(ev)) => SourceFateV3::Final(ev.clone()),
                    (Some(row), None) if row.convicted || matches!(row.life.state, C::Unavailable { .. } | C::TimedOut { .. }) => {
                        SourceFateV3::Failed
                    }
                    (Some(_), None) => SourceFateV3::Live,
                    (None, None) => SourceFateV3::Failed,
                };
                SealRevealV3 { reveal_position: revealed_daa, salt, fate }
            });
            out.push(SealedSourceV3 {
                source_profile_id: profile,
                attribution: SourceAttributionV1 { producer_id: s.producer, consumer_id: RootV1::Absent },
                seal: s.seal,
                seal_position: s.sealed_daa,
                reveal,
            });
        }
        Ok(out)
    }

    /// **The beacon of `attempt` at `daa`** under its own policy. `Err`: a complete check (no beacon), an invalid policy, or rows that
    /// do not rebuild.
    pub fn attempt_beacon_v1(&self, attempt: &ConformanceAttemptRowV1, daa: u64) -> Result<AttemptBeaconV1, String> {
        let policy = attempt.policy();
        if attempt.is_complete_check() {
            return Err("a complete check draws no beacon".into());
        }
        let ctx = attempt.beacon_context(&policy);
        if attempt.is_sealed_source() {
            use misaka_palw_challenge::SealedBeaconStateV3 as B;
            return Ok(
                match misaka_palw_challenge::collect_sealed_work_beacon_v3(&ctx, &self.beacon_sealed_sources_v1()?, daa)
                    .map_err(|e| e.to_string())?
                {
                    B::Sealing { sealed, .. } => AttemptBeaconV1::Waiting { state: "SEALING", have: sealed, lock_position: None },
                    B::Revealing { revealed, .. } => {
                        AttemptBeaconV1::Waiting { state: "REVEALING", have: revealed, lock_position: None }
                    }
                    B::Settling { finals, .. } => AttemptBeaconV1::Waiting { state: "SETTLING", have: finals, lock_position: None },
                    B::Candidate { mixed, lock_position } => {
                        AttemptBeaconV1::Waiting { state: "CANDIDATE", have: mixed, lock_position: Some(lock_position) }
                    }
                    B::Locked(b) => AttemptBeaconV1::Locked(b),
                    B::Unavailable { mixed, need } => AttemptBeaconV1::Unavailable { have: mixed, need },
                    B::Vetoed { withheld, failed } => AttemptBeaconV1::Vetoed { withheld, failed },
                },
            );
        }
        use misaka_palw_challenge::WorkBeaconStateV1 as B;
        Ok(
            match misaka_palw_challenge::collect_attributed_work_beacon_v1(&ctx, &self.beacon_events_v1()?, daa)
                .map_err(|e| e.to_string())?
            {
                B::Collecting { have, .. } => AttemptBeaconV1::Waiting { state: "COLLECTING", have, lock_position: None },
                B::Candidate { have, lock_position } => {
                    AttemptBeaconV1::Waiting { state: "CANDIDATE", have, lock_position: Some(lock_position) }
                }
                B::Locked(b) => AttemptBeaconV1::Locked(b),
                B::Unavailable { have, need } => AttemptBeaconV1::Unavailable { have, need },
            },
        )
    }
}

/// **The capture-proof chunk lane's target for conformance evidence** (G14-R4's tag-113 lane, `PalwKernelChunkTargetV1::Conformance
/// { v2_class }`): the LAST DAA at which a part of a tag-109 object naming `v2_class` may arrive, or `None` when the class has no
/// attempt that can still take one. The lane bounds a group's life by `min(64 DAA, this)`; its opener must be the class's registrant
/// (a Post) — the lane checks that, this function only says until when.
///
/// * an open SAMPLED attempt with no evidence — a Post: `lock + PALW_CONFORMANCE_EVIDENCE_DEADLINE_DAA_V1` once the beacon locked (a
///   Post in that block is applied before the closing step defaults the attempt); before the lock, the latest lock the collection
///   window allows (`S + W − 1 + D`) plus the deadline; `None` once the beacon is unavailable;
/// * an open sampled attempt with evidence — a Refute: `window_end − 1` (a refutation is taken while `daa < window_end`), `None` after;
/// * a complete-check attempt: `None` — a `PostComplete` rides one carrier by construction and never needs a chunk lane;
/// * no attempt, or a closed one: `None`.
pub fn palw_conformance_chunk_target_v1(route: &PalwKernelRouteStateV1, v2_class: &Hash64, daa: u64) -> Option<u64> {
    let attempt = route.conformance_attempt_v1(v2_class)?;
    if !attempt.open() || attempt.is_complete_check() {
        return None;
    }
    if let Some(posted) = attempt.evidence {
        return (daa < posted.window_end_daa).then(|| posted.window_end_daa - 1);
    }
    let policy = attempt.policy();
    let ctx = attempt.beacon_context(&policy);
    let deadline = crate::palw_conformance_evidence_v1::PALW_CONFORMANCE_EVIDENCE_DEADLINE_DAA_V1;
    match route.attempt_beacon_v1(&attempt, daa).ok()? {
        AttemptBeaconV1::Locked(b) => {
            let last = b.lock_position.saturating_add(deadline);
            (daa <= last).then_some(last)
        }
        // v2: the latest lock its collection window allows, plus the deadline. v3 locks only once every mixed source is Final, which
        // no window bounds in advance: no part before the lock (a Post needs the seed anyway).
        AttemptBeaconV1::Waiting { .. } if !attempt.is_sealed_source() => {
            Some(ctx.window_end().saturating_sub(1).saturating_add(policy.settlement_depth_d).saturating_add(deadline))
        }
        AttemptBeaconV1::Waiting { .. } | AttemptBeaconV1::Unavailable { .. } | AttemptBeaconV1::Vetoed { .. } => None,
    }
}

/// **The conformance read of a V2 class (RPC op 231)** — the raw rows of tables 39 and 40 (a fresh verifier decodes them itself and
/// can check them against the aux root op 211's rows rebuild), the network's challenge policy, and the beacon state this node derives
/// at the read's DAA from the same Finals op 212 serves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceEvidenceReadV1 {
    pub class_id: Hash64,
    /// Table 39's row, raw.
    pub attempt_row: Option<Vec<u8>>,
    /// Table 40's row, raw (a `ConformanceEvidencePostV1`).
    pub evidence_row: Option<Vec<u8>>,
    /// The class's canonical program bytes (its IR record; byte for byte its bound kernel class's, tag 106).
    pub program: Option<Vec<u8>>,
    pub attempt: Option<ConformanceAttemptRowV1>,
    pub policy: misaka_palw_challenge::PostCommitChallengePolicyV1,
    /// `COLLECTING`, `CANDIDATE`, `LOCKED`, `UNAVAILABLE` (empty with no open attempt).
    pub beacon: &'static str,
    pub beacon_have: u32,
    pub beacon_need: u32,
    pub lock_position: Option<u64>,
    pub beacon_output: Option<Hash64>,
    pub gate: PalwOnboardingGateV1,
    pub ledger_root: Hash64,
    pub aux_root: Hash64,
}

impl crate::palw_state_v2::PalwChainStateV2 {
    /// **The conformance read of a V2 class** at `daa` (`None`: no such class).
    pub fn conformance_evidence_read_v1(&self, class: &Hash64, daa: u64) -> Option<ConformanceEvidenceReadV1> {
        let state = self.class(class)?;
        let route = self.kernel_route();
        let key = borsh::to_vec(class).expect("a digest serializes");
        let raw = |table: u8| route.and_then(|r| r.aux.get(&(table, key.clone())).cloned());
        let attempt = route.and_then(|r| r.conformance_attempt_v1(class));
        let policy = attempt
            .as_ref()
            .map(|a| a.policy())
            .unwrap_or_else(crate::palw_conformance_evidence_v1::palw_onboarding_challenge_policy_v1);
        let (mut beacon, mut have, mut need, mut lock, mut output) = ("", 0u32, policy.work_count_k, None, None);
        if attempt.as_ref().is_some_and(|a| a.open() && a.is_complete_check()) {
            // A complete check draws no randomness: there is no beacon to wait for.
            beacon = "COMPLETE_CHECK";
        } else if let (Some(route), Some(a)) = (route, attempt.as_ref())
            && a.open()
        {
            match route.attempt_beacon_v1(a, daa) {
                Ok(AttemptBeaconV1::Waiting { state, have: h, lock_position }) => (beacon, have, lock) = (state, h, lock_position),
                Ok(AttemptBeaconV1::Locked(b)) => {
                    (beacon, have, lock, output) =
                        ("LOCKED", b.sources.len() as u32, Some(b.lock_position), Some(Hash64::from_bytes(b.output)))
                }
                Ok(AttemptBeaconV1::Unavailable { have: h, need: n }) => (beacon, have, need) = ("UNAVAILABLE", h, n),
                Ok(AttemptBeaconV1::Vetoed { .. }) => beacon = "VETOED",
                Err(_) => beacon = "POLICY_INVALID",
            }
        }
        Some(ConformanceEvidenceReadV1 {
            class_id: *class,
            attempt_row: raw(PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1),
            evidence_row: raw(PALW_ONBOARDING_TABLE_CONFORMANCE_EVIDENCE_V1),
            program: self.tir_class_v1(class).map(|r| r.program.as_slice().to_vec()),
            attempt,
            policy,
            beacon,
            beacon_have: have,
            beacon_need: need,
            lock_position: lock,
            beacon_output: output,
            gate: route
                .map(|r| r.onboarding_gate_v1(class, &state.artifact_root, daa))
                .unwrap_or(PalwOnboardingGateV1::NotKernelBound),
            ledger_root: route.map(|r| r.ledger_root()).unwrap_or_default(),
            aux_root: route.map(|r| r.aux_root()).unwrap_or_default(),
        })
    }
}

// ---- the fraud proof ----------------------------------------------------------------------------------------------------------

/// **Proof that a bound kernel artifact is not the V2 class's artifact.**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ArtifactMismatchProofV1 {
    /// The bound commitments' instance set is not the program's declared instance set (a tensor missing or surplus).
    Instances { commitments: ParamCommitmentsV1 },
    /// One tensor instance: the kernel's committed dtype / shape is not the declaration's, or its row bytes differ from the V2
    /// inventory's leaf over the same bytes.
    Row {
        commitments: ParamCommitmentsV1,
        param: u16,
        layer: Option<u16>,
        kernel_row: TensorOpeningV1,
        v2_opening: PalwArtifactOpeningV1,
    },
}

/// **Verify a refutation** against what the chain holds: the V2 class's program and registered `artifact_root`, and the bound kernel
/// root. `Ok(())` means the binding is FALSE (a fraud proven); `Err` says why the proof proves nothing. Pure and total: every path
/// is a `Result`, no allocation is proportional to an attacker-chosen shape beyond the opening's own bounded fields.
pub fn verify_artifact_mismatch_v1(
    program: &TirProgramV1,
    v2_artifact_root: Hash64,
    kernel_param_root: Hash64,
    proof: &ArtifactMismatchProofV1,
) -> Result<(), &'static str> {
    let commitments = match proof {
        ArtifactMismatchProofV1::Instances { commitments } | ArtifactMismatchProofV1::Row { commitments, .. } => commitments,
    };
    if commitments.root() != kernel_param_root.as_bytes() {
        return Err("the carried commitments do not root to the bound kernel root");
    }
    let declared: BTreeSet<(u16, Option<u16>)> = palw_tir_param_instances_v1(program)
        .into_iter()
        .enumerate()
        .flat_map(|(j, layers)| layers.into_iter().map(move |l| (j as u16, l)))
        .collect();
    match proof {
        ArtifactMismatchProofV1::Instances { commitments } => {
            let committed: BTreeSet<(u16, Option<u16>)> = commitments.by_instance.keys().copied().collect();
            if committed == declared { Err("the bound commitments hold exactly the program's declared instances") } else { Ok(()) }
        }
        ArtifactMismatchProofV1::Row { commitments, param, layer, kernel_row, v2_opening } => {
            if !declared.contains(&(*param, *layer)) {
                return Err("the program declares no such tensor instance");
            }
            let decl = program.params.get(*param as usize).ok_or("the program declares no such param")?;
            let Some(commitment) = commitments.by_instance.get(&(*param, *layer)) else {
                return Err("the bound commitments hold no such instance (use the Instances proof)");
            };
            if !kernel_row.authenticates(commitment) {
                return Err("the kernel opening does not authenticate against the bound commitment");
            }
            // The V2 opening is a leaf of the registered inventory, at the leaf the program's declarations put these bytes in.
            verify_artifact_opening_v1(v2_opening, v2_artifact_root)
                .map_err(|_| "the V2 opening does not reach the registered artifact root")?;
            let op = &v2_opening.operand;
            if op.tensor_name != decl.name || op.layer != *layer {
                return Err("the V2 opening names another tensor instance");
            }
            if Some(v2_opening.leaf_index) != palw_tir_leaf_index_v1(program, *param, *layer, op.row_start as u64)
                || palw_tir_inventory_leaf_count_v1(program).ok() != Some(v2_opening.leaf_count)
            {
                return Err("the V2 opening is not at the leaf the program's layout puts these bytes in");
            }
            // (a) the kernel's committed tensor is not the declared one.
            let declared_shape: Vec<u64> = decl.shape.iter().map(|d| *d as u64).collect();
            if kernel_row.dtype != decl.dtype.tag() || kernel_row.shape != declared_shape {
                return Ok(());
            }
            // (b) the same bytes, two answers: compare the overlap of the kernel row's byte range with the V2 leaf's.
            if kernel_row.axis != AXIS_ROW {
                return Err("only a row opening of the kernel tensor is compared");
            }
            let shape = kernel_row.shape_usize().ok_or("the kernel opening's shape is not addressable")?;
            let layout = LayoutV1::try_of(&shape).ok_or("the kernel opening's shape overflows")?;
            let width = decl.dtype.width() as u64;
            let row_start = kernel_row
                .index
                .checked_mul(layout.row_len)
                .and_then(|e| e.checked_mul(width))
                .ok_or("the kernel row's byte offset overflows")?;
            let row_end =
                row_start.checked_add(kernel_row.values.len() as u64 * width).ok_or("the kernel row's byte range overflows")?;
            let (leaf_start, leaf_end) = (op.row_start as u64, (op.row_start as u64).saturating_add(op.bytes.len() as u64));
            let (lo, hi) = (row_start.max(leaf_start), row_end.min(leaf_end));
            if lo >= hi {
                return Err("the two openings cover no common byte");
            }
            for at in lo..hi {
                let kernel_byte = {
                    let element = ((at - row_start) / width) as usize;
                    let within = ((at - row_start) % width) as usize;
                    kernel_row.values[element].to_le_bytes()[within]
                };
                if op.bytes[(at - leaf_start) as usize] != kernel_byte {
                    return Ok(());
                }
            }
            Err("the two openings agree on every byte they share")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The object tags are the lead's allocation (104–108 lane D's, 109 P0's) and are declared, not inferred.
    #[test]
    fn the_onboarding_object_tags_are_the_allocated_ones() {
        use crate::palw_state_v2::PalwConsensusObjectV2 as O;
        let bond = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(1), 0));
        let h = Hash64::from_bytes([1; 64]);
        let commitment = misaka_palw_challenge::ConformanceCommitmentV1 {
            version: 1,
            chain_genesis: [0; 64],
            ruleset_id: [0; 64],
            subject_kind: misaka_palw_challenge::SubjectKindV1::ModelConformance,
            candidate_id: [0; 64],
            kernel_descriptor_id: [0; 64],
            challenge_policy_id: [0; 64],
            artifact_root: [0; 64],
            program_root: [0; 64],
            source_root: misaka_palw_challenge::RootV1::Absent,
            tokenizer_or_input_schema_root: misaka_palw_challenge::RootV1::Absent,
            layout_root: [0; 64],
            verification_plan_root: [0; 64],
            constraint_root: misaka_palw_challenge::RootV1::Absent,
            implementation_set_root: [0; 64],
            test_scope_root: [0; 64],
            calibration_id: misaka_palw_challenge::RootV1::Absent,
            input_and_state_binding_root: misaka_palw_challenge::RootV1::Absent,
            resource_profile_id: [0; 64],
            commitment_object_id: None,
            canonical_commitment_position: None,
        };
        let bound = O::ArtifactBoundV1 { v2_class: h, kernel_param_root: h, signer: bond, signature: vec![1] };
        let objects = [
            (bound.clone(), 104u8),
            (
                O::ArtifactBindingChallengedV1 {
                    v2_class: h,
                    kernel_param_root: h,
                    challenger: bond,
                    proof: Box::new(ArtifactMismatchProofV1::Instances { commitments: ParamCommitmentsV1::default() }),
                    signature: vec![1],
                },
                105,
            ),
            (O::KernelBoundV1 { v2_class: h, kernel_class: h, challenge_policy_id: h, signer: bond, signature: vec![1] }, 106),
            (O::ConformanceCommittedV1 { commitment: Box::new(commitment), signer: bond, signature: vec![1] }, 107),
            (
                O::SignedRegistrationV1 {
                    registration: Box::new(bound),
                    valid_from_daa: 1,
                    valid_until_daa: 9,
                    fork_digest: crate::Hash::from_bytes([2; 32]),
                    signer: bond,
                    signature: vec![1],
                },
                108,
            ),
            (
                O::ConformanceEvidenceV1 {
                    v2_class: h,
                    action: Box::new(crate::palw_conformance_evidence_v1::ConformanceEvidenceActionV1::Refute {
                        evidence_id: h,
                        fault: Box::new(crate::palw_conformance_evidence_v1::ConformanceFaultV1::VectorTokens {
                            check: 0,
                            kernel_claim: h,
                        }),
                    }),
                    signer: bond,
                    signature: vec![1],
                },
                109,
            ),
        ];
        for (object, tag) in objects {
            assert_eq!(borsh::to_vec(&object).unwrap()[0], tag, "{object:?}");
            assert_eq!(borsh::from_slice::<O>(&borsh::to_vec(&object).unwrap()).unwrap(), object, "round trip, tag {tag}");
            assert_eq!(crate::palw_state_v2::palw_object_is_onboarding_v1(&object), (104..=107).contains(&tag) || tag == 109);
            assert_eq!(crate::palw_state_v2::palw_object_is_signed_registration_v1(&object), tag == 108);
        }
    }

    /// **F-C4R3-01(b)**: the envelope's digest is the fork id's fired digest; a fence not yet fired does not move it, a fence that
    /// fires between signing and inclusion does (the envelope expires across it).
    #[test]
    fn the_envelope_names_the_fork_id_fired_digest_which_a_future_fence_does_not_move() {
        let params = Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12));
        let schedule = params.fence_schedule_v1();
        for daa in [0u64, 1, 500, 9_000, 1_000_000] {
            assert_eq!(
                palw_envelope_fork_digest_v1(params.genesis.hash, &schedule, daa),
                crate::fork_id_v1::fork_id_v1(&params, daa).fired,
                "exactly fork_id_v1's fired digest at {daa}"
            );
        }
        let g = Hash64::from_bytes([3; 64]);
        let (now, later) = ([10u64, 20], [10u64, 20, 5_000_000]);
        assert_eq!(
            palw_envelope_fork_digest_v1(g, &now, 15),
            palw_envelope_fork_digest_v1(g, &later, 15),
            "a future fence moves nothing"
        );
        assert_ne!(
            palw_envelope_fork_digest_v1(g, &now, 15),
            palw_envelope_fork_digest_v1(g, &now, 25),
            "a fence fired since signing"
        );
        assert_ne!(
            palw_envelope_fork_digest_v1(g, &now, 15),
            palw_envelope_fork_digest_v1(Hash64::from_bytes([4; 64]), &now, 15),
            "another chain"
        );
    }

    #[test]
    fn the_tables_are_the_allocated_ones_and_a_binding_reads_its_state_from_the_clock() {
        assert_eq!(
            (
                PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1,
                PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1,
                PALW_ONBOARDING_TABLE_CONFORMANCE_V1
            ),
            (36, 37, 38)
        );
        assert_eq!((PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1, PALW_ONBOARDING_TABLE_CONFORMANCE_EVIDENCE_V1), (39, 40));
        let row = ArtifactBindingRowV1 {
            binder: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(1), 0)),
            bound_daa: 10,
            matures_daa: 50,
            final_daa: 210,
            reserved: 7,
            refuted: false,
        };
        assert_eq!(row.state_at(49), ArtifactBindingStateV1::Pending);
        assert_eq!(row.state_at(50), ArtifactBindingStateV1::Matured);
        assert_eq!(row.state_at(209), ArtifactBindingStateV1::Matured);
        assert_eq!(row.state_at(210), ArtifactBindingStateV1::Final);
        assert_eq!(ArtifactBindingRowV1 { refuted: true, ..row }.state_at(300), ArtifactBindingStateV1::Refuted);
    }
}
