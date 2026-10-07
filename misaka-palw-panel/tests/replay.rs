use kaspa_hashes::Hash64;
use misaka_palw_panel::*;
use std::collections::{BTreeMap, BTreeSet};

fn h(i: u64) -> Hash64 {
    Hash64::from_u64_word(i)
}
fn bond(i: u64) -> BondIdV1 {
    BondIdV1 { transaction: h(1000 + i), index: 0 }
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
        seat_count: 2,
        outsider_seats: 0,
        max_retries: 1,
        min_collateral: 100,
        max_candidates: 16,
        max_pending: 8,
        max_pending_per_bond: 4,
        max_assignments_per_block: 8,
        max_admissions_per_block: 8,
        max_tracked_claims: 100,
        max_beacons_per_block: 2,
        max_beacon_proof_bytes: 100,
        beacon_scheme: h(777),
    }
}
fn claim(i: u64) -> AdmittedClaimV1 {
    AdmittedClaimV1 {
        claim_id: h(i),
        work_id: h(i + 100),
        class_id: h(900),
        producer: bond(900),
        producer_operator: h(900),
        producer_key: h(901),
        immutable_fields: h(i + 200),
        required_exposure: 100,
    }
}
fn seat(i: u64) -> SeatCandidateV1 {
    SeatCandidateV1 {
        bond: bond(i),
        operator: h(i),
        key: h(i + 50),
        collateral: 100,
        registered_daa: 0,
        capability_root: h(2),
        readiness_root: h(3),
        roles: CLASS_ROLE_V1,
    }
}

#[derive(Clone)]
struct View {
    seats: Vec<SeatCandidateV1>,
    free: BTreeMap<BondIdV1, u128>,
    terminal: BTreeSet<Hash64>,
    accept_proof: bool,
}
impl View {
    fn new(n: u64) -> Self {
        let seats: Vec<_> = (1..=n).map(seat).collect();
        let free = seats.iter().map(|s| (s.bond, s.collateral as u128)).collect();
        Self { seats, free, terminal: BTreeSet::new(), accept_proof: true }
    }
}
impl ConsensusViewV1 for View {
    fn candidates(&self, _: &AdmittedClaimV1) -> Result<Vec<SeatCandidateV1>, PanelErrorV1> {
        Ok(self.seats.clone())
    }
    fn available_collateral(&self, b: &BondIdV1) -> u128 {
        self.free.get(b).copied().unwrap_or(0)
    }
    fn terminal_claim(&self, c: &Hash64) -> bool {
        self.terminal.contains(c)
    }
    // FIXTURE ONLY. No production verifier accepts these bytes. Assert the whole request is passed.
    fn verify_beacon(&self, r: &BeaconRequestV1, p: &BeaconProofV1) -> Result<(), PanelErrorV1> {
        if !self.accept_proof
            || r.network != h(700)
            || r.ruleset != h(701)
            || r.scheme != h(777)
            || r.epoch != 1
            || r.release_daa != 10
            || r.deadline_daa != 12
            || p.proof != b"fixture certificate"
        {
            return Err(PanelErrorV1::InvalidBeacon);
        }
        Ok(())
    }
}
fn initial(p: PanelPolicyV1) -> PermissionlessPanelStateV1 {
    PermissionlessPanelStateV1::new(h(700), h(701), p, h(0), 0, 0).unwrap()
}
fn step(parent: Hash64, height: u64, daa: u64, admissions: Vec<AdmittedClaimV1>, beacons: Vec<BeaconProofV1>) -> SelectedChainStepV1 {
    SelectedChainStepV1 { block: h(height), parent, height, daa, admissions, beacons }
}
fn proof() -> BeaconProofV1 {
    BeaconProofV1 { epoch: 1, output: h(42), proof: b"fixture certificate".to_vec() }
}
fn sealed(p: PanelPolicyV1, view: &View, claims: Vec<AdmittedClaimV1>) -> PermissionlessPanelStateV1 {
    let s = initial(p).fold(&step(h(0), 1, 1, claims, vec![]), view).unwrap().0;
    let s = s.fold(&step(h(1), 2, 2, vec![], vec![]), view).unwrap().0;
    s.fold(&step(h(2), 3, 3, vec![], vec![]), view).unwrap().0
}
fn certified(p: PanelPolicyV1, view: &View, claims: Vec<AdmittedClaimV1>) -> PermissionlessPanelStateV1 {
    sealed(p, view, claims).fold(&step(h(3), 4, 10, vec![], vec![proof()]), view).unwrap().0
}
fn bound(p: PanelPolicyV1, view: &View) -> PermissionlessPanelStateV1 {
    certified(p, view, vec![claim(10)]).fold(&step(h(4), 5, 13, vec![], vec![]), view).unwrap().0
}
fn binding(s: &PermissionlessPanelStateV1, id: u64) -> &PanelBoundV3 {
    match &s.claim(&h(id)).unwrap().phase {
        ClaimPhaseV3::Bound(b) => b,
        other => panic!("{other:?}"),
    }
}

#[test]
fn two_carriers_including_heartbeat_have_identical_panel_and_reservations() {
    let v = View::new(6);
    let s = certified(policy(), &v, vec![claim(10)]);
    let a = step(h(4), 5, 13, vec![], vec![]); // valid bonded attempt, caller validated its lane
    let mut b = a.clone();
    b.block = h(500); // valid heartbeat, same canonical pre-object base
    let (sa, ea) = s.fold(&a, &v).unwrap();
    let (sb, eb) = s.fold(&b, &v).unwrap();
    let ba = &ea.bindings[&h(10)];
    let bb = &eb.bindings[&h(10)];
    assert_eq!(ba.panel_seed_v3, bb.panel_seed_v3);
    assert_eq!(ba.seats, bb.seats);
    assert_ne!(ba.binding_block, bb.binding_block);
    for seat in &ba.seats {
        assert_eq!(sa.reserved(seat), 100);
        assert_eq!(sb.reserved(seat), 100);
    }
    assert_eq!(ba.assignment_point, 13);
}

#[test]
fn signature_equivalent_lookup_ids_do_not_change_seal_or_draw() {
    let v = View::new(6);
    let a = claim(10);
    let mut b = a.clone();
    b.claim_id = h(99);
    let sa = certified(policy(), &v, vec![a]).fold(&step(h(4), 5, 13, vec![], vec![]), &v).unwrap().0;
    let sb = certified(policy(), &v, vec![b]).fold(&step(h(4), 5, 13, vec![], vec![]), &v).unwrap().0;
    assert_eq!(binding(&sa, 10).claim_seal_id, binding(&sb, 99).claim_seal_id);
    assert_eq!(binding(&sa, 10).panel_seed_v3, binding(&sb, 99).panel_seed_v3);
    assert_eq!(binding(&sa, 10).seats, binding(&sb, 99).seats);
}

#[test]
fn post_snapshot_registration_split_topup_and_readiness_cannot_change_draw() {
    let v = View::new(6);
    let s = certified(policy(), &v, vec![claim(10)]);
    let mut late = v.clone();
    late.seats.push(seat(7));
    late.free.insert(bond(7), 1000);
    late.seats[0].collateral = 1000;
    late.seats[0].readiness_root = h(888);
    late.free.insert(bond(1), 1000);
    let a = s.fold(&step(h(4), 5, 13, vec![], vec![]), &v).unwrap().0;
    let b = s.fold(&step(h(4), 5, 13, vec![], vec![]), &late).unwrap().0;
    assert_eq!(a.root(), b.root());
    assert!(!binding(&b, 10).seats.contains(&bond(7)));
}

#[test]
fn canonical_acceptance_order_reserves_live_exposure_atomically_without_hash_priority() {
    let v = View::new(2);
    let s = certified(policy(), &v, vec![claim(9000), claim(1)]);
    let (s, e) = s.fold(&step(h(4), 5, 13, vec![], vec![]), &v).unwrap();
    assert_eq!(e.bindings.keys().copied().collect::<Vec<_>>(), vec![h(9000)]);
    assert_eq!(e.non_fraud_voids[&h(1)], NonFraudReasonV1::NoCapablePanel);
    for b in &binding(&s, 9000).seats {
        assert_eq!(s.reserved(b), 100);
    }
}

#[test]
fn backlog_cap_cannot_select_a_producer_chosen_subset() {
    let mut p = policy();
    p.max_assignments_per_block = 1;
    let v = View::new(6);
    let s = certified(p, &v, vec![claim(9000), claim(1)]);
    let (s, e) = s.fold(&step(h(4), 5, 13, vec![], vec![]), &v).unwrap();
    assert!(e.bindings.contains_key(&h(9000)));
    assert!(!e.bindings.contains_key(&h(1)));
    let (s, e) = s.fold(&step(h(5), 6, 14, vec![], vec![]), &v).unwrap();
    assert!(e.bindings.contains_key(&h(1)));
    let a: BTreeSet<_> = binding(&s, 9000).seats.iter().collect();
    assert!(binding(&s, 1).seats.iter().all(|b| !a.contains(b)));
}

#[test]
fn missing_beacon_terminates_without_fraud_or_fallback_seed() {
    let v = View::new(6);
    let s = sealed(policy(), &v, vec![claim(10)]);
    let (s, e) = s.fold(&step(h(3), 4, 13, vec![], vec![]), &v).unwrap();
    assert_eq!(e.non_fraud_voids[&h(10)], NonFraudReasonV1::BeaconUnavailable);
    assert!(e.bindings.is_empty());
    assert!(s.claim(&h(10)).unwrap().binding_history.is_empty());
    assert_eq!(s.reserved(&bond(1)), 0);
}

#[test]
fn invalid_conflicting_early_late_or_oversized_beacon_is_rejected_transactionally() {
    let v = View::new(6);
    let s = sealed(policy(), &v, vec![claim(10)]);
    let root = s.root();
    let mut bad = v.clone();
    bad.accept_proof = false;
    assert_eq!(s.fold(&step(h(3), 4, 10, vec![], vec![proof()]), &bad).unwrap_err(), PanelErrorV1::InvalidBeacon);
    for daa in [9, 13] {
        assert_eq!(s.fold(&step(h(3), 4, daa, vec![], vec![proof()]), &v).unwrap_err(), PanelErrorV1::InvalidBeacon);
    }
    let mut large = proof();
    large.proof = vec![0; 101];
    assert_eq!(s.fold(&step(h(3), 4, 10, vec![], vec![large]), &v).unwrap_err(), PanelErrorV1::ResourceLimit);
    let certified = s.fold(&step(h(3), 4, 10, vec![], vec![proof()]), &v).unwrap().0;
    let mut conflict = proof();
    conflict.output = h(43);
    assert_eq!(certified.fold(&step(h(4), 5, 11, vec![], vec![conflict]), &v).unwrap_err(), PanelErrorV1::BeaconEquivocation);
    assert_eq!(s.root(), root);
}

#[test]
fn retries_use_original_seed_non_reused_public_seats_and_release_on_exhaustion() {
    let v = View::new(6);
    let first = bound(policy(), &v);
    let b0 = binding(&first, 10).clone();
    let (second, _) = first.fold(&step(h(5), 6, 17, vec![], vec![]), &v).unwrap();
    let b1 = binding(&second, 10);
    assert_eq!(b1.retry_index, 1);
    assert_eq!(b0.panel_seed_v3, b1.panel_seed_v3);
    assert!(b1.seats.iter().all(|b| !b0.seats.contains(b)));
    for b in &b0.seats {
        assert_eq!(second.reserved(b), 0);
    }
    let (end, events) = second.fold(&step(h(6), 7, 21, vec![], vec![]), &v).unwrap();
    assert_eq!(events.non_fraud_voids[&h(10)], NonFraudReasonV1::PanelUnavailable);
    for i in 1..=6 {
        assert_eq!(end.reserved(&bond(i)), 0);
    }
}

#[test]
fn timeout_carrier_cannot_change_retry_and_newly_registered_seats_cannot_join_it() {
    let v = View::new(6);
    let s = bound(policy(), &v);
    let a = step(h(5), 6, 17, vec![], vec![]);
    let mut b = a.clone();
    b.block = h(600);
    let mut late = v.clone();
    late.seats.push(seat(7));
    late.free.insert(bond(7), 1000);
    let sa = s.fold(&a, &v).unwrap().0;
    let sb = s.fold(&b, &late).unwrap().0;
    assert_eq!(binding(&sa, 10).seats, binding(&sb, 10).seats);
    assert_eq!(binding(&sa, 10).panel_seed_v3, binding(&sb, 10).panel_seed_v3);
}

#[test]
fn replay_reorg_and_pruning_carriage_reproduce_identical_roots() {
    let v = View::new(6);
    let parent = certified(policy(), &v, vec![claim(10)]);
    let before = parent.root();
    let bytes = borsh::to_vec(&parent).unwrap();
    let loaded = PermissionlessPanelStateV1::import(&bytes, before, h(700), h(701), policy()).unwrap();
    let step = step(h(4), 5, 13, vec![], vec![]);
    let child = parent.fold(&step, &v).unwrap();
    let replayed = loaded.fold(&step, &v).unwrap();
    assert_eq!(child, replayed);
    assert_eq!(parent.root(), before); // reorg selects the retained parent
    let json = serde_json::to_value(&child.0).unwrap();
    assert_eq!(json["version"], 1);
    assert!(json["claims"].as_object().unwrap().values().next().unwrap()["phase"]["Bound"]["panelSeedV3"].is_string());
    assert_eq!(
        PermissionlessPanelStateV1::import(&bytes, h(999), h(700), h(701), policy()).unwrap_err(),
        PanelErrorV1::InvalidCarriage
    );
    assert!(PermissionlessPanelStateV1::import(&bytes, before, h(701), h(701), policy()).is_err());
    let mut changed = policy();
    changed.receipt_window_daa += 1;
    assert!(PermissionlessPanelStateV1::import(&bytes, before, h(700), h(701), changed).is_err());
}

#[test]
fn duplicate_work_stays_spent_after_non_fraud_expiry() {
    let v = View::new(6);
    let s = sealed(policy(), &v, vec![claim(10)]);
    let s = s.fold(&step(h(3), 4, 13, vec![], vec![]), &v).unwrap().0;
    let mut copy = claim(10);
    copy.claim_id = h(999);
    assert_eq!(s.fold(&step(h(4), 5, 14, vec![copy], vec![]), &v).unwrap_err(), PanelErrorV1::DuplicateClaim);
}

#[test]
fn executor_exclusions_and_maturity_are_enforced_before_entropy() {
    for mutate in [0, 1, 2, 3, 4] {
        let mut v = View::new(6);
        match mutate {
            0 => v.seats[0].bond = claim(10).producer,
            1 => v.seats[0].operator = claim(10).producer_operator,
            2 => v.seats[0].key = claim(10).producer_key,
            3 => v.seats[0].registered_daa = 2,
            _ => v.seats.push(v.seats[0].clone()),
        }
        let s = initial(policy()).fold(&step(h(0), 1, 1, vec![claim(10)], vec![]), &v).unwrap().0;
        let s = s.fold(&step(h(1), 2, 2, vec![], vec![]), &v).unwrap().0;
        assert!(matches!(s.fold(&step(h(2), 3, 3, vec![], vec![]), &v), Err(PanelErrorV1::InvalidSnapshot)));
    }
}

#[test]
fn same_key_cannot_sit_twice_and_outsider_role_is_frozen() {
    let mut p = policy();
    p.outsider_seats = 1;
    let mut v = View::new(6);
    v.seats[0].roles = OUTSIDER_ROLE_V1;
    v.seats[1].operator = v.seats[0].operator; // another bond for the same key/identity
    let s = bound(p, &v);
    let b = binding(&s, 10);
    assert_eq!(b.seats[0], bond(1));
    assert_ne!(b.seats[1], bond(2));
}

#[test]
fn same_operator_collateral_split_keeps_exact_first_race_weight() {
    let v = View::new(6);
    let s = sealed(policy(), &v, vec![claim(10)]);
    let snap = s.claim(&h(10)).unwrap().snapshot.as_ref().unwrap();
    let mut whole = snap.clone();
    whole.candidates[0].collateral = 1000;
    let mut split = snap.clone();
    split.candidates[0].collateral = 100;
    for i in 0..9 {
        let mut small = split.candidates[0].clone();
        small.bond = bond(100 + i);
        split.candidates.push(small);
    }
    let op = |snapshot: &PanelSnapshotV1, b: BondIdV1| snapshot.candidates.iter().find(|s| s.bond == b).unwrap().operator;
    for i in 0..1000 {
        let a = seat_order_v1(&whole, h(i), 0, CLASS_ROLE_V1, &[]).unwrap()[0];
        let b = seat_order_v1(&split, h(i), 0, CLASS_ROLE_V1, &[]).unwrap()[0];
        assert_eq!(op(&whole, a), op(&split, b));
    }
}

#[test]
fn certified_terminal_outcome_releases_without_changing_payout_ownership() {
    let mut v = View::new(6);
    let s = bound(policy(), &v);
    v.terminal.insert(h(10));
    let (s, e) = s.fold(&step(h(5), 6, 14, vec![], vec![]), &v).unwrap();
    assert_eq!(e.released, vec![h(10)]);
    assert!(s.claim(&h(10)).unwrap().phase.terminal());
    assert!(e.non_fraud_voids.is_empty());
    for i in 1..=6 {
        assert_eq!(s.reserved(&bond(i)), 0);
    }
}

#[test]
fn never_sealed_claim_cannot_pin_collateral_and_missing_source_is_not_success() {
    let v = View::new(6);
    let mut p = policy();
    p.seal_depth_blocks = 100;
    p.seal_wait_daa = 2;
    let s = initial(p).fold(&step(h(0), 1, 1, vec![claim(10)], vec![]), &v).unwrap().0;
    let (_, e) = s.fold(&step(h(1), 2, 4, vec![], vec![]), &v).unwrap();
    assert_eq!(e.non_fraud_voids[&h(10)], NonFraudReasonV1::SealUnavailable);
    let mut bad = policy();
    bad.beacon_scheme = Hash64::default();
    assert!(bad.validate().is_err());
}

#[test]
fn resource_and_chain_limits_are_enforced_before_mutation() {
    let v = View::new(6);
    let mut p = policy();
    p.max_pending_per_bond = 1;
    let s = initial(p);
    let root = s.root();
    assert_eq!(s.fold(&step(h(0), 1, 1, vec![claim(10), claim(11)], vec![]), &v).unwrap_err(), PanelErrorV1::ResourceLimit);
    assert_eq!(s.fold(&step(h(999), 1, 1, vec![], vec![]), &v).unwrap_err(), PanelErrorV1::NoncanonicalStep);
    assert_eq!(s.root(), root);
    let mut overflow = policy();
    overflow.beacon_period_daa = u64::MAX;
    assert!(overflow.validate().is_ok()); // Checked schedule arithmetic refuses it when encountered.
}

#[test]
fn malformed_bound_carriage_cannot_bypass_invariants_or_panic_the_fold() {
    let v = View::new(6);
    let s = bound(policy(), &v);
    let old = borsh::to_vec(s.claim(&h(10)).unwrap()).unwrap();
    let mut malformed = s.claim(&h(10)).unwrap().clone();
    malformed.seal = None;
    malformed.snapshot = None;
    let replacement = borsh::to_vec(&malformed).unwrap();
    let bytes = borsh::to_vec(&s).unwrap();
    let offsets: Vec<_> = bytes.windows(old.len()).enumerate().filter_map(|(i, w)| (w == old).then_some(i)).collect();
    assert_eq!(offsets.len(), 1);
    let start = offsets[0];
    let mut bad = bytes[..start].to_vec();
    bad.extend(replacement);
    bad.extend_from_slice(&bytes[start + old.len()..]);
    // Even a caller providing this malformed state's own commitment cannot make it a valid import.
    let decoded: PermissionlessPanelStateV1 = borsh::from_slice(&bad).unwrap();
    assert_eq!(
        PermissionlessPanelStateV1::import(&bad, decoded.root(), h(700), h(701), policy()).unwrap_err(),
        PanelErrorV1::InvalidCarriage
    );
    assert_eq!(decoded.fold(&step(h(5), 6, 14, vec![], vec![]), &v).unwrap_err(), PanelErrorV1::InvalidCarriage);
}

#[test]
fn new_admission_in_the_binder_cannot_compete_with_preexisting_due_claims() {
    let v = View::new(2);
    let s = certified(policy(), &v, vec![claim(9000)]);
    let (s, e) = s.fold(&step(h(4), 5, 13, vec![claim(1)], vec![]), &v).unwrap();
    assert!(e.bindings.contains_key(&h(9000)));
    assert_eq!(s.claim(&h(1)).unwrap().phase, ClaimPhaseV3::PendingSeal);
    assert!(s.claim(&h(1)).unwrap().snapshot.is_none());
}
