//! Adversarial review (economic / DoS re-exploit lens) of 8e28aa17 (#5/#7/#8): the conviction the
//! reversal trusts is forgeable by a stranger.
//!
//! `PanelFalseValid` with an `ExecutorEquivocation` contradiction is verified by
//! `verify_palw_panel_false_valid_v1` under `payload.executor_pubkey` — a field of the EVIDENCE.
//! Nothing (the stateless verifier, `processor.rs`'s ObjectiveOffence arm, or the fold's
//! `bind_panel_false_valid` / `palw_panel_contradiction_convicts_execution_v1`) ties that key or the
//! carriage's `accused_bond_outpoint` to the claim's executor. So a fresh ML-DSA key the filer
//! generated signs two "contradicting" attestations for the victim claim's job id, and the honest
//! seat's PUBLIC Valid receipt (it is carried on chain in `ReceiptLicensed`) completes the evidence.
//!
//! Before 8e28aa17 that slashed an honest seat. Since it, the same object also voids the honest
//! claim's `Final` (safe_weight, probe pass, usage — `reverse_convicted_final`), forfeits its
//! execution root's receipt rights (`apply_receipt_spend`) and prunes its minted tickets
//! (`forfeit_minted_round_rights`). `dos_g2_conviction_takes_back` already folds that reversal
//! for an ExecutorEquivocation contradiction whose attestations are unsigned (the fold verifies
//! nothing); this test shows the acceptance layer's verifier passes a stranger-signed one with REAL
//! ML-DSA-87 signatures, using the processor's own verify routine (libcrux portable).
//!
//! **What this still asserts, after the fix (2026-09-24 review):** the STATELESS verifier cannot
//! know whose key `executor_pubkey` is, so it still passes the forgery — that is its contract, and
//! this test pins it so nobody mistakes it for the binding. The binding is state's: past
//! `palw_audit_2026_09_23` the processor's ObjectiveOffence arm and the fold's
//! `bind_panel_false_valid` require the carriage to accuse the claim's executor bond and the key to
//! be the one that bond registered. The forgery is refused there —
//! `dos_g2_conviction_takes_back::dos_g2_review_a_forged_equivocation_convicts_nobody_past_the_fence`
//! folds it through the real t12 fold.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_offence_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V1, PalwOffenceKindV1, PalwPanelContradictionV1, PalwPanelFalseValidEvidenceV1,
    palw_offence_evidence_digest_v1, palw_panel_contradiction_convicts_execution_v1, palw_verify_objective_offence_v1,
};
use kaspa_consensus_core::palw_panel_v2::{PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2};
use kaspa_consensus_core::palw_slash::{PALW_S_MLDSA87_ATTESTATION_CONTEXT, PALW_S_OBJECT_VERSION_V3, PalwExecutionAttestationV1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

fn verify(pk: &[u8], msg: &[u8], sig: &[u8], ctx: &[u8]) -> bool {
    // The processor's `verify_mldsa87_with_context` (crypto/txscript/src/lib.rs:141), inlined:
    // length checks, then libcrux's PORTABLE verify.
    use libcrux_ml_dsa::ml_dsa_87::{MLDSA87Signature, MLDSA87VerificationKey, portable};
    let (Ok(k), Ok(s)) = (<[u8; 2592]>::try_from(pk), <[u8; 4627]>::try_from(sig)) else { return false };
    portable::verify(&MLDSA87VerificationKey::new(k), msg, ctx, &MLDSA87Signature::new(s)).is_ok()
}

fn sign(kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair, msg: &[u8], ctx: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, msg, ctx, [7u8; 32]).expect("signs").as_ref().to_vec()
}

#[test]
fn review_economic_a_stranger_key_convicts_an_honest_valid_seat_of_any_claim() {
    use libcrux_ml_dsa::ml_dsa_87::generate_key_pair;
    // t12's network domain as the processor derives it is some 64-byte hash; any value works the
    // same way here because the verifier compares the certificate's network id to the one it is given.
    let domain = Hash64::from_u64_word(0x7E57_0012);
    let network_id = domain.as_byte_slice().to_vec();

    // An HONEST claim X, and an honest seat S that signed Valid on it — that receipt is public.
    let claim_x = Hash64::from_u64_word(0xC1A1_0000_0000_0001);
    let seat = TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0x5EA7), index: 0 };
    let seat_kp = generate_key_pair([1u8; 32]);
    let seat_pk: Vec<u8> = seat_kp.verification_key.as_ref().to_vec();
    let receipt_msg = palw_receipt_message_v2(domain, claim_x, PalwReceiptVerdictV2::Valid, 0);
    let honest_receipt = PalwSeatReceiptV2 {
        claim: claim_x,
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(seat),
        signed_daa: 0,
        signature: sign(&seat_kp, receipt_msg.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
    };

    // The STRANGER: a fresh key, no bond, never the executor of anything.
    let stranger = generate_key_pair([2u8; 32]);
    let stranger_pk: Vec<u8> = stranger.verification_key.as_ref().to_vec();
    let profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
        .expect("floor profile");
    let mut job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, 512, 256);
    job.job_id = claim_x; // the only binding the verifier asks for
    job.network_id = network_id.clone();
    // A well-formed context: the filer picks every field but the job id.
    job.max_context_tokens = job.max_context_tokens.max(job.declared_prefill_tokens + job.exact_decode_tokens);
    let attest = |root: u64| {
        let mut a = PalwExecutionAttestationV1 {
            version: PALW_S_OBJECT_VERSION_V3,
            executor_id: kaspa_consensus_core::mldsa87_primitives::mldsa87_key_id(&stranger_pk),
            job_context_hash: job.context_hash(),
            full_logits_trace_root: Hash64::from_u64_word(root),
            committed_root: Hash64::from_u64_word(root),
            bond_outpoint: seat,
            signature: Vec::new(),
        };
        let msg = a.message(&network_id);
        a.signature = sign(&stranger, &msg.as_bytes(), PALW_S_MLDSA87_ATTESTATION_CONTEXT);
        a
    };
    let carriage = kaspa_consensus_core::palw_carriage::PalwEquivocationCarriageV1 {
        version: kaspa_consensus_core::palw_carriage::PALW_CARRIAGE_VERSION_V1,
        accused_bond_outpoint: seat,
        certificate: kaspa_consensus_core::palw_slash::PalwClassContradictionCertificateV1 {
            version: PALW_S_OBJECT_VERSION_V3,
            job_context: job.clone(),
            attestation_a: attest(0xAA),
            attestation_b: attest(0xBB),
        },
    };
    let payload = PalwPanelFalseValidEvidenceV1 {
        version: PALW_PANEL_FALSE_VALID_VERSION_V1,
        claim_id: claim_x,
        network_domain: domain,
        accused_seat: seat,
        valid_receipt: honest_receipt,
        executor_pubkey: stranger_pk.clone(), // NOT the claim executor's key
        contradiction: PalwPanelContradictionV1::ExecutorEquivocation(carriage),
    };
    let evidence = borsh::to_vec(&payload).unwrap();
    let evidence_id = palw_offence_evidence_digest_v1(&evidence);

    // processor.rs ObjectiveOffence arm, step 1: the stateless verifier, with the ACCUSED SEAT's
    // registered key (record.pubkey) as the processor passes it.
    let verdict = palw_verify_objective_offence_v1(
        PalwOffenceKindV1::PanelFalseValid,
        &seat,
        &evidence_id,
        &evidence,
        &seat_pk,
        true,
        &network_id,
        1 << 26,
        verify,
    );
    println!("stateless verifier on a stranger-signed equivocation: {verdict:?}");
    assert!(verdict.is_ok(), "the forged conviction passes the acceptance layer's verifier: {verdict:?}");

    // Step 2: the root binding the processor and the fold both call. ExecutorEquivocation is not
    // bound to the claim's execution root at all — any root, any artifact.
    let honest_execution_root = Hash64::from_u64_word(0x4E00_0001);
    let bound = palw_panel_contradiction_convicts_execution_v1(&payload.contradiction, honest_execution_root, Hash64::default(), 64);
    assert!(bound.is_ok(), "{bound:?}");

    // Control: the same evidence with a tampered stranger signature is refused, so the verify
    // closure above is doing real work.
    let mut tampered = payload.clone();
    if let PalwPanelContradictionV1::ExecutorEquivocation(c) = &mut tampered.contradiction {
        c.certificate.attestation_a.signature[10] ^= 1;
    }
    let tampered_ev = borsh::to_vec(&tampered).unwrap();
    let refused = palw_verify_objective_offence_v1(
        PalwOffenceKindV1::PanelFalseValid,
        &seat,
        &palw_offence_evidence_digest_v1(&tampered_ev),
        &tampered_ev,
        &seat_pk,
        true,
        &network_id,
        1 << 26,
        verify,
    );
    assert!(refused.is_err(), "a bad signature is refused: {refused:?}");
}
