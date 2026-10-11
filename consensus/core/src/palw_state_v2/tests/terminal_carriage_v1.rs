//! V-fold evidence, not a signature/service fixture. Ordinary reservation and objective courts
//! are applied; the supplied MLDSA-sized declarations are signature-checked by separate gate tests.
use super::*;
use crate::palw_legacy_public_filer_v1::*;
use crate::palw_offence_v1::PalwOffenceKindV1;
use crate::palw_terminal_carriage_v1::*;
fn fixture() -> (PalwStateParamsV2, PalwChainStateV2, Hash64, PalwConsensusObjectV2, Vec<PalwTerminalPartV1>, PalwTransitionExtrasV1) {
    let p = params().with_legacy_public_filer_from_daa(Some(0));
    let (licensed, claim, c, _) = f2_licensed_false_execution(&p);
    let x = PalwTransitionExtrasV1 { terminal_evidence_max_bytes: Some(PALW_TERMINAL_MAX_BYTES_V1), ..f2_extras() };
    let row = licensed.claim(&claim).unwrap();
    let reserve = PalwConsensusObjectV2::DisputeReservedV1 {
        reservation: Box::new(PalwDisputeReservationV1 {
            version: 1,
            claim,
            execution_root: row.execution_root,
            trace_root: row.trace_root,
            reserver: bond_key(7),
        }),
        signature: vec![1],
    };
    let (s, _) = f2_apply(&licensed, &p, &ctx(4, 104, 104), &[seat_bond_reg(7, 1_000), reserve], &x)
        .expect("ordinary independently collateralized reservation");
    let evidence = borsh::to_vec(&f2_payload(claim, 2, c)).unwrap();
    let (d, parts) = palw_terminal_split_v1(
        PalwTerminalDeclarationV1 {
            version: 1,
            claim,
            reserver: bond_key(7),
            reserved_daa: 104,
            kind: PalwOffenceKindV1::PanelFalseValidV2,
            accused: bond_key(2),
            evidence_id: crate::palw_offence_v1::palw_offence_evidence_digest_v1(&evidence),
            heavy_prompt_ids: 0,
            total_bytes: 0,
            part_bytes: (evidence.len() / 2) as u32,
            part_digests: vec![],
            signature: vec![],
        },
        &evidence,
        PALW_TERMINAL_MAX_BYTES_V1,
        |_| Ok(vec![8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN]),
    )
    .unwrap();
    assert!(parts.len() > 1);
    (p, s, claim, PalwConsensusObjectV2::ObjectiveEvidenceDeclaredV1 { declaration: Box::new(d) }, parts, x)
}
#[test]
fn owned_terminal_fold_partial_restart_final_conviction_and_each_delta_reverts() {
    let (p, base, claim, decl, parts, x) = fixture();
    let (s1, d1) = f2_apply(&base, &p, &ctx(5, 105, 105), &[decl], &x).unwrap();
    assert_ne!(s1.state_root(), base.state_root());
    assert_eq!(revert_delta_v2(&s1, &d1, &p).unwrap().state_root(), base.state_root());
    let mut current = s1;
    for (n, part) in parts.into_iter().enumerate() {
        let before = current.clone();
        let object = PalwConsensusObjectV2::ObjectiveEvidenceChunkV1 { part: Box::new(part) };
        let (next, delta) = f2_apply(&before, &p, &ctx(6 + n as u64, 106 + n as u64, 106 + n as u64), &[object], &x).unwrap();
        assert_eq!(apply_delta_v2(&before, &delta, &p).unwrap().state_root(), next.state_root());
        assert_eq!(revert_delta_v2(&next, &delta, &p).unwrap().state_root(), before.state_root());
        let carriage = borsh::to_vec(&PalwStateCarriageV2::from_state(&next)).unwrap();
        let restarted = borsh::from_slice::<PalwStateCarriageV2>(&carriage).unwrap().into_state(&p, Some(next.state_root())).unwrap();
        assert_eq!(restarted.state_root(), next.state_root());
        current = restarted;
    }
    assert!(current.terminal_pending_v1(&(claim, bond_key(7))).is_none());
    let key = crate::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(&bond_key(2).0, &claim);
    assert!(current.consumed_offence(&key).is_some());
    assert!(current.bond(&bond_key(2)).unwrap().collateral < base.bond(&bond_key(2)).unwrap().collateral);
}
#[test]
fn owned_terminal_fold_cannot_replace_take_over_corrupt_or_extend_closed_reservation() {
    let (p, base, claim, decl, parts, x) = fixture();
    let (s1, _) = f2_apply(&base, &p, &ctx(5, 105, 105), &[decl.clone()], &x).unwrap();
    assert!(f2_apply(&s1, &p, &ctx(6, 106, 106), &[decl.clone()], &x).is_err());
    let mut foreign = decl.clone();
    if let PalwConsensusObjectV2::ObjectiveEvidenceDeclaredV1 { declaration } = &mut foreign {
        declaration.reserver = bond_key(4);
    }
    assert!(f2_apply(&s1, &p, &ctx(6, 106, 106), &[foreign], &x).is_err());
    let mut bad = parts[0].clone();
    bad.bytes[0] ^= 1;
    assert!(
        f2_apply(&s1, &p, &ctx(6, 106, 106), &[PalwConsensusObjectV2::ObjectiveEvidenceChunkV1 { part: Box::new(bad) }], &x).is_err()
    );
    let released = PalwConsensusObjectV2::DisputeReleasedV1 { claim, reserver: bond_key(7), signature: vec![1] };
    let (s2, _) = f2_apply(&s1, &p, &ctx(6, 106, 106), &[released], &x).unwrap();
    assert!(s2.terminal_pending_v1(&(claim, bond_key(7))).is_none());
    assert!(
        f2_apply(
            &s2,
            &p,
            &ctx(7, 107, 107),
            &[PalwConsensusObjectV2::ObjectiveEvidenceChunkV1 { part: Box::new(parts[0].clone()) }],
            &x
        )
        .is_err()
    );
    let dormant = p.clone().with_legacy_public_filer_from_daa(None);
    assert!(f2_apply(&base, &dormant, &ctx(5, 105, 105), &[decl], &x).is_err());
}
