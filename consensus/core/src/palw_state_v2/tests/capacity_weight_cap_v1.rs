//! **ADR-0160 F-W (lane cap-weight) through the fold** — staged claim weight, the per-bond weight
//! cap (J-1) and the capped consensus reservation, named by the ADR's test ids (§7.1): W-T1 … W-T9.
//! W-T10 (the fence's plumbing and pins) is `consensus/core/tests/palw_capacity_weight_cap_is_t12_only.rs`.
//!
//! The classes carry testnet-12's numbers (ADR-0160 Appendix A): a floor claim's raw weight is one
//! FCW (604,250,611) and its reservation 10,752,660 sompi; 8k's raw weight is the C7 ceiling
//! (138,892,697,241) and its reservation 2,471,600,230; 2M's raw weight is ≈ 555,611 FCW and its
//! reservation 5,974,294,206,820. Raw weight is `⌊β·pwu⌋` of the attempt's pwu (β = 100‰) and the
//! reservation `pwu_per_inference × 5` (`DerivedV1`), so the two are set independently, as on the chain.

use super::*;
use crate::palw_economic_safety_v1::PalwLicenceDoorTagV1;
use crate::palw_fork_authority_v2::{PalwDeepReorgV2, decide_deep_reorg_v2, palw_deep_reorg_strict_economic_v1};
use crate::palw_weight_cap_v1::*;

const MSK: u64 = 100_000_000;
const FLOOR: u64 = 1;
const K8: u64 = 0x88;
const M2: u64 = 0x22;
const SEAT: u64 = 0xEE;
const W_FLOOR: u128 = 10_752_660;
const W_8K: u128 = 2_471_600_230;
const W_2M: u128 = 5_974_294_206_820;

/// `(artifact root word, pwu_per_inference, attempt pwu)` of a fixture class.
fn class_row(class: u64) -> (u64, u64, u64) {
    match class {
        FLOOR => (11, 2_150_532, 6_042_506_110),
        K8 => (0x8811, 494_320_046, 1_388_926_972_410),
        M2 => (0x2211, 1_194_858_841_364, 3_357_310_000_000_000),
        _ => unreachable!("a fixture class"),
    }
}

fn raw_of(class: u64) -> u128 {
    u128::from(class_row(class).2) / 10
}

fn w_of(class: u64) -> u128 {
    u128::from(class_row(class).1) * 5
}

fn cp(fence: Option<u64>) -> PalwStateParamsV2 {
    params().with_capacity_weight_cap_from_daa(fence)
}

fn class_registered(class: u64) -> PalwConsensusObjectV2 {
    let (root, pwu_per_inference, _) = class_row(class);
    PalwConsensusObjectV2::ClassRegistered {
        class_id: h64(class),
        artifact_root: h64(root),
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
        initial_target: u128::MAX / 2,
        share_permille: if class == FLOOR { 1000 } else { 0 },
        activation_daa: 0,
        admission: None,
    }
}

fn bond_registered(word: u64, collateral: u64) -> PalwConsensusObjectV2 {
    assert!(word < 256, "op_key keys on the low byte");
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(word),
        pubkey: vec![word as u8; 4],
        operator_pubkey: op_key(word),
        collateral,
        payout_payload: h64(0x9A00 + word),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

/// The three classes, the seat and `bonds` (`(word, collateral)`).
fn genesis_objects(bonds: &[(u64, u64)]) -> Vec<PalwConsensusObjectV2> {
    let mut objects = vec![class_registered(FLOOR), class_registered(K8), class_registered(M2), bond_registered(SEAT, 1_000 * MSK)];
    objects.extend(bonds.iter().map(|(word, collateral)| bond_registered(*word, *collateral)));
    objects
}

fn env(class: u64, bond: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
    let (root, _, pwu) = class_row(class);
    attempt_for_class(pwu, nonce, h64(class), bond_key(bond), vec![bond as u8; 4], op_id(bond), h64(root))
}

fn bind(claim: Hash64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::PanelBound {
        claim,
        anchor: h64(77),
        seats: vec![PalwPanelSeatV2 { bond: bond_key(SEAT), operator_id: op_id(SEAT) }],
    }
}

fn licence(claim: Hash64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ReceiptLicensed {
        claim,
        receipts: vec![crate::palw_panel_v2::PalwSeatReceiptV2 {
            claim: Hash64::default(),
            verdict: crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
            seat_bond: bond_key(SEAT),
            signed_daa: 0,
            signature: Vec::new(),
        }],
    }
}

fn fcw(n: u128) -> u128 {
    n * PALW_CAPACITY_FCW_V1
}

/// `Σ` raw weight of the live claims the fence does not apply to.
fn old_rule_raw(s: &PalwChainStateV2, p: &PalwStateParamsV2) -> u128 {
    s.claims_iter()
        .filter(|(_, c)| !c.phase.is_terminal() && !palw_weight_cap_applies_v1(p, c))
        .map(|(_, c)| c.immature_contribution)
        .sum()
}

fn live(s: &PalwChainStateV2) -> u128 {
    s.safe_weight() + s.bounded_immature()
}

/// A little chain driver: one block per `step`, blue +1, the DAA given.
struct World {
    p: PalwStateParamsV2,
    extras: PalwTransitionExtrasV1,
    s: PalwChainStateV2,
    blue: u64,
    nonce: u64,
    /// Run the delta / revert / carriage checks on every block (off for the long piles).
    heavy: bool,
}

impl World {
    fn new(p: PalwStateParamsV2, bonds: &[(u64, u64)], heavy: bool) -> Self {
        let mut world = World { p, extras: PalwTransitionExtrasV1::default(), s: PalwChainStateV2::genesis(), blue: 0, nonce: 0, heavy };
        world.step(100, genesis_objects(bonds), None).unwrap();
        world
    }

    fn try_step(
        &mut self,
        daa: u64,
        objects: Vec<PalwConsensusObjectV2>,
        att: Option<&PalwAttemptEnvelopeV2>,
    ) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
        let c = ctx(0x10_0000 + self.blue + 1, daa, self.blue + 1);
        apply_palw_transition_v2_with_extras(&self.s, &self.p, &c, &objects, att, false, false, false, false, &self.extras)
    }

    fn step(&mut self, daa: u64, objects: Vec<PalwConsensusObjectV2>, att: Option<&PalwAttemptEnvelopeV2>) -> Result<PalwStateDeltaV2, PalwStateV2Error> {
        let (child, delta) = self.try_step(daa, objects, att)?;
        if self.heavy {
            checked(&self.s, &child, &delta, &self.p);
        } else {
            assert_eq!(palw_bounded_immature_v2(&child, &self.p), child.bounded_immature(), "W-I3 after the block");
        }
        self.s = child;
        self.blue += 1;
        Ok(delta)
    }

    /// Accept one claim of `class` on `bond` at `daa`; its id.
    fn claim(&mut self, daa: u64, class: u64, bond: u64) -> Hash64 {
        self.nonce += 1;
        let e = env(class, bond, self.nonce);
        let id = attempt_id_v2(&e.attempt);
        self.step(daa, vec![], Some(&e)).unwrap_or_else(|err| panic!("claim {class:#x} on {bond:#x}: {err:?}"));
        id
    }
}

/// **W-I3 on every door, and the delta / carriage paths rebuild what the fold maintained**: the
/// running `bounded_immature` equals its re-derivation, the delta reproduces the child (root, weight,
/// index) and reverts to the parent, and a carriage re-imports under the committed root with the
/// index rebuilt identically (the restart path).
fn checked(parent: &PalwChainStateV2, child: &PalwChainStateV2, delta: &PalwStateDeltaV2, p: &PalwStateParamsV2) {
    child.assert_internal_consistency(p).expect("internal consistency (W-I3 and the index included)");
    assert_eq!(palw_bounded_immature_v2(child, p), child.bounded_immature(), "W-I3: the running value is the re-derivation");
    let applied = apply_delta_v2(parent, delta, p).unwrap();
    assert_eq!(applied.state_root(), child.state_root(), "the delta reproduces the fold");
    assert_eq!(applied.bounded_immature(), child.bounded_immature());
    assert_eq!(applied.capacity_weight_index(), child.capacity_weight_index(), "and its weight index");
    let back = revert_delta_v2(child, delta, p).unwrap();
    assert_eq!(back.state_root(), parent.state_root(), "the delta reverts to the parent");
    assert_eq!(back.bounded_immature(), parent.bounded_immature());
    assert_eq!(back.capacity_weight_index(), parent.capacity_weight_index(), "and its weight index");
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(child)).unwrap();
    let imported = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(p, Some(child.state_root())).unwrap();
    assert_eq!(imported.capacity_weight_index(), child.capacity_weight_index(), "a restart rebuilds the index");
    assert_eq!(imported.bounded_immature(), child.bounded_immature());
}

// ---- W-T2: the staging table ------------------------------------------------------------------

/// **W-T2: the staging table, phase by phase** (ADR-0160 §4.2), on real claim records: Created 0,
/// Anchored 10‰, Licensed full (the counted licence) or 250‰ (S2), an open DA accusation keeps the
/// stage it found, Final and void 0 in the immature set; the full weight is `min(raw, C7 ceiling)`
/// and the Final `safe_weight` contribution follows the same ceiling. An old-rule claim is staged at
/// nothing (it weighs raw, uncapped) and a free-prompt claim is never under the fence.
#[test]
fn w_t2_the_staging_table_phase_by_phase() {
    let p = cp(Some(0));
    let mut world = World::new(p.clone(), &[(0x21, 100_000 * MSK)], true);
    let ids: Vec<(u64, Hash64)> = [FLOOR, K8, M2].into_iter().map(|class| (class, world.claim(101, class, 0x21))).collect();
    for (class, id) in ids {
        let claim = world.s.claim(&id).unwrap().clone();
        let full = raw_of(class).min(PALW_CAPACITY_C7_WEIGHT_CEILING_V1);
        assert_eq!(claim.immature_contribution, raw_of(class), "the stored raw weight is today's, untouched (C is kept)");
        assert_eq!(palw_weight_full_v1(&p, &claim), full);
        let staged = |phase: PalwClaimPhaseV2, door: Option<PalwLicenceDoorTagV1>, basis_k: u8| {
            let mut c = claim.clone();
            c.phase = phase;
            c.rcore.licence_door = door;
            c.rcore.basis_k = basis_k;
            (palw_weight_stage_of_claim_v1(&c), palw_staged_weight_v1(&p, &c))
        };
        let licensed = PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 103 };
        let bound = PalwClaimPhaseV2::PanelBound { bound_daa: 102 };
        assert_eq!(staged(PalwClaimPhaseV2::Provisional, None, 0), (PalwWeightStageV1::Created, 0));
        assert_eq!(staged(bound.clone(), None, 0), (PalwWeightStageV1::Anchored, full / 100));
        assert_eq!(staged(licensed.clone(), None, 0), (PalwWeightStageV1::Licensed { permille: 1000 }, full), "below R-core+");
        assert_eq!(
            staged(licensed.clone(), Some(PalwLicenceDoorTagV1::Coverage), 2),
            (PalwWeightStageV1::Licensed { permille: 1000 }, full),
            "a counted licence (basis_k ≥ 2)"
        );
        assert_eq!(
            staged(licensed.clone(), Some(PalwLicenceDoorTagV1::Optimistic), 1),
            (PalwWeightStageV1::Licensed { permille: 250 }, full / 4),
            "S2"
        );
        for resumed in [PalwClaimPhaseV2::Provisional, bound.clone(), licensed.clone()] {
            let disputed = PalwClaimPhaseV2::DefaultDisputed {
                accused_daa: 104,
                missing_event_index: 0,
                accuser: bond_key(SEAT),
                accuser_exposure: 1,
                resumed: Box::new(resumed.clone()),
            };
            assert_eq!(staged(disputed, None, 0), staged(resumed, None, 0), "an accusation keeps the stage it found (W-I2)");
        }
        assert_eq!(staged(PalwClaimPhaseV2::Final { final_daa: 124 }, None, 0), (PalwWeightStageV1::Final, 0));
        assert_eq!(
            staged(PalwClaimPhaseV2::Voided { voided_daa: 111, reason: PalwVoidReasonV2::BindTimeout }, None, 0),
            (PalwWeightStageV1::Terminal, 0)
        );
        // Final: the safe contribution under the ceiling — the identity below it.
        let contribution = u128::from(claim.pwu);
        let safe = palw_weight_final_safe_v1(&p, &claim, contribution);
        if class == M2 {
            assert_eq!(safe, contribution * PALW_CAPACITY_C7_WEIGHT_CEILING_V1 / raw_of(M2), "2M's Final weighs as 8k's (D-3)");
            assert!(safe < contribution / 2_000);
        } else {
            assert_eq!(safe, contribution, "every attributable class keeps its Final full");
        }
        // Old-rule: the same record accepted below the fence.
        let old = cp(Some(102));
        assert_eq!(palw_staged_weight_v1(&old, &claim), 0, "an old-rule claim is not staged");
        assert_eq!(palw_weight_full_v1(&old, &claim), raw_of(class), "and weighs raw");
        assert_eq!(palw_weight_final_safe_v1(&old, &claim, contribution), contribution);
        // A free-prompt claim is never under the fence (E-6).
        let mut fp = claim.clone();
        fp.source = PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() };
        assert!(!palw_weight_cap_applies_v1(&p, &fp));
    }
}

// ---- W-T1 / W-T9: claims × N is not fork power × N ----------------------------------------------

struct Pile {
    created: u128,
    anchored: u128,
    licensed: u128,
    reserved: Vec<u128>,
    state: PalwChainStateV2,
}

/// `n` claims of `class` on one 13,000 MSK bond, all accepted at DAA 101, bound at 102, licensed at 103.
fn pile(fence: Option<u64>, class: u64, n: u64) -> Pile {
    let bond = 0x21;
    let mut world = World::new(cp(fence), &[(bond, 13_000 * MSK)], false);
    let mut ids = Vec::new();
    let mut reserved = Vec::new();
    for _ in 0..n {
        // SR-7: the ONE reading admission and the producer's headroom share predicts the stored value.
        let predicted = palw_claim_weight_reservation_v1(&world.s, &world.p, &bond_key(bond), w_of(class), 101);
        let id = world.claim(101, class, bond);
        let stored = world.s.claim(&id).unwrap().reserved;
        assert_eq!(stored, predicted, "the fold stores what admission and the producer predict");
        reserved.push(stored);
        ids.push(id);
    }
    let created = world.s.bounded_immature();
    world.step(102, ids.iter().map(|id| bind(*id)).collect(), None).unwrap();
    let anchored = world.s.bounded_immature();
    world.step(103, ids.iter().map(|id| licence(*id)).collect(), None).unwrap();
    let licensed = world.s.bounded_immature();
    Pile { created, anchored, licensed, reserved, state: world.s }
}

/// **W-T1 `claims_times_n_is_not_fork_power_times_n`** (ADR-0160 §4.2, invariant W-I1): 1, 10, 100
/// and 1,000 claims on a 13,000 MSK bond keep the bond's provisional weight at or under 2 FCW —
/// floor, 8k and 2M alike — at every stage, where the dormant twin weighs `N × raw` at every stage.
///
/// And **W-T9's reservation half**: the bond's whole weight reservation is at most `R_budget` =
/// 0.215 MSK (a 2M claim's 59,742.94 MSK falls to ≤ 0.215 MSK; the floor's two claims fill it exactly,
/// the third takes nothing), against `N × w` dormant. Printed as the lane's measurement.
#[test]
fn w_t1_claims_times_n_is_not_fork_power_times_n() {
    let cap = palw_bond_weight_cap_v1(13_000 * MSK);
    let budget = palw_bond_weight_budget_sompi_v1(13_000 * MSK);
    assert_eq!((cap, budget), (fcw(2), 21_505_320));
    for class in [FLOOR, K8, M2] {
        let full = raw_of(class).min(PALW_CAPACITY_C7_WEIGHT_CEILING_V1);
        for n in [1u64, 10, 100, 1_000] {
            let armed = pile(Some(0), class, n);
            let dormant = pile(None, class, n);
            let n128 = u128::from(n);
            // Dormant: today's accounting, additive in N at every stage.
            assert_eq!(
                (dormant.created, dormant.anchored, dormant.licensed),
                (n128 * raw_of(class), n128 * raw_of(class), n128 * raw_of(class)),
                "{class:#x} × {n}: the dormant twin is today's N × raw"
            );
            assert!(dormant.reserved.iter().all(|r| *r == w_of(class)), "dormant, every claim reserves w");
            // Armed: staged and capped.
            assert_eq!(armed.created, 0, "{class:#x} × {n}: Created weighs nothing");
            assert_eq!(armed.anchored, (n128 * (full / 100)).min(cap), "{class:#x} × {n}: Anchored 10‰ under the cap");
            assert_eq!(armed.licensed, (n128 * full).min(cap), "{class:#x} × {n}: Licensed full under the cap");
            for stage in [armed.created, armed.anchored, armed.licensed] {
                assert!(stage <= fcw(2), "W-I1: {class:#x} × {n} stays ≤ 2 FCW");
            }
            let total: u128 = armed.reserved.iter().sum();
            assert!(total <= budget, "W-I4: {class:#x} × {n} reserves {total} ≤ R_budget {budget}");
            assert_eq!(total, (n128 * w_of(class)).min(budget), "the budget fills in acceptance order, then nothing");
            assert_eq!(armed.state.capacity_weight_index().bond(&bond_key(0x21)).reserved_w, total);
            println!(
                "W-T1 {:>5} × {n:>4} on 13k: provisional weight {:>10.2} FCW armed vs {:>14.2} FCW dormant; weight reservation {:.6} MSK armed vs {:.2} MSK dormant",
                match class {
                    FLOOR => "floor",
                    K8 => "8k",
                    _ => "2M",
                },
                armed.licensed as f64 / PALW_CAPACITY_FCW_V1 as f64,
                dormant.licensed as f64 / PALW_CAPACITY_FCW_V1 as f64,
                total as f64 / MSK as f64,
                dormant.reserved.iter().sum::<u128>() as f64 / MSK as f64,
            );
        }
    }
    // The 2M row by name: the reservation per claim.
    let one = pile(Some(0), M2, 2);
    assert_eq!(one.reserved, vec![budget, 0], "a 2M claim reserves ≤ 0.215 MSK at 13k, the next nothing");
    let floor = pile(Some(0), FLOOR, 3);
    assert_eq!(floor.reserved, vec![W_FLOOR, W_FLOOR, 0], "the floor's two instant claims fill the budget exactly");
    assert_eq!(w_of(K8), W_8K);
    assert_eq!(w_of(M2), W_2M);
}

/// **W-T9's collateral half: concurrent claims on a 13,000 MSK bond with option A's escrow** (`E`
/// reserved on the bond, the 500‰ ceiling, 6,500 MSK) — what this lane ALONE changes, before lane escrow
/// takes `E` off the bond:
///
/// * floor: `E + 0.1075` a claim, 2 either way (E binds);
/// * 8k: `E + 24.716` dormant, `E + ≤ 0.215` armed — 2 instantly either way, the pair committing 49.2 MSK
///   less (the "1 sustained → 2" half needs R-core+'s licence release and the capacity harness
///   `C_8k_13k`, audit/claim-capacity, not re-run here);
/// * 2M: `E + 59,742.94` dormant — NOT ONE fits (the minimum collateral for one is 125,887.6 MSK) — and
///   `E + ≤ 0.215` armed: two fit on 13k. (C7's network cap of one 2M claim is unchanged.)
#[test]
fn w_t9_a_13k_bond_holds_e_plus_at_most_the_budget_per_claim() {
    const SUBSIDY: u64 = 444_562_000_000;
    let run = |class: u64, fence: Option<u64>| {
        let p = cp(fence)
            .with_worker_carve_permille(720)
            .unwrap()
            .with_escrow_backed_exposure_from_daa(Some(0))
            .with_fp_exposure_ceiling(500)
            .unwrap();
        let mut world = World::new(p, &[(0x21, 13_000 * MSK)], true);
        world.extras = PalwTransitionExtrasV1 { audit_2026_09_23_active: true, ..Default::default() };
        let mut commitments = Vec::new();
        for nonce in 1..=4u64 {
            let e = env(class, 0x21, nonce);
            let c = ctx(0x20_0000 + nonce, 100 + nonce, world.blue + 1);
            let c = PalwBlockContextV2 { subsidy: SUBSIDY, ..c };
            let (child, delta) =
                apply_palw_transition_v2_with_extras(&world.s, &world.p, &c, &[], Some(&e), false, false, false, false, &world.extras)
                    .unwrap();
            checked(&world.s, &child, &delta, &world.p);
            world.s = child;
            world.blue += 1;
            // A refused own attempt is SKIPPED (no claim; the block stands) — the ceiling's refusal.
            if world.s.claim(&attempt_id_v2(&e.attempt)).is_none() {
                break;
            }
            commitments.push(world.s.reserved_exposure(&bond_key(0x21)));
        }
        commitments
    };
    let e = u128::from(worker_carve_v2(&cp(None).with_worker_carve_permille(720).unwrap(), SUBSIDY, None));
    let budget = palw_bond_weight_budget_sompi_v1(13_000 * MSK);
    for (class, name, dormant_n) in [(FLOOR, "floor", 2usize), (K8, "8k", 2), (M2, "2M", 0)] {
        let (dormant, armed) = (run(class, None), run(class, Some(0)));
        assert_eq!((dormant.len(), armed.len()), (dormant_n, 2), "{name}: concurrent claims on 13k, dormant vs armed (E binds armed)");
        if let Some(last) = dormant.last() {
            assert_eq!(*last, dormant_n as u128 * (e + w_of(class)), "{name} dormant: E + w each");
        }
        let armed_w = (2 * w_of(class)).min(budget);
        assert_eq!(armed[1], 2 * e + armed_w, "{name} armed: E each, and at most the budget once");
        println!(
            "W-T9 {name:>5}: 13k holds {} dormant ({:.4} MSK committed) vs {} armed ({:.4} MSK committed); per-claim reserve {:.4} → ≤ {:.4} MSK",
            dormant.len(),
            dormant.last().copied().unwrap_or(0) as f64 / MSK as f64,
            armed.len(),
            armed[1] as f64 / MSK as f64,
            (e + w_of(class)) as f64 / MSK as f64,
            (e + w_of(class).min(budget)) as f64 / MSK as f64,
        );
    }
}

// ---- W-T4: below the fence, byte for byte -----------------------------------------------------

/// A scripted chain over every door (acceptance on three classes and two bonds, bind, licence, a
/// bind timeout, a redraw, a second receipt timeout, Final), driven by `world`.
fn script(world: &mut World) -> Vec<PalwChainStateV2> {
    let mut states = vec![world.s.clone()];
    let a1 = world.claim(101, FLOOR, 0x21);
    states.push(world.s.clone());
    let a2 = world.claim(102, K8, 0x21);
    states.push(world.s.clone());
    world.step(102, vec![bind(a1), bind(a2)], None).unwrap();
    states.push(world.s.clone());
    let b1 = world.claim(103, M2, 0x31);
    states.push(world.s.clone());
    world.step(103, vec![licence(a1), bind(b1)], None).unwrap();
    states.push(world.s.clone());
    let b2 = world.claim(104, K8, 0x31);
    states.push(world.s.clone());
    world.step(105, vec![licence(b1)], None).unwrap(); // a2's receipt window (102 + 10) still open
    states.push(world.s.clone());
    // 113: a2's receipt window closed → redraw to Provisional; b2 (104 + 10) → bind timeout void.
    world.step(115, vec![], None).unwrap();
    states.push(world.s.clone());
    world.step(116, vec![bind(a2)], None).unwrap();
    states.push(world.s.clone());
    // 124: a1 Final (103 + 20); 126: a2's second receipt window closed → void (ReceiptTimeout).
    world.step(130, vec![], None).unwrap();
    states.push(world.s.clone());
    let _ = b2;
    states
}

/// **W-T4: the below-fence twin fold** — the same chain folded with the fence `None` and armed at a
/// height above its tip gives identical states (root, weights, every index) at every block (W-I5):
/// the rule is keyed on `accepted_daa`, so a claim accepted below the height is today's claim.
#[test]
fn w_t4_below_the_fence_the_twin_folds_are_identical() {
    let bonds = [(0x21, 13_000 * MSK), (0x31, 100_000 * MSK)];
    let mut dormant = World::new(cp(None), &bonds, true);
    let mut above = World::new(cp(Some(1_000)), &bonds, true);
    let (a, b) = (script(&mut dormant), script(&mut above));
    assert_eq!(a.len(), b.len());
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        assert_eq!(x.state_root(), y.state_root(), "block {i}: identical roots");
        assert_eq!(x, y, "block {i}: identical states, the weight index included (empty)");
        assert!(y.capacity_weight_index().is_empty());
    }
    let last = a.last().unwrap();
    assert!(last.claims_iter().any(|(_, c)| matches!(c.phase, PalwClaimPhaseV2::Final { .. })), "the script reached Final");
    assert!(
        last.claims_iter().any(|(_, c)| matches!(c.phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. })),
        "and a bind timeout"
    );
}

// ---- W-T5: reorg and restart across bind, licence, Final and the fence -------------------------

/// **W-T5: a chain crossing the fence** (armed at 105) through acceptance, bind, licence, redraw,
/// Final and void, every block checked (delta, revert, carriage import = the restart path); old-rule
/// claims keep their raw weight past the height (W-I5); then a reorg reverts every delta back to the
/// pre-fence state and a different branch crosses again.
#[test]
fn w_t5_reorg_and_restart_across_the_fence() {
    let p = cp(Some(105));
    let bonds = [(0x21, 13_000 * MSK), (0x31, 100_000 * MSK)];
    let mut world = World::new(p.clone(), &bonds, true);
    let a1 = world.claim(101, FLOOR, 0x21); // old rule
    let a2 = world.claim(102, K8, 0x21); // old rule
    world.step(102, vec![bind(a1), bind(a2)], None).unwrap();
    world.step(103, vec![licence(a1), licence(a2)], None).unwrap();
    let b1 = world.claim(104, M2, 0x31); // old rule: 555,611 FCW, uncapped for its whole life
    let fork_point = world.s.clone();
    let mut deltas = Vec::new();
    let mut states = vec![world.s.clone()];
    let mut record = |world: &World, delta: PalwStateDeltaV2, deltas: &mut Vec<PalwStateDeltaV2>| {
        deltas.push(delta);
        states.push(world.s.clone());
    };
    let n1 = {
        world.nonce += 1;
        let e = env(K8, 0x21, world.nonce);
        let d = world.step(105, vec![bind(b1)], Some(&e)).unwrap();
        record(&world, d, &mut deltas);
        attempt_id_v2(&e.attempt)
    };
    assert!(!palw_weight_cap_applies_v1(&p, world.s.claim(&a1).unwrap()), "accepted at 101: old rule");
    assert!(palw_weight_cap_applies_v1(&p, world.s.claim(&n1).unwrap()), "accepted at 105: new rule");
    let old_raw = raw_of(FLOOR) + raw_of(K8) + raw_of(M2);
    assert_eq!(old_rule_raw(&world.s, &p), old_raw);
    assert_eq!(world.s.bounded_immature(), old_raw, "W-I5: the old-rule claims weigh raw past the height; n1 is Created");
    let n2 = {
        world.nonce += 1;
        let e = env(FLOOR, 0x31, world.nonce);
        let d = world.step(106, vec![bind(n1), licence(b1)], Some(&e)).unwrap();
        record(&world, d, &mut deltas);
        attempt_id_v2(&e.attempt)
    };
    let d = world.step(107, vec![licence(n1), bind(n2)], None).unwrap();
    record(&world, d, &mut deltas);
    assert_eq!(
        world.s.bounded_immature(),
        old_raw + fcw(2) + raw_of(FLOOR) / 100,
        "n1 (8k, 229.86 FCW) licensed under 13k's 2 FCW; n2 anchored at 10‰ under 100k's 15"
    );
    // 124: a1 and a2 Final (old rule, raw into immature out, pwu into safe); 118: n2's receipt window
    // closed → redraw; 128: n1 Final.
    for daa in [119, 125, 129] {
        let d = world.step(daa, vec![], None).unwrap();
        record(&world, d, &mut deltas);
    }
    assert!(matches!(world.s.claim(&n1).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
    assert!(matches!(world.s.claim(&n2).unwrap().phase, PalwClaimPhaseV2::Provisional), "n2 was redrawn");
    assert!(world.s.capacity_weight_index().bond(&bond_key(0x21)).staged == 0, "n1's weight left the index at Final");
    // The reorg: revert every delta to the fork point, each state exactly the recorded one.
    let mut s = world.s.clone();
    for (delta, parent) in deltas.iter().rev().zip(states.iter().rev().skip(1)) {
        s = revert_delta_v2(&s, delta, &p).unwrap();
        assert_eq!(&s, parent, "the revert restores the parent, index and weight included");
    }
    assert_eq!(s, fork_point);
    // …and a different branch crosses the fence from it: 2M junk on the 13k bond, all licensed.
    world.s = fork_point;
    let mut junk = Vec::new();
    for i in 0..5u64 {
        world.nonce += 1;
        let e = env(M2, 0x21, world.nonce);
        world.step(105 + i, vec![], Some(&e)).unwrap();
        junk.push(attempt_id_v2(&e.attempt));
    }
    world.step(110, junk.iter().map(|id| bind(*id)).collect(), None).unwrap();
    world.step(111, junk.iter().map(|id| licence(*id)).collect(), None).unwrap();
    assert_eq!(
        world.s.bounded_immature() - old_raw,
        fcw(2),
        "five licensed 2M claims on 13k weigh two floor claims (the old-rule 555,611 FCW 2M claim above them is today's)"
    );
}

// ---- W-T3: a property test over every door on three bonds ---------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// **W-T3: accept, bind, licence, redraw, Final, void and slash on three bonds**, in random order over
/// many seeds (a deterministic property test — the crate carries no proptest), with the audit fence's
/// second-timeout charge on so the second receipt timeout SLASHES the producer (a collateral move the
/// cap follows). After every block: W-I1 (each bond's term ≤ its cap), W-I3 (re-derivation, the
/// delta, the revert and — every eighth block — the carriage import), W-I4 at every acceptance (the
/// stored reservation is `min(w, R_budget − held)` of the parent), W-I2 (live weight falls only in a
/// block that voided, redrew or slashed), W-I6 (no underflow: the fold's debug asserts).
#[test]
fn w_t3_a_random_lattice_on_three_bonds_keeps_every_invariant() {
    let bonds = [(0x21u64, 13_000 * MSK), (0x31, 26_000 * MSK), (0x41, 100_000 * MSK)];
    let mut slashes = 0;
    let mut falls = 0;
    for seed in 1..=10u64 {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed.wrapping_mul(0xA24B_AED4_963E_E407));
        let mut world = World::new(cp(Some(0)), &bonds, false);
        world.extras = PalwTransitionExtrasV1 { audit_2026_09_23_active: true, ..Default::default() };
        let mut daa = 101;
        for block in 0..120u64 {
            let parent = world.s.clone();
            let live_claims: Vec<(Hash64, PalwClaimStateV2)> =
                parent.claims_iter().filter(|(_, c)| !c.phase.is_terminal()).map(|(id, c)| (*id, c.clone())).collect();
            let mut objects = Vec::new();
            for (id, claim) in &live_claims {
                match claim.phase {
                    PalwClaimPhaseV2::Provisional if rng.below(3) == 0 => objects.push(bind(*id)),
                    PalwClaimPhaseV2::PanelBound { .. } if rng.below(3) == 0 => objects.push(licence(*id)),
                    _ => {}
                }
            }
            daa += [0, 0, 1, 1, 3, 11, 21][rng.below(7) as usize];
            let att = (rng.below(2) == 0).then(|| {
                world.nonce += 1;
                let class = [FLOOR, FLOOR, K8, M2][rng.below(4) as usize];
                let word = bonds[rng.below(3) as usize].0;
                let e = env(class, word, world.nonce);
                let predicted = palw_claim_weight_reservation_v1(&parent, &world.p, &bond_key(word), w_of(class), daa);
                (attempt_id_v2(&e.attempt), bond_key(word), predicted, e)
            });
            let (child, delta) = match world.try_step(daa, objects.clone(), att.as_ref().map(|a| &a.3)) {
                Ok(ok) => ok,
                // A bind of a claim this very block's sweep voided, say: the fixture's objects are not
                // admission-checked. Drop the objects and retry the block once.
                Err(_) => world.try_step(daa, vec![], att.as_ref().map(|a| &a.3)).unwrap_or_else(|e| panic!("seed {seed} block {block}: {e:?}")),
            };
            // W-I3 + the delta and revert paths; the carriage import every eighth block.
            if block % 8 == 0 {
                checked(&parent, &child, &delta, &world.p);
            } else {
                assert_eq!(palw_bounded_immature_v2(&child, &world.p), child.bounded_immature(), "seed {seed} block {block}: W-I3");
                let applied = apply_delta_v2(&parent, &delta, &world.p).unwrap();
                assert_eq!(applied.capacity_weight_index(), child.capacity_weight_index());
                assert_eq!(applied.bounded_immature(), child.bounded_immature());
                let back = revert_delta_v2(&child, &delta, &world.p).unwrap();
                assert_eq!(back.capacity_weight_index(), parent.capacity_weight_index());
                assert_eq!(back.bounded_immature(), parent.bounded_immature());
            }
            // W-I1: each bond's staged sum is held under its cap in the total.
            let capped: u128 =
                bonds.iter().map(|(b, _)| child.capacity_weight_index().term(&bond_key(*b), child.bond(&bond_key(*b)).map(|r| r.collateral))).sum();
            let caps: u128 = bonds.iter().map(|(b, _)| palw_bond_weight_cap_v1(child.bond(&bond_key(*b)).unwrap().collateral)).sum();
            assert_eq!(child.bounded_immature(), capped, "seed {seed} block {block}: every claim is new-rule, so the total is the capped sum");
            assert!(capped <= caps, "W-I1");
            // W-I4 at acceptance. The fold reads the bond AFTER this block's sweep and objects (step 4
            // follows steps 2 and 3), so the parent's reading — admission's and the producer's — is
            // the stored value exactly when neither moved the bond's held reservation or collateral;
            // otherwise the fold's own ceiling re-check is the authority (the finding-17 rule).
            if let Some((id, bond, predicted, _)) = &att
                && let Some(claim) = child.claim(id)
                && claim.accepted_daa == daa
            {
                let (before, after) = (parent.capacity_weight_index().bond(bond), child.capacity_weight_index().bond(bond));
                let collateral = child.bond(bond).unwrap().collateral;
                let budget = palw_bond_weight_budget_sompi_v1(collateral);
                if after.reserved_w == before.reserved_w + claim.reserved && collateral == parent.bond(bond).unwrap().collateral {
                    assert_eq!(claim.reserved, *predicted, "seed {seed} block {block}: SR-7, the parent's capped reading");
                }
                let unslashed = collateral == parent.bond(bond).unwrap().collateral;
                assert!(claim.reserved <= budget || !unslashed, "W-I4: one claim never reserves past R_budget");
                assert!(
                    claim.reserved == 0 || !unslashed || after.reserved_w <= budget,
                    "seed {seed} block {block}: W-I4 — a claim that took a reservation left the bond within R_budget"
                );
            }
            // W-I2: live weight falls only where a claim voided, was redrawn, or a bond was slashed.
            let slashed = bonds.iter().any(|(b, _)| {
                child.bond(&bond_key(*b)).unwrap().collateral < parent.bond(&bond_key(*b)).unwrap().collateral
            });
            let voided_or_redrawn = parent.claims_iter().any(|(id, before)| {
                let after = child.claim(id);
                (!before.phase.is_terminal() && after.is_some_and(|a| matches!(a.phase, PalwClaimPhaseV2::Voided { .. })))
                    || (matches!(before.phase, PalwClaimPhaseV2::PanelBound { .. })
                        && after.is_some_and(|a| matches!(a.phase, PalwClaimPhaseV2::Provisional)))
            });
            slashes += usize::from(slashed);
            if live(&child) < live(&parent) {
                falls += 1;
                assert!(slashed || voided_or_redrawn, "seed {seed} block {block}: W-I2 — live weight fell with no void, redraw or slash");
            }
            world.s = child;
            world.blue += 1;
        }
    }
    println!("W-T3: 10 seeds × 120 blocks, {slashes} blocks slashed a producer, {falls} blocks lowered live weight (each by a void, redraw or slash)");
    assert!(slashes > 0, "the property run exercised the slash recap");
}

// ---- W-T6: the private-fork weight burst (§8 A1), fold level -------------------------------------

/// **W-T6 (A1) at the fold**: an attacker holding `m` 13,000 MSK bonds forks off the public chain and
/// piles `K` 2M junk claims (the heaviest class) on its private branch, self-licensing them through its
/// own seat where no operator anchor stops it. The honest public branch carries two licensed floor
/// claims on its own 13k bond.
///
/// * Fence dormant (today): ONE junk claim outweighs the public branch by ~555,609 FCW and the deep
///   reorg is allowed — the hole.
/// * Fence armed: `live_A − live_public ≤ Σ W_cap(C_A)` = `2m` FCW, the same at K = 10, 100 and 1,000.
/// * Operator anchor armed (the stopgap: a non-operator's claims never bind): the junk stays Created,
///   weighs 0, and with strict-win the incumbent is kept (a tie is not a win).
///
/// The GHOSTDAG half (heartbeat transparency, blue work) is independent of staged weight — it touches
/// no `bounded_immature` — and is the processor's (`hb_fork_choice_probe`), not re-run here.
#[test]
fn w_t6_a_private_weight_burst_is_bounded_by_the_attackers_cap_whatever_its_claim_count() {
    let honest = 0x21;
    let attackers = [0x51u64, 0x52, 0x53];
    for fence in [None, Some(0)] {
        for m in [1usize, 3] {
            for self_licence in [false, true] {
                for k in [10u64, 100, 1_000] {
                    let mut bonds = vec![(honest, 13_000 * MSK)];
                    bonds.extend(attackers.iter().map(|a| (*a, 13_000 * MSK)));
                    let mut world = World::new(cp(fence), &bonds, false);
                    let fork = world.s.clone();
                    let fork_blue = world.blue;
                    // Public: two honest floor claims, licensed.
                    let h1 = world.claim(101, FLOOR, honest);
                    let h2 = world.claim(101, FLOOR, honest);
                    world.step(102, vec![bind(h1), bind(h2)], None).unwrap();
                    world.step(103, vec![licence(h1), licence(h2)], None).unwrap();
                    let public = world.s.candidate_order(h64(0x1));
                    // Private: K junk 2M claims over the attacker's m bonds.
                    world.s = fork;
                    world.blue = fork_blue;
                    let mut junk = Vec::new();
                    for i in 0..k {
                        junk.push(world.claim(101, M2, attackers[(i as usize) % m]));
                    }
                    if self_licence {
                        world.step(102, junk.iter().map(|id| bind(*id)).collect(), None).unwrap();
                        world.step(103, junk.iter().map(|id| licence(*id)).collect(), None).unwrap();
                    } else {
                        world.step(103, vec![], None).unwrap();
                    }
                    let private = world.s.candidate_order(h64(0xFFFF));
                    let gain = private.live_total.saturating_sub(public.live_total);
                    match fence {
                        None if self_licence => {
                            assert!(gain > fcw(555_000), "dormant: the junk outweighs the public branch by 555k FCW per claim");
                            assert_eq!(decide_deep_reorg_v2(&public, &private), PalwDeepReorgV2::Allow, "today: the reorg is allowed");
                        }
                        None => assert_eq!(gain, u128::from(k) * raw_of(M2) - 2 * raw_of(FLOOR), "dormant: even Created junk weighs raw"),
                        Some(_) => {
                            let cap = fcw(2 * m as u128);
                            assert!(
                                private.live_total <= cap && gain <= cap,
                                "fence, m = {m}, K = {k}: live_A − live_public ≤ Σ W_cap(C_A) = {cap} (gain {gain})"
                            );
                            if self_licence {
                                assert_eq!(private.live_total, cap, "self-licensed junk saturates the attacker's cap exactly, whatever K");
                            } else {
                                assert_eq!(private.live_total, 0, "Created junk (the operator anchor's case) weighs nothing");
                                assert_eq!(
                                    palw_deep_reorg_strict_economic_v1(&public, &private),
                                    PalwDeepReorgV2::Refuse,
                                    "strict-win keeps the incumbent"
                                );
                            }
                        }
                    }
                    if fence.is_some() && k == 1_000 {
                        println!(
                            "W-T6 m={m} self_licence={self_licence}: live_A − live_public = {:.2} FCW at K = 10, 100 and 1,000 (cap {} FCW)",
                            gain as f64 / PALW_CAPACITY_FCW_V1 as f64 - 0.0,
                            2 * m
                        );
                    }
                }
            }
        }
    }
}

// ---- W-T7: split neutrality ---------------------------------------------------------------------

/// **W-T7: one 1M bond vs 76 × 13k** (988,000 MSK): the cap and the budget are linear in collateral,
/// so the split gains at most one flooring unit per piece — here it LOSES one (153 vs 152 FCW), and
/// the fold agrees once every bond is saturated with a licensed 8k claim (229.86 FCW each).
#[test]
fn w_t7_splitting_a_bond_buys_no_weight() {
    assert_eq!(palw_bond_weight_cap_v1(1_000_000 * MSK), fcw(153));
    assert_eq!(76 * palw_bond_weight_cap_v1(13_000 * MSK), fcw(152));
    assert_eq!(palw_bond_weight_budget_sompi_v1(1_000_000 * MSK), 153 * W_FLOOR);
    assert_eq!(76 * palw_bond_weight_budget_sompi_v1(13_000 * MSK), 152 * W_FLOOR);
    // Every split of C into pieces gains at most one FCW per piece over the whole.
    for pieces in [2u64, 7, 13, 76, 153] {
        let c = 1_000_000 * MSK;
        let split: u128 = (0..pieces).map(|i| palw_bond_weight_cap_v1(c / pieces + u64::from(i < c % pieces))).sum();
        assert!(split <= palw_bond_weight_cap_v1(c) + fcw(u128::from(pieces)) && split <= palw_bond_weight_cap_v1(c));
    }
    let saturate = |bonds: &[(u64, u64)]| {
        let mut world = World::new(cp(Some(0)), bonds, false);
        let ids: Vec<Hash64> = bonds.iter().map(|(b, _)| world.claim(101, K8, *b)).collect();
        world.step(102, ids.iter().map(|id| bind(*id)).collect(), None).unwrap();
        world.step(103, ids.iter().map(|id| licence(*id)).collect(), None).unwrap();
        world.s.bounded_immature()
    };
    let whole = saturate(&[(0x21, 1_000_000 * MSK)]);
    let split = saturate(&(0..76u64).map(|i| (100 + i, 13_000 * MSK)).collect::<Vec<_>>());
    assert_eq!((whole, split), (fcw(153), fcw(152)), "the fold: 153 FCW whole, 152 split");
}

// ---- W-T8: a slash lowers the cap ----------------------------------------------------------------

/// **W-T8: a slash lowers the cap, a refund restores it, and neither underflows** — through the fold's
/// one bond writer. A 13,000 MSK bond holding a licensed 8k claim (229.86 FCW, capped at 2): a
/// one-sompi slash floors it to one FCW of cap; a slash of everything to zero; the refund of the whole
/// back to two. `bounded_immature` equals its re-derivation after each move.
#[test]
fn w_t8_a_slash_lowers_the_cap_without_underflow() {
    let p = cp(Some(0));
    let mut world = World::new(p.clone(), &[(0x21, 13_000 * MSK), (0x31, 26_000 * MSK)], true);
    let a = world.claim(101, K8, 0x21);
    let b = world.claim(101, FLOOR, 0x31);
    world.step(102, vec![bind(a), bind(b)], None).unwrap();
    world.step(103, vec![licence(a), licence(b)], None).unwrap();
    assert_eq!(world.s.bounded_immature(), fcw(2) + fcw(1));
    let extras = PalwTransitionExtrasV1::default();
    let mut builder = TransitionBuilder::new(&world.s, &p, false, false, false, false, &extras);
    let bond = bond_key(0x21);
    let expect = |b: &TransitionBuilder<'_>, fcws: u128, why: &str| {
        assert_eq!(b.state.bounded_immature, fcw(fcws) + fcw(1), "{why}");
        assert_eq!(palw_bounded_immature_v2(&b.state, &p), b.state.bounded_immature, "{why}: W-I3");
    };
    assert_eq!(builder.slash_bond(bond, 1).unwrap(), 1);
    expect(&builder, 1, "one sompi below 13,000 MSK floors the cap to one FCW");
    let rest = u128::from(builder.state.bonds[&bond].collateral);
    builder.slash_bond(bond, rest + 5).unwrap();
    assert_eq!(builder.state.bonds[&bond].collateral, 0);
    expect(&builder, 0, "an emptied bond puts no provisional weight into fork choice");
    builder.slash_bond(bond, 1).unwrap();
    expect(&builder, 0, "a slash of nothing moves nothing (W-I6)");
    let mut restored = builder.state.bonds[&bond].clone();
    restored.collateral = 13_000 * MSK;
    builder.write_bond(bond, Some(restored));
    expect(&builder, 2, "a refund (collateral back) restores the cap");
    // The 26k bond's floor claim was never touched.
    assert_eq!(builder.state.capacity_weight_index.term(&bond_key(0x31), Some(26_000 * MSK)), fcw(1));
}

// ---- W-I2 at Final: maturing never lowers the total ----------------------------------------------

/// **W-I2 at Final**: a new-rule claim's Final moves at most its staged weight out of the bond's capped
/// term and its whole contribution into `safe_weight`, so `live_total` does not fall — here with the
/// bond saturated (the Final frees cap another licensed claim immediately fills) and unsaturated.
#[test]
fn w_i2_a_final_never_lowers_live_weight() {
    let mut world = World::new(cp(Some(0)), &[(0x21, 13_000 * MSK)], true);
    let ids: Vec<Hash64> = (0..3).map(|_| world.claim(101, FLOOR, 0x21)).collect();
    world.step(102, ids.iter().map(|id| bind(*id)).collect(), None).unwrap();
    world.step(103, vec![licence(ids[0]), licence(ids[1])], None).unwrap();
    world.step(104, vec![licence(ids[2])], None).unwrap();
    assert_eq!(world.s.bounded_immature(), fcw(2), "three licensed floor claims on 13k: capped at two");
    let before = live(&world.s);
    world.step(124, vec![], None).unwrap(); // ids[0] and ids[1] Final (103 + 20); ids[2] still licensed
    assert!(ids[..2].iter().all(|id| matches!(world.s.claim(id).unwrap().phase, PalwClaimPhaseV2::Final { .. })));
    assert_eq!(world.s.bounded_immature(), fcw(1), "the third claim now fits the cap alone");
    assert!(live(&world.s) >= before, "W-I2: Final never lowers the total ({} → {})", before, live(&world.s));
    assert_eq!(world.s.safe_weight(), 2 * u128::from(class_row(FLOOR).2), "the full pwu of both into safe");
}
