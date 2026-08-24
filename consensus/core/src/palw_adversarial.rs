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
    PALW_STEP_INPUT_LAYER_IN, PALW_STEP_OBJECT_VERSION_V1, PalwLayerKindV1, PalwShapeProfileV3, PalwStepCoordinateV1,
    PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1, canonical_step_coordinates, canonical_step_leaf_index,
    kernel_semantics_id_v1, transcendental_algorithm_id_v1,
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
        weight_dtypes: Vec::new(),
        out_len: PalwStepOutLenV1::Fixed { elements: 16 },
        tile_len: 16,
        kernel_semantics_id: h64(0x11),
        input_refs: refs,
    };
    PalwShapeProfileV3 {
        version: PALW_STEP_OBJECT_VERSION_V1,
        lane: crate::palw_step::PalwStepLaneV1::Float32,
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
        base0_rms_eps_q: 1 << 8,
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
        gpu_offload_layers: 0,
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

// =============================================================================================
// Attack 11 — routing-key relabeling ("my receipt claims a cheaper band / a different family")
// =============================================================================================

/// ADR-0034 §5: the registry, not the miner, gives carried keys meaning. A receipt declaring
/// a lower band (dodging band-indexed bond floors) or a foreign family (fishing for a panel
/// that cannot re-execute it) is invalid at acceptance — and a registry row resolved by a
/// colliding lookup is refused by the recomputed id, the exact bug class that once emptied
/// every committee silently.
#[test]
fn attack_routing_key_relabeling_is_invalid_at_acceptance() {
    use crate::palw_registry::tests::fleet_registration;
    use crate::palw_routing::{
        PalwBindingCoverageStateV1, PalwExecutionFamilyV1, PalwModelBandV1, PalwRoutingError, validate_receipt_routing_keys_v1,
    };
    let row = fleet_registration();
    let id = row.registration_id();
    // Honest keys pass…
    validate_receipt_routing_keys_v1(&id, PalwExecutionFamilyV1::Cpu, PalwModelBandV1::B0, &row, PalwBindingCoverageStateV1::Active)
        .unwrap();
    // …the down-banded claim is refused as forgery, not "corrected"…
    assert!(matches!(
        validate_receipt_routing_keys_v1(
            &id,
            PalwExecutionFamilyV1::Cpu,
            PalwModelBandV1::B1,
            &row,
            PalwBindingCoverageStateV1::Active
        ),
        Err(PalwRoutingError::BandForged { .. })
    ));
    // …a family relabel is refused (cross-family comparison stays diagnostic, never a verdict:
    // this module offers no API that could slash on it)…
    assert!(matches!(
        validate_receipt_routing_keys_v1(
            &id,
            PalwExecutionFamilyV1::Metal,
            PalwModelBandV1::B0,
            &row,
            PalwBindingCoverageStateV1::Active
        ),
        Err(PalwRoutingError::FamilyMismatch { .. })
    ));
    // …and a wrong-row resolution is caught by the recomputed id even with matching coarse keys.
    assert!(matches!(
        validate_receipt_routing_keys_v1(
            &h64(0x5C),
            PalwExecutionFamilyV1::Cpu,
            PalwModelBandV1::B0,
            &row,
            PalwBindingCoverageStateV1::Active
        ),
        Err(PalwRoutingError::BindingIdMismatch)
    ));
}

// =============================================================================================
// Attack 12 — ready-set forgery ("I claim readiness for a model I do not hold")
// =============================================================================================

/// ADR-0034 §6: a ready claim without a proof is not a claim. The three forgeries that would
/// let an unequipped verifier take duties (and no-show or stall them): riding another
/// binding's proof, grafting an internal node as a leaf, and lying about the tree geometry.
#[test]
fn attack_ready_set_forgery_cannot_claim_an_unheld_binding() {
    use crate::palw_routing::{ready_binding_proof_v1, ready_binding_root_v1, verify_ready_binding_v1};
    let held: Vec<Hash64> = (1..=4u8).map(h64).collect();
    let root = ready_binding_root_v1(&held).unwrap();
    let proof_of_first = ready_binding_proof_v1(&held, 0).unwrap();
    // The binding the verifier does NOT hold cannot ride any held binding's proof.
    let unheld = h64(0x66);
    assert!(!verify_ready_binding_v1(&root, &unheld, &proof_of_first));
    // Nor can a re-indexed proof, nor a geometry lie, manufacture membership.
    for index in 0..4u32 {
        for count in [1u32, 3, 4, 5] {
            let mut forged = proof_of_first.clone();
            forged.leaf_index = index;
            forged.leaf_count = count;
            assert!(!verify_ready_binding_v1(&root, &unheld, &forged), "forged geometry ({index},{count}) admitted an unheld binding");
        }
    }
    // The honest claim still stands (the defense rejects forgery, not readiness).
    assert!(verify_ready_binding_v1(&root, &held[0], &proof_of_first));
}

// =============================================================================================
// Attack 13 — unchecked credit ("nobody could replay it, so credit it" / FINALIZED_WITHOUT_REPLAY)
// =============================================================================================

/// ADR-0034 §7 rejects the draft's lottery-to-credit path outright: no rule of the form "not
/// drawn, therefore creditable unchecked" may exist. The routing module exposes no crediting
/// API at all, so the only door to credit is ADR-0033's `decide_credit_v1` — which at Stage 0
/// derives its own ADR-0028 class panel and does not yet read the routed lottery (that
/// substitution is the Stage-1 wiring, ADR-0028 §2 as amended). What this attack pins is the
/// door itself: with nobody eligible and zero attestations, the §1 predicate credits nothing —
/// there is no quorum-shrinking, no lottery-miss pass, and nothing a routing draw could say
/// that would mint against an unattested commitment.
#[test]
fn attack_finalized_without_replay_is_untypable() {
    use crate::palw_credit::{PalwCreditParamsV1, PalwObservedCommitmentV1, decide_credit_v1};
    use crate::palw_registry::tests::fleet_registration;
    let registration = fleet_registration();
    let params = PalwCreditParamsV1 {
        registration: registration.clone(),
        s_eff_sompi: 20_000 * 100_000_000,
        unbonding_period_blocks: 10_083,
        activation_daa: 0,
        class_daa: crate::palw_class_daa::PalwClassDaaParamsV1::stage1_defaults(),
    };
    let commitment = PalwObservedCommitmentV1 {
        committed_root: h64(0x01),
        logits_root: h64(0x02),
        executor_id: h64(0x03),
        runtime_class_id: registration.runtime_class_id,
        accepted_daa: 1_000,
    };
    let subsidy = 370_468_345 * 1_200;
    // No eligible verifier existed; the window closed with zero attestations. The §1
    // predicate credits nothing — the lottery having "missed" the job is never a pass.
    let decision = decide_credit_v1(&params, &commitment, b"misaka-adversarial", h64(0x05), &h64(0x04), 1_010, &[], &[], &[], subsidy);
    assert!(!decision.creditable && decision.paid_attesters.is_empty(), "an unchecked job was credited");
    assert_eq!(decision.base_sompi, 0, "an uncreditable job carries no mint");
}
