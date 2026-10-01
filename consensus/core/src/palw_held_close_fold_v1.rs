//! **RFC-0003 §I.4.7, decision 22 in the fold: the held leaf challenge** (`HeldLeafChallengeDeclared`, tag 90)
//! — dormant behind `Params::palw_held_close_chunks_v1`. A child module of `palw_state_v2`, as the tensor
//! claim's fold is, so it reads the builder and the state's tables directly and writes them only through their
//! one writers. The object, its digest and its fence are [`crate::palw_held_close_v1`]'s; spec 04b §15.15.6 is
//! normative.
//!
//! The arm is the one-move accusation's gates (a live claim of an IR or a pipeline class, the executor and the
//! roots the claim's, an accuser that is not the producer, Active and at the floor, the held-dissection rule
//! for a claim that already holds a session) and then two writes that are the court's own: the session opened
//! at `Terminal` on the named leaf (`open_dissection_at_named_leaf_v1`, ADR-0103 Decision 5) and the
//! challenger-side close group the existing declaration arm writes (`CourtCloseDeclared`), declarer the
//! accuser's own bond, deposit `palw_close_assembly_deposit_v1(count)`, assembly clock `4 · count` DAA. The
//! chunks, the assembly, the verdict and the failures are the court's, unchanged.
//!
//! **Every refusal comes before the first write**: the builder's writes are not rolled back by a failed arm
//! (the rehearsal drops what the fold refuses on a clone), and a session opened beside a group that could not
//! be written would be a freeze no clock ends.

use super::*;
use crate::palw_held_close_v1::{PalwHeldLeafChallengeV1, palw_held_leaf_challenge_session_id_v1};

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::HeldLeafChallengeRefused(why.into())
}

/// **The held leaf challenge** (see the module doc).
pub(super) fn apply_held_leaf_challenge_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    c: &PalwHeldLeafChallengeV1,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    // 1. The fence: the second lock behind the acceptance walk's drop.
    if !builder.params.held_close_chunks_active_at(daa) {
        return Err(refused("a held leaf challenge below palw_held_close_chunks_v1 (RFC-0003 decision 22)"));
    }
    // 2. The held regime: where bisection is played the ladder reaches the leaf, and this object has no
    // place — the legacy session carries its own close declaration.
    let Some(held) = builder.extras.held_context_ladder else {
        return Err(refused("a held leaf challenge on a chain that plays bisection: the ladder reaches the leaf there"));
    };
    // 3. The claim: live, of an IR or a pipeline class, the executor and the roots the claim's.
    let claim_id = c.claim;
    let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
    if claim.phase.is_terminal() {
        return Err(PalwStateV2Error::WrongPhase { claim: claim_id, edge: "HeldLeafChallengeDeclared" });
    }
    if !builder.state.tir_classes.contains_key(&claim.class_id) && !builder.state.gen_classes.contains_key(&claim.class_id) {
        return Err(refused("a held leaf challenge names a claim of an IR or a pipeline class"));
    }
    if c.executor_bond != claim.bond {
        return Err(PalwStateV2Error::ShardCourtExecutorIsNotTheClaims(claim_id));
    }
    if c.execution_root != claim.execution_root || c.trace_root != claim.trace_root {
        return Err(PalwStateV2Error::ShardCourtRootsDiffer(claim_id));
    }
    let accuser = c.accuser_bond;
    if accuser == claim.bond {
        return Err(PalwStateV2Error::ShardCourtAccuserIsTheProducer(accuser));
    }
    // 4. The accuser: Active, at or above the floor — an accusation is priced, not privileged.
    let accuser_record = builder.state.bonds.get(&accuser).ok_or(PalwStateV2Error::MissingBond(accuser))?;
    if !matches!(accuser_record.status, PalwBondStatusV2::Active) {
        return Err(PalwStateV2Error::BondNotActive(accuser));
    }
    let floor = builder.params.min_collateral_sompi();
    if accuser_record.collateral < floor {
        return Err(PalwStateV2Error::BondBelowFloor { bond: accuser, collateral: accuser_record.collateral, floor });
    }
    // 5. The leaf: inside the claim's own step space — its priced leaf count where it prices its work (an
    // FP claim: a tensor claim, an evaluation claim), else the class's ladder.
    let ladder = builder.state.class_step_ladder_v1(&claim.class_id, held);
    let bound = if crate::palw_backend::palw_claim_prices_work_v1(claim.work_leaves) { claim.work_leaves.min(ladder) } else { ladder };
    if c.leaf_index >= bound {
        return Err(refused(format!("the named leaf {} is outside the claim's step space of {bound} leaves", c.leaf_index)));
    }
    // 6. The close declaration: between one chunk and the structural bound, one digest per chunk.
    if c.count == 0 || c.count > PALW_COURT_CLOSE_MAX_CHUNKS {
        return Err(PalwStateV2Error::CourtCloseCountOutOfRange { count: c.count, max: PALW_COURT_CLOSE_MAX_CHUNKS });
    }
    let session_id = palw_held_leaf_challenge_session_id_v1(c, ladder);
    if c.chunk_digests.len() != c.count as usize {
        return Err(PalwStateV2Error::CourtCloseDigestsIncoherent { session: session_id });
    }
    // 7. A session already open on the claim admits another challenge only from a seat of its panel, one
    // per seat, at most `1 + seat_count` — the held dissection's own rule, a decoy opened by the producer's
    // Sybil holds off no seat.
    if builder.state.court_sessions.values().any(|s| s.claim == claim_id) {
        check_further_held_dissection_v1(&builder.state, claim_id, accuser)?;
    }
    if builder.state.court_sessions.contains_key(&session_id) {
        return Err(PalwStateV2Error::DuplicateSession(session_id));
    }
    // 8. **The clock: the group must be able to finish inside the session's backstop.** The session this
    // opens is dated `daa + window_court` (`open_dissection_at_named_leaf_v1`); a declaration that cannot
    // assemble inside it is a freeze that suspends a rung for nothing, refused here — where the accuser can
    // still make a smaller one — exactly as the declaration arm refuses it (`CourtCloseCannotAssemble`).
    let backstop = daa.checked_add(builder.params.window_court).ok_or(PalwStateV2Error::Overflow("court deadline"))?;
    let needed = daa.checked_add(palw_close_assembly_daa_v1(c.count)).ok_or(PalwStateV2Error::Overflow("close assembly deadline"))?;
    if needed > backstop {
        return Err(PalwStateV2Error::CourtCloseCannotAssemble { session: session_id, count: c.count, needed, deadline: backstop });
    }
    // The declarer is the session's challenger, read from the bond the session will name: a group can
    // never name a third party. (`bonds.contains_key` is step 4's.)
    // 9. The writes — the session first (its capacity, the accuser's reservation and the claim's path to
    // `Final` are the court's), then the group.
    let turn = builder.params.turn_deadline_daa();
    let opened = open_dissection_at_named_leaf_v1(builder, ctx, claim_id, &claim, accuser, c.leaf_index, ladder, turn)?;
    debug_assert_eq!(opened, session_id, "the session the court opened is the one the challenge names");
    builder.write_court_close_group(
        (opened, PalwCourtSideV1::Challenger),
        Some(PalwCourtCloseGroupV2 {
            declarer: accuser,
            count: c.count,
            chunk_digests: c.chunk_digests.clone(),
            close_digest: c.close_digest,
            // A challenge prosecutes: the close it pins convicts or it fails.
            verdict: PalwCourtVerdictV2::ExecutorGuilty,
            declared_daa: daa,
            assembly_deadline_daa: needed,
            present: 0,
            deposit_outpoint: accuser.0,
            deposit: palw_close_assembly_deposit_v1(c.count),
            chunks: BTreeMap::new(),
        }),
    );
    Ok(())
}
