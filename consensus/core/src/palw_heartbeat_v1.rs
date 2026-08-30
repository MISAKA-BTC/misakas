//! **The heartbeat lane** — ADR-0060 Decisions 1 and 2: time is permissionless.
//!
//! A `ConsensusV2` network accepts, beside its two bonded PALW lanes, the self-verifying
//! BLAKE2b-512 ∥ SHA3-512 hash lane (`algo_id = 3`) as a **bondless, claimless, near-weightless
//! clock**. A heartbeat block advances the DAA (so every PALW timeout — bind, receipt,
//! challenge, court, withdrawal — sweeps on a clock no bond can stop), carries ordinary
//! transactions (so bond registration and funding can ride it when no bonded lane is alive),
//! and contributes a fixed [`HEARTBEAT_BLUE_WORK_EPSILON`] to fork choice (so no amount of hash
//! power buys chain weight).
//!
//! Every rule here is a pure function of DAA-window data — timestamps, bits and algo ids of the
//! blocks the POV already orders — so header validation can enforce the lane without new state.
//!
//! ## The two rules
//!
//! * **The slot rule** ([`check_heartbeat_slot`]): at most one heartbeat block per
//!   [`heartbeat_interval_ms`] of chain time, measured against the youngest heartbeat block in
//!   the POV's DAA window. This is the hard cadence cap.
//! * **The difficulty rule** ([`heartbeat_expected_bits`]): a heartbeat header's `bits` must be
//!   the lane's own retarget — a windowed adjustment over the window's heartbeat blocks toward
//!   one block per interval, floored at [`heartbeat_easiest_target`] (≈ 2²⁴ hashes) so a flood
//!   of sibling heartbeats is never free. This prices spam; the slot rule bounds rate.
//!
//! ## The ramp (ADR-0060 Decision 2)
//!
//! The interval is a step function of **bonded-lane silence** — `header.timestamp` minus the
//! youngest bonded-lane (algo-6/7) timestamp in the window: nominal one block per hour; above
//! one hour of silence, one per ten minutes; above six hours, the full 120 s cadence. Keyed on
//! timestamps, not epochs, because when only the heartbeat is alive an epoch retarget is weeks
//! away — the exact regime the ramp exists for. When a bonded lane produces again the silence
//! resets and the interval steps back up the same ladder.
//!
//! ## What is deliberately NOT here
//!
//! No bond, no claim, no escrow, no court: a hash proof is self-verifying, so there is nothing
//! to slash and nobody to license. The coinbase rule (a heartbeat block's declared subsidy is
//! zero — fees only) lives with the other coinbase validation in the body processor; the ε
//! fork-choice rule lives in the GHOSTDAG protocol beside the receipt lane's zero. Both cite
//! this module.

use crate::pow_layer0::{POW_ALGO_ID_BLAKE2B_SHA3, is_palw_v2_algo_id};
use kaspa_math::{Uint256, Uint320};

/// The heartbeat lane IS the Phase-3 hash lane: same finalizer arm, same L1 tag, re-admitted
/// under the ADR-0060 bounds. A new id would need a new finalizer arm for identical bytes.
pub const PALW_HEARTBEAT_ALGO_ID: u8 = POW_ALGO_ID_BLAKE2B_SHA3;

/// Nominal cadence: one heartbeat per hour (≈ 24/day ≈ 33‰ of the 120 s cadence).
pub const HEARTBEAT_NOMINAL_INTERVAL_MS: u64 = 3_600_000;
/// Above one hour of bonded-lane silence: one per ten minutes.
pub const HEARTBEAT_RAMP1_SILENCE_MS: u64 = 3_600_000;
pub const HEARTBEAT_RAMP1_INTERVAL_MS: u64 = 600_000;
/// Above six hours of bonded-lane silence: the full 120 s cadence — timeout sweeping at normal
/// speed with every bonded lane dead.
pub const HEARTBEAT_RAMP2_SILENCE_MS: u64 = 21_600_000;
pub const HEARTBEAT_RAMP2_INTERVAL_MS: u64 = 120_000;

/// **ε: the whole fork-choice weight of a heartbeat block** — the named exception to
/// ADR-0045's DerivedV1 work equality that ADR-0060 Decision 1.2 is. One unit: any bonded PALW
/// block (≈ 10⁶ work) outweighs a million heartbeats, while among heartbeat-only branches
/// (total collapse) `ε × n` still orders the longer chain first — which zero (the receipt
/// lane's figure) would not.
pub const HEARTBEAT_BLUE_WORK_EPSILON: u64 = 1;

/// How many recent heartbeats the lane retarget measures its span over, and how deep the
/// evidence walk may go hunting for them (and for the youngest bonded block) before giving up.
/// The walk is chain-order over the POV's selected-parent chain — see
/// `processes::heartbeat_evidence` in the consensus crate for why the sampled difficulty
/// window cannot serve this.
pub const HEARTBEAT_RETARGET_ROWS: usize = 32;
pub const HEARTBEAT_EVIDENCE_MAX_BLOCKS: usize = 30_000;

/// The floor on heartbeat difficulty, as work: the easiest admissible target demands ~2²⁴
/// hash evaluations (~seconds of one CPU). A legitimate miner pays it once per interval; a
/// sibling-flooder pays it per block, which is the point — with the trivial global bits a V2
/// network otherwise runs at, a heartbeat header would cost ~20 hashes and relay spam would be
/// free.
pub const HEARTBEAT_MIN_WORK_LOG2: u32 = 24;

/// The easiest target a heartbeat header may declare (see [`HEARTBEAT_MIN_WORK_LOG2`]).
pub fn heartbeat_easiest_target() -> Uint256 {
    Uint256::MAX >> HEARTBEAT_MIN_WORK_LOG2
}

/// The interval the lane is currently held to, from bonded-lane silence (ADR-0060 Decision 2).
pub fn heartbeat_interval_ms(bonded_silence_ms: u64) -> u64 {
    if bonded_silence_ms > HEARTBEAT_RAMP2_SILENCE_MS {
        HEARTBEAT_RAMP2_INTERVAL_MS
    } else if bonded_silence_ms > HEARTBEAT_RAMP1_SILENCE_MS {
        HEARTBEAT_RAMP1_INTERVAL_MS
    } else {
        HEARTBEAT_NOMINAL_INTERVAL_MS
    }
}

/// One DAA-window row, as the heartbeat rules read it. The caller (header validation) builds
/// these from the same window it already fetched for the difficulty check.
#[derive(Clone, Copy, Debug)]
pub struct HeartbeatWindowBlock {
    /// Header timestamp, milliseconds.
    pub timestamp: u64,
    /// Header bits (meaningful for the retarget only on heartbeat rows).
    pub bits: u32,
    /// The header's declared Layer-1 algo id.
    pub algo_id: u8,
}

/// Bonded-lane silence at `header_timestamp`: the time since the youngest bonded-lane
/// (algo-6/7) block in the window. `u64::MAX` when the window holds none — with every bonded
/// lane silent for longer than the window remembers, the ramp's fastest step is the right
/// answer, and saturating at MAX selects it without a sentinel.
pub fn bonded_silence_ms(window: &[HeartbeatWindowBlock], header_timestamp: u64) -> u64 {
    window
        .iter()
        .filter(|b| is_palw_v2_algo_id(b.algo_id))
        .map(|b| b.timestamp)
        .max()
        .map(|last| header_timestamp.saturating_sub(last))
        .unwrap_or(u64::MAX)
}

/// Why a heartbeat header was refused by the slot rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeartbeatTooEarly {
    pub last_heartbeat_timestamp: u64,
    pub interval_ms: u64,
}

/// **The slot rule**: a heartbeat header must sit at least one interval after the youngest
/// heartbeat block its POV window holds. A window with no heartbeat block admits one freely —
/// that is the lane (re)starting, and the difficulty floor still prices it.
pub fn check_heartbeat_slot(window: &[HeartbeatWindowBlock], header_timestamp: u64) -> Result<(), HeartbeatTooEarly> {
    let Some(last) = window.iter().filter(|b| b.algo_id == PALW_HEARTBEAT_ALGO_ID).map(|b| b.timestamp).max() else {
        return Ok(());
    };
    let interval_ms = heartbeat_interval_ms(bonded_silence_ms(window, header_timestamp));
    if header_timestamp < last.saturating_add(interval_ms) {
        return Err(HeartbeatTooEarly { last_heartbeat_timestamp: last, interval_ms });
    }
    Ok(())
}

/// **The difficulty rule**: the `bits` a heartbeat header at `header_timestamp` must declare.
///
/// A windowed retarget over the window's OWN heartbeat rows, toward one block per
/// [`heartbeat_interval_ms`]: `new_target = avg_target × measured_span / expected_span`, the
/// same arithmetic as the global difficulty manager, restricted to the lane. With fewer than
/// two heartbeat rows there is no span to measure and the easiest admissible target is the
/// answer — the lane starting (or restarting after an outage long enough to outlive the
/// window) begins at the floor and walks to the ambient hash rate from there.
///
/// The result is clamped to [`heartbeat_easiest_target`]: the retarget may make the lane
/// arbitrarily hard (that is just hash rate arriving), never cheaper than the spam floor.
pub fn heartbeat_expected_bits(window: &[HeartbeatWindowBlock], header_timestamp: u64) -> u32 {
    let easiest = heartbeat_easiest_target();
    let heartbeats: Vec<&HeartbeatWindowBlock> = window.iter().filter(|b| b.algo_id == PALW_HEARTBEAT_ALGO_ID).collect();
    if heartbeats.len() < 2 {
        return easiest.compact_target_bits();
    }
    let interval_ms = heartbeat_interval_ms(bonded_silence_ms(window, header_timestamp));
    let (min_ts, max_ts) =
        heartbeats.iter().fold((u64::MAX, 0u64), |(lo, hi), b| (lo.min(b.timestamp), hi.max(b.timestamp)));
    let measured_ms = (max_ts - min_ts).max(1);
    // n rows span n-1 intervals.
    let expected_ms = interval_ms.saturating_mul(heartbeats.len() as u64 - 1).max(1);
    // Uint320 for the same reason the difficulty manager uses it: summing Uint256 targets and
    // multiplying by a span must not overflow.
    let targets_sum: Uint320 = heartbeats.iter().map(|b| Uint320::from(Uint256::from_compact_target_bits(b.bits))).sum();
    let average_target = targets_sum / (heartbeats.len() as u64);
    let new_target = average_target * measured_ms / expected_ms;
    let clamped = new_target.min(Uint320::from(easiest));
    Uint256::try_from(clamped).expect("clamped to a Uint256 ceiling").compact_target_bits()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pow_layer0::{POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3};

    fn hb(ts: u64, bits: u32) -> HeartbeatWindowBlock {
        HeartbeatWindowBlock { timestamp: ts, bits, algo_id: PALW_HEARTBEAT_ALGO_ID }
    }
    fn bonded(ts: u64) -> HeartbeatWindowBlock {
        HeartbeatWindowBlock { timestamp: ts, bits: 0x200ccccc, algo_id: POW_ALGO_ID_PALW_COMMITTED_V2 }
    }

    /// The ladder is the ADR's, exactly: nominal below one hour of silence, 1/10 min above it,
    /// full cadence above six hours — boundaries exclusive, so "exactly one hour" is still calm.
    #[test]
    fn the_ramp_ladder_is_the_adr() {
        assert_eq!(heartbeat_interval_ms(0), HEARTBEAT_NOMINAL_INTERVAL_MS);
        assert_eq!(heartbeat_interval_ms(HEARTBEAT_RAMP1_SILENCE_MS), HEARTBEAT_NOMINAL_INTERVAL_MS);
        assert_eq!(heartbeat_interval_ms(HEARTBEAT_RAMP1_SILENCE_MS + 1), HEARTBEAT_RAMP1_INTERVAL_MS);
        assert_eq!(heartbeat_interval_ms(HEARTBEAT_RAMP2_SILENCE_MS), HEARTBEAT_RAMP1_INTERVAL_MS);
        assert_eq!(heartbeat_interval_ms(HEARTBEAT_RAMP2_SILENCE_MS + 1), HEARTBEAT_RAMP2_INTERVAL_MS);
        assert_eq!(heartbeat_interval_ms(u64::MAX), HEARTBEAT_RAMP2_INTERVAL_MS);
    }

    /// A window with no bonded-lane block reads as infinite silence — the fastest ramp step —
    /// and a receipt (algo-7) block is a bonded block for this purpose: both lanes are bonded
    /// production, and either one resets the ramp.
    #[test]
    fn silence_is_measured_against_either_bonded_lane() {
        let now = 10_000_000_000;
        assert_eq!(bonded_silence_ms(&[], now), u64::MAX);
        assert_eq!(bonded_silence_ms(&[hb(now - 50, 0x1d00ffff)], now), u64::MAX, "a heartbeat is not bonded production");
        let receipt = HeartbeatWindowBlock { timestamp: now - 7_000, bits: 0, algo_id: POW_ALGO_ID_PALW_RECEIPT_V3 };
        assert_eq!(bonded_silence_ms(&[bonded(now - 9_000), receipt], now), 7_000);
    }

    /// The slot rule: one heartbeat per interval, and the interval tightens as the ramp fires.
    #[test]
    fn the_slot_rule_caps_cadence_and_the_ramp_loosens_it() {
        let t0 = 1_000_000_000_000u64;
        // Calm network (bonded block just now): interval is the nominal hour.
        let calm = [bonded(t0), hb(t0, 0x1d00ffff)];
        assert!(check_heartbeat_slot(&calm, t0 + HEARTBEAT_NOMINAL_INTERVAL_MS - 1).is_err(), "an hour has not passed");
        assert!(check_heartbeat_slot(&calm, t0 + HEARTBEAT_NOMINAL_INTERVAL_MS).is_ok());
        // Dead network (no bonded block in the window): full cadence.
        let dead = [hb(t0, 0x1d00ffff)];
        assert!(check_heartbeat_slot(&dead, t0 + HEARTBEAT_RAMP2_INTERVAL_MS - 1).is_err());
        assert!(check_heartbeat_slot(&dead, t0 + HEARTBEAT_RAMP2_INTERVAL_MS).is_ok());
        // No heartbeat in the window at all: the lane may (re)start freely.
        assert!(check_heartbeat_slot(&[bonded(t0)], t0 + 1).is_ok());
        assert!(check_heartbeat_slot(&[], t0).is_ok());
    }

    /// Fewer than two heartbeat rows: the floor. The floor is a real price (2²⁴), not free.
    #[test]
    fn the_retarget_starts_at_the_spam_floor() {
        let bits = heartbeat_expected_bits(&[], 0);
        assert_eq!(bits, heartbeat_easiest_target().compact_target_bits());
        let one = [hb(1_000, 0x1d00ffff)];
        assert_eq!(heartbeat_expected_bits(&one, 2_000), heartbeat_easiest_target().compact_target_bits());
        // The floor demands ~2^24 work: target is MAX >> 24.
        assert_eq!(heartbeat_easiest_target(), Uint256::MAX >> 24u32);
    }

    /// The retarget walks toward one block per interval: blocks arriving too fast tighten the
    /// target (harder), too slow ease it (easier), and the easiest it can ever get is the floor.
    #[test]
    fn the_retarget_tracks_the_interval() {
        let t0 = 2_000_000_000_000u64;
        let start_bits = heartbeat_easiest_target().compact_target_bits();
        // Two heartbeats one MINUTE apart on a calm network (expected: one HOUR apart) → the
        // lane is 60× too fast → the new target is ~60× harder than the average.
        let fast = [bonded(t0 + 60_000), hb(t0, start_bits), hb(t0 + 60_000, start_bits)];
        let fast_bits = heartbeat_expected_bits(&fast, t0 + 120_000);
        let fast_target = Uint256::from_compact_target_bits(fast_bits);
        let floor = heartbeat_easiest_target();
        assert!(fast_target < floor, "faster than schedule must tighten below the floor");
        assert!(fast_target > floor >> 8, "one adjustment is bounded (≈60×, well under 256×)");
        // Two heartbeats two hours apart (expected one) → easier — but the floor clamps it.
        let slow = [bonded(t0 + 7_200_000), hb(t0, fast_bits), hb(t0 + 7_200_000, fast_bits)];
        let slow_bits = heartbeat_expected_bits(&slow, t0 + 7_260_000);
        assert!(
            Uint256::from_compact_target_bits(slow_bits) > fast_target,
            "slower than schedule must ease"
        );
        let very_slow = [bonded(t0 + 7_200_000), hb(t0, start_bits), hb(t0 + 7_200_000, start_bits)];
        assert_eq!(
            heartbeat_expected_bits(&very_slow, t0 + 7_260_000),
            start_bits,
            "easing from the floor clamps at the floor"
        );
    }

    /// ε is one, and one is not zero: heartbeat-only branches order by length, while any real
    /// PALW block's work (≥ ~10⁶ at the shipped trivial bits) dwarfs any plausible run of them.
    #[test]
    fn epsilon_is_one_unit_of_work() {
        assert_eq!(HEARTBEAT_BLUE_WORK_EPSILON, 1);
    }
}
