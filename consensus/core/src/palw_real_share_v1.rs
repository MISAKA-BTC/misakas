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
