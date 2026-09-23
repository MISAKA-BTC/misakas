//! **testnet-12's clock floor, through the real pipeline** — the 2026-09-24 heartbeat audit's H3 and
//! H5 on the shipped testnet-12 ruleset (`Params::from(testnet-12)`, harness keys only, and the EVM
//! lane inert on every build because these tests stamp their own clock onto templates — see
//! `t12_round_lane_e2e`), against the same ruleset with `palw_clock_floor` unset, which is the rule
//! every other preset keeps. The last test runs the node's own path instead — its template, the
//! adapter, no re-stamp — with the EVM lane exactly as shipped.
//!
//! Only heartbeats are mined here: on testnet-12 no lane `bits` prices exists, so the heartbeat is
//! the clock, a slot takes two beats (the one stamped into the open slot and the one that merges it
//! and steps), and everything this file asserts is a statement about those two blocks.
use super::t12_round_lane_e2e::{stamp_harness_time, t12_with_harness_cards, t12_with_harness_cards_and_evm};
use super::{OnetimeTxSelector, TestContext, new_miner_data};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::headers::HeaderStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::config::{Config, ConfigBuilder, params::Params};
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_heartbeat_v1::{HEARTBEAT_RECOVERY_INTERVAL_MS as I, HeartbeatYieldHintV1};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
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
    let ctx = t12_at_genesis(&config, &premine);
    (ctx, config)
}

/// A chain at `config`'s genesis with the premine imported as a node imports it, its simulated clock
/// at the genesis time.
fn t12_at_genesis(config: &Config, premine: &[(TransactionOutpoint, UtxoEntry)]) -> TestContext {
    let consensus = TestConsensus::new(config);
    {
        let mut imported = MuHash::new();
        consensus.append_imported_pruning_point_utxos(premine, &mut imported);
        consensus
            .import_pruning_point_utxo_set(config.params.genesis.hash, imported)
            .expect("the premine imports against the genesis commitment it was hashed into");
    }
    let mut ctx = TestContext::new(consensus);
    ctx.simulated_time = config.params.genesis.timestamp;
    ctx
}

/// A beat as a miner whose clock reads `clock` builds it: the node's template stamped with that
/// clock, then the lane's adapter — which stamps it for the slot if the clock reads earlier.
fn beat(ctx: &TestContext, nonce: u64, clock: u64) -> (MutableBlock, u64) {
    let mut t = ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    stamp_harness_time(ctx.consensus.params(), &mut t.block.header, clock);
    t.block.header.nonce = nonce;
    t.block.header.finalize();
    let (t, earliest) = ctx.consensus.virtual_processor().heartbeat_adapt_block_template(t).expect("the heartbeat lane is open");
    (t.block, earliest)
}

/// The same block with a different stamp — what a miner that ignores the adapter would submit.
fn restamped(ctx: &TestContext, mut block: MutableBlock, timestamp: u64) -> MutableBlock {
    stamp_harness_time(ctx.consensus.params(), &mut block.header, timestamp);
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
        let early = restamped(&ctx, built, slot - 1);
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

/// **H5, acceleration: a step stamped before the slot it consumed is refused — and without the floor
/// it was admitted and opened the next slot two seconds after the last.**
///
/// The drift tolerance (132 s) is longer than the interval (120 s), so a beat stamped for the slot is
/// admissible the moment the reference exists; the block that merges it, stamped "now", became the
/// next reference. Nothing floored the spacing between two ticks.
#[tokio::test]
async fn t12_a_step_cannot_open_the_next_slot_early_and_could_without_the_floor() {
    kaspa_core::log::try_init_logger("info");
    for floor in [true, false] {
        let (mut ctx, _config) = t12_clock(floor);
        let (step, slot) = to_a_taken_slot(&mut ctx).await;
        let reference = step.header.timestamp;
        // A beat stamped for the slot a second after the reference — a clock two minutes ahead.
        let (built, _) = beat(&ctx, 700, reference + 1_000);
        assert_eq!(built.header.timestamp, slot, "the adapter stamps it for its slot");
        let holder = accepted(&mut ctx, built, "a beat stamped for the slot").await;
        // The step over it, stamped "now": two seconds after the reference.
        let (built, earliest) = beat(&ctx, 701, reference + 2_000);
        assert_eq!(earliest, slot, "the adapter stamps a step for the slot it consumes");
        let early = restamped(&ctx, built, reference + 2_000);
        let hash = early.header.hash;
        match (floor, submit(&mut ctx, early).await) {
            // A heartbeat step is a beat as well as a step, over one cursor, so H3 answers first; a
            // step from any other lane meets H5's own refusal (`h5_a_step_from_another_lane_...`).
            (
                true,
                Err(RuleError::HeartbeatBeforeItsSlot(h, stamped, opens) | RuleError::ClockStepBeforeItsSlot(h, stamped, opens)),
            ) => {
                assert_eq!((h, stamped, opens), (hash, reference + 2_000, slot));
                let (built, _) = beat(&ctx, 702, reference + 2_000);
                let honest = accepted(&mut ctx, built, "the step, stamped for its slot").await;
                assert_eq!(honest.header.timestamp, slot, "an honest step is stamped at the slot");
                assert_eq!(daa_of(&ctx, honest.header.hash), daa_of(&ctx, holder.header.hash) + 1, "and ticks");
                assert_eq!(
                    taken_until(&ctx, honest.header.timestamp + 1),
                    Some(reference + 2 * I),
                    "the next slot is two intervals after the last reference: the spacing is floored"
                );
            }
            (false, Ok(block)) => {
                assert_eq!(daa_of(&ctx, block.header.hash), daa_of(&ctx, holder.header.hash) + 1, "the early step ticked");
                assert_eq!(
                    taken_until(&ctx, block.header.timestamp + 1),
                    Some(reference + 2_000 + I),
                    "and the next slot opened two seconds after the last one did — the clock ran fast"
                );
            }
            (floor, other) => panic!("floor={floor}: a step two seconds after the reference answered {other:?}"),
        }
    }
}

/// **H5, delay: a future-stamped sibling step cannot push the next slot back — and without the
/// floor it did, whenever its hash was the lower one.**
///
/// Two steps over the same beat have the same parents, so the same blue score; v1 broke that tie by
/// hash. Measured on testnet-12: +31 s at DAA 8 and +18 s at DAA 24.
#[tokio::test]
async fn t12_a_future_stamped_sibling_step_cannot_delay_the_slot_and_could_without_the_floor() {
    kaspa_core::log::try_init_logger("info");
    for floor in [true, false] {
        let (mut ctx, _config) = t12_clock(floor);
        let (_step, slot) = to_a_taken_slot(&mut ctx).await;
        let (built, _) = beat(&ctx, 800, slot);
        accepted(&mut ctx, built, "the beat that holds the slot").await;
        // Both steps are built on the same virtual — the same parents.
        let (honest, _) = beat(&ctx, 801, slot + 1_000);
        let (sibling, _) = beat(&ctx, 802, slot + 1_000);
        assert_eq!(honest.header.direct_parents(), sibling.header.direct_parents(), "siblings");
        let ahead = (802..5_000u64)
            .map(|nonce| {
                let mut b = sibling.clone();
                b.header.nonce = nonce;
                restamped(&ctx, b, slot + 100_000)
            })
            .find(|b| b.header.hash < honest.header.hash)
            .expect("a nonce whose hash sorts below the honest step's — the case the hash tie-break got wrong");
        let honest_ts = honest.header.timestamp;
        let ahead_ts = ahead.header.timestamp;
        // Two tips; at most one of them is the sink, so neither is asserted UTXO-valid.
        submit(&mut ctx, honest).await.expect("the honest step is valid");
        submit(&mut ctx, ahead).await.expect("the step stamped 100 s ahead is valid: it is within the drift tolerance");
        let next = taken_until(&ctx, ahead_ts + 1).expect("the slot is taken until the next one");
        if floor {
            assert_eq!(next, honest_ts + I, "the earliest tied step is the reference: no delay");
        } else {
            assert_eq!(next, ahead_ts + I, "v1: the lower hash chose the reference, and the slot moved back 99 s");
        }
    }
}

/// The node's own beat, as the H1 miner builds it: the node's template — stamped by the builder from
/// this host's clock, every commitment executed against that stamp — then the lane's adapter and a
/// nonce. Nothing is re-stamped. Returns the block, the adapter's `earliest` and the header the
/// builder produced.
fn own_beat(ctx: &TestContext, nonce: u64) -> (MutableBlock, u64, Header) {
    let mut t = ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    let built = t.block.header.clone();
    t.block.header.nonce = nonce;
    t.block.header.finalize();
    let (t, earliest) = ctx.consensus.virtual_processor().heartbeat_adapt_block_template(t).expect("the heartbeat lane is open");
    (t.block, earliest, built)
}

/// **The path a node mines, on testnet-12 exactly as shipped — the EVM lane included — from
/// genesis.**
///
/// Every other test in this file drives the chain with a simulated clock, stamping each template
/// after the build, and therefore runs with the EVM lane inert (`t12_round_lane_e2e`'s module doc
/// says why). This one stamps nothing: each beat is [`own_beat`] — the H1 miner's pass without the
/// wait. It starts at testnet-12's real genesis (2026-09-01T00:00:00Z, three weeks before the wall
/// clock its templates carry), with the six-validator DNS set, `palw_clock_floor` and every other
/// fence as shipped, and runs as far as the wall clock lets it — through the step that consumes the
/// first slot the adapter had to stamp ahead of the node's clock:
///
/// * **the beat that claims a slot opening two minutes from now is stamped AT the slot by the
///   adapter** — its own future, inside the 132 s drift tolerance — and must still be a valid block.
///   With the EVM lane active that stamp moves `evm_commitment_root` (the lane executes against the
///   header's timestamp, in whole seconds), and the adapter used to move the stamp without the
///   commitment: the node's own beat was disqualified from the chain. Found while diagnosing the
///   2026-09-24 merge battery, which builds this crate with `evm` (through `kaspad`'s default
///   features); before the fix this test failed at exactly that beat;
/// * **the step over it is stamped at the slot by the TEMPLATE** (H5's construction half), so the
///   builder's own commitment stands and the adapter has nothing to move.
///
/// A build without the `evm` feature cannot have the lane active; there the test runs it inert and
/// states the clock half only.
#[tokio::test]
async fn t12_the_node_s_own_beats_tick_from_genesis_with_the_evm_lane_as_shipped() {
    kaspa_core::log::try_init_logger("info");
    let keep_evm = cfg!(feature = "evm");
    let (config, _bundle, premine, _floats) = t12_with_harness_cards_and_evm(keep_evm);
    let shipped = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    assert_eq!(config.params.genesis.timestamp, 1_788_220_800_000, "testnet-12's own genesis time, 2026-09-01T00:00:00Z");
    assert_eq!(config.params.genesis.timestamp, shipped.genesis.timestamp);
    assert!(shipped.dns_params.is_some() && config.params.dns_params == shipped.dns_params, "the DNS set as shipped");
    assert!(config.params.palw_clock_floor.is_some_and(|f| f.is_active(0)), "the clock floor from genesis");
    assert_eq!(config.params.is_evm_active(0), keep_evm, "the EVM lane as shipped wherever this build can run it");
    let mut ctx = t12_at_genesis(&config, &premine);
    let genesis = ctx.consensus.get_sink();
    assert_eq!(genesis, config.params.genesis.hash);
    assert_eq!(daa_of(&ctx, genesis), 0);

    // The node's clock is the wall clock here, so the chain can run only as far as the drift
    // tolerance lets a beat be stamped ahead of it: up to the step that consumes the first slot the
    // adapter had to stamp into the future. That is the case this test exists for.
    let drift_ms = config.params.timestamp_deviation_tolerance * 1000;
    let mut held_ahead_at: Option<u64> = None;
    let mut last_daa = 0u64;
    for nonce in 1..=6u64 {
        let (built, earliest, builder) = own_beat(&ctx, nonce);
        let built_at = builder.timestamp;
        assert_eq!(built.header.timestamp, built_at.max(earliest), "beat {nonce}: the adapter stamps max(the node's clock, the slot)");
        let ahead = built.header.timestamp.saturating_sub(kaspa_core::time::unix_now());
        assert!(ahead <= drift_ms, "beat {nonce}: stamped {ahead} ms ahead of the node's clock, past the drift tolerance");
        let stamped_ahead = built.header.timestamp > built_at;
        // The rule the adapter has to keep: the lane executes against the stamp in whole seconds, so
        // a beat stamped into another second carries another commitment than its builder's.
        if keep_evm && built.header.timestamp / 1000 != built_at / 1000 {
            assert_ne!(
                built.header.evm_commitment_root, builder.evm_commitment_root,
                "beat {nonce}: stamped into another second, the EVM commitment is re-derived for the stamp"
            );
        } else {
            assert_eq!(built.header.evm_commitment_root, builder.evm_commitment_root, "beat {nonce}: the builder's commitment stands");
        }
        let block = accepted(&mut ctx, built, &format!("the node's own beat {nonce}")).await;
        assert_eq!(ctx.consensus.get_sink(), block.header.hash, "beat {nonce} is the sink");
        let daa = daa_of(&ctx, block.header.hash);
        eprintln!(
            "[t12-own-beat] beat {nonce}: built at {built_at}, stamped {} (+{} ms), DAA {daa}",
            block.header.timestamp,
            block.header.timestamp - built_at
        );
        assert!(daa == last_daa || daa == last_daa + 1, "beat {nonce}: the clock moves one tick at a time ({last_daa} -> {daa})");
        last_daa = daa;
        if stamped_ahead && held_ahead_at.is_none() {
            held_ahead_at = Some(daa);
        }
        if held_ahead_at.is_some_and(|held| daa > held) {
            break;
        }
    }
    let held = held_ahead_at.expect("a beat was stamped into a slot ahead of the node's clock — the case the adapter must re-commit");
    assert_eq!(last_daa, held + 1, "the step over the beat stamped for its slot ticked the clock");
    assert!(last_daa >= 1, "the chain ticked from genesis on the node's own beats");
    let step = ctx.consensus.get_sink();
    let step_ts = ctx.consensus.virtual_processor().headers_store.get_timestamp(step).unwrap();
    assert_eq!(taken_until(&ctx, step_ts + 1), Some(step_ts + I), "and the next slot is one interval after the last step");
}
