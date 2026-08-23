//! **CAT-M-0001 — the Metal/GGUF family's first class** (ADR-0051 step 5).
//!
//! # A class is still its graph, even when the graph is a black box
//!
//! `class_id` is `PalwShapeProfileV3::shape_profile_id()` in every family (ADR-0049 Decision G),
//! and Family M does not get an exception: what changes is what the profile *means*. For the
//! deterministic floor the profile is a node table a court walks. Here nothing walks it — the
//! runtime is opaque — so the profile serves as the **shape half of the pinned identity**: the
//! geometry a loaded GGUF must report, beside the hashes that pin which GGUF, which tokenizer and
//! which runtime build. A profile that disagrees with the loaded model is the wrong model, and the
//! worker refuses to run before it can commit to anything.
//!
//! The node tables are therefore empty, deliberately. An empty table in Family D would be a class
//! with nothing to adjudicate — a real defect, and `verify_profile_coverage_v1` would say so.
//! Here it is the honest statement that this family adjudicates no steps at all, and the admission
//! gate for a non-court family never asks (ADR-0051 step 4).
//!
//! # What "registering a model" costs after this
//!
//! Everything in [`cat_m_0001`] except the pins is shared. A second Family-M class is a second set
//! of hashes and budgets — no arithmetic, no IR projection, no converter, no re-quantized
//! artifact. That was the whole argument of ADR-0051, and this module is where it either holds or
//! does not.

use crate::MetalClassPinsV1;
use kaspa_consensus_core::palw_backend::PalwExecutionFamilyV1;
use kaspa_consensus_core::palw_state_v2::{PalwClassTermsV2, PalwConsensusObjectV2, PalwPwuRuleV2};
use kaspa_consensus_core::palw_step::{PALW_STEP_OBJECT_VERSION_V1, PalwShapeProfileV3, PalwStepLaneV1};
use kaspa_consensus_core::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, prompt_token_ids_hash_v2};
use kaspa_hashes::Hash64;
use std::path::PathBuf;

/// The canonical job CAT-M-0001 is paid per: eight prompt tokens in, four generated.
///
/// Small on purpose. It is the unit of work the class's `pwu_per_inference` counts and the unit a
/// SEAT re-runs — and this family's seat rule is full replay (ADR-0051 Decision 4 as revised), so
/// the canonical job's cost is paid by every seat on every claim. Measured: 3.0 s.
pub const CAT_M_0001_CANONICAL: (u32, u32) = (8, 4);

/// The class's context. The runtime is built at `n_ctx = 4096`; the class declares the same, so a
/// job inside the class is a job inside the runtime.
pub const CAT_M_0001_N_CTX: u32 = 4096;

/// The shape half of the pinned identity: the geometry a loaded GGUF must report.
///
/// `lane = Float32` is not decoration. It is what `palw_step_refute`'s decode-token dispatch reads
/// to decide which trace scheme a class committed under, and the float lane's court arm is
/// **refused by name** — which is exactly right for a family that has no court.
pub fn cat_m_0001_profile() -> PalwShapeProfileV3 {
    let mut p = PalwShapeProfileV3 {
        version: PALW_STEP_OBJECT_VERSION_V1,
        lane: PalwStepLaneV1::Float32,
        layer_count: kaspa_consensus_core::vlt::qwen35_pins::MODEL_LAYER_COUNT as u16,
        full_attention_interval: 1,
        hidden_dim: kaspa_consensus_core::vlt::qwen35_pins::MODEL_HIDDEN_DIM,
        ffn_dim: 0,
        attn_heads: 16,
        attn_kv_heads: 2,
        attn_head_dim: kaspa_consensus_core::vlt::qwen35_pins::MODEL_HIDDEN_DIM / 16,
        rope_dims: 0,
        rope_sections: [0; 4],
        rope_freq_base_bits: 0,
        rms_eps_bits: 0,
        base0_rms_eps_q: 0,
        l2_eps_bits: 0,
        gdn_heads: 0,
        gdn_head_k_dim: 0,
        gdn_head_v_dim: 0,
        gdn_conv_kernel: 0,
        vocab_size: 248_320,
        repack_on: 0,
        llamafile_on: 0,
        flash_attn_disabled: 1,
        fused_gdn_on: 0,
        use_ref_off: 0,
        kv_cache_f16: 1,
        n_ctx: CAT_M_0001_N_CTX,
        n_batch: 512,
        n_ubatch: 512,
        n_seq: 1,
        n_threads: 4,
        // **Empty by construction.** No court walks this graph; see the module docs.
        pre_nodes: Vec::new(),
        gdn_nodes: Vec::new(),
        attn_nodes: Vec::new(),
        post_nodes: Vec::new(),
        // The adjudication apparatus, empty for the same reason the node tables are: a family
        // with no court binds no transcendentals, states no contraction facts and maps no state
        // chunks, because nothing recomputes a step to compare against them.
        reference_ruleset_id: Hash64::default(),
        transcendental_bindings: Vec::new(),
        contraction_facts: Vec::new(),
        kv_chunk_calls: 0,
        state_chunk_map_id: Hash64::default(),
    };
    // ffn_dim is part of the identity even though nothing here reads it: two models differing only
    // in FFN width are two models, and a class id that could not tell them apart would let one be
    // run under the other's registration.
    p.ffn_dim = 6144;
    p
}

/// The canonical job context, from the class's pins.
///
/// The prompt is a fixed one here — the class's *definition* needs a job to count `pwu` over, and
/// that job is a yardstick rather than an execution. A producer's actual job is derived from its
/// template anchor ([`crate::MetalBackend::job_for_anchor`]), which is what stops an executor
/// choosing an input whose output it likes.
pub fn cat_m_0001_canonical_job(pins: &MetalClassPinsV1) -> PalwJobContextV2 {
    let prompt: Vec<u32> = (0..CAT_M_0001_CANONICAL.0).collect();
    PalwJobContextV2 {
        version: PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: pins.network_id.clone(),
        job_id: Hash64::default(),
        job_nullifier: Hash64::default(),
        assignment_id: Hash64::default(),
        execution_seed: [0; 32],
        model_profile_id: pins.model_profile_id,
        runtime_manifest_hash: pins.runtime_manifest_hash,
        runtime_class_id: pins.runtime_class_id,
        shape_profile_id: cat_m_0001_profile().shape_profile_id(),
        trace_scheme_id: pins.trace_scheme_id,
        cu_ruleset_id: pins.cu_ruleset_id,
        tokenizer_id: pins.tokenizer_id,
        prompt_token_ids_hash: prompt_token_ids_hash_v2(&prompt),
        declared_prefill_tokens: CAT_M_0001_CANONICAL.0,
        exact_decode_tokens: CAT_M_0001_CANONICAL.1,
        max_context_tokens: CAT_M_0001_N_CTX,
    }
}

/// The pins CAT-M-0001 registers under, given a worker path and the identities that worker
/// reports. Everything but `worker_path` is a consensus value.
pub fn cat_m_0001_pins(
    worker_path: PathBuf,
    network_id: Vec<u8>,
    runtime_manifest_hash: Hash64,
    runtime_class_id: Hash64,
    model_profile_id: Hash64,
    shape_profile_id: Hash64,
    trace_scheme_id: Hash64,
    cu_ruleset_id: Hash64,
    tokenizer_id: Hash64,
) -> MetalClassPinsV1 {
    MetalClassPinsV1 {
        model_id: kaspa_consensus_core::vlt::qwen35_pins::BASE_REPO_ID.to_string(),
        worker_path,
        runtime_manifest_hash,
        model_profile_id,
        runtime_class_id,
        shape_profile_id,
        trace_scheme_id,
        cu_ruleset_id,
        tokenizer_id,
        prefill_tokens: CAT_M_0001_CANONICAL.0,
        exact_decode_tokens: CAT_M_0001_CANONICAL.1,
        max_context_tokens: CAT_M_0001_N_CTX,
        vocab_size: 248_320,
        network_id,
    }
}

/// **The registration object a chain accepts** (ADR-0051 Decisions 1, 2 and 6).
///
/// `share_permille` is the caller's: the family is capped at 500‰ in total and a first entrant
/// should take the minimum grantable share and grow, not claim the cap.
pub fn cat_m_0001_registration(
    pins: &MetalClassPinsV1,
    panel_seats: u16,
    panel_quorum: u16,
    share_permille: u16,
    initial_target: u128,
    slash_value_per_pwu: u64,
    activation_daa: u64,
) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ClassRegistered {
        class_id: cat_m_0001_profile().shape_profile_id(),
        terms: PalwClassTermsV2 {
            family: PalwExecutionFamilyV1::MetalGguf,
            runtime_pins: Some(cat_m_0001_runtime_pins(pins)),
            panel_seats: Some(panel_seats),
            panel_quorum: Some(panel_quorum),
        },
        // The GGUF's own digest IS the artifact root for this family: there is no operand
        // inventory to Merkleise, because no opening is ever proved against one.
        artifact_root: gguf_artifact_root_v1(),
        slash_value_per_pwu,
        // Counted from the canonical job by the gate, so this must BE the decode budget.
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: CAT_M_0001_CANONICAL.1 as u64 },
        initial_target,
        share_permille,
        activation_daa,
        admission: None,
    }
}

/// **The on-chain half of the pins.** The node-local [`MetalClassPinsV1`] adds a worker path,
/// which is a local fact; everything else is consensus and lives here so a node can check its own
/// worker against what the CHAIN registered rather than against itself.
pub fn cat_m_0001_runtime_pins(pins: &MetalClassPinsV1) -> kaspa_consensus_core::palw_state_v2::PalwRuntimePinsV2 {
    kaspa_consensus_core::palw_state_v2::PalwRuntimePinsV2 {
        runtime_manifest_hash: pins.runtime_manifest_hash,
        runtime_class_id: pins.runtime_class_id,
        model_profile_id: pins.model_profile_id,
        trace_scheme_id: pins.trace_scheme_id,
        cu_ruleset_id: pins.cu_ruleset_id,
        tokenizer_id: pins.tokenizer_id,
        prefill_tokens: pins.prefill_tokens,
        exact_decode_tokens: pins.exact_decode_tokens,
        max_context_tokens: pins.max_context_tokens,
        vocab_size: pins.vocab_size,
    }
}

/// The inverse: node-local pins from what the chain holds, plus this node's worker.
pub fn pins_from_chain(
    worker_path: PathBuf,
    network_id: Vec<u8>,
    on_chain: &kaspa_consensus_core::palw_state_v2::PalwRuntimePinsV2,
) -> MetalClassPinsV1 {
    MetalClassPinsV1 {
        model_id: kaspa_consensus_core::vlt::qwen35_pins::BASE_REPO_ID.to_string(),
        worker_path,
        runtime_manifest_hash: on_chain.runtime_manifest_hash,
        model_profile_id: on_chain.model_profile_id,
        runtime_class_id: on_chain.runtime_class_id,
        shape_profile_id: cat_m_0001_profile().shape_profile_id(),
        trace_scheme_id: on_chain.trace_scheme_id,
        cu_ruleset_id: on_chain.cu_ruleset_id,
        tokenizer_id: on_chain.tokenizer_id,
        prefill_tokens: on_chain.prefill_tokens,
        exact_decode_tokens: on_chain.exact_decode_tokens,
        max_context_tokens: on_chain.max_context_tokens,
        vocab_size: on_chain.vocab_size,
        network_id,
    }
}

/// `artifact_root` for a Family-M class: a domain-separated hash of the pinned GGUF's digest and
/// size.
///
/// Not the raw sha256 re-labelled — a bare hash reused across two meanings is how one value comes
/// to be checked by the wrong rule — and not a Merkle root either, because there is nothing to
/// open. It answers the one question the chain asks of an artifact root here: *are two producers
/// running the same weights?*
pub fn gguf_artifact_root_v1() -> Hash64 {
    let pins = kaspa_consensus_core::vlt::qwen35_pins::GGUF_SHA256;
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/metal/artifact-root/v1").to_state();
    h.update(pins.as_bytes());
    h.update(&kaspa_consensus_core::vlt::qwen35_pins::GGUF_SIZE.to_le_bytes());
    h.update(kaspa_consensus_core::vlt::qwen35_pins::GGUF_FILENAME.as_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pins() -> MetalClassPinsV1 {
        cat_m_0001_pins(
            PathBuf::from("/worker"),
            b"misaka-palw-rc".to_vec(),
            Hash64::from_u64_word(0x11),
            Hash64::from_u64_word(0x22),
            Hash64::from_u64_word(0x33),
            cat_m_0001_profile().shape_profile_id(),
            Hash64::from_u64_word(0x55),
            Hash64::from_u64_word(0x66),
            Hash64::from_u64_word(0x77),
        )
    }

    /// **The class is registrable**: the profile validates, the id is the profile's, and the job
    /// the registration is priced over is inside the class's own context.
    #[test]
    fn cat_m_0001_is_a_well_formed_class() {
        let p = cat_m_0001_profile();
        p.validate_geometry().expect("the class's GEOMETRY is well-formed — the node tables are empty by design");
        assert!(
            p.validate_shape().is_err(),
            "and the adjudicable-graph rules refuse it, which is what makes the family split real rather than nominal"
        );
        assert_eq!(p.lane, PalwStepLaneV1::Float32, "the float lane is what has no decode-token court arm");
        assert!(p.attn_nodes.is_empty(), "no court walks this graph, and saying so is the point");

        let job = cat_m_0001_canonical_job(&pins());
        assert_eq!(job.shape_profile_id, p.shape_profile_id(), "the job names the class it prices");
        assert!(job.declared_prefill_tokens + job.exact_decode_tokens <= job.max_context_tokens);
        assert!(job.max_context_tokens <= p.n_ctx);
    }

    /// The registration declares the family, a thin panel, and a pwu that IS the decode budget —
    /// the three things ADR-0051's gate recomputes rather than believes.
    #[test]
    fn the_registration_declares_what_the_gate_recomputes() {
        let reg = cat_m_0001_registration(&pins(), 2, 2, 1, u128::MAX / 2, 5, 0);
        let PalwConsensusObjectV2::ClassRegistered { class_id, terms, pwu_rule, artifact_root, .. } = &reg else {
            panic!("not a registration")
        };
        assert_eq!(*class_id, cat_m_0001_profile().shape_profile_id());
        assert_eq!(terms.family, PalwExecutionFamilyV1::MetalGguf);
        assert!(!terms.family.is_court_adjudicable());
        assert_eq!(terms.panel_seats, Some(2));
        assert_eq!(*pwu_rule, PalwPwuRuleV2::DerivedV1 { pwu_per_inference: CAT_M_0001_CANONICAL.1 as u64 });
        assert_ne!(*artifact_root, Hash64::default(), "an unset artifact root is refused by the gate");
    }

    /// **The artifact root is the GGUF's identity and nothing else's.** A different digest, size
    /// or filename is a different class — and it is domain-separated from the raw sha256 so the
    /// two cannot be confused for one another somewhere downstream.
    #[test]
    fn the_artifact_root_is_the_pinned_gguf() {
        let root = gguf_artifact_root_v1();
        assert_ne!(root, Hash64::default());
        let raw = kaspa_consensus_core::vlt::qwen35_pins::GGUF_SHA256;
        assert!(!format!("{root}").starts_with(raw), "the root must not be the bare sha256 wearing a new name");
        assert_eq!(root, gguf_artifact_root_v1(), "and it is a pure function of the pins");
    }

    /// **CAT-M-0001 passes the chain's own gate.** Not a restatement of the gate's rules — the
    /// real `verify_class_admission_v2`, against a bundle that admits thin panels, with the class
    /// this module builds. If this fails, the family cannot be registered, whatever else works.
    #[test]
    fn cat_m_0001_is_admitted_by_the_chain() {
        use kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v2;
        let Some(mut bundle) = shipped_bundle() else { return };
        // ADR-0051 Decision 6: a network that means to carry this family states the floor its
        // classes may thin down to. Without one, `min_class_panel` is (0,0) and any per-class
        // panel is refused — which is the right default for a chain that never opted in.
        bundle.min_class_panel = (2, 2);

        let pins = pins();
        let profile = cat_m_0001_profile();
        let job = cat_m_0001_canonical_job(&pins);
        let reg = cat_m_0001_registration(&pins, 2, 2, 1, u128::MAX / 2, 5, 0);

        let entry = verify_class_admission_v2(&bundle, &profile, &job, &reg)
            .unwrap_or_else(|e| panic!("CAT-M-0001 is not admissible: {e}"));
        assert_eq!(entry.class_id, profile.shape_profile_id());
        assert_eq!(entry.canonical_step_leaf_count, CAT_M_0001_CANONICAL.1 as u64, "pwu is the decode budget");
        assert_eq!(entry.artifact_root, gguf_artifact_root_v1());
        println!("CAT-M-0001 ADMITTED  class_id={}  pwu={}", entry.class_id, entry.canonical_step_leaf_count);
    }

    /// **The shipped floor, measured, and what it costs in operators.**
    ///
    /// Read twice, minutes apart, this once answered `(2, 2)` then `(0, 0)`, and the conclusion
    /// drawn — that the first read caught a half-written file — was wrong. Both reads were real:
    /// `(0, 0)` was the shipped value, and `(2, 2)` was another session declaring the floor so
    /// Family M could be registered at all. The lesson kept: a value read from a tree two people
    /// are editing is a measurement of a moment, and the way to make it a fact is to pin it in a
    /// test — which is what this is.
    ///
    /// The shipped value is now **`(2, 2)`**, declared in testnet-11's genesis because it lives
    /// inside `palw_ruleset_id_v2` and is the one thing about Family M that cannot arrive later by
    /// transaction. The distinction is the whole operator budget of a family:
    ///
    /// * the network panel is **5 seats, quorum 3**, and `derive_panel_v2` REFUSES a short draw
    ///   (`InsufficientEligibleBonds`) rather than seating fewer — so a class on the network panel
    ///   needs five seats *plus* the executor, and one seat per operator, which is **6 distinct
    ///   operators**;
    /// * a class may thin to the declared floor, and `(2, 2)` is **3 distinct operators** — which
    ///   for Family M is three Apple Silicon machines rather than seven. That is the difference
    ///   between a lane that can be opened by acquiring hardware and one that cannot be opened at
    ///   all without a re-mint.
    ///
    /// For Family M every one of those operators must hold Apple Silicon, because a seat verifies
    /// by re-running the job. That is the number this test exists to keep honest.
    #[test]
    fn the_shipped_floor_and_what_it_costs_in_operators() {
        let Some(bundle) = shipped_bundle() else { return };
        assert_eq!(bundle.min_class_panel, (2, 2), "the shipped RC declares a per-class panel floor of 2/2");
        assert_eq!((bundle.panel.seat_count(), bundle.panel.quorum()), (5, 3), "the network panel");

        // At the declared floor a class may draw its own panel; below it, still refused. The
        // second row is what keeps the floor a floor: a registrant does not get to pick 1/1
        // because it owns fewer machines.
        let pins = pins();
        for (seats, quorum, admissible) in [(2u16, 2u16, true), (1, 1, false)] {
            let reg = cat_m_0001_registration(&pins, seats, quorum, 1, u128::MAX / 2, 5, 0);
            let got = kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v2(
                &bundle,
                &cat_m_0001_profile(),
                &cat_m_0001_canonical_job(&pins),
                &reg,
            );
            assert_eq!(got.is_ok(), admissible, "a {seats}/{quorum} panel: admissible={admissible}, got {got:?}");
        }
    }

    fn shipped_bundle() -> Option<kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2> {
        match &kaspa_consensus_core::config::params::palw_rc_shipped_params().palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => Some(b.clone()),
            _ => None,
        }
    }

}
