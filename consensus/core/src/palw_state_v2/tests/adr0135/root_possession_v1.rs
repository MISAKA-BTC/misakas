//! **Possession is per ROOT** (lane MU, ADR-0173, `palw_audit_1004_v1`): a seat that proved a class's founding root R1 is not a seat
//! for the root R2 a model line publishes later, and the chain refuses to behave as though it were — no R2 claim before `seat_count`
//! operators besides the executor prove R2, no R1-only seat drawn onto an R2 claim, no version made current before its floor.
//! Below the fence every path is the one that existed (a proof opens the registered root alone; the draw reads the class's row).

use super::*;
use crate::palw_artifact::{
    PalwArtifactMultiproofV1, PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, palw_artifact_multiproof_v1,
};
use crate::palw_model_lines_v1::{PalwModelVersionV1, PalwVersionStatusV1, founding_line_v1};
use crate::palw_model_registry_v1::{
    PalwReadinessPolicyV1, PalwSeatReadinessRowV1, palw_model_registry_ready_seats_v1, palw_readiness_v2_challenge_seed_v1, palw_readiness_v2_draw_v1,
};

const FENCE: u64 = 50;
const SUPERSEDED_UNTIL: u64 = 4_100;

fn inventory(tensor: &str, n: u32) -> Vec<PalwArtifactOperandV1> {
    (0..n)
        .map(|i| PalwArtifactOperandV1 { tensor_name: tensor.to_string(), layer: None, row_start: i * 8, bytes: vec![i as u8; 8] })
        .collect()
}

fn root_of(operands: &[PalwArtifactOperandV1]) -> Hash64 {
    artifact_root_v1(&operands.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).expect("a non-empty inventory")
}

fn proof_over(class: Hash64, operands: &[PalwArtifactOperandV1], seat: PalwBondKeyV2, span: u64) -> PalwConsensusObjectV2 {
    let leaves: Vec<Hash64> = operands.iter().map(artifact_leaf_v1).collect();
    let seed = palw_readiness_v2_challenge_seed_v1(&class, &borsh::to_vec(&seat).unwrap(), span);
    let draw = palw_readiness_v2_draw_v1(&seed, operands.len() as u32);
    let mut opened: Vec<(u32, PalwArtifactOperandV1)> = draw.iter().map(|i| (*i, operands[*i as usize].clone())).collect();
    opened.sort_by_key(|(index, _)| *index);
    let proof: PalwArtifactMultiproofV1 = palw_artifact_multiproof_v1(&leaves, &opened).expect("the seat holds the inventory");
    PalwConsensusObjectV2::SeatReadinessProvedV2 { bond: seat, class_id: class, span, proof: Box::new(proof), signature: vec![1] }
}

fn version(root: Hash64, status: PalwVersionStatusV1) -> PalwModelVersionV1 {
    PalwModelVersionV1 {
        root,
        parent: None,
        adopted_from: None,
        runtime_hash: None,
        dataset_commitment: None,
        training_config_hash: None,
        notes_hash: None,
        published_daa: 100,
        published_by: None,
        status,
        usage: Default::default(),
    }
}

struct Fx {
    r1: Vec<PalwArtifactOperandV1>,
    r2: Vec<PalwArtifactOperandV1>,
    other: Vec<PalwArtifactOperandV1>,
    /// Kimi rowed at the first span boundary, with a founding line whose version 1 is R1 and whose version 2 is R2 (a preview).
    s2: PalwChainStateV2,
    on: PalwStateParamsV2,
    off: PalwStateParamsV2,
}

impl Fx {
    fn new() -> Self {
        let (r1, r2, other) = (inventory("w", 64), inventory("w2", 64), inventory("w3", 64));
        let (on, off) = (params().with_audit_1004_from_daa(Some(FENCE)), params());
        let fx = Self { r1, r2, other, s2: PalwChainStateV2::genesis(), on, off };
        let f = fx.fold();
        let ex = PalwTransitionExtrasV1 { readiness_v2_active: true, model_lines_active: true, ..extras(Some(f)) };
        let (s1, _) = apply_palw_transition_v2_with_extras(
            &PalwChainStateV2::genesis(),
            &fx.on,
            &ctx(1, 100, 1),
            &network(root_of(&fx.r1)),
            None,
            false,
            false,
            false,
            false,
            &ex,
        )
        .unwrap();
        let (mut s2, _) =
            apply_palw_transition_v2_with_extras(&s1, &fx.on, &ctx(2, 110, 2), &[], None, false, false, false, false, &ex).unwrap();
        let mut line = founding_line_v1(kimi_id(), Some(bond_key(2)), Vec::new(), 100);
        line.previews = vec![2];
        line.versions_published = 2;
        s2.model_lines.insert(kimi_id(), line);
        s2.model_versions.insert((kimi_id(), 1), version(root_of(&fx.r1), PalwVersionStatusV1::Current));
        s2.model_versions.insert((kimi_id(), 2), version(root_of(&fx.r2), PalwVersionStatusV1::Preview));
        Self { s2, ..fx }
    }

    fn fold(&self) -> PalwModelRegistryFoldV1 {
        let mut f = fold(kimi_work());
        f.readiness_v2_active = true;
        f
    }

    fn extras(&self) -> PalwTransitionExtrasV1 {
        PalwTransitionExtrasV1 { readiness_v2_active: true, model_lines_active: true, ..extras(Some(self.fold())) }
    }

    fn step(
        &self,
        p: &PalwStateParamsV2,
        parent: &PalwChainStateV2,
        c: &PalwBlockContextV2,
        objects: &[PalwConsensusObjectV2],
    ) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
        let out = apply_palw_transition_v2_with_extras(parent, p, c, objects, None, false, false, false, false, &self.extras())?;
        out.0.assert_internal_consistency(p).expect("internal consistency after apply");
        Ok(out)
    }

    fn prove(&self, ops: &[PalwArtifactOperandV1], seats: std::ops::RangeInclusive<u64>, span: u64) -> Vec<PalwConsensusObjectV2> {
        seats.map(|n| proof_over(kimi_id(), ops, bond_key(n), span)).collect()
    }

    fn door(&self, state: &PalwChainStateV2, p: &PalwStateParamsV2, root: Hash64, executor: u64, daa: u64) -> Result<(), PalwStateV2Error> {
        let extras = self.extras();
        PalwFoldReadV1::outside(state, p, &extras).check_root_possession_v1(&kimi_id(), &root, Some(&bond_key(executor)), daa)
    }
}

/// **A proof of R2 writes R2's row, leaves the founding row alone, reverts, and survives the carriage.** The legacy table IS the
/// founding root's, so there is nothing to migrate: an R1 proof lands where it always did.
#[test]
fn a_proof_of_a_published_root_writes_its_own_row_and_round_trips() {
    let fx = Fx::new();
    let (r1, r2) = (root_of(&fx.r1), root_of(&fx.r2));
    let (s3, d3) = fx.step(&fx.on, &fx.s2, &ctx(3, 111, 3), &fx.prove(&fx.r2, 2..=8, 11)).expect("R2 is a root the class has in force");
    assert_eq!(s3.seat_root_readiness_iter().count(), 7, "one row per seat for R2");
    assert_eq!(s3.seat_readiness_iter().count(), 0, "and the founding table is untouched");
    assert!(s3.seat_readiness_for_root(&bond_key(2), &kimi_id(), Some(&r2)).is_some());
    assert!(s3.seat_readiness_for_root(&bond_key(2), &kimi_id(), Some(&r1)).is_none(), "R2 possession is not R1 possession");
    assert_ne!(s3.state_root(), fx.s2.state_root(), "the rows are rooted");
    assert_eq!(revert_delta_v2(&s3, &d3, &fx.on).expect("reverts").state_root(), fx.s2.state_root(), "reorg revert");
    let again = apply_delta_v2(&fx.s2, &d3, &fx.on).expect("the delta re-applies");
    assert_eq!(again.state_root(), s3.state_root(), "and applies");
    assert_eq!(
        borsh::from_slice::<PalwDeltaEntryV2>(&borsh::to_vec(&d3.entries[0]).unwrap()).unwrap(),
        d3.entries[0],
        "a delta entry round-trips"
    );

    // An R1 proof goes to the legacy table, exactly as before the fence: this is the migration.
    let (s4, _) = fx.step(&fx.on, &s3, &ctx(4, 112, 4), &fx.prove(&fx.r1, 2..=4, 11)).expect("the founding root");
    assert_eq!(s4.seat_readiness_iter().count(), 3);
    assert_eq!(s4.seat_root_readiness_iter().count(), 7);
    let legacy = *s4.seat_readiness(&bond_key(2), &kimi_id()).unwrap();
    assert_eq!(s4.seat_readiness_for_root(&bond_key(2), &kimi_id(), Some(&r1)), Some(&legacy), "a legacy row reads as the founding root");
    assert_eq!(s4.seat_readiness_for_root(&bond_key(2), &kimi_id(), None), Some(&legacy));

    // The carriage carries the tail, and a state without rows carries none.
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s4)).unwrap();
    let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
    let restored = back.into_state(&fx.on, Some(s4.state_root())).expect("the carriage decodes to the same root");
    assert_eq!(restored.seat_root_readiness_iter().count(), 7);
    assert_eq!(restored.state_root(), s4.state_root());
    let empty = borsh::to_vec(&PalwStateCarriageV2::from_state(&fx.s2)).unwrap();
    assert!(!empty.contains(&0xEB) || fx.s2.seat_root_readiness_iter().count() == 0, "no rows, no tail");
    assert_eq!(
        PalwStateCarriageV2::from_state(&fx.s2).seat_root_readiness.len(),
        0,
        "the carriage of a state with no root rows has none"
    );
}

/// **Below the fence the proof opens the registered root alone**: R2 is refused, and so is a root the class does not have in force
/// past it.
#[test]
fn below_the_fence_only_the_registered_root_and_past_it_only_a_root_in_force() {
    let fx = Fx::new();
    let below = fx.step(&fx.off, &fx.s2, &ctx(3, 111, 3), &fx.prove(&fx.r2, 2..=2, 11));
    assert!(matches!(below, Err(PalwStateV2Error::ReadinessProofRefused(_))), "{below:?}");
    let unknown = fx.step(&fx.on, &fx.s2, &ctx(3, 111, 3), &fx.prove(&fx.other, 2..=2, 11));
    assert!(matches!(unknown, Err(PalwStateV2Error::ReadinessProofRefused(_))), "a root nobody published: {unknown:?}");
    let (founding, _) = fx.step(&fx.off, &fx.s2, &ctx(3, 111, 3), &fx.prove(&fx.r1, 2..=2, 11)).expect("the registered root, as ever");
    assert_eq!(founding.seat_readiness_iter().count(), 1);
    assert_eq!(founding.seat_root_readiness_iter().count(), 0);
}

/// **R2 is NotReady until its floor; the executor's operator never counts; R1 stays live through its grace.**
#[test]
fn a_new_root_is_refused_to_claims_until_seat_count_operators_besides_the_executor_prove_it() {
    let fx = Fx::new();
    let (r1, r2) = (root_of(&fx.r1), root_of(&fx.r2));
    let none = fx.door(&fx.s2, &fx.on, r2, 1, 120);
    assert!(matches!(none, Err(PalwStateV2Error::RootNotSeated { have: 0, need: 5, .. })), "{none:?}");
    // Four operators prove R2: one short of seat_count (5).
    let (six, _) = fx.step(&fx.on, &fx.s2, &ctx(3, 111, 3), &fx.prove(&fx.r2, 2..=5, 11)).unwrap();
    assert!(matches!(fx.door(&six, &fx.on, r2, 1, 120), Err(PalwStateV2Error::RootNotSeated { have: 4, need: 5, .. })));
    // R1 seats do not help: all seven prove R1 and R2 is still short.
    let (r1_only, _) = fx.step(&fx.on, &six, &ctx(4, 112, 4), &fx.prove(&fx.r1, 2..=8, 11)).unwrap();
    assert!(matches!(fx.door(&r1_only, &fx.on, r2, 1, 120), Err(PalwStateV2Error::RootNotSeated { have: 4, need: 5, .. })));
    assert!(fx.door(&r1_only, &fx.on, r1, 1, 120).is_ok(), "the founding root's claims stay live (grace)");
    // The fifth arrives.
    let (seven, _) = fx.step(&fx.on, &six, &ctx(4, 112, 4), &fx.prove(&fx.r2, 6..=6, 11)).unwrap();
    assert!(fx.door(&seven, &fx.on, r2, 1, 120).is_ok(), "five operators besides the executor");
    // If the executor is one of the five, its operator is excluded and the floor is short again.
    assert!(matches!(fx.door(&seven, &fx.on, r2, 2, 120), Err(PalwStateV2Error::RootNotSeated { have: 4, need: 5, .. })));
    // Below the fence the door is not asked.
    assert!(fx.door(&fx.s2, &fx.off, r2, 1, 120).is_ok());
    // Grace: R1 is in force until its supersession ends; R2 is in force as a preview.
    let mut s = seven.clone();
    s.model_versions.insert((kimi_id(), 1), version(r1, PalwVersionStatusV1::Superseded { until_daa: SUPERSEDED_UNTIL }));
    s.model_versions.insert((kimi_id(), 2), version(r2, PalwVersionStatusV1::Current));
    s.model_lines.get_mut(&kimi_id()).unwrap().current = 2;
    assert_eq!(s.class_roots_in_force(&kimi_id(), 120), vec![r1, r2]);
    assert_eq!(s.class_roots_in_force(&kimi_id(), SUPERSEDED_UNTIL), vec![r2], "and R1 leaves force when its grace ends");
}

/// **The draw reads possession of the claim's root**: an R1-only seat is never eligible for an R2 claim; below the fence the policy is
/// not root-keyed and the draw is the one it was.
#[test]
fn an_r1_only_seat_is_never_drawn_onto_an_r2_claim() {
    let fx = Fx::new();
    let (r1, r2) = (root_of(&fx.r1), root_of(&fx.r2));
    // Bond 2: R1 only. Bond 3: R2 only. Bond 4: both.
    let (s3, _) = fx.step(&fx.on, &fx.s2, &ctx(3, 111, 3), &[fx.prove(&fx.r1, 2..=2, 11), fx.prove(&fx.r2, 3..=3, 11), fx.prove(&fx.r1, 4..=4, 11), fx.prove(&fx.r2, 4..=4, 11)].concat()).unwrap();
    let f = fx.fold();
    let keyed = PalwReadinessPolicyV1::at(&f, 120, h64(1), true).with_root_keyed(true);
    let plain = PalwReadinessPolicyV1::at(&f, 120, h64(1), true);
    let may = |n: u64, root: Option<&Hash64>, policy: PalwReadinessPolicyV1| {
        let key = bond_key(n);
        palw_bond_may_judge_class_v5(&s3, &key, s3.bond(&key).unwrap(), &kimi_id(), root, false, Some(policy))
    };
    assert!(!may(2, Some(&r2), keyed), "R1 possession is no seat for an R2 claim");
    assert!(may(3, Some(&r2), keyed));
    assert!(may(4, Some(&r2), keyed));
    assert!(may(2, Some(&r1), keyed) && !may(3, Some(&r1), keyed), "an R1 claim keeps its R1 seats (grace)");
    assert!(may(2, None, keyed), "no recorded root: the founding root");
    assert!(may(2, None, plain) && !may(3, None, plain), "the unkeyed draw reads the class row, as it did");
    assert!(!keyed.admits(&PalwSeatReadinessRowV1 { proved_daa: 0, proved_span: 0, leaf_index: 0, proof_version: 1, chunks: 1 }));
    // The class-level count still sees every seat that holds a root in force.
    assert_eq!(palw_model_registry_ready_seats_v1(&s3, &fx.on, &kimi_id(), 120, &f), 3);
    assert_eq!(palw_model_registry_ready_seats_v1(&s3, &fx.off, &kimi_id(), 120, &f), 2, "below the fence: the legacy rows only");
}

/// **Possession gates activation**: past the fence a version cannot be made current (published or promoted) before its floor; below it
/// the gate does not exist.
#[test]
fn a_version_is_activated_only_once_its_root_is_possessed() {
    let fx = Fx::new();
    let r3 = root_of(&fx.other);
    let publish = |root: Hash64, version: u32, preview: bool| PalwConsensusObjectV2::ModelVersionPublished {
        line_id: kimi_id(),
        version,
        root,
        parent: None,
        adopted_from: None,
        runtime_hash: None,
        dataset_commitment: None,
        training_config_hash: None,
        notes_hash: None,
        preview,
        signature: vec![1],
    };
    let promote = |version: u32| PalwConsensusObjectV2::ModelVersionPromoted { line_id: kimi_id(), version, signature: vec![1] };
    // A new root cannot enter as current.
    let direct = fx.step(&fx.on, &fx.s2, &ctx(3, 111, 3), &[publish(r3, 3, false)]);
    assert!(matches!(direct, Err(PalwStateV2Error::RootNotSeated { .. })), "{direct:?}");
    assert!(fx.step(&fx.off, &fx.s2, &ctx(3, 111, 3), &[publish(r3, 3, false)]).is_ok(), "below the fence it could");
    // As a preview it enters; the promotion waits for the floor.
    let (s3, _) = fx.step(&fx.on, &fx.s2, &ctx(3, 111, 3), &[publish(r3, 3, true)]).expect("a preview enters");
    let early = fx.step(&fx.on, &s3, &ctx(4, 112, 4), &[promote(2)]);
    assert!(matches!(early, Err(PalwStateV2Error::RootNotSeated { have: 0, need: 5, .. })), "{early:?}");
    let (s4, _) = fx.step(&fx.on, &s3, &ctx(4, 112, 4), &fx.prove(&fx.r2, 2..=8, 11)).unwrap();
    let (s5, _) = fx.step(&fx.on, &s4, &ctx(5, 113, 5), &[promote(2)]).expect("five or more operators hold R2");
    assert_eq!(s5.model_lines.get(&kimi_id()).unwrap().current, 2);
    assert!(matches!(s5.model_versions.get(&(kimi_id(), 1)).unwrap().status, PalwVersionStatusV1::Superseded { .. }), "R1 keeps its grace");
}

/// **RFC-0004: a head moves only to a class that is Active and seated.** Below the fence the question is not asked.
#[test]
fn a_line_head_moves_only_to_an_active_seated_class() {
    let fx = Fx::new();
    let (s3, _) = fx.step(&fx.on, &fx.s2, &ctx(3, 111, 3), &fx.prove(&fx.r1, 2..=8, 11)).unwrap();
    let extras = fx.extras();
    let ready = |s: &PalwChainStateV2, p: &PalwStateParamsV2| PalwFoldReadV1::outside(s, p, &extras).class_ready_for_head_v1(&kimi_id(), 120);
    let mut s = s3.clone();
    s.model_lifecycles.get_mut(&kimi_id()).expect("the row").state = PalwModelLifecycleV1::Probation { probes_passed: 1 };
    assert!(!ready(&s, &fx.on), "Probation is not Active: the old head stays live");
    assert!(ready(&s, &fx.off), "below the fence the question is not asked");
    s.model_lifecycles.get_mut(&kimi_id()).unwrap().state = PalwModelLifecycleV1::Active;
    assert!(ready(&s, &fx.on), "Active and seven seats hold it (floor 5)");
    let mut few = fx.step(&fx.on, &fx.s2, &ctx(3, 111, 3), &fx.prove(&fx.r1, 2..=4, 11)).unwrap().0;
    few.model_lifecycles.get_mut(&kimi_id()).unwrap().state = PalwModelLifecycleV1::Active;
    assert!(!ready(&few, &fx.on), "Active but not seated: three of five");
}
