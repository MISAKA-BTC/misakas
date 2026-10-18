//! **RFC-0016 through the real virtual processor on testnet-12** — V+ block rights, the clock and heartbeat protection, and the
//! node-to-node / reorg consistency of everything RFC-0016 reads.
//!
//! **TEST SOURCES ONLY — not run in the round that wrote them; nothing here is a PASS. These tests bypass `validate_palw_v2`, and
//! say so.** `palw_model_block_rights_v1` cannot be armed on any network (placeholder values, no measured record). Here it is set on a
//! copy of testnet-12's params at DAA [`FENCE`] after `ConfigBuilder::build` (which validates), mirrored by `sync_palw_rfc0016_v1`.
//! No allocation is armed, so `ΣA = 0` and every model's issued rights are ZERO — exactly RFC-0016 §12.4 H1's world: every model
//! Attempt is refused (skipped, its block standing), and the floor and the heartbeat must carry the clock alone.
//!
//! | test | RFC-0016 id |
//! |---|---|
//! | `t12_rfc0016_zero_rights_heartbeat_keeps_the_cadence` | H1 (cadence with every right at zero), H3 (rights never move the clock) |
//! | `t12_rfc0016_heartbeat_is_not_tightened_and_earns_nothing` | H2 (ADR-0140 C1/C3/C4 under V+) |
//! | `t12_rfc0016_a_second_node_and_a_reorg_reach_the_same_state` | A8, H4 (IBD block by block, reorg) |
use super::t12_round_lane_e2e::{T12Chain, t12_genesis_chain, t12_with_harness_cards};
use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::PalwChainStateV2;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// The first DAA V+ is in force.
const FENCE: u64 = 20;
/// How far past the fence the heartbeat alone must carry the clock.
const SPAN: u64 = 40;

fn armed(on: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (mut config, bundle, premine, floats) = t12_with_harness_cards();
    if !on {
        return (config, bundle, premine, floats);
    }
    config.params.palw_model_block_rights_v1 = Some(ForkActivation::new(FENCE));
    config.params.sync_palw_rfc0016_v1();
    assert!(config.params.validate_palw_v2().is_err(), "the real validator still refuses what this test bypasses");
    let PalwConsensusMode::ConsensusV2(armed) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    assert!(armed.state.rfc0016_rights_active_at(FENCE), "the mirror is set");
    let bundle = armed.clone();
    (config, bundle, premine, floats)
}

fn ttpb(chain: &T12Chain) -> u64 {
    chain.config.params.target_time_per_block()
}

fn state_of(chain: &T12Chain) -> PalwChainStateV2 {
    chain.tip_state().1
}

/// The same script on any chain: heartbeats to the fence, then for `SPAN` DAA an attempt by card 0 before every heartbeat. Returns
/// the claims the chain held and the heartbeat blocks' `bits` in order (attempt blocks excluded).
async fn script(chain: &mut T12Chain) -> (usize, Vec<u32>) {
    let mut heartbeat_bits = Vec::new();
    while chain.daa_of(chain.sink()) < FENCE {
        heartbeat_bits.push(chain.heartbeat(ttpb(chain), Vec::new()).await.header.bits);
    }
    let mut held = 0;
    while chain.daa_of(chain.sink()) < FENCE + SPAN {
        let (_, claim) = chain.attempt(0, ttpb(chain), Vec::new(), &|_| true).await;
        if state_of(chain).claim(&claim).is_some() {
            held += 1;
        }
        heartbeat_bits.push(chain.heartbeat(ttpb(chain), Vec::new()).await.header.bits);
    }
    (held, heartbeat_bits)
}

/// H3 (and the floor half of H1): with every model's rights at zero, the clock reaches `FENCE + SPAN` in the same number of
/// heartbeats as an unarmed twin fed the same script, and the FLOOR class keeps its own rule — `rfc0016_rights_admit_v1` returns `Ok`
/// for `base_class_id` ("the floor and the heartbeat keep their own rules"), so card 0's floor Attempts are claims armed or not.
///
/// **Validation 2026-10-17 (stale premise fixed):** this source was written unbuilt and asserted `held == 0`, i.e. that the floor's
/// Attempts are refused for rights; the rule exempts the floor by design, and the armed chain held 41 claims. The refusal of a
/// NON-floor model Attempt (`ModelBlockRightsRefused`) is not exercised at node level by this harness (it registers no other model
/// class) and stays open.
#[tokio::test]
async fn t12_rfc0016_zero_rights_heartbeat_keeps_the_cadence() {
    let (config, bundle, premine, floats) = armed(true);
    let mut a = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let (held, beats_a) = script(&mut a).await;
    assert!(a.daa_of(a.sink()) >= FENCE + SPAN, "the clock reaches the span past the fence");
    let (config, bundle, premine, floats) = armed(false);
    let mut twin = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let (held_twin, beats_twin) = script(&mut twin).await;
    assert!(held_twin > 0, "unarmed, the same attempts are claims");
    assert_eq!(held, held_twin, "the floor class is exempt from V+ rights: armed, its Attempts are the same claims");
    assert_eq!(beats_a.len(), beats_twin.len(), "the same heartbeats reach the same DAA: rights never move the clock");
}

/// H2: under V+ the heartbeat is not tightened — every heartbeat's `bits` equals the unarmed twin's at the same position of the same
/// script (ADR-0140 C1) — and the refused Attempts leave the Final weight where it was (no weight for the clock's carrier, C3).
#[tokio::test]
async fn t12_rfc0016_heartbeat_is_not_tightened_and_earns_nothing() {
    let (config, bundle, premine, floats) = armed(true);
    let mut a = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let before = state_of(&a).safe_weight();
    let (_, beats_a) = script(&mut a).await;
    let (config, bundle, premine, floats) = armed(false);
    let mut twin = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let (_, beats_twin) = script(&mut twin).await;
    assert_eq!(beats_a, beats_twin, "the heartbeat's difficulty is the same with and without V+");
    assert_eq!(state_of(&a).safe_weight(), before, "no claim, no Final weight: the clock's carrier takes none");
}

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

/// A8 / H4: a second node fed A's blocks one by one reaches A's PALW state root (the rights consumed are the epoch counters, the
/// snapshot / queue / telemetry rows are rooted aux rows), and the tip's carriage reloads under its root (restart).
#[tokio::test]
async fn t12_rfc0016_a_second_node_and_a_reorg_reach_the_same_state() {
    let (config, bundle, premine, floats) = armed(true);
    let mut a = t12_genesis_chain(&config, &bundle, &premine, &floats);
    script(&mut a).await;
    let b = t12_genesis_chain(&config, &bundle, &premine, &floats);
    for block in chain_blocks(&a, a.sink()) {
        arrive(&b, block, "A's block").await;
    }
    assert_eq!(state_of(&b).state_root(), state_of(&a).state_root(), "IBD reaches the same RFC-0016 state");
    let state = state_of(&a);
    let reloaded = kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&state)
        .into_state(&bundle.state, Some(state.state_root()))
        .expect("restart");
    assert_eq!(reloaded.state_root(), state.state_root());
}
