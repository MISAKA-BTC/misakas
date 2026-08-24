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

/// **What a GGUF reports about itself, and everything a Family-M class needs to be one.**
///
/// ADR-0051's argument was that a second model in this family costs "a second set of hashes and
/// budgets — no arithmetic, no IR projection, no converter". That held for the hashes and failed
/// for the shape: `cat_m_0001_profile` read `qwen35_pins` directly, so the family that was meant to
/// accept any pinned GGUF accepted exactly one, and a second model needed a second hand-written
/// profile function beside it.
///
/// Every field here is read out of the GGUF's own metadata (`*.block_count`,
/// `*.embedding_length`, `*.feed_forward_length`, `*.attention.head_count`,
/// `*.attention.head_count_kv`, `tokenizer.ggml.tokens`), plus the context the runtime is built
/// at. Nothing is a judgement call, which is what makes two operators deriving the same class id
/// from the same file a fact rather than a coincidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GgufModelGeometryV1 {
    pub layer_count: u16,
    pub hidden_dim: u32,
    pub ffn_dim: u32,
    pub attn_heads: u16,
    pub attn_kv_heads: u16,
    pub vocab_size: u32,
    /// The context the runtime is BUILT at. The class declares the same, so a producer cannot
    /// quietly serve a longer one.
    pub n_ctx: u32,
    pub n_batch: u32,
    pub n_ubatch: u32,
    pub n_threads: u32,
}

impl GgufModelGeometryV1 {
    /// `attn_head_dim` is not independent — it is `hidden_dim / attn_heads`, and a pair that does
    /// not divide is a model this family cannot describe. Returned rather than stored so the two
    /// can never disagree inside one value.
    pub fn attn_head_dim(&self) -> Result<u32, String> {
        if self.attn_heads == 0 {
            return Err("a model with no attention heads is not one this family can pin".into());
        }
        if self.hidden_dim % self.attn_heads as u32 != 0 {
            return Err(format!(
                "hidden_dim {} is not divisible by attn_heads {} — the head dimension would be a rounding",
                self.hidden_dim, self.attn_heads
            ));
        }
        Ok(self.hidden_dim / self.attn_heads as u32)
    }

    /// The checks that must hold before a geometry can become a class id, so a malformed one fails
    /// where it is stated rather than at the admission gate on someone else's node.
    pub fn validate(&self) -> Result<(), String> {
        self.attn_head_dim()?;
        for (what, v) in [
            ("layer_count", self.layer_count as u32),
            ("hidden_dim", self.hidden_dim),
            ("ffn_dim", self.ffn_dim),
            ("vocab_size", self.vocab_size),
            ("n_ctx", self.n_ctx),
            ("n_batch", self.n_batch),
            ("n_ubatch", self.n_ubatch),
            ("n_threads", self.n_threads),
        ] {
            if v == 0 {
                return Err(format!("a Family-M geometry must state its {what}; zero is not a model"));
            }
        }
        if self.attn_kv_heads == 0 || self.attn_kv_heads > self.attn_heads {
            return Err(format!(
                "attn_kv_heads {} must be between 1 and attn_heads {}",
                self.attn_kv_heads, self.attn_heads
            ));
        }
        if self.n_ubatch > self.n_batch {
            return Err(format!("n_ubatch {} exceeds n_batch {}", self.n_ubatch, self.n_batch));
        }
        Ok(())
    }
}

/// The pinned Qwen3.5-2B geometry — CAT-M-0001's, and the one the fleet's worker is built for.
pub const CAT_M_0001_GEOMETRY: GgufModelGeometryV1 = GgufModelGeometryV1 {
    layer_count: kaspa_consensus_core::vlt::qwen35_pins::MODEL_LAYER_COUNT as u16,
    hidden_dim: kaspa_consensus_core::vlt::qwen35_pins::MODEL_HIDDEN_DIM,
    ffn_dim: 6144,
    attn_heads: 16,
    attn_kv_heads: 2,
    vocab_size: 248_320,
    n_ctx: CAT_M_0001_N_CTX,
    n_batch: 512,
    n_ubatch: 512,
    n_threads: 4,
};

/// **The shape half of the pinned identity, for ANY GGUF**: the geometry a loaded model must
/// report.
///
/// `lane = Float32` is not decoration. It is what `palw_step_refute`'s decode-token dispatch reads
/// to decide which trace scheme a class committed under, and the float lane's court arm is
/// **refused by name** — which is exactly right for a family that has no court.
///
/// Two models differing in any field here derive different class ids, which is the property that
/// stops one being run under the other's registration. That includes `ffn_dim`, which nothing in
/// this file reads: two models differing only in FFN width are two models.
pub fn family_m_profile_v1(g: &GgufModelGeometryV1) -> Result<PalwShapeProfileV3, String> {
    g.validate()?;
    Ok(PalwShapeProfileV3 {
        version: PALW_STEP_OBJECT_VERSION_V1,
        lane: PalwStepLaneV1::Float32,
        layer_count: g.layer_count,
        full_attention_interval: 1,
        hidden_dim: g.hidden_dim,
        ffn_dim: g.ffn_dim,
        attn_heads: g.attn_heads,
        attn_kv_heads: g.attn_kv_heads,
        attn_head_dim: g.attn_head_dim()?,
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
        vocab_size: g.vocab_size,
        repack_on: 0,
        llamafile_on: 0,
        flash_attn_disabled: 1,
        fused_gdn_on: 0,
        use_ref_off: 0,
        kv_cache_f16: 1,
        n_ctx: g.n_ctx,
        n_batch: g.n_batch,
        n_ubatch: g.n_ubatch,
        n_seq: 1,
        n_threads: g.n_threads,
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
    })
}

/// CAT-M-0001's profile: [`family_m_profile_v1`] at [`CAT_M_0001_GEOMETRY`]. Kept as a named
/// function because the class id it derives is a published fact, and infallible because its
/// geometry is a constant this crate's tests validate.
pub fn cat_m_0001_profile() -> PalwShapeProfileV3 {
    family_m_profile_v1(&CAT_M_0001_GEOMETRY).expect("CAT-M-0001's geometry is well-formed")
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

/// **The message a registrant signs**, re-exported so a caller does not have to know which of the
/// object's fields are covered — signing the wrong preimage is a registration the chain rejects
/// with a signature error, which reads like a key problem and is not one.
pub fn family_m_registration_message_v1(
    network_id: &[u8],
    class_id: Hash64,
    share_permille: u16,
    activation_daa: u64,
    registrant_bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
) -> Hash64 {
    kaspa_consensus_core::palw_state_v2::palw_class_registration_message_v2(
        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2(network_id),
        class_id,
        share_permille,
        activation_daa,
        registrant_bond,
    )
}

/// **A Family-M registration for a chain that is ALREADY RUNNING** (ADR-0049 Decision H).
///
/// [`cat_m_0001_registration`] builds the genesis form, whose `admission` is `None` because the
/// ruleset id already commits to a catalog that describes the class. A model added later has no
/// such entry: there is nothing on chain to check its graph, its canonical job or its `pwu` rule
/// against, so the object carries them and `verify_class_admission_v2` decides. Without this the
/// family that was designed to accept any pinned GGUF could only ever gain one by re-minting the
/// network — which is the flag day ADR-0051 existed to avoid.
///
/// Three things are NOT the caller's to choose, and are therefore not parameters:
///
/// * **the share** — a post-genesis entrant joins at the ruleset's minimum grantable share, and a
///   registrant naming its own permille would be donating itself a slice of every incumbent's
///   cadence. The validator refuses anything else; taking it as an argument would only move that
///   refusal to a node that is not the caller's.
/// * **the class id** — it is the profile's id in every family. A class IS its graph.
/// * **`pwu_per_inference`** — it is the canonical job's decode budget, counted. A declared value
///   is checked against the count, so the only thing a choice could do here is fail.
///
/// The signature is the caller's because the bond key is: this crate never sees key material.
#[allow(clippy::too_many_arguments)]
pub fn family_m_post_genesis_registration_v1(
    geometry: &GgufModelGeometryV1,
    pins: &MetalClassPinsV1,
    artifact_root: Hash64,
    panel_seats: u16,
    panel_quorum: u16,
    min_grantable_share_permille: u16,
    initial_target: u128,
    slash_value_per_pwu: u64,
    activation_daa: u64,
    registrant_bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    signature: Vec<u8>,
) -> Result<PalwConsensusObjectV2, String> {
    let profile = family_m_profile_v1(geometry)?;
    let class_id = profile.shape_profile_id();
    // The pins are the class's identity and the canonical job is the same identity priced; the
    // gate checks the two field by field, so they are built from ONE value here rather than
    // assembled twice by a caller who could get one of them wrong.
    if pins.shape_profile_id != class_id {
        return Err(format!(
            "the pins name shape profile {} but this geometry derives {class_id} — the worker is pinned to a different model",
            pins.shape_profile_id
        ));
    }
    let canonical = cat_m_0001_canonical_job(pins);
    Ok(PalwConsensusObjectV2::ClassRegistered {
        class_id,
        terms: PalwClassTermsV2 {
            family: PalwExecutionFamilyV1::MetalGguf,
            runtime_pins: Some(cat_m_0001_runtime_pins(pins)),
            panel_seats: Some(panel_seats),
            panel_quorum: Some(panel_quorum),
        },
        artifact_root,
        slash_value_per_pwu,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: pins.exact_decode_tokens as u64 },
        initial_target,
        share_permille: min_grantable_share_permille,
        activation_daa,
        admission: Some(Box::new(kaspa_consensus_core::palw_state_v2::PalwClassAdmissionCarriageV2 {
            profile,
            canonical,
            registrant_bond,
            signature,
        })),
    })
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
    /// A stand-in for "some other GGUF": Llama-3.2-3B's published shape. Nothing here is checked
    /// against a downloaded file — the point is that a DIFFERENT geometry travels the same path,
    /// not that these particular numbers are that model's.
    fn other_model_geometry() -> GgufModelGeometryV1 {
        GgufModelGeometryV1 {
            layer_count: 28,
            hidden_dim: 3072,
            ffn_dim: 8192,
            attn_heads: 24,
            attn_kv_heads: 8,
            vocab_size: 128_256,
            n_ctx: CAT_M_0001_N_CTX,
            n_batch: 512,
            n_ubatch: 512,
            n_threads: 4,
        }
    }

    /// **CAT-M-0001's class id is a published fact and must not move.**
    ///
    /// Generalising the profile over a geometry is only safe if the shipped class derives what it
    /// always did: the id is on chain, and a change to it is a class nobody registered.
    #[test]
    fn generalising_the_profile_did_not_move_the_shipped_class() {
        let from_geometry = family_m_profile_v1(&CAT_M_0001_GEOMETRY).expect("the shipped geometry is well-formed");
        assert_eq!(
            from_geometry.shape_profile_id(),
            cat_m_0001_profile().shape_profile_id(),
            "CAT-M-0001's id must be exactly what its geometry derives"
        );
        assert_eq!(from_geometry, cat_m_0001_profile(), "and the whole profile, not just its id");
    }

    /// **A different model is a different class.** The property the whole family rests on: if two
    /// models could derive one id, either could be run under the other's registration and the pins
    /// would be describing a model that was never loaded.
    #[test]
    fn a_second_model_derives_a_second_class() {
        let a = family_m_profile_v1(&CAT_M_0001_GEOMETRY).expect("shipped geometry");
        let b = family_m_profile_v1(&other_model_geometry()).expect("a second geometry");
        assert_ne!(a.shape_profile_id(), b.shape_profile_id(), "two models must not share a class id");

        // And every field of the identity actually participates — including `ffn_dim`, which
        // nothing in this module reads and which two otherwise-identical models can differ in.
        let mut only_ffn = CAT_M_0001_GEOMETRY;
        only_ffn.ffn_dim += 1;
        assert_ne!(
            family_m_profile_v1(&only_ffn).expect("still well-formed").shape_profile_id(),
            a.shape_profile_id(),
            "a model differing only in FFN width is a different model"
        );
    }

    /// **A geometry that cannot be a model is refused where it is stated**, not at the admission
    /// gate on somebody else's node.
    #[test]
    fn a_malformed_geometry_is_refused_before_it_becomes_a_class() {
        let mut g = CAT_M_0001_GEOMETRY;
        g.hidden_dim = CAT_M_0001_GEOMETRY.attn_heads as u32 * 7 + 1; // not divisible
        assert!(family_m_profile_v1(&g).is_err(), "a head dimension that is a rounding is not a class");

        let mut g = CAT_M_0001_GEOMETRY;
        g.attn_kv_heads = g.attn_heads + 1;
        assert!(family_m_profile_v1(&g).is_err(), "more kv heads than heads is not a model");

        let mut g = CAT_M_0001_GEOMETRY;
        g.vocab_size = 0;
        assert!(family_m_profile_v1(&g).is_err(), "a model with no vocabulary is not one");
    }

    /// **A second model is admitted by the chain's own gate**, carrying its own profile and job.
    ///
    /// This is the claim the goal rests on: adding Llama or Mistral to a RUNNING network costs a
    /// set of hashes and a signature, not a re-mint. It runs the same
    /// `verify_class_admission_v2` a node runs, on a registration built for a model that is not
    /// the shipped one.
    #[test]
    fn a_second_model_is_admitted_on_a_running_chain() {
        let g = other_model_geometry();
        let profile = family_m_profile_v1(&g).expect("a second geometry");
        let class_id = profile.shape_profile_id();

        // The node-local pins the worker would report for that model. Distinct values so a field
        // that failed to travel shows up as a mismatch rather than as a coincidence.
        let mut p = pins();
        p.shape_profile_id = class_id;
        p.vocab_size = g.vocab_size;
        p.max_context_tokens = g.n_ctx;

        let Some(mut bundle) = shipped_bundle() else { return };
        bundle.min_class_panel = (2, 2);
        let reg = family_m_post_genesis_registration_v1(
            &g,
            &p,
            Hash64::from_u64_word(0xA5A5_A5A5),
            2,
            2,
            bundle.state.min_grantable_share_permille(),
            u128::MAX / 2,
            5,
            0,
            kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_u64_word(7), 0)),
            vec![0u8; 8],
        )
        .expect("a well-formed second registration");

        let PalwConsensusObjectV2::ClassRegistered { admission, class_id: registered, .. } = &reg else {
            panic!("not a registration");
        };
        assert_eq!(*registered, class_id, "the object registers the class its geometry derives");
        let carriage = admission.as_ref().expect("a post-genesis registration carries its admission");

        kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v2(
            &bundle,
            &carriage.profile,
            &carriage.canonical,
            &reg,
        )
        .expect("a second model must be admissible on a running chain");
    }

    /// **The pins and the geometry are one statement.** A worker pinned to one model cannot be
    /// used to register another: the mismatch is caught where both are in hand, not by a gate on a
    /// node that would report it as a profile error.
    #[test]
    fn pins_from_a_different_model_are_refused() {
        let err = family_m_post_genesis_registration_v1(
            &other_model_geometry(),
            &pins(), // the SHIPPED model's pins
            Hash64::from_u64_word(1),
            2,
            2,
            1,
            u128::MAX / 2,
            5,
            0,
            kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_u64_word(7), 0)),
            vec![0u8; 8],
        )
        .expect_err("pins for one model must not register another");
        assert!(err.contains("pinned to a different model"), "the error must say which half disagrees: {err}");
    }

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
