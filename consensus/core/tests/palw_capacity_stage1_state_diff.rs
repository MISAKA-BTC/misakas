//! **ADR-0160 stage 1 — the rho = 1 state diff** (rcore/cap-s1; the user's staged-safety plan of
//! 2026-09-26, stage 1's gate (a)).
//!
//! One scripted scenario is folded twice through testnet-12's own V2 fold (`rcore_common::Chain`: every
//! block's delta re-applies and reverts, and its carriage reloads under its committed root):
//!
//! * **shipped** — `palw_t12_shipped_params()`, the DAA-750 release (`dbbc9104…`), every capacity fence
//!   dormant;
//! * **armed** — the same ruleset with every entry of `PALW_T12_CAPACITY_FENCES_V1` armed at [`H`]
//!   (`palw_t12_arm_capacity_fences_v1`: F-W, F-E, F-L at stage 1's ρ = 1 / q = 0, F-B, F-R).
//!
//! over {floor, 8k, 2M} × {13,000, 100,000, 1,000,000 MSK} producer bonds (the 2M row opened on BOTH
//! twins by its flag-day measured row, `t12_2m_flag_day_row`: at launch it takes no claim at all). The
//! script (one run per twin, identical blocks, identical DAAs):
//!
//! 1. **fill** — the subject bond attempts once a block, four blocks a DAA, for a fixed number of
//!    blocks (the class's), so the ceiling's (and the class room's) refusals are measured on both twins
//!    over the same blocks;
//! 2. **bind** the first (up to) three claims on an honest five-seat panel, **licence** them with five
//!    `Valid`s, take them to **`Final`**;
//! 3. **void** — the unbound claims run out their bind window (`BindTimeout`), then past `h_obl`;
//! 4. **convict** — the subject makes two claims, one is licensed and convicted by a proven court verdict
//!    (`CourtFraud`, the intent class).
//!
//! At each stage both states are snapshotted per component — **reservations** (the bond's ledger and
//! each claim's `reserved` and commitment), **escrow** (each claim's escrowed reward and the bond's
//! escrow slot), **weights** (`bounded_immature`, `safe_weight`), **liabilities** (collateral, slashed,
//! the freeze, AggregateForfeit voids), **payouts** (the vesting rows' legs and the counters) and **seat
//! locks** (the duty each seat reserved at bind and the lock each counted `Valid` posted) — and diffed.
//! Every difference must be one ADR-0160 intends ([`intended`], with its section); anything else fails
//! the test (`unexpected`). The table prints with `--nocapture`.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_capacity_stage1_state_diff -- --nocapture`

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_arm_capacity_fences_v1, palw_t12_shipped_params};
use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwVoidReasonV2, palw_bond_committed_raw_v1};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// 1 MSK in sompi.
const MSK: u64 = 100_000_000;
/// The capacity fences' height on the armed twin — above the release's 750 (their prerequisites) and
/// below every block the script folds (the chain starts at DAA 1,000).
const H: u64 = 1_001;
/// The subject producer bond.
const SUBJECT: u64 = 90;
/// The court challenger of the conviction stage (L-T6's bond 61).
const CHALLENGER: u64 = 61;
/// The three bond sizes (MSK).
const BONDS: [u64; 3] = [13_000, 100_000, 1_000_000];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    Floor,
    K8,
    M2,
}

impl Class {
    fn label(self) -> &'static str {
        match self {
            Class::Floor => "floor",
            Class::K8 => "8k",
            Class::M2 => "2M",
        }
    }

    /// Attempt blocks in the fill: past the largest ceiling any twin reaches (156 floor or 8k claims at
    /// 1M by the 500‰ ceiling; the 2M row's C7 cap is one).
    fn fill_blocks(self) -> u64 {
        match self {
            Class::Floor | Class::K8 => 164,
            Class::M2 => 6,
        }
    }
}

/// testnet-12 as shipped for `class` (the 2M row opened by its flag-day row on both twins), and with the
/// capacity list armed at [`H`] where `armed`.
pub fn params_for(class: Class, armed: bool) -> Params {
    let mut p = palw_t12_shipped_params();
    if class == Class::M2 {
        p.palw_class_verify_rows = Box::leak(Box::new([t12_2m_flag_day_row()]));
        p.sync_palw_class_verify_deadline();
        p.validate_palw_v2().expect("the 2M flag-day fixture validates on the shipped release");
    }
    if armed {
        palw_t12_arm_capacity_fences_v1(&mut p, Some(ForkActivation::new(H))).expect("the capacity list over the release");
    }
    p
}

/// One claim's components at a stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimSnap {
    pub phase: String,
    pub reserved: u128,
    pub commitment: u128,
    pub escrowed_reward: u64,
    pub escrow_slot: u128,
    /// Each seat's duty at bind (the duty row's `seat_exposure`), where a panel is bound.
    pub duty: Option<u128>,
    /// The locks the counted `Valid`s posted (sum over seats).
    pub locks: u128,
    /// The vesting row's legs: (producer, Σ seats, reserve).
    pub vesting: Option<(u64, u64, u64)>,
}

/// One stage's components.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snap {
    pub stage: &'static str,
    pub daa: u64,
    pub accepted: usize,
    pub refused: usize,
    pub claims: BTreeMap<Hash64, ClaimSnap>,
    pub bond_reserved_exposure: u128,
    pub bond_committed: u128,
    pub bounded_immature: u128,
    pub safe_weight: u128,
    pub collateral: u64,
    pub slashed: u64,
    pub freeze: Option<(bool, u64)>,
    pub vesting_created: u128,
    pub vesting_burned: u128,
    pub vesting_moved: u128,
}

/// The run's snapshots, in stage order.
pub struct Run {
    pub snaps: Vec<Snap>,
    pub windows: (u64, u64, u64),
    /// Why the fill's refused attempts were skipped (the fold's messages, first 60 characters), counted.
    pub skips: BTreeMap<String, usize>,
}

/// A [`Chain`] folded with its own blue-score counter, so several blocks may share a DAA (the fill runs
/// four attempts a DAA) while every block stays a strict chain step. Every block is checked as
/// `Chain::step_at` checks one: the delta re-applies and reverts, the carriage reloads under its root.
pub struct Sim {
    pub c: Chain,
    blue: u64,
    model: Option<Hash64>,
    /// The fold's skip messages (first 60 characters), counted.
    skips: BTreeMap<String, usize>,
}

impl Sim {
    /// The subject's and the court's bonds, and (for a model class) the class made `Active` and its seats
    /// ready — the same objects on both twins.
    fn new(p: Params, class: Class, collateral: u64) -> Sim {
        let mut c = Chain::new(p);
        c.attribution = true;
        let model = match class {
            Class::Floor => None,
            Class::K8 => Some(model_classes(&c.p).0),
            Class::M2 => Some(model_classes(&c.p).1),
        };
        if let Some(class_id) = model {
            c.room = true;
            let honest = honest(&c.p);
            c.s = readied(&c.sp, &activated(&c.sp, &c.s, class_id), &honest, class_id, c.daa);
        }
        let blue = c.daa;
        let mut sim = Sim { c, blue, model, skips: BTreeMap::new() };
        let daa = sim.c.daa + 1;
        sim.block(daa, vec![bond_obj(SUBJECT, collateral * MSK), bond_obj(CHALLENGER, 400_000 * MSK), bond_obj(1, 50_000 * MSK)], None);
        sim
    }

    /// Re-prove the genesis cards ready for the model class (testnet-12's readiness horizon).
    fn reready(&mut self) {
        if let Some(class_id) = self.model {
            let honest = honest(&self.c.p);
            self.c.s = readied(&self.c.sp, &self.c.s, &honest, class_id, self.c.daa);
        }
    }

    /// One block at `daa` (≥ the tip's) with `objects` and, optionally, the subject's attempt `seed`.
    /// Returns the attempt's claim id if the fold recorded it; a skip is counted, the block stands.
    fn block(&mut self, daa: u64, objects: Vec<PalwConsensusObjectV2>, seed: Option<u64>) -> Option<Hash64> {
        assert!(daa >= self.c.daa, "DAA never falls");
        self.reready();
        let c = &mut self.c;
        let attempt = seed.map(|seed| match self.model {
            None => {
                let (env, key, id) = floor_attempt_of(c, SUBJECT, seed);
                (env, key, id, floor_job_anchor(&c.p, bond_key(SUBJECT), 0x10C0 + seed))
            }
            Some(class_id) => {
                let pwu = class_pwu(&c.p, &c.s, class_id, daa);
                let (env, key, id) =
                    junk_attempt(class_id, bond_key(SUBJECT), pubkey_of(SUBJECT), &operator_pubkey_of(SUBJECT), pwu, seed, 0x5_0000 + seed);
                (env, key, id, Hash64::default())
            }
        });
        self.blue += 1;
        let mut e = c.extras_at(daa);
        let (work, key) = match &attempt {
            Some((env, key, _, anchor)) => {
                e.own_job_anchor = *anchor;
                (PalwBlockWorkV3::Attempt(env), *key)
            }
            None => (PalwBlockWorkV3::None, Hash64::default()),
        };
        let subsidy = if attempt.is_some() { T12_BLOCK_SUBSIDY_SOMPI } else { 0 };
        let x = PalwBlockContextV2 { block: h(0xD1FF_0000_0000 + self.blue), daa_score: daa, blue_score: self.blue, subsidy };
        let parent = c.s.clone();
        let folded = fold_with(&c.p, &c.sp, &parent, &x, &objects, work, key, &e);
        // An attempt the fold refuses at BLOCK level (the class gate's `ClassNotAdmitting`: a producer's
        // pre-check never mines it) is counted as refused, and the block is folded without it.
        let (child, delta, skips, attempt) = match (folded, attempt) {
            (Ok((child, delta, skips)), attempt) => (child, delta, skips, attempt),
            (Err(err), Some(_)) => {
                *self.skips.entry(format!("(block) {}", err.to_string().chars().take(52).collect::<String>())).or_insert(0) += 1;
                let e = c.extras_at(daa);
                let (child, delta, skips) = fold_with(&c.p, &c.sp, &parent, &x, &objects, PalwBlockWorkV3::None, Hash64::default(), &e)
                    .unwrap_or_else(|err| panic!("the block at DAA {daa} folds without the attempt: {err}"));
                (child, delta, skips, None)
            }
            (Err(err), None) => panic!("the block at DAA {daa} folds: {err}"),
        };
        assert_eq!(apply_delta_v2(&parent, &delta, &c.sp).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
        assert_eq!(revert_delta_v2(&child, &delta, &c.sp).expect("reverts"), parent, "DAA {daa}: the delta reverts");
        let reloaded =
            PalwStateCarriageV2::from_state(&child).into_state(&c.sp, Some(child.state_root())).expect("the carriage reloads");
        assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
        for (_, reason) in &skips {
            *self.skips.entry(reason.chars().take(60).collect()).or_insert(0) += 1;
        }
        c.s = child;
        c.daa = daa;
        attempt.and_then(|(_, _, id, _)| c.s.claim(&id).is_some().then_some(id))
    }

    /// The next block, with `objects`.
    fn step(&mut self, objects: Vec<PalwConsensusObjectV2>) {
        let daa = self.c.daa + 1;
        self.block(daa, objects, None);
    }

    /// `claim` bound to `seats` in the next block; the bind's DAA.
    fn bind(&mut self, claim: Hash64, seats: &[(PalwBondKeyV2, Hash64)]) -> u64 {
        let anchor = h(0xAC_0000 + self.c.daa);
        self.step(vec![PalwConsensusObjectV2::PanelBound { claim, anchor, seats: seats_of(seats) }]);
        assert!(matches!(self.c.claim(&claim).phase, PalwClaimPhaseV2::PanelBound { .. }), "the panel binds");
        self.c.daa
    }

    /// `claim` licensed by a `Valid` from each of `seats` in the next block.
    fn license(&mut self, claim: Hash64, seats: &[(PalwBondKeyV2, Hash64)], bound: u64) {
        self.step(vec![PalwConsensusObjectV2::ReceiptLicensed { claim, receipts: seats.iter().map(|(k, _)| valid(claim, *k, bound)).collect() }]);
        assert!(matches!(self.c.claim(&claim).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the licence folds");
    }

}

fn snap(c: &Chain, stage: &'static str, ids: &[Hash64], refused: usize) -> Snap {
    let bond = bond_key(SUBJECT);
    let record = c.s.bond(&bond).expect("the subject bond").clone();
    let at = c.daa + 1;
    let raw = c.extras_at(at).settled_anchor_depth;
    let mut claims = BTreeMap::new();
    for id in ids {
        let Some(claim) = c.s.claim(id) else {
            claims.insert(
                *id,
                ClaimSnap {
                    phase: "retired".into(),
                    reserved: 0,
                    commitment: 0,
                    escrowed_reward: 0,
                    escrow_slot: 0,
                    duty: None,
                    locks: 0,
                    vesting: None,
                },
            );
            continue;
        };
        let phase = match &claim.phase {
            PalwClaimPhaseV2::Provisional => "Provisional".to_string(),
            PalwClaimPhaseV2::PanelBound { .. } => "PanelBound".to_string(),
            PalwClaimPhaseV2::ReceiptLicensed { .. } => "Licensed".to_string(),
            PalwClaimPhaseV2::Final { .. } => "Final".to_string(),
            PalwClaimPhaseV2::Voided { reason, .. } => format!("Voided({reason:?})"),
            PalwClaimPhaseV2::DefaultDisputed { .. } => "Disputed".to_string(),
        };
        let seats: Vec<PalwBondKeyV2> = c.s.panel(id).map(|panel| panel.seats.iter().map(|seat| seat.bond).collect()).unwrap_or_default();
        let locks = seats.iter().filter_map(|seat| c.s.slashable_lock(*seat, *id).map(|lock| lock.amount)).sum();
        claims.insert(
            *id,
            ClaimSnap {
                phase,
                reserved: claim.reserved,
                commitment: palw_claim_commitment_v1(&c.sp, claim, at).unwrap_or(0),
                escrowed_reward: claim.escrowed_reward,
                escrow_slot: c.sp.claim_escrow_term_v2(claim.accepted_daa, claim.escrowed_reward, &claim.class_id),
                duty: c.s.panel_duty_row_of(id).map(|row| row.seat_exposure),
                locks,
                vesting: c.s.vesting_row(id).map(|row| (row.producer.amount, row.seats.iter().map(|(_, leg)| leg.amount).sum(), row.reserve)),
            },
        );
    }
    let counters = c.s.vesting_counters();
    Snap {
        stage,
        daa: c.daa,
        accepted: ids.len(),
        refused,
        claims,
        bond_reserved_exposure: c.s.reserved_exposure(&bond),
        bond_committed: palw_bond_committed_raw_v1(&c.s, &c.sp, &bond, at, raw),
        bounded_immature: c.s.bounded_immature(),
        safe_weight: c.s.safe_weight(),
        collateral: record.collateral,
        slashed: record.slashed,
        freeze: c.s.bond_freeze_of_v1(&bond).map(|f| (f.final_, f.forfeited_sompi)),
        vesting_created: counters.created,
        vesting_burned: counters.burned,
        vesting_moved: counters.moved,
    }
}

/// The seats a panel of `class` binds: the floor's five genesis seats, or five ready genesis cards.
fn panel_seats(c: &Chain, model: Option<Hash64>) -> Vec<(PalwBondKeyV2, Hash64)> {
    match model {
        None => c.floor_seats(),
        Some(_) => honest_seats(&c.p, 5),
    }
}

/// `guilty_close` of the liab suite: a court's `ExecutorGuilty` close on an arithmetic proof (the fold
/// reads the verdict, never the proof).
fn guilty_close(session_id: Hash64) -> PalwConsensusObjectV2 {
    let PalwConsensusObjectV2::CourtClosed { proof, .. } = court_cleared(session_id) else { unreachable!("a close") };
    PalwConsensusObjectV2::CourtClosed { session_id, verdict: kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty, proof }
}

/// **The scripted scenario on both twins in lockstep**: every block is folded on the shipped and the
/// armed twin at the same DAA (a step one twin has nothing for is an empty block there), so their
/// snapshots line up stage by stage.
pub fn run_twins(class: Class, collateral_msk: u64) -> (Run, Run) {
    let mut sims = [Sim::new(params_for(class, false), class, collateral_msk), Sim::new(params_for(class, true), class, collateral_msk)];
    let windows = (sims[0].c.sp.window_bind(), sims[0].c.sp.window_receipt(), sims[0].c.sp.window_challenge_at(sims[0].c.daa));
    let mut snaps: [Vec<Snap>; 2] = [Vec::new(), Vec::new()];
    let mut ids: [Vec<Hash64>; 2] = [Vec::new(), Vec::new()];
    let mut refused = [0usize; 2];
    let snap_both = |sims: &[Sim; 2], snaps: &mut [Vec<Snap>; 2], stage: &'static str, ids: &[Vec<Hash64>; 2], refused: &[usize; 2]| {
        for t in 0..2 {
            snaps[t].push(snap(&sims[t].c, stage, &ids[t], refused[t]));
        }
    };
    // 1. The fill: four attempt blocks a DAA.
    let base = sims[0].c.daa;
    for k in 0..class.fill_blocks() {
        for t in 0..2 {
            match sims[t].block(base + 1 + k / 4, vec![], Some(0x5_1000 + k)) {
                Some(id) => ids[t].push(id),
                None => refused[t] += 1,
            }
        }
    }
    snap_both(&sims, &mut snaps, "filled", &ids, &refused);
    // 2. Bind, licence and Final the first (up to) three, one slot a block.
    let seats = panel_seats(&sims[0].c, sims[0].model);
    let mut bound_at: [Vec<(Hash64, u64)>; 2] = [Vec::new(), Vec::new()];
    for i in 0..3 {
        for t in 0..2 {
            match ids[t].get(i).copied() {
                Some(id) => bound_at[t].push((id, sims[t].bind(id, &seats))),
                None => sims[t].step(vec![]),
            }
        }
    }
    snap_both(&sims, &mut snaps, "bound", &ids, &refused);
    for i in 0..3 {
        for t in 0..2 {
            match bound_at[t].get(i).copied() {
                Some((id, bound)) => sims[t].license(id, &seats, bound),
                None => sims[t].step(vec![]),
            }
        }
    }
    snap_both(&sims, &mut snaps, "licensed", &ids, &refused);
    let final_daa = (0..2)
        .flat_map(|t| bound_at[t].iter().filter_map(|(id, _)| sims[t].c.s.deadline_of(id)).collect::<Vec<_>>())
        .max()
        .map_or(sims[0].c.daa + 1, |deadline| deadline + 1)
        .max(sims[0].c.daa + 1);
    for t in 0..2 {
        sims[t].block(final_daa, vec![], None);
        for (id, _) in &bound_at[t] {
            assert!(matches!(sims[t].c.claim(id).phase, PalwClaimPhaseV2::Final { .. }), "twin {t}: {id} Final at {final_daa}");
        }
    }
    snap_both(&sims, &mut snaps, "final", &ids, &refused);
    // 3. The unbound claims void BindTimeout; then past h_obl.
    let last_accept =
        (0..2).flat_map(|t| ids[t].iter().filter_map(|id| sims[t].c.s.claim(id).map(|claim| claim.accepted_daa)).collect::<Vec<_>>()).max();
    let voided = last_accept.map_or(0, |at| at + windows.0 + 2).max(sims[0].c.daa + 1);
    for sim in sims.iter_mut() {
        sim.block(voided, vec![], None);
    }
    snap_both(&sims, &mut snaps, "voided", &ids, &refused);
    for sim in sims.iter_mut() {
        sim.block(voided + windows.1 + 2, vec![], None);
    }
    snap_both(&sims, &mut snaps, "released", &ids, &refused);
    // 4. Four more attempt blocks; the first new claim licensed and convicted by a proven verdict.
    let mut more: [Vec<Hash64>; 2] = [Vec::new(), Vec::new()];
    for k in 0..4u64 {
        let daa = sims[0].c.daa + 1;
        for t in 0..2 {
            if let Some(id) = sims[t].block(daa, vec![], Some(0x5_2000 + k)) {
                more[t].push(id);
            }
        }
    }
    let targets: [Option<Hash64>; 2] = [more[0].first().copied(), more[1].first().copied()];
    let mut bound = [0u64; 2];
    for t in 0..2 {
        match targets[t] {
            Some(target) => bound[t] = sims[t].bind(target, &seats),
            None => sims[t].step(vec![]),
        }
    }
    for t in 0..2 {
        match targets[t] {
            Some(target) => sims[t].license(target, &seats, bound[t]),
            None => sims[t].step(vec![]),
        }
    }
    for t in 0..2 {
        let objects = targets[t].map(|target| vec![court_opened(&sims[t].c.s, target, bond_key(CHALLENGER))]).unwrap_or_default();
        sims[t].step(objects);
    }
    for t in 0..2 {
        let objects = targets[t].map(|target| vec![guilty_close(court_session_of(&sims[t].c.s, target, bond_key(CHALLENGER)))]).unwrap_or_default();
        sims[t].step(objects);
        if let Some(target) = targets[t] {
            assert!(
                matches!(sims[t].c.claim(&target).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }),
                "twin {t}: the verdict voids the claim"
            );
        }
    }
    let all: [Vec<Hash64>; 2] = [0, 1].map(|t| ids[t].iter().chain(more[t].iter()).copied().collect());
    snap_both(&sims, &mut snaps, "convicted", &all, &refused);
    let [shipped, armed] = sims;
    let [shipped_snaps, armed_snaps] = snaps;
    (Run { snaps: shipped_snaps, windows, skips: shipped.skips }, Run { snaps: armed_snaps, windows, skips: armed.skips })
}

/// **How one difference between the twins is classed.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// A difference ADR-0160 intends, with the section that intends it.
    Intended(&'static str),
    /// A consequence of the twins admitting different claim sets (the bond's totals over different
    /// claims), whose cause is the `claims.accepted` line's.
    CountDriven,
    /// A difference this stage-1 proof FOUND and reports (not intended by the ADR as written; see
    /// [`FINDINGS`]).
    Finding(&'static str),
    /// Anything else: a bug.
    Unexpected,
}

/// **The findings of stage 1's state diff** — differences the armed rules make at ρ = 1 that ADR-0160
/// (v1 + v3) does not intend, reported rather than silently allowed:
///
/// * **F1 — the Valid lock (and every `G`) reads the capped reservation.** L-1's lock is priced on
///   `G_res = palw_max_fraud_gain_v1(facts) − E + s`, whose weight term is `claim.reserved` (the
///   collateral-unit exposure, `palw_claim_exposure_pwu_v1`). F-W caps `claim.reserved` per bond
///   (`min(w, R_budget − held)`), so the lock loses the weight term while the fraud's Final weight
///   gain does not shrink (J-1 caps PROVISIONAL weight; a fraudulent `Final` still adds `W_full`).
///   Measured: an 8k Valid locks 178.25 instead of 359.42 MSK a seat; a 2M Valid 406.38 instead of
///   22,306.09 MSK a seat. v3 §5.6 (AS-1′) says an uncredited claim is priced exactly as today. The same
///   `G` feeds the S-tiers' `3G` and SA-7's court bound. Fix (lane D): freeze the uncapped `w` at
///   acceptance in the price record and price `G`'s weight term on `w · W_full / raw`.
/// * **F2 — the C7 ceiling clips 8k itself.** `PALW_CAPACITY_C7_WEIGHT_CEILING_V1` (138,892,697,241)
///   was derived as "8k's raw weight at testnet-12's launch W₀" with one attempt a claim; at the genesis
///   class target an 8k claim carries 20 attempts, raw 2,777,853,944,832, so F-W's D-3 ceiling cuts each
///   8k `Final` to 1/20 of today's safe weight (D-3 caps the 2M row AT 8k's raw weight, not 8k). Fix:
///   derive the ceiling from the 8k row's weight at the canonical work (or measure it on the live chain
///   and re-derive) before F-W arms.
pub const FINDINGS: [(&str, &str); 2] = [
    ("F1", "G's weight term reads the F-W-capped reservation: Valid locks / 3G / SA-7 fall for 8k and 2M"),
    ("F2", "the C7 ceiling (D-3) clips 8k's own Final weight at this class target"),
];

/// **The differences ADR-0160 intends at ρ = 1**, by component, with the direction it names.
pub fn intended(class: Class, stage: &str, component: &str, shipped: u128, armed: u128) -> Verdict {
    use Verdict::*;
    if stage == "convicted" {
        // A proven verdict past F-L forfeits the whole bond (AG-2): collateral, the commitments of its
        // claims (voided AggregateForfeit, their weight with them), its own unmatured legs (burned), and
        // a final freeze (AG-3). Decision 4 keeps the reporter reward on the tier.
        if matches!(
            component,
            "bond.collateral"
                | "bond.slashed"
                | "bond.freeze"
                | "claim.phase"
                | "vesting.burned"
                | "claim.vesting"
                | "bond.reserved_exposure"
                | "bond.committed"
                | "claim.commitment"
                | "weights.bounded_immature"
        ) {
            return Intended("v3 §5.5 AG-2/AG-3: an intent-class conviction forfeits the whole bond and freezes it for good");
        }
    }
    match component {
        "claim.reserved" if armed <= shipped => Intended("v3 §5.1 / v1 §4.1: J-1's capped reservation min(w, R_budget − held)"),
        "claim.commitment" | "bond.reserved_exposure" | "bond.committed" if stage == "voided" && armed > shipped => {
            Intended("v3 §5.4 / v1 §4.4 E-4: an unconvicted void keeps m_c + reserved for h_obl")
        }
        "claim.commitment" | "bond.reserved_exposure" | "bond.committed" if armed < shipped => {
            Intended("v3 §5.1: the capped reservation in the commitment")
        }
        "weights.bounded_immature" if armed <= shipped => Intended("v3 §5.2 / v1 §4.2: staged weight and W_cap (J-1)"),
        "weights.safe_weight" if armed <= shipped && class == Class::M2 => Intended("v3 §5.2, D-3: the C7 Final ceiling (8k raw weight)"),
        "weights.safe_weight" if armed <= shipped && class == Class::K8 => Finding("F2"),
        "claims.accepted" if armed >= shipped && class != Class::Floor => Intended(
            "v3 §5.1 J-1 reservation (w capped) and v1 §7.4 V2 room v2 (k = 2, measured speed, stake share); D-13 keeps 2M at 1",
        ),
        "claims.refused" if armed <= shipped && class != Class::Floor => Intended("(the refusals the extra admissions replace)"),
        "claim.duty" if armed <= shipped => Intended("v3 §5.6 AS-1: the duty is capped by the (capped) commitment over the seats"),
        "claim.locks" if armed < shipped => Finding("F1"),
        _ => Unexpected,
    }
}

/// One line of the diff.
#[derive(Clone, Debug)]
pub struct Diff {
    pub stage: &'static str,
    pub component: String,
    pub shipped: String,
    pub armed: String,
    pub verdict: Verdict,
}

fn claim_value(c: &ClaimSnap, component: &str) -> u128 {
    match component {
        "claim.reserved" => c.reserved,
        "claim.commitment" => c.commitment,
        "claim.escrowed_reward" => u128::from(c.escrowed_reward),
        "claim.escrow_slot" => c.escrow_slot,
        "claim.duty" => c.duty.unwrap_or(0),
        "claim.locks" => c.locks,
        _ => unreachable!("{component}"),
    }
}

/// Every difference between the two runs' snapshots, stage by stage, each classed by [`intended`].
/// Bond-level totals are compared as rule differences only where the twins hold the same claims; where
/// they admitted different sets the totals' differences are [`Verdict::CountDriven`].
pub fn diff(class: Class, shipped: &Run, armed: &Run) -> Vec<Diff> {
    let mut out = Vec::new();
    for (s, a) in shipped.snaps.iter().zip(&armed.snaps) {
        assert_eq!((s.stage, s.daa), (a.stage, a.daa), "the twins fold the same stages at the same DAAs");
        let same_claims = s.claims.keys().eq(a.claims.keys());
        let mut scalar = |component: &str, sv: u128, av: u128, bond_total: bool| {
            if sv != av {
                let verdict = match intended(class, s.stage, component, sv, av) {
                    Verdict::Unexpected | Verdict::Finding(_) if bond_total && !same_claims => Verdict::CountDriven,
                    verdict => verdict,
                };
                out.push(Diff { stage: s.stage, component: component.to_string(), shipped: sv.to_string(), armed: av.to_string(), verdict });
            }
        };
        if s.stage == "filled" || s.stage == "convicted" {
            scalar("claims.accepted", s.accepted as u128, a.accepted as u128, false);
            scalar("claims.refused", s.refused as u128, a.refused as u128, false);
        }
        scalar("bond.reserved_exposure", s.bond_reserved_exposure, a.bond_reserved_exposure, true);
        scalar("bond.committed", s.bond_committed, a.bond_committed, true);
        scalar("weights.bounded_immature", s.bounded_immature, a.bounded_immature, true);
        scalar("weights.safe_weight", s.safe_weight, a.safe_weight, !same_claims && class != Class::K8);
        scalar("bond.collateral", u128::from(s.collateral), u128::from(a.collateral), false);
        scalar("bond.slashed", u128::from(s.slashed), u128::from(a.slashed), false);
        scalar("vesting.created", s.vesting_created, a.vesting_created, true);
        scalar("vesting.burned", s.vesting_burned, a.vesting_burned, true);
        scalar("vesting.moved", s.vesting_moved, a.vesting_moved, true);
        if s.freeze != a.freeze {
            out.push(Diff {
                stage: s.stage,
                component: "bond.freeze".into(),
                shipped: format!("{:?}", s.freeze),
                armed: format!("{:?}", a.freeze),
                verdict: intended(class, s.stage, "bond.freeze", 0, 0),
            });
        }
        // Per claim, over the claims both twins hold.
        let mut per: BTreeMap<String, (u128, u128, usize, Verdict)> = BTreeMap::new();
        for (id, sc) in &s.claims {
            let Some(ac) = a.claims.get(id) else { continue };
            for component in ["claim.reserved", "claim.commitment", "claim.escrowed_reward", "claim.escrow_slot", "claim.duty", "claim.locks"] {
                let (sv, av) = (claim_value(sc, component), claim_value(ac, component));
                if sv != av {
                    let verdict = intended(class, s.stage, component, sv, av);
                    let entry = per.entry(component.to_string()).or_insert((sv, av, 0, verdict));
                    entry.2 += 1;
                    if verdict == Verdict::Unexpected {
                        entry.3 = Verdict::Unexpected;
                    }
                }
            }
            if sc.phase != ac.phase {
                let verdict = intended(class, s.stage, "claim.phase", 0, 0);
                per.entry(format!("claim.phase {} -> {}", sc.phase, ac.phase)).or_insert((0, 0, 0, verdict)).2 += 1;
            }
            if sc.vesting != ac.vesting {
                let verdict = intended(class, s.stage, "claim.vesting", 0, 0);
                per.entry("claim.vesting".to_string()).or_insert((0, 0, 0, verdict)).2 += 1;
            }
        }
        for (component, (sv, av, n, verdict)) in per {
            let dash = component.starts_with("claim.phase") || component == "claim.vesting";
            out.push(Diff {
                stage: s.stage,
                component: format!("{component} (×{n})"),
                shipped: if dash { "-".into() } else { format!("{sv}") },
                armed: if dash { "-".into() } else { format!("{av}") },
                verdict,
            });
        }
    }
    out
}

/// The whole table for one (class, bond size): the two runs and their classed diff.
pub fn compare(class: Class, collateral_msk: u64) -> (Run, Run, Vec<Diff>) {
    let (shipped, armed) = run_twins(class, collateral_msk);
    let d = diff(class, &shipped, &armed);
    (shipped, armed, d)
}

fn render(class: Class, collateral_msk: u64, shipped: &Run, armed: &Run, d: &[Diff]) -> String {
    let mut s = String::new();
    let (sf, af) = (&shipped.snaps[0], &armed.snaps[0]);
    let _ = writeln!(
        s,
        "== {} @ {} MSK: accepted shipped {} / armed {}; refused {} / {}; windows bind/receipt/challenge {:?}",
        class.label(),
        collateral_msk,
        sf.accepted,
        af.accepted,
        sf.refused,
        af.refused,
        shipped.windows,
    );
    let _ = writeln!(s, "   skips shipped {:?}\n   skips armed   {:?}", shipped.skips, armed.skips);
    for line in d {
        let _ = writeln!(
            s,
            "   [{:<9}] {:<46} shipped {:>20}  armed {:>20}  {}",
            line.stage,
            line.component,
            line.shipped,
            line.armed,
            match line.verdict {
                Verdict::Intended(why) => format!("INTENDED {why}"),
                Verdict::CountDriven => "COUNT-DRIVEN (the claims.accepted line's cause)".to_string(),
                Verdict::Finding(id) => format!("FINDING {id}"),
                Verdict::Unexpected => "UNEXPECTED".to_string(),
            }
        );
    }
    s
}

/// **Stage 1's gate (a): the ρ = 1 state diff**, over {floor, 8k, 2M} × {13k, 100k, 1M}. Every
/// difference is classed; the test fails on any `Unexpected` one and prints the table with
/// `--nocapture`. Pinned beside it: the escrow slot is `E` on both twins at every stage short of the
/// conviction (ρ = 1 credits nothing); the floor admits exactly as many claims on both twins; the 2M row
/// never holds more than one claim (D-13's C7 cap); the armed bond's live weight never exceeds its
/// `W_cap` (J-1); and the findings the diff reports are exactly [`FINDINGS`] (so a fixed finding, or a
/// new one, changes this test).
#[test]
fn stage1_the_rho_1_state_diff_shows_only_intended_differences() {
    let mut report = String::new();
    let mut unexpected = Vec::new();
    let mut found = std::collections::BTreeSet::new();
    for class in [Class::Floor, Class::K8, Class::M2] {
        for collateral in BONDS {
            let (shipped, armed, d) = compare(class, collateral);
            report.push_str(&render(class, collateral, &shipped, &armed, &d));
            for line in &d {
                match line.verdict {
                    Verdict::Unexpected => unexpected.push(format!(
                        "{} @ {collateral}: [{}] {} ({} -> {})",
                        class.label(),
                        line.stage,
                        line.component,
                        line.shipped,
                        line.armed
                    )),
                    Verdict::Finding(id) => {
                        found.insert(id);
                    }
                    _ => {}
                }
            }
            if class == Class::Floor {
                assert_eq!(shipped.snaps[0].accepted, armed.snaps[0].accepted, "floor @ {collateral}: no claim-count increase at ρ = 1");
            }
            if class == Class::M2 {
                for run in [&shipped, &armed] {
                    let live = run.snaps[0].claims.values().filter(|c| !c.phase.starts_with("Voided") && c.phase != "retired").count();
                    assert!(live <= 1, "2M @ {collateral}: D-13 keeps the C7 cap of one");
                }
            }
            let w_cap = kaspa_consensus_core::palw_weight_cap_v1::palw_bond_weight_cap_v1(collateral * MSK);
            for (s, a) in shipped.snaps.iter().zip(&armed.snaps) {
                if s.stage == "convicted" {
                    continue;
                }
                for (id, sc) in &s.claims {
                    if let Some(ac) = a.claims.get(id) {
                        assert_eq!(sc.escrow_slot, ac.escrow_slot, "{} @ {collateral} [{}]: m_c = E at ρ = 1", class.label(), s.stage);
                    }
                }
                // The subject is the only bond with claims, so the armed twin's live weight IS its term.
                assert!(a.bounded_immature <= w_cap, "{} @ {collateral} [{}]: J-1, {} ≤ W_cap {w_cap}", class.label(), s.stage, a.bounded_immature);
            }
        }
    }
    println!("{report}");
    assert!(unexpected.is_empty(), "differences ADR-0160 does not intend:\n{}", unexpected.join("\n"));
    assert_eq!(found.into_iter().collect::<Vec<_>>(), FINDINGS.iter().map(|(id, _)| *id).collect::<Vec<_>>(), "the reported findings");
}
