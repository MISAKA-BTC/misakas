//! **Lane F1 at the processor: a testnet-12 chain that crosses the panel-seed fence mid-run**
//! (`Params::palw_panel_seed_execution`, the post-launch fix for the anchor-identity re-roll,
//! `wf_72c1a397-e23`).
//!
//! testnet-12 launched with the panel seed keyed on the anchor block's identity. The fix arms at a
//! post-launch height `H`, keyed on the claim's ANCHOR DAA (the processor's one anchor walk resolves it,
//! for the chain's own derivation and for the gate alike). One script, run twice on testnet-12 with
//! harness cards — the release's rule (fence dormant) and the fence armed at `H = 3 × anchor_delay`:
//!
//! 1. card 0's attempt makes claim A; card 7's attempt at A's slot anchors it BELOW `H` (and makes
//!    claim C);
//! 2. heartbeats past `H`; card 1's attempt anchors C — a claim whose SLOT is below `H` and whose
//!    anchor is past it — and makes claim X;
//! 3. card 2's attempt at X's slot anchors X past `H`.
//!
//! **Below the fence an armed node IS a released node.** The released chain's own blocks, fed to a node
//! with the fence armed (as a syncing peer feeds them), are accepted with the same PALW state root after
//! every block up to the first anchor past `H`. A's panel is keyed on its anchor block under both rules.
//! **Past it** the armed node keys C's panel on `H(anchor attempt's execution commitment ‖ claim)` —
//! spelled here from the anchor header's own bytes — while the released rule keys it on the block
//! identity; the roots part at that block, and the released chain's NEXT block (which commits the
//! released root as its parent's) is not followed: the split is at the fence, never before it. **An
//! armed node building its own chain** keys C and X on the execution,
//! and **the seed is chain data**: a second armed node fed that chain reaches the same sink, the same
//! state root and the same stored seeds and seats.
use super::t12_round_lane_e2e::{T12Chain, t12_genesis_chain, t12_with_harness_cards};
use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_attempt_v2::{PalwAttemptEnvelopeV2, attempt_id_v2, palw_network_domain_v2_for};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_v2::{palw_panel_anchor_execution_v1, palw_panel_draw_seed_v1};
use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2, PalwPanelSeatV2};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// testnet-12 with harness cards; `fence: Some(h)` arms lane F1 at `h` on a copy of the shipped
/// params, exactly as an operator's post-launch build would (nothing else moves).
fn t12_with_fence(fence: Option<u64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    assert_eq!(config.params.palw_panel_seed_execution, None, "testnet-12 ships lane F1's fence dormant");
    let Some(at) = fence else { return (config, bundle, premine, floats) };
    let mut params = config.params.clone();
    params.palw_panel_seed_execution = Some(ForkActivation::new(at));
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("testnet-12 with lane F1 armed is a runnable ruleset");
    let PalwConsensusMode::ConsensusV2(armed) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    assert_eq!(armed, &bundle, "the fence is a Params field: the bundle does not move");
    (config, bundle, premine, floats)
}

/// One claim's outcome: the anchor block, its DAA, the claim's slot, and what the fold stored.
#[derive(Clone, Debug)]
struct Anchored {
    claim: Hash64,
    slot: u64,
    anchor: Block,
    stored_anchor: Hash64,
    seats: Vec<PalwPanelSeatV2>,
}

struct Crossing {
    chain: T12Chain,
    /// Every chain block in insertion order, with the PALW state root after it.
    blocks: Vec<(BlockHash, u64, Hash64)>,
    a: Anchored,
    c: Anchored,
    x: Anchored,
    /// Index into `blocks` of the first anchor past the fence (card 1's attempt).
    first_past: usize,
}

fn claim_of(block: &Block) -> Hash64 {
    attempt_id_v2(&PalwAttemptEnvelopeV2::decode_wire(&block.header.palw_commitment).expect("an attempt block").attempt)
}

fn record(chain: &T12Chain, blocks: &mut Vec<(BlockHash, u64, Hash64)>, block: &Block) {
    let (tip, state) = chain.tip_state();
    assert_eq!(tip, block.header.hash, "the recorded block is the tip");
    blocks.push((block.header.hash, block.header.daa_score, state.state_root()));
}

fn anchored(chain: &T12Chain, claim: Hash64, slot: u64, anchor: &Block) -> Anchored {
    let (_, state) = chain.tip_state();
    let record = state.claim(&claim).expect("the claim stays");
    let PalwClaimPhaseV2::PanelBound { bound_daa } = record.phase else {
        panic!("claim {claim} is bound by its anchor block; it is {:?}", record.phase)
    };
    assert_eq!(bound_daa, anchor.header.daa_score, "SW-8: bound in its anchor block");
    let panel = state.panel(&claim).expect("a bound claim has a panel");
    Anchored { claim, slot, anchor: anchor.clone(), stored_anchor: panel.anchor, seats: panel.seats.clone() }
}

/// Heartbeats (each recorded) until the chain's DAA reaches `slot`, then card `card`'s attempt — the
/// first attempt block at or past the slot, so `claim`'s anchor (heartbeats anchor nothing past
/// `palw_rcore_plus`, asserted).
async fn attempt_at_slot(
    chain: &mut T12Chain,
    blocks: &mut Vec<(BlockHash, u64, Hash64)>,
    claim: Hash64,
    slot: u64,
    card: usize,
) -> Block {
    let ttpb = chain.config.params.target_time_per_block();
    for _ in 0..(4 * slot + 400) {
        if chain.daa_of(chain.sink()) >= slot {
            break;
        }
        let beat = chain.heartbeat(ttpb, Vec::new()).await;
        record(chain, blocks, &beat);
        assert_eq!(chain.tip_state().1.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Provisional, "a heartbeat anchors nothing");
    }
    assert!(chain.daa_of(chain.sink()) >= slot, "the chain reaches the slot {slot}");
    let (block, _) = chain.attempt(card, ttpb, Vec::new(), &|_| true).await;
    record(chain, blocks, &block);
    assert!(block.header.daa_score >= slot, "the attempt stands at or past the slot");
    block
}

/// The script, on testnet-12 with the fence at `fence` (or dormant), crossing `h`.
async fn cross(fence: Option<u64>, h: u64) -> Crossing {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_with_fence(fence);
    let ttpb = config.params.target_time_per_block();
    let delay = bundle.panel.anchor_delay();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let mut blocks = Vec::new();

    let beat = chain.heartbeat(ttpb, Vec::new()).await;
    record(&chain, &mut blocks, &beat);
    let (made_a, a) = chain.attempt(0, ttpb, Vec::new(), &|_| true).await;
    record(&chain, &mut blocks, &made_a);
    let slot_a = chain.tip_state().1.claim(&a).expect("card 0's attempt made its claim").bind_base_daa() + delay;
    let anchor_a = attempt_at_slot(&mut chain, &mut blocks, a, slot_a, 7).await;
    assert!(anchor_a.header.daa_score < h, "A is anchored BELOW the fence ({} < {h})", anchor_a.header.daa_score);
    let a = anchored(&chain, a, slot_a, &anchor_a);

    // Claim C: card 7's own attempt, the block that anchored A. Its slot is below the fence.
    let c = claim_of(&anchor_a);
    let slot_c = chain.tip_state().1.claim(&c).expect("card 7's attempt made its claim").bind_base_daa() + delay;
    assert!(slot_c < h, "C's slot is below the fence ({slot_c} < {h})");
    for _ in 0..(4 * h + 400) {
        if chain.daa_of(chain.sink()) >= h {
            break;
        }
        let beat = chain.heartbeat(ttpb, Vec::new()).await;
        record(&chain, &mut blocks, &beat);
        assert_eq!(chain.tip_state().1.claim(&c).unwrap().phase, PalwClaimPhaseV2::Provisional, "a heartbeat anchors nothing");
    }
    assert!(chain.daa_of(chain.sink()) >= h, "the chain reaches the fence");
    let first_past = blocks.len();
    let (anchor_c, x) = chain.attempt(1, ttpb, Vec::new(), &|_| true).await;
    record(&chain, &mut blocks, &anchor_c);
    assert!(anchor_c.header.daa_score >= h, "C is anchored PAST the fence");
    let c = anchored(&chain, c, slot_c, &anchor_c);

    let slot_x = chain.tip_state().1.claim(&x).expect("card 1's attempt made its claim").bind_base_daa() + delay;
    let anchor_x = attempt_at_slot(&mut chain, &mut blocks, x, slot_x, 2).await;
    assert!(anchor_x.header.daa_score >= h);
    let x = anchored(&chain, x, slot_x, &anchor_x);
    Crossing { chain, blocks, a, c, x, first_past }
}

fn seed_of(chain: &T12Chain, anchored: &Anchored) -> Hash64 {
    let network = palw_network_domain_v2_for(chain.config.params.net.to_string().as_bytes(), Some(chain.config.params.genesis.hash));
    let execution = palw_panel_anchor_execution_v1(network, &anchored.anchor.header).expect("an attempt block anchors past R-core+");
    palw_panel_draw_seed_v1(&execution, &anchored.claim)
}

/// **The crossing.** Below the fence the armed chain is the released chain (blocks and roots); past it
/// the armed chain keys each panel on the anchor attempt's execution commitment and the claim — the
/// straddling claim included — and the released rule keys it on the block.
#[tokio::test]
async fn a_chain_crossing_the_panel_seed_fence_draws_on_the_block_below_and_the_execution_past_it() {
    let (_, bundle, ..) = t12_with_harness_cards();
    let delay = bundle.panel.anchor_delay();
    assert!(delay >= 2, "the script needs room between two slots");
    let h = 3 * delay;
    let released = cross(None, h).await;
    let armed = cross(Some(h), h).await;

    // ---- the released chain, fed to an ARMED node: identical below the fence --------------------------
    // (Two runs cannot be compared block for block — the harness's heartbeat payout is random — so the
    // released chain's OWN blocks are fed to an armed node, as a syncing peer would feed them.)
    let (config, bundle, premine, floats) = t12_with_fence(Some(h));
    let replay = t12_genesis_chain(&config, &bundle, &premine, &floats);
    for (i, (hash, daa, released_root)) in released.blocks.iter().enumerate().take(released.first_past + 1) {
        let block = released.chain.ctx.consensus.get_block(*hash).expect("the released node holds its chain");
        replay
            .ctx
            .consensus
            .validate_and_insert_block(block)
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("released block #{i} (DAA {daa}) was refused by an armed node: {e}"));
        assert_eq!(replay.sink(), *hash, "released block #{i} (DAA {daa}) is the armed node's sink too");
        let armed_root = replay.tip_state().1.state_root();
        if i < released.first_past {
            assert_eq!(armed_root, *released_root, "block #{i} (DAA {daa}): below the fence the armed node folds the released root");
        } else {
            assert_ne!(armed_root, *released_root, "block #{i} (DAA {daa}): the first anchor past the fence folds another panel");
        }
    }
    let (_, replayed) = replay.tip_state();
    let panel_a = replayed.panel(&released.a.claim).expect("A: the armed node bound it on the released chain");
    assert_eq!(
        (panel_a.anchor, &panel_a.seats),
        (released.a.anchor.header.hash, &released.a.seats),
        "A (anchored below the fence): the released seed and the released panel"
    );
    let panel_c =
        replayed.panel(&released.c.claim).expect("C: the armed node bound it in the released chain's first anchor past the fence");
    assert_eq!(panel_c.anchor, seed_of(&replay, &released.c), "C: the armed node keys it on the released anchor attempt's execution");
    assert_eq!(released.c.stored_anchor, released.c.anchor.header.hash, "C: the released node keyed it on the block");
    // **The split is at the fence, and it is loud**: the released chain's next block commits the
    // released post-state root (`palw_state_root` is the parent's root), which the armed node did not
    // fold — so an armed node never follows a released node past the first anchor past the fence.
    let (next, next_daa, _) = released.blocks[released.first_past + 1];
    let block = released.chain.ctx.consensus.get_block(next).expect("the released node holds its chain");
    let verdict = replay.ctx.consensus.validate_and_insert_block(block).virtual_state_task.await;
    eprintln!(
        "[t12-f1] the released chain's block after the first anchor past the fence (DAA {next_daa}) on an armed node: {verdict:?}"
    );
    assert_ne!(replay.sink(), next, "an armed node does not follow the released chain past the fence");

    // ---- an armed node building its own chain ---------------------------------------------------------
    let (a_a, r_a) = (&armed.a, &released.a);
    assert_eq!(a_a.stored_anchor, a_a.anchor.header.hash, "below the fence the seed is the anchor block (the released rule)");
    assert_eq!(r_a.stored_anchor, r_a.anchor.header.hash);
    assert_eq!(armed.first_past, released.first_past, "the same script, the same shape");

    // ---- past it: the execution commitment and the claim --------------------------------------------
    for (name, r, a) in [("C (slot below, anchor past)", &released.c, &armed.c), ("X", &released.x, &armed.x)] {
        assert_eq!(r.stored_anchor, r.anchor.header.hash, "{name}: the released rule keys the panel on the block");
        let seed = seed_of(&armed.chain, a);
        assert_eq!(a.stored_anchor, seed, "{name}: past the fence the panel is keyed on H(execution ‖ claim)");
        assert_ne!(a.stored_anchor, a.anchor.header.hash, "{name}: and never on the block identity");
        assert!(a.anchor.header.daa_score >= h, "{name}: anchored at or past the fence");
        eprintln!(
            "[t12-f1] {name}: slot {} anchor DAA {} — released seed {} / armed seed {} ({} seats)",
            a.slot,
            a.anchor.header.daa_score,
            r.stored_anchor,
            a.stored_anchor,
            a.seats.len()
        );
    }
    assert!(armed.c.slot < h && armed.c.anchor.header.daa_score >= h, "C straddles the fence: the key is the ANCHOR's DAA");

    // ---- the seed is chain data: a second node fed the armed chain agrees ------------------------------
    let (config, bundle, premine, floats) = t12_with_fence(Some(h));
    let follower = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let vp = armed.chain.vp();
    let genesis = armed.chain.config.params.genesis.hash;
    let mut hashes = Vec::new();
    let mut at = armed.chain.sink();
    while at != genesis {
        hashes.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    hashes.reverse();
    for hash in &hashes {
        let block = armed.chain.ctx.consensus.get_block(*hash).expect("the node holds every block of its chain");
        follower
            .ctx
            .consensus
            .validate_and_insert_block(block)
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("block {hash} of the armed chain was refused by a second armed node: {e}"));
    }
    assert_eq!(follower.sink(), armed.chain.sink(), "the second node walks the same chain");
    let (_, theirs) = follower.tip_state();
    let (_, ours) = armed.chain.tip_state();
    for anchored in [&armed.a, &armed.c, &armed.x] {
        let panel = theirs.panel(&anchored.claim).expect("the second node bound the claim");
        assert_eq!((panel.anchor, &panel.seats), (anchored.stored_anchor, &anchored.seats), "the same seed, the same seats");
    }
    assert_eq!(theirs.state_root(), ours.state_root(), "the same state");
    eprintln!(
        "[t12-f1] fence at {h}: {} blocks; A anchored at {} (block seed), C at {} and X at {} (execution seeds); a second node agreed",
        hashes.len(),
        armed.a.anchor.header.daa_score,
        armed.c.anchor.header.daa_score,
        armed.x.anchor.header.daa_score
    );
}
