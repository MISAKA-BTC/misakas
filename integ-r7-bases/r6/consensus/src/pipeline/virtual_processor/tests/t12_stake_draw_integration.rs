//! **ADR-0152 M4 at the processor, on testnet-12: the stake-weighted draw where the chain draws.**
//!
//! * **SW-1, the one resolver.** `palw_panel_draw_policy_at` sets `stake: Some(PalwPanelStakeDrawV1::V1)`
//!   iff `palw_rcore_plus` is active at the anchor's DAA — on testnet-12 as shipped, at every anchor;
//!   on its fence-off twin, never — and leaves every other field of the policy as it was (T88: the
//!   draw adds no params field). testnet-11, devnet and mainnet carry no fence at all.
//! * **SW-8 and T89 on a real chain.** A floor claim made by card 0's attempt block is bound by the
//!   chain itself, in its anchor block. The chain's derivation for that block (build), re-run on the
//!   anchor's parent state, is exactly the bound panel; the acceptance walk (accept) takes it on the
//!   same state; the fold stored it (fold). The same `PanelBound`, offered by the next chain block
//!   inside the bind window, is refused by the gate under the stake draw — and accepted on the
//!   fence-off twin, whose window is today's.
//! * **The anchor is an attempt block (M4 review finding 2).** Past `palw_rcore_plus` the heartbeat
//!   at the claim's slot — `2^24` hashes to re-roll — anchors nothing, and the claim binds in the
//!   next attempt block; on the fence-off twin the heartbeat at the slot binds, as it always did.
//! * **The anchor block voids what it does not bind (finding 3, DL-1's exact DAA).** Folded without
//!   its binding — as if its draw, the gate or the fold had refused it — the anchor block voids the
//!   claim `BindTimeout` at its own DAA, without forfeit, on the processor's own extras; a heartbeat
//!   carries no such rule; the fence-off twin keeps the claim to its window.
//! * **The pure half's review note, pinned.** testnet-12's genesis state binds a floor panel under
//!   `stake: Some` (no `InsufficientEligibleStake` at genesis): the eight genesis seats carry no load,
//!   and the executor's own term keeps the floor over all eight (8/8).
//!
//! The chain is `t12_round_lane_e2e`'s harness (testnet-12 as shipped, harness keys on its eight
//! cards, the premine imported, the EVM lane inert), heartbeats and one attempt block; PoW is skipped
//! and nothing else is.
use super::TestContext;
use super::t12_round_lane_e2e::{T12Chain, t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::headers::HeaderStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_v2::{PalwPanelDrawPolicyV1, PalwPanelStakeDrawV1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwVoidReasonV2,
};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// testnet-12 with harness cards; `armed = false` is the fence-off twin (the fence and C7 unset, the
/// bundle's mirrors re-synced to the dormant values), exactly as the v22 skeleton's gate suite builds it.
fn t12(armed: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    assert!(config.params.palw_rcore_plus.is_some_and(|f| f.is_active(0)), "testnet-12 arms R-core+ from genesis");
    if armed {
        return (config, bundle, premine, floats);
    }
    let mut params = config.params.clone();
    params.palw_rcore_plus = None;
    params.palw_rcore_conservative_classes = &[];
    params.sync_palw_rcore_plus();
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("the fence-off twin is a runnable ruleset");
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    let bundle = bundle.clone();
    (config, bundle, premine, floats)
}

/// **SW-1: one resolver, `stake` iff `palw_rcore_plus` at the anchor.** And nothing else about the
/// policy moves with it: the armed policy with `stake` cleared is the twin's, field for field.
#[tokio::test]
async fn sw1_the_one_resolver_sets_stake_iff_rcore_plus_is_active_at_the_anchor() {
    let armed = {
        let (config, ..) = t12(true);
        TestContext::new(TestConsensus::new(&config))
    };
    let off = {
        let (config, ..) = t12(false);
        TestContext::new(TestConsensus::new(&config))
    };
    for anchor_daa in [0u64, 1, 20, 7_000, 1_000_000] {
        let on: PalwPanelDrawPolicyV1 = armed.consensus.virtual_processor().palw_panel_draw_policy_at(anchor_daa);
        let twin: PalwPanelDrawPolicyV1 = off.consensus.virtual_processor().palw_panel_draw_policy_at(anchor_daa);
        assert_eq!(on.stake, Some(PalwPanelStakeDrawV1::V1), "anchor {anchor_daa}: testnet-12 draws by stake");
        assert_eq!(twin.stake, None, "anchor {anchor_daa}: the fence-off twin draws by ADR-0130's lottery");
        assert_eq!(PalwPanelDrawPolicyV1 { stake: None, ..on }, twin, "anchor {anchor_daa}: nothing else in the policy moves");
    }
    assert_eq!(PalwPanelStakeDrawV1::V1.weight_cap_msk, 1_000_000);
    assert_eq!(PalwPanelStakeDrawV1::V1.eligible_floor_permille, 875);
    for (net, params) in [
        ("testnet-11", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11))),
        ("devnet", Params::from(NetworkId::new(NetworkType::Devnet))),
        ("mainnet", Params::from(NetworkId::new(NetworkType::Mainnet))),
    ] {
        assert!(params.palw_rcore_plus_fence().is_none(), "{net} carries no palw_rcore_plus: its draw is today's");
    }
}

/// One floor claim on a fresh testnet-12 chain, bound by the chain; the anchor block, its parent
/// state, the stored panel, and the heartbeat that stood at the claim's slot.
struct Bound {
    chain: T12Chain,
    claim_id: Hash64,
    anchor: BlockHash,
    anchor_daa: u64,
    parent: PalwChainStateV2,
    object: Obj,
    beat_at_slot: BlockHash,
}

/// Card 0's attempt, then heartbeats until one stands at (or past) the claim's anchor slot. Armed,
/// that heartbeat binds nothing (finding 2: past `palw_rcore_plus` a panel anchors only on an attempt
/// block) and card 7's attempt right after it binds the claim; on the fence-off twin the heartbeat
/// binds it. Keeps the anchor block's parent state, so its derivation can be re-run on exactly the
/// state the chain ran it on.
async fn bind_one_floor_claim(armed: bool) -> Bound {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12(armed);
    let ttpb = config.params.target_time_per_block();
    let anchor_delay = bundle.panel.anchor_delay();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    chain.heartbeat(ttpb, Vec::new()).await;
    let (_, claim_id) = chain.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let accepted_daa = chain.tip_state().1.claim(&claim_id).expect("the attempt block made its claim").accepted_daa;
    let slot = accepted_daa + anchor_delay;
    let mut at_slot = None;
    for _ in 0..(4 * anchor_delay + 400) {
        let (_, parent) = chain.tip_state();
        let beat = chain.heartbeat(ttpb, Vec::new()).await;
        if beat.header.daa_score >= slot {
            at_slot = Some((beat, parent));
            break;
        }
        assert_eq!(chain.tip_state().1.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Provisional, "below the slot");
    }
    let (beat, before_beat) = at_slot.expect("the chain reaches the slot");
    let (_, after_beat) = chain.tip_state();
    let (anchor_block, parent) = if armed {
        assert_eq!(
            after_beat.claim(&claim_id).unwrap().phase,
            PalwClaimPhaseV2::Provisional,
            "finding 2: past palw_rcore_plus a heartbeat at the slot does not anchor a panel"
        );
        let (block, _) = chain.attempt(7, ttpb, Vec::new(), &|_| true).await;
        (block, after_beat)
    } else {
        (beat.clone(), before_beat)
    };
    let (_, state) = chain.tip_state();
    let claim = state.claim(&claim_id).expect("the claim stays");
    let PalwClaimPhaseV2::PanelBound { bound_daa } = claim.phase else {
        panic!("armed {armed}: the anchor block binds the claim; it is {:?}", claim.phase)
    };
    assert_eq!(bound_daa, anchor_block.header.daa_score, "bound by the block that folded it");
    assert!(bound_daa >= slot, "at or past the anchor slot");
    let panel = state.panel(&claim_id).expect("a bound claim has a panel").clone();
    assert_eq!(panel.anchor, anchor_block.header.hash, "SW-8: the panel names the block that bound it as its anchor");
    assert_eq!(panel.seats.len(), bundle.panel.seat_count() as usize, "a full jury");
    let object = Obj::PanelBound { claim: claim_id, anchor: anchor_block.header.hash, seats: panel.seats };
    Bound { chain, claim_id, anchor: anchor_block.header.hash, anchor_daa: bound_daa, parent, object, beat_at_slot: beat.header.hash }
}

/// **SW-8 / T89 on a real testnet-12 chain, and the genesis state binds under `stake: Some`.**
///
/// Build = accept = fold, on the one state: the chain's derivation for the anchor block, re-run on
/// the anchor's parent state, is exactly the panel the fold stored; the acceptance walk at the
/// anchor block's own chain point takes it whole. Then the same object, offered by the next chain
/// block inside the bind window, is refused by the gate: under the stake draw a panel binds only in
/// its own anchor block. On the fence-off twin the same chain binds by ADR-0130's lottery and the
/// next block may still carry the binding, as it always could.
#[tokio::test]
async fn t12_genesis_binds_under_the_stake_draw() {
    for armed in [true, false] {
        let mut bound = bind_one_floor_claim(armed).await;
        let vp = bound.chain.vp();
        let bundle = bound.chain.bundle.clone();
        let policy = vp.palw_panel_draw_policy_at(bound.anchor_daa);
        assert_eq!(policy.stake.is_some(), armed, "the anchor's policy");

        // Build: the chain's own derivation for the anchor block, on its parent state.
        let derived = vp.palw_v2_derived_panel_bindings_for_tests(&bound.parent, bound.anchor, bound.anchor_daa);
        assert_eq!(derived, vec![bound.object.clone()], "armed {armed}: build = fold");

        // Accept: the walk at the anchor block's own chain point, on the same parent.
        let header = vp.headers_store.get_header(bound.anchor).unwrap();
        let point = PalwBlockContextV2 {
            block: bound.anchor,
            daa_score: bound.anchor_daa,
            blue_score: header.blue_score,
            subsidy: vp.coinbase_manager.calc_block_subsidy(bound.anchor_daa),
        };
        let accepted = vp.palw_v2_accepted_objects_for_tests(&bound.parent, &bundle.state, &point, derived.clone(), bound.anchor);
        assert_eq!(accepted, derived, "armed {armed}: accept = build");

        // A later chain block, inside the bind window, offering the same binding on the same state.
        let ttpb = bound.chain.config.params.target_time_per_block();
        let later = bound.chain.heartbeat(ttpb, Vec::new()).await;
        let later_point = PalwBlockContextV2 {
            block: later.header.hash,
            daa_score: later.header.daa_score,
            blue_score: later.header.blue_score,
            subsidy: 0,
        };
        assert!(
            later.header.daa_score <= bound.parent.claim(&bound.claim_id).unwrap().bind_base_daa() + bundle.state.window_bind(),
            "the later block is inside the bind window"
        );
        let verdict = vp.palw_v2_validate_objects(&bound.parent, &bundle.state, &later_point, std::slice::from_ref(&bound.object));
        if armed {
            let why = verdict.expect_err("SW-8: a later block cannot bind the claim");
            assert!(why.contains("only in its own anchor block"), "{why}");
        } else {
            assert_eq!(verdict, Ok(()), "the fence-off twin keeps today's bind window");
        }
        eprintln!(
            "[t12-sw] armed {armed}: claim {} bound at DAA {} in its anchor block; seats {:?}",
            bound.claim_id,
            bound.anchor_daa,
            match &bound.object {
                Obj::PanelBound { seats, .. } =>
                    seats.iter().map(|s| bound.chain.bonds.iter().position(|b| *b == s.bond).unwrap()).collect::<Vec<_>>(),
                _ => unreachable!(),
            }
        );
    }
}

/// **Finding 3 (DL-1's exact void DAA) and finding 2's lane rule, on the processor's own extras.**
///
/// The anchor block's own point resolves `sw8_anchor_delay` to the panel's anchor delay (armed) — the
/// heartbeat at the slot, and every point on the fence-off twin, to `None`. Folded on its parent
/// state WITHOUT its binding (what the chain folds when the draw, the gate or the fold refused it),
/// the anchor block voids the claim `BindTimeout` at its own DAA: no bond's collateral or slash
/// moves, and the deadline index is the claims' recomputed deadlines. With its binding the same fold
/// binds the claim. The fence-off twin folded without its binding keeps the claim `Provisional`.
#[tokio::test]
async fn sw8_the_anchor_block_voids_a_claim_it_does_not_bind() {
    for armed in [true, false] {
        let bound = bind_one_floor_claim(armed).await;
        let vp = bound.chain.vp();
        let bundle = bound.chain.bundle.clone();
        let point_of = |block: BlockHash| {
            let header = vp.headers_store.get_header(block).unwrap();
            PalwBlockContextV2 {
                block,
                daa_score: header.daa_score,
                blue_score: header.blue_score,
                subsidy: vp.coinbase_manager.calc_block_subsidy(header.daa_score),
            }
        };
        let point = point_of(bound.anchor);
        assert_eq!(
            vp.palw_sw8_anchor_delay_for(&point),
            armed.then(|| bundle.panel.anchor_delay()),
            "armed {armed}: the anchor block carries step 4c's delay"
        );
        assert_eq!(vp.palw_sw8_anchor_delay_for(&point_of(bound.beat_at_slot)), None, "armed {armed}: a heartbeat never does");

        let unbound = vp.palw_v2_fold_accepted_for_tests(&bound.parent, &bundle.state, &point, &[]).expect("the anchor block folds");
        let phase = unbound.claim(&bound.claim_id).unwrap().phase.clone();
        if armed {
            assert_eq!(
                phase,
                PalwClaimPhaseV2::Voided { voided_daa: bound.anchor_daa, reason: PalwVoidReasonV2::BindTimeout },
                "the anchor block voids the claim it did not bind, at its own DAA"
            );
            for (key, bond) in bound.parent.bonds_iter() {
                let after = unbound.bond(key).unwrap();
                assert_eq!((after.collateral, after.slashed), (bond.collateral, bond.slashed), "S0: no forfeit on {key:?}");
            }
            unbound.assert_deadline_consistency(&bundle.state).expect("the index is the claims' recomputed deadlines");
        } else {
            assert_eq!(phase, PalwClaimPhaseV2::Provisional, "the fence-off twin waits out the bind window");
        }
        let with_binding = vp
            .palw_v2_fold_accepted_for_tests(&bound.parent, &bundle.state, &point, std::slice::from_ref(&bound.object))
            .expect("the anchor block folds its binding");
        assert!(
            matches!(with_binding.claim(&bound.claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }),
            "armed {armed}: build = fold"
        );
    }
}
