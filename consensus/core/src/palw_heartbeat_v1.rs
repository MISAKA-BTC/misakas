//! **The heartbeat lane** — ADR-0060 Decisions 1 and 2 as ADR-0066 redesigned them: time is
//! permissionless, and the price of time never touches `header.bits`.
//!
//! A `ConsensusV2` network whose `palw_heartbeat` fence is in force accepts, beside its two bonded
//! PALW lanes, a **bondless, claimless, near-weightless clock** on its own algorithm id
//! ([`crate::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1`]). A heartbeat block advances the DAA (so every
//! PALW timeout — bind, receipt, challenge, court, withdrawal — sweeps on a clock no bond can
//! stop), carries ordinary transactions (so bond registration and funding can ride it when no
//! bonded lane is alive), and contributes a fixed [`HEARTBEAT_BLUE_WORK_EPSILON`] to fork choice.
//!
//! ## The two rules, and where each one lives
//!
//! * **The price is a network constant** ([`crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2`]),
//!   substituted for `header.bits` inside `StateLayer0::new`. A heartbeat header's `bits` are the
//!   GLOBAL expected bits, like every other lane's, so heartbeat rows enter the difficulty window
//!   as ordinary rows.
//! * **The slot rule** ([`check_heartbeat_slot`]) is one block deep: a heartbeat must sit at least
//!   one interval after its SELECTED PARENT's timestamp. No walk, no window, no ancestor evidence.
//!
//! ## Why both of those are different from the first implementation
//!
//! The 2026-08-30 audit recorded four structural findings and ADR-0066 sorted them by mechanism.
//! Two were `bits`:
//!
//! 1. **The lane could price the bonded lane off its own chain, permanently.** Heartbeat headers
//!    carried the lane's own 2²⁴-hard `bits`, and those rows sat in the GLOBAL difficulty window.
//!    A V2 network's ambient target is `MAX_DIFFICULTY_TARGET` — work 2 — because the class
//!    lottery, not the hash target, is its throttle. Measured over the shipped 264-row window:
//!    255 bonded + 9 heartbeat rows still demanded work 2, but **0 bonded + 263 heartbeat rows
//!    demanded 33,554,432**. After a bonded outage longer than the window a returning producer
//!    needed ~33 M inferences for one block, so no bonded block could re-enter the window, so the
//!    average never re-mixed: a heartbeat-only chain recoverable only by re-mint, which is the
//!    self-feeding refusal ADR-0060 exists to abolish, reintroduced by its own remedy.
//! 3b. **The retarget could never rise above its floor**, because the slot rule guaranteed
//!    `measured ≥ expected` and the clamp turned that back into the floor.
//!
//! A fixed target removes the quantity that fed back on itself, so 1 and 3b are gone as arithmetic
//! rather than as tuning. The retarget is deleted, not bounded.
//!
//! One was node-local:
//!
//! 4. **The evidence walk terminated on `Err(get_header) => break`** — a fact about THIS node.
//!    An archival node never hit it; a pruned node hit it at its own pruning point. Two honest
//!    nodes computed different verdicts for one header: a partition along the `--archival` flag.
//!    A retarget is what needed ancestor evidence, and there is no retarget, so the walk is gone
//!    and the slot rule reads the selected parent alone.
//!
//! And one is **still open, recorded rather than closed**:
//!
//! 3a. **Sibling width.** The slot rule bounds the chain, not the DAG: sibling heartbeats share
//!    one selected parent, so they share one admissible timestamp, and nothing here bounds how
//!    many of them exist. What bounds width is the price, which is now a fixed 2²⁴ per block
//!    rather than a floor a retarget could never leave. ADR-0066 records 3a as open.
//!
//! 2. **ε against a V2 block's work** is independent of all of the above and survives untouched —
//!    on a V2 preset `calc_work(0x207fffff) = 2`, so a heartbeat is worth half a bonded block.
//!    ADR-0066 Decision 3 is the fix (a V2 attempt block's blue work should reflect the inference
//!    it carries, not the hash target it did not need); it moves `header.blue_work` on every V2
//!    block and is deliberately staged after this.
//!
//! ## The ramp, and why it has two steps instead of three
//!
//! The interval is a step function of the SELECTED PARENT's lane: after a bonded block the chain
//! was alive one block ago, so the lane waits the full nominal hour; after a heartbeat the chain
//! is already running on the clock, so the lane runs at the recovery cadence.
//!
//! The old middle step ("above one hour of bonded silence, one per ten minutes") is gone because
//! it asked *how long has the bonded lane been silent*, and that is ancestor evidence — finding 4
//! in one question. One block deep admits exactly two states, and they are the two that matter:
//! the chain is producing, or it is not.
//!
//! ## What is deliberately NOT here
//!
//! No bond, no claim, no escrow, no court: a hash proof is self-verifying, so there is nothing to
//! slash and nobody to license. The coinbase rule (a heartbeat block's declared subsidy is zero —
//! fees only) lives with the other coinbase validation in the body processor; the ε fork-choice
//! rule lives in the GHOSTDAG protocol beside the receipt lane's zero. Both cite this module.

use crate::pow_layer0::{POW_ALGO_ID_HEARTBEAT_V1, is_palw_v2_algo_id};

/// The heartbeat lane's algorithm id. See [`POW_ALGO_ID_HEARTBEAT_V1`] for why it is its own id
/// and no longer `POW_ALGO_ID_BLAKE2B_SHA3`.
pub const PALW_HEARTBEAT_ALGO_ID: u8 = POW_ALGO_ID_HEARTBEAT_V1;

/// Nominal cadence: one heartbeat per hour (≈ 24/day ≈ 33‰ of the 120 s cadence) — the interval
/// that applies when the selected parent is a bonded block, i.e. the chain is producing.
pub const HEARTBEAT_NOMINAL_INTERVAL_MS: u64 = 3_600_000;

/// The recovery cadence: the full 120 s block time, applied when the selected parent is itself a
/// heartbeat — timeout sweeping at normal speed with every bonded lane dead.
pub const HEARTBEAT_RECOVERY_INTERVAL_MS: u64 = 120_000;

/// **ε: the whole fork-choice weight of a heartbeat block** — the named exception to ADR-0045's
/// DerivedV1 work equality that ADR-0060 Decision 1.2 is. One unit: any bonded PALW block
/// (≈ 10⁶ work) outweighs a million heartbeats, while among heartbeat-only branches (total
/// collapse) `ε × n` still orders the longer chain first — which zero (the receipt lane's figure)
/// would not.
///
/// **This value is known to be too large against a V2 block and is not the fix.** See finding 2 in
/// the module header: ADR-0066 Decision 3 moves the OTHER side of the comparison.
pub const HEARTBEAT_BLUE_WORK_EPSILON: u64 = 1;

/// The interval a heartbeat is held to, given the lane of its selected parent.
///
/// One block deep by construction — the argument is the parent's algo id and nothing else, so
/// there is no walk to bound, no window to sample and no node-local fact to terminate on.
pub fn heartbeat_interval_ms(selected_parent_algo_id: u8) -> u64 {
    if is_palw_v2_algo_id(selected_parent_algo_id) {
        // A bonded block one block ago: the chain is producing and the lane stays out of the way.
        HEARTBEAT_NOMINAL_INTERVAL_MS
    } else {
        // The parent is a heartbeat (or anything else this network admits): the chain is running
        // on the clock, so the clock runs at cadence.
        HEARTBEAT_RECOVERY_INTERVAL_MS
    }
}

/// Why a heartbeat header was refused by the slot rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeartbeatTooEarly {
    pub last_heartbeat_timestamp: u64,
    pub interval_ms: u64,
}

/// **The slot rule**: a heartbeat header must sit at least one [`heartbeat_interval_ms`] after its
/// SELECTED PARENT's timestamp.
///
/// The old rule measured against the youngest heartbeat in the POV's DAA window, which needed a
/// chain-order walk — and that walk terminated on a node-local fact (finding 4). This asks one
/// question of one header the caller already holds.
///
/// It bounds the CHAIN, not the DAG: siblings share a selected parent and therefore share one
/// admissible timestamp. That is finding 3a and it is open; the fixed price is what bounds width.
pub fn check_heartbeat_slot(
    selected_parent_timestamp: u64,
    selected_parent_algo_id: u8,
    header_timestamp: u64,
) -> Result<(), HeartbeatTooEarly> {
    let interval_ms = heartbeat_interval_ms(selected_parent_algo_id);
    // **`checked_add`, not `saturating_add`.** Saturating clamps the earliest admissible time DOWN
    // to `u64::MAX`, so a parent near the top of the range would admit a header at zero distance —
    // the arithmetic failing OPEN, on the one rule whose whole job is to refuse. Overflow here is
    // not reachable through the timestamp rules, which is exactly the reasoning that leaves a
    // fail-open path in place; the closed direction costs nothing.
    match selected_parent_timestamp.checked_add(interval_ms) {
        Some(earliest) if header_timestamp >= earliest => Ok(()),
        _ => Err(HeartbeatTooEarly { last_heartbeat_timestamp: selected_parent_timestamp, interval_ms }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pow_layer0::{PALW_HEARTBEAT_WORK_LOG2, POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3};

    /// **The ramp has exactly two steps, and which one applies is a question about ONE header.**
    ///
    /// The old ladder had three, keyed on "how long has the bonded lane been silent" — which is
    /// ancestor evidence, and the walk that answered it terminated on a node-local fact (finding
    /// 4: an archival node never hit `Err(get_header)`, a pruned node hit it at its own pruning
    /// point, and the two computed different verdicts for one header). One block deep admits two
    /// states because that is how many a single parent can distinguish.
    #[test]
    fn the_interval_is_a_function_of_the_parents_lane_and_nothing_else() {
        // A bonded parent means the chain was producing one block ago: stay out of the way.
        assert_eq!(heartbeat_interval_ms(POW_ALGO_ID_PALW_COMMITTED_V2), HEARTBEAT_NOMINAL_INTERVAL_MS);
        assert_eq!(heartbeat_interval_ms(POW_ALGO_ID_PALW_RECEIPT_V3), HEARTBEAT_NOMINAL_INTERVAL_MS);
        // A heartbeat parent means the chain is already running on the clock: run at cadence.
        assert_eq!(heartbeat_interval_ms(PALW_HEARTBEAT_ALGO_ID), HEARTBEAT_RECOVERY_INTERVAL_MS);
        // The recovery step is the real cadence, not a compromise between the two.
        assert!(HEARTBEAT_RECOVERY_INTERVAL_MS < HEARTBEAT_NOMINAL_INTERVAL_MS);
    }

    /// **The slot rule, both sides, against a bonded parent.**
    #[test]
    fn a_heartbeat_waits_a_full_hour_behind_a_producing_chain() {
        let parent_ts = 1_700_000_000_000;
        let too_soon = parent_ts + HEARTBEAT_NOMINAL_INTERVAL_MS - 1;
        assert_eq!(
            check_heartbeat_slot(parent_ts, POW_ALGO_ID_PALW_COMMITTED_V2, too_soon),
            Err(HeartbeatTooEarly { last_heartbeat_timestamp: parent_ts, interval_ms: HEARTBEAT_NOMINAL_INTERVAL_MS })
        );
        // Exactly on the boundary is admitted — the rule is "at least", and an off-by-one here
        // would silently double the effective interval.
        assert!(check_heartbeat_slot(parent_ts, POW_ALGO_ID_PALW_COMMITTED_V2, parent_ts + HEARTBEAT_NOMINAL_INTERVAL_MS).is_ok());
    }

    /// **…and against a heartbeat parent, where the whole point is that it is faster.**
    ///
    /// Asserted as a DIFFERENCE at one timestamp: the same instant that is too early behind a
    /// bonded parent is admissible behind a heartbeat one. A test that only checked each side
    /// separately would pass on a ramp that had collapsed to a single interval.
    #[test]
    fn the_recovery_cadence_is_what_makes_a_stopped_chain_recoverable() {
        let parent_ts = 1_700_000_000_000;
        let one_cadence = parent_ts + HEARTBEAT_RECOVERY_INTERVAL_MS;
        assert!(check_heartbeat_slot(parent_ts, PALW_HEARTBEAT_ALGO_ID, one_cadence).is_ok());
        assert!(
            check_heartbeat_slot(parent_ts, POW_ALGO_ID_PALW_COMMITTED_V2, one_cadence).is_err(),
            "the same instant behind a producing chain is far too early — the ramp is the difference"
        );
        assert!(check_heartbeat_slot(parent_ts, PALW_HEARTBEAT_ALGO_ID, one_cadence - 1).is_err());
    }

    /// A parent timestamp near `u64::MAX` must refuse, never wrap into admitting everything.
    #[test]
    fn the_slot_rule_saturates_rather_than_wrapping() {
        assert!(check_heartbeat_slot(u64::MAX, PALW_HEARTBEAT_ALGO_ID, u64::MAX).is_err());
        assert!(check_heartbeat_slot(u64::MAX - 1, POW_ALGO_ID_PALW_COMMITTED_V2, u64::MAX).is_err());
    }

    /// **The lane has its own id, and it is not the hash lane's.**
    ///
    /// Sharing `POW_ALGO_ID_BLAKE2B_SHA3` was what forced the triple gate, and it meant a solved
    /// header from a hash network was a heartbeat's bytes. The Layer-0 digest binds `pow_algo_id`,
    /// so distinct ids make the two lanes' solutions non-interchangeable by construction.
    #[test]
    fn the_heartbeat_id_is_its_own_and_is_known_to_this_binary() {
        assert_ne!(PALW_HEARTBEAT_ALGO_ID, crate::pow_layer0::POW_ALGO_ID_BLAKE2B_SHA3);
        assert!(!is_palw_v2_algo_id(PALW_HEARTBEAT_ALGO_ID), "it is bondless — not a V2 lineage lane");
        crate::pow_layer0::check_algo_id_known(PALW_HEARTBEAT_ALGO_ID).expect("this binary can derive the heartbeat tag");
    }

    /// **A fixed-price lane must not be able to buy pruning-proof hierarchy.**
    ///
    /// With a constant target a lucky solve lands as far under it as under a hard one, so a level
    /// derived from the digest would be luck sold as structure — at 2²⁴ hashes a go. The receipt
    /// lane answers no for the neighbouring reason (a free digest), and both go through the one
    /// predicate so neither can be exempted by editing the other.
    /// **The heartbeat buys no LEVEL and yet weighs ε — two answers, so two predicates.**
    ///
    /// Reading the shared `algo_id_carries_no_chain_position` for both is a mistake with a silent
    /// failure mode: the ghostdag zero-arm runs before the ε-arm, so the lane would weigh nothing
    /// and a fully collapsed chain could not order its own branches — the exact regime the lane
    /// exists for. This pins the difference.
    #[test]
    fn the_heartbeat_buys_no_hierarchy_but_still_weighs_epsilon() {
        assert!(crate::pow_layer0::algo_id_derives_no_block_level(PALW_HEARTBEAT_ALGO_ID), "a fixed target buys no level");
        assert!(
            !crate::pow_layer0::algo_id_carries_no_chain_position(PALW_HEARTBEAT_ALGO_ID),
            "…but it is not weightless: ε is what orders heartbeat-only branches"
        );
        // The receipt lane answers no to BOTH, which is why one predicate was enough until now.
        assert!(crate::pow_layer0::algo_id_derives_no_block_level(POW_ALGO_ID_PALW_RECEIPT_V3));
        assert!(crate::pow_layer0::algo_id_carries_no_chain_position(POW_ALGO_ID_PALW_RECEIPT_V3));
        // And the attempt lane IS the hierarchy — its digests are inference-priced.
        assert!(!crate::pow_layer0::algo_id_derives_no_block_level(POW_ALGO_ID_PALW_COMMITTED_V2));
        assert!(!crate::pow_layer0::algo_id_carries_no_chain_position(POW_ALGO_ID_PALW_COMMITTED_V2));
    }

    /// The price is a constant this crate states once, and it is the spam floor the withdrawn
    /// design tried to hold with a retarget clamp.
    #[test]
    fn the_price_is_a_constant_and_a_real_one() {
        assert_eq!(PALW_HEARTBEAT_WORK_LOG2, 24, "≈2²⁴ hashes: seconds of one CPU per interval, per BLOCK for a flooder");
        // A price of zero would make sibling flooding free, which is the only thing standing
        // between finding 3a (open) and an unbounded DAG.
        assert!(PALW_HEARTBEAT_WORK_LOG2 > 0);
    }
}
