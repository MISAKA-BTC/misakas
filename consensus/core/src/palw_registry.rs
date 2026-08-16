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
//! Consensus-inert: nothing reads this yet. ADR-0033's gate is its first consumer.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use thiserror::Error;

use crate::config::params::BlockrateParams;
use crate::palw_schedule::{
    PalwEconomicFactsV1, PalwLeverageRemedyV1, PalwReplayCostMeasurementV1, PalwScheduleError, PalwScheduleParamsV1,
    credited_ceiling_tokens_v1, max_leverage_holds_v1,
};
use crate::palw_step::{PalwShapeProfileV3, PalwTranscendentalSiteV1};

// ---------------------------------------------------------------------------------------------
// Domains, caps
// ---------------------------------------------------------------------------------------------

pub const PALW_REGISTRY_OBJECT_VERSION_V1: u16 = 1;

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
    #[error("the class's measured p99 does not fit its own replay window at κ")]
    ReplayDoesNotFit,
    #[error("transcendental site {site:?} is bound by the profile but has no registered algorithm")]
    TranscendentalUnbound { site: PalwTranscendentalSiteV1 },
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

    // --- form and depth ---
    pub commitment_form: PalwCommitmentFormV1,
    pub adjudication_depth: PalwAdjudicationDepthV1,
    /// False when the class's libm cannot be transcribed (e.g. a closed-source libm): the
    /// class registers honestly as structural-only rather than claiming arithmetic depth.
    pub libm_transcribed: bool,

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

        // The B13 rule: a declared ceiling must be the one its own measurement derives.
        let derived = credited_ceiling_tokens_v1(&self.replay_cost, &self.windows, target_time_per_block_ms);
        if derived != self.credited_ceiling_tokens {
            return Err(PalwRegistryError::CeilingNotDerived { declared: self.credited_ceiling_tokens, derived });
        }
        // And the measured p99 must fit the window it registered.
        if !crate::palw_schedule::replay_p99_fits_v1(self.p99_cold_replay_ms, &self.windows, target_time_per_block_ms) {
            return Err(PalwRegistryError::ReplayDoesNotFit);
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
        }

        // Every site the profile binds must have a registered algorithm id.
        for binding in &self.shape_profile.transcendental_bindings {
            if !self.transcendental_algorithms.iter().any(|(site, _)| *site == binding.site) {
                return Err(PalwRegistryError::TranscendentalUnbound { site: binding.site });
            }
        }
        Ok(())
    }

    /// Whether this class may operate at ADR-0027 §6 Stage 2 (slash-bearing credit), given the
    /// external facts a registration cannot know by itself.
    ///
    /// `chunked_carriage_drilled` is ADR-0029 §6's gate for bare-v2 classes: the carriage
    /// landed, but the DRILL is a fleet fact. `economics` carries the chain facts (bond,
    /// subsidy, unbonding period) the B15 precondition (ADR-0028 §4e amendment) is evaluated
    /// against: the registered remedy must actually bound the aggregate mint — an asserted
    /// "remedy encoded" flag proved nothing, so the flag was replaced by the evaluation.
    pub fn stage2_eligible(&self, chunked_carriage_drilled: bool, economics: &PalwEconomicFactsV1) -> bool {
        if !max_leverage_holds_v1(&self.leverage_remedy, economics) || self.credited_ceiling_tokens == 0 {
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

    fn profile_with_libm(binds_libm: bool) -> PalwShapeProfileV3 {
        let node = |kind| PalwStepNodeV1 {
            op_kind: kind,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: String::new(),
            weight_dtype: 0,
            out_len: PalwStepOutLenV1::Fixed { elements: 16 },
            tile_len: 16,
            kernel_semantics_id: h64(0x11),
            input_refs: vec![PALW_STEP_INPUT_LAYER_IN],
        };
        let bindings = if binds_libm {
            vec![PalwTranscendentalBindingV1 { site: PalwTranscendentalSiteV1::LibmExpf, algorithm_id: h64(0x33) }]
        } else {
            vec![PalwTranscendentalBindingV1 { site: PalwTranscendentalSiteV1::VectorExpPolynomial, algorithm_id: h64(0x34) }]
        };
        PalwShapeProfileV3 {
            version: PALW_STEP_OBJECT_VERSION_V1,
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
        PalwClassRegistrationV1 {
            version: PALW_REGISTRY_OBJECT_VERSION_V1,
            label: "misaka-palw-lite-fp/x86-64-cpu/v1".into(),
            runtime_class_id: h64(0x01),
            runtime_manifest_hash: h64(0x02),
            model_profile_id: h64(0x03),
            tokenizer_id: h64(0x04),
            shape_profile: profile_with_libm(false),
            tap_semantics_id: h64(0x05),
            state_layout_id: h64(0x06),
            state_chunk_map_id: h64(0x44),
            tap_layer_indices: vec![0, 1, 2, 3],
            checkpoint_interval: 8,
            commitment_form: PalwCommitmentFormV1::CompositeV2,
            adjudication_depth: PalwAdjudicationDepthV1::ArithmeticCatalogued,
            libm_transcribed: true,
            replay_cost,
            credited_ceiling_tokens: ceiling,
            rho_v_permille: 1_000,
            p99_cold_replay_ms: 90_716,
            leverage_remedy: PalwLeverageRemedyV1 { min_credit_interval_daa: 10, base_subsidy_permille: 2 },
            windows,
            transcendental_algorithms: vec![(PalwTranscendentalSiteV1::VectorExpPolynomial, h64(0x34))],
        }
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
        reg.shape_profile = profile_with_libm(true);
        reg.transcendental_algorithms = vec![(PalwTranscendentalSiteV1::LibmExpf, h64(0x33))];
        reg.libm_transcribed = false;
        assert!(matches!(reg.validate(&two_minute_blockrate(), 120_000), Err(PalwRegistryError::NotCanonical(_))));
        // Registered honestly as structural-only, it validates.
        reg.adjudication_depth = PalwAdjudicationDepthV1::StructuralOnly;
        reg.validate(&two_minute_blockrate(), 120_000).unwrap();
    }

    #[test]
    fn arithmetic_depth_requires_the_step_leg_form() {
        let mut reg = fleet_registration();
        reg.commitment_form = PalwCommitmentFormV1::CompositeV1;
        assert!(matches!(reg.validate(&two_minute_blockrate(), 120_000), Err(PalwRegistryError::NotCanonical(_))));
        reg.adjudication_depth = PalwAdjudicationDepthV1::StructuralOnly;
        reg.validate(&two_minute_blockrate(), 120_000).unwrap();
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
        assert_ne!(mutate(&|r| r.windows.q = 3), base);
        assert_ne!(mutate(&|r| r.leverage_remedy.min_credit_interval_daa = 5_042), base);
        assert_ne!(mutate(&|r| r.leverage_remedy.base_subsidy_permille = 1), base);
    }
}
