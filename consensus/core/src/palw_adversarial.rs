//! PALW adversarial suite — the §12 "adversarial test" gate item, as executable attacks.
//!
//! ADR-0027 v0.1 §29 gate 4 ("red-team the major shortcuts, partial execution, DA
//! withholding") and the v2 design §15 forbidden-claims list are the specification. Each test
//! below encodes ONE attack against the landed Layer-1 machinery and asserts the defense holds
//! — through public APIs only, so a regression that reopens an attack breaks a named test with
//! a sentence explaining what it lets through.
//!
//! This module has no non-test code: it is a permanent red-team harness that rides the crate's
//! own test run. New attacks are added here, never removed; a defense that must change is a
//! change to the module it defends, and this suite is what proves the change did not reopen an
//! old door.

#![cfg(test)]

use kaspa_hashes::Hash64;

use crate::palw_reference::{
    ref_add_v1, ref_dot_v1, ref_fma_v2, ref_mul_v1, ref_sqrt_v2, reference_arithmetic_ruleset_id_v1,
    reference_arithmetic_ruleset_id_v2,
};
use crate::palw_step::{
    canonical_step_coordinates, canonical_step_leaf_index, kernel_semantics_id_v1, transcendental_algorithm_id_v1,
    PalwLayerKindV1, PalwShapeProfileV3, PalwStepCoordinateV1, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1,
    PalwStepOutLenV1, PALW_STEP_INPUT_LAYER_IN, PALW_STEP_OBJECT_VERSION_V1,
};
use crate::palw_transcendental::{ggml_v_expf_v1, ggml_v_silu_v1, glibc_expf_v1};

fn h64(fill: u8) -> Hash64 {
    Hash64::from_bytes([fill; 64])
}

// =============================================================================================
// Attack 1 — reduction reassociation ("my hardware sums in a different order")
// =============================================================================================

/// The whole class mechanism exists to stop an honest node with a different reduction order
/// from computing a different logit and refuting an honest miner. The reference must NOT be
/// order-invariant, or it would bless both orders and the class would be meaningless. Prove
/// the dot's ascending order produces a value a reassociating order does not.
#[test]
fn attack_reduction_reassociation_is_caught_by_a_non_associative_reference() {
    // [2^24, 1, 1] · [1,1,1]: ascending stays 2^24 (both +1s tie to even), a "sum the small
    // ones first" order gives 2^24 + 2. The reference commits to exactly one.
    let a = [0x4B80_0000u32, 0x3F80_0000, 0x3F80_0000];
    let ones = [0x3F80_0000u32; 3];
    let ascending = ref_dot_v1(&a, &ones).unwrap();
    // A reassociating adversary: (a[1]+a[2]) first, then + a[0].
    let small_first = ref_add_v1(ref_add_v1(ref_mul_v1(a[1], ones[1]), ref_mul_v1(a[2], ones[2])), ref_mul_v1(a[0], ones[0]));
    assert_ne!(ascending, small_first, "the reference is order-invariant — the class mechanism is a no-op");
    assert_eq!(ascending, 0x4B80_0000);
    assert_eq!(small_first, 0x4B80_0001);
}

// =============================================================================================
// Attack 2 — FMA contraction smuggling ("I fused a multiply-add the reference splits")
// =============================================================================================

/// A miner whose compiler contracted `a*b+c` into one rounding computes a different value than
/// the v1 no-FMA rule. The reference's v1 (split) and v2 (fused) MUST disagree on a witness,
/// or a class could not distinguish the two and an honest fused executor would be refutable by
/// an honest split one under the same ruleset id.
#[test]
fn attack_fma_contraction_is_distinguished_by_the_ruleset_id() {
    let a = (1.0f32 + f32::from_bits(0x3480_0000)).to_bits();
    // Addend = −round(a·a): the split path self-cancels to exactly +0, the fused path keeps
    // the sub-ulp residual the single rounding preserves. This is where contraction is visible.
    let product = ref_mul_v1(a, a);
    let neg_product = product ^ 0x8000_0000;
    let split = ref_add_v1(product, neg_product); // = +0 exactly
    let fused = ref_fma_v2(a, a, neg_product); // = the residual, ≠ 0
    assert_eq!(split, 0, "the split witness should self-cancel");
    assert_ne!(split, fused, "fma and mul-then-add agree on the witness — contraction is invisible");
    // And the two rulesets have distinct ids, so a profile cannot claim one arithmetic and use
    // the other.
    assert_ne!(reference_arithmetic_ruleset_id_v1(), reference_arithmetic_ruleset_id_v2());
}

// =============================================================================================
// Attack 3 — transcendental substitution ("I used a faster exp")
// =============================================================================================

/// A miner who swaps the pinned exp algorithm for another (a different polynomial, or a libm
/// with a different last bit) diverges. The catalog's three exp identities must be distinct,
/// and the vector polynomial must differ from glibc's on some input, or "which exp" would not
/// be a bindable, refutable fact.
#[test]
fn attack_transcendental_substitution_diverges_and_is_id_separated() {
    // The vector polynomial and glibc's table-driven expf are different algorithms: they agree
    // to ~1 ulp but not everywhere. Find one input where they differ.
    let mut differs_at = None;
    let mut x = -10.0f32;
    while x < 10.0 {
        if ggml_v_expf_v1(x.to_bits()) != glibc_expf_v1(x.to_bits(), true) {
            differs_at = Some(x);
            break;
        }
        x += 0.001;
    }
    assert!(differs_at.is_some(), "two distinct exp algorithms never disagree — 'which exp' is not observable");
    // The three exp identities are distinct strings ⇒ distinct ids.
    let v = transcendental_algorithm_id_v1("source-poly/ggml-v-expf/llama-030ebb558/per-lane/v1");
    let g_fma = transcendental_algorithm_id_v1("libm/glibc-2.39/expf/fma/v1");
    let g_nofma = transcendental_algorithm_id_v1("libm/glibc-2.39/expf/nofma/v1");
    assert_ne!(v, g_fma);
    assert_ne!(g_fma, g_nofma, "the two contraction variants must be separately bindable");
    assert_ne!(v, g_nofma);
    // SiLU is the exp composed with a divide: a substitution in the inner exp propagates.
    assert_ne!(ggml_v_silu_v1(0x3FC0_0000), 0x3FC0_0000);
}

// =============================================================================================
// Attack 4 — manufactured mismatch ("I open unrelated honest tiles as this step's inputs")
// =============================================================================================
//
// This is the attack the input_refs wiring exists to stop. It is verified end-to-end in
// `palw_step_refute::tests::wrong_input_set_is_rejected_not_convicted`; here we assert the
// STRUCTURAL precondition that makes that defense sound: the canonical input leaf-index of a
// step is a pure function of (profile, context, coordinates), so a challenger cannot choose it.

fn attack_profile() -> PalwShapeProfileV3 {
    let mk = |kind, refs: Vec<u16>| PalwStepNodeV1 {
        op_kind: kind,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: String::new(),
        weight_dtype: 0,
        out_len: PalwStepOutLenV1::Fixed { elements: 16 },
        tile_len: 16,
        kernel_semantics_id: h64(0x11),
        input_refs: refs,
    };
    PalwShapeProfileV3 {
        version: PALW_STEP_OBJECT_VERSION_V1,
        layer_count: 2,
        full_attention_interval: 2,
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
        pre_nodes: vec![mk(PalwStepOpKindV1::EmbedLookup, vec![])],
        gdn_nodes: vec![mk(PalwStepOpKindV1::RmsNorm, vec![PALW_STEP_INPUT_LAYER_IN]), mk(PalwStepOpKindV1::GatedDeltaNet, vec![0])],
        attn_nodes: vec![mk(PalwStepOpKindV1::RmsNorm, vec![PALW_STEP_INPUT_LAYER_IN])],
        post_nodes: vec![mk(PalwStepOpKindV1::MatMulQuant, vec![PALW_STEP_INPUT_LAYER_IN])],
        reference_ruleset_id: h64(0x22),
        transcendental_bindings: vec![],
        contraction_facts: vec![],
        kv_chunk_calls: 0,
        state_chunk_map_id: h64(0x44),
    }
}

fn attack_context() -> crate::palw_v2::PalwJobContextV2 {
    let mut ctx = crate::palw_v2::PalwJobContextV2 {
        version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"adversarial".to_vec(),
        job_id: h64(1),
        job_nullifier: h64(2),
        assignment_id: h64(3),
        execution_seed: [7; 32],
        model_profile_id: h64(4),
        runtime_manifest_hash: h64(5),
        runtime_class_id: h64(6),
        shape_profile_id: h64(7),
        trace_scheme_id: h64(8),
        cu_ruleset_id: h64(9),
        tokenizer_id: h64(10),
        prompt_token_ids_hash: h64(11),
        declared_prefill_tokens: 2,
        exact_decode_tokens: 2,
        max_context_tokens: 64,
    };
    ctx.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
    ctx
}

#[test]
fn attack_input_leaf_index_is_not_challenger_choosable() {
    let p = attack_profile();
    let ctx = attack_context();
    // The same coordinates always rank to the same leaf — the challenger cannot re-target.
    let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 1, position: 0, tile_index: 0 };
    let idx = canonical_step_leaf_index(&p, &ctx, &coord).unwrap();
    assert_eq!(canonical_step_leaf_index(&p, &ctx, &coord).unwrap(), idx, "leaf index is not a pure function");
    // And it is the inverse of the enumeration — no gaps, no aliases to exploit.
    assert_eq!(canonical_step_coordinates(&p, &ctx, idx).unwrap(), coord);
}

// =============================================================================================
// Attack 5 — cross-class collusion ("I registered a second class that agrees with my forgery")
// =============================================================================================

/// A class is defined by conformance to the reference, not pairwise agreement (ADR-0027 §2).
/// Two profiles that differ in ANY reduction-order-affecting field must get different ids, so
/// a colluding pair cannot present as one class. Attack every such field.
#[test]
fn attack_two_profiles_differing_in_any_order_fact_cannot_share_an_id() {
    let base = attack_profile().shape_profile_id();
    let mutate = |f: &dyn Fn(&mut PalwShapeProfileV3)| {
        let mut p = attack_profile();
        f(&mut p);
        p.shape_profile_id()
    };
    // Kernel semantics (the reduction order itself).
    assert_ne!(mutate(&|p| p.gdn_nodes[1].kernel_semantics_id = h64(0x99)), base);
    // Thread count (a cross-thread reduction hazard the profile pins).
    assert_ne!(mutate(&|p| p.n_threads = 8), base);
    // The previously-unpinned build flags.
    assert_ne!(mutate(&|p| p.repack_on = 0), base);
    assert_ne!(mutate(&|p| p.llamafile_on = 0), base);
    assert_ne!(mutate(&|p| p.fused_gdn_on = 0), base);
    // Tile geometry.
    assert_ne!(mutate(&|p| p.gdn_nodes[1].tile_len = 8), base);
    // Contraction facts (measured per class).
    assert_ne!(
        mutate(&|p| p.contraction_facts.push(crate::palw_step::PalwContractionFactV1 {
            site: crate::palw_step::PalwContractionSiteV1::RopeRotate,
            contracted: 1
        })),
        base
    );
    // The arithmetic itself.
    assert_ne!(mutate(&|p| p.reference_ruleset_id = h64(0x88)), base);
    // Geometry bits.
    assert_ne!(mutate(&|p| p.rope_freq_base_bits ^= 1), base);
}

// =============================================================================================
// Attack 6 — flash-attention re-enable ("I turned the fast attention path back on")
// =============================================================================================

/// The pinned graph disables flash attention because its enabled form reintroduces a
/// KV-axis cross-thread float reduction that makes logits thread-count-dependent (ADR-0030
/// Fact 7). A profile MUST refuse to validate with flash attention enabled, or a miner could
/// register a class whose numerics are not a function of (source, thread count) alone.
#[test]
fn attack_flash_attention_reenable_is_refused_at_validation() {
    let mut p = attack_profile();
    p.flash_attn_disabled = 0;
    assert!(p.validate_shape().is_err(), "a flash-attention-enabled profile validated — the thread-count hazard is admissible");
    // And such a profile's id differs from the honest one, so it cannot masquerade.
    assert_ne!(p.shape_profile_id(), attack_profile().shape_profile_id());
}

// =============================================================================================
// Attack 7 — non-finite smuggling ("I committed a NaN/Inf where the check does not look")
// =============================================================================================

/// The fail-closed rule says an honest execution aborts rather than commit a non-finite value,
/// and any committed non-finite value is a refutable fault. The reference arithmetic must make
/// non-finite results TOTAL and canonical (not a platform artifact), so the check sees exactly
/// the same bits everywhere.
#[test]
fn attack_non_finite_results_are_canonical_not_platform_dependent() {
    // Overflow to a canonical +Inf, not a signaling pattern.
    let big = 0x7F7F_FFFFu32; // max finite
    let overflow = ref_mul_v1(big, 0x4000_0000); // × 2 → +Inf
    assert_eq!(overflow, 0x7F80_0000);
    // sqrt of a negative is THE canonical NaN, not any NaN.
    assert_eq!(ref_sqrt_v2(0xBF80_0000), 0x7FC0_0000);
    // fma 0×Inf is the canonical NaN regardless of the addend.
    assert_eq!(ref_fma_v2(0, 0x7F80_0000, 0x3F80_0000), 0x7FC0_0000);
    // A payload NaN never survives an operation — it canonicalizes.
    assert_eq!(ref_add_v1(0x7FC0_1234, 0x3F80_0000), 0x7FC0_0000);
}

// =============================================================================================
// Attack 8 — domain-key bridge ("I reused a leaf hash across families as another object")
// =============================================================================================

/// A preimage hashed under one family's domain must never validate under another's — a shared
/// domain string would let a committed object of one kind be replayed as another. The
/// per-family uniqueness tests assert this within each module; here we assert the CROSS-family
/// property directly over the whole domain set, the way an attacker would search for a bridge.
#[test]
fn attack_no_domain_string_bridges_two_families() {
    use crate::palw_bisect::PALW_BISECT_ALL_DOMAINS;
    use crate::palw_carriage::PALW_CARRIAGE_ALL_DOMAINS;
    use crate::palw_legs::PALW_LEGS_ALL_DOMAINS;
    use crate::palw_reference::PALW_REFERENCE_ALL_DOMAINS;
    use crate::palw_schedule::PALW_SCHEDULE_ALL_DOMAINS;
    use crate::palw_slash::PALW_S_ALL_DOMAINS;
    use crate::palw_step::PALW_STEP_ALL_DOMAINS;
    use crate::palw_step_leg::PALW_STEP_LEG_ALL_DOMAINS;
    use crate::palw_v2::PALW_V2_ALL_DOMAINS;

    let mut seen = std::collections::HashMap::<&[u8], &str>::new();
    let families: &[(&str, &[&[u8]])] = &[
        ("v2", PALW_V2_ALL_DOMAINS),
        ("slash", PALW_S_ALL_DOMAINS),
        ("legs", PALW_LEGS_ALL_DOMAINS),
        ("reference", PALW_REFERENCE_ALL_DOMAINS),
        ("schedule", PALW_SCHEDULE_ALL_DOMAINS),
        ("carriage", PALW_CARRIAGE_ALL_DOMAINS),
        ("step", PALW_STEP_ALL_DOMAINS),
        ("step_leg", PALW_STEP_LEG_ALL_DOMAINS),
        ("bisect", PALW_BISECT_ALL_DOMAINS),
    ];
    for (fam, domains) in families {
        for d in *domains {
            if let Some(prev) = seen.insert(d, fam) {
                panic!("domain {:?} bridges families {} and {}", String::from_utf8_lossy(d), prev, fam);
            }
            assert!(d.len() <= 64, "domain over the blake2b key cap: {:?}", String::from_utf8_lossy(d));
        }
    }
    // The whole PALW surface: a healthy count of distinct domains, no collisions.
    assert!(seen.len() > 50, "suspiciously few domains — a family list may be empty");
}

// =============================================================================================
// Attack 9 — kernel-id guessing ("I claim a kernel program that is not in the catalog")
// =============================================================================================

/// A refutation whose step names a kernel program the adjudicator does not have MUST be
/// unadjudicable (nobody slashed), not guessed. The id is a hash of a descriptor string, so
/// an attacker cannot forge an id that resolves to a program of their choosing.
#[test]
fn attack_forged_kernel_id_does_not_resolve() {
    // A random id resolves to nothing; only exact catalog descriptors do.
    let forged = h64(0xAB);
    let real = kernel_semantics_id_v1("l2-norm/whole-row/double-sum-ascending/llama-030ebb558/v1");
    assert_ne!(forged, real);
    // A near-miss descriptor (one char off) gives a completely different id — no partial match.
    let near = kernel_semantics_id_v1("l2-norm/whole-row/double-sum-ascending/llama-030ebb559/v1");
    assert_ne!(near, real, "a near-miss descriptor collided — ids are not preimage-bound");
}

// =============================================================================================
// Attack 10 — layer-kind confusion ("I mislabel a GDN layer as attention to dodge the GDN check")
// =============================================================================================

/// Which layers are GDN vs attention follows from `full_attention_interval` by a pinned rule
/// (ADR-0030 Fact 1), not from a per-layer flag a miner sets. A profile cannot relabel a layer
/// to route its steps through a different (weaker) kernel table.
#[test]
fn attack_layer_kind_is_derived_not_declared() {
    let mut p = attack_profile();
    p.layer_count = 24;
    p.full_attention_interval = 4;
    // The rule is fixed: attention exactly at 3,7,11,15,19,23.
    let attn: Vec<u16> = (0..24).filter(|&l| p.layer_kind(l) == PalwLayerKindV1::Attention).collect();
    assert_eq!(attn, vec![3, 7, 11, 15, 19, 23]);
    // Changing the interval changes the id (so it is not a free relabel).
    let base = p.shape_profile_id();
    p.full_attention_interval = 6;
    assert_ne!(p.shape_profile_id(), base);
}
