//! PALW class registration v1 — the object every "pinned at registration" sentence meant.
//!
//! A dozen places across ADR-0026…0033 say a value is "opaque until registration",
//! "registration-measured", or "a network fact" — and until now there was no object to put
//! them in. This is that object: one record per determinism class, carrying every measured
//! identity and number the adjudication machinery reads, with the validation that makes an
//! incoherent registration unrepresentable.
//!
//! # What registration is, and is not
//!
//! It **is** the act of turning measurements into consensus-visible facts: this class exists,
//! these are its identities, this is what it may credit, these windows apply. It is **not** an
//! activation — a registered class with `credited_ceiling_tokens = 0` is exactly the
//! zero-credit state the §12 gate and the emergency rollback both use (the ceiling IS the
//! switch, ADR-0033 §6).
//!
//! # The discipline the validation enforces
//!
//! * **Derived values must be derived, not asserted.** `credited_ceiling_tokens` must equal
//!   `credited_ceiling_tokens_v1(measurement, windows, block_time)` — a registration cannot
//!   claim a ceiling its own measurement does not support (the B13 rule, made unrepresentable
//!   to violate).
//! * **Windows must satisfy their inequalities against the real network** (ADR-0028 §3, via
//!   `PalwScheduleParamsV1::validate`), and the class's measured p99 must fit them at κ.
//! * **Every transcendental site the class's profile binds must have an algorithm id**, and a
//!   class that binds a libm site must say which libm (ADR-0031 Fact 4's admission boundary:
//!   a class whose libm cannot be transcribed registers with `libm_transcribed = false` and is
//!   *structurally* adjudicable only — an honest, visible limitation rather than a silent one).
//! * **The commitment form determines the Stage-2 eligibility** (ADR-0029 §6: bare-v2 needs
//!   drilled chunked carriage).
//!
//! # Consumers — this struct is READ, and its Borsh layout is fingerprint-relevant
//!
//! ADR-0033's credit gate embeds a row in `PalwCreditParamsV1` (which the virtual processor
//! reads and `config/params.rs` Borsh-hashes into the consensus fingerprint whenever a fence
//! is `Some` — every shipped network carries `None`); ADR-0034 routing reads rows for
//! eligibility, panels and receipt keys. A layout change is therefore a **version-generation
//! change** (bump [`PALW_REGISTRY_OBJECT_VERSION_V1`]), never a silent edit: on a
//! fence-active devnet it would be a peering split.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use thiserror::Error;

use crate::config::params::BlockrateParams;
use crate::palw_routing::{
    PalwExecutionFamilyV1, PalwModelBandV1, derived_model_band_v1, replay_work_ms_v1, routing_keys_for_class_tag_v1,
};
use crate::palw_schedule::{
    PalwEconomicFactsV1, PalwLeverageRemedyV1, PalwReplayCostMeasurementV1, PalwScheduleError, PalwScheduleParamsV1,
    credited_ceiling_tokens_v1, max_leverage_holds_v1,
};
use crate::palw_step::{PalwShapeProfileV3, PalwTranscendentalSiteV1};

// ---------------------------------------------------------------------------------------------
// Domains, caps
// ---------------------------------------------------------------------------------------------

/// Layout generation 2 (2026-08-16): the ADR-0034 routing keys joined the preimage. The bump
/// exists so any stray generation-1 bytes fail `UnsupportedVersion` instead of misdecoding —
/// at the insertion point a gen-1 `commitment_form` byte would otherwise parse as an
/// `execution_family` (`CompositeV1`/`V2` → `Metal`/`Cuda`) and misalign everything after
/// it. Generation 1 was never durably serialized anywhere; the guard is against strays, not
/// stores.
pub const PALW_REGISTRY_OBJECT_VERSION_V1: u16 = 2;

/// The identity of a whole registration record — what a governance/coordinated-release action
/// references, and what a class-freeze names.
pub const PALW_REGISTRY_DOMAIN_RECORD_ID: &[u8] = b"misaka-palw/class-registration-id/v1";

pub const PALW_REGISTRY_ALL_DOMAINS: &[&[u8]] = &[PALW_REGISTRY_DOMAIN_RECORD_ID];

/// Longest human-readable class label a registration may carry.
pub const PALW_REGISTRY_MAX_LABEL_BYTES: usize = 128;

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwRegistryError {
    #[error("unsupported registration version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("registration is not canonical: {0}")]
    NotCanonical(&'static str),
    #[error("the shape profile is not canonical: {0}")]
    Profile(crate::palw_step::PalwStepError),
    #[error("the window parameters are not canonical: {0}")]
    Windows(PalwScheduleError),
    #[error("registered credited ceiling {declared} is not the value its own measurement derives ({derived})")]
    CeilingNotDerived { declared: u32, derived: u32 },
    #[error("declared model band {declared:?} is not the band the registered resources derive ({derived:?})")]
    BandNotDerived { declared: PalwModelBandV1, derived: PalwModelBandV1 },
    #[error("the registered resources exceed every band (past 16× the base) — not registrable in v1")]
    BandNotRegistrable,
    #[error("the registered runtime_class_id is not the hash of the carried class tag")]
    ClassIdNotDerivedFromTag,
    #[error("the declared execution family / family version are not what the class tag reads")]
    RoutingKeysNotDerivedFromTag,
    #[error("the class's measured p99 does not fit its own replay window at κ")]
    ReplayDoesNotFit,
    #[error("transcendental site {site:?} is bound by the profile but has no registered algorithm")]
    TranscendentalUnbound { site: PalwTranscendentalSiteV1 },
    #[error("the class claims arithmetic depth but its catalog coverage is incomplete: {0}")]
    Coverage(crate::palw_catalog_coverage::PalwCoverageError),
}

// ---------------------------------------------------------------------------------------------
// The record
// ---------------------------------------------------------------------------------------------

/// Which commitment form a class produces — a registry fact, never encoded in a job context
/// (ADR-0030 §3, ADR-0026 consequences).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwCommitmentFormV1 {
    /// The frozen v2 logits root alone. ADR-0029 §6: Stage 2 requires drilled chunked carriage.
    BareV2 = 0,
    /// `misaka-palw/execution-commitment/v1` (logits + activation + checkpoint legs).
    CompositeV1 = 1,
    /// `misaka-palw/execution-commitment/v2` (adds the step leg; ADR-0030 §3).
    CompositeV2 = 2,
}

/// How far a class may be adjudicated — an honest, registered limitation (ADR-0031 Fact 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwAdjudicationDepthV1 {
    /// Structural faults only: no transcribable libm, or no step leg. Refutations may convict
    /// on shape/finiteness/ancestry, never on arithmetic.
    StructuralOnly = 0,
    /// Arithmetic conviction available for the steps whose kernel programs are catalogued.
    ArithmeticCatalogued = 1,
}

/// The measured identities and numbers of one determinism class.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClassRegistrationV1 {
    /// = [`PALW_REGISTRY_OBJECT_VERSION_V1`].
    pub version: u16,
    /// Human-readable label (operational only; never load-bearing).
    pub label: String,

    // --- identities (all measured; every one of them appears in a job context or a leg) ---
    pub runtime_class_id: Hash64,
    pub runtime_manifest_hash: Hash64,
    pub model_profile_id: Hash64,
    pub tokenizer_id: Hash64,
    /// The v3 profile — carried in full: the gate and the adjudicator both need its tables,
    /// and its id is recomputed rather than trusted.
    pub shape_profile: PalwShapeProfileV3,
    /// ADR-0030 §3 / palw_legs: which tensor a tap reads, and the replay state's byte layout.
    /// Opaque hashes whose preimages are registration-time strings.
    pub tap_semantics_id: Hash64,
    pub state_layout_id: Hash64,
    /// The measured checkpoint chunk geometry (ADR-0030 §3).
    pub state_chunk_map_id: Hash64,
    /// Tap layer indices and checkpoint interval — profile parameters, valued here.
    pub tap_layer_indices: Vec<u16>,
    pub checkpoint_interval: u32,

    // --- routing keys (ADR-0034 §3: the registration row IS the binding; binding_id =
    // --- registration_id() over this extended preimage) ---
    /// The class tag itself — the SAME string `runtime_class_id` hashes, carried so
    /// `validate()` can recompute the id from it AND machine-read the family/version out of
    /// it. Load-bearing, unlike `label` (which stays free-form and never checked).
    pub class_tag: String,
    /// Must equal what [`crate::palw_routing::routing_keys_for_class_tag_v1`] reads out of
    /// `class_tag` — checked, not trusted: a self-declared family would let a Metal-runtime
    /// row draft CPU panels that can never reproduce its trace.
    pub execution_family: PalwExecutionFamilyV1,
    /// The tag's `/vN` segment; a coordinated runtime generation, never zero. Checked
    /// against the tag like the family.
    pub family_version: u16,
    /// Derived (ADR-0034 §4), never declared: `validate()` rejects a band the registered
    /// resources do not derive — the `CeilingNotDerived` pattern, applied to bands.
    pub model_band: PalwModelBandV1,
    pub quantization_id: Hash64,
    /// Measured resource envelope — the band derivation's inputs. `model_artifact_bytes`
    /// must equal the signed `ModelDefinitionV1::gguf_size` at activation
    /// (`binding_matches_definition_v1`); the replay deadline in wall-clock terms is an
    /// accessor ([`Self::replay_deadline_secs`]), not a stored field — a stored copy would
    /// be uninterpretable without also storing the block time it assumed.
    pub model_artifact_bytes: u64,
    pub peak_memory_bytes: u64,
    pub max_proof_material_bytes: u64,

    // --- form and depth ---
    pub commitment_form: PalwCommitmentFormV1,
    pub adjudication_depth: PalwAdjudicationDepthV1,
    /// False when the class's libm cannot be transcribed (e.g. a closed-source libm): the
    /// class registers honestly as structural-only rather than claiming arithmetic depth.
    pub libm_transcribed: bool,

    // --- consensus work: the NORMATIVE per-inference cost (ADR-0038 D / ADR-0039 §5) ---
    /// The normative operation count of one canonical inference under this class's frozen
    /// kernel graph — the second factor of
    /// [`crate::palw_pwu::palw_pwu_v1`]`(class_target, pwu_per_inference)`.
    ///
    /// Deliberately adjacent to `replay_cost` below and deliberately **not** derived from it.
    /// `replay_cost` is measured wall-clock: host-dependent, hardware-dependent, self-reported,
    /// and correct for sizing dispute windows. This is a counted consequence of the registered
    /// model shape, the pinned kernel graph and the frozen decode budget
    /// ([`crate::pow_layer0::POW_L1_PALW_N_PREDICT_V1`]), which is why one number per class is
    /// enough: every ticket in a class has the same job shape. Using a millisecond figure here
    /// would put a host's clock into fork-choice weight — ADR-0038 Decision D's "static
    /// intra-class, never wall-clock" is exactly this line.
    ///
    /// It is also **not** a cross-class price. See [`crate::palw_pwu`] — pricing classes against
    /// each other by this number reintroduces the hand-tuned coefficient table ADR-0038
    /// Decision D rejects; that job belongs to the epoch share cap (ADR-0039 Decision 5).
    pub pwu_per_inference: u64,

    // --- measured cost and the values derived from it ---
    pub replay_cost: PalwReplayCostMeasurementV1,
    /// MUST equal `credited_ceiling_tokens_v1(replay_cost, windows, block_time)` (B13).
    pub credited_ceiling_tokens: u32,
    /// `ρ_v` — the measured replay/primary cost ratio, in parts per thousand (no floats in a
    /// preimage; 1000 = 1.0×).
    pub rho_v_permille: u32,
    /// The class's measured `p99_cold_replay` at the credited ceiling, milliseconds.
    pub p99_cold_replay_ms: u64,

    // --- §4e leverage remedy (B15 amendment) ---
    /// The encoded ADR-0028 §4e remedy — the (rate, fraction) pair the aggregate
    /// `max_leverage ≤ 1` inequality is checked against. Its *canonical form* is validated
    /// here; whether it actually BOUNDS the mint is [`Self::stage2_eligible`]'s question,
    /// because the inequality reads chain facts (bond, subsidy, unbonding) a registration
    /// cannot know by itself.
    pub leverage_remedy: PalwLeverageRemedyV1,

    // --- windows ---
    pub windows: PalwScheduleParamsV1,

    // --- transcendental bindings (site → algorithm id), ADR-0031 -------------------------
    pub transcendental_algorithms: Vec<(PalwTranscendentalSiteV1, Hash64)>,
}

impl PalwClassRegistrationV1 {
    /// The record's identity: canonical Borsh under the registry domain. Any measured value
    /// moving is a new registration, never an edit — the same discipline as every other PALW
    /// identity.
    pub fn registration_id(&self) -> Hash64 {
        let bytes = borsh::to_vec(self).expect("borsh of an owned registration cannot fail");
        let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_REGISTRY_DOMAIN_RECORD_ID).to_state();
        h.update(&bytes);
        let mut out = [0u8; 64];
        out.copy_from_slice(h.finalize().as_bytes());
        Hash64::from_bytes(out)
    }

    /// Everything a registration must satisfy to be coherent. `blockrate` is the target
    /// network's real constants — the windows are checked against them, not against a guess.
    pub fn validate(&self, blockrate: &BlockrateParams, target_time_per_block_ms: u64) -> Result<(), PalwRegistryError> {
        if self.version != PALW_REGISTRY_OBJECT_VERSION_V1 {
            return Err(PalwRegistryError::UnsupportedVersion { got: self.version, expected: PALW_REGISTRY_OBJECT_VERSION_V1 });
        }
        if self.label.len() > PALW_REGISTRY_MAX_LABEL_BYTES {
            return Err(PalwRegistryError::NotCanonical("label exceeds the cap"));
        }
        self.shape_profile.validate_shape().map_err(PalwRegistryError::Profile)?;
        self.windows.validate(blockrate).map_err(PalwRegistryError::Windows)?;

        // A class whose canonical inference costs nothing would contribute zero pwu at every
        // target, i.e. a class that mines blocks weighing nothing — indistinguishable from an
        // inert class, but able to occupy a difficulty domain and a share.
        if self.pwu_per_inference == 0 {
            return Err(PalwRegistryError::NotCanonical("pwu_per_inference is zero — a canonical inference costs something"));
        }

        // The B13 rule: a declared ceiling must be the one its own measurement derives.
        let derived = credited_ceiling_tokens_v1(&self.replay_cost, &self.windows, target_time_per_block_ms);
        if derived != self.credited_ceiling_tokens {
            return Err(PalwRegistryError::CeilingNotDerived { declared: self.credited_ceiling_tokens, derived });
        }
        // And the measured p99 must fit the window it registered.
        if !crate::palw_schedule::replay_p99_fits_v1(self.p99_cold_replay_ms, &self.windows, target_time_per_block_ms) {
            return Err(PalwRegistryError::ReplayDoesNotFit);
        }

        // ADR-0034 §3/§4: the routing keys. The class id must derive from the carried tag,
        // the family/version must be what the tag reads, and the band must be what the
        // resources derive — declared-but-not-derived is the same lie as a claimed ceiling,
        // and gets the same refusal.
        if self.class_tag.is_empty() || self.class_tag.len() > PALW_REGISTRY_MAX_LABEL_BYTES {
            return Err(PalwRegistryError::NotCanonical("class tag is empty or exceeds the cap"));
        }
        if crate::vlt::derive_runtime_class_id(&self.class_tag) != self.runtime_class_id {
            return Err(PalwRegistryError::ClassIdNotDerivedFromTag);
        }
        match routing_keys_for_class_tag_v1(&self.class_tag) {
            Some((family, version)) if family == self.execution_family && version == self.family_version => {}
            _ => return Err(PalwRegistryError::RoutingKeysNotDerivedFromTag),
        }
        if self.model_artifact_bytes == 0 || self.peak_memory_bytes == 0 || self.max_proof_material_bytes == 0 {
            return Err(PalwRegistryError::NotCanonical("a routing resource measurement is zero"));
        }
        let work_ms = replay_work_ms_v1(&self.replay_cost, self.credited_ceiling_tokens);
        match derived_model_band_v1(self.model_artifact_bytes, self.peak_memory_bytes, work_ms, self.max_proof_material_bytes) {
            None => return Err(PalwRegistryError::BandNotRegistrable),
            Some(band) if band != self.model_band => {
                return Err(PalwRegistryError::BandNotDerived { declared: self.model_band, derived: band });
            }
            Some(_) => {}
        }

        if self.rho_v_permille == 0 {
            return Err(PalwRegistryError::NotCanonical("rho_v is zero — a replay costs something"));
        }
        if self.leverage_remedy.min_credit_interval_daa == 0 {
            return Err(PalwRegistryError::NotCanonical("leverage remedy has a zero credit interval — not a rate"));
        }
        if self.leverage_remedy.base_subsidy_permille > 1000 {
            return Err(PalwRegistryError::NotCanonical("leverage remedy claims base(C) above the whole subsidy"));
        }
        if self.checkpoint_interval == 0 {
            return Err(PalwRegistryError::NotCanonical("checkpoint interval is zero"));
        }
        if self.tap_layer_indices.is_empty() || !self.tap_layer_indices.windows(2).all(|w| w[0] < w[1]) {
            return Err(PalwRegistryError::NotCanonical("tap layers are empty or not strictly ascending"));
        }
        if self.tap_layer_indices.iter().any(|&l| l >= self.shape_profile.layer_count) {
            return Err(PalwRegistryError::NotCanonical("a tap layer is not below the profile's layer count"));
        }

        // Depth honesty: arithmetic depth requires a step-leg commitment form AND, if the
        // profile binds any libm site, a transcribed libm.
        let binds_libm = self.shape_profile.transcendental_bindings.iter().any(|b| {
            matches!(
                b.site,
                PalwTranscendentalSiteV1::LibmExpf
                    | PalwTranscendentalSiteV1::LibmLogf
                    | PalwTranscendentalSiteV1::LibmSinf
                    | PalwTranscendentalSiteV1::LibmCosf
            )
        });
        if self.adjudication_depth == PalwAdjudicationDepthV1::ArithmeticCatalogued {
            if self.commitment_form != PalwCommitmentFormV1::CompositeV2 {
                return Err(PalwRegistryError::NotCanonical("arithmetic depth requires the step-leg commitment form (composite v2)"));
            }
            if binds_libm && !self.libm_transcribed {
                return Err(PalwRegistryError::NotCanonical(
                    "arithmetic depth claimed while binding an untranscribed libm — register structural-only",
                ));
            }
            // ADR-0039 1a / ADR-0038 A4: the claim must be TRUE of this build's catalog, not
            // merely self-consistent. Every kernel a step of this profile can reach must be one
            // the court can resolve — otherwise the adjudicator answers `Unadjudicable` for the
            // uncovered kernel, `settle_dispute_v3` slashes nobody, and the class earns
            // slash-bearing credit against a conviction path that cannot convict.
            self.catalog_coverage_certificate_v1().map_err(PalwRegistryError::Coverage)?;
        }

        // Every site the profile binds must have a registered algorithm id.
        for binding in &self.shape_profile.transcendental_bindings {
            if !self.transcendental_algorithms.iter().any(|(site, _)| *site == binding.site) {
                return Err(PalwRegistryError::TranscendentalUnbound { site: binding.site });
            }
        }
        Ok(())
    }

    /// The ADR-0038 A4 coverage certificate for this registration, or the gap that denies it.
    ///
    /// The reachable side is derived from the registered profile through the same walk the court
    /// uses; the catalogued side is THIS BUILD's table and never a caller's claim about it. So
    /// the certificate is a fact about a registration, not an assertion a registrant may make.
    ///
    /// One consequence to keep in view: `validate` therefore depends on the build's kernel
    /// table, and two builds can disagree about whether a registration validates. That is
    /// acceptable while `validate` is only ever applied to locally configured params and local
    /// binding-row files. If an on-chain registration flow ever validates a row that arrived
    /// over P2P, the catalog becomes consensus-critical and needs its own activation fence —
    /// taking the catalog as an argument instead is NOT the fix (that was the blocker this
    /// module's own note records).
    pub fn catalog_coverage_certificate_v1(
        &self,
    ) -> Result<crate::palw_catalog_coverage::PalwCatalogCoverageCertificateV1, crate::palw_catalog_coverage::PalwCoverageError> {
        crate::palw_catalog_coverage::verify_catalog_coverage_v1(&crate::palw_catalog_coverage::PalwReachableKernelSetV1 {
            execution_class_id: self.runtime_class_id,
            kernel_ids: self.shape_profile.reachable_kernel_ids_v1(),
        })
    }

    /// ONE credited job's full mint, in sompi, at a given block subsidy: `base(C)` plus its
    /// `q` attester shares, each `ρ_v · base(C)`.
    ///
    /// THE single definition of that amount. Two rules need it and must not be able to
    /// disagree: the per-block crediting ceiling that actually pays it out
    /// ([`crate::palw_credit::PalwCreditParamsV1::one_job_ceiling_sompi`], which delegates
    /// here) and the §4e leverage inequality that decides whether the bond covers it
    /// ([`max_leverage_holds_v1`]). They previously each did their own arithmetic and the
    /// inequality's was smaller, so the check licensed a mint it had not measured.
    ///
    /// The arithmetic itself lives in [`crate::palw_schedule::one_job_payout_sompi_v1`], beside
    /// the remedy it reads, so the inequality can reach it without a registration in hand.
    pub fn one_job_payout_sompi(&self, block_subsidy_sompi: u64) -> u64 {
        crate::palw_schedule::one_job_payout_sompi_v1(&self.leverage_remedy, self.rho_v_permille, self.windows.q, block_subsidy_sompi)
    }

    /// Whether this class may operate at ADR-0027 §6 Stage 2 (slash-bearing credit), given the
    /// external facts a registration cannot know by itself.
    ///
    /// Deliberately does NOT check `adjudication_depth`, and that asymmetry with
    /// [`crate::palw_credit::PalwCreditParamsV1::active_for`] is the point rather than an
    /// oversight. Two reasons: this function still has no non-test caller, so it could not be an
    /// enforcement point even if it wanted to be; and `BareV2` is already forced to
    /// `StructuralOnly` by the coherence check in `validate`, so a depth conjunct here would make
    /// `chunked_carriage_drilled` — ADR-0029 §6's drill gate, the only reason this parameter
    /// exists — unreachable for the sole commitment form it applies to. The ADR-0039 1a depth gate
    /// lives at the two live seams instead: `active_for` (per commitment) and
    /// `Params::validate_palw_v1` (at startup). Do not "complete" the symmetry here by deleting
    /// `chunked_carriage_drilled`.
    ///
    /// `chunked_carriage_drilled` is ADR-0029 §6's gate for bare-v2 classes: the carriage
    /// landed, but the DRILL is a fleet fact. `economics` carries the chain facts (bond,
    /// subsidy, unbonding period) the B15 precondition (ADR-0028 §4e amendment) is evaluated
    /// against: the registered remedy must actually bound the aggregate mint — an asserted
    /// "remedy encoded" flag proved nothing, so the flag was replaced by the evaluation.
    pub fn stage2_eligible(&self, chunked_carriage_drilled: bool, economics: &PalwEconomicFactsV1) -> bool {
        let payout = self.one_job_payout_sompi(economics.block_subsidy_sompi);
        if !max_leverage_holds_v1(&self.leverage_remedy, economics, payout) || self.credited_ceiling_tokens == 0 {
            return false;
        }
        match self.commitment_form {
            PalwCommitmentFormV1::BareV2 => chunked_carriage_drilled,
            PalwCommitmentFormV1::CompositeV1 | PalwCommitmentFormV1::CompositeV2 => true,
        }
    }

    /// The zero-credit form of this registration — the emergency rollback and the §12
    /// zero-credit stage are the same operation (ADR-0033 §6: the ceiling IS the switch).
    pub fn to_zero_credit(&self) -> Self {
        let mut zeroed = self.clone();
        zeroed.credited_ceiling_tokens = 0;
        zeroed
    }

    /// The replay deadline in wall-clock seconds, for UI/agents (ADR-0034 §3's "redundant
    /// view", provided as a derivation instead of a stored field: a stored copy would be
    /// meaningless without also storing the block time it assumed, and would invalidate
    /// every drafted registration each time a network re-parameterizes its block rate).
    pub fn replay_deadline_secs(&self, target_time_per_block_ms: u64) -> u64 {
        self.windows.w_replay.saturating_mul(target_time_per_block_ms) / 1000
    }
}

// =============================================================================================
// Tests
// =============================================================================================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::palw_carriage::PALW_CARRIAGE_ALL_DOMAINS;
    use crate::palw_legs::PALW_LEGS_ALL_DOMAINS;
    use crate::palw_reference::PALW_REFERENCE_ALL_DOMAINS;
    use crate::palw_schedule::PALW_SCHEDULE_ALL_DOMAINS;
    use crate::palw_slash::PALW_S_ALL_DOMAINS;
    use crate::palw_step::{
        PALW_STEP_ALL_DOMAINS, PALW_STEP_INPUT_LAYER_IN, PALW_STEP_OBJECT_VERSION_V1, PalwStepNodeRoleV1, PalwStepNodeV1,
        PalwStepOpKindV1, PalwStepOutLenV1, PalwTranscendentalBindingV1,
    };
    use crate::palw_step_leg::PALW_STEP_LEG_ALL_DOMAINS;
    use crate::palw_v2::PALW_V2_ALL_DOMAINS;

    fn h64(fill: u8) -> Hash64 {
        Hash64::from_bytes([fill; 64])
    }

    fn two_minute_blockrate() -> BlockrateParams {
        BlockrateParams::new_two_minute_bps()
    }

    /// The fixture profile, for `palw_schedule`'s ladder-reachability probe. `pub(crate)` so the
    /// activation condition is tested against a shape this tree actually builds rather than one
    /// invented beside it.
    pub(crate) fn profile_for_schedule_probe() -> PalwShapeProfileV3 {
        profile_with_libm(false)
    }

    fn profile_with_libm(binds_libm: bool) -> PalwShapeProfileV3 {
        // `h64(0x11)` is deliberately NOT a catalogued kernel id: this is the float class's
        // profile, and the float catalog is incomplete by design (ADR-0031 Fact 4). A class
        // built on it can only register structural-only.
        profile_with_kernels(binds_libm, |_| h64(0x11))
    }

    /// The profile above with each node's `kernel_semantics_id` chosen by `kernel_for`, so a
    /// fixture can build either the uncatalogued float profile or a genuinely covered one
    /// without two copies of thirty measured fields drifting apart.
    fn profile_with_kernels(binds_libm: bool, kernel_for: impl Fn(PalwStepOpKindV1) -> Hash64) -> PalwShapeProfileV3 {
        let node = |kind| PalwStepNodeV1 {
            op_kind: kind,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: String::new(),
            weight_dtypes: Vec::new(),
            out_len: PalwStepOutLenV1::Fixed { elements: 16 },
            tile_len: 16,
            kernel_semantics_id: kernel_for(kind),
            input_refs: vec![PALW_STEP_INPUT_LAYER_IN],
        };
        let bindings = if binds_libm {
            vec![PalwTranscendentalBindingV1 { site: PalwTranscendentalSiteV1::LibmExpf, algorithm_id: h64(0x33) }]
        } else {
            vec![PalwTranscendentalBindingV1 { site: PalwTranscendentalSiteV1::VectorExpPolynomial, algorithm_id: h64(0x34) }]
        };
        PalwShapeProfileV3 {
            version: PALW_STEP_OBJECT_VERSION_V1,
            lane: crate::palw_step::PalwStepLaneV1::Float32,
            layer_count: 4,
            full_attention_interval: 4,
            hidden_dim: 16,
            ffn_dim: 16,
            attn_heads: 1,
            attn_kv_heads: 1,
            attn_head_dim: 16,
            rope_dims: 2,
            rope_sections: [1, 1, 0, 0],
            rope_freq_base_bits: 0x4CBE_BC20,
            rms_eps_bits: 0x3583_37BD,
            base0_rms_eps_q: 1 << 8,
            logits_scheme_id: crate::palw_step_refute::flat_logits_scheme_id_v1(),
            l2_eps_bits: 0x3583_37BD,
            gdn_heads: 1,
            gdn_head_k_dim: 16,
            gdn_head_v_dim: 16,
            gdn_conv_kernel: 4,
            vocab_size: 16,
            repack_on: 1,
            llamafile_on: 1,
            flash_attn_disabled: 1,
            fused_gdn_on: 1,
            use_ref_off: 1,
            kv_cache_f16: 1,
            n_ctx: 64,
            n_batch: 64,
            n_ubatch: 64,
            n_seq: 1,
            n_threads: 4,
            pre_nodes: vec![node(PalwStepOpKindV1::EmbedLookup)],
            gdn_nodes: vec![node(PalwStepOpKindV1::RmsNorm)],
            attn_nodes: vec![node(PalwStepOpKindV1::RmsNorm)],
            post_nodes: vec![node(PalwStepOpKindV1::MatMulQuant)],
            reference_ruleset_id: h64(0x22),
            transcendental_bindings: bindings,
            contraction_facts: vec![],
            kv_chunk_calls: 0,
            state_chunk_map_id: h64(0x44),
        }
    }

    /// A registration built from the REAL fleet measurement (slowest host, 2026-08-16).
    /// `pub(crate)`: the credit-gate tests reuse it as their registered class.
    pub(crate) fn fleet_registration() -> PalwClassRegistrationV1 {
        let windows = PalwScheduleParamsV1::stage1_defaults_two_minute_bps();
        let replay_cost =
            PalwReplayCostMeasurementV1 { fixed_overhead_ms: 4_300, ms_per_decode_token: 165, format_ceiling_tokens: 4_095 };
        let ceiling = credited_ceiling_tokens_v1(&replay_cost, &windows, 120_000);
        let class_tag = "misaka-palw-lite-cpu/x86_64/v1"; // the live CPU tag (vlt::CPU_RUNTIME_CLASS on x86_64)
        PalwClassRegistrationV1 {
            version: PALW_REGISTRY_OBJECT_VERSION_V1,
            // Indicative normative op count for one canonical 128-token inference of the pinned
            // 2B class. A fixture value, not a measurement — the real one is counted from the
            // frozen kernel graph at registration.
            pwu_per_inference: 512_000_000,
            label: class_tag.into(),
            class_tag: class_tag.into(),
            runtime_class_id: crate::vlt::derive_runtime_class_id(class_tag),
            runtime_manifest_hash: h64(0x02),
            model_profile_id: h64(0x03),
            tokenizer_id: h64(0x04),
            shape_profile: profile_with_libm(false),
            tap_semantics_id: h64(0x05),
            state_layout_id: h64(0x06),
            state_chunk_map_id: h64(0x44),
            tap_layer_indices: vec![0, 1, 2, 3],
            checkpoint_interval: 8,
            execution_family: crate::palw_routing::PalwExecutionFamilyV1::Cpu,
            family_version: 1,
            model_band: crate::palw_routing::PalwModelBandV1::B0,
            quantization_id: h64(0x07),
            // The pinned Qwen3.5-2B-Q4_K_M gguf (`.palw-gguf-sha.json`, sha aaf42c8b…).
            model_artifact_bytes: 1_280_835_840,
            // Fleet bench ran to completion inside `systemd-run` MemoryMax 5 G scopes.
            peak_memory_bytes: 5_000_000_000,
            max_proof_material_bytes: 8 << 20,
            commitment_form: PalwCommitmentFormV1::CompositeV2,
            // STRUCTURAL-ONLY, and that is the ADR-0031 Fact 4 boundary rather than a fixture
            // convenience: this is the FLOAT CPU class, whose catalog closes on 7 of 17 kernels.
            // Its profile nodes carry an uncatalogued id, so under the ADR-0039 1a coverage gate
            // it cannot claim arithmetic depth — which means it cannot carry weight or credit.
            // `base0_registration` below is the covered class. Faking coverage here by pointing
            // these nodes at catalogued float descriptors would erase exactly the distinction the
            // ordering rule draws.
            adjudication_depth: PalwAdjudicationDepthV1::StructuralOnly,
            libm_transcribed: true,
            replay_cost,
            credited_ceiling_tokens: ceiling,
            rho_v_permille: 1_000,
            p99_cold_replay_ms: 90_716,
            // One job per 14 blocks at 0.1 % of the subsidy. NOT the amendment's printed
            // (10, 0.2 %): with ρ_v = 1 000‰ and q = 2, one job pays 3 × base(C), and §4e is
            // now checked against that full payout — under which (10, 0.2 %) fails and 14 is
            // the tightest interval 0.1 % admits. `palw_schedule` pins both directions.
            leverage_remedy: PalwLeverageRemedyV1 { min_credit_interval_daa: 14, base_subsidy_permille: 1 },
            windows,
            transcendental_algorithms: vec![(PalwTranscendentalSiteV1::VectorExpPolynomial, h64(0x34))],
        }
    }

    /// The PALW-BASE-0 class: the integer class whose kernel catalog actually closes, and
    /// therefore the only fixture that may claim `ArithmeticCatalogued` under the ADR-0039 1a
    /// coverage gate.
    ///
    /// Every measured number (`replay_cost`, `windows`, band inputs, `leverage_remedy`,
    /// `pwu_per_inference`) is borrowed BYTE-IDENTICALLY from `fleet_registration` so downstream
    /// assertions about ceiling 4 095 / band B0 / the 3 600 s deadline stay valid — the envelope
    /// is a loan, NOT a BASE-0 measurement, and no reader should treat it as one.
    ///
    /// What is genuinely BASE-0 here is the part the gate reads: each node carries a real
    /// descriptor id from `palw_step_refute`'s catalog, including `KDESC_BASE0_RESCALE` (ADR-0040
    /// Decision H's op 9). That last one is deliberate — a regression that dropped op 9 from
    /// `KDESC_ALL` would now fail REGISTRATION, not merely a coverage unit test.
    pub(crate) fn base0_registration() -> PalwClassRegistrationV1 {
        use crate::palw_step::kernel_semantics_id_v1;
        use crate::palw_step_refute::{
            KDESC_BASE0_EMBED, KDESC_BASE0_MATMUL, KDESC_BASE0_RESCALE, KDESC_BASE0_RMS_NORM, KDESC_BASE0_SOFTMAX,
        };
        let class_tag = "misaka-palw-base0-cpu/x86_64/v1";
        let mut reg = fleet_registration();
        let node = |kind, descriptor: &str| PalwStepNodeV1 {
            op_kind: kind,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: String::new(),
            weight_dtypes: Vec::new(),
            out_len: PalwStepOutLenV1::Fixed { elements: 16 },
            tile_len: 16,
            kernel_semantics_id: kernel_semantics_id_v1(descriptor),
            input_refs: vec![PALW_STEP_INPUT_LAYER_IN],
        };
        // No transcendental site at all: BASE-0's exp/reciprocal/rsqrt are integer programs in
        // the catalog, so there is nothing to bind and nothing to transcribe. That absence is
        // the reason this class's coverage can reach 100 %.
        reg.shape_profile.transcendental_bindings = vec![];
        reg.transcendental_algorithms = vec![];
        reg.libm_transcribed = false;
        reg.shape_profile.pre_nodes = vec![node(PalwStepOpKindV1::EmbedLookup, KDESC_BASE0_EMBED)];
        reg.shape_profile.gdn_nodes = vec![node(PalwStepOpKindV1::RmsNorm, KDESC_BASE0_RMS_NORM)];
        reg.shape_profile.attn_nodes = vec![node(PalwStepOpKindV1::SoftMax, KDESC_BASE0_SOFTMAX)];
        reg.shape_profile.post_nodes =
            vec![node(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL), node(PalwStepOpKindV1::Scale, KDESC_BASE0_RESCALE)];
        reg.label = class_tag.into();
        reg.class_tag = class_tag.into();
        reg.runtime_class_id = crate::vlt::derive_runtime_class_id(class_tag);
        reg.adjudication_depth = PalwAdjudicationDepthV1::ArithmeticCatalogued;
        reg
    }

    /// The B15 live facts (`docs/palw-economic-parameters-2026-08-16.md`): the 120 s subsidy
    /// rate-preserved from the 10 BPS genesis value, the 20 000 MSK bond, unbonding 10 083.
    fn b15_economic_facts() -> PalwEconomicFactsV1 {
        PalwEconomicFactsV1 {
            block_subsidy_sompi: 370_468_345 * 1_200, // 444 562 014 000 sompi = 4 445.62 MSK
            s_eff_sompi: 20_000 * 100_000_000,        // the 20 000 MSK bond
            unbonding_period_blocks: 10_083,
        }
    }

    #[test]
    fn registry_domain_is_unique_across_all_palw_modules() {
        let mut seen = std::collections::HashSet::new();
        for d in PALW_REGISTRY_ALL_DOMAINS {
            assert!(seen.insert(*d));
            assert!(d.len() <= 64);
        }
        for d in PALW_V2_ALL_DOMAINS
            .iter()
            .chain(PALW_S_ALL_DOMAINS.iter())
            .chain(PALW_LEGS_ALL_DOMAINS.iter())
            .chain(PALW_REFERENCE_ALL_DOMAINS.iter())
            .chain(PALW_SCHEDULE_ALL_DOMAINS.iter())
            .chain(PALW_CARRIAGE_ALL_DOMAINS.iter())
            .chain(PALW_STEP_ALL_DOMAINS.iter())
            .chain(PALW_STEP_LEG_ALL_DOMAINS.iter())
        {
            assert!(!seen.contains(d), "registry reuses a foreign domain");
        }
    }

    #[test]
    fn the_fleet_registration_validates_and_is_format_bound() {
        let reg = fleet_registration();
        reg.validate(&two_minute_blockrate(), 120_000).unwrap();
        // The B13 finding, now a registration fact: the pinned Q4 class is format-bound.
        assert_eq!(reg.credited_ceiling_tokens, 4_095);
    }

    #[test]
    fn a_claimed_ceiling_its_measurement_does_not_support_is_rejected() {
        let mut reg = fleet_registration();
        reg.credited_ceiling_tokens = 4_095 + 1; // past the format ceiling
        assert!(matches!(reg.validate(&two_minute_blockrate(), 120_000), Err(PalwRegistryError::CeilingNotDerived { .. })));
        // And a class whose measurement is far slower cannot claim the fast ceiling either.
        let mut slow = fleet_registration();
        slow.replay_cost.ms_per_decode_token = 1_650;
        assert!(matches!(slow.validate(&two_minute_blockrate(), 120_000), Err(PalwRegistryError::CeilingNotDerived { .. })));
    }

    #[test]
    fn an_untranscribed_libm_cannot_claim_arithmetic_depth() {
        // The ADR-0031 Fact 4 admission boundary: an Apple-libm class registers honestly.
        let mut reg = fleet_registration();
        // The float class registers structural-only now, so the depth claim has to be made
        // explicitly for the depth defence to be the thing under test.
        reg.adjudication_depth = PalwAdjudicationDepthV1::ArithmeticCatalogued;
        reg.shape_profile = profile_with_libm(true);
        reg.transcendental_algorithms = vec![(PalwTranscendentalSiteV1::LibmExpf, h64(0x33))];
        reg.libm_transcribed = false;
        // Exact message, not a wildcard: an earlier NotCanonical (a zeroed routing resource,
        // say) must not be able to satisfy this assertion while the depth defense regresses.
        assert!(matches!(
            reg.validate(&two_minute_blockrate(), 120_000),
            Err(PalwRegistryError::NotCanonical(
                "arithmetic depth claimed while binding an untranscribed libm — register structural-only"
            ))
        ));
        // Registered honestly as structural-only, it validates.
        reg.adjudication_depth = PalwAdjudicationDepthV1::StructuralOnly;
        reg.validate(&two_minute_blockrate(), 120_000).unwrap();
    }

    #[test]
    fn arithmetic_depth_requires_the_step_leg_form() {
        let mut reg = fleet_registration();
        reg.adjudication_depth = PalwAdjudicationDepthV1::ArithmeticCatalogued;
        reg.commitment_form = PalwCommitmentFormV1::CompositeV1;
        assert!(matches!(
            reg.validate(&two_minute_blockrate(), 120_000),
            Err(PalwRegistryError::NotCanonical("arithmetic depth requires the step-leg commitment form (composite v2)"))
        ));
        reg.adjudication_depth = PalwAdjudicationDepthV1::StructuralOnly;
        reg.validate(&two_minute_blockrate(), 120_000).unwrap();
    }

    #[test]
    fn arithmetic_depth_requires_the_catalog_to_actually_cover_the_profile() {
        // The BASE-0 class is the covered one, and it validates AT arithmetic depth.
        let base0 = base0_registration();
        assert_eq!(base0.adjudication_depth, PalwAdjudicationDepthV1::ArithmeticCatalogued);
        base0.validate(&two_minute_blockrate(), 120_000).unwrap();
        let cert = base0.catalog_coverage_certificate_v1().expect("the integer class closes");
        assert_eq!(cert.execution_class_id, base0.runtime_class_id, "a certificate names the class it covers");

        // ONE node repointed at an id no build resolves is enough to deny the claim. This is the
        // hole the gate exists for: the court answers `Unadjudicable` for that kernel, the
        // settlement slashes nobody, and a class in that state would earn slash-bearing credit
        // against a conviction path that cannot convict.
        let mut uncovered = base0_registration();
        uncovered.shape_profile.attn_nodes[0].kernel_semantics_id = h64(0xEE);
        let err = uncovered.validate(&two_minute_blockrate(), 120_000).unwrap_err();
        match err {
            PalwRegistryError::Coverage(crate::palw_catalog_coverage::PalwCoverageError::CoverageGap { missing }) => {
                assert_eq!(missing, vec![h64(0xEE)], "the gap must name every missing id, not a count");
            }
            other => panic!("expected a coverage gap, got {other:?}"),
        }
        // The same registration is perfectly legal once it stops claiming what it cannot do.
        uncovered.adjudication_depth = PalwAdjudicationDepthV1::StructuralOnly;
        uncovered.validate(&two_minute_blockrate(), 120_000).unwrap();

        // ADR-0040 Decision H's op 9 is IN the fixture's reachable set on purpose: dropping it
        // from the catalog is a registration failure, not just a coverage unit-test failure.
        let rescale = crate::palw_step::kernel_semantics_id_v1(crate::palw_step_refute::KDESC_BASE0_RESCALE);
        assert!(base0.shape_profile.reachable_kernel_ids_v1().contains(&rescale));

        // And the reachable set is what the COURT can reach, not what the tables declare: an
        // attention node table in a graph with no attention layers contributes nothing, because
        // no slot ever resolves into it.
        let mut no_attn = base0_registration();
        no_attn.shape_profile.full_attention_interval = 0;
        let reachable = no_attn.shape_profile.reachable_kernel_ids_v1();
        let softmax = crate::palw_step::kernel_semantics_id_v1(crate::palw_step_refute::KDESC_BASE0_SOFTMAX);
        assert_eq!(
            reachable.contains(&softmax),
            no_attn.shape_profile.attention_layer_exists(),
            "reachability must follow the same walk the adjudicator uses"
        );
    }

    #[test]
    fn an_unbound_transcendental_site_is_rejected() {
        let mut reg = fleet_registration();
        reg.transcendental_algorithms.clear();
        assert!(matches!(reg.validate(&two_minute_blockrate(), 120_000), Err(PalwRegistryError::TranscendentalUnbound { .. })));
    }

    #[test]
    fn stage2_eligibility_encodes_both_external_gates() {
        let facts = b15_economic_facts();
        let composite = fleet_registration();
        // Composite classes are not bare-v2-blocked, but the leverage remedy is universal.
        assert!(composite.stage2_eligible(false, &facts));

        // The pre-amendment live shape — full subsidy, credit every block — is exactly the
        // B15 violation: the registration VALIDATES (it is a canonical encoding) but can
        // never be Stage-2 eligible against the live facts. The asserted-boolean era would
        // have let a caller claim otherwise.
        let mut unremedied = fleet_registration();
        unremedied.leverage_remedy = PalwLeverageRemedyV1 { min_credit_interval_daa: 1, base_subsidy_permille: 1000 };
        unremedied.validate(&two_minute_blockrate(), 120_000).unwrap();
        assert!(!unremedied.stage2_eligible(true, &facts), "the un-remedied live shape mints against nothing");

        let mut bare = fleet_registration();
        bare.commitment_form = PalwCommitmentFormV1::BareV2;
        bare.adjudication_depth = PalwAdjudicationDepthV1::StructuralOnly;
        bare.validate(&two_minute_blockrate(), 120_000).unwrap();
        assert!(!bare.stage2_eligible(false, &facts), "ADR-0029 §6: bare-v2 needs the drill");
        assert!(bare.stage2_eligible(true, &facts));
    }

    #[test]
    fn a_non_canonical_leverage_remedy_is_rejected_at_validation() {
        let mut zero_interval = fleet_registration();
        zero_interval.leverage_remedy.min_credit_interval_daa = 0;
        assert!(matches!(
            zero_interval.validate(&two_minute_blockrate(), 120_000),
            Err(PalwRegistryError::NotCanonical("leverage remedy has a zero credit interval — not a rate"))
        ));
        let mut oversized = fleet_registration();
        oversized.leverage_remedy.base_subsidy_permille = 1_001;
        assert!(matches!(
            oversized.validate(&two_minute_blockrate(), 120_000),
            Err(PalwRegistryError::NotCanonical("leverage remedy claims base(C) above the whole subsidy"))
        ));
    }

    #[test]
    fn zero_credit_is_the_rollback_and_moves_the_id() {
        let reg = fleet_registration();
        let zeroed = reg.to_zero_credit();
        assert_eq!(zeroed.credited_ceiling_tokens, 0);
        assert!(!zeroed.stage2_eligible(true, &b15_economic_facts()), "a zero-ceiling class credits nothing");
        assert_ne!(zeroed.registration_id(), reg.registration_id(), "the rollback is a visible new registration");
        // It is still a COHERENT registration — the rollback does not produce garbage state.
        // (Its derived-ceiling check fails by construction, which is the point: a zero-credit
        // record is deliberately not re-derivable from its measurement, so it can only have
        // been set deliberately.)
        assert!(matches!(
            zeroed.validate(&two_minute_blockrate(), 120_000),
            Err(PalwRegistryError::CeilingNotDerived { declared: 0, .. })
        ));
    }

    #[test]
    fn registration_id_moves_with_every_measured_fact() {
        let base = fleet_registration().registration_id();
        let mutate = |f: &dyn Fn(&mut PalwClassRegistrationV1)| {
            let mut r = fleet_registration();
            f(&mut r);
            r.registration_id()
        };
        assert_ne!(mutate(&|r| r.runtime_class_id = h64(0x99)), base);
        assert_ne!(mutate(&|r| r.tap_semantics_id = h64(0x99)), base);
        assert_ne!(mutate(&|r| r.state_layout_id = h64(0x99)), base);
        assert_ne!(mutate(&|r| r.state_chunk_map_id = h64(0x99)), base);
        assert_ne!(mutate(&|r| r.checkpoint_interval = 16), base);
        assert_ne!(mutate(&|r| r.tap_layer_indices = vec![0, 2]), base);
        assert_ne!(mutate(&|r| r.rho_v_permille = 1_500), base);
        assert_ne!(mutate(&|r| r.p99_cold_replay_ms = 1), base);
        assert_ne!(mutate(&|r| r.commitment_form = PalwCommitmentFormV1::BareV2), base);
        assert_ne!(mutate(&|r| r.libm_transcribed = false), base);
        assert_ne!(mutate(&|r| r.replay_cost.ms_per_decode_token = 1), base);
        // The consensus work factor: two classes that differ only in what one inference costs
        // are different classes, and their blocks must weigh differently.
        assert_ne!(mutate(&|r| r.pwu_per_inference = 1), base);
        assert_ne!(mutate(&|r| r.windows.q = 3), base);
        assert_ne!(mutate(&|r| r.leverage_remedy.min_credit_interval_daa = 5_042), base);
        assert_ne!(mutate(&|r| r.leverage_remedy.base_subsidy_permille = 2), base);
        // The ADR-0034 routing keys are part of the binding's identity: any of them moving
        // is a NEW binding id with its own activation epoch — re-banding-by-edit untypable.
        assert_ne!(mutate(&|r| r.class_tag = "misaka-palw-lite-cpu/aarch64-dotprod/v1".into()), base);
        assert_ne!(mutate(&|r| r.execution_family = PalwExecutionFamilyV1::Metal), base);
        assert_ne!(mutate(&|r| r.family_version = 2), base);
        assert_ne!(mutate(&|r| r.model_band = PalwModelBandV1::B1), base);
        assert_ne!(mutate(&|r| r.quantization_id = h64(0x99)), base);
        assert_ne!(mutate(&|r| r.model_artifact_bytes = 1), base);
        assert_ne!(mutate(&|r| r.peak_memory_bytes = 1), base);
        assert_ne!(mutate(&|r| r.max_proof_material_bytes = 1), base);
    }

    // -----------------------------------------------------------------------------------------
    // ADR-0034 §4 — the band is derived, never declared
    // -----------------------------------------------------------------------------------------

    #[test]
    fn the_fleet_registration_derives_b0_and_the_work_base_is_frozen() {
        let reg = fleet_registration();
        reg.validate(&two_minute_blockrate(), 120_000).unwrap();
        assert_eq!(reg.model_band, PalwModelBandV1::B0, "row 1 is B0 — ADR-0034 §3");
        // The work base is a FROZEN snapshot of row 1's cost at v1-definition time — moving
        // it would re-band every registered binding at once (re-banding-by-constant), so a
        // fleet re-bench does NOT update it; a base change is a new derivation version.
        assert_eq!(
            crate::palw_routing::PALW_ROUTING_BASE_REPLAY_WORK_MS,
            679_975,
            "the v1 work base moved — that is a new derivation version, not an edit"
        );
        assert!(
            replay_work_ms_v1(&reg.replay_cost, reg.credited_ceiling_tokens) <= crate::palw_routing::PALW_ROUTING_BASE_REPLAY_WORK_MS,
            "row 1 no longer fits the frozen B0 work base"
        );
        // And the wall-clock deadline view is a derivation, not a stored field.
        assert_eq!(reg.replay_deadline_secs(120_000), 3_600, "w_replay 30 blocks × 120 s");
    }

    #[test]
    fn a_declared_band_the_resources_do_not_derive_is_rejected() {
        // Declaring one band up on B0 resources is the same lie as a claimed ceiling.
        let mut inflated = fleet_registration();
        inflated.model_band = PalwModelBandV1::B1;
        assert_eq!(
            inflated.validate(&two_minute_blockrate(), 120_000),
            Err(PalwRegistryError::BandNotDerived { declared: PalwModelBandV1::B1, derived: PalwModelBandV1::B0 })
        );
        // And declaring B0 on genuinely B1-sized resources is refused in the other
        // direction — under-banding would dodge the band-indexed bond floors.
        let mut oversized = fleet_registration();
        oversized.model_artifact_bytes = (4 << 30) + 1;
        assert_eq!(
            oversized.validate(&two_minute_blockrate(), 120_000),
            Err(PalwRegistryError::BandNotDerived { declared: PalwModelBandV1::B0, derived: PalwModelBandV1::B1 })
        );
        oversized.model_band = PalwModelBandV1::B1;
        oversized.validate(&two_minute_blockrate(), 120_000).unwrap();
    }

    /// A class whose canonical inference costs nothing would mine blocks weighing nothing at
    /// every target — inert in fork choice, yet occupying a difficulty domain and a share.
    #[test]
    fn a_zero_cost_inference_cannot_register() {
        let mut free = fleet_registration();
        free.pwu_per_inference = 0;
        assert_eq!(
            free.validate(&two_minute_blockrate(), 120_000),
            Err(PalwRegistryError::NotCanonical("pwu_per_inference is zero — a canonical inference costs something"))
        );
        // And the honest fixture still passes, so the new conjunct did not close the door on
        // everything else.
        assert!(fleet_registration().validate(&two_minute_blockrate(), 120_000).is_ok());
    }

    #[test]
    fn resources_past_every_band_cannot_register_at_all() {
        let mut hopeless = fleet_registration();
        hopeless.model_artifact_bytes = (4u64 << 30) * 17; // past 16× the artifact base
        hopeless.model_band = PalwModelBandV1::B4;
        assert_eq!(hopeless.validate(&two_minute_blockrate(), 120_000), Err(PalwRegistryError::BandNotRegistrable));
    }

    #[test]
    fn routing_keys_must_derive_from_the_carried_class_tag() {
        // A row whose class id is not the hash of its own tag is refused — the tag is the
        // load-bearing string, not the label.
        let mut forged_id = fleet_registration();
        forged_id.runtime_class_id = h64(0x99);
        assert_eq!(forged_id.validate(&two_minute_blockrate(), 120_000), Err(PalwRegistryError::ClassIdNotDerivedFromTag));

        // A Metal-runtime tag claiming the Cpu family is refused: the family is READ from the
        // tag, never believed — a self-declared family would draft panels of verifiers that
        // can never reproduce the trace.
        let metal_tag = "misaka-palw-lite-fp/apple-metal-arm64/v1";
        let mut cross_family = fleet_registration();
        cross_family.class_tag = metal_tag.into();
        cross_family.runtime_class_id = crate::vlt::derive_runtime_class_id(metal_tag);
        assert_eq!(cross_family.validate(&two_minute_blockrate(), 120_000), Err(PalwRegistryError::RoutingKeysNotDerivedFromTag));
        cross_family.execution_family = PalwExecutionFamilyV1::Metal;
        cross_family.validate(&two_minute_blockrate(), 120_000).unwrap();

        // A version relabel is refused the same way, and an unparseable tag registers nothing.
        let mut wrong_generation = fleet_registration();
        wrong_generation.family_version = 2;
        assert_eq!(wrong_generation.validate(&two_minute_blockrate(), 120_000), Err(PalwRegistryError::RoutingKeysNotDerivedFromTag));
        let mut alien_tag = fleet_registration();
        alien_tag.class_tag = "misaka-palw-lite-npu/exotic/v1".into();
        alien_tag.runtime_class_id = crate::vlt::derive_runtime_class_id("misaka-palw-lite-npu/exotic/v1");
        assert_eq!(alien_tag.validate(&two_minute_blockrate(), 120_000), Err(PalwRegistryError::RoutingKeysNotDerivedFromTag));
    }
}
