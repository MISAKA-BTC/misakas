//! **RFC-0002 Part II §II.7.5 Proposal A — `palw_class_seating`: one seating rule for every class kind**, fence-level and
//! fold-level (the drills' SEAT-1 … SEAT-11 as the RFC lists them, each as a deterministic test; the chain-level crossing is
//! `scripts/misaka-palw-seating-drill.sh`, evidence under `lanes/evidence/rfc2-rest/`).
//!
//! A class is *seated* for a claim whose executor is `e` iff (1) at least `seat_count` DISTINCT operators other than `e`'s hold
//! it with a fresh readiness V2 proof, and (2) at least `independent_floor` of them are neither the registrant's nor `e`'s and
//! are in the network's base population (the admission jury's). The predicate is `palw_class_seating_v1` (pure over the state,
//! the bundle and the registry fold); every door — the attempt claim, the free-prompt claim, the evaluation claim, the generative
//! tensor claim, the lifecycle step, the registry read — asks it. The fence is dormant on every preset.
//!
//! Fixture: testnet-12 itself with the generative fence, the decode rules and the lane's job fence armed (the fence's
//! prerequisites), `palw_class_seating` at [`AT`]; the eight genesis bonds are eight distinct operators of the base population.

#[path = "rcore_common.rs"]
mod rcore;
use rcore::*;

use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_shipped_params};
use kaspa_consensus_core::palw_class_seating_fence_v1::{PalwClassSeatingFenceV1, PalwClassSeatingTermsV1};
use kaspa_consensus_core::palw_gen_v1::PalwGenFenceV1;
use kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwClassNotSeatedV1, PalwSeatingFloorV1, PalwStateV2Error, palw_class_seated_admits_v1, palw_class_seating_v1,
};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;

const AT: u64 = 1_100;

/// testnet-12 with the fence's prerequisites and the fence armed at [`AT`] (floor 3).
fn armed() -> Params {
    armed_with(PalwClassSeatingFenceV1::testnet12_v1(ForkActivation::new(AT)))
}

fn armed_with(fence: PalwClassSeatingFenceV1) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT)));
    p.sync_palw_tir_v1();
    p.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(AT)));
    p.palw_fp_decode_rules = Some(ForkActivation::new(AT));
    p.sync_palw_fp_decode_rules();
    p.palw_fp_job_v5 = Some(ForkActivation::new(AT));
    p.sync_palw_gen_v1();
    p.palw_class_seating = Some(fence);
    p.sync_palw_class_seating();
    p.validate_palw_v2().unwrap_or_else(|e| panic!("the seating fence at {AT}: {e}"));
    p
}

/// What a test reads one class's seating from: the state after `edit`, with every genesis bond in `ready` proved fresh for the
/// class at `daa`.
struct Fx {
    p: Params,
    sp: kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
    class: Hash64,
    daa: u64,
    s: PalwChainStateV2,
}

impl Fx {
    /// A genesis model class made `Active`, no seat ready.
    fn new(p: Params) -> Self {
        let sp = bundle(&p).state.clone();
        let class = model_classes(&p).0;
        let s = activated(&sp, &genesis_state(&p), class);
        Self { p, sp, class, daa: AT + 40, s }
    }

    /// Eight genesis bonds, in registry order.
    fn bonds(&self) -> Vec<PalwBondKeyV2> {
        honest(&self.p)
    }

    /// The first `n` genesis bonds proved ready for the class now.
    fn ready(mut self, which: &[usize]) -> Self {
        let bonds = self.bonds();
        let keys: Vec<PalwBondKeyV2> = which.iter().map(|i| bonds[*i]).collect();
        self.s = readied(&self.sp, &self.s, &keys, self.class, self.daa);
        self
    }

    fn edit(mut self, edit: impl FnOnce(&mut kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2)) -> Self {
        self.s = edited(&self.sp, &self.s, edit);
        self
    }

    /// The seating the predicate reads for `executor` (a genesis index) or none.
    fn seating(&self, executor: Option<usize>) -> kaspa_consensus_core::palw_state_v2::PalwClassSeatingV1 {
        let fold = registry_fold(&self.p, self.daa).expect("the registry is in force");
        let exec = executor.map(|i| self.bonds()[i]);
        let terms = self.sp.class_seating_terms_at(self.daa).expect("the fence is in force");
        palw_class_seating_v1(&self.s, &self.sp, &fold, &self.class, exec.as_ref(), self.daa, terms, true).expect("the executor is a bond")
    }

    /// The fold's seating door for a genesis executor.
    fn door(&self, executor: usize) -> Result<(), PalwStateV2Error> {
        palw_class_seated_admits_v1(&self.s, &self.sp, &room_extras(&self.p, self.daa), &self.bonds()[executor], &self.class, self.daa)
    }
}

fn refused_with(result: Result<(), PalwStateV2Error>, floor: PalwSeatingFloorV1, have: u32, need: u32) {
    match result {
        Err(PalwStateV2Error::ClassNotSeated { floor: f, have: h, need: n, .. }) => assert_eq!((f, h, n), (floor, have, need)),
        other => panic!("expected ClassNotSeated {{ {floor:?}, {have}, {need} }}, got {other:?}"),
    }
}

/// A bond that does not serve the liveness floor's class: out of the base population, still able to hold the model class ready.
fn out_of_base(fx: Fx, which: &[usize]) -> Fx {
    let keys: Vec<PalwBondKeyV2> = which.iter().map(|i| fx.bonds()[*i]).collect();
    let class = fx.class;
    fx.edit(move |c| {
        for k in &keys {
            c.bonds.get_mut(k).expect("a genesis bond").capable_classes = std::iter::once(class).collect();
        }
    })
}

fn registered_by(fx: Fx, registrant: usize) -> Fx {
    let key = fx.bonds()[registrant];
    let class = fx.class;
    let price = fx.sp.registration_exposure_sompi() as u128;
    fx.edit(move |c| {
        c.classes.get_mut(&class).expect("the class").registrant_bond = Some(key);
        if price > 0 {
            c.registration_exposure.insert(key, price);
        }
    })
}

// ---------------------------------------------------------------------------------------------
// The fence is dormant, and what arming it asks
// ---------------------------------------------------------------------------------------------

#[test]
fn the_fence_is_dormant_on_every_shipped_preset_and_asks_nothing_below_its_height() {
    use kaspa_consensus_core::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, Params as P};
    for params in [&DEVNET_PARAMS, &MAINNET_PARAMS, &SIMNET_PARAMS, &TESTNET_PARAMS, &TESTNET11_PARAMS] {
        assert_eq!(params.palw_class_seating, None);
    }
    let t12 = P::from(kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 12));
    // int-12: testnet-12 ships it armed at the 5,300 flag day (the list), at the drill's floor (3, no raise).
    let at_h = kaspa_consensus_core::config::params::PALW_T12_INT11_FLAG_DAY_DAA.map(kaspa_consensus_core::config::params::ForkActivation::new);
    assert_eq!(t12.palw_class_seating.map(|f| f.activation).map(Some), at_h.map(Some), "testnet-12 arms it at the flag day");
    assert_eq!(palw_t12_shipped_params().palw_class_seating, t12.palw_class_seating);
    // Below the height the terms are absent and the door is `Ok` whatever the seats.
    let fx = Fx::new(armed());
    let sp = &fx.sp;
    assert_eq!(sp.class_seating_terms_at(AT - 1), None);
    assert_eq!(sp.class_seating_terms_at(AT), Some(PalwClassSeatingTermsV1 { independent_floor: 3 }));
    let below = palw_class_seated_admits_v1(&fx.s, sp, &room_extras(&fx.p, AT - 1), &fx.bonds()[7], &fx.class, AT - 1);
    assert!(below.is_ok(), "no seat is ready and nothing is asked below the fence: {below:?}");
    // On a ruleset that never armed it the door is `Ok` at every height.
    let dormant = Fx::new(kaspa_consensus_core::config::params::palw_t12_release_v5_params());
    assert_eq!(dormant.sp.class_seating_terms_at(u64::MAX), None);
    assert!(dormant.door(7).is_ok());
}

#[test]
fn arming_it_is_refused_without_its_prerequisites_or_with_a_floor_no_panel_could_meet() {
    let fence = PalwClassSeatingFenceV1::testnet12_v1(ForkActivation::new(AT));
    // Without the generative fence at or below it.
    let mut p = kaspa_consensus_core::config::params::palw_t12_release_v5_params();
    p.palw_class_seating = Some(fence);
    p.sync_palw_class_seating();
    let why = p.validate_palw_v2().unwrap_err().to_string();
    assert!(why.contains("palw_class_seating needs palw_gen_v1"), "{why}");
    // A floor of 0 and a floor above the panel's seat count (5 on testnet-12).
    for floor in [0u16, 6] {
        let mut p = armed();
        p.palw_class_seating = Some(PalwClassSeatingFenceV1 { independent_floor: floor, ..fence });
        p.sync_palw_class_seating();
        let why = p.validate_palw_v2().unwrap_err().to_string();
        assert!(why.contains("independent_floor"), "floor {floor}: {why}");
    }
    // The mirror must agree with the fence.
    let mut p = armed();
    p.palw_class_seating = Some(PalwClassSeatingFenceV1 { independent_floor: 4, ..fence });
    let why = p.validate_palw_v2().unwrap_err().to_string();
    assert!(why.contains("mirror"), "{why}");
    // A raise below the fence's own height, or one that lowers the floor, is refused; a real raise validates.
    let raised = |at: u64, floor: u16| {
        let mut p = armed();
        p.palw_class_seating = Some(fence.with_raise(ForkActivation::new(at), floor));
        p.sync_palw_class_seating();
        p.validate_palw_v2()
    };
    assert!(raised(AT, 4).is_err() && raised(AT + 50, 2).is_err());
    raised(AT + 50, 4).expect("a raise from a later height");
}

#[test]
fn the_fence_is_hashed_into_both_fingerprints_some_only_and_its_floor_with_it() {
    let shipped = palw_t12_shipped_params();
    let a = armed();
    assert_ne!(a.consensus_params_id(), shipped.consensus_params_id(), "an armed fence is a different ruleset");
    assert_ne!(a.consensus_schedule_id(), shipped.consensus_schedule_id());
    // The same height with another floor is another identity; a `never()` fence is the dormant ruleset.
    let four = armed_with(PalwClassSeatingFenceV1 { independent_floor: 4, ..PalwClassSeatingFenceV1::testnet12_v1(ForkActivation::new(AT)) });
    assert_ne!(a.consensus_params_id(), four.consensus_params_id());
    // A raise moves the schedule (it is a height of the fork id).
    let raised = armed_with(PalwClassSeatingFenceV1::testnet12_v1(ForkActivation::new(AT)).with_raise(ForkActivation::new(AT + 50), 4));
    assert_ne!(a.consensus_schedule_id(), raised.consensus_schedule_id());
    // A scheduled fence never moves the IDENTITY (the handshake's id: the fork id carries the height, so a rollout does not
    // partition), and `Some(never())` collapses whole into the dormant ruleset's.
    let mut baseline = a.clone();
    baseline.palw_class_seating = None;
    baseline.sync_palw_class_seating();
    assert_eq!(a.consensus_identity_id(), baseline.consensus_identity_id(), "a scheduled fence is normalised out of the identity");
    let mut never = a.clone();
    never.palw_class_seating = Some(PalwClassSeatingFenceV1::testnet12_v1(ForkActivation::never()));
    never.sync_palw_class_seating();
    assert_eq!(never.consensus_identity_id(), baseline.consensus_identity_id(), "Some(never()) collapses whole in the normaliser");
    never.validate_palw_v2().expect("a dormant value validates");
    assert!(never.palw_class_seating_terms_at(u64::MAX - 1).is_none() && never.palw_class_seating_fence().is_none());
}

// ---------------------------------------------------------------------------------------------
// SEAT-1 … SEAT-11
// ---------------------------------------------------------------------------------------------

/// **SEAT-1: too few operators.** Four distinct operators ready (the executor's excluded): refused, possession floor 4 of 5.
#[test]
fn seat_1_too_few_operators_is_refused_at_the_possession_floor() {
    let fx = Fx::new(armed()).ready(&[0, 1, 2, 3]);
    let seating = fx.seating(Some(7));
    assert_eq!((seating.ready_operators, seating.needed_operators), (4, 5));
    assert_eq!(seating.verdict(), Err(PalwClassNotSeatedV1::Possession { ready: 4, needed: 5 }));
    refused_with(fx.door(7), PalwSeatingFloorV1::Possession, 4, 5);
    // The fifth operator proves readiness and the same claim is admitted.
    let fx = fx.ready(&[4]);
    assert!(fx.door(7).is_ok(), "{:?}", fx.door(7));
}

/// **SEAT-2: the executor does not count.** Exactly five operators ready, the executor's among them: refused with four; the same
/// class and claim under an executor that is not among them is admitted.
#[test]
fn seat_2_the_executor_does_not_count() {
    let fx = Fx::new(armed()).ready(&[0, 1, 2, 3, 4]);
    refused_with(fx.door(0), PalwSeatingFloorV1::Possession, 4, 5);
    refused_with(fx.door(4), PalwSeatingFloorV1::Possession, 4, 5);
    assert!(fx.door(7).is_ok(), "an executor outside the ready set leaves five: {:?}", fx.door(7));
    // With no executor (the lifecycle's and the registry's reading) nothing is excluded: five.
    assert_eq!(fx.seating(None).ready_operators, 5);
}

/// **SEAT-3: enough operators, too few independent.** Six operators ready of which only two are in the base population: refused
/// at the independence floor, 2 of 3; a third independent operator and the next claim is admitted. The registrant's own operator
/// is not independent either.
#[test]
fn seat_3_enough_operators_too_few_independent() {
    // Operators 0..=5 ready; 2..=5 serve only the model class (not the liveness floor's), so they are not in the base population.
    let fx = out_of_base(Fx::new(armed()).ready(&[0, 1, 2, 3, 4, 5]), &[2, 3, 4, 5]);
    let seating = fx.seating(Some(7));
    assert_eq!((seating.ready_operators, seating.independent_operators, seating.needed_independent), (6, 2, 3));
    refused_with(fx.door(7), PalwSeatingFloorV1::Independence, 2, 3);
    // The registry would say so too: the independence floor is unmet.
    assert!(!fx.seating(None).independence_floor_met());
    // A third independent operator proves readiness.
    let fx = fx.ready(&[6]);
    assert!(fx.door(7).is_ok(), "{:?}", fx.door(7));
    // The registrant's operator is neither in Base nor counted: with operator 0 the registrant, operator 6 is the only other
    // base operator ready besides 1 -> two independent, refused.
    let fx = registered_by(fx, 0);
    let seating = fx.seating(Some(7));
    assert_eq!(seating.independent_operators, 2, "the registrant's operator is not independent");
    refused_with(fx.door(7), PalwSeatingFloorV1::Independence, 2, 3);
    // And the executor's operator is never independent: executor operator 1 (ready, in Base) is excluded from both counts.
    let seating = fx.seating(Some(1));
    assert_eq!(seating.independent_operators, 1);
}

/// **SEAT-4: enough.** At least `seat_count` ready operators, three independent. For an attempt class in `Prefetching` with six
/// ready operators it stays `Prefetching`; with a seventh (three independent) it enters `Probation` at the next span boundary.
#[test]
fn seat_4_enough_seats_and_the_lifecycle_enters_probation_only_with_independent_ones() {
    let p = armed();
    let class = model_classes(&p).0;
    let mut chain = Chain::new(p);
    chain.room = true;
    chain.step_at(AT + 10, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let bonds = honest(&chain.p);
    let state_of = |c: &Chain| c.s.model_lifecycle(&class).expect("a model row").clone();
    assert_eq!(state_of(&chain).state, PalwModelLifecycleV1::Prefetching, "a genesis class opens Prefetching");
    // Six ready: READY_SEATS 6/7.
    chain.s = readied(&chain.sp, &chain.s, &bonds[..6], class, chain.daa + 1);
    chain.step(&[]);
    let row = state_of(&chain);
    assert_eq!((row.state, row.ready_seats), (PalwModelLifecycleV1::Prefetching, 6));
    // Seven ready but only two independent (five serve only the model class): enough possession, not enough independence.
    let keys: Vec<PalwBondKeyV2> = bonds[2..7].to_vec();
    chain.s = edited(&chain.sp, &chain.s, |c| {
        for k in &keys {
            c.bonds.get_mut(k).unwrap().capable_classes = std::iter::once(class).collect();
        }
    });
    chain.s = readied(&chain.sp, &chain.s, &bonds[..7], class, chain.daa + 1);
    chain.step(&[]);
    let row = state_of(&chain);
    assert_eq!((row.state, row.ready_seats), (PalwModelLifecycleV1::Prefetching, 7), "seven ready operators, two independent: held in Prefetching");
    // A third independent operator (bond 2 serves the floor's class again): the class enters Probation at the next boundary.
    chain.s = edited(&chain.sp, &chain.s, |c| {
        c.bonds.get_mut(&bonds[2]).unwrap().capable_classes = [class, chain.sp.base_class_id()].into_iter().collect();
    });
    chain.s = readied(&chain.sp, &chain.s, &bonds[..7], class, chain.daa + 1);
    chain.step(&[]);
    assert!(matches!(state_of(&chain).state, PalwModelLifecycleV1::Probation { .. }), "{:?}", state_of(&chain).state);
}

/// **SEAT-5: a lapse.** One independent operator's readiness expires while the class is in `Probation`: the next claim is refused
/// (independence 2/3) and flows again when the operator re-proves.
#[test]
fn seat_5_a_lapse_refuses_the_next_claim_until_the_operator_re_proves() {
    let fx = out_of_base(Fx::new(armed()).ready(&[0, 1, 2, 3, 4, 5]), &[3, 4, 5]);
    // Independent: operators 0, 1, 2 -> exactly the floor.
    assert!(fx.door(7).is_ok(), "{:?}", fx.door(7));
    // Operator 2's proof is older than the horizon (24 spans) and stops counting; the others are fresh.
    let stale = fx.daa - 40;
    let keys = [fx.bonds()[2]];
    let mut fx = fx;
    fx.s = readied(&fx.sp, &fx.s, &keys, fx.class, stale);
    let seating = fx.seating(Some(7));
    assert_eq!((seating.ready_operators, seating.independent_operators), (5, 2));
    refused_with(fx.door(7), PalwSeatingFloorV1::Independence, 2, 3);
    // It re-proves: claims flow again.
    fx.s = readied(&fx.sp, &fx.s, &keys, fx.class, fx.daa);
    assert!(fx.door(7).is_ok());
}

/// **SEAT-6: operators, not bonds.** One operator with three ready bonds counts once: five bonds from three operators are three
/// ready operators, and the claim is refused at the possession floor.
#[test]
fn seat_6_operators_not_bonds() {
    let bonds_of = |fx: &Fx| fx.bonds();
    let fx = Fx::new(armed()).ready(&[0, 1, 2, 3, 4]);
    let op_of = |fx: &Fx, i: usize| fx.s.bond(&bonds_of(fx)[i]).unwrap().operator_id;
    let (shared_a, shared_b) = (op_of(&fx, 0), op_of(&fx, 2));
    let keys = fx.bonds();
    // Bonds 0 and 1 are one operator, bonds 2, 3 and 4 another.
    let fx = fx.edit(move |c| {
        c.bonds.get_mut(&keys[1]).unwrap().operator_id = shared_a;
        c.bonds.get_mut(&keys[3]).unwrap().operator_id = shared_b;
        c.bonds.get_mut(&keys[4]).unwrap().operator_id = shared_b;
    });
    let seating = fx.seating(Some(7));
    assert_eq!(seating.ready_operators, 2, "five bonds, two operators");
    refused_with(fx.door(7), PalwSeatingFloorV1::Possession, 2, 5);
}

/// **SEAT-7: a genesis class** (no registrant): the floor excludes only the executor. Eight genesis operators ready: admitted;
/// with four: refused.
#[test]
fn seat_7_a_genesis_class_excludes_only_the_executor() {
    let all: Vec<usize> = (0..8).collect();
    let fx = Fx::new(armed()).ready(&all);
    assert_eq!(fx.s.class(&fx.class).and_then(|c| c.registrant_bond), None, "a genesis class has no registrant");
    let seating = fx.seating(Some(7));
    assert_eq!((seating.ready_operators, seating.independent_operators), (7, 7));
    assert!(fx.door(7).is_ok());
    let four = Fx::new(armed()).ready(&[0, 1, 2, 3]);
    refused_with(four.door(7), PalwSeatingFloorV1::Possession, 4, 5);
}

/// **SEAT-8: the outsider.** A class held by 3 of a base of 20 reports a licensable share of 150 ‰ before the first claim, and
/// the fraction of outsider draws that hold it is within tolerance of it (ADR-0147's price, kept on purpose: the draw is over all
/// of `Base`, not over its holders).
#[test]
fn seat_8_the_outsider_share_is_reported_and_is_what_the_draw_gives() {
    // The seating's own arithmetic over a base of twenty operators, three independent holders.
    let seating = kaspa_consensus_core::palw_state_v2::PalwClassSeatingV1 {
        ready_operators: 5,
        needed_operators: 5,
        independent_operators: 3,
        needed_independent: 3,
        base_operators: 20,
    };
    assert_eq!(seating.licensable_share_permille(), 150);
    assert_eq!(kaspa_consensus_core::palw_state_v2::PalwClassSeatingV1::default().licensable_share_permille(), 0, "an empty base");
    // The draw: an outsider drawn per claim over Base (here a population of twenty bonds, three of them holders): over 400 claims
    // the licensed fraction is within tolerance of the reported share.
    use kaspa_consensus_core::palw_panel_v2::palw_admission_jury_v1;
    use kaspa_consensus_core::palw_state_v2::{PalwBondStateV2, PalwBondStatusV2};
    let population: Vec<(PalwBondKeyV2, PalwBondStateV2)> = (0..20u64)
        .map(|n| {
            (
                bond_key(100 + n),
                PalwBondStateV2 {
                    pubkey: pubkey_of(n),
                    operator_id: h(0x5000 + n),
                    collateral: RICH,
                    slashed: 0,
                    status: PalwBondStatusV2::Active,
                    registered_daa: 0,
                    payout_payload: h(0x9A00 + n),
                    capable_classes: Default::default(),
                },
            )
        })
        .collect();
    let view: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> = population.iter().map(|(k, b)| (k, b)).collect();
    let holders: std::collections::BTreeSet<Hash64> = (0..3u64).map(|n| h(0x5000 + n)).collect();
    let mut licensed = 0u32;
    for claim in 0..400u64 {
        let outsider = palw_admission_jury_v1(&h(0xABC0_0000 + claim), &view, 1);
        assert_eq!(outsider.len(), 1);
        if holders.contains(&outsider[0]) {
            licensed += 1;
        }
    }
    // 400 draws at p = 0.15: mean 60, sigma ~7: a +-35 band is five sigma.
    assert!((25..=95).contains(&licensed), "the licensed fraction of 400 draws is {licensed}/400, reported share 150‰");
}

/// **SEAT-9: the fence's edge.** A claim below the fence is judged by the old rules (no seating asked); at the fence's own height
/// by the seating rule; and the dormant ruleset asks it at no height.
#[test]
fn seat_9_the_fence_edge() {
    let fx = Fx::new(armed()).ready(&[0, 1]);
    for daa in [AT - 1, AT] {
        let door = palw_class_seated_admits_v1(&fx.s, &fx.sp, &room_extras(&fx.p, daa), &fx.bonds()[7], &fx.class, daa);
        if daa < AT {
            assert!(door.is_ok(), "below the fence: the release's rules byte for byte ({daa}): {door:?}");
        } else {
            refused_with(door, PalwSeatingFloorV1::Possession, 2, 5);
        }
    }
    // The terms are a function of the claim's own DAA, so the claim's whole life reads one answer (ADR-0147 §2.4's way).
    assert_eq!(fx.sp.class_seating_terms_at(AT - 1), None);
    assert!(fx.sp.class_seating_terms_at(AT).is_some() && fx.sp.class_seating_terms_at(AT + 1_000_000).is_some());
}

/// **SEAT-10: the parameter.** `independent_floor` raised from 3 to 4 at a later flag-day entry: a class with three independent
/// operators is admitted below the new height and refused (`Independence`, 3/4) above it.
#[test]
fn seat_10_a_raised_floor_binds_from_its_height() {
    let raise_at = AT + 100;
    let fence = PalwClassSeatingFenceV1::testnet12_v1(ForkActivation::new(AT)).with_raise(ForkActivation::new(raise_at), 4);
    // Eight ready operators of which exactly three are independent of the base: 3, 4, 5, 6, 7 are not in Base.
    let mut fx = out_of_base(Fx::new(armed_with(fence)).ready(&[0, 1, 2, 3, 4, 5, 6]), &[3, 4, 5, 6]);
    fx.daa = AT + 40;
    assert!(fx.door(7).is_ok(), "three independent operators meet the floor of 3: {:?}", fx.door(7));
    fx.s = readied(&fx.sp, &fx.s, &fx.bonds()[..7], fx.class, raise_at + 1);
    fx.daa = raise_at + 1;
    refused_with(fx.door(7), PalwSeatingFloorV1::Independence, 3, 4);
    assert_eq!(fx.sp.class_seating_terms_at(raise_at - 1), Some(PalwClassSeatingTermsV1 { independent_floor: 3 }));
    assert_eq!(fx.sp.class_seating_terms_at(raise_at), Some(PalwClassSeatingTermsV1 { independent_floor: 4 }));
}

/// **SEAT-11: a composite class.** Readiness is per class id (and a composite adds its parent clause): the parent held by seven
/// operators and a second class id by two — the second is refused and the parent's claims are unaffected.
#[test]
fn seat_11_readiness_is_per_class_id() {
    let parent = Fx::new(armed()).ready(&[0, 1, 2, 3, 4, 5, 6]);
    let other = model_classes(&parent.p).1;
    let mut child = Fx::new(armed());
    child.class = other;
    child.s = activated(&child.sp, &child.s, other);
    child.s = readied(&child.sp, &child.s, &child.bonds()[..2], other, child.daa);
    assert!(parent.door(7).is_ok(), "{:?}", parent.door(7));
    refused_with(child.door(7), PalwSeatingFloorV1::Possession, 2, 5);
    // The parent's rows did not move the other class's count.
    assert_eq!(parent.seating(None).ready_operators, 7);
}

/// The base class is never gated, and a class the registry gates no claim of (no lifecycle row, no generative row) is not
/// either: the legacy rule.
#[test]
fn the_floor_and_an_unrowed_class_are_never_gated() {
    let fx = Fx::new(armed());
    let base = fx.sp.base_class_id();
    let door = palw_class_seated_admits_v1(&fx.s, &fx.sp, &room_extras(&fx.p, fx.daa), &fx.bonds()[7], &base, fx.daa);
    assert!(door.is_ok(), "{door:?}");
    let stranger = h(0xDEAD_BEEF);
    let door = palw_class_seated_admits_v1(&fx.s, &fx.sp, &room_extras(&fx.p, fx.daa), &fx.bonds()[7], &stranger, fx.daa);
    assert!(door.is_ok(), "{door:?}");
}

/// **The doors in the real fold**: a model attempt of a class the lifecycle admits but no panel with three independent operators
/// can be drawn for is refused with `ClassNotSeated` BEFORE its reservation is taken (no `NoCapablePanel` void follows); with the
/// seats independent it is accepted.
#[test]
fn the_attempt_door_in_the_real_fold_refuses_before_a_reservation_and_admits_once_seated() {
    let p = armed();
    let class = model_classes(&p).0;
    let mut chain = model_chain(p, class, 2);
    chain.daa = AT + 30;
    // The class is Active (made so by the harness) and every genesis bond ready and independent: seated. A rich non-genesis bond
    // executes.
    let id = model_claim(&mut chain, class, 1, 1);
    assert!(chain.s.claim(&id).is_some(), "seated: the attempt is accepted");
    // Six of the eight seats serve only this class (outside the base population): the lifecycle still admits (Active, a panel can
    // be drawn) but only two independent operators remain.
    let bonds = honest(&chain.p);
    let outside: Vec<PalwBondKeyV2> = bonds[2..].to_vec();
    chain.s = edited(&chain.sp, &chain.s, |c| {
        for k in &outside {
            c.bonds.get_mut(k).unwrap().capable_classes = std::iter::once(class).collect();
        }
    });
    chain.s = readied(&chain.sp, &chain.s, &bonds, class, chain.daa);
    let reserved_before = chain.reserved(&bond_key(2));
    let pwu = class_pwu(&chain.p, &chain.s, class, chain.daa + 1);
    let (env, key, _) = junk_attempt(class, bond_key(2), pubkey_of(2), &operator_pubkey_of(2), pwu, 2, 0x5_0000 + 2);
    let at = chain.daa + 1;
    let refused = chain.try_fold(&chain.s, &ctx(0xCA_0000 + at, at, at, 0), &[], PalwBlockWorkV3::Attempt(&env), key);
    match refused {
        Err(PalwStateV2Error::ClassNotSeated { floor: PalwSeatingFloorV1::Independence, have: 2, need: 3, .. }) => {}
        other => panic!("expected ClassNotSeated {{ Independence, 2, 3 }}: {:?}", other.map(|_| ())),
    }
    assert_eq!(chain.reserved(&bond_key(2)), reserved_before, "nothing was reserved");
}
