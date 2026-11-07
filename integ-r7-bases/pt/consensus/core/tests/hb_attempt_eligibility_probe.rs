//! **Does a DAA lead on a private branch buy attempt eligibility?** (2026-09-25, the user's refined
//! threat model: future-stamped heartbeats -> the branch's DAA runs ahead of wall time -> MORE
//! attempt blocks per wall-clock second than the honest branch can have -> +2^20 blue work each.)
//!
//! Everything below is testnet-12's own `Params`, its bundle's `PalwStateParamsV2`, its genesis
//! state folded as `process_genesis` folds it, and the real fold (`apply_palw_transition_v7`,
//! through `dos_l5_common`). Nothing is a restated rule; the lead `L = ⌊T / I⌋ + 1` is the burst the
//! mainnet-values review measured (`t12_a_run_ahead_burst_outlasts_a_readiness_row`).
//!
//! * **A** — the gates' units, read off the preset: the lottery's two targets are chain state that
//!   moves only at an epoch boundary (the floor's class target, and past ADR-0137 the models' work
//!   ticket `MAX · min(1, CCU / W)`, which at testnet-12's `W₀` admits EVERY model forward), the work
//!   target's step counts a DAA-denominated expectation and sits at its floor `W₀` at launch, and a
//!   bond's capacity is `K` concurrent floor claims (146 on a genesis bond, 2 on a registration-floor
//!   bond) — each reserves its escrow until the claim is LICENSED or voided.
//! * **B** — the real fold, one greedy producer, the same wall-clock horizon with and without a lead.
//!   A compute-bound producer gains exactly 0. A capacity-bound one gains exactly the claims in flight
//!   whose anchor slot (`accepted + anchor_delay`, a DAA) the lead reaches early — only when those
//!   slots release anything on the private branch (the draw binds nobody, or a colluding quorum
//!   licenses at once); an honest panel, which never sees the branch, releases nothing. At most one
//!   capacity `K` at any reveal, and a phase shift that recurs rather than grows: never a rate.
//! * **C** — the one fork-choice comparator at equal wall time: the lead finalizes PRE-FORK licensed
//!   claims early (`Final` = licence + `window_challenge`, a DAA deadline), so the led branch is ahead
//!   on the matured-work keys (safe frontier, safe weight) with no attempt of its own, and
//!   `decide_deep_reorg_v2` allows it whatever immature weight the incumbent adds.
//!
//! Run: cargo test -p kaspa-consensus-core --test hb_attempt_eligibility_probe -- --nocapture --test-threads=1

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_fork_authority_v2::{PalwDeepReorgV2, decide_deep_reorg_v2};
use kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1;
use kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS as I_MS;
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateParamsV2,
    apply_palw_transition_v7, palw_work_floor_for_block_v1,
};
use kaspa_consensus_core::palw_work_target_v1::{
    PALW_WORK_TARGET_SHADOW_RATE_SOMPI_PER_GIGA_V1, palw_work_target_step_v1, palw_work_ticket_target_v1,
};

/// `⌊T / I⌋ + 1`: the clock-stepping beats one producer can stamp at their slots with nothing
/// between them, every stamp at most `now + T` (`t12_run_ahead_burst_vs_readiness.rs::burst`).
fn lead(tolerance_s: u64) -> u64 {
    tolerance_s * 1_000 / I_MS + 1
}

const ATTACKER: u64 = 0xE1;

// =================================================================================================
// A — the gates' units
// =================================================================================================

#[test]
fn a_every_attempt_gate_and_the_clock_it_reads() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let g = genesis_state(&p);
    let classes = genesis_classes(&p);
    let (floor, floor_leaves, _, _) = classes[0];

    let t = p.timestamp_deviation_tolerance;
    println!("=== the lead ===");
    println!("timestamp_deviation_tolerance T = {t} s, heartbeat interval I = {} s", I_MS / 1000);
    println!("L(T = {t} s) = {}   L(132 s, the beat-future-cap / old tolerance) = {}", lead(t), lead(132));
    assert_eq!(t, 1_620, "testnet-12 runs mainnet's tolerance");
    assert_eq!((lead(1_620), lead(132)), (14, 2));

    // ---- the lottery: the floor's class target and the models' work ticket are chain STATE ------
    let floor_target = g.class_target(&floor).expect("the floor has a class target").target;
    let draws = palw_expected_attempts_v1(floor_target);
    let epoch = sp.epoch_length();
    let split = sp.fp_attempt_share_permille();
    let max_factor = sp.class_daa_max_factor();
    let expected_model_blocks = epoch * split as u64 / 1_000;
    println!();
    println!("=== the lottery (per inference; the anchor is H(network, pre_pow_hash, class, bond, bucket)) ===");
    println!("floor class target = {floor_target:#x} -> expected draws per floor win = {draws}");
    println!("epoch_length = {epoch} DAA, fp_attempt_share = {split} permille, class_daa_max_factor = {max_factor}");
    println!("work-target expectation per epoch = epoch x split = {expected_model_blocks} model blocks (a DAA count)");

    // W₀ for an attempt block (its escrow at rate_max) and the genesis model rows' tickets at it.
    let rate = p.palw_economic_payout.map(|e| e.rate_sompi_per_giga).unwrap_or(PALW_WORK_TARGET_SHADOW_RATE_SOMPI_PER_GIGA_V1);
    let w0 = palw_work_floor_for_block_v1(&sp, T12_BLOCK_SUBSIDY_SOMPI, extras(&p, 1_000).escrow_carve, rate);
    println!("W0 = escrow / rate_max = {w0} CCU (rate {rate} sompi per G)");
    let works = registry_fold(&p, 1_000).expect("the registry is armed on testnet-12").genesis_works;
    for (id, leaves, _, _) in classes.iter().skip(1) {
        let ccu = works.get(id).map(|w| w.economic_ccu_per_claim).unwrap_or(0);
        let ticket = palw_work_ticket_target_v1(ccu, w0);
        println!(
            "model row {id} (leaves {leaves}): CCU/claim = {ccu}, ticket at W0 = {} of the space -> {} forwards per win",
            if ticket == u128::MAX { "ALL".to_string() } else { format!("{:.3e}", ticket as f64 / u128::MAX as f64) },
            if ticket == u128::MAX { 1 } else { palw_expected_attempts_v1(ticket) }
        );
    }

    // The work target's step at launch: W starts at W₀ (`apply_work_target`: `unwrap_or(floor)`), and
    // the step is floored at W₀, so an epoch the models under-fill — a SHORTER epoch in wall time,
    // which is all a DAA lead can make — cannot ease the lottery at all.
    for model_blocks in [0, 1, expected_model_blocks / 2, expected_model_blocks.saturating_sub(1)] {
        assert_eq!(
            palw_work_target_step_v1(w0, w0, model_blocks, expected_model_blocks, max_factor),
            w0,
            "at W = W0 the step cannot go below W0 whatever the closed epoch produced ({model_blocks})"
        );
    }
    // Above the floor (the controller active) a lead of L shortens the closed epoch's wall time by
    // L x I, i.e. a compute-bound producer's count by L/E: a one-off easing of that ratio.
    let w_hi = w0.saturating_mul(3);
    let m = expected_model_blocks;
    let m_led = m * (epoch - lead(t)) / epoch;
    let eased = palw_work_target_step_v1(w_hi, w0, m_led, expected_model_blocks, max_factor);
    println!(
        "controller ACTIVE (W = 3 W0): a closed epoch that met its expectation steps W to {w_hi}; the same compute with a {}-DAA lead \
         counts {m_led}/{m} and steps W to {eased} (x{:.4}) — the next epoch's model tickets are that much easier, once",
        lead(t),
        eased as f64 / w_hi as f64
    );

    // ---- capacity: what a genesis bond can hold at once -----------------------------------------
    let pwu = palw_pwu_v1(floor_target, floor_leaves);
    let per = admitted_per_attempt(&p, floor, pwu, T12_BLOCK_SUBSIDY_SOMPI);
    let genesis_collateral = genesis_bonds(&p)[0].2;
    let k_genesis = genesis_collateral as u128 * per.ratio_permille as u128 / 1_000 / per.reservation;
    let min_bond = registration_floor(&p, 1_000);
    let k_min = min_bond as u128 * per.ratio_permille as u128 / 1_000 / per.reservation;
    println!();
    println!("=== capacity (the fold's own reservation, option A) ===");
    println!(
        "one concurrent floor claim reserves {:.2} MSK (weight {:.4} + escrow {:.2}); ceiling {} permille of collateral",
        msk(per.reservation),
        msk(per.reserved),
        msk(per.escrow),
        per.ratio_permille
    );
    println!("genesis bond {:.2} MSK -> K = {k_genesis} concurrent floor claims", msk(genesis_collateral as u128));
    println!("a registration-floor bond {:.2} MSK -> K = {k_min}", msk(min_bond as u128));

    // ---- the windows a claim's reservation is released by ---------------------------------------
    let anchor_delay = b.panel.anchor_delay();
    println!();
    println!("=== windows (DAA; one DAA = one heartbeat slot = I) ===");
    println!(
        "anchor_delay {anchor_delay}, window_bind {}, window_receipt {}, window_challenge {}, window_court {}",
        sp.window_bind(),
        sp.window_receipt(),
        sp.window_challenge_at(1_000),
        sp.window_court()
    );
    println!(
        "claim occupancy: anchor-block void (draw fails) ~{anchor_delay}; bound, no receipts: first ReceiptTimeout ~{} then a redraw; \
         licensed: Final at licence + {}",
        anchor_delay + sp.window_receipt(),
        sp.window_challenge_at(1_000)
    );
    println!("ghostdag_k = {}", p.ghostdag_k);
    assert!(k_genesis >= 1);
}

// =================================================================================================
// B + C — the real fold, with and without a lead
// =================================================================================================

/// A chain the probe drives: the state, and the next DAA / blue score / block id it will fold at.
#[derive(Clone)]
struct Branch {
    s: PalwChainStateV2,
    daa: u64,
    blue: u64,
    block: u64,
}

struct Rig {
    p: Params,
    sp: PalwStateParamsV2,
    floor: Hash64,
    pwu: u64,
    seats: Vec<(PalwBondKeyV2, Hash64)>,
    anchor_delay: u64,
}

impl Rig {
    fn new() -> Self {
        let p = t12();
        let b = bundle(&p);
        let sp = b.state.clone();
        let (floor, leaves, target, _) = genesis_classes(&p)[0];
        let pwu = palw_pwu_v1(target, leaves);
        let seats: Vec<(PalwBondKeyV2, Hash64)> = genesis_bonds(&p)[1..6].iter().map(|(k, o, _)| (*k, *o)).collect();
        let anchor_delay = b.panel.anchor_delay();
        Rig { p, sp, floor, pwu, seats, anchor_delay }
    }

    /// One block through the real fold. `anchor_block` mirrors the processor's
    /// `palw_sw8_anchor_delay_for`: past `palw_rcore_plus` an ATTEMPT block may anchor a panel, and the
    /// fold's step 4c then voids every `Provisional` claim whose slot it reached and did not bind.
    fn fold(
        &self,
        br: &mut Branch,
        daa: u64,
        objects: &[PalwConsensusObjectV2],
        work: PalwBlockWorkV3<'_>,
        key: Hash64,
        anchor_block: bool,
    ) {
        assert!(daa >= br.daa);
        br.daa = daa;
        br.blue += 1;
        br.block += 1;
        let subsidy = if matches!(work, PalwBlockWorkV3::Attempt(_)) { T12_BLOCK_SUBSIDY_SOMPI } else { 0 };
        let f = flags(&self.p, daa);
        let mut x = extras(&self.p, daa);
        if anchor_block {
            x.sw8_anchor_delay = Some(self.anchor_delay);
        }
        let (next, _, _) = apply_palw_transition_v7(
            &br.s,
            &self.sp,
            None,
            &ctx(br.block, daa, br.blue, subsidy),
            objects,
            work,
            &[],
            key,
            f.unavailable_abstains,
            f.capability_bound,
            f.uncertified_weightless,
            f.da_court,
            &x,
        )
        .unwrap_or_else(|e| panic!("block at DAA {daa} folds: {e:?}"));
        br.s = next;
    }

    /// A claimless block (the heartbeat lane's stand-in tick) at `daa`.
    fn beat(&self, br: &mut Branch, daa: u64, objects: &[PalwConsensusObjectV2]) {
        self.fold(br, daa, objects, PalwBlockWorkV3::None, Hash64::default(), false);
    }

    /// The attacker's attempt block at the branch's current DAA; `true` iff the fold recorded the claim
    /// (a bond at its ceiling has it skipped: `AttemptExposureCeiling`). `anchor_block` as in [`Self::fold`].
    fn attempt(&self, br: &mut Branch, bond: u64, seed: u64, anchor_block: bool) -> bool {
        let (env, key, id) =
            junk_attempt(self.floor, bond_key(bond), pubkey_of(bond), &operator_pubkey_of(bond), self.pwu, seed, 0xE1_0000 + seed);
        let daa = br.daa;
        self.fold(br, daa, &[], PalwBlockWorkV3::Attempt(&env), key, anchor_block);
        br.s.claim(&id).is_some()
    }

    /// One claim of `bond` walked to `ReceiptLicensed` (attempt, bind, licence — one DAA each).
    fn license_one(&self, br: &mut Branch, bond: u64, seed: u64) -> Hash64 {
        let (env, key, id) =
            junk_attempt(self.floor, bond_key(bond), pubkey_of(bond), &operator_pubkey_of(bond), self.pwu, seed, 0xE1_0000 + seed);
        let d = br.daa + 1;
        self.fold(br, d, &[], PalwBlockWorkV3::Attempt(&env), key, false);
        assert!(br.s.claim(&id).is_some(), "pre-fork claim {seed} admitted");
        let bound = PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xB0C0 + seed), seats: seats_of(&self.seats) };
        let d = br.daa + 1;
        self.beat(br, d, &[bound]);
        let receipts = valid_receipts(id, &self.seats.iter().map(|s| s.0).collect::<Vec<_>>());
        let d = br.daa + 1;
        self.beat(br, d, &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
        assert!(matches!(br.s.claim(&id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "claim {seed} licensed");
        id
    }
}

/// How the attacker's post-fork claims leave flight on its private branch — every path is keyed on
/// the claim's anchor slot `accepted_daa + anchor_delay`, a DAA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Release {
    /// The draw at the anchor block binds nobody (SW-10, or no seat with room): the fold's step 4c
    /// voids the claim in that block, S0 — the reservation comes back, nothing is forfeited.
    AnchorVoid,
    /// The anchor binds seats that answer at once (a colluding quorum): licensed one step later, and
    /// the licence releases the producer's reservation.
    ColludingPanel,
    /// The anchor binds seats that never see the private branch: nothing comes back before the receipt
    /// window (600 DAA) lapses, and then a redraw holds it again.
    SilentPanel,
}

/// The objects a step's claimless block carries for `mode`: a binding for every `bond` claim whose
/// anchor slot `daa` reached, and (colluding) a licence for every claim bound on an earlier step.
fn settle(rig: &Rig, br: &Branch, bond: u64, daa: u64, mode: Release) -> Vec<PalwConsensusObjectV2> {
    if mode == Release::AnchorVoid {
        return Vec::new();
    }
    let mut objects = Vec::new();
    let mut due: Vec<(Hash64, bool)> =
        br.s.claims_iter()
            .filter(|(_, c)| c.bond == bond_key(bond))
            .filter_map(|(id, c)| match c.phase {
                PalwClaimPhaseV2::Provisional if c.accepted_daa + rig.anchor_delay <= daa => Some((*id, false)),
                PalwClaimPhaseV2::PanelBound { .. } if mode == Release::ColludingPanel => Some((*id, true)),
                _ => None,
            })
            .collect();
    due.sort();
    for (id, bound) in due {
        if bound {
            let receipts = valid_receipts(id, &rig.seats.iter().map(|s| s.0).collect::<Vec<_>>());
            objects.push(PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts });
        } else {
            objects.push(PalwConsensusObjectV2::PanelBound {
                claim: id,
                anchor: h(0xB1D0_0000 ^ br.block),
                seats: seats_of(&rig.seats),
            });
        }
    }
    objects
}

/// `steps` wall steps of one greedy producer. Wall step `w` advances the DAA by one (the clock's one
/// slot per `I` of wall time) and step 1 by `1 + lead` — the burst of `lead` future-stamped clock
/// steps. Then the producer tries `r` attempt blocks at that DAA (its compute: `r` wins per wall
/// step). Returns the admissions per step.
fn run(rig: &Rig, start: &Branch, bond: u64, lead: u64, steps: u64, r: u64, mode: Release, seed0: u64) -> (Vec<u64>, Branch) {
    let mut br = start.clone();
    let mut per_step = Vec::new();
    let mut seed = seed0;
    for w in 1..=steps {
        let daa = br.daa + 1 + if w == 1 { lead } else { 0 };
        let objects = settle(rig, &br, bond, daa, mode);
        rig.beat(&mut br, daa, &objects);
        let mut admitted = 0;
        for _ in 0..r {
            seed += 1;
            admitted += u64::from(rig.attempt(&mut br, bond, seed, mode == Release::AnchorVoid));
        }
        per_step.push(admitted);
    }
    (per_step, br)
}

#[test]
fn b_a_daa_lead_buys_only_the_releases_it_pulls_forward_and_only_once() {
    let rig = Rig::new();
    let per = admitted_per_attempt(&rig.p, rig.floor, rig.pwu, T12_BLOCK_SUBSIDY_SOMPI);
    const K: u64 = 16; // concurrent floor claims the attacker bond backs (a genesis bond backs 146)
    const PREFIX: u64 = 60; // wall steps of steady production before the fork
    const HORIZON: u64 = 40; // wall steps after it: 80 minutes
    const R: u64 = 3; // the attacker's wins per wall step (the live ~3 floor attempts per DAA)
    const ATT: u64 = ATTACKER;
    const RICH: u64 = ATTACKER + 1;

    let g = genesis_state(&rig.p);
    let mut base = Branch { s: g, daa: 0, blue: 0, block: 0x4000 };
    rig.beat(
        &mut base,
        1_000,
        &[
            bond_obj(ATT, at_least_the_floor(&rig.p, per.collateral * K)),
            bond_obj(RICH, at_least_the_floor(&rig.p, per.collateral * 10_000)),
        ],
    );
    // The ceiling is where the capacity-bound producer lives: K fit, the (K+1)-th is skipped.
    let mut probe = base.clone();
    let d = probe.daa + 1;
    rig.beat(&mut probe, d, &[]);
    let fit = (0..K + 5).map(|i| rig.attempt(&mut probe, ATT, 0xF1_0000 + i, false)).filter(|ok| *ok).count() as u64;
    println!(
        "attacker bond {:.2} MSK backs {fit} concurrent floor claims (asked for K = {K})",
        msk(per.collateral as u128 * K as u128)
    );
    assert_eq!(fit, K);

    // The lead's gain at wall time t is the cumulative admissions of the led branch minus the unled
    // one's at the same t. The attacker picks its reveal, so the figure is the MAXIMUM over t — and
    // over where in the release wave it forks (three fork phases). "One-off" is that the maximum over
    // the second half of a long horizon is no larger than over the first.
    println!();
    println!(
        "K = {K}, producer tries {R} attempt blocks a wall step, fork after {PREFIX}/+7/+14 steps of steady production, {HORIZON} x 2 steps per variant"
    );
    println!(
        "{:<16} {:>4} | {:>28} | {:>28} | {:>12}",
        "release path", "lead", "max gain (step) over 1..40", "max gain (step) over 41..80", "end gain"
    );
    let mut results = Vec::new();
    for mode in [Release::AnchorVoid, Release::ColludingPanel, Release::SilentPanel] {
        for l in [lead(132), lead(1_620)] {
            let mut best = (i64::MIN, 0usize, i64::MIN, 0usize, 0i64);
            for phase in [0u64, 7, 14] {
                let (_, fork) = run(&rig, &base, ATT, 0, PREFIX + phase, R, mode, 0x10_0000);
                let (none, _) = run(&rig, &fork, ATT, 0, 2 * HORIZON, R, mode, 0x20_0000);
                let (led, _) = run(&rig, &fork, ATT, l, 2 * HORIZON, R, mode, 0x20_0000);
                let mut diff = Vec::new();
                let (mut a, mut b) = (0i64, 0i64);
                for (x, y) in led.iter().zip(none.iter()) {
                    a += *x as i64;
                    b += *y as i64;
                    diff.push(a - b);
                }
                let h = HORIZON as usize;
                let (i1, m1) =
                    diff[..h].iter().enumerate().max_by_key(|(i, d)| (**d, usize::MAX - i)).map(|(i, d)| (i + 1, *d)).unwrap();
                let (i2, m2) =
                    diff[h..].iter().enumerate().max_by_key(|(i, d)| (**d, usize::MAX - i)).map(|(i, d)| (i + 1 + h, *d)).unwrap();
                if m1 > best.0 {
                    best = (m1, i1, m2, i2, *diff.last().unwrap());
                }
            }
            println!(
                "{:<16} {:>4} | {:>21} ({:>3}) | {:>21} ({:>3}) | {:>12}",
                format!("{mode:?}"),
                l,
                best.0,
                best.1,
                best.2,
                best.3,
                best.4
            );
            results.push((mode, l, best.0, best.2));
        }
    }
    // The compute-bound control: the same producer with room for every win.
    let (_, rich_fork) = run(&rig, &base, RICH, 0, PREFIX, R, Release::AnchorVoid, 0x30_0000);
    let rich: Vec<u64> = [0, lead(132), lead(1_620)]
        .iter()
        .map(|l| run(&rig, &rich_fork, RICH, *l, HORIZON, R, Release::AnchorVoid, 0x40_0000).0.iter().sum())
        .collect();
    println!("compute-bound control (room for everything): admitted {rich:?} at leads 0 / 2 / 14 (of {} tries)", HORIZON * R);
    assert_eq!(rich, vec![HORIZON * R; 3], "compute-bound: a DAA lead buys exactly 0 attempts");

    for (mode, l, first, second) in &results {
        assert!(*first <= K as i64 && *second <= K as i64, "{mode:?} lead {l}: the gain is at most one capacity K");
        assert!(second <= first, "{mode:?} lead {l}: one-off — the second half gains no more than the first ({first} vs {second})");
        if *mode == Release::SilentPanel {
            assert_eq!((*first, *second), (0, 0), "an honest panel holds the claims past every horizon: the lead releases nothing");
        }
    }
    let gain = |m: Release, l: u64| results.iter().find(|r| r.0 == m && r.1 == l).unwrap().2;
    assert!(gain(Release::AnchorVoid, 14) >= 1 && gain(Release::ColludingPanel, 14) >= 1, "a 14-DAA lead pulls anchor slots forward");
    println!();
    println!(
        "=> max gain at a chosen reveal, K = {K}: AnchorVoid {} / {}, ColludingPanel {} / {}, SilentPanel {} / {} at L = 2 / 14",
        gain(Release::AnchorVoid, 2),
        gain(Release::AnchorVoid, 14),
        gain(Release::ColludingPanel, 2),
        gain(Release::ColludingPanel, 14),
        gain(Release::SilentPanel, 2),
        gain(Release::SilentPanel, 14)
    );
}

// =================================================================================================
// C — the same clock read by the one fork-choice comparator
// =================================================================================================

#[test]
fn c_a_daa_lead_finalizes_pre_fork_claims_early_and_the_comparator_follows_it() {
    let rig = Rig::new();
    let per = admitted_per_attempt(&rig.p, rig.floor, rig.pwu, T12_BLOCK_SUBSIDY_SOMPI);
    const N: u64 = 16; // pre-fork licensed claims, one per 3 DAA (a producer every 6 minutes)
    const HORIZON: u64 = 30;
    const EXEC: u64 = 0xEC;

    let g = genesis_state(&rig.p);
    let mut br = Branch { s: g, daa: 0, blue: 0, block: 0x8000 };
    rig.beat(&mut br, 1_000, &[bond_obj(EXEC, at_least_the_floor(&rig.p, per.collateral * N))]);
    let ids: Vec<Hash64> = (0..N).map(|i| rig.license_one(&mut br, EXEC, 0xF0_0000 + i)).collect();
    let licensed: Vec<u64> = ids
        .iter()
        .map(|id| match br.s.claim(id).unwrap().phase {
            PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => licensed_daa,
            ref other => panic!("{other:?}"),
        })
        .collect();
    let window = rig.sp.window_challenge_at(licensed[0]);
    // The attacker forks 3 DAA before the first licensed claim's Final (its deadline is public).
    let fork_daa = licensed[0] + window - 3;
    rig.beat(&mut br, fork_daa, &[]);
    assert!(ids.iter().all(|id| !matches!(br.s.claim(id).unwrap().phase, PalwClaimPhaseV2::Final { .. })));
    println!(
        "{N} pre-fork claims licensed at DAA {:?}..{:?}; Final at licence + {window}; fork at DAA {fork_daa}",
        licensed[0],
        licensed[N as usize - 1]
    );

    // Both branches carry NO attempt after the fork: only the clock differs.
    let keys = |lead_steps: u64| -> Vec<(u64, u128, u128, u64)> {
        let mut b = br.clone();
        (1..=HORIZON)
            .map(|w| {
                let daa = b.daa + 1 + if w == 1 { lead_steps } else { 0 };
                rig.beat(&mut b, daa, &[]);
                (b.s.safe_frontier().0, b.s.safe_weight(), b.s.bounded_immature(), b.daa)
            })
            .collect()
    };
    let unled = keys(0);
    for l in [lead(132), lead(1_620)] {
        let led = keys(l);
        let mut decided = Vec::new();
        for (w, (a, b)) in led.iter().zip(unled.iter()).enumerate() {
            let by_matured_work = a.0 > b.0 || (a.0 == b.0 && a.1 > b.1);
            if by_matured_work {
                decided.push(w as u64 + 1);
                // The led branch as challenger against the unled incumbent — whatever immature weight
                // the incumbent's own producers add after the fork.
                let challenger = PalwCandidateOrderV1::new(a.0, a.1, a.2, h(1));
                let incumbent = PalwCandidateOrderV1::new(b.0, b.1, u128::MAX / 2, h(2));
                assert_eq!(decide_deep_reorg_v2(&incumbent, &challenger), PalwDeepReorgV2::Allow);
            }
        }
        let first = decided.first().copied();
        println!(
            "lead {l:>2}: at equal wall time the led branch is ahead on (frontier, safe weight) at {}/{HORIZON} steps (first {first:?}); \
             end: led (frontier {}, safe {}) at DAA {} vs unled (frontier {}, safe {}) at DAA {}",
            decided.len(),
            led.last().unwrap().0,
            led.last().unwrap().1,
            led.last().unwrap().3,
            unled.last().unwrap().0,
            unled.last().unwrap().1,
            unled.last().unwrap().3
        );
        assert!(!decided.is_empty(), "lead {l}: a pre-fork Final inside the lead decides the comparator before any attempt counts");
    }
}
