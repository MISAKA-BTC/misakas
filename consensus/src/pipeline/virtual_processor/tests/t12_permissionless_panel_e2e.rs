//! **RFC-0010's production fold through the real virtual processor on testnet-12** (heartbeats, attempt blocks, GHOSTDAG, the
//! lifecycle carrier, IBD and reorg).
//!
//! **These tests bypass `validate_palw_v2`, and say so.** `palw_permissionless_panel_v1` cannot be armed on any network
//! (`consensus/core/tests/rfc0010_permissionless_panel.rs` pins the refusal): the Panel beacon is `BEACON_UNAVAILABLE` and its bias
//! review is external. Here the fence is set on a copy of testnet-12's params at DAA [`FENCE`], mirrored by
//! `sync_palw_permissionless_panel_v1`, and the node resolves a REFERENCE beacon source in place of the release's empty registry
//! (`VirtualStateProcessor::panel_v3_test_beacon`, compiled out of every non-test build) — one `PanelIndependent` source per epoch, the
//! only way to exercise the verification path, since no such Final exists on a real chain. Nothing here is evidence that the
//! permissionless Panel is complete.
//!
//! One script on node A, then replayed on others:
//!
//! 1. card 1's attempt BELOW the fence makes a **legacy** claim `L`; card 0's attempt at the fence makes the **V3** claim `C`; card 2's
//!    attempt at `L`'s anchor slot binds `L` by lane A's own binder — and leaves `C` alone, though `C`'s own slot has passed;
//! 2. the engine seals `C` against its parent checkpoint; a lifecycle **carrier** brings the epoch's certified output (tag 120) in a
//!    heartbeat; the contribution window closes; **a heartbeat block** draws and binds `C`'s Panel — no named operator, no anchor
//!    attempt: the same Panel an attempt block at that point draws (the seed is the claim's seal and the certified output, never the
//!    carrier);
//! 3. a lane-A `PanelBound` of `C` is refused at the gate; a certified-output object is dropped by name below the fence;
//! 4. a second node fed A's blocks (IBD) reaches A's PALW state root block by block, and the tip's carriage reloads under it;
//! 5. a third node sees A's chain, then a heavier sibling branch that binds `C` on another carrier, then A's again: at every switch
//!    its engine state is the one a fresh replay of that branch commits, and `C`'s Panel is the same on both.
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, sign_spend, t12_genesis_chain, t12_with_harness_cards};
use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_beacon_v1::challenge::policy::reference_policy_v1;
use kaspa_consensus_core::palw_panel_beacon_v1::challenge::{
    FinalPathV1, PostCommitChallengePolicyV1, WorkBeaconStateV1, WorkFinalEventV1, WorkSourceKindV1, collect_work_beacon_v1,
};
use kaspa_consensus_core::palw_panel_beacon_v1::{panel_beacon_context_v1, panel_beacon_scheme_of_v1};
use kaspa_consensus_core::palw_permissionless_panel_v1::{
    BeaconProofV1, BeaconRequestV1, BondIdV1, ClaimPhaseV3, PalwPermissionlessPanelV1, PanelPolicyV1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwPanelV3BeaconSourceV1, PalwStateCarriageV2,
};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use std::collections::BTreeSet;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// The first DAA a claim is accepted under the V3 rule.
const FENCE: u64 = 30;
const PROFILE: [u8; 64] = [0x77; 64];
/// Epochs the reference history covers (period 40: epoch `e` releases at `40 e`).
const EPOCHS: u64 = 8;

fn challenge_policy() -> PostCommitChallengePolicyV1 {
    reference_policy_v1(1, 1, 10, 1, 1)
}

fn engine_policy() -> PanelPolicyV1 {
    PanelPolicyV1 {
        seal_depth_blocks: 2,
        seal_wait_daa: 40,
        bond_maturity_daa: 1,
        beacon_period_daa: 40,
        beacon_wait_daa: 12,
        assignment_delay_daa: 1,
        receipt_window_daa: 12,
        seat_count: 5,
        outsider_seats: 0,
        max_retries: 1,
        min_collateral: 1,
        max_candidates: 64,
        max_pending: 64,
        max_pending_per_bond: 16,
        max_assignments_per_block: 8,
        max_admissions_per_block: 8,
        max_tracked_claims: 256,
        max_beacons_per_block: 2,
        max_beacon_proof_bytes: 4096,
        beacon_scheme: panel_beacon_scheme_of_v1(&challenge_policy()),
    }
}

/// One independent source per epoch: accepted one DAA after the epoch's release, settled one later (the contract's start `S` is
/// `release + 1`; its window ten wide; the lock one deeper).
fn reference_source() -> PalwPanelV3BeaconSourceV1 {
    let events = (1..=EPOCHS)
        .map(|e| WorkFinalEventV1 {
            kind: WorkSourceKindV1::RealUsefulWork,
            source_profile_id: PROFILE,
            canonical_work_id: [e as u8; 64],
            execution_commitment: [e as u8; 64],
            accepted_position: 40 * e + 1,
            settlement_position: 40 * e + 2,
            occurrence_index: 0,
            claim_final: true,
            da_satisfied: true,
            validity_independent: true,
            depends_on_profiles: Vec::new(),
            final_path: FinalPathV1::PanelIndependent,
        })
        .collect();
    PalwPanelV3BeaconSourceV1::Reference { events, eligible_profiles: BTreeSet::from([PROFILE]) }
}

/// testnet-12 with harness cards and the permissionless Panel armed at [`FENCE`] on a copy of the params (see the module doc).
///
/// The fence is set AFTER `ConfigBuilder::build`, because `build` runs `validate_palw_v2` and panics — as it must, on every real
/// network — at an armed `palw_permissionless_panel_v1` (the refusal is `rfc0010_permissionless_panel.rs`'s). `Config.params` is a
/// public field: this is the bypass, and it is spelled out here and nowhere else.
fn armed(fence: Option<u64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (mut config, bundle, premine, floats) = t12_with_harness_cards();
    let Some(at) = fence else { return (config, bundle, premine, floats) };
    config.params.palw_permissionless_panel_v1 = Some(PalwPermissionlessPanelV1 { activation: ForkActivation::new(at), policy: engine_policy() });
    config.params.sync_palw_permissionless_panel_v1();
    assert!(config.params.validate_palw_v2().is_err(), "the real validator still refuses what this test bypasses");
    let PalwConsensusMode::ConsensusV2(armed) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    assert!(armed.state.panel_v3().is_some(), "the mirror is set");
    let bundle = armed.clone();
    (config, bundle, premine, floats)
}

fn node(config: &Config, bundle: &PalwConsensusParamsV2, premine: &Premine, floats: &Premine) -> T12Chain {
    let chain = t12_genesis_chain(config, bundle, premine, floats);
    // The node resolves the reference source: one approved scheme, one independent source per epoch.
    *chain.vp().panel_v3_test_beacon.lock() = Some((vec![challenge_policy()], reference_source()));
    chain
}

fn ttpb(chain: &T12Chain) -> u64 {
    chain.config.params.target_time_per_block()
}

async fn beat_to(chain: &mut T12Chain, daa: u64) -> Option<Block> {
    let mut last = None;
    while chain.daa_of(chain.sink()) < daa {
        last = Some(chain.heartbeat(ttpb(chain), Vec::new()).await);
    }
    last
}

fn state_of(chain: &T12Chain) -> PalwChainStateV2 {
    chain.tip_state().1
}

/// Every selected-chain block of `chain` from genesis (exclusive) to `upto` (inclusive), oldest first.
fn chain_blocks(chain: &T12Chain, upto: BlockHash) -> Vec<Block> {
    let vp = chain.vp();
    let genesis = chain.config.params.genesis.hash;
    let mut hashes = Vec::new();
    let mut at = upto;
    while at != genesis {
        hashes.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    hashes.reverse();
    hashes.into_iter().map(|hash| chain.ctx.consensus.get_block(hash).expect("the node holds every block of its chain")).collect()
}

async fn arrive(chain: &T12Chain, block: Block, what: &str) {
    let hash = block.header.hash;
    chain
        .ctx
        .consensus
        .validate_and_insert_block(block)
        .virtual_state_task
        .await
        .unwrap_or_else(|e| panic!("{what} {hash} was refused: {e}"));
}

/// The certified output of the epoch `claim` was sealed to: what a producer builds from the epoch's one independent source.
fn proof_for(bundle: &PalwConsensusParamsV2, epoch: u64) -> BeaconProofV1 {
    let mirror = *bundle.state.panel_v3().expect("the mirror");
    let release = epoch * mirror.policy.beacon_period_daa;
    let request = BeaconRequestV1 {
        network: mirror.network,
        ruleset: mirror.ruleset,
        scheme: mirror.policy.beacon_scheme,
        epoch,
        release_daa: release,
        deadline_daa: release + mirror.policy.beacon_wait_daa,
    };
    let context = panel_beacon_context_v1(&request, &challenge_policy(), BTreeSet::from([PROFILE]));
    let PalwPanelV3BeaconSourceV1::Reference { events, .. } = reference_source() else { unreachable!() };
    let WorkBeaconStateV1::Locked(beacon) = collect_work_beacon_v1(&context, &events, release + 5).expect("a valid policy") else {
        panic!("the reference history locks epoch {epoch}");
    };
    BeaconProofV1 { epoch, output: Hash64::from_bytes(beacon.output), proof: borsh::to_vec(beacon.beacon()).unwrap() }
}

/// A lifecycle carrier bringing `object`, paid from card 0's fee float.
fn carrier(config: &Config, floats: &Premine, object: Obj) -> Transaction {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
    let (float_outpoint, float_entry) = floats[0].clone();
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(float_outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(float_entry.amount - 300_000, card_payout_spk(0))],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    sign_spend(&mut tx, float_entry, 0, config.params.storage_mass_parameter);
    tx
}

struct Run {
    a: T12Chain,
    config: Config,
    bundle: PalwConsensusParamsV2,
    premine: Premine,
    floats: Premine,
    legacy: Hash64,
    v3: Hash64,
    /// The block before the one that bound `v3`, and that block.
    bind_parent: BlockHash,
    bind_block: Block,
}

/// The script of the module doc, steps 1–2, on node A.
async fn run_a() -> Run {
    let (config, bundle, premine, floats) = armed(Some(FENCE));
    let mut a = node(&config, &bundle, &premine, &floats);
    // 1. A legacy claim below the fence, a V3 claim at it.
    beat_to(&mut a, FENCE - 12).await;
    let (_, legacy) = a.attempt(1, ttpb(&a), Vec::new(), &|_| true).await;
    beat_to(&mut a, FENCE).await;
    let (_, v3) = a.attempt(0, ttpb(&a), Vec::new(), &|_| true).await;
    {
        let state = state_of(&a);
        let (l, c) = (state.claim(&legacy).unwrap(), state.claim(&v3).unwrap());
        assert!(l.accepted_daa < FENCE && c.accepted_daa >= FENCE, "accepted {} / {}", l.accepted_daa, c.accepted_daa);
        let engine = state.panel_v3().expect("the engine exists past the fence");
        assert!(engine.claim_rows().contains_key(&v3) && !engine.claim_rows().contains_key(&legacy), "rule by acceptance");
        assert_eq!(c.phase, PalwClaimPhaseV2::Provisional);
        assert_eq!(state.deadline_of(&v3), None, "the engine is the V3 claim's clock");
        assert!(state.deadline_of(&legacy).is_some(), "the legacy claim keeps its V2 bind window");
    }
    // Card 2's attempt at the legacy claim's anchor slot: lane A's own binder binds `legacy` and — the V3 claim's slot has passed
    // too — does not touch it.
    let slot = state_of(&a).claim(&legacy).unwrap().bind_base_daa() + a.bundle.panel.anchor_delay();
    beat_to(&mut a, slot).await;
    a.attempt(2, ttpb(&a), Vec::new(), &|_| true).await;
    {
        let state = state_of(&a);
        assert!(matches!(state.claim(&legacy).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "lane A binds the legacy claim");
        assert!(
            !matches!(state.claim(&v3).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }),
            "no lane-A anchor binds a V3 claim, whatever its slot"
        );
    }
    // 2. Seal, certified output by a carrier in a heartbeat, window closes, a heartbeat draws and binds.
    let epoch = {
        let state = state_of(&a);
        let record = state.panel_v3().unwrap().claim_rows().get(&v3).unwrap().clone();
        let seal = record.seal.clone().unwrap_or_else(|| panic!("sealed by now: {:?}", record.phase));
        seal.beacon_epoch
    };
    let release = epoch * engine_policy().beacon_period_daa;
    beat_to(&mut a, release + 4).await;
    let proof = proof_for(&bundle, epoch);
    {
        // The gate's verdict on the object at the point the carrying block will have.
        let (tip, state) = a.tip_state();
        let point = PalwBlockContextV2 { block: tip, daa_score: a.daa_of(tip) + 1, blue_score: 1_000_000, subsidy: 0 };
        let object = Obj::PanelBeaconProofV3 { proof: Box::new(proof.clone()) };
        let verdict = a.vp().palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(&object));
        eprintln!("[e2e] epoch {epoch} release {release}; tip DAA {}; gate: {verdict:?}", a.daa_of(tip));
    }
    let carried = a.heartbeat(ttpb(&a), vec![carrier(&config, &floats, Obj::PanelBeaconProofV3 { proof: Box::new(proof) })]).await;
    assert_eq!(carried.transactions.len(), 2, "the carrier rides the heartbeat");
    // A block's transactions are accepted by its selected child: the next block folds the object.
    a.heartbeat(ttpb(&a), Vec::new()).await;
    assert!(state_of(&a).panel_v3().unwrap().beacon_rows().contains_key(&epoch), "the certified output is retained");
    // The contribution window closes at `release + wait`; the first block past it draws and binds. It is a HEARTBEAT.
    beat_to(&mut a, release + 12).await;
    let (mut bind_parent, mut bind_block);
    loop {
        bind_parent = a.sink();
        bind_block = a.heartbeat(ttpb(&a), Vec::new()).await;
        if matches!(state_of(&a).panel_v3().unwrap().claim_rows().get(&v3).unwrap().phase, ClaimPhaseV3::Bound(_)) {
            break;
        }
        assert!(a.daa_of(a.sink()) < release + 40, "the claim binds within a window of the draw's point");
    }
    Run { a, config, bundle, premine, floats, legacy, v3, bind_parent, bind_block }
}


/// Fork-choice facts of `blocks` as `chain` holds them (for a failing assertion's context).
fn dump(chain: &T12Chain, blocks: &[(&str, &Block)]) {
    let vp = chain.vp();
    for (name, b) in blocks {
        eprintln!(
            "[e2e] {name}: daa {} status {:?} blue_work {:?} blue_score {}",
            b.header.daa_score,
            chain.ctx.consensus.block_status(b.header.hash),
            vp.ghostdag_store.get_blue_work(b.header.hash).ok(),
            b.header.blue_score
        );
    }
    eprintln!("[e2e] sink {}", chain.sink());
}

fn binding_of(state: &PalwChainStateV2, claim: &Hash64) -> kaspa_consensus_core::palw_permissionless_panel_v1::PanelBoundV3 {
    match &state.panel_v3().unwrap().claim_rows().get(claim).unwrap().phase {
        ClaimPhaseV3::Bound(b) => b.clone(),
        other => panic!("not bound: {other:?}"),
    }
}

#[tokio::test]
async fn t12_a_v3_claim_binds_on_a_heartbeat_with_no_named_operator_while_lane_a_drains_the_legacy_claim() {
    let run = run_a().await;
    let state = state_of(&run.a);
    let binding = binding_of(&state, &run.v3);
    // Bound on the HEARTBEAT block, in its acceptance, from the claim's seal and the certified output.
    assert_eq!(binding.binding_block, run.bind_block.header.hash);
    assert_eq!(run.bind_block.header.pow_algo_id, kaspa_consensus_core::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1);
    assert!(matches!(state.claim(&run.v3).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }));
    let panel = state.panel(&run.v3).expect("a V2 panel record").clone();
    assert_eq!(panel.anchor, binding.panel_seed_v3);
    let seats: Vec<BondIdV1> = binding.seats.clone();
    assert_eq!(seats.len(), 5);
    // A public population: every seat a registered, mature, bonded participant that is not the producer, and each holds the duty.
    let producer = run.a.bonds[0];
    for seat in &panel.seats {
        assert_ne!(seat.bond, producer);
        assert!(state.bond(&seat.bond).is_some());
        assert!(state.panel_duties_of(&run.v3).unwrap().contains_key(&seat.bond));
    }
    // The legacy claim is still lane A's: its panel was drawn from its anchor attempt, not by the engine.
    assert!(state.panel_v3().unwrap().claim_rows().get(&run.legacy).is_none());
    assert!(state.panel(&run.legacy).is_some());

    // The gate: a lane-A `PanelBound` of the V3 claim is refused by name.
    let vp = run.a.vp();
    let (tip, _) = run.a.tip_state();
    let point = PalwBlockContextV2 { block: tip, daa_score: run.a.daa_of(tip), blue_score: 1_000_000, subsidy: 0 };
    let carried = Obj::PanelBound { claim: run.v3, anchor: Hash64::from_u64_word(9), seats: panel.seats.clone() };
    let refused = vp.palw_v2_validate_objects(&state, &run.bundle.state, &point, std::slice::from_ref(&carried));
    assert!(refused.is_err());

    // The node's read (what RPC op 220 serves) is the pure function of this tip's state: the V3 claim reads as bound with the
    // binding's seed and seats, the legacy claim as lane A's, and an unknown id is named, not silently absent.
    let observed = vp.palw_panel_v3_observation_v1_impl(vec![run.v3, run.legacy, Hash64::from_u64_word(77)], 0).expect("a V2 node answers");
    assert_eq!(
        observed,
        kaspa_consensus_core::palw_permissionless_panel_v1::panel_v3_observation_v1(
            &state,
            &run.bundle.state,
            &[run.v3, run.legacy, Hash64::from_u64_word(77)],
            0
        )
    );
    assert_eq!(observed.unknown, vec![Hash64::from_u64_word(77)]);
    let (v3, legacy) = (&observed.claims[0], &observed.claims[1]);
    assert_eq!((v3.rule, v3.engine_phase, v3.v2_phase.as_str()), ("permissionlessV3", Some("bound"), "panelBound"));
    assert_eq!(v3.assignment.as_ref().unwrap().seed, binding.panel_seed_v3);
    assert_eq!(v3.assignment.as_ref().unwrap().binding_block, run.bind_block.header.hash);
    assert_eq!((legacy.rule, legacy.engine_phase), ("historicalLaneA", None));
    assert!(observed.overview.active && observed.overview.bound == observed.overview.tracked_claims, "{:?}", observed.overview);
    let listed = vp.palw_panel_v3_observation_v1_impl(Vec::new(), 0).unwrap();
    assert_eq!(listed.claims.len() as u32, listed.overview.tracked_claims, "no id named: the tracked claims");
    assert!(listed.claims.iter().all(|c| c.rule == "permissionlessV3" && c.claim_id != run.legacy), "the legacy claim is not the engine's");
    assert!(listed.claims.iter().any(|c| c.claim_id == run.v3));
    assert!(observed.to_json().contains("\"permissionlessV3\""));

    // The engine's state and the V2 tables agree, and the tip's carriage reloads under its root.
    state.assert_internal_consistency(&run.bundle.state).expect("consistent");
    state.assert_deadline_consistency(&run.bundle.state).expect("deadlines");
    let reloaded = PalwStateCarriageV2::from_state(&state).into_state(&run.bundle.state, Some(state.state_root())).expect("restart");
    assert_eq!(reloaded, state);
}

/// A second node fed A's blocks (IBD from genesis) holds A's PALW state root after every block, A's engine at the tip, and the
/// binding A recorded.
#[tokio::test]
async fn t12_a_node_that_syncs_the_chain_reaches_the_same_engine_state_block_by_block() {
    let run = run_a().await;
    let b = node(&run.config, &run.bundle, &run.premine, &run.floats);
    for block in chain_blocks(&run.a, run.a.sink()) {
        let hash = block.header.hash;
        arrive(&b, block, "a block of A's chain").await;
        assert_eq!(b.sink(), hash, "B walks A's chain");
        let root_b = b.vp().palw_state_v2_store.read().state_root_of(hash).expect("B recorded the block's root");
        let root_a = run.a.vp().palw_state_v2_store.read().state_root_of(hash).expect("A recorded the block's root");
        assert_eq!(root_a, root_b, "the PALW state root after block {hash}");
    }
    assert_eq!(state_of(&b), state_of(&run.a));
    assert_eq!(binding_of(&state_of(&b), &run.v3), binding_of(&state_of(&run.a), &run.v3));
}

/// **Binder independence and reorg.** Node A2 follows A to the block before the bind, then mines the bind point on another carrier
/// (a heartbeat at another time) and an attempt block after it. PALW fork choice orders by safe frontier, safe weight and live total —
/// a heartbeat adds none of them — so the branch with more live attempt work wins: A2's one attempt beats A's bare bind block, and
/// A's two attempts beat A2's one. A third node sees A, then A2's branch, then A's again: at every switch its PALW state is the one the
/// branch it stands on commits, and `C`'s Panel — seed, seats, exposure — is the same on both branches (only the inclusion witness,
/// `binding_block`, differs).
#[tokio::test]
async fn t12_the_bind_survives_a_reorg_and_the_carrier_does_not_change_the_panel() {
    let mut run = run_a().await;
    let a_state_at_bind = state_of(&run.a);
    let a_binding = binding_of(&a_state_at_bind, &run.v3);

    // A2: a follower of A up to the bind point's parent, which then mines its own carrier and an attempt after it.
    let mut a2 = node(&run.config, &run.bundle, &run.premine, &run.floats);
    for block in chain_blocks(&run.a, run.bind_parent) {
        arrive(&a2, block, "A's chain up to the bind point").await;
    }
    a2.ctx.simulated_time = run.a.ctx.simulated_time;
    assert_eq!(a2.sink(), run.bind_parent);
    let x2 = a2.heartbeat(ttpb(&a2) + 7, Vec::new()).await;
    let a2_binding = binding_of(&state_of(&a2), &run.v3);
    assert_ne!(x2.header.hash, run.bind_block.header.hash, "another carrier");
    assert_eq!(a2_binding.panel_seed_v3, a_binding.panel_seed_v3, "the carrier is not entropy: same seed");
    assert_eq!(a2_binding.seats, a_binding.seats, "…same Panel");
    assert_eq!(a2_binding.exposure, a_binding.exposure);
    assert_ne!(a2_binding.binding_block, a_binding.binding_block, "only the inclusion witness differs");
    let (y2, _) = a2.attempt(3, ttpb(&a2), Vec::new(), &|_| true).await;

    // A grows two attempts, so that its branch carries the more live work.
    let (z1, _) = run.a.attempt(4, ttpb(&run.a), Vec::new(), &|_| true).await;
    let (z2, _) = run.a.attempt(5, ttpb(&run.a), Vec::new(), &|_| true).await;

    // The third node.
    let r = node(&run.config, &run.bundle, &run.premine, &run.floats);
    for block in chain_blocks(&run.a, run.bind_block.header.hash) {
        arrive(&r, block, "A's chain to the bind block").await;
    }
    assert_eq!(r.sink(), run.bind_block.header.hash);
    assert_eq!(state_of(&r), a_state_at_bind, "R stands where A stood at the bind block");
    // A2's branch arrives, carrying one attempt more: R reorgs off the bind block.
    arrive(&r, x2.clone(), "A2's carrier").await;
    arrive(&r, y2.clone(), "A2's attempt").await;
    dump(&r, &[("x(A)", &run.bind_block), ("x2", &x2), ("y2", &y2)]);
    assert_eq!(r.sink(), y2.header.hash, "the branch with the live attempt wins");
    assert_eq!(state_of(&r), state_of(&a2), "R's PALW state is the one A2's branch commits");
    assert_eq!(binding_of(&state_of(&r), &run.v3).binding_block, a2_binding.binding_block);
    assert_eq!(binding_of(&state_of(&r), &run.v3).panel_seed_v3, a_binding.panel_seed_v3);
    // A's branch grows past it: R reorgs back, and the bind is A's again.
    arrive(&r, z1.clone(), "A's first attempt").await;
    arrive(&r, z2.clone(), "A's second attempt").await;
    dump(&r, &[("x(A)", &run.bind_block), ("z1", &z1), ("z2", &z2), ("x2", &x2), ("y2", &y2)]);
    assert_eq!(r.sink(), z2.header.hash, "A's branch carries the most live work again");
    assert_eq!(state_of(&r), state_of(&run.a), "…and R's PALW state is A's again");
    assert_eq!(binding_of(&state_of(&r), &run.v3).binding_block, a_binding.binding_block);
    assert_eq!(binding_of(&state_of(&r), &run.v3).panel_seed_v3, a_binding.panel_seed_v3);
}

/// **A certified-output object below the fence is dropped by name and the block stands.** The gate refuses it by name for the mempool
/// (an older build could not decode it either); a block that nonetheless carries it is accepted whole — its object is dropped first
/// and charged nothing — and no engine exists. Past the fence the gate lets the same object through to the fold.
#[tokio::test]
async fn t12_a_certified_output_carried_below_the_fence_is_dropped_by_name_and_the_block_stands() {
    let (config, bundle, premine, floats) = armed(Some(FENCE));
    let mut a = node(&config, &bundle, &premine, &floats);
    beat_to(&mut a, 6).await;
    let proof = proof_for(&bundle, 1);
    let object = Obj::PanelBeaconProofV3 { proof: Box::new(proof.clone()) };
    let gate_at = |chain: &T12Chain, daa: u64| {
        let (tip, state) = chain.tip_state();
        let point = PalwBlockContextV2 { block: tip, daa_score: daa, blue_score: 1_000_000, subsidy: 0 };
        chain.vp().palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(&object))
    };
    let tip_daa = a.daa_of(a.sink());
    assert!(tip_daa + 1 < FENCE);
    let refused = gate_at(&a, tip_daa + 1).expect_err("the gate names the fence");
    assert!(refused.contains("palw_permissionless_panel_v1"), "{refused}");
    assert!(gate_at(&a, FENCE).is_ok(), "past the fence the fold judges the proof, not the gate");

    let carried = a.heartbeat(ttpb(&a), vec![carrier(&config, &floats, object.clone())]).await;
    assert_eq!(carried.transactions.len(), 2, "the carrier rides the heartbeat");
    let child = a.heartbeat(ttpb(&a), Vec::new()).await;
    assert_eq!(a.sink(), child.header.hash, "the block that accepts the carrier stands");
    assert!(state_of(&a).panel_v3().is_none(), "no engine below the fence; the dropped object left no trace");
    assert!(a.daa_of(child.header.hash) < FENCE);
}
