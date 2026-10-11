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

/// Host-authenticated native retirement is deliberately separate from ReceiptLicensed/Final.
struct RetirementView {
    base: View,
    terminal: BTreeSet<Hash64>,
    retired: BTreeSet<Hash64>,
}
impl ConsensusViewV1 for RetirementView {
    fn candidates(&self, c: &AdmittedClaimV1) -> Result<Vec<SeatCandidateV1>, PanelErrorV1> {
        self.base.candidates(c)
    }
    fn available_collateral(&self, b: &BondIdV1) -> u128 {
        self.base.available_collateral(b)
    }
    fn terminal_claim(&self, id: &Hash64) -> bool {
        self.terminal.contains(id)
    }
    fn retired_claim(&self, id: &Hash64) -> Option<PanelRetirementAuthorityV1> {
        self.retired
            .contains(id)
            .then_some(PanelRetirementAuthorityV1 { native_parent_root: h(8800), native_parent_checkpoint: h(8801) })
    }
    fn verify_beacon(&self, r: &BeaconRequestV1, p: &BeaconProofV1) -> Result<(), PanelErrorV1> {
        self.base.verify_beacon(r, p)
    }
}
fn retirement_view(n: u64) -> RetirementView {
    RetirementView { base: View { rows: (1..=n).map(seat).collect() }, terminal: BTreeSet::new(), retired: BTreeSet::new() }
}
fn retirement_initial(version: u16, active_cap: u32) -> PermissionlessPanelStateV1 {
    let mut cursor = initial().cursor();
    cursor.version = version;
    cursor.policy.max_tracked_claims = active_cap;
    PermissionlessPanelStateV1::from_cursor(cursor).unwrap()
}
fn retirement_advance(s: &PermissionlessPanelStateV1, daa: u64, v: &RetirementView) -> PermissionlessPanelStateV1 {
    s.advance(
        &SelectedChainStepV1 {
            block: h(s.height() + 1),
            parent: s.tip(),
            height: s.height() + 1,
            daa,
            admissions: Vec::new(),
            beacons: Vec::new(),
        },
        v,
    )
    .unwrap()
    .0
}
fn distinct_claim(n: u64) -> AdmittedClaimV1 {
    let mut c = claim();
    c.claim_id = h(9_100_000 + n);
    c.work_id = h(9_200_000 + n);
    c.producer = bond(9_300_000 + n);
    c
}

#[test]
fn retired_population_waits_for_native_removal_keeps_work_and_seed_and_replays_undo_exactly() {
    let mut v = retirement_view(96);
    let mut s = retirement_advance(&retirement_initial(WIRE_VERSION_RETIRED_V1, 8), 1, &v);
    s = s.admit(&[claim()]).unwrap().0;
    s = retirement_advance(&retirement_advance(&s, 2, &v), 3, &v);
    s = retirement_advance(&s, 10, &v)
        .accept_beacon(&BeaconProofV1 { epoch: 1, output: h(42), proof: b"fixture only".to_vec() }, &v)
        .unwrap();
    s = retirement_advance(&s, 13, &v);
    let before = s.claim(&claim().claim_id).unwrap().clone();
    assert!(matches!(before.phase, ClaimPhaseV3::Bound(_)));
    v.terminal.insert(claim().claim_id);
    s = retirement_advance(&s, 14, &v);
    assert!(matches!(s.claim(&claim().claim_id).unwrap().phase, ClaimPhaseV3::Released { .. }));
    assert!(s.claim(&claim().claim_id).unwrap().snapshot.is_some(), "receipt release is not native retirement");
    let full = s.clone();
    let full_bytes = borsh::to_vec(&full).unwrap();
    v.retired.insert(claim().claim_id);
    let compact = retirement_advance(&s, 15, &v);
    let r = compact.claim(&claim().claim_id).unwrap();
    let ClaimPhaseV3::Retired(t) = &r.phase else { panic!("{:?}", r.phase) };
    assert!(r.snapshot.is_none() && r.binding_history.is_empty() && r.used_operators.is_empty());
    assert_eq!(t.population.as_ref().unwrap().root, before.snapshot.as_ref().unwrap().root);
    assert_eq!(t.population.as_ref().unwrap().candidate_count, 96);
    assert_eq!(t.last_seed, before.binding_history.last().map(|b| b.panel_seed_v3));
    assert_eq!(t.binding_count, 1);
    assert_eq!(r.claim, before.claim);
    assert_eq!(r.seal, before.seal);
    assert_eq!(r.entropy_output, before.entropy_output);
    assert!(compact.work_id_rows().contains(&claim().work_id));
    assert!(before.binding_history.iter().flat_map(|b| &b.seats).all(|bond| compact.reserved(bond) == 0));
    let bytes = borsh::to_vec(&compact).unwrap();
    assert!(bytes.len() * 4 < full_bytes.len());
    assert_eq!(
        PermissionlessPanelStateV1::import(&bytes, compact.root(), compact.network(), compact.ruleset(), compact.policy()).unwrap(),
        compact
    );
    // Production deltas use the existing cursor/claim/beacon rows. Work rows never disappear.
    let mut replay = full.clone();
    replay.set_cursor(compact.cursor());
    replay.put_claim_row(claim().claim_id, Some(r.clone()));
    for epoch in full.beacon_rows().keys() {
        replay.put_beacon_row(*epoch, compact.beacon_rows().get(epoch).copied());
    }
    replay.refresh_derived().unwrap();
    assert_eq!(replay.root(), compact.root());
    replay.check_consistency().unwrap();
    replay.set_cursor(full.cursor());
    replay.put_claim_row(claim().claim_id, full.claim(&claim().claim_id).cloned());
    for (epoch, output) in full.beacon_rows() {
        replay.put_beacon_row(*epoch, Some(*output));
    }
    replay.refresh_derived().unwrap();
    assert_eq!(replay.root(), full.root());
    assert_eq!(borsh::to_vec(&replay).unwrap(), full_bytes);
    let (same, refused) = compact.admit(&[claim()]).unwrap();
    assert_eq!(same.root(), compact.root());
    assert_eq!(refused[0].1, PanelErrorV1::DuplicateClaim);
    let mut reused = distinct_claim(1);
    reused.work_id = claim().work_id;
    let (same, refused) = compact.admit(&[reused]).unwrap();
    assert_eq!(same.root(), compact.root());
    assert_eq!(refused[0].1, PanelErrorV1::DuplicateClaim);
    let mut bad = compact.clone();
    let mut row = r.clone();
    let ClaimPhaseV3::Retired(ref mut t) = row.phase else { unreachable!() };
    t.population.as_mut().unwrap().root = h(111);
    bad.put_claim_row(claim().claim_id, Some(row));
    assert_eq!(bad.check_consistency(), Err(PanelErrorV1::InvalidCarriage));
    let corrupt = borsh::to_vec(&bad).unwrap();
    assert_eq!(
        PermissionlessPanelStateV1::import(&corrupt, compact.root(), compact.network(), compact.ruleset(), compact.policy()),
        Err(PanelErrorV1::InvalidCarriage)
    );
}

#[test]
fn retired_population_is_version_four_only_and_active_limit_is_atomic() {
    for version in [WIRE_VERSION_V2, WIRE_VERSION_COMPLETE_V1, WIRE_VERSION_RETIRED_V1] {
        let mut v = retirement_view(12);
        let mut cursor = retirement_initial(WIRE_VERSION_RETIRED_V1, 8).cursor();
        cursor.version = version;
        if version == WIRE_VERSION_V2 {
            cursor.policy.max_candidates = 16;
        }
        let mut s = retirement_advance(&PermissionlessPanelStateV1::from_cursor(cursor).unwrap(), 1, &v);
        s = s.admit(&[claim()]).unwrap().0;
        s = retirement_advance(&retirement_advance(&s, 2, &v), 3, &v);
        v.terminal.insert(claim().claim_id);
        v.retired.insert(claim().claim_id);
        s = retirement_advance(&s, 4, &v);
        assert_eq!(matches!(s.claim(&claim().claim_id).unwrap().phase, ClaimPhaseV3::Retired(_)), version == WIRE_VERSION_RETIRED_V1);
        assert_eq!(s.claim(&claim().claim_id).unwrap().snapshot.is_none(), version == WIRE_VERSION_RETIRED_V1);
    }
    let mut v = retirement_view(12);
    let mut s = retirement_initial(WIRE_VERSION_RETIRED_V1, 8);
    for n in 0..8 {
        let c = distinct_claim(n);
        s = retirement_advance(&s, n + 1, &v);
        s = s.admit(&[c.clone()]).unwrap().0;
        v.terminal.insert(c.claim_id);
        s = retirement_advance(&s, n + 1, &v);
    }
    let before = s.root();
    let order = s.cursor().next_order;
    let (same, refusal) = s.admit(&[distinct_claim(8)]).unwrap();
    assert_eq!(refusal[0].1, PanelErrorV1::ResourceLimit);
    assert_eq!(same.root(), before);
    assert_eq!(same.cursor().next_order, order);
    v.retired.insert(distinct_claim(0).claim_id);
    s = retirement_advance(&s, 9, &v);
    let (s, refusal) = s.admit(&[distinct_claim(8)]).unwrap();
    assert!(refusal.is_empty());
    assert_eq!(s.claim_rows().len(), 9);
    assert_eq!(s.work_id_rows().len(), 9);
    assert_eq!(s.claim_rows().values().filter(|r| !matches!(r.phase, ClaimPhaseV3::Retired(_))).count(), 8);
}

#[test]
fn retired_population_permanent_archive_capacity_refuses_before_writing_identity_or_order() {
    let mut v = retirement_view(0);
    let mut s = retirement_initial(WIRE_VERSION_RETIRED_V1, 8);
    s = retirement_advance(&s, 1, &v).admit(&[distinct_claim(0)]).unwrap().0;
    v.terminal.insert(distinct_claim(0).claim_id);
    v.retired.insert(distinct_claim(0).claim_id);
    s = retirement_advance(&s, 2, &v);
    let exemplar = s.claim(&distinct_claim(0).claim_id).unwrap().clone();
    for n in 1..MAX_RETIRED_ARCHIVE_CLAIMS_V1 as u64 {
        let mut r = exemplar.clone();
        r.claim = distinct_claim(n);
        r.acceptance_order = n;
        s.put_work_id_row(r.claim.work_id, true);
        s.put_claim_row(r.claim.claim_id, Some(r));
    }
    let mut cursor = s.cursor();
    cursor.next_order = MAX_RETIRED_ARCHIVE_CLAIMS_V1 as u64;
    s.set_cursor(cursor);
    s.check_consistency().unwrap();
    let before = s.root();
    let next = distinct_claim(MAX_RETIRED_ARCHIVE_CLAIMS_V1 as u64);
    let (same, refused) = s.admit(&[next.clone()]).unwrap();
    assert_eq!(refused[0].1, PanelErrorV1::ResourceLimit);
    assert_eq!(same.root(), before);
    assert_eq!(same.cursor(), s.cursor());
    assert!(!same.work_id_rows().contains(&next.work_id));
    assert!(same.claim(&next.claim_id).is_none());
}
