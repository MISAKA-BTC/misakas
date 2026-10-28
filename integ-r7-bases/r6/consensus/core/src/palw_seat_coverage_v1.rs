//! **ADR-0098 — a panel's coverage of one claim is a number, measured with the draw the seats run.**
//!
//! ADR-0081 Decision 8 asked for "what fraction of a job's segments the panel collectively covers
//! per claim and what a producer's expected cost is of a single forged link", and its U-03 was to
//! measure it. The decision was withdrawn with ADR-0081 §3 (ADR-0082) before anyone took the
//! number, and the question was never about segments: it is about ADR-0077 Decision 8's LIVE draw,
//! where a free-prompt seat replays [`PALW_FP_SEAT_INTERVAL_SAMPLES_V1`] intervals of the claim it
//! is seated on. This module answers it there, two ways that must agree —
//! [`palw_seat_detection_ppm_v1`] in closed form, and [`palw_seat_coverage_measured_v1`] by running
//! [`palw_fp_interval_draw_v1`] itself over many beacons.
//!
//! **The model.** The cheapest lie is one leaf: every leaf after it is computed honestly FROM the
//! lie, so the committed trace is self-consistent and the only inconsistency anywhere is that one
//! step. The producer picks the leaf when it commits; the draw is a function of the panel's beacon,
//! which did not exist yet. A seat whose drawn intervals include the lie's interval replays it and
//! finds the fault; a seat whose draw misses it finds nothing. So the chance the panel's sampling
//! catches a one-leaf lie is the chance some replaying seat drew that interval — and, in units of
//! the claim's own reservation, it is also the producer's expected cost of the lie, because a
//! conviction slashes exactly `claim.reserved` (`void_and_slash`).
//!
//! **Two ways a seat sees a lie, and which one binds.** Before it replays interval `j` a seat
//! recomputes the state the interval resumes from — the cache over every position before `j`, from
//! the prompt and the committed answer — and compares its root with the one the producer committed
//! (ADR-0082 Decision 9). So a lie that ALTERS THE CACHE is caught by any seat that draws any
//! interval at or after it ([`palw_seat_state_or_row_detection_ppm_v1`]); only a lie the cache
//! never records — the token a position selects, or the last layer's work after its K/V rows are
//! written — is left to the row replay of its own interval. The selection lie is also the one that
//! changes the answer, so it is the forger's best lie, and [`palw_seat_detection_ppm_v1`] is the
//! floor under every other.
//!
//! **What this is not.** It is not the lane's whole defence, and the module does not say it is.
//! ADR-0077 Decision 8: a sampled verdict convicts nobody and the claim stays disputable for the
//! whole challenge window; a bonded watchdog (`--palw-challenge`) re-runs every licensed claim, and
//! where one runs a forged leaf is found with certainty — at one inference per claim. What is
//! measured here is the half that costs a seat `k` intervals, because it is the half a network
//! whose seats cannot run the whole model is left with (ADR-0097 Decision 5).
//!
//! Every probability is in parts per million and every rounding is in the direction that never
//! overstates coverage: a pinned number here is a floor, and a limit is not a verdict.

use crate::Hash64;
use crate::palw_economic_locus_v1::palw_receipt_licensed_wire_bytes_v1;
use crate::palw_fp_interval_v1::{PALW_FP_SEAT_INTERVAL_SAMPLES_V1, palw_fp_interval_draw_v1};
use crate::palw_mode_v2::PALW_STANDARD_TX_BYTES;

/// One in a million — the unit of every probability in this module.
pub const PPM: u64 = 1_000_000;

/// The fixed point the closed form multiplies in: `10^18`, so `N^s` never has to be formed.
const SCALE: u128 = 1_000_000_000_000_000_000;

/// **The closed form.** The chance, in ppm, that at least one of `seats` independent draws of `k`
/// DISTINCT intervals out of `interval_count` contains one fixed interval:
/// `1 − ((N − k) / N)^seats`.
///
/// The per-seat miss `(N − k) / N` is exact for a uniform draw without replacement (a fixed index
/// is in a uniformly random `k`-subset with probability `k / N`), and seats draw independently —
/// the draw keys the seat index into its own hash. The all-miss product is rounded UP at every
/// step, so the detection this returns is a floor. `k ≥ N` is the whole job: every seat replays
/// every interval, and the answer is certain.
pub fn palw_seat_detection_ppm_v1(interval_count: u32, k: u32, seats: u32) -> u64 {
    if interval_count == 0 || k == 0 || seats == 0 {
        return 0;
    }
    if k >= interval_count {
        return PPM;
    }
    let (n, miss) = (u128::from(interval_count), u128::from(interval_count - k));
    let mut all_miss = SCALE;
    for _ in 0..seats {
        all_miss = (all_miss * miss).div_ceil(n);
    }
    ((SCALE - all_miss.min(SCALE)) * u128::from(PPM) / SCALE) as u64
}

/// **The cache-altering lie's closed form.** A lie in interval `interval` that changes the cache
/// is caught by a seat whose draw includes `interval` (the row replay) or any later interval (the
/// state-root comparison at that interval's anchor) — so it escapes a seat only when all `k` of its
/// draws fall before it: `C(i, k) / C(N, k)`. The panel's chance is `1 − (C(i, k) / C(N, k))^s`,
/// rounded as [`palw_seat_detection_ppm_v1`] rounds. At the last interval it IS that function;
/// earlier, it is larger, which is why the forger's best cache-altering lie is at the end.
pub fn palw_seat_state_or_row_detection_ppm_v1(interval_count: u32, k: u32, seats: u32, interval: u32) -> u64 {
    if interval_count == 0 || k == 0 || seats == 0 || interval >= interval_count {
        return 0;
    }
    if k >= interval_count || interval < k {
        // Fewer than `k` intervals precede it: some draw lands at or after it, every time.
        return PPM;
    }
    // One seat's miss: every draw in `0..interval`, `Π_{t<k} (i − t) / (N − t)`, rounded up.
    let mut seat_miss = SCALE;
    for t in 0..k {
        seat_miss = (seat_miss * u128::from(interval - t)).div_ceil(u128::from(interval_count - t));
    }
    let mut all_miss = SCALE;
    for _ in 0..seats {
        all_miss = (all_miss * seat_miss).div_ceil(SCALE);
    }
    ((SCALE - all_miss.min(SCALE)) * u128::from(PPM) / SCALE) as u64
}

/// The smallest `k` a seat must draw for the panel's `seats` to reach `target_ppm` on a job of
/// `interval_count` intervals — `None` only for a target above certainty.
pub fn palw_draws_for_detection_v1(interval_count: u32, seats: u32, target_ppm: u64) -> Option<u32> {
    if target_ppm > PPM || interval_count == 0 || seats == 0 {
        return None;
    }
    (1..=interval_count).find(|k| palw_seat_detection_ppm_v1(interval_count, *k, seats) >= target_ppm)
}

/// The smallest number of replaying seats that reaches `target_ppm` at `k` draws each.
pub fn palw_seats_for_detection_v1(interval_count: u32, k: u32, target_ppm: u64, max_seats: u32) -> Option<u32> {
    (1..=max_seats).find(|s| palw_seat_detection_ppm_v1(interval_count, k, *s) >= target_ppm)
}

/// **The capture arm's leaf sample, as the same arithmetic.** A seat checking a served capture
/// (`fp_capture_samples_clear`) opens leaf 0 and then draws leaves uniformly WITH replacement;
/// a one-leaf lie anywhere but leaf 0 is found only if a random draw lands on it. That is
/// [`palw_seat_detection_ppm_v1`] with one leaf per draw — `N` the capture's leaf count, `k = 1`,
/// and one "seat" per random draw — which is why the two arms can be compared in one table.
pub fn palw_capture_sample_detection_ppm_v1(step_leaf_count: u64, random_draws: u32) -> u64 {
    let leaves = u32::try_from(step_leaf_count).unwrap_or(u32::MAX);
    palw_seat_detection_ppm_v1(leaves, 1, random_draws)
}

/// **The measurement**: for each interval, in how many of `trials` beacons at least one of the
/// panel's seats drew it — with the draw the seats run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSeatCoverageMeasuredV1 {
    pub interval_count: u32,
    pub k: u32,
    pub seats: u8,
    pub trials: u32,
    /// `hits[i]`: the trials in which some seat drew interval `i` — the row replay.
    pub hits: Vec<u32>,
    /// `state_or_row_hits[i]`: the trials in which some seat drew interval `i` or a later one —
    /// what catches a lie that alters the cache.
    pub state_or_row_hits: Vec<u32>,
}

impl PalwSeatCoverageMeasuredV1 {
    fn ppm_of(&self, hits: u32) -> u64 {
        u64::from(hits) * PPM / u64::from(self.trials.max(1))
    }

    /// The least-drawn interval's rate — the forger's best place for the lie.
    pub fn min_ppm(&self) -> u64 {
        self.hits.iter().map(|h| self.ppm_of(*h)).min().unwrap_or(0)
    }

    /// The most-drawn interval's rate.
    pub fn max_ppm(&self) -> u64 {
        self.hits.iter().map(|h| self.ppm_of(*h)).max().unwrap_or(0)
    }

    /// The cache-altering lie's rate at `interval`.
    pub fn state_or_row_ppm(&self, interval: u32) -> u64 {
        self.state_or_row_hits.get(interval as usize).map(|h| self.ppm_of(*h)).unwrap_or(0)
    }

    /// The mean over intervals: a lie placed uniformly, and the expected fraction of the job the
    /// panel's sampling covers on one claim.
    pub fn mean_ppm(&self) -> u64 {
        let total: u64 = self.hits.iter().map(|h| u64::from(*h)).sum();
        let denominator = u64::from(self.trials.max(1)) * u64::from(self.interval_count.max(1));
        total * PPM / denominator
    }
}

/// The beacon of trial `t`: distinct per trial and chosen by nobody. The draw keys it into a
/// BLAKE2b state beside the claim and the seat index, so a structured beacon draws exactly as a
/// random one does — which is what the agreement with the closed form checks.
fn measurement_beacon_v1(trial: u32) -> Hash64 {
    Hash64::from_u64_word(u64::from(trial) + 1)
}

/// **Run the panel's draw `trials` times** — seats `0..seats`, each drawing `k` intervals with
/// [`palw_fp_interval_draw_v1`] under one beacon per trial — and count, per interval, the trials in
/// which at least one seat drew it. Deterministic: the same arguments give the same counts on every
/// host, so a measured number can be pinned.
pub fn palw_seat_coverage_measured_v1(
    network_domain: &Hash64,
    claim_id: &Hash64,
    interval_count: u32,
    k: u32,
    seats: u8,
    trials: u32,
) -> PalwSeatCoverageMeasuredV1 {
    let n = interval_count as usize;
    let mut hits = vec![0u32; n];
    let mut stamp = vec![u32::MAX; n];
    // The latest interval any seat drew, per trial, as a histogram: interval `i` is caught through
    // the state by every trial whose latest draw is at or after it.
    let mut latest = vec![0u32; n];
    for trial in 0..trials {
        let beacon = measurement_beacon_v1(trial);
        let mut furthest: Option<usize> = None;
        for seat in 0..seats {
            for index in palw_fp_interval_draw_v1(network_domain, &beacon, claim_id, seat, k, interval_count) {
                let i = index as usize;
                if stamp[i] != trial {
                    stamp[i] = trial;
                    hits[i] += 1;
                }
                furthest = Some(furthest.map_or(i, |f| f.max(i)));
            }
        }
        if let Some(f) = furthest {
            latest[f] += 1;
        }
    }
    let mut state_or_row_hits = vec![0u32; n];
    let mut suffix = 0u32;
    for i in (0..n).rev() {
        suffix += latest[i];
        state_or_row_hits[i] = suffix;
    }
    PalwSeatCoverageMeasuredV1 { interval_count, k, seats, trials, hits, state_or_row_hits }
}

/// Whether `hits` out of `trials` lies within `z` binomial standard deviations of `p_ppm`:
/// `(h·PPM − T·p)² ≤ z²·T·p·(PPM − p)`, in integers. At `p` of 0 or certainty the band is zero
/// wide and only the exact count passes.
pub fn palw_within_binomial_v1(hits: u64, trials: u64, p_ppm: u64, z: u64) -> bool {
    let p = i128::from(p_ppm.min(PPM));
    let deviation = i128::from(hits) * i128::from(PPM) - i128::from(trials) * p;
    let band = i128::from(z * z) * i128::from(trials) * p * (i128::from(PPM) - p);
    deviation * deviation <= band
}

/// **What a shard-stratified panel costs** (ADR-0098 Decision 5): every seat holds one shard and
/// is drawn for it, `seats_per_shard` of them per shard, and a claim is licensed when every shard
/// has its own quorum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwShardedPanelV1 {
    pub shards: u32,
    pub seats_per_shard: u32,
    pub quorum_per_shard: u32,
    /// `shards × seats_per_shard`.
    pub panel_seats: u64,
    /// `shards × quorum_per_shard`: what one licensing object must carry.
    pub licensing_receipts: u64,
    /// That object on the wire — `palw_receipt_licensed_wire_bytes_v1`.
    pub licensing_bytes: u64,
    /// Whether it fits one standard transaction (`PALW_STANDARD_TX_BYTES`), which is where a
    /// licensing object rides today.
    pub fits_one_standard_transaction: bool,
    /// A one-leaf lie lives in ONE shard and only that shard's seats can replay it, so the panel's
    /// chance of drawing it is `palw_seat_detection_ppm_v1(N, k, seats_per_shard)` — the same at
    /// every shard count. Sharding does not dilute coverage; it multiplies the panel.
    pub detection_ppm: u64,
}

pub fn palw_sharded_panel_v1(
    shards: u32,
    seats_per_shard: u32,
    quorum_per_shard: u32,
    interval_count: u32,
    k: u32,
) -> PalwShardedPanelV1 {
    let licensing_receipts = u64::from(shards) * u64::from(quorum_per_shard);
    let licensing_bytes = palw_receipt_licensed_wire_bytes_v1(licensing_receipts);
    PalwShardedPanelV1 {
        shards,
        seats_per_shard,
        quorum_per_shard,
        panel_seats: u64::from(shards) * u64::from(seats_per_shard),
        licensing_receipts,
        licensing_bytes,
        fits_one_standard_transaction: licensing_bytes <= PALW_STANDARD_TX_BYTES,
        detection_ppm: palw_seat_detection_ppm_v1(interval_count, k, seats_per_shard),
    }
}

/// The widest shard count whose per-shard quorum still licenses in one standard transaction.
pub fn palw_widest_shard_count_in_one_transaction_v1(quorum_per_shard: u32) -> u32 {
    (1..=u32::MAX / quorum_per_shard.max(1))
        .take_while(|shards| {
            palw_receipt_licensed_wire_bytes_v1(u64::from(*shards) * u64::from(quorum_per_shard)) <= PALW_STANDARD_TX_BYTES
        })
        .last()
        .unwrap_or(0)
}

/// The shipped draw: `k` is [`PALW_FP_SEAT_INTERVAL_SAMPLES_V1`].
pub const fn palw_shipped_draws_per_seat_v1() -> u32 {
    PALW_FP_SEAT_INTERVAL_SAMPLES_V1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The closed form, asked a second way: one exact rational with a single rounding, against the
    /// stepwise product this module ships. They may differ by the per-step roundings, and only in
    /// the direction that understates detection.
    #[test]
    fn the_closed_form_is_the_exact_rational_rounded_down() {
        for (n, k, s) in [(299u32, 4u32, 5u32), (299, 4, 3), (1023, 4, 5), (5, 4, 5), (64, 4, 1), (2, 1, 3)] {
            let (nn, mm) = (u128::from(n), u128::from(n - k.min(n)));
            let exact = (nn.pow(s) - mm.pow(s)) * u128::from(PPM) / nn.pow(s);
            let shipped = u128::from(palw_seat_detection_ppm_v1(n, k, s));
            assert!(shipped <= exact && exact - shipped <= u128::from(s), "N={n} k={k} s={s}: shipped {shipped} exact {exact}");
        }
        assert_eq!(palw_seat_detection_ppm_v1(4, 4, 1), PPM, "k ≥ N is the whole job");
        assert_eq!(palw_seat_detection_ppm_v1(1, 4, 5), PPM, "the hybrid's one-interval job is replayed whole");
        assert_eq!(palw_seat_detection_ppm_v1(0, 4, 5), 0);
        assert_eq!(palw_seat_detection_ppm_v1(299, 4, 0), 0);
        assert!(palw_seat_detection_ppm_v1(299, 4, 5) > palw_seat_detection_ppm_v1(299, 4, 3), "more replaying seats, more coverage");
        assert!(
            palw_seat_detection_ppm_v1(299, 4, 5) > palw_seat_detection_ppm_v1(1023, 4, 5),
            "a longer job, less coverage at a fixed k"
        );
    }

    /// The same number, measured: the draw the seats run, over deterministic beacons, agrees with
    /// the closed form within five binomial deviations — on the mean and on every single interval,
    /// which is what "uniform" has to mean for a forger choosing where to lie.
    #[test]
    fn the_measured_draw_agrees_with_the_closed_form_on_every_interval() {
        let network = Hash64::from_u64_word(11);
        let claim = Hash64::from_u64_word(22);
        for (n, s, trials) in [(12u32, 5u8, 4_000u32), (64, 5, 4_000), (299, 5, 3_000), (299, 3, 3_000)] {
            let k = palw_shipped_draws_per_seat_v1();
            let measured = palw_seat_coverage_measured_v1(&network, &claim, n, k, s, trials);
            let p = palw_seat_detection_ppm_v1(n, k, u32::from(s));
            let total: u64 = measured.hits.iter().map(|h| u64::from(*h)).sum();
            assert!(
                palw_within_binomial_v1(total, u64::from(trials) * u64::from(n), p, 5),
                "N={n} s={s}: mean {} ppm against the closed form's {p}",
                measured.mean_ppm()
            );
            for (interval, hits) in measured.hits.iter().enumerate() {
                assert!(
                    palw_within_binomial_v1(u64::from(*hits), u64::from(trials), p, 6),
                    "N={n} s={s}: interval {interval} was drawn in {hits}/{trials} trials against {p} ppm"
                );
            }
        }
    }

    /// The cache-altering lie, both ways: measured through the draw's latest index, and in closed
    /// form. At the last interval it is exactly the row replay's number; before it, never less.
    #[test]
    fn a_lie_the_cache_records_is_caught_by_any_later_draw_and_the_last_interval_is_the_floor() {
        let (n, k, s, trials) = (64u32, palw_shipped_draws_per_seat_v1(), 5u8, 4_000u32);
        let measured = palw_seat_coverage_measured_v1(&Hash64::from_u64_word(5), &Hash64::from_u64_word(6), n, k, s, trials);
        for interval in [0u32, 3, 4, 16, 32, 48, 62, 63] {
            let p = palw_seat_state_or_row_detection_ppm_v1(n, k, u32::from(s), interval);
            assert!(
                palw_within_binomial_v1(u64::from(measured.state_or_row_hits[interval as usize]), u64::from(trials), p, 6),
                "interval {interval}: measured {} ppm against {p}",
                measured.state_or_row_ppm(interval)
            );
            assert!(p >= palw_seat_detection_ppm_v1(n, k, u32::from(s)), "the state path only adds");
        }
        let last = palw_seat_state_or_row_detection_ppm_v1(n, k, u32::from(s), n - 1);
        let row = palw_seat_detection_ppm_v1(n, k, u32::from(s));
        assert!(
            last.abs_diff(row) <= u64::from(s) + u64::from(k),
            "at the last interval the two paths are one number: {last} vs {row}"
        );
        assert_eq!(palw_seat_state_or_row_detection_ppm_v1(n, k, 5, 2), PPM, "fewer than k intervals before it: always overtaken");
        assert_eq!(palw_seat_state_or_row_detection_ppm_v1(n, k, 5, n), 0, "past the job is no interval");
    }

    /// A job no longer than the draw is replayed whole by every seat — measured, not assumed.
    #[test]
    fn a_job_no_longer_than_the_draw_is_covered_with_certainty() {
        let measured = palw_seat_coverage_measured_v1(&Hash64::from_u64_word(1), &Hash64::from_u64_word(2), 4, 4, 1, 50);
        assert_eq!(measured.min_ppm(), PPM);
        assert_eq!(palw_seat_coverage_measured_v1(&Hash64::from_u64_word(1), &Hash64::from_u64_word(2), 1, 4, 3, 10).min_ppm(), PPM);
    }

    /// The shard arithmetic reads the tree's own receipt size and transaction bound, and the
    /// LIMITATION it finds is pinned: a per-shard quorum of three licenses in one standard
    /// transaction for at most eight shards.
    #[test]
    fn a_stratified_panel_keeps_its_coverage_and_outgrows_one_transaction_past_eight_shards() {
        assert_eq!(palw_widest_shard_count_in_one_transaction_v1(3), 8);
        let eight = palw_sharded_panel_v1(8, 5, 3, 299, 4);
        let nine = palw_sharded_panel_v1(9, 5, 3, 299, 4);
        assert!(eight.fits_one_standard_transaction && !nine.fits_one_standard_transaction);
        assert_eq!(eight.panel_seats, 40);
        assert_eq!(eight.licensing_bytes, palw_receipt_licensed_wire_bytes_v1(24));
        for shards in [1u32, 2, 8, 64] {
            assert_eq!(
                palw_sharded_panel_v1(shards, 5, 3, 299, 4).detection_ppm,
                palw_seat_detection_ppm_v1(299, 4, 5),
                "stratified by shard, the coverage of a one-leaf lie does not depend on the shard count"
            );
        }
        assert_eq!(palw_sharded_panel_v1(1, 5, 3, 299, 4).licensing_bytes, 14_385, "one shard is today's panel");
    }

    #[test]
    fn the_inverse_questions_find_the_smallest_draw_and_panel() {
        let k = palw_draws_for_detection_v1(299, 5, 500_000).expect("certainty is reachable");
        assert!(palw_seat_detection_ppm_v1(299, k, 5) >= 500_000 && palw_seat_detection_ppm_v1(299, k - 1, 5) < 500_000);
        assert_eq!(palw_draws_for_detection_v1(299, 5, PPM), Some(299), "certainty needs the whole job");
        assert_eq!(palw_draws_for_detection_v1(299, 5, PPM + 1), None);
        let s = palw_seats_for_detection_v1(299, 4, 100_000, 64).expect("reachable within 64 seats");
        assert!(palw_seat_detection_ppm_v1(299, 4, s) >= 100_000 && palw_seat_detection_ppm_v1(299, 4, s - 1) < 100_000);
        assert_eq!(palw_capture_sample_detection_ppm_v1(4, 3), palw_seat_detection_ppm_v1(4, 1, 3));
    }

    #[test]
    fn the_binomial_band_is_zero_wide_at_the_edges() {
        assert!(palw_within_binomial_v1(10, 10, PPM, 6));
        assert!(!palw_within_binomial_v1(9, 10, PPM, 6));
        assert!(palw_within_binomial_v1(0, 10, 0, 6));
        assert!(palw_within_binomial_v1(500, 1_000, 500_000, 1));
        assert!(!palw_within_binomial_v1(700, 1_000, 500_000, 5));
    }
}
