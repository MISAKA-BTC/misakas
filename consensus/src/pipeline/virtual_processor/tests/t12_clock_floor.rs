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
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::headers::HeaderStoreReader;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::config::{Config, ConfigBuilder, params::Params};
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_heartbeat_v1::{HEARTBEAT_RECOVERY_INTERVAL_MS as I, HeartbeatYieldHintV1};
use kaspa_consensus_core::pruning::PruningProofMetadata;
use kaspa_consensus_core::trusted::TrustedBlock;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_consensus_core::{BlockHash, BlockHashSet};
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
        // ADR-0152: R-core+ is armed above this fence on testnet-12 and refuses to stand without
        // it, so the fence-off twin takes R-core+ off too (every R-core+ writer is dormant, so the
        // twin folds exactly as it did before the v22 skeleton).
        params.palw_rcore_plus = None;
        params.palw_rcore_conservative_classes = &[];
        params.sync_palw_rcore_plus();
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
/// slot itself — its own future, by its skew, which peers accept up to 132 s: the lead cap
/// (`palw_clock_lead_cap`) holds a beat and a step to that however far the drift tolerance reaches
/// (1,620 s, mainnet's, since the user's 2026-09-25 decision) — and a fast one stamps its own reading. Here thirty slots are mined by miners whose clocks read
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
/// The drift tolerance (1,620 s; 132 s before 2026-09-25) is longer than the interval (120 s), so a
/// beat stamped for the slot is admissible the moment the reference exists; the block that merges it,
/// stamped "now", became the next reference. Nothing floored the spacing between two ticks.
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

/// A chain whose clock last stepped at exactly `reference` (a wall-clock time in the past): honest
/// slots an hour back, then a holder one second before `reference` and the step at it — the lowest
/// blue score at its DAA score, so the next slot opens at `reference + I`.
async fn stepped_at(ctx: &mut TestContext, reference: u64, nonce: u64) -> Block {
    ctx.simulated_time = reference - 3_600_000;
    honest_slot(ctx, nonce).await;
    let (built, _) = beat(ctx, nonce + 10, reference - 1_000);
    let holder = accepted(ctx, built, "a holder a second before the reference").await;
    let (built, _) = beat(ctx, nonce + 11, reference);
    let step = accepted(ctx, built, "the step at the reference").await;
    assert_eq!(daa_of(ctx, step.header.hash), daa_of(ctx, holder.header.hash) + 1, "it steps the clock");
    assert_eq!(taken_until(ctx, reference + 1), Some(reference + I), "and is the next slot's reference");
    step
}

/// **The lead cap (the 2026-09-25 mainnet-values review's HIGH): at mainnet's 1,620 s tolerance one
/// producer's burst advances the DAA by 2 — and by 14 without the cap.**
///
/// The burst the review measured, from a chain in step with wall time: the clock last stepped 114 s
/// ago, so the next slot opens 6 s from now. A producer then mints the holder and the step of every
/// slot the moment it can, each stamped AT its slot, with no other block in between. Without the cap
/// it stops only where a slot opens past `now + 1,620 s` — slots at `now + 6 s + 120 k`, 14 of them
/// (`⌊T / I⌋ + 1`); with it, the first beat stamped more than 132 s past the node's clock is refused
/// `ClockLeadTooFarAhead` — 2 slots (`⌊132 / I⌋ + 1`), the readiness escalation's 2-DAA margin.
/// Measured against the node's real clock, which is what both bounds read. (A chain that MISSED
/// slots can be caught up by one tick a missed interval, stamped in the past and above the median,
/// exactly as at the 132 s tolerance before 2026-09-25; the cap bounds the lead, not the backlog.)
#[tokio::test]
async fn t12_the_lead_cap_bounds_a_run_ahead_burst_to_two_ticks() {
    use kaspa_consensus_core::config::params::ForkActivation;
    kaspa_core::log::try_init_logger("info");
    let (shipped, _bundle, premine, _floats) = t12_with_harness_cards();
    assert_eq!(shipped.params.timestamp_deviation_tolerance, 1_620, "testnet-12 runs mainnet's tolerance");
    assert_eq!(shipped.params.palw_clock_lead_cap, Some(ForkActivation::always()), "and arms the cap from genesis");
    for capped in [true, false] {
        let config = if capped {
            shipped.clone()
        } else {
            let mut params = shipped.params.clone();
            params.palw_clock_lead_cap = None;
            ConfigBuilder::new(params).skip_proof_of_work().build()
        };
        let mut ctx = t12_at_genesis(&config, &premine);
        let mut step = stepped_at(&mut ctx, kaspa_core::time::unix_now() - I + 6_000, 1).await;
        let start = daa_of(&ctx, step.header.hash);
        let begun = std::time::Instant::now();
        let mut nonce = 3_000u64;
        let refusal = loop {
            assert!(daa_of(&ctx, step.header.hash) - start <= 16, "capped={capped}: the burst ran past any budget");
            let slot = step.header.timestamp + I;
            let (built, _) = beat(&ctx, nonce, slot);
            assert_eq!(built.header.timestamp, slot, "the holder is stamped at its slot");
            if let Err(refusal) = submit(&mut ctx, built).await {
                break refusal;
            }
            let (built, _) = beat(&ctx, nonce + 1, slot);
            nonce += 2;
            match submit(&mut ctx, built).await {
                Ok(next) => {
                    assert_eq!(daa_of(&ctx, next.header.hash), daa_of(&ctx, step.header.hash) + 1, "one tick a slot");
                    step = next;
                }
                Err(refusal) => break refusal,
            }
        };
        let ticks = daa_of(&ctx, step.header.hash) - start;
        eprintln!(
            "[t12-lead-cap] capped={capped}: one producer advanced the DAA by {ticks} in {:?} ({start} -> {}), then: {refusal}",
            begun.elapsed(),
            start + ticks
        );
        if capped {
            assert!(
                matches!(refusal, RuleError::ClockLeadTooFarAhead(..)),
                "capped: refused by the cap, not the tolerance: {refusal}"
            );
            assert_eq!(ticks, 2, "capped: a burst is two ticks");
        } else {
            assert!(matches!(refusal, RuleError::TimeTooFarIntoTheFuture(..)), "uncapped: refused by the tolerance alone: {refusal}");
            assert_eq!(ticks, 14, "uncapped: the burst the review measured");
        }
    }
}

/// **A refused step is not a verdict: it is not stored, not `StatusInvalid`, and the same block is
/// admitted once the node's clock reaches its stamp less 132 s** — the local-clock contract of
/// `TimeTooFarIntoTheFuture`, which the cap shares (the header stage caches only post-PoW errors).
#[tokio::test]
async fn t12_a_step_past_the_lead_cap_is_not_invalid_and_is_admitted_once_the_clock_catches_up() {
    use kaspa_consensus_core::palw_clock_cursor_v1::PALW_CLOCK_LEAD_CAP_MS as CAP;
    kaspa_core::log::try_init_logger("info");
    let (mut ctx, _config) = t12_clock(true);
    let step = stepped_at(&mut ctx, kaspa_core::time::unix_now() - 100_000, 1).await;
    let slot = step.header.timestamp + I;
    let (built, _) = beat(&ctx, 900, slot);
    let holder = accepted(&mut ctx, built, "the beat that holds the slot, 20 s ahead: inside the cap").await;
    // The step over it, stamped 1.5 s past the cap — well inside the 1,620 s tolerance.
    let ahead = kaspa_core::time::unix_now() + CAP + 1_500;
    assert!(ahead >= slot, "stamped at or past the slot, so the floor admits it");
    let (built, _) = beat(&ctx, 901, ahead);
    assert_eq!(built.header.timestamp, ahead);
    let hash = built.header.hash;
    match submit(&mut ctx, built.clone()).await {
        Err(RuleError::ClockLeadTooFarAhead(h, stamped, latest)) => {
            assert_eq!((h, stamped), (hash, ahead), "refused for its own stamp");
            assert!(latest < ahead && ahead - latest <= 1_500, "the bound is the node's clock plus 132 s");
        }
        other => panic!("a step 1.5 s past the cap answered {other:?}"),
    }
    assert_eq!(ctx.consensus.get_block_status(hash), None, "not stored, and above all not StatusInvalid");
    assert_eq!(daa_of(&ctx, ctx.consensus.get_sink()), daa_of(&ctx, holder.header.hash), "the clock did not tick");
    // Wall time catches up; the SAME block is admitted and ticks.
    let wait = (ahead - CAP).saturating_sub(kaspa_core::time::unix_now()) + 250;
    tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
    let admitted = accepted(&mut ctx, built, "the same step once the clock caught up").await;
    assert_eq!(admitted.header.hash, hash);
    assert_eq!(daa_of(&ctx, hash), daa_of(&ctx, holder.header.hash) + 1, "and it ticks the clock");
}

/// **What a past-median time held ahead of wall time does under the cap: the clock stalls, it does
/// not run — and a block that moves no clock keeps the full tolerance.**
///
/// A producer that is not bounded by the cap is one whose blocks move no clock: here eight cards'
/// attempt blocks, each stamped 1,000 s past the node's clock (between the cap and the tolerance),
/// merging nothing a beat was granted on. Every one is ADMITTED (the cap does not reach them), and
/// enough of them hold the past-median time more than 132 s ahead. Then the next heartbeat — the node's
/// own template, which a block must stamp above the median — is refused `ClockLeadTooFarAhead` and
/// the DAA score does not move: every DAA-denominated window (a readiness row's age among them) keeps
/// its wall length and waits with the clock, so no row lapses for want of a block. The stall ends when
/// wall time reaches the median less 132 s — at most the tolerance less the cap after the last push.
/// Before the cap the same push ran the DAA clock ahead instead (a step is stamped above the median
/// and was admitted up to 1,620 s out).
#[tokio::test]
async fn t12_a_median_pushed_ahead_stalls_the_clock_and_lapses_no_row() {
    use kaspa_consensus_core::palw_clock_cursor_v1::PALW_CLOCK_LEAD_CAP_MS as CAP;
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let tolerance_ms = config.params.timestamp_deviation_tolerance * 1_000;
    let mut chain = super::t12_round_lane_e2e::t12_genesis_chain(&config, &bundle, &premine, &floats);
    let ttpb = config.params.target_time_per_block();
    // Honest beats until the next block would step nothing: the tip is a step, not a holder.
    let steps_nothing = |chain: &super::t12_round_lane_e2e::T12Chain| {
        let parents: Vec<BlockHash> = chain.ctx.consensus.get_virtual_parents().into_iter().collect();
        !chain.vp().palw_clock_step_for_parents(&parents).expect("a clock for the virtual").granted
    };
    for _ in 0..8 {
        chain.heartbeat(ttpb, Vec::new()).await;
    }
    for _ in 0..4 {
        if steps_nothing(&chain) {
            break;
        }
        chain.heartbeat(ttpb, Vec::new()).await;
    }
    assert!(steps_nothing(&chain), "the tip is a step");
    let daa = chain.daa_of(chain.sink());

    let push_to = kaspa_core::time::unix_now() + 1_000_000;
    chain.ctx.simulated_time = push_to;
    let mut pushed = 0usize;
    while chain.ctx.consensus.get_virtual_past_median_time() <= kaspa_core::time::unix_now() + CAP {
        assert!(pushed < 40, "the median did not move in {pushed} blocks");
        let (block, _) = chain.attempt(pushed % 8, 1, Vec::new(), &|_| true).await;
        let lead = block.header.timestamp - kaspa_core::time::unix_now();
        assert!(lead > CAP && lead <= tolerance_ms, "attempt {pushed}: stamped {lead} ms ahead — between the cap and the tolerance");
        assert_eq!(chain.daa_of(block.header.hash), daa, "attempt {pushed}: admitted, and it moves no clock");
        pushed += 1;
    }
    eprintln!(
        "[t12-lead-cap] {pushed} attempt blocks stamped ~1,000 s ahead hold the past-median time {} ms past the node's clock",
        chain.ctx.consensus.get_virtual_past_median_time() - kaspa_core::time::unix_now()
    );

    // The node's own heartbeat: its template is stamped above the median, and nothing else.
    let t = chain
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template that steps nothing is built at any stamp the tolerance admits");
    assert!(t.block.header.timestamp > kaspa_core::time::unix_now() + CAP, "the template is stamped above the median");
    let (t, earliest) = chain.vp().heartbeat_adapt_block_template(t).expect("the heartbeat lane is open");
    assert!(earliest > kaspa_core::time::unix_now() + CAP, "the H1 miner, which mints nothing past its own clock, would wait");
    let beat = t.block.to_immutable();
    let hash = beat.header.hash;
    match chain.ctx.consensus.validate_and_insert_block(beat).virtual_state_task.await {
        Err(RuleError::ClockLeadTooFarAhead(h, ..)) => assert_eq!(h, hash),
        other => panic!("a beat above a pushed median answered {other:?}"),
    }
    assert_eq!(chain.ctx.consensus.get_block_status(hash), None, "refused for now, not invalid");
    assert_eq!(chain.daa_of(chain.sink()), daa, "the clock stalls: no DAA-denominated window moved, so no row lapsed");
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
///   adapter** — its own future, inside the drift tolerance — and must still be a valid block.
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

/// **The drift budget at mainnet's tolerance: a producer runs the DAA clock up to `T` ahead of wall
/// time — about 13 slots at testnet-12's 1,620 s, about 1 at the 132 s it ran before** (user decision
/// 2026-09-25: testnet-12 runs mainnet's numbers).
///
/// The clock is paced by header stamps, and a stamp is bounded above only by
/// `unix_now() + timestamp_deviation_tolerance`. So a miner whose clock claims every slot the moment
/// it opens — the holder and the step both stamped AT the slot, which the floor admits — ticks one DAA
/// per slot until the next slot opens past that bound, and there it stops: the header is refused
/// `TimeTooFarIntoTheFuture` and the chain waits for wall time. The lead is a one-time offset in
/// STAMPS — every tick is still exactly one interval after the last (the floor's spacing) — but not
/// in wall time: the ticks below are minted in well under a second, so one producer advances the DAA
/// by up to `⌊T / I⌋ + 1` = 14 with no other block between (2 at 132 s), and every DAA-denominated
/// window loses that much wall time (what that does to a readiness row:
/// `t12_run_ahead_burst_vs_readiness`, the 2026-09-25 review's HIGH, open for the user). Measured
/// against the node's real clock (the future bound reads `unix_now`, and the refusal reports it):
/// at the refusal the last tick's stamp leads the node's clock by more than `T − I` and at most `T`
/// — `(1,500 s, 1,620 s]`, 12.5–13.5 slots, at 1,620 s; `(12 s, 132 s]` at 132 s.
#[tokio::test]
async fn t12_a_producer_runs_the_clock_at_most_the_drift_budget_ahead() {
    kaspa_core::log::try_init_logger("info");
    let (shipped, _bundle, premine, _floats) = t12_with_harness_cards();
    assert_eq!(shipped.params.timestamp_deviation_tolerance, 1_620, "testnet-12 runs mainnet's tolerance: 27 samples x 120 s / 2");
    for tolerance in [1_620u64, 132] {
        // The tolerance ALONE: past `palw_clock_lead_cap` (testnet-12 arms it) a clock-moving header
        // is refused 132 s past the node's clock whatever the tolerance, so this is the budget the cap
        // takes away (`t12_the_lead_cap_bounds_a_run_ahead_burst_to_two_ticks`).
        let mut params = shipped.params.clone();
        params.timestamp_deviation_tolerance = tolerance;
        params.palw_clock_lead_cap = None;
        let config = ConfigBuilder::new(params).skip_proof_of_work().build();
        let budget_ms = tolerance * 1_000;
        let mut ctx = t12_at_genesis(&config, &premine);
        // Genesis is three weeks behind the wall clock: an honest slot mined from "now" brings the
        // reference to about wall time (`honest_slot` waits for a taken slot on the simulated clock,
        // so the reference may already sit up to one interval ahead — the lead below is measured
        // against the node's clock, not against it).
        ctx.simulated_time = kaspa_core::time::unix_now();
        let mut step = honest_slot(&mut ctx, 1).await;
        let at_wall_time = daa_of(&ctx, step.header.hash);
        let mut ticks = 0u64;
        let mut nonce = 2_000u64;
        let node_clock_at_refusal = loop {
            assert!(ticks * I <= budget_ms + I, "T = {tolerance} s: the clock ran {ticks} slots ahead, past the budget");
            let slot = step.header.timestamp + I;
            // A miner whose clock reads the slot the moment it opens: the holder...
            let (built, _) = beat(&ctx, nonce, slot);
            assert_eq!(built.header.timestamp, slot, "stamped at the slot");
            match submit(&mut ctx, built).await {
                Ok(_) => {}
                Err(RuleError::TimeTooFarIntoTheFuture(stamped, bound)) => {
                    assert!(stamped > bound && stamped == slot, "T = {tolerance} s: refused for the future bound alone");
                    break bound - budget_ms;
                }
                Err(e) => panic!("T = {tolerance} s: the run-ahead holder after {ticks} ticks was refused for {e}"),
            }
            // ...and the step over it, which the adapter stamps at the slot it consumes.
            let (built, _) = beat(&ctx, nonce + 1, slot);
            nonce += 2;
            let next = accepted(&mut ctx, built, "a run-ahead step").await;
            assert_eq!(next.header.timestamp, slot, "the step is stamped at its slot");
            assert_eq!(daa_of(&ctx, next.header.hash), daa_of(&ctx, step.header.hash) + 1, "one tick a slot");
            step = next;
            ticks += 1;
        };
        let lead_ms = step.header.timestamp.saturating_sub(node_clock_at_refusal);
        eprintln!(
            "[t12-drift-budget] T = {tolerance} s: the last tick leads the node's clock by {:.1} s = {:.2} slots ({ticks} run-ahead \
             ticks, DAA {at_wall_time} -> {})",
            lead_ms as f64 / 1_000.0,
            lead_ms as f64 / I as f64,
            daa_of(&ctx, step.header.hash)
        );
        assert!(lead_ms <= budget_ms, "T = {tolerance} s: no tick is ever stamped past the future bound");
        assert!(lead_ms + I > budget_ms, "T = {tolerance} s: and a producer gets within one interval of it");
    }
    // The two tolerances' budgets in whole slots: 13 at mainnet's, 1 at the hash lineage's.
    assert_eq!((1_620 * 1_000 / I, 132 * 1_000 / I), (13, 1));
}

/// **Mainnet's block-level ceiling on testnet-12 (225, user decision 2026-09-25): every block is level
/// 0, a header lists parents at level 0 alone, and the pruning proof is `225 + 1` levels.**
///
/// On a hash lineage the ceiling places blocks in the pruning proof's hierarchy (`calc_level_from_pow`
/// = ceiling − bits(pow)). Here no block buys a level: a heartbeat derives none, a receipt none, and
/// an attempt header none past the single lottery, which testnet-12 arms from genesis. So the ceiling
/// sets only the genesis level, the proof's level count and with it the proof's header budget — the
/// chain's headers are the same at 225 as at 250. Checked on the pipeline's own stores, over the
/// honest heartbeat slots `to_a_taken_slot` mines, and on the proof the node would serve.
#[tokio::test]
async fn t12_blocks_are_level_zero_under_mainnet_s_ceiling() {
    kaspa_core::log::try_init_logger("info");
    let (mut ctx, config) = t12_clock(true);
    assert_eq!(config.params.max_block_level, 225, "testnet-12 runs mainnet's ceiling");
    assert!(config.params.palw_single_lottery_at(0), "an attempt header derives no level from genesis");
    let genesis = config.params.genesis.hash;
    let headers = ctx.consensus.virtual_processor().headers_store.clone();
    assert_eq!(headers.get_header_with_block_level(genesis).unwrap().block_level, 225, "genesis sits at the ceiling");
    let mut checked = 0;
    for n in 0..6u64 {
        let step = honest_slot(&mut ctx, 100 + 10 * n).await;
        for hash in std::iter::once(step.header.hash).chain(step.header.direct_parents().iter().copied()) {
            if hash == genesis {
                continue;
            }
            let h = headers.get_header_with_block_level(hash).unwrap();
            assert_eq!(h.block_level, 0, "block {hash}: a heartbeat buys no level");
            assert_eq!(
                h.header.parents_by_level.expanded_len(),
                1,
                "block {hash}: parents at level 0 only — above it every level is genesis"
            );
            checked += 1;
        }
    }
    assert!(checked >= 12, "both beats of every slot were checked ({checked})");
    let proof = ctx.consensus.get_pruning_point_proof();
    assert_eq!(proof.len(), 225 + 1, "one proof level per block level, the ceiling included");
    assert!(
        proof.iter().all(|level| level.len() == 1 && level[0].hash == genesis),
        "at genesis's pruning point every level is genesis"
    );
}

/// The part of the trusted set `apply_pruning_proof` reads for `pp`, as the IBD server sends it:
/// the pruning point and its anticone from `sink`'s point of view, as full blocks with their GHOSTDAG
/// data, in blue-work order. (The server's `get_pruning_point_anticone_and_trusted_data` answers only
/// for the node's own pruning point and only while its virtual is deep above it; the DAA and
/// GHOSTDAG windows it adds are what the blocks ABOVE the point are validated with, which this test
/// does not process.)
fn t12_trusted_set(source: &TestConsensus, pp: BlockHash, sink: BlockHash) -> Vec<TrustedBlock> {
    let mut hashes = vec![pp];
    hashes.extend(source.dag_traversal_manager().anticone(pp, std::iter::once(sink), None).expect("the anticone from the sink"));
    let mut blocks: Vec<TrustedBlock> = hashes
        .into_iter()
        .map(|hash| {
            let block = source.get_block(hash).expect("a block the source holds in full");
            let ghostdag = source.ghostdag_store().get_data(hash).expect("its GHOSTDAG data");
            TrustedBlock::new(block, ghostdag.as_ref().into())
        })
        .collect();
    blocks.sort_by(|a, b| a.block.header.blue_work.cmp(&b.block.header.blue_work));
    blocks
}

/// **Mainnet's ceiling on testnet-12 past genesis: a pruning point that MOVED, its proof built at
/// 225, validated by a node at genesis and applied by a staging node** (the 2026-09-25
/// mainnet-values review, LOW: the test above sees only the trivial proof of a genesis pruning
/// point, and every other pruning-proof test runs a hash lineage's params).
///
/// testnet-12's ruleset, with three depths shrunk so a pruning point is due inside a test — finality
/// 20, pruning 50 (`50 mod 20 = 10`, inside `(k, finality − k)` at k = 1) and the proof's `m` 10 —
/// and heartbeats mined as honest slots until the headers declare one. Measured, not assumed:
///
/// * **on heartbeats alone the node's pruning point does not move at all**: past a V2 bundle it is
///   capped by the PALW safe frontier (the deepest `Final` claim), and a chain that matured no work
///   has none. Until testnet-12's first claim is `Final` its nodes serve the genesis proof the test
///   above checks. The point the headers declare is then installed the way a header-syncing node
///   installs it (`intrusive_pruning_point_update`), standing in for the `Final` that would allow it;
/// * every level above 0 is genesis alone, and a level's root must lie in the past of the block `m`
///   deep on the level above — genesis — so **the level-0 proof is the whole history below the
///   pruning point**, every header of it, not the `2m` window a hash lineage's proof keeps. A
///   testnet-12 proof therefore grows with the chain until it meets the header budget
///   `(max_block_level + 1) × 2 × m`: 452,000 at 225 and m = 1,000, about 314 days of two-block
///   heartbeat slots below the pruning point (which trails the sink by the pruning depth, 74,920
///   DAA ≈ 104 days), 502,000 and about 348 days at 250 — sooner with every other block the chain
///   carries. Owed at either ceiling; 225 brings it about 34 days nearer;
/// * a node at genesis validates the proof (`validate_pruning_proof`, which derives each header's
///   level with the single lottery);
/// * a staging node applies it with the trusted set, then the pruning points (`apply_pruning_proof`,
///   `import_pruning_points`, which derive levels WITHOUT the single lottery — a heartbeat derives
///   none either way, so every stored level is 0 here), and builds the same proof back (the IBD
///   flow's sanity check).
#[tokio::test]
async fn t12_a_moved_pruning_point_s_proof_builds_validates_and_applies_at_225() {
    kaspa_core::log::try_init_logger("info");
    let (shipped, _bundle, premine, _floats) = t12_with_harness_cards();
    assert_eq!(shipped.params.max_block_level, 225, "testnet-12 runs mainnet's ceiling");
    // Shrunk AFTER the build: `validate_palw_v2` rightly refuses a pruning depth under the DNS BFT
    // gate's 5,274-blue-score walk (and under the claim lattice), and neither is what this test is
    // about — no claim is made and no stake moves, and the source keeps every block (archival) so
    // nothing the shrunk depth would prune is missing from what it serves.
    let mut config = shipped.clone();
    config.params.blockrate.finality_depth = 20;
    config.params.blockrate.pruning_depth = 50;
    config.params.pruning_proof_m = 10;
    config.is_archival = true;
    let m = config.params.pruning_proof_m;
    let genesis = config.params.genesis.hash;
    let mut ctx = t12_at_genesis(&config, &premine);
    let headers = ctx.consensus.virtual_processor().headers_store.clone();
    // Heartbeats until the headers DECLARE a pruning point (`header.pruning_point` is a function of
    // GHOSTDAG and the depths alone) at least `6m` deep — so a proof that kept only the `2m` window
    // would be told apart from one that keeps the whole history — and already
    // `anticone_finalization_depth` under the sink.
    let settle = config.params.anticone_finalization_depth() + 4;
    let mut slots = 0u64;
    let pp = loop {
        honest_slot(&mut ctx, 10_000 + 4 * slots).await;
        slots += 1;
        let sink = headers.get_header(ctx.consensus.get_sink()).unwrap();
        let declared_bs = headers.get_blue_score(sink.pruning_point).unwrap();
        if declared_bs >= 6 * m && sink.blue_score >= declared_bs + settle {
            break sink.pruning_point;
        }
        assert!(slots < 300, "no header declared a settled pruning point {} deep in {slots} slots", 6 * m);
    };
    // **The node does not move there on heartbeats alone.** Past a V2 bundle the pruning point is
    // capped by the PALW safe frontier — the deepest `Final` claim's blue score — and a chain that
    // matured no work has none (`palw_pruning_point_allowed_v2`, the pruning processor's Unit D gate).
    // So a testnet-12 node serves the genesis proof of the test above until its first claim is Final;
    // the proof asked about here is the one it serves after that.
    assert_eq!(ctx.consensus.pruning_point(), genesis, "heartbeats alone mature no work, so the frontier holds the pruning point");
    let sink = ctx.consensus.get_sink();
    let relay = headers.get_header(sink).unwrap();
    let trusted = t12_trusted_set(&ctx.consensus, pp, sink);
    assert_eq!(trusted.first().map(|tb| tb.block.hash()), Some(pp), "the trusted set starts at the pruning point");
    // Moved as a node syncing headers moves it (the IBD catch-up's `intrusive_pruning_point_update`,
    // which checks the point is a pruning sample, deep enough under the sink and on its chain) — the
    // one step a claim's `Final` would otherwise take.
    ctx.consensus
        .intrusive_pruning_point_update(pp, sink)
        .unwrap_or_else(|e| panic!("the declared pruning point {pp} is not a pruning point: {e}"));
    assert_eq!(ctx.consensus.pruning_point(), pp);
    let pp_header = headers.get_header_with_block_level(pp).unwrap();
    assert_eq!(pp_header.block_level, 0, "the pruning point is a heartbeat and buys no level");

    let proof = ctx.consensus.get_pruning_point_proof();
    assert_eq!(proof.len(), 225 + 1, "one proof level per block level");
    assert!(proof[1..].iter().all(|level| level.len() == 1 && level[0].hash == genesis), "above level 0 every level is genesis");
    // past(pp) ∪ {pp}, walked from the headers the source stored.
    let mut past = BlockHashSet::default();
    let mut stack = vec![pp];
    while let Some(hash) = stack.pop() {
        if past.insert(hash) && hash != genesis {
            stack.extend(headers.get_header(hash).unwrap().direct_parents().iter().copied());
        }
    }
    let level0: BlockHashSet = proof[0].iter().map(|h| h.hash).collect();
    assert_eq!(level0.len(), proof[0].len(), "no header twice");
    assert_eq!(level0, past, "the level-0 proof is the whole history below the pruning point");
    assert!(proof[0].len() as u64 > 3 * (2 * m), "…three times the 2m window a hash lineage's proof keeps and more");
    let total: usize = proof.iter().map(|level| level.len()).sum();
    let budget = (config.params.max_block_level as u64 + 1) * 2 * m;
    assert!((total as u64) <= budget, "inside the header budget here ({total} of {budget})");
    // Headers a slot below the pruning point: each DAA tick is one slot of `I`.
    let per_slot = (proof[0].len() - 1) as f64 / pp_header.header.daa_score as f64;
    let days = |ceiling: u64| {
        ((ceiling + 1) * 2 * shipped.params.pruning_proof_m - ceiling) as f64 / per_slot * (I as f64 / 1_000.0) / 86_400.0
    };
    let days_at_two = |ceiling: u64| ((ceiling + 1) * 2 * shipped.params.pruning_proof_m - ceiling) as f64 / 2.0 * 120.0 / 86_400.0;
    eprintln!(
        "[t12-proof-225] declared after {slots} slots: pruning point at blue score {}, DAA {}; proof {total} headers, {} at level 0 = \
         past(pp) ({per_slot:.2} a slot here); at testnet-12's m = {} the budget ((mbl + 1) x 2 x m) is {} headers = {:.0} days of \
         two-block slots below the pruning point ({:.0} at this test's rate); at 250: {} headers = {:.0} days ({:.0})",
        pp_header.header.blue_score,
        pp_header.header.daa_score,
        proof[0].len(),
        shipped.params.pruning_proof_m,
        (225u64 + 1) * 2 * shipped.params.pruning_proof_m,
        days_at_two(225),
        days(225),
        (250u64 + 1) * 2 * shipped.params.pruning_proof_m,
        days_at_two(250),
        days(250)
    );

    // A node at genesis validates it against the source's sink.
    let fresh = t12_at_genesis(&config, &premine);
    fresh
        .consensus
        .validate_pruning_proof(&proof, &PruningProofMetadata::new(relay.blue_work))
        .unwrap_or_else(|e| panic!("a node at genesis refused the proof: {e}"));

    // A staging node applies it and the pruning points, as the IBD flow does before it processes the
    // trusted set (which, at these shrunk depths, would send the DNS gate's walk below the pruning
    // point — a property of the test's depths, not of the proof).
    let mut staging_config = config.clone();
    staging_config.process_genesis = false;
    staging_config.is_archival = false;
    let staging = TestContext::new(TestConsensus::new(&staging_config));
    staging.consensus.apply_pruning_proof((*proof).clone(), &trusted).unwrap_or_else(|e| panic!("the proof did not apply: {e}"));
    staging
        .consensus
        .import_pruning_points(ctx.consensus.pruning_point_headers())
        .unwrap_or_else(|e| panic!("the pruning points: {e}"));
    let staged = staging.consensus.virtual_processor().headers_store.clone();
    for header in proof[0].iter().filter(|h| h.hash != genesis) {
        assert_eq!(staged.get_header_with_block_level(header.hash).unwrap().block_level, 0, "{}: applied at level 0", header.hash);
    }
    assert_eq!(staging.consensus.pruning_point(), pp, "the staging node stands at the source's pruning point");
    let rebuilt = staging.consensus.get_pruning_point_proof();
    for (level, (sent, built)) in proof.iter().zip(rebuilt.iter()).enumerate() {
        assert_eq!(
            sent.iter().map(|h| h.hash).collect::<BlockHashSet>(),
            built.iter().map(|h| h.hash).collect::<BlockHashSet>(),
            "level {level}: the staging node builds the proof it was sent"
        );
    }
}
