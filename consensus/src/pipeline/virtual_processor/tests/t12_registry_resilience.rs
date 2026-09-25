//! **Lane F1 at the processor, on testnet-12: the registry-resilience fence (the 2026-09-25 sweep's
//! V03 and V05) armed at a height a real chain crosses.**
//!
//! testnet-12 as released with `Params::palw_registry_resilience` armed at DAA 20 — a copy, as the
//! operator's post-launch build will carry it — beside the launch twin (the fence unset). The chain is
//! `t12_round_lane_e2e`'s harness (harness keys on the eight cards, the premine imported, the EVM lane
//! inert): heartbeats, card 0's floor attempt below the fence, and card 7's attempt block past it —
//! the claim's anchor block. PoW is skipped and nothing else is.
//!
//! * **The processor folds with the fence it was built with** — the bundle's mirror, which the fold's
//!   load-time re-derivations read — and the launch twin with none.
//! * **Crossing the fence changes nothing a floor claim sees**: every block of a chain that crosses the
//!   height folds, at the processor, to the same state under the armed state params as under the
//!   launch build's, root for root; the floor claim binds in its anchor block on both builds. And an
//!   anchor block that leaves a floor claim unbound still voids it `BindTimeout` past the fence: the
//!   floor can always seat a panel, so the re-anchor never reaches it.
//! * **V03(1) at a real anchor block.** The anchor block's parent state is rebuilt through the
//!   carriage with the claim moved to a model class no seat is ready for (testnet-12's 8k row,
//!   copied under a class id of its own, the way `t47` seeds a class — no block of this harness can
//!   register and ready one). Folded by the processor at the anchor block's own point (its
//!   `sw8_anchor_delay`, its registry fold, the bundle's mirror), the claim — accepted BELOW the
//!   fence, anchored past it — is re-based on the anchor block, its reservation, escrow and backstop
//!   untouched; on the launch twin the same fold voids it `NoCapablePanel`. A heartbeat past the new
//!   slot anchors nothing; the next attempt block at the new slot is its anchor: with the class still
//!   unready the processor re-bases it again, and with the class's seats ready (V2 rows, rebuilt
//!   through the carriage) the processor's OWN derivation — anchor walk, draw, gate, fold — binds it
//!   there, and the bound claim keeps its one redraw.
use super::t12_round_lane_e2e::{T12Chain, t12_genesis_chain, t12_with_harness_cards};
use crate::model::stores::headers::HeaderStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::{PalwModelLifecycleV1, PalwSeatReadinessRowV1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwStateCarriageV2, PalwVoidReasonV2,
};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// The height the armed chain crosses: above the claim's acceptance, below its anchor.
const FENCE: u64 = 20;

/// The DAA the chain's heartbeats reach before card 0 attempts: late enough that the claim's anchor
/// block (twenty DAA on) is past the registry's activation grace — testnet-12 arms the registry at
/// genesis, and for one readiness age (thirty spans) it steps no row and judges no class unready —
/// and early enough that the claim is accepted below [`FENCE`].
const PRE_DAA: u64 = 12;

/// testnet-12 with harness cards; `fence = Some(h)` arms `palw_registry_resilience` at `h` and
/// re-mirrors the bundle, as the operator's build assembles it; `None` is the launch build.
fn t12(fence: Option<u64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    assert_eq!(config.params.palw_registry_resilience, None, "testnet-12 ships the fence dormant");
    let Some(at) = fence else { return (config, bundle, premine, floats) };
    let mut params = config.params.clone();
    params.palw_registry_resilience = Some(ForkActivation::new(at));
    params.sync_palw_registry_resilience();
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

/// The chain up to a floor claim's anchor block: heartbeats, card 0's attempt (the claim), heartbeats
/// to its slot, card 7's attempt (the anchor). Every block with its PARENT state, in order, the claim,
/// the anchor block and its parent state.
struct ToTheAnchor {
    chain: T12Chain,
    blocks: Vec<(BlockHash, PalwChainStateV2)>,
    claim_id: Hash64,
    accepted_daa: u64,
    anchor: BlockHash,
    parent: PalwChainStateV2,
}

async fn to_the_anchor(fence: Option<u64>) -> ToTheAnchor {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12(fence);
    let ttpb = config.params.target_time_per_block();
    let anchor_delay = bundle.panel.anchor_delay();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let mut blocks = Vec::new();
    for _ in 0..(4 * PRE_DAA + 400) {
        let before = chain.tip_state().1;
        let beat = chain.heartbeat(ttpb, Vec::new()).await;
        blocks.push((beat.header.hash, before));
        if beat.header.daa_score >= PRE_DAA {
            break;
        }
    }
    let before = chain.tip_state().1;
    let (block, claim_id) = chain.attempt(0, ttpb, Vec::new(), &|_| true).await;
    blocks.push((block.header.hash, before));
    let accepted_daa = chain.tip_state().1.claim(&claim_id).expect("the attempt block made its claim").accepted_daa;
    let slot = accepted_daa + anchor_delay;
    for _ in 0..(4 * anchor_delay + 400) {
        let before = chain.tip_state().1;
        let beat = chain.heartbeat(ttpb, Vec::new()).await;
        blocks.push((beat.header.hash, before));
        if beat.header.daa_score >= slot {
            break;
        }
    }
    let (_, parent) = chain.tip_state();
    let (anchor, _) = chain.attempt(7, ttpb, Vec::new(), &|_| true).await;
    blocks.push((anchor.header.hash, parent.clone()));
    assert!(anchor.header.daa_score >= slot, "the anchor block is at or past the claim's slot");
    let globals = kaspa_consensus_core::palw_model_registry_v1::palw_registry_globals_of_bundle_v1(&chain.bundle);
    let span_daa = chain.config.params.palw_execution_lane.expect("t12 opens the lane").schedule_span_daa_at(0);
    let grace = kaspa_consensus_core::palw_model_registry_v1::PalwModelRegistryFoldV1::grace_until_v1(0, span_daa, &globals);
    assert!(anchor.header.daa_score >= grace, "the premise: the registry governs at the anchor block ({grace})");
    ToTheAnchor { chain, blocks, claim_id, accepted_daa, anchor: anchor.header.hash, parent }
}

/// `s` rebuilt through the carriage by `edit`, and loaded by the node's own loader checks.
fn rebuilt(chain: &T12Chain, s: &PalwChainStateV2, edit: impl FnOnce(&mut PalwStateCarriageV2)) -> PalwChainStateV2 {
    let daa = s.last_point().map(|p| p.daa_score).unwrap_or(0);
    let mut c = PalwStateCarriageV2::from_state(s);
    edit(&mut c);
    c.into_state_v3(
        &chain.bundle.state,
        None,
        chain.config.params.palw_uncertified_weightless.is_some_and(|f| f.is_active(daa)),
        chain.config.params.palw_canonical_work_daa(),
    )
    .expect("the rebuilt carriage is consistent")
}

/// The model class this suite seeds: a class id of its own.
fn model_class() -> Hash64 {
    Hash64::from_u64_word(0xF1_0325)
}

/// `s` with [`model_class`] seeded — the floor's record, target and a share donated from the table,
/// and testnet-12's 8k lifecycle row (the model row with the least verification work, as `t47` picks
/// it), `Active` — and `claim_id` moved into it. No seat has a readiness row for the class, so no
/// panel can be seated for it.
fn with_the_claim_in_an_unready_model_class(chain: &T12Chain, s: &PalwChainStateV2, claim_id: Hash64) -> PalwChainStateV2 {
    let floor = chain.bundle.base_class_id;
    let class_id = model_class();
    rebuilt(chain, s, |c| {
        let record = c.classes[&floor].clone();
        c.classes.insert(class_id, record);
        let target = c.class_targets[&floor].clone();
        c.class_targets.insert(class_id, target);
        let donor = c
            .class_shares
            .iter()
            .filter(|(id, share)| **id != floor && **share >= 2)
            .max_by_key(|(_, share)| **share)
            .map(|(id, _)| *id)
            .unwrap_or(floor);
        let given = if donor == floor { 1 } else { c.class_shares[&donor] / 2 };
        *c.class_shares.get_mut(&donor).unwrap() -= given;
        c.class_shares.insert(class_id, given);
        let (_, row) = c
            .model_lifecycles
            .iter()
            .filter(|(id, _)| **id != floor)
            .min_by_key(|(_, row)| row.work.verification_ccu)
            .expect("testnet-12 registers model rows");
        let mut row = row.clone();
        row.state = PalwModelLifecycleV1::Active;
        row.profile.max_inflight_claims = row.profile.max_inflight_claims.max(8);
        c.model_lifecycles.insert(class_id, row);
        assert!(!c.seat_readiness.keys().any(|(_, class)| *class == class_id), "no seat is ready for the class");
        c.claims.get_mut(&claim_id).expect("the claim").class_id = class_id;
    })
}

/// `s` with every genesis card holding a fresh V2 readiness row for [`model_class`] at `daa`.
fn with_the_model_class_ready(chain: &T12Chain, s: &PalwChainStateV2, daa: u64) -> PalwChainStateV2 {
    let span_daa = chain.config.params.palw_execution_lane.expect("t12 opens the lane").schedule_span_daa_at(daa).max(1);
    let cards = chain.bonds.clone();
    rebuilt(chain, s, |c| {
        for card in cards {
            c.seat_readiness.insert(
                (card, model_class()),
                PalwSeatReadinessRowV1 { proved_daa: daa, proved_span: daa / span_daa, leaf_index: 0, proof_version: 2, chunks: 16 },
            );
        }
    })
}

/// **The processor folds with the fence it was built with** — the bundle's mirror — and the launch
/// twin with none.
#[tokio::test]
async fn the_processor_folds_with_the_fence_it_was_built_with() {
    for fence in [None, Some(FENCE)] {
        let (config, bundle, premine, floats) = t12(fence);
        let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
        let vp = chain.vp();
        let mirror = vp.palw_state_params_v2.as_ref().expect("testnet-12 is ConsensusV2").registry_resilience_from_daa();
        assert_eq!(mirror, fence, "the processor's state params carry the fence");
        assert_eq!(bundle.state.registry_resilience_from_daa(), fence, "…as the bundle's mirror does");
    }
}

/// **Crossing the fence changes nothing a floor claim sees.** On a real chain that crosses the fence
/// between a floor claim and its anchor block, the processor's fold of EVERY block — its parent state,
/// its own point and extras, the bindings the chain derives for it — gives the same state under the
/// armed ruleset's state params as under the launch build's, root for root, below the fence and past
/// it; the floor claim binds in its anchor block, as on the launch chain; no probation memory is
/// written. Folded at the anchor block's own point without its binding, a floor claim is SW-8's
/// `BindTimeout` void on both builds: the floor can always seat a panel, so the re-anchor never
/// reaches it. (Two chains cannot be compared block for block: the harness's coinbase payload is
/// random, so every block of a second chain is a different block.)
#[tokio::test]
async fn a_chain_crossing_the_fence_is_the_launch_chain_for_a_floor_claim() {
    let launch = to_the_anchor(None).await;
    let armed = to_the_anchor(Some(FENCE)).await;
    let anchor_daa = armed.chain.daa_of(armed.anchor);
    assert!(armed.accepted_daa < FENCE && FENCE <= anchor_daa, "the premise: the chain crosses the fence between the claim and its anchor");
    let vp = armed.chain.vp();
    let armed_sp = armed.chain.bundle.state.clone();
    let launch_sp = launch.chain.bundle.state.clone();
    assert_eq!((armed_sp.registry_resilience_from_daa(), launch_sp.registry_resilience_from_daa()), (Some(FENCE), None));
    let mut crossed = false;
    for (block, parent) in &armed.blocks {
        let point = point_of(&armed.chain, *block);
        crossed |= point.daa_score >= FENCE;
        let derived = vp.palw_v2_derived_panel_bindings_for_tests(parent, *block, point.daa_score);
        let under_armed = vp.palw_v2_fold_accepted_for_tests(parent, &armed_sp, &point, &derived).expect("folds armed");
        let under_launch = vp.palw_v2_fold_accepted_for_tests(parent, &launch_sp, &point, &derived).expect("folds as launched");
        assert_eq!(
            under_armed.state_root(),
            under_launch.state_root(),
            "block at DAA {}: the armed rules fold it exactly as the launch rules do",
            point.daa_score
        );
        assert!(under_armed.model_lifecycles_iter().all(|(id, _)| under_armed.probation_memory_of_v1(id).is_none()));
    }
    assert!(crossed, "the walk crossed the fence");
    for run in [&launch, &armed] {
        let anchor_daa = run.chain.daa_of(run.anchor);
        let (_, state) = run.chain.tip_state();
        assert_eq!(
            state.claim(&run.claim_id).unwrap().phase,
            PalwClaimPhaseV2::PanelBound { bound_daa: anchor_daa },
            "the floor claim binds in its anchor block"
        );
        let vp = run.chain.vp();
        let point = point_of(&run.chain, run.anchor);
        assert_eq!(vp.palw_sw8_anchor_delay_for(&point), Some(run.chain.bundle.panel.anchor_delay()), "an anchor block");
        let unbound =
            vp.palw_v2_fold_accepted_for_tests(&run.parent, &run.chain.bundle.state, &point, &[]).expect("the anchor block folds");
        assert_eq!(
            unbound.claim(&run.claim_id).unwrap().phase,
            PalwClaimPhaseV2::Voided { voided_daa: anchor_daa, reason: PalwVoidReasonV2::BindTimeout },
            "left unbound, a floor claim is SW-8's void on both builds"
        );
    }
}

/// **V03(1) at a real anchor block, and the claim binding at its next one** (see the module doc).
#[tokio::test]
async fn an_anchor_block_re_anchors_a_claim_no_capable_panel_could_take_and_the_next_one_binds_it() {
    let launch = to_the_anchor(None).await;
    let mut armed = to_the_anchor(Some(FENCE)).await;
    let anchor_daa = armed.chain.daa_of(armed.anchor);
    let window_bind = armed.chain.bundle.state.window_bind();
    let anchor_delay = armed.chain.bundle.panel.anchor_delay();
    let claim_id = armed.claim_id;

    // The launch twin (its own chain, so its own claim and anchor): the anchor block voids the claim
    // `NoCapablePanel`.
    {
        let parent = with_the_claim_in_an_unready_model_class(&launch.chain, &launch.parent, launch.claim_id);
        let point = point_of(&launch.chain, launch.anchor);
        let folded = launch.chain.vp().palw_v2_fold_accepted_for_tests(&parent, &launch.chain.bundle.state, &point, &[]).unwrap();
        assert_eq!(
            folded.claim(&launch.claim_id).unwrap().phase,
            PalwClaimPhaseV2::Voided { voided_daa: point.daa_score, reason: PalwVoidReasonV2::NoCapablePanel },
            "the launch build voids an unbindable claim in its anchor block"
        );
    }

    // The armed build: re-based on the anchor block, everything it owed kept.
    let vp = armed.chain.vp();
    let bundle = armed.chain.bundle.clone();
    let parent = with_the_claim_in_an_unready_model_class(&armed.chain, &armed.parent, claim_id);
    let before = parent.claim(&claim_id).unwrap().clone();
    let producer = before.bond;
    let point = point_of(&armed.chain, armed.anchor);
    let retried = vp.palw_v2_fold_accepted_for_tests(&parent, &bundle.state, &point, &[]).expect("the anchor block folds");
    let claim = retried.claim(&claim_id).unwrap().clone();
    assert_eq!(
        (claim.phase.clone(), claim.rebound_daa),
        (PalwClaimPhaseV2::Provisional, Some(anchor_daa)),
        "accepted below the fence, anchored past it: re-based on the anchor block"
    );
    assert_eq!(claim.escrowed_reward, before.escrowed_reward, "the escrow stays with the claim");
    assert_eq!(retried.reserved_exposure(&producer), parent.reserved_exposure(&producer), "so does the reservation");
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
    let beat = beat_past_slot.expect("the chain reaches the new slot");
    let beat_point = point_of(&armed.chain, beat);
    assert_eq!(vp.palw_sw8_anchor_delay_for(&beat_point), None, "a heartbeat is no anchor block");
    let after_beat = vp.palw_v2_fold_accepted_for_tests(&retried, &bundle.state, &beat_point, &[]).expect("the heartbeat folds");
    assert_eq!(after_beat.claim(&claim_id).unwrap().rebound_daa, Some(anchor_daa), "a heartbeat past the slot changes nothing");

    // The next attempt block is the claim's new anchor block.
    let (next, _) = armed.chain.attempt(5, ttpb, Vec::new(), &|_| true).await;
    let next = next.header.hash;
    let next_point = point_of(&armed.chain, next);
    assert!(next_point.daa_score >= slot && next_point.daa_score <= armed.accepted_daa + window_bind, "inside the bind window");
    // Still unready: re-based again.
    let again = vp.palw_v2_fold_accepted_for_tests(&retried, &bundle.state, &next_point, &[]).expect("the next anchor folds");
    assert_eq!(
        (again.claim(&claim_id).unwrap().phase.clone(), again.claim(&claim_id).unwrap().rebound_daa),
        (PalwClaimPhaseV2::Provisional, Some(next_point.daa_score)),
        "still unbindable: re-based on the next anchor block"
    );
    // Ready: the processor's own derivation binds it in this, its new anchor block.
    let ready = with_the_model_class_ready(&armed.chain, &retried, anchor_daa);
    let derived = vp.palw_v2_derived_panel_bindings_for_tests(&ready, next, next_point.daa_score);
    // The binding's `anchor` is whatever the draw's seed rule names for this anchor block — the block
    // itself as released, the anchor attempt's execution commitment once the panel-seed lane's fence
    // is in force — so it is compared with the stored panel, never spelled here.
    let (drawn_anchor, seats) = match derived.as_slice() {
        [Obj::PanelBound { claim, anchor, seats }] => {
            assert_eq!(*claim, claim_id, "the claim's binding, derived in its new anchor block");
            (*anchor, seats.clone())
        }
        other => panic!("the new anchor block derives the claim's binding, got {other:?}"),
    };
    assert_eq!(seats.len(), bundle.panel.seat_count() as usize, "a full jury");
    let bound = vp.palw_v2_fold_accepted_for_tests(&ready, &bundle.state, &next_point, &derived).expect("the binding folds");
    let claim = bound.claim(&claim_id).unwrap();
    assert_eq!(claim.phase, PalwClaimPhaseV2::PanelBound { bound_daa: next_point.daa_score }, "bound at the new anchor");
    assert_eq!(claim.rebound_daa, None, "a re-anchor is not the one redraw: the bound claim keeps it");
    assert_eq!(claim.escrowed_reward, before.escrowed_reward, "the escrow rides to the bind, nothing burned");
    assert_eq!(bound.panel(&claim_id).expect("a bound claim has a panel").anchor, drawn_anchor, "the fold stores the drawn panel");
    eprintln!(
        "[t12-f1] claim {claim_id}: accepted {} (fence {FENCE}), re-anchored at {anchor_daa}, bound at {} by seats {:?}",
        armed.accepted_daa,
        next_point.daa_score,
        seats.iter().map(|s| armed.chain.bonds.iter().position(|b| *b == s.bond).unwrap()).collect::<Vec<_>>()
    );
}
