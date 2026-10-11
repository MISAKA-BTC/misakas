//! Experimental complete-population mechanics. Fixture entropy/eligibility are not a
//! certified source, actual native population or an activation/RAM/latency approval.
use kaspa_hashes::Hash64;
use misaka_palw_panel::*;
use std::collections::{BTreeMap, BTreeSet};

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn bond(n: u64) -> BondIdV1 {
    BondIdV1 { transaction: h(n + 1_000_000), index: 0 }
}
fn seat(n: u64) -> SeatCandidateV1 {
    SeatCandidateV1 {
        bond: bond(n),
        operator: h(n / 3 + 1),
        key: h(n + 2_000_000),
        collateral: n % 13 + 100,
        registered_daa: 0,
        capability_root: h(7),
        readiness_root: h(8),
        roles: CLASS_ROLE_V1 | OUTSIDER_ROLE_V1,
    }
}
fn policy() -> PanelPolicyV1 {
    PanelPolicyV1 {
        seal_depth_blocks: 1,
        seal_wait_daa: 50,
        bond_maturity_daa: 1,
        beacon_period_daa: 10,
        beacon_wait_daa: 2,
        assignment_delay_daa: 1,
        receipt_window_daa: 3,
        seat_count: 3,
        outsider_seats: 1,
        max_retries: 1,
        min_collateral: 100,
        max_candidates: 100_000,
        max_pending: 8,
        max_pending_per_bond: 4,
        max_assignments_per_block: 1,
        max_admissions_per_block: 1,
        max_tracked_claims: 100,
        max_beacons_per_block: 2,
        max_beacon_proof_bytes: 100,
        beacon_scheme: h(777),
    }
}
fn claim() -> AdmittedClaimV1 {
    AdmittedClaimV1 {
        claim_id: h(9_000_000),
        work_id: h(9_000_001),
        class_id: h(9_000_002),
        producer: bond(9_000_003),
        producer_operator: h(9_000_004),
        producer_key: h(9_000_005),
        immutable_fields: h(9_000_006),
        required_exposure: 100,
    }
}
fn snapshot(mut rows: Vec<SeatCandidateV1>) -> PanelSnapshotV1 {
    rows.sort_by_key(|s| (s.operator, s.bond));
    PanelSnapshotV1 {
        root: h(0),
        checkpoint: h(1),
        checkpoint_daa: 2,
        checkpoint_height: 2,
        policy_id: policy().id(),
        class_id: claim().class_id,
        excluded_bond: claim().producer,
        excluded_operator: claim().producer_operator,
        excluded_key: claim().producer_key,
        candidates: rows,
        strata_members: Vec::new(),
    }
}

fn prefix_from_full(s: &PanelSnapshotV1, seed: Hash64, retry: u16, role: u8, used: &[Hash64], needed: usize) -> Vec<BondIdV1> {
    let rows: BTreeMap<_, _> = s.candidates.iter().map(|c| (c.bond, c)).collect();
    let mut seen: BTreeSet<_> = used.iter().copied().collect();
    seat_order_v1(s, seed, retry, role, used)
        .unwrap()
        .into_iter()
        .filter(|b| {
            let c = rows[b];
            // Some highest-ranked bonds have no headroom: lower-ranked bonds of the same
            // operator must be tried without reducing that operator's aggregate race weight.
            c.bond.transaction.as_bytes()[0] % 5 != 0 && seen.insert(c.operator)
        })
        .take(needed)
        .collect()
}

#[test]
fn streaming_is_exactly_the_full_operator_race_with_headroom_retries_and_used_operators() {
    let s = snapshot((1..=768).map(seat).collect());
    let used = [h(3), h(7), h(31)];
    for seed in [h(1), h(9), h(513)] {
        for retry in [0, 1, 16] {
            for role in [CLASS_ROLE_V1, OUTSIDER_ROLE_V1] {
                let got = seat_prefix_streaming_v1(s.candidates.iter().cloned(), seed, retry, role, &used, 15, 100_000, |c| {
                    c.bond.transaction.as_bytes()[0] % 5 != 0
                })
                .unwrap();
                assert_eq!(got, prefix_from_full(&s, seed, retry, role, &used, 15));
            }
        }
    }
}

#[test]
fn streaming_refuses_truncation_unordered_duplicate_rows_and_operator_weight_overflow() {
    let s = snapshot((1..=9).map(seat).collect());
    let run = |rows: Vec<SeatCandidateV1>, cap| seat_prefix_streaming_v1(rows, h(1), 0, CLASS_ROLE_V1, &[], 3, cap, |_| true);
    assert_eq!(run(s.candidates.clone(), 8), Err(PanelErrorV1::ResourceLimit));
    let mut unordered = s.candidates.clone();
    unordered.reverse();
    assert_eq!(run(unordered, 100), Err(PanelErrorV1::InvalidSnapshot));
    assert_eq!(run(vec![s.candidates[0].clone(), s.candidates[0].clone()], 100), Err(PanelErrorV1::InvalidSnapshot));
    let mut overflow = s.candidates[..2].to_vec();
    overflow[0].operator = h(9);
    overflow[1].operator = h(9);
    overflow[0].collateral = u64::MAX;
    overflow[1].collateral = 1;
    overflow.sort_by_key(|s| (s.operator, s.bond));
    assert_eq!(run(overflow, 100), Err(PanelErrorV1::InvalidSnapshot));
    assert!(policy().validate().is_err(), "100k is never a valid historical policy");
    assert!(policy().validate_for_version(WIRE_VERSION_COMPLETE_V1).is_ok());
}

struct View {
    rows: Vec<SeatCandidateV1>,
}
impl ConsensusViewV1 for View {
    fn candidates(&self, _: &AdmittedClaimV1) -> Result<Vec<SeatCandidateV1>, PanelErrorV1> {
        Ok(self.rows.clone())
    }
    fn available_collateral(&self, _: &BondIdV1) -> u128 {
        1000
    }
    fn terminal_claim(&self, _: &Hash64) -> bool {
        false
    }
    fn verify_beacon(&self, _: &BeaconRequestV1, proof: &BeaconProofV1) -> Result<(), PanelErrorV1> {
        if proof.proof == b"fixture only" { Ok(()) } else { Err(PanelErrorV1::InvalidBeacon) }
    }
}
fn advance(s: &PermissionlessPanelStateV1, height: u64, daa: u64, v: &View) -> PermissionlessPanelStateV1 {
    s.advance(&SelectedChainStepV1 { block: h(height), parent: s.tip(), height, daa, admissions: Vec::new(), beacons: Vec::new() }, v)
        .unwrap()
        .0
}
fn initial() -> PermissionlessPanelStateV1 {
    PermissionlessPanelStateV1::from_cursor(PanelCursorV1 {
        version: WIRE_VERSION_COMPLETE_V1,
        network: h(700),
        ruleset: h(701),
        policy: policy(),
        tip: h(0),
        height: 0,
        daa: 0,
        next_order: 0,
    })
    .unwrap()
}
fn sealed(v: &View) -> PermissionlessPanelStateV1 {
    let s = advance(&initial(), 1, 1, v);
    let (s, refused) = s.admit(&[claim()]).unwrap();
    assert!(refused.is_empty());
    advance(&advance(&s, 2, 2, v), 3, 3, v)
}

#[test]
fn complete_100k_seal_draw_restore_and_100001_refusal_are_explicit() {
    let begun = std::time::Instant::now();
    let v = View { rows: (1..=100_000).map(seat).collect() };
    let s = sealed(&v);
    let r = s.claim(&claim().claim_id).unwrap();
    let snapshot = r.snapshot.as_ref().unwrap();
    assert_eq!(snapshot.candidates.len(), 100_000);
    assert_eq!(snapshot.root, snapshot.computed_root());
    assert_eq!(borsh::to_vec(&snapshot.candidates[0]).unwrap().len(), 341);
    let bytes = borsh::to_vec(&s).unwrap();
    assert!(bytes.len() as u64 <= MAX_COMPLETE_STATE_BYTES_V1);
    let restored = PermissionlessPanelStateV1::import(&bytes, s.root(), s.network(), s.ruleset(), s.policy()).unwrap();
    assert_eq!(restored, s);
    let s =
        advance(&s, 4, 10, &v).accept_beacon(&BeaconProofV1 { epoch: 1, output: h(42), proof: b"fixture only".to_vec() }, &v).unwrap();
    let s = advance(&s, 5, 13, &v);
    let r = s.claim(&claim().claim_id).unwrap();
    let ClaimPhaseV3::Bound(b) = &r.phase else { panic!("{:?}", r.phase) };
    assert_eq!(b.seats.len(), 3);
    for bond in &b.seats {
        assert_eq!(s.reserved(bond), 100);
    }
    let s = advance(&s, 6, 17, &v);
    let r = s.claim(&claim().claim_id).unwrap();
    assert_eq!(r.binding_history.len(), 2);
    let mut used = BTreeSet::new();
    let by_bond: BTreeMap<_, _> = r.snapshot.as_ref().unwrap().candidates.iter().map(|c| (c.bond, c.operator)).collect();
    for b in &r.binding_history {
        for bond in &b.seats {
            assert!(used.insert(by_bond[bond]));
        }
    }
    let encoded = borsh::to_vec(&s).unwrap();
    assert_eq!(PermissionlessPanelStateV1::import(&encoded, s.root(), s.network(), s.ruleset(), s.policy()).unwrap(), s);
    eprintln!(
        "RFC06 complete fixture: population=100000 encoded={} bytes elapsed={:?}; fixture mechanics, no release PASS",
        encoded.len(),
        begun.elapsed()
    );
    drop((s, restored, bytes, v));
    let over = View { rows: (1..=100_001).map(seat).collect() };
    let refused = sealed(&over);
    let r = refused.claim(&claim().claim_id).unwrap();
    assert!(r.snapshot.is_none(), "no entropy-dependent or capital-prioritised truncation");
    assert!(matches!(r.phase, ClaimPhaseV3::Voided { reason: NonFraudReasonV1::PopulationResourceLimit, .. }));
    assert_eq!(refused.reserved(&bond(1)), 0);
}
