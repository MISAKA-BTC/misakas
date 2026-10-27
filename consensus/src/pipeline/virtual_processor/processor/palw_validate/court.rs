//! Object authentication and admission for the existing court family.
//! Immutable chain inputs are explicit; ordered state application remains in the fold.
use super::*;

impl VirtualStateProcessor {
    pub(super) fn palw_validate_court_object_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        match object {
            Obj::CourtOpened { session_id, claim, challenger_bond, space, space_size, signature } => {
                // **ADR-0103 Decision 1: the held regime plays no bisection.** Refused here and
                // in the fold, for the DA court's doubled-fence reason.
                if self.palw_held_context_at(point.daa_score) {
                    return Err(format!(
                        "claim {claim}: the held regime plays no bisection — accuse the leaf in one move (ADR-0103 Decision 1)"
                    ));
                }
                // The object now DECLARES the space, because the transition opens a ladder
                // from it — but the ruleset still DECIDES it. H2's rule is unchanged: a space
                // the accuser chose is a ladder depth the accuser chose, so a declaration that
                // is not the catalog's own step-leaf count is refused here, before the
                // transition ever sees it.
                let court = self.palw_court_params_v2.as_ref().ok_or("no court params")?;
                if *space != kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves
                    || *space_size != court.max_step_leaf_count()
                {
                    return Err(format!(
                        "court {session_id} declares a {space_size}-wide {space:?} space; the ruleset's is {} step leaves",
                        court.max_step_leaf_count()
                    ));
                }
                kaspa_consensus_core::palw_court_v2::validate_court_opened_v2(
                    state,
                    state_params,
                    point,
                    session_id,
                    claim,
                    challenger_bond,
                    kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves,
                    court.max_step_leaf_count(),
                    // The challenger's own signature over the session id (audit M-01). Without
                    // it every check above is a fact ABOUT the bond and none about who spoke
                    // for it, so anyone could prosecute under a stranger's identity — and the
                    // transition disarms the claim's final deadline while a session is open,
                    // which turns that into a freeze of an honest producer's claim.
                    signature,
                    Self::verify_mldsa87_with_context_bool,
                )
                .map_err(|e| e.to_string())?;
            }
            // **ADR-0080 design A, W6: the declaration is a court move, and it is signed.**
            //
            // This arm refused every `CourtCloseDeclared` outright until W6 landed, and the
            // refusal was right for as long as it stood: a declaration pins every byte of a
            // close and asserts a verdict on behalf of one of the two bonds the session id
            // binds, the transition acts on it, and nothing proved the SIDE authorised it —
            // P0-9's hole in its original words, where either party could write the other's
            // move. Unsigned it was worse than that: a declaration is singular per
            // `(session, side)` for the life of the session and its failure to assemble is the
            // declarer's conviction, so one forged object would have voided an honest
            // producer's claim.
            //
            // `check_court_close_declaration_acceptance_v2` is the same split
            // `CourtDisclosed` and `CourtVerdictPosted` use: the transition reads WHICH bond
            // may declare, this layer reads whether that bond spoke — its registered key is in
            // the candidate state, and neither party names its own.
            Obj::CourtCloseDeclared { session_id, side, count, chunk_digests, close_digest, verdict, signature } => {
                // The ruleset's own carriage count, which the transition cannot ask for: it has
                // no `PalwCourtParamsV2` and enforces only the bitmap's structural bound. On
                // devnet this is 1, so the split path is refused there rather than engaged.
                let court = self
                    .palw_court_params_v2
                    .as_ref()
                    .ok_or_else(|| "a court close declaration on a network with no V2 court parameters".to_string())?;
                kaspa_consensus_core::palw_court_v2::check_close_declared_chunk_count_v2(session_id, *count, court)
                    .map_err(|e| e.to_string())?;
                kaspa_consensus_core::palw_court_v2::check_court_close_declaration_acceptance_v2(
                    state,
                    session_id,
                    *side,
                    *count,
                    chunk_digests,
                    close_digest,
                    *verdict,
                    signature,
                    |key, message, sig, context| {
                        kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                    },
                )
                .map_err(|e| e.to_string())?;
            }
            // **ADR-0080 design A, W7: the chunk that COMPLETES a group is adjudicated here.**
            //
            // A chunk carries no signature and needs none — the declaration already pinned
            // these bytes at this index, and the transition refuses anything else — so for
            // every chunk but the last there is nothing for this layer to add.
            //
            // The last one is a court close. The split exists so that a close too wide for one
            // carrier can be filed at all, and the whole point of design A is that it reaches
            // the SAME verdict by the SAME function: `adjudicate_court_close_v2` over the
            // assembled proof, compared against the verdict the declaration announced, refused
            // in either direction — "an asserted conviction and an asserted acquittal are the
            // same lie", exactly as the one-carrier `CourtClosed` arm below says it.
            //
            // **What this arm does NOT refuse, and why that is not an omission.** Bytes that do
            // not hash to the declaration's own `close_digest`, or that do not decode to this
            // session's `CourtClosed`, are ADMITTED here — because a refused lifecycle object
            // is dropped with the block standing, which would leave the group intact and the
            // declarer sitting on its reserve until the backstop. Those two facts are
            // establishable from rooted state alone and are entirely the declarer's doing, so
            // the transition convicts on them in the block that carries the last chunk (W5's
            // "a failed declaration loses on its OWN side"). This layer only ever refuses what
            // it alone can see: whether the proof inside adjudicates.
            Obj::CourtCloseChunk { session_id, side, index, bytes } => {
                if kaspa_consensus_core::palw_state_v2::palw_court_close_completes_a_group_v1(state, object)
                    && let Some(group) = state.court_close_group(session_id, *side)
                {
                    let mut assembled = Vec::new();
                    for i in 0..group.count {
                        match (i == *index).then_some(bytes).or_else(|| group.chunks.get(&i)) {
                            Some(part) => assembled.extend_from_slice(part),
                            // Unreachable behind the predicate above, which is exactly the
                            // "every part present" test — asserted rather than assumed, because
                            // a hole here would adjudicate a truncated proof.
                            None => return Err(format!("court {session_id} completes with chunk {i} missing")),
                        }
                    }
                    let assembles =
                        kaspa_consensus_core::palw_state_v2::palw_court_close_chunk_digest_v1(&assembled) == group.close_digest;
                    let decoded = assembles
                            .then(|| borsh::from_slice::<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>(&assembled).ok())
                            .flatten()
                            // RFC-0002 Phase F: below `palw_tir_v1` an IR proof does not decode on an
                            // older build, so here it is bytes that do not decode as well — admitted,
                            // and the transition convicts the declarer exactly as the older build does.
                            .filter(|object| {
                                !matches!(object, Obj::CourtClosed { proof, .. }
                                    if proof.is_tir_v1() && !self.palw_tir_at(point.daa_score))
                            })
                            // RFC-0003: likewise a generative proof below `palw_gen_v1`.
                            .filter(|object| {
                                !matches!(object, Obj::CourtClosed { proof, .. }
                                    if proof.is_gen_v1() && !self.palw_gen_at(point.daa_score))
                            })
                            // The task-head profile: likewise a proof carrying a `Head` job below `palw_task_heads_v1`.
                            .filter(|object| {
                                !kaspa_consensus_core::palw_task_heads_v1::palw_object_needs_task_heads_v1(object)
                                    || self.palw_task_heads_at(point.daa_score)
                            })
                            // RFC-0004 A6: likewise an evaluation proof below `palw_improvement_v1`.
                            .filter(|object| {
                                !matches!(object, Obj::CourtClosed { proof, .. }
                                    if proof.is_eval_v1() && !self.palw_improvement_at(point.daa_score))
                            })
                            // ADR-0096 Decision 7: likewise a constrained decode proof below `palw_fp_decode_constraint`.
                            .filter(|object| {
                                !matches!(object, Obj::CourtClosed { proof, .. }
                                    if proof.is_constrained_decode_v1() && !self.palw_fp_decode_constraint_at(point.daa_score))
                            });
                    // Only when the bytes ARE this session's close does the adjudication run;
                    // anything else is the transition's conviction, not this layer's refusal.
                    if let Some(Obj::CourtClosed { session_id: decoded_session, verdict, proof }) = &decoded
                        && decoded_session == session_id
                        && *verdict == group.verdict
                    {
                        let court = self
                            .palw_court_params_v2
                            .as_ref()
                            .ok_or_else(|| "a court close on a network with no V2 court parameters".to_string())?;
                        let derived = kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v5(
                            state,
                            session_id,
                            proof,
                            court,
                            self.palw_court_step_ladder_at(point.daa_score, court),
                            self.palw_prompt_ids_form_at(point.daa_score),
                            // ADR-0119 Decision 4: a fused site's rows open at the claim's
                            // ladder under the held regime.
                            self.palw_held_context_at(point.daa_score),
                            // ADR-0152 v3.1 addendum §4-bis.9: the court door.
                            self.palw_offence_attribution_at(point.daa_score),
                            // RFC-0001 §A: a decode-token close does not try an FP Job V4 claim.
                            self.palw_fp_decode_rules
                                .filter(|f| *f != kaspa_consensus_core::config::params::ForkActivation::never())
                                .map(|f| f.daa_score()),
                            // RFC-0006 R6-CELL round 4: the coverage closure's mirror (None on every shipped network).
                            self.palw_state_params_v2.as_ref().and_then(|p| p.cell_coverage_closure_from_daa()),
                            // RFC-0006 consumer v2's mirror (None on every shipped network).
                            self.palw_state_params_v2.as_ref().and_then(|p| p.memory_routing_court_from_daa()),
                        )
                        .map_err(|e| e.to_string())?;
                        if derived != *verdict {
                            return Err(format!(
                                "court {session_id}: the {} side declared {verdict:?} and the close it assembled adjudicates \
                                     {derived:?}",
                                side.name()
                            ));
                        }
                    }
                }
            }
            Obj::CourtDisclosed { session_id, disclosure, signature } => {
                // The responder's rung, under the responder's key. Unsigned, a challenger
                // could write the answers it wants to convict.
                kaspa_consensus_core::palw_court_v2::check_court_disclosure_acceptance_v2(
                    state,
                    session_id,
                    disclosure,
                    signature,
                    |key, message, sig, context| {
                        kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                    },
                )
                .map_err(|e| e.to_string())?;
            }
            Obj::CourtVerdictPosted { session_id, verdict, signature } => {
                kaspa_consensus_core::palw_court_v2::check_court_verdict_acceptance_v2(
                    state,
                    session_id,
                    verdict,
                    signature,
                    |key, message, sig, context| {
                        kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                    },
                )
                .map_err(|e| e.to_string())?;
            }
            Obj::CourtClosed { session_id, verdict, proof } => {
                // RFC-0002 Phase F: an IR close is dropped by name below `palw_tir_v1`, exactly
                // as an older build skips the payload it cannot decode (A-2).
                if proof.is_tir_v1() && !self.palw_tir_at(point.daa_score) {
                    return Err(format!(
                        "court {session_id}: an IR close is refused: palw_tir_v1 is not in force at this block (RFC-0002 Phase F)"
                    ));
                }
                // RFC-0003: a generative close likewise below `palw_gen_v1`.
                if proof.is_gen_v1() && !self.palw_gen_at(point.daa_score) {
                    return Err(format!(
                        "court {session_id}: a generative close is refused: palw_gen_v1 is not in force at this block (RFC-0003)"
                    ));
                }
                // ADR-0096 Decision 7: a constrained decode close likewise below `palw_fp_decode_constraint`.
                if proof.is_constrained_decode_v1() && !self.palw_fp_decode_constraint_at(point.daa_score) {
                    return Err(format!(
                        "court {session_id}: a constrained decode close is refused: palw_fp_decode_constraint is not in force at this block (ADR-0096)"
                    ));
                }
                // RFC-0004 A6: an evaluation close likewise below `palw_improvement_v1`.
                if proof.is_eval_v1() && !self.palw_improvement_at(point.daa_score) {
                    return Err(format!(
                        "court {session_id}: an evaluation close is refused: palw_improvement_v1 is not in force at this block (RFC-0004)"
                    ));
                }
                // The close carries its proof now, so there is something to adjudicate. The
                // node re-derives the verdict from the proof and compares: a declared verdict
                // that its own proof does not produce is refused, in EITHER direction — an
                // asserted conviction and an asserted acquittal are the same lie.
                //
                // `adjudicate_court_close_v2` refuses outright when the proof does not
                // adjudicate (an out-of-catalog kernel, a non-canonical operand set, a
                // binding naming another execution), so an unadjudicable object convicts
                // nobody AND acquits nobody — P0-8's rule, on both sides.
                //
                // ADR-0049 Decision C, audit H-03: the ruleset's cost ceilings are applied to
                // the OBJECT, before a single Merkle path is walked. They were checked only
                // against a CLASS's geometry at admission — a bound the ruleset id commits to
                // and that nothing enforced where it is actually spendable.
                let court = self
                    .palw_court_params_v2
                    .as_ref()
                    .ok_or_else(|| "a court close on a network with no V2 court parameters".to_string())?;
                let derived = kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v5(
                    state,
                    session_id,
                    proof,
                    court,
                    self.palw_court_step_ladder_at(point.daa_score, court),
                    self.palw_prompt_ids_form_at(point.daa_score),
                    // ADR-0119 Decision 4.
                    self.palw_held_context_at(point.daa_score),
                    // ADR-0152 v3.1 addendum §4-bis.9: the court door.
                    self.palw_offence_attribution_at(point.daa_score),
                    // RFC-0001 §A: a decode-token close does not try an FP Job V4 claim.
                    self.palw_fp_decode_rules
                        .filter(|f| *f != kaspa_consensus_core::config::params::ForkActivation::never())
                        .map(|f| f.daa_score()),
                    // RFC-0006 R6-CELL round 4: the coverage closure's mirror (None on every shipped network).
                    self.palw_state_params_v2.as_ref().and_then(|p| p.cell_coverage_closure_from_daa()),
                    // RFC-0006 consumer v2's mirror (None on every shipped network).
                    self.palw_state_params_v2.as_ref().and_then(|p| p.memory_routing_court_from_daa()),
                )
                .map_err(|e| e.to_string())?;
                if derived != *verdict {
                    return Err(format!("court {session_id} declares {verdict:?}; its own proof adjudicates {derived:?}"));
                }
            }
            // -------------------------------------------------------------------------
            // ADR-0082 Decision 2/3 — the dissection's three moves.
            //
            // Two gates, in this order: the FENCE (a move on a network that never armed the
            // k-ary court is refused by name — the arm exists in the binary but the rule does
            // not exist on this chain), then the party's signature, the same split every
            // other court move uses.
            // -------------------------------------------------------------------------
            Obj::CourtAttnRootClaimed { session_id, root, arity, signature, .. }
            | Obj::CourtAttnRootClaimedAnchored { session_id, root, arity, signature, .. }
            | Obj::CourtAttnRootClaimedHeld { session_id, root, arity, signature, .. } => {
                // ADR-0093 Decision 8: the anchored form exists only where its fence does. (The
                // plain form's refusal past the fence needs the site, so the fold makes it.)
                if matches!(object, Obj::CourtAttnRootClaimedAnchored { .. }) && !self.palw_attn_anchored_root_at(point.daa_score) {
                    return Err(format!(
                        "session {session_id}: an anchored root claim before palw_attn_anchored_root is armed (ADR-0093 Decision 8)"
                    ));
                }
                // ADR-0152 §4-ter C2: the held form (tag 57) exists only past
                // `palw_offence_attribution`, at the block's own DAA. Whether the class is an
                // answerable held one, and whether the sub-roots root to the anchor, need the
                // session and the site, so the fold makes those refusals; the signature and the
                // arity are the same checks as the other two forms', below.
                if matches!(object, Obj::CourtAttnRootClaimedHeld { .. }) && !self.palw_offence_attribution_at(point.daa_score) {
                    return Err(format!(
                        "session {session_id}: a held root claim before palw_offence_attribution is armed (ADR-0152 §4-ter)"
                    ));
                }
                kaspa_consensus_core::palw_court_v2::palw_attn_move_is_admissible_v2(
                    object,
                    self.palw_kary_court_active_at(point.daa_score),
                )
                .map_err(|e| format!("session {session_id}: {e}"))?;
                // **The declared arity must be the ruleset's own** (patch note 7). The object
                // states it because the fold cannot derive it — the derivation reads the
                // bundle — and this is the layer that holds the bundle, so this is the layer
                // that refuses any other value. Without the comparison the responder would
                // choose how many children it discloses per round, which is the move budget
                // Z4 sizes the window against.
                let derived = self
                    .palw_court_params_at(point.daa_score)
                    .ok_or_else(|| "a dissection move on a network with no V2 bundle".to_string())?
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                if *arity != derived.dissection_arity() {
                    return Err(format!(
                        "session {session_id}: the root claim declares dissection arity {arity}; this ruleset derives {}",
                        derived.dissection_arity()
                    ));
                }
                kaspa_consensus_core::palw_court_v2::check_court_attn_root_claim_acceptance_v2(
                    state,
                    session_id,
                    root,
                    signature,
                    |key, message, sig, context| {
                        kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                    },
                )
                .map_err(|e| e.to_string())?;
            }
            Obj::CourtAttnDissected { session_id, round, signature } => {
                kaspa_consensus_core::palw_court_v2::palw_attn_move_is_admissible_v2(
                    object,
                    self.palw_kary_court_active_at(point.daa_score),
                )
                .map_err(|e| format!("session {session_id}: {e}"))?;
                kaspa_consensus_core::palw_court_v2::check_court_attn_round_acceptance_v2(
                    state,
                    session_id,
                    round,
                    signature,
                    |key, message, sig, context| {
                        kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                    },
                )
                .map_err(|e| e.to_string())?;
            }
            Obj::CourtAttnChildChosen { session_id, choice, signature } => {
                kaspa_consensus_core::palw_court_v2::palw_attn_move_is_admissible_v2(
                    object,
                    self.palw_kary_court_active_at(point.daa_score),
                )
                .map_err(|e| format!("session {session_id}: {e}"))?;
                kaspa_consensus_core::palw_court_v2::check_court_attn_choice_acceptance_v2(
                    state,
                    session_id,
                    choice,
                    signature,
                    |key, message, sig, context| {
                        kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                    },
                )
                .map_err(|e| e.to_string())?;
            }
            // -------------------------------------------------------------------------
            // RFC-0002 Phase F, F7 — the IR history dissection's three moves (spec 04b §9.5).
            //
            // ADR-0082's gates: the FENCE (the k-ary court — `palw_tir_v1` dropped the object by
            // name above), the party's SIGNATURE, and for the root claim the arity against the
            // ruleset and the FINALIZE: the tile evaluated from the claimed totals must be the
            // committed one, a demand evaluation at the IR court's limits and this block's step
            // ladder — the same the close of its bottom is judged at — which is why the move
            // takes the block's adjudication slot (`palw_court_move_spends_the_slot_v1`).
            // -------------------------------------------------------------------------
            Obj::CourtTirRootClaimed { session_id, root, arity, signature } => {
                kaspa_consensus_core::palw_court_v2::palw_tir_dissection_move_is_admissible_v1(
                    object,
                    self.palw_kary_court_active_at(point.daa_score),
                )
                .map_err(|e| format!("session {session_id}: {e}"))?;
                let derived = self
                    .palw_court_params_at(point.daa_score)
                    .ok_or_else(|| "an IR dissection move on a network with no V2 bundle".to_string())?
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                let court = self
                    .palw_court_params_v2
                    .as_ref()
                    .ok_or_else(|| "an IR dissection move on a network with no V2 court parameters".to_string())?;
                kaspa_consensus_core::palw_court_v2::check_court_tir_root_claim_acceptance_v1(
                    state,
                    session_id,
                    root,
                    signature,
                    |key, message, sig, context| {
                        kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                    },
                )
                .map_err(|e| e.to_string())?;
                // The claim's CLASS ladder, as every held court arm reads it (an IR class records
                // none, so it is the network's; a missing session is refused by the check below).
                let bare = self.palw_court_step_ladder_at(point.daa_score, court);
                let ladder = state
                    .court_session(session_id)
                    .and_then(|session| state.claim(&session.claim))
                    .map_or(bare, |claim| state.class_step_ladder_v1(&claim.class_id, bare));
                kaspa_consensus_core::palw_court_v2::check_court_tir_root_claim_admits_v1(
                    state,
                    session_id,
                    root,
                    *arity,
                    derived.dissection_arity(),
                    court,
                    ladder,
                    self.palw_prompt_ids_form_at(point.daa_score),
                )
                .map_err(|e| format!("session {session_id}: {e}"))?;
            }
            // RFC-0003 (tag 69): the generative root claim — F7's gates over the generative
            // registry: the k-ary fence (`palw_gen_v1` dropped it by name below), the responder's
            // signature, the arity and the finalize at the court's limits.
            Obj::CourtGenRootClaimed { session_id, root, arity, signature } => {
                kaspa_consensus_core::palw_court_v2::palw_tir_dissection_move_is_admissible_v1(
                    object,
                    self.palw_kary_court_active_at(point.daa_score),
                )
                .map_err(|e| format!("session {session_id}: {e}"))?;
                let derived = self
                    .palw_court_params_at(point.daa_score)
                    .ok_or_else(|| "a generative dissection move on a network with no V2 bundle".to_string())?
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                let court = self
                    .palw_court_params_v2
                    .as_ref()
                    .ok_or_else(|| "a generative dissection move on a network with no V2 court parameters".to_string())?;
                kaspa_consensus_core::palw_court_v2::check_court_gen_root_claim_acceptance_v1(
                    state,
                    session_id,
                    root,
                    signature,
                    |key, message, sig, context| {
                        kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                    },
                )
                .map_err(|e| e.to_string())?;
                kaspa_consensus_core::palw_court_v2::check_court_gen_root_claim_admits_v1(
                    state,
                    session_id,
                    root,
                    *arity,
                    derived.dissection_arity(),
                    court,
                    self.palw_prompt_ids_form_at(point.daa_score),
                )
                .map_err(|e| format!("session {session_id}: {e}"))?;
            }
            // RFC-0004 A6: the evaluation root claim — the generative arm's gates over the claim's own evaluation
            // context: the improvement fence (the walk dropped it by name below), the k-ary court, the
            // responder's signature, the arity and the finalize at the court's limits.
            Obj::CourtEvalRootClaimed { session_id, root, arity, signature } => {
                if !self.palw_improvement_at(point.daa_score) {
                    return Err(format!(
                        "session {session_id}: an evaluation root claim is refused: palw_improvement_v1 is not in force (RFC-0004)"
                    ));
                }
                kaspa_consensus_core::palw_court_v2::palw_tir_dissection_move_is_admissible_v1(
                    object,
                    self.palw_kary_court_active_at(point.daa_score),
                )
                .map_err(|e| format!("session {session_id}: {e}"))?;
                let derived = self
                    .palw_court_params_at(point.daa_score)
                    .ok_or_else(|| "an evaluation dissection move on a network with no V2 bundle".to_string())?
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                let court = self
                    .palw_court_params_v2
                    .as_ref()
                    .ok_or_else(|| "an evaluation dissection move on a network with no V2 court parameters".to_string())?;
                kaspa_consensus_core::palw_court_v2::check_court_eval_root_claim_acceptance_v1(
                    state,
                    session_id,
                    root,
                    signature,
                    |key, message, sig, context| {
                        kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                    },
                )
                .map_err(|e| e.to_string())?;
                kaspa_consensus_core::palw_court_v2::check_court_eval_root_claim_admits_v1(
                    state,
                    session_id,
                    root,
                    *arity,
                    derived.dissection_arity(),
                    court,
                )
                .map_err(|e| format!("session {session_id}: {e}"))?;
            }
            Obj::CourtTirDissected { session_id, round, signature } => {
                kaspa_consensus_core::palw_court_v2::palw_tir_dissection_move_is_admissible_v1(
                    object,
                    self.palw_kary_court_active_at(point.daa_score),
                )
                .map_err(|e| format!("session {session_id}: {e}"))?;
                kaspa_consensus_core::palw_court_v2::check_court_tir_round_acceptance_v1(
                    state,
                    session_id,
                    round,
                    signature,
                    |key, message, sig, context| {
                        kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                    },
                )
                .map_err(|e| e.to_string())?;
            }
            Obj::CourtTirChildChosen { session_id, choice, signature } => {
                kaspa_consensus_core::palw_court_v2::palw_tir_dissection_move_is_admissible_v1(
                    object,
                    self.palw_kary_court_active_at(point.daa_score),
                )
                .map_err(|e| format!("session {session_id}: {e}"))?;
                kaspa_consensus_core::palw_court_v2::check_court_tir_choice_acceptance_v1(
                    state,
                    session_id,
                    choice,
                    signature,
                    |key, message, sig, context| {
                        kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                    },
                )
                .map_err(|e| e.to_string())?;
            }
            Obj::DefaultAccused { claim, missing_event_index, accuser, signature } => {
                if !self.palw_da_court_at(point.daa_score) {
                    return Err(format!("claim {claim}: the data-availability court is not armed on this network (ADR-0062)"));
                }
                let record =
                    state.bond(accuser).ok_or_else(|| format!("an accusation names bond {accuser:?} this chain does not have"))?;
                let message = kaspa_consensus_core::palw_state_v2::palw_da_accusation_message_v2(
                    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    ),
                    claim,
                    *missing_event_index,
                    accuser,
                );
                if !Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    message.as_byte_slice(),
                    signature,
                    kaspa_consensus_core::palw_state_v2::PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT,
                ) {
                    return Err(format!("claim {claim}'s accusation is not signed by the bond it names"));
                }
            }
            // **ADR-0099 Decision 5, built by ADR-0100: the one-move court's accusation.** Two
            // gates in the order every court move uses — the FENCE, then the accuser's bond
            // key over the session id — then the object's shape, the ruleset's own close
            // ceiling, the claim it names, and the verdict DERIVED here at the ladder the fold
            // will derive it at too (the extras carry that ladder to the fold). A fused site is
            // refused, because its verdict convicts nobody; an accusation whose refutation does
            // not adjudicate is refused too — P0-8's rule on both sides. The FENCE is checked
            // here as well as in the fold, for the DA court's reason.
            // **RFC-0002 Phase F (tag 62): an IR claim accused in one move.** Signed over its
            // session id by the accuser's registered key; about a live claim of an IR class,
            // its executor and roots the claim's; and the verdict it DECLARES is the one its proof
            // produces under this block's court (`palw_tir_one_move_verdict_v1`, the court close's
            // own adjudication) — refused in either direction otherwise, so the fold may apply it.
            Obj::TirShardCourtAccused { accusation } => {
                let claim_id = accusation.claim;
                if !self.palw_tir_at(point.daa_score) {
                    return Err(format!("claim {claim_id}: an IR accusation is refused: palw_tir_v1 is not in force (RFC-0002)"));
                }
                // RFC-0004 A6: an evaluation close rides this accusation past `palw_improvement_v1` only.
                if accusation.proof.is_eval_v1() && !self.palw_improvement_at(point.daa_score) {
                    return Err(format!(
                        "claim {claim_id}: an evaluation accusation is refused: palw_improvement_v1 is not in force (RFC-0004)"
                    ));
                }
                kaspa_consensus_core::palw_tir_one_move_v1::palw_tir_one_move_shape_v1(accusation)
                    .map_err(|e| format!("claim {claim_id}: {e}"))?;
                let court = self
                    .palw_court_params_v2
                    .as_ref()
                    .ok_or_else(|| "an IR one-move accusation on a network with no V2 court parameters".to_string())?;
                let record = state
                    .bond(&accusation.accuser_bond)
                    .ok_or_else(|| format!("an accusation names bond {:?} this chain does not have", accusation.accuser_bond))?;
                let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                    self.network_id_bytes.as_slice(),
                    Some(self.genesis.hash),
                );
                let session_id =
                    kaspa_consensus_core::palw_tir_one_move_v1::palw_tir_one_move_session_id_v1(domain.as_byte_slice(), accusation);
                if !Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    session_id.as_byte_slice(),
                    &accusation.signature,
                    kaspa_consensus_core::palw_tir_one_move_v1::PALW_TIR_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1,
                ) {
                    return Err(format!("claim {claim_id}'s IR accusation is not signed by the bond it names"));
                }
                let claim =
                    state.claim(&claim_id).ok_or_else(|| format!("an accusation names claim {claim_id} this chain does not have"))?;
                if state.tir_class_v1(&claim.class_id).is_none() {
                    return Err(format!("claim {claim_id} is not a claim of an IR class"));
                }
                if claim.bond != accusation.executor_bond
                    || claim.execution_root != accusation.execution_root
                    || claim.trace_root != accusation.trace_root
                {
                    return Err(format!("claim {claim_id}: the accusation's executor or roots are not the claim's"));
                }
                let ladder = state.class_step_ladder_v1(&claim.class_id, self.palw_court_step_ladder_at(point.daa_score, court));
                // RFC-0002 F7: under the held regime a cone accusation at a dissected leaf opens a
                // dissection there (the fold asks the same question); it declares the conviction
                // it is prosecuting. Every other accusation's verdict is re-derived and compared.
                let outcome = kaspa_consensus_core::palw_tir_one_move_v1::palw_tir_one_move_outcome_v1(
                    state,
                    claim,
                    accusation,
                    court,
                    ladder,
                    self.palw_prompt_ids_form_at(point.daa_score),
                    self.palw_held_context_at(point.daa_score),
                )
                .map_err(|e| format!("claim {claim_id}: the IR accusation does not adjudicate: {e}"))?;
                match outcome {
                    kaspa_consensus_core::palw_tir_one_move_v1::PalwTirOneMoveOutcomeV1::Verdict(derived) => {
                        if derived != accusation.verdict {
                            return Err(format!(
                                "claim {claim_id}: the IR accusation declares {:?} and its proof produces {derived:?}",
                                accusation.verdict
                            ));
                        }
                    }
                    kaspa_consensus_core::palw_tir_one_move_v1::PalwTirOneMoveOutcomeV1::NeedsDissection { leaf } => {
                        if accusation.verdict != kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty {
                            return Err(format!(
                                "claim {claim_id}: an IR accusation at dissected leaf {leaf} opens a dissection and declares \
                                     ExecutorGuilty, not {:?}",
                                accusation.verdict
                            ));
                        }
                    }
                }
            }
            // **RFC-0003 §I.4.7 (tensor claims' court under the held regime): a pipeline claim accused in one
            // move.** The IR accusation's gates with the generative registry: signed over its session id by
            // the accuser's registered key; about a live claim of a generative class, its executor and roots the
            // claim's; and the verdict it DECLARES is the one its proof produces under this block's court
            // (`palw_gen_one_move_verdict_v1`, the court close's own adjudication) — refused in either direction
            // otherwise, so the fold may apply it. A cone accusation at a dissected leaf declares ExecutorGuilty
            // and opens a dissection under the held regime.
            Obj::GenShardCourtAccused { accusation } => {
                let claim_id = accusation.claim;
                if !self.palw_gen_at(point.daa_score) {
                    return Err(format!(
                        "claim {claim_id}: a generative accusation is refused: palw_gen_v1 is not in force (RFC-0003)"
                    ));
                }
                kaspa_consensus_core::palw_gen_one_move_v1::palw_gen_one_move_shape_v1(accusation)
                    .map_err(|e| format!("claim {claim_id}: {e}"))?;
                let court = self
                    .palw_court_params_v2
                    .as_ref()
                    .ok_or_else(|| "a generative one-move accusation on a network with no V2 court parameters".to_string())?;
                let record = state
                    .bond(&accusation.accuser_bond)
                    .ok_or_else(|| format!("an accusation names bond {:?} this chain does not have", accusation.accuser_bond))?;
                let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                    self.network_id_bytes.as_slice(),
                    Some(self.genesis.hash),
                );
                let session_id =
                    kaspa_consensus_core::palw_gen_one_move_v1::palw_gen_one_move_session_id_v1(domain.as_byte_slice(), accusation);
                if !Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    session_id.as_byte_slice(),
                    &accusation.signature,
                    kaspa_consensus_core::palw_gen_one_move_v1::PALW_GEN_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1,
                ) {
                    return Err(format!("claim {claim_id}'s generative accusation is not signed by the bond it names"));
                }
                let claim =
                    state.claim(&claim_id).ok_or_else(|| format!("an accusation names claim {claim_id} this chain does not have"))?;
                if state.gen_class_v1(&claim.class_id).is_none() {
                    return Err(format!("claim {claim_id} is not a claim of a pipeline class"));
                }
                if claim.bond != accusation.executor_bond
                    || claim.execution_root != accusation.execution_root
                    || claim.trace_root != accusation.trace_root
                {
                    return Err(format!("claim {claim_id}: the accusation's executor or roots are not the claim's"));
                }
                let ladder = state.class_step_ladder_v1(&claim.class_id, self.palw_court_step_ladder_at(point.daa_score, court));
                let outcome = kaspa_consensus_core::palw_gen_one_move_v1::palw_gen_one_move_outcome_v1(
                    state,
                    claim,
                    accusation,
                    court,
                    ladder,
                    self.palw_prompt_ids_form_at(point.daa_score),
                    self.palw_held_context_at(point.daa_score),
                )
                .map_err(|e| format!("claim {claim_id}: the generative accusation does not adjudicate: {e}"))?;
                match outcome {
                    kaspa_consensus_core::palw_gen_one_move_v1::PalwGenOneMoveOutcomeV1::Verdict(derived) => {
                        if derived != accusation.verdict {
                            return Err(format!(
                                "claim {claim_id}: the generative accusation declares {:?} and its proof produces {derived:?}",
                                accusation.verdict
                            ));
                        }
                    }
                    kaspa_consensus_core::palw_gen_one_move_v1::PalwGenOneMoveOutcomeV1::NeedsDissection { leaf } => {
                        if accusation.verdict != kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty {
                            return Err(format!(
                                "claim {claim_id}: a generative accusation at dissected leaf {leaf} opens a dissection and                                      declares ExecutorGuilty, not {:?}",
                                accusation.verdict
                            ));
                        }
                    }
                }
            }
            // **RFC-0003 decision 22 (the held leaf challenge): a named-leaf challenge with the declaration of its
            // close.** Signed over its digest by the accuser's registered key, within the ruleset's own carriage
            // count; everything about the claim (live, its class, its roots, the leaf, the open sessions, the
            // accuser's standing) is the fold's, where the state is in hand, and the rehearsal drops what the fold
            // refuses. It adjudicates nothing — the close it pins is graded, once, by the completing chunk.
            Obj::HeldLeafChallengeDeclared { challenge } => {
                let claim_id = challenge.claim;
                if !self.palw_held_close_chunks_at(point.daa_score) {
                    return Err(format!(
                        "claim {claim_id}: a held leaf challenge is refused: palw_held_close_chunks_v1 is not in force (RFC-0003)"
                    ));
                }
                let court = self
                    .palw_court_params_v2
                    .as_ref()
                    .ok_or_else(|| "a held leaf challenge on a network with no V2 court parameters".to_string())?;
                let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                    self.network_id_bytes.as_slice(),
                    Some(self.genesis.hash),
                );
                kaspa_consensus_core::palw_held_close_v1::check_held_leaf_challenge_acceptance_v1(
                    state,
                    domain.as_byte_slice(),
                    challenge,
                    court,
                    |key, message, sig, context| {
                        kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                    },
                )
                .map_err(|e| format!("claim {claim_id}: {e}"))?;
            }
            Obj::GdnStepAccusedV2 { accusation } => {
                if !self.palw_memory_routing_court_v2_at(point.daa_score) {
                    return Err("GDN version-2 court is dormant on this network".into());
                }
                let court = self.palw_court_params_v2.as_ref().ok_or("GDN accusation without V2 court parameters")?;
                let claim = state.claim(&accusation.claim).ok_or("GDN accusation names an absent native claim")?;
                let ladder = state.class_step_ladder_v1(&claim.class_id, self.palw_court_step_ladder_at(point.daa_score, court));
                accusation.validate_shape(ladder)?;
                if accusation.encoded_bytes()? > court.max_close_bytes() {
                    return Err("GDN accusation exceeds the current close-byte ceiling".into());
                }
                if claim.bond != accusation.executor_bond
                    || claim.execution_root != accusation.execution_root
                    || claim.trace_root != accusation.trace_root
                {
                    return Err("GDN accusation executor/roots are not the actual native claim's".into());
                }
                state.class(&claim.class_id).ok_or("GDN accusation class is absent")?;
                let signer = state.bond(&accusation.accuser_bond).ok_or("GDN accuser bond is absent")?;
                let network = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                    self.network_id_bytes.as_slice(),
                    Some(self.genesis.hash),
                );
                let message = kaspa_consensus_core::palw_gdn_court_v2::palw_gdn_court_session_id_v2(network, accusation);
                if !Self::verify_mldsa87_with_context_bool(
                    &signer.pubkey,
                    message.as_byte_slice(),
                    &accusation.signature,
                    kaspa_consensus_core::palw_gdn_court_v2::PALW_GDN_COURT_MLDSA87_CONTEXT_V2,
                ) {
                    return Err("GDN accusation is not signed by its registered accuser".into());
                }
                // Full deterministic adjudication runs in the typed fold.
                // This String-only walk must not turn a local workspace
                // refusal into a skipped invalid peer object.
            }
            // RFC-0006 consumer v2 (tag 244): the fence, the close ceiling and the signer — the class registrant for a router
            // binding (like `ClassSemanticsDeclaredV1`), the accuser for an accusation (like tag 243). Adjudication and the
            // binding's graph checks run in the typed fold.
            // ADR-0191 step B (tag 245): the same shape as tag 244 below. Below `palw_tir_designated_answers_v1` it is the dormant
            // payload (A-2) and refused; armed, it is folded into the designated registry by the typed fold (`apply_tir_designated_event_v1`).
            Obj::TirDesignatedEventV1 { .. } => {
                if !self.palw_tir_designated_answers_v1_at(point.daa_score) {
                    return Err("the designated-custodian event is dormant on this network".into());
                }
            }
            // RFC-0032 A material intake (tag 255, lane r5-f03-ingress). A-2: below its owning fence (`palw_staged_tir_route_v1`)
            // the kind rides unjudged, as every build without it reads it. Past it: the submitter's registered Active bond and
            // its ML-DSA-87 signature over the network-bound step. The material itself (exact native width, input cover,
            // checksum, per-submitter budget) and the typed-missing storage schedule are judged by the typed fold.
            // ADR-0206 PanelDa disclosure (tag 233). It was routed here with no arm, so the gate hit `unreachable!` on any tag-233
            // object (lane J1's node E2E found it). A-2: below `palw_panel_da_disclosure_v1` it rides unjudged. Past it, no signature
            // (the hash commitment fixes which ids, so anyone may file) — the row's own admission: a governed claim, the ids bind,
            // inside `a..=deadline`, once. The fold repeats the same check.
            Obj::PanelDaPromptDisclosedV1 { claim, prompt_token_ids } => {
                if !self.palw_lifecycle_kind_fences.kind_in_force_at(object, point.daa_score) {
                    return Ok(());
                }
                let row = state.panel_da_disclosure_v1(claim).ok_or_else(|| format!("claim {claim}: no PanelDa disclosure row"))?;
                row.admit_v1(prompt_token_ids, point.daa_score)
                    .map_err(|why| format!("claim {claim}: disclosure refused: {why:?}"))?;
            }
            Obj::MaterialIntakeV1 { intake } => {
                if !self.palw_lifecycle_kind_fences.kind_in_force_at(object, point.daa_score) {
                    return Ok(());
                }
                let record = state.bond(&intake.submitter).ok_or("a material intake names a bond this chain does not have")?;
                if !matches!(record.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                    return Err("a material intake submitter's bond is not Active".into());
                }
                intake.verify_v1(self.palw_network_domain_v2(), &record.pubkey)?;
            }
            // R-D2-READY (tag 230, ADR-0188 B / ADR-0200 C1): below `palw_court_model_scope_v1` the dormant payload (A-2), refused.
            // Past it: the ADOPTED draw cap first — unadopted (`None`), so every root is refused here; then the literal seat
            // authority (the seat bond's ML-DSA-87 signature over the network-bound root). No ready credit or slash is granted.
            Obj::SeatReadinessRootV1 { root, signature } => {
                use kaspa_consensus_core::palw_court_model_scope_routes_v1::readiness_root as rr;
                if !state_params.court_model_scope_from_daa().is_some_and(|from| point.daa_score >= from) {
                    return Err("the seat readiness root is dormant on this network (palw_court_model_scope_v1)".into());
                }
                rr::palw_d2_readiness_root_admit_v1(root, rr::PALW_D2_READINESS_DRAW_CAP_V1)?;
                let record = state.bond(&root.seat).ok_or("a readiness root names a seat bond this chain does not have")?;
                let message = rr::palw_d2_readiness_root_message_v1(
                    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    ),
                    root,
                );
                if !Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    message.as_byte_slice(),
                    signature,
                    rr::PALW_D2_READINESS_ROOT_MLDSA87_CONTEXT_V1,
                ) {
                    return Err("the readiness root is not signed by its seat's bond".into());
                }
            }
            // ADR-0200 C1 (tags 231/232, G14 item 6): dormant below `palw_court_model_scope_v1`; past it the unadopted draw cap
            // refuses first (as for 230), then the literal signer: the challenger's bond for a demand, the RECORDED root's seat
            // bond for an answer (read from the rooted registry, never from the object). The court's rules are the fold's.
            Obj::SeatReadinessDemandedV1 { demand, signature } => {
                use kaspa_consensus_core::palw_court_model_scope_routes_v1::{readiness_disclosure as dc, readiness_root as rr};
                if !state_params.court_model_scope_from_daa().is_some_and(|from| point.daa_score >= from) {
                    return Err("the readiness disclosure demand is dormant on this network (palw_court_model_scope_v1)".into());
                }
                if rr::PALW_D2_READINESS_DRAW_CAP_V1.is_none() || dc::PALW_D2_DISCLOSURE_FEE_SCHEDULE_V1.is_none() {
                    return Err("the readiness draw cap or disclosure fee schedule is unadopted".into());
                }
                let record =
                    state.bond(&demand.challenger).ok_or("a readiness demand names a challenger bond this chain does not have")?;
                let message = dc::palw_d2_disclosure_demand_message_v1(
                    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    ),
                    demand,
                );
                if !Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    message.as_byte_slice(),
                    signature,
                    dc::PALW_D2_DISCLOSURE_DEMAND_MLDSA87_CONTEXT_V1,
                ) {
                    return Err("the readiness demand is not signed by its challenger's bond".into());
                }
            }
            Obj::SeatReadinessAnsweredV1 { answer, signature } => {
                use kaspa_consensus_core::palw_court_model_scope_routes_v1::readiness_disclosure as dc;
                if !state_params.court_model_scope_from_daa().is_some_and(|from| point.daa_score >= from) {
                    return Err("the readiness disclosure answer is dormant on this network (palw_court_model_scope_v1)".into());
                }
                let Some(dc::PalwD2ReadinessEntryV1::Root(row)) =
                    state.d2_readiness_v1().get(&dc::PalwD2ReadinessKeyV1::Root(answer.root_key))
                else {
                    return Err("a readiness answer names no recorded readiness root".into());
                };
                let record = state.bond(&row.root.seat).ok_or("a readiness answer's seat bond is not on this chain")?;
                let message = dc::palw_d2_disclosure_answer_message_v1(
                    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    ),
                    answer,
                );
                if !Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    message.as_byte_slice(),
                    signature,
                    dc::PALW_D2_DISCLOSURE_ANSWER_MLDSA87_CONTEXT_V1,
                ) {
                    return Err("the readiness answer is not signed by the root's seat bond".into());
                }
            }
            // RFC-0024 §4 / §5 (tags 201-204): below `palw_tir_bounded_door_v1` the dormant payload (A-2), refused by name.
            // Past it: the mover's signature (responder: the claim's bond; challenger: the session's), the position round's
            // binding to the terminal phase, and — for a close — the full adjudication, whose verdict must be the declared
            // one (either direction is the same lie, as for `CourtClosed`). The lookahead bundle itself is checked by the fold
            // against the adopted frontier bound (typed-missing: refused while unadopted).
            Obj::TirPositionDissectedV2 { dissection, signature } => {
                if !self.palw_tir_bounded_door_v1_at(point.daa_score) {
                    return Err("the position terminal is dormant on this network (palw_tir_bounded_door_v1)".into());
                }
                kaspa_consensus_core::palw_court_v2::check_tir_position_dissected_acceptance_v2(
                    state,
                    dissection,
                    signature,
                    &Self::verify_mldsa87_with_context_bool,
                )
                .map_err(|e| e.to_string())?;
            }
            Obj::TirPositionClosedV2 { close } => {
                if !self.palw_tir_bounded_door_v1_at(point.daa_score) {
                    return Err("the position close is dormant on this network (palw_tir_bounded_door_v1)".into());
                }
                if close.proof.is_tir_v1() && !self.palw_tir_at(point.daa_score) {
                    return Err("a TIR position close is refused: palw_tir_v1 is not in force at this block".into());
                }
                if close.proof.is_gen_v1() && !self.palw_gen_at(point.daa_score) {
                    return Err("a generative position close is refused: palw_gen_v1 is not in force at this block".into());
                }
                if close.proof.is_eval_v1() && !self.palw_improvement_at(point.daa_score) {
                    return Err("an evaluation position close is refused: palw_improvement_v1 is not in force at this block".into());
                }
                kaspa_consensus_core::palw_court_v2::check_tir_position_close_signatures_v2(
                    state,
                    close,
                    Self::verify_mldsa87_with_context_bool,
                )
                .map_err(|e| e.to_string())?;
                let court = self
                    .palw_court_params_v2
                    .as_ref()
                    .ok_or_else(|| "a position close on a network with no V2 court parameters".to_string())?;
                let derived = kaspa_consensus_core::palw_court_v2::adjudicate_tir_position_close_v2(
                    state,
                    close,
                    court,
                    self.palw_court_step_ladder_at(point.daa_score, court),
                    self.palw_prompt_ids_form_at(point.daa_score),
                    kaspa_consensus_core::palw_tir_position_terminal_v2::PALW_TIR_POSITION_WITNESS_BUDGET_V2,
                )
                .map_err(|e| e.to_string())?;
                if derived != close.verdict {
                    return Err(format!(
                        "position close {} declares {:?}; its own proof adjudicates {derived:?}",
                        close.session_id, close.verdict
                    ));
                }
            }
            Obj::TirLookaheadDisclosedV2 { body } => {
                if !self.palw_tir_bounded_door_v1_at(point.daa_score) {
                    return Err("the lookahead disclosure is dormant on this network (palw_tir_bounded_door_v1)".into());
                }
                kaspa_consensus_core::palw_court_v2::check_tir_lookahead_disclosed_acceptance_v2(
                    state,
                    body,
                    Self::verify_mldsa87_with_context_bool,
                )
                .map_err(|e| e.to_string())?;
            }
            Obj::TirLookaheadChosenV2 { body } => {
                if !self.palw_tir_bounded_door_v1_at(point.daa_score) {
                    return Err("the lookahead choice path is dormant on this network (palw_tir_bounded_door_v1)".into());
                }
                kaspa_consensus_core::palw_court_v2::check_tir_lookahead_chosen_acceptance_v2(
                    state,
                    body,
                    Self::verify_mldsa87_with_context_bool,
                )
                .map_err(|e| e.to_string())?;
            }
            Obj::MemoryRoutingV2 { object } => {
                use kaspa_consensus_core::palw_memory_routing_v2 as mr;
                if !self.palw_memory_routing_court_v2_at(point.daa_score) {
                    return Err("the memory/routing consumer v2 is dormant on this network".into());
                }
                let court = self.palw_court_params_v2.as_ref().ok_or("memory/routing object without V2 court parameters")?;
                object.may_ride(court.max_close_bytes())?;
                let network = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                    self.network_id_bytes.as_slice(),
                    Some(self.genesis.hash),
                );
                match object.as_ref() {
                    mr::PalwMemoryRoutingObjectV2::RouterBindingDeclared { declaration, signature } => {
                        use kaspa_consensus_core::palw_router_binding_v2 as rb;
                        let class = state.class(&declaration.class_id).ok_or("router binding names an unknown class")?;
                        if class.artifact_root != declaration.artifact_root {
                            return Err("router binding names another artifact".into());
                        }
                        let registrant = class
                            .registrant_bond
                            .as_ref()
                            .ok_or("a sentinel registration has no signing registrant for a router binding")?;
                        let bond = state.bond(registrant).ok_or("the signing registrant is not a public bond")?;
                        if !matches!(bond.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                            return Err("the signing registrant is not Active".into());
                        }
                        let message = rb::palw_router_binding_message_v2(network, declaration);
                        if !Self::verify_mldsa87_with_context_bool(
                            &bond.pubkey,
                            message.as_byte_slice(),
                            signature,
                            rb::PALW_ROUTER_BINDING_SIGNATURE_CONTEXT_V2,
                        ) {
                            return Err("the class registrant did not sign this router binding".into());
                        }
                        // **RFC06-4: the class's admission price re-derived under THIS binding** — each routed reader's
                        // appendix priced at the row of the router it is bound to (its own width and tile), not the table's
                        // first q36 router — under the court and ladder in force at this block, against the same ceilings the
                        // registration gate applied. A binding under which some routed close could not be carried is refused.
                        // Only here, past the fence; a class with no binding keeps the price it was admitted at.
                        rb::palw_router_binding_check_v2(declaration)?;
                        let bundle = self.palw_v2_bundle.as_ref().ok_or("router binding on a network with no V2 bundle")?;
                        let profile = &declaration.profile;
                        let kary = match self.palw_kary_court_active_at(point.daa_score) {
                            false => None,
                            true => {
                                let derived = self
                                    .palw_court_params_at(point.daa_score)
                                    .ok_or("router binding on a chain with no V2 ruleset")?
                                    .map_err(|e| format!("router binding judged under a court with no shape: {e}"))?;
                                Some(kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 {
                                    dissection_arity: derived.dissection_arity(),
                                    prompt_ids_form: self.palw_prompt_ids_form_at(point.daa_score),
                                    window_court_daa:
                                        kaspa_consensus_core::palw_class_admission_v2::palw_class_court_window_for_shape_v1(
                                            bundle.state.window_court(),
                                            self.palw_model_court_window_active_at(point.daa_score),
                                            self.palw_held_context_at(point.daa_score),
                                            &derived,
                                            profile,
                                        )
                                        .map_err(|e| format!("router binding: {e}"))?,
                                })
                            }
                        };
                        let ladder = self
                            .palw_context_ladder_at(point.daa_score)
                            .then(|| {
                                kaspa_consensus_core::palw_context_ladder::palw_class_ladder_rules_for_court_v1(
                                    profile,
                                    kary,
                                    bundle.court.max_step_leaf_count(),
                                )
                            })
                            .flatten();
                        let held = kaspa_consensus_core::palw_class_admission_v2::PalwHeldAdmissionV1 {
                            armed: self.palw_held_context_at(point.daa_score),
                            panel_da: self.palw_panel_da_at(point.daa_score),
                        };
                        let row = rb::PalwRouterBindingRowV2 {
                            version: rb::PALW_ROUTER_BINDING_VERSION_V2,
                            bound_from_daa: point.daa_score,
                            readers: declaration.readers.clone(),
                        };
                        kaspa_consensus_core::palw_class_admission_v2::palw_router_binding_admission_v2(
                            bundle, profile, ladder, kary, held, &row,
                        )
                        .map_err(|e| format!("class {} is not admissible under this router binding: {e}", declaration.class_id))?;
                    }
                    mr::PalwMemoryRoutingObjectV2::Accused { accusation } => {
                        let claim = state.claim(&accusation.claim).ok_or("memory/routing accusation names an absent native claim")?;
                        let ladder =
                            state.class_step_ladder_v1(&claim.class_id, self.palw_court_step_ladder_at(point.daa_score, court));
                        accusation.validate_shape(ladder)?;
                        if claim.bond != accusation.executor_bond
                            || claim.execution_root != accusation.execution_root
                            || claim.trace_root != accusation.trace_root
                        {
                            return Err("memory/routing accusation executor/roots are not the actual native claim's".into());
                        }
                        state.class(&claim.class_id).ok_or("memory/routing accusation class is absent")?;
                        let signer = state.bond(&accusation.accuser_bond).ok_or("memory/routing accuser bond is absent")?;
                        let message = mr::palw_memory_routing_session_id_v2(network, accusation);
                        if !Self::verify_mldsa87_with_context_bool(
                            &signer.pubkey,
                            message.as_byte_slice(),
                            &accusation.signature,
                            mr::PALW_MEMORY_ROUTING_MLDSA87_CONTEXT_V2,
                        ) {
                            return Err("memory/routing accusation is not signed by its registered accuser".into());
                        }
                    }
                }
            }
            Obj::ShardCourtAccused { accusation } => {
                let claim_id = accusation.claim;
                if !self.palw_shard_court_at(point.daa_score) {
                    return Err(format!("claim {claim_id}: the one-move court is not armed on this network (ADR-0099 Decision 5)"));
                }
                let court = self
                    .palw_court_params_v2
                    .as_ref()
                    .ok_or_else(|| "a one-move accusation on a network with no V2 court parameters".to_string())?;
                let ladder = self.palw_court_step_ladder_at(point.daa_score, court);
                let record = state
                    .bond(&accusation.accuser_bond)
                    .ok_or_else(|| format!("an accusation names bond {:?} this chain does not have", accusation.accuser_bond))?;
                let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                    self.network_id_bytes.as_slice(),
                    Some(self.genesis.hash),
                );
                let session_id =
                    kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_session_id_v1(domain.as_byte_slice(), accusation);
                if !Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    session_id.as_byte_slice(),
                    &accusation.signature,
                    kaspa_consensus_core::palw_shard_court_v1::PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT,
                ) {
                    return Err(format!("claim {claim_id}'s accusation is not signed by the bond it names"));
                }
                // ADR-0119 Decision 4: the shape is bounded at the CLAIM's ladder, so the claim
                // is found first — a held class's recorded ladder, every other the network's.
                let claim =
                    state.claim(&claim_id).ok_or_else(|| format!("an accusation names claim {claim_id} this chain does not have"))?;
                let ladder = state.class_step_ladder_v1(&claim.class_id, ladder);
                accusation.validate_shape(ladder).map_err(|e| format!("claim {claim_id}: {e}"))?;
                let bytes = kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_accusation_bytes_v1(accusation);
                if bytes > court.max_close_bytes() {
                    return Err(format!(
                        "claim {claim_id}: the accusation carries {bytes} bytes and this ruleset prices a close at {}",
                        court.max_close_bytes()
                    ));
                }
                if claim.bond != accusation.executor_bond
                    || claim.execution_root != accusation.execution_root
                    || claim.trace_root != accusation.trace_root
                {
                    return Err(format!("claim {claim_id}: the accusation's executor or roots are not the claim's"));
                }
                let class = state
                    .class(&claim.class_id)
                    .ok_or_else(|| format!("claim {claim_id} names class {} this chain does not have", claim.class_id))?;
                // t12 (`palw_audit_2026_09_23`): bound to the claim before a fused site is deferred —
                // the same function, at the same fence, the fold derives the verdict with.
                let bound_to = kaspa_consensus_core::palw_shard_court_v1::PalwOneMoveClaimV2 {
                    execution_root: claim.execution_root,
                    class_id: claim.class_id,
                    artifact_root: class.artifact_root,
                };
                match kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_verdict_at_v2(
                    accusation,
                    &bound_to,
                    ladder,
                    self.palw_audit_2026_09_23_at(point.daa_score),
                ) {
                    // ADR-0103 Decision 5: under the held regime the accusation at a fused leaf IS
                    // the challenge — the fold opens the dissection there. Dormant, refused.
                    Ok(kaspa_consensus_core::palw_shard_court_v1::PalwShardCourtVerdictV1::NeedsDissection)
                        if !self.palw_held_context_at(point.daa_score) =>
                    {
                        return Err(format!(
                            "claim {claim_id}: leaf {} is a fused-attention site, which one move cannot try, and this network \
                                 opens no held dissection at a named leaf",
                            accusation.leaf_index
                        ));
                    }
                    // ADR-0152 §4-ter (A-held): past `palw_offence_attribution` a held class no
                    // honest party can dissect inside a turn (the 2M row) opens no session — the
                    // fold refuses it (`ShardCourtHeldSiteUnanswerable`) off the same mirror.
                    Ok(kaspa_consensus_core::palw_shard_court_v1::PalwShardCourtVerdictV1::NeedsDissection)
                        if self.palw_offence_attribution_at(point.daa_score)
                            && state.held_claim_is_unanswerable_v2(state_params, claim) =>
                    {
                        return Err(format!(
                            "claim {claim_id}: leaf {} is a fused-attention site of held class {}, whose dissection no honest \
                                 party can play inside a turn: refused, not opened (ADR-0152 §4-ter)",
                            accusation.leaf_index, claim.class_id
                        ));
                    }
                    Ok(_) => {}
                    Err(e) => return Err(format!("claim {claim_id}: the accusation does not adjudicate: {e}")),
                }
            }
            // **ADR-0103 Decision 1: the checkpoint court.** The shard court's gates in its order —
            // the fence, the accuser's key over the session id, the ceiling, the claim — and the
            // verdict DERIVED here at the ladder the fold derives it at.
            Obj::CheckpointAccused { accusation } => {
                let claim_id = accusation.claim;
                if !self.palw_held_context_at(point.daa_score) {
                    return Err(format!("claim {claim_id}: the held regime is not armed on this network (ADR-0103)"));
                }
                let court = self
                    .palw_court_params_v2
                    .as_ref()
                    .ok_or_else(|| "a checkpoint accusation on a network with no V2 court parameters".to_string())?;
                let ladder = self.palw_court_step_ladder_at(point.daa_score, court);
                let record = state
                    .bond(&accusation.accuser_bond)
                    .ok_or_else(|| format!("an accusation names bond {:?} this chain does not have", accusation.accuser_bond))?;
                let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                    self.network_id_bytes.as_slice(),
                    Some(self.genesis.hash),
                );
                let session_id = kaspa_consensus_core::palw_checkpoint_court_v1::palw_checkpoint_court_session_id_v1(
                    domain.as_byte_slice(),
                    accusation,
                );
                if !Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    session_id.as_byte_slice(),
                    &accusation.signature,
                    kaspa_consensus_core::palw_checkpoint_court_v1::PALW_CHECKPOINT_COURT_MLDSA87_ACCUSE_CONTEXT,
                ) {
                    return Err(format!("claim {claim_id}'s checkpoint accusation is not signed by the bond it names"));
                }
                let bytes = kaspa_consensus_core::palw_checkpoint_court_v1::palw_checkpoint_court_accusation_bytes_v1(accusation);
                if bytes > court.max_close_bytes() {
                    return Err(format!(
                        "claim {claim_id}: the checkpoint accusation carries {bytes} bytes and this ruleset prices a close at {}",
                        court.max_close_bytes()
                    ));
                }
                let claim =
                    state.claim(&claim_id).ok_or_else(|| format!("an accusation names claim {claim_id} this chain does not have"))?;
                if claim.bond != accusation.executor_bond
                    || claim.execution_root != accusation.execution_root
                    || claim.trace_root != accusation.trace_root
                {
                    return Err(format!("claim {claim_id}: the accusation's executor or roots are not the claim's"));
                }
                kaspa_consensus_core::palw_checkpoint_court_v1::palw_checkpoint_court_verdict_v1(
                    accusation,
                    claim.class_id,
                    // ADR-0119 Decision 4: the CLAIM's ladder.
                    state.class_step_ladder_v1(&claim.class_id, ladder),
                )
                .map_err(|e| format!("claim {claim_id}: the checkpoint accusation does not adjudicate: {e}"))?;
            }
            // **ADR-0103 Decision 4: the held DA court's two moves.** The fence, the DA court's
            // own fence, the signer (the accuser's key; the claim's producer for an answer), the
            // ceiling, and the unit bounded — or answered — against the claim's own roots.
            // **The second IR fence: an IR step demand** (evidence transport C), keyed by the claim
            // alone. The fence, the DA court, a step unit inside the widest execution, the ruleset's
            // close ceiling and the accuser's registered key over the demand; the claim and its
            // class are the fold's (`open_da_session_tir_step_v1`), and a unit past the claim's own
            // execution is the accused's to prove (`TirStepOutOfRange`).
            Obj::DefaultAccusedTirStep { accusation } => {
                let claim = accusation.claim;
                if !self.palw_tir_fence2_at(point.daa_score) {
                    return Err(format!("claim {claim}: an IR step demand is refused: palw_tir_fence2 is not in force (RFC-0002)"));
                }
                if !self.palw_da_court_at(point.daa_score) {
                    return Err(format!("claim {claim}: the data-availability court is not armed on this network (ADR-0062)"));
                }
                let court = self
                    .palw_court_params_v2
                    .as_ref()
                    .ok_or_else(|| "an IR step demand on a network with no V2 court parameters".to_string())?;
                // The widest execution at the court's step ladder where the held regime rides it,
                // the release's 2^22 otherwise; the fold holds the unit to the claim's class's.
                let ladder = if self.palw_held_context_at(point.daa_score) {
                    self.palw_court_step_ladder_at(point.daa_score, court)
                        .max(kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES)
                } else {
                    kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES
                };
                kaspa_consensus_core::palw_da_rcore_v1::palw_tir_step_unit_is_admissible_v1(&accusation.unit, ladder)
                    .map_err(|why| format!("claim {claim}: an IR step demand for {:?} is refused: {why}", accusation.unit))?;
                let bytes = borsh::to_vec(accusation.as_ref()).map(|b| b.len() as u64).unwrap_or(u64::MAX);
                if bytes > court.max_close_bytes() {
                    return Err(format!(
                        "claim {claim}'s IR step demand is {bytes} bytes, above this ruleset's {}-byte close ceiling",
                        court.max_close_bytes()
                    ));
                }
                let record = state
                    .bond(&accusation.accuser)
                    .ok_or_else(|| format!("an IR step demand names bond {:?} this chain does not have", accusation.accuser))?;
                let message = kaspa_consensus_core::palw_da_rcore_v1::palw_tir_step_accusation_message_v1(
                    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    ),
                    &claim,
                    &accusation.unit,
                    &accusation.accuser,
                );
                if !Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    message.as_byte_slice(),
                    &accusation.signature,
                    kaspa_consensus_core::palw_da_rcore_v1::PALW_TIR_STEP_ACCUSATION_MLDSA87_CONTEXT_V1,
                ) {
                    return Err(format!("claim {claim}'s IR step demand is not signed by the bond it names"));
                }
            }
            // **Pipeline-claim data availability: a pipeline step demand** (spec 17 §17.14, object tag 83),
            // keyed by the claim alone. The fence (`palw_improvement_v1`), the DA court, a unit inside the
            // widest execution, the ruleset's close ceiling and the accuser's registered key over the
            // demand; the claim and its kind are the fold's (`open_da_session_pipeline_step_v1`), and a unit
            // past the claim's own stage is the accused's to prove (`PipelineStepOutOfRange`).
            Obj::DefaultAccusedPipelineStep { accusation } => {
                let claim = accusation.claim;
                if !self.palw_pipeline_da_at(point.daa_score) {
                    return Err(format!(
                        "claim {claim}: a pipeline step demand is refused: palw_improvement_v1 is not in force (RFC-0004 Phase F)"
                    ));
                }
                if !self.palw_da_court_at(point.daa_score) {
                    return Err(format!("claim {claim}: the data-availability court is not armed on this network (ADR-0062)"));
                }
                let court = self
                    .palw_court_params_v2
                    .as_ref()
                    .ok_or_else(|| "a pipeline step demand on a network with no V2 court parameters".to_string())?;
                kaspa_consensus_core::palw_pipeline_da_v1::palw_pipeline_step_unit_is_admissible_v1(&accusation.unit)
                    .map_err(|why| format!("claim {claim}: a pipeline step demand for {:?} is refused: {why}", accusation.unit))?;
                let bytes = borsh::to_vec(accusation.as_ref()).map(|b| b.len() as u64).unwrap_or(u64::MAX);
                if bytes > court.max_close_bytes() {
                    return Err(format!(
                        "claim {claim}'s pipeline step demand is {bytes} bytes, above this ruleset's {}-byte close ceiling",
                        court.max_close_bytes()
                    ));
                }
                let record = state
                    .bond(&accusation.accuser)
                    .ok_or_else(|| format!("a pipeline step demand names bond {:?} this chain does not have", accusation.accuser))?;
                let message = kaspa_consensus_core::palw_pipeline_da_v1::palw_pipeline_step_accusation_message_v1(
                    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    ),
                    &claim,
                    &accusation.unit,
                    &accusation.accuser,
                );
                if !Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    message.as_byte_slice(),
                    &accusation.signature,
                    kaspa_consensus_core::palw_pipeline_da_v1::PALW_PIPELINE_ACCUSATION_MLDSA87_CONTEXT_V1,
                ) {
                    return Err(format!("claim {claim}'s pipeline step demand is not signed by the bond it names"));
                }
            }
            Obj::DefaultAccusedHeld { accusation } => {
                let claim_id = accusation.claim;
                if !self.palw_held_context_at(point.daa_score) || !self.palw_da_court_at(point.daa_score) {
                    return Err(format!(
                        "claim {claim_id}: a held accusation needs the held regime and the data-availability court (ADR-0103)"
                    ));
                }
                let court = self.palw_court_params_v2.as_ref().ok_or("no court params")?;
                let bytes = kaspa_consensus_core::palw_held_da_v1::palw_held_da_bytes_v1(accusation.as_ref());
                if bytes > court.max_close_bytes() {
                    return Err(format!("claim {claim_id}: the held accusation carries {bytes} bytes, above the close ceiling"));
                }
                let record = state
                    .bond(&accusation.accuser)
                    .ok_or_else(|| format!("an accusation names bond {:?} this chain does not have", accusation.accuser))?;
                let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                    self.network_id_bytes.as_slice(),
                    Some(self.genesis.hash),
                );
                let message =
                    kaspa_consensus_core::palw_held_da_v1::palw_held_da_accusation_message_v1(domain.as_byte_slice(), accusation);
                if !Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    message.as_byte_slice(),
                    &accusation.signature,
                    kaspa_consensus_core::palw_held_da_v1::PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT,
                ) {
                    return Err(format!("claim {claim_id}'s held accusation is not signed by the bond it names"));
                }
                let claim =
                    state.claim(&claim_id).ok_or_else(|| format!("an accusation names claim {claim_id} this chain does not have"))?;
                kaspa_consensus_core::palw_held_da_v1::palw_held_da_check_accusation_v1(
                    &claim.execution_root,
                    &accusation.missing,
                    &accusation.binding,
                    self.palw_prompt_ids_form_at(point.daa_score),
                )
                .map_err(|e| format!("claim {claim_id}: {e}"))?;
                // **ADR-0111 Decision 3: a leaf is demanded only where the chain assigned it.**
                // The demanding seat's own draw — keyed by the network domain, which is held here
                // and not in the fold — must have put the leaf's interval in that seat's sample,
                // so no seat can pick the leaf an executor must put on chain.
                //
                // **ADR-0152 DA-3 (M3): not past `palw_rcore_plus`**, where a named `StepLeaf` is
                // free — any leaf inside the binding's bound, from a seat or not — and the per-seat
                // session budget (DA-8) replaces the once-per-seat rule; drawn units, seeded by the
                // accepting block, are what the accuser cannot choose.
                if let kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1::StepLeaf { leaf } = accusation.missing
                    && !state_params.rcore_plus_active_at(point.daa_score)
                {
                    let panel = state.panel(&claim_id).ok_or_else(|| format!("claim {claim_id} has no bound panel to demand from"))?;
                    let seat_index = panel
                        .seats
                        .iter()
                        .position(|seat| seat.bond == accusation.accuser)
                        .ok_or_else(|| format!("claim {claim_id}: a leaf's evidence is demanded by a seat of its panel"))?;
                    kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_demand_is_the_seats_v1(
                        &domain,
                        &panel.anchor,
                        &claim_id,
                        seat_index as u8,
                        &accusation.binding,
                        leaf,
                    )
                    .map_err(|e| format!("claim {claim_id}: {e}"))?;
                }
            }
            Obj::MaterialDisclosedHeld { disclosure } => {
                let claim_id = disclosure.claim;
                if state_params.rcore_plus_active_at(point.daa_score) {
                    return Err(format!(
                        "claim {claim_id}: MaterialDisclosedHeld is retired past palw_rcore_plus; MaterialDisclosedV2 answers (DA-1)"
                    ));
                }
                if !self.palw_held_context_at(point.daa_score) || !self.palw_da_court_at(point.daa_score) {
                    return Err(format!(
                        "claim {claim_id}: a held disclosure needs the held regime and the data-availability court (ADR-0103)"
                    ));
                }
                let court = self.palw_court_params_v2.as_ref().ok_or("no court params")?;
                let ladder = self.palw_court_step_ladder_at(point.daa_score, court);
                let bytes = kaspa_consensus_core::palw_held_da_v1::palw_held_da_bytes_v1(disclosure.as_ref());
                if bytes > court.max_close_bytes() {
                    return Err(format!("claim {claim_id}: the held disclosure carries {bytes} bytes, above the close ceiling"));
                }
                let claim =
                    state.claim(&claim_id).ok_or_else(|| format!("a disclosure names claim {claim_id} this chain does not have"))?;
                let producer = state
                    .bond(&claim.bond)
                    .ok_or_else(|| format!("claim {claim_id}'s producing bond is not in this chain's registry"))?;
                let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                    self.network_id_bytes.as_slice(),
                    Some(self.genesis.hash),
                );
                let message =
                    kaspa_consensus_core::palw_held_da_v1::palw_held_da_disclosure_message_v1(domain.as_byte_slice(), disclosure);
                if !Self::verify_mldsa87_with_context_bool(
                    &producer.pubkey,
                    message.as_byte_slice(),
                    &disclosure.signature,
                    kaspa_consensus_core::palw_held_da_v1::PALW_HELD_DA_MLDSA87_DISCLOSE_CONTEXT,
                ) {
                    return Err(format!("claim {claim_id}'s held disclosure is not signed by the bond that produced it"));
                }
                kaspa_consensus_core::palw_held_da_v1::palw_held_da_check_disclosure_v1(
                    &claim.execution_root,
                    &disclosure.missing,
                    &disclosure.binding,
                    &disclosure.disclosure,
                    // ADR-0119 Decision 4: the answer opens at the CLAIM's ladder, as the fold's.
                    state.class_step_ladder_v1(&claim.class_id, ladder),
                    self.palw_prompt_ids_form_at(point.daa_score),
                )
                .map_err(|e| format!("claim {claim_id}: {e}"))?;
            }
            // **RFC-0006: layer-sharded panels' three objects** (tags 91-93). The fence first, the doubled-fence doctrine of
            // every court move; then each object's authority: the class's registrant over a plan, the seats' own cell-masked
            // receipts over a part (checked exactly as a whole licence checks theirs, over that shard's slice, by
            // `validate_tir_shard_part_v1`), the bond over a shard possession proof.
            Obj::TirShardPlanDeclared { class_id, s_l, s_p, signature } => {
                if !self.palw_tir_shard_at(point.daa_score) {
                    return Err(format!("class {class_id}: layer-sharded panels are not armed on this network (RFC-0006)"));
                }
                let class = state
                    .class(class_id)
                    .ok_or_else(|| format!("a layer-shard plan names class {class_id} this chain does not have"))?;
                let registrant = class
                    .registrant_bond
                    .ok_or_else(|| format!("class {class_id} was registered at genesis and has no registrant"))?;
                let record = state.bond(&registrant).ok_or_else(|| format!("class {class_id}'s registrant bond is gone"))?;
                let message = kaspa_consensus_core::palw_tir_shard_v1::palw_tir_shard_plan_message_v1(
                    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    ),
                    class_id,
                    *s_l,
                    *s_p,
                );
                if !Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    message.as_byte_slice(),
                    signature,
                    kaspa_consensus_core::palw_tir_shard_v1::PALW_TIR_SHARD_PLAN_MLDSA87_CONTEXT,
                ) {
                    return Err(format!("class {class_id}'s layer-shard plan is not signed by its registrant"));
                }
            }
            // **Lane LG14-A (tags 154–155): the dispute reservation and its release** — the fence, an Active signer, its signature
            // over the payload; the claim, the caps, the deposit and the hold are the fold's.
            Obj::DisputeReservedV1 { reservation, signature } => {
                let payload = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_dispute_reserved_payload_v1(reservation);
                self.palw_legacy_dispute_signature_ok(
                    state,
                    point.daa_score,
                    kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_DISPUTE_RESERVED_TAG_V1,
                    &reservation.reserver,
                    &payload,
                    signature,
                )?;
            }
            Obj::DisputeReacquiredV1 { request, signature } => {
                let payload = borsh::to_vec(request.as_ref()).map_err(|e| e.to_string())?;
                let court = self.palw_court_params_v2.as_ref().ok_or("public reacquisition needs V2 court parameters")?;
                if payload.len() as u64 > court.max_close_bytes() {
                    return Err("public reacquisition exceeds the ruleset's close carriage ceiling".into());
                }
                if let kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::LegacyHeldV2(unit) = request.unit
                    && matches!(
                        unit,
                        kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2::PromptIds { .. }
                            | kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2::FpJob
                    )
                {
                    let binding = request.binding.as_ref().ok_or("input/job reacquisition needs its binding")?;
                    let params = self.palw_state_params_v2.as_ref().ok_or("input/job reacquisition needs V2 state parameters")?;
                    kaspa_consensus_core::palw_legacy_answer_chunks_v2::palw_legacy_input_answer_transport_check_v2(
                        binding,
                        &unit,
                        self.palw_prompt_ids_form_at(point.daa_score),
                        court.max_close_bytes(),
                        kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(params),
                    )?;
                }
                self.palw_legacy_dispute_signature_ok(
                    state,
                    point.daa_score,
                    kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_DISPUTE_REACQUIRE_TAG_V1,
                    &request.reserver,
                    &payload,
                    signature,
                )?;
            }
            Obj::DisputeReleasedV1 { claim, reserver, signature } => {
                let payload = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_dispute_released_payload_v1(claim);
                self.palw_legacy_dispute_signature_ok(
                    state,
                    point.daa_score,
                    kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_DISPUTE_RELEASED_TAG_V1,
                    reserver,
                    &payload,
                    signature,
                )?;
            }
            // **Lane LG14-B (tags 157–159): the legacy held DA** — the fence, the named bond's signature over the object's message,
            // the close ceiling on what it carries; the units, answers and verdicts are the fold's.
            Obj::LegacyHeldDemandedV2 { demand } => {
                use kaspa_consensus_core::palw_legacy_held_da_v2 as l;
                let domain = self.palw_network_domain_v2();
                let message = l::palw_legacy_held_demand_message_v2(domain.as_byte_slice(), demand);
                let bytes = borsh::to_vec(&demand.binding).map(|b| b.len() as u64).unwrap_or(u64::MAX);
                if matches!(demand.unit, l::PalwLegacyHeldUnitV2::PromptIds { .. } | l::PalwLegacyHeldUnitV2::FpJob) {
                    let court = self.palw_court_params_v2.as_ref().ok_or("input/job disclosure needs V2 court parameters")?;
                    let params = self.palw_state_params_v2.as_ref().ok_or("input/job disclosure needs V2 state parameters")?;
                    kaspa_consensus_core::palw_legacy_answer_chunks_v2::palw_legacy_input_answer_transport_check_v2(
                        &demand.binding,
                        &demand.unit,
                        self.palw_prompt_ids_form_at(point.daa_score),
                        court.max_close_bytes(),
                        kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(params),
                    )?;
                }
                self.palw_legacy_held_signature_ok(
                    state,
                    point.daa_score,
                    &demand.claim,
                    &demand.accuser,
                    &message,
                    &demand.signature,
                    l::PALW_LEGACY_HELD_DEMAND_MLDSA87_CONTEXT_V2,
                    bytes,
                )?;
            }
            Obj::LegacyHeldAnsweredV2 { answer } => {
                use kaspa_consensus_core::palw_legacy_held_da_v2 as l;
                let domain = self.palw_network_domain_v2();
                let message = l::palw_legacy_held_answer_message_v2(domain.as_byte_slice(), answer);
                let mut bytes = l::palw_legacy_held_answer_bytes_v2(&answer.answer);
                if matches!(answer.unit, l::PalwLegacyHeldUnitV2::PromptIds { .. } | l::PalwLegacyHeldUnitV2::FpJob) {
                    bytes = bytes.saturating_add(borsh::to_vec(&answer.binding).map_err(|e| e.to_string())?.len() as u64);
                    if bytes > kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES as u64 {
                        return Err("the input answer cannot fit a single carrier".into());
                    }
                }
                self.palw_legacy_held_signature_ok(
                    state,
                    point.daa_score,
                    &answer.claim,
                    &answer.discloser,
                    &message,
                    &answer.signature,
                    l::PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2,
                    bytes,
                )?;
            }
            Obj::PublicCourtStreamOpenedV1 { seed } => {
                use kaspa_consensus_core::palw_constraint_stream_carriage_v1 as stage;
                // A-2: below its owning fence (`palw_staged_public_court_v1`) the kind rides unjudged, as int-12 reads it.
                if !self.palw_staged_public_court_v1.is_some_and(|activation| activation.is_active(point.daa_score)) {
                    return Ok(());
                }
                // A-2: a route's appended form below the route's own fence rides unjudged too.
                if !self.palw_lifecycle_kind_fences.kind_in_force_at(object, point.daa_score) {
                    return Ok(());
                }
                let court = self.palw_court_params_v2.as_ref().ok_or("staged court needs V2 carrier parameters")?;
                seed.check(court.max_close_bytes())?;
                let owner = state.bond(&seed.reserver).ok_or("staged owner has no registered bond")?;
                let bundle = match state.staged_owned_row_v1(seed.claim, seed.reserver) {
                    Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtRowV1::OwnerBundle(bundle)) => Some(bundle),
                    _ => None,
                };
                let authorized = match bundle {
                    Some(bundle) => {
                        bundle.reserved_daa == seed.reserved_daa
                            && bundle.opened.len() < bundle.plan.stages.len()
                            && bundle.opened.len() == bundle.active.map_or(0, |n| n as usize + 1)
                            && point.daa_score >= bundle.plan.terms.accepted_daa
                            && point.daa_score < bundle.plan.stages[bundle.opened.len()].terms.open_until_daa
                    }
                    None => state
                        .legacy_dispute_reservation_v1(&seed.claim, &seed.reserver)
                        .is_some_and(|reservation| reservation.reserved_daa == seed.reserved_daa),
                };
                if !authorized || !matches!(owner.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                    return Err("staged move changes live reservation identity or owner is inactive".into());
                }
                if !Self::verify_mldsa87_with_context_bool(
                    &owner.pubkey,
                    &seed.signing_message(),
                    &seed.signature,
                    stage::PALW_CONSTRAINT_STREAM_CONTEXT_V1,
                ) {
                    return Err("staged move is not signed by its literal reserver".into());
                }
            }
            Obj::TirPublicControlV1 { control } => {
                if !self.palw_lifecycle_kind_fences.kind_in_force_at(object, point.daa_score) {
                    return Ok(());
                }
                // Admission and paid allocation checks remain the fold's.
                // A valid signature alone grants no original or owner standing.
                control.validate_native_signature_v1(self.palw_network_domain_v2(), state)?;
            }
            // G14 lane r5-f02-wire (tag 178, RFC-0029): below `palw_tir_public_conviction_v1` the dormant payload (A-2), riding
            // unjudged; past it the outsider lane must be in force too (the fold's `shared_court_lifecycle_active_v1`), and a
            // request's literal signer must be a CURRENT Active bond at the floor whose ML-DSA-87 signature verifies over the
            // network-bound intent. Content authentication (bodies, IDs, numerical controls) and the notice window are the fold's.
            Obj::SharedCourtOpV1 { op } => {
                if !self.palw_lifecycle_kind_fences.kind_in_force_at(object, point.daa_score) {
                    return Ok(());
                }
                let native_default = matches!(
                    op.as_ref(),
                    kaspa_consensus_core::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1::NativeHeldDefault { .. }
                );
                if native_default && !state_params.native_liability_horizon_active_at(point.daa_score) {
                    return Err("the post-Final held default is dormant on this network (palw_native_liability_horizon_v1)".into());
                }
                if !native_default && !state_params.court_outsider_lane_active_v1(point.daa_score) {
                    return Err("the shared court lifecycle is dormant on this network (palw_court_outsider_lane_v1)".into());
                }
                op.validate_authority_v1(self.palw_network_domain_v2(), state, state_params.min_collateral_sompi())?;
            }
            Obj::TirPublicGrantReadyV1 { grant } => {
                if !self.palw_lifecycle_kind_fences.kind_in_force_at(object, point.daa_score) {
                    return Ok(());
                }
                // The fold additionally authenticates the exact Valid receipt,
                // raw grant, native S4 lock and prepared descriptor atomically.
                grant.validate_native_signature_v1(self.palw_network_domain_v2(), state)?;
            }
            Obj::PublicCourtStreamOpenedNativeV1 { seed } => {
                use kaspa_consensus_core::palw_constraint_stream_carriage_v1 as stage;
                // A-2: below its owning fence (`palw_staged_tir_route_v1`, in force only with the staged court) the kind
                // rides unjudged, as every build without the route reads it.
                if !self.palw_lifecycle_kind_fences.kind_in_force_at(object, point.daa_score)
                    || !self.palw_staged_public_court_v1.is_some_and(|activation| activation.is_active(point.daa_score))
                {
                    return Ok(());
                }
                let court = self.palw_court_params_v2.as_ref().ok_or("staged court needs V2 carrier parameters")?;
                seed.check(court.max_close_bytes())?;
                let owner = state.bond(&seed.reserver).ok_or("staged owner has no registered bond")?;
                // A native seed is always a stage of a prepaid required-family bundle.
                let authorized = match state.staged_owned_row_v1(seed.claim, seed.reserver) {
                    Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtRowV1::OwnerBundle(bundle)) => {
                        bundle.reserved_daa == seed.reserved_daa
                            && bundle.opened.len() < bundle.plan.stages.len()
                            && bundle.opened.len() == bundle.active.map_or(0, |n| n as usize + 1)
                            && point.daa_score >= bundle.plan.terms.accepted_daa
                            && point.daa_score < bundle.plan.stages[bundle.opened.len()].terms.open_until_daa
                    }
                    _ => false,
                };
                if !authorized || !matches!(owner.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                    return Err("native staged seed changes its paid bundle frame or owner is inactive".into());
                }
                if !Self::verify_mldsa87_with_context_bool(
                    &owner.pubkey,
                    &seed.signing_message(),
                    &seed.signature,
                    stage::PALW_CONSTRAINT_STREAM_CONTEXT_V1,
                ) {
                    return Err("native staged seed is not signed by its literal reserver".into());
                }
            }
            Obj::PublicCourtStreamLeafV1 { leaf } => {
                use kaspa_consensus_core::palw_constraint_stream_carriage_v1 as stage;
                // A-2: below its owning fence (`palw_staged_public_court_v1`) the kind rides unjudged, as int-12 reads it.
                if !self.palw_staged_public_court_v1.is_some_and(|activation| activation.is_active(point.daa_score)) {
                    return Ok(());
                }
                let _court = self.palw_court_params_v2.as_ref().ok_or("staged court needs V2 carrier parameters")?;
                leaf.check()?;
                let owner = state.bond(&leaf.reserver).ok_or("staged owner has no registered bond")?;
                let paid = kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_paid_authority_v1(
                    &state,
                    (leaf.claim, leaf.reserver),
                )?;
                let authorized = match paid {
                    Some(paid) => {
                        paid.reserved_daa == leaf.reserved_daa
                            && paid.seed_id == leaf.seed_id
                            && point.daa_score >= paid.terms.accepted_daa
                            && point.daa_score < paid.terms.stream_until_daa
                            && point.daa_score < paid.terms.material_until_daa
                    }
                    // A seed earlier in the same block is checked by the ordered fold.
                    None => state
                        .legacy_dispute_reservation_v1(&leaf.claim, &leaf.reserver)
                        .is_some_and(|r| r.reserved_daa == leaf.reserved_daa),
                };
                if !authorized || !matches!(owner.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                    return Err("staged move changes live reservation identity or owner is inactive".into());
                }
                if !Self::verify_mldsa87_with_context_bool(
                    &owner.pubkey,
                    &leaf.signing_message(),
                    &leaf.signature,
                    stage::PALW_CONSTRAINT_STREAM_CONTEXT_V1,
                ) {
                    return Err("staged move is not signed by its literal reserver".into());
                }
            }
            Obj::PublicCourtStreamMoveV1 { movement } => {
                use kaspa_consensus_core::palw_constraint_stream_carriage_v1 as stage;
                // A-2: below its owning fence (`palw_staged_public_court_v1`) the kind rides unjudged, as int-12 reads it.
                if !self.palw_staged_public_court_v1.is_some_and(|activation| activation.is_active(point.daa_score)) {
                    return Ok(());
                }
                // A-2: a route's appended form below the route's own fence rides unjudged too.
                if !self.palw_lifecycle_kind_fences.kind_in_force_at(object, point.daa_score) {
                    return Ok(());
                }
                let _court = self.palw_court_params_v2.as_ref().ok_or("staged court needs V2 carrier parameters")?;
                movement.check()?;
                let owner = state.bond(&movement.reserver).ok_or("staged owner has no registered bond")?;
                let paid = kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_paid_authority_v1(
                    &state,
                    (movement.claim, movement.reserver),
                )?;
                let artifact = match &movement.action {
                    stage::PalwConstraintStreamActionV1::HistoryCustodyGrant { grant, receipt } => {
                        let contract = match state.staged_owned_row_v1(movement.claim, movement.reserver) {
                            _ => state
                                .staged_claim_contract_v1(&movement.claim)
                                .ok_or("history grant has no accepted material contract")?,
                        };
                        let signer = state.bond(&grant.seat).ok_or("history grant signer is not registered")?;
                        grant.validate_signature_v1(receipt, &signer.pubkey, Self::verify_mldsa87_with_context_bool)?;
                        Some(
                            movement.reserver == grant.seat
                                && movement.claim == grant.claim
                                && movement.reserved_daa == contract.terms.accepted_daa
                                && point.daa_score >= contract.terms.accepted_daa
                                && point.daa_score < contract.terms.material_until_daa
                                && contract
                                    .prefix_custody
                                    .as_ref()
                                    .is_some_and(|custody| custody.matches_credited_receipt_v1(movement.claim, receipt)),
                        )
                    }
                    stage::PalwConstraintStreamActionV1::ArtifactInventoryOpen { offer, .. } => {
                        offer.validate_signature_v1(&owner.pubkey, Self::verify_mldsa87_with_context_bool)?;
                        Some(
                            movement.claim == offer.artifact_hash
                                && movement.reserver == offer.opener
                                && movement.seed_id == offer.id_v1()
                                && movement.reserved_daa == 0
                                && point.daa_score < offer.accept_until_daa,
                        )
                    }
                    stage::PalwConstraintStreamActionV1::ArtifactInventoryMove(_) => {
                        Some(state.staged_tokenizer_inventory_open_v1(movement.claim, movement.reserver).is_some_and(|row| {
                            movement.seed_id == row.offer.id_v1()
                                && movement.reserved_daa == row.opened_daa
                                && point.daa_score >= row.opened_daa
                                && point.daa_score < row.until_daa
                        }))
                    }
                    _ => None,
                };
                let authorized = if let Some(artifact) = artifact {
                    artifact
                } else {
                    match paid {
                        Some(paid) => {
                            let control_terms = match (&movement.action, state.staged_owned_row_v1(movement.claim, movement.reserver))
                            {
                                (
                                    stage::PalwConstraintStreamActionV1::ActivateStage { .. }
                                    | stage::PalwConstraintStreamActionV1::InventoryPart { .. }
                                    | stage::PalwConstraintStreamActionV1::InventoryFinish
                                    | stage::PalwConstraintStreamActionV1::SelectedEventDemand { .. }
                                    | stage::PalwConstraintStreamActionV1::SelectedEventAnswer(_)
                                    | stage::PalwConstraintStreamActionV1::SelectedEventCollect { .. }
                                    | stage::PalwConstraintStreamActionV1::NativeSelectedDemand { .. }
                                    | stage::PalwConstraintStreamActionV1::NativeSelectedAnswer(_)
                                    | stage::PalwConstraintStreamActionV1::FlatSourceOpen { .. }
                                    | stage::PalwConstraintStreamActionV1::FlatSourceAnswer(_)
                                    | stage::PalwConstraintStreamActionV1::FlatSourceNominate { .. }
                                    | stage::PalwConstraintStreamActionV1::FlatSourceCollect
                                    | stage::PalwConstraintStreamActionV1::FloatSourceOpen { .. }
                                    | stage::PalwConstraintStreamActionV1::FloatSourceAnswer(_)
                                    | stage::PalwConstraintStreamActionV1::FloatSourceNominate { .. }
                                    | stage::PalwConstraintStreamActionV1::FloatSourceCollect
                                    | stage::PalwConstraintStreamActionV1::HistoryRecovery { .. },
                                    Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtRowV1::OwnerBundle(bundle)),
                                ) => bundle.plan.terms,
                                _ => paid.terms,
                            };
                            let until = if matches!(
                                movement.action,
                                stage::PalwConstraintStreamActionV1::PrefixPublicCollect { .. }
                                    | stage::PalwConstraintStreamActionV1::FlatSourceCollect
                                    | stage::PalwConstraintStreamActionV1::FloatSourceCollect
                                    | stage::PalwConstraintStreamActionV1::NativeTirPrefixCollect
                                    | stage::PalwConstraintStreamActionV1::SelectedEventCollect { .. }
                                    | stage::PalwConstraintStreamActionV1::HistoryRecovery {
                                        action: kaspa_consensus_core::palw_history_custody_v1::PalwHistoryRecoveryActionV1::Collect,
                                        ..
                                    }
                            ) {
                                control_terms.collect_until_daa
                            } else {
                                control_terms.stream_until_daa
                            };
                            let literal_seed = match (&movement.action, state.staged_owned_row_v1(movement.claim, movement.reserver)) {
                                (
                                    stage::PalwConstraintStreamActionV1::SelectedEventDemand { .. }
                                    | stage::PalwConstraintStreamActionV1::SelectedEventAnswer(_)
                                    | stage::PalwConstraintStreamActionV1::SelectedEventCollect { .. }
                                    | stage::PalwConstraintStreamActionV1::NativeSelectedDemand { .. }
                                    | stage::PalwConstraintStreamActionV1::NativeSelectedAnswer(_),
                                    Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtRowV1::OwnerBundle(bundle)),
                                ) => bundle.plan.id_v1(bundle.claim),
                                (
                                    stage::PalwConstraintStreamActionV1::FlatSourceOpen { .. }
                                    | stage::PalwConstraintStreamActionV1::FlatSourceAnswer(_)
                                    | stage::PalwConstraintStreamActionV1::FlatSourceNominate { .. }
                                    | stage::PalwConstraintStreamActionV1::FlatSourceCollect
                                    | stage::PalwConstraintStreamActionV1::FloatSourceOpen { .. }
                                    | stage::PalwConstraintStreamActionV1::FloatSourceAnswer(_)
                                    | stage::PalwConstraintStreamActionV1::FloatSourceNominate { .. }
                                    | stage::PalwConstraintStreamActionV1::FloatSourceCollect,
                                    Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtRowV1::OwnerBundle(bundle)),
                                ) => bundle.plan.id_v1(bundle.claim),
                                (
                                    stage::PalwConstraintStreamActionV1::HistoryRecovery { .. },
                                    Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtRowV1::OwnerBundle(bundle)),
                                ) => bundle.plan.id_v1(bundle.claim),
                                _ => paid.seed_id,
                            };
                            paid.reserved_daa == movement.reserved_daa
                                && literal_seed == movement.seed_id
                                && point.daa_score >= paid.terms.accepted_daa
                                && point.daa_score < until
                                && point.daa_score < paid.terms.material_until_daa
                        }
                        None => state
                            .legacy_dispute_reservation_v1(&movement.claim, &movement.reserver)
                            .is_some_and(|r| r.reserved_daa == movement.reserved_daa),
                    }
                };
                if !authorized || !matches!(owner.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                    return Err("staged move changes live reservation identity or owner is inactive".into());
                }
                // Only a typed public-prefix Answer has distinct signature authority.
                // The fold additionally checks its saved literal custody/owner slot.
                let authority = movement.authorization_bond_v1();
                let signer = state.bond(&authority).ok_or("staged answerer has no registered bond")?;
                let verify = |key: &[u8]| {
                    Self::verify_mldsa87_with_context_bool(
                        key,
                        &movement.signing_message(),
                        &movement.signature,
                        stage::PALW_CONSTRAINT_STREAM_CONTEXT_V1,
                    )
                };
                // An external producer's accepted node custodian may sign the producer's literal
                // answer slot (never an owner move: the authority must be the claim's producer
                // in a typed answer). The slot, its deadlines and its liability stay the producer's.
                let delegated = || {
                    state.staged_bundle_contract_v1(movement.claim).is_some_and(|contract| {
                        authority == contract.claim.producer
                            && authority != movement.reserver
                            && contract.producer_custodian.and_then(|custodian| state.bond(&custodian)).is_some_and(|custodian| {
                                matches!(custodian.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active)
                                    && verify(&custodian.pubkey)
                            })
                    })
                };
                if !verify(&signer.pubkey) && !delegated() {
                    return Err("staged move is not signed by its literal authorized owner/answerer".into());
                }
            }
            Obj::ObjectiveEvidenceDeclaredV1 { declaration } => {
                use kaspa_consensus_core::palw_terminal_carriage_v1 as t;
                // A-2: below its owning fence (`palw_legacy_public_filer_v1`) the kind rides unjudged, as int-12 reads it; past it, the
                // objective-offence and attribution fences are this kind's own rule.
                if !self.palw_legacy_public_filer_at(point.daa_score) {
                    return Ok(());
                }
                if !self.palw_objective_offence_at(point.daa_score) || !self.palw_offence_attribution_at(point.daa_score) {
                    return Err("terminal evidence is below its objective-offence/attribution fence".into());
                }
                let court = self.palw_court_params_v2.as_ref().ok_or("terminal evidence needs V2 court parameters")?;
                declaration.check(court.max_close_bytes())?;
                let owner = state.bond(&declaration.reserver).ok_or("terminal reserver has no registered bond")?;
                let reservation = state
                    .legacy_dispute_reservation_v1(&declaration.claim, &declaration.reserver)
                    .ok_or("terminal reserver has no live reservation")?;
                if reservation.reserved_daa != declaration.reserved_daa {
                    return Err("terminal declaration belongs to another reservation".into());
                }
                if !matches!(owner.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                    return Err("terminal reserver is not active".into());
                }
                let message = t::palw_terminal_message_v1(self.palw_network_domain_v2(), declaration)?;
                if !Self::verify_mldsa87_with_context_bool(
                    &owner.pubkey,
                    message.as_byte_slice(),
                    &declaration.signature,
                    t::PALW_TERMINAL_CONTEXT_V1,
                ) {
                    return Err("terminal declaration is not signed by the reserver".into());
                }
            }
            Obj::ObjectiveEvidenceChunkV1 { part } => {
                use kaspa_consensus_core::palw_terminal_carriage_v1 as t;
                // A-2: below its owning fence (`palw_legacy_public_filer_v1`) the kind rides unjudged, as int-12 reads it; past it, the
                // objective-offence and attribution fences are this kind's own rule.
                if !self.palw_legacy_public_filer_at(point.daa_score) {
                    return Ok(());
                }
                if !self.palw_objective_offence_at(point.daa_score) || !self.palw_offence_attribution_at(point.daa_score) {
                    return Err("terminal evidence is below its objective-offence/attribution fence".into());
                }
                let row = state.terminal_pending_v1(&(part.claim, part.reserver)).ok_or("terminal part has no owned declaration")?;
                let court = self.palw_court_params_v2.as_ref().ok_or("terminal evidence needs V2 court parameters")?;
                if let Some(evidence) = row.preview(part, court.max_close_bytes())? {
                    let d = &row.declaration;
                    if matches!(d.kind, kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::TirIdentityMismatch)
                        && !self.palw_tir_at(point.daa_score)
                    {
                        return Err("terminal IR identity proof below its IR fence".into());
                    }
                    if t::palw_terminal_is_constraint_v1(d.kind, &evidence)
                        && !state_params.fp_decode_constraint_active_at(point.daa_score)
                    {
                        return Err("terminal constraint proof below its constraint fence".into());
                    }
                    if !state_params.capacity_batch_active_at(point.daa_score)
                        && kaspa_consensus_core::palw_offence_attribution_v1::palw_false_valid_evidence_is_windowed_v1(&evidence)
                    {
                        return Err("terminal Windowed receipt below its batch fence".into());
                    }
                    let accused = state.bond(&d.accused).ok_or("terminal accused has no registered bond")?;
                    // RFC06-4: the block's two closure mirrors (both `None` on every shipped network: the V1 call).
                    t::palw_terminal_adjudicate_v2(
                        state,
                        d,
                        &evidence,
                        court.max_close_bytes(),
                        state_params.fp_decode_rules_at(point.daa_score),
                        self.palw_identity_rules_v1(point.daa_score),
                        self.palw_session_rule_at(point.daa_score),
                        Some(kaspa_consensus_core::palw_offence_attribution_v1::PalwFalseValidSigCheckV1 {
                            chain_domain: self.palw_network_domain_v2(),
                            seat_pubkey: &accused.pubkey,
                            seat_active: matches!(
                                accused.status,
                                kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active
                                    | kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Retiring { .. }
                            ),
                            verify: &Self::verify_mldsa87_with_context_bool,
                        }),
                        kaspa_consensus_core::palw_cell_coverage_closure_v1::PalwClosureMirrorsV1::of(state_params),
                    )?;
                }
            }
            Obj::LegacyHeldAnswerDeclaredV2 { declaration } => {
                use kaspa_consensus_core::palw_legacy_answer_chunks_v2 as chunks;
                let court = self.palw_court_params_v2.as_ref().ok_or("multipart DA answer needs V2 court parameters")?;
                declaration.check(court.max_close_bytes())?;
                let domain = self.palw_network_domain_v2();
                let message = chunks::palw_legacy_answer_declaration_message_v2(domain.as_byte_slice(), declaration);
                self.palw_legacy_held_signature_ok(
                    state,
                    point.daa_score,
                    &declaration.claim,
                    &declaration.discloser,
                    &message,
                    &declaration.signature,
                    chunks::PALW_LEGACY_ANSWER_DECLARE_CONTEXT_V2,
                    borsh::to_vec(declaration).map_err(|e| e.to_string())?.len() as u64,
                )?;
            }
            Obj::LegacyHeldAnswerChunkV2 { chunk } => {
                use kaspa_consensus_core::palw_legacy_held_da_v2 as l;
                if !self.palw_legacy_held_da_v2_at(point.daa_score) {
                    return Err("multipart DA answer is below palw_legacy_held_da_v2".into());
                }
                let row = state.legacy_answer_pending_v2(&chunk.key()).ok_or("no signed multipart DA declaration")?;
                let court = self.palw_court_params_v2.as_ref().ok_or("multipart DA answer needs V2 court parameters")?;
                row.declaration.check(court.max_close_bytes())?;
                if let Some(answer) = row.preview(chunk)? {
                    let domain = self.palw_network_domain_v2();
                    let message = l::palw_legacy_held_answer_message_v2(domain.as_byte_slice(), &answer);
                    self.palw_legacy_held_signature_ok(
                        state,
                        point.daa_score,
                        &answer.claim,
                        &answer.discloser,
                        &message,
                        &answer.signature,
                        l::PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2,
                        row.declaration.total_bytes,
                    )?;
                }
            }
            Obj::LegacyLeafRecomputedV2 { accusation } => {
                use kaspa_consensus_core::palw_legacy_held_da_v2 as l;
                let domain = self.palw_network_domain_v2();
                let message = l::palw_legacy_leaf_recompute_message_v2(domain.as_byte_slice(), accusation);
                let bytes = l::palw_legacy_leaf_recompute_bytes_v2(accusation);
                self.palw_legacy_held_signature_ok(
                    state,
                    point.daa_score,
                    &accusation.claim,
                    &accusation.accuser_bond,
                    &message,
                    &accusation.signature,
                    l::PALW_LEGACY_LEAF_RECOMPUTE_MLDSA87_CONTEXT_V2,
                    bytes,
                )?;
            }
            _ => unreachable!("court validator received another object family"),
        }
        Ok(())
    }
}
