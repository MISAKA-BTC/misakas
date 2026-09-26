//! **Lane V02 at the processor, on testnet-12: `Params::palw_final_lock_full_collateral` armed at a
//! height a real chain crosses (DAA 60).**
//!
//! testnet-12 as released with the fence armed at [`FENCE`] — a copy, as the operator's post-launch
//! build will carry it. The chain is `t12_round_lane_e2e`'s harness (harness keys on the eight cards,
//! the premine imported, the EVM lane inert): heartbeats, card 0's floor attempts, card 7's attempt
//! blocks as their anchors. PoW is skipped and nothing else is.
//!
//! The V02 scenario is the parent state of each anchor block with every genesis seat already standing
//! behind post-`Final` locks past its 500‰ ceiling — the accumulated locks of retired claims, written
//! through the carriage as a Final leaves them once its claim row is gone (a real chain needs thousands
//! of Finals for that). On it, the processor's OWN derivation (anchor walk, stake draw, the seat filter
//! the processor resolves at the binding block) and the processor's fold:
//!
//! * **below the height** (claim A's anchor block, DAA < 60): the draw seats nobody, the anchor block
//!   voids the claim `BindTimeout` — the escrow the launch chain burns;
//! * **from the height** (claim B's anchor block, DAA ≥ 60): the draw seats a full panel from the same
//!   lock-heavy seats and the fold binds it; each seat's ledger then stands past 500‰ of its
//!   collateral and never past 100%, and every lock is still in the ledger at its full amount (the
//!   locks' conviction route is `rcore_v02_final_lock_budget`'s, at the fold). The launch build's fold
//!   of the same binding leaves it inert and voids the claim at the same anchor block.
//! * **the processor carries the fence it was built with** (the bundle's mirror), the launch twin none.
use super::t12_round_lane_e2e::{T12Chain, t12_genesis_chain, t12_with_harness_cards};
use crate::model::stores::headers::HeaderStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_var_v1::PalwSlashableLockV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwStateCarriageV2,
    PalwStateParamsV2, PalwVoidReasonV2, palw_accuser_exposure_v1, palw_bond_committed_v1, palw_bond_resolved_locks_v1,
    palw_second_clock_depth_v1,
};
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// The height the chain crosses: above claim A's anchor block, at or below claim B's.
const FENCE: u64 = 60;
/// Claim A is accepted at or past this DAA (its anchor block, `anchor_delay` = 20 later, is below [`FENCE`]).
const A_DAA: u64 = 12;
/// Claim B is accepted at or past this DAA (its anchor block is past [`FENCE`]).
const B_DAA: u64 = 45;
/// What each seat's post-`Final` locks stand at: its 500‰ ceiling plus this, in MSK.
const PAST_THE_CEILING_MSK: u128 = 100_000;
const MSK: u128 = 100_000_000;

/// testnet-12 with harness cards and lane V02's fence armed at [`FENCE`], re-mirrored, as the
/// operator's build assembles it.
fn t12_armed() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, _, premine, floats) = t12_with_harness_cards();
    assert_eq!(config.params.palw_final_lock_full_collateral, None, "testnet-12 ships the fence dormant");
    let mut params = config.params.clone();
    params.palw_final_lock_full_collateral = Some(ForkActivation::new(FENCE));
    params.sync_palw_final_lock_full_collateral();
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("the armed copy is a runnable ruleset");
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    let bundle = bundle.clone();
    (config, bundle, premine, floats)
}

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

/// Heartbeats until the sink's DAA reaches `daa`.
async fn beat_to(chain: &mut T12Chain, daa: u64) {
    let ttpb = chain.config.params.target_time_per_block();
    for _ in 0..(4 * daa + 400) {
        if chain.daa_of(chain.sink()) >= daa {
            return;
        }
        chain.heartbeat(ttpb, Vec::new()).await;
    }
    panic!("the chain reaches DAA {daa}");
}

/// A floor claim by card 0 accepted at or past `from`, and its anchor block (card 7's attempt at the
/// claim's slot) with that block's parent state.
async fn claim_and_anchor(chain: &mut T12Chain, from: u64) -> (Hash64, BlockHash, PalwChainStateV2) {
    beat_to(chain, from).await;
    let ttpb = chain.config.params.target_time_per_block();
    let (_, claim_id) = chain.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let slot =
        chain.tip_state().1.claim(&claim_id).expect("the attempt made its claim").bind_base_daa() + chain.bundle.panel.anchor_delay();
    beat_to(chain, slot).await;
    let (_, parent) = chain.tip_state();
    assert_eq!(parent.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Provisional, "unbound until its anchor block");
    let (anchor, _) = chain.attempt(7, ttpb, Vec::new(), &|_| true).await;
    (claim_id, anchor.header.hash, parent)
}

/// `committed` as the processor's gates read it at `daa` (the raw depth escaped on the state's ring).
fn committed(chain: &T12Chain, s: &PalwChainStateV2, bond: &PalwBondKeyV2, daa: u64) -> u128 {
    let wc = chain.bundle.state.window_court();
    let depth = palw_second_clock_depth_v1(chain.config.params.palw_settled_anchor_depth, s.recent_anchor_daas(), daa, wc);
    palw_bond_committed_v1(s, bond, daa, depth, wc)
}

fn collateral(s: &PalwChainStateV2, bond: &PalwBondKeyV2) -> u128 {
    s.bond(bond).expect("a genesis card").collateral as u128
}

fn ceiling(chain: &T12Chain, s: &PalwChainStateV2, bond: &PalwBondKeyV2) -> u128 {
    collateral(s, bond) * chain.bundle.state.fp_max_exposure_ratio_permille() as u128 / 1000
}

/// `s` with every genesis card standing behind one retired claim's lock that puts its ledger at its
/// ceiling + [`PAST_THE_CEILING_MSK`] at `daa` — rebuilt through the carriage and loaded by the
/// node's own loader checks.
fn with_post_final_locks(chain: &T12Chain, s: &PalwChainStateV2, daa: u64, retired: Hash64) -> PalwChainStateV2 {
    assert!(s.claim(&retired).is_none(), "a retired claim");
    let fills: Vec<(PalwBondKeyV2, u128)> = chain
        .bonds
        .iter()
        .map(|k| (*k, (ceiling(chain, s, k) + PAST_THE_CEILING_MSK * MSK).saturating_sub(committed(chain, s, k, daa))))
        .collect();
    let settled = s.settled_attempt_finals();
    let expiry = daa + chain.bundle.state.window_court();
    let mut carriage = PalwStateCarriageV2::from_state(s);
    for (seat, amount) in fills {
        carriage.slashable_locks.insert(
            (seat, retired),
            PalwSlashableLockV1 {
                claim: retired,
                amount,
                expiry_daa: expiry,
                settled_at_final: settled,
                attested: PalwSegmentMaskV2::NONE,
                segments: 0,
            },
        );
    }
    let loaded = carriage
        .into_state_v3(
            &chain.bundle.state,
            None,
            chain.config.params.palw_uncertified_weightless.is_some_and(|f| f.is_active(daa)),
            chain.config.params.palw_canonical_work_daa(),
        )
        .expect("the rebuilt carriage is consistent");
    for k in &chain.bonds {
        assert!(committed(chain, &loaded, k, daa) > ceiling(chain, &loaded, k), "the premise: post-Final locks pass the 500‰ ceiling");
    }
    loaded
}

fn launch_state_params(chain: &T12Chain) -> PalwStateParamsV2 {
    chain.bundle.state.clone().with_final_lock_full_collateral_from_daa(None)
}

/// **The processor folds with the fence it was built with** — the bundle's mirror.
#[tokio::test]
async fn v02_the_processor_carries_the_fence_it_was_built_with() {
    let (config, bundle, premine, floats) = t12_armed();
    let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let sp = chain.vp().palw_state_params_v2.clone().expect("testnet-12 is ConsensusV2");
    assert_eq!(sp.final_lock_full_collateral_from_daa(), Some(FENCE), "the processor's state params carry the fence");
    assert!(!sp.final_lock_full_collateral_active_at(FENCE - 1) && sp.final_lock_full_collateral_active_at(FENCE));
    let (launch, launch_bundle, premine, floats) = t12_with_harness_cards();
    let chain = t12_genesis_chain(&launch, &launch_bundle, &premine, &floats);
    assert_eq!(chain.vp().palw_state_params_v2.clone().unwrap().final_lock_full_collateral_from_daa(), None, "the launch build: none");
}

/// **The crossing** (see the module doc).
#[tokio::test]
async fn v02_lock_heavy_seats_bind_from_the_fence_and_void_below_it() {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_armed();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let vp = chain.vp();
    let armed_sp = chain.bundle.state.clone();
    let launch_sp = launch_state_params(&chain);

    // ---- Claim A: anchored BELOW the height ----
    let (claim_a, anchor_a, parent_a) = claim_and_anchor(&mut chain, A_DAA).await;
    let point_a = point_of(&chain, anchor_a);
    assert!(point_a.daa_score < FENCE, "the premise: claim A's anchor block is below the fence ({})", point_a.daa_score);
    assert_eq!(vp.palw_sw8_anchor_delay_for(&point_a), Some(chain.bundle.panel.anchor_delay()), "an anchor block");
    // On the chain as it is (no locks) the processor bound it: the derivation is live.
    assert!(
        matches!(chain.tip_state().1.claim(&claim_a).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }),
        "the clean chain binds A"
    );
    let heavy_a = with_post_final_locks(&chain, &parent_a, point_a.daa_score, Hash64::from_u64_word(0x0E_7A00));
    let derived_a = vp.palw_v2_derived_panel_bindings_for_tests(&heavy_a, anchor_a, point_a.daa_score);
    assert!(
        !derived_a.iter().any(|o| matches!(o, Obj::PanelBound { claim, .. } if *claim == claim_a)),
        "below the height the draw seats nobody on lock-heavy seats: {derived_a:?}"
    );
    let folded_a = vp.palw_v2_fold_accepted_for_tests(&heavy_a, &armed_sp, &point_a, &derived_a).expect("the anchor block folds");
    assert_eq!(
        folded_a.claim(&claim_a).unwrap().phase,
        PalwClaimPhaseV2::Voided { voided_daa: point_a.daa_score, reason: PalwVoidReasonV2::BindTimeout },
        "below the height the anchor block voids the claim it cannot bind (V02)"
    );

    // ---- Claim B: anchored FROM the height ----
    let (claim_b, anchor_b, parent_b) = claim_and_anchor(&mut chain, B_DAA).await;
    let point_b = point_of(&chain, anchor_b);
    assert!(point_b.daa_score >= FENCE, "the premise: claim B's anchor block is past the fence ({})", point_b.daa_score);
    let heavy_b = with_post_final_locks(&chain, &parent_b, point_b.daa_score, Hash64::from_u64_word(0x0E_7B00));
    let derived_b = vp.palw_v2_derived_panel_bindings_for_tests(&heavy_b, anchor_b, point_b.daa_score);
    let seats = match derived_b.iter().find(|o| matches!(o, Obj::PanelBound { claim, .. } if *claim == claim_b)) {
        Some(Obj::PanelBound { seats, .. }) => seats.clone(),
        _ => panic!("from the height the draw seats a panel on the same lock-heavy seats: {derived_b:?}"),
    };
    assert_eq!(seats.len(), chain.bundle.panel.seat_count() as usize, "a full jury");
    let bound = vp.palw_v2_fold_accepted_for_tests(&heavy_b, &armed_sp, &point_b, &derived_b).expect("the anchor block folds");
    assert_eq!(
        bound.claim(&claim_b).unwrap().phase,
        PalwClaimPhaseV2::PanelBound { bound_daa: point_b.daa_score },
        "bound at its anchor"
    );
    for seat in &seats {
        let k = seat.bond;
        let c = committed(&chain, &bound, &k, point_b.daa_score);
        assert!(c > ceiling(&chain, &bound, &k), "the seat's ledger stands past 500‰");
        assert!(c + palw_accuser_exposure_v1(&bound, &k) <= collateral(&bound, &k), "and never past 100%");
        let wc = chain.bundle.state.window_court();
        assert_eq!(
            palw_bond_resolved_locks_v1(&bound, &k, point_b.daa_score, None, wc),
            bound.slashable_lock(k, Hash64::from_u64_word(0x0E_7B00)).unwrap().amount,
            "the retired claim's lock stands whole in the ledger (slashable against the whole collateral)"
        );
        eprintln!(
            "[t12-v02] seat {}: committed {} MSK = {}‰ of {} MSK after binding claim B at DAA {}",
            chain.bonds.iter().position(|b| *b == k).unwrap(),
            c / MSK,
            c * 1000 / collateral(&bound, &k),
            collateral(&bound, &k) / MSK,
            point_b.daa_score
        );
    }
    // The launch build: the same binding is inert at the same block, and the claim voids there.
    let launch = vp.palw_v2_fold_accepted_for_tests(&heavy_b, &launch_sp, &point_b, &derived_b).expect("folds as launched");
    assert_eq!(
        launch.claim(&claim_b).unwrap().phase,
        PalwClaimPhaseV2::Voided { voided_daa: point_b.daa_score, reason: PalwVoidReasonV2::BindTimeout },
        "without the fence the lock-heavy panel binds nothing and the anchor block voids the claim"
    );
    eprintln!(
        "[t12-v02] claim A anchored at DAA {} (below {FENCE}): voided; claim B anchored at DAA {}: bound by cards {:?}",
        point_a.daa_score,
        point_b.daa_score,
        seats.iter().map(|s| chain.bonds.iter().position(|b| *b == s.bond).unwrap()).collect::<Vec<_>>()
    );
}
