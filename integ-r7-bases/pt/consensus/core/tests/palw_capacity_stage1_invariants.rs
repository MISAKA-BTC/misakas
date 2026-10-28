//! **ADR-0160 stage 1 — the six invariants as executable property tests** (rcore/cap-s1; the user's
//! staged-safety plan of 2026-09-26, stage 1's gate (b)).
//!
//! Every test folds testnet-12's own V2 fold (`rcore_common::Chain`, driven by the stage-1 fixture's
//! [`Sim`] or by `rcore_common::Tape`): every block's delta re-applies and reverts, its carriage reloads
//! under its committed root. "Armed" is `palw_t12_release_v2_params()` (the DAA-750 release) with
//! `PALW_T12_CAPACITY_FENCES_V1` armed at [`H`] (F-W, F-E, F-L at ρ = 1 / q = 0, F-B, F-R); "shipped" is
//! the release itself. The families are deterministic: seeded [`Rng`] scripts (splitmix64) and small
//! exhaustive grids; a failure prints its seed.
//!
//! The operational definitions (the coordinator's brief, tightened where ADR-0160 v3 is stricter —
//! each tightening says so):
//!
//! * **HONEST-NO-LOSS** — with no fraud, every honest producer's and seat's collateral, paid reward and
//!   vesting are on the armed twin at least what they are on the shipped twin; no honest party is
//!   slashed, frozen or forfeited. *Tightened (v3 §5.8 "honest users never lose money", G6):* an honest
//!   claim the network fails to serve (a `BindTimeout`) costs no collateral on either twin, and the armed
//!   twin holds at most that claim's stage commitment, and only to `voided + h_obl` (E-4).
//! * **J1-CAP** — after every block, every reorg and every restart, each bond's provisional fork weight
//!   is at most `W_cap` of its collateral at that state, and `bounded_immature` is its re-derivation.
//! * **LIABILITY-SURVIVES** — a licensed claim's liability stays collectible until its challenge window
//!   closes: the claim stays a conviction target (AG-5), its bond's exit stays shut (B-3), and a proven
//!   verdict at any point of that window forfeits the whole bond (AG-2) — across a sibling's void, a
//!   retirement request, a reorg and a restart.
//! * **NO-FREE-VOID** — a void never returns more than not voiding (an honest claim: void ≤ served), and
//!   never releases the obligation fixed at acceptance: *tightened to v3 §4.2 Proposition 1* — the
//!   commitment is held to `voided + h_obl` (E-4), the claim stays a conviction target through it
//!   (AG-5), and a conviction landing in the hold collects exactly what it collects without the void.
//! * **REORG-DETERMINISM** — any apply/revert walk over recorded blocks, a fork and its reorg, a restart
//!   at any tip and an IBD from the base all reach the fresh fold's states, root for root.
//! * **SPLIT-NEUTRAL** — the same total collateral split across k bonds never gains issuance capacity,
//!   fork weight or reward against one bond holding it; at ρ = 1 the armed split is also never above
//!   the shipped split (stage 1: equal or less).
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_capacity_stage1_invariants -- --nocapture`

#[path = "capacity_stage1_common.rs"]
mod stage1;
use stage1::*;

use kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_target_v1;
use kaspa_consensus_core::palw_state_v2::{PalwBondStateV2, palw_bond_collateral_is_locked_v6, palw_claim_commitment_v1};
use kaspa_consensus_core::palw_weight_cap_v1::{palw_bond_weight_cap_v1, palw_bounded_immature_v2};

/// The producer bond sizes the random families draw from (MSK).
const SIZES: [u64; 4] = [13_000, 26_000, 100_000, 1_000_000];

// ---------------------------------------------------------------------------------------------
// Shared readings
// ---------------------------------------------------------------------------------------------

/// What one party holds on a state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Holding {
    collateral: u64,
    slashed: u64,
    frozen: bool,
    /// Σ of the vesting legs it is payee of (latched or not).
    vesting: u128,
}

fn holding(s: &PalwChainStateV2, b: &PalwBondKeyV2) -> Holding {
    let record = s.bond(b);
    Holding {
        collateral: record.map_or(0, |r| r.collateral),
        slashed: record.map_or(0, |r| r.slashed),
        frozen: s.bond_freeze_of_v1(b).is_some(),
        vesting: s.vesting_legs_of_payee(b).map(|(_, leg)| u128::from(leg.amount)).sum(),
    }
}

/// The payouts the state asks the next coinbase to pay, credited to the bond whose payout payload each
/// names (a payload no bond carries is dropped), for the claims `only` names (all where `None`).
fn credit_payouts(s: &PalwChainStateV2, paid: &mut BTreeMap<PalwBondKeyV2, u128>, only: Option<&std::collections::BTreeSet<Hash64>>) {
    let payees: BTreeMap<Hash64, PalwBondKeyV2> = s.bonds_iter().map(|(key, record)| (record.payout_payload, *key)).collect();
    for (claim, payout) in s.pending_payouts_iter() {
        if only.is_some_and(|only| !only.contains(claim)) {
            continue;
        }
        if let Some(bond) = payees.get(&payout.payload) {
            *paid.entry(*bond).or_insert(0) += u128::from(payout.amount);
        }
    }
}

/// Σ of the vesting legs `b` is payee of, over the claims `only` names.
fn vesting_of(s: &PalwChainStateV2, b: &PalwBondKeyV2, only: &std::collections::BTreeSet<Hash64>) -> u128 {
    s.vesting_legs_of_payee(b).filter(|(row, _)| only.contains(&row.claim_id)).map(|(_, leg)| u128::from(leg.amount)).sum()
}

/// **Lockstep twins**, `[shipped, armed]`: the same blocks at the same DAAs. The twins may admit
/// different attempts where the armed room is smaller than today's (stage 1 admits equal or fewer:
/// several 8k producers share `ρ·⌈c/2⌉`), and once their claim sets differ, a later attempt may meet
/// room on one twin and not the other; only the claims BOTH admit (`common`) are served, and the
/// rewards compared are theirs. The one-sided admissions are counted and printed.
struct Twins {
    sims: [Sim; 2],
    common: std::collections::BTreeSet<Hash64>,
    shipped_only: usize,
    armed_only: usize,
    paid: [BTreeMap<PalwBondKeyV2, u128>; 2],
}

impl Twins {
    fn new(class: Class, producers: &[(u64, u64)]) -> Twins {
        Twins {
            sims: [Sim::new(params_for(class, false), class, producers), Sim::new(params_for(class, true), class, producers)],
            common: Default::default(),
            shipped_only: 0,
            armed_only: 0,
            paid: [BTreeMap::new(), BTreeMap::new()],
        }
    }

    fn daa(&self) -> u64 {
        assert_eq!(self.sims[0].c.daa, self.sims[1].c.daa, "the twins move in lockstep");
        self.sims[0].c.daa
    }

    fn credit(&mut self) {
        for t in 0..2 {
            credit_payouts(&self.sims[t].c.s, &mut self.paid[t], Some(&self.common));
        }
    }

    /// One block at `daa` on both twins; the attempt's claim id where BOTH admitted it.
    fn block(&mut self, daa: u64, objects: Vec<PalwConsensusObjectV2>, attempt: Option<(u64, u64)>) -> Option<Hash64> {
        let a = self.sims[0].block(daa, objects.clone(), attempt);
        let b = self.sims[1].block(daa, objects, attempt);
        match (a, b) {
            (Some(_), None) => self.shipped_only += 1,
            (None, Some(_)) => self.armed_only += 1,
            _ => {}
        }
        let common = if a == b { b } else { None };
        if let Some(id) = common {
            self.common.insert(id);
        }
        self.credit();
        common
    }

    fn step(&mut self) {
        let daa = self.daa() + 1;
        self.block(daa, vec![], None);
    }

    fn bind(&mut self, claim: Hash64, seats: &[(PalwBondKeyV2, Hash64)]) -> u64 {
        let a = self.sims[0].bind(claim, seats);
        let b = self.sims[1].bind(claim, seats);
        assert_eq!(a, b);
        self.credit();
        a
    }

    fn license(&mut self, claim: Hash64, seats: &[(PalwBondKeyV2, Hash64)], bound: u64) {
        self.sims[0].license(claim, seats, bound);
        self.sims[1].license(claim, seats, bound);
        self.credit();
    }

    /// One block at `daa` (past the tip) on both twins.
    fn jump(&mut self, daa: u64) {
        let daa = daa.max(self.daa() + 1);
        self.block(daa, vec![], None);
    }
}

/// **The honest-no-loss comparison of one party** on the twins: collateral, slashes, freeze, and the
/// paid and vesting rewards of the claims both twins admitted.
fn assert_whole(tw: &Twins, b: &PalwBondKeyV2, what: &str) {
    let (s, a) = (holding(&tw.sims[0].c.s, b), holding(&tw.sims[1].c.s, b));
    let (ps, pa) = (tw.paid[0].get(b).copied().unwrap_or(0), tw.paid[1].get(b).copied().unwrap_or(0));
    let (vs, va) = (vesting_of(&tw.sims[0].c.s, b, &tw.common), vesting_of(&tw.sims[1].c.s, b, &tw.common));
    assert!(a.collateral >= s.collateral, "{what}: collateral armed {} < shipped {}", a.collateral, s.collateral);
    assert_eq!(a.slashed, 0, "{what}: an honest party is slashed on the armed twin");
    assert_eq!(s.slashed, 0, "{what}: (no fraud: nothing slashed on the shipped twin either)");
    assert!(!a.frozen, "{what}: an honest party is frozen on the armed twin");
    assert!(pa + va >= ps + vs, "{what}: paid + vesting armed {} < shipped {}", pa + va, ps + vs);
}

/// No claim of `b` is voided by a conviction route on `s`.
fn assert_never_convicted(s: &PalwChainStateV2, b: &PalwBondKeyV2, what: &str) {
    for (id, claim) in s.claims_iter().filter(|(_, claim)| claim.bond == *b) {
        assert!(
            !matches!(
                claim.phase,
                PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud | PalwVoidReasonV2::AggregateForfeit, .. }
            ),
            "{what}: honest claim {id} voided by a conviction route: {:?}",
            claim.phase
        );
    }
}

/// The bond's committed amount (the SR-7 ledger's reading) at the next DAA.
fn committed(sim: &Sim, bond: &PalwBondKeyV2) -> u128 {
    let at = sim.c.daa + 1;
    let raw = sim.c.extras_at(at).settled_anchor_depth;
    palw_bond_committed_raw_v1(&sim.c.s, &sim.c.sp, bond, at, raw)
}

/// B-3's exit gate on `bond` at the next DAA.
fn exit_shut(sim: &Sim, bond: &PalwBondKeyV2) -> bool {
    let at = sim.c.daa + 1;
    let raw = sim.c.extras_at(at).settled_anchor_depth;
    let record: &PalwBondStateV2 = sim.c.s.bond(bond).expect("a bond");
    palw_bond_collateral_is_locked_v6(&sim.c.s, &sim.c.sp, bond, record, at, sim.c.sp.withdrawal_delay_daa(), raw, true)
}

fn phase_of(s: &PalwChainStateV2, id: &Hash64) -> Option<PalwClaimPhaseV2> {
    s.claim(id).map(|claim| claim.phase.clone())
}

/// `id` is voided by a timeout or a panel failure — not by a conviction route (`BindTimeout` for the
/// floor; a model class whose unbound claim meets no capable panel voids `NoCapablePanel`).
fn unconvicted_void(s: &PalwChainStateV2, id: &Hash64) -> bool {
    matches!(
        phase_of(s, id),
        Some(PalwClaimPhaseV2::Voided { reason, .. }) if !matches!(reason, PalwVoidReasonV2::CourtFraud | PalwVoidReasonV2::AggregateForfeit)
    )
}

// ---------------------------------------------------------------------------------------------
// HONEST-NO-LOSS
// ---------------------------------------------------------------------------------------------

/// **HONEST-NO-LOSS, served runs.** Eight seeded scripts (floor and 8k alternately), three producers of
/// random size, 48 steps of random attempts, binds and licences on honest five-seat panels; every
/// claim is then served, taken to `Final` and past its vesting's maturity. At every step and at the end,
/// for every producer, every seat and the bystanders: collateral armed ≥ shipped, nothing slashed,
/// nothing frozen, paid + vesting armed ≥ shipped, no claim voided by a conviction route.
#[test]
fn honest_no_loss_served_runs_leave_every_honest_party_whole() {
    for seed in 0..8u64 {
        let mut rng = Rng(0x5EED_0000 ^ (seed << 8));
        let class = if seed % 2 == 0 { Class::Floor } else { Class::K8 };
        let producers: Vec<(u64, u64)> = (0..3).map(|i| (90 + i, SIZES[rng.below(4) as usize])).collect();
        let mut tw = Twins::new(class, &producers);
        let seats = tw.sims[0].seats();
        let mut parties: Vec<PalwBondKeyV2> = producers.iter().map(|(n, _)| bond_key(*n)).collect();
        parties.extend(seats.iter().map(|(k, _)| *k));
        parties.extend([bond_key(CHALLENGER), bond_key(ACCUSER)]);
        let (mut open, mut bound, mut licensed) = (Vec::new(), Vec::new(), Vec::new());
        for k in 0..48u64 {
            match rng.below(4) {
                0 | 1 => {
                    let n = producers[rng.below(producers.len() as u64) as usize].0;
                    let daa = tw.daa() + 1;
                    if let Some(id) = tw.block(daa, vec![], Some((n, (seed << 20) + k))) {
                        open.push(id);
                    }
                }
                2 if !open.is_empty() => {
                    let id = open.remove(0);
                    let at = tw.bind(id, &seats);
                    bound.push((id, at));
                }
                3 if !bound.is_empty() => {
                    let (id, at) = bound.remove(0);
                    tw.license(id, &seats, at);
                    licensed.push(id);
                }
                _ => tw.step(),
            }
            for b in &parties {
                assert_whole(&tw, b, &format!("seed {seed} step {k} {b:?}"));
            }
        }
        for id in open.drain(..) {
            let at = tw.bind(id, &seats);
            bound.push((id, at));
        }
        for (id, at) in bound.drain(..) {
            tw.license(id, &seats, at);
            licensed.push(id);
        }
        let last = licensed.iter().filter_map(|id| tw.sims[1].c.s.deadline_of(id)).max().unwrap_or(0);
        tw.jump(last + 1);
        for id in &licensed {
            for t in 0..2 {
                assert!(
                    matches!(phase_of(&tw.sims[t].c.s, id), Some(PalwClaimPhaseV2::Final { .. })),
                    "seed {seed}: {id} Final (twin {t})"
                );
            }
        }
        // On, in steps, past the rows' maturity where the harness's settled clock matures them (each
        // step credits what it pays); the comparison holds at every step either way.
        for _ in 0..8 {
            let daa = tw.daa() + 500;
            tw.jump(daa);
            for b in &parties {
                assert_whole(&tw, b, &format!("seed {seed} maturity {b:?}"));
            }
        }
        for b in &parties {
            assert_never_convicted(&tw.sims[1].c.s, b, &format!("seed {seed}"));
        }
        let owed = |t: usize| -> u128 {
            parties.iter().map(|b| tw.paid[t].get(b).copied().unwrap_or(0) + vesting_of(&tw.sims[t].c.s, b, &tw.common)).sum()
        };
        let (owed_shipped, owed_armed) = (owed(0), owed(1));
        assert!(licensed.is_empty() || owed_armed > 0, "seed {seed}: the served claims earned something (paid or vesting)");
        assert_eq!(owed_armed, owed_shipped, "seed {seed}: at ρ = 1 the served claims earn exactly today's rewards");
        println!(
            "HONEST-NO-LOSS seed {seed} ({}): {} claims served; one-sided admissions shipped {} / armed {} (divergent rooms); paid + vesting armed {owed_armed} = shipped {owed_shipped}",
            class.label(),
            licensed.len(),
            tw.shipped_only,
            tw.armed_only,
        );
    }
}

/// **HONEST-NO-LOSS, an honest claim the network does not serve** (tightened, v3 §5.8/G6): a claim no
/// panel binds voids `BindTimeout` on both twins; nobody loses collateral, nothing is slashed or frozen;
/// the armed twin holds at most that claim's stage commitment (`m_c + reserved`, E-4) and releases it
/// at `voided + h_obl`, after which the twins' ledgers agree again.
#[test]
fn honest_no_loss_an_unserved_claim_costs_no_collateral_and_holds_only_its_commitment() {
    for class in [Class::Floor, Class::K8] {
        for size in [13_000u64, 100_000] {
            let mut tw = Twins::new(class, &[(90, size)]);
            let bond = bond_key(90);
            let daa = tw.daa() + 1;
            let id = tw.block(daa, vec![], Some((90, 0xB1D0))).expect("the claim is admitted");
            let claim = tw.sims[1].c.claim(&id);
            let commitment = palw_claim_commitment_v1(&tw.sims[1].c.sp, &claim, daa + 1).expect("a live claim commits");
            let (window_bind, h_obl) = (tw.sims[1].c.sp.window_bind(), tw.sims[1].c.sp.window_receipt());
            let voided_at = claim.accepted_daa + window_bind + 1;
            tw.jump(voided_at);
            for t in 0..2 {
                assert!(
                    unconvicted_void(&tw.sims[t].c.s, &id),
                    "{} @ {size}: an unconvicted void on twin {t}: {:?}",
                    class.label(),
                    phase_of(&tw.sims[t].c.s, &id)
                );
            }
            let held = committed(&tw.sims[1], &bond);
            let shipped_held = committed(&tw.sims[0], &bond);
            assert!(held <= shipped_held + commitment, "{} @ {size}: the hold is at most the claim's commitment", class.label());
            for b in [bond, bond_key(CHALLENGER), bond_key(ACCUSER)] {
                assert_whole(&tw, &b, &format!("{} @ {size} voided", class.label()));
            }
            tw.jump(voided_at + h_obl + 1);
            assert_eq!(
                committed(&tw.sims[1], &bond),
                committed(&tw.sims[0], &bond),
                "{} @ {size}: released at voided + h_obl",
                class.label()
            );
            for b in [bond, bond_key(CHALLENGER), bond_key(ACCUSER)] {
                assert_whole(&tw, &b, &format!("{} @ {size} released", class.label()));
            }
            println!(
                "HONEST-NO-LOSS unserved {} @ {size}: held {} (commitment {commitment}) for h_obl {h_obl}, collateral unchanged",
                class.label(),
                held.saturating_sub(shipped_held)
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// J1-CAP
// ---------------------------------------------------------------------------------------------

/// **J-1 on one state**: every bond's provisional term is at most `W_cap` of its collateral there, the
/// rooted `bounded_immature` is its re-derivation, and past the fence it is at most `Σ W_cap` (every
/// claim of these runs is a new-rule claim).
fn assert_j1(s: &PalwChainStateV2, sp: &PalwStateParamsV2, what: &str) {
    let derived = palw_bounded_immature_v2(s, sp);
    assert_eq!(s.bounded_immature(), derived, "{what}: W-I3, the rooted value is the re-derivation");
    let index = s.capacity_weight_index();
    let mut caps = 0u128;
    for (bond, _) in index.iter() {
        let collateral = s.bond(bond).map_or(0, |r| r.collateral);
        let cap = palw_bond_weight_cap_v1(collateral);
        let term = index.term(bond, Some(collateral));
        assert!(term <= cap, "{what}: J1-CAP {bond:?} weighs {term} > W_cap {cap}");
        caps += cap;
    }
    assert!(derived <= caps, "{what}: bounded_immature {derived} > Σ W_cap {caps}");
}

/// One random step of a J-1 script on `sim` (attempts by any producer, binds, licences, a `Final`
/// jump, and — once in a while — a proven verdict on a licensed claim, which zeroes its bond's cap).
fn j1_random_step(sim: &mut Sim, rng: &mut Rng, producers: &[(u64, u64)], seeds: &mut u64, lists: &mut [Vec<Hash64>; 3]) {
    let seats = sim.seats();
    match rng.below(10) {
        0..=3 => {
            let n = producers[rng.below(producers.len() as u64) as usize].0;
            *seeds += 1;
            if let Some(id) = sim.claim(n, *seeds) {
                lists[0].push(id);
            }
        }
        4 | 5 if !lists[0].is_empty() => {
            let id = lists[0].remove(rng.below(lists[0].len() as u64) as usize);
            if matches!(phase_of(&sim.c.s, &id), Some(PalwClaimPhaseV2::Provisional)) {
                sim.bind(id, &seats);
                lists[1].push(id);
            } else {
                sim.step(vec![]);
            }
        }
        6 | 7 if !lists[1].is_empty() => {
            let id = lists[1].remove(0);
            if matches!(phase_of(&sim.c.s, &id), Some(PalwClaimPhaseV2::PanelBound { .. })) {
                let bound = sim.c.s.panel(&id).expect("bound").bound_daa;
                sim.license(id, &seats, bound);
                lists[2].push(id);
            } else {
                sim.step(vec![]);
            }
        }
        8 if !lists[2].is_empty() && rng.below(4) == 0 => {
            let id = lists[2].remove(0);
            if matches!(phase_of(&sim.c.s, &id), Some(PalwClaimPhaseV2::ReceiptLicensed { .. })) {
                sim.court_fraud(id);
            } else {
                sim.step(vec![]);
            }
        }
        9 => {
            let daa = sim.c.daa + 1 + rng.below(150);
            sim.block(daa, vec![], None);
        }
        _ => sim.step(vec![]),
    }
}

/// **J1-CAP after every block, every reorg and every restart.** Six seeded armed scripts (floor and 8k),
/// four producers (13k always among them), 60 random steps including binds, licences, `Final` jumps and
/// proven verdicts; after every block [`assert_j1`]; then reorgs — back to a random tip and a
/// different random suffix of 20 blocks, [`assert_j1`] after each — and restarts: every tip's carriage
/// reloaded under its root and checked again.
#[test]
fn j1_cap_holds_after_every_block_reorg_and_restart() {
    for seed in 0..6u64 {
        let mut rng = Rng(0x71CA_0000 ^ (seed << 8));
        let class = if seed % 2 == 0 { Class::Floor } else { Class::K8 };
        let mut producers = vec![(90u64, 13_000u64)];
        producers.extend((1..4).map(|i| (90 + i, SIZES[rng.below(4) as usize])));
        let mut sim = Sim::new(params_for(class, true), class, &producers);
        let mut lists: [Vec<Hash64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        let mut seeds = seed << 24;
        for k in 0..60 {
            j1_random_step(&mut sim, &mut rng, &producers, &mut seeds, &mut lists);
            assert_j1(&sim.c.s, &sim.c.sp, &format!("seed {seed} step {k}"));
        }
        // Restarts: every recorded tip reloads under its root, and J-1 holds on what loads.
        for (j, b) in sim.tape.iter().enumerate() {
            let reloaded = PalwStateCarriageV2::from_state(&b.child)
                .into_state(&sim.c.sp, Some(b.child.state_root()))
                .expect("the carriage reloads");
            assert_eq!(reloaded, b.child, "seed {seed}: tip {j} reloads");
            assert_j1(&reloaded, &sim.c.sp, &format!("seed {seed} restart at {j}"));
        }
        // Reorgs: three forks off random tips, each a different random suffix.
        let tape = sim.tape.clone();
        for fork in 0..3u64 {
            let j = 1 + rng.below(tape.len() as u64 - 1) as usize;
            sim.tape = tape.clone();
            sim.rewind_to(j);
            if class == Class::Floor {
                // The floor tape takes no edits, so the chain of reverts from the old tip is exact.
                let mut s = tape.last().unwrap().child.clone();
                for i in (j..tape.len()).rev() {
                    s = revert_delta_v2(&s, &tape[i].delta, &sim.c.sp).expect("the old branch reverts");
                    assert_eq!(s, tape[i].parent, "seed {seed}: reverting block {i} reaches its recorded parent");
                    assert_j1(&s, &sim.c.sp, &format!("seed {seed} revert {i}"));
                }
            }
            let mut lists_f = lists.clone();
            for k in 0..20 {
                j1_random_step(&mut sim, &mut rng, &producers, &mut seeds, &mut lists_f);
                assert_j1(&sim.c.s, &sim.c.sp, &format!("seed {seed} fork {fork} step {k}"));
            }
        }
        println!("J1-CAP seed {seed} ({}): {} blocks, 3 forks, every tip restarted", class.label(), tape.len());
    }
}

// ---------------------------------------------------------------------------------------------
// LIABILITY-SURVIVES
// ---------------------------------------------------------------------------------------------

/// **A trial conviction on a copy of `sim`**: a proven court verdict on licensed or `Final` `id`, two
/// blocks on a fork. Returns the bond's collateral before and after, or `None` where no court opens on
/// the claim now (it is gone, or in a phase a court does not open on).
fn trial_conviction(sim: &Sim, id: Hash64) -> Option<(u64, u64)> {
    let phase = phase_of(&sim.c.s, &id)?;
    if !matches!(phase, PalwClaimPhaseV2::ReceiptLicensed { .. } | PalwClaimPhaseV2::Final { .. }) {
        return None;
    }
    let bond = sim.c.s.claim(&id)?.bond;
    let mut copy = sim.fork();
    let before = copy.c.s.bond(&bond)?.collateral;
    let open = court_opened(&copy.c.s, id, bond_key(CHALLENGER));
    let daa = copy.c.daa + 1;
    copy.try_block(daa, vec![open]).ok()?;
    let session = court_session_of(&copy.c.s, id, bond_key(CHALLENGER));
    copy.step(vec![guilty_close(session)]);
    Some((before, copy.c.s.bond(&bond)?.collateral))
}

/// **LIABILITY-SURVIVES.** For floor and 8k, a producer's licensed junk claim `x` is followed from its
/// licence to the end of its conviction window (its retirement) through: a sibling claim's
/// unconvicted void (it was accepted first and is never bound, so it voids while `x` is licensed), a
/// retirement request by its bond, a reorg (the blocks since the licence rewound and folded again,
/// input for input) and a restart (the carriage reloaded). At each checkpoint: `x` is a conviction
/// target (AG-5), the bond's exit is shut (B-3), and — while `x` is licensed — a proven verdict on a
/// copy of the state takes the whole bond (AG-2; the trial must run at every licensed checkpoint). Past
/// `Final` the same target and exit checks run every 200 DAA until `x` retires.
#[test]
fn liability_survives_void_retirement_reorg_and_restart_until_the_window_closes() {
    for class in [Class::Floor, Class::K8] {
        let mut sim = Sim::new(params_for(class, true), class, &[(90, 100_000)]);
        let bond = bond_key(90);
        let seats = sim.seats();
        let sibling = sim.claim(90, 0x11AC).expect("the sibling is admitted");
        let sibling_void_at = sim.c.claim(&sibling).accepted_daa + sim.c.sp.window_bind() + 1;
        sim.block(sibling_void_at - 12, vec![], None);
        let x = sim.claim(90, 0x11AB).expect("x is admitted");
        let bound = sim.bind(x, &seats);
        sim.license(x, &seats, bound);
        let licensed_at = sim.c.daa;
        let (mut checkpoints, mut trials) = (0, 0);
        let mut check = |sim: &Sim, what: &str| {
            assert!(palw_offence_target_v1(&sim.c.s, &x).is_some(), "{} {what}: x is a conviction target (AG-5)", class.label());
            assert!(exit_shut(sim, &bond), "{} {what}: the bond's exit is shut (B-3)", class.label());
            let licensed = matches!(phase_of(&sim.c.s, &x), Some(PalwClaimPhaseV2::ReceiptLicensed { .. }));
            match trial_conviction(sim, x) {
                Some((before, after)) => {
                    assert!(before > 0, "{} {what}: the bond still posts collateral", class.label());
                    assert_eq!(after, 0, "{} {what}: a proven verdict takes the whole bond (AG-2)", class.label());
                    trials += 1;
                }
                None => assert!(!licensed, "{} {what}: a licensed x must be convictable", class.label()),
            }
            checkpoints += 1;
        };
        check(&sim, "licensed");
        // The sibling's void, while x is licensed.
        sim.block(sibling_void_at, vec![], None);
        assert!(unconvicted_void(&sim.c.s, &sibling), "{}: the sibling voided: {:?}", class.label(), phase_of(&sim.c.s, &sibling));
        check(&sim, "after the sibling's void");
        // A retirement request: refused, or accepted without releasing anything the liability needs.
        let retire = PalwConsensusObjectV2::BondRetireRequested { bond, signature: vec![1] };
        let daa = sim.c.daa + 1;
        let refused = sim.try_block(daa, vec![retire]).is_err();
        if refused {
            sim.step(vec![]);
        }
        check(&sim, if refused { "after a refused retirement" } else { "after an accepted retirement request" });
        // A reorg: back to the licence's block, and the same blocks folded again.
        let at = sim.tape.iter().rposition(|b| b.daa == licensed_at).expect("the licence's block");
        let tip = sim.c.s.clone();
        let off = sim.rewind_to(at + 1);
        check(&sim, "rewound to the licence");
        sim.replay(&off);
        assert_eq!(sim.c.s, tip, "{}: the reorg's refold is the tip it left", class.label());
        check(&sim, "after the reorg");
        // A restart.
        let reloaded = PalwStateCarriageV2::from_state(&sim.c.s).into_state(&sim.c.sp, Some(sim.c.s.state_root())).expect("reloads");
        assert_eq!(reloaded, sim.c.s, "{}: the restart loads the state", class.label());
        sim.c.s = reloaded;
        check(&sim, "after the restart");
        // To Final, then on through the window to retirement.
        if let Some(PalwClaimPhaseV2::ReceiptLicensed { .. }) = phase_of(&sim.c.s, &x) {
            let deadline = sim.c.s.deadline_of(&x).expect("a licensed claim's deadline");
            sim.block(deadline + 1, vec![], None);
        }
        assert!(
            matches!(phase_of(&sim.c.s, &x), Some(PalwClaimPhaseV2::Final { .. })),
            "{}: x Final: {:?}",
            class.label(),
            phase_of(&sim.c.s, &x)
        );
        check(&sim, "Final");
        let mut t = sim.c.daa;
        let end = t + sim.c.sp.claim_retirement_daa() + 400;
        while t + 200 <= end {
            t += 200;
            sim.block(t, vec![], None);
            if sim.c.s.claim(&x).is_none() {
                break;
            }
            check(&sim, &format!("DAA {t} of the window"));
        }
        assert!(trials >= 5, "{}: the trial conviction ran at every licensed checkpoint ({trials})", class.label());
        println!(
            "LIABILITY-SURVIVES {}: {checkpoints} checkpoints from licence ({licensed_at}) to DAA {t}, {trials} trial convictions took the whole bond; retirement request {}",
            class.label(),
            if refused { "refused" } else { "accepted, liability intact" }
        );
    }
}

// ---------------------------------------------------------------------------------------------
// NO-FREE-VOID
// ---------------------------------------------------------------------------------------------

/// A party's value at a horizon: collateral + paid + vesting.
fn value(sim: &Sim, paid: &BTreeMap<PalwBondKeyV2, u128>, b: &PalwBondKeyV2) -> u128 {
    let h = holding(&sim.c.s, b);
    u128::from(h.collateral) + h.vesting + paid.get(b).copied().unwrap_or(0)
}

/// **NO-FREE-VOID (a): a void never returns more than not voiding.** For floor and 8k at 13k and 100k,
/// the same claim is served (bound, licensed, `Final`, matured) on one armed run and left to void
/// `BindTimeout` on the other; at a common horizon past `h_obl` and the rows' maturity the producer's
/// value (collateral + paid + vesting) on the void run is at most the served run's.
#[test]
fn no_free_void_a_void_never_returns_more_than_serving() {
    for class in [Class::Floor, Class::K8] {
        for size in [13_000u64, 100_000] {
            let bond = bond_key(90);
            let mut runs =
                [Sim::new(params_for(class, true), class, &[(90, size)]), Sim::new(params_for(class, true), class, &[(90, size)])];
            let mut paid = [BTreeMap::new(), BTreeMap::new()];
            let id = runs[0].claim(90, 0xF0F0).expect("admitted");
            assert_eq!(runs[1].claim(90, 0xF0F0), Some(id));
            let seats = runs[0].seats();
            let bound = runs[0].bind(id, &seats);
            runs[0].license(id, &seats, bound);
            let deadline = runs[0].c.s.deadline_of(&id).expect("deadline");
            runs[0].block(deadline + 1, vec![], None);
            credit_payouts(&runs[0].c.s, &mut paid[0], None);
            let horizon = runs[1].c.daa + runs[1].c.sp.window_bind() + runs[1].c.sp.window_receipt() + 4_000;
            let mut t = runs[0].c.daa.max(runs[1].c.daa);
            while t < horizon {
                t += 500;
                for (r, sim) in runs.iter_mut().enumerate() {
                    sim.block(t, vec![], None);
                    credit_payouts(&sim.c.s, &mut paid[r], None);
                }
            }
            assert!(matches!(phase_of(&runs[1].c.s, &id), Some(PalwClaimPhaseV2::Voided { .. }) | None), "the unserved claim voided");
            let (served, voided) = (value(&runs[0], &paid[0], &bond), value(&runs[1], &paid[1], &bond));
            assert!(voided <= served, "{} @ {size}: a void returned {voided} > serving's {served}", class.label());
            println!("NO-FREE-VOID (a) {} @ {size}: void {voided} ≤ served {served}", class.label());
        }
    }
}

/// **NO-FREE-VOID (b): a void never releases the obligation fixed at acceptance** (v3 §4.2 Proposition
/// 1). A claim voided `BindTimeout` keeps its stage commitment reserved to `voided + h_obl` (E-4) and
/// stays a conviction target through it (AG-5); and a conviction of its bond landing inside the hold (an
/// objective equivocation, intent class) collects exactly what the same conviction collects on the twin
/// run where the claim is still live — the void shields nothing.
#[test]
fn no_free_void_a_void_keeps_the_obligation_and_shields_no_conviction() {
    for class in [Class::Floor, Class::K8] {
        let bond = bond_key(90);
        let mut runs =
            [Sim::new(params_for(class, true), class, &[(90, 100_000)]), Sim::new(params_for(class, true), class, &[(90, 100_000)])];
        let id = runs[0].claim(90, 0xE4E4).expect("admitted");
        assert_eq!(runs[1].claim(90, 0xE4E4), Some(id));
        let accepted = runs[0].c.claim(&id).accepted_daa;
        let commitment = palw_claim_commitment_v1(&runs[0].c.sp, &runs[0].c.claim(&id), accepted + 1).expect("commits");
        let (window_bind, h_obl) = (runs[0].c.sp.window_bind(), runs[0].c.sp.window_receipt());
        let voided_at = accepted + window_bind + 1;
        // Run 0 lets it void; run 1 keeps it live (bound one block before the bind window closes).
        runs[0].block(voided_at, vec![], None);
        assert!(unconvicted_void(&runs[0].c.s, &id), "{}: an unconvicted void: {:?}", class.label(), phase_of(&runs[0].c.s, &id));
        runs[1].block(voided_at - 2, vec![], None);
        let seats = runs[1].seats();
        runs[1].bind(id, &seats);
        let mut t = voided_at;
        while t + 100 < voided_at + h_obl {
            t += 100;
            runs[0].block(t, vec![], None);
            assert!(committed(&runs[0], &bond) >= commitment, "{}: E-4 holds the commitment at DAA {t}", class.label());
            assert!(palw_offence_target_v1(&runs[0].c.s, &id).is_some(), "{}: AG-5, a conviction target at DAA {t}", class.label());
            assert!(exit_shut(&runs[0], &bond), "{}: the exit stays shut through the hold", class.label());
        }
        // A conviction inside the hold, on both runs at the same DAA.
        let (offence, _) = equivocation_of(bond, runs[0].model.unwrap_or_else(|| runs[0].c.sp.base_class_id()), 0xE0);
        let at = t + 1;
        let mut collected = Vec::new();
        for sim in runs.iter_mut() {
            let before = sim.c.s.bond(&bond).unwrap().collateral;
            sim.block(at.max(sim.c.daa + 1), vec![offence.clone()], None);
            collected.push(before - sim.c.s.bond(&bond).unwrap().collateral);
        }
        assert!(collected[0] > 0, "{}: the conviction collects in the hold", class.label());
        assert_eq!(collected[0], collected[1], "{}: the void shielded nothing", class.label());
        // Past the hold the commitment is released (or charged): never before.
        runs[0].block(voided_at + h_obl + 1, vec![], None);
        println!(
            "NO-FREE-VOID (b) {}: commitment {commitment} held to voided + {h_obl}; the in-hold conviction collected {} on both runs",
            class.label(),
            collected[0]
        );
    }
}

// ---------------------------------------------------------------------------------------------
// REORG-DETERMINISM
// ---------------------------------------------------------------------------------------------

/// A floor attempt by bond `n` on a tape (not asserted: a refusal is a skip, a block-level refusal is
/// an empty block).
fn tape_attempt(tape: &mut Tape, n: u64, seed: u64) -> Option<Hash64> {
    let (env, key, id) = floor_attempt_of(&tape.c, n, seed);
    let anchor = floor_job_anchor(&tape.c.p, bond_key(n), 0x10C0 + seed);
    let daa = tape.c.daa + 1;
    match tape.block(daa, vec![], Some((env, key, anchor)), T12_BLOCK_SUBSIDY_SOMPI) {
        Ok(_) => tape.c.s.claim(&id).is_some().then_some(id),
        Err(_) => {
            tape.at(daa, vec![]);
            None
        }
    }
}

/// One random floor step on a tape: attempts by any of `producers`, binds, licences, a proven verdict,
/// a DAA jump.
fn tape_random_step(tape: &mut Tape, rng: &mut Rng, producers: &[u64], seeds: &mut u64, lists: &mut [Vec<Hash64>; 3]) {
    let seats = tape.c.floor_seats();
    match rng.below(9) {
        0..=3 => {
            *seeds += 1;
            if let Some(id) = tape_attempt(tape, producers[rng.below(producers.len() as u64) as usize], *seeds) {
                lists[0].push(id);
            }
        }
        4 if !lists[0].is_empty() => {
            let id = lists[0].remove(0);
            if matches!(tape.c.s.claim(&id).map(|c| c.phase.clone()), Some(PalwClaimPhaseV2::Provisional)) {
                tape.bind_to(id, &seats);
                lists[1].push(id);
            } else {
                tape.step(vec![]);
            }
        }
        5 if !lists[1].is_empty() => {
            let id = lists[1].remove(0);
            if let Some(PalwClaimPhaseV2::PanelBound { .. }) = tape.c.s.claim(&id).map(|c| c.phase.clone()) {
                let bound = tape.c.s.panel(&id).expect("bound").bound_daa;
                tape.step(vec![PalwConsensusObjectV2::ReceiptLicensed {
                    claim: id,
                    receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect(),
                }]);
                lists[2].push(id);
            } else {
                tape.step(vec![]);
            }
        }
        6 if !lists[2].is_empty() => {
            let id = lists[2].remove(0);
            if let Some(PalwClaimPhaseV2::ReceiptLicensed { .. }) = tape.c.s.claim(&id).map(|c| c.phase.clone()) {
                let open = court_opened(&tape.c.s, id, bond_key(CHALLENGER));
                tape.step(vec![open]);
                let session = court_session_of(&tape.c.s, id, bond_key(CHALLENGER));
                tape.step(vec![guilty_close(session)]);
            } else {
                tape.step(vec![]);
            }
        }
        7 => {
            let daa = tape.c.daa + 1 + rng.below(700);
            tape.at(daa, vec![]);
        }
        _ => tape.step(vec![]),
    }
}

/// **REORG-DETERMINISM.** Six seeded armed floor scripts (four producers, the challenger), 50 random
/// steps each — attempts, binds, licences, proven verdicts (AG-2's whole-bond forfeiture and its
/// `AggregateForfeit` voids), jumps across `Final`, void and retirement deadlines — on
/// `rcore_common::Tape`, then: every block reverted to the base and re-applied (roots at every tip);
/// an IBD from the base (the fresh fold: delta, root and state equal); a restart at three random tips
/// (the loaded carriage folds the rest identically); and three forks at random tips with different
/// random suffixes, reorged to and back.
#[test]
fn reorg_determinism_every_walk_reaches_the_fresh_folds_roots() {
    for seed in 0..6u64 {
        let mut rng = Rng(0xDE7E_0000 ^ (seed << 8));
        let mut c = Chain::new(params_for(Class::Floor, true));
        c.attribution = true;
        let producers: Vec<u64> = (0..4).map(|i| 90 + i).collect();
        let mut objects: Vec<PalwConsensusObjectV2> =
            producers.iter().map(|n| bond_obj(*n, SIZES[rng.below(4) as usize] * MSK)).collect();
        objects.push(bond_obj(CHALLENGER, 400_000 * MSK));
        c.step(&objects);
        let mut tape = Tape::new(c);
        let mut lists: [Vec<Hash64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        let mut seeds = seed << 24;
        for _ in 0..50 {
            tape_random_step(&mut tape, &mut rng, &producers, &mut seeds, &mut lists);
        }
        tape.revert_to_base_and_reapply();
        tape.ibd_from(tape.base.clone());
        for _ in 0..3 {
            let j = rng.below(tape.len() as u64) as usize;
            tape.restart_at(j);
        }
        for _ in 0..3 {
            let j = 1 + rng.below(tape.len() as u64 - 1) as usize;
            let mut fork = tape.fork(j);
            let mut lists_f: [Vec<Hash64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
            for _ in 0..12 {
                tape_random_step(&mut fork, &mut rng, &producers, &mut seeds, &mut lists_f);
            }
            tape.reorg_to(j, &fork);
            fork.revert_to_base_and_reapply();
        }
        println!("REORG-DETERMINISM seed {seed}: {} blocks, tip root {}", tape.len(), tape.c.s.state_root());
    }
}

/// **REORG-DETERMINISM on 8k** — the same walks over a short 8k tape (six blocks inside the readiness
/// horizon a tape cannot re-prove: two attempts, a bind, a licence), armed at ρ = 1.
#[test]
fn reorg_determinism_holds_on_the_8k_class() {
    let mut c = Chain::new(params_for(Class::K8, true));
    c.attribution = true;
    c.room = true;
    let class_id = model_classes(&c.p).0;
    let honest_cards = honest(&c.p);
    c.s = readied(&c.sp, &activated(&c.sp, &c.s, class_id), &honest_cards, class_id, c.daa);
    c.step(&[bond_obj(90, 100_000 * MSK), bond_obj(91, 13_000 * MSK)]);
    c.s = readied(&c.sp, &c.s, &honest_cards, class_id, c.daa);
    let mut tape = Tape::new(c);
    let a = tape.model_attempt(class_id, 90, 0x8A01);
    let _b = tape.model_attempt(class_id, 91, 0x8A02);
    let seats = honest_seats(&tape.c.p, 5);
    let bound = tape.bind_to(a, &seats);
    tape.step(vec![PalwConsensusObjectV2::ReceiptLicensed {
        claim: a,
        receipts: seats.iter().map(|(k, _)| valid(a, *k, bound)).collect(),
    }]);
    tape.step(vec![]);
    tape.revert_to_base_and_reapply();
    tape.ibd_from(tape.base.clone());
    for j in 0..tape.len() {
        tape.restart_at(j);
    }
    let mut fork = tape.fork(2);
    fork.step(vec![]);
    fork.step(vec![]);
    tape.reorg_to(2, &fork);
    println!("REORG-DETERMINISM 8k: {} blocks walked", tape.len());
}

// ---------------------------------------------------------------------------------------------
// SPLIT-NEUTRAL
// ---------------------------------------------------------------------------------------------

/// What `bonds` (n, MSK) take of `class` on one chain: the claims admitted by a round-robin fill (four
/// attempt blocks a DAA until every bond has been refused `fill` times in a row), the fork weight once
/// every admitted claim is bound and licensed, and the reward they escrow.
fn take(class: Class, armed: bool, bonds: &[(u64, u64)], per_bond: u64) -> (usize, u128, u128) {
    let mut sim = Sim::new(params_for(class, armed), class, bonds);
    let mut ids = Vec::new();
    let base = sim.c.daa;
    let total = per_bond * bonds.len() as u64;
    for k in 0..total {
        let n = bonds[(k % bonds.len() as u64) as usize].0;
        if let Some(id) = sim.block(base + 1 + k / 4, vec![], Some((n, 0x5C_0000 + k))) {
            ids.push(id);
        }
    }
    let reward: u128 = ids.iter().map(|id| u128::from(sim.c.claim(id).escrowed_reward)).sum();
    let seats = sim.seats();
    for id in &ids {
        let bound = sim.bind(*id, &seats);
        sim.license(*id, &seats, bound);
    }
    (ids.len(), sim.c.s.bounded_immature(), reward)
}

/// **SPLIT-NEUTRAL.** For floor and 8k: 130,000 MSK as ten 13,000 MSK bonds against one 130,000 MSK
/// bond, and 1,000,000 MSK as ten 100,000 MSK bonds against one 1,000,000 MSK bond. On the armed chain
/// (ρ = 1) the split never takes more claims, more fork weight (all of them licensed) or more escrowed
/// reward than the whole; and the armed split never takes more claims than the shipped split (equal or
/// less). The shipped split is printed beside it: under T-2(a) (a share per bond, not per stake) it can
/// beat the whole on 8k, which the armed room share removes.
#[test]
fn split_neutral_ten_small_bonds_never_beat_one_bond_of_their_total() {
    for class in [Class::Floor, Class::K8] {
        for (small, whole) in [(13_000u64, 130_000u64), (100_000, 1_000_000)] {
            let split: Vec<(u64, u64)> = (0..10).map(|i| (100 + i, small)).collect();
            let one = [(90u64, whole)];
            let per_bond = match class {
                Class::Floor => 170 / 10 + 4 + (whole / 6_500),
                _ => 8,
            };
            let armed_split = take(class, true, &split, per_bond.div_ceil(10) + 4);
            let armed_one = take(class, true, &one, per_bond);
            let shipped_split = take(class, false, &split, per_bond.div_ceil(10) + 4);
            println!(
                "SPLIT-NEUTRAL {} {whole} MSK: armed 10×{small} {:?} vs 1×{whole} {:?}; shipped 10×{small} {:?}",
                class.label(),
                armed_split,
                armed_one,
                shipped_split
            );
            assert!(armed_split.0 <= armed_one.0, "{}: issuance, split {} > whole {}", class.label(), armed_split.0, armed_one.0);
            assert!(armed_split.1 <= armed_one.1, "{}: fork weight, split {} > whole {}", class.label(), armed_split.1, armed_one.1);
            assert!(armed_split.2 <= armed_one.2, "{}: reward, split {} > whole {}", class.label(), armed_split.2, armed_one.2);
            assert!(armed_split.0 <= shipped_split.0, "{}: the armed split takes more than today's", class.label());
        }
    }
}
