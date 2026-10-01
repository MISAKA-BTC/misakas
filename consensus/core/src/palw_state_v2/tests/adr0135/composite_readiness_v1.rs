//! **A composite class's readiness** (RFC-0004 §6.3/§6.7, spec 17 §17.7.1), through the registry's own fold:
//! a composite's registered root is `H(parent_class ‖ parent_root ‖ adapter_root ‖ P)`, which no multiproof
//! opens, so — before this rule — no seat could prove a composite and none ever left `Prefetching`. Past
//! `palw_improvement_v1` a V2 possession proof of a class with a composite record reconstructs the ADAPTER
//! section's own inventory root, and a seat is ready for the composite only if it is also ready for the
//! parent. Dormant: a class without a record reads the class-blind predicate, byte for byte.

use super::*;
use crate::palw_artifact::{
    PalwArtifactMultiproofV1, PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, palw_artifact_multiproof_v1,
};
use crate::palw_improve_composite_v1::PalwTirCompositeRefV1;
use crate::palw_model_registry_v1::{
    PALW_COMPOSITE_READINESS_MAX_DEPTH_V1, PALW_SEAT_NOT_READY_PARENT_V1, PalwReadinessPolicyV1, palw_composite_ancestry_v1,
    palw_model_registry_ready_seats_v1, palw_readiness_v2_challenge_seed_v1, palw_readiness_v2_draw_v1,
    palw_seat_class_not_ready_reason_v1,
};

/// The composite candidate's class; its parent is Kimi (`kimi_id()`), the fixture's registered class.
fn composite_id() -> Hash64 {
    h64(3)
}

/// A second composite over the same adapter, rested on another parent: what a replay would be aimed at.
fn other_composite_id() -> Hash64 {
    h64(4)
}

/// The improvement fence's height: below every block these tests fold at.
const FENCE: u64 = 50;

fn improve_params() -> PalwStateParamsV2 {
    params().with_improve_from_daa(Some(FENCE)).with_improve_ceilings(Some(crate::palw_improve_v1::PALW_DRILL_IMPROVE_CEILINGS_V1))
}

/// `n` operands of eight bytes each, named `tensor` — an inventory a seat can hold.
fn operands(tensor: &str, n: u32) -> Vec<PalwArtifactOperandV1> {
    (0..n)
        .map(|i| PalwArtifactOperandV1 { tensor_name: tensor.to_string(), layer: None, row_start: i * 8, bytes: vec![i as u8; 8] })
        .collect()
}

fn root_of(operands: &[PalwArtifactOperandV1]) -> Hash64 {
    artifact_root_v1(&operands.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).expect("a non-empty inventory")
}

/// The fixture: Kimi's inventory (the parent), the adapter section's (a different inventory), the composite's
/// record, and the registry's fold with the composite's genesis work and readiness V2.
struct Fixture {
    parent: Vec<PalwArtifactOperandV1>,
    adapter: Vec<PalwArtifactOperandV1>,
    record: PalwTirCompositeRefV1,
}

impl Fixture {
    fn new() -> Self {
        let (parent, adapter) = (operands("w", 64), operands("lora", 64));
        let record =
            PalwTirCompositeRefV1 { parent_class: kimi_id(), parent_root: root_of(&parent), adapter_root: root_of(&adapter), p: 290 };
        Self { parent, adapter, record }
    }

    fn fold(&self) -> PalwModelRegistryFoldV1 {
        let mut f = fold(kimi_work());
        f.genesis_works.insert(composite_id(), kimi_work());
        f.genesis_works.insert(other_composite_id(), kimi_work());
        f.readiness_v2_active = true;
        f
    }

    fn extras(&self) -> PalwTransitionExtrasV1 {
        PalwTransitionExtrasV1 { readiness_v2_active: true, ..extras(Some(self.fold())) }
    }

    fn registered(&self, class_id: Hash64, root: Hash64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root: root,
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 0,
            activation_daa: 0,
            admission: None,
        }
    }

    /// The floor and bond 1; Kimi on its inventory; the composite on its composite root; bonds 2..=8.
    fn network(&self) -> Vec<PalwConsensusObjectV2> {
        let mut objects = register_class_and_bond();
        objects.push(kimi_registered(self.record.parent_root));
        objects.push(self.registered(composite_id(), self.record.artifact_root()));
        objects.extend((2..=8).map(|n| bond(n, 1_000)));
        objects
    }

    fn step(
        &self,
        p: &PalwStateParamsV2,
        parent: &PalwChainStateV2,
        c: &PalwBlockContextV2,
        objects: &[PalwConsensusObjectV2],
    ) -> Result<PalwChainStateV2, PalwStateV2Error> {
        let (next, _) = apply_palw_transition_v2_with_extras(parent, p, c, objects, None, false, false, false, false, &self.extras())?;
        next.assert_internal_consistency(p).expect("internal consistency after apply");
        Ok(next)
    }

    /// The chain at the first span boundary: everything registered, the composite's record written (what its
    /// candidate's acceptance does), and every class rowed.
    fn rowed(&self, p: &PalwStateParamsV2) -> PalwChainStateV2 {
        let s1 = self.step(p, &PalwChainStateV2::genesis(), &ctx(1, 100, 1), &self.network()).unwrap();
        let mut s1 = s1;
        s1.improvement_composite_classes.insert(composite_id(), self.record);
        self.step(p, &s1, &ctx(2, 110, 2), &[]).unwrap()
    }
}

/// A seat's V2 possession proof of `class` over `inventory` for `span`: the challenge's leaves, the multiproof.
fn proof_over(class: Hash64, inventory: &[PalwArtifactOperandV1], seat: PalwBondKeyV2, span: u64) -> PalwConsensusObjectV2 {
    let leaves: Vec<Hash64> = inventory.iter().map(artifact_leaf_v1).collect();
    let seed = palw_readiness_v2_challenge_seed_v1(&class, &borsh::to_vec(&seat).unwrap(), span);
    let draw = palw_readiness_v2_draw_v1(&seed, inventory.len() as u32);
    let mut opened: Vec<(u32, PalwArtifactOperandV1)> = draw.iter().map(|i| (*i, inventory[*i as usize].clone())).collect();
    opened.sort_by_key(|(index, _)| *index);
    let proof: PalwArtifactMultiproofV1 = palw_artifact_multiproof_v1(&leaves, &opened).expect("the seat holds the inventory");
    PalwConsensusObjectV2::SeatReadinessProvedV2 { bond: seat, class_id: class, span, proof: Box::new(proof), signature: vec![1] }
}

fn ready(state: &PalwChainStateV2, p: &PalwStateParamsV2, f: &PalwModelRegistryFoldV1, class: &Hash64, now: u64) -> u32 {
    palw_model_registry_ready_seats_v1(state, p, class, now, f)
}

/// **The headline: a composite reaches Probation once seven seats prove the adapter and are ready for the
/// parent.** The adapter proofs alone leave it in `Prefetching` with no ready seat (each seat's reason is the
/// parent clause); the parent's proofs complete the seats; the next boundary admits it.
#[test]
fn a_composite_reaches_probation_once_k_seats_prove_the_adapter_and_hold_the_parent() {
    let (fx, p) = (Fixture::new(), improve_params());
    let f = fx.fold();
    let s2 = fx.rowed(&p);
    for class in [kimi_id(), composite_id()] {
        assert_eq!(
            s2.model_lifecycle(&class).unwrap().state,
            PalwModelLifecycleV1::Prefetching,
            "{class}: a class with work starts PREFETCHING"
        );
    }
    // Seven seats prove the ADAPTER of the composite (span 11): V2 proofs reconstruct `adapter_root`.
    let adapter_proofs: Vec<_> = (2..=8).map(|n| proof_over(composite_id(), &fx.adapter, bond_key(n), 11)).collect();
    let s3 = fx.step(&p, &s2, &ctx(3, 111, 3), &adapter_proofs).expect("the adapter section is possession of the composite");
    assert_eq!(s3.seat_readiness_iter().count(), 7, "every proof is a row");
    // …and the composite does not count them yet: the seats hold no parent.
    let s4 = fx.step(&p, &s3, &ctx(4, 120, 4), &[]).unwrap();
    let row = s4.model_lifecycle(&composite_id()).unwrap();
    assert_eq!((row.state, row.ready_seats), (PalwModelLifecycleV1::Prefetching, 0), "no parent, no ready seat");
    assert_eq!(ready(&s4, &p, &f, &composite_id(), 120), 0, "the registry read counts what the fold counts");
    let seat_row = s4.seat_readiness(&bond_key(2), &composite_id()).unwrap();
    assert_eq!(
        palw_seat_class_not_ready_reason_v1(&s4, &p, &bond_key(2), &composite_id(), seat_row, 120, &f),
        Some(PALW_SEAT_NOT_READY_PARENT_V1),
        "the reason a seat that proved the adapter is not ready is the parent clause"
    );
    // The same seats prove the parent (span 12): the composite's seven are now ready, and so is Kimi.
    let parent_proofs: Vec<_> = (2..=8).map(|n| proof_over(kimi_id(), &fx.parent, bond_key(n), 12)).collect();
    let s5 = fx.step(&p, &s4, &ctx(5, 121, 5), &parent_proofs).unwrap();
    let s6 = fx.step(&p, &s5, &ctx(6, 130, 6), &[]).unwrap();
    for class in [kimi_id(), composite_id()] {
        let row = s6.model_lifecycle(&class).unwrap();
        assert_eq!(
            (row.state, row.ready_seats),
            (PalwModelLifecycleV1::Probation { probes_passed: 0 }, 7),
            "{class}: admitted to PROBATION"
        );
    }
    assert_eq!(ready(&s6, &p, &f, &composite_id(), 130), 7);
    assert_eq!(
        palw_seat_class_not_ready_reason_v1(
            &s6,
            &p,
            &bond_key(2),
            &composite_id(),
            s6.seat_readiness(&bond_key(2), &composite_id()).unwrap(),
            130,
            &f
        ),
        None
    );
}

/// **Refused by name otherwise**: a seat's ready-for-the-composite status needs the parent per SEAT — six of
/// seven holders keep the composite one short of the seven it requires — and a parent whose proof lapses takes
/// the composite's count with it.
#[test]
fn the_parent_clause_is_per_seat_and_follows_the_parents_row() {
    let (fx, p) = (Fixture::new(), improve_params());
    let f = fx.fold();
    let s2 = fx.rowed(&p);
    let adapter_proofs: Vec<_> = (2..=8).map(|n| proof_over(composite_id(), &fx.adapter, bond_key(n), 11)).collect();
    let s3 = fx.step(&p, &s2, &ctx(3, 111, 3), &adapter_proofs).unwrap();
    // Six seats prove the parent: the composite counts six, one short.
    let parent_proofs: Vec<_> = (2..=7).map(|n| proof_over(kimi_id(), &fx.parent, bond_key(n), 11)).collect();
    let s4 = fx.step(&p, &s3, &ctx(4, 112, 4), &parent_proofs).unwrap();
    let s5 = fx.step(&p, &s4, &ctx(5, 120, 5), &[]).unwrap();
    let row = s5.model_lifecycle(&composite_id()).unwrap();
    assert_eq!((row.state, row.ready_seats), (PalwModelLifecycleV1::Prefetching, 6), "six of seven hold the parent: one short");
    assert_eq!(ready(&s5, &p, &f, &composite_id(), 120), 6);
    let eighth = s5.seat_readiness(&bond_key(8), &composite_id()).unwrap();
    assert_eq!(
        palw_seat_class_not_ready_reason_v1(&s5, &p, &bond_key(8), &composite_id(), eighth, 120, &f),
        Some(PALW_SEAT_NOT_READY_PARENT_V1)
    );
    // The seventh proves the parent too: seven, and the next boundary admits.
    let s6 = fx.step(&p, &s5, &ctx(6, 121, 6), &[proof_over(kimi_id(), &fx.parent, bond_key(8), 12)]).unwrap();
    let s7 = fx.step(&p, &s6, &ctx(7, 130, 7), &[]).unwrap();
    assert_eq!(s7.model_lifecycle(&composite_id()).unwrap().state, PalwModelLifecycleV1::Probation { probes_passed: 0 });
    // The parent's row lapses: the composite counts the parent row's own freshness, so a seat whose parent row is
    // stale is not ready for the composite whatever its adapter row says. Age one seat's parent row to DAA 0 and ask
    // at a DAA where the adapter row is fresh.
    let adapter_row = *s7.seat_readiness(&bond_key(2), &composite_id()).unwrap();
    let mut state = s7.clone();
    let mut old = *state.seat_readiness(&bond_key(2), &kimi_id()).unwrap();
    old.proved_daa = 0;
    old.proved_span = 0;
    state.seat_readiness.insert((bond_key(2), kimi_id()), old);
    assert_eq!(
        palw_seat_class_not_ready_reason_v1(&state, &p, &bond_key(2), &composite_id(), &adapter_row, 130, &f),
        Some(PALW_SEAT_NOT_READY_PARENT_V1),
        "a fresh adapter row over a stale parent row is no readiness"
    );
    assert_eq!(
        palw_seat_class_not_ready_reason_v1(
            &state,
            &p,
            &bond_key(3),
            &composite_id(),
            state.seat_readiness(&bond_key(3), &composite_id()).unwrap(),
            130,
            &f
        ),
        None,
        "while the seats whose parent rows stand are ready"
    );
}

/// **The proof opens the adapter root and nothing else, and a record that disagrees opens nothing.**
#[test]
fn a_composite_proof_must_open_the_adapter_root_over_a_record_that_roots_to_the_class() {
    let (fx, p) = (Fixture::new(), improve_params());
    let s2 = fx.rowed(&p);
    // The parent's inventory is not the composite's possession, and the adapter's is not the parent's.
    let wrong = fx.step(&p, &s2, &ctx(3, 111, 3), &[proof_over(composite_id(), &fx.parent, bond_key(2), 11)]);
    assert!(
        matches!(wrong, Err(PalwStateV2Error::ReadinessProofRefused(_))),
        "the parent's leaves do not reconstruct the adapter root: {wrong:?}"
    );
    let swapped = fx.step(&p, &s2, &ctx(3, 111, 3), &[proof_over(kimi_id(), &fx.adapter, bond_key(2), 11)]);
    assert!(
        matches!(swapped, Err(PalwStateV2Error::ReadinessProofRefused(_))),
        "the adapter's leaves are not the parent's possession: {swapped:?}"
    );
    // A record that does not root to the class's registered artifact root opens nothing.
    let mut stale = s2.clone();
    let mut record = fx.record;
    record.p += 1;
    stale.improvement_composite_classes.insert(composite_id(), record);
    let refused = fx.step(&p, &stale, &ctx(3, 111, 3), &[proof_over(composite_id(), &fx.adapter, bond_key(2), 11)]);
    assert!(
        matches!(&refused, Err(PalwStateV2Error::ReadinessProofRefused(why)) if why.contains("does not root to the class")),
        "{refused:?}"
    );
    // The honest proof lands.
    assert!(fx.step(&p, &s2, &ctx(3, 111, 3), &[proof_over(composite_id(), &fx.adapter, bond_key(2), 11)]).is_ok());
}

/// **The proof is the composite's own**: the challenge names the class, so another composite over the same
/// adapter draws other leaves and a proof written for one is no proof of the other; and the row it writes is
/// keyed by its class, so it counts for no other.
#[test]
fn a_composite_proof_is_bound_to_its_class_id() {
    let (fx, p) = (Fixture::new(), improve_params());
    let s2 = fx.rowed(&p);
    // Another composite C' over the SAME adapter, on another parent root, registered and recorded.
    let other =
        PalwTirCompositeRefV1 { parent_class: kimi_id(), parent_root: h64(0x0FFE), adapter_root: fx.record.adapter_root, p: 290 };
    let mut s = fx.step(&p, &s2, &ctx(3, 111, 3), &[fx.registered(other_composite_id(), other.artifact_root())]).unwrap();
    s.improvement_composite_classes.insert(other_composite_id(), other);
    let seat = bond_key(2);
    let draw = |class: Hash64| {
        palw_readiness_v2_draw_v1(&palw_readiness_v2_challenge_seed_v1(&class, &borsh::to_vec(&seat).unwrap(), 11), 64)
    };
    assert_ne!(draw(composite_id()), draw(other_composite_id()), "another class draws other leaves of the same adapter");
    // The proof written for C, submitted as C': the leaves are not C''s draw.
    let PalwConsensusObjectV2::SeatReadinessProvedV2 { proof, .. } = proof_over(composite_id(), &fx.adapter, seat, 11) else {
        unreachable!()
    };
    let replay = PalwConsensusObjectV2::SeatReadinessProvedV2 {
        bond: seat,
        class_id: other_composite_id(),
        span: 11,
        proof,
        signature: vec![1],
    };
    let refused = fx.step(&p, &s, &ctx(4, 112, 4), &[replay]);
    assert!(matches!(refused, Err(PalwStateV2Error::ReadinessProofRefused(_))), "a proof of C is no proof of C': {refused:?}");
    // Each composite's own proof lands, as its own row.
    let both = [proof_over(composite_id(), &fx.adapter, seat, 11), proof_over(other_composite_id(), &fx.adapter, seat, 11)];
    let s4 = fx.step(&p, &s, &ctx(4, 112, 4), &both).unwrap();
    assert!(s4.seat_readiness(&seat, &composite_id()).is_some() && s4.seat_readiness(&seat, &other_composite_id()).is_some());
    assert!(s4.seat_readiness(&bond_key(3), &composite_id()).is_none(), "and nobody else's row");
}

/// **Dormant until `palw_improvement_v1`**: with the fence off, a class with a composite record reads exactly
/// the class-blind rule — its adapter proof does not reconstruct its registered root, and no parent clause is
/// asked of any seat.
#[test]
fn below_the_improvement_fence_nothing_changes() {
    let fx = Fixture::new();
    let off = params();
    assert!(!off.improve_active_at(u64::MAX), "the fixture's plain params arm no improvement fence");
    let s2 = fx.rowed(&off);
    let refused = fx.step(&off, &s2, &ctx(3, 111, 3), &[proof_over(composite_id(), &fx.adapter, bond_key(2), 11)]);
    assert!(matches!(refused, Err(PalwStateV2Error::ReadinessProofRefused(_))), "the adapter root is no registered root: {refused:?}");
    // A seat's rows for the composite and for no parent: ready as any class's seat would be (no parent clause).
    let f = fx.fold();
    let mut state = s2.clone();
    state.seat_readiness.insert(
        (bond_key(2), composite_id()),
        crate::palw_model_registry_v1::PalwSeatReadinessRowV1 {
            proved_daa: 110,
            proved_span: 11,
            leaf_index: 0,
            proof_version: 2,
            chunks: 16,
        },
    );
    let row = *state.seat_readiness(&bond_key(2), &composite_id()).unwrap();
    assert_eq!(palw_seat_class_not_ready_reason_v1(&state, &off, &bond_key(2), &composite_id(), &row, 111, &f), None);
    assert_eq!(
        palw_seat_class_not_ready_reason_v1(&state, &improve_params(), &bond_key(2), &composite_id(), &row, 111, &f),
        Some(PALW_SEAT_NOT_READY_PARENT_V1),
        "the same state under the fence asks the parent clause"
    );
}

/// **The panel draw asks the parent too**, and the ancestry walk is bounded.
#[test]
fn the_panel_draw_and_the_ancestry_walk_follow_the_parent() {
    let (fx, p) = (Fixture::new(), improve_params());
    let f = fx.fold();
    let s2 = fx.rowed(&p);
    let s3 = fx.step(&p, &s2, &ctx(3, 111, 3), &[proof_over(composite_id(), &fx.adapter, bond_key(2), 11)]).unwrap();
    let policy = PalwReadinessPolicyV1::at(&f, 112, h64(1), true);
    let seat = s3.bond(&bond_key(2)).expect("the bond");
    let judges =
        |state: &PalwChainStateV2| palw_bond_may_judge_class_v4(state, &bond_key(2), seat, &composite_id(), false, Some(policy));
    assert!(!judges(&s3), "an adapter row without a parent row is not drawn");
    let s4 = fx.step(&p, &s3, &ctx(4, 112, 4), &[proof_over(kimi_id(), &fx.parent, bond_key(2), 11)]).unwrap();
    assert!(judges(&s4), "with the parent's row it is");
    // The walk: a composite over Kimi; and a chain deeper than the bound is none at all.
    assert_eq!(palw_composite_ancestry_v1(&s4, &composite_id()), Some(vec![kimi_id()]));
    assert_eq!(palw_composite_ancestry_v1(&s4, &kimi_id()), Some(Vec::new()), "a class without a record has no ancestry");
    let mut deep = PalwChainStateV2::genesis();
    for i in 0..=PALW_COMPOSITE_READINESS_MAX_DEPTH_V1 as u64 + 1 {
        let record = PalwTirCompositeRefV1 { parent_class: h64(0x500 + i + 1), parent_root: h64(1), adapter_root: h64(2), p: 1 };
        deep.improvement_composite_classes.insert(h64(0x500 + i), record);
    }
    // Ten records, 0x500..=0x509, each over the next class: the walk from the first two is past the bound, the
    // walk from the third is exactly the bound.
    assert!(palw_composite_ancestry_v1(&deep, &h64(0x500)).is_none(), "ten composites deep is past the bound");
    assert!(palw_composite_ancestry_v1(&deep, &h64(0x501)).is_none(), "nine is too");
    assert_eq!(
        palw_composite_ancestry_v1(&deep, &h64(0x502)).map(|chain| chain.len()),
        Some(PALW_COMPOSITE_READINESS_MAX_DEPTH_V1),
        "…and a chain of exactly the bound is followed"
    );
}
