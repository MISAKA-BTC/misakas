//! ADR-0042 Decision 8, P0-9 item 2: **the terminal opening, and why it also closes the steering.**
//!
//! A bisection that narrows to one index has LOCATED a disputed step and decided nothing. Without a
//! terminal move the ladder ends in silence: a responder that answers every rung and then stops
//! leaves the block Provisional forever (a liveness hole), and a terminal that simply credited the
//! responder would let it steer the interval and win (a soundness hole). The 2026-08-17 design was
//! rejected on four grounds for exactly that reason.
//!
//! Two of those grounds are now gone — the ladder's window budget was corrected, and the announced
//! root and the logits leg were separated. A third, the missing authorship half for a WITHHELD
//! execution, is answered by `PalwWithheldAuthorshipV1`. This module answers the fourth.
//!
//! ## Why steering stops working
//!
//! The remaining objection was that a guilty responder discloses junk at every rung, an honest
//! challenger disagrees every time, and the interval collapses on an index the RESPONDER chose —
//! where it opens an honest leaf and the challenger has nothing.
//!
//! That argument assumes the terminal is checked against the *execution*. It is checked against the
//! **anchor pair the responder itself pinned**: `lo_state` and `hi_state`, which every rung's
//! disclosure replaced one of. So a responder that disclosed junk arrives at a terminal whose
//! endpoints are junk, and must now produce a real step that carries `lo_state` to `hi_state`. It
//! cannot: a primitive recomputed from proven operands yields what it yields. Steering moves WHERE
//! the contradiction surfaces, never whether it does.
//!
//! Junk therefore stops being a free move and becomes a slower way to lose — which is what "the
//! rungs bind" has to mean when no full node can check a midpoint directly.

use crate::Hash64;

/// What the responder must produce when the interval is one index wide.
///
/// The operands travel with it (`crate::palw_artifact`), so a full node adjudicates without the
/// model — the same discipline W1 demands everywhere else.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTerminalOpeningV1 {
    pub session_id: Hash64,
    /// The disputed index — the ladder's `lo` when the interval is `[lo, lo + 1]`.
    pub index: u64,
    /// The state entering the step. MUST equal the ladder's `lo_state`.
    pub pre_state: Hash64,
    /// The state leaving it. MUST equal the ladder's `hi_state`.
    pub post_state: Hash64,
    /// The operands the primitive consumes, each proven against the class's artifact root.
    pub operands: Vec<crate::palw_artifact::PalwArtifactOpeningV1>,
}

/// What a completed ladder decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTerminalVerdictV1 {
    /// The step reproduces: the responder answered honestly and the challenge fails. The
    /// challenger's bond answers for having opened it.
    ChallengerLoses,
    /// The step does not reproduce: the responder is convicted at this index.
    ResponderConvicted,
    /// The responder did not open by the deadline, or opened something that does not fit the
    /// interval it is answering. Withholding the opening is losing it — a ladder that let silence
    /// end in stalemate would make "answer every rung and stop" a free permanent veto.
    ResponderDefaults,
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwTerminalError {
    #[error("the ladder is not at a one-index interval, so nothing is located to open")]
    NotTerminal,
    #[error("the opening answers session {got}, not {expected}")]
    SessionMismatch { got: Hash64, expected: Hash64 },
}

/// Adjudicate a terminal opening against the interval the ladder narrowed to.
///
/// `recompute` is the primitive: given the pre-state and the proven operands, it returns the state
/// the step actually produces, or `None` when this node cannot decide (an opcode outside the
/// catalog). **`None` is a `ResponderDefaults`, not a conviction and not a credit** — the same
/// direction every other undecidable outcome takes, and the reason it is safe here is that the
/// responder chose which step to open: an opening this node cannot check is one the responder
/// could have made checkable.
///
/// The endpoint equalities come first and are not negotiable. An opening that does not carry the
/// pinned `lo_state` into the pinned `hi_state` is not an answer to THIS dispute, whatever it
/// recomputes to — that check is what makes every earlier rung binding.
pub fn adjudicate_terminal_opening_v1<F>(
    opening: &PalwTerminalOpeningV1,
    session_id: Hash64,
    interval: (u64, u64),
    lo_state: Hash64,
    hi_state: Hash64,
    artifact_root: Hash64,
    recompute: F,
) -> Result<PalwTerminalVerdictV1, PalwTerminalError>
where
    F: FnOnce(Hash64, &crate::palw_artifact::PalwProvenOperandsV1) -> Option<Hash64>,
{
    let (lo, hi) = interval;
    if hi != lo.saturating_add(1) {
        return Err(PalwTerminalError::NotTerminal);
    }
    if opening.session_id != session_id {
        return Err(PalwTerminalError::SessionMismatch { got: opening.session_id, expected: session_id });
    }
    // Answering a different index, or different endpoints, is not answering this dispute. Treated
    // as a default rather than an error: the responder produced something, and producing the wrong
    // thing must not be cheaper than producing nothing.
    if opening.index != lo || opening.pre_state != lo_state || opening.post_state != hi_state {
        return Ok(PalwTerminalVerdictV1::ResponderDefaults);
    }
    // Every operand must open against the class's registered artifact root. One forged opening
    // fails the whole set rather than being dropped — a dropped operand would read as "this node
    // cannot decide", which is the verdict a forger wants.
    let Ok(operands) = crate::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&opening.operands, artifact_root) else {
        return Ok(PalwTerminalVerdictV1::ResponderDefaults);
    };
    match recompute(opening.pre_state, &operands) {
        Some(actual) if actual == opening.post_state => Ok(PalwTerminalVerdictV1::ChallengerLoses),
        Some(_) => Ok(PalwTerminalVerdictV1::ResponderConvicted),
        None => Ok(PalwTerminalVerdictV1::ResponderDefaults),
    }
}

/// The ladder ran out of time. Who loses is decided by **whose turn it was**.
///
/// Symmetry is the point: a ladder where only one side could time out would be a weapon rather than
/// a procedure. The responder owes disclosures and the terminal opening; the challenger owes
/// verdicts. Whoever was owed the next move and did not make it loses.
pub fn adjudicate_timeout_v1(responder_owed_the_move: bool) -> PalwTerminalVerdictV1 {
    if responder_owed_the_move { PalwTerminalVerdictV1::ResponderDefaults } else { PalwTerminalVerdictV1::ChallengerLoses }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_artifact::{PalwArtifactOpeningV1, PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1};

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    /// One operand, one leaf, so the tree is a single node and its root IS the leaf.
    fn one_operand() -> (Vec<PalwArtifactOpeningV1>, Hash64) {
        let operand = PalwArtifactOperandV1 {
            tensor_name: "blk.{layer}.attn_q.weight".into(),
            layer: Some(0),
            row_start: 0,
            bytes: vec![1, 2, 3, 4],
        };
        let root = artifact_root_v1(&[artifact_leaf_v1(&operand)]).unwrap();
        (vec![PalwArtifactOpeningV1 { operand, leaf_index: 0, leaf_count: 1, path: Vec::new() }], root)
    }

    fn opening(index: u64, pre: Hash64, post: Hash64, operands: Vec<PalwArtifactOpeningV1>) -> PalwTerminalOpeningV1 {
        PalwTerminalOpeningV1 { session_id: h(0x5E), index, pre_state: pre, post_state: post, operands }
    }

    /// The honest responder reproduces the step and the challenge fails.
    #[test]
    fn a_step_that_reproduces_defeats_the_challenge() {
        let (operands, root) = one_operand();
        let o = opening(4, h(0xA), h(0xB), operands);
        let verdict = adjudicate_terminal_opening_v1(&o, h(0x5E), (4, 5), h(0xA), h(0xB), root, |_pre, _ops| Some(h(0xB))).unwrap();
        assert_eq!(verdict, PalwTerminalVerdictV1::ChallengerLoses);
    }

    /// A step that does not reproduce convicts the responder — the ladder's whole purpose.
    #[test]
    fn a_step_that_does_not_reproduce_convicts() {
        let (operands, root) = one_operand();
        let o = opening(4, h(0xA), h(0xB), operands);
        let verdict = adjudicate_terminal_opening_v1(&o, h(0x5E), (4, 5), h(0xA), h(0xB), root, |_pre, _ops| Some(h(0xFF))).unwrap();
        assert_eq!(verdict, PalwTerminalVerdictV1::ResponderConvicted);
    }

    /// **This is the test that closes the steering objection (P0-9 item 1's survivor).**
    ///
    /// A guilty responder disclosed junk at every rung, so the interval it steered onto carries junk
    /// endpoints. It must now open a step that carries THOSE endpoints — and a real primitive does
    /// not produce a junk post-state from a junk pre-state. Steering moved where the contradiction
    /// surfaces; it did not remove it.
    ///
    /// The second half is the other escape: opening a DIFFERENT index, or different endpoints, in
    /// the hope that answering the wrong question beats answering none. It does not — producing the
    /// wrong thing must never be cheaper than producing nothing.
    #[test]
    fn steering_relocates_the_contradiction_it_does_not_remove_it() {
        let (operands, root) = one_operand();
        // Endpoints the responder pinned by disclosing junk. The real step from `lo_state` lands
        // somewhere else entirely.
        let junk_pre = h(0xDEAD);
        let junk_post = h(0xBEEF);
        let o = opening(4, junk_pre, junk_post, operands.clone());
        let verdict = adjudicate_terminal_opening_v1(&o, h(0x5E), (4, 5), junk_pre, junk_post, root, |pre, _ops| {
            assert_eq!(pre, junk_pre, "the primitive is run from the state the responder pinned");
            Some(h(0x1111)) // what the arithmetic actually gives
        })
        .unwrap();
        assert_eq!(verdict, PalwTerminalVerdictV1::ResponderConvicted, "junk endpoints convict at the terminal");

        // Answering another index, or other endpoints, is a default rather than a fresh chance.
        for wrong in [opening(9, junk_pre, junk_post, operands.clone()), opening(4, h(1), junk_post, operands.clone())] {
            let v =
                adjudicate_terminal_opening_v1(&wrong, h(0x5E), (4, 5), junk_pre, junk_post, root, |_p, _o| Some(junk_post)).unwrap();
            assert_eq!(v, PalwTerminalVerdictV1::ResponderDefaults);
        }
    }

    /// A forged operand is a default, not an "unadjudicable" that credits the responder.
    #[test]
    fn an_operand_that_does_not_open_is_a_default() {
        let (mut operands, root) = one_operand();
        operands[0].operand.bytes = vec![0xFF; 4];
        let o = opening(4, h(0xA), h(0xB), operands);
        let v = adjudicate_terminal_opening_v1(&o, h(0x5E), (4, 5), h(0xA), h(0xB), root, |_p, _o| Some(h(0xB))).unwrap();
        assert_eq!(v, PalwTerminalVerdictV1::ResponderDefaults);
    }

    /// A step this node cannot recompute defaults the RESPONDER, and that is safe precisely because
    /// the responder chose which step to open: an unopenable opening is one it could have made
    /// openable.
    #[test]
    fn an_uncheckable_step_defaults_the_responder_who_chose_it() {
        let (operands, root) = one_operand();
        let o = opening(4, h(0xA), h(0xB), operands);
        let v = adjudicate_terminal_opening_v1(&o, h(0x5E), (4, 5), h(0xA), h(0xB), root, |_p, _o| None).unwrap();
        assert_eq!(v, PalwTerminalVerdictV1::ResponderDefaults);
    }

    /// Structural refusals: a ladder that has not narrowed, and an opening for another session.
    #[test]
    fn a_ladder_that_has_not_narrowed_has_nothing_to_open() {
        let (operands, root) = one_operand();
        let o = opening(4, h(0xA), h(0xB), operands);
        assert_eq!(
            adjudicate_terminal_opening_v1(&o, h(0x5E), (4, 9), h(0xA), h(0xB), root, |_p, _o| Some(h(0xB))),
            Err(PalwTerminalError::NotTerminal)
        );
        assert!(matches!(
            adjudicate_terminal_opening_v1(&o, h(0x99), (4, 5), h(0xA), h(0xB), root, |_p, _o| Some(h(0xB))),
            Err(PalwTerminalError::SessionMismatch { .. })
        ));
    }

    /// Timeouts are symmetric — a ladder only one side can lose by silence is a weapon.
    #[test]
    fn whoever_owed_the_move_loses_by_silence() {
        assert_eq!(adjudicate_timeout_v1(true), PalwTerminalVerdictV1::ResponderDefaults);
        assert_eq!(adjudicate_timeout_v1(false), PalwTerminalVerdictV1::ChallengerLoses);
    }
}
