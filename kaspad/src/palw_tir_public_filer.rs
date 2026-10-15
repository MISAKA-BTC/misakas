//! **G14 R4: the common public-conviction filing of an ORIGINAL TIR frame, node side** (node policy; nothing here is a consensus
//! rule and nothing runs below `palw_tir_public_conviction_v1`, where the consensus reader answers `None`).
//!
//! One bond — a drawn Panel seat that signed no Valid, or a bond outside the Panel — turns a LOCAL finding over authenticated
//! public material (the arithmetic scheduler's `Ready` cone/logits primitive, an EOF-authenticated ID sequence, or a greedy
//! decode-token pin) into the one signed object the fold adjudicates. The pre-checks mirror the fold's rules so a node never pays
//! a conviction rent the fold would refuse for a reason it could have read; they are never a substitute for the fold's verdict,
//! and a primitive that HOLDS is never filed (it would be refused, and it certifies nothing).
//!
//! Remaining node integration (recorded in docs/design/palw/g14-objective-conviction-remaining-2026-10-15.md): the scheduler that
//! decides WHEN to file, its queue lane and restart journal, and funding the carrier — this module only builds and signs.
#![allow(dead_code)] // wired by the filer scheduler once its queue lane lands; exercised by the tests below.

use kaspa_consensus_core::{
    Hash64,
    palw_court_v2::PalwCourtVerdictProofV2,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    palw_tir_public_carriage_v1::{PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1, PalwTirPublicControlV1, PalwTirPublicConvictionEvidenceV1},
    palw_tir_public_claim_v1::PalwTirPublicFrameViewV1,
    palw_tir_public_source_v1::PalwTirPublicAuthenticatedIdsV1,
};

/// The fold's filer and window rules, read from the accepted frame view: not liable on the claim, inside the d0 door.
pub(crate) fn palw_tir_public_may_file_v1(view: &PalwTirPublicFrameViewV1, filer: &PalwBondKeyV2, now_daa: u64) -> Result<(), String> {
    if view.liable.contains(filer) {
        return Err("a liable bond (the producer or a credited Valid custodian) cannot file against its own claim".into());
    }
    if now_daa < view.accepted_daa || now_daa >= view.collect_until_daa {
        return Err(format!(
            "the frame's conviction door is [{}, {}) and the accepted clock is {now_daa}",
            view.accepted_daa, view.collect_until_daa
        ));
    }
    Ok(())
}

/// The arithmetic scheduler's `Ready` cone/logits primitive, program stripped (the fold fills the registered one).
pub(crate) fn palw_tir_public_arithmetic_evidence_v1(mut proof: PalwCourtVerdictProofV2) -> Result<PalwTirPublicConvictionEvidenceV1, String> {
    proof.tir_strip_program_v1();
    let evidence = PalwTirPublicConvictionEvidenceV1::Arithmetic { proof: Box::new(proof) };
    evidence.check_v1()?;
    Ok(evidence)
}

/// A complete EOF-authenticated original ID sequence (an owner's `Finished(Ids)` or the producer's public answer).
pub(crate) fn palw_tir_public_output_evidence_v1(ids: &PalwTirPublicAuthenticatedIdsV1) -> Result<PalwTirPublicConvictionEvidenceV1, String> {
    let evidence = PalwTirPublicConvictionEvidenceV1::Output { rows_root: ids.rows_root_v1(), ids: ids.ids_v1().to_vec() };
    evidence.check_v1()?;
    Ok(evidence)
}

/// A greedy original's tiled decode-token primitive. Refused locally for a non-greedy original (the fold would refuse it).
pub(crate) fn palw_tir_public_selection_evidence_v1(
    view: &PalwTirPublicFrameViewV1,
    mut proof: PalwCourtVerdictProofV2,
) -> Result<PalwTirPublicConvictionEvidenceV1, String> {
    if !view.greedy_selection {
        return Err("the original selection rule is not plain greedy; no frame court judges it".into());
    }
    proof.tir_strip_program_v1();
    let evidence = PalwTirPublicConvictionEvidenceV1::DecodeToken { proof: Box::new(proof) };
    evidence.check_v1()?;
    Ok(evidence)
}

/// **Build and sign the one common filing.** `sign(message, context)` is the filer bond's ML-DSA-87 signer (the in-process key
/// or a sidecar); the message is the control's own signing message under the NATIVE network domain. The carrier that rides it
/// must pay at least [`PalwTirPublicFrameViewV1::conviction_rent_sompi`] above relay (`palw_lifecycle_carrier_rent_v2` adds it).
pub(crate) fn palw_tir_public_conviction_object_v1(
    sign: impl Fn(&[u8], &[u8]) -> Vec<u8>,
    network_domain: Hash64,
    view: &PalwTirPublicFrameViewV1,
    filer: PalwBondKeyV2,
    now_daa: u64,
    evidence: PalwTirPublicConvictionEvidenceV1,
) -> Result<PalwConsensusObjectV2, String> {
    palw_tir_public_may_file_v1(view, &filer, now_daa)?;
    let mut control = PalwTirPublicControlV1::conviction_unsigned_v1(
        view.claim,
        view.allocation_id,
        view.source_id,
        filer,
        evidence,
        view.conviction_rent_sompi,
    )?;
    let digest = control.signing_message_v1(network_domain)?;
    control.signature = sign(digest.as_byte_slice(), PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1);
    control.check_v1()?;
    Ok(PalwConsensusObjectV2::TirPublicControlV1 { control: Box::new(control) })
}

/// **G14 R3: a drawn seat's atomic Valid + raw custody grant + Ready** (node side; builds and signs, nothing else). The three
/// signatures are the seat's own, each in its ORIGINAL domain: the whole-job V2 receipt under the receipt context, the raw grant
/// over its id under the source-grant context, and the outer object under the grant-Ready context. `descriptor` is the backend's
/// prepared descriptor of the seat's retained original source (the preparation worker that produces it is remaining work).
pub(crate) fn palw_tir_public_grant_ready_object_v1(
    sign: impl Fn(&[u8], &[u8]) -> Vec<u8>,
    network_domain: Hash64,
    view: &PalwTirPublicFrameViewV1,
    seat: PalwBondKeyV2,
    signed_daa: u64,
    descriptor: kaspa_consensus_core::palw_backend::PalwTirPublicPreparedDescriptorV1,
) -> Result<PalwConsensusObjectV2, String> {
    use kaspa_consensus_core::palw_panel_v2::{PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2};
    use kaspa_consensus_core::palw_tir_public_carriage_v1::{PALW_TIR_PUBLIC_GRANT_READY_CONTEXT_V1, PalwTirPublicGrantReadyV1};
    use kaspa_consensus_core::palw_tir_public_custody_v1::{PALW_TIR_SOURCE_GRANT_CONTEXT_V1, PalwTirPublicCustodyGrantV1};
    if !view.drawn_panel.contains(&seat) || view.liable.contains(&seat) {
        return Err("only a drawn seat that holds no custody grant yet can grant raw custody".into());
    }
    let message = palw_receipt_message_v2(network_domain, view.claim, PalwReceiptVerdictV2::Valid, signed_daa);
    let receipt = PalwSeatReceiptV2 {
        claim: view.claim,
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: seat,
        signed_daa,
        signature: sign(message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
    };
    let mut grant = PalwTirPublicCustodyGrantV1 {
        version: 1,
        claim: view.claim,
        seat,
        signed_daa,
        pledge_id: view.pledge_id,
        valid_receipt_digest: PalwTirPublicCustodyGrantV1::receipt_digest_v1(&receipt),
        signature: Vec::new(),
    };
    grant.signature = sign(grant.id_v1().as_byte_slice(), PALW_TIR_SOURCE_GRANT_CONTEXT_V1);
    let mut object = PalwTirPublicGrantReadyV1 {
        version: 1,
        claim: view.claim,
        allocation_id: view.allocation_id,
        source_id: view.source_id,
        signer: seat,
        receipt,
        grant,
        descriptor,
        signature: Vec::new(),
    };
    let digest = object.signing_message_v1(network_domain)?;
    object.signature = sign(digest.as_byte_slice(), PALW_TIR_PUBLIC_GRANT_READY_CONTEXT_V1);
    object.check_v1()?;
    Ok(PalwConsensusObjectV2::TirPublicGrantReadyV1 { grant: Box::new(object) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_tir_public_carriage_v1::{PalwTirPublicControlActionV1 as A, PalwTirPublicControlTargetV1 as T};
    use std::collections::BTreeSet;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(n), 0))
    }
    fn view() -> PalwTirPublicFrameViewV1 {
        let header = kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1 {
            version: 1,
            job_context: kaspa_consensus_core::palw_v2::PalwJobContextV2 {
                version: 2,
                network_id: b"testnet-12".to_vec(),
                job_id: h(1),
                job_nullifier: h(2),
                assignment_id: h(3),
                execution_seed: [0u8; 32],
                model_profile_id: h(4),
                runtime_manifest_hash: h(5),
                runtime_class_id: h(6),
                shape_profile_id: h(7),
                trace_scheme_id: h(8),
                cu_ruleset_id: h(9),
                tokenizer_id: h(10),
                prompt_token_ids_hash: h(11),
                declared_prefill_tokens: 3,
                exact_decode_tokens: 2,
                max_context_tokens: 64,
            },
            artifact_root: h(12),
            full_logits_trace_root: h(13),
            step_leaf_count: 19,
            step_merkle_root: h(14),
            committed_execution_root: h(15),
        };
        PalwTirPublicFrameViewV1 {
            claim: h(100),
            allocation_id: h(101),
            source_id: h(102),
            producer: bond(1),
            pledge_id: h(103),
            liable: BTreeSet::from([bond(1), bond(2)]),
            drawn_panel: BTreeSet::from([bond(2), bond(3)]),
            accepted_daa: 100,
            collect_until_daa: 200,
            material_until_daa: 300,
            conviction_rent_sompi: 777,
            maximum_witness_bytes: 1 << 20,
            greedy_selection: true,
            header,
        }
    }
    fn evidence() -> PalwTirPublicConvictionEvidenceV1 {
        PalwTirPublicConvictionEvidenceV1::Output { rows_root: h(20), ids: vec![1, 2] }
    }

    /// Real ML-DSA-87 (the validator key), not an injected verifier: the filing verifies under the native domain and the control
    /// context only, burns the frame's exact rent, names no role, and the same object is built for a drawn seat and an outsider.
    #[test]
    fn the_common_filing_is_signed_by_the_literal_filer_and_identical_for_seat_and_outsider() {
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed([9u8; kaspa_pq_validator_core::VALIDATOR_SEED_LEN]);
        let sign = |message: &[u8], context: &[u8]| key.sign_with_context(message, context).to_vec();
        let network = h(0x4E7);
        let v = view();
        for filer in [bond(3), bond(300)] {
            let PalwConsensusObjectV2::TirPublicControlV1 { control } =
                palw_tir_public_conviction_object_v1(sign, network, &v, filer, 150, evidence()).unwrap()
            else {
                unreachable!()
            };
            assert_eq!((control.target, control.signer, control.declared_rent_sompi), (T::Conviction, filer, 777));
            assert!(matches!(control.action, A::Convict(_)));
            control
                .validate_signature_v1(network, key.public_key(), kaspa_consensus_core::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1)
                .unwrap();
            assert!(
                control
                    .validate_signature_v1(h(0x4E8), key.public_key(), kaspa_consensus_core::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1)
                    .is_err(),
                "another network's domain"
            );
        }
        // Liable bonds and closed doors are refused before anything is signed.
        for (filer, at) in [(bond(1), 150), (bond(2), 150), (bond(300), 99), (bond(300), 200)] {
            assert!(palw_tir_public_conviction_object_v1(sign, network, &v, filer, at, evidence()).is_err());
        }
    }
}
