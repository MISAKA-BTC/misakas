//! **LANE L4 (areas 6 + 8) — rooted state that no rule pruned, and the rules that now do.**
//!
//! `panel_liabilities` and `slashable_locks` are written at every terminal claim past the
//! objective-offence fence (`persist_panel_liability` from `finalize_claim` AND `void_claim`,
//! `lock_valid_seat` per Valid receipt). At the audit's commit the only `None` write anywhere in the
//! fold was the slash of one accused lock, so nothing swept an expired lock or an expired liability
//! — not on the DAA clock, not on the settled clock — and every chain block re-hashed them all
//! (`collection_root` borsh-encodes and hashes each entry) and every virtual resolve re-serialized
//! them as the tip carriage: per-block CPU and per-resolve disk-write grew linearly with chain age.
//!
//! **Converted to the fix (2026-09-24 DoS audit #12 (a) and (c)).** Past `palw_audit_2026_09_23`
//! `sweep_panel_obligations` drops, at the first block of each epoch, every lock and liability row
//! that is dead on both clocks and `window_court` past its expiry, each as its own delta entry; and a
//! `BondRegistered` must post the panel floor (4,000,000 sompi on testnet-12), not the producer
//! floor. Below the fence nothing moves (asserted). The per-row cost measurements are kept: they
//! are what the pruning now bounds.
//!
//! Synthetic, deterministic, and bounded: the largest state built here is ~60 MB.

use std::time::Instant;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_panel_var_v1::{PalwPanelLiabilityRecordV1, PalwSlashableLockV1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwBondStateV2, PalwBondStatusV2, PalwChainStateV2, PalwConsensusObjectV2,
    PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v7,
    palw_bond_registration_floor_v1, revert_delta_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn state_params(p: &Params) -> PalwStateParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.state.clone(),
        _ => panic!("testnet-12 is ConsensusV2"),
    }
}

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}

fn bond_key(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
}

fn bond(n: u64) -> PalwBondStateV2 {
    PalwBondStateV2 {
        pubkey: vec![n as u8; 32],
        operator_id: h(0x0900_0000 + n),
        collateral: 51_642_900_000_000,
        slashed: 0,
        status: PalwBondStatusV2::Active,
        registered_daa: 0,
        payout_payload: h(0x0A00_0000 + n),
        capable_classes: Default::default(),
    }
}

/// A liability as `persist_panel_liability` writes it after an honest Final with `signers` Valid
/// seats (every field populated, the shapes the fold writes).
fn liability(claim: u64, signers: usize, expiry_daa: u64, settled_at_final: u64) -> PalwPanelLiabilityRecordV1 {
    PalwPanelLiabilityRecordV1 {
        claim_id: h(claim),
        work_id: h(claim ^ 0x5555),
        class_id: h(0xC1A55),
        execution_root: h(claim ^ 0xE1),
        output_root: h(claim ^ 0x0E),
        executor_bond: bond_key(0x99),
        voided_daa: None,
        void_reason: None,
        valid_signers: (0..signers as u64).map(|s| (bond_key(s + 1).0, h(claim))).collect(),
        locked_sompi: 1_000_000_000,
        expiry_daa,
        settled_at_final,
    }
}

fn lock(claim: u64, expiry_daa: u64, settled_at_final: u64) -> PalwSlashableLockV1 {
    PalwSlashableLockV1 { claim: h(claim), amount: 1_000_000_000, expiry_daa, settled_at_final }
}

/// A state holding `claims` expired liabilities (both clocks run out) and `locks_per_claim` expired
/// locks each, built through the carriage — the same path a restart or a pruning-point import takes.
fn grown_state(p: &PalwStateParamsV2, claims: u64, locks_per_claim: u64, seats: u64) -> PalwChainStateV2 {
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    for s in 1..=seats {
        carriage.bonds.insert(bond_key(s), bond(s));
    }
    // Both clocks past every record: DAA expiry 100, settled_at_final 0, counter far beyond depth.
    carriage.settled_attempt_finals = 1_000_000;
    for c in 0..claims {
        let claim = 0x1_0000_0000 + c;
        carriage.panel_liabilities.insert(h(claim), liability(claim, locks_per_claim as usize, 100, 0));
        for s in 0..locks_per_claim {
            carriage.slashable_locks.insert((bond_key(1 + (c + s) % seats), h(claim)), lock(claim, 100, 0));
        }
    }
    carriage.into_state(p, None).expect("a grown carriage is a consistent state")
}

#[test]
fn bytes_per_never_pruned_entry() {
    for signers in [0usize, 3, 5] {
        let key = borsh::to_vec(&h(1)).unwrap().len();
        let val = borsh::to_vec(&liability(1, signers, 100, 0)).unwrap().len();
        println!("panel_liabilities entry, {signers} Valid signers: key {key} B + value {val} B = {} B", key + val);
    }
    let lk = borsh::to_vec(&(bond_key(1), h(1))).unwrap().len();
    let lv = borsh::to_vec(&lock(1, 100, 0)).unwrap().len();
    println!("slashable_locks entry: key {lk} B + value {lv} B = {} B", lk + lv);
    // The shapes the formula below depends on — if these move, re-derive the growth numbers.
    assert_eq!(borsh::to_vec(&liability(1, 0, 100, 0)).unwrap().len(), 426);
    assert_eq!(lk + lv, 228);
}

/// The t12 audit extras this lane folds under (`audit` on) or the fold every other network runs.
fn lane_extras(audit: bool) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        audit_2026_09_23_active: audit,
        settled_anchor_depth: audit.then_some(30),
        objective_offence_daa: Some(0),
        ..Default::default()
    }
}

/// One claimless chain block at `daa` through the REAL transition.
fn empty_block(
    sp: &PalwStateParamsV2,
    state: &PalwChainStateV2,
    i: u64,
    daa: u64,
    extras: &PalwTransitionExtrasV1,
) -> (PalwChainStateV2, PalwStateDeltaV2) {
    let ctx = PalwBlockContextV2 { block: h(0xB10C_0000 + i), daa_score: daa, blue_score: 10_000 + i, subsidy: 0 };
    let (next, delta, _) = apply_palw_transition_v7(
        state,
        sp,
        None,
        &ctx,
        &[],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        extras,
    )
    .expect("a claimless block is a valid transition");
    (next, delta)
}

/// **#12 (a): the fold prunes every lock and liability past its evidence horizon.** Both clocks
/// are past every record (DAA expiry 100 vs DAA 10,000+; settled_at_final 0 vs counter 1,000,000,
/// depth 30) and `window_court` has run past the expiry, so the first epoch boundary the chain
/// crosses sweeps every row, and the collateral they no longer locked is still all available.
///
/// Fails without the fix: 64 blocks spanning 63,000 DAA left every one of the 50 liabilities and
/// 150 locks rooted.
#[test]
fn expired_locks_and_liabilities_are_pruned_by_the_fold() {
    let p = t12();
    let sp = state_params(&p);
    let mut state = grown_state(&sp, 50, 3, 8);
    let extras = lane_extras(true);
    let (l0, k0) = (count_liabilities(&state), count_locks(&state));
    for i in 0..64u64 {
        state = empty_block(&sp, &state, i, 10_000 + i * 1_000, &extras).0;
    }
    let (l1, k1) = (count_liabilities(&state), count_locks(&state));
    println!("after 64 blocks spanning 63,000 DAA past every expiry: liabilities {l0} -> {l1}, locks {k0} -> {k1}");
    assert_eq!((l0, k0), (50, 150));
    assert_eq!((l1, k1), (0, 0), "every row past its evidence horizon is pruned");
    let live = state.slashable_available_v2(&bond_key(1), 80_000, Some(30));
    assert_eq!(live, 51_642_900_000_000u128, "and none of them locked anything");
}

/// **Below the audit fence nothing is pruned** — the fold every other network runs, byte for byte.
#[test]
fn below_the_fence_the_rows_stay_as_they_always_did() {
    let p = t12();
    let sp = state_params(&p);
    let mut state = grown_state(&sp, 50, 3, 8);
    let extras = lane_extras(false);
    for i in 0..8u64 {
        state = empty_block(&sp, &state, i, 10_000 + i * 1_000, &extras).0;
    }
    assert_eq!((count_liabilities(&state), count_locks(&state)), (50, 150), "the dormant fold prunes nothing");
}

/// **The horizon is `expiry + window_court`, on both clocks, at an epoch boundary.** A row whose DAA
/// clock ran out inside the last `window_court` survives the boundary; so does one the second clock
/// still holds (the anchor counter short of `depth`, inside `2 × window_court` of its expiry, and a
/// recent licence so the escape is closed); the next boundary past both drops them.
#[test]
fn a_row_inside_its_horizon_survives_the_boundary() {
    let p = t12();
    let sp = state_params(&p);
    let wc = sp.window_court();
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    for s in 1..=8 {
        carriage.bonds.insert(bond_key(s), bond(s));
    }
    carriage.settled_attempt_finals = 100;
    // A licence at DAA 20,000 keeps the escape shut through the test (2 x window_court = 6,000).
    carriage.recent_anchor_daas = vec![20_000];
    // Claim A: DAA clock out at 18,000, second clock satisfied (settled 0, counter 100 >= 30).
    carriage.panel_liabilities.insert(h(0xA), liability(0xA, 1, 18_000, 0));
    carriage.slashable_locks.insert((bond_key(1), h(0xA)), lock(0xA, 18_000, 0));
    // Claim B: DAA clock out at 16,000, but the second clock holds it (settled 90: 10 < 30 anchors).
    carriage.panel_liabilities.insert(h(0xB), liability(0xB, 1, 16_000, 90));
    carriage.slashable_locks.insert((bond_key(2), h(0xB)), lock(0xB, 16_000, 90));
    // Claim C: DAA clock out at 19,000, second clock satisfied — its horizon is 22,000.
    carriage.panel_liabilities.insert(h(0xC), liability(0xC, 1, 19_000, 0));
    carriage.slashable_locks.insert((bond_key(3), h(0xC)), lock(0xC, 19_000, 0));
    let mut state = carriage.into_state(&sp, None).expect("consistent");
    let extras = lane_extras(true);
    state = empty_block(&sp, &state, 0, 20_000, &extras).0;
    // Boundary 21,000: A is 3,000 past expiry = exactly its horizon (18,000 + 3,000) -> pruned; B is
    // 5,000 past expiry but its second clock holds until 16,000 + 2 x 3,000 = 22,000.
    state = empty_block(&sp, &state, 1, 21_000, &extras).0;
    assert!(state.panel_liability(&h(0xA)).is_none() && state.slashable_lock(bond_key(1), h(0xA)).is_none(), "A is past its horizon");
    assert!(
        state.panel_liability(&h(0xB)).is_some() && state.slashable_lock(bond_key(2), h(0xB)).is_some(),
        "the second clock holds B"
    );
    assert!(
        state.panel_liability(&h(0xC)).is_some() && state.slashable_lock(bond_key(3), h(0xC)).is_some(),
        "C is dead on both clocks but only 2,000 DAA past its expiry: inside its horizon"
    );
    state = empty_block(&sp, &state, 2, 22_000, &extras).0;
    assert!(
        state.panel_liability(&h(0xB)).is_none() && state.slashable_lock(bond_key(2), h(0xB)).is_none(),
        "B goes at the next boundary"
    );
    assert!(state.panel_liability(&h(0xC)).is_none() && state.slashable_lock(bond_key(3), h(0xC)).is_none(), "and so does C");
    assert_eq!(wc, 3_000, "the arithmetic above is t12's window_court");
}

/// **Every pruning is its own delta entry, and the revert restores the rows exactly** — a reorg
/// across the boundary un-prunes what it swept, root for root.
#[test]
fn every_pruning_delta_reverts() {
    let p = t12();
    let sp = state_params(&p);
    let state = grown_state(&sp, 50, 3, 8);
    let extras = lane_extras(true);
    let (s1, _) = empty_block(&sp, &state, 0, 10_000, &extras);
    let (s2, delta) = empty_block(&sp, &s1, 1, 11_000, &extras);
    assert_eq!((count_liabilities(&s2), count_locks(&s2)), (0, 0), "the boundary block pruned everything");
    let kinds = delta
        .entries
        .iter()
        .filter(|e| matches!(format!("{e:?}").split(' ').next(), Some("SlashableLock" | "PanelLiability")))
        .count();
    assert_eq!(kinds, 200, "one entry per pruned row (150 locks + 50 liabilities)");
    let back = revert_delta_v2(&s2, &delta, &sp).expect("the pruning reverts");
    assert_eq!(back.state_root(), s1.state_root(), "the revert restores the parent's root");
    assert_eq!((count_liabilities(&back), count_locks(&back)), (50, 150));
    assert_eq!(PalwStateCarriageV2::from_state(&back).panel_liabilities, PalwStateCarriageV2::from_state(&s1).panel_liabilities);
    assert_eq!(PalwStateCarriageV2::from_state(&back).slashable_locks, PalwStateCarriageV2::from_state(&s1).slashable_locks);
}

fn count_liabilities(s: &PalwChainStateV2) -> usize {
    PalwStateCarriageV2::from_state(s).panel_liabilities.len()
}

fn count_locks(s: &PalwChainStateV2) -> usize {
    PalwStateCarriageV2::from_state(s).slashable_locks.len()
}

/// **What the dead rows cost every node, per block, forever.** `state_root()` (per chain block,
/// `palw_state_v2_sync.rs:314`) and the tip carriage (per virtual resolve, `processor.rs:2127`) both
/// walk every row. Measured at three sizes; the fit is linear.
#[test]
fn per_block_cost_grows_linearly_with_dead_rows() {
    let p = t12();
    let sp = state_params(&p);
    let mut rows: Vec<(u64, usize, f64, f64)> = Vec::new();
    for claims in [2_000u64, 8_000, 32_000] {
        let state = grown_state(&sp, claims, 3, 8);
        let t = Instant::now();
        let reps = 3;
        for _ in 0..reps {
            std::hint::black_box(state.state_root());
        }
        let root_ms = t.elapsed().as_secs_f64() * 1_000.0 / reps as f64;
        let t = Instant::now();
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&state)).unwrap().len();
        let carriage_ms = t.elapsed().as_secs_f64() * 1_000.0;
        rows.push((claims, bytes, root_ms, carriage_ms));
    }
    println!("{:>8} {:>12} {:>12} {:>14}", "claims", "carriage B", "root ms", "carriage ms");
    for (c, b, r, cm) in &rows {
        println!("{c:>8} {b:>12} {r:>12.2} {cm:>14.2}");
    }
    let per_claim = (rows[2].1 - rows[0].1) as f64 / (rows[2].0 - rows[0].0) as f64;
    let root_us_per_claim = (rows[2].2 - rows[0].2) * 1_000.0 / (rows[2].0 - rows[0].0) as f64;
    println!("marginal carriage bytes per terminal claim (3 Valid seats) = {per_claim:.0} B");
    println!("marginal state_root cost per terminal claim (debug build)  = {root_us_per_claim:.3} us");
    // One terminal claim adds one liability (3 signers) and three locks: 64 + 822 + 3*228 = 1,570 B.
    assert!(per_claim > 1_400.0 && per_claim < 1_700.0, "per-claim growth {per_claim}");
    assert!(rows[2].1 > rows[0].1 * 10, "carriage grows with dead rows");
}

/// **The bond registry is append-only** (`bond_of_pubkey_v2`'s own doc: "nothing else writes a bond
/// row away"). A withdrawn bond's row — ML-DSA-87 pubkey and all — stays rooted forever, and the
/// collateral that registered it is free to register the next one after the withdrawal delay.
#[test]
fn a_withdrawn_bond_row_is_permanent_and_its_collateral_recycles() {
    let p = t12();
    let b = match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b,
        _ => unreachable!(),
    };
    let mut row = bond(1);
    row.pubkey = vec![0u8; 2_592]; // ML-DSA-87 public key
    row.status = PalwBondStatusV2::Retiring { since_daa: 1, settled_at_since: 0 };
    let bytes = borsh::to_vec(&bond_key(1)).unwrap().len() + borsh::to_vec(&row).unwrap().len();
    // #12 (c): past the audit fence a bond must post the panel floor, not the producer floor.
    let min = palw_bond_registration_floor_v1(b.state.min_collateral_sompi(), p.palw_audit_2026_09_23_active_at(0));
    assert_eq!(min, 4_000_000, "testnet-12 registers at the panel floor");
    let delay = b.bond.withdrawal_delay_daa();
    let cadence_s = p.target_time_per_block() / 1_000;
    let cycle_s = delay * cadence_s;
    println!("withdrawn bond row                       = {bytes} B (never pruned)");
    println!("registration floor                       = {min} sompi = {:.2} MSK", min as f64 / 100_000_000.0);
    println!("withdrawal delay                         = {delay} DAA = {:.1} days at {cadence_s} s/DAA", cycle_s as f64 / 86_400.0);
    if min > 0 {
        let rows_per_msk_year = (365.0 * 86_400.0 / cycle_s as f64) * (100_000_000.0 / min as f64);
        println!("permanent rows per MSK of recycled capital per year = {rows_per_msk_year:.4}");
        println!("permanent bytes per MSK-year                         = {:.1} B", rows_per_msk_year * bytes as f64);
    }
    assert!(bytes > 2_700);
}

/// Bond-row spam rate bound: block mass / carrier mass. The carrier is at least the object's own
/// bytes (ML-DSA-87 pubkey 2,592 + operator key + ML-DSA-87 signature 4,627) plus one PQ input
/// (signature 4,627 + pubkey 2,592). Mass per byte is taken as 1 (a LOWER bound on mass, so an
/// UPPER bound on the rate).
#[test]
fn bond_row_spam_rate_bound() {
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
    let p = t12();
    let obj = PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(1),
        pubkey: vec![0u8; 2_592],
        operator_pubkey: vec![0u8; 2_592],
        collateral: 4_000_000,
        payout_payload: h(1),
        capable_classes: Default::default(),
        signature: vec![0u8; 4_627],
    };
    let floor_msk = 0.04; // #12 (c): the panel floor, ten times the 0.004 MSK the audit measured.
    let obj_bytes = borsh::to_vec(&obj).unwrap().len() as u64;
    let carrier = obj_bytes + 4_627 + 2_592;
    let per_block = p.max_block_mass / carrier;
    let blocks_per_day = 86_400 / (p.target_time_per_block() / 1_000);
    let rows_day = per_block * blocks_per_day;
    println!("BondRegistered object = {obj_bytes} B; carrier >= {carrier} B; max_block_mass = {}", p.max_block_mass);
    println!("<= {per_block} registrations per block, {blocks_per_day} blocks/day -> <= {rows_day} permanent rows/day");
    println!(
        "-> <= {:.1} MB/day of permanent rooted state for {:.2} MSK/day of collateral ({floor_msk} MSK each)",
        rows_day as f64 * 2_837.0 / 1e6,
        rows_day as f64 * floor_msk
    );
    assert!(per_block > 0);
}

/// One `BondRegistered` at `collateral`, folded at DAA 5 on testnet-12's own genesis, with the audit
/// fence on (`audit`) or off.
fn register_at(collateral: u64, audit: bool) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let p = t12();
    let sp = state_params(&p);
    let b = match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => unreachable!(),
    };
    let extras = lane_extras(audit);
    let genesis = PalwBlockContextV2 { block: h(0x6E0), daa_score: 0, blue_score: 0, subsidy: 0 };
    let (g, _, _) = apply_palw_transition_v7(
        &PalwChainStateV2::genesis(),
        &sp,
        None,
        &genesis,
        &b.genesis_objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("the t12 genesis list folds");
    let object = PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(0x51),
        pubkey: vec![0x51; 32],
        operator_pubkey: vec![0x52; 32],
        collateral,
        payout_payload: h(0x53),
        capable_classes: Default::default(),
        signature: Vec::new(),
    };
    let ctx = PalwBlockContextV2 { block: h(0x6E1), daa_score: 5, blue_score: 1, subsidy: 0 };
    apply_palw_transition_v7(
        &g,
        &sp,
        None,
        &ctx,
        &[object],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .map(|(s, _, _)| s)
}

/// **#12 (c): past the fence a bond registers at the panel floor, 4,000,000 sompi** — 3,999,999 is
/// refused and 4,000,000 folds; below the fence the producer floor (400,000) still registers, as it
/// always did.
///
/// Fails without the fix: 3,999,999 (and 400,000) register past the fence.
#[test]
fn the_registration_floor_is_the_panel_floor_past_the_fence() {
    assert!(matches!(register_at(3_999_999, true), Err(PalwStateV2Error::CollateralBelowMinimum { got: 3_999_999, .. })));
    assert!(matches!(register_at(400_000, true), Err(PalwStateV2Error::CollateralBelowMinimum { .. })));
    let s = register_at(4_000_000, true).expect("the panel floor registers");
    assert_eq!(s.bond(&bond_key(0x51)).map(|b| b.collateral), Some(4_000_000));
    register_at(400_000, false).expect("below the fence the producer floor still registers");
    assert!(matches!(register_at(399_999, false), Err(PalwStateV2Error::CollateralBelowMinimum { .. })));
}
