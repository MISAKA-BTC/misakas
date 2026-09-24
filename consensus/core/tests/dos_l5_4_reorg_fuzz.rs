//! **L5 stress test 4 — reorg + liability-reservation fuzz.**
//!
//! Seeded random sequences over testnet-12's genesis state (real fold, real bundle params, real
//! fences): junk attempts from a genesis executor and from newcomer bonds, `PanelBound` on random
//! eligible seats, `ReceiptLicensed` (all-Valid), `BondRegistered`, `BondRetireRequested`, and
//! claimless DAA jumps (timeouts, redraws, voids, `Final`, retirement). After EVERY accepted step:
//!
//!   R1  `revert_delta_v2(child, delta)` restores the parent's exact `state_root`, and
//!       `apply_delta_v2(parent, delta)` rebuilds the child's exact root (the reorg primitive).
//!   R2  `settled_attempt_finals` is monotone along the chain and reverts exactly.
//!   I1  per bond, `reserved_exposure + registration_exposure <= collateral x ratio` (the
//!       admission ceiling every reservation claims to live under).
//!   I2  per bond, `reserved_exposure + live slashable locks <= collateral x ratio` — "no sompi
//!       backs two claims": a seat's duty exposure (ADR-0124) and its Valid lock (ADR-0144 §9) are
//!       two ledgers over ONE collateral; the lock ledger's `available` is `posted - locks`
//!       (palw_panel_var_v1.rs:148) and never subtracts `reserved_exposure`.
//!
//! What is NOT driven (and so not covered): court objects (`void_and_slash` via a verdict), DA
//! accusations, objective offences, free-prompt commitments and receipt spends, merged work.
//! Admission (`check_palw_attempt_admission_v2`) is applied as the producer-side ceiling check
//! `reserved + new <= collateral x ratio` rather than by calling the gate (it needs the processor's
//! epoch-budget fences); the panel draw's headroom predicate is the real `palw_seat_has_headroom_v1`.
//!
//! Run: cargo test -p kaspa-consensus-core --test dos_l5_4_reorg_fuzz -- --nocapture --test-threads=2

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_panel_economy_v1::palw_seat_has_headroom_v1;
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, apply_delta_v2,
    revert_delta_v2,
};
use std::collections::BTreeMap;

const STEPS: usize = 360;
const SEEDS: [u64; 3] = [0x5EED_0001, 0x5EED_0002, 0x5EED_0003];

struct Outcome {
    accepted: usize,
    refused: usize,
    reverts_checked: usize,
    revert_failures: Vec<String>,
    settled_violations: Vec<String>,
    i1_worst: (f64, String),
    i2_worst: (f64, String),
    finals: u64,
    kinds: BTreeMap<&'static str, usize>,
}

fn live_locks(s: &PalwChainStateV2, bond: &PalwBondKeyV2, now: u64, depth: Option<u64>) -> u128 {
    // Public read of the same predicate the fold uses: posted - available.
    let posted = s.bond(bond).map(|b| b.collateral as u128).unwrap_or(0);
    posted - s.slashable_available_v2(bond, now, depth).min(posted)
}

fn run(seed: u64) -> Outcome {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let ratio = sp.fp_max_exposure_ratio_permille() as u128;
    let depth = p.palw_settled_anchor_depth;
    let (floor, leaves, target, _) = genesis_classes(&p)[0];
    let pwu = palw_pwu_v1(target, leaves);
    let seat_pool: Vec<(PalwBondKeyV2, Hash64, u64)> = genesis_bonds(&p);
    let genesis_exec = {
        let mut out = None;
        for o in b.genesis_objects.iter() {
            if let PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } = o {
                out = Some((*bond, pubkey.clone(), operator_pubkey.clone()));
                break;
            }
        }
        out.unwrap()
    };
    let economy = p.palw_seat_economy_at(0);

    let mut rng = Rng(seed);
    let mut s = genesis_state(&p);
    let mut daa = 1_000u64;
    let mut blue = 1u64;
    let mut block = seed << 20;
    let mut newcomers: Vec<u64> = Vec::new();
    let mut next_newcomer = 100u64;
    let mut attempt_seed = seed & 0xFFFF_FFFF;
    let mut out = Outcome {
        accepted: 0,
        refused: 0,
        reverts_checked: 0,
        revert_failures: vec![],
        settled_violations: vec![],
        i1_worst: (0.0, String::new()),
        i2_worst: (0.0, String::new()),
        finals: 0,
        kinds: BTreeMap::new(),
    };

    for step in 0..STEPS {
        let roll = rng.below(100);
        let mut next_daa = daa + 1;
        let mut objects: Vec<PalwConsensusObjectV2> = Vec::new();
        let mut env_holder = None;
        let mut key = Hash64::default();
        let kind: &'static str;
        let provisional: Vec<Hash64> =
            s.claims_iter().filter(|(_, c)| matches!(c.phase, PalwClaimPhaseV2::Provisional)).map(|(id, _)| *id).collect();
        let bound: Vec<Hash64> = s.claims_iter().filter(|(_, c)| matches!(c.phase, PalwClaimPhaseV2::PanelBound { .. })).map(|(id, _)| *id).collect();
        if roll < 30 {
            // an attempt from the genesis executor or a newcomer
            attempt_seed += 1;
            let (bk, pk, op) = if newcomers.is_empty() || rng.below(2) == 0 {
                genesis_exec.clone()
            } else {
                let n = newcomers[rng.below(newcomers.len() as u64) as usize];
                (bond_key(n), pubkey_of(n), operator_pubkey_of(n))
            };
            let (env, k, _) = junk_attempt(floor, bk, pk, &op, pwu, attempt_seed, attempt_seed ^ 0xABCD);
            // producer-side ceiling (admission's exposure ceiling, as the producer's headroom reads it)
            let bond = s.bond(&bk);
            let room = bond.map(|x| x.collateral as u128 * ratio / 1000).unwrap_or(0);
            let used = s.reserved_exposure(&bk) + s.registration_exposure(&bk);
            if used + 38_540 > room {
                kind = "attempt-refused-ceiling";
            } else {
                env_holder = Some(env);
                key = k;
                kind = "attempt";
            }
        } else if roll < 50 && !provisional.is_empty() {
            let id = provisional[rng.below(provisional.len() as u64) as usize];
            let claim = s.claim(&id).unwrap().clone();
            let exec_op = s.bond(&claim.bond).unwrap().operator_id;
            let mut cands: Vec<(PalwBondKeyV2, Hash64)> = seat_pool
                .iter()
                .filter(|(k, o, _)| *k != claim.bond && *o != exec_op)
                .filter(|(k, _, _)| {
                    let bnd = s.bond(k).unwrap();
                    matches!(bnd.status, PalwBondStatusV2::Active)
                        && economy.map_or(true, |e| {
                            palw_seat_has_headroom_v1(
                                bnd.collateral,
                                s.reserved_exposure(k) + s.registration_exposure(k),
                                e.seat_exposure(claim.reserved, claim.escrowed_reward, b.panel.seat_count() as usize),
                                e.max_exposure_ratio_permille,
                            )
                        })
                })
                .map(|(k, o, _)| (*k, *o))
                .collect();
            if cands.len() < b.panel.seat_count() as usize {
                kind = "bind-refused-no-headroom";
            } else {
                // seeded shuffle, take seat_count
                for i in (1..cands.len()).rev() {
                    let j = rng.below(i as u64 + 1) as usize;
                    cands.swap(i, j);
                }
                cands.truncate(b.panel.seat_count() as usize);
                objects.push(PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(rng.next()), seats: seats_of(&cands) });
                kind = "panel-bound";
            }
        } else if roll < 70 && !bound.is_empty() {
            let id = bound[rng.below(bound.len() as u64) as usize];
            let seats: Vec<PalwBondKeyV2> = s.panel(&id).unwrap().seats.iter().map(|x| x.bond).collect();
            objects.push(PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: valid_receipts(id, &seats) });
            kind = "receipt-licensed";
        } else if roll < 76 {
            next_newcomer += 1;
            // newcomers post 5,000..50,000 MSK
            let coll = (5_000 + rng.below(45_000)) * 100_000_000;
            objects.push(bond_obj(next_newcomer, coll));
            newcomers.push(next_newcomer);
            kind = "bond-registered";
        } else if roll < 80 && !newcomers.is_empty() {
            let n = newcomers[rng.below(newcomers.len() as u64) as usize];
            objects.push(PalwConsensusObjectV2::BondRetireRequested { bond: bond_key(n), signature: vec![1] });
            kind = "bond-retire";
        } else {
            let span = if rng.below(5) == 0 { 700 } else { 60 };
            next_daa = daa + 1 + rng.below(span);
            kind = "heartbeat-jump";
        }
        *out.kinds.entry(kind).or_default() += 1;
        if kind.ends_with("refused-ceiling") || kind.ends_with("no-headroom") {
            continue;
        }
        blue += 1;
        block += 1;
        let work = match &env_holder {
            Some(env) => PalwBlockWorkV3::Attempt(env),
            None => PalwBlockWorkV3::None,
        };
        let subsidy = if env_holder.is_some() { T12_BLOCK_SUBSIDY_SOMPI } else { 0 };
        let parent = s.clone();
        let parent_root = parent.state_root();
        match fold(&p, &sp, &parent, &ctx(block, next_daa, blue, subsidy), &objects, work, key) {
            Err(_) => {
                out.refused += 1;
                blue -= 1;
                continue;
            }
            Ok((child, delta, _)) => {
                out.accepted += 1;
                let child_root = child.state_root();
                // R1
                match revert_delta_v2(&child, &delta, &sp) {
                    Ok(back) => {
                        if back.state_root() != parent_root {
                            out.revert_failures.push(format!("step {step} ({kind}): revert root != parent root"));
                        }
                        if back.settled_attempt_finals() != parent.settled_attempt_finals() {
                            out.revert_failures.push(format!("step {step} ({kind}): revert did not restore settled_attempt_finals"));
                        }
                    }
                    Err(e) => out.revert_failures.push(format!("step {step} ({kind}): revert errored {e:?}")),
                }
                match apply_delta_v2(&parent, &delta, &sp) {
                    Ok(again) => {
                        if again.state_root() != child_root {
                            out.revert_failures.push(format!("step {step} ({kind}): re-apply root != child root"));
                        }
                    }
                    Err(e) => out.revert_failures.push(format!("step {step} ({kind}): re-apply errored {e:?}")),
                }
                out.reverts_checked += 1;
                // R2
                if child.settled_attempt_finals() < parent.settled_attempt_finals() {
                    out.settled_violations.push(format!("step {step}: settled went backwards"));
                }
                out.finals = child.settled_attempt_finals();
                // I1 / I2
                for (k, bnd) in child.bonds_iter() {
                    let ceiling = bnd.collateral as u128 * ratio / 1000;
                    if ceiling == 0 {
                        continue;
                    }
                    let res = child.reserved_exposure(k) + child.registration_exposure(k);
                    let r1 = res as f64 / ceiling as f64;
                    if r1 > out.i1_worst.0 {
                        out.i1_worst = (r1, format!("step {step} bond {:.12}: reserved {} / ceiling {}", k.0.transaction_id.to_string(), res, ceiling));
                    }
                    let locks = live_locks(&child, k, next_daa, depth);
                    let r2 = (res + locks) as f64 / ceiling as f64;
                    if r2 > out.i2_worst.0 {
                        out.i2_worst = (
                            r2,
                            format!(
                                "step {step} bond {:.12}: reserved {} + live locks {} = {} vs ceiling {} (collateral {})",
                                k.0.transaction_id.to_string(),
                                res,
                                locks,
                                res + locks,
                                ceiling,
                                bnd.collateral
                            ),
                        );
                    }
                }
                s = child;
                daa = next_daa;
            }
        }
    }
    out
}

#[test]
fn dos_l5_4_reorg_and_liability_fuzz() {
    let mut all_revert = Vec::new();
    let mut all_settled = Vec::new();
    let mut worst_i1: (f64, String) = (0.0, String::new());
    let mut worst_i2: (f64, String) = (0.0, String::new());
    for seed in SEEDS {
        let o = run(seed);
        println!("=== seed {seed:#x}: accepted {} refused {} reverts checked {} settled finals {} ===", o.accepted, o.refused, o.reverts_checked, o.finals);
        println!("    ops: {:?}", o.kinds);
        println!("    R1 failures: {}   R2 violations: {}", o.revert_failures.len(), o.settled_violations.len());
        println!("    I1 worst (reserved / ceiling)           = {:.4}  {}", o.i1_worst.0, o.i1_worst.1);
        println!("    I2 worst ((reserved + locks) / ceiling) = {:.4}  {}", o.i2_worst.0, o.i2_worst.1);
        for f in o.revert_failures.iter().take(5) {
            println!("    R1: {f}");
        }
        all_revert.extend(o.revert_failures);
        all_settled.extend(o.settled_violations);
        if o.i1_worst.0 > worst_i1.0 {
            worst_i1 = o.i1_worst;
        }
        if o.i2_worst.0 > worst_i2.0 {
            worst_i2 = o.i2_worst;
        }
    }
    assert!(all_revert.is_empty(), "apply-then-revert must restore the exact parent root: {all_revert:?}");
    assert!(all_settled.is_empty(), "settled_attempt_finals must be monotone: {all_settled:?}");
    assert!(worst_i1.0 <= 1.0, "I1: a bond's reservations exceed its ceiling: {}", worst_i1.1);
    assert!(
        worst_i2.0 <= 1.0,
        "I2 (no sompi backs two claims): a seat's duty exposure plus its live Valid locks exceed collateral x ratio by {:.2}x: {}",
        worst_i2.0,
        worst_i2.1
    );
}

/// What one run of 4b measured.
struct LockLedgerRun {
    licensed: u64,
    stuck: Option<(u64, &'static str)>,
    lock_each: u128,
    posted: u128,
    locks: u128,
    ceiling: u128,
    committed: u128,
    duty_room: u128,
    /// `(worst committed ÷ ceiling over every step, where)` on the first seat, past the fence.
    worst_committed: (f64, u64),
    /// Steps where `palw_bond_committed_v1` differed from Σ over claims of `max(duty, live lock)`
    /// plus the seat's own ledger — ADR-0152 A-1's identity, recomputed independently (T09).
    identity_breaks: Vec<String>,
}

/// Five genesis seats license floor claims (all-Valid) back to back, up to 2,000, until the fold
/// refuses a set. The fixture carries no execution lane, so the lock omits the realizable-rights
/// term — the measured counts are UPPER bounds on t12's.
fn lock_ledger_run(p: &kaspa_consensus_core::config::params::Params) -> LockLedgerRun {
    use kaspa_consensus_core::palw_state_v2::{palw_bond_committed_v1, palw_seat_duty_of_v1, palw_second_clock_depth_v1};
    let b = bundle(p);
    let sp = b.state.clone();
    let ratio = sp.fp_max_exposure_ratio_permille() as u128;
    let depth = p.palw_settled_anchor_depth;
    let rcore = sp.rcore_plus_from_daa().is_some();
    let (floor, leaves, target, _) = genesis_classes(p)[0];
    let pwu = palw_pwu_v1(target, leaves);
    let bonds = genesis_bonds(p);
    let exec = {
        let mut out = None;
        for o in b.genesis_objects.iter() {
            if let PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } = o {
                out = Some((*bond, pubkey.clone(), operator_pubkey.clone()));
                break;
            }
        }
        out.unwrap()
    };
    let seats: Vec<(PalwBondKeyV2, Hash64)> = bonds[1..6].iter().map(|(k, o, _)| (*k, *o)).collect();
    let seat_keys: Vec<PalwBondKeyV2> = seats.iter().map(|s| s.0).collect();
    let seat = seats[0].0;
    let mut s = genesis_state(p);
    let mut daa = 1_000u64;
    let mut blue = 0u64;
    let mut licensed = 0u64;
    let mut lock_each = 0u128;
    let mut stuck = None;
    let mut worst_committed = (0f64, 0u64);
    let mut identity_breaks = Vec::new();
    for i in 0..2_000u64 {
        let (env, key, id) = junk_attempt(floor, exec.0, exec.1.clone(), &exec.2, pwu, 50_000 + i, 0x10C0 + i);
        daa += 1;
        blue += 1;
        s = fold(p, &sp, &s, &ctx(0x40_0000 + blue, daa, blue, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
            .unwrap()
            .0;
        daa += 1;
        blue += 1;
        s = fold(
            p,
            &sp,
            &s,
            &ctx(0x40_0000 + blue, daa, blue, 0),
            &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(i), seats: seats_of(&seats) }],
            PalwBlockWorkV3::None,
            Hash64::default(),
        )
        .unwrap()
        .0;
        if !matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }) {
            stuck = Some((i, "PanelBound inert (seat lock not eligible)"));
            break;
        }
        daa += 1;
        blue += 1;
        s = fold(
            p,
            &sp,
            &s,
            &ctx(0x40_0000 + blue, daa, blue, 0),
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: valid_receipts(id, &seat_keys) }],
            PalwBlockWorkV3::None,
            Hash64::default(),
        )
        .unwrap()
        .0;
        if !matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
            stuck = Some((i, "ReceiptLicensed inert (a Valid seat cannot post its lock)"));
            break;
        }
        if lock_each == 0 {
            lock_each = s.slashable_lock(seats[0].0, id).unwrap().amount;
        }
        licensed += 1;
        if rcore {
            // T09: the one ledger, term by term — the seat's own ledger (its duties, here) plus, per
            // claim it locks, the live lock's excess over its duty — and it never passes the ceiling.
            let escaped = palw_second_clock_depth_v1(depth, s.recent_anchor_daas(), daa, sp.window_court());
            let committed = palw_bond_committed_v1(&s, &seat, daa, escaped, sp.window_court());
            let settled = s.settled_attempt_finals();
            let by_claim: u128 = s
                .slashable_locks_of(&seat)
                .filter(|(_, lock)| lock.is_live_v3(daa, settled, escaped, sp.window_court()))
                .map(|((_, c), lock)| lock.amount.saturating_sub(palw_seat_duty_of_v1(&s, c, &seat)))
                .sum();
            let expected = s.reserved_exposure(&seat) + s.registration_exposure(&seat) + by_claim;
            if committed != expected {
                identity_breaks.push(format!("claim {i}: committed {committed} vs Σ max(duty, lock) {expected}"));
            }
            let posted = s.bond(&seat).unwrap().collateral as u128;
            let share = committed as f64 / (posted * ratio / 1000) as f64;
            if share > worst_committed.0 {
                worst_committed = (share, i);
            }
        }
    }
    let posted = s.bond(&seat).unwrap().collateral as u128;
    let locks = posted - s.slashable_available_v2(&seat, daa, depth).min(posted);
    let ceiling = posted * ratio / 1000;
    let escaped = palw_second_clock_depth_v1(depth, s.recent_anchor_daas(), daa, sp.window_court());
    let committed = palw_bond_committed_v1(&s, &seat, daa, escaped, sp.window_court());
    let duty_room = ceiling.saturating_sub(s.reserved_exposure(&seat) + s.registration_exposure(&seat));
    LockLedgerRun { licensed, stuck, lock_each, posted, locks, ceiling, committed, duty_room, worst_committed, identity_breaks }
}

fn print_lock_ledger_run(label: &str, r: &LockLedgerRun, p: &kaspa_consensus_core::config::params::Params) {
    let sp = bundle(p).state;
    println!("=== 4b ({label}): lock ledger ===");
    println!(
        "lock per Valid seat per floor claim   = {} sompi ({:.2} MSK)  (no execution-lane rights term: a lower bound)",
        r.lock_each,
        msk(r.lock_each)
    );
    println!("claims licensed on the same 5 seats   = {}; then {:?}", r.licensed, r.stuck);
    println!(
        "seat posted {:.2} MSK: live locks {:.2} MSK ({:.1}% of posted); committed {:.2} MSK; duty ceiling still free {:.2} MSK",
        msk(r.posted),
        msk(r.locks),
        100.0 * r.locks as f64 / r.posted as f64,
        msk(r.committed),
        msk(r.duty_room)
    );
    println!(
        "=> locks + duty ceiling = {:.1}% of posted (no sompi should back two claims: <= 100%); worst committed/ceiling {:.3} at claim {}",
        100.0 * (r.locks + r.ceiling) as f64 / r.posted as f64,
        r.worst_committed.0,
        r.worst_committed.1
    );
    println!("lock life >= challenge {} + window_court {}", sp.window_challenge_at(0), sp.window_court());
}

/// **4b / ADR-0152 T09: past `palw_rcore_plus` one ledger backs every lock and every duty.**
///
/// Five genesis seats license floor claims (all-Valid) back to back. Below the fence (the twin
/// test) the Valid locks live on a second ledger over the same collateral — `slashable_available`
/// at 100% of posted, beside the 500‰ duty/claim ceiling — and the fixture reached that ledger's end
/// with 150% of a seat's collateral committed. Past it (A-1, A-3, L-4b) a seat's `committed` is its
/// own ledger plus Σ `max(duty, live lock)` per claim, recomputed here independently at every step,
/// and the bind and the licence ask it against the one ceiling: no sompi backs two claims.
#[test]
fn dos_l5_4b_one_ledger_backs_every_lock_and_duty() {
    let p = t12();
    let r = lock_ledger_run(&p);
    print_lock_ledger_run("R-core+", &r, &p);
    assert!(r.identity_breaks.is_empty(), "T09: committed = own ledger + Σ max(duty, live lock): {:?}", r.identity_breaks);
    assert!(r.worst_committed.0 <= 1.0, "A-3: committed stays under the 500‰ ceiling at every step ({:.3})", r.worst_committed.0);
    assert!(
        r.locks + r.ceiling <= r.posted,
        "I2: no sompi backs two claims ({:.1}% of posted)",
        100.0 * (r.locks + r.ceiling) as f64 / r.posted as f64
    );
    if let Some((_, why)) = r.stuck {
        panic!(
            "the one ledger stopped licensing within 2,000 claims ({why}) while committed peaked at {:.3} of the ceiling",
            r.worst_committed.0
        );
    }
}

/// **4b's PRE-FENCE DEFECT RECORD (the fence-off twin: testnet-12 with `palw_rcore_plus = None`)**:
/// the lock ledger double-backs collateral — live Valid locks plus the duty/claim ceiling reach more
/// than 100% of a seat's posted collateral, and the 100% lock ledger is what stops licensing. Kept
/// green as a record of what R-core+ closes (row 12 of the DoS report).
#[test]
fn dos_l5_4b_pre_fence_record_the_lock_ledger_double_backs_collateral() {
    let mut p = t12();
    p.palw_rcore_plus = None;
    p.palw_rcore_conservative_classes = &[];
    p.sync_palw_rcore_plus();
    let r = lock_ledger_run(&p);
    print_lock_ledger_run("fence off", &r, &p);
    assert!(r.stuck.is_some(), "below the fence the 100% lock ledger ends licensing");
    assert!(r.locks + r.ceiling > r.posted, "the defect: one collateral backs locks AND the whole duty/claim ceiling");
}
