//! **The Useful Work Transition's two consensus rules (ADR-0165): the floor is a reserve, and the
//! clock is carried by the work.**
//!
//! * **`Params::palw_floor_reserve_v1` (A).** `PALW-BASE-0` — the integer-only floor class that every
//!   network holds Active so it can always make blocks — becomes a *reserve*. While real-model work is
//!   being FINALISED at a meaningful rate the floor is dormant: a floor attempt is refused by name
//!   (`FloorDormant`), which the fold skips like every other pre-write refusal, so it earns no reward
//!   and no fork-choice weight (no claim is written). When real work stalls the reserve activates
//!   again, with hysteresis so a rate hovering at the threshold cannot flap it.
//! * **`Params::palw_real_clock_tick_v1` (B).** An attempt-lane block is a tick source beside the
//!   heartbeat. The DAA score still advances **once per clock slot** however many attempts a slot
//!   holds; the cursor, the stamp floor, the lead cap and the merge rules of ADR-0142 are untouched.
//!
//! This module holds the pure parts: the rolling ledger the reserve's mode is read from, and the
//! constants both fences hash into the fingerprint.

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use std::collections::BTreeMap;

/// One ledger bucket, in DAA. A DAA is a 120 s clock slot, so a bucket is 20 minutes.
pub const PALW_REAL_WORK_BUCKET_DAA_V1: u64 = 10;
/// The rolling window, in whole buckets: 6 × 10 = 60 DAA, two hours of clock. A Final trails the work
/// it records by the replay and panel time (measured 7–50 minutes on testnet-12, 4–25 DAA), so the
/// window must hold several lags or a healthy producer reads as stalled between its own Finals.
pub const PALW_REAL_WORK_WINDOW_BUCKETS_V1: u64 = 6;
/// **Dormant at or above this many Final real-class attempt claims in the window** (0.2 a DAA).
pub const PALW_REAL_WORK_DORMANT_AT_V1: u64 = 12;
/// **Active again below this many** (0.05 a DAA). The gap between the two is the hysteresis: a rate
/// has to fall to a quarter of the one that switched the floor off before the reserve returns.
pub const PALW_REAL_WORK_ACTIVE_BELOW_V1: u64 = 3;
/// The ledger key holding the reserve's mode (`1` dormant; absent or `0` active). Bucket indexes are
/// `daa / BUCKET`, which can never reach these.
pub const PALW_REAL_WORK_MODE_KEY_V1: u64 = u64::MAX;
/// The ledger key holding the last bucket the mode was evaluated at.
pub const PALW_REAL_WORK_EVAL_KEY_V1: u64 = u64::MAX - 1;

/// **The heartbeat miner's grace** (node policy, not consensus): after a slot opens it waits this long
/// for an attempt-lane block to carry the tick before it mints a heartbeat. 20 s is a sixth of the
/// interval, so a network with no real producer ticks every 120 + 20 s at the worst.
pub const PALW_REAL_TICK_GRACE_MS_V1: u64 = 20_000;

/// The bucket a DAA score falls in.
#[inline]
pub const fn palw_real_work_bucket_v1(daa: u64) -> u64 {
    daa / PALW_REAL_WORK_BUCKET_DAA_V1
}

/// **Final real-class claims in the window that ends just before `bucket`** — the COMPLETED buckets
/// `[bucket − W, bucket − 1]`. Integer, order-free, saturating.
pub fn palw_real_work_count_v1(ledger: &BTreeMap<u64, u64>, bucket: u64) -> u64 {
    let from = bucket.saturating_sub(PALW_REAL_WORK_WINDOW_BUCKETS_V1);
    ledger.range(from..bucket).fold(0u64, |sum, (_, n)| sum.saturating_add(*n))
}

/// The stored mode: `true` is dormant. Absent is active — the reserve is on until real work proves
/// itself, so the flag day itself cannot switch the floor off.
#[inline]
pub fn palw_real_work_stored_dormant_v1(ledger: &BTreeMap<u64, u64>) -> bool {
    ledger.get(&PALW_REAL_WORK_MODE_KEY_V1).copied().unwrap_or(0) != 0
}

/// **The mode in force at `daa`, as a pure function of the ledger.** Piecewise constant over a bucket:
/// the first block of a new bucket re-evaluates it from the completed window and the stored mode
/// (the hysteresis), and every block of the bucket reads that. The fold writes the result
/// ([`palw_real_work_roll_v1`]); a producer's pre-check reads it without writing, and the two agree
/// because both start from the parent's ledger.
pub fn palw_real_work_dormant_at_v1(ledger: &BTreeMap<u64, u64>, daa: u64) -> bool {
    let bucket = palw_real_work_bucket_v1(daa);
    let stored = palw_real_work_stored_dormant_v1(ledger);
    match ledger.get(&PALW_REAL_WORK_EVAL_KEY_V1) {
        Some(evaluated) if *evaluated >= bucket => stored,
        _ => {
            let count = palw_real_work_count_v1(ledger, bucket);
            if stored { count >= PALW_REAL_WORK_ACTIVE_BELOW_V1 } else { count >= PALW_REAL_WORK_DORMANT_AT_V1 }
        }
    }
}

/// **The writes the first block of a new bucket makes**: the mode, the evaluated bucket, and the
/// buckets that left the window pruned. `(key, old, new)` triples, in key order; empty inside a
/// bucket already evaluated.
pub fn palw_real_work_roll_v1(ledger: &BTreeMap<u64, u64>, daa: u64) -> Vec<(u64, Option<u64>, Option<u64>)> {
    let bucket = palw_real_work_bucket_v1(daa);
    if ledger.get(&PALW_REAL_WORK_EVAL_KEY_V1).is_some_and(|evaluated| *evaluated >= bucket) {
        return Vec::new();
    }
    let dormant = palw_real_work_dormant_at_v1(ledger, daa);
    let mut writes = Vec::new();
    let keep_from = bucket.saturating_sub(PALW_REAL_WORK_WINDOW_BUCKETS_V1);
    for (key, old) in ledger.range(..keep_from) {
        writes.push((*key, Some(*old), None));
    }
    let old_mode = ledger.get(&PALW_REAL_WORK_MODE_KEY_V1).copied();
    let new_mode = if dormant { Some(1) } else { None };
    if old_mode != new_mode {
        writes.push((PALW_REAL_WORK_MODE_KEY_V1, old_mode, new_mode));
    }
    writes.push((PALW_REAL_WORK_EVAL_KEY_V1, ledger.get(&PALW_REAL_WORK_EVAL_KEY_V1).copied(), Some(bucket)));
    writes
}

/// **The ledger key and new value for one Final real-class claim at `daa`.**
pub fn palw_real_work_note_final_v1(ledger: &BTreeMap<u64, u64>, daa: u64) -> (u64, Option<u64>, Option<u64>) {
    let key = palw_real_work_bucket_v1(daa);
    let old = ledger.get(&key).copied();
    (key, old, Some(old.unwrap_or(0).saturating_add(1)))
}

/// **Is the ledger well formed?** Bucket keys only below the sentinels; the mode `0`/`1`/absent.
pub fn palw_real_work_ledger_consistent_v1(ledger: &BTreeMap<u64, u64>) -> bool {
    ledger.get(&PALW_REAL_WORK_MODE_KEY_V1).is_none_or(|mode| *mode == 1)
}

/// The text the fingerprint hashes beside the fence's height, so a change of any constant is a new id.
pub fn palw_floor_reserve_value_v1() -> [u64; 4] {
    [PALW_REAL_WORK_BUCKET_DAA_V1, PALW_REAL_WORK_WINDOW_BUCKETS_V1, PALW_REAL_WORK_DORMANT_AT_V1, PALW_REAL_WORK_ACTIVE_BELOW_V1]
}

/// **The entries a testnet-12 flag-day list takes to arm the Useful Work Transition's two consensus
/// fences** (ADR-0165), through their own `set`, which writes the bundle's mirror.
pub const PALW_T12_FLOOR_RESERVE_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_floor_reserve_v1",
    set: |params, at| {
        params.palw_floor_reserve_v1 = at;
        params.sync_palw_floor_reserve_v1();
    },
};
pub const PALW_T12_REAL_CLOCK_TICK_ENTRY: PalwPostLaunchFenceV1 =
    PalwPostLaunchFenceV1 { name: "palw_real_clock_tick_v1", set: |params, at| params.palw_real_clock_tick_v1 = at };

/// **What a mergeset holds, as the clock reads it** — the counts and newest stamps `palw_clock_step_v1`
/// gathers from the headers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwClockMergesetFactsV1 {
    /// Blocks `bits` prices (any one of them is the clock's own source and no tick source is needed).
    pub priced: u64,
    pub heartbeats: u64,
    pub attempts: u64,
    pub newest_beat_ms: Option<u64>,
    pub newest_attempt_ms: Option<u64>,
}

/// **The tick source rule, one function** (ADR-0165 B; read by `palw_clock_step_v1`, asked by the tests):
/// `(stand_in, source_ms)` — whether the mergeset has a tick source, and the stamp the cursor's slot is
/// measured against (the NEWEST source's; `0` where there is none). With `attempt_ticks` false it is the
/// heartbeat-only rule (ADR-0138 §3b), byte for byte; with it true an attempt-lane block is a source
/// beside the heartbeat. A mergeset has ONE tick however many sources it holds: the caller removes one
/// exemption, never one per source.
pub fn palw_clock_tick_source_v1(facts: &PalwClockMergesetFactsV1, attempt_ticks: bool) -> (bool, u64) {
    let beat = facts.priced == 0 && facts.heartbeats > 0;
    let attempt = attempt_ticks && facts.priced == 0 && facts.attempts > 0;
    let newest = if attempt_ticks { facts.newest_beat_ms.into_iter().chain(facts.newest_attempt_ms).max() } else { facts.newest_beat_ms };
    (beat || attempt, newest.unwrap_or(0))
}

/// **The release's list** (ADR-0165): the Useful Work Transition's two consensus fences, which the
/// DAA-5,300 flag day arms together (lane INT places the entries in its own list).
pub const PALW_T12_USEFUL_WORK_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_T12_FLOOR_RESERVE_ENTRY, PALW_T12_REAL_CLOCK_TICK_ENTRY];

/// The drill's list (`--palw-drill-useful-work-at`, [`crate::config::drill::palw_drill_useful_work_at_v1`]).
pub const PALW_DRILL_USEFUL_WORK_FENCES_V1: &[PalwPostLaunchFenceV1] = PALW_T12_USEFUL_WORK_FENCES_V1;

impl Params {
    /// `palw_floor_reserve_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_floor_reserve_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_floor_reserve_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// Is the floor a reserve at `daa_score`? `false` on every shipped preset.
    pub fn palw_floor_reserve_active_at(&self, daa_score: u64) -> bool {
        self.palw_floor_reserve_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// `palw_real_clock_tick_v1`, resolved.
    pub fn palw_real_clock_tick_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_real_clock_tick_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// Do attempt-lane blocks carry the clock tick at `daa_score`? `false` on every shipped preset.
    pub fn palw_real_clock_tick_active_at(&self, daa_score: u64) -> bool {
        self.palw_real_clock_tick_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The reserve fence's mirror** on the V2 bundle's state params, which the fold reads.
    pub fn sync_palw_floor_reserve_v1(&mut self) {
        let from_daa = self.palw_floor_reserve_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_floor_reserve_from_daa(from_daa);
        }
    }

    /// **The Useful Work Transition's refusals**, asked by [`Params::validate_palw_v2`]:
    ///
    /// * the reserve's mirror disagrees with the fence;
    /// * either fence off a `ConsensusV2` network;
    /// * the reserve without the model registry at or below it — "real class" is a registry fact (an
    ///   Active class other than the base), and a network without a registry has none;
    /// * the clock tick without the cursor, the floor, the lead cap and the single lottery at or below
    ///   it: it is a rule ABOUT the cursor's slots and the lead cap's bound, and an attempt is
    ///   unpriced by `bits` only past the single lottery, so before it an attempt in a mergeset is a
    ///   priced block and no tick source at all.
    pub fn validate_palw_useful_work_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.floor_reserve_from_daa(),
            _ => None,
        };
        let armed = self.palw_floor_reserve_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_floor_reserve_v1 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_floor_reserve_v1",
            ));
        }
        let below = |fence: Option<ForkActivation>, at: u64| fence.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at);
        if let Some(at) = armed {
            if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
                return Err(PalwModeV2Error::Invalid("palw_floor_reserve_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
            }
            if !below(self.palw_model_registry, at) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_floor_reserve_v1 needs palw_model_registry at or below it: a real class is a registry fact",
                ));
            }
        }
        if let Some(at) = self.palw_real_clock_tick_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score()) {
            if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
                return Err(PalwModeV2Error::Invalid("palw_real_clock_tick_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
            }
            if !(below(self.palw_clock_cursor, at)
                && below(self.palw_clock_floor, at)
                && below(self.palw_clock_lead_cap, at)
                && below(self.palw_single_lottery, at)
                && below(self.palw_anchor_clock, at))
            {
                return Err(PalwModeV2Error::Invalid(
                    "palw_real_clock_tick_v1 needs palw_anchor_clock, palw_single_lottery, palw_clock_cursor, palw_clock_floor and \
                     palw_clock_lead_cap at or below it: it adds a tick source to the cursor's slots, under the floor's stamp rules \
                     and the lead cap",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_clock_cursor_v1::{palw_clock_cursor_from_reference_v1, palw_clock_slot_admits_v1};
    use crate::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS as I;

    fn facts(heartbeats: u64, attempts: u64, beat: Option<u64>, attempt: Option<u64>) -> PalwClockMergesetFactsV1 {
        PalwClockMergesetFactsV1 { priced: 0, heartbeats, attempts, newest_beat_ms: beat, newest_attempt_ms: attempt }
    }

    #[test]
    fn without_the_fence_an_attempt_is_no_tick_source_and_with_it_the_newest_source_decides() {
        let f = facts(0, 7, None, Some(500));
        assert_eq!(palw_clock_tick_source_v1(&f, false), (false, 0), "heartbeat-only below the fence");
        assert_eq!(palw_clock_tick_source_v1(&f, true), (true, 500));
        let both = facts(2, 100, Some(900), Some(400));
        assert_eq!(palw_clock_tick_source_v1(&both, true), (true, 900), "the newest of either kind");
        assert_eq!(palw_clock_tick_source_v1(&both, false), (true, 900));
        let priced = PalwClockMergesetFactsV1 { priced: 1, ..f };
        assert_eq!(palw_clock_tick_source_v1(&priced, true).0, false, "a bits-priced block is the clock; no stand-in");
    }

    /// **The clock-safety simulation (ADR-0165 §3.4).** A chain of steps, each merging every tick source
    /// that arrived since the last one; sources are heartbeats or attempts stamped by an adversary anywhere
    /// from `now` to `now + 132 s`, in bursts of up to 100 at one instant. A step is stamped at
    /// `max(now, slot)` and may be withheld by the adversary for a while. Whatever it does, with the
    /// fence ON: (1) the DAA advances at most ONCE per step, whatever the burst; (2) ticks are never
    /// closer than one interval in stamp; (3) over any horizon the DAA has advanced at most
    /// `horizon / interval + 2` — what a heartbeat-only miner can do, and the burst bound `⌊132/120⌋ + 1`.
    #[test]
    fn a_producer_of_any_mix_cannot_run_the_clock_faster_than_a_heartbeat_miner() {
        let mut seed = 0xC10C_u64;
        let mut next = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            seed >> 33
        };
        for attempt_ticks in [false, true] {
            for _round in 0..300 {
                let start = 10_000_000u64;
                let mut now = start;
                let mut reference = start; // the stamp of the block that last advanced the score
                let mut daa = 0u64;
                let mut last_tick_stamp = start;
                let steps = 40 + next() % 80;
                for _ in 0..steps {
                    now += next() % (2 * I);
                    // a burst of sources at this instant: counts and stamps as the adversary likes
                    let n_beats = next() % 4;
                    let n_attempts = next() % 101;
                    let stamp = |r: u64| now + r % 132_001;
                    let newest_beat = (n_beats > 0).then(|| (0..n_beats).map(|_| stamp(next())).max().unwrap());
                    let newest_attempt = (n_attempts > 0).then(|| (0..n_attempts).map(|_| stamp(next())).max().unwrap());
                    let f = facts(n_beats, n_attempts, newest_beat, newest_attempt);
                    let (stand_in, source_ms) = palw_clock_tick_source_v1(&f, attempt_ticks);
                    let cursor = palw_clock_cursor_from_reference_v1(reference, I);
                    let granted = stand_in && palw_clock_slot_admits_v1(&cursor, source_ms).is_ok();
                    if granted {
                        // H5 + the lead cap: the step is stamped at or past its slot and not past now + 132 s.
                        let step_stamp = now.max(cursor.next_slot_ms);
                        if step_stamp > now + 132_000 {
                            continue; // refused by the lead cap: not merged now
                        }
                        daa += 1;
                        assert!(step_stamp >= last_tick_stamp, "stamps of ticks never go back");
                        assert!(step_stamp - reference >= I, "two ticks are at least one interval apart in stamp");
                        last_tick_stamp = step_stamp;
                        reference = step_stamp;
                    }
                }
                let horizon = (now - start).max(1);
                assert!(daa <= horizon / I + 2, "attempt_ticks={attempt_ticks}: {daa} ticks over {horizon} ms (≤ {})", horizon / I + 2);
            }
        }
    }

    /// One tick however many attempts: the exemption count a mergeset leaves is `exempt − 1` for any
    /// number of attempts (the arithmetic `palw_clock_step_v1` performs on `granted`).
    #[test]
    fn a_hundred_attempts_in_one_slot_advance_the_score_by_one() {
        let cursor = palw_clock_cursor_from_reference_v1(1_000, I);
        for attempts in [1u64, 2, 100, 10_000] {
            let f = facts(0, attempts, None, Some(1_000 + I));
            let (stand_in, ms) = palw_clock_tick_source_v1(&f, true);
            let granted = stand_in && palw_clock_slot_admits_v1(&cursor, ms).is_ok();
            let exempt = attempts; // every attempt is exempt past the single lottery
            let after = if granted { exempt.saturating_sub(1) } else { exempt };
            assert_eq!(exempt - after, 1, "{attempts} attempts, one tick");
        }
    }

    fn ledger(finals: &[(u64, u64)]) -> BTreeMap<u64, u64> {
        finals.iter().copied().collect()
    }

    /// Apply a roll's writes the way the fold does.
    fn apply(l: &mut BTreeMap<u64, u64>, writes: Vec<(u64, Option<u64>, Option<u64>)>) {
        for (key, old, new) in writes {
            assert_eq!(l.get(&key).copied(), old, "a write's `old` is what the ledger holds");
            match new {
                Some(v) => l.insert(key, v),
                None => l.remove(&key),
            };
        }
    }

    #[test]
    fn an_empty_ledger_is_active_so_the_flag_day_cannot_switch_the_floor_off() {
        assert!(!palw_real_work_dormant_at_v1(&BTreeMap::new(), 5_300));
        assert!(!palw_real_work_dormant_at_v1(&BTreeMap::new(), u64::MAX));
    }

    #[test]
    fn the_floor_goes_dormant_at_the_threshold_and_not_before() {
        // Buckets 0..=5 are the window ending before bucket 6 (DAA 60..69).
        let just_short = ledger(&[(0, 5), (3, PALW_REAL_WORK_DORMANT_AT_V1 - 6)]);
        assert!(!palw_real_work_dormant_at_v1(&just_short, 60));
        let at = ledger(&[(0, 5), (3, PALW_REAL_WORK_DORMANT_AT_V1 - 5)]);
        assert!(palw_real_work_dormant_at_v1(&at, 60));
        // Finals in the bucket being lived in do not count yet: the window is the completed buckets.
        let current = ledger(&[(6, 100)]);
        assert!(!palw_real_work_dormant_at_v1(&current, 60));
        assert!(palw_real_work_dormant_at_v1(&current, 70));
    }

    #[test]
    fn hysteresis_a_rate_between_the_thresholds_holds_whichever_mode_it_found() {
        let mid = PALW_REAL_WORK_ACTIVE_BELOW_V1 + 2;
        assert!(mid < PALW_REAL_WORK_DORMANT_AT_V1);
        // Active, a middling count does not switch the floor off.
        let mut l = ledger(&[(1, mid)]);
        assert!(!palw_real_work_dormant_at_v1(&l, 60));
        // Dormant, the same count does not bring it back.
        l.insert(PALW_REAL_WORK_MODE_KEY_V1, 1);
        l.insert(PALW_REAL_WORK_EVAL_KEY_V1, 5);
        assert!(palw_real_work_dormant_at_v1(&l, 60));
        // Below the low threshold it does.
        let mut stalled = ledger(&[(1, PALW_REAL_WORK_ACTIVE_BELOW_V1 - 1)]);
        stalled.insert(PALW_REAL_WORK_MODE_KEY_V1, 1);
        stalled.insert(PALW_REAL_WORK_EVAL_KEY_V1, 5);
        assert!(!palw_real_work_dormant_at_v1(&stalled, 60));
    }

    #[test]
    fn the_mode_is_constant_inside_a_bucket_whatever_finalises_in_it() {
        let mut l = ledger(&[(1, 20)]);
        let w = palw_real_work_roll_v1(&l, 60);
        apply(&mut l, w);
        assert!(palw_real_work_dormant_at_v1(&l, 60));
        // Nothing evaluates twice in one bucket, and later Finals do not move the mode until the next.
        assert!(palw_real_work_roll_v1(&l, 69).is_empty());
        for _ in 0..50 {
            let (k, o, n) = palw_real_work_note_final_v1(&l, 65);
            apply(&mut l, vec![(k, o, n)]);
        }
        assert!(palw_real_work_dormant_at_v1(&l, 69));
    }

    #[test]
    fn a_full_cycle_off_on_off_with_the_producers_and_the_pruning() {
        let mut l = BTreeMap::new();
        let mut modes = Vec::new();
        // 40 buckets; real Finals at 3 a bucket for buckets 2..14, none 14..28, 3 a bucket after.
        for bucket in 0..40u64 {
            let daa = bucket * PALW_REAL_WORK_BUCKET_DAA_V1;
            let w = palw_real_work_roll_v1(&l, daa);
            apply(&mut l, w);
            modes.push(palw_real_work_dormant_at_v1(&l, daa));
            let n = if (2..14).contains(&bucket) || bucket >= 28 { 3 } else { 0 };
            for _ in 0..n {
                let (k, o, nw) = palw_real_work_note_final_v1(&l, daa);
                apply(&mut l, vec![(k, o, nw)]);
            }
        }
        // 3 a bucket over 6 buckets = 18 >= 12: dormant from bucket 6 (the window 0..6 holds 12), until the
        // window empties below 3: a stall of 6 buckets from bucket 14 -> reserve active at bucket 20.
        assert!(!modes[5] && modes[6], "dormant once the window holds the threshold");
        assert!(modes[19] && !modes[20], "active again once the window drains below the low threshold");
        assert!(!modes[27] && !modes[28], "and stays active until the rate returns");
        assert!(modes[34] || modes[35], "and dormant again on the rate's return");
        assert!(l.keys().filter(|k| **k < PALW_REAL_WORK_EVAL_KEY_V1).all(|k| *k + PALW_REAL_WORK_WINDOW_BUCKETS_V1 + 1 >= 39), "old buckets pruned");
    }

    /// A deterministic property: the mode at a score is a function of the ledger alone, and a roll
    /// then a read equals a read, so a producer (read) and the fold (roll) cannot disagree.
    #[test]
    fn a_producers_read_equals_the_folds_roll_then_read_over_generated_ledgers() {
        let mut seed = 0x5eed_u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            seed >> 33
        };
        for _ in 0..2_000 {
            let mut l = BTreeMap::new();
            for _ in 0..(next() % 8) {
                l.insert(next() % 40, next() % 20);
            }
            if next() % 2 == 0 {
                l.insert(PALW_REAL_WORK_MODE_KEY_V1, 1);
            }
            if next() % 2 == 0 {
                l.insert(PALW_REAL_WORK_EVAL_KEY_V1, next() % 40);
            }
            let daa = (next() % 45) * PALW_REAL_WORK_BUCKET_DAA_V1 + next() % PALW_REAL_WORK_BUCKET_DAA_V1;
            let read = palw_real_work_dormant_at_v1(&l, daa);
            let mut rolled = l.clone();
            let w = palw_real_work_roll_v1(&l, daa);
            apply(&mut rolled, w);
            assert_eq!(palw_real_work_dormant_at_v1(&rolled, daa), read);
            // Idempotent inside the bucket.
            assert!(palw_real_work_roll_v1(&rolled, daa).is_empty());
        }
    }

    #[test]
    fn counting_saturates_and_never_panics() {
        let l = ledger(&[(0, u64::MAX), (1, u64::MAX)]);
        assert_eq!(palw_real_work_count_v1(&l, 3), u64::MAX);
        assert_eq!(palw_real_work_count_v1(&l, 0), 0);
        let (k, o, n) = palw_real_work_note_final_v1(&l, 0);
        assert_eq!((k, o, n), (0, Some(u64::MAX), Some(u64::MAX)));
    }
}
