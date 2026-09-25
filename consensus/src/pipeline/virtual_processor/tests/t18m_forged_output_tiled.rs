//! **ADR-0152 v3.1 T18m (F1-M, in the launch gate: §8.3 item 1): `ForgedOutputTiled` convicts a
//! forged token on a real tiled decode, by kind 3 (full-mask signers) and kind 4, before and after
//! `Final`; an honest tiled decode never convicts.** A child of `t47_model_class_attribution`, so the
//! claims are its held model fixtures' own runs — the A16 graph-v7 row and the Qwen3.6 graph-v7 row,
//! each under testnet-12's `CoreV1` — carried through T46's gate, acceptance walk and fold.
//!
//! What was already in the suite: kind 4 before any licence on both fixtures (`t47d`, T18r's chain
//! half) and the base0 adjudicator over real runs (`f1c_logits_not_step_output`'s `t18r`). This file
//! adds the rest of T18m's cells: kind 3 against the licence's full-mask signer (a partial seat is
//! `SiteNotAttested` — a forged token is a whole-execution fault no segment replay attests), kind 3
//! and kind 4 after `Final` (the `Final` reversed, the root forfeit, the vesting row burned with S3 on
//! the producer), and the honest decode refused `TokenHolds` by both kinds.
//!
//! **The 2M cell is not run, and cannot be at launch:** no producer of the 2M width runs in a test
//! (`t47e`'s note), and the 2M row is closed at launch by §4-quater (U-D1: its attempts and free
//! prompts are refused `ClassDeadlineUnmeasured`, `t12_class_verify_deadline`'s T-D2), so no 2M claim
//! exists to carry a forged token until the flag day that measures its row. `ForgedOutputTiled`'s
//! adjudicator is profile-generic (`palw_forged_output_tiled_fault_v1` reads the binding's tiled
//! integer head, nothing of the width); T18m's 2M cell belongs to that flag day's gate.

use super::*;

/// An honest claim's own first token, "refuted" by its runner-up: the tiled pin of a lane that does
/// NOT beat the committed selection — the proof a griefer files against an honest decode.
fn honest_tiled_proof(claim: &ModelClaim) -> C {
    use kaspa_consensus_core::palw_offence_v1::PalwForgedOutputTiledProofV1 as P;
    use kaspa_consensus_core::palw_step_refute::{base0_decode_token_select_v1, tiled_decode_pin_v1};
    let retention = misaka_palw_base0::produce::base0_material_decode_any_v1(&claim.material).expect("the capture decodes");
    let (rows, ids) = (retention.logits_rows().to_vec(), retention.generated_token_ids().to_vec());
    let ctx = claim.binding.job_context.clone();
    let top = base0_decode_token_select_v1(&rows[0]) as u32;
    assert_eq!(ids[0], top, "the honest run committed its row's selection");
    let vocab = claim.binding.shape_profile.vocab_size;
    let runner_up = (0..vocab).filter(|l| *l != top).max_by_key(|l| (rows[0][*l as usize], std::cmp::Reverse(*l))).unwrap();
    C::ForgedOutputTiled {
        binding: claim.binding.clone(),
        proof: P::NotSelected { pin: tiled_decode_pin_v1(&ctx, &rows, &ids, 0, runner_up).expect("the pin opens") },
    }
}

/// A model claim of `fault` on a fresh walk with `m` seeded, bound to T46's panel and licensed
/// through Verification V2.
fn licensed_model_claim(h: &H, m: &ModelClass, fault: ModelFault, nonce: u64) -> (Walk, ModelClaim, Licence) {
    let mut walk = h.genesis_walk();
    seed(h, &mut walk, m);
    let claim = open_model_claim(h, &mut walk, m, fault, nonce);
    h.bind(&mut walk, claim.claim_id);
    let licence = h.license_v2(&mut walk, claim.claim_id);
    (walk, claim, licence)
}

/// **T18m, kind 3 before `Final`, and the honest decode.** On both fixtures and both forgeries (`NotSelected`,
/// `OutOfVocab`): every partial-mask signer is refused `SiteNotAttested`, and the full-mask signer is
/// convicted — its lock and S4's action taken, one kind-3 record, the claim voided `CourtFraud`, the
/// executor charged S2, the root forfeit. On an honest licensed claim the same proof over its own
/// decode is refused `TokenHolds` by kind 3 (the gate, the walk drops it) and by kind 4 (the
/// adjudicator both layers run).
#[tokio::test]
async fn t18m_forged_output_tiled_convicts_the_full_mask_signer_before_final_and_never_an_honest_decode() {
    let h = harness(true);
    for m in families(&h) {
        let (walk, honest, licence) = licensed_model_claim(&h, &m, ModelFault::Honest, bucket(0x18E0));
        let proof = honest_tiled_proof(&honest);
        let full = licence.full_card();
        h.refused(&walk, &h.v2(full, honest.claim_id, licence.segmented(full), proof.clone()), &E::TokenHolds.to_string());
        assert_eq!(
            h.judge_refuted(&walk.state, honest.claim_id, proof).err(),
            Some(E::TokenHolds),
            "{}: kind 4 refuses an honest decode",
            m.label
        );
        for (n, fault) in [ModelFault::TokenNotSelected, ModelFault::TokenOutOfVocab].into_iter().enumerate() {
            let (mut walk, claim, licence) = licensed_model_claim(&h, &m, fault, bucket(0x18E1 + n as u64));
            let id = claim.claim_id;
            let c = claim.contradiction();
            assert!(matches!(c, C::ForgedOutputTiled { .. }), "{} {fault:?}: 11", m.label);
            let full = licence.full_card();
            for partial in licence.partials() {
                h.refused(&walk, &h.v2(partial, id, licence.segmented(partial), c.clone()), &E::SiteNotAttested.to_string());
            }
            let (licensed, _) = h.carry(&mut walk, vec![h.v2(full, id, licence.segmented(full), c)]);
            assert_convicted_before_final(&h, &licensed, &walk.state, id, &[full], walk.daa);
            h.reloads(&walk.state);
        }
    }
}

/// **T18m after `Final`, by kind 3 and by kind 4.** A forged-token claim licensed and swept to
/// `Final` (its vesting row written), then convicted: the `Final` is reversed (voided `CourtFraud` at
/// the conviction, `safe_weight` back by exactly what the `Final` added), the root forfeit, the
/// vesting row burned and the producer charged S3 = `min(25% · C₀, 3 G)` once, `G` the liability
/// record's. Kind 3 also takes the full seat's S4 and records both legs; kind 4 charges the producer
/// alone (every seat's lock and bond untouched) and records S3 under the per-claim key. The block
/// reverts to its parent exactly.
#[tokio::test]
async fn t18m_forged_output_tiled_after_final_reverses_it_by_kind_3_and_by_kind_4() {
    use kaspa_consensus_core::palw_state_v2::palw_rcore_s3s4_action_v1;
    assert!(kaspa_consensus_core::palw_state_v2::PALW_RCORE_VESTING_ROWS_LANDED_V1, "the rows landed (IA-7)");
    let h = harness(true);
    for m in families(&h) {
        for (n, kind4) in [false, true].into_iter().enumerate() {
            let label = format!("{} kind {}", m.label, if kind4 { 4 } else { 3 });
            let (mut walk, claim, licence) = licensed_model_claim(&h, &m, ModelFault::TokenNotSelected, bucket(0x18F0 + n as u64));
            let id = claim.claim_id;
            let before_final = h.sweep_to_final(&mut walk, id);
            let at_final = walk.state.clone();
            assert!(matches!(at_final.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "{label}: Final");
            let contribution = at_final.safe_weight() - before_final.safe_weight();
            let row = at_final.vesting_row(&id).expect("an attempt's Final writes its vesting row").clone();
            let root = claim.envelope.attempt.execution_root;
            let executor = h.cards[EXECUTOR];
            let full = licence.full_card();
            let seat = h.cards[full];
            let lock = *at_final.slashable_lock(seat, id).expect("the full seat's lock outlives the Final");
            let object = if kind4 {
                h.refuted(id, claim.contradiction())
            } else {
                h.v2(full, id, licence.segmented(full), claim.contradiction())
            };
            let (parent, delta) = h.carry(&mut walk, vec![object]);
            let s = &walk.state;
            let daa = walk.daa;
            assert!(
                matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtFraud } if voided_daa == daa),
                "{label}: the Final is reversed: {:?}",
                s.claim(&id).unwrap().phase
            );
            assert_eq!(s.safe_weight(), at_final.safe_weight() - contribution, "{label}: safe_weight falls by what the Final added");
            assert!(s.palw_execution_root_is_forfeited_v1(&root), "{label}: the proven-false root is forfeit");
            assert!(s.vesting_row(&id).is_none(), "{label}: the vesting row is burned");
            assert_eq!(s.vesting_counters().burned - at_final.vesting_counters().burned, row.total_sompi_u128(), "{label}: once, whole");
            let g = g_of(s, id);
            let c0 = at_final.bond(&executor).unwrap().collateral;
            let s3 = palw_rcore_s3s4_action_v1(c0, g);
            assert!(s3 > 0);
            assert_eq!(u128::from(c0 - s.bond(&executor).unwrap().collateral), s3, "{label}: S3 on the producer, once");
            if kind4 {
                for card in PANEL {
                    let panel_seat = h.cards[card];
                    assert_eq!(
                        (s.bond(&panel_seat).unwrap().collateral, s.slashable_lock(panel_seat, id).map(|l| l.amount)),
                        (at_final.bond(&panel_seat).unwrap().collateral, at_final.slashable_lock(panel_seat, id).map(|l| l.amount)),
                        "{label}: card {card} — kind 4 charges the executor alone"
                    );
                }
                let record =
                    s.consumed_offence(&palw_executor_refuted_offence_id_v1(&executor.0, &id)).expect("one kind-4 record per claim");
                assert_eq!(
                    (record.kind, u128::from(record.amount), u128::from(record.collected), record.claim_id, record.execution_root),
                    (PalwOffenceKindV1::ExecutorRefuted, s3, s3, id, root),
                    "{label}: the kind-4 record — S3, collected, the claim, the root"
                );
            } else {
                let (nominal, debit) = s4_charge(&h, &at_final, seat, lock.amount, g, daa);
                assert_eq!(
                    u128::from(at_final.bond(&seat).unwrap().collateral - s.bond(&seat).unwrap().collateral),
                    debit,
                    "{label}: S4 on the full seat"
                );
                assert!(s.slashable_lock(seat, id).is_none(), "{label}: its lock is taken");
                let record = s.consumed_offence(&palw_false_valid_offence_id_v2(&seat.0, &id)).expect("one kind-3 record");
                assert_eq!(
                    (record.kind, u128::from(record.amount), u128::from(record.collected), record.claim_id, record.execution_root),
                    (PalwOffenceKindV1::PanelFalseValidV2, nominal + s3, debit + s3, id, root),
                    "{label}: the kind-3 record — the seat's S4 and the producer's S3"
                );
            }
            h.reloads(s);
            assert_eq!(revert_delta_v2(s, &delta, h.sp()).expect("reverts").state_root(), parent.state_root(), "{label}: reverts");
        }
    }
}
