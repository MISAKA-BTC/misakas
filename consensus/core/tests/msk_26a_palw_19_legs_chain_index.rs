//! MSK-26A-PALW-19 (regression, flipped from the audit PoC) -- check_legs_refutation_v1 added 1 to
//! an attacker-chosen u32 CheckpointChain leaf index before either opening was verified against the
//! committed tree, so carried evidence with `earlier_opening.leaf_index == u32::MAX` panicked
//! ("attempt to add with overflow"). The release profile sets `overflow-checks = true` and kaspad's
//! panic hook (core/src/panic.rs) exits the process, so on a network where the V1 `PanelFalseValid`
//! gate is live (testnet-11/RC from DAA 8,500) one carried object would stop every validating node.
//!
//! Audit PoC: origin/audit-q6vnqe:docs/security/audits/2026-09-claude-pre-freeze/poc/MSK-26A-PALW-19.rs
//! (commit 3d2bd6dc; reproduced on the testnet-12 release 0e8ec984e, both panic tests passed there).
//!
//! Now:
//!   * test 1: the same evidence is refused as `ChainEvidenceNotAdjacent { earlier: u32::MAX, .. }`;
//!   * test 2: the processor's stateless V1 gate refuses it (`PanelFalseValidNeedsContradiction`),
//!     no panic;
//!   * test 3: unchanged -- where the V1 gate is live on the shipped parameter sets.
//!
//! The sibling on the SAME gate (verification of the fix, 2026-09-26): a `StepStructural`
//! contradiction whose binding claims `step_leaf_count = u64::MAX`. The structural checker's shape
//! pass counts the job at the binding's own claim as the cap, before any opening is verified; a
//! main enumeration that clamps to `u64::MAX` clears that cap, and `step_leaf_count_capped_v1`
//! then added the KV aux series with a plain `+` -- "attempt to add with overflow" at
//! palw_step.rs:1471 on 0e8ec984e. Tests 4 and 5 pin that it now answers instead of panicking.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_legs::{
    PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_LEAF, PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_NODE, PALW_LEGS_MAX_CHECKPOINTS,
    PALW_LEGS_OBJECT_VERSION_V1, PalwActivationTapProfileV1, PalwCheckpointProfileV1, PalwLegOpeningV1, PalwLegsBindingV1,
    PalwLegsCommitmentBuilderV1, PalwLegsError, PalwLegsEvidenceV1, PalwLegsMaterial, PalwLegsRefutationV1, check_legs_refutation_v1,
    leg_opening_v1,
};
use kaspa_consensus_core::palw_offence_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V1, PalwOffenceKindV1, PalwOffenceVerifyError, PalwPanelContradictionV1,
    PalwPanelFalseValidEvidenceV1, palw_offence_evidence_digest_v1, palw_verify_objective_offence_v1,
};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, PalwLogitsDtypeV2, trace_scheme_id_v2};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn h64(seed: u8) -> Hash64 {
    Hash64::from_bytes([seed; 64])
}

/// An arbitrary job context the attacker picks (it need not be any real claim's).
fn attacker_context() -> PalwJobContextV2 {
    PalwJobContextV2 {
        version: PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"misaka-testnet-11".to_vec(),
        job_id: h64(0x11),
        job_nullifier: h64(0x12),
        assignment_id: h64(0x13),
        execution_seed: [0x22; 32],
        model_profile_id: h64(0x31),
        runtime_manifest_hash: h64(0x32),
        runtime_class_id: h64(0x33),
        shape_profile_id: h64(0x34),
        trace_scheme_id: trace_scheme_id_v2(),
        cu_ruleset_id: h64(0x36),
        tokenizer_id: h64(0x37),
        prompt_token_ids_hash: h64(0x38),
        // P = 3, D = 5 => 4 decode calls; interval 2 => 2 checkpoints.
        declared_prefill_tokens: 3,
        exact_decode_tokens: 5,
        max_context_tokens: 64,
    }
}

/// A self-consistent binding with NO shape faults, built through the public honest builder.
fn honest_binding() -> (PalwLegsBindingV1, PalwLegsMaterial) {
    let context = attacker_context();
    let taps = PalwActivationTapProfileV1 {
        version: PALW_LEGS_OBJECT_VERSION_V1,
        tap_semantics_id: h64(0x41),
        tap_layer_indices: vec![8, 16],
        model_total_layers: 28,
        hidden_dim: 4,
        dtype: PalwLogitsDtypeV2::F32Le,
    };
    let ckpt = PalwCheckpointProfileV1 { version: PALW_LEGS_OBJECT_VERSION_V1, checkpoint_interval: 2, state_layout_id: h64(0x51) };
    let mut b = PalwLegsCommitmentBuilderV1::new(context.clone(), taps.clone(), ckpt.clone()).expect("builder");
    // Activation rows in canonical order: prefill (tap-major, then position), then decode calls.
    for tap in 0..taps.tap_count() {
        for pos in 0..context.declared_prefill_tokens {
            b.push_activation_row(0, tap, pos, &[1.0, 2.0, 3.0, 4.0]).expect("prefill row");
        }
    }
    for call in 1..context.exact_decode_tokens {
        for tap in 0..taps.tap_count() {
            b.push_activation_row(call, tap, 0, &[0.5, 0.25, 0.125, 1.5]).expect("decode row");
        }
    }
    assert_eq!(b.expected_checkpoint_count(), 2);
    b.push_checkpoint(2, h64(0x61)).expect("checkpoint 0");
    b.push_checkpoint(4, h64(0x62)).expect("checkpoint 1");
    b.finish_with_material(h64(0x71)).expect("finish")
}

fn ckpt_opening(material: &PalwLegsMaterial, index: u32) -> PalwLegOpeningV1 {
    leg_opening_v1(
        PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_LEAF,
        PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_NODE,
        &material.checkpoint_leaf_hashes,
        index,
        PALW_LEGS_MAX_CHECKPOINTS,
    )
    .expect("opening")
}

/// CheckpointChain evidence whose earlier index is u32::MAX (nothing else needs to be valid).
fn overflowing_refutation() -> PalwLegsRefutationV1 {
    let (binding, material) = honest_binding();
    let mut earlier = ckpt_opening(&material, 0);
    earlier.leaf_index = u32::MAX; // attacker-chosen, never bounded by checkpoint_count before the add
    let later = ckpt_opening(&material, 1);
    PalwLegsRefutationV1 {
        binding,
        evidence: PalwLegsEvidenceV1::CheckpointChain {
            earlier_opening: earlier,
            earlier_preimage: material.checkpoint_leaves[0].clone(),
            later_opening: later,
            later_preimage: material.checkpoint_leaves[1].clone(),
        },
    }
}

fn panic_message(p: Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default()
}

#[test]
fn msk_26a_palw_19_checkpoint_chain_u32_max_index_is_refused_not_a_panic() {
    // Controls: the honest binding verifies and is shape-clean (Shape evidence -> NoFaultFound), and a
    // non-adjacent pair with SMALL indices is refused cleanly -- the adjacency guard is the only thing hit.
    let (binding, material) = honest_binding();
    assert_eq!(
        check_legs_refutation_v1(&PalwLegsRefutationV1 { binding: binding.clone(), evidence: PalwLegsEvidenceV1::Shape }),
        Err(PalwLegsError::NoFaultFound),
        "binding must be self-consistent and free of shape faults"
    );
    let clean = PalwLegsRefutationV1 {
        binding,
        evidence: PalwLegsEvidenceV1::CheckpointChain {
            earlier_opening: ckpt_opening(&material, 1),
            earlier_preimage: material.checkpoint_leaves[1].clone(),
            later_opening: ckpt_opening(&material, 1),
            later_preimage: material.checkpoint_leaves[1].clone(),
        },
    };
    assert_eq!(check_legs_refutation_v1(&clean), Err(PalwLegsError::ChainEvidenceNotAdjacent { earlier: 1, later: 1 }));

    // The attack: earlier_opening.leaf_index = u32::MAX. Refused by name, no panic.
    let refutation = overflowing_refutation();
    let outcome = catch_unwind(AssertUnwindSafe(|| check_legs_refutation_v1(&refutation)));
    match outcome {
        Err(p) => panic!("check_legs_refutation_v1 panicked on a u32::MAX chain index: {:?}", panic_message(p)),
        Ok(r) => assert_eq!(r, Err(PalwLegsError::ChainEvidenceNotAdjacent { earlier: u32::MAX, later: 1 })),
    }
}

// ---- the processor's V1 PanelFalseValid gate (processor.rs:9737) ---------------------------------

fn verify(pk: &[u8], msg: &[u8], sig: &[u8], ctx: &[u8]) -> bool {
    // processor.rs `verify_mldsa87_with_context_bool` -> crypto/txscript/src/lib.rs:141, inlined:
    // length checks, then libcrux's PORTABLE verify.
    use libcrux_ml_dsa::ml_dsa_87::{MLDSA87Signature, MLDSA87VerificationKey, portable};
    let (Ok(k), Ok(s)) = (<[u8; 2592]>::try_from(pk), <[u8; 4627]>::try_from(sig)) else { return false };
    portable::verify(&MLDSA87VerificationKey::new(k), msg, ctx, &MLDSA87Signature::new(s)).is_ok()
}

fn sign(kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair, msg: &[u8], ctx: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, msg, ctx, [7u8; 32]).expect("signs").as_ref().to_vec()
}

#[test]
fn msk_26a_palw_19_v1_panel_false_valid_gate_refuses_without_panicking() {
    use libcrux_ml_dsa::ml_dsa_87::generate_key_pair;
    let domain = Hash64::from_u64_word(0x7E57_0011);
    // Any claim a seat signed Valid on; deliberately NOT the binding's job id (0x11..).
    let claim = Hash64::from_u64_word(0xC1A1_0000_0000_0042);
    let seat = TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0x5EA7), index: 0 };
    let seat_kp = generate_key_pair([3u8; 32]);
    let seat_pk: Vec<u8> = seat_kp.verification_key.as_ref().to_vec();
    let msg = palw_receipt_message_v2(domain, claim, PalwReceiptVerdictV2::Valid, 0);
    let receipt = PalwSeatReceiptV2 {
        claim,
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: PalwBondKeyV2(seat),
        signed_daa: 0,
        signature: sign(&seat_kp, msg.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
    };
    assert!(verify(&seat_pk, msg.as_byte_slice(), &receipt.signature, PALW_RECEIPT_V2_MLDSA87_CONTEXT), "genuine receipt");

    let refutation = overflowing_refutation();
    assert_ne!(refutation.binding.job_context.job_id, claim, "the binding belongs to no real claim");
    let payload = PalwPanelFalseValidEvidenceV1 {
        version: PALW_PANEL_FALSE_VALID_VERSION_V1,
        claim_id: claim,
        network_domain: domain,
        accused_seat: seat,
        valid_receipt: receipt,
        executor_pubkey: vec![0u8; 4],
        contradiction: PalwPanelContradictionV1::Legs(refutation),
    };
    let evidence = borsh::to_vec(&payload).expect("serializes");
    let evidence_id = palw_offence_evidence_digest_v1(&evidence);
    println!("V1 PanelFalseValid evidence: {} bytes", evidence.len());

    // Control: the same payload with a SMALL non-adjacent pair is refused, not accepted.
    let mut control = payload.clone();
    if let PalwPanelContradictionV1::Legs(r) = &mut control.contradiction
        && let PalwLegsEvidenceV1::CheckpointChain { earlier_opening, .. } = &mut r.evidence
    {
        earlier_opening.leaf_index = 5;
    }
    let control_bytes = borsh::to_vec(&control).unwrap();
    let control_verdict = palw_verify_objective_offence_v1(
        PalwOffenceKindV1::PanelFalseValid,
        &seat,
        &palw_offence_evidence_digest_v1(&control_bytes),
        &control_bytes,
        &seat_pk,
        true,
        domain.as_byte_slice(),
        1 << 22,
        verify,
    );
    println!("control (earlier index 5): {control_verdict:?}");
    assert!(control_verdict.is_err());

    // The attack: the processor's stateless gate, exactly as processor.rs:9737 calls it.
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        palw_verify_objective_offence_v1(
            PalwOffenceKindV1::PanelFalseValid,
            &seat,
            &evidence_id,
            &evidence,
            &seat_pk,
            true,
            domain.as_byte_slice(),
            1 << 22,
            verify,
        )
    }));
    match outcome {
        Err(p) => panic!("palw_verify_objective_offence_v1 panicked on a u32::MAX chain index: {:?}", panic_message(p)),
        Ok(r) => assert_eq!(r, Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)),
    }
}

// ---- where the V1 gate is live (runtime, from the shipped constructors) ---------------------------

#[test]
fn msk_26a_palw_19_reachability_on_shipped_params() {
    use kaspa_consensus_core::config::params::{mainnet_shipped_params, palw_rc_shipped_params, palw_t12_shipped_params};
    let probe = [0u64, 1_000, 8_500, 1_000_000, u64::MAX - 1];
    let v1_route_live = |p: &kaspa_consensus_core::config::params::Params, d: u64| {
        p.palw_objective_offence_at(d) && !p.palw_offence_attribution.is_some_and(|f| f.is_active(d))
    };
    let t12 = palw_t12_shipped_params();
    let main = mainnet_shipped_params();
    let rc = palw_rc_shipped_params();
    for (name, p) in [("testnet-12", &t12), ("mainnet", &main), ("testnet-11/rc", &rc)] {
        println!(
            "{name:14} objective_offence={:?} attribution={:?} v1_route_live@{:?}={:?}",
            p.palw_objective_offence.map(|f| f.daa_score()),
            p.palw_offence_attribution.map(|f| f.daa_score()),
            probe,
            probe.iter().map(|d| v1_route_live(p, *d)).collect::<Vec<_>>()
        );
    }
    assert!(probe.iter().all(|d| !v1_route_live(&t12, *d)), "testnet-12: V1 PanelFalseValid superseded from genesis");
    assert!(probe.iter().all(|d| !main.palw_objective_offence_at(*d)), "mainnet preset: objective offences not armed");
    assert!(v1_route_live(&rc, 8_500) && !v1_route_live(&rc, 8_499), "testnet-11/RC: V1 gate live from DAA 8,500");
}

// ---- the sibling: a StepStructural contradiction whose job saturates the leaf count --------------

/// A self-consistent step binding claiming `u64::MAX` leaves for a job whose main enumeration
/// clamps there, on a profile WITH a KV aux series (the add that overflowed). Built by hand from the
/// RC BASE-0 profile: the honest builder refuses such a job, the attacker does not use it.
fn saturating_step_refutation() -> kaspa_consensus_core::palw_step_leg::PalwStepRefutationV1 {
    use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    use kaspa_consensus_core::palw_step::{PalwStepOpKindV1, PalwStepOutLenV1, kv_aux_leaf_count, step_leaf_count_capped_v1};
    use kaspa_consensus_core::palw_step_leg::{
        PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2, PalwStepEvidenceV1, PalwStepRefutationV1, binding_commitment_root_v1,
        checkpoint_empty_root_v2,
    };

    let mut profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the RC BASE-0 profile");
    profile.n_ctx = 1 << 21; // 4 layers x 2^21 = 2^23, under the 2^24 enumeration ceiling
    profile.n_batch = profile.n_ctx;
    profile.n_ubatch = profile.n_ctx;
    profile.kv_chunk_calls = 1; // a non-empty aux series: the add the clamp used to overflow
    for node in profile.attn_nodes.iter_mut().filter(|n| n.op_kind != PalwStepOpKindV1::AttnFused) {
        node.out_len = PalwStepOutLenV1::KvScaled { multiplier: u32::MAX };
    }
    profile.validate_shape().expect("the attacker's profile is inside every declared ceiling");

    let mut context = attacker_context();
    context.shape_profile_id = profile.shape_profile_id();
    context.declared_prefill_tokens = 1 << 20;
    context.exact_decode_tokens = 2;
    context.max_context_tokens = (1 << 20) + 2;
    assert!(kv_aux_leaf_count(&profile, &context) > 0);
    let mut without_aux = profile.clone();
    without_aux.kv_chunk_calls = 0;
    assert_eq!(step_leaf_count_capped_v1(&without_aux, &context, u64::MAX), Ok(u64::MAX), "the main enumeration clamps");

    let checkpoint_profile = PalwCheckpointProfileV1 {
        version: PALW_LEGS_OBJECT_VERSION_V1,
        checkpoint_interval: 1,
        state_layout_id: kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_layout_id_v1(),
    };
    let checkpoint_count = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_count_v1(
        &profile,
        &context,
        checkpoint_profile.checkpoint_interval,
    );
    let checkpoint_merkle_root = if checkpoint_count == 0 { checkpoint_empty_root_v2(&context.context_hash()) } else { h64(0x81) };
    let mut binding = PalwStepBindingV2 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        job_context: context,
        state_chunk_map_id: profile.state_chunk_map_id,
        shape_profile: profile,
        checkpoint_profile,
        full_logits_trace_root: h64(0x82),
        activation_leg_root: h64(0x83),
        step_leaf_count: u64::MAX, // attacker-chosen: it is also the cap the shape pass counts at
        step_merkle_root: h64(0x84),
        checkpoint_count,
        checkpoint_merkle_root,
        committed_execution_root: Hash64::default(),
    };
    binding.committed_execution_root = binding_commitment_root_v1(&binding);
    PalwStepRefutationV1 { binding, evidence: PalwStepEvidenceV1::Shape }
}

#[test]
fn msk_26a_palw_19_sibling_saturating_step_leaf_count_is_answered_not_a_panic() {
    use kaspa_consensus_core::palw_step_leg::{PalwStepLegError, check_step_refutation_capped_v1};
    let refutation = saturating_step_refutation();
    // At the RC court's ladder, exactly as the V1 gate passes it (the cap only bounds openings).
    let outcome = catch_unwind(AssertUnwindSafe(|| check_step_refutation_capped_v1(&refutation, 1 << 22)));
    match outcome {
        Err(p) => panic!("check_step_refutation_capped_v1 panicked on a saturating job with an aux series: {:?}", panic_message(p)),
        // The clamped count equals the claim, as it already did on the shipped binary for a job with
        // no aux series; nothing else in the binding is at fault, so Shape evidence finds nothing.
        Ok(r) => assert_eq!(r, Err(PalwStepLegError::NoFaultFound)),
    }
}

#[test]
fn msk_26a_palw_19_sibling_v1_gate_refuses_a_saturating_step_structural_without_panicking() {
    use libcrux_ml_dsa::ml_dsa_87::generate_key_pair;
    let domain = Hash64::from_u64_word(0x7E57_0011);
    let claim = Hash64::from_u64_word(0xC1A1_0000_0000_0042);
    let seat = TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0x5EA7), index: 0 };
    let seat_kp = generate_key_pair([3u8; 32]);
    let seat_pk: Vec<u8> = seat_kp.verification_key.as_ref().to_vec();
    let msg = palw_receipt_message_v2(domain, claim, PalwReceiptVerdictV2::Valid, 0);
    let receipt = PalwSeatReceiptV2 {
        claim,
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: PalwBondKeyV2(seat),
        signed_daa: 0,
        signature: sign(&seat_kp, msg.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
    };
    assert!(verify(&seat_pk, msg.as_byte_slice(), &receipt.signature, PALW_RECEIPT_V2_MLDSA87_CONTEXT), "genuine receipt");

    let refutation = saturating_step_refutation();
    assert_ne!(refutation.binding.job_context.job_id, claim, "the binding belongs to no real claim");
    let payload = PalwPanelFalseValidEvidenceV1 {
        version: PALW_PANEL_FALSE_VALID_VERSION_V1,
        claim_id: claim,
        network_domain: domain,
        accused_seat: seat,
        valid_receipt: receipt,
        executor_pubkey: vec![0u8; 4],
        contradiction: PalwPanelContradictionV1::StepStructural(refutation),
    };
    let evidence = borsh::to_vec(&payload).expect("serializes");
    let evidence_id = palw_offence_evidence_digest_v1(&evidence);
    println!("V1 PanelFalseValid/StepStructural evidence: {} bytes", evidence.len());

    let outcome = catch_unwind(AssertUnwindSafe(|| {
        palw_verify_objective_offence_v1(
            PalwOffenceKindV1::PanelFalseValid,
            &seat,
            &evidence_id,
            &evidence,
            &seat_pk,
            true,
            domain.as_byte_slice(),
            1 << 22,
            verify,
        )
    }));
    match outcome {
        Err(p) => panic!("palw_verify_objective_offence_v1 panicked on a saturating StepStructural: {:?}", panic_message(p)),
        Ok(r) => assert_eq!(r, Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)),
    }
}
