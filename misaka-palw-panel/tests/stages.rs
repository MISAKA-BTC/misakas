//! The staged fold a production state transition runs (`advance` against the parent base, `accept_beacon` per carried output,
//! `admit` per claim) is the same code, in the same order, as the reference `fold` — and the keyed decomposition (cursor + rows)
//! a state store journals reproduces the committed root. Fixtures mirror `replay.rs`.

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
        max_admissions_per_block: 2,
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
    terminal: BTreeSet<Hash64>,
    clock: BTreeMap<Hash64, ReceiptClockV1>,
}
impl View {
    fn new(n: u64) -> Self {
        Self { seats: (1..=n).map(seat).collect(), terminal: BTreeSet::new(), clock: BTreeMap::new() }
    }
}
impl ConsensusViewV1 for View {
    fn candidates(&self, _: &AdmittedClaimV1) -> Result<Vec<SeatCandidateV1>, PanelErrorV1> {
        Ok(self.seats.clone())
    }
    fn available_collateral(&self, _: &BondIdV1) -> u128 {
        100
    }
    fn terminal_claim(&self, c: &Hash64) -> bool {
        self.terminal.contains(c)
    }
    // FIXTURE ONLY (no production verifier accepts these bytes).
    fn verify_beacon(&self, r: &BeaconRequestV1, p: &BeaconProofV1) -> Result<(), PanelErrorV1> {
        if r.epoch != 1 || p.proof != b"fixture certificate" {
            return Err(PanelErrorV1::InvalidBeacon);
        }
        Ok(())
    }
    fn receipt_clock(&self, c: &Hash64) -> Option<ReceiptClockV1> {
        self.clock.get(c).copied()
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

/// The same step folded whole and in the three stages a production block runs them.
fn staged(
    s: &PermissionlessPanelStateV1,
    st: &SelectedChainStepV1,
    v: &View,
) -> (PermissionlessPanelStateV1, PanelFoldEventsV1, Vec<(Hash64, PanelErrorV1)>) {
    let mut prefix = st.clone();
    prefix.admissions.clear();
    prefix.beacons.clear();
    let (mut next, events) = s.advance(&prefix, v).unwrap();
    for p in &st.beacons {
        next = next.accept_beacon(p, v).unwrap();
    }
    let (next, refused) = next.admit(&st.admissions).unwrap();
    (next, events, refused)
}

#[test]
fn the_staged_fold_is_the_whole_fold_at_every_phase_of_a_claims_life() {
    let v = View::new(6);
    // Admission, sealing, the contribution window, the due assignment, a retry and exhaustion.
    let steps = vec![
        step(h(0), 1, 1, vec![claim(10)], vec![]),
        step(h(1), 2, 2, vec![], vec![]),
        step(h(2), 3, 3, vec![claim(11)], vec![]),
        step(h(3), 4, 10, vec![], vec![proof()]),
        step(h(4), 5, 13, vec![], vec![]),
        step(h(5), 6, 17, vec![], vec![]),
        step(h(6), 7, 21, vec![], vec![]),
        step(h(7), 8, 22, vec![], vec![]),
    ];
    let mut whole = initial(policy());
    let mut parts = initial(policy());
    for st in &steps {
        let (w, we) = whole.fold(st, &v).unwrap();
        let (p, pe, refused) = staged(&parts, st, &v);
        assert!(refused.is_empty());
        assert_eq!(w, p, "state at height {}", st.height);
        assert_eq!(w.root(), p.root());
        assert_eq!(we, pe, "events at height {}", st.height);
        whole = w;
        parts = p;
    }
    // The run was not vacuous: the first claim was bound, redrawn and ended; the second was sealed late and never certified.
    assert!(!whole.claim(&h(10)).unwrap().binding_history.is_empty());
}

#[test]
fn a_refused_admission_costs_only_that_claim_and_the_per_block_cap_is_refused_not_fatal() {
    let s = initial(policy());
    let (s, _) = s.advance(&step(h(0), 1, 1, vec![], vec![]), &View::new(6)).unwrap();
    let mut dup = claim(11);
    dup.work_id = claim(10).work_id; // the same work under another lookup id
    let mut zero = claim(12);
    zero.required_exposure = 0;
    let (s, refused) = s.admit(&[claim(10), dup, zero, claim(13), claim(14)]).unwrap();
    // Cap 2: claim 10 and claim 13 are admitted (the refusals before them did not spend the cap); claim 14 is over it.
    let refusals: BTreeMap<_, _> = refused.into_iter().collect();
    assert_eq!(refusals[&h(11)], PanelErrorV1::DuplicateClaim);
    assert_eq!(refusals[&h(12)], PanelErrorV1::InvalidSnapshot);
    assert_eq!(refusals[&h(14)], PanelErrorV1::ResourceLimit);
    assert_eq!(refusals.len(), 3);
    assert_eq!(s.claims().map(|(id, _)| *id).collect::<Vec<_>>(), vec![h(10), h(13)]);
    // Acceptance order is the chain's, in the order given; the cursor's block is the accepting block.
    assert_eq!(s.claim(&h(10)).unwrap().acceptance_order, 0);
    assert_eq!(s.claim(&h(13)).unwrap().acceptance_order, 1);
    assert_eq!(s.claim(&h(13)).unwrap().accepted_block, h(1));
    s.check_consistency().unwrap();
}

#[test]
fn advance_refuses_a_step_that_carries_admissions_or_outputs_and_accept_beacon_is_transactional() {
    let v = View::new(6);
    let s = initial(policy());
    assert_eq!(s.advance(&step(h(0), 1, 1, vec![claim(10)], vec![]), &v).unwrap_err(), PanelErrorV1::NoncanonicalStep);
    assert_eq!(s.advance(&step(h(0), 1, 1, vec![], vec![proof()]), &v).unwrap_err(), PanelErrorV1::NoncanonicalStep);
    // A refused output leaves the state as it was; the next output is judged on its own.
    let (s, _) = s.advance(&step(h(0), 1, 1, vec![], vec![]), &v).unwrap();
    let (s, _) = s.admit(&[claim(10)]).unwrap();
    let root = s.root();
    let mut bad = proof();
    bad.proof = b"forged".to_vec();
    assert_eq!(s.accept_beacon(&bad, &v).unwrap_err(), PanelErrorV1::InvalidBeacon);
    assert_eq!(s.root(), root);
}

#[test]
fn the_cursor_and_the_keyed_rows_reproduce_the_committed_state() {
    let v = View::new(6);
    let mut s = initial(policy());
    for st in [
        step(h(0), 1, 1, vec![claim(10), claim(11)], vec![]),
        step(h(1), 2, 2, vec![], vec![]),
        step(h(2), 3, 3, vec![], vec![]),
        step(h(3), 4, 10, vec![], vec![proof()]),
        step(h(4), 5, 13, vec![], vec![]),
    ] {
        s = s.fold(&st, &v).unwrap().0;
    }
    assert!(s.beacon_rows().contains_key(&1), "the run holds a certified output");
    assert!(s.live_claims().count() > 0);
    // Rebuild from the decomposition a store journals.
    let mut t = PermissionlessPanelStateV1::from_cursor(s.cursor()).unwrap();
    for (id, record) in s.claim_rows() {
        t.put_claim_row(*id, Some(record.clone()));
    }
    for id in s.work_id_rows() {
        t.put_work_id_row(*id, true);
    }
    for (epoch, output) in s.beacon_rows() {
        t.put_beacon_row(*epoch, Some(*output));
    }
    t.refresh_derived().unwrap();
    t.check_consistency().unwrap();
    assert_eq!(t, s);
    assert_eq!(t.root(), s.root());
    // …and a row undone is a row undone: removing one claim, its work id and the output changes the root, restoring them does not.
    let mut u = t.clone();
    u.put_claim_row(h(11), None);
    u.put_work_id_row(h(111), false);
    u.put_beacon_row(1, None);
    u.refresh_derived().unwrap();
    assert_ne!(u.root(), s.root());
    u.put_claim_row(h(11), s.claim(&h(11)).cloned());
    u.put_work_id_row(h(111), true);
    u.put_beacon_row(1, Some(h(42)));
    u.refresh_derived().unwrap();
    assert_eq!(u.root(), s.root());
}

#[test]
fn the_hosts_receipt_clock_pauses_and_rebases_the_engines_timeout() {
    let mut v = View::new(6);
    let mut s = initial(policy());
    for st in [
        step(h(0), 1, 1, vec![claim(10)], vec![]),
        step(h(1), 2, 2, vec![], vec![]),
        step(h(2), 3, 3, vec![], vec![]),
        step(h(3), 4, 10, vec![], vec![proof()]),
        step(h(4), 5, 13, vec![], vec![]),
    ] {
        s = s.fold(&st, &v).unwrap().0;
    }
    assert!(matches!(s.claim(&h(10)).unwrap().phase, ClaimPhaseV3::Bound(_)));
    // The engine's own window (13 + 3) would redraw at DAA 17. A paused clock draws nothing, however late.
    v.clock.insert(h(10), ReceiptClockV1::Paused);
    let (paused, e) = s.fold(&step(h(5), 6, 30, vec![], vec![]), &v).unwrap();
    assert!(e.bindings.is_empty());
    assert_eq!(paused.claim(&h(10)).unwrap().binding_history.len(), 1);
    // A host that re-based the window to a later bound DAA delays the redraw to that window's end…
    v.clock.insert(h(10), ReceiptClockV1::Running { bound_daa: 20 });
    assert!(s.fold(&step(h(5), 6, 22, vec![], vec![]), &v).unwrap().1.bindings.is_empty());
    // …and the redraw happens once it has elapsed, with the retry index and the original seed.
    let (retried, e) = s.fold(&step(h(5), 6, 24, vec![], vec![]), &v).unwrap();
    assert_eq!(e.bindings[&h(10)].retry_index, 1);
    assert_eq!(retried.claim(&h(10)).unwrap().binding_history.len(), 2);
}
