//! **The stratified draw** (RFC-0006's per-shard Panel under RFC-0010's permissionless rule): a claim admitted with a
//! `PanelStrataV1` is drawn stratum by stratum from its own claim-sealed seed — each stratum `[outsider?] ++ class seats`, the
//! class seats from the stratum's members, operators distinct inside a stratum, outsiders distinct across the round,
//! alternates never reused per stratum — and every rule is re-checked by the carriage's own validation. A flat claim is
//! exactly what it was. Fixtures mirror `stages.rs` (the staged fold a production block runs).

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
        max_candidates: 32,
        max_pending: 8,
        max_pending_per_bond: 4,
        max_assignments_per_block: 8,
        max_admissions_per_block: 4,
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

/// Two strata, three class seats each, an outsider each.
const STRATA: PanelStrataV1 = PanelStrataV1 { count: 2, class_seats: 3, outsider: true };

/// A candidate `i` (operator `i`) with its roles and stratum bits.
fn cand(i: u64, roles: u8) -> SeatCandidateV1 {
    SeatCandidateV1 {
        bond: bond(i),
        operator: h(i),
        key: h(i + 50),
        collateral: 100,
        registered_daa: 0,
        capability_root: h(2),
        readiness_root: h(3),
        roles,
    }
}

#[derive(Clone)]
struct View {
    /// `(candidate, stratum bits)`; the CLASS role is set iff the bits are.
    population: Vec<(SeatCandidateV1, u64)>,
    free: BTreeMap<BondIdV1, u128>,
    terminal: BTreeSet<Hash64>,
    clock: BTreeMap<Hash64, ReceiptClockV1>,
}

impl View {
    /// Stratum 0 is held by `s0`, stratum 1 by `s1`, the outsider population is `out` (the base class's).
    fn new(s0: &[u64], s1: &[u64], out: &[u64]) -> Self {
        let mut ids: BTreeSet<u64> = BTreeSet::new();
        ids.extend(s0.iter().chain(s1).chain(out));
        let population: Vec<(SeatCandidateV1, u64)> = ids
            .into_iter()
            .map(|i| {
                let bits = u64::from(s0.contains(&i)) | (u64::from(s1.contains(&i)) << 1);
                let roles = if bits != 0 { CLASS_ROLE_V1 } else { 0 } | if out.contains(&i) { OUTSIDER_ROLE_V1 } else { 0 };
                (cand(i, roles), bits)
            })
            .collect();
        let free = population.iter().map(|(c, _)| (c.bond, c.collateral as u128)).collect();
        Self { population, free, terminal: BTreeSet::new(), clock: BTreeMap::new() }
    }
}

impl ConsensusViewV1 for View {
    fn candidates(&self, _: &AdmittedClaimV1) -> Result<Vec<SeatCandidateV1>, PanelErrorV1> {
        // The flat population: the class members (a flat claim of the same fixture).
        Ok(self
            .population
            .iter()
            .filter(|(c, _)| c.roles & CLASS_ROLE_V1 != 0)
            .map(|(c, _)| SeatCandidateV1 { roles: CLASS_ROLE_V1, ..c.clone() })
            .collect())
    }
    fn stratified_candidates(
        &self,
        _: &AdmittedClaimV1,
        strata: &PanelStrataV1,
    ) -> Result<(Vec<SeatCandidateV1>, Vec<u64>), PanelErrorV1> {
        assert_eq!(*strata, STRATA, "the engine asks for the claim's own strata");
        // Handed over in REVERSE bond order: the engine sorts the population and keeps the bits aligned.
        Ok(self.population.iter().rev().cloned().unzip())
    }
    fn available_collateral(&self, b: &BondIdV1) -> u128 {
        self.free.get(b).copied().unwrap_or(0)
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

fn step(parent: Hash64, height: u64, daa: u64) -> SelectedChainStepV1 {
    SelectedChainStepV1 { block: h(height), parent, height, daa, admissions: vec![], beacons: vec![] }
}
fn proof() -> BeaconProofV1 {
    BeaconProofV1 { epoch: 1, output: h(42), proof: b"fixture certificate".to_vec() }
}

/// Admit `claims` (with their strata) at height 1, seal at height 3, certify epoch 1 at DAA 10 (height 4), and run the due draw at
/// DAA 13 (height 5).
fn drive(
    v: &impl ConsensusViewV1,
    claims: &[(AdmittedClaimV1, Option<PanelStrataV1>)],
) -> (PermissionlessPanelStateV1, PanelFoldEventsV1) {
    let s = PermissionlessPanelStateV1::new(h(700), h(701), policy(), h(0), 0, 0).unwrap();
    let (s, _) = s.advance(&step(h(0), 1, 1), v).unwrap();
    let (s, refused) = s.admit_with_strata(claims).unwrap();
    assert!(refused.is_empty(), "{refused:?}");
    let (s, _) = s.advance(&step(h(1), 2, 2), v).unwrap();
    let (s, _) = s.advance(&step(h(2), 3, 3), v).unwrap();
    assert!(claims.iter().all(|(c, _)| s.claim(&c.claim_id).unwrap().phase == ClaimPhaseV3::Sealed));
    let (s, _) = s.advance(&step(h(3), 4, 10), v).unwrap();
    let s = s.accept_beacon(&proof(), v).unwrap();
    let (s, events) = s.advance(&step(h(4), 5, 13), v).unwrap();
    s.check_consistency().unwrap();
    (s, events)
}

fn binding(s: &PermissionlessPanelStateV1, id: u64) -> PanelBoundV3 {
    match &s.claim(&h(id)).unwrap().phase {
        ClaimPhaseV3::Bound(b) => b.clone(),
        other => panic!("{other:?}"),
    }
}

fn id_of(b: &BondIdV1) -> u64 {
    (1..=64).find(|i| bond(*i) == *b).expect("a fixture bond")
}

#[test]
fn a_stratified_claim_binds_stratum_by_stratum_from_its_members_with_an_outsider_each() {
    // Stratum 0: 1..=6, stratum 1: 4..=9 (4..=6 hold both), outsiders from 10..=14 and 1 (one operator in both populations).
    let v = View::new(&[1, 2, 3, 4, 5, 6], &[4, 5, 6, 7, 8, 9], &[1, 10, 11, 12, 13, 14]);
    let (s, events) = drive(&v, &[(claim(10), Some(STRATA))]);
    let b = binding(&s, 10);
    assert_eq!(events.bindings[&h(10)], b);
    assert_eq!(b.seats.len(), 8, "2 strata x (outsider + 3 class seats)");
    let seats: Vec<u64> = b.seats.iter().map(id_of).collect();
    for (stratum, slice) in seats.chunks(4).enumerate() {
        let members: Vec<u64> = if stratum == 0 { (1..=6).collect() } else { (4..=9).collect() };
        assert!([1, 10, 11, 12, 13, 14].contains(&slice[0]), "stratum {stratum}: its first seat is an outsider: {slice:?}");
        for class_seat in &slice[1..] {
            assert!(members.contains(class_seat), "stratum {stratum}: {class_seat} holds the stratum");
        }
        let distinct: BTreeSet<u64> = slice.iter().copied().collect();
        assert_eq!(distinct.len(), 4, "operators distinct inside a stratum, the outsider not a class seat: {slice:?}");
    }
    assert_ne!(seats[0], seats[4], "one operator holds at most one outsider seat of a round");
    // The snapshot froze the bits aligned with the sorted candidates.
    let record = s.claim(&h(10)).unwrap();
    let snapshot = record.snapshot.as_ref().unwrap();
    assert_eq!(snapshot.strata_members.len(), snapshot.candidates.len());
    for (c, bits) in snapshot.candidates.iter().zip(&snapshot.strata_members) {
        let i = id_of(&c.bond);
        assert_eq!(*bits & 1 != 0, (1..=6).contains(&i));
        assert_eq!(*bits & 2 != 0, (4..=9).contains(&i));
    }
    assert_eq!(record.strata, Some(STRATA));
    assert_eq!(record.used_operators, b.seats.iter().map(|x| h(id_of(x))).collect::<Vec<_>>());
    // Deterministic: the same chain draws the same Panel.
    assert_eq!(drive(&v, &[(claim(10), Some(STRATA))]).0.root(), s.root());
}

#[test]
fn a_flat_claim_is_untouched_and_its_seal_differs_from_a_stratified_twin() {
    let v = View::new(&[1, 2, 3, 4, 5, 6], &[4, 5, 6, 7, 8, 9], &[10, 11, 12]);
    // `admit` is `admit_with_strata(None)` exactly.
    let s = PermissionlessPanelStateV1::new(h(700), h(701), policy(), h(0), 0, 0).unwrap();
    let (s, _) = s.advance(&step(h(0), 1, 1), &v).unwrap();
    let (a, _) = s.admit(&[claim(10)]).unwrap();
    let (b, _) = s.admit_with_strata(&[(claim(10), None)]).unwrap();
    assert_eq!(a.root(), b.root());
    let (flat, _) = drive(&v, &[(claim(10), None)]);
    let fb = binding(&flat, 10);
    assert_eq!(fb.seats.len(), policy().seat_count as usize);
    assert!(flat.claim(&h(10)).unwrap().snapshot.as_ref().unwrap().strata_members.is_empty());
    let (stratified, _) = drive(&v, &[(claim(10), Some(STRATA))]);
    let (fs, ss) = (flat.claim(&h(10)).unwrap().seal.clone().unwrap(), stratified.claim(&h(10)).unwrap().seal.clone().unwrap());
    assert_ne!(fs.id, ss.id, "a stratified seal names its strata");
}

#[test]
fn a_stratum_short_of_members_or_outsiders_ends_the_claim_no_capable_panel() {
    // Stratum 1 has two members: no stratum-1 Panel of three class seats.
    let thin = View::new(&[1, 2, 3, 4], &[5, 6], &[10, 11, 12]);
    let (s, events) = drive(&thin, &[(claim(10), Some(STRATA))]);
    assert_eq!(events.non_fraud_voids[&h(10)], NonFraudReasonV1::NoCapablePanel);
    assert!(matches!(s.claim(&h(10)).unwrap().phase, ClaimPhaseV3::Voided { reason: NonFraudReasonV1::NoCapablePanel, .. }));
    assert_eq!(s.reserved(&bond(1)), 0, "nothing is reserved for a Panel that was not drawn");
    // One outsider for two strata: the second stratum has none left.
    let one_outsider = View::new(&[1, 2, 3, 4], &[4, 5, 6, 7], &[10]);
    let (_, events) = drive(&one_outsider, &[(claim(10), Some(STRATA))]);
    assert_eq!(events.non_fraud_voids[&h(10)], NonFraudReasonV1::NoCapablePanel);
    // A host that cannot answer for strata (the default) seals an empty population: non-fraud, never a failed fold.
    struct Flat(View);
    impl ConsensusViewV1 for Flat {
        fn candidates(&self, c: &AdmittedClaimV1) -> Result<Vec<SeatCandidateV1>, PanelErrorV1> {
            self.0.candidates(c)
        }
        fn available_collateral(&self, b: &BondIdV1) -> u128 {
            self.0.available_collateral(b)
        }
        fn terminal_claim(&self, c: &Hash64) -> bool {
            self.0.terminal_claim(c)
        }
        fn verify_beacon(&self, r: &BeaconRequestV1, p: &BeaconProofV1) -> Result<(), PanelErrorV1> {
            self.0.verify_beacon(r, p)
        }
    }
    let host = Flat(View::new(&[1, 2, 3, 4], &[4, 5, 6, 7], &[10, 11]));
    let (_, events) = drive(&host, &[(claim(10), Some(STRATA))]);
    assert_eq!(events.non_fraud_voids[&h(10)], NonFraudReasonV1::NoCapablePanel);
}

#[test]
fn a_bond_seated_in_two_strata_is_reserved_once_and_needs_headroom_once() {
    // Stratum 0 = {1, 2, 5}, stratum 1 = {5, 6, 7}: bond 5 must sit in both. Its free collateral covers ONE exposure.
    let strata = PanelStrataV1 { count: 2, class_seats: 3, outsider: false };
    struct Two(View);
    impl ConsensusViewV1 for Two {
        fn candidates(&self, c: &AdmittedClaimV1) -> Result<Vec<SeatCandidateV1>, PanelErrorV1> {
            self.0.candidates(c)
        }
        fn stratified_candidates(
            &self,
            _: &AdmittedClaimV1,
            _: &PanelStrataV1,
        ) -> Result<(Vec<SeatCandidateV1>, Vec<u64>), PanelErrorV1> {
            Ok(self.0.population.iter().cloned().unzip())
        }
        fn available_collateral(&self, b: &BondIdV1) -> u128 {
            self.0.available_collateral(b)
        }
        fn terminal_claim(&self, c: &Hash64) -> bool {
            self.0.terminal_claim(c)
        }
        fn verify_beacon(&self, r: &BeaconRequestV1, p: &BeaconProofV1) -> Result<(), PanelErrorV1> {
            self.0.verify_beacon(r, p)
        }
    }
    let v = Two(View::new(&[1, 2, 5], &[5, 6, 7], &[]));
    let (s, _) = drive(&v, &[(claim(10), Some(strata))]);
    let b = binding(&s, 10);
    assert_eq!(b.seats.iter().filter(|x| **x == bond(5)).count(), 2, "bond 5 sits in both strata");
    assert_eq!(s.reserved(&bond(5)), 100, "reserved once: the one ledger's duty row holds a bond once");
    for i in [1, 2, 6, 7] {
        assert_eq!(s.reserved(&bond(i)), 100);
    }
    s.check_consistency().unwrap();
}

#[test]
fn a_retry_draws_fresh_operators_per_stratum_and_fresh_outsiders_then_expires() {
    let v = View::new(&(1..=8).collect::<Vec<_>>(), &(5..=12).collect::<Vec<_>>(), &[20, 21, 22, 23]);
    let (s, _) = drive(&v, &[(claim(10), Some(STRATA))]);
    let first = binding(&s, 10);
    // The window (3 DAA) lapses: the engine redraws from the original seed, the next retry index.
    let (s, events) = s.advance(&step(h(5), 6, 17), &v).unwrap();
    let second = binding(&s, 10);
    assert_eq!(events.bindings[&h(10)], second);
    assert_eq!((second.retry_index, second.panel_seed_v3), (1, first.panel_seed_v3));
    for stratum in 0..2 {
        let a: BTreeSet<_> = first.seats[stratum * 4..stratum * 4 + 4].iter().collect();
        let b: BTreeSet<_> = second.seats[stratum * 4..stratum * 4 + 4].iter().collect();
        assert!(a.is_disjoint(&b), "stratum {stratum}: alternates are never reused: {a:?} / {b:?}");
    }
    let outsiders: BTreeSet<BondIdV1> = [first.seats[0], first.seats[4], second.seats[0], second.seats[4]].into_iter().collect();
    assert_eq!(outsiders.len(), 4, "every outsider seat of the claim's life is another operator");
    s.check_consistency().unwrap();
    // Spent: the next lapse expires the claim, uncharged, and the reservations go.
    let (s, events) = s.advance(&step(h(6), 7, 21), &v).unwrap();
    assert_eq!(events.non_fraud_voids[&h(10)], NonFraudReasonV1::PanelUnavailable);
    assert!((1..=23).all(|i| s.reserved(&bond(i)) == 0));
}

#[test]
fn a_tampered_stratified_binding_or_snapshot_does_not_load() {
    // Disjoint strata and outsiders, so a seat moved to another slot is out of place whatever the draw was.
    let v = View::new(&[1, 2, 3, 4], &[5, 6, 7, 8], &[10, 11, 12, 13]);
    let (s, _) = drive(&v, &[(claim(10), Some(STRATA))]);
    let record = s.claim(&h(10)).unwrap().clone();
    let tamper = |edit: &dyn Fn(&mut ClaimRecordV3)| {
        let mut r = record.clone();
        edit(&mut r);
        let mut t = s.clone();
        t.put_claim_row(h(10), Some(r));
        t.refresh_derived().unwrap();
        t.check_consistency()
    };
    // Swap the two strata's slices: the class seats are no longer their strata's members.
    let swapped = |r: &mut ClaimRecordV3| {
        let ClaimPhaseV3::Bound(mut b) = r.phase.clone() else { unreachable!() };
        b.seats.rotate_left(4);
        r.binding_history = vec![b.clone()];
        r.used_operators = b.seats.iter().map(|x| h(id_of(x))).collect();
        r.phase = ClaimPhaseV3::Bound(b);
    };
    // The first stratum's outsider moved into the class seats.
    let outsider_as_class = |r: &mut ClaimRecordV3| {
        let ClaimPhaseV3::Bound(mut b) = r.phase.clone() else { unreachable!() };
        b.seats.swap(0, 1);
        r.binding_history = vec![b.clone()];
        r.used_operators = b.seats.iter().map(|x| h(id_of(x))).collect();
        r.phase = ClaimPhaseV3::Bound(b);
    };
    // The strata dropped (a flat record with a stratified snapshot), and a bit beyond the strata.
    let unstratified = |r: &mut ClaimRecordV3| r.strata = None;
    let stray_bit = |r: &mut ClaimRecordV3| {
        let snapshot = r.snapshot.as_mut().unwrap();
        snapshot.strata_members[0] |= 1 << 5;
        snapshot.root = snapshot.computed_root();
    };
    for (name, edit) in [
        ("swapped strata", &swapped as &dyn Fn(&mut ClaimRecordV3)),
        ("outsider as class seat", &outsider_as_class),
        ("strata dropped", &unstratified),
        ("stray stratum bit", &stray_bit),
    ] {
        assert_eq!(tamper(edit), Err(PanelErrorV1::InvalidCarriage), "{name}");
    }
    // The untouched record reloads (the control).
    assert_eq!(tamper(&|_| {}), Ok(()));
}
