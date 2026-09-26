//! **ADR-0160 lane escrow on testnet-12's own fold** — E-T1 and E-T3 … E-T8: the escrow slot holds
//! `m_c` past F-E (`Params::palw_capacity_escrow_at_licence`), the licence releases exactly `m_c`, an
//! unconvicted void keeps the commitment for `h_obl`, the S0′ charge is the stage commitment, and
//! below the fence nothing moves.
//!
//! Every block goes through `apply_palw_transition_v7` with the extras the processor resolves; on a
//! [`Tape`] each block's delta re-applies and reverts and its carriage reloads under its root (the
//! ledger re-derived from the claims' commitments — X-I7), and the runs are restarted at every tip,
//! reverted to their base and replayed by IBD.
//!
//! **The credit.** F-L (lane liab) carries the ramp's `(ρ, q_credit)`; this branch has no F-L, so the
//! fixtures set the escrow lane's stand-in schedule on the bundle
//! (`PalwStateParamsV2::with_capacity_escrow_credits_v1`) — the one accessor `rcore/cap-int` rewires.
//! Without a step the term is `E` (see `f_e_alone_without_a_credit_changes_only_the_hold`).
//!
//! **Measured here, not in the pipeline harness.** E-T3's `N_instant` is counted by the fold's own
//! ceiling (`apply_attempt`'s finding-17 check: the admission ceiling on the state the claim joins),
//! one floor attempt a block, four blocks a DAA so that 2,030 claims fit inside one bind window; the
//! producer's headroom (`palw_producer_facts_v4`) is checked to predict the same refusal (SR-7).
//!
//! Run: cargo test -p kaspa-consensus-core --test palw_capacity_escrow_at_licence -- --nocapture

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_escrow_funding_v2::{PalwEscrowCreditStepV1, palw_claim_obligation_release_at_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::{PalwStateV2Error, PalwVoidReasonV2};

/// 1 MSK in sompi.
const MSK: u64 = 100_000_000;
/// testnet-12's escrow `E` at the full subsidy: 720‰ of 4,445.62014 MSK.
const E: u128 = 320_084_650_080;
/// The credited attribution the lifecycle fixtures use: past every ramp step's need (143‰ at L = 3E).
const Q: u16 = 150;

/// testnet-12 with F-E at `at` and the stand-in credit schedule `(from_daa, ρ, q‰)`.
fn armed(at: u64, credits: &[(u64, u32, u16)]) -> Params {
    let mut p = t12();
    p.palw_capacity_escrow_at_licence = Some(ForkActivation::new(at));
    p.sync_palw_capacity_escrow();
    with_credits(&mut p, credits);
    p.validate_palw_v2().expect("F-E on testnet-12 validates");
    p
}

fn with_credits(p: &mut Params, credits: &[(u64, u32, u16)]) {
    let PalwConsensusMode::ConsensusV2(b) = &mut p.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    b.state = b.state.clone().with_capacity_escrow_credits_v1(
        credits
            .iter()
            .map(|&(from_daa, rho, q_credit_permille)| PalwEscrowCreditStepV1 { from_daa, rho, q_credit_permille })
            .collect(),
    );
}

/// The escrow slot the ledger holds for `claim` (`m_c` past F-E, option A's `E` below).
fn slot(sp: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> u128 {
    sp.claim_escrow_term_v2(claim.accepted_daa, claim.escrowed_reward, &claim.class_id)
}

fn tape_on(p: Params) -> Tape {
    let mut c = Chain::new(p);
    c.attribution = true;
    Tape::new(c)
}

fn restart_everywhere(t: &Tape) {
    restart_every(t, 1);
}

/// Restart at every `stride`-th tip and at the tip (a long tape's quadratic replay kept short).
fn restart_every(t: &Tape, stride: usize) {
    for j in (0..=t.len()).step_by(stride.max(1)).chain([t.len()]) {
        t.restart_at(j);
    }
}

/// The V1 door's receipts: `Valid` from `valid_seats`, `Unavailable` from `unavailable_seats`.
fn v1(
    id: Hash64,
    seats: &[(PalwBondKeyV2, Hash64)],
    valid_seats: &[usize],
    unavailable_seats: &[usize],
    signed: u64,
) -> PalwConsensusObjectV2 {
    let mut receipts: Vec<_> = valid_seats.iter().map(|i| valid(id, seats[*i].0, signed)).collect();
    receipts.extend(unavailable_seats.iter().map(|i| unavailable(id, seats[*i].0, signed)));
    PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }
}

fn bond_of(s: &PalwChainStateV2, n: u64) -> (u64, u64) {
    let b = s.bond(&bond_key(n)).expect("the bond");
    (b.collateral, b.slashed)
}

// =============================================================================================
// E-T1 (and X-I1, X-I2): the commitment table by phase and door, with its fence-off twin.
// =============================================================================================

/// What one door did to one claim: `(label, the slot, the ledger's fall at the licence)`.
type DoorRow = (&'static str, u128, u128);

/// **The staged lifecycle on `p`** (rcore_m5's T01 set, with the doors that matter to the slot): A the
/// V1 quorum door with five `Valid`s (released), B the coverage door (released), C three `Valid`s and
/// two `Unavailable`s (held), D three `Valid`s and two missing (held), E redrawn then five `Valid`s
/// (held: SR-1 never releases after a redraw), F S2 (held) then its V2-door upgrade (released in that
/// block). Returns each licence's fall, checks Final releases the rest and every row the Final writes
/// carries the whole `E` unmatured, and restarts / reverts / replays the tape.
fn lifecycle(p: Params) -> Vec<DoorRow> {
    let mut t = tape_on(p);
    let (producer, _, _) = floor_producer(&t.c.p);
    let seats = t.c.floor_seats();
    let base = t.c.s.reserved_exposure(&producer);
    let mut rows: Vec<DoorRow> = Vec::new();
    let mut licence = |t: &mut Tape, id: Hash64, object: PalwConsensusObjectV2, what: &'static str| {
        let claim = t.c.s.claim(&id).unwrap().clone();
        let before = t.c.s.reserved_exposure(&producer);
        t.step(vec![object]);
        let licensed = t.c.s.claim(&id).unwrap().clone();
        assert!(matches!(licensed.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "{what}: licensed");
        assert_eq!(t.c.s.vesting_row(&id), None, "{what}: X-I2 — no row, nothing to mint, before Final");
        rows.push((what, slot(&t.c.sp, &claim), before - t.c.s.reserved_exposure(&producer)));
    };
    let e_id = t.attempt(None, 0x0E0);
    let accepted = t.c.s.claim(&e_id).unwrap().clone();
    assert_eq!(t.c.s.reserved_exposure(&producer) - base, accepted.reserved + slot(&t.c.sp, &accepted), "Provisional: w + the slot");
    let e_bound = t.bind(e_id);
    assert_eq!(
        palw_claim_commitment_v1(&t.c.sp, t.c.s.claim(&e_id).unwrap(), t.c.daa),
        Some(accepted.reserved + slot(&t.c.sp, &accepted)),
        "PanelBound: unchanged"
    );
    t.at(e_bound + t.c.sp.window_receipt() + 1, vec![]);
    assert!(matches!(t.c.s.claim(&e_id).unwrap().phase, PalwClaimPhaseV2::Provisional), "RT#1 redraws E");
    let e_rebound = t.bind(e_id);
    licence(&mut t, e_id, v1(e_id, &seats, &[0, 1, 2, 3, 4], &[], e_rebound), "E redrawn, five Valid");
    let a = t.attempt(None, 0x0A0);
    let bound = t.bind(a);
    licence(&mut t, a, v1(a, &seats, &[0, 1, 2, 3, 4], &[], bound), "A quorum, five Valid");
    let b = t.attempt(None, 0x0B0);
    let bound = t.bind(b);
    let anchor = t.c.anchor(&b);
    licence(
        &mut t,
        b,
        PalwConsensusObjectV2::ReceiptLicensedV2 { claim: b, receipts: covered(b, anchor, &seats, &[0, 1, 2, 3, 4], bound) },
        "B coverage, every seat",
    );
    let cc = t.attempt(None, 0x0C0);
    let bound = t.bind(cc);
    licence(&mut t, cc, v1(cc, &seats, &[0, 1, 2], &[3, 4], bound), "C three Valid, two Unavailable");
    let d = t.attempt(None, 0x0D0);
    let bound = t.bind(d);
    licence(&mut t, d, v1(d, &seats, &[0, 1, 2], &[], bound), "D three Valid, two missing");
    let f = t.attempt(None, 0x0F0);
    let f_bound = t.bind(f);
    let anchor = t.c.anchor(&f);
    let assignment = palw_segment_assignment_v2(anchor, f, 5);
    let (full, partial) = (assignment.full_seat as usize, (assignment.full_seat as usize + 1) % 5);
    licence(
        &mut t,
        f,
        PalwConsensusObjectV2::OptimisticLicensed { claim: f, receipts: covered(f, anchor, &seats, &[full, partial], f_bound) },
        "F S2",
    );
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = t.c.s.claim(&f).unwrap().phase else { unreachable!() };
    let rest: Vec<_> = (0..5).filter(|i| *i != full && *i != partial).map(|i| valid(f, seats[i].0, f_bound)).collect();
    let before = t.c.s.reserved_exposure(&producer);
    t.at(licensed_daa + 60, vec![PalwConsensusObjectV2::ReceiptLicensed { claim: f, receipts: rest }]);
    rows.push((
        "F upgraded through the V2 door",
        slot(&t.c.sp, t.c.s.claim(&f).unwrap()),
        before - t.c.s.reserved_exposure(&producer),
    ));
    // Final releases the rest: the ledger ends where it began, and every Final writes a row of the whole
    // E, unmatured (X-I2: the reward is still E, minted only at the row's maturity).
    let ids = [a, b, cc, d, e_id, f];
    let last = ids
        .iter()
        .map(|id| match t.c.s.claim(id).unwrap().phase {
            PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => licensed_daa + t.c.sp.window_challenge_at(licensed_daa),
            ref other => panic!("{id} licensed: {other:?}"),
        })
        .max()
        .unwrap();
    t.at(last + 1, vec![]);
    for id in ids {
        assert!(matches!(t.c.s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "{id} Final");
        let row = t.c.s.vesting_row(&id).expect("a Final writes its vesting row");
        assert_eq!(u128::from(row.escrowed_reward), E, "X-I2: the row carries the whole withheld reward");
        assert_eq!(row.matured_at, None, "X-I2: nothing matures at Final");
    }
    assert_eq!(t.c.s.reserved_exposure(&producer), base, "Final releases the rest; the ledger ends where it began");
    restart_everywhere(&t);
    t.revert_to_base_and_reapply();
    t.ibd_from(Chain::new(t.c.p.clone()).s);
    rows
}

/// **E-T1: the commitment table by phase and door.** Past F-E at ρ = 10 the slot is `⌈E/10⌉` and the
/// licence releases exactly it where SR-1 says the release is due (A, B, F's upgrade) and nothing
/// otherwise (C, D, E, F's S2); the fence-off twin is the same table with `E`. X-I1: no `E` in any
/// new-rule commitment.
#[test]
fn e_t1_the_commitment_table_by_phase_and_door() {
    let m = E.div_ceil(10);
    let armed_rows = lifecycle(armed(1_000, &[(0, 10, Q)]));
    let twin_rows = lifecycle(t12());
    println!("{:<34} {:>16} {:>16} | {:>16} {:>16}", "door", "slot (F-E)", "released", "slot (twin)", "released");
    for ((what, slot_a, fall_a), (_, slot_t, fall_t)) in armed_rows.iter().zip(&twin_rows) {
        println!("{what:<34} {:>16.4} {:>16.4} | {:>16.4} {:>16.4}", msk(*slot_a), msk(*fall_a), msk(*slot_t), msk(*fall_t));
        assert_eq!(*slot_a, m, "{what}: past F-E the slot is ⌈E/ρ⌉ (X-I1: no E on the bond)");
        assert_eq!(*slot_t, E, "{what}: below it option A's E");
        let released = matches!(*what, "A quorum, five Valid" | "B coverage, every seat" | "F upgraded through the V2 door");
        assert_eq!(*fall_a, if released { m } else { 0 }, "{what}: E-3 — m_c leaves at a counted, fully served licence only");
        assert_eq!(*fall_t, if released { E } else { 0 }, "{what}: the twin releases E on the same doors");
    }
}

// =============================================================================================
// E-T3: N_instant at 13,000 MSK by ramp step, counted by the fold's own ceiling.
// =============================================================================================

/// **How many floor claims a bond of `collateral` holds at once**, counted by the fold: one attempt a
/// block by planted bond 90 (no capable classes, so never seated: a pure producer), four blocks a DAA,
/// until `apply_attempt`'s ceiling skips one. Returns `(N, the claims' commitments summed, the next
/// claim's commitment, w)`. Checks that the refused claim is exactly the one that would pass the 500‰
/// ceiling, that the producer's facts predict the same refusal (SR-7), and that the state reloads
/// (X-I7).
fn n_instant(p: Params, collateral: u64, cap: usize) -> (usize, u128, u128, u128) {
    let mut c = Chain::new(p);
    c.attribution = true;
    c.step(&[bond_obj(90, collateral)]);
    let producer = bond_key(90);
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let base_daa = c.daa;
    let mut held = 0u128;
    let mut w = 0u128;
    for k in 0..(cap as u64 + 1) {
        let daa = base_daa + 1 + k / 4;
        c.daa = daa - 1;
        let seed = 0x3_0000 + k;
        let (env, key, id) = floor_attempt_of(&c, 90, seed);
        let mut e = c.extras_at(daa);
        e.own_job_anchor = floor_job_anchor(&c.p, producer, 0x10C0 + seed);
        let x = ctx(0xE3_0000 + k, daa, base_daa + 1 + k, T12_BLOCK_SUBSIDY_SOMPI);
        let (child, _, skips) = fold_with(&c.p, &c.sp, &c.s, &x, &[], PalwBlockWorkV3::Attempt(&env), key, &e)
            .unwrap_or_else(|err| panic!("block {k} at DAA {daa} folds: {err}"));
        if !skips.is_empty() {
            assert!(skips[0].1.contains("above its exposure ceiling"), "the ceiling refuses: {skips:?}");
            // SR-7: the producer's facts predict this refusal from the parent.
            let b = bundle(&c.p);
            let facts = kaspa_consensus_core::palw_producer_v2::palw_producer_facts_v4(
                &c.s,
                &c.sp,
                &b.admission,
                kaspa_consensus_core::BlockHash::default(),
                daa,
                floor,
                Some(&producer),
                None,
                c.p.palw_canonical_work_daa(),
                registry_fold(&c.p, daa).and_then(|fold| fold.genesis_works.get(&floor).map(|w| w.economic_ccu_per_claim)),
                true,
                kaspa_consensus_core::palw_state_v2::palw_claim_escrow_v1(&c.sp, T12_BLOCK_SUBSIDY_SOMPI, e.escrow_carve),
                None,
            )
            .expect("the floor has facts");
            let bf = facts.bond.expect("the bond's facts");
            let next = bf.claim_exposure;
            assert_eq!(bf.committed, held, "the producer reads the ledger the fold wrote");
            assert!(bf.committed + next > bf.exposure_ceiling, "the producer predicts the refusal");
            assert!(held + next > u128::from(collateral) / 2, "the refused claim is the one past 500‰");
            let loaded = PalwStateCarriageV2::from_state(&c.s)
                .into_state(&c.sp, Some(c.s.state_root()))
                .unwrap_or_else(|err| panic!("X-I7: the ledger re-derives at load: {err}"));
            assert_eq!(loaded, c.s);
            return (k as usize, held, next, w);
        }
        let claim = child.claim(&id).expect("accepted").clone();
        w = claim.reserved;
        held += palw_claim_commitment_v1(&c.sp, &claim, daa).unwrap();
        assert_eq!(child.reserved_exposure(&producer), held, "the ledger is the sum of the commitments");
        c.s = child;
    }
    panic!("no refusal within {cap} claims");
}

/// **E-T3: `N_instant` at 13,000 MSK is `⌊(6,500 − Σw)/m⌋` at every ramp step** — 2 today (and with
/// F-E but no credit), then 20 / 50 / 101 / 202 / 1,964 at ρ = 10 / 25 / 50 / 100 / 1,000 on this
/// branch, where `w` (0.1075 MSK a floor claim) is not yet capped by F-W. On `rcore/cap-int` with J-1's
/// per-bond cap (`reserved ≤ R_budget = 0.215` MSK at 13k) the last two are ADR-0160 §5.2's 203 and
/// 2,030. Also at 100,000 MSK and ρ = 100.
#[test]
fn e_t3_n_instant_at_13k_by_ramp_step() {
    let bond_13k = 13_000 * MSK;
    let mut table = Vec::new();
    for (label, p, want) in [
        ("today (F-E off)", t12(), 2usize),
        ("F-E, no credit", armed(1_000, &[]), 2),
        ("rho 10", armed(1_000, &[(0, 10, Q)]), 20),
        ("rho 25", armed(1_000, &[(0, 25, Q)]), 50),
        ("rho 50", armed(1_000, &[(0, 50, Q)]), 101),
        ("rho 100", armed(1_000, &[(0, 100, Q)]), 202),
        ("rho 1000", armed(1_000, &[(0, 1_000, Q)]), 1_964),
    ] {
        let (n, held, next, w) = n_instant(p, bond_13k, 2_100);
        let m = next - w;
        let formula = ((u128::from(bond_13k) / 2) / (m + w)) as usize;
        println!(
            "13k {label:<16}: N = {n:>5}  (w {:.4} + m {:.4} = {:.4} MSK a claim; held {:.2} of 6,500; formula {formula})",
            msk(w),
            msk(m),
            msk(m + w),
            msk(held)
        );
        assert_eq!(n, want, "{label}");
        assert_eq!(n, formula, "{label}: the gate is the formula");
        table.push((label, n));
    }
    let (n, _, next, w) = n_instant(armed(1_000, &[(0, 100, Q)]), 100_000 * MSK, 2_100);
    println!("100k rho 100: N = {n} (m {:.4} MSK)", msk(next - w));
    assert_eq!(n, ((50_000 * u128::from(MSK)) / next) as usize);
    assert!(n >= 1_550, "ADR-0160 §5.2: 1,562 at 100k with J-1's cap, {n} with w uncapped");
    println!("ramp at 13k: {table:?}");
}

/// **Per-bond floor throughput at licence latency `h_l`**: one 13,000 MSK producer (bond 96) attempts
/// in every block, `per_daa` blocks a DAA; each claim is bound `h_l − 1` DAA after acceptance and
/// licensed by its five floor seats in the next DAA (the fully served quorum: its slot leaves the bond),
/// and reaches Final `window_challenge` later. Returns the claims accepted in the first 80 DAA.
fn claims_per_80_daa(p: Params, collateral: u64, h_l: u64, per_daa: u64) -> usize {
    let mut c = Chain::new(p);
    c.attribution = true;
    c.step(&[bond_obj(96, collateral)]);
    let producer = bond_key(96);
    let seats = c.floor_seats();
    let start = c.daa;
    let mut blue = c.daa;
    let mut to_bind: std::collections::BTreeMap<u64, Vec<Hash64>> = Default::default();
    let mut to_license: std::collections::BTreeMap<u64, Vec<(Hash64, u64)>> = Default::default();
    let mut accepted = 0usize;
    let mut k = 0u64;
    for daa in (start + 1)..=(start + 80) {
        for j in 0..per_daa {
            let mut objects = Vec::new();
            if j == 0 {
                for id in to_bind.remove(&daa).unwrap_or_default() {
                    objects.push(PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xAC_0000 + daa), seats: seats_of(&seats) });
                    to_license.entry(daa + 1).or_default().push((id, daa));
                }
                for (id, bound) in to_license.remove(&daa).unwrap_or_default() {
                    objects.push(v1(id, &seats, &[0, 1, 2, 3, 4], &[], bound));
                }
            }
            c.daa = daa - 1;
            k += 1;
            blue += 1;
            let seed = 0x96_0000 + k;
            let (env, key, id) = floor_attempt_of(&c, 96, seed);
            let mut e = c.extras_at(daa);
            e.own_job_anchor = floor_job_anchor(&c.p, producer, 0x10C0 + seed);
            let x = ctx(0xE9_0000 + k, daa, blue, T12_BLOCK_SUBSIDY_SOMPI);
            let (child, _, skips) = fold_with(&c.p, &c.sp, &c.s, &x, &objects, PalwBlockWorkV3::Attempt(&env), key, &e)
                .unwrap_or_else(|err| panic!("DAA {daa} block {j}: {err}"));
            c.s = child;
            c.daa = daa;
            if skips.is_empty() {
                accepted += 1;
                to_bind.entry(daa + h_l - 1).or_default().push(id);
            }
        }
    }
    let loaded = PalwStateCarriageV2::from_state(&c.s).into_state(&c.sp, Some(c.s.state_root())).expect("X-I7: the ledger re-derives");
    assert_eq!(loaded, c.s);
    accepted
}

/// **What the lane alone changes in per-bond throughput** (ADR-0160 §5.3: floor, 13k, `H_L = 21` DAA,
/// per 80 DAA): the slot turns over at the licence, so a bond runs `N_instant` claims per licence
/// latency — ADR-0160 §5.3's Little's-law 7.6 / 76 / 773, measured here in whole DAA as 8 / 80 / 804
/// (a slot freed by a licence in a DAA's first block is taken in that DAA; the attempt lane opened to
/// twelve blocks a DAA so that it does not bind first). The licence rides the DAA after the bind.
#[test]
fn e_t3_per_bond_throughput_per_80_daa_at_licence_latency_21() {
    let bond_13k = 13_000 * MSK;
    let mut rows = Vec::new();
    for (label, p, lo, hi) in [
        ("today (F-E off)", t12(), 6usize, 9usize),
        ("rho 10", armed(1_000, &[(0, 10, Q)]), 70, 85),
        ("rho 100", armed(1_000, &[(0, 100, Q)]), 700, 850),
    ] {
        let n = claims_per_80_daa(p, bond_13k, 21, 12);
        println!("13k {label:<16}: {n} floor claims per 80 DAA (H_L 21, 12 blocks a DAA)");
        assert!((lo..=hi).contains(&n), "{label}: {n} outside {lo}..={hi}");
        rows.push((label, n));
    }
    assert!(rows[1].1 >= 9 * rows[0].1 && rows[2].1 >= 9 * rows[1].1, "each ρ step multiplies the flow: {rows:?}");
}

// =============================================================================================
// E-T4 / E-T5 / E-T8 (withdrawal principle T1–T8, S0′, parity): the obligation survives the void.
// =============================================================================================

/// **T1, T2 (before the panel), T7, T8 — a void after an omitted bind** by a 13,000 MSK bond at ρ = 10:
/// the void holds `w + m_c` on the bond for `window_receipt` past it (no charge, no debt), the weight
/// leaves at once, nothing is minted, the frontier and the licence counter do not move; past the hold
/// the capacity returns and the claim retires. Then every tip restarts, the tape reverts to its base
/// and replays. The fence-off twin frees the capacity at the void.
#[test]
fn e_t4_t1_t7_t8_a_void_after_an_omitted_bind_keeps_the_obligation_for_h_obl() {
    for fe in [true, false] {
        let p = if fe { armed(1_000, &[(0, 10, Q)]) } else { t12() };
        let mut t = tape_on(p);
        t.step(vec![bond_obj(91, 13_000 * MSK)]);
        let producer = bond_key(91);
        let (collateral, slashed) = bond_of(&t.c.s, 91);
        let (immature, safe, settled) = (t.c.s.bounded_immature(), t.c.s.safe_weight(), t.c.s.settled_attempt_finals());
        let id = t.attempt(Some(91), 0x71);
        let claim = t.c.s.claim(&id).unwrap().clone();
        let held = claim.reserved + slot(&t.c.sp, &claim);
        assert_eq!(t.c.s.reserved_exposure(&producer), held);
        assert!(t.c.s.bounded_immature() > immature, "the claim weighs while live");
        // The bind is omitted; the window closes.
        let voided_at = claim.accepted_daa + t.c.sp.window_bind() + 1;
        t.at(voided_at, vec![]);
        let voided = t.c.s.claim(&id).unwrap().clone();
        assert_eq!(voided.phase, PalwClaimPhaseV2::Voided { voided_daa: voided_at, reason: PalwVoidReasonV2::BindTimeout });
        assert_eq!(t.c.s.bounded_immature(), immature, "T7: the weight leaves at the void (W5)");
        assert_eq!((t.c.s.safe_weight(), t.c.s.settled_attempt_finals()), (safe, settled), "T7: no frontier, no licence trace");
        assert_eq!(bond_of(&t.c.s, 91), (collateral, slashed), "T1: an omitted bind is charged nothing (not a conviction)");
        assert_eq!(t.c.s.vesting_row(&id), None, "T1: the withheld E is never minted");
        if !fe {
            assert_eq!(t.c.s.reserved_exposure(&producer), 0, "twin: the void frees the capacity at once");
            continue;
        }
        let release_at = voided_at + t.c.sp.window_receipt();
        assert_eq!(palw_claim_obligation_release_at_v1(&voided, &t.c.sp), Some(release_at));
        assert_eq!(t.c.s.reserved_exposure(&producer), held, "E-4: the void keeps w + m_c on the bond");
        assert_eq!(t.c.s.deadline_of(&id), Some(release_at), "DL-1: the hold's row");
        t.at(voided_at + t.c.sp.window_receipt() / 2, vec![]);
        let mid = t.len();
        let loaded = t.restart_at(mid);
        assert_eq!(loaded.reserved_exposure(&producer), held, "X-I7: the held commitment re-derived at load");
        t.at(release_at, vec![]);
        assert_eq!(t.c.s.reserved_exposure(&producer), held, "X-I3: still held at voided + h_obl (inclusive)");
        t.at(release_at + 1, vec![]);
        assert_eq!(t.c.s.reserved_exposure(&producer), 0, "past the hold the capacity returns");
        assert_eq!(bond_of(&t.c.s, 91), (collateral, slashed), "a delay, never a confiscation");
        let retire = t.c.s.deadline_of(&id).expect("the retirement follows the hold");
        assert_eq!(retire, voided_at + t.c.sp.claim_retirement_daa());
        t.at(retire + 1, vec![]);
        assert!(t.c.s.claim(&id).is_none(), "the claim retires");
        restart_everywhere(&t);
        t.revert_to_base_and_reapply();
        t.ibd_from(t.base.clone());
    }
}

/// **T2 (after the panel), T3, E-T5: the producer sees its panel and withholds, twice** — RT#1 redraws
/// (the claim stays live, its commitment held), RT#2 voids it S0′: the charge is EXACTLY the stage
/// commitment `w + m_c` (not `E`, not the pool, never debt), the bond's other live claim is untouched,
/// the commitment stays held for `h_obl`, and the withheld `E` is burned (never minted). The void's
/// accounting is the same whichever producer's block carries the panel (T3: the claimant's own
/// attempt block or another bond's). Old rule: the charge is `w + E` and nothing is held.
#[test]
fn e_t4_t2_t3_e_t5_withholding_after_the_panel_costs_the_stage_commitment_and_is_held() {
    let mut rows = Vec::new();
    for (fe, carrier_is_claimant) in [(true, true), (true, false), (false, true)] {
        let p = if fe { armed(1_000, &[(0, 10, Q)]) } else { t12() };
        let mut t = tape_on(p);
        t.step(vec![bond_obj(92, 100_000 * MSK), bond_obj(93, 100_000 * MSK)]);
        let producer = bond_key(92);
        let seats = t.c.floor_seats();
        let id = t.attempt(Some(92), 0x92_02);
        let claim = t.c.s.claim(&id).unwrap().clone();
        let stage = claim.reserved + slot(&t.c.sp, &claim);
        // T3: the bind rides a block whose own work is the claimant's next attempt, or bond 93's.
        let daa = t.c.daa + 1;
        let (env, key, _) = floor_attempt_of(&t.c, if carrier_is_claimant { 92 } else { 93 }, 0x92_03);
        let anchor = floor_job_anchor(&t.c.p, bond_key(if carrier_is_claimant { 92 } else { 93 }), 0x10C0 + 0x92_03);
        let bind = PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xAC_0000 + t.c.daa), seats: seats_of(&seats) };
        t.block(daa, vec![bind], Some((env, key, anchor)), T12_BLOCK_SUBSIDY_SOMPI).expect("the bind block folds");
        let bound = t.c.daa;
        t.at(bound + t.c.sp.window_receipt() + 1, vec![]);
        assert!(matches!(t.c.s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Provisional), "RT#1 redraws");
        let rebound = t.bind(id);
        // The bond's other claim, live across the S0′ void (its own bind window outlasts it by a DAA).
        let other = t.attempt(Some(92), 0x92_01);
        let other_commitment = palw_claim_commitment_v1(&t.c.sp, t.c.s.claim(&other).unwrap(), t.c.daa).unwrap();
        let (collateral, slashed) = bond_of(&t.c.s, 92);
        let before = t.c.s.reserved_exposure(&producer);
        t.at(rebound + t.c.sp.window_receipt() + 1, vec![]);
        let voided = t.c.s.claim(&id).unwrap().clone();
        let PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::ReceiptTimeout } = voided.phase else {
            panic!("RT#2 voids S0′: {:?}", voided.phase)
        };
        let (collateral_after, slashed_after) = bond_of(&t.c.s, 92);
        let charged = u128::from(slashed_after - slashed);
        assert_eq!(charged, stage, "E-5 / X-I4: the S0′ charge is the stage commitment w + slot");
        assert_eq!(u128::from(collateral - collateral_after), stage, "and it leaves the collateral, never more");
        assert_eq!(
            palw_claim_commitment_v1(&t.c.sp, t.c.s.claim(&other).unwrap(), t.c.daa),
            Some(other_commitment),
            "never the pool: the bond's other claim is untouched"
        );
        assert_eq!(t.c.s.vesting_row(&id), None, "the withheld E is burned with the void");
        // Per claim (the bond also carries the T3 carrier block's own claim, whose own hold may end in
        // this block); the Tape's reload re-derives the bond's sum from these every block (X-I7).
        let commitment = |t: &Tape| palw_claim_commitment_v1(&t.c.sp, t.c.s.claim(&id).unwrap(), t.c.daa).unwrap();
        let _ = before;
        if fe {
            assert_eq!(commitment(&t), stage, "E-4: the S0′ void holds its commitment too");
            t.at(voided_daa + t.c.sp.window_receipt(), vec![]);
            assert_eq!(commitment(&t), stage, "X-I3: held through voided + h_obl");
            t.at(voided_daa + t.c.sp.window_receipt() + 1, vec![]);
            assert_eq!(commitment(&t), 0, "past h_obl the commitment returns");
        } else {
            assert_eq!(commitment(&t), 0, "old rule: released at the void");
        }
        rows.push((fe, carrier_is_claimant, msk(stage)));
        restart_every(&t, 7);
        t.revert_to_base_and_reapply();
    }
    println!("S0′ charge (fe, bind carried by the claimant, MSK): {rows:?}");
    assert_eq!(rows[0].2, rows[1].2, "T3: the same charge whoever carries the bind");
    assert!(rows[0].2 * 9.0 < rows[2].2, "F-E at ρ = 10 charges about a tenth of option A's w + E");
}

/// **T4 and T5 on a 13,000 MSK bond at ρ = 10: 100 void-after-omitted-bind cycles.** Every cycle's claim
/// holds `w + m_c` from acceptance to `voided + h_obl`, so the bond runs at most `N_instant = 20` such
/// claims in any `window_bind + h_obl` stretch: an attempt made while the holds fill the ceiling is
/// refused (T5, immediate reuse), and one made after a hold ends is admitted. Each cycle burns its own
/// `E` (nothing minted) and is charged nothing — so a re-draw is never free (it costs the reward) and
/// never faster than the capacity turns over. The cost per cycle does not fall with repetition.
#[test]
fn e_t4_t4_t5_a_hundred_redraw_cycles_are_bounded_by_the_held_capacity() {
    let p = armed(1_000, &[(0, 10, Q)]);
    let mut t = tape_on(p);
    t.step(vec![bond_obj(94, 13_000 * MSK)]);
    let producer = bond_key(94);
    let (collateral, slashed) = bond_of(&t.c.s, 94);
    let (mut accepted, mut refused, mut seed) = (Vec::new(), 0usize, 0x94_0000u64);
    let per_claim = {
        let id = t.attempt(Some(94), seed);
        accepted.push(id);
        let claim = t.c.s.claim(&id).unwrap().clone();
        claim.reserved + slot(&t.c.sp, &claim)
    };
    let n_instant = (u128::from(13_000 * MSK) / 2 / per_claim) as usize;
    assert_eq!(n_instant, 20);
    let life = t.c.sp.window_bind() + 1 + t.c.sp.window_receipt() + 1; // acceptance to the block that frees it
    while accepted.len() < 100 {
        seed += 1;
        let daa = t.c.daa + 1;
        let (env, key, id) = floor_attempt_of(&t.c, 94, seed);
        let anchor = floor_job_anchor(&t.c.p, producer, 0x10C0 + seed);
        let skips = t.block(daa, vec![], Some((env, key, anchor)), T12_BLOCK_SUBSIDY_SOMPI).expect("folds");
        if skips.is_empty() {
            accepted.push(id);
        } else {
            refused += 1;
        }
        // The ledger is the held + live claims, never more than the ceiling.
        let live_or_held =
            t.c.s
                .claims_iter()
                .filter(|(_, c)| c.bond == producer && palw_claim_commitment_v1(&t.c.sp, c, daa).unwrap_or(0) > 0)
                .count();
        assert!(live_or_held <= n_instant, "at most N_instant live or held at DAA {daa}");
        assert_eq!(t.c.s.reserved_exposure(&producer), live_or_held as u128 * per_claim, "X-I7 at DAA {daa}");
        // Jump over DAA where nothing can be admitted, to keep the run short: the next hold's end.
        if !skips.is_empty() {
            let next_free =
                t.c.s
                    .claims_iter()
                    .filter(|(_, c)| c.bond == producer)
                    .filter_map(|(_, c)| match c.phase {
                        PalwClaimPhaseV2::Voided { voided_daa, .. } => Some(voided_daa + t.c.sp.window_receipt() + 1),
                        _ => Some(c.accepted_daa + life),
                    })
                    .filter(|d| *d > daa)
                    .min()
                    .unwrap_or(daa + 1);
            if next_free - 1 > t.c.daa {
                t.at(next_free - 1, vec![]);
            }
        }
    }
    let span = t.c.daa - t.base_daa;
    let bound = n_instant * (span as usize / life as usize + 1);
    println!(
        "100 cycles on 13k at rho 10: {refused} refused (T5), {span} DAA, bound {bound} claims by capacity, charge 0, E burned 100 times"
    );
    assert!(refused > 0, "T5: an attempt while the holds fill the ceiling is refused");
    assert!(accepted.len() <= bound, "the redraw rate is bounded by N_instant per claim life");
    assert_eq!(bond_of(&t.c.s, 94), (collateral, slashed), "charged nothing: a delay, never a confiscation");
    for id in &accepted {
        assert_eq!(t.c.s.vesting_row(id), None, "every cycle's E is unminted");
    }
    // T5, the same execution again: its claim id is still on the chain (until retirement), refused.
    let first = accepted[0];
    let (env, key, id) = floor_attempt_of(&t.c, 94, 0x94_0000);
    assert_eq!(id, first);
    let daa = t.c.daa + 1;
    let anchor = floor_job_anchor(&t.c.p, producer, 0x10C0 + 0x94_0000);
    if t.c.s.claim(&first).is_some() {
        let refused = t.block(daa, vec![], Some((env, key, anchor)), T12_BLOCK_SUBSIDY_SOMPI);
        assert!(
            matches!(refused, Err(PalwStateV2Error::DuplicateClaim(_)) | Err(PalwStateV2Error::DuplicateWork { .. })),
            "T5: a voided claim's own attempt cannot be re-presented: {refused:?}"
        );
    }
    restart_every(&t, 25);
    t.revert_to_base_and_reapply();
}

/// **T4 at ×1,000 on a rich bond**: a thousand claims, four a DAA, none bound — every one voids, every
/// one holds `w + m_c` to its own `voided + h_obl`, the ledger re-derives at load at the peak and at the
/// end, each claim's hold and burn are the first one's (no discount for repetition), and past the last
/// hold the ledger is where it began.
#[test]
fn e_t4_t4_a_thousand_omitted_binds_hold_and_release_exactly() {
    let p = armed(1_000, &[(0, 10, Q)]);
    let mut c = Chain::new(p);
    c.attribution = true;
    let (producer, pubkey, operator) = floor_producer(&c.p);
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let base = c.s.reserved_exposure(&producer);
    let base_daa = c.daa;
    let mut ids = Vec::new();
    let mut per_claim = None;
    let mut blue = c.daa;
    let mut fold = |c: &mut Chain,
                    daa: u64,
                    work: Option<(&kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2, Hash64, Hash64)>,
                    k: u64| {
        blue += 1;
        let mut e = c.extras_at(daa);
        let (w, key) = match work {
            Some((env, key, anchor)) => {
                e.own_job_anchor = anchor;
                (PalwBlockWorkV3::Attempt(env), key)
            }
            None => (PalwBlockWorkV3::None, Hash64::default()),
        };
        let x = ctx(0xE4_0000 + k, daa, blue, T12_BLOCK_SUBSIDY_SOMPI);
        let (child, _, skips) = fold_with(&c.p, &c.sp, &c.s, &x, &[], w, key, &e).unwrap_or_else(|err| panic!("DAA {daa}: {err}"));
        assert!(skips.is_empty(), "DAA {daa}: {skips:?}");
        c.s = child;
        c.daa = daa;
    };
    for k in 0..1_000u64 {
        let daa = base_daa + 1 + k / 4;
        let pwu = c.floor_pwu(daa);
        let seed = 0xE4_0000 + k;
        let (env, key, id) = junk_attempt(floor, producer, pubkey.clone(), &operator, pwu, seed, 0x10C0 + seed);
        let anchor = floor_job_anchor(&c.p, producer, 0x10C0 + seed);
        fold(&mut c, daa, Some((&env, key, anchor)), k);
        let claim = c.s.claim(&id).expect("accepted").clone();
        let each = claim.reserved + slot(&c.sp, &claim);
        assert_eq!(*per_claim.get_or_insert(each), each, "claim {k}: the same commitment as the first");
        ids.push(id);
    }
    let each = per_claim.unwrap();
    let last_accept = c.daa;
    // Walk to past the last void: every claim voided, every one held.
    let all_voided = last_accept + c.sp.window_bind() + 1;
    for (i, daa) in ((last_accept + 1)..=all_voided).step_by(25).chain([all_voided]).enumerate() {
        fold(&mut c, daa, None, 10_000 + i as u64);
    }
    for id in &ids {
        assert!(
            matches!(c.s.claim(id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }),
            "every claim voided"
        );
        assert_eq!(c.s.vesting_row(id), None, "every E unminted");
    }
    assert_eq!(c.s.reserved_exposure(&producer) - base, 1_000 * each, "the peak: a thousand held commitments");
    let loaded = PalwStateCarriageV2::from_state(&c.s).into_state(&c.sp, Some(c.s.state_root())).expect("X-I7 at the peak");
    assert_eq!(loaded, c.s);
    // Past the last hold: the ledger is where it began.
    let released = all_voided + c.sp.window_receipt() + 1;
    for (i, daa) in ((all_voided + 1)..=released).step_by(25).chain([released]).enumerate() {
        if daa > c.daa {
            fold(&mut c, daa, None, 20_000 + i as u64);
        }
    }
    assert_eq!(c.s.reserved_exposure(&producer), base, "every hold released, exactly");
    let loaded = PalwStateCarriageV2::from_state(&c.s).into_state(&c.sp, Some(c.s.state_root())).expect("X-I7 at the end");
    assert_eq!(loaded, c.s);
    println!(
        "x1000: {} claims, {:.4} MSK held each to voided + {}, peak {:.2} MSK",
        ids.len(),
        msk(each),
        c.sp.window_receipt(),
        msk(1_000 * each)
    );
}

/// **T6: a private branch with a DAA lead releases nothing earlier.** Forked at the void, the public
/// branch advances one DAA a block and a private one thirteen: the hold is keyed on DAA, so each
/// branch holds `w + m_c` exactly through `voided + h_obl` and frees it in its first block past it —
/// the lead buys blocks, not capacity. Reorgs between the branches revert and re-apply to the recorded
/// states (E-T6's void half).
#[test]
fn e_t4_t6_a_dao_lead_on_a_private_branch_releases_nothing_early() {
    let p = armed(1_000, &[(0, 10, Q)]);
    let mut t = tape_on(p);
    t.step(vec![bond_obj(95, 13_000 * MSK)]);
    let producer = bond_key(95);
    let id = t.attempt(Some(95), 0x95);
    let claim = t.c.s.claim(&id).unwrap().clone();
    let held = claim.reserved + slot(&t.c.sp, &claim);
    let voided_at = claim.accepted_daa + t.c.sp.window_bind() + 1;
    t.at(voided_at, vec![]);
    let fork_tip = t.len();
    let release_at = voided_at + t.c.sp.window_receipt();
    let mut public = t.fork(fork_tip);
    let mut private = t.fork(fork_tip);
    for (branch, lead) in [(&mut public, 1u64), (&mut private, 13)] {
        while branch.c.daa <= release_at + 1 {
            let daa = branch.c.daa + lead;
            branch.at(daa, vec![]);
            let expect = if daa <= release_at { held } else { 0 };
            assert_eq!(branch.c.s.reserved_exposure(&producer), expect, "lead {lead}, DAA {daa}: the hold is the DAA's");
        }
    }
    println!("T6: public {} blocks, private {} blocks to free the same hold at DAA {}", public.len(), private.len(), release_at + 1);
    assert!(private.len() < public.len());
    let mut whole = t.fork(0);
    for b in t.blocks.iter().chain(public.blocks.iter()) {
        whole.block(b.daa, b.objects.clone(), b.attempt.clone(), b.subsidy).expect("the public branch refolds");
    }
    whole.reorg_to(fork_tip, &private);
    restart_everywhere(&private);
}

// =============================================================================================
// E-T6 / E-T7: reorgs across the licence, and the timing games.
// =============================================================================================

/// **E-T6: a reorg across a licence.** From one bound claim, branch L licenses it (the fully served
/// quorum: `m_c` leaves the bond) and branch S stays silent (the commitment stays); reorging L → S → L
/// reverts and re-applies to the recorded states, and each branch restarts at every tip. **E-T7 (A6):**
/// the credit that prices a claim is its own `accepted_daa`'s — a step appended later (ρ 10 → 25)
/// neither re-prices it nor what its licence releases; a claim accepted on a smaller subsidy holds its
/// own smaller `m_c`; nothing is minted before Final.
#[test]
fn e_t6_e_t7_a_reorg_across_a_licence_and_the_step_that_prices_a_claim_is_its_own() {
    let step_at = 1_040;
    let p = armed(1_000, &[(0, 10, Q), (step_at, 25, Q)]);
    let mut t = tape_on(p);
    let (producer, _, _) = floor_producer(&t.c.p);
    let seats = t.c.floor_seats();
    let x = t.attempt(None, 0xE6_01);
    let x_claim = t.c.s.claim(&x).unwrap().clone();
    assert!(x_claim.accepted_daa < step_at);
    assert_eq!(slot(&t.c.sp, &x_claim), E.div_ceil(10), "ρ 10 before the step");
    let half = t.attempt_with(None, 0xE6_02, T12_BLOCK_SUBSIDY_SOMPI / 2);
    let half_claim = t.c.s.claim(&half).unwrap().clone();
    assert!(half_claim.escrowed_reward < x_claim.escrowed_reward);
    assert_eq!(slot(&t.c.sp, &half_claim), u128::from(half_claim.escrowed_reward).div_ceil(10), "its own E, its own m_c");
    t.at(step_at, vec![]);
    let y = t.attempt(None, 0xE6_03);
    assert_eq!(slot(&t.c.sp, t.c.s.claim(&y).unwrap()), E.div_ceil(25), "ρ 25 from the step");
    assert_eq!(slot(&t.c.sp, t.c.s.claim(&x).unwrap()), E.div_ceil(10), "X keeps ρ 10 past the step");
    let bound = t.bind(x);
    let fork_tip = t.len();
    let mut licensed = t.fork(fork_tip);
    let before = licensed.c.s.reserved_exposure(&producer);
    licensed.step(vec![v1(x, &seats, &[0, 1, 2, 3, 4], &[], bound)]);
    assert_eq!(before - licensed.c.s.reserved_exposure(&producer), E.div_ceil(10), "X's licence releases X's own m_c");
    assert_eq!(licensed.c.s.vesting_row(&x), None, "no mint at the licence");
    let mut silent = t.fork(fork_tip);
    silent.step(vec![]);
    silent.step(vec![]);
    assert_eq!(silent.c.s.reserved_exposure(&producer), before, "the silent branch holds it");
    let mut whole = t.fork(0);
    for b in t.blocks.iter().chain(licensed.blocks.iter()) {
        whole.block(b.daa, b.objects.clone(), b.attempt.clone(), b.subsidy).expect("the whole licensed branch refolds");
    }
    whole.reorg_to(fork_tip, &silent);
    restart_everywhere(&licensed);
    restart_everywhere(&silent);
    whole.revert_to_base_and_reapply();
}

// =============================================================================================
// E-T8 and the crossing: below the fence nothing moves.
// =============================================================================================

/// **The twin fold (byte identity below the fence):** one lifecycle — claims, a bind, a licence, a
/// void, Final — folded on testnet-12 as shipped and on testnet-12 with F-E armed ABOVE the run's tip:
/// every block's state root is the shipped one's.
#[test]
fn e_t8_twin_fold_below_the_fence_is_byte_identical() {
    let run = |p: Params| -> Vec<Hash64> {
        let mut t = tape_on(p);
        let seats = t.c.floor_seats();
        let a = t.attempt(None, 0xE8_01);
        let b = t.attempt(None, 0xE8_02);
        let bound = t.bind(a);
        t.step(vec![v1(a, &seats, &[0, 1, 2, 3, 4], &[], bound)]);
        let voided_at = t.c.s.claim(&b).unwrap().accepted_daa + t.c.sp.window_bind() + 1;
        t.at(voided_at, vec![]);
        t.at(voided_at + t.c.sp.window_receipt() + 2, vec![]);
        t.blocks.iter().map(|b| b.state.state_root()).collect()
    };
    let shipped = run(t12());
    let dormant = run(armed(1_000_000, &[(0, 1_000, 1_000)]));
    assert_eq!(shipped.len(), dormant.len());
    assert_eq!(shipped, dormant, "F-E above the tip: every root is the shipped fold's, credit and all");
}

/// **E-T8 and the crossing at a low height:** F-E at DAA 1,010. A claim accepted below it keeps `w + E`
/// for its whole life — through the crossing, at its licence (which releases `E`) and at its void
/// (released at once, no hold) — while a claim accepted past it holds `w + m_c` and is held after its
/// void. The run restarts at every tip (the crossing included) and replays by IBD.
#[test]
fn e_t8_crossing_old_rule_claims_keep_w_plus_e() {
    let fe = 1_010;
    let p = armed(fe, &[(0, 10, Q)]);
    let mut t = tape_on(p);
    let (producer, _, _) = floor_producer(&t.c.p);
    let seats = t.c.floor_seats();
    let old_licensed = t.attempt(None, 0xE8_11);
    let old_voided = t.attempt(None, 0xE8_12);
    for id in [old_licensed, old_voided] {
        let claim = t.c.s.claim(&id).unwrap().clone();
        assert!(claim.accepted_daa < fe);
        assert_eq!(slot(&t.c.sp, &claim), E, "below F-E: option A's E");
    }
    t.at(fe, vec![]);
    let new_licensed = t.attempt(None, 0xE8_13);
    let new_voided = t.attempt(None, 0xE8_14);
    for id in [new_licensed, new_voided] {
        assert_eq!(slot(&t.c.sp, t.c.s.claim(&id).unwrap()), E.div_ceil(10), "past F-E: m_c");
    }
    for (id, want) in [(old_licensed, E), (new_licensed, E.div_ceil(10))] {
        let bound = t.bind(id);
        let before = t.c.s.reserved_exposure(&producer);
        t.step(vec![v1(id, &seats, &[0, 1, 2, 3, 4], &[], bound)]);
        assert_eq!(before - t.c.s.reserved_exposure(&producer), want, "the licence releases the claim's own slot");
    }
    // The licensed pair reach Final first, so the ledger's fall below is the voids' alone.
    let finals = [old_licensed, new_licensed]
        .iter()
        .map(|id| t.c.s.deadline_of(id).expect("a licensed claim owes its Final deadline"))
        .max()
        .unwrap();
    t.at(finals + 1, vec![]);
    let old_claim = t.c.s.claim(&old_voided).unwrap().clone();
    let new_claim = t.c.s.claim(&new_voided).unwrap().clone();
    let old_stage = old_claim.reserved + E;
    let new_stage = new_claim.reserved + E.div_ceil(10);
    let before = t.c.s.reserved_exposure(&producer);
    t.at(new_claim.accepted_daa + t.c.sp.window_bind() + 1, vec![]);
    for id in [old_voided, new_voided] {
        assert!(matches!(t.c.s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { .. }), "voided");
    }
    assert_eq!(palw_claim_obligation_release_at_v1(t.c.s.claim(&old_voided).unwrap(), &t.c.sp), None, "old rule: no E-4 hold");
    assert_eq!(before - t.c.s.reserved_exposure(&producer), old_stage, "the old-rule void released w + E at once; the new one holds");
    let released = t.c.s.claim(&new_voided).map(|c| palw_claim_obligation_release_at_v1(c, &t.c.sp)).flatten().expect("E-4");
    t.at(released + 1, vec![]);
    assert_eq!(before - t.c.s.reserved_exposure(&producer), old_stage + new_stage, "and the new one after h_obl");
    restart_everywhere(&t);
    t.revert_to_base_and_reapply();
    t.ibd_from(t.base.clone());
}

/// **F-E alone, without a credit (this branch before liab's F-L): the slot is `E`** — the commitments,
/// the licence release and the S0′ charge are option A's, and only the E-4 hold is new.
#[test]
fn f_e_alone_without_a_credit_changes_only_the_hold() {
    let mut t = tape_on(armed(1_000, &[]));
    let (producer, _, _) = floor_producer(&t.c.p);
    let id = t.attempt(None, 0xFE_01);
    let claim = t.c.s.claim(&id).unwrap().clone();
    assert_eq!(slot(&t.c.sp, &claim), E, "no credit: m_c = E");
    let before = t.c.s.reserved_exposure(&producer);
    t.at(claim.accepted_daa + t.c.sp.window_bind() + 1, vec![]);
    assert_eq!(t.c.s.reserved_exposure(&producer), before, "the hold is the one change");
}

// =============================================================================================
// X-I5: C7 keeps m = E; the window half of C7 is refused at acceptance where the list omits it.
// =============================================================================================

/// **X-I5 on a live 2M claim** (the 2M row opened by its flag-day measured row, `t12_2m_open`): at ρ =
/// 1,000 and certain attribution a floor claim holds `⌈E/1000⌉`, and the 2M claim holds `E` — C7 has no
/// conviction route. **The window half:** with C7's list emptied (the 2M row C7 by its window alone), a
/// 2M attempt past F-E is refused by the class gate (`ClassNotAdmitting`), and admitted with F-E off.
#[test]
fn x_i5_c7_keeps_the_whole_escrow_and_an_unlisted_long_window_class_is_refused() {
    let mut p = t12_2m_open();
    p.palw_capacity_escrow_at_licence = Some(ForkActivation::new(1_000));
    p.sync_palw_capacity_escrow();
    with_credits(&mut p, &[(0, 1_000, 1_000)]);
    p.validate_palw_v2().expect("the flag-day fixture with F-E validates");
    let (_, id2m) = model_classes(&p);
    let mut c = model_chain(p.clone(), id2m, 1);
    let id = model_claim(&mut c, id2m, 1, 0xC7);
    let claim = c.claim(&id);
    assert_eq!(slot(&c.sp, &claim), u128::from(claim.escrowed_reward), "X-I5: C7 holds E");
    assert_eq!(c.reserved(&bond_key(1)), claim.reserved + u128::from(claim.escrowed_reward));
    let floor = c.floor_claim(0xC8);
    assert_eq!(slot(&c.sp, &c.claim(&floor)), E.div_ceil(1_000), "the floor at ρ 1,000");
    // The window half: the list emptied on the fold's params (the 2M row stays C7 by its window).
    for fe in [true, false] {
        let mut q = p.clone();
        if !fe {
            q.palw_capacity_escrow_at_licence = None;
            q.sync_palw_capacity_escrow();
        }
        let PalwConsensusMode::ConsensusV2(b) = &mut q.palw_consensus_mode else { panic!("V2") };
        let rcore_from = b.state.rcore_plus_from_daa();
        let delay = b.state.withdrawal_delay_daa();
        b.state = b.state.clone().with_rcore_plus_mirrors(rcore_from, delay, Vec::new());
        let mut c = model_chain(q, id2m, 1);
        c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
        let pwu = class_pwu(&c.p, &c.s, id2m, c.daa + 1);
        let (env, key, _) = junk_attempt(id2m, bond_key(1), pubkey_of(1), &operator_pubkey_of(1), pwu, 0xC9, 0x5_00C9);
        let x = ctx(0xC9_0000, c.daa + 1, c.daa + 1, T12_BLOCK_SUBSIDY_SOMPI);
        let got = c.try_fold(&c.s.clone(), &x, &[], PalwBlockWorkV3::Attempt(&env), key);
        if fe {
            assert!(
                matches!(&got, Err(PalwStateV2Error::ClassNotAdmitting { class, state }) if *class == id2m && state.contains("X-I5")),
                "past F-E the unlisted long-window class is refused: {:?}",
                got.as_ref().err()
            );
        } else {
            assert!(got.is_ok_and(|(_, _, skips)| skips.is_empty()), "F-E off: the gate is today's");
        }
    }
}
