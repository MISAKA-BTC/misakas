//! **The Useful Work Transition's two consensus rules (ADR-0165, revision 2): the floor is retired and the
//! work carries the clock.**
//!
//! * **`Params::palw_floor_reserve_v1` (A′).** Past it a `PALW-BASE-0` attempt is refused by name
//!   (`FloorRetired`), which the fold skips like every other pre-write refusal, so no new floor claim earns a
//!   reward or fork-choice weight. Claims the floor won earlier settle normally. The fence keeps its name; the
//!   class stays registered for settlement only. A rooted ledger records when a REAL attempt (any class other
//!   than the base) was last accepted — the fallback block's idle rule reads it (ADR-0165 §5.2).
//! * **`Params::palw_real_clock_tick_v1` (B).** An attempt-lane block is a tick source beside the heartbeat. The
//!   DAA score still advances **once per clock slot** however many attempts a slot holds.
//!
//! This module holds the pure parts: the idle ledger, the tick-source rule and the fences' plumbing.

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use std::collections::BTreeMap;

/// **K — clock slots without an accepted REAL attempt before the chain is idle** (ADR-0165 §5.2). A slot is
/// one DAA (120 s), so 3 is six minutes: a REAL producer winning a few attempts a slot is never idle between
/// them, and a stall costs at most six minutes of fallback weight. K does not bound the clock — the tick is
/// lane-based and moves regardless.
pub const PALW_REAL_IDLE_K_SLOTS_V1: u64 = 3;
/// The ledger's one key: the DAA a REAL attempt was last accepted at, plus one (so a value of 0 is never stored).
pub const PALW_REAL_LAST_ACCEPT_KEY_V1: u64 = 0;

/// **The heartbeat miner's grace** (node policy, not consensus): after a slot opens it waits this long
/// for an attempt-lane block to carry the tick before it mints a heartbeat. 20 s is a sixth of the
/// interval, so a network with no real producer ticks every 120 + 20 s at the worst.
pub const PALW_REAL_TICK_GRACE_MS_V1: u64 = 20_000;

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

/// **The write one accepted REAL attempt at `daa` makes**: `(key, old, new)`, or `None` where the ledger already
/// holds a DAA at or past it.
pub fn palw_real_accept_note_v1(ledger: &BTreeMap<u64, u64>, daa: u64) -> Option<(u64, Option<u64>, Option<u64>)> {
    let old = ledger.get(&PALW_REAL_LAST_ACCEPT_KEY_V1).copied();
    let new = daa.saturating_add(1);
    (old.is_none_or(|old| new > old)).then_some((PALW_REAL_LAST_ACCEPT_KEY_V1, old, Some(new)))
}

/// **The DAA a REAL attempt was last accepted at**, `None` if none ever was.
pub fn palw_real_last_accept_v1(ledger: &BTreeMap<u64, u64>) -> Option<u64> {
    ledger.get(&PALW_REAL_LAST_ACCEPT_KEY_V1).map(|v| v.saturating_sub(1))
}

/// **Is the chain idle at `now`** — no REAL attempt accepted in the last [`PALW_REAL_IDLE_K_SLOTS_V1`] slots.
/// Absent is idle: a chain that never saw real work (an all-floor network crossing the fence) is live.
pub fn palw_real_idle_at_v1(ledger: &BTreeMap<u64, u64>, now: u64) -> bool {
    palw_real_last_accept_v1(ledger).is_none_or(|last| now.saturating_sub(last) >= PALW_REAL_IDLE_K_SLOTS_V1)
}

/// The values the fingerprint hashes beside the reserve fence's height, so changing K is a new network id.
pub fn palw_floor_reserve_value_v1() -> [u64; 1] {
    [PALW_REAL_IDLE_K_SLOTS_V1]
}

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

    #[test]
    fn idle_detection_reads_the_last_accepted_real_attempt() {
        let mut l = BTreeMap::new();
        assert!(palw_real_idle_at_v1(&l, 5_300), "a chain that never saw real work is idle, so an all-floor network is live");
        let (k, old, new) = palw_real_accept_note_v1(&l, 100).expect("the first");
        assert_eq!((k, old, new), (PALW_REAL_LAST_ACCEPT_KEY_V1, None, Some(101)));
        l.insert(k, 101);
        assert_eq!(palw_real_last_accept_v1(&l), Some(100));
        for now in [100, 101, 102] {
            assert!(!palw_real_idle_at_v1(&l, now), "within K slots of the last real attempt: {now}");
        }
        assert!(palw_real_idle_at_v1(&l, 103), "K = 3 slots without one: idle");
        // Only a LATER acceptance writes; an earlier or equal one (a merged blue at a lower score) does not.
        assert!(palw_real_accept_note_v1(&l, 100).is_none() && palw_real_accept_note_v1(&l, 50).is_none());
        assert_eq!(palw_real_accept_note_v1(&l, 105), Some((PALW_REAL_LAST_ACCEPT_KEY_V1, Some(101), Some(106))));
        let _ = palw_real_idle_at_v1(&BTreeMap::from([(0, u64::MAX)]), 0); // saturating, never a panic
    }
}
