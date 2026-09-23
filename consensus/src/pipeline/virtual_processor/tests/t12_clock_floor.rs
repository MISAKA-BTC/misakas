//! **testnet-12's clock floor, through the real pipeline** — the 2026-09-24 heartbeat audit's H3 and
//! H5 on the shipped testnet-12 ruleset (`Params::from(testnet-12)`, harness keys only, EVM inert on
//! a build without the `evm` feature — see `t12_round_lane_e2e`), against the same ruleset with
//! `palw_clock_floor` unset, which is the rule every other preset keeps.
//!
//! Only heartbeats are mined here: on testnet-12 no lane `bits` prices exists, so the heartbeat is
//! the clock, a slot takes two beats (the one stamped into the open slot and the one that merges it
//! and steps), and everything this file asserts is a statement about those two blocks.
use super::t12_round_lane_e2e::t12_with_harness_cards;
use super::{OnetimeTxSelector, TestContext, new_miner_data};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::headers::HeaderStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::palw_heartbeat_v1::{HEARTBEAT_RECOVERY_INTERVAL_MS as I, HeartbeatYieldHintV1};
use kaspa_muhash::MuHash;

/// A testnet-12 chain at genesis, the premine imported as a node imports it. `floor = false` is the
/// same ruleset with `palw_clock_floor` unset — what the chain ran before H3/H5, and what testnet-11,
/// devnet and mainnet still run.
fn t12_clock(floor: bool) -> (TestContext, Config) {
    let (config, _bundle, premine, _floats) = t12_with_harness_cards();
    assert!(config.params.palw_clock_floor.is_some_and(|f| f.is_active(0)), "testnet-12 arms the clock floor from genesis");
    assert!(config.params.palw_clock_cursor.is_some_and(|f| f.is_active(0)), "…and the cursor it refines");
    let config = if floor {
        config
    } else {
        let mut params = config.params.clone();
        params.palw_clock_floor = None;
        ConfigBuilder::new(params).skip_proof_of_work().build()
    };
    config.params.validate_palw_v2().expect("the fixture is a runnable ruleset");
    let consensus = TestConsensus::new(&config);
    {
        let mut imported = MuHash::new();
        consensus.append_imported_pruning_point_utxos(&premine, &mut imported);
        consensus
            .import_pruning_point_utxo_set(config.params.genesis.hash, imported)
            .expect("the premine imports against the genesis commitment it was hashed into");
    }
    let mut ctx = TestContext::new(consensus);
    ctx.simulated_time = config.params.genesis.timestamp;
    (ctx, config)
}

/// A beat as a miner whose clock reads `clock` builds it: the node's template stamped with that
/// clock, then the lane's adapter — which stamps it for the slot if the clock reads earlier.
fn beat(ctx: &TestContext, nonce: u64, clock: u64) -> (MutableBlock, u64) {
    let mut t = ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    t.block.header.timestamp = clock;
    t.block.header.nonce = nonce;
    t.block.header.finalize();
    let (t, earliest) = ctx.consensus.virtual_processor().heartbeat_adapt_block_template(t).expect("the heartbeat lane is open");
    (t.block, earliest)
}

/// The same block with a different stamp — what a miner that ignores the adapter would submit.
fn restamped(mut block: MutableBlock, timestamp: u64) -> MutableBlock {
    block.header.timestamp = timestamp;
    block.header.finalize();
    block
}

async fn submit(ctx: &mut TestContext, block: MutableBlock) -> Result<Block, RuleError> {
    let block = block.to_immutable();
    ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await?;
    ctx.simulated_time = ctx.simulated_time.max(block.header.timestamp);
    Ok(block)
}

async fn accepted(ctx: &mut TestContext, block: MutableBlock, what: &str) -> Block {
    let hash = block.header.hash;
    let block = submit(ctx, block).await.unwrap_or_else(|e| panic!("{what} {hash} was refused: {e}"));
    assert_eq!(ctx.consensus.block_status(hash), BlockStatus::StatusUTXOValid, "{what} is UTXO-valid");
    block
}

fn daa_of(ctx: &TestContext, hash: BlockHash) -> u64 {
    ctx.consensus.virtual_processor().headers_store.get_daa_score(hash).unwrap()
}

/// The slot the next beat would claim, as the hint reads it for the virtual — `None` while a beat
/// already holds the open slot (the next block steps).
fn taken_until(ctx: &TestContext, now: u64) -> Option<u64> {
    match ctx.consensus.virtual_processor().heartbeat_yield_hint_at(now) {
        HeartbeatYieldHintV1::SlotTaken(next) => Some(next),
        _ => None,
    }
}

/// One honest slot by one honest miner, as the H1 miner runs: wait while the slot is taken, beat,
/// and beat again until a beat steps the clock. Returns the step, whose timestamp is the next slot's
/// reference.
async fn honest_slot(ctx: &mut TestContext, nonce: u64) -> Block {
    let start = daa_of(ctx, ctx.consensus.get_sink());
    for n in nonce..nonce + 4 {
        ctx.simulated_time += 1_000;
        if let Some(next) = taken_until(ctx, ctx.simulated_time) {
            ctx.simulated_time = ctx.simulated_time.max(next);
        }
        let (built, _) = beat(ctx, n, ctx.simulated_time);
        let block = accepted(ctx, built, "an honest beat").await;
        if daa_of(ctx, block.header.hash) > start {
            return block;
        }
    }
    panic!("an honest miner did not step the clock in four beats")
}

/// Mine until the chain has a reference a slot is measured from, and stand at a taken slot.
async fn to_a_taken_slot(ctx: &mut TestContext) -> (Block, u64) {
    let mut step = honest_slot(ctx, 1).await;
    for n in 0..4u64 {
        step = honest_slot(ctx, 10 + 4 * n).await;
    }
    let next = taken_until(ctx, step.header.timestamp + 1).expect("after a step the next slot is taken until one interval later");
    assert_eq!(next, step.header.timestamp + I, "the step is the reference");
    (step, next)
}

/// **H3: a heartbeat stamped before its slot is invalid on testnet-12 — and valid without the
/// floor, which is how 89% of testnet-12's beats were blocks that could never tick.**
#[tokio::test]
async fn t12_a_beat_before_its_slot_is_refused_and_was_admitted_without_the_floor() {
    kaspa_core::log::try_init_logger("info");
    for floor in [true, false] {
        let (mut ctx, _config) = t12_clock(floor);
        let (step, slot) = to_a_taken_slot(&mut ctx).await;
        // A miner that did not wait: ten seconds after the step, one millisecond before the slot.
        let (built, earliest) = beat(&ctx, 500, step.header.timestamp + 10_000);
        assert_eq!(earliest, slot, "the adapter names the slot (H1)");
        let early = restamped(built, slot - 1);
        let hash = early.header.hash;
        match (floor, submit(&mut ctx, early).await) {
            (true, Err(RuleError::HeartbeatBeforeItsSlot(h, stamped, opens))) => {
                assert_eq!((h, stamped, opens), (hash, slot - 1, slot), "refused with the slot it came early for");
            }
            (false, Ok(block)) => {
                assert_eq!(daa_of(&ctx, block.header.hash), daa_of(&ctx, step.header.hash), "admitted, and it ticks nothing");
            }
            (floor, other) => panic!("floor={floor}: a beat 1 ms before its slot answered {other:?}"),
        }
    }
}

/// **H3's liveness half: an honest miner whose clock is BEHIND still produces an acceptable beat,
/// and the chain keeps ticking under clock skew.**
///
/// The rule's margin below the slot is zero, and that is safe because of who stamps: the adapter
/// stamps `max(the miner's clock, the slot)`, and the slot is a function of the beat's own parents,
/// so no honest node ever builds a beat below it whatever its clock says. A slow clock stamps the
/// slot itself — its own future, by its skew, which peers accept up to the 132 s drift tolerance —
/// and a fast one stamps its own reading. Here thirty slots are mined by miners whose clocks read
/// from a minute behind to a minute ahead, and every slot ticks exactly once, one interval or more
/// after the last.
#[tokio::test]
async fn t12_a_slow_clock_still_beats_and_the_chain_keeps_ticking_under_skew() {
    kaspa_core::log::try_init_logger("info");
    let (mut ctx, _config) = t12_clock(true);
    let (mut step, _) = to_a_taken_slot(&mut ctx).await;
    // The miner five seconds behind, at the moment the slot opens by the network's clock.
    let slot = step.header.timestamp + I;
    let (built, earliest) = beat(&ctx, 600, slot - 5_000);
    assert_eq!((earliest, built.header.timestamp), (slot, slot), "a slow clock is stamped at the slot, not before it");
    let holder = accepted(&mut ctx, built, "a slow miner's beat").await;
    assert_eq!(daa_of(&ctx, holder.header.hash), daa_of(&ctx, step.header.hash), "it holds the slot");
    let (built, _) = beat(&ctx, 601, slot + 2_000);
    step = accepted(&mut ctx, built, "the step over it").await;
    assert_eq!(daa_of(&ctx, step.header.hash), daa_of(&ctx, holder.header.hash) + 1, "and the clock ticks");

    // Thirty slots, skew cycling from a minute behind to a minute ahead, for the holder and the step
    // independently — a fleet whose clocks disagree by up to two minutes.
    let skews: [i64; 7] = [-60_000, -5_000, -1, 0, 3_000, 40_000, 60_000];
    let mut last_reference = step.header.timestamp;
    for k in 0..30usize {
        let slot = last_reference + I;
        let holder_clock = slot.saturating_add_signed(skews[k % skews.len()]);
        let (built, _) = beat(&ctx, 1_000 + 2 * k as u64, holder_clock);
        assert!(built.header.timestamp >= slot, "slot {k}: never stamped before the slot");
        let holder = accepted(&mut ctx, built, "a skewed holder").await;
        let step_clock = holder.header.timestamp.saturating_add_signed(skews[(k * 3 + 1) % skews.len()]);
        let (built, _) = beat(&ctx, 1_001 + 2 * k as u64, step_clock);
        let before = daa_of(&ctx, holder.header.hash);
        let next_step = accepted(&mut ctx, built, "a skewed step").await;
        assert_eq!(daa_of(&ctx, next_step.header.hash), before + 1, "slot {k}: exactly one tick");
        assert!(next_step.header.timestamp >= last_reference + I, "slot {k}: the tick is at least one interval after the last");
        last_reference = next_step.header.timestamp;
    }
}
