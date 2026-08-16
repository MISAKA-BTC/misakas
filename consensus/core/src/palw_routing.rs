//! PALW re-verification routing v1 — ADR-0034's three keys, consensus-inert.
//!
//! Normative source: ADR-0034 (Accepted 2026-08-16). Routing identity is three-keyed with
//! strictly ordered authority: an **execution family** (four values, frozen) routes work to
//! hardware that could hold it; a **model band** (five values, frozen) sizes windows, bonds
//! and capacity; only the exact **binding** — a [`PalwClassRegistrationV1`] row carrying the
//! routing keys, `binding_id = registration_id()` — decides what a replay means. A family is
//! a routing index, never a determinism claim (the measured premise: Metal ≠ CPU on the same
//! job, arm ≡ x86 only 7/8 seeds, EPYC ≠ Broadwell 4/8 — comparing roots because families
//! match repeats the golden-gate label bug at protocol scale).
//!
//! Everything here is **computed and logged only**: no consensus validation, fork choice,
//! acceptance or credit path reads any of it. The credit predicate stays ADR-0033's
//! `decide_credit_v1`, untouched; ADR-0034 §7 explicitly rejects any
//! `FINALIZED_WITHOUT_REPLAY`-shaped path, so this module deliberately exposes **no**
//! crediting API at all — randomness and risk may schedule work and scale redundancy, never
//! make an unchecked job safe.
//!
//! What lives here, in ADR order:
//!
//! * **§1** [`PalwExecutionFamilyV1`] / [`PalwModelBandV1`] — the two coarse keys, with the
//!   authority table in their docs.
//! * **§3** [`ModelDefinitionV1`] (the registered preimage of `model_profile_id`) and
//!   [`PalwExecutionFamilyManifestV1`] (a family generation's admitted runtime lineage).
//! * **§4** [`derived_model_band_v1`] — bands are derived from measured resources, never
//!   declared; the registry's `validate()` rejects a declared band that does not equal this
//!   derivation (the `CeilingNotDerived` pattern, applied to bands).
//! * **§5** [`validate_receipt_routing_keys_v1`] — the registry, not the miner, gives carried
//!   keys meaning; band forgery is invalidity, and the resolved row's recomputed id must
//!   equal the carried `binding_id` (the lookup-collision guard).
//! * **§6** [`PalwVerifierCapabilityV1`] — hardware capability plus a Merkle-committed ready
//!   set ([`ready_binding_root_v1`]); a ready claim without a proof is not a claim.
//! * **§7** [`select_routed_replay_panel_v1`] — the ADR-0028 §2 lottery with binding-aware
//!   eligibility, under its own domain key (the assignment-twin discipline), plus the
//!   escalation re-draw that replaces the draft's monopoly claiming.
//! * **§10** [`coverage_state_v1`] / [`binding_may_activate_v1`] — a binding nobody can
//!   replay does not get to exist quietly.
//!
//! Every number that is not a landed constant is a **placeholder until devnet measurement or
//! the economic-simulation gate fixes it** (ADR-0034 "Initial parameters"), and is named as
//! such at its definition.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash, Hash64};
use thiserror::Error;

use crate::palw_registry::PalwClassRegistrationV1;
use crate::palw_schedule::PalwReplayCostMeasurementV1;
use crate::palw_slash::PALW_S_MAX_SIGNATURE_BYTES;
use crate::tx::TransactionOutpoint;

// ---------------------------------------------------------------------------------------------
// Versions, domains, constants
// ---------------------------------------------------------------------------------------------

pub const PALW_ROUTING_OBJECT_VERSION_V1: u16 = 1;

/// Identity of a registered model definition (keyed BLAKE2b-512 over canonical Borsh).
pub const PALW_ROUTING_DOMAIN_MODEL_DEFINITION_ID: &[u8] = b"misaka-palw/model-definition-id/v1";
/// Keyed-BLAKE2b-256 signing message of a model definition (network-bound, signature-less
/// preimage — the publisher signs this).
pub const PALW_ROUTING_DOMAIN_MODEL_DEFINITION_MESSAGE: &[u8] = b"misaka-palw/model-definition-message/v1";
/// ML-DSA-87 signing context for model definitions (key resolution is the caller's).
pub const PALW_ROUTING_MLDSA87_MODEL_DEFINITION_CONTEXT: &[u8] = b"misaka-palw/model-definition/mldsa87/v1";
/// Identity of a family-version manifest.
pub const PALW_ROUTING_DOMAIN_FAMILY_MANIFEST_ID: &[u8] = b"misaka-palw/family-manifest-id/v1";
/// Identity of a verifier capability record.
pub const PALW_ROUTING_DOMAIN_CAPABILITY_ID: &[u8] = b"misaka-palw/verifier-capability-id/v1";
/// Keyed-BLAKE2b-256 signing message of a verifier capability (network-bound).
pub const PALW_ROUTING_DOMAIN_CAPABILITY_MESSAGE: &[u8] = b"misaka-palw/verifier-capability-message/v1";
/// ML-DSA-87 signing context for verifier capabilities.
pub const PALW_ROUTING_MLDSA87_CAPABILITY_CONTEXT: &[u8] = b"misaka-palw/verifier-capability/mldsa87/v1";
/// Ready-set Merkle leaf domain. Leaf ≠ node ≠ root domains: an internal node can never be
/// presented as a leaf (second-preimage separation).
pub const PALW_ROUTING_DOMAIN_READY_LEAF: &[u8] = b"misaka-palw/ready-binding-leaf/v1";
/// Ready-set Merkle internal-node domain.
pub const PALW_ROUTING_DOMAIN_READY_NODE: &[u8] = b"misaka-palw/ready-binding-node/v1";
/// Ready-set root finalization domain — commits the leaf COUNT beside the top hash, so a
/// proof's claimed tree geometry is part of what the root pins.
pub const PALW_ROUTING_DOMAIN_READY_ROOT: &[u8] = b"misaka-palw/ready-binding-root/v1";
/// The routed assignment ticket (ADR-0034 §7). Must never equal the ADR-0028 ticket domain or
/// `vlt::VERIFIER_SORTITION_KEY`: three lotteries of the same shape, three keys — one shared
/// key would make one draw predict another.
pub const PALW_ROUTING_DOMAIN_ASSIGNMENT_TICKET: &[u8] = b"misaka-palw/routed-replay-assignment-ticket/v1";

/// Every domain this module introduces (uniqueness-tested against every other PALW family and
/// the VLT sortition key).
pub const PALW_ROUTING_ALL_DOMAINS: &[&[u8]] = &[
    PALW_ROUTING_DOMAIN_MODEL_DEFINITION_ID,
    PALW_ROUTING_DOMAIN_MODEL_DEFINITION_MESSAGE,
    PALW_ROUTING_MLDSA87_MODEL_DEFINITION_CONTEXT,
    PALW_ROUTING_DOMAIN_FAMILY_MANIFEST_ID,
    PALW_ROUTING_DOMAIN_CAPABILITY_ID,
    PALW_ROUTING_DOMAIN_CAPABILITY_MESSAGE,
    PALW_ROUTING_MLDSA87_CAPABILITY_CONTEXT,
    PALW_ROUTING_DOMAIN_READY_LEAF,
    PALW_ROUTING_DOMAIN_READY_NODE,
    PALW_ROUTING_DOMAIN_READY_ROOT,
    PALW_ROUTING_DOMAIN_ASSIGNMENT_TICKET,
];

/// ADR-0034 §4 band bases — **placeholders until devnet measurement fixes them**. A binding
/// is band `b` when every measured resource fits `base << b`; shipping a placeholder as
/// measured is the §15-class violation it always is.
pub const PALW_ROUTING_BAND_ARTIFACT_BASE_BYTES: u64 = 4 << 30; // 4 GiB
pub const PALW_ROUTING_BAND_MEMORY_BASE_BYTES: u64 = 8 << 30; // 8 GiB
pub const PALW_ROUTING_BAND_PROOF_BASE_BYTES: u64 = 64 << 20; // 64 MiB

/// `BASE_REPLAY_WORK_UNITS` — a **frozen snapshot** of row 1's measured full-replay cost at
/// its credited ceiling when the v1 derivation was defined: `4 300 + 165 · 4 095 = 679 975 ms`
/// (the 2026-08-16 fleet bench, slowest host). B0 means "costs about what the reference
/// binding cost at v1-definition time". Future re-benches do NOT move this constant: every
/// registered band re-derives against it, so an edit here re-bands every binding at once —
/// re-banding-by-constant, the silent global flip the band-in-the-id design exists to prevent.
/// A base change is a new derivation version, never an edit (the profile rule, applied to a
/// number). The registry test pins the literal AND that row 1 still derives B0 under it.
pub const PALW_ROUTING_BASE_REPLAY_WORK_MS: u64 = 679_975;

/// Largest ready set a capability may commit (2²⁰ bindings) and the sibling bound it implies
/// (derived, so the two cannot drift apart). Caps precede any hashing — adversarial
/// allocations stay bounded.
pub const PALW_ROUTING_MAX_READY_BINDINGS: u32 = 1 << 20;
pub const PALW_ROUTING_MAX_READY_SIBLINGS: usize = PALW_ROUTING_MAX_READY_BINDINGS.ilog2() as usize;

/// A family generation admits a bounded runtime-manifest lineage (per-class signed bundles,
/// ADR-0026 §8 — a lineage, not an open set).
pub const PALW_ROUTING_MAX_ADMITTED_MANIFESTS: usize = 64;

/// At most two versions of one family are active at once (`current`, `previous`) — ADR-0034 §1.
pub const PALW_ROUTING_MAX_ACTIVE_FAMILY_VERSIONS: usize = 2;

/// Coverage-ladder epochs (ADR-0034 §10) — **placeholders**, simulation-gated.
pub const PALW_ROUTING_COVERAGE_THROTTLE_EPOCHS: u32 = 2;
pub const PALW_ROUTING_COVERAGE_FREEZE_EPOCHS: u32 = 4;

/// Minimum independent ready re-executors for binding activation (ADR-0034 §10; same control
/// domain counts once). The capability TTL/heartbeat numbers (30 min / 5 min) deliberately do
/// NOT live here: they are agent-surface policy, recorded in the ADR's parameter table, and a
/// constant with no consumer is how doc-rot starts.
pub const PALW_ROUTING_MIN_READY_DEVNET: u32 = 3;
pub const PALW_ROUTING_MIN_READY_MAINNET: u32 = 5;

/// The one keyed-BLAKE2b-512 → `Hash64` construction every identity in this module uses
/// (sequential `update` = concatenation, so multi-part call sites hash the same bytes as a
/// pre-concatenated buffer). One body instead of seven hand-rolled copies: a mistyped
/// `hash_length` or a dropped `.key()` in a lone copy would silently fork that object's id
/// space — the sibling `keyed64` idiom of palw_step/palw_legs/palw_slash, continued.
fn keyed_hash64(domain: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for part in parts {
        h.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwRoutingError {
    #[error("unsupported palw-routing object version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("routing object is not canonical: {0}")]
    NotCanonical(&'static str),
    #[error("the resolved registry row's recomputed id does not equal the carried binding id")]
    BindingIdMismatch,
    #[error("carried execution family {carried:?} does not equal the registered {registered:?}")]
    FamilyMismatch { carried: PalwExecutionFamilyV1, registered: PalwExecutionFamilyV1 },
    #[error("carried model band {carried:?} does not equal the registered {registered:?} — band forgery is invalidity")]
    BandForged { carried: PalwModelBandV1, registered: PalwModelBandV1 },
    #[error("binding is not accepting receipts in coverage state {0:?}")]
    BindingNotAccepting(PalwBindingCoverageStateV1),
}

// ---------------------------------------------------------------------------------------------
// §1 — the two coarse keys
// ---------------------------------------------------------------------------------------------

/// Where a job *could* run — never a set whose members agree (ADR-0034's premise). A family
/// may influence candidate discovery, capability indexing, coverage accounting and UI
/// grouping; it may never influence a verdict, a comparison of roots, or a slash. Frozen at
/// four: new acceleration hardware must be shown to fit an existing family before any
/// proposal to add one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwExecutionFamilyV1 {
    /// Apple GPU runtimes. Every Apple-libm-dependent class is `StructuralOnly` in depth
    /// (ADR-0031's admission boundary) — a family fact surfaced per binding, not assumed.
    Metal = 1,
    /// **Reserved, empty.** The tree's only CUDA reference is the literal `cuda-off`;
    /// admitting the first binding is the ADR-0027 falsifiable conformance campaign.
    Cuda = 2,
    /// **Reserved, empty**, same campaign rule as CUDA.
    Rocm = 3,
    /// Pinned deterministic CPU builds (today's `CPU_BUILD_PROFILE` discipline:
    /// no-blas/no-openmp/single-variant). NOT the adjudicator: one-step adjudication runs in
    /// canonical reference arithmetic, a separate thing needing no family membership.
    Cpu = 4,
}

/// Coarse load class of a binding (ADR-0034 §4): sizes windows, bond floors, capacity caps
/// and audit-rate scaling. Never self-declared, never a verdict input, and band alone never
/// selects a verifier. Frozen at five — no B5.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwModelBandV1 {
    B0 = 0,
    B1 = 1,
    B2 = 2,
    B3 = 3,
    B4 = 4,
}

impl PalwModelBandV1 {
    const ALL: [PalwModelBandV1; 5] =
        [PalwModelBandV1::B0, PalwModelBandV1::B1, PalwModelBandV1::B2, PalwModelBandV1::B3, PalwModelBandV1::B4];
}

/// The v1 **bootstrap default** a family-manifest author starts from when valuing
/// [`PalwExecutionFamilyManifestV1::max_active_band`] — the ADR grants exactly one cap
/// ("CPU max active band = B1"; row 1 is B0); every other family stays at B0 until its own
/// registration wave argues otherwise. Activation never reads this function: the operative
/// cap is the manifest field, a registration-observable fact, so raising it is a new manifest
/// record — never a code edit two observers could disagree across mid-rollout.
pub fn initial_family_max_active_band_v1(family: PalwExecutionFamilyV1) -> PalwModelBandV1 {
    match family {
        PalwExecutionFamilyV1::Cpu => PalwModelBandV1::B1,
        PalwExecutionFamilyV1::Metal | PalwExecutionFamilyV1::Cuda | PalwExecutionFamilyV1::Rocm => PalwModelBandV1::B0,
    }
}

/// The two reserved families (ADR-0034 §9): no code paths exist for them today, and admitting
/// the FIRST binding in either is the ADR-0027 falsifiable conformance campaign — so v1
/// activation refuses them outright rather than trusting caller-supplied golden counts.
pub fn family_is_reserved_v1(family: PalwExecutionFamilyV1) -> bool {
    matches!(family, PalwExecutionFamilyV1::Cuda | PalwExecutionFamilyV1::Rocm)
}

/// Every class tag registered on some PALW network that routing can NAME — the one
/// reverse-index authority agents (the re-executor, future explorers) import instead of each
/// keeping a private copy that lags. Grows by transcription when a class registers, exactly
/// like the kernel catalog. `misaka-palw-lite-cpu/other-arch/v1` (vlt's
/// decline-to-participate placeholder) is deliberately absent: a build that declines to
/// participate is not a routable backend, and resolving it would route duties to a host that
/// abstains by design.
pub const PALW_REGISTERED_CLASS_TAGS: &[&str] =
    &["misaka-palw-lite-cpu/x86_64/v1", "misaka-palw-lite-cpu/aarch64-dotprod/v1", "misaka-palw-lite-fp/apple-metal-arm64/v1"];

/// Machine-reads the routing keys OUT of a class tag (`family-segment/arch-segment/vN`), so
/// a registration's declared `execution_family`/`family_version` can be checked against the
/// same string its `runtime_class_id` hashes — without this, the family would be
/// self-declared, and a Metal-runtime row claiming `Cpu` would draft panels of CPU verifiers
/// who can never reproduce the trace (duties lapse into escalation instead of refutation).
/// Fail-closed: a tag this parser does not recognize registers nothing — a new tag shape is
/// a deliberate code change, never an inferred family.
pub fn routing_keys_for_class_tag_v1(class_tag: &str) -> Option<(PalwExecutionFamilyV1, u16)> {
    let mut segments = class_tag.split('/');
    let (family_segment, arch_segment, version_segment) = (segments.next()?, segments.next()?, segments.next()?);
    if segments.next().is_some() || family_segment.is_empty() || arch_segment.is_empty() {
        return None;
    }
    let family_version: u16 = version_segment.strip_prefix('v')?.parse().ok()?;
    if family_version == 0 {
        return None;
    }
    let family = if arch_segment.contains("metal") {
        PalwExecutionFamilyV1::Metal
    } else if arch_segment.contains("cuda") {
        PalwExecutionFamilyV1::Cuda
    } else if arch_segment.contains("rocm") {
        PalwExecutionFamilyV1::Rocm
    } else if family_segment.ends_with("cpu") {
        PalwExecutionFamilyV1::Cpu
    } else {
        return None;
    };
    Some((family, family_version))
}

// ---------------------------------------------------------------------------------------------
// §4 — band derivation (derived, never declared)
// ---------------------------------------------------------------------------------------------

/// The replay-work input of the band derivation: a binding's full-replay cost at its credited
/// ceiling, from its own registered measurement — measurement-derived, never miner-declared.
/// Saturating on purpose: an overflowing claim derives `u64::MAX` work and lands past B4,
/// which is "not registrable", the fail-closed answer.
pub fn replay_work_ms_v1(replay_cost: &PalwReplayCostMeasurementV1, credited_ceiling_tokens: u32) -> u64 {
    replay_cost.fixed_overhead_ms.saturating_add(replay_cost.ms_per_decode_token.saturating_mul(credited_ceiling_tokens as u64))
}

/// ADR-0034 §4: `resource_score = max(S_artifact, S_memory, S_work, S_proof)`; band `b` is
/// the smallest with `score ≤ 2^b`, integer-exact (`value ≤ base << b` per dimension — no
/// floats near a registered fact). `None` = past 16× = **not registrable in v1**.
pub fn derived_model_band_v1(
    model_artifact_bytes: u64,
    peak_memory_bytes: u64,
    max_replay_work_ms: u64,
    max_proof_material_bytes: u64,
) -> Option<PalwModelBandV1> {
    PalwModelBandV1::ALL.into_iter().find(|band| {
        let m = *band as u32;
        model_artifact_bytes <= PALW_ROUTING_BAND_ARTIFACT_BASE_BYTES << m
            && peak_memory_bytes <= PALW_ROUTING_BAND_MEMORY_BASE_BYTES << m
            && max_replay_work_ms <= PALW_ROUTING_BASE_REPLAY_WORK_MS << m
            && max_proof_material_bytes <= PALW_ROUTING_BAND_PROOF_BASE_BYTES << m
    })
}

// ---------------------------------------------------------------------------------------------
// §3 — the model definition (the registered preimage of `model_profile_id`)
// ---------------------------------------------------------------------------------------------

/// Model identity, registered separately from execution identity; the two join in a binding.
/// `model_profile_id` is the id every v2 envelope already carries — until now an opaque
/// input, here given a preimage under the `qwen35_pins` discipline, generalized.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelDefinitionV1 {
    /// = [`PALW_ROUTING_OBJECT_VERSION_V1`].
    pub version: u16,
    pub model_profile_id: Hash64,
    /// Artifact identity: the exact gguf digest and size.
    pub gguf_sha256: [u8; 32],
    pub gguf_size: u64,
    /// `tokenizer_id_v2_for_gguf` lineage — pinned outside the runtime.
    pub tokenizer_id: Hash64,
    pub architecture_id: Hash64,
    pub total_parameter_count: u64,
    /// MoE honesty (ADR-0034 draft §7.1): totals alone misclassify. Recorded so future
    /// work-unit derivations can read it; the **v1 band's work input is the measured replay
    /// cost** ([`replay_work_ms_v1`]), not a parameter count — nothing derives from this
    /// field yet, and no doc should claim otherwise.
    pub active_parameter_count: u64,
    /// Publisher's ML-DSA-87 signature over [`model_definition_message_v1`]; key resolution
    /// and verification are the caller's (the registry resolves publishers, never this module).
    pub publisher_signature: Vec<u8>,
}

/// Keyed-BLAKE2b-256 signing message of a model definition — network-bound, layout mirroring
/// `palw_execution_attestation_message_v1`: length-prefixed network id, then fixed-width
/// fields in struct order, signature excluded.
pub fn model_definition_message_v1(network_id: &[u8], definition: &ModelDefinitionV1) -> Hash {
    let mut hasher = blake2b_simd::Params::new().hash_length(32).key(PALW_ROUTING_DOMAIN_MODEL_DEFINITION_MESSAGE).to_state();
    hasher.update(&(network_id.len() as u32).to_le_bytes());
    hasher.update(network_id);
    hasher.update(&definition.version.to_le_bytes());
    hasher.update(definition.model_profile_id.as_byte_slice());
    hasher.update(&definition.gguf_sha256);
    hasher.update(&definition.gguf_size.to_le_bytes());
    hasher.update(definition.tokenizer_id.as_byte_slice());
    hasher.update(definition.architecture_id.as_byte_slice());
    hasher.update(&definition.total_parameter_count.to_le_bytes());
    hasher.update(&definition.active_parameter_count.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

impl ModelDefinitionV1 {
    /// The record's identity: canonical Borsh (signature included — the id names the signed
    /// record) under the definition domain. Ids move, edits do not exist.
    pub fn definition_id(&self) -> Hash64 {
        let bytes = borsh::to_vec(self).expect("borsh of an owned definition cannot fail");
        keyed_hash64(PALW_ROUTING_DOMAIN_MODEL_DEFINITION_ID, &[&bytes])
    }

    pub fn validate(&self) -> Result<(), PalwRoutingError> {
        if self.version != PALW_ROUTING_OBJECT_VERSION_V1 {
            return Err(PalwRoutingError::UnsupportedVersion { got: self.version, expected: PALW_ROUTING_OBJECT_VERSION_V1 });
        }
        if self.gguf_size == 0 {
            return Err(PalwRoutingError::NotCanonical("gguf size is zero — not an artifact"));
        }
        if self.total_parameter_count == 0 {
            return Err(PalwRoutingError::NotCanonical("total parameter count is zero — not a model"));
        }
        if self.active_parameter_count == 0 || self.active_parameter_count > self.total_parameter_count {
            return Err(PalwRoutingError::NotCanonical("active parameters must be in 1..=total (MoE honesty)"));
        }
        if self.publisher_signature.is_empty() || self.publisher_signature.len() > PALW_S_MAX_SIGNATURE_BYTES {
            return Err(PalwRoutingError::NotCanonical("publisher signature is empty or exceeds the cap"));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// §1 — the family-version manifest (a generation's admitted runtime lineage)
// ---------------------------------------------------------------------------------------------

/// One coordinated runtime generation inside a family: the `/vN` tag segment made a record,
/// enumerating exactly which runtime manifests the generation admits. Records are never
/// overwritten (the profile rule): **retiring or re-capping a generation is publishing a
/// FURTHER record for the same `(family, version)`**, and the set-level readers below take
/// the most restrictive view across all of them — the minimum retirement epoch and the
/// minimum band cap win, so a published restriction can never be shadowed by an older record.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwExecutionFamilyManifestV1 {
    /// = [`PALW_ROUTING_OBJECT_VERSION_V1`].
    pub version: u16,
    pub execution_family: PalwExecutionFamilyV1,
    /// The generation number (the tag's `/vN`); zero is not a generation.
    pub family_version: u16,
    /// Admitted `runtime_manifest_hash`es — strictly ascending (one canonical encoding per
    /// set; an unordered list would give one lineage many ids).
    pub admitted_runtime_manifests: Vec<Hash64>,
    /// Root of the generation's golden set (the per-class signed-bundle discipline, indexed).
    pub golden_set_root: Hash64,
    /// The generation's activation cap on bands — the registration-observable fact behind
    /// ADR-0034 §4's "CPU max active band = B1". Seed it from
    /// [`initial_family_max_active_band_v1`]; raising it is a new record, never a code edit.
    pub max_active_band: PalwModelBandV1,
    pub activation_epoch: u64,
    /// `None` while current; `Some(e)` retires the generation at epoch `e` (exclusive).
    pub retirement_epoch: Option<u64>,
}

impl PalwExecutionFamilyManifestV1 {
    pub fn manifest_id(&self) -> Hash64 {
        let bytes = borsh::to_vec(self).expect("borsh of an owned manifest cannot fail");
        keyed_hash64(PALW_ROUTING_DOMAIN_FAMILY_MANIFEST_ID, &[&bytes])
    }

    pub fn validate(&self) -> Result<(), PalwRoutingError> {
        if self.version != PALW_ROUTING_OBJECT_VERSION_V1 {
            return Err(PalwRoutingError::UnsupportedVersion { got: self.version, expected: PALW_ROUTING_OBJECT_VERSION_V1 });
        }
        if self.family_version == 0 {
            return Err(PalwRoutingError::NotCanonical("family version zero is not a generation"));
        }
        if self.admitted_runtime_manifests.is_empty() || self.admitted_runtime_manifests.len() > PALW_ROUTING_MAX_ADMITTED_MANIFESTS {
            return Err(PalwRoutingError::NotCanonical("admitted manifest lineage is empty or exceeds the cap"));
        }
        if !self.admitted_runtime_manifests.windows(2).all(|w| w[0] < w[1]) {
            return Err(PalwRoutingError::NotCanonical("admitted manifests are not strictly ascending"));
        }
        if self.retirement_epoch.is_some_and(|r| r <= self.activation_epoch) {
            return Err(PalwRoutingError::NotCanonical("retirement does not follow activation"));
        }
        Ok(())
    }

    /// Whether this single record, read alone, is active at `epoch`. Set-level activity is
    /// [`family_version_active_in_set_v1`] — the most restrictive record of a generation wins.
    pub fn active_at(&self, epoch: u64) -> bool {
        self.activation_epoch <= epoch && self.retirement_epoch.is_none_or(|r| epoch < r)
    }
}

/// The most-restrictive-record view of one generation inside a manifest set: earliest
/// activation, **minimum** retirement epoch across all its records (a retirement, once
/// published, cannot be shadowed by the original record — records are never overwritten, so
/// retiring IS publishing a further record), and **minimum** band cap. `None` if the set
/// holds no record of the generation.
fn generation_view(
    manifests: &[PalwExecutionFamilyManifestV1],
    family: PalwExecutionFamilyV1,
    family_version: u16,
) -> Option<(u64, Option<u64>, PalwModelBandV1)> {
    let mut view: Option<(u64, Option<u64>, PalwModelBandV1)> = None;
    for m in manifests.iter().filter(|m| m.execution_family == family && m.family_version == family_version) {
        view = Some(match view {
            None => (m.activation_epoch, m.retirement_epoch, m.max_active_band),
            Some((activation, retirement, cap)) => (
                activation.min(m.activation_epoch),
                match (retirement, m.retirement_epoch) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                },
                cap.min(m.max_active_band),
            ),
        });
    }
    view
}

/// Whether generation `(family, family_version)` is active at `epoch` under the
/// most-restrictive-record view of `manifests`.
pub fn family_version_active_in_set_v1(
    manifests: &[PalwExecutionFamilyManifestV1],
    family: PalwExecutionFamilyV1,
    family_version: u16,
    epoch: u64,
) -> bool {
    match generation_view(manifests, family, family_version) {
        Some((activation, retirement, _)) => activation <= epoch && retirement.is_none_or(|r| epoch < r),
        None => false,
    }
}

/// The §1 set rule over manifests: no duplicate record ids (true re-publication), and at most
/// [`PALW_ROUTING_MAX_ACTIVE_FAMILY_VERSIONS`] active generations per family at `epoch`
/// (`current`, `previous`). Generations are counted under the most-restrictive-record view,
/// so a generation's history (original + retiring record) is one generation, not a duplicate.
pub fn family_versions_ok_v1(manifests: &[PalwExecutionFamilyManifestV1], epoch: u64) -> bool {
    let mut record_ids = std::collections::HashSet::new();
    let mut generations = std::collections::HashSet::new();
    for m in manifests {
        if !record_ids.insert(m.manifest_id()) {
            return false;
        }
        generations.insert((m.execution_family, m.family_version));
    }
    let mut active_per_family = std::collections::HashMap::new();
    for (family, version) in generations {
        if family_version_active_in_set_v1(manifests, family, version, epoch) {
            let count = active_per_family.entry(family).or_insert(0usize);
            *count += 1;
            if *count > PALW_ROUTING_MAX_ACTIVE_FAMILY_VERSIONS {
                return false;
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------------------------
// §6 — the ready-set Merkle commitment
// ---------------------------------------------------------------------------------------------

fn ready_leaf_hash(binding_id: &Hash64) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_ROUTING_DOMAIN_READY_LEAF).to_state();
    h.update(binding_id.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn ready_node_hash(left: &Hash64, right: &Hash64) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_ROUTING_DOMAIN_READY_NODE).to_state();
    h.update(left.as_byte_slice());
    h.update(right.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn ready_root_finalize(leaf_count: u32, top: &Hash64) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_ROUTING_DOMAIN_READY_ROOT).to_state();
    h.update(&leaf_count.to_le_bytes());
    h.update(top.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The canonical-set predicate both the root builder and the proof builder gate on — one
/// body, so the two cannot drift: strictly ascending (sorted, unique), non-empty, within the
/// cap. Hash-free by design.
fn ready_set_is_canonical(sorted_binding_ids: &[Hash64]) -> bool {
    !sorted_binding_ids.is_empty()
        && sorted_binding_ids.len() <= PALW_ROUTING_MAX_READY_BINDINGS as usize
        && sorted_binding_ids.windows(2).all(|w| w[0] < w[1])
}

/// Root of a verifier's ready set. The input must satisfy [`ready_set_is_canonical`] — one
/// set, one encoding, one root; anything else is `None`, not a best-effort hash. Odd levels
/// promote their last node unhashed; the final step commits the leaf count under its own
/// domain, so tree geometry is pinned by the root itself.
pub fn ready_binding_root_v1(sorted_binding_ids: &[Hash64]) -> Option<Hash64> {
    if !ready_set_is_canonical(sorted_binding_ids) {
        return None;
    }
    let mut level: Vec<Hash64> = sorted_binding_ids.iter().map(ready_leaf_hash).collect();
    while level.len() > 1 {
        level = level.chunks(2).map(|pair| if pair.len() == 2 { ready_node_hash(&pair[0], &pair[1]) } else { pair[0] }).collect();
    }
    Some(ready_root_finalize(sorted_binding_ids.len() as u32, &level[0]))
}

/// Membership proof for one binding in a ready set: the claimed tree geometry plus the
/// hashing-level siblings (promoted levels consume none).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwReadyBindingProofV1 {
    pub leaf_count: u32,
    pub leaf_index: u32,
    pub siblings: Vec<Hash64>,
}

/// Builds the proof for `sorted_binding_ids[index]` — the verifier-agent side of
/// [`verify_ready_binding_v1`], kept beside it so the two walk the same tree by construction.
pub fn ready_binding_proof_v1(sorted_binding_ids: &[Hash64], index: usize) -> Option<PalwReadyBindingProofV1> {
    if index >= sorted_binding_ids.len() || !ready_set_is_canonical(sorted_binding_ids) {
        return None;
    }
    let mut siblings = Vec::new();
    let mut level: Vec<Hash64> = sorted_binding_ids.iter().map(ready_leaf_hash).collect();
    let mut idx = index;
    while level.len() > 1 {
        if idx.is_multiple_of(2) {
            if idx + 1 < level.len() {
                siblings.push(level[idx + 1]);
            }
        } else {
            siblings.push(level[idx - 1]);
        }
        level = level.chunks(2).map(|pair| if pair.len() == 2 { ready_node_hash(&pair[0], &pair[1]) } else { pair[0] }).collect();
        idx /= 2;
    }
    Some(PalwReadyBindingProofV1 { leaf_count: sorted_binding_ids.len() as u32, leaf_index: index as u32, siblings })
}

/// Verifies that `binding_id` is a member of the ready set committed by `root`. Rejects
/// malformed geometry (index past count, sibling surplus or deficit) before accepting any
/// hash equality — a ready claim without a valid proof is not a claim.
pub fn verify_ready_binding_v1(root: &Hash64, binding_id: &Hash64, proof: &PalwReadyBindingProofV1) -> bool {
    if proof.leaf_count == 0
        || proof.leaf_count > PALW_ROUTING_MAX_READY_BINDINGS
        || proof.leaf_index >= proof.leaf_count
        || proof.siblings.len() > PALW_ROUTING_MAX_READY_SIBLINGS
    {
        return false;
    }
    let mut h = ready_leaf_hash(binding_id);
    let mut idx = proof.leaf_index;
    let mut width = proof.leaf_count;
    let mut siblings = proof.siblings.iter();
    while width > 1 {
        if idx.is_multiple_of(2) && idx + 1 == width {
            // Last node of an odd level: promoted, no sibling consumed.
        } else {
            let Some(sibling) = siblings.next() else {
                return false;
            };
            h = if idx.is_multiple_of(2) { ready_node_hash(&h, sibling) } else { ready_node_hash(sibling, &h) };
        }
        idx /= 2;
        width = width.div_ceil(2);
    }
    if siblings.next().is_some() {
        return false;
    }
    ready_root_finalize(proof.leaf_count, &h) == *root
}

// ---------------------------------------------------------------------------------------------
// §6 — the verifier capability
// ---------------------------------------------------------------------------------------------

/// Two claims with different verification costs, both required: hardware capability (cheap to
/// declare and index) and the Merkle-committed ready set (artifact held, runtime boots,
/// goldens passed, bench measured — the parts band can never substitute for). A capability is
/// a signed, TTL'd, nonce-monotonic statement; an expired one is simply not eligible —
/// silence is never an offense at this layer.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwVerifierCapabilityV1 {
    /// = [`PALW_ROUTING_OBJECT_VERSION_V1`].
    pub version: u16,
    pub verifier_id: Hash64,
    pub execution_family: PalwExecutionFamilyV1,
    pub family_version: u16,
    pub max_model_band: PalwModelBandV1,
    /// [`ready_binding_root_v1`] over the verifier's ready binding ids.
    pub ready_binding_root: Hash64,
    pub max_concurrency: u16,
    pub available_slots: u16,
    /// Advisory offer term: the longest replay this verifier WANTS. Deliberately not an
    /// eligibility conjunct — ADR-0034 §7's predicate is closed; the agent enforces its own
    /// offer terms by managing slots/TTL, and a duty drawn past them lapses into escalation
    /// rather than silently reshaping panel composition.
    pub max_accepted_replay_secs: u32,
    /// Advisory offer term, same rule: smallest reward this verifier accepts duty for
    /// (sompi); zero = any. Not an eligibility conjunct.
    pub minimum_reward: u64,
    /// The bond-UTXO discipline (ADR-0016 lineage): where the replay bond lives.
    pub replay_bond_outpoint: TransactionOutpoint,
    pub available_bond: u64,
    pub availability_expiry_daa: u64,
    /// Strictly increasing per verifier; a replacement capability must supersede.
    pub capability_nonce: u64,
    /// ML-DSA-87 over [`verifier_capability_message_v1`]; key resolution is the caller's.
    pub signature: Vec<u8>,
}

/// Keyed-BLAKE2b-256 signing message of a capability — network-bound, signature excluded,
/// fixed-width fields in struct order after the length-prefixed network id.
pub fn verifier_capability_message_v1(network_id: &[u8], capability: &PalwVerifierCapabilityV1) -> Hash {
    let mut hasher = blake2b_simd::Params::new().hash_length(32).key(PALW_ROUTING_DOMAIN_CAPABILITY_MESSAGE).to_state();
    hasher.update(&(network_id.len() as u32).to_le_bytes());
    hasher.update(network_id);
    hasher.update(&capability.version.to_le_bytes());
    hasher.update(capability.verifier_id.as_byte_slice());
    hasher.update(&[capability.execution_family as u8]);
    hasher.update(&capability.family_version.to_le_bytes());
    hasher.update(&[capability.max_model_band as u8]);
    hasher.update(capability.ready_binding_root.as_byte_slice());
    hasher.update(&capability.max_concurrency.to_le_bytes());
    hasher.update(&capability.available_slots.to_le_bytes());
    hasher.update(&capability.max_accepted_replay_secs.to_le_bytes());
    hasher.update(&capability.minimum_reward.to_le_bytes());
    hasher.update(capability.replay_bond_outpoint.transaction_id.as_byte_slice());
    hasher.update(&capability.replay_bond_outpoint.index.to_le_bytes());
    hasher.update(&capability.available_bond.to_le_bytes());
    hasher.update(&capability.availability_expiry_daa.to_le_bytes());
    hasher.update(&capability.capability_nonce.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

impl PalwVerifierCapabilityV1 {
    /// The record's identity (signature included — the id names the signed statement).
    pub fn capability_id(&self) -> Hash64 {
        let bytes = borsh::to_vec(self).expect("borsh of an owned capability cannot fail");
        keyed_hash64(PALW_ROUTING_DOMAIN_CAPABILITY_ID, &[&bytes])
    }

    pub fn validate(&self) -> Result<(), PalwRoutingError> {
        if self.version != PALW_ROUTING_OBJECT_VERSION_V1 {
            return Err(PalwRoutingError::UnsupportedVersion { got: self.version, expected: PALW_ROUTING_OBJECT_VERSION_V1 });
        }
        if self.family_version == 0 {
            return Err(PalwRoutingError::NotCanonical("family version zero is not a generation"));
        }
        if self.max_concurrency == 0 {
            return Err(PalwRoutingError::NotCanonical("zero concurrency can hold no duty"));
        }
        if self.available_slots > self.max_concurrency {
            return Err(PalwRoutingError::NotCanonical("available slots exceed declared concurrency"));
        }
        if self.max_accepted_replay_secs == 0 {
            return Err(PalwRoutingError::NotCanonical("a zero replay-time acceptance is not an offer"));
        }
        if self.signature.is_empty() || self.signature.len() > PALW_S_MAX_SIGNATURE_BYTES {
            return Err(PalwRoutingError::NotCanonical("capability signature is empty or exceeds the cap"));
        }
        Ok(())
    }

    /// TTL: live means eligible-in-time; an expired capability is not an offense, just absent.
    pub fn live_at(&self, now_daa: u64) -> bool {
        self.availability_expiry_daa > now_daa
    }

    /// Nonce monotonicity: `new` replaces `old` only for the same verifier with a strictly
    /// greater nonce (replay of a stale capability can never displace a fresh one).
    pub fn supersedes(&self, old: &Self) -> bool {
        self.verifier_id == old.verifier_id && self.capability_nonce > old.capability_nonce
    }
}

// ---------------------------------------------------------------------------------------------
// §7 — binding-aware eligibility and the routed panel
// ---------------------------------------------------------------------------------------------

/// One candidate for a routed panel: the capability facts plus the per-job ready proof. As
/// with `PalwPanelCandidateV1`, the CALLER is responsible for the fields being the chain's
/// truth at the anchor (capability signature verified, bond looked up, reputation floor
/// evaluated) — but the eligibility RULE, including the Merkle check, lives in the function,
/// because a duty must be derivable identically by every observer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwRoutedCandidateV1 {
    pub verifier_id: Hash64,
    /// Operator aggregation counts once (v0.1): candidates sharing the executor's control
    /// domain are excluded with the executor itself.
    pub control_domain_id: Hash64,
    pub execution_family: PalwExecutionFamilyV1,
    pub family_version: u16,
    pub max_model_band: PalwModelBandV1,
    pub ready_binding_root: Hash64,
    /// Proof that the job's binding is in this verifier's ready set — the part band can never
    /// substitute for.
    pub ready_proof: PalwReadyBindingProofV1,
    pub available_slots: u16,
    pub available_bond: u64,
    pub availability_expiry_daa: u64,
    /// `reputation(v) ≥ floor`, evaluated by the caller against its chain view.
    pub reputation_ok: bool,
    pub frozen: bool,
}

/// ADR-0034 §7's full conjunction. Band alone never selects — the named anti-pattern
/// (`Receipt = B4, Verifier max = B4 → assign`) is untypable here because the ready proof
/// and every other conjunct are checked in the same breath. The Merkle proof is evaluated
/// LAST: `&&` short-circuits, every other conjunct is O(1), and a candidate disqualified by
/// a scalar fact should not cost a hash chain first (the predicate is pure, so ordering is
/// unobservable in the result).
pub fn routed_candidate_eligible_v1(
    candidate: &PalwRoutedCandidateV1,
    binding: &PalwClassRegistrationV1,
    binding_id: &Hash64,
    executor_id: &Hash64,
    executor_control_domain: &Hash64,
    required_replay_bond: u64,
    now_daa: u64,
) -> bool {
    candidate.execution_family == binding.execution_family
        && candidate.family_version == binding.family_version
        && candidate.max_model_band >= binding.model_band
        && candidate.available_slots > 0
        && candidate.available_bond >= required_replay_bond
        && candidate.availability_expiry_daa > now_daa
        && candidate.reputation_ok
        && candidate.verifier_id != *executor_id
        && candidate.control_domain_id != *executor_control_domain
        && !candidate.frozen
        && verify_ready_binding_v1(&candidate.ready_binding_root, binding_id, &candidate.ready_proof)
}

/// Escalation widening (ADR-0034 §7): round 0 is the panel; each lapse re-draws wider.
/// Linear widening is a **placeholder** pending simulation.
pub fn escalated_panel_width_v1(base_q: u16, escalation_round: u32) -> usize {
    (base_q as usize).saturating_mul((escalation_round as usize).saturating_add(1))
}

/// The routed duty lottery: `select_replay_panel_v1`'s construction under this module's own
/// domain key, with §7 eligibility, the escalation round AND the binding id folded into the
/// ticket (the twins key their tickets by job alone because their candidates come from one
/// consensus-registered set; routed candidates are self-published capabilities, and a
/// commitment root reused across two bindings must not produce correlated draws). The
/// binding's `windows.q` is the funded panel size (`q ≥ 2` on the crediting path is the
/// windows' own validation); `binding_id` is recomputed from the row here, never accepted
/// from a caller — the two can not disagree.
///
/// **One seat per verifier, one seat per control domain**: routed candidates are
/// self-published records, so without deduplication one operator could occupy the whole
/// panel with duplicate entries (or sibling identities in one control domain) and collapse
/// the funded redundancy to a single machine — the §10 counts-once rule, applied to seats.
/// A verifier id appearing more than once in `candidates` is dropped entirely (fail-closed:
/// the caller failed nonce supersession, and picking one of two conflicting records would
/// make the panel depend on input order); among distinct verifiers sharing a control domain,
/// the lowest ticket holds the seat — ticket order, so still input-order-invariant.
///
/// Deterministic in every input and invariant under candidate order. An empty eligible set
/// yields an empty panel, never a shrunk quorum. Stage-0 note: `decide_credit_v1` does NOT
/// read this panel yet — it derives the ADR-0028 class panel until the Stage-1 wiring
/// substitutes this one (ADR-0028 §2 as amended by ADR-0034 §7); until that wiring lands,
/// the routed exclusions gate scheduling telemetry only, and no credit path consumes them.
pub fn select_routed_replay_panel_v1(
    commitment_root: &Hash64,
    executor_id: &Hash64,
    executor_control_domain: &Hash64,
    anchor: &Hash64,
    binding: &PalwClassRegistrationV1,
    required_replay_bond: u64,
    now_daa: u64,
    candidates: &[PalwRoutedCandidateV1],
    escalation_round: u32,
) -> Vec<Hash64> {
    let q = escalated_panel_width_v1(binding.windows.q, escalation_round);
    if q == 0 || candidates.is_empty() {
        return Vec::new();
    }
    let binding_id = binding.registration_id();
    let mut occurrences = std::collections::HashMap::new();
    for c in candidates {
        *occurrences.entry(c.verifier_id).or_insert(0u32) += 1;
    }
    let mut ticketed: Vec<(Hash64, Hash64, Hash64)> = candidates
        .iter()
        .filter(|c| occurrences[&c.verifier_id] == 1)
        .filter(|c| {
            routed_candidate_eligible_v1(c, binding, &binding_id, executor_id, executor_control_domain, required_replay_bond, now_daa)
        })
        .map(|c| {
            let ticket = keyed_hash64(
                PALW_ROUTING_DOMAIN_ASSIGNMENT_TICKET,
                &[
                    commitment_root.as_byte_slice(),
                    executor_id.as_byte_slice(),
                    anchor.as_byte_slice(),
                    &escalation_round.to_le_bytes(),
                    binding_id.as_byte_slice(),
                    c.verifier_id.as_byte_slice(),
                ],
            );
            (ticket, c.verifier_id, c.control_domain_id)
        })
        .collect();
    ticketed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut seen_domains = std::collections::HashSet::new();
    let mut panel = Vec::with_capacity(q.min(ticketed.len()));
    for (_, verifier_id, control_domain_id) in ticketed {
        if panel.len() == q {
            break;
        }
        if seen_domains.insert(control_domain_id) {
            panel.push(verifier_id);
        }
    }
    panel
}

// ---------------------------------------------------------------------------------------------
// §5 — carried routing keys against the registry (the miner's claim is checked, never believed)
// ---------------------------------------------------------------------------------------------

/// The routing slice of ADR-0034 §5's acceptance checks: the resolved registry row must BE
/// the carried binding (recomputed id — the lookup-collision guard), the carried family and
/// band must equal the registered ones (band forgery = invalidity), and the binding must be
/// in a receipt-accepting coverage state. The envelope-equality checks (`model_profile_id`,
/// `runtime_manifest_hash`, …) stay with the carriage validation that already owns them.
pub fn validate_receipt_routing_keys_v1(
    carried_binding_id: &Hash64,
    carried_family: PalwExecutionFamilyV1,
    carried_band: PalwModelBandV1,
    binding: &PalwClassRegistrationV1,
    coverage: PalwBindingCoverageStateV1,
) -> Result<(), PalwRoutingError> {
    if binding.registration_id() != *carried_binding_id {
        return Err(PalwRoutingError::BindingIdMismatch);
    }
    if binding.execution_family != carried_family {
        return Err(PalwRoutingError::FamilyMismatch { carried: carried_family, registered: binding.execution_family });
    }
    if binding.model_band != carried_band {
        return Err(PalwRoutingError::BandForged { carried: carried_band, registered: binding.model_band });
    }
    match coverage {
        PalwBindingCoverageStateV1::Active
        | PalwBindingCoverageStateV1::LowCoverage
        | PalwBindingCoverageStateV1::Throttled
        | PalwBindingCoverageStateV1::Deprecated => Ok(()),
        PalwBindingCoverageStateV1::Frozen | PalwBindingCoverageStateV1::Retired | PalwBindingCoverageStateV1::ContradictionFreeze => {
            Err(PalwRoutingError::BindingNotAccepting(coverage))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// §10 — coverage: a binding nobody can replay does not get to exist quietly
// ---------------------------------------------------------------------------------------------

/// The coverage ladder. `Throttled` still accepts receipts (admission is rate-limited by the
/// caller); `Frozen`, `Retired` and `ContradictionFreeze` do not. Registry-side, a
/// contradiction freeze is `to_zero_credit()` — the ceiling IS the switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwBindingCoverageStateV1 {
    Active = 0,
    LowCoverage = 1,
    Throttled = 2,
    Frozen = 3,
    Deprecated = 4,
    Retired = 5,
    ContradictionFreeze = 6,
}

/// The on-chain-observable facts one coverage evaluation reads. None is a governance vote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCoverageFactsV1 {
    /// Independent ready re-executors (same control domain counts once).
    pub ready_independent_count: u32,
    /// The activation threshold in force ([`PALW_ROUTING_MIN_READY_DEVNET`] /
    /// [`PALW_ROUTING_MIN_READY_MAINNET`]). Zero is not a threshold: the walk freezes on it
    /// (fail-closed), the same refusal `binding_may_activate_v1` gives it.
    pub min_ready: u32,
    /// Consecutive epochs the ready count has been below `min_ready` — updated ONLY via
    /// [`next_epochs_below_min_v1`] each epoch, so every observer counts identically; a
    /// privately invented increment/reset rule would let two observers classify the same
    /// chain history into different coverage states.
    pub epochs_below_min: u32,
    /// A `ClassContradictionCertificateV1` names this binding's class.
    pub contradiction_observed: bool,
    pub deprecation_declared: bool,
    pub retirement_epoch_reached: bool,
}

/// The one counting rule behind [`PalwCoverageFactsV1::epochs_below_min`]: consecutive means
/// consecutive — an epoch at or above the threshold resets to zero, an epoch below it adds
/// one (saturating). Kept beside the classifier so the temporal semantics are this module's,
/// not each caller's.
pub fn next_epochs_below_min_v1(previous_epochs_below: u32, ready_independent_count: u32, min_ready: u32) -> u32 {
    if ready_independent_count >= min_ready && min_ready > 0 { 0 } else { previous_epochs_below.saturating_add(1) }
}

/// The §10 walk as a pure function of observable facts, most severe first. A contradiction
/// outranks everything (bonds frozen, not released, no slash — ADR-0027 §5). Starvation
/// outranks deprecation: zero ready re-executors — or a zero threshold, which is no
/// threshold at all — is an immediate freeze REGARDLESS of any planned retirement, because
/// assigning duties nobody can serve is exactly what the ladder exists to prevent, and
/// `Deprecated` still accepts receipts.
pub fn coverage_state_v1(facts: &PalwCoverageFactsV1) -> PalwBindingCoverageStateV1 {
    if facts.contradiction_observed {
        return PalwBindingCoverageStateV1::ContradictionFreeze;
    }
    if facts.retirement_epoch_reached {
        return PalwBindingCoverageStateV1::Retired;
    }
    if facts.min_ready == 0 || facts.ready_independent_count == 0 {
        return PalwBindingCoverageStateV1::Frozen;
    }
    if facts.ready_independent_count < facts.min_ready && facts.epochs_below_min >= PALW_ROUTING_COVERAGE_FREEZE_EPOCHS {
        return PalwBindingCoverageStateV1::Frozen;
    }
    if facts.deprecation_declared {
        return PalwBindingCoverageStateV1::Deprecated;
    }
    if facts.ready_independent_count >= facts.min_ready {
        return PalwBindingCoverageStateV1::Active;
    }
    if facts.epochs_below_min >= PALW_ROUTING_COVERAGE_THROTTLE_EPOCHS {
        return PalwBindingCoverageStateV1::Throttled;
    }
    PalwBindingCoverageStateV1::LowCoverage
}

/// The registration↔definition join (ADR-0034 §3: "model identity and execution identity
/// register separately and join in a binding"): same `model_profile_id`, and the binding's
/// self-measured artifact envelope must BE the definition's signed artifact size — the one
/// band dimension the system can mechanically pin (the artifact is `gguf_sha256`-exact) is
/// not left to the registrant's own number. Without this, a 9 GiB model registers a 1 GiB
/// `model_artifact_bytes` and lands in B0, dodging every band-indexed floor.
pub fn binding_matches_definition_v1(binding: &PalwClassRegistrationV1, definition: &ModelDefinitionV1) -> bool {
    binding.model_profile_id == definition.model_profile_id && binding.model_artifact_bytes == definition.gguf_size
}

/// The set rule a registry store MUST hold before serving lookups: no two rows share a
/// `registration_id` (true duplicates) and no two rows share a `runtime_class_id`. The
/// second is load-bearing across layers: the credit gate keys the class by
/// `runtime_class_id` while routing keys the binding by `registration_id()` — two rows
/// sharing a class id would let a receipt key-validate under one row and credit under the
/// other's windows and ceiling.
pub fn binding_rows_coherent_v1(rows: &[PalwClassRegistrationV1]) -> bool {
    let mut ids = std::collections::HashSet::new();
    let mut class_ids = std::collections::HashSet::new();
    rows.iter().all(|row| ids.insert(row.registration_id()) && class_ids.insert(row.runtime_class_id))
}

/// Binding activation (ADR-0034 §10): golden set passed on at least the minimum independent
/// re-executors, artifact retrievable, and — derived HERE from registered records, not from
/// caller booleans — the generation is active in the manifest set, the binding's runtime
/// manifest is one the generation admits, the declared band is within the generation's
/// registered cap, the signed model definition joins the row, and the family is not one of
/// the reserved ones (their first binding is the ADR-0027 conformance campaign, not an
/// activation). Fail-closed on a zero threshold — a requirement of zero is not a requirement
/// ("registered but operated on prayer" is the state this refuses). The window and ceiling
/// coherence is `validate()`'s, adjudication depth is a recorded field by construction; a
/// binding that cannot activate may still run SHADOW (uncredited).
pub fn binding_may_activate_v1(
    binding: &PalwClassRegistrationV1,
    definition: &ModelDefinitionV1,
    manifests: &[PalwExecutionFamilyManifestV1],
    epoch: u64,
    golden_passed_independent: u32,
    min_ready_independent: u32,
    artifact_retrievable: bool,
) -> bool {
    let Some((_, _, max_active_band)) = generation_view(manifests, binding.execution_family, binding.family_version) else {
        return false; // no manifest record: the generation does not exist, so nothing activates
    };
    min_ready_independent > 0
        && golden_passed_independent >= min_ready_independent
        && artifact_retrievable
        && !family_is_reserved_v1(binding.execution_family)
        && family_version_active_in_set_v1(manifests, binding.execution_family, binding.family_version, epoch)
        && manifests.iter().any(|m| {
            m.execution_family == binding.execution_family
                && m.family_version == binding.family_version
                && m.admitted_runtime_manifests.binary_search(&binding.runtime_manifest_hash).is_ok()
        })
        && binding.model_band <= max_active_band
        && binding_matches_definition_v1(binding, definition)
}

// =============================================================================================
// Tests — every line of ADR-0034 "Required tests" that names a routing object lands here or
// in `palw_adversarial`; the registry-side derivation lines land in `palw_registry`.
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_bisect::PALW_BISECT_ALL_DOMAINS;
    use crate::palw_carriage::PALW_CARRIAGE_ALL_DOMAINS;
    use crate::palw_legs::PALW_LEGS_ALL_DOMAINS;
    use crate::palw_reference::PALW_REFERENCE_ALL_DOMAINS;
    use crate::palw_registry::PALW_REGISTRY_ALL_DOMAINS;
    use crate::palw_schedule::{PALW_SCHEDULE_ALL_DOMAINS, PalwPanelCandidateV1, select_replay_panel_v1};
    use crate::palw_slash::PALW_S_ALL_DOMAINS;
    use crate::palw_step::PALW_STEP_ALL_DOMAINS;
    use crate::palw_step_leg::PALW_STEP_LEG_ALL_DOMAINS;
    use crate::palw_v2::PALW_V2_ALL_DOMAINS;
    use crate::vlt::VERIFIER_SORTITION_KEY;

    fn h64(fill: u8) -> Hash64 {
        Hash64::from_bytes([fill; 64])
    }

    /// The binding every routing test routes for: the registry's own fleet row (B0, Cpu/v1).
    fn binding() -> PalwClassRegistrationV1 {
        crate::palw_registry::tests::fleet_registration()
    }

    /// A three-leaf ready set containing `binding_id`, so proofs exercise real siblings.
    fn ready_set_with(binding_id: Hash64) -> (Hash64, PalwReadyBindingProofV1) {
        let mut ids = vec![h64(0x0A), binding_id, h64(0xFA)];
        ids.sort();
        ids.dedup();
        let index = ids.iter().position(|x| *x == binding_id).unwrap();
        let root = ready_binding_root_v1(&ids).unwrap();
        let proof = ready_binding_proof_v1(&ids, index).unwrap();
        (root, proof)
    }

    fn eligible_candidate(id: u8, binding_id: Hash64) -> PalwRoutedCandidateV1 {
        let (root, proof) = ready_set_with(binding_id);
        PalwRoutedCandidateV1 {
            verifier_id: h64(id),
            control_domain_id: h64(0xB0 ^ id),
            execution_family: PalwExecutionFamilyV1::Cpu,
            family_version: 1,
            max_model_band: PalwModelBandV1::B0,
            ready_binding_root: root,
            ready_proof: proof,
            available_slots: 1,
            available_bond: 1_000,
            availability_expiry_daa: 1_000_000,
            reputation_ok: true,
            frozen: false,
        }
    }

    const EXECUTOR: u8 = 0x2E;
    const EXECUTOR_DOMAIN: u8 = 0xE0;
    const NOW: u64 = 500_000;
    const BOND: u64 = 1_000;

    fn routed_panel(candidates: &[PalwRoutedCandidateV1], round: u32) -> Vec<Hash64> {
        select_routed_replay_panel_v1(
            &h64(0x01),
            &h64(EXECUTOR),
            &h64(EXECUTOR_DOMAIN),
            &h64(0x03),
            &binding(),
            BOND,
            NOW,
            candidates,
            round,
        )
    }

    // -----------------------------------------------------------------------------------------
    // Domains
    // -----------------------------------------------------------------------------------------

    #[test]
    fn routing_domains_are_unique_across_every_palw_family_and_the_vlt_sortition_key() {
        let mut all: Vec<&[u8]> = Vec::new();
        all.extend_from_slice(PALW_ROUTING_ALL_DOMAINS);
        all.extend_from_slice(PALW_SCHEDULE_ALL_DOMAINS);
        all.extend_from_slice(PALW_REGISTRY_ALL_DOMAINS);
        all.extend_from_slice(PALW_CARRIAGE_ALL_DOMAINS);
        all.extend_from_slice(PALW_BISECT_ALL_DOMAINS);
        all.extend_from_slice(PALW_LEGS_ALL_DOMAINS);
        all.extend_from_slice(PALW_V2_ALL_DOMAINS);
        all.extend_from_slice(PALW_S_ALL_DOMAINS);
        all.extend_from_slice(PALW_REFERENCE_ALL_DOMAINS);
        all.extend_from_slice(PALW_STEP_ALL_DOMAINS);
        all.extend_from_slice(PALW_STEP_LEG_ALL_DOMAINS);
        all.push(VERIFIER_SORTITION_KEY);
        let before = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), before, "a domain string is shared across families — a preimage bridge");
        for d in PALW_ROUTING_ALL_DOMAINS {
            assert!(d.len() <= 64, "keyed-BLAKE2b key cap");
        }
    }

    // -----------------------------------------------------------------------------------------
    // §4 band derivation
    // -----------------------------------------------------------------------------------------

    #[test]
    fn each_resource_dimension_alone_raises_the_band_and_boundaries_are_exact() {
        let b0 = (1, 1, 1, 1);
        let derive = |a: u64, m: u64, w: u64, p: u64| derived_model_band_v1(a, m, w, p);
        assert_eq!(derive(b0.0, b0.1, b0.2, b0.3), Some(PalwModelBandV1::B0));
        // Exactly the base is still the band; one past it is the next.
        assert_eq!(derive(PALW_ROUTING_BAND_ARTIFACT_BASE_BYTES, 1, 1, 1), Some(PalwModelBandV1::B0));
        assert_eq!(derive(PALW_ROUTING_BAND_ARTIFACT_BASE_BYTES + 1, 1, 1, 1), Some(PalwModelBandV1::B1));
        assert_eq!(derive(1, PALW_ROUTING_BAND_MEMORY_BASE_BYTES + 1, 1, 1), Some(PalwModelBandV1::B1));
        assert_eq!(derive(1, 1, PALW_ROUTING_BASE_REPLAY_WORK_MS + 1, 1), Some(PalwModelBandV1::B1));
        assert_eq!(derive(1, 1, 1, PALW_ROUTING_BAND_PROOF_BASE_BYTES + 1), Some(PalwModelBandV1::B1));
        // The max rule: the worst dimension is the band.
        assert_eq!(derive(PALW_ROUTING_BAND_ARTIFACT_BASE_BYTES * 16, 1, 1, 1), Some(PalwModelBandV1::B4), "16× exactly is B4");
        assert_eq!(derive(PALW_ROUTING_BAND_ARTIFACT_BASE_BYTES * 16 + 1, 1, 1, 1), None, "past 16× is not registrable");
        assert_eq!(derive(1, 1, u64::MAX, 1), None, "a saturated work claim fails closed");
    }

    #[test]
    fn bands_order_and_the_initial_family_caps_are_the_adr_values() {
        assert!(PalwModelBandV1::B0 < PalwModelBandV1::B4);
        assert_eq!(initial_family_max_active_band_v1(PalwExecutionFamilyV1::Cpu), PalwModelBandV1::B1, "ADR-0034 §4");
        assert_eq!(initial_family_max_active_band_v1(PalwExecutionFamilyV1::Cuda), PalwModelBandV1::B0, "reserved family");
    }

    // -----------------------------------------------------------------------------------------
    // §3 model definition
    // -----------------------------------------------------------------------------------------

    fn definition() -> ModelDefinitionV1 {
        ModelDefinitionV1 {
            version: PALW_ROUTING_OBJECT_VERSION_V1,
            model_profile_id: h64(0x03),
            gguf_sha256: [0xAA; 32],
            gguf_size: 1_280_835_840,
            tokenizer_id: h64(0x04),
            architecture_id: h64(0x05),
            total_parameter_count: 2_000_000_000,
            active_parameter_count: 2_000_000_000,
            publisher_signature: vec![0x55; 64],
        }
    }

    #[test]
    fn a_model_definition_validates_and_its_id_moves_with_every_field() {
        definition().validate().unwrap();
        let base = definition().definition_id();
        let mutate = |f: &dyn Fn(&mut ModelDefinitionV1)| {
            let mut d = definition();
            f(&mut d);
            d.definition_id()
        };
        assert_ne!(mutate(&|d| d.gguf_sha256 = [0xBB; 32]), base);
        assert_ne!(mutate(&|d| d.gguf_size = 1), base);
        assert_ne!(mutate(&|d| d.active_parameter_count = 1), base);
        assert_ne!(mutate(&|d| d.publisher_signature = vec![0x66; 64]), base);
        // The signing message binds the network and every identity field, minus the signature.
        let msg = model_definition_message_v1(b"misaka-devnet", &definition());
        assert_ne!(msg, model_definition_message_v1(b"misaka-testnet-11", &definition()));
        let mut resigned = definition();
        resigned.publisher_signature = vec![0x77; 64];
        assert_eq!(msg, model_definition_message_v1(b"misaka-devnet", &resigned), "the signature is not its own message");
        let mut other = definition();
        other.gguf_size += 1;
        assert_ne!(msg, model_definition_message_v1(b"misaka-devnet", &other));
    }

    #[test]
    fn moe_honesty_and_shape_caps_reject_at_validation() {
        let mut more_active_than_total = definition();
        more_active_than_total.active_parameter_count = more_active_than_total.total_parameter_count + 1;
        assert!(matches!(more_active_than_total.validate(), Err(PalwRoutingError::NotCanonical(_))));
        let mut unsigned = definition();
        unsigned.publisher_signature.clear();
        assert!(matches!(unsigned.validate(), Err(PalwRoutingError::NotCanonical(_))));
        let mut empty_artifact = definition();
        empty_artifact.gguf_size = 0;
        assert!(matches!(empty_artifact.validate(), Err(PalwRoutingError::NotCanonical(_))));
        let mut wrong_version = definition();
        wrong_version.version = 2;
        assert!(matches!(wrong_version.validate(), Err(PalwRoutingError::UnsupportedVersion { .. })));
    }

    // -----------------------------------------------------------------------------------------
    // §1 family-version manifests
    // -----------------------------------------------------------------------------------------

    fn manifest(
        family: PalwExecutionFamilyV1,
        version: u16,
        activation: u64,
        retirement: Option<u64>,
    ) -> PalwExecutionFamilyManifestV1 {
        PalwExecutionFamilyManifestV1 {
            version: PALW_ROUTING_OBJECT_VERSION_V1,
            execution_family: family,
            family_version: version,
            admitted_runtime_manifests: vec![h64(0x02), h64(0x10), h64(0x20)],
            golden_set_root: h64(0x30),
            max_active_band: initial_family_max_active_band_v1(family),
            activation_epoch: activation,
            retirement_epoch: retirement,
        }
    }

    #[test]
    fn a_family_manifest_validates_only_in_canonical_shape() {
        manifest(PalwExecutionFamilyV1::Cpu, 1, 0, None).validate().unwrap();
        let mut unsorted = manifest(PalwExecutionFamilyV1::Cpu, 1, 0, None);
        unsorted.admitted_runtime_manifests = vec![h64(0x20), h64(0x10)];
        assert!(matches!(unsorted.validate(), Err(PalwRoutingError::NotCanonical(_))));
        let mut duplicated = manifest(PalwExecutionFamilyV1::Cpu, 1, 0, None);
        duplicated.admitted_runtime_manifests = vec![h64(0x10), h64(0x10)];
        assert!(matches!(duplicated.validate(), Err(PalwRoutingError::NotCanonical(_))));
        let mut zero_generation = manifest(PalwExecutionFamilyV1::Cpu, 0, 0, None);
        assert!(matches!(zero_generation.validate(), Err(PalwRoutingError::NotCanonical(_))));
        zero_generation.family_version = 1;
        zero_generation.retirement_epoch = Some(0);
        assert!(matches!(zero_generation.validate(), Err(PalwRoutingError::NotCanonical(_))), "retirement must follow activation");
    }

    #[test]
    fn at_most_two_versions_of_one_family_are_active_at_once() {
        let current = manifest(PalwExecutionFamilyV1::Cpu, 2, 100, None);
        let previous = manifest(PalwExecutionFamilyV1::Cpu, 1, 0, Some(200));
        let ancient = manifest(PalwExecutionFamilyV1::Cpu, 3, 300, None); // not yet active at 150
        let metal = manifest(PalwExecutionFamilyV1::Metal, 1, 0, None);
        assert!(family_versions_ok_v1(&[current.clone(), previous.clone(), ancient.clone(), metal.clone()], 150));
        // A third concurrently active generation of ONE family is refused…
        let third = manifest(PalwExecutionFamilyV1::Cpu, 3, 120, None);
        assert!(!family_versions_ok_v1(&[current.clone(), previous.clone(), third], 150));
        // …a byte-identical re-publication is refused as a true duplicate…
        assert!(!family_versions_ok_v1(&[current.clone(), current.clone(), previous.clone()], 150));
        assert!(manifest(PalwExecutionFamilyV1::Cpu, 1, 0, Some(200)).active_at(199));
        assert!(!manifest(PalwExecutionFamilyV1::Cpu, 1, 0, Some(200)).active_at(200), "retirement epoch is exclusive");

        // …but a generation's HISTORY is one generation, not a duplicate: records are never
        // overwritten, so retiring (Cpu, v1) is publishing a further record with a
        // retirement epoch, and the most restrictive record wins in the set view.
        let v1_original = manifest(PalwExecutionFamilyV1::Cpu, 1, 0, None);
        let v1_retiring = manifest(PalwExecutionFamilyV1::Cpu, 1, 0, Some(200));
        let history = [v1_original.clone(), v1_retiring, current.clone()];
        assert!(family_versions_ok_v1(&history, 150), "a retiring record beside its original is not a duplicate");
        assert!(family_version_active_in_set_v1(&history, PalwExecutionFamilyV1::Cpu, 1, 150));
        assert!(
            !family_version_active_in_set_v1(&history, PalwExecutionFamilyV1::Cpu, 1, 200),
            "the published retirement cannot be shadowed by the original record"
        );
        assert!(
            !family_version_active_in_set_v1(&history, PalwExecutionFamilyV1::Cpu, 9, 150),
            "an unregistered generation is not active"
        );
    }

    // -----------------------------------------------------------------------------------------
    // §6 ready-set Merkle
    // -----------------------------------------------------------------------------------------

    #[test]
    fn ready_roots_demand_a_canonical_set_and_proofs_round_trip_at_every_size() {
        assert!(ready_binding_root_v1(&[]).is_none(), "an empty ready set has no root");
        assert!(ready_binding_root_v1(&[h64(2), h64(1)]).is_none(), "unsorted is not canonical");
        assert!(ready_binding_root_v1(&[h64(1), h64(1)]).is_none(), "duplicates are not a set");
        for n in 1usize..=9 {
            let ids: Vec<Hash64> = (1..=n as u8).map(h64).collect();
            let root = ready_binding_root_v1(&ids).unwrap();
            for (i, id) in ids.iter().enumerate() {
                let proof = ready_binding_proof_v1(&ids, i).unwrap();
                assert!(verify_ready_binding_v1(&root, id, &proof), "leaf {i} of {n} fails its own proof");
                assert!(!verify_ready_binding_v1(&root, &h64(0xEE), &proof), "a foreign binding rides leaf {i}'s proof");
            }
        }
    }

    #[test]
    fn forged_geometry_and_grafted_nodes_are_refused() {
        let ids: Vec<Hash64> = (1..=5u8).map(h64).collect();
        let root = ready_binding_root_v1(&ids).unwrap();
        let proof = ready_binding_proof_v1(&ids, 2).unwrap();
        // Claiming a different tree size cannot reuse the same root: the count is committed.
        let mut miscount = proof.clone();
        miscount.leaf_count = 6;
        assert!(!verify_ready_binding_v1(&root, &ids[2], &miscount));
        // Sibling surplus and deficit are malformed before any equality is considered.
        let mut surplus = proof.clone();
        surplus.siblings.push(h64(0x77));
        assert!(!verify_ready_binding_v1(&root, &ids[2], &surplus));
        let mut deficit = proof.clone();
        deficit.siblings.pop();
        assert!(!verify_ready_binding_v1(&root, &ids[2], &deficit));
        // An index past the committed count is not a leaf.
        let mut out_of_range = proof.clone();
        out_of_range.leaf_index = 5;
        assert!(!verify_ready_binding_v1(&root, &ids[2], &out_of_range));
        // Leaf/node domain separation: an internal node presented as a two-leaf tree's leaf
        // does not reproduce the four-leaf root (the classic second-preimage graft).
        let four: Vec<Hash64> = (1..=4u8).map(h64).collect();
        let four_root = ready_binding_root_v1(&four).unwrap();
        let internal = ready_node_hash(&ready_leaf_hash(&four[0]), &ready_leaf_hash(&four[1]));
        let graft = PalwReadyBindingProofV1 {
            leaf_count: 2,
            leaf_index: 0,
            siblings: vec![ready_node_hash(&ready_leaf_hash(&four[2]), &ready_leaf_hash(&four[3]))],
        };
        assert!(!verify_ready_binding_v1(&four_root, &internal, &graft), "an internal node was accepted as a leaf");
    }

    // -----------------------------------------------------------------------------------------
    // §6 capability
    // -----------------------------------------------------------------------------------------

    fn capability() -> PalwVerifierCapabilityV1 {
        PalwVerifierCapabilityV1 {
            version: PALW_ROUTING_OBJECT_VERSION_V1,
            verifier_id: h64(0x01),
            execution_family: PalwExecutionFamilyV1::Cpu,
            family_version: 1,
            max_model_band: PalwModelBandV1::B0,
            ready_binding_root: h64(0x02),
            max_concurrency: 4,
            available_slots: 2,
            max_accepted_replay_secs: 3_600,
            minimum_reward: 0,
            replay_bond_outpoint: TransactionOutpoint::new(h64(0x03), 0),
            available_bond: 20_000,
            availability_expiry_daa: 1_000,
            capability_nonce: 7,
            signature: vec![0x44; 64],
        }
    }

    #[test]
    fn a_capability_validates_expires_and_supersedes_by_nonce_alone() {
        let cap = capability();
        cap.validate().unwrap();
        assert!(cap.live_at(999) && !cap.live_at(1_000), "expiry DAA is exclusive");
        let mut stale = capability();
        stale.capability_nonce = 6;
        assert!(cap.supersedes(&stale) && !stale.supersedes(&cap), "a stale capability cannot displace a fresh one");
        let mut other_verifier = capability();
        other_verifier.verifier_id = h64(0x09);
        other_verifier.capability_nonce = 99;
        assert!(!other_verifier.supersedes(&cap), "supersession never crosses verifiers");

        let mut overcommitted = capability();
        overcommitted.available_slots = 5;
        assert!(matches!(overcommitted.validate(), Err(PalwRoutingError::NotCanonical(_))));
        let mut idle = capability();
        idle.max_concurrency = 0;
        assert!(matches!(idle.validate(), Err(PalwRoutingError::NotCanonical(_))));
        let mut unsigned = capability();
        unsigned.signature.clear();
        assert!(matches!(unsigned.validate(), Err(PalwRoutingError::NotCanonical(_))));

        // Message: network-bound, signature-excluded, field-sensitive.
        let msg = verifier_capability_message_v1(b"misaka-devnet", &cap);
        assert_ne!(msg, verifier_capability_message_v1(b"misaka-testnet-11", &cap));
        let mut resigned = capability();
        resigned.signature = vec![0x55; 64];
        assert_eq!(msg, verifier_capability_message_v1(b"misaka-devnet", &resigned));
        let mut moved_root = capability();
        moved_root.ready_binding_root = h64(0x0F);
        assert_ne!(msg, verifier_capability_message_v1(b"misaka-devnet", &moved_root));
        assert_ne!(cap.capability_id(), resigned.capability_id(), "the id names the signed statement");
    }

    // -----------------------------------------------------------------------------------------
    // §7 eligibility — every conjunct refuses on its own
    // -----------------------------------------------------------------------------------------

    #[test]
    fn every_eligibility_conjunct_refuses_on_its_own() {
        let binding = binding();
        let binding_id = binding.registration_id();
        let base = eligible_candidate(0x01, binding_id);
        let eligible = |c: &PalwRoutedCandidateV1| {
            routed_candidate_eligible_v1(c, &binding, &binding_id, &h64(EXECUTOR), &h64(EXECUTOR_DOMAIN), BOND, NOW)
        };
        assert!(eligible(&base), "the base candidate must be eligible or nothing below proves anything");

        let mut wrong_family = base.clone();
        wrong_family.execution_family = PalwExecutionFamilyV1::Metal;
        assert!(!eligible(&wrong_family), "family mismatch panels");
        let mut wrong_generation = base.clone();
        wrong_generation.family_version = 2;
        assert!(!eligible(&wrong_generation), "family-version mismatch panels");
        let mut foreign_ready = base.clone();
        foreign_ready.ready_proof.leaf_index ^= 1;
        assert!(!eligible(&foreign_ready), "a band match without a valid ready proof panels");
        let mut no_slots = base.clone();
        no_slots.available_slots = 0;
        assert!(!eligible(&no_slots));
        let mut bond_short = base.clone();
        bond_short.available_bond = BOND - 1;
        assert!(!eligible(&bond_short), "a bond-short capability is eligible");
        let mut expired = base.clone();
        expired.availability_expiry_daa = NOW;
        assert!(!eligible(&expired), "an expired capability is eligible");
        let mut disreputable = base.clone();
        disreputable.reputation_ok = false;
        assert!(!eligible(&disreputable));
        let mut the_executor = base.clone();
        the_executor.verifier_id = h64(EXECUTOR);
        assert!(!eligible(&the_executor), "self-verification");
        let mut same_operator = base.clone();
        same_operator.control_domain_id = h64(EXECUTOR_DOMAIN);
        assert!(!eligible(&same_operator), "operator aggregation counts once");
        let mut frozen = base.clone();
        frozen.frozen = true;
        assert!(!eligible(&frozen));
    }

    #[test]
    fn a_b4_receipt_never_panels_a_max_b3_verifier() {
        // A B4 binding (the routing keys mutated for the scenario; eligibility reads the row,
        // not its validation) against verifiers below and at its band.
        let mut b4_binding = binding();
        b4_binding.model_band = PalwModelBandV1::B4;
        let b4_id = b4_binding.registration_id();
        let mut candidate = eligible_candidate(0x01, b4_id);
        candidate.max_model_band = PalwModelBandV1::B3;
        assert!(
            !routed_candidate_eligible_v1(&candidate, &b4_binding, &b4_id, &h64(EXECUTOR), &h64(EXECUTOR_DOMAIN), BOND, NOW),
            "a max-B3 verifier held a B4 duty"
        );
        candidate.max_model_band = PalwModelBandV1::B4;
        assert!(routed_candidate_eligible_v1(&candidate, &b4_binding, &b4_id, &h64(EXECUTOR), &h64(EXECUTOR_DOMAIN), BOND, NOW));
    }

    // -----------------------------------------------------------------------------------------
    // §7 the routed panel
    // -----------------------------------------------------------------------------------------

    #[test]
    fn the_routed_panel_is_deterministic_order_invariant_and_sized_by_the_windows() {
        let binding_id = binding().registration_id();
        let candidates: Vec<PalwRoutedCandidateV1> = (1u8..=8).map(|i| eligible_candidate(i, binding_id)).collect();
        let panel = routed_panel(&candidates, 0);
        assert_eq!(panel.len(), 2, "windows.q = 2 is the funded panel size");
        let mut shuffled = candidates.clone();
        shuffled.reverse();
        shuffled.swap(0, 3);
        assert_eq!(panel, routed_panel(&shuffled, 0));
        // The executor and its control domain never appear.
        assert!(!panel.contains(&h64(EXECUTOR)));
    }

    #[test]
    fn escalation_re_draws_wider_not_a_prefix() {
        let binding_id = binding().registration_id();
        let candidates: Vec<PalwRoutedCandidateV1> = (1u8..=8).map(|i| eligible_candidate(i, binding_id)).collect();
        let round0 = routed_panel(&candidates, 0);
        let round1 = routed_panel(&candidates, 1);
        assert_eq!(round0.len(), 2);
        assert_eq!(round1.len(), 4, "each lapse widens the panel");
        assert_ne!(round0, round1[..2].to_vec(), "the escalation is a re-draw, not an extension of the lapsed panel");
        assert_eq!(routed_panel(&candidates, 1), round1, "escalation rounds are as deterministic as round 0");
        // The width computation saturates instead of panicking at the adversarial extreme.
        assert!(escalated_panel_width_v1(2, u32::MAX) >= escalated_panel_width_v1(2, 1_000_000));
    }

    #[test]
    fn the_three_lotteries_never_share_a_draw() {
        // Same job, same executor, same anchor, same eight ids — the VLT sortition, the
        // ADR-0028 class panel and the ADR-0034 routed panel must all order differently.
        let binding = binding();
        let binding_id = binding.registration_id();
        let routed: Vec<PalwRoutedCandidateV1> = (1u8..=8).map(|i| eligible_candidate(i, binding_id)).collect();
        let routed_full = select_routed_replay_panel_v1(
            &h64(0x01),
            &h64(EXECUTOR),
            &h64(EXECUTOR_DOMAIN),
            &h64(0x03),
            &binding,
            BOND,
            NOW,
            &routed,
            3, // q · 4 = 8: the full ordering is the sensitive object
        );
        let class_candidates: Vec<PalwPanelCandidateV1> = (1u8..=8)
            .map(|i| PalwPanelCandidateV1 {
                validator_id: h64(i),
                runtime_class_id: binding.runtime_class_id,
                bonded: true,
                frozen: false,
            })
            .collect();
        let class_panel =
            select_replay_panel_v1(&h64(0x01), &h64(EXECUTOR), &h64(0x03), &binding.runtime_class_id, &class_candidates, 8);
        let vlt_pairs: Vec<(Hash64, Hash64)> = (1u8..=8).map(|i| (h64(i), binding.runtime_class_id)).collect();
        let vlt_panel = crate::vlt::select_verifiers(h64(0x01), h64(EXECUTOR), h64(0x03), binding.runtime_class_id, &vlt_pairs, 8);
        assert_eq!(routed_full.len(), 8);
        assert_ne!(routed_full, class_panel, "the routed draw predicts the class draw");
        assert_ne!(routed_full, vlt_panel, "the routed draw predicts the VLT draw");
        assert_ne!(class_panel, vlt_panel);
    }

    #[test]
    fn one_operator_cannot_hold_two_seats() {
        let binding_id = binding().registration_id();
        // A verifier fed twice (two live capabilities — the caller failed nonce supersession)
        // is dropped entirely: picking one record would make the panel input-order-dependent.
        let mut candidates: Vec<PalwRoutedCandidateV1> = (1u8..=4).map(|i| eligible_candidate(i, binding_id)).collect();
        let mut duplicate = eligible_candidate(1, binding_id);
        duplicate.available_slots = 2; // a conflicting second record, not a byte-identical echo
        candidates.push(duplicate);
        let panel = routed_panel(&candidates, 0);
        assert!(!panel.contains(&h64(1)), "a duplicated verifier id held a seat");
        assert_eq!(panel.len(), 2, "the remaining unique verifiers still fill the panel");

        // Two DISTINCT verifiers in one control domain get one seat between them — the §10
        // counts-once rule applied to seats; the lowest ticket holds it.
        let mut same_domain: Vec<PalwRoutedCandidateV1> = (1u8..=4).map(|i| eligible_candidate(i, binding_id)).collect();
        let shared = h64(0xDD);
        for c in same_domain.iter_mut() {
            c.control_domain_id = shared;
        }
        let collapsed = routed_panel(&same_domain, 0);
        assert_eq!(collapsed.len(), 1, "one control domain is one seat, not a full panel");
        // And with an honest mixed set, the panel never seats two of one domain.
        let mut mixed: Vec<PalwRoutedCandidateV1> = (1u8..=6).map(|i| eligible_candidate(i, binding_id)).collect();
        mixed[0].control_domain_id = shared;
        mixed[1].control_domain_id = shared;
        let mixed_panel = routed_panel(&mixed, 1); // widened to 4 of 6
        assert_eq!(mixed_panel.len(), 4);
        let seated_shared = mixed_panel.iter().filter(|id| **id == h64(1) || **id == h64(2)).count();
        assert!(seated_shared <= 1, "two verifiers of one control domain were both seated");
    }

    #[test]
    fn an_empty_eligible_set_yields_an_empty_panel_never_a_shrunk_quorum() {
        let binding_id = binding().registration_id();
        let mut all_frozen: Vec<PalwRoutedCandidateV1> = (1u8..=4).map(|i| eligible_candidate(i, binding_id)).collect();
        for c in &mut all_frozen {
            c.frozen = true;
        }
        assert!(routed_panel(&all_frozen, 0).is_empty());
        assert!(routed_panel(&all_frozen, 5).is_empty(), "escalation widens the draw, never the eligibility");
        assert!(routed_panel(&[], 0).is_empty());
    }

    // -----------------------------------------------------------------------------------------
    // §5 carried keys against the registry
    // -----------------------------------------------------------------------------------------

    #[test]
    fn carried_routing_keys_are_checked_against_the_registry_never_believed() {
        let binding = binding();
        let id = binding.registration_id();
        let ok = |coverage| validate_receipt_routing_keys_v1(&id, PalwExecutionFamilyV1::Cpu, PalwModelBandV1::B0, &binding, coverage);
        ok(PalwBindingCoverageStateV1::Active).unwrap();
        ok(PalwBindingCoverageStateV1::LowCoverage).unwrap();
        ok(PalwBindingCoverageStateV1::Throttled).unwrap();
        ok(PalwBindingCoverageStateV1::Deprecated).unwrap();
        assert_eq!(
            ok(PalwBindingCoverageStateV1::Frozen),
            Err(PalwRoutingError::BindingNotAccepting(PalwBindingCoverageStateV1::Frozen))
        );
        assert_eq!(
            ok(PalwBindingCoverageStateV1::Retired),
            Err(PalwRoutingError::BindingNotAccepting(PalwBindingCoverageStateV1::Retired)),
            "receipts against retired bindings are rejected"
        );
        assert_eq!(
            ok(PalwBindingCoverageStateV1::ContradictionFreeze),
            Err(PalwRoutingError::BindingNotAccepting(PalwBindingCoverageStateV1::ContradictionFreeze))
        );

        // Band forgery is invalidity (ADR-0034 §5) — in both directions.
        assert_eq!(
            validate_receipt_routing_keys_v1(
                &id,
                PalwExecutionFamilyV1::Cpu,
                PalwModelBandV1::B2,
                &binding,
                PalwBindingCoverageStateV1::Active
            ),
            Err(PalwRoutingError::BandForged { carried: PalwModelBandV1::B2, registered: PalwModelBandV1::B0 })
        );
        assert_eq!(
            validate_receipt_routing_keys_v1(
                &id,
                PalwExecutionFamilyV1::Metal,
                PalwModelBandV1::B0,
                &binding,
                PalwBindingCoverageStateV1::Active
            ),
            Err(PalwRoutingError::FamilyMismatch { carried: PalwExecutionFamilyV1::Metal, registered: PalwExecutionFamilyV1::Cpu })
        );
        // The lookup-collision guard: a row that is not the carried binding is refused even
        // when its coarse keys happen to match.
        assert_eq!(
            validate_receipt_routing_keys_v1(
                &h64(0x99),
                PalwExecutionFamilyV1::Cpu,
                PalwModelBandV1::B0,
                &binding,
                PalwBindingCoverageStateV1::Active
            ),
            Err(PalwRoutingError::BindingIdMismatch)
        );
    }

    // -----------------------------------------------------------------------------------------
    // §10 coverage and activation
    // -----------------------------------------------------------------------------------------

    #[test]
    fn the_coverage_walk_descends_with_starvation_and_contradiction_outranks_everything() {
        let facts = |ready: u32, below: u32| PalwCoverageFactsV1 {
            ready_independent_count: ready,
            min_ready: PALW_ROUTING_MIN_READY_DEVNET,
            epochs_below_min: below,
            contradiction_observed: false,
            deprecation_declared: false,
            retirement_epoch_reached: false,
        };
        assert_eq!(coverage_state_v1(&facts(3, 0)), PalwBindingCoverageStateV1::Active);
        assert_eq!(coverage_state_v1(&facts(2, 1)), PalwBindingCoverageStateV1::LowCoverage);
        assert_eq!(coverage_state_v1(&facts(2, 2)), PalwBindingCoverageStateV1::Throttled);
        assert_eq!(coverage_state_v1(&facts(2, 4)), PalwBindingCoverageStateV1::Frozen);
        assert_eq!(coverage_state_v1(&facts(0, 0)), PalwBindingCoverageStateV1::Frozen, "zero ready freezes immediately");
        let mut contradicted = facts(3, 0);
        contradicted.contradiction_observed = true;
        assert_eq!(coverage_state_v1(&contradicted), PalwBindingCoverageStateV1::ContradictionFreeze);
        let mut retired = facts(3, 0);
        retired.retirement_epoch_reached = true;
        assert_eq!(coverage_state_v1(&retired), PalwBindingCoverageStateV1::Retired);
        let mut deprecated = facts(3, 0);
        deprecated.deprecation_declared = true;
        assert_eq!(coverage_state_v1(&deprecated), PalwBindingCoverageStateV1::Deprecated);
        // Recovery: the ready count coming back IS the transition back to Active.
        assert_eq!(coverage_state_v1(&facts(3, 4)), PalwBindingCoverageStateV1::Active);

        // Fail-closed on degenerate thresholds: min_ready 0 is no threshold, and a starved
        // binding freezes even mid-deprecation — Deprecated still accepts receipts, and a
        // planned retirement must not keep admitting work nobody can replay.
        let mut no_threshold = facts(0, 10);
        no_threshold.min_ready = 0;
        assert_eq!(coverage_state_v1(&no_threshold), PalwBindingCoverageStateV1::Frozen, "a zero threshold is not Active");
        let mut deprecated_starved = facts(0, 5);
        deprecated_starved.deprecation_declared = true;
        assert_eq!(coverage_state_v1(&deprecated_starved), PalwBindingCoverageStateV1::Frozen, "starvation outranks deprecation");
        let mut deprecated_below = facts(2, 4);
        deprecated_below.deprecation_declared = true;
        assert_eq!(coverage_state_v1(&deprecated_below), PalwBindingCoverageStateV1::Frozen);

        // The counting rule is this module's, not each caller's: consecutive means
        // consecutive, and recovery resets.
        assert_eq!(next_epochs_below_min_v1(0, 2, 3), 1);
        assert_eq!(next_epochs_below_min_v1(3, 2, 3), 4);
        assert_eq!(next_epochs_below_min_v1(3, 3, 3), 0, "an epoch at the threshold resets the streak");
        assert_eq!(next_epochs_below_min_v1(0, 5, 0), 1, "a zero threshold never counts as recovered");
        assert_eq!(next_epochs_below_min_v1(u32::MAX, 0, 3), u32::MAX, "the streak saturates");
    }

    /// Fixtures for activation: a signed definition that joins the fleet row, and the CPU/v1
    /// generation manifest admitting the row's runtime manifest.
    fn definition_for(row: &PalwClassRegistrationV1) -> ModelDefinitionV1 {
        ModelDefinitionV1 {
            version: PALW_ROUTING_OBJECT_VERSION_V1,
            model_profile_id: row.model_profile_id,
            gguf_sha256: [0xAA; 32],
            gguf_size: row.model_artifact_bytes,
            tokenizer_id: row.tokenizer_id,
            architecture_id: h64(0x05),
            total_parameter_count: 2_000_000_000,
            active_parameter_count: 2_000_000_000,
            publisher_signature: vec![0x55; 64],
        }
    }

    #[test]
    fn activation_fails_closed_and_every_gate_is_derived_from_registered_records() {
        let row = binding();
        let definition = definition_for(&row);
        let manifests = [manifest(PalwExecutionFamilyV1::Cpu, 1, 0, None)];
        let activate = |row: &PalwClassRegistrationV1, golden: u32, min: u32, retrievable: bool| {
            binding_may_activate_v1(row, &definition, &manifests, 100, golden, min, retrievable)
        };
        assert!(activate(&row, 3, PALW_ROUTING_MIN_READY_DEVNET, true));
        assert!(!activate(&row, 2, PALW_ROUTING_MIN_READY_DEVNET, true), "two of three goldens");
        assert!(!activate(&row, 3, PALW_ROUTING_MIN_READY_DEVNET, false), "artifact unretrievable");
        assert!(!activate(&row, 3, 0, true), "a zero threshold is not a requirement — operated on prayer");

        // The generation gates are read from the manifest set, not caller booleans: a
        // missing generation, a retired one, and an unadmitted runtime manifest all refuse.
        assert!(
            !binding_may_activate_v1(&row, &definition, &[], 100, 3, PALW_ROUTING_MIN_READY_DEVNET, true),
            "no manifest record — the generation does not exist"
        );
        let retired = [manifest(PalwExecutionFamilyV1::Cpu, 1, 0, Some(50))];
        assert!(!binding_may_activate_v1(&row, &definition, &retired, 100, 3, PALW_ROUTING_MIN_READY_DEVNET, true));
        let mut foreign_runtime = row.clone();
        foreign_runtime.runtime_manifest_hash = h64(0x77);
        assert!(
            !binding_may_activate_v1(&foreign_runtime, &definition, &manifests, 100, 3, PALW_ROUTING_MIN_READY_DEVNET, true),
            "a runtime manifest the generation never admitted activates nothing"
        );

        // The band cap is the manifest's registered fact: B1 fits the CPU cap, B2 does not,
        // and a further record can LOWER the cap without overwriting anything.
        let mut b1 = row.clone();
        b1.model_band = PalwModelBandV1::B1;
        assert!(binding_may_activate_v1(&b1, &definition, &manifests, 100, 3, PALW_ROUTING_MIN_READY_DEVNET, true));
        let mut b2 = row.clone();
        b2.model_band = PalwModelBandV1::B2;
        assert!(!binding_may_activate_v1(&b2, &definition, &manifests, 100, 5, PALW_ROUTING_MIN_READY_DEVNET, true));
        let mut recapped = manifest(PalwExecutionFamilyV1::Cpu, 1, 0, None);
        recapped.max_active_band = PalwModelBandV1::B0;
        let with_recap = [manifests[0].clone(), recapped];
        assert!(
            !binding_may_activate_v1(&b1, &definition, &with_recap, 100, 3, PALW_ROUTING_MIN_READY_DEVNET, true),
            "the most restrictive published cap wins"
        );

        // Reserved families never activate in v1 (their first binding is the conformance
        // campaign), and a definition that does not join the row refuses.
        let mut cuda = row.clone();
        cuda.execution_family = PalwExecutionFamilyV1::Cuda;
        let cuda_manifests = [manifest(PalwExecutionFamilyV1::Cuda, 1, 0, None)];
        assert!(!binding_may_activate_v1(&cuda, &definition, &cuda_manifests, 100, 9, PALW_ROUTING_MIN_READY_DEVNET, true));
        let mut lying_size = definition_for(&row);
        lying_size.gguf_size = row.model_artifact_bytes - 1;
        assert!(
            !binding_may_activate_v1(&row, &lying_size, &manifests, 100, 3, PALW_ROUTING_MIN_READY_DEVNET, true),
            "the registered artifact envelope must BE the signed definition's size"
        );
        assert!(binding_matches_definition_v1(&row, &definition));
        assert!(!binding_matches_definition_v1(&row, &lying_size));
    }

    #[test]
    fn the_registered_tag_ledger_is_parseable_unique_and_covers_this_builds_cpu_class() {
        let mut seen = std::collections::HashSet::new();
        for tag in PALW_REGISTERED_CLASS_TAGS {
            assert!(routing_keys_for_class_tag_v1(tag).is_some(), "registered tag {tag} must parse into routing keys");
            assert!(seen.insert(crate::vlt::derive_runtime_class_id(tag)), "two ledger tags derive one class id");
        }
        // The build's own CPU class tag is in the ledger — except the decline-to-participate
        // placeholder, which is deliberately unroutable.
        let build_tag = crate::vlt::qwen35_pins::CPU_RUNTIME_CLASS;
        if build_tag != "misaka-palw-lite-cpu/other-arch/v1" {
            assert!(PALW_REGISTERED_CLASS_TAGS.contains(&build_tag), "this build's CPU tag {build_tag} is missing from the ledger");
        }
    }

    #[test]
    fn routing_keys_parse_from_the_live_tags_and_fail_closed_on_alien_shapes() {
        use PalwExecutionFamilyV1::*;
        assert_eq!(routing_keys_for_class_tag_v1("misaka-palw-lite-cpu/x86_64/v1"), Some((Cpu, 1)));
        assert_eq!(routing_keys_for_class_tag_v1("misaka-palw-lite-cpu/aarch64-dotprod/v1"), Some((Cpu, 1)));
        assert_eq!(routing_keys_for_class_tag_v1("misaka-palw-lite-fp/apple-metal-arm64/v1"), Some((Metal, 1)));
        assert_eq!(routing_keys_for_class_tag_v1("misaka-palw-lite-fp/cuda-sm90/v2"), Some((Cuda, 2)));
        assert_eq!(routing_keys_for_class_tag_v1("misaka-palw-lite-fp/rocm-gfx11/v1"), Some((Rocm, 1)));
        // Fail-closed: unknown shapes, zero generations and trailing segments register nothing.
        assert_eq!(routing_keys_for_class_tag_v1("misaka-palw-lite-npu/exotic/v1"), None);
        assert_eq!(routing_keys_for_class_tag_v1("misaka-palw-lite-cpu/x86_64/v0"), None);
        assert_eq!(routing_keys_for_class_tag_v1("misaka-palw-lite-cpu/x86_64"), None);
        assert_eq!(routing_keys_for_class_tag_v1("misaka-palw-lite-cpu/x86_64/v1/extra"), None);
        assert_eq!(routing_keys_for_class_tag_v1("misaka-palw-lite-cpu/x86_64/1"), None);
    }

    #[test]
    fn a_registry_store_must_refuse_rows_sharing_a_class_id() {
        let row = binding();
        assert!(binding_rows_coherent_v1(std::slice::from_ref(&row)));
        // A re-registration (new windows, same tag → same class id) is exactly the shape the
        // rule exists for: the credit layer keys by class id, routing by registration id —
        // two rows sharing a class id would credit a receipt under the wrong row's windows.
        let mut re_registered = row.clone();
        re_registered.checkpoint_interval = 16;
        assert_ne!(re_registered.registration_id(), row.registration_id());
        assert!(!binding_rows_coherent_v1(&[row.clone(), re_registered]));
        assert!(!binding_rows_coherent_v1(&[row.clone(), row.clone()]), "a true duplicate is refused");
    }
}
