//! Lane EM (emission audit 2026-10-04) — PoC for "PALW issuance is per CLAIM-BEARING BLOCK, the DAA score is a slot clock".
//!
//! Pure functions only (no fold): what the schedule intends (one subsidy per DAA tick), what the clock grants (at most one tick per
//! mergeset, whatever it holds), what F-EM bounds (16 whole carves per DAA, not one), and the proposed next fence (the DAA's PALW
//! issuance <= its schedule subsidy, split by claim weight, unused not minted).
//!
//! Run: `cargo test -p kaspa-consensus-core --test audit_emission_per_daa -- --nocapture`

use kaspa_consensus_core::palw_capacity_s567_v1::{
    PALW_EMISSION_BLOCKS_PER_DAA_V1, PALW_EMISSION_UNIT_MILLI_V1, palw_emission_admits_v1, palw_emission_budget_milli_v1, palw_rider_shares_v1,
};
use kaspa_consensus_core::palw_real_share_v1::{PalwClockMergesetFactsV1, palw_clock_tick_source_v1};

/// testnet-12's month-0 subsidy of one block, sompi (4,445.62014 MSK) — read from the live escrow 320,084,650,080 = 720 permille of it.
const S: u64 = 444_562_014_000;
/// ADR-0126 / t12: a claim escrows 720 permille of the subsidy of the block that carried its attempt.
const CARVE_PERMILLE: u64 = 720;
const CARVE: u64 = S * CARVE_PERMILLE / 1000;

#[test]
fn live_escrow_is_720_permille_of_the_block_subsidy() {
    assert_eq!(CARVE, 320_084_650_080, "the escrow every live vesting row carries (getPalwVesting, 10,848 rows, all identical)");
}

/// The clock: a mergeset holding k attempt-lane blocks (and no `bits`-priced block) is ONE tick at most, and zero where the
/// attempt-tick fence is off and no heartbeat is there. The coinbase of that same mergeset pays k blocks' subsidies.
#[test]
fn the_clock_ticks_once_per_mergeset_however_many_attempt_blocks_it_pays() {
    for k in [1u64, 2, 5, 31, 180] {
        let facts = PalwClockMergesetFactsV1 { priced: 0, heartbeats: 0, attempts: k, newest_beat_ms: None, newest_attempt_ms: Some(1_000) };
        let (tick_before_5300, _) = palw_clock_tick_source_v1(&facts, false);
        let (tick_after_5300, _) = palw_clock_tick_source_v1(&facts, true);
        assert!(!tick_before_5300, "int-10.3: attempt blocks are no tick source (the heartbeat stands in)");
        assert!(tick_after_5300, "int-12 (ADR-0165): one tick, a boolean, never k");
        // DAA +1 at most; subsidies declared (and carves withheld) = k.
        let ticks = u64::from(tick_after_5300);
        let schedule_per_mergeset = ticks * S; // calc_block_subsidy(daa) once per DAA tick
        let paid_carves = k * CARVE; // each attempt block declares calc_block_subsidy(its DAA) and escrows 720 permille of it
        println!("k={k:>3}  schedule={:>12.2} MSK  carves paid={:>12.2} MSK  ratio={:.2}", schedule_per_mergeset as f64 / 1e8, paid_carves as f64 / 1e8, paid_carves as f64 / schedule_per_mergeset as f64);
        assert_eq!(paid_carves / CARVE, k);
    }
}

/// F-EM: the hard per-DAA ceiling is 16 whole carves = 11.52 x the schedule's per-DAA subsidy; the 17th claim-bearing block is refused.
#[test]
fn f_em_bounds_a_daa_at_sixteen_carves_which_is_eleven_and_a_half_schedules() {
    let mut spent = 0u64;
    let mut admitted = 0u64;
    while let Ok(after) = palw_emission_admits_v1(spent) {
        spent = after;
        admitted += 1;
    }
    assert_eq!(admitted, PALW_EMISSION_BLOCKS_PER_DAA_V1);
    assert_eq!(spent, palw_emission_budget_milli_v1());
    let max_per_daa = admitted as u128 * CARVE as u128;
    let ratio_milli = max_per_daa * 1000 / S as u128;
    println!("F-EM ceiling per DAA = {:.2} MSK = {:.3} x calc_block_subsidy", max_per_daa as f64 / 1e8, ratio_milli as f64 / 1000.0);
    assert_eq!(ratio_milli, 11_520);
    assert_eq!(PALW_EMISSION_UNIT_MILLI_V1, 1_000);
    // riders re-slice a lead's carve and charge nothing: the ceiling does not move with 64 riders.
    let (share, lead) = palw_rider_shares_v1(CARVE, 64).unwrap();
    assert_eq!(share * 64 + lead, CARVE);
}

/// The proposed fence, as a pure allocator: a DAA's PALW pool is `S * 720/1000` (<= calc_block_subsidy); claims accepted in the DAA
/// split it by weight; whatever no claim takes is not minted. Property: the sum never exceeds the pool, for any k, any weights.
fn allocate(pool: u64, weights: &[u64]) -> Vec<u64> {
    let total: u128 = weights.iter().map(|w| *w as u128).sum();
    if total == 0 {
        return vec![0; weights.len()];
    }
    weights.iter().map(|w| ((pool as u128) * (*w as u128) / total) as u64).collect()
}

#[test]
fn per_daa_pool_split_by_weight_never_exceeds_the_schedule() {
    let pool = CARVE; // the claim side of one DAA's subsidy
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..20_000 {
        let k = (next() % 200) as usize;
        let weights: Vec<u64> = (0..k).map(|_| 1 + next() % 1_000_000).collect();
        let paid: u128 = allocate(pool, &weights).iter().map(|v| *v as u128).sum();
        assert!(paid <= pool as u128, "k={k}: {paid} > {pool}");
        if k == 0 {
            assert_eq!(paid, 0, "unused is not minted");
        }
    }
    // honest case today: 3.5 claims a DAA -> each claim gets 1/3.5 of a carve, not a whole carve.
    let each = allocate(pool, &[1; 4]);
    assert_eq!(each[0], pool / 4);
}
