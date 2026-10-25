//! RFC-0006 × RFC-0018 Gate C: the actual certification sweep's aux-136 row snapshot is admitted
//! before construction, refused below its quote with no state/journal change, retried, and held
//! by the construction owner until the builder drops. Fixture rows are stored test rows; they are
//! not a signed attestation campaign, approval or public arming.
use super::*;
use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};
use crate::palw_rfc0018_v1::{PalwGateCRuleV1, PalwRfc0018MirrorV1};
use crate::palw_security_certification_v1::*;
use std::sync::Arc;

const FENCE: u64 = 50_000;
const NOW: u64 = FENCE + 5_000;

fn p() -> PalwStateParamsV2 {
    let base = match crate::config::params::palw_t12_shipped_params().palw_consensus_mode {
        crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.state,
        _ => unreachable!(),
    };
    base.with_rfc0018_v1(Some(PalwRfc0018MirrorV1 {
        sibling_attribution_from_daa: Some(FENCE),
        panel_anchor_permissionless_from_daa: Some(FENCE),
        rule_e_participation_v2_from_daa: Some(FENCE),
        gate_c_from_daa: Some(FENCE),
        gate_c_rule: PalwGateCRuleV1::SecurityCertifiedV1,
        certification: PalwSecurityCertificationPolicyV1::design_unapproved_v1(720),
        fp_receipt_tail_daa: 0,
    }))
}
fn class() -> Hash64 {
    Hash64::from_u64_word(0xC1A5_0002)
}
fn id(n: u64) -> Hash64 {
    Hash64::from_u64_word(0x5EC1_0000 + n)
}
fn decided(s: &mut PalwChainStateV2, claim: Hash64, draws: usize) -> Vec<u8> {
    let row = PalwClaimCertificationRowV1 {
        claim: claim.as_bytes(),
        class_id: class().as_bytes(),
        producer_bond: [1; 64],
        producer_principal: [2; 64],
        attempt: true,
        final_daa: FENCE + 10,
        committed_daa: FENCE + 10,
        committed_span: 0,
        anchor: None,
        draws: vec![[7; 64]; draws],
        drawn_daa: 0,
        deadline_daa: 0,
        attestations: Vec::new(),
        held_reward: 0,
        payout_payload: [3; 64],
        outcome: Some((PalwCertificationOutcomeV1::Certified, FENCE + 99)),
    };
    let key = certification_claim_key_v1(&palw_certification_id_v1(&claim));
    s.set_kernel_aux_row_for_tests(PALW_SECURITY_CERTIFICATION_TABLE_V1, key.clone(), Some(borsh::to_vec(&row).unwrap()));
    key
}
/// Two decided rows whose claims have left the state (the sweep retires them: real writes) and one
/// whose claim stands (kept). Three nonempty snapshot rows.
fn world() -> (PalwChainStateV2, Vec<Vec<u8>>) {
    let mut s = PalwChainStateV2::genesis();
    let mut c = palw_claim_template_v1(
        class(),
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(9), 0)),
        FENCE + 1,
        0,
        1_000,
    );
    c.phase = PalwClaimPhaseV2::Final { final_daa: FENCE + 100 };
    s.set_false_valid_rows_for_tests(id(3), Some(c), None, None, 0);
    let retired = vec![decided(&mut s, id(1), 3), decided(&mut s, id(2), 0)];
    decided(&mut s, id(3), 5);
    (s, retired)
}
fn checker(bytes: u64) -> Arc<NativeDemandCheckerV1> {
    NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: bytes, database_bytes: 0 })
}
fn builder<'a>(
    s: &PalwChainStateV2,
    p: &'a PalwStateParamsV2,
    e: &'a PalwTransitionExtrasV1,
    c: &Arc<NativeDemandCheckerV1>,
) -> TransitionBuilder<'a> {
    let mut b = TransitionBuilder::new(s, p, false, false, false, false, e);
    b.native_reservation = NativeTransitionReservationV1::for_checker_v1(c);
    b
}
fn point() -> PalwBlockContextV2 {
    ctx(NOW, NOW, NOW)
}
fn has(s: &PalwChainStateV2, key: &[u8]) -> bool {
    s.kernel_route.as_ref().unwrap().aux.contains_key(&(PALW_SECURITY_CERTIFICATION_TABLE_V1, key.to_vec()))
}

#[test]
fn certification_sweep_snapshot_refuses_below_quote_without_change_and_retries() {
    let (s, retired) = world();
    let p = p();
    let e = PalwTransitionExtrasV1::default();
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    b.reserve_security_certification_sweep_workspace_v2().unwrap();
    let q = c.outstanding().0;
    assert!(q > 0, "three nonempty rows are quoted");
    drop(b);
    assert_eq!(c.outstanding(), (0, 0), "the construction owner releases on drop");
    for cap in [0, q - 1] {
        let c = checker(cap);
        let mut b = builder(&s, &p, &e, &c);
        assert!(matches!(
            palw_security_certification_fold_v1::sweep_security_certification_v1(&mut b, &point()),
            Err(PalwStateV2Error::NativeResource(_))
        ));
        assert_eq!(b.state, s);
        assert_eq!(b.state.state_root(), s.state_root());
        assert!(b.entries.is_empty(), "refused before any journal entry");
        assert!(retired.iter().all(|k| has(&b.state, k)), "no row was retired");
        assert_eq!(c.outstanding(), (0, 0), "a refused quote holds nothing");
    }
    // Exactly Q admits the snapshot and keeps it owned until the builder drops.
    let c = checker(q);
    let mut b = builder(&s, &p, &e, &c);
    b.reserve_security_certification_sweep_workspace_v2().unwrap();
    assert_eq!(c.outstanding(), (q, 0));
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    // A sufficient limit: the actual sweep succeeds, retires the two rows and holds at least Q until drop.
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    palw_security_certification_fold_v1::sweep_security_certification_v1(&mut b, &point()).unwrap();
    assert!(retired.iter().all(|k| !has(&b.state, k)), "both orphaned decided rows retire");
    assert!(!b.entries.is_empty());
    assert!(c.outstanding().0 >= q, "the snapshot quote is held past its last use");
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
}

#[test]
fn certification_sweep_snapshot_quote_tracks_actual_rows_and_preserves_flags_off_bytes() {
    let (s, _) = world();
    let p = p();
    let e = PalwTransitionExtrasV1::default();
    // Untracked (every shipped preset): the historical sweep, byte for byte.
    let mut raw = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
    palw_security_certification_fold_v1::sweep_security_certification_v1(&mut raw, &point()).unwrap();
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    palw_security_certification_fold_v1::sweep_security_certification_v1(&mut b, &point()).unwrap();
    assert_eq!(b.state, raw.state);
    assert_eq!(b.state.state_root(), raw.state.state_root());
    assert_eq!(borsh::to_vec(&b.entries).unwrap(), borsh::to_vec(&raw.entries).unwrap());
    let delta = PalwStateDeltaV2 { point: point(), entries: b.entries.clone() };
    assert_eq!(apply_delta_v2(&s, &delta, &p).unwrap().state_root(), b.state.state_root());
    assert_eq!(revert_delta_v2(&b.state, &delta, &p).unwrap().state_root(), s.state_root());
    drop(b);
    // The quote grows with the actual row bytes (more draws ⇒ a longer row clone).
    let quote_of = |s: &PalwChainStateV2| {
        let c = checker(u64::MAX);
        let mut b = builder(s, &p, &e, &c);
        b.reserve_security_certification_sweep_workspace_v2().unwrap();
        c.outstanding().0
    };
    let base = quote_of(&s);
    let mut bigger = s.clone();
    decided(&mut bigger, id(3), 50);
    assert!(quote_of(&bigger) > base);
    // No route / below the fence: nothing is quoted and the sweep is a no-op even at a zero limit.
    let c = checker(0);
    let mut empty = builder(&PalwChainStateV2::genesis(), &p, &e, &c);
    palw_security_certification_fold_v1::sweep_security_certification_v1(&mut empty, &point()).unwrap();
    assert!(empty.entries.is_empty());
    let below = ctx(FENCE - 1, FENCE - 1, FENCE - 1);
    let mut dormant = builder(&s, &p, &e, &c);
    palw_security_certification_fold_v1::sweep_security_certification_v1(&mut dormant, &below).unwrap();
    assert_eq!(dormant.state, s);
    assert!(dormant.entries.is_empty());
    assert_eq!(c.outstanding(), (0, 0));
}
