//! **ADR-0092 — the ladder is a one-time commitment and the court window is what binds.**
//!
//! Section 6's invariants, as tests. Each expresses the RULE and derives its numbers from the
//! shipped presets; none transcribes a figure from the ADR, because ADR-0092 §5 says every number
//! in it is a generated artifact. `misaka-palw-base0 --bin base0-class-sizing` is the generator;
//! these are the assertions that make its findings hold.

use kaspa_consensus_core::config::params::{Params, devnet_shipped_params, palw_rc_shipped_params};
use kaspa_consensus_core::palw_attn_court_v1::palw_attn_court_admits_row_v1;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwCourtParamsV2};
use kaspa_consensus_core::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4;

/// The court and the window of a V2 preset, or `None` for a preset that ships no bundle.
fn court_and_window(params: &Params) -> Option<(PalwCourtParamsV2, u64)> {
    match &params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => Some((bundle.court, bundle.state.window_court())),
        _ => None,
    }
}

/// The same court at another ladder and arity — every other ceiling kept, so a comparison varies
/// one thing at a time. Mirrors `base0-class-sizing`'s `court_at`.
fn court_at(rc: &PalwCourtParamsV2, ladder: u64, arity: u8) -> Option<PalwCourtParamsV2> {
    PalwCourtParamsV2::with_cost_ceilings(
        ladder,
        rc.turn_deadline_daa(),
        rc.terminal_rounds(),
        rc.max_close_bytes(),
        rc.max_terminal_macs(),
        rc.max_operand_count(),
    )
    .ok()?
    .with_dissection_arity(arity)
    .ok()
}

/// The widest ladder this court's arity can prosecute inside `window` at `history` positions.
fn widest_admissible(rc: &PalwCourtParamsV2, arity: u8, history: u64, window: u64) -> Option<u32> {
    (2u32..=44)
        .filter_map(|exp| {
            let court = court_at(rc, 1u64 << exp, arity)?;
            palw_attn_court_admits_row_v1(&court, history, PALW_ATTN_HISTORY_TILE_V4, window).ok().map(|_| exp)
        })
        .max()
}

/// Every shipped V2 preset must be able to prosecute the ladder it froze, inside the window it
/// froze, or it ships a court that cannot try its own widest row. Invariant 1.
///
/// The zero-history arm is the one that makes a preset's ladder/arity pair legal at all; the
/// sibling test below is what stops that from being read as "legal at any history".
#[test]
fn every_shipped_preset_prosecutes_its_own_ladder_inside_its_own_window() {
    let presets: [(&str, Params); 2] = [("testnet-11 (RC)", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())];
    for (name, params) in presets {
        let Some((court, window)) = court_and_window(&params) else {
            panic!("{name} ships no ConsensusV2 bundle; this test's premise is wrong, not its subject");
        };
        let verdict = palw_attn_court_admits_row_v1(&court, 0, PALW_ATTN_HISTORY_TILE_V4, window);
        assert!(
            verdict.is_ok(),
            "{name}: its own ladder of {} does not fit its own court window of {window} DAA at arity {} — {:?}",
            court.max_step_leaf_count(),
            court.dissection_arity(),
            verdict
        );
    }
}

/// **ADR-0092's central finding, pinned: the shipped ladder sits AT the wall, not below it.**
///
/// At the dense row's own context the shipped arity admits the shipped `max_step_leaf_count` and
/// nothing wider. This test fails if the ladder is raised without the arity or the window moving
/// with it — which is Decision 1 — and it fails if the pair silently gains headroom, which would
/// mean one of the three terms moved and nobody said so.
#[test]
fn the_shipped_ladder_is_the_widest_the_shipped_arity_can_prosecute_at_its_own_context() {
    let params = palw_rc_shipped_params();
    let (court, window) = court_and_window(&params).expect("the RC ships a bundle");
    // The dense graph-v5 row's declared context. Not a constant of this test: it is the width the
    // registered row is admitted at, and the generator prints the whole row of contexts beside it.
    const DENSE_ROW_CONTEXT: u64 = 512;

    let widest = widest_admissible(&court, court.dissection_arity(), DENSE_ROW_CONTEXT, window)
        .expect("some ladder fits, or the preset could not have booted");
    assert_eq!(
        1u64 << widest,
        court.max_step_leaf_count(),
        "the shipped ladder ({}) is not the widest the shipped arity {} can prosecute at {DENSE_ROW_CONTEXT} \
         positions inside {window} DAA (that is 2^{widest}). ADR-0092 Decision 1: the ladder is minted \
         once, at the top of the wall-clock budget — if this moved, say which of the three terms moved \
         and re-mint deliberately.",
        court.max_step_leaf_count(),
        court.dissection_arity(),
    );
}

/// **Decision 3's measurement, not its assumption.** Raising the arity must buy a strictly deeper
/// ladder at the same window and the same history, or the arity is not the knob the ADR says it is.
///
/// This asserts only the wall-clock half. What one round CARRIES rises with the arity, and that
/// half is the close ceiling's — ADR-0092 Decision 3 refuses to conclude anything about total cost
/// from this test alone, and neither does this test.
#[test]
fn a_higher_arity_buys_a_deeper_ladder_at_the_same_window() {
    let params = palw_rc_shipped_params();
    let (court, window) = court_and_window(&params).expect("the RC ships a bundle");
    const HISTORY: u64 = 4_096;

    let shipped = widest_admissible(&court, court.dissection_arity(), HISTORY, window).expect("the shipped arity admits some ladder");
    let eightfold = widest_admissible(&court, 8, HISTORY, window).expect("arity 8 admits some ladder");
    assert!(
        eightfold > shipped,
        "at {HISTORY} history positions and a {window} DAA window, arity 8 admits 2^{eightfold} and the \
         shipped arity {} admits 2^{shipped} — Decision 3 says the arity is where a width problem is paid \
         for, and this is the assertion that says it still is",
        court.dissection_arity()
    );
}

/// **Decision 4's irreversibility, asserted rather than assumed.** `max_step_leaf_count` is inside
/// the bundle and the bundle is what the ruleset id hashes, so raising the ladder on a running
/// chain is a new ruleset. If this ever stops being true, Decision 4's whole argument — "a model
/// too wide for the minted ladder is a new class on a new ruleset, not a raised ceiling" — is
/// vacuous, and the ADR must be amended rather than quietly outlived.
#[test]
fn raising_the_ladder_moves_the_rulesets_fingerprint() {
    let params = palw_rc_shipped_params();
    let (court, _) = court_and_window(&params).expect("the RC ships a bundle");

    let mut wider = params.clone();
    let PalwConsensusMode::ConsensusV2(bundle) = &mut wider.palw_consensus_mode else { panic!("V2") };
    bundle.court = court_at(&court, court.max_step_leaf_count() * 2, court.dissection_arity())
        .expect("one more doubling is expressible even where it is not admissible");

    assert_ne!(
        params.consensus_params_id(),
        wider.consensus_params_id(),
        "doubling max_step_leaf_count left the fingerprint alone — the ladder would then be raisable in \
         place, and ADR-0092 Decision 4 is written on the premise that it is not"
    );
}
