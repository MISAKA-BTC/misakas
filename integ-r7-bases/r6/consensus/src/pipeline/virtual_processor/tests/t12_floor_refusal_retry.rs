//! **Lane F2 at the processor, on testnet-12: the floor-refusal retry (`palw_floor_refusal_retry`)
//! armed at a height a real chain crosses.**
//!
//! testnet-12 (with harness cards) with the fence set through its entry of the second post-launch list
//! (`PALW_T12_POST_LAUNCH_FENCES_V2`) at DAA 20 — a copy, as the next flag day's build will carry it —
//! beside the released twin (the fence unset). The chain is `t12_round_lane_e2e`'s harness: heartbeats,
//! card 0's floor attempt below the fence, and card 7's attempt block past it — the claim's anchor
//! block. PoW is skipped and nothing else is.
//!
//! * **The processor carries the draw points past the fence** (`PalwTransitionExtrasV1::sw8_draw`), one
//!   at the anchor block's own DAA below lane A, with the draw policy it resolves there — and none below
//!   the fence or on the released twin.
//! * **The live chain's own saturation, rebuilt at a real anchor block.** The anchor block's parent
//!   state is rebuilt through the carriage with two other cards standing behind live locks of their
//!   whole collateral — testnet-12's DAA 624–749 shape, where SW-10's floor refused `6 / 8` of the
//!   genesis weight (the executor's own card on both sides). The processor's OWN derivation binds
//!   nothing there (the draw refuses), and its fold at the anchor block's point — its `sw8_anchor_delay`,
//!   its draw points, the bundle's mirror — re-bases the claim, accepted below the fence and anchored
//!   past it, on the anchor block: reservation, escrow and backstop untouched. The released twin's fold
//!   voids it `BindTimeout`, as the live chain did.
//! * **The next anchor block binds it once the room is back**: a heartbeat past the new slot anchors
//!   nothing; card 5's attempt block at the new slot is its anchor — with the locks still standing the
//!   processor re-bases it again, and with them lapsed the processor's own derivation (anchor walk,
//!   draw, gate, fold) binds a full jury there, and the bound claim keeps its one redraw.
use super::t12_round_lane_e2e::{T12Chain, t12_genesis_chain, t12_with_harness_cards};
use crate::model::stores::headers::HeaderStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::config::params::{ForkActivation, PALW_T12_POST_LAUNCH_FENCES_V2};
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_var_v1::PalwSlashableLockV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwStateCarriageV2, PalwVoidReasonV2,
    palw_claim_awaits_ncp_retry_v1,
};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// The height the armed chain crosses: above the claim's acceptance, below its anchor.
const FENCE: u64 = 20;

/// The DAA the chain's heartbeats reach before card 0 attempts (the claim is accepted below [`FENCE`]).
const PRE_DAA: u64 = 12;

/// The two cards whose seats the rebuilt parent locks: neither the executor (card 0) nor a later anchor.
const LOCKED: [usize; 2] = [2, 3];

/// testnet-12 with harness cards; `fence = Some(h)` arms `palw_floor_refusal_retry` at `h` through the
/// second list's own entry (the field and the bundle's mirror), as the next flag day's build assembles
/// it; `None` is the released build.
fn t12(fence: Option<u64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    assert_eq!(config.params.palw_floor_refusal_retry, None, "testnet-12 ships the fence dormant");
    let Some(at) = fence else { return (config, bundle, premine, floats) };
    let mut params = config.params.clone();
    let entry = PALW_T12_POST_LAUNCH_FENCES_V2.iter().find(|f| f.name == "palw_floor_refusal_retry").expect("the second list's entry");
    (entry.set)(&mut params, Some(ForkActivation::new(at)));
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("the armed copy is a runnable ruleset");
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    let bundle = bundle.clone();
    (config, bundle, premine, floats)
}

/// One chain block's point, as the pipeline spells it.
fn point_of(chain: &T12Chain, block: BlockHash) -> PalwBlockContextV2 {
    let vp = chain.vp();
    let header = vp.headers_store.get_header(block).unwrap();
    PalwBlockContextV2 {
        block,
        daa_score: header.daa_score,
        blue_score: header.blue_score,
        subsidy: vp.coinbase_manager.calc_block_subsidy(header.daa_score),
    }
}

struct ToTheAnchor {
    chain: T12Chain,
    claim_id: Hash64,
    accepted_daa: u64,
    anchor: BlockHash,
    parent: PalwChainStateV2,
}

/// Heartbeats to [`PRE_DAA`], card 0's attempt (the claim), heartbeats to its slot, card 7's attempt
/// (the anchor block) — and the anchor block's parent state.
async fn to_the_anchor(fence: Option<u64>) -> ToTheAnchor {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12(fence);
    let ttpb = config.params.target_time_per_block();
    let anchor_delay = bundle.panel.anchor_delay();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    for _ in 0..(4 * PRE_DAA + 400) {
        if chain.heartbeat(ttpb, Vec::new()).await.header.daa_score >= PRE_DAA {
            break;
        }
    }
    let (_, claim_id) = chain.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let accepted_daa = chain.tip_state().1.claim(&claim_id).expect("the attempt block made its claim").accepted_daa;
    let slot = accepted_daa + anchor_delay;
    for _ in 0..(4 * anchor_delay + 400) {
        if chain.heartbeat(ttpb, Vec::new()).await.header.daa_score >= slot {
            break;
        }
    }
    let (_, parent) = chain.tip_state();
    let (anchor, _) = chain.attempt(7, ttpb, Vec::new(), &|_| true).await;
    assert!(anchor.header.daa_score >= slot, "the anchor block is at or past the claim's slot");
    ToTheAnchor { chain, claim_id, accepted_daa, anchor: anchor.header.hash, parent }
}

/// `s` rebuilt through the carriage with each card of `locked` standing behind a live lock of its whole
/// collateral until `until`, loaded by the node's own loader checks.
fn with_locks(chain: &T12Chain, s: &PalwChainStateV2, locked: &[usize], until: u64) -> PalwChainStateV2 {
    let daa = s.last_point().map(|p| p.daa_score).unwrap_or(0);
    let mut c = PalwStateCarriageV2::from_state(s);
    for card in locked {
        let bond = chain.bonds[*card];
        let collateral = c.bonds[&bond].collateral as u128;
        let claim = Hash64::from_u64_word(0xF2_0000 + *card as u64);
        c.slashable_locks.insert(
            (bond, claim),
            PalwSlashableLockV1 {
                claim,
                amount: collateral,
                expiry_daa: until,
                settled_at_final: 0,
                attested: kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2::NONE,
                segments: 0,
            },
        );
    }
    c.into_state_v3(
        &chain.bundle.state,
        None,
        chain.config.params.palw_uncertified_weightless.is_some_and(|f| f.is_active(daa)),
        chain.config.params.palw_canonical_work_daa(),
    )
    .expect("the rebuilt carriage is consistent")
}

/// **The processor carries the draw points past the fence, and none below it or on the released twin.**
#[tokio::test]
async fn the_processor_carries_the_draw_points_past_the_fence() {
    for fence in [None, Some(FENCE)] {
        let run = to_the_anchor(fence).await;
        let vp = run.chain.vp();
        assert_eq!(
            vp.palw_state_params_v2.as_ref().expect("testnet-12 is ConsensusV2").floor_refusal_retry_from_daa(),
            fence,
            "the processor's state params carry the fence"
        );
        let point = point_of(&run.chain, run.anchor);
        let draw = vp.palw_sw8_draw_inputs_for(&point);
        match fence {
            None => assert_eq!(draw, None, "the released build carries no draw points"),
            Some(_) => {
                let draw = draw.expect("an anchor block past the fence carries its draw points");
                assert_eq!(draw.seat_count, run.chain.bundle.panel.seat_count());
                assert_eq!(draw.points.len(), 1, "below lane A one point, the anchor block's own");
                let point_at = draw.points[0];
                assert_eq!((point_at.max_slot, point_at.draw_daa), (point.daa_score, point.daa_score));
                assert_eq!(point_at.policy, vp.palw_panel_draw_policy_at(point.daa_score), "the draw's own policy");
                assert!(point_at.policy.stake.is_some(), "the stake draw");
            }
        }
    }
}

/// **The live chain's saturation at a real anchor block, the re-anchor, and the bind at the next
/// anchor block once the locks lapse** (see the module doc).
#[tokio::test]
async fn an_anchor_block_re_anchors_a_floor_refused_claim_and_the_next_one_binds_it() {
    let launch = to_the_anchor(None).await;
    let mut armed = to_the_anchor(Some(FENCE)).await;
    let anchor_daa = armed.chain.daa_of(armed.anchor);
    let window_bind = armed.chain.bundle.state.window_bind();
    let anchor_delay = armed.chain.bundle.panel.anchor_delay();
    let claim_id = armed.claim_id;
    assert!(
        armed.accepted_daa < FENCE && FENCE <= anchor_daa,
        "the premise: the chain crosses the fence between the claim and its anchor"
    );
    let (_, tip) = armed.chain.tip_state();
    assert_eq!(
        tip.claim(&claim_id).unwrap().phase,
        PalwClaimPhaseV2::PanelBound { bound_daa: anchor_daa },
        "the premise: unloaded, the live chain binds the claim in its anchor block"
    );

    // The released twin (its own chain, so its own claim and anchor): the anchor block voids it.
    {
        let parent = with_locks(&launch.chain, &launch.parent, &LOCKED, u64::MAX);
        let point = point_of(&launch.chain, launch.anchor);
        let vp = launch.chain.vp();
        let derived = vp.palw_v2_derived_panel_bindings_for_tests(&parent, launch.anchor, point.daa_score);
        assert!(derived.is_empty(), "two locked seats: the draw refuses on SW-10's floor, nothing is derived");
        let folded = vp.palw_v2_fold_accepted_for_tests(&parent, &launch.chain.bundle.state, &point, &derived).unwrap();
        assert_eq!(
            folded.claim(&launch.claim_id).unwrap().phase,
            PalwClaimPhaseV2::Voided { voided_daa: point.daa_score, reason: PalwVoidReasonV2::BindTimeout },
            "the released build voids a floor-refused claim in its anchor block"
        );
    }

    // The armed build: re-based on the anchor block, everything it owed kept.
    let vp = armed.chain.vp();
    let bundle = armed.chain.bundle.clone();
    let parent = with_locks(&armed.chain, &armed.parent, &LOCKED, u64::MAX);
    let before = parent.claim(&claim_id).unwrap().clone();
    let point = point_of(&armed.chain, armed.anchor);
    let derived = vp.palw_v2_derived_panel_bindings_for_tests(&parent, armed.anchor, point.daa_score);
    assert!(derived.is_empty(), "the processor's own derivation binds nothing: the draw refuses");
    let retried = vp.palw_v2_fold_accepted_for_tests(&parent, &bundle.state, &point, &derived).expect("the anchor block folds");
    let claim = retried.claim(&claim_id).unwrap().clone();
    assert_eq!(
        (claim.phase.clone(), claim.rebound_daa),
        (PalwClaimPhaseV2::Provisional, Some(anchor_daa)),
        "accepted below the fence, anchored past it: re-based on the anchor block"
    );
    assert!(palw_claim_awaits_ncp_retry_v1(&retried, &bundle.state, &claim_id, &claim), "the retry is read off rooted state");
    assert_eq!(claim.escrowed_reward, before.escrowed_reward, "the escrow stays with the claim");
    assert_eq!(retried.reserved_exposure(&before.bond), parent.reserved_exposure(&before.bond), "so does the reservation");
    assert_eq!(retried.deadline_of(&claim_id), Some(armed.accepted_daa + window_bind), "the backstop is acceptance's");
    retried.assert_deadline_consistency(&bundle.state).expect("the index is the claims' recomputed deadlines");

    // Heartbeats to the new slot: a heartbeat anchors nothing, so its fold leaves the claim as it is.
    let ttpb = armed.chain.config.params.target_time_per_block();
    let slot = anchor_daa + anchor_delay;
    let mut beat_past_slot = None;
    for _ in 0..(4 * anchor_delay + 400) {
        let beat = armed.chain.heartbeat(ttpb, Vec::new()).await;
        if beat.header.daa_score >= slot {
            beat_past_slot = Some(beat.header.hash);
            break;
        }
    }
    let beat_point = point_of(&armed.chain, beat_past_slot.expect("the chain reaches the new slot"));
    assert_eq!(vp.palw_sw8_draw_inputs_for(&beat_point), None, "a heartbeat carries no draw points");
    let after_beat = vp.palw_v2_fold_accepted_for_tests(&retried, &bundle.state, &beat_point, &[]).expect("the heartbeat folds");
    assert_eq!(after_beat.claim(&claim_id).unwrap().rebound_daa, Some(anchor_daa), "a heartbeat past the slot changes nothing");

    // The next attempt block is the claim's new anchor block.
    let (next, _) = armed.chain.attempt(5, ttpb, Vec::new(), &|_| true).await;
    let next = next.header.hash;
    let next_point = point_of(&armed.chain, next);
    assert!(next_point.daa_score >= slot && next_point.daa_score <= armed.accepted_daa + window_bind, "inside the bind window");
    // Still locked: re-based again, by the processor's derivation and fold alike.
    let still = vp.palw_v2_derived_panel_bindings_for_tests(&retried, next, next_point.daa_score);
    assert!(still.is_empty(), "still two locked seats: nothing derived");
    let again = vp.palw_v2_fold_accepted_for_tests(&retried, &bundle.state, &next_point, &still).expect("the next anchor folds");
    assert_eq!(
        (again.claim(&claim_id).unwrap().phase.clone(), again.claim(&claim_id).unwrap().rebound_daa),
        (PalwClaimPhaseV2::Provisional, Some(next_point.daa_score)),
        "still refused at every seed: re-based on the next anchor block"
    );
    // The locks lapsed: the processor's own derivation binds it in this, its new anchor block.
    let lapsed = {
        let mut c = PalwStateCarriageV2::from_state(&retried);
        c.slashable_locks
            .retain(|(_, lock_claim), _| LOCKED.iter().all(|card| *lock_claim != Hash64::from_u64_word(0xF2_0000 + *card as u64)));
        let daa = retried.last_point().map(|p| p.daa_score).unwrap_or(0);
        c.into_state_v3(
            &bundle.state,
            None,
            armed.chain.config.params.palw_uncertified_weightless.is_some_and(|f| f.is_active(daa)),
            armed.chain.config.params.palw_canonical_work_daa(),
        )
        .expect("consistent")
    };
    let derived = vp.palw_v2_derived_panel_bindings_for_tests(&lapsed, next, next_point.daa_score);
    let (drawn_anchor, seats) = match derived.as_slice() {
        [Obj::PanelBound { claim, anchor, seats }] => {
            assert_eq!(*claim, claim_id, "the claim's binding, derived in its new anchor block");
            (*anchor, seats.clone())
        }
        other => panic!("the new anchor block derives the claim's binding, got {other:?}"),
    };
    assert_eq!(seats.len(), bundle.panel.seat_count() as usize, "a full jury");
    let bound = vp.palw_v2_fold_accepted_for_tests(&lapsed, &bundle.state, &next_point, &derived).expect("the binding folds");
    let claim = bound.claim(&claim_id).unwrap();
    assert_eq!(claim.phase, PalwClaimPhaseV2::PanelBound { bound_daa: next_point.daa_score }, "bound at the new anchor");
    assert_eq!(claim.rebound_daa, None, "a floor retry is not the one redraw: the bound claim keeps it");
    assert_eq!(claim.escrowed_reward, before.escrowed_reward, "the escrow rides to the bind, nothing burned");
    assert_eq!(bound.panel(&claim_id).expect("a bound claim has a panel").anchor, drawn_anchor, "the fold stores the drawn panel");
    eprintln!(
        "[t12-f2] claim {claim_id}: accepted {} (fence {FENCE}), re-anchored at {anchor_daa} (cards {LOCKED:?} locked), \
         at {} re-anchored again while locked and bound once they lapse, by seats {:?}",
        armed.accepted_daa,
        next_point.daa_score,
        seats.iter().map(|s| armed.chain.bonds.iter().position(|b| *b == s.bond).unwrap()).collect::<Vec<_>>()
    );
}
