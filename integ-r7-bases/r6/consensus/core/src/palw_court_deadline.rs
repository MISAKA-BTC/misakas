//! **The turn deadline is a slashing rule, so it is derived and checked against the rows that
//! actually ship** — ADR-0077 SA-4.
//!
//! # The defect this module closes
//!
//! `court_turn_deadline` is a CHOSEN constant. [`crate::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1`]
//! sets 60 and [`crate::palw_fp_devnet_v3::PALW_DEVNET_WINDOWS_V1`] sets 4, and until this file
//! nothing in the tree related either number to what an honest responder needs. That silence is
//! the whole defect: a responder who does not answer inside the deadline LOSES, so a deadline
//! shorter than the honest replay of one checkpoint interval on the slowest host, plus the round
//! trip, is a rule that convicts honest participants by the clock — and it would do it without any
//! test going red, because the two numbers were never in one expression.
//!
//! SA-4 states the repair as an inequality with two ends, and both ends are here:
//!
//! ```text
//!   from below   court_turn_deadline  >=  replay(one interval, slowest host) + 2 x NETWORK_DELAY_BOUND
//!   from above   (2 x ceil(log2 leaves) + terminal) x court_turn_deadline  <  window_court
//! ```
//!
//! # What is NOT here, deliberately
//!
//! **No shipped constant moves.** This module derives, asserts and reports; it changes no window,
//! no deadline, no ladder and no class row, so it moves no `palw_ruleset_id_v2`, no
//! `consensus_params_id` and no fingerprint. If a shipped row ever fails the assertion, the answer
//! is a sequenced ruleset move made in daylight, not a looser derivation here.
//!
//! And this is the SHIPPED side of SA-4. [`crate::palw_context_ladder`] carries the same rule for
//! the FENCED ADR-0077 Phase B ladder — the `2^32` rows nothing arms — and every measurement, unit
//! and helper below is imported from it rather than restated. Two spellings of one cost model is
//! the defect class this tree keeps recording; there is one cost model, it lives beside the ladder,
//! and this file is the part of it that reads the constants a node boots on today.
//!
//! # Which end binds, measured
//!
//! At the widths that ship — `n_ctx` 8, 9, 12 and 16 — the replay term is 5.1 seconds at worst and
//! the round trip is 10 seconds, against a 120-second block. **Every shipped row's floor is one
//! DAA**, and the RC ships sixty. The lower bound is therefore not merely satisfied, it is
//! dominated by network delay: replay is a fifth of the sum and the sum is an eighth of a single
//! block. The end that actually binds today is the one from above — the RC's `2^22` ladder spends
//! 2,760 of its 3,000-DAA court window, 8 % of margin — which is why [`Self`]-shaped reasoning
//! about "is 60 enough" was never the risk and "is 60 too much for the ladder" always was.
//! [`tests::the_deadline_is_dominated_by_the_round_trip_not_the_replay`] pins that reading so a
//! later row that inverts it fails here rather than in a dispute.

use crate::Hash64;
use crate::palw_context_ladder::{
    PALW_COURT_COST_A16, PALW_COURT_COST_BASE0, PALW_COURT_COST_QWEN36, PalwCourtRowCostV1, palw_court_replay_floor_daa_v1,
    palw_ladder_fits_window_court_v1,
};
use crate::palw_mode_v2::{PalwConsensusParamsV2, PalwCourtParamsV2};
use crate::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
use crate::palw_step::{PalwShapeProfileV3, PalwStepError};

// =================================================================================================
// What an honest responder replays for ONE court move
// =================================================================================================

/// **How many positions one honest move costs, read off the class's own registered profile.**
///
/// Two regimes, and which one a class is in is a fact about the class rather than a choice:
///
/// * a class that REGISTERS a state chunk map is prosecuted through the anchored form — a
///   refutation at position `p` opens the checkpoint chunk and replays at most `interval`
///   positions after it (ADR-0077 Decision 10; this is what the attention path does today under
///   the v2 map geometry), so the responder replays `min(n_ctx, interval)`;
/// * a class that registers the SENTINEL has no map, and the shipped replay is genesis-anchored —
///   `gdn_core_genesis_replay` walks every prior position — so the responder replays `n_ctx`.
///
/// The second branch is not hypothetical. The shipped hybrid row sets
/// `state_chunk_map_id: Hash64::default()` (`palw_qwen36_profile.rs`, and the profile's own comment
/// says why: "the anchor consumption is wired for attention and not yet for the recurrence"), so
/// the widest honest replay any shipped row demands is that class's whole context and not one
/// interval. A derivation that priced every row at `interval` would have understated the only row
/// that is not anchored, which is exactly the row a deadline would convict first.
///
/// Both inputs are inside the class id — `n_ctx` and `state_chunk_map_id` are fields of the
/// registered [`PalwShapeProfileV3`] and therefore inside `shape_profile_id` — so this is "per
/// class row" in the sense SA-4 means it: a row cannot move its own replay cost without becoming a
/// different class.
pub fn palw_court_replay_positions_v1(profile: &PalwShapeProfileV3, checkpoint_interval: u32) -> u32 {
    let n_ctx = if profile.n_ctx == 0 { 1 } else { profile.n_ctx };
    if profile.state_chunk_map_id == Hash64::default() {
        n_ctx
    } else {
        let interval = if checkpoint_interval == 0 { 1 } else { checkpoint_interval };
        if interval < n_ctx { interval } else { n_ctx }
    }
}

/// **How many blocks a mover needs to LAND one close: one, today.**
///
/// A court move is an assembled close and a close is a standard transaction
/// (`DEFAULT_MAX_CLOSE_BYTES` mirrors the mempool's standard-transaction mass for exactly that
/// reason), so a mover spends one block getting it into the chain. Named as a constant rather than
/// left implicit because it is the term a SPLIT close changes: a close that takes `k` blocks to
/// assemble spends `k` of the mover's deadline, and the deadline the other party is left with
/// shrinks by the same amount. [`palw_court_move_cost_daa_v1`] takes it as a parameter so a rule
/// that splits a close derives its deadline from this expression rather than from a second one.
pub const PALW_COURT_CLOSE_BLOCKS_V1: u64 = 1;

/// **SA-4's floor, in DAA, for one row at one replay length** — the smallest deadline that does
/// not convict an honest responder.
///
/// `max(1, ceil((positions x ms_per_position + 2 x NETWORK_DELAY_BOUND) / cadence)) + (close_blocks - 1)`.
///
/// The first term is [`palw_court_replay_floor_daa_v1`] verbatim — the replay, the round trip, the
/// frozen 120 s cadence and the never-below-one rule all live there, next to the measurements they
/// consume. This function adds the one term that derivation does not carry: the blocks the close
/// itself occupies. At [`PALW_COURT_CLOSE_BLOCKS_V1`] it adds zero, so today's numbers are the
/// ladder module's numbers and nothing is restated.
pub const fn palw_court_move_cost_daa_v1(row: &PalwCourtRowCostV1, replay_positions: u32, close_blocks: u64) -> u64 {
    let replay = palw_court_replay_floor_daa_v1(row, replay_positions);
    replay.saturating_add(close_blocks.saturating_sub(1))
}

/// [`palw_court_move_cost_daa_v1`] over a class row's own geometry — the form a caller with a
/// profile in hand wants, and the one the shipped-row assertion uses.
pub fn palw_court_deadline_floor_daa_v1(
    row: &PalwCourtRowCostV1,
    profile: &PalwShapeProfileV3,
    checkpoint_interval: u32,
    close_blocks: u64,
) -> u64 {
    palw_court_move_cost_daa_v1(row, palw_court_replay_positions_v1(profile, checkpoint_interval), close_blocks)
}

// =================================================================================================
// The two ends, as checks that name what failed
// =================================================================================================

/// What a court's clock can be wrong about. Both arms carry the arithmetic, because a deadline
/// failure read as "assertion failed" is a finding nobody can sequence.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwCourtDeadlineError {
    /// SA-4 from below: the deadline is shorter than the honest replay plus the round trip.
    #[error(
        "{row}: the court convicts by clock — {replay_positions} positions of honest replay plus the round trip need \
         {floor} DAA and the deadline is {deadline} ({measured_on})"
    )]
    ConvictsByClock { row: &'static str, replay_positions: u32, floor: u64, deadline: u64, measured_on: &'static str },
    /// SA-4 from above (ADR-0077 W4): the ladder does not fit the court window at this deadline.
    #[error(
        "the ladder does not fit its own court window: {moves} moves x {deadline} DAA = {worst} against window_court \
         {window_court}"
    )]
    LadderOverrunsWindow { moves: u64, deadline: u64, worst: u64, window_court: u64 },
    /// A preset registers a class this build cannot price. Refused rather than skipped: SA-4 is
    /// "derived and pinned PER CLASS ROW", so a row with no replay measurement is a row whose
    /// deadline nobody checked, and silence there is the defect this module exists to end.
    #[error("the preset registers class {class_id} and no row cost prices it — SA-4 leaves no row underived")]
    UnpricedRow { class_id: Hash64 },
}

/// **SA-4 from below, for one row against one court.**
pub fn palw_court_deadline_admits_row_v1(
    court: &PalwCourtParamsV2,
    row: &PalwCourtRowCostV1,
    profile: &PalwShapeProfileV3,
    checkpoint_interval: u32,
    close_blocks: u64,
) -> Result<u64, PalwCourtDeadlineError> {
    let replay_positions = palw_court_replay_positions_v1(profile, checkpoint_interval);
    let floor = palw_court_move_cost_daa_v1(row, replay_positions, close_blocks);
    let deadline = court.turn_deadline_daa();
    if deadline < floor {
        return Err(PalwCourtDeadlineError::ConvictsByClock {
            row: row.row,
            replay_positions,
            floor,
            deadline,
            measured_on: row.measured_on,
        });
    }
    Ok(floor)
}

/// **SA-4 from above (ADR-0077 W4), for one court against its own window.**
///
/// The move count is the bundle's own ([`PalwCourtParamsV2::worst_case_duration_daa`]) and the
/// predicate is [`palw_ladder_fits_window_court_v1`]; this function is the two of them agreeing.
///
/// **The assembly reserve is the bundle's own too** (ADR-0080 W4): `court.max_close_chunks()`, not
/// the ladder module's default. A close spans as many carriers as THIS ruleset admits, and the
/// window has to hold the blocks THIS ruleset's closes occupy — the RC pays 216 DAA for 27 carriers
/// and the devnet lattice 8 for one, out of windows that differ by the same order.
/// **The shipped clock counts MOVES, not rounds** — `worst_case_duration_daa` is
/// `(2 x bisection_rounds + terminal_rounds) x turn_deadline`, a round being a disclosure and a
/// verdict (audit M2-24) — so ADR-0077 Decision 12's "`(32 + 2) x 60 = 2,040`" is a round count
/// where the code counts moves, and the shipped figure is `(2 x 22 + 2) x 60 = 2,760`.
pub fn palw_court_ladder_fits_window_v1(court: &PalwCourtParamsV2, window_court: u64) -> Result<u64, PalwCourtDeadlineError> {
    let deadline = court.turn_deadline_daa();
    let moves = 2 * u64::from(court.bisection_rounds()) + u64::from(court.terminal_rounds());
    let worst = court.worst_case_duration_daa().unwrap_or(u64::MAX);
    if !palw_ladder_fits_window_court_v1(
        window_court,
        court.max_step_leaf_count(),
        court.terminal_rounds(),
        deadline,
        court.max_close_chunks(),
    ) {
        return Err(PalwCourtDeadlineError::LadderOverrunsWindow { moves, deadline, worst, window_court });
    }
    Ok(worst)
}

// =================================================================================================
// The rows that ship, and the whole check over a bundle
// =================================================================================================

/// One shipped class row, with the class id it registers under and the measurement its replay is
/// priced by.
#[derive(Clone, Debug)]
pub struct PalwShippedCourtRowV1 {
    pub class_id: Hash64,
    pub profile: PalwShapeProfileV3,
    pub cost: PalwCourtRowCostV1,
    /// The checkpoint interval the class's family pins.
    pub checkpoint_interval: u32,
}

/// **Every class row a shipped preset can register, paired with a replay measurement.**
///
/// Derived from the shipped constructors, never from a hand-kept list of ids: a class IS its
/// graph, so the id here is `shape_profile_id()` of the same profile the registration files, and a
/// geometry that moved would show up as an unpriced row rather than as a stale entry that still
/// matches nothing.
pub fn palw_shipped_court_rows_v1() -> Result<Vec<PalwShippedCourtRowV1>, PalwStepError> {
    let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let mut rows = Vec::new();
    let base0 = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)?;
    rows.push(PalwShippedCourtRowV1 {
        class_id: base0.shape_profile_id(),
        profile: base0,
        cost: PALW_COURT_COST_BASE0,
        checkpoint_interval: interval,
    });
    let a16 = crate::palw_qwen25_profile::qwen25_a16_profile_v2(crate::palw_qwen25_profile::QWEN25_1_5B_A16)?;
    rows.push(PalwShippedCourtRowV1 {
        class_id: a16.shape_profile_id(),
        profile: a16,
        cost: PALW_COURT_COST_A16,
        checkpoint_interval: interval,
    });
    // **ADR-0082's graph-v5 512 row, which the testnet-11 genesis registers.** Same family, same
    // weights and the same four attention tensors (Decision 1 fuses nodes; it moves no tensor), so
    // the replay is priced at the A16 family's own measured throughput — the sentence the hybrid
    // loop below already makes for its three members.
    //
    // The INTERVAL is not the family's constant, and that is the whole of Decision 4 for this row:
    // a fused class's anchored replay is one history TILE, not one checkpoint interval, and
    // `palw_anchored_interval_for_profile_v1` is the one place that is spelled. Writing the
    // integer-KV interval here would price a replay 32× the one an honest responder performs —
    // conservative on the deadline, and a figure quoted under a configuration it was not measured
    // in, which is what this module exists to stop.
    let a16_v5 = crate::palw_qwen25_profile::qwen25_a16_graph_v5_profile_v1()?;
    rows.push(PalwShippedCourtRowV1 {
        class_id: a16_v5.shape_profile_id(),
        checkpoint_interval: crate::palw_context_ladder::palw_anchored_interval_for_profile_v1(&a16_v5),
        profile: a16_v5,
        cost: PALW_COURT_COST_A16,
    });
    // The hybrid family's members are all priced at the 35B's measured throughput. Conservative by
    // construction: every other member is smaller (the Coder tier has no recurrence at all, the
    // 2B is a fourteenth of the parameters), so no member replays a position more slowly than the
    // row this figure was measured on, and a floor derived from it can only be too large.
    for geometry in [
        crate::palw_qwen36_profile::QWEN36_35B_A3B,
        crate::palw_qwen36_profile::QWEN3_CODER_30B_A3B,
        crate::palw_qwen36_profile::QWEN35_2B,
    ] {
        let profile =
            crate::palw_qwen36_profile::qwen36_profile_v2(crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(geometry))?;
        rows.push(PalwShippedCourtRowV1 {
            class_id: profile.shape_profile_id(),
            profile,
            cost: PALW_COURT_COST_QWEN36,
            checkpoint_interval: interval,
        });
    }
    Ok(rows)
}

/// **What one shipped row's worst close costs, under the court that can TRY that row.**
///
/// [`crate::palw_class_admission_v2::derive_court_cost_v1`] is the genesis-anchored binary
/// derivation, and it is exactly right for every row whose terminal leaf is refuted by opening it.
/// A FUSED row (ADR-0082 Decision 1) is not one of those: its terminal is a dissection, no ruleset
/// may admit it while `palw_kary_court` is dormant (`FusedAttentionNeedsTheKaryCourt`), and pricing
/// it at the binary court measures a court no network that registers it will be running —
/// 3,446,708 bytes against 81,312 on the graph-v5 512 row, 42 carriers against one.
///
/// So the arity, the ladder and the id form come from the RULESET the caller is asking about, and
/// every non-fused row is priced exactly as before: `genesis_anchored_v1`'s walk takes its path
/// depth from the class's own worst case, so a row inside the executor's `2^22` prices identically
/// at either ladder (measured when the row builders moved onto the ruleset's ladder).
pub fn palw_shipped_row_court_cost_v1(
    profile: &PalwShapeProfileV3,
    ladder: u64,
    dissection_arity: u8,
    window_court_daa: u64,
    prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<crate::palw_class_admission_v2::PalwCourtCostV1, crate::palw_class_admission_v2::PalwClassAdmissionError> {
    if crate::palw_class_admission_v2::palw_profile_has_fused_attention_v1(profile) {
        let court = crate::palw_class_admission_v2::PalwKaryCourtV1 { dissection_arity, prompt_ids_form, window_court_daa };
        let rules = crate::palw_context_ladder::palw_class_ladder_rules_for_court_v1(profile, Some(court), ladder).ok_or(
            crate::palw_class_admission_v2::PalwClassAdmissionError::Profile(
                "a fused row declares no map the ladder prices".to_string(),
            ),
        )?;
        return crate::palw_class_admission_v2::derive_court_cost_shaped_v1(profile, rules.cost_shape);
    }
    crate::palw_class_admission_v2::derive_court_cost_shaped_v1(
        profile,
        crate::palw_class_admission_v2::PalwCourtCostShapeV1::genesis_anchored_v1(profile, ladder),
    )
}

/// **The whole of SA-4 over one bundle**: every class the bundle registers at genesis clears the
/// deadline from below, and the bundle's own ladder clears the window from above.
///
/// Returns the derived floor for each registered row, so a caller can PIN the numbers rather than
/// only learn that they passed.
pub fn palw_court_deadline_audit_bundle_v1(
    bundle: &PalwConsensusParamsV2,
    rows: &[PalwShippedCourtRowV1],
    close_blocks: u64,
) -> Result<Vec<(Hash64, u64)>, PalwCourtDeadlineError> {
    palw_court_ladder_fits_window_v1(&bundle.court, bundle.state.window_court())?;
    let mut derived = Vec::new();
    for class_id in palw_registered_class_ids_v1(bundle) {
        let row = rows.iter().find(|r| r.class_id == class_id).ok_or(PalwCourtDeadlineError::UnpricedRow { class_id })?;
        let floor = palw_court_deadline_admits_row_v1(&bundle.court, &row.cost, &row.profile, row.checkpoint_interval, close_blocks)?;
        derived.push((class_id, floor));
    }
    Ok(derived)
}

/// The class ids a bundle registers at genesis.
pub fn palw_registered_class_ids_v1(bundle: &PalwConsensusParamsV2) -> Vec<Hash64> {
    bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { class_id, .. } => Some(*class_id),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_context_ladder::PALW_COURT_ROUND_TRIP_MS;
    use crate::palw_fp_devnet_v3::{PALW_DEVNET_WINDOWS_V1, PALW_RC_WINDOWS_V1};
    use crate::palw_mode_v2::{PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS, PalwConsensusMode};

    /// Every params set a shipped binary can boot, named once. A preset with no V2 bundle carries
    /// no court and is skipped by `palw_bundled_presets_v1`; the test below refuses to pass if the
    /// whole list turns out to be empty, which is the way this check could silently check nothing.
    fn shipped_presets() -> Vec<(&'static str, crate::config::params::Params)> {
        vec![
            ("MAINNET_PARAMS", crate::config::params::MAINNET_PARAMS),
            ("TESTNET_PARAMS", crate::config::params::TESTNET_PARAMS),
            ("DEVNET_PARAMS", crate::config::params::DEVNET_PARAMS),
            ("SIMNET_PARAMS", crate::config::params::SIMNET_PARAMS),
            ("mainnet_shipped_params", crate::config::params::mainnet_shipped_params()),
            ("palw_rc_shipped_params", crate::config::params::palw_rc_shipped_params()),
            ("devnet_shipped_params", crate::config::params::devnet_shipped_params()),
        ]
    }

    fn bundled_presets() -> Vec<(&'static str, PalwConsensusParamsV2)> {
        shipped_presets()
            .into_iter()
            .filter_map(|(name, p)| match &p.palw_consensus_mode {
                PalwConsensusMode::ConsensusV2(b) => Some((name, b.clone())),
                _ => None,
            })
            .collect()
    }

    /// **SA-4, over every shipped row of every preset that carries a V2 bundle.**
    ///
    /// This is the assertion the amendment is for. Before it, `court_turn_deadline` and the honest
    /// replay of an interval were two numbers in two files with no expression containing both; a
    /// row whose replay outgrew the deadline would have been discovered by an honest responder
    /// losing a dispute. Now it is discovered here.
    ///
    /// A registered class this build cannot price fails as loudly as one that cannot answer in
    /// time — `UnpricedRow` — because "we did not derive that one" is the state SA-4 ends.
    #[test]
    fn every_shipped_row_clears_the_derived_turn_deadline() {
        let rows = palw_shipped_court_rows_v1().expect("the shipped rows project");
        let presets = bundled_presets();
        assert!(!presets.is_empty(), "no shipped preset carries a V2 bundle — this check verified nothing");
        let mut checked_rows = 0usize;
        for (name, bundle) in &presets {
            let derived = palw_court_deadline_audit_bundle_v1(bundle, &rows, PALW_COURT_CLOSE_BLOCKS_V1)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(!derived.is_empty(), "{name}: a V2 bundle registers at least the base class");
            for (class_id, floor) in &derived {
                // Printed, not only asserted: SA-4's content is that these two numbers are
                // RELATED, and a reader who runs this with `--nocapture` should be able to read
                // the relation off rather than re-derive it.
                println!(
                    "{name}: class {class_id} floor {floor} DAA against turn_deadline {} (window_court {})",
                    bundle.court.turn_deadline_daa(),
                    bundle.state.window_court()
                );
                assert!(
                    bundle.court.turn_deadline_daa() >= *floor,
                    "{name}: class {class_id} needs {floor} DAA and the clock is {}",
                    bundle.court.turn_deadline_daa()
                );
                checked_rows += 1;
            }
        }
        assert!(checked_rows >= presets.len(), "fewer rows checked than presets — a bundle registered nothing");
    }

    /// **The conservative reading passes too**, so the green above does not rest on this file's
    /// judgement about which shipped classes are anchored today.
    ///
    /// [`palw_court_replay_positions_v1`] charges an anchored class `min(n_ctx, interval)` and an
    /// unanchored one its whole context. If that reading were wrong in the dangerous direction —
    /// if the shipped court replayed from genesis for EVERY class — the floors would be the ones
    /// derived here, and they clear the shipped clock as well.
    #[test]
    fn the_shipped_deadline_clears_the_pessimistic_floor_as_well() {
        let rows = palw_shipped_court_rows_v1().expect("the shipped rows project");
        for (name, windows) in [("testnet-11 (RC)", PALW_RC_WINDOWS_V1), ("devnet", PALW_DEVNET_WINDOWS_V1)] {
            for row in &rows {
                // Every position of the context replayed, anchored or not.
                let pessimistic = palw_court_move_cost_daa_v1(&row.cost, row.profile.n_ctx, PALW_COURT_CLOSE_BLOCKS_V1);
                assert!(
                    windows.court_turn_deadline >= pessimistic,
                    "{name}: {} needs {pessimistic} DAA if every position is replayed and the clock is {}",
                    row.cost.row,
                    windows.court_turn_deadline
                );
            }
        }
    }

    /// **W4 at every preset that carries a V2 bundle**, through the bundle's own arithmetic, and
    /// the reading of it that ADR-0077 got wrong: the clock counts MOVES.
    #[test]
    fn the_ladder_fits_the_court_window_at_every_bundled_preset() {
        let presets = bundled_presets();
        assert!(!presets.is_empty(), "no shipped preset carries a V2 bundle — this check verified nothing");
        for (name, bundle) in &presets {
            let worst =
                palw_court_ladder_fits_window_v1(&bundle.court, bundle.state.window_court()).unwrap_or_else(|e| panic!("{name}: {e}"));
            // The move count is two per bisection round plus the terminal moves — NOT one per
            // round, which is the count ADR-0077 Decision 12's "(32 + 2) x 60 = 2,040" used.
            let moves = 2 * u64::from(bundle.court.bisection_rounds()) + u64::from(bundle.court.terminal_rounds());
            assert_eq!(worst, moves * bundle.court.turn_deadline_daa(), "{name}: the bundle's worst case is not moves x deadline");
            assert!(
                moves > u64::from(bundle.court.bisection_rounds()) + u64::from(bundle.court.terminal_rounds()),
                "{name}: the clock counts rounds — every deadline derived in this tree is half what it should be"
            );
        }
    }

    /// **The RC's numbers, pinned**, so a change to any input fails here with its arithmetic
    /// visible rather than somewhere downstream.
    #[test]
    fn the_rc_ladder_spends_2484_of_its_3000() {
        // The RULESET's ladder is 2^26 (2026-09-03) = 26 bisection rounds; 2 moves each; 2
        // terminal moves; 42 DAA a move. The clock is the derivation at the deepest reachable
        // ladder rather than at this one, so it is legal here with room and legal when the 2^32
        // fence arms — which a clock derived for 2^26 alone (51) would not be.
        assert_eq!(PALW_RC_WINDOWS_V1.court_turn_deadline, 42);
        assert_eq!(PALW_RC_WINDOWS_V1.window_court, 3_000);
        assert_eq!((2 * 26 + 2) * 42, 2_268);
        assert_eq!((2 * 32 + 2) * 42, 2_772, "and it still fits when the deeper fence arms");
        assert!(2_268 < PALW_RC_WINDOWS_V1.window_court);
        // The devnet set, same arithmetic at ITS clock: (2 x 22 + 2) x 4 = 184 < 300.
        //
        // **Neither number moved with ADR-0080, and a commit in between says they did.** The
        // assembly reserve is `2 x 4 x max_close_chunks` and `max_close_chunks` is a RULESET field:
        // the RC admits a 27-carrier close and reserves 216, the devnet admits one carrier and
        // reserves 8. Read from the RC's count the devnet's 300-DAA window holds the ladder at no
        // clock at all, so window and clock were widened to 600 and 5 to fund a reserve this
        // network never carries; both are back where they shipped. What the reserve DOES cost here
        // is eight DAA, and 184 + 8 = 192 is still inside 300.
        assert_eq!((2 * 22 + 2) * PALW_DEVNET_WINDOWS_V1.court_turn_deadline, 184);
        assert_eq!(PALW_DEVNET_WINDOWS_V1.court_turn_deadline, 4);
        assert_eq!(PALW_DEVNET_WINDOWS_V1.window_court, 300);
        assert_eq!(crate::palw_context_ladder::palw_close_assembly_daa_v1(PALW_DEVNET_WINDOWS_V1.max_close_chunks()), 8);
        assert!(184 + 8 < PALW_DEVNET_WINDOWS_V1.window_court);
    }

    /// **Which term dominates, in numbers** — the finding, not a decoration.
    ///
    /// At shipped widths the replay is a fifth of the sum and the sum is an eighth of one block,
    /// so SA-4's lower bound is satisfied by the ROUND TRIP alone and would be satisfied if the
    /// replay were free. That is worth pinning: it says the shipped deadline is safe for a reason
    /// that has nothing to do with the models, and it will stop being true at a width this test
    /// will name.
    #[test]
    fn the_deadline_is_dominated_by_the_round_trip_not_the_replay() {
        let rows = palw_shipped_court_rows_v1().expect("the shipped rows project");
        let mut worst_replay_ms = 0u64;
        for row in &rows {
            let positions = u64::from(palw_court_replay_positions_v1(&row.profile, row.checkpoint_interval));
            worst_replay_ms = worst_replay_ms.max(positions * row.cost.replay_ms_per_position());
        }
        // The widest unanchored shipped row is the Coder tier at n_ctx 9: 9 x 572 = 5,148 ms.
        // (The 35B hybrid and the 2B are n_ctx 8, so 4,576; the dense tier and the floor are
        // anchored and replay one position each.)
        assert_eq!(worst_replay_ms, 5_148);
        assert_eq!(PALW_COURT_ROUND_TRIP_MS, 10_000);
        assert!(worst_replay_ms < PALW_COURT_ROUND_TRIP_MS, "the replay has overtaken the round trip — re-read SA-4's bound");
        // And the sum is well inside one block, so every shipped row floors at one DAA.
        assert!(worst_replay_ms + PALW_COURT_ROUND_TRIP_MS < PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS);
        for row in &rows {
            assert_eq!(
                palw_court_deadline_floor_daa_v1(&row.cost, &row.profile, row.checkpoint_interval, PALW_COURT_CLOSE_BLOCKS_V1),
                1,
                "{}: the floor is no longer one DAA — the dominance reading above needs re-deriving",
                row.cost.row
            );
        }
    }

    /// **The unanchored branch is real, and it is the row that would fail first.** The shipped
    /// hybrid registers the sentinel map, so its replay is its whole context; the dense tier and
    /// the floor register maps and replay one ANCHORED INTERVAL — which is the checkpoint interval
    /// for a recurrence-anchored row and one history TILE for a fused one (ADR-0082 Decision 4).
    ///
    /// This asserted the checkpoint interval directly and read as "a mapped class replays one
    /// interval". That was true of every mapped row that existed, and the graph-v5 512 row makes it
    /// false: its bottom opens a tile, so an honest responder replays sixteen positions and not the
    /// family's interval. Asserted through `palw_anchored_interval_for_profile_v1` now — the one
    /// place the interval is spelled — so a row cannot be priced at an interval nobody replays.
    #[test]
    fn the_hybrid_replays_its_whole_context_and_the_others_replay_one_interval() {
        use crate::palw_context_ladder::palw_anchored_interval_for_profile_v1;
        let rows = palw_shipped_court_rows_v1().expect("the shipped rows project");
        for row in &rows {
            let positions = palw_court_replay_positions_v1(&row.profile, row.checkpoint_interval);
            if row.profile.state_chunk_map_id == Hash64::default() {
                assert_eq!(positions, row.profile.n_ctx, "{}: an unmapped class must be priced at its whole context", row.cost.row);
            } else {
                assert_eq!(
                    positions, row.checkpoint_interval,
                    "{}: a mapped class replays the interval its row declares",
                    row.cost.row
                );
                assert_eq!(
                    row.checkpoint_interval,
                    palw_anchored_interval_for_profile_v1(&row.profile),
                    "{}: the row declares an interval that is not the one this class's own anchor replays",
                    row.cost.row
                );
            }
        }
        // At least one of each, or the branch above is untested by construction — and both KINDS of
        // mapped row, because the fused one is the case this test used to assert away.
        assert!(rows.iter().any(|r| r.profile.state_chunk_map_id == Hash64::default()), "no unmapped shipped row");
        assert!(
            rows.iter().any(|r| r.profile.state_chunk_map_id != Hash64::default()
                && r.checkpoint_interval == PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1),
            "no checkpoint-anchored mapped shipped row"
        );
        assert!(
            rows.iter().any(|r| crate::palw_class_admission_v2::palw_profile_has_fused_attention_v1(&r.profile)
                && r.checkpoint_interval == crate::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4),
            "no fused shipped row priced at the history tile — the graph-v5 case is untested again"
        );
    }

    /// **Where the floor starts to bite** — the number a wider row has to be checked against, and
    /// the one that says the earlier "60 -> 40" proposal had no margin.
    #[test]
    fn the_floor_reaches_the_clock_only_at_thousands_of_positions() {
        let ms = PALW_COURT_COST_QWEN36.replay_ms_per_position();
        assert_eq!(ms, 572);
        // The RC's 60-DAA clock is 7,200,000 ms; net of the round trip that is 12,569 positions.
        let rc_positions = (60 * PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS - PALW_COURT_ROUND_TRIP_MS) / ms;
        assert_eq!(rc_positions, 12_569);
        // ADR-0077 Decision 13's top row is 8,192 positions. Replayed UNANCHORED at the hybrid's
        // throughput that is 40 DAA exactly — so the amendment's superseded proposal of a 40-DAA
        // clock would have put that row ON its own floor with nothing to spare, while the ladder
        // module's derived 45 leaves five. Only reachable for a row that registers no state chunk
        // map; Decision 10 refuses the long form for a mapped class, which replays 256.
        assert_eq!(palw_court_move_cost_daa_v1(&PALW_COURT_COST_QWEN36, 8_192, PALW_COURT_CLOSE_BLOCKS_V1), 40);
        assert_eq!(palw_court_move_cost_daa_v1(&PALW_COURT_COST_QWEN36, 256, PALW_COURT_CLOSE_BLOCKS_V1), 2);
    }

    /// **The close-assembly term does what it says**, so a rule that splits a close across blocks
    /// derives its deadline from this expression instead of writing a second one.
    #[test]
    fn a_split_close_spends_the_movers_deadline_block_for_block() {
        let one = palw_court_move_cost_daa_v1(&PALW_COURT_COST_QWEN36, 8, 1);
        for k in 1..=8u64 {
            assert_eq!(palw_court_move_cost_daa_v1(&PALW_COURT_COST_QWEN36, 8, k), one + (k - 1), "close over {k} blocks");
        }
        // And the whole point of naming it: at the shipped clock a close may take up to 60 blocks
        // to assemble before the floor reaches the deadline — every one of them taken out of the
        // margin, not out of nothing.
        assert_eq!(palw_court_move_cost_daa_v1(&PALW_COURT_COST_QWEN36, 8, 60), 60);
        assert_eq!(palw_court_move_cost_daa_v1(&PALW_COURT_COST_QWEN36, 8, 61), 61);
        assert!(61 > PALW_RC_WINDOWS_V1.court_turn_deadline, "a 61-block close outruns the RC clock");
    }

    /// **A deadline below the floor is refused BY NAME**, with the arithmetic in the message —
    /// the negative case, so the check is known to be capable of failing.
    #[test]
    fn a_short_clock_is_refused_with_its_arithmetic() {
        let court = PalwCourtParamsV2::new(1 << 22, 1, 2).expect("buildable");
        let rows = palw_shipped_court_rows_v1().expect("the shipped rows project");
        let hybrid = rows.iter().find(|r| r.profile.state_chunk_map_id == Hash64::default()).expect("an unmapped row");
        // One DAA clears the shipped rows' one-DAA floor; a close that takes three blocks does not.
        assert!(palw_court_deadline_admits_row_v1(&court, &hybrid.cost, &hybrid.profile, hybrid.checkpoint_interval, 1).is_ok());
        let err = palw_court_deadline_admits_row_v1(&court, &hybrid.cost, &hybrid.profile, hybrid.checkpoint_interval, 3)
            .expect_err("a three-block close needs three DAA");
        match err {
            PalwCourtDeadlineError::ConvictsByClock { floor, deadline, .. } => {
                assert_eq!((floor, deadline), (3, 1));
            }
            other => panic!("wrong arm: {other}"),
        }
        // And the ladder end refuses too, at a clock the window cannot hold.
        let deep = PalwCourtParamsV2::new(1 << 22, 60, 2).expect("buildable");
        let err = palw_court_ladder_fits_window_v1(&deep, 2_000).expect_err("2,760 does not fit 2,000");
        match err {
            PalwCourtDeadlineError::LadderOverrunsWindow { worst, window_court, .. } => {
                assert_eq!((worst, window_court), (2_760, 2_000));
            }
            other => panic!("wrong arm: {other}"),
        }
    }
}
