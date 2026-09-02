use crate::{
    consensus::test_consensus::TestConsensus,
    model::{services::reachability::ReachabilityService, stores::headers::HeaderStoreReader},
    pipeline::virtual_processor::ContributionWeight,
};
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{
    BlockHashSet,
    api::ConsensusApi,
    block::{Block, BlockTemplate, MutableBlock, TemplateBuildMode, TemplateTransactionSelector},
    blockhash,
    blockstatus::BlockStatus,
    coinbase::MinerData,
    config::{
        ConfigBuilder,
        params::{DEVNET_PARAMS, MAINNET_PARAMS},
    },
    dns_finality::p2pkh_mldsa87_spk,
    tx::{Transaction, TransactionOutpoint},
};
use std::{collections::VecDeque, thread::JoinHandle};

struct OnetimeTxSelector {
    txs: Option<Vec<Transaction>>,
    rejected: bool,
}

impl OnetimeTxSelector {
    fn new(txs: Vec<Transaction>) -> Self {
        Self { txs: Some(txs), rejected: false }
    }
}

impl TemplateTransactionSelector for OnetimeTxSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        // First call returns the fixed set; subsequent calls (the builder's
        // rejection re-selection loop) return empty so the loop terminates
        // instead of unwrapping `None`.
        self.txs.take().unwrap_or_default()
    }

    fn reject_selection(&mut self, _tx_id: kaspa_consensus_core::tx::TransactionId) {
        // Record the rejection so `is_successful` reports failure and
        // `build_block_template` surfaces the per-tx `RuleError` (instead of
        // panicking or silently dropping the tx).
        self.rejected = true;
    }

    fn is_successful(&self) -> bool {
        !self.rejected
    }
}

struct TestContext {
    consensus: TestConsensus,
    join_handles: Vec<JoinHandle<()>>,
    miner_data: MinerData,
    simulated_time: u64,
    current_templates: VecDeque<BlockTemplate>,
    current_tips: BlockHashSet,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        self.consensus.shutdown(std::mem::take(&mut self.join_handles));
    }
}

impl TestContext {
    fn new(consensus: TestConsensus) -> Self {
        let join_handles = consensus.init();
        let genesis_hash = consensus.params().genesis.hash;
        let simulated_time = consensus.params().genesis.timestamp;
        Self {
            consensus,
            join_handles,
            miner_data: new_miner_data(),
            simulated_time,
            current_templates: Default::default(),
            current_tips: BlockHashSet::from_iter([genesis_hash]),
        }
    }

    pub fn build_block_template_row(&mut self, nonces: impl Iterator<Item = usize>) -> &mut Self {
        for nonce in nonces {
            self.simulated_time += self.consensus.params().target_time_per_block();
            self.current_templates.push_back(self.build_block_template(nonce as u64, self.simulated_time));
        }
        self
    }

    pub fn assert_row_parents(&mut self) -> &mut Self {
        for t in self.current_templates.iter() {
            assert_eq!(self.current_tips, BlockHashSet::from_iter(t.block.header.direct_parents().iter().copied()));
        }
        self
    }

    pub async fn validate_and_insert_row(&mut self) -> &mut Self {
        self.current_tips.clear();
        while let Some(t) = self.current_templates.pop_front() {
            self.current_tips.insert(t.block.header.hash);
            self.validate_and_insert_block(t.block.to_immutable()).await;
        }
        self
    }

    pub async fn build_and_insert_disqualified_chain(&mut self, mut parents: Vec<BlockHash>, len: usize) -> BlockHash {
        // The chain will be disqualified since build_block_with_parents builds utxo-invalid blocks
        for _ in 0..len {
            self.simulated_time += self.consensus.params().target_time_per_block();
            let b = self.build_block_with_parents(parents, 0, self.simulated_time);
            parents = vec![b.header.hash];
            self.validate_and_insert_block(b.to_immutable()).await;
        }
        parents[0]
    }

    /// A template with the timestamp the BUILDER chose, which is what a real miner mines.
    ///
    /// `build_block_template` below re-stamps, and on a network whose EVM lane is active that is
    /// silently fatal: `evm_commitment_root` is computed during the build over
    /// `EvmBlockInput { header_timestamp_ms, .. }`, so moving the timestamp afterwards leaves the
    /// header committing a root for a block that no longer exists and every such block is
    /// disqualified at `evm_commitment_root mismatch`. It went unnoticed because the lane is inert
    /// (`u64::MAX`) on every network the harness builds against except the RC's — and because
    /// `kaspa-consensus` had never been compiled with `--features evm`.
    ///
    /// `kaspad`'s own producer reads `template.block.header.timestamp` and never writes it, so
    /// this is the harness diverging from the miner, not the miner from the chain.
    pub fn build_block_template_keeping_time(&self, nonce: u64) -> BlockTemplate {
        let mut t = self
            .consensus
            .build_block_template(
                self.miner_data.clone(),
                Box::new(OnetimeTxSelector::new(Default::default())),
                TemplateBuildMode::Standard,
            )
            .unwrap();
        t.block.header.nonce = nonce;
        if t.block.header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2 {
            t.block.header.palw_commitment = self.consensus.palw_v2_test_carriage(&t.block.header);
        }
        t.block.header.finalize();
        t
    }

    pub fn build_block_template(&self, nonce: u64, timestamp: u64) -> BlockTemplate {
        let mut t = self
            .consensus
            .build_block_template(
                self.miner_data.clone(),
                Box::new(OnetimeTxSelector::new(Default::default())),
                TemplateBuildMode::Standard,
            )
            .unwrap();
        // See `build_block_template_keeping_time`: moving the timestamp after the build breaks
        // `evm_commitment_root`, so the re-stamp is only legal while the lane is inert. Asserted
        // rather than assumed — a silently invalid block is the failure this whole method caused.
        assert!(
            !self.consensus.params().is_evm_active(t.block.header.daa_score),
            "re-stamping a template whose EVM lane is active invalidates its evm_commitment_root — use build_block_template_keeping_time"
        );
        t.block.header.timestamp = timestamp;
        t.block.header.nonce = nonce;
        // ADR-0042 Decision 3a: the template DECLARES algo-6 on a `ConsensusV2` network but does
        // not carry an attempt — the carriage is the miner's, and producing one is what the work
        // IS. The harness stands in for that miner, and it must stamp after the timestamp and the
        // nonce because the challenge binds both: an envelope built before them would be an
        // attempt mounted at a different position, which is exactly what the finalizer refuses.
        if t.block.header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2 {
            t.block.header.palw_commitment = self.consensus.palw_v2_test_carriage(&t.block.header);
        }
        t.block.header.finalize();
        t
    }

    pub fn build_block_with_parents(&self, parents: Vec<BlockHash>, nonce: u64, timestamp: u64) -> MutableBlock {
        let mut b = self.consensus.build_block_with_parents_and_transactions(blockhash::NONE, parents, Default::default());
        b.header.timestamp = timestamp;
        b.header.nonce = nonce;
        // Same reason as `build_block_template`: the challenge binds the timestamp and the nonce,
        // so the carriage is stamped after they are final.
        if b.header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2 {
            b.header.palw_commitment = self.consensus.palw_v2_test_carriage(&b.header);
        }
        b.header.finalize(); // This overrides the NONE hash we passed earlier with the actual hash
        b
    }

    pub async fn validate_and_insert_block(&mut self, block: Block) -> &mut Self {
        let status = self.consensus.validate_and_insert_block(block).virtual_state_task.await.unwrap();
        assert!(status.has_block_body());
        self
    }

    /// kaspa-pq ADR-0018 §G (DAG-2 harness): build ONE block from a template with a
    /// custom `miner_data` (so the coinbase can pay a known, spendable key) and a
    /// custom tx set fed through `OnetimeTxSelector` (so the coinbase is computed
    /// correctly and the block can reach a valid UTXO tip — unlike
    /// `build_block_with_parents_and_transactions`, which builds a utxo-invalid
    /// coinbase). Parents are auto-selected from the current virtual tips, so the
    /// caller just mines a linear chain. Returns the inserted (immutable) block so
    /// the caller can read its coinbase outputs / daa score. NOTE: an invalid tx in
    /// `txs` makes the template builder call `OnetimeTxSelector::reject_selection`,
    /// which panics — i.e. an invalid funded spend fails loudly here.
    pub async fn mine_block(&mut self, miner_data: MinerData, txs: Vec<Transaction>) -> Block {
        self.simulated_time += self.consensus.params().target_time_per_block();
        let mut t = self
            .consensus
            .build_block_template(miner_data, Box::new(OnetimeTxSelector::new(txs)), TemplateBuildMode::Standard)
            .unwrap();
        t.block.header.timestamp = self.simulated_time;
        t.block.header.nonce = self.simulated_time;
        t.block.header.finalize();
        let block = t.block.to_immutable();
        self.validate_and_insert_block(block.clone()).await;
        block
    }

    pub fn assert_tips(&mut self) -> &mut Self {
        assert_eq!(BlockHashSet::from_iter(self.consensus.get_tips().into_iter()), self.current_tips);
        self
    }

    pub fn assert_tips_num(&mut self, expected_num: usize) -> &mut Self {
        assert_eq!(BlockHashSet::from_iter(self.consensus.get_tips().into_iter()).len(), expected_num);
        self
    }

    pub fn assert_virtual_parents_subset(&mut self) -> &mut Self {
        assert!(self.consensus.get_virtual_parents().is_subset(&self.current_tips));
        self
    }

    pub fn assert_valid_utxo_tip(&mut self) -> &mut Self {
        // Assert that at least one body tip was resolved with valid UTXO
        assert!(self.consensus.body_tips().iter().copied().any(|h| self.consensus.block_status(h) == BlockStatus::StatusUTXOValid));
        self
    }
}

#[tokio::test]
async fn template_mining_sanity_test() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    let rounds = 10;
    let width = 3;
    for _ in 0..rounds {
        ctx.build_block_template_row(0..width)
            .assert_row_parents()
            .validate_and_insert_row()
            .await
            .assert_tips()
            .assert_virtual_parents_subset()
            .assert_valid_utxo_tip();
    }
}

/// **ADR-0042 Unit C, through the real pipeline: the PALW V2 state is ABSENT — not idle — on a
/// network with no V2 bundle.**
///
/// The wiring note this closes says the fork-choice and state sites "consume these functions when
/// `PalwConsensusMode::ConsensusV2` exists to demand them — a dead handle in today's blue-work
/// pipeline would be surface without semantics". This is that property, asserted against real
/// block processing rather than against the gate's source: four rows of blocks go through the
/// whole virtual processor on a shipped-shaped network, and the PALW store is untouched at the
/// end — no genesis tip, no delta rows.
///
/// The V2 half is `palw_v2_state_walks_with_the_utxo_diff` below, which the harness can now build
/// because `TestConsensus::build_header_with_parents` stamps a position-bound attempt envelope on
/// a `ConsensusV2` network.
#[tokio::test]
async fn a_network_without_a_v2_bundle_keeps_no_palw_state() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    assert!(
        config.params.palw_consensus_mode.required_algo_id().is_none(),
        "the fixture must be a network with no V2 bundle — that is the property under test"
    );

    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..4 {
        ctx.build_block_template_row(0..2).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let store = ctx.consensus.virtual_processor().palw_state_v2_store.read();
    assert!(store.tip_record().unwrap().is_none(), "no V2 bundle, no PALW tip — the walk is absent, not idle");
    assert!(
        store.iter_delta_blocks().next().is_none(),
        "no V2 bundle, no delta rows — real block processing wrote nothing into the PALW store"
    );
}

/// **The other half: on a `ConsensusV2` network the state really walks with the diff.**
///
/// Every chain block writes its delta in its own batch, the tip ends at the walk's own end point,
/// and — the property the whole store shape exists for — folding the deltas from genesis
/// reproduces the materialized tip exactly. A walk that drifted from its own deltas would be a
/// node whose resumed state and whose replayed state disagree, which is P0-4 wearing a different
/// hat.
#[tokio::test]
async fn palw_v2_state_walks_with_the_utxo_diff() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_state_v2::{PalwChainStateV2, apply_delta_v2};

    let catalog = palw_v2_test_catalog();
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(palw_v2_test_bundle(&catalog));
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    // The fixture is a ruleset a node would really boot on, not a shape that merely type-checks.
    config.params.validate_palw_v2().expect("the fixture bundle is a runnable ruleset");
    let state_params = match &config.params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.clone(),
        _ => unreachable!(),
    };
    let genesis_hash = config.params.genesis.hash;

    let consensus = TestConsensus::new(&config);
    {
        let store = consensus.virtual_processor().palw_state_v2_store.read();
        let tip = store.tip_record().unwrap().expect("a V2 network installs its genesis tip");
        assert_eq!(tip.block, genesis_hash, "the zero point stands at genesis");
    }

    let mut ctx = TestContext::new(consensus);
    for _ in 0..4 {
        ctx.build_block_template_row(0..2).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let store = ctx.consensus.virtual_processor().palw_state_v2_store.read();
    let tip = store.tip_record().unwrap().expect("the walk wrote a tip");
    assert_ne!(tip.block, genesis_hash, "the walk advanced past genesis — the chain really was built");

    // The tip reloads under its OWN committed root: the same refusal a peer-supplied pruning
    // carriage gets, run against what this node just wrote.
    let (block, state) = store.load_tip(&state_params).unwrap().expect("the tip loads");
    assert_eq!(block, tip.block);
    assert_eq!(state.state_root(), tip.state_root, "the stored root is the state's own");
    assert!(store.has_delta(tip.block).unwrap(), "the tip block's own delta is on disk");

    // Fold every delta from genesis and compare. This is the differential the store exists to
    // make checkable — resume and replay must be the same state.
    // Genesis is INCLUDED now: it applies the bundle's registration list, so it has a delta like
    // every other chain block. Stopping the walk above it (the shape this test had before the
    // registrations landed) folds from a state the chain never had, and the second delta refuses
    // to apply — which is `revert/apply_delta_v2` doing its job, not a test detail.
    let chain: Vec<_> = ctx
        .consensus
        .virtual_processor()
        .reachability_service
        .default_backward_chain_iterator(tip.block)
        .take_while(|h| *h != kaspa_consensus_core::blockhash::ORIGIN)
        .collect();
    assert!(chain.len() > 1, "the fixture must produce chain blocks beyond genesis for the fold to mean anything");
    let mut folded = PalwChainStateV2::genesis();
    for block in chain.iter().rev() {
        let (_, delta) = store.delta_of(*block).expect("every chain block on the walk has a delta");
        folded = apply_delta_v2(&folded, &delta, &state_params).expect("the deltas fold");
    }
    assert_eq!(folded.state_root(), state.state_root(), "folding the deltas reproduces the materialized tip");
}

/// **ADR-0060 Decisions 1 + 2, through the real pipeline.** A heartbeat block — bondless,
/// carriage-less, fee-only — is accepted by a ConsensusV2 network, folds through the V2 state
/// walk as `PalwBlockWorkV3::None` (the clock ticking IS the sweep advancing), weighs exactly ε
/// in its child's blue work, and is refused when it violates the slot rule or declares a
/// subsidy. The ramp's fast arm is exercised end-to-end: with every bonded lane silent past six
/// hours, the interval is the full cadence.
#[tokio::test]
async fn palw_heartbeat_blocks_tick_the_clock_and_weigh_epsilon() {
    use kaspa_consensus_core::palw_heartbeat_v1 as hb;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    // **The lane RUNS here, because ADR-0066 gave it a fence that can be armed.**
    //
    // This test used to skip: the lane was a `const bool` set to false, so the only way to run it
    // was to rebuild the binary — which is also the only way an operator could have turned the
    // lane on, and that was the defect. Arming `Params::palw_heartbeat` is a config change, so
    // the test arms it, and the thing under test is the thing an operator would deploy.
    kaspa_core::log::try_init_logger("info");
    let catalog = palw_v2_test_catalog();
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(palw_v2_test_bundle(&catalog));
            *p = p.clone().with_palw_v2_cadence();
            p.palw_heartbeat = Some(kaspa_consensus_core::config::params::PalwHeartbeatV1 {
                activation: kaspa_consensus_core::config::params::ForkActivation::always(),
                work_log2: kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_WORK_LOG2,
                max_per_mergeset: kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET,
            });
        })
        .build();
    config.params.validate_palw_v2().expect("the fixture bundle is a runnable ruleset");
    assert!(config.params.palw_heartbeat_lane_open_at(0), "the fence is armed, so the lane is open");
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Two bonded (algo-6) blocks so the chain has real bonded production behind it.
    for _ in 0..2 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    // 1) Adapt an ordinary template into the lane. The carriage is emptied, the declared subsidy
    //    is zero — and the `bits` are the ones the template already had.
    ctx.simulated_time += 120_000;
    let template = ctx.build_block_template(7, ctx.simulated_time);
    let global_bits = template.block.header.bits;
    let (hb_template, earliest) =
        ctx.consensus.virtual_processor().heartbeat_adapt_block_template(template).expect("the lane adapts a template");
    let hb_header = hb_template.block.header.clone();
    assert_eq!(hb_header.pow_algo_id, hb::PALW_HEARTBEAT_ALGO_ID);
    assert!(hb_header.palw_commitment.is_empty(), "a heartbeat carries no attempt — that is the lane");

    // **ADR-0066 Decision 1, as an assertion: the lane's price is NOT in `header.bits`.**
    //
    // This is the fix for the finding that withdrew the first implementation. The old adapter
    // overwrote `bits` with the lane's own 2²⁴-hard retarget, and those rows then sat in the
    // GLOBAL difficulty window — a window that filled with them demanded work 33,554,432, no
    // bonded block could re-enter it, and the chain was heartbeat-only for good. A heartbeat now
    // carries the same `bits` any other block at this position carries, so it is an ordinary
    // difficulty row and the average has nothing to run away from.
    assert_eq!(
        hb_header.bits, global_bits,
        "a heartbeat header carries the GLOBAL expected bits — the lane's own price lives in \
         StateLayer0, where nothing averages it"
    );

    assert!(earliest <= hb_header.timestamp, "the first heartbeat after a full hour has its slot");
    let payload = &hb_template.block.transactions[0].payload;
    assert_eq!(u64::from_le_bytes(payload[8..16].try_into().unwrap()), 0, "Decision 1.4: the declared subsidy is zero");
    assert_ne!(hb_header.palw_state_root, Default::default(), "a heartbeat chain block commits the parent state root");

    // 2) The pipeline accepts it and the V2 walk folds it (work = None) — the sink moves.
    let hb_hash = hb_header.hash;
    ctx.validate_and_insert_block(hb_template.block.clone().to_immutable()).await.assert_valid_utxo_tip();
    // The slot rule pushed the heartbeat a full interval past its parent, so the simulated clock
    // has to follow it or every later template is `TimeTooOld` against its own ancestor.
    ctx.simulated_time = ctx.simulated_time.max(hb_header.timestamp);

    // 3) ε: the bonded child credits the heartbeat exactly one unit of blue work — a bonded
    //    block is ~10⁶ at these bits, so the lane is byte-for-byte "near-weightless".
    ctx.simulated_time += 120_000;
    let child = ctx.build_block_template(8, ctx.simulated_time);
    assert!(child.block.header.direct_parents().contains(&hb_hash), "the child extends the heartbeat");
    assert_eq!(
        child.block.header.blue_work - hb_header.blue_work,
        kaspa_consensus_core::BlueWorkType::from(hb::HEARTBEAT_BLUE_WORK_EPSILON),
        "the heartbeat's whole fork-choice contribution is ε"
    );
    ctx.validate_and_insert_block(child.block.clone().to_immutable()).await.assert_valid_utxo_tip();
    let child_ts = child.block.header.timestamp;

    // 4) **The slot rule, one block deep.** The selected parent is now the bonded child, so the
    //    interval is the nominal hour, measured from THAT header — not from a walk over the
    //    window. A heartbeat inside it is refused.
    ctx.simulated_time += 1_000;
    let template = ctx.build_block_template(9, ctx.simulated_time);
    let (early_hb, earliest) =
        ctx.consensus.virtual_processor().heartbeat_adapt_block_template(template).expect("adapt reports the slot");
    assert_eq!(earliest, child_ts + hb::HEARTBEAT_NOMINAL_INTERVAL_MS, "a producing chain: one heartbeat per hour");
    let mut too_early = early_hb.block.clone();
    too_early.header.timestamp = child_ts + 1_000;
    too_early.header.finalize();
    match ctx.consensus.validate_and_insert_block(too_early.to_immutable()).virtual_state_task.await {
        Err(kaspa_consensus_core::errors::block::RuleError::HeartbeatTooEarly(_, _, last, interval)) => {
            assert_eq!(last, child_ts, "the boundary is the SELECTED PARENT's timestamp");
            assert_eq!(interval, hb::HEARTBEAT_NOMINAL_INTERVAL_MS);
        }
        other => panic!("a heartbeat inside the slot must be HeartbeatTooEarly, got {other:?}"),
    }

    // 5) Past the interval it is admitted — the clock the doctrine promises a stalled network.
    ctx.simulated_time = child_ts + hb::HEARTBEAT_NOMINAL_INTERVAL_MS + 60_000;
    let template = ctx.build_block_template(10, ctx.simulated_time);
    let (ramped, earliest) = ctx.consensus.virtual_processor().heartbeat_adapt_block_template(template).unwrap();
    assert!(earliest <= ramped.block.header.timestamp, "past the interval the slot is open");
    let ramped_ts = ramped.block.header.timestamp;
    ctx.validate_and_insert_block(ramped.block.clone().to_immutable()).await.assert_valid_utxo_tip();
    ctx.simulated_time = ctx.simulated_time.max(ramped_ts);

    // 6) **And now the recovery cadence, which is what makes a stopped chain recoverable.** The
    //    selected parent is a heartbeat, so the interval drops from an hour to one block time —
    //    asserted as a DIFFERENCE against step 4, where the same call returned the hour.
    ctx.simulated_time = ramped_ts + hb::HEARTBEAT_RECOVERY_INTERVAL_MS;
    let template = ctx.build_block_template(11, ctx.simulated_time);
    let (recovering, earliest) = ctx.consensus.virtual_processor().heartbeat_adapt_block_template(template).unwrap();
    assert_eq!(
        earliest,
        ramped_ts + hb::HEARTBEAT_RECOVERY_INTERVAL_MS,
        "behind a heartbeat the lane runs at cadence, not at the calm-network hour"
    );
    assert!(hb::HEARTBEAT_RECOVERY_INTERVAL_MS < hb::HEARTBEAT_NOMINAL_INTERVAL_MS);
    ctx.validate_and_insert_block(recovering.block.clone().to_immutable()).await.assert_valid_utxo_tip();
    let recovering_ts = recovering.block.header.timestamp;
    ctx.simulated_time = ctx.simulated_time.max(recovering_ts);

    // 6b) **With the fence closed the adapter refuses, instead of handing back a block every
    //     peer would reject.** Built as a separate consensus because the fence is a params value:
    //     same bundle, same everything, `palw_heartbeat` unset.
    {
        let closed = ConfigBuilder::new(MAINNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|p| {
                p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(palw_v2_test_bundle(&catalog));
                *p = p.clone().with_palw_v2_cadence();
            })
            .build();
        assert!(closed.params.palw_heartbeat.is_none());
        let mut shut = TestContext::new(TestConsensus::new(&closed));
        shut.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        shut.simulated_time += 120_000;
        let t = shut.build_block_template(20, shut.simulated_time);
        match shut.consensus.virtual_processor().heartbeat_adapt_block_template(t) {
            Err(kaspa_consensus_core::errors::block::RuleError::UnknownPowAlgoId(id)) => {
                assert_eq!(id, hb::PALW_HEARTBEAT_ALGO_ID, "refused with the validator's own answer");
            }
            other => panic!("a closed lane must refuse to build, got {other:?}"),
        }
    }

    // 7) Decision 1.4 is enforced, not advisory: the same heartbeat with its subsidy declared
    //    back at the full figure is refused as WrongSubsidy.
    ctx.simulated_time = recovering_ts + 240_000;
    let donor = ctx.build_block_template(12, ctx.simulated_time); // algo-6: full-subsidy coinbase
    let full_coinbase = donor.block.transactions[0].clone();
    let template = ctx.build_block_template(13, ctx.simulated_time + 1_000);
    let (greedy, _) = ctx.consensus.virtual_processor().heartbeat_adapt_block_template(template).unwrap();
    let mut greedy = greedy.block.clone();
    greedy.transactions[0] = full_coinbase;
    greedy.header.hash_merkle_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(greedy.transactions.iter());
    greedy.header.finalize();
    match ctx.consensus.validate_and_insert_block(greedy.to_immutable()).virtual_state_task.await {
        Err(kaspa_consensus_core::errors::block::RuleError::WrongSubsidy(expected, _)) => {
            assert_eq!(expected, 0, "the lane's subsidy is zero by rule");
        }
        other => panic!("a heartbeat declaring a subsidy must be WrongSubsidy, got {other:?}"),
    }
}

/// **ADR-0066 Decision 3 (finding F2), closed by ADR-0068 Phase 1: under the fence an attempt
/// block's blue work is the network constant — and without it, `calc_work(bits)` as before.**
///
/// The audit's finding 2: on a V2 preset the class lottery is the throttle, so the ambient bits
/// price a bonded block at ~2 — parity with two ε = 1 sibling heartbeats for ~280 kH/s. Both
/// sides of the fence are asserted as blue-work DIFFERENCES through the real pipeline, so the
/// number a peer actually adds is the number under test.
#[tokio::test]
async fn palw_attempt_blocks_weigh_the_constant_under_the_fence() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    kaspa_core::log::try_init_logger("info");
    let catalog = palw_v2_test_catalog();

    // Fence ON: two bonded blocks, the second merging exactly the first — its blue-work delta is
    // the first block's whole contribution, and the rule says that is the constant.
    let armed = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(palw_v2_test_bundle(&catalog));
            *p = p.clone().with_palw_v2_cadence();
            p.palw_attempt_work = Some(kaspa_consensus_core::config::params::PalwAttemptWorkV1 {
                activation: kaspa_consensus_core::config::params::ForkActivation::always(),
                work_log2: kaspa_consensus_core::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2,
                ticket_bucket_log2: kaspa_consensus_core::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2,
            });
        })
        .build();
    armed.params.validate_palw_v2().expect("the fixture bundle plus the attempt-work fence is a runnable ruleset");
    assert!(armed.params.palw_attempt_work_open_at(0), "the fence is armed, so the constant is in force");
    let mut ctx = TestContext::new(TestConsensus::new(&armed));
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    ctx.simulated_time += 120_000;
    let a = ctx.build_block_template(1, ctx.simulated_time);
    ctx.validate_and_insert_block(a.block.clone().to_immutable()).await.assert_valid_utxo_tip();
    ctx.simulated_time += 120_000;
    let b = ctx.build_block_template(2, ctx.simulated_time);
    assert_eq!(b.block.header.direct_parents(), &[a.block.header.hash], "b merges exactly a");
    let constant = kaspa_consensus_core::BlueWorkType::from(1u64 << kaspa_consensus_core::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2);
    assert_eq!(
        b.block.header.blue_work - a.block.header.blue_work,
        constant,
        "under the fence a bonded block's whole fork-choice contribution is the constant — \
         2^20, a million heartbeats, not the ambient 2"
    );
    ctx.validate_and_insert_block(b.block.clone().to_immutable()).await.assert_valid_utxo_tip();

    // Fence OFF: the same two-block shape prices the bonded block from its bits — whatever that
    // figure is at these params, it is NOT the constant, or the fence gates nothing.
    let unfenced = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(palw_v2_test_bundle(&catalog));
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    assert!(unfenced.params.palw_attempt_work.is_none());
    let mut off = TestContext::new(TestConsensus::new(&unfenced));
    off.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    off.simulated_time += 120_000;
    let a = off.build_block_template(1, off.simulated_time);
    off.validate_and_insert_block(a.block.clone().to_immutable()).await.assert_valid_utxo_tip();
    off.simulated_time += 120_000;
    let b = off.build_block_template(2, off.simulated_time);
    assert_ne!(
        b.block.header.blue_work - a.block.header.blue_work,
        constant,
        "without the fence the old pricing stands — the constant only enters by activation"
    );
}

/// **The Phase 1 drill (ADR-0068 / ADR-0064): the heartbeat clock sweeps a stopped chain back
/// to life, unattended.** This is the block-600 wedge — the exposure deadlock ADR-0060 §1 names
/// first — re-run WITH the clock, as the regression it should have been from the start:
///
/// 1. The only bond fills its exposure ceiling: four claims open, the fifth block exists in the
///    DAG but cannot become the sink. Releasing a claim requires DAA to advance; before the
///    lane, only this bond could advance it. Held forever, by arithmetic.
/// 2. The heartbeat lane ticks — one block an hour behind a producing chain, the full cadence
///    behind itself — and every tick advances the DAA the timeout sweep runs on. No bond, no
///    claim, no operator.
/// 3. The sweep voids the stuck claims on the clock's time, exposure releases, and the SAME
///    bonded producer's next block becomes the sink again. Nobody restarted anything.
#[tokio::test]
async fn the_heartbeat_clock_sweeps_a_stopped_chain_back_to_life() {
    use kaspa_consensus_core::palw_heartbeat_v1 as hb;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    kaspa_core::log::try_init_logger("info");
    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_at_min_collateral(&catalog);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
            // The whole Phase 1 configuration: the clock, its width bound, and the attempt-work
            // constant — armed together, the way a deployment would.
            p.palw_heartbeat = Some(kaspa_consensus_core::config::params::PalwHeartbeatV1 {
                activation: kaspa_consensus_core::config::params::ForkActivation::always(),
                work_log2: kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_WORK_LOG2,
                max_per_mergeset: kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET,
            });
            p.palw_attempt_work = Some(kaspa_consensus_core::config::params::PalwAttemptWorkV1 {
                activation: kaspa_consensus_core::config::params::ForkActivation::always(),
                work_log2: kaspa_consensus_core::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2,
                ticket_bucket_log2: kaspa_consensus_core::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2,
            });
        })
        .build();
    config.params.validate_palw_v2().expect("the Phase 1 configuration is a runnable ruleset");
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // 1) Fill the ceiling: as many claims as it admits fit, the next block cannot become the
    //    sink. This is the wedge — the one party that could advance the clock is the one party
    //    that is stuck. The count is derived so the wedge, not an arithmetic coincidence, is
    //    what this reproduces.
    let fits = palw_v2_claims_that_fit(&bundle);
    assert!(fits >= 2, "the fixture must admit more than one claim for the wedge to be the thing under test");
    for _ in 0..fits {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    {
        let store = ctx.consensus.virtual_processor().palw_state_v2_store.read();
        let (_, state) = store.load_tip(&bundle.state).unwrap().unwrap();
        assert_eq!(state.claims_iter().count() as u64, fits, "the bond is at its ceiling");
    }
    let wedged_sink = ctx.consensus.get_sink();
    ctx.build_block_template_row(0..1).validate_and_insert_row().await;
    assert_eq!(ctx.consensus.get_sink(), wedged_sink, "one claim past the ceiling: the chain is wedged");

    // 2) The clock, and nothing else. First tick an hour out (the chain was producing a block
    //    ago), then the recovery cadence — each tick one DAA unit, sweeping the lifecycle
    //    windows a stuck bond never could. Claims void along the way; the loop watches for the
    //    release rather than assuming the horizon, and the ceiling on iterations IS the
    //    documented sweep horizon plus rebind slack.
    let mut released_at = None;
    for tick in 0u64..9_000 {
        let sink_ts = {
            let h = ctx.consensus.headers_store.get_header(ctx.consensus.get_sink()).unwrap();
            h.timestamp
        };
        let interval = if tick == 0 { hb::HEARTBEAT_NOMINAL_INTERVAL_MS } else { hb::HEARTBEAT_RECOVERY_INTERVAL_MS };
        ctx.simulated_time = ctx.simulated_time.max(sink_ts) + interval;
        let template = ctx.build_block_template(1_000 + tick, ctx.simulated_time);
        let (hb_template, _) = ctx.consensus.virtual_processor().heartbeat_adapt_block_template(template).expect("the lane adapts");
        ctx.validate_and_insert_block(hb_template.block.clone().to_immutable()).await;
        if tick % 100 == 99 {
            let store = ctx.consensus.virtual_processor().palw_state_v2_store.read();
            let (_, state) = store.load_tip(&bundle.state).unwrap().unwrap();
            if state.claims_iter().count() == 0 {
                released_at = Some(tick + 1);
                break;
            }
        }
    }
    let ticks = released_at.expect("the sweep must void every stuck claim within the lifecycle horizon — that is the doctrine");

    // 3) Unattended recovery: the SAME bond's next block binds a fresh claim and becomes the
    //    sink. Nothing was restarted, re-registered or forced — the clock did all of it.
    ctx.simulated_time += 120_000;
    let sink_before = ctx.consensus.get_sink();
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    assert_ne!(ctx.consensus.get_sink(), sink_before, "with exposure released the bonded lane re-enters by itself");
    {
        let store = ctx.consensus.virtual_processor().palw_state_v2_store.read();
        let (_, state) = store.load_tip(&bundle.state).unwrap().unwrap();
        assert_eq!(state.claims_iter().count(), 1, "one fresh claim — the ceiling has room again");
    }
    kaspa_core::info!("heartbeat sweep released the wedge after {ticks} clock ticks");
}

/// **F3a, closed (ADR-0068 Phase 1): a mergeset holds at most `PALW_HEARTBEAT_MAX_PER_MERGESET`
/// heartbeats — refused by consensus, and never built by a template.**
///
/// Sibling heartbeats share one selected parent, one admissible timestamp and one fixed price,
/// so nothing bounded how many the DAG accepts. The bound is a mergeset property beside
/// `mergeset_size_limit`: five siblings exist, a block merging all five is refused, a block
/// merging four is valid, and the template builder chunks — four this block, the fifth against
/// the next block's fresh budget — so a flood is absorbed at a bounded rate rather than refused
/// forever or accepted wholesale.
#[tokio::test]
async fn a_mergeset_holds_at_most_four_heartbeats_and_templates_chunk_the_rest() {
    use kaspa_consensus_core::palw_heartbeat_v1 as hb;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    kaspa_core::log::try_init_logger("info");
    let catalog = palw_v2_test_catalog();
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(palw_v2_test_bundle(&catalog));
            *p = p.clone().with_palw_v2_cadence();
            p.palw_heartbeat = Some(kaspa_consensus_core::config::params::PalwHeartbeatV1 {
                activation: kaspa_consensus_core::config::params::ForkActivation::always(),
                work_log2: kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_WORK_LOG2,
                max_per_mergeset: kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET,
            });
        })
        .build();
    config.params.validate_palw_v2().expect("the fixture bundle is a runnable ruleset");
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // A bonded base, then FIVE sibling heartbeats behind it — each valid alone (they share the
    // one admissible slot, which is exactly the width the slot rule cannot see).
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    ctx.simulated_time += 120_000;
    let base = ctx.build_block_template(1, ctx.simulated_time);
    let base_hash = base.block.header.hash;
    ctx.validate_and_insert_block(base.block.clone().to_immutable()).await.assert_valid_utxo_tip();
    ctx.simulated_time += hb::HEARTBEAT_NOMINAL_INTERVAL_MS + 60_000;
    let template = ctx.build_block_template(2, ctx.simulated_time);
    let (hb_template, _) =
        ctx.consensus.virtual_processor().heartbeat_adapt_block_template(template).expect("the lane adapts a template");
    assert_eq!(hb_template.block.header.direct_parents(), &[base_hash], "the siblings all sit behind the bonded base");
    let mut siblings = Vec::new();
    for nonce in 0..5u64 {
        let mut sib = hb_template.block.clone();
        sib.header.nonce = nonce;
        sib.header.finalize();
        siblings.push(sib.header.hash);
        ctx.validate_and_insert_block(sib.to_immutable()).await;
    }
    ctx.simulated_time = ctx.simulated_time.max(hb_template.block.header.timestamp);

    // Consensus: merging all five is one heartbeat too many; merging four is a valid block.
    ctx.simulated_time += 120_000;
    let over = ctx.build_block_with_parents(siblings.clone(), 77, ctx.simulated_time);
    match ctx.consensus.validate_and_insert_block(over.to_immutable()).virtual_state_task.await {
        Err(kaspa_consensus_core::errors::block::RuleError::MergeSetTooManyHeartbeats(count, bound)) => {
            assert_eq!(bound, kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET);
            assert!(count > bound, "refused at the first heartbeat past the bound");
        }
        other => panic!("five heartbeats in one mergeset must be MergeSetTooManyHeartbeats, got {other:?}"),
    }
    let four = ctx.build_block_with_parents(siblings[..4].to_vec(), 78, ctx.simulated_time + 1);
    ctx.consensus.validate_and_insert_block(four.to_immutable()).virtual_state_task.await.expect("four heartbeats fit the bound");

    // Template: pointed at the five-sibling DAG (before the manual merger reshaped it, the
    // template path saw the same five tips) the builder never over-merges — count the heartbeat
    // parents of the four-merger's OWN template-built successor: the fifth sibling rides a later
    // block's fresh budget instead of being merged here.
    ctx.simulated_time += 120_000;
    let next = ctx.build_block_template(3, ctx.simulated_time);
    let hb_parents = next.block.header.direct_parents().iter().filter(|p| siblings.contains(p)).count() as u64;
    let mergeset_hbs = {
        // The template's parents are not the whole story — the bound is on the MERGESET. Count
        // heartbeats the way the rule does.
        let gd = ctx.consensus.services.ghostdag_manager.ghostdag(next.block.header.direct_parents());
        gd.mergeset_blues.iter().chain(gd.mergeset_reds.iter()).filter(|h| siblings.contains(h)).count() as u64
    };
    assert!(
        mergeset_hbs <= kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET,
        "a template never builds what consensus refuses: {mergeset_hbs} heartbeats in its mergeset"
    );
    ctx.validate_and_insert_block(next.block.clone().to_immutable()).await.assert_valid_utxo_tip();
    let _ = hb_parents;

    // And the fifth is absorbed, not orphaned: within a couple of template rounds every sibling
    // is in the selected chain's past — the flood was chunked, which is the whole design.
    ctx.simulated_time += 120_000;
    let after = ctx.build_block_template(4, ctx.simulated_time);
    ctx.validate_and_insert_block(after.block.clone().to_immutable()).await.assert_valid_utxo_tip();
    let sink = ctx.consensus.get_sink();
    for sib in &siblings {
        assert!(
            ctx.consensus.services.reachability_service.is_dag_ancestor_of(*sib, sink),
            "every sibling heartbeat ends up merged — the bound chunks, it does not strand"
        );
    }
}

/// **F5, closed: a heartbeat CHAIN of any depth merges in one mergeset — a heartbeat TREE does
/// not** (ADR-0068 Phase 1, the drill's fifth finding).
///
/// The live drill manufactured the strand: a heavier bonded fork put five outage heartbeats in
/// its anticone, merging the tip meant dragging all five ancestors into one mergeset, the flat
/// bound refused, and — because the intermediates are not tips — no chunking path existed. 400+
/// templates excluded the branch forever, and whatever rode it would have unwound. But a CHAIN
/// is the lane doing its job through a long outage, already rate-priced by the slot ladder;
/// what F3a is about is WIDTH. So the rule is "flat bound, or one chain", and this test pins
/// both edges: the drill's exact shape now merges, and a tree — one root, many children, the
/// shape that would fool a chain-HEAD count — still refuses.
#[tokio::test]
async fn a_heartbeat_chain_of_any_depth_merges_but_a_tree_does_not() {
    use kaspa_consensus_core::palw_heartbeat_v1 as hb;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    kaspa_core::log::try_init_logger("info");
    let catalog = palw_v2_test_catalog();
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(palw_v2_test_bundle(&catalog));
            *p = p.clone().with_palw_v2_cadence();
            p.palw_heartbeat = Some(kaspa_consensus_core::config::params::PalwHeartbeatV1 {
                activation: kaspa_consensus_core::config::params::ForkActivation::always(),
                work_log2: kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_WORK_LOG2,
                max_per_mergeset: kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET,
            });
            p.palw_attempt_work = Some(kaspa_consensus_core::config::params::PalwAttemptWorkV1 {
                activation: kaspa_consensus_core::config::params::ForkActivation::always(),
                work_log2: kaspa_consensus_core::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2,
                ticket_bucket_log2: kaspa_consensus_core::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2,
            });
        })
        .build();
    config.params.validate_palw_v2().expect("the fixture bundle is a runnable ruleset");
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // A bonded base, then the drill's outage: SIX heartbeats in one chain, each the next's
    // selected parent — they become the sink chain, one ε at a time.
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    ctx.simulated_time += 120_000;
    let base = ctx.build_block_template(1, ctx.simulated_time);
    let base_hash = base.block.header.hash;
    ctx.validate_and_insert_block(base.block.clone().to_immutable()).await.assert_valid_utxo_tip();
    let mut chain = Vec::new();
    for nonce in 0..6u64 {
        ctx.simulated_time += if nonce == 0 { hb::HEARTBEAT_NOMINAL_INTERVAL_MS + 60_000 } else { hb::HEARTBEAT_RECOVERY_INTERVAL_MS };
        let template = ctx.build_block_template(2 + nonce, ctx.simulated_time);
        let (hb_template, _) = ctx.consensus.virtual_processor().heartbeat_adapt_block_template(template).expect("the lane adapts");
        ctx.simulated_time = ctx.simulated_time.max(hb_template.block.header.timestamp);
        chain.push(hb_template.block.header.hash);
        ctx.validate_and_insert_block(hb_template.block.clone().to_immutable()).await;
    }

    // The heavier bonded fork from BASE: a tip is compared by the work of its PAST, so the
    // fork's own 2^20 counts once a child stands on it — one extension block, and the bonded
    // branch (W + 2^20) dwarfs the heartbeat chain's W + 5ε. The six become a deep side
    // branch: the drill's stranded shape, exactly.
    let fork = ctx.build_block_with_parents(vec![base_hash], 77, ctx.simulated_time + 1_000);
    let fork_hash = fork.header.hash;
    ctx.consensus.validate_and_insert_block(fork.to_immutable()).virtual_state_task.await.expect("the bonded fork is valid");
    let fork_ext = ctx.build_block_with_parents(vec![fork_hash], 78, ctx.simulated_time + 1_500);
    let fork_ext_hash = fork_ext.header.hash;
    ctx.consensus.validate_and_insert_block(fork_ext.to_immutable()).virtual_state_task.await.expect("the extension is valid");

    // THE F5 EDGE: a block merging the chain's tip drags all six into one mergeset — over the
    // flat bound, admissible because they are one chain. Before the exemption this was the
    // permanent strand. The shape is asserted, not assumed: the merger's selected parent is the
    // bonded branch (F2's constant makes its past the heaviest), so the whole heartbeat chain
    // is its mergeset.
    let merger = ctx.build_block_with_parents(vec![fork_ext_hash, chain[5]], 79, ctx.simulated_time + 2_000);
    {
        let gd = ctx.consensus.services.ghostdag_manager.ghostdag(merger.header.direct_parents());
        assert_eq!(gd.selected_parent, fork_ext_hash, "the bonded branch's past outweighs six ε — F2's constant at work");
        let hbs_in_mergeset = gd.mergeset_blues.iter().chain(gd.mergeset_reds.iter()).filter(|h| chain.contains(h)).count() as u64;
        assert_eq!(hbs_in_mergeset, 6, "all six outage heartbeats land in ONE mergeset — the drill's exact strand shape");
        assert!(hbs_in_mergeset > kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET);
    }
    ctx.consensus
        .validate_and_insert_block(merger.to_immutable())
        .virtual_state_task
        .await
        .expect("six heartbeats in ONE chain merge in one mergeset — the drill's strand is absorbed");

    // And the TREE: two chained heartbeats, then four siblings on the second — six members, one
    // head, NOT one chain. A chain-head count would admit it; the pairwise order refuses it.
    // A FRESH consensus, deliberately: the chain half left a heavy disqualified branch behind,
    // and a template would merge its tips as extra parents — the sibling POV then out-weighs
    // the fork and the six split across past and mergeset, which is a different diagram than
    // the one under test (measured: hbs-in-mergeset dropped to three).
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    ctx.simulated_time += 120_000;
    let base2 = ctx.build_block_template(20, ctx.simulated_time);
    let base2_hash = base2.block.header.hash;
    ctx.validate_and_insert_block(base2.block.clone().to_immutable()).await.assert_valid_utxo_tip();
    let mut tree = Vec::new();
    for nonce in 0..2u64 {
        ctx.simulated_time += if nonce == 0 { hb::HEARTBEAT_NOMINAL_INTERVAL_MS + 60_000 } else { hb::HEARTBEAT_RECOVERY_INTERVAL_MS };
        let template = ctx.build_block_template(21 + nonce, ctx.simulated_time);
        let (hb_template, _) = ctx.consensus.virtual_processor().heartbeat_adapt_block_template(template).expect("the lane adapts");
        ctx.simulated_time = ctx.simulated_time.max(hb_template.block.header.timestamp);
        tree.push(hb_template.block.header.hash);
        ctx.validate_and_insert_block(hb_template.block.clone().to_immutable()).await;
    }
    ctx.simulated_time += hb::HEARTBEAT_RECOVERY_INTERVAL_MS;
    let template = ctx.build_block_template(23, ctx.simulated_time);
    let (sib_template, _) = ctx.consensus.virtual_processor().heartbeat_adapt_block_template(template).expect("the lane adapts");
    assert_eq!(sib_template.block.header.direct_parents(), &[tree[1]], "the siblings all ride the chain's tip");
    for nonce in 0..4u64 {
        let mut sib = sib_template.block.clone();
        sib.header.nonce = 100 + nonce;
        sib.header.finalize();
        tree.push(sib.header.hash);
        ctx.validate_and_insert_block(sib.to_immutable()).await;
    }
    ctx.simulated_time = ctx.simulated_time.max(sib_template.block.header.timestamp);
    let fork2 = ctx.build_block_with_parents(vec![base2_hash], 81, ctx.simulated_time + 1_000);
    let fork2_hash = fork2.header.hash;
    ctx.consensus.validate_and_insert_block(fork2.to_immutable()).virtual_state_task.await.expect("the second fork is valid");
    let fork2_ext = ctx.build_block_with_parents(vec![fork2_hash], 82, ctx.simulated_time + 1_500);
    let fork2_ext_hash = fork2_ext.header.hash;
    ctx.consensus.validate_and_insert_block(fork2_ext.to_immutable()).virtual_state_task.await.expect("the second extension is valid");
    let tree_merger =
        ctx.build_block_with_parents(vec![fork2_ext_hash, tree[2], tree[3], tree[4], tree[5]], 83, ctx.simulated_time + 2_000);
    {
        let gd = ctx.consensus.services.ghostdag_manager.ghostdag(tree_merger.header.direct_parents());
        assert_eq!(gd.selected_parent, fork2_ext_hash, "the bonded branch's past is the heaviest here too");
        let hbs_in_mergeset = gd.mergeset_blues.iter().chain(gd.mergeset_reds.iter()).filter(|h| tree.contains(h)).count() as u64;
        assert_eq!(hbs_in_mergeset, 6, "the whole tree — two chained, four siblings — lands in one mergeset");
    }
    match ctx.consensus.validate_and_insert_block(tree_merger.to_immutable()).virtual_state_task.await {
        Err(kaspa_consensus_core::errors::block::RuleError::MergeSetTooManyHeartbeats(count, bound)) => {
            assert_eq!(count, 6, "two chained plus four siblings — six members, not one chain");
            assert_eq!(bound, kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET);
        }
        other => panic!("a heartbeat TREE over the bound must be MergeSetTooManyHeartbeats, got {other:?}"),
    }
}

/// **Unit C step 5: the header's committed state root is CHECKED, in both of the two ways a
/// wrong one can be wrong.**
///
/// A commitment nothing verifies is a field, not a commitment. Two refusals, and the first is the
/// stronger one:
///
/// 1. **Moving the root alone is refused at the door.** The root is in the PRE-PoW preimage, so
///    moving it moves `pre_pow_hash`, so the carried attempt's challenge is no longer the one its
///    header position derives. A block cannot claim a wrong root and keep a valid ticket.
/// 2. **Re-mining the ticket for the new position gets a valid block with a lying root**, and
///    that is what the chain-level check is for: it never becomes the sink. Asserted the way this
/// **One solve, unbounded blocks: the receipt lane inherited the attempt lane's shape without its fix.**
///
/// The proof-of-work pre-image EXCLUDES `palw_commitment` (`PalwCommitmentDigestRule::Exclude`,
/// hashing/header.rs) while the block identity includes it. `validate_stateless_v3` checks the
/// signature's LENGTH and recomputes the challenge — and the challenge commits to `pre_pow_hash`,
/// `timestamp` and `nonce`, not to the signature. So on a node that never verifies the bytes, any
/// solved receipt block can be re-signed with garbage, re-hashed, and re-announced as a different
/// block, for free, without redoing the work. Every peer accepts, stores and relays each one.
///
/// The attempt lane (algo 6) documents this attack verbatim ten lines above the receipt arm and
/// verifies `validate_signature_v2` against it. The receipt arm was written without the second
/// half. This test is the receipt lane's copy of that gate; deleting the `validate_signature_v3`
/// call in `check_palw_carriage_stateless` makes it pass a junk signature again.
#[tokio::test]
async fn palw_v3_a_receipt_carriage_with_a_junk_signature_is_refused_at_the_header() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(palw_v2_test_bundle(&catalog));
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();

    let honest = ctx.build_block_template(7, ctx.simulated_time + 1);

    // A receipt-lane header whose spend binds THIS position correctly and carries a signature of
    // the right length and no authority — exactly what the stateless list accepts.
    let mut forged = honest.block.clone();
    forged.header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3;
    forged.header.palw_commitment = ctx.consensus.palw_v3_test_receipt_carriage(&forged.header, false);
    forged.header.finalize();

    match ctx.consensus.validate_and_insert_block(forged.to_immutable()).virtual_state_task.await {
        Err(kaspa_consensus_core::errors::block::RuleError::BadPalwCarriageAdmission { algo_id: 7, reason }) => {
            assert!(
                reason.to_lowercase().contains("signature"),
                "the refusal must NAME the signature — a receipt block refused for some other reason \
                 would let this test pass while the gate stayed unwired. got: {reason}"
            );
        }
        other => panic!("a receipt carriage with a junk signature must be refused at the header stage, got {other:?}"),
    }

    // **And the gate refuses the SIGNATURE, not the lane.** The same header with a real ML-DSA-87
    // signature over the same spend must get past this stage — it is refused later, on the stateful
    // facts (there is no such claim on this chain), and that is a different complaint. Without this
    // half, a test that merely rejected every algo-7 block would look identical to a working gate.
    let mut signed = honest.block.clone();
    signed.header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3;
    signed.header.palw_commitment = ctx.consensus.palw_v3_test_receipt_carriage(&signed.header, true);
    signed.header.finalize();
    // Anything other than a signature complaint is out of this gate's scope and deliberately not
    // asserted. Measured while writing this: a correctly signed spend naming a claim this chain
    // does not hold is ACCEPTED as a block — the stateful admission (`palw_v2_check_receipt_spend`)
    // runs on the chain candidate, not on every insertion, so a receipt block that never becomes
    // chain is never asked about its claim. That is a separate question and must not be smuggled
    // into this test's assertions.
    if let Err(kaspa_consensus_core::errors::block::RuleError::BadPalwCarriageAdmission { algo_id: 7, reason }) =
        ctx.consensus.validate_and_insert_block(signed.to_immutable()).virtual_state_task.await
    {
        assert!(
            !reason.to_lowercase().contains("signature"),
            "a correctly signed spend must not be refused for its signature; got: {reason}"
        );
    }
}

///    file's existing disqualification tests assert it — the sink does not move — because "did
///    not become the selected chain" is the property, and a status code is only its shadow.
#[tokio::test]
async fn a_block_committing_to_the_wrong_palw_state_root_is_disqualified() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(palw_v2_test_bundle(&catalog));
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let sink_before = ctx.consensus.get_sink();

    // The honest template commits to the parent's root — the state this chain really is at.
    let honest = ctx.build_block_template(7, ctx.simulated_time + 1);
    assert_ne!(honest.block.header.palw_state_root, kaspa_hashes::ZERO_HASH64, "a V2 template commits to a real root");

    // (1) Moving the commitment alone breaks the position binding.
    let mut liar = honest.block.clone();
    liar.header.palw_state_root = kaspa_hashes::Hash64::from_u64_word(0xBAD);
    liar.header.finalize();
    assert_ne!(liar.header.hash, honest.block.header.hash, "moving the commitment moves the identity");
    match ctx.consensus.validate_and_insert_block(liar.to_immutable()).virtual_state_task.await {
        Err(kaspa_consensus_core::errors::block::RuleError::BadPalwCarriageAdmission { algo_id: 6, .. }) => {}
        other => panic!("a moved state root must break the position binding, got {other:?}"),
    }

    // (2) A block that re-mines its ticket for the new position: valid carriage, lying root. It is
    //     a well-formed block, and it must never become the chain.
    let mut forger = honest.block.clone();
    forger.header.palw_state_root = kaspa_hashes::Hash64::from_u64_word(0xBAD);
    forger.header.palw_commitment = ctx.consensus.palw_v2_test_carriage(&forger.header);
    forger.header.finalize();
    let forger_hash = forger.header.hash;
    ctx.validate_and_insert_block(forger.to_immutable()).await;
    assert_ne!(forger_hash, sink_before);
    assert_eq!(ctx.consensus.get_sink(), sink_before, "a block whose committed root is not this chain's cannot become the sink");

    // …and the honest sibling, built the same way and differing only in the root, DOES advance the
    // chain — so the refusal is about the commitment and not about V2 blocks in general.
    let honest_hash = honest.block.header.hash;
    ctx.validate_and_insert_block(honest.block.to_immutable()).await;
    assert_eq!(ctx.consensus.get_sink(), honest_hash, "the honest commitment advances the chain");
}

/// **Unit C step 4: the beacon is derived from the candidate's chain, and a block cannot name
/// its own.**
///
/// This is the property the wiring note calls out by name — "a fact taken from the spending
/// block's own bytes would be the producer asserting its own randomness". The receipt lane's right
/// to spend a quantum is a DRAW, so whoever picks the beacon picks the winner.
///
/// Asserted at the derivation, which is where the property lives: the same slot walked from two
/// different starting points on ONE chain yields the same fact, and the fact names a block the
/// walk found rather than one anybody supplied. The end-to-end spend path additionally needs a
/// certified free-prompt claim on chain, which needs the FP worker's legs capture (see
/// `docs/palw-fp-wiring-atomicity.md`) — until then the pipeline's arm is exercised by the
/// derivation it calls, and by every V2 block going through it with no receipt lane in play.
#[tokio::test]
async fn the_beacon_fact_comes_from_the_chain_not_from_the_block() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            // Six chain blocks is six concurrent claims; the default bond backs four. Funded for
            // the chain it mines — see `palw_v2_test_bundle_funded_for`.
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(palw_v2_test_bundle_funded_for(&catalog, 16));
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..6 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor();
    let sink = ctx.consensus.get_sink();
    let sink_header = vp.headers_store.get_header(sink).unwrap();
    let sink_daa = sink_header.daa_score;

    // Every block this harness mines is attempt-class (algo 6), so every DAA at or below the sink
    // has a beacon and the fact's own witness must sit strictly below the slot.
    for slot in 0..=sink_daa {
        let fact = vp.palw_beacon_fact_of_candidate(sink, slot).expect("a chain of attempt blocks has a beacon for every slot");
        assert!(fact.beacon_daa >= slot, "the beacon is at or after the slot it draws for");
        assert!(
            fact.prev_attempt_daa < slot || (slot == 0 && fact.prev_attempt_daa == 0),
            "the witness is the last attempt block strictly below the slot: slot {slot}, witness {}",
            fact.prev_attempt_daa
        );
        // The named block is one this chain really contains, at the DAA the fact claims.
        let header = vp.headers_store.get_header(fact.beacon_block).expect("the beacon names a block on this chain");
        assert_eq!(header.daa_score, fact.beacon_daa);
    }

    // The derivation is a function of the CHAIN, not of the walker's position: walking from the
    // sink and from the sink's own parent agree wherever both can see the answer. Two nodes with
    // different sinks on one candidate therefore draw the same beacon, which is the whole point.
    let parent = vp.headers_store.get_header(sink).unwrap().direct_parents()[0];
    let parent_daa = vp.headers_store.get_header(parent).unwrap().daa_score;
    for slot in 0..=parent_daa {
        assert_eq!(
            vp.palw_beacon_fact_of_candidate(sink, slot).unwrap(),
            vp.palw_beacon_fact_of_candidate(parent, slot).unwrap(),
            "slot {slot}: the fact is the chain's, not the walker's"
        );
    }

    // A slot past the tip has no beacon yet, and that is a REFUSAL rather than a zero. A
    // derivation that invented one would be a draw nobody made.
    let err = vp.palw_beacon_fact_of_candidate(sink, sink_daa + 1).unwrap_err();
    assert!(matches!(err, kaspa_consensus_core::palw_fp_beacon_v3::PalwBeaconDeriveV3Error::NoBeaconYet { .. }), "got {err:?}");
}

/// **Unit D: one authority, and it is the candidate's — not the sink's.**
///
/// The order is what the IBD commit, the pruning ceiling and the deep-reorg gate all compare, so
/// the property that matters is that it is a function of the CANDIDATE. An order that answered
/// the same thing for every candidate would be a constant, and a constant authority is no
/// authority — every contest would fall through to whatever ran next.
#[tokio::test]
async fn the_palw_candidate_order_is_the_candidates_own() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(palw_v2_test_bundle(&catalog));
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..4 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor();
    let sink = ctx.consensus.get_sink();
    let parent = vp.headers_store.get_header(sink).unwrap().direct_parents()[0];

    let sink_order = vp.palw_candidate_order_v2(sink).expect("a V2 network orders its own sink");
    let parent_order = vp.palw_candidate_order_v2(parent).expect("and any candidate on its chain");
    // Each order names the candidate it is about. Without this key the comparator would not be
    // total, and two candidates equal in every weight would compare `Equal` — each node keeping
    // whichever it happened to hold.
    assert_eq!(sink_order.candidate, sink);
    assert_eq!(parent_order.candidate, parent);
    assert_ne!(sink_order, parent_order, "the order is the candidate's, not a constant");
    // `live_total >= safe_weight` is a type-level invariant of the constructor, asserted here on
    // real chain data rather than on a fixture.
    assert!(sink_order.live_total >= sink_order.safe_weight);

    // A network with no V2 bundle has no order at all — the three consumers fall through to what
    // they did before, which is what keeps every shipped preset unchanged.
    let plain = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let plain_consensus = TestConsensus::new(&plain);
    let _lt = plain_consensus.init();
    let plain_sink = plain_consensus.get_sink();
    assert!(
        plain_consensus.virtual_processor().palw_candidate_order_v2(plain_sink).is_none(),
        "no bundle, no order — the authority is absent rather than defaulted"
    );
    // …and its pruning ceiling is trivially satisfied, so pruning is unchanged there.
    assert!(plain_consensus.virtual_processor().palw_pruning_point_allowed_v2(plain_sink));
}

/// **Unit D, site 1: the tip order is the PALW authority, and the DAG agrees with it.**
///
/// Wiring tip selection is the site that fought back twice, and both fights were the same fact:
/// GHOSTDAG's `find_selected_parent` is `max by blue_work`, and `pick_virtual_parents` ASSERTS
/// that the sink the search chose is that maximum. A sink ordered by PALW weight and a DAG whose
/// selected parent is ordered by blue work are two canonical chains inside one node — the P0-5
/// this unit exists to close, arriving through the floor.
///
/// The repair is to keep that assumption TRUE rather than to weaken the assert: under the PALW
/// order, virtual parent candidates heavier than the sink are filtered out. It costs liveness
/// (virtual merges fewer tips this round; the excluded ones stay in the DAG) and not safety.
///
/// This pins the invariant on a real V2 chain, and pins the scoping too: applying the filter
/// unconditionally broke two blue-work tests, because there the sink IS the maximum and `<` also
/// drops the equal-work siblings virtual is supposed to merge.
#[tokio::test]
async fn palw_v2_sink_is_the_blue_work_maximum_of_its_virtual_parents() {
    use crate::model::stores::ghostdag::GhostdagStoreReader;
    use crate::model::stores::virtual_state::VirtualStateStoreReader;
    use kaspa_consensus_core::palw_chain_weight::PalwTipOrderV1;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::sortable_block::SortableBlock;

    let catalog = palw_v2_test_catalog();
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            // Five wide rows is five chain blocks, so five concurrent claims — one more than the
            // bundle's default bond can back. Funded for the chain it mines; see the fixture.
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(palw_v2_test_bundle_funded_for(&catalog, 16));
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    // A V2 network orders tips by PALW — not by the V1 fence, which it does not and may not set.
    assert_eq!(config.params.palw_tip_order_v1(), PalwTipOrderV1::PalwWeighted, "a ConsensusV2 network is PALW-ordered");
    assert!(config.params.palw_fork_choice.is_none(), "and it reaches that without any V1 fence");

    // Wide rows, so virtual really has competing parents to choose among — a single-tip chain
    // would satisfy the invariant vacuously.
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..5 {
        ctx.build_block_template_row(0..3).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor();
    let sink = ctx.consensus.get_sink();
    let virtual_parents = vp.virtual_stores.read().state.get().unwrap().parents.clone();
    assert!(virtual_parents.contains(&sink), "the sink is one of virtual's parents");
    // **The invariant is GHOSTDAG's OWN key, which is `SortableBlock` — `(blue_work, hash)`.**
    //
    // This asserted strictly-lower blue work, which is stronger than the rule and therefore wrong
    // in the merge-narrowing direction: `find_selected_parent` is `max()` over `SortableBlock`, so
    // an equal-work parent whose hash sorts BELOW the sink's cannot be selected over it and is a
    // perfectly legal virtual parent. Demanding strict `<` forced the filter that dropped exactly
    // those siblings on every block of the only lineage that ships V2 (launch blockers §7).
    let sink_sortable = SortableBlock { hash: sink, blue_work: vp.ghostdag_store.get_blue_work(sink).unwrap() };
    for parent in &virtual_parents {
        if *parent == sink {
            continue;
        }
        let parent_sortable = SortableBlock { hash: *parent, blue_work: vp.ghostdag_store.get_blue_work(*parent).unwrap() };
        assert!(
            parent_sortable < sink_sortable,
            "virtual parent {parent} out-ranks the sink under GHOSTDAG's own key — it would be selected, not the sink"
        );
    }
    // And the consequence stated directly, so the invariant is pinned by the rule it protects
    // rather than by a proxy for it.
    assert_eq!(
        vp.ghostdag_manager.find_selected_parent(virtual_parents.iter().copied()),
        sink,
        "GHOSTDAG selects the sink out of virtual's parent set"
    );

    // The blue-work network keeps its own behaviour: the filter is scoped, so equal-work siblings
    // are still merged there. Asserted as the rule rather than as a chain shape.
    let plain = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    assert_eq!(plain.params.palw_tip_order_v1(), PalwTipOrderV1::BlueWorkOnly);
}

/// **Two submitters racing the same claim must not kill the block that carries both** (testnet-12,
/// 2026-08-22).
///
/// One funded submitter per network suffices, so several MAY be funded — and when they are, two
/// nodes independently assemble the same quorum and both submit. Both objects are valid against
/// the parent state, so a filter that judges each one there lets both through; the transition then
/// applies them in order and refuses the second as `wrong phase for ReceiptLicensed`, taking the
/// honest block with it. Measured: 175 blocks produced, 23 accepted, 74 disqualified, DAA frozen
/// while three hosts submitted correctly.
///
/// The filter must ask the question the transition will ask, which means folding each accepted
/// object in before judging the next.
#[tokio::test]
async fn a_duplicate_lifecycle_object_is_dropped_and_the_block_stands() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_panel_v2::{
        PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
    };
    use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2, PalwConsensusObjectV2};

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 64);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..26 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor();
    let (_, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let (claim_id, _) =
        state.claims_iter().find(|(_, c)| matches!(c.phase, PalwClaimPhaseV2::PanelBound { .. })).expect("a bound claim");
    let claim_id = *claim_id;
    let panel = state.panel(&claim_id).expect("a bound claim has a panel");
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let signed_daa = ctx.consensus.get_virtual_daa_score();

    let receipts: Vec<PalwSeatReceiptV2> = panel
        .seats
        .iter()
        .take(3)
        .map(|seat| {
            let bond = state.bond(&seat.bond).expect("registered");
            let kp = (0..16u64)
                .map(TestConsensus::palw_v2_registry_keypair)
                .find(|kp| kp.verification_key.as_ref() == bond.pubkey.as_slice())
                .expect("a registry key");
            let message = palw_receipt_message_v2(network_domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
            let signature =
                libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT, [0u8; 32])
                    .expect("sign")
                    .as_ref()
                    .to_vec();
            PalwSeatReceiptV2 { claim: claim_id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: seat.bond, signed_daa, signature }
        })
        .collect();

    let object = PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts };
    let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: ctx.consensus.get_sink(),
        daa_score: signed_daa,
        blue_score: 27,
        subsidy: 0,
    };

    // What two racing submitters put in one block: the same valid object, twice.
    let accepted = vp.palw_v2_accepted_objects_for_tests(
        &state,
        &bundle.state,
        &point,
        vec![object.clone(), object.clone()],
        ctx.consensus.get_sink(),
    );
    assert_eq!(accepted.len(), 1, "the duplicate is dropped as an OBJECT — the block must not die for carrying it");

    // And the one that survived is applicable, which is the property the filter exists to give the
    // transition: what it returns, the fold takes.
    kaspa_consensus_core::palw_state_v2::apply_palw_transition_v2(&state, &bundle.state, &point, &accepted, None)
        .expect("what the filter accepts, the transition applies");

    // **And the fold must not eat the objects that are NOT duplicates.** The first version of this
    // filter re-applied at the block's own chain point, which the transition accepts exactly once
    // (it demands a strictly increasing blue score) — so every object after the first was dropped
    // with `blue_score must strictly increase`, whatever claim it named. The chain then produced
    // blocks, accepted submissions and licensed nothing: 356 blocks, 72 submissions, weight zero.
    let mut many: Vec<PalwConsensusObjectV2> = Vec::new();
    for (id, claim) in state.claims_iter() {
        if !matches!(claim.phase, PalwClaimPhaseV2::PanelBound { .. }) {
            continue;
        }
        let Some(p) = state.panel(id) else { continue };
        let rs: Vec<PalwSeatReceiptV2> = p
            .seats
            .iter()
            .take(3)
            .map(|seat| {
                let bond = state.bond(&seat.bond).expect("registered");
                let kp = (0..16u64)
                    .map(TestConsensus::palw_v2_registry_keypair)
                    .find(|kp| kp.verification_key.as_ref() == bond.pubkey.as_slice())
                    .expect("a registry key");
                let message = palw_receipt_message_v2(network_domain, *id, PalwReceiptVerdictV2::Valid, signed_daa);
                let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                    &kp.signing_key,
                    message.as_byte_slice(),
                    PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                    [0u8; 32],
                )
                .expect("sign")
                .as_ref()
                .to_vec();
                PalwSeatReceiptV2 { claim: *id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: seat.bond, signed_daa, signature }
            })
            .collect();
        many.push(PalwConsensusObjectV2::ReceiptLicensed { claim: *id, receipts: rs });
        if many.len() == 3 {
            break;
        }
    }
    assert!(many.len() >= 2, "the fixture must offer at least two distinct bound claims, got {}", many.len());
    let all = vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &point, many.clone(), ctx.consensus.get_sink());
    assert_eq!(all.len(), many.len(), "distinct claims must all survive the fold — only duplicates are dropped");
    kaspa_consensus_core::palw_state_v2::apply_palw_transition_v2(&state, &bundle.state, &point, &all, None)
        .expect("and the whole accepted set applies in one block");
}

/// **ADR-0064 — the bootstrap registry is the one the transition will have, not the one the
/// object list looks like.**
///
/// The rule lets a chain block's own attempt name a bond that this block's ACCEPTED objects
/// register — the registration riding a transaction in the selected parent's body or a merged
/// block's, never in this block's own body, which is why it buys a joining producer one chain
/// block and does not restart a stopped chain (ADR-0064's correction). The bond it resolves
/// against therefore has to be the registry the transition will hold after it folds those
/// objects — and the cheap way to write that lookup, scanning the object list for the first
/// matching `BondRegistered`, is a DIFFERENT question with a different answer.
///
/// It differs exactly when the block touches the bond again. A mergeset carrying a registration
/// and then a retirement for the same bond leaves it `Retiring`; the first-object reading says
/// `Active`, and admission item 1 refuses `Retiring`. One reading admits the block, the other
/// refuses it, and the whole of ADR-0064 is the removal of a disagreement of precisely that shape.
///
/// Both readings are exercised: the registration alone (where they agree, so a broken lookup still
/// passes) and the registration followed by a retirement (where they cannot).
#[tokio::test]
async fn palw_v2_the_bootstrap_registry_is_the_state_the_transition_will_hold() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_state_v2::{
        PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT, PALW_BOND_RETIREMENT_V2_MLDSA87_CONTEXT, PalwBondKeyV2, PalwBondStatusV2,
        PalwConsensusObjectV2, palw_bond_registration_message_v2, palw_bond_retirement_message_v2,
    };
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 64);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..4 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor();
    let (_, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );

    // A newcomer: an ML-DSA-87 identity past the genesis registry's rows, and a registration
    // outpoint the chain has never accepted. This is the party ADR-0064 exists for.
    let kp = TestConsensus::palw_v2_registry_keypair(40);
    let pubkey = kp.verification_key.as_ref().to_vec();
    let operator_pubkey = vec![0x64u8; 8];
    let bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::default(), 640));
    let collateral = bundle.bond.min_collateral_sompi();
    let payout_payload = kaspa_hashes::Hash64::from_u64_word(0x9A11);
    let sign = |message: kaspa_hashes::Hash64, context: &[u8]| {
        libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, message.as_byte_slice(), context, [0u8; 32]).expect("sign").as_ref().to_vec()
    };
    let registered = PalwConsensusObjectV2::BondRegistered {
        bond,
        pubkey: pubkey.clone(),
        operator_pubkey: operator_pubkey.clone(),
        collateral,
        payout_payload,
        capable_classes: Default::default(),
        signature: sign(
            palw_bond_registration_message_v2(
                network_domain,
                &bond,
                &pubkey,
                &operator_pubkey,
                collateral,
                &payout_payload,
                &Default::default(),
            ),
            PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT,
        ),
    };

    let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: ctx.consensus.get_sink(),
        daa_score: ctx.consensus.get_virtual_daa_score(),
        blue_score: 5,
        subsidy: 0,
    };
    assert!(state.bond(&bond).is_none(), "the parent chain has never seen this bond — that IS the deadlock");

    // The registration alone. Both readings agree here, which is why this case cannot be the whole
    // test: it is the one a first-object lookup also passes.
    let (accepted, folded) = vp.palw_v2_accepted_objects_and_state_for_tests(
        &state,
        &bundle.state,
        &point,
        vec![registered.clone()],
        ctx.consensus.get_sink(),
    );
    assert_eq!(accepted.len(), 1, "an honest registration is accepted: {accepted:?}");
    let fresh = folded.bond(&bond).expect("ADR-0064's whole point: the block's own mergeset registers it");
    assert_eq!(fresh.status, PalwBondStatusV2::Active);
    assert_eq!(fresh.pubkey, pubkey, "and it is the newcomer's key, so item 2 still has something to compare");
    assert_eq!(fresh.registered_daa, point.daa_score, "registered AT this block, which is what a maturity rule would measure from");

    // Registration then retirement, one mergeset. The transition leaves the bond `Retiring`; the
    // reading this test exists to forbid would have answered `Active`.
    let retired = PalwConsensusObjectV2::BondRetireRequested {
        bond,
        signature: sign(palw_bond_retirement_message_v2(network_domain, &bond), PALW_BOND_RETIREMENT_V2_MLDSA87_CONTEXT),
    };
    let (accepted, folded) = vp.palw_v2_accepted_objects_and_state_for_tests(
        &state,
        &bundle.state,
        &point,
        vec![registered, retired],
        ctx.consensus.get_sink(),
    );
    assert_eq!(accepted.len(), 2, "both objects are valid in sequence — neither is dropped: {accepted:?}");
    let after = folded.bond(&bond).expect("still registered, just on its way out");
    assert!(
        matches!(after.status, PalwBondStatusV2::Retiring { .. }),
        "the registry the transition will hold says Retiring; reading the first BondRegistered off the \
         list would have said Active, and admission treats those two answers oppositely: {:?}",
        after.status
    );

    // And that difference is load-bearing: item 1 refuses a retiring bond, so the two readings
    // disagree about whether this block may produce at all.
    let attempt = kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2 {
        version: 1,
        network_domain,
        challenge: kaspa_hashes::Hash64::from_u64_word(0),
        class_id: kaspa_hashes::Hash64::from_u64_word(0),
        executor_bond: bond.0,
        executor_pubkey: pubkey,
        operator_id: kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&operator_pubkey),
        artifact_root: kaspa_hashes::Hash64::from_u64_word(0),
        trace_root: kaspa_hashes::Hash64::from_u64_word(0),
        output_root: kaspa_hashes::Hash64::from_u64_word(0),
        pwu: 1,
        trace_manifest_root: kaspa_hashes::Hash64::from_u64_word(0),
        trace_chunk_count: 1,
        trace_retention_daa: 0,
        execution_root: kaspa_hashes::Hash64::from_u64_word(0),
    };
    let err = kaspa_consensus_core::palw_admission_v2::check_palw_producer_entitlement_v2_with_bootstrap(
        &state,
        &attempt,
        folded.bond(&bond),
    )
    .expect_err("a bond this block itself put into retirement may not also produce under it");
    assert!(
        matches!(err, kaspa_consensus_core::palw_admission_v2::PalwAdmissionV2Error::BondRetiring(k) if k == bond),
        "refused for the retirement, which is the fact the folded registry carries and the object list does not: {err:?}"
    );
}

/// **A merged blue is paid only if this chain can show its producer is bonded** (launch blockers
/// §8, first bullet).
///
/// The subsidy pays for PALW work, but the stateful half of admission runs only on the selected
/// chain — so every other merged blue in the DAA window was paid its full worker share without the
/// chain ever asking whether the key that signed it belongs to a live bond. A miner with no bond
/// at all collected on a solved hash.
///
/// Both directions, because a filter that refuses everything would also make the first assertion
/// pass: the honest chain's blues are all entitled and all paid, and the predicate that decides it
/// really does refuse an attempt whose bond this chain does not hold.
#[tokio::test]
async fn palw_v2_an_unbonded_merged_blue_is_not_paid() {
    use crate::model::stores::daa::DaaStoreReader;
    use crate::model::stores::ghostdag::GhostdagStoreReader;
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 16);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..4 {
        ctx.build_block_template_row(0..2).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor().clone();
    let sink = ctx.consensus.get_sink();
    let ghostdag_data = vp.ghostdag_store.get_data(sink).unwrap();
    assert!(ghostdag_data.mergeset_blues.len() > 1, "the fixture must merge a blue besides the selected parent");
    let non_daa = vp.daa_excluded_store.get_mergeset_non_daa(sink).unwrap();

    // The state AT the sink's selected parent — the one the coinbase is computed against.
    let (tip_block, tip_state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    assert_eq!(tip_block, sink, "the walk left the tip at the sink");
    let parent_state = {
        let store = vp.palw_state_v2_store.read();
        let (_, delta) = store.delta_of(sink).expect("the sink has a delta");
        kaspa_consensus_core::palw_state_v2::revert_delta_v2(&tip_state, &delta, &bundle.state).expect("the sink's delta reverts")
    };

    // The sink's own evaluation point — the one its transition ran at, and therefore the one the
    // payment gate must ask the admission at (audit3 S-04).
    let sink_header = vp.headers_store.get_header(sink).expect("the sink's header");
    let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: sink,
        daa_score: sink_header.daa_score,
        blue_score: sink_header.blue_score,
        subsidy: vp.coinbase_manager.calc_block_subsidy(sink_header.daa_score),
    };

    // Honest chain: nothing is withheld. A filter that dropped an honest miner's subsidy would be
    // a worse bug than the one it closes.
    let unentitled = vp.palw_v2_unentitled_blues(&parent_state, &ghostdag_data, &non_daa, &point);
    assert!(unentitled.is_empty(), "every blue of an honest V2 chain is bonded and paid, got {unentitled:?}");

    // And the predicate really separates: the same attempt, naming a bond this chain does not hold.
    let blue = *ghostdag_data
        .mergeset_blues
        .iter()
        .find(|h| **h != ghostdag_data.selected_parent && !non_daa.contains(h))
        .expect("a merged blue that is not the selected parent");
    let header = vp.headers_store.get_header(blue).expect("the blue's header");
    let envelope = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment).expect("a V2 block carries an attempt");
    kaspa_consensus_core::palw_admission_v2::check_palw_producer_entitlement_v2(&parent_state, &envelope.attempt)
        .expect("the real attempt is entitled");
    let mut forged = envelope.attempt.clone();
    forged.executor_bond = kaspa_consensus_core::tx::TransactionOutpoint {
        transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(0xDEAD),
        index: 7,
    };
    assert!(
        kaspa_consensus_core::palw_admission_v2::check_palw_producer_entitlement_v2(&parent_state, &forged).is_err(),
        "an attempt naming a bond this chain does not hold is not entitled to the subsidy"
    );
}

/// **An unclean shutdown between the PALW tip batch and the virtual-state commit is repaired, not
/// fatal** (launch blockers §7, fourth bullet).
///
/// The tip row carries the whole registry as a carriage, so it is written once at the end of the
/// UTXO walk rather than in each block's batch — which means there is a window where the tip has
/// landed and the sink has not. The walk used to take the tip's state verbatim as the state at its
/// own starting point; in that window the two are different blocks, `revert_delta_v2` rejected the
/// value it was asked to replace, and the `.expect` around it panicked on every start from then on.
///
/// Both legs are exercised: the pipeline recovers when the tip trails the sink, and the revert leg
/// — the shape the crash window actually produces — is walked directly.
#[tokio::test]
async fn palw_v2_a_tip_that_does_not_stand_at_the_sink_is_re_derived() {
    use crate::model::stores::ghostdag::GhostdagStoreReader;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor().clone();
    let (tip_block, tip_state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let parent = vp.ghostdag_store.get_selected_parent(tip_block).expect("the tip has a selected parent");
    let parent_state = {
        let store = vp.palw_state_v2_store.read();
        let (_, delta) = store.delta_of(tip_block).expect("the tip has a delta");
        kaspa_consensus_core::palw_state_v2::revert_delta_v2(&tip_state, &delta, &bundle.state)
            .expect("reverting the tip's own delta yields its parent's state")
    };

    // The revert leg, walked directly: this is the shape the crash window leaves — the tip standing
    // one block AHEAD of where the next walk starts.
    {
        let store = vp.palw_state_v2_store.read();
        let rederived =
            crate::processes::palw_state_walk::walk_chain_path(&store, &bundle.state, tip_state.clone(), &[tip_block], &[])
                .expect("the path from the tip back to the sink walks");
        assert_eq!(rederived.state_root(), parent_state.state_root(), "the re-derived state is the state at the sink");
    }

    // The apply leg, through the real pipeline. The template is built BEFORE the tip is moved,
    // which is what the crash window really looks like: the block on the wire is honest and commits
    // the right root — it is this node's own tip row that no longer stands where the walk starts.
    // Moving the tip first would instead corrupt the template's own state root and test nothing.
    ctx.build_block_template_row(0..1);
    vp.palw_state_v2_store.write().set_tip_for_tests(parent, &parent_state).unwrap();
    assert_eq!(vp.palw_state_v2_store.read().tip_record().unwrap().unwrap().block, parent, "the tip really moved");

    // Before the repair this panicked inside `calculate_utxo_state_relatively` on this insert.
    ctx.validate_and_insert_row().await.assert_valid_utxo_tip();

    let (recovered_block, recovered_state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    assert_eq!(recovered_block, ctx.consensus.get_sink(), "the tip is back at the sink");
    assert_ne!(recovered_block, parent, "and the chain advanced rather than stalling");
    let expected = {
        let store = vp.palw_state_v2_store.read();
        let (_, delta) = store.delta_of(recovered_block).expect("the new tip has a delta");
        let (_, prev_delta) = store.delta_of(tip_block).expect("the old tip has a delta");
        let at_old = kaspa_consensus_core::palw_state_v2::apply_delta_v2(&parent_state, &prev_delta, &bundle.state).unwrap();
        kaspa_consensus_core::palw_state_v2::apply_delta_v2(&at_old, &delta, &bundle.state).unwrap()
    };
    assert_eq!(recovered_state.state_root(), expected.state_root(), "and it is the state the deltas fold to");
}

/// **The V2 sink-search heap carries no PALW key, and the deep-reorg gate carries all of it**
/// (launch blockers §7, first bullet).
///
/// The wedge this pins: `palw_candidate_order_v2` derives a candidate's standing by walking stored
/// deltas, and `commit_utxo_state` writes a delta only for a block already committed to the
/// selected chain. Feeding that into `order_tips_v1` — which ranks `Some` above `None` — makes
/// "weighable" mean "already mine", so the incumbent outranks every challenger by construction. A
/// node whose own branch stalls then never reorgs again, and one privately-delivered block puts a
/// chosen victim in that state.
///
/// Two halves, and the test is only worth something with both: the heap key is gone, AND the
/// authority still exists at the site that can evaluate it.
#[tokio::test]
async fn palw_v2_tip_heap_has_no_weight_key_but_the_gate_does() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(palw_v2_test_bundle_funded_for(&catalog, 16));
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..2).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor();
    let sink = ctx.consensus.get_sink();
    // The V2 branch answers before it reads either of these, which is the property being pinned:
    // no finality window and no bond view can make the heap prefer the incumbent.
    let finality_point = config.params.genesis.hash;
    let bond_view = kaspa_consensus_core::dns_finality::ActiveBondView::new();

    // Half one: the incumbent gets no rank the heap can prefer it by. If this ever answers `Some`
    // again, every challenger — which is every block not yet on this node's chain — loses to the
    // sink whatever its weight.
    assert!(
        vp.palw_tip_weights_v1(sink, finality_point, sink, &bond_view).is_none(),
        "the V2 sink-search heap must be blue-work ordered — a PALW key here is a permanent wedge"
    );
    for tip in ctx.consensus.body_tips().iter().copied() {
        assert!(vp.palw_tip_weights_v1(tip, finality_point, sink, &bond_view).is_none(), "tip {tip} carries a heap weight key");
    }

    // Half two: the authority did not evaporate with it. The sink IS weighable — the gate runs
    // after UTXO validation, which is the first moment both sides of a reorg comparison are.
    assert!(vp.palw_candidate_order_v2(sink).is_some(), "the deep-reorg gate must still be able to weigh a validated chain");
}

/// **ADR-0042 Decision 6: attempt admission runs on the live path, and every clause bites.**
///
/// The pure function was tested; what this asserts is that the PIPELINE calls it — and it is
/// worth a test of its own because wiring it is what proved the harness had been mining blocks no
/// chain would admit. Before this the fixture carried a hand-written operator id, a chosen `pwu`,
/// a losing class ticket and a bond with a tenth of the collateral its own claim reserves. Each
/// of those is a real rule, and each refused every block until the harness was made to do what a
/// miner does.
#[tokio::test]
async fn palw_v2_attempt_admission_runs_on_the_live_path() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle(&catalog);
    // The genesis list is the only place a V2 network gets a class and a bond. Without it every
    // attempt names a bond the chain does not have and the network cannot make its first block —
    // which is what the harness measured the moment admission was wired.
    // One class and a REGISTRY of bonds: a panel seats one bond per operator and never the
    // claim's own executor, so `seat_count + 1` of them is the smallest registry that can license
    // anything — `verify_palw_genesis_v2` refuses a shorter one.
    assert_eq!(
        bundle.genesis_objects.len(),
        1 + kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1(),
        "the bundle registers its class and a seatable bond registry"
    );
    assert!(bundle.genesis_objects.iter().any(|o| matches!(o, PalwConsensusObjectV2::ClassRegistered { .. })));
    assert!(bundle.genesis_objects.iter().any(|o| matches!(o, PalwConsensusObjectV2::BondRegistered { .. })));

    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let genesis_hash = config.params.genesis.hash;
    let consensus = TestConsensus::new(&config);

    // Genesis APPLIED the registrations — the class and the bond exist before block 1.
    {
        let vp = consensus.virtual_processor();
        let store = vp.palw_state_v2_store.read();
        let (_, state) = store.load_tip(&bundle.state).unwrap().expect("the genesis tip loads");
        assert!(state.class(&bundle.base_class_id).is_some(), "the liveness floor is registered at genesis");
        assert_eq!(state.class_share_permille(&bundle.base_class_id), Some(1000), "and holds the whole table");
        assert!(store.has_delta(genesis_hash).unwrap(), "genesis carries a delta like any chain block");
    }

    let mut ctx = TestContext::new(consensus);
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    // Blocks whose attempts pass every admission clause DO advance the chain — the wiring is a
    // gate, not a wall.
    assert_ne!(ctx.consensus.get_sink(), genesis_hash, "admissible attempts build a chain");

    // …and one that does not is refused. The pwu is the sharpest clause: ADR-0045 Decision 1
    // makes item 6 an EQUALITY against a value derived from chain state, so a miner that picks a
    // number is a miner refused.
    let sink_before = ctx.consensus.get_sink();
    let template = ctx.build_block_template(11, ctx.simulated_time + 1);
    let mut liar = template.block.clone();
    let mut envelope =
        kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&liar.header.palw_commitment).unwrap();
    envelope.attempt.pwu += 1;
    liar.header.palw_commitment = envelope.encode_wire();
    liar.header.finalize();
    let liar_hash = liar.header.hash;
    // `pwu` is inside `attempt_id_v2`, so moving it also breaks the signature — and since launch
    // blockers §5 the relay path verifies that, so this lie is now refused at the DOOR rather than
    // surviving to the chain walk. Both refusals are the point: the door one is what stops a
    // forgery from costing every peer storage, and the pwu equality below is what stops it from
    // ever being chain.
    let outcome = ctx.consensus.validate_and_insert_block(liar.to_immutable()).virtual_state_task.await;
    assert!(outcome.is_err(), "an attempt whose carriage was edited after signing does not enter the DAG");
    assert_eq!(ctx.consensus.get_sink(), sink_before, "an attempt claiming a pwu it did not derive cannot become the sink");
    assert_ne!(liar_hash, sink_before);

    // And a lie that keeps its signature honest — re-signed by the real bond holder — still loses
    // on the pwu clause itself, which is the equality this test is named for.
    let mut resigned = template.block.clone();
    let mut env2 =
        kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&resigned.header.palw_commitment).unwrap();
    env2.attempt.pwu += 1;
    env2.signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &crate::consensus::test_consensus::TestConsensus::palw_v2_harness_keypair().signing_key,
        kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&env2.attempt).as_byte_slice(),
        kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
        [0x5Au8; 32],
    )
    .expect("sign")
    .as_ref()
    .to_vec();
    resigned.header.palw_commitment = env2.encode_wire();
    resigned.header.finalize();
    let resigned_hash = resigned.header.hash;
    ctx.validate_and_insert_block(resigned.to_immutable()).await;
    assert_eq!(ctx.consensus.get_sink(), sink_before, "a correctly-signed pwu lie is still refused by the equality");
    assert_ne!(resigned_hash, sink_before);
}

/// **Unit C step 4's missing half: the block's own work reaches the state machine.**
///
/// `palw_v2_check_attempt_admission` validated the carried envelope and then the caller passed
/// `None` to the transition. Everything downstream of a claim therefore had nothing to act on:
/// no `PanelBound` could be derived (there was no `Provisional` claim to bind), nothing could be
/// licensed or challenged, no escrow was ever funded, the safe frontier never moved off the zero
/// point, and `bounded_immature` — the quantity the per-bond exposure ceiling is measured in —
/// stayed at zero on a chain of admissible attempts. PALW weight, which is this network's entire
/// fork choice, was structurally unreachable.
///
/// What this asserts is the seam, not the arithmetic: one attempt-lane chain block, one claim,
/// attributed to that block, in `Provisional`, with real reserved exposure behind it.
#[tokio::test]
async fn palw_v2_an_attempt_block_creates_its_claim() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2, PalwClaimSourceV2};

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle(&catalog);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let genesis_hash = config.params.genesis.hash;
    let consensus = TestConsensus::new(&config);

    // The zero point: genesis registers a class and a bond and creates no claim.
    {
        let store = consensus.virtual_processor().palw_state_v2_store.read();
        let (_, state) = store.load_tip(&bundle.state).unwrap().expect("the genesis tip loads");
        assert_eq!(state.claims_iter().count(), 0, "genesis registers; it does not work");
        assert_eq!(state.bounded_immature(), 0);
    }

    let mut ctx = TestContext::new(consensus);
    const BLOCKS: usize = 3;
    for _ in 0..BLOCKS {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor();
    let store = vp.palw_state_v2_store.read();
    let tip = store.tip_record().unwrap().expect("the walk wrote a tip");
    assert_ne!(tip.block, genesis_hash, "the chain really advanced");
    let (_, state) = store.load_tip(&bundle.state).unwrap().expect("the tip loads");

    // One chain block, one claim — the claim IS the block's work.
    let claims: Vec<_> = state.claims_iter().collect();
    assert_eq!(claims.len(), BLOCKS, "every attempt-lane chain block created exactly one claim");
    let chain: Vec<_> = vp
        .reachability_service
        .default_backward_chain_iterator(tip.block)
        .take_while(|h| *h != kaspa_consensus_core::blockhash::ORIGIN)
        .collect();
    for (id, claim) in &claims {
        assert_eq!(claim.source, PalwClaimSourceV2::Attempt, "an algo-6 block's work is an attempt");
        assert!(matches!(claim.phase, PalwClaimPhaseV2::Provisional), "a fresh claim starts unbound");
        assert!(chain.contains(&claim.accepted_block), "claim {id} names a block on this chain");
        assert!(claim.reserved > 0, "and it reserves real collateral against its bond");
    }

    // The exposure the ceiling is measured in is no longer decorative.
    assert!(state.bounded_immature() > 0, "immature weight accumulated — it could not before");
    let bond = claims[0].1.bond;
    assert_eq!(
        state.reserved_exposure(&bond),
        claims.iter().map(|(_, c)| c.reserved).sum::<u128>(),
        "the bond carries exactly the sum of what its claims reserved"
    );
}

/// **ADR-0042 Decision 10: the PALW reward is a CARVE, and now it is actually carved.**
///
/// The Decision is explicit — "PALW reward is a **carve of the fixed subsidy** ..., never an
/// addition to it — the schedule is never exceeded (I6/I15)" — and only the release half was
/// built. Escrows were appended to `validator_reward_outputs` and nothing anywhere was taken out
/// to fund them: the accepting block's miner was paid its whole worker share, the same sompi were
/// escrowed against the claim, and every finalized claim minted its carve a second time, above the
/// emission schedule. `coinbase.rs` contained no PALW arithmetic at all, which is why reading it
/// could not settle the question either way.
///
/// This pins the funding side, which is the half that is observable on a three-block chain: the
/// coinbase that pays an attempt-lane block pays its worker share MINUS the escrow that block's
/// own claim took. The release side is the queue the same state carries, and the two are the same
/// number by construction — `palw_v2_escrow_withheld_at` sums the very field
/// `palw_v2_payout_outputs` later renders.
#[tokio::test]
async fn palw_v2_the_escrow_is_carved_out_of_the_block_that_earned_it() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::dns_finality::split_block_subsidy;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle(&catalog);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let fee_split = config.params.dns_params.as_ref().expect("the fixture network carves").reward_params.fee_split.clone();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..4 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor();
    let tip = ctx.consensus.get_sink();
    let selected_parent = vp.headers_store.get_header(tip).unwrap().direct_parents()[0];
    let sp_daa = vp.headers_store.get_header(selected_parent).unwrap().daa_score;

    // What the emission schedule allots the selected parent, and what its own claim escrowed.
    let sp_subsidy = vp.coinbase_manager.calc_block_subsidy(sp_daa);
    let sp_worker_share = split_block_subsidy(sp_subsidy, &fee_split).worker_base_sompi;
    let (_, sp_state) =
        vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().filter(|(b, _)| *b == tip).expect("the walk's tip is the sink");
    let escrow = vp.palw_v2_escrow_withheld_at(&sp_state, selected_parent);
    assert!(escrow > 0, "an attempt-lane block escrows its worker carve — otherwise this proves nothing");
    // Measured: 370,468,345 subsidy → 229,690,375 worker share → 229,690,373 escrow. The carve
    // permille (620) and `subsidy_worker_base_bps` (6200) are the same number expressed twice, so
    // an attempt-lane block's ENTIRE worker reward is escrowed and the miner keeps only the
    // rounding difference — which is exactly Decision 10's "block accepted → reward escrow →
    // Final → spendable". `validate_palw_v2` refuses a bundle where the carve is the larger of the
    // two, because then the escrow could not be funded from the block it is carved from.
    assert!(escrow <= sp_worker_share, "the carve must fit inside the share it is carved from");

    // The coinbase that pays the selected parent pays exactly the difference. Not `<=`: an
    // inequality would also hold if the escrow were withheld twice, or from the wrong block.
    let coinbase = &ctx.consensus.get_block(tip).unwrap().transactions[0];
    let paid: u64 = coinbase.outputs.iter().map(|o| o.value).sum();
    assert_eq!(
        paid,
        sp_worker_share - escrow,
        "the tip's coinbase must pay its selected parent's worker share LESS the escrow that block's claim took"
    );

    // And the whole coinbase stays inside what the schedule allows the block to mint. This is the
    // I6/I15 statement, in one line, on a real chain.
    assert!(paid <= sp_subsidy, "a block may never mint more than the subsidy of what it merges");
}

/// **Audit C-08's lock, resolved from a real chain's registry.**
///
/// A `PalwBondKeyV2` is the outpoint holding the bond's collateral, and nothing kept the money
/// there: the owner could spend that output in block 1 and every exposure ceiling, every slash and
/// Decision 7's Sybil bound would have been denominated in a balance the bond no longer had.
///
/// This pins the resolver — the seam between the V2 registry and the UTXO spend filter — against a
/// state a real chain produced, at the two DAA scores that matter. What it does NOT do is spend
/// the output: the fixture's genesis bond names an outpoint no test key can open (a genesis
/// premine output needs its vault's ML-DSA key, and no harness holds one), so the last link is
/// covered where it can be — `BondSpendFilter`'s own unit test, over the set this function returns.
#[tokio::test]
async fn palw_v2_a_live_bonds_collateral_outpoint_is_locked() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle(&catalog);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor();
    let (_, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let registered: Vec<_> = state.bonds_iter().map(|(key, _)| key.0).collect();
    assert_eq!(
        registered.len(),
        kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1(),
        "the fixture registers a seatable bond registry at genesis"
    );

    // EVERY Active bond's collateral is locked at every score — the panel's seats included, since
    // a seat that could spend its stake out from under a claim is a seat with nothing at risk.
    for daa in [0, 1, 1_000_000, u64::MAX] {
        let locked = vp.palw_v2_locked_bond_outpoints(&state, daa);
        for outpoint in &registered {
            assert!(locked.contains(outpoint), "an Active bond's collateral must be locked at daa {daa}");
        }
        assert_eq!(locked.len(), registered.len(), "and nothing else is");
    }

    // A network with no V2 bundle locks nothing here — the filter stays inert where it always was.
    let plain = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let plain_consensus = TestConsensus::new(&plain);
    let _lt = plain_consensus.init();
    assert!(
        plain_consensus.virtual_processor().palw_v2_locked_bond_outpoints(&state, 0).is_empty(),
        "no bundle, no bond policy, nothing locked — every shipped preset is unchanged"
    );
}

/// **The measurement that proves the reservations are real: the ceiling can now be hit.**
///
/// The bundle's default bond funds four concurrent claims (`MIN_COLLATERAL_SOMPI`, whose own
/// doc comment does the arithmetic). While the transition never saw a block's work, `reserved`
/// stayed at zero forever and a single bond could have backed an unbounded chain — the P0-10
/// check was wired, ran on every block, and could not fail. The fifth chain block is refused now,
/// which is the ceiling doing what it exists to do.
///
/// Deliberately uses the DEFAULT fixture, so `palw_v2_test_bundle_funded_for` raising a bond
/// elsewhere cannot quietly retire this.
/// **A block produced the way the RC will produce blocks, accepted by consensus.**
///
/// Everything else about `ConsensusV2` was tested against `palw_v2_test_carriage`, a harness that
/// reads its class facts out of the genesis bundle and invents its roots. This runs the REAL path
/// end to end on the REAL ruleset: `palw_rc_params_from_artifacts` builds the network,
/// `palw_producer_facts_v2` supplies the six chain facts, `base0_execute_for_attempt_v1` runs an
/// actual BASE-0 inference over the job the template anchors, and the attempt is signed with an
/// ML-DSA-87 key the genesis bond registered. If it lands, testnet-12 can make blocks.
///
/// Before this, nothing could: `misaminer` and `pq-miner` both branch on algo 4 and 5 only, and the
/// one algo-6 carriage builder in the tree was a `pub(crate)` test helper. The network had a
/// genesis and no second block, and no test said so.
#[tokio::test]
async fn palw_rc_a_real_execution_produces_a_block_the_chain_accepts() {
    use kaspa_consensus_core::api::ConsensusApi;
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
        challenge_v2, class_ticket_v3, palw_network_domain_v2_for,
    };
    use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    use misaka_palw_base0::produce::{base0_execute_for_attempt_v1, base0_rc_job_anchor_v1, base0_rc_job_v1};

    // The operator half of the genesis card, generated here the way an operator generates it.
    let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0xB0u8; 32]);
    let bond_key = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(0));
    let artifact_root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's artifact derives");
    // The registry, not a bond: row 0 is the producer's, and the rest are the panel's. A registry
    // that cannot seat a panel — or that cannot carry a claim through the bind window — is refused
    // by `verify_palw_genesis_v2`, which is what stops a network from shipping in a state where it
    // makes two blocks and stops.
    let registry: Vec<_> = (0..kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1() as u32)
        .map(|i| kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(i)),
            pubkey: if i == 0 { keypair.verification_key.as_ref().to_vec() } else { vec![7u8.wrapping_add(i as u8); 32] },
            operator_pubkey: vec![21u8, i as u8, 0, 0, 0, 0, 0, 0],
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11 + i as u64),
        })
        .collect();
    let params = kaspa_consensus_core::config::params::palw_rc_params_from_artifacts(artifact_root, registry)
        .expect("the RC genesis card assembles");
    let bundle = match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("palw_rc_params_from_artifacts must yield a ConsensusV2 network"),
    };
    // The shipped RC activates the EVM lane at DAA 0 (inherited from `TESTNET_PARAMS` and kept
    // deliberately — testnet-11 carries it and the RC is the network t11's traffic moves onto). A
    // template cannot be built for an active lane by a binary without the feature, so a non-evm
    // test build disables it HERE and asserts what it disabled. The lane is orthogonal to what
    // this test measures — a real execution producing an accepted block — and stating the
    // divergence is better than a test that silently runs on a different ruleset.
    assert_eq!(params.evm_activation_daa_score, 0, "the shipped RC carries the EVM lane");
    let config = ConfigBuilder::new(params)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            if !cfg!(feature = "evm") {
                p.evm_activation_daa_score = u64::MAX;
            }
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let facts = ctx
        .consensus
        .palw_producer_facts_v2(bundle.base_class_id, Some(bond_key.0))
        .expect("the RC network answers for its own floor");
    assert_eq!(
        facts.ready_to_produce(keypair.verification_key.as_ref()),
        Ok(()),
        "the genesis bond, this key, and an untouched epoch budget"
    );
    assert_eq!(facts.artifact_root, artifact_root, "the class registered the artifact the producer will name");

    // The template. Its pre-pow hash anchors the job, so one template is one inference.
    // The builder's own timestamp, not one stamped over it: the RC's EVM lane is active at DAA 0
    // and `evm_commitment_root` is computed during the build against the header's timestamp.
    let mut block = ctx.build_block_template_keeping_time(0).block;
    let timestamp = block.header.timestamp;
    assert_eq!(
        block.header.pow_algo_id,
        kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
        "a ConsensusV2 network declares the attempt lane"
    );
    let network_domain = palw_network_domain_v2_for(config.params.net.to_string().as_bytes(), Some(config.params.genesis.hash));
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
    let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor's graph is expressible");
    let artifact = misaka_palw_base0::rc::palw_rc_base0_artifact_v1().expect("derives");

    // The work, and the draw (ADR-0072): a real inference over the job this template and bucket
    // name, whose execution IS the ticket. A lost draw is not re-rolled by moving the nonce —
    // every nonce in the bucket derives the same anchor and the same ticket — but by the next
    // bucket, which is a different job and a second real inference.
    let mut won = None;
    for bucket in 0u64..4096 {
        let nonce = bucket << kaspa_consensus_core::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2;
        let anchor = base0_rc_job_anchor_v1(network_domain, pre_pow, facts.class_id, &bond_key.0, bucket);
        let (job, prompt) =
            base0_rc_job_v1(&profile, anchor, artifact.shape.vocab, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &job, &prompt).expect("the floor runs its own job");
        let attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, pre_pow, timestamp, nonce, facts.class_id, &bond_key.0),
            class_id: facts.class_id,
            executor_bond: bond_key.0,
            executor_pubkey: keypair.verification_key.as_ref().to_vec(),
            operator_id: facts.bond.as_ref().unwrap().operator_id,
            artifact_root: facts.artifact_root,
            trace_root: run.trace_root,
            output_root: run.output_root,
            execution_root: run.execution_root,
            pwu: facts.pwu,
            trace_manifest_root: run.trace_manifest_root,
            trace_chunk_count: run.trace_chunk_count,
            trace_retention_daa: block.header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        if class_ticket_v3(&attempt, anchor) <= facts.class_target {
            won = Some((nonce, attempt));
            break;
        }
    }
    let (nonce, attempt) = won.expect("the floor's genesis target is winnable");

    // Signed ONCE, after the search, over the attempt id — the signature is outside the commitment
    // root, so signing per nonce would be an ML-DSA-87 operation thrown away every try.
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &keypair.signing_key,
        attempt_id_v2(&attempt).as_byte_slice(),
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
        [0x5Au8; 32],
    )
    .expect("ML-DSA-87 sign")
    .as_ref()
    .to_vec();
    block.header.nonce = nonce;
    block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
    block.header.finalize();
    let hash = block.header.hash;

    ctx.validate_and_insert_block(block.to_immutable()).await.assert_valid_utxo_tip();
    assert_eq!(ctx.consensus.get_sink(), hash, "the block a real execution produced is the chain");

    // And the chain moved because of it: the floor's epoch counter is the producer's own receipt.
    let after = ctx.consensus.palw_producer_facts_v2(bundle.base_class_id, Some(bond_key.0)).expect("still a V2 network");
    assert_eq!(after.epoch_produced_blocks, 1, "one block of this class, counted");
    assert!(after.bond.as_ref().unwrap().reserved_exposure > 0, "the claim it created reserves against the bond");
}

/// **A block produced by the REAL Qwen2.5-1.5B A16 model, accepted by consensus.**
///
/// The dense tier's own goal sentence. This is not a shaped fixture: it maps the converted
/// `.palwart` of `Qwen/Qwen2.5-1.5B-Instruct` (1.7 GiB, 28 layers, vocabulary 151,936 — the
/// checkpoint that answers "The capital of France is Paris." on this engine), registers it beside
/// the floor and the hybrid in a three-class genesis, runs the anchored canonical job through
/// `A16Engine`, commits under the TILED logits trace, wins the class's lottery, signs under a
/// genesis bond, and lands in the UTXO tip.
///
/// Ignored in CI only because of the 1.7 GiB artifact; everything it exercises is the shipped path.
///
/// ```text
/// MISAKA_QWEN25_A16_ARTIFACT=/path/to/qwen25-1.5b-a16.palwart \
///   cargo test --release -p kaspa-consensus real_qwen25_a16 -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "maps the real 1.7 GiB Qwen2.5 A16 artifact; set MISAKA_QWEN25_A16_ARTIFACT and --ignored"]
async fn palw_rc_the_real_qwen25_a16_model_produces_a_block() {
    use kaspa_consensus_core::api::ConsensusApi;
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
        challenge_v2, class_ticket_v3, palw_network_domain_v2_for,
    };
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_A16_CANONICAL, qwen25_a16_class_id_v2};
    use misaka_palw_base0::produce::base0_rc_job_anchor_v1;
    use misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend;

    let path = std::env::var("MISAKA_QWEN25_A16_ARTIFACT").expect("MISAKA_QWEN25_A16_ARTIFACT=/path/to/qwen25-1.5b-a16.palwart");
    let opened = std::time::Instant::now();
    let bytes = std::fs::read(&path).expect("the artifact reads");
    let artifact = misaka_palw_base0::artifact::decode_artifact_file_v1(&bytes).expect("the artifact decodes");
    let artifact_root = artifact.artifact_digest();
    eprintln!(
        "dense drill: {} layers / vocab {} / root {artifact_root} in {:?}",
        artifact.shape.n_layers,
        artifact.shape.vocab,
        opened.elapsed()
    );

    let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0xB0u8; 32]);
    let bond_key = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(0));
    let base_root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's artifact derives");
    let registry: Vec<_> = (0..kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1() as u32)
        .map(|i| kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(i)),
            pubkey: if i == 0 { keypair.verification_key.as_ref().to_vec() } else { vec![7u8.wrapping_add(i as u8); 32] },
            operator_pubkey: vec![21u8, i as u8, 0, 0, 0, 0, 0, 0],
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11 + i as u64),
        })
        .collect();

    // THREE classes: the floor, the hybrid (its dev fixture's root — this drill is about the dense
    // one), and the dense model whose weights are on disk.
    let hybrid_root = misaka_palw_base0::qwen36::qwen36_dev_fixture(1, 8).artifact_root();
    let params =
        kaspa_consensus_core::config::params::palw_rc_params_with_classes(base_root, hybrid_root, Some(artifact_root), registry)
            .expect("the three-class RC genesis card assembles");
    let bundle = match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("the RC ships a ConsensusV2 bundle"),
    };
    // The REGISTERED dense class is the corrected `graph-v2` one (ADR-0069): the v1 declaration
    // announces a one-byte state map against an i32 cache, so its backend is not court-capable and
    // a class on it cannot hold weight.
    let dense_class_id = qwen25_a16_class_id_v2();
    assert_ne!(dense_class_id, bundle.base_class_id, "the dense class is not the floor");

    let config = ConfigBuilder::new(params)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            if !cfg!(feature = "evm") {
                p.evm_activation_daa_score = u64::MAX;
            }
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let facts = ctx
        .consensus
        .palw_producer_facts_v2(dense_class_id, Some(bond_key.0))
        .expect("the three-class network answers for its dense entrant");
    assert_eq!(facts.artifact_root, artifact_root, "the chain names the artifact this node holds");
    assert_eq!(facts.ready_to_produce(keypair.verification_key.as_ref()), Ok(()), "bond, key and an epoch budget");

    let mut block = ctx.build_block_template_keeping_time(0).block;
    let timestamp = block.header.timestamp;
    let network_domain = palw_network_domain_v2_for(config.params.net.to_string().as_bytes(), Some(config.params.genesis.hash));
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
    let backend = Qwen25A16Backend::new(
        std::sync::Arc::new(artifact),
        config.params.net.to_string().into_bytes(),
        // The class's graph, not only its id — the backend needs it for the step space and the
        // state map, and `shape_profile_id()` of this profile IS `dense_class_id`.
        kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v1(
            kaspa_consensus_core::palw_qwen25_profile::qwen25_canonical_geometry_v1("Qwen/Qwen2.5-1.5B-Instruct")
                .expect("the canonical A16 geometry"),
        )
        .expect("a valid A16 profile"),
        QWEN25_A16_CANONICAL,
    );
    // The work, and the draw (ADR-0072): the execution IS the ticket, so a lost draw is re-rolled
    // by the next bucket — a different job, a second real inference — never by moving the nonce.
    let mut won = None;
    for bucket in 0u64..4096 {
        let nonce = bucket << kaspa_consensus_core::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2;
        let anchor = base0_rc_job_anchor_v1(network_domain, pre_pow, dense_class_id, &bond_key.0, bucket);
        let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job inside the artifact's table");
        let ran = std::time::Instant::now();
        let run = backend.execute(&job, &prompt).expect("a real Qwen2.5-1.5B forward pass over the anchored job");
        eprintln!(
            "dense drill: executed ({} prefill + {} decode) in {:?}; material {} bytes [bucket {}]",
            job.declared_prefill_tokens,
            job.exact_decode_tokens,
            ran.elapsed(),
            run.material.len(),
            bucket
        );
        let attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, pre_pow, timestamp, nonce, dense_class_id, &bond_key.0),
            class_id: dense_class_id,
            executor_bond: bond_key.0,
            executor_pubkey: keypair.verification_key.as_ref().to_vec(),
            operator_id: facts.bond.as_ref().expect("the pre-flight held a bond").operator_id,
            artifact_root: facts.artifact_root,
            trace_root: run.trace_root,
            output_root: run.output_root,
            execution_root: run.execution_root,
            pwu: facts.pwu,
            trace_manifest_root: run.trace_manifest_root,
            trace_chunk_count: run.trace_chunk_count,
            trace_retention_daa: block.header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        if class_ticket_v3(&attempt, anchor) <= facts.class_target {
            won = Some((nonce, attempt));
            break;
        }
    }
    let (nonce, attempt) = won.expect("the entrant's genesis target is winnable");
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &keypair.signing_key,
        attempt_id_v2(&attempt).as_byte_slice(),
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
        [0x5Au8; 32],
    )
    .expect("ML-DSA-87 sign")
    .as_ref()
    .to_vec();
    block.header.nonce = nonce;
    block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
    block.header.finalize();
    let hash = block.header.hash;

    ctx.validate_and_insert_block(block.to_immutable()).await.assert_valid_utxo_tip();
    assert_eq!(ctx.consensus.get_sink(), hash, "the block a real Qwen2.5-1.5B execution produced is the chain");
    eprintln!("dense drill: block {hash} accepted");
}

/// **A block produced by a REAL Qwen3.6-shaped execution, accepted by consensus** — the goal
/// gate's own sentence, "実用的にブロック生成に使用できる", as a test.
///
/// The floor's twin (`palw_rc_a_real_execution_produces_a_block_the_chain_accepts`) proved the RC
/// path for the class every node derives. This proves the SECOND class — registered from genesis
/// beside the floor by `palw_rc_params_with_qwen36`, answered for by `palw_producer_facts_v2`,
/// executed by the real `Qwen36Engine` (a hybrid GDN + gated-attention + MoE forward pass, not a
/// stub), committed under the TILED logits trace, won at the class's own lottery, signed under a
/// genesis bond, and accepted into the UTXO tip.
///
/// # What stands in, and what does not
///
/// The artifact is `qwen36_dev_fixture` — the Qwen3.6-shaped toy, because a CI test cannot mmap
/// 33 GiB. The stand-in is the WEIGHTS, not the path: the engine, the ops, the commitment scheme
/// and every consensus check are the production ones. The registered class's geometry is the real
/// `QWEN36_35B_A3B` (that is what `qwen36_registration_v1` derives), so the chain-side facts —
/// class id, pwu, target — are the real class's; the drill binary swaps the fixture for the real
/// artifact and nothing else. The profile's court constants are marked PRE-DERIVATION: the
/// admission cost work (per-node tiles, head-slice GDN openings, small registered context) tunes
/// them before any real network mints this genesis, and none of it changes this path.
#[tokio::test]
async fn palw_rc_a_qwen36_execution_produces_a_block_the_chain_accepts() {
    qwen36_block_e2e(misaka_palw_base0::qwen36::qwen36_dev_fixture(4, 8), "Qwen3.6-dev-fixture").await;
}

/// **The same path over the REAL 33 GiB artifact** — the drill the goal names.
///
/// `MISAKA_QWEN36_ARTIFACT=/path/to/q36-40L.palwq36 cargo test --release -p kaspa-consensus ///  real_qwen36_artifact -- --ignored --nocapture`
///
/// Ignored in CI because it memory-maps 33 GiB and runs ten true 35B forward passes (about a
/// minute of inference on the reference M4 Pro, plus one pass over the file for the artifact
/// root). Everything else — the two-class genesis, the facts, the lottery, the signature, the
/// acceptance — is byte-for-byte the fixture test's path, which is the point: the drill swaps the
/// weights and nothing else.
#[tokio::test]
#[ignore = "runs the real 33 GiB Qwen3.6 artifact; set MISAKA_QWEN36_ARTIFACT and --ignored"]
async fn palw_rc_the_real_qwen36_artifact_produces_a_block() {
    let path = std::env::var("MISAKA_QWEN36_ARTIFACT").expect("MISAKA_QWEN36_ARTIFACT=/path/to/q36-40L.palwq36");
    let opened = std::time::Instant::now();
    let artifact = misaka_palw_base0::qwen36::open_artifact(std::path::Path::new(&path)).expect("the artifact opens");
    // **The drill's artifact must be the PUBLIC NETWORK's artifact.** Otherwise this passes for a
    // file nobody on testnet-11 can use, which is the one thing the drill exists to rule out.
    assert_eq!(
        artifact.artifact_root(),
        kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT,
        "MISAKA_QWEN36_ARTIFACT is not the artifact testnet-11 registers"
    );
    eprintln!(
        "drill: mapped {} layers / {:.2} GiB in {:?}",
        artifact.shape.n_layers(),
        artifact.weight_bytes() as f64 / (1u64 << 30) as f64,
        opened.elapsed()
    );
    qwen36_block_e2e(artifact, "Qwen3.6-35B-A3B").await;
}

async fn qwen36_block_e2e(artifact: misaka_palw_base0::qwen36::Qwen36ArtifactV1, model_id: &str) {
    use kaspa_consensus_core::api::ConsensusApi;
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
        challenge_v2, class_ticket_v3, palw_network_domain_v2_for,
    };
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_qwen36_profile::QWEN36_RC_CANONICAL;
    use misaka_palw_base0::produce::base0_rc_job_anchor_v1;
    use misaka_palw_base0::qwen36_backend::Qwen36Backend;

    // The operator half, exactly as the floor test builds it: row 0's key is ours.
    let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0xB0u8; 32]);
    let bond_key = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(0));
    let base_root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's artifact derives");
    let registry: Vec<_> = (0..kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1() as u32)
        .map(|i| kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(i)),
            pubkey: if i == 0 { keypair.verification_key.as_ref().to_vec() } else { vec![7u8.wrapping_add(i as u8); 32] },
            operator_pubkey: vec![21u8, i as u8, 0, 0, 0, 0, 0, 0],
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11 + i as u64),
        })
        .collect();

    // The Qwen3.6 side: whichever weights the caller supplied, the real class's identity.
    let rooted = std::time::Instant::now();
    let artifact_root = artifact.artifact_root();
    eprintln!("drill: artifact root {artifact_root} in {:?}", rooted.elapsed());
    // The two-class genesis, assembled by the same function the shipped network uses. It once
    // failed here at the boot cost gate (the class's derived close was 1.24x the carrier ceiling)
    // and this call was wrapped in a match that asserted the refusal and returned — which was
    // honest while it lasted and became a FAIL-OPEN the moment the cost fit: a future regression
    // past the ceiling would have made this test pass while producing no block. Unconditional now.
    let params = kaspa_consensus_core::config::params::palw_rc_params_with_qwen36(base_root, artifact_root, registry)
        .expect("the two-class RC genesis card assembles");
    let bundle = match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("a ConsensusV2 network"),
    };
    // The REGISTERED hybrid class is `graph-v3` (ADR-0069) — the v2 projection over the
    // eps-corrected geometry. v1 names a GDN node no backend can serve, so a class on it has no
    // court; asked through the accessor rather than projected here, so this test cannot drift away
    // from what the genesis card actually registers.
    let qwen36_class_id = kaspa_consensus_core::palw_qwen36_profile::qwen36_class_id_v3();
    assert_ne!(qwen36_class_id, bundle.base_class_id, "two classes, and the floor is not the entrant");

    let config = ConfigBuilder::new(params)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            if !cfg!(feature = "evm") {
                p.evm_activation_daa_score = u64::MAX;
            }
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // The chain answers for the entrant: registered at genesis, active, budgeted.
    let facts = ctx
        .consensus
        .palw_producer_facts_v2(qwen36_class_id, Some(bond_key.0))
        .expect("the two-class network answers for its entrant");
    assert_eq!(facts.artifact_root, artifact_root, "the chain names the artifact this node holds");
    assert_eq!(facts.ready_to_produce(keypair.verification_key.as_ref()), Ok(()), "bond, key, and an epoch budget for 1‰");

    // The template anchors the job; the REAL engine runs it.
    let mut block = ctx.build_block_template_keeping_time(0).block;
    let timestamp = block.header.timestamp;
    let network_domain = palw_network_domain_v2_for(config.params.net.to_string().as_bytes(), Some(config.params.genesis.hash));
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);

    let backend = Qwen36Backend::new(
        std::sync::Arc::new(artifact),
        model_id,
        QWEN36_RC_CANONICAL,
        qwen36_class_id,
        config.params.net.to_string().into_bytes(),
    );
    // The work, and the draw (ADR-0072): the execution IS the ticket, so a lost draw is re-rolled
    // by the next bucket — a different job, a second real inference — never by moving the nonce.
    let mut won = None;
    for bucket in 0u64..4096 {
        let nonce = bucket << kaspa_consensus_core::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2;
        let anchor = base0_rc_job_anchor_v1(network_domain, pre_pow, qwen36_class_id, &bond_key.0, bucket);
        let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job inside the artifact's table");
        let ran = std::time::Instant::now();
        let run = backend.execute(&job, &prompt).expect("a real hybrid forward pass over the anchored job");
        eprintln!(
            "drill: executed the canonical job ({} prefill + {} decode) in {:?}; material {} bytes [bucket {}]",
            job.declared_prefill_tokens,
            job.exact_decode_tokens,
            ran.elapsed(),
            run.material.len(),
            bucket
        );
        let attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, pre_pow, timestamp, nonce, qwen36_class_id, &bond_key.0),
            class_id: qwen36_class_id,
            executor_bond: bond_key.0,
            executor_pubkey: keypair.verification_key.as_ref().to_vec(),
            operator_id: facts.bond.as_ref().expect("the pre-flight held a bond").operator_id,
            artifact_root: facts.artifact_root,
            trace_root: run.trace_root,
            output_root: run.output_root,
            execution_root: run.execution_root,
            pwu: facts.pwu,
            trace_manifest_root: run.trace_manifest_root,
            trace_chunk_count: run.trace_chunk_count,
            trace_retention_daa: block.header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        if class_ticket_v3(&attempt, anchor) <= facts.class_target {
            won = Some((nonce, attempt));
            break;
        }
    }
    let (nonce, attempt) = won.expect("the entrant's genesis target is winnable");
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &keypair.signing_key,
        attempt_id_v2(&attempt).as_byte_slice(),
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
        [0x5Au8; 32],
    )
    .expect("ML-DSA-87 sign")
    .as_ref()
    .to_vec();
    block.header.nonce = nonce;
    block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
    block.header.finalize();
    let hash = block.header.hash;

    ctx.validate_and_insert_block(block.to_immutable()).await.assert_valid_utxo_tip();
    assert_eq!(ctx.consensus.get_sink(), hash, "the block a Qwen3.6 execution produced is the chain");

    // Counted under the ENTRANT's class, and reserving against the bond — a Qwen3.6 block, not a
    // floor block wearing its id.
    let after = ctx.consensus.palw_producer_facts_v2(qwen36_class_id, Some(bond_key.0)).expect("still a V2 network");
    assert_eq!(after.epoch_produced_blocks, 1, "one block of THIS class, counted");
    assert!(after.bond.as_ref().expect("bonded").reserved_exposure > 0, "its claim reserves against the bond");
}

/// **A stranger can create their own bond, and the chain accepts the form they can build.**
///
/// This is the seam that decides whether a network is open. Producing needs a bond
/// (`ready_to_produce`: "the named bond is not registered on this chain"), and the only bonds any
/// chain had were the ones its genesis registry named — so mining was closed to everyone else. The
/// rules were not what closed it: `palw_lifecycle_object_may_ride_v2` has always let a
/// `BondRegistered` ride. What closed it was the carrier-binding rule demanding an outpoint that
/// named its own carrying transaction by id, which is a hash fixed point (see
/// `naming_the_carrier_by_id_is_a_fixed_point_no_registrant_can_solve`): the object rides in the
/// payload, and the payload is in the id.
///
/// So the registrant names the output by INDEX with a zero id, the extractor substitutes the id it
/// observes, and the signature is made over the zero form. Two parties have to agree about which
/// bytes were signed, and this drives BOTH of them — the real extractor and the real validator —
/// because a correspondence like that only shows up in a round trip. Signing the substituted form
/// instead is refused below, which is what makes the passing half meaningful.
#[tokio::test]
async fn palw_v2_a_stranger_can_register_their_own_bond() {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{
        PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2, palw_bond_registration_signed_key_v2,
        palw_lifecycle_objects_from_accepted_txs_v2,
    };
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_state_v2::{
        PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT, PalwBondKeyV2, PalwConsensusObjectV2 as Obj, palw_bond_registration_message_v2,
    };
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
    use kaspa_consensus_core::tx::{Transaction, TransactionId, TransactionOutpoint, TransactionOutput};

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    let vp = ctx.consensus.virtual_processor();
    let (tip, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: tip,
        daa_score: ctx.consensus.get_virtual_daa_score(),
        blue_score: 2,
        subsidy: 0,
    };

    // A stranger: a key this chain has never seen, on no registry.
    let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0xC1u8; 32]);
    let pubkey = keypair.verification_key.as_ref().to_vec();
    let payout_payload = kaspa_hashes::Hash64::from_bytes([0x5Au8; 64]);
    let collateral = bundle.state.min_collateral_sompi();
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );

    // What a registrant can compute at signing time: "the output at index 0 of whatever carries me".
    let declared = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::default(), 0));
    let sign_over = |key: &PalwBondKeyV2| {
        let message =
            palw_bond_registration_message_v2(network_domain, key, &pubkey, &pubkey, collateral, &payout_payload, &Default::default());
        libcrux_ml_dsa::ml_dsa_87::sign(
            &keypair.signing_key,
            message.as_byte_slice(),
            PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT,
            [9u8; 32],
        )
        .expect("sign")
        .as_ref()
        .to_vec()
    };

    let carry = |signature: Vec<u8>| {
        let object = Obj::BondRegistered {
            bond: declared,
            pubkey: pubkey.clone(),
            operator_pubkey: pubkey.clone(),
            collateral,
            payout_payload,
            capable_classes: Default::default(),
            signature,
        };
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object })
            .expect("the lifecycle payload serializes");
        Transaction::new(
            0,
            vec![],
            // The collateral, in an output of this very transaction, paying the declared payee.
            vec![TransactionOutput::new(
                collateral,
                kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(payout_payload.as_byte_slice()),
            )],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE.clone(),
            0,
            payload,
        )
    };

    // **The honest trip.** Signed over the zero form, carried, extracted, validated.
    let tx = carry(sign_over(&declared));
    let extracted = palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(&tx));
    assert!(extracted.skipped.is_empty(), "the carrier binds: {:?}", extracted.skipped);
    let [carried] = &extracted.objects[..] else { panic!("exactly one object rides") };
    let Obj::BondRegistered { bond, .. } = &carried.object else { panic!("and it is the registration") };
    assert_eq!(bond.0, TransactionOutpoint::new(tx.id(), 0), "the chain keyed the bond to its carrier");
    assert_eq!(palw_bond_registration_signed_key_v2(bond), declared, "and a verifier recovers what was signed");

    vp.palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(&carried.object))
        .expect("a stranger's own signed, collateral-locking bond registration must be accepted");

    // **Signing the substituted form instead is refused.** Without this the assertion above would
    // also pass if the verifier had simply stopped checking, and the two halves of the
    // correspondence would be free to drift apart.
    let substituted = PalwBondKeyV2(TransactionOutpoint::new(tx.id(), 0));
    let wrong = carry(sign_over(&substituted));
    let extracted = palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(&wrong));
    let [carried_wrong] = &extracted.objects[..] else { panic!("it still rides — the lock is about money, not signatures") };
    let err = vp
        .palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(&carried_wrong.object))
        .expect_err("a signature over the key the chain substituted is not the one the registrant makes");
    assert!(err.contains("not signed by the key it declares"), "got {err}");
}

/// **Nobody can register a class without a bond that signed for it** (launch blockers §3).
///
/// ADR-0049 Decision H made post-genesis registration a live path and nothing signed it. A
/// registration takes a permille from EVERY incumbent through largest-remainder donation, and the
/// share field's own doc says "whoever may register a class may fund it, and nobody else may move a
/// permille" — there was no `whoever`. Any stranger could move the cadence table for a transaction
/// fee.
#[tokio::test]
async fn palw_v2_a_class_registration_needs_a_bond_that_signed_for_it() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_state_v2::{
        PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT, PalwBondKeyV2, PalwClassAdmissionCarriageV2, PalwConsensusObjectV2 as Obj,
        PalwPwuRuleV2, palw_class_registration_message_v2,
    };

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();

    let vp = ctx.consensus.virtual_processor();
    let (tip, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: tip,
        daa_score: ctx.consensus.get_virtual_daa_score(),
        blue_score: 2,
        subsidy: 0,
    };

    // A Qwen-shaped entrant, admissible on its own merits — so what decides this test is the
    // authority and nothing else.
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_profile_v1(
        kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
            tile_len: 16_384,
            ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B
        },
    )
    .expect("expressible");
    let class_id = profile.shape_profile_id();
    let share = bundle.state.min_grantable_share_permille();
    let registrant = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0),
        0,
    ));
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    // The signed preimage is the whole object (audit M2-6), so the fixture states the same values
    // the `make` closure below builds it with — a mismatch here is the attack the fix closes.
    let signed_artifact_root = kaspa_hashes::Hash64::from_u64_word(0xA7);
    let signed_rule = PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1 };
    let signed_canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, 8, 4);
    let message = palw_class_registration_message_v2(
        network_domain,
        class_id,
        share,
        0,
        &registrant,
        signed_artifact_root,
        1,
        u128::MAX / 2,
        &signed_rule,
        &signed_canonical,
    );
    let sign = |kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair| {
        libcrux_ml_dsa::ml_dsa_87::sign(
            &kp.signing_key,
            message.as_byte_slice(),
            PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT,
            [7u8; 32],
        )
        .expect("sign")
        .as_ref()
        .to_vec()
    };
    let make = |bond: PalwBondKeyV2, signature: Vec<u8>| Obj::ClassRegistered {
        class_id,
        artifact_root: kaspa_hashes::Hash64::from_u64_word(0xA7),
        slash_value_per_pwu: 1,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1 },
        initial_target: u128::MAX / 2,
        share_permille: share,
        activation_daa: 0,
        admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
            profile: profile.clone(),
            canonical: kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, 8, 4),
            registrant_bond: bond,
            signature,
        })),
    };

    // **Unsigned: refused.** This is the attack — a stranger moving the cadence table.
    let err = vp
        .palw_v2_validate_objects(&state, &bundle.state, &point, &[make(registrant, Vec::new())])
        .expect_err("an unsigned registration must not move a permille");
    assert!(err.contains("not signed by the bond it names"), "got {err}");

    // Signed by the WRONG key under a real bond: also refused. Holding a bond is not enough.
    let wrong = crate::consensus::test_consensus::TestConsensus::palw_v2_registry_keypair(3);
    let err = vp
        .palw_v2_validate_objects(&state, &bundle.state, &point, &[make(registrant, sign(wrong))])
        .expect_err("a signature from another key is not this bond's authority");
    assert!(err.contains("not signed by the bond it names"), "got {err}");

    // Naming a bond the chain does not have: refused before any signature is looked at.
    let stranger = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        kaspa_consensus_core::tx::TransactionId::from_u64_word(0xDEAD),
        0,
    ));
    let err = vp
        .palw_v2_validate_objects(&state, &bundle.state, &point, &[make(stranger, Vec::new())])
        .expect_err("a bond this chain does not have is nobody");
    assert!(err.contains("does not have"), "got {err}");

    // And the real holder's signature passes the authority check — whatever the graph gates then
    // say is a different question, and it is not "who".
    let kp = crate::consensus::test_consensus::TestConsensus::palw_v2_harness_keypair();
    let outcome = vp.palw_v2_validate_objects(&state, &bundle.state, &point, &[make(registrant, sign(kp))]);
    if let Err(e) = &outcome {
        assert!(!e.contains("not signed by the bond it names") && !e.contains("does not have"), "authority must pass: {e}");
    }
}

/// **A panel really binds, and a signed quorum really licenses the claim** (launch blockers §2)./// **A panel really binds, and a signed quorum really licenses the claim** (launch blockers §2).
///
/// Nothing in the tree ever filed a `ReceiptLicensed`, so no claim could reach `Final`: every panel
/// voided at `ReceiptTimeout` with all its seats slashed, `safe_weight` stayed zero forever, and
/// the escrowed worker carve of every block — its entire 620-permille worker base share — was
/// burned. The lattice had no configuration in which it turned over.
///
/// This drives the real edges: the chain derives the panel, `palw_seat_duties_v2` reports what the
/// seats owe, real ML-DSA-87 receipts are signed over `palw_receipt_message_v2`, and the acceptance
/// layer's quorum check licenses the claim.
#[tokio::test]
async fn palw_v2_a_signed_quorum_licenses_a_claim() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2};
    use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj};

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 64);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Past the anchor delay, so the chain can derive a panel for the first claims.
    for _ in 0..(bundle.panel.anchor_delay() + 4) {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor();
    let (tip, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");

    // **A panel really bound.** Before the registry gates this was impossible with one bond.
    let (claim_id, claim) = state
        .claims_iter()
        .find(|(id, c)| matches!(c.phase, PalwClaimPhaseV2::PanelBound { .. }) && state.panel(id).is_some())
        .map(|(id, c)| (*id, c.clone()))
        .expect("the chain derives a panel once the anchor exists");
    let panel = state.panel(&claim_id).expect("bound").clone();
    assert_eq!(panel.seats.len(), bundle.panel.seat_count() as usize, "a full jury, not a short one");

    // **The seats see their duty.** A seat cannot act on something it cannot see.
    let mine: Vec<_> = panel.seats.iter().map(|s| s.bond).collect();
    let duties = kaspa_consensus_core::palw_producer_v2::palw_seat_duties_v2(&state, &bundle.state, &mine);
    let for_this_claim: Vec<_> = duties.iter().filter(|d| d.claim_id == claim_id).collect();
    assert_eq!(for_this_claim.len(), mine.len(), "every seat of this panel is a duty this node holds");
    assert!(duties.len() >= for_this_claim.len(), "and duties across every bound claim are reported, not just one");
    let duty = for_this_claim[0];
    assert_eq!(duty.execution_root, claim.execution_root, "and it carries what the seat must decide against");
    assert_ne!(duty.executor_bond, duty.seat_bond, "a seat never judges its own claim");
    assert!(!duty.free_prompt, "an attempt claim's duty names the anchor-derived lane — the seat re-hashes, never replays");

    // Real signatures, from the harness identity every genesis bond registers.
    // Each seat signs under ITS OWN registered key — the quorum check resolves the seat bond to its
    // registry pubkey, so one shared key would (correctly) fail to verify for the others.
    // `palw_devnet_bond_registry_v1` keys row `n` at txid `0xB0 + n`, so the row index is the
    // outpoint's own offset — and row 0 is the harness identity the executor bond registers.
    let seat_key = |bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2| {
        let index = (0..16u64)
            .find(|i| bond.0.transaction_id == kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0 + i) && bond.0.index == 0)
            .expect("a registry row");
        if index == 0 {
            crate::consensus::test_consensus::TestConsensus::palw_v2_harness_keypair()
        } else {
            crate::consensus::test_consensus::TestConsensus::palw_v2_registry_keypair(index)
        }
    };
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let signed_daa = ctx.consensus.get_virtual_daa_score();
    let receipts: Vec<PalwSeatReceiptV2> = panel
        .seats
        .iter()
        .take(bundle.panel.quorum() as usize)
        .map(|seat| {
            let message = palw_receipt_message_v2(network_domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &seat_key(&seat.bond).signing_key,
                message.as_byte_slice(),
                kaspa_consensus_core::palw_panel_v2::PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                [0x11u8; 32],
            )
            .expect("sign")
            .as_ref()
            .to_vec();
            PalwSeatReceiptV2 { claim: claim_id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: seat.bond, signed_daa, signature }
        })
        .collect();

    let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: tip,
        daa_score: signed_daa,
        blue_score: signed_daa,
        subsidy: 0,
    };
    let object = Obj::ReceiptLicensed { claim: claim_id, receipts: receipts.clone() };
    vp.palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(&object))
        .expect("a signed quorum licenses the claim");

    // And one seat short of quorum does not — the quorum is a bound, not a formality.
    let short = Obj::ReceiptLicensed { claim: claim_id, receipts: receipts[..receipts.len() - 1].to_vec() };
    assert!(
        vp.palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(&short)).is_err(),
        "below quorum is refused"
    );

    // Folding it moves the claim out of PanelBound — the edge that did not exist.
    // The transition demands a strictly-increasing chain point; acceptance above does not, so the
    // fold gets its own point one step past the tip's.
    let next_point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: kaspa_consensus_core::BlockHash::from_u64_word(0xF01D),
        daa_score: signed_daa + 1,
        blue_score: state.last_point().map(|p| p.blue_score).unwrap_or(0) + 1,
        subsidy: 0,
    };
    let (licensed, _) = kaspa_consensus_core::palw_state_v2::apply_palw_transition_v2(
        &state,
        &bundle.state,
        &next_point,
        std::slice::from_ref(&object),
        None,
    )
    .expect("the transition takes it");
    assert!(
        matches!(licensed.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
        "the claim is licensed, and its path to Final is open"
    );
}

/// **The pruning-point PALW import installs a real state and REFUSES a forged one**/// **The pruning-point PALW import installs a real state and REFUSES a forged one** (launch
/// blockers §1, the import half).
///
/// `PalwChainStateV2` was written only by `process_genesis`, so a node joining by pruned IBD had
/// none. The startup guard now stops such a node from running; this is what lets it exist at all.
///
/// The gate is the ROOT, and it runs before the write: `into_state` rebuilds the carriage and
/// demands back the root the pruning point's own header commits. A peer that forges one byte of
/// bonds, shares or claims produces a different root and is refused HERE — not detected after a
/// durable write, which would be no defence at all, since forged bonds are block-production rights
/// and forged claims are `safe_weight`.
#[tokio::test]
async fn palw_v2_the_pruning_point_import_verifies_the_root_before_it_writes() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor();
    // The import verifies against the CHILD header — `palw_state_root` commits the state as-of the
    // block's selected parent — so the point being imported has to be one with a child on the
    // chain. The store's tip has none yet, so walk back one: the tip's selected parent is a chain
    // block whose child (the tip) commits its state.
    let (tip, _) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let point = {
        use crate::model::stores::ghostdag::GhostdagStoreReader;
        vp.ghostdag_store.get_selected_parent(tip).expect("the tip has a selected parent")
    };
    let state = {
        // The state AT `point`, rebuilt the way the chain walk rebuilds it.
        let store = vp.palw_state_v2_store.read();
        let (_, delta) = store.delta_of(tip).expect("the tip has a delta");
        let (_, tip_state) = store.load_tip(&bundle.state).unwrap().unwrap();
        kaspa_consensus_core::palw_state_v2::revert_delta_v2(&tip_state, &delta, &bundle.state)
            .expect("reverting the tip's own delta yields its parent's state")
    };
    let honest = PalwStateCarriageV2::from_state(&state);

    // **What a peer would serve, through the production path — which is the SNAPSHOT, not the tip.**
    //
    // This assertion used to read the tip, and passed for that reason alone: on a running node the
    // tip is rewritten to the sink on every virtual walk, so the server's "is the tip the pruning
    // point?" test was permanently false and every pruned IBD aborted. The test could not see it
    // because it served the same row it had just written. Now it captures first, as
    // `advance_pruning_point_if_possible` does, and asks for what the capture put there.
    assert!(vp.pruning_point_palw_state(tip).is_none(), "nothing is servable before a snapshot is captured");
    vp.capture_pruning_point_palw_state(tip);
    let served = vp.pruning_point_palw_state(tip).expect("the captured snapshot is servable");
    assert_eq!(served, PalwStateCarriageV2::from_state(&vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap().1));
    assert!(vp.pruning_point_palw_state(kaspa_consensus_core::BlockHash::from_u64_word(0xBAD)).is_none(), "and nothing else");

    // Empty the store the way a pruned join leaves it, then import what the peer served.
    {
        let mut store = vp.palw_state_v2_store.write();
        store.delete_tip_for_tests().expect("empty the store");
    }
    assert!(vp.palw_state_v2_store.read().tip_record().unwrap().is_none());

    // **A forged carriage is refused, and the store stays empty.** One extra bond is all it takes:
    // bonds are block-production rights, so this is the exact lie the gate exists to stop.
    let mut forged = honest.clone();
    forged.class_shares.insert(kaspa_hashes::Hash64::from_u64_word(0x5EED), 1);
    let err = vp.import_pruning_point_palw_state(point, forged).expect_err("a forged carriage must not install");
    assert!(format!("{err}").contains("does not rebuild to the root"), "and the refusal names the reason: {err}");
    assert!(
        vp.palw_state_v2_store.read().tip_record().unwrap().is_none(),
        "the refusal happens BEFORE the write — detection afterwards would be no defence"
    );

    // The honest one installs, and what lands is the state the chain had.
    vp.import_pruning_point_palw_state(point, honest).expect("the honest carriage installs");
    let (imported_block, imported) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("installed");
    assert_eq!(imported_block, point);
    assert_eq!(imported.state_root(), state.state_root(), "and it is the same state, root for root");
}

/// **The pruning-point witness is the selected-chain child — a side block can neither poison the
/// import nor choose the examiner.** (audit M1-2, re-audit R-3)
///
/// The acceptance test both audits asked for and neither had. `import_pruning_point_palw_state` and
/// `import_pruning_point_overlay_snapshot` are the two places a joining node writes peer-supplied
/// state — bonds, which are block-production rights, and a reserve balance, which is mintable coin —
/// and both verify the payload against a header committed by a CHILD of the pruning point. Which
/// child is therefore the whole security of the gate, and it went wrong twice:
///
/// * requiring **every** child to agree (before M1-2) let one cheap side block with a garbage root
///   veto the pruning point permanently: the disagreement is a fact of the DAG, so no retry and no
///   other peer could clear it;
/// * taking the **heaviest** child (M1-2's repair) still let the peer choose, because blue-work ties
///   between siblings are the normal case and the tiebreak was the block hash — which is grindable.
///
/// The witness is now the child on the selected chain to the header DAG's selected tip, of which
/// there is at most one by construction. The shape below is what separates the three rules: one
/// pruning point, one honest chain child, and three siblings committing garbage roots.
///
/// **The discrimination is pinned, not sampled.** All four children of the pruning point carry the
/// same blue work, so the heaviest-child rule turns entirely on its hash tiebreak — and the block
/// hashes here are not stable between runs. Written the obvious way this test caught a revert to
/// that rule only about three runs in four: flaky, not red, which pins nothing. Each sibling is
/// therefore ground until it out-ranks the chain child under exactly the comparison the discarded
/// rule used, and the test asserts that precondition before it asserts anything else. Under the
/// unanimity rule assertion (1) fails; under the heaviest-child rule assertion (1) fails too, and
/// on EVERY run, because the examiner the gate reaches for is then always one of the liars.
#[tokio::test]
async fn the_pruning_point_witness_is_the_selected_chain_child_not_a_side_block() {
    use crate::model::stores::ghostdag::GhostdagStoreReader;
    use crate::model::stores::headers::HeaderStoreReader;
    use crate::model::stores::relations::RelationsStoreReader;
    use kaspa_consensus_core::errors::pruning::PruningImportError;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..2 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    // `point` is the block whose state a joining node will import. It is about to acquire one
    // honest chain child and three lying siblings.
    let point = ctx.consensus.get_sink();

    // ---- The attacker's block TEMPLATES, built NOW so their only parent is `point`. They are held
    //      back and finalized further down, once the honest chain child is known: the garbage root
    //      each one commits is chosen so that the DISCARDED rule would deterministically pick it.
    //      An attacker can always publish such a block, which is the premise of the whole gate.
    let mut siblings = Vec::new();
    for i in 0..3u64 {
        let side = ctx.build_block_template(0x51D0 + i, ctx.simulated_time + 1).block;
        assert_eq!(side.header.direct_parents(), &[point], "the sibling hangs off the point being imported");
        siblings.push(side);
    }

    // ---- The honest chain continues from `point` and outruns them. Three blocks is enough that no
    //      sibling can win a blue-work tie at the header selected tip: to move the selected chain
    //      off `point`'s honest child an attacker would have to out-work the chain from the pruning
    //      point forward, which is the security level this gate is supposed to have.
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let tip = ctx.consensus.get_sink();

    // The child of `point` on the selected chain to `tip` — the one witness the repaired gate may
    // use. Read BEFORE the siblings arrive, so it is the chain's answer and not a race with them.
    let chain_child = {
        let vp = ctx.consensus.virtual_processor();
        let mut c = vp.ghostdag_store.get_selected_parent(tip).expect("the tip has a selected parent");
        let mut prev = tip;
        while c != point {
            prev = c;
            c = vp.ghostdag_store.get_selected_parent(c).expect("selected parent");
        }
        prev
    };

    // ---- **Pin the discrimination instead of hoping for it.** All four children of `point` carry
    //      the SAME blue work — one block each, one shared parent — so M1-2's "heaviest child" rule
    //      decides on its hash tiebreak alone, and with three siblings against one honest child the
    //      honest child holds the maximum about one run in four. Block hashes here are not stable
    //      across runs (the templates carry a wall-clock timestamp), so left to chance this test
    //      catches a revert to that rule only ~75% of the time — it was flaky, not red, and a flaky
    //      test does not pin a consensus rule. So each sibling is ground until it OUT-RANKS the
    //      chain child under exactly the comparison the discarded rule used. The grind is free:
    //      `palw_state_root` is garbage by construction, so any value in the 0xBAD* family serves.
    let mut side_roots = Vec::new();
    for (i, side) in siblings.iter_mut().enumerate() {
        let mut attempt = 0u64;
        let garbage = loop {
            let garbage = kaspa_hashes::Hash64::from_u64_word(0xBAD0 + i as u64 + (attempt << 16));
            side.header.palw_state_root = garbage;
            side.header.palw_commitment = ctx.consensus.palw_v2_test_carriage(&side.header);
            side.header.finalize();
            if side.header.hash > chain_child {
                break garbage;
            }
            attempt += 1;
            assert!(attempt < 4096, "could not out-rank the chain child in 4096 grinds — the tiebreak is not what this test assumes");
        };
        side_roots.push(garbage);
    }
    assert!(
        siblings.iter().all(|s| s.header.hash > chain_child),
        "precondition of the discrimination: every sibling out-ranks the chain child, so the heaviest-child rule picks a LIAR on every run"
    );

    // ---- Now the siblings arrive, as they would from any peer.
    for side in siblings {
        let hash = side.header.hash;
        ctx.validate_and_insert_block(side.to_immutable()).await;
        assert_eq!(
            ctx.consensus.virtual_processor().ghostdag_store.get_selected_parent(hash).expect("the sibling is in the DAG"),
            point,
            "the sibling is a child of the point being imported — that is the whole premise"
        );
    }
    assert_eq!(ctx.consensus.get_sink(), tip, "and no sibling became the chain");
    assert_eq!(
        {
            let vp = ctx.consensus.virtual_processor();
            let mut c = vp.ghostdag_store.get_selected_parent(tip).expect("the tip has a selected parent");
            let mut prev = tip;
            while c != point {
                prev = c;
                c = vp.ghostdag_store.get_selected_parent(c).expect("selected parent");
            }
            prev
        },
        chain_child,
        "and the arriving siblings did not move the selected chain off the honest child"
    );

    let vp = ctx.consensus.virtual_processor();
    let honest_root = vp.headers_store.get_header(chain_child).expect("the chain child's header").palw_state_root;
    assert_ne!(honest_root, kaspa_hashes::ZERO_HASH64, "the chain child commits a real root");

    // The point really does have four children, and only one of them is the chain's.
    let children: Vec<_> = RelationsStoreReader::get_children(&vp.relations_service, point)
        .map(|c| c.read().iter().copied().collect())
        .unwrap_or_default();
    assert_eq!(children.len(), 4, "one honest chain child and three siblings");

    // The state AT `point`, rebuilt the way the chain walk rebuilds it: revert the selected chain's
    // deltas from the tip back down.
    let state = {
        let store = vp.palw_state_v2_store.read();
        let (_, mut st) = store.load_tip(&bundle.state).unwrap().expect("the tip loads");
        let mut cur = tip;
        while cur != point {
            let (_, delta) = store.delta_of(cur).expect("every chain block has a delta");
            st = kaspa_consensus_core::palw_state_v2::revert_delta_v2(&st, &delta, &bundle.state)
                .expect("reverting walks back one block");
            cur = vp.ghostdag_store.get_selected_parent(cur).expect("selected parent");
        }
        st
    };
    let honest = PalwStateCarriageV2::from_state(&state);

    // Empty the store the way a pruned join leaves it, then import.
    {
        let mut store = vp.palw_state_v2_store.write();
        store.delete_tip_for_tests().expect("empty the store");
    }

    // **(1) The siblings did not poison the pruning point.** Under the pre-M1-2 unanimity rule this
    // is the assertion that failed, and no retry against any peer could have cleared it.
    vp.import_pruning_point_palw_state(point, honest.clone()).expect("the honest carriage installs despite three lying siblings");
    let (imported_block, imported) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("installed");
    assert_eq!(imported_block, point);
    assert_eq!(imported.state_root(), state.state_root(), "and it is the state the chain had, root for root");

    // **(2) The examiner was the chain child, not a sibling.** A forged carriage is refused and the
    // error names the root the gate demanded, which must be the chain child's. This is what
    // separates "heaviest child" from "selected-chain child": under the old rule the demanded root
    // would be one of the planted `0xBAD*` values whenever that sibling won the blue-work tiebreak,
    // and the carriage the attacker also supplied would then be the one that installs.
    {
        let mut store = vp.palw_state_v2_store.write();
        store.delete_tip_for_tests().expect("empty the store again");
    }
    let mut forged = honest.clone();
    forged.class_shares.insert(kaspa_hashes::Hash64::from_u64_word(0x5EED), 1);
    let err = vp.import_pruning_point_palw_state(point, forged).expect_err("a forged carriage must not install");
    let demanded = match &err {
        PruningImportError::ImportedPalwStateInvalid(_, root, _) => *root,
        other => panic!("expected the root-mismatch refusal, got {other:?}"),
    };
    assert_eq!(demanded, honest_root, "the gate examined the selected-chain child");
    for garbage in &side_roots {
        assert_ne!(demanded, *garbage, "and never a sibling's planted root");
    }
    assert!(
        vp.palw_state_v2_store.read().tip_record().unwrap().is_none(),
        "the refusal happens BEFORE the write — detection afterwards would be no defence"
    );
}

/// **A ConsensusV2 node with no PALW state refuses to run** (launch blockers §1)./// **A ConsensusV2 node with no PALW state refuses to run** (launch blockers §1).
///
/// Absent state was read as "no policy", and every PALW authority then failed OPEN: the state root
/// unchecked, no transition applied, tips ordered by blue work alone, any pruning point allowed,
/// the deep-reorg comparator skipped. It does not FORK — it is strictly more permissive — so a node
/// in that state follows the blue-work-heaviest chain that frontier-first ordering exists to refuse,
/// and nothing anywhere reports it.
///
/// Four ways in: pruned IBD, a datadir predating the bundle (bundle-free and bundled testnet-12
/// share a genesis, so the re-genesis guard cannot see the difference), a schema bump that
/// `reindex_if_stale` answers by deleting the tip, and the staging consensus.
///
/// This drives the same construction path a node takes, with the store emptied — the state a
/// pruned join lands in. Deleting the guard makes it build happily, which is the point.
#[tokio::test]
async fn palw_v2_a_node_with_no_palw_state_refuses_to_run() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle(&catalog);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();

    // A healthy node installs the tip at genesis and runs.
    let tc = TestConsensus::new(&config);
    let _lt = tc.init();
    assert!(
        tc.virtual_processor().palw_state_v2_store.read().tip_record().unwrap().is_some(),
        "a genesis-processed ConsensusV2 node holds a PALW tip"
    );

    // Now the state a pruned join lands in: the store emptied under a live bundle. Every PALW leg
    // reads `None` and becomes a no-op — the silent-downgrade the guard exists to make impossible.
    let vp = tc.virtual_processor();
    {
        let mut store = vp.palw_state_v2_store.write();
        store.delete_tip_for_tests().expect("the fixture can empty its own store");
    }
    assert!(
        vp.palw_state_v2_store.read().tip_record().unwrap().is_none(),
        "and with the tip gone every PALW authority would silently fail open"
    );

    // Every PALW leg is now a no-op on this node, which is the silent downgrade itself: the
    // pruning ceiling allows anything, and the candidate order has no authority to express.
    assert!(
        vp.palw_pruning_point_allowed_v2(tc.get_sink()),
        "with no state the pruning ceiling permits any point — the fail-open the guard exists to stop"
    );
}

/// **A staging consensus is not a node, and refusing it closed the network to newcomers.**
///
/// `IbdType::DownloadHeadersProof` — the path every fresh join takes onto a chain with history —
/// creates a staging consensus to replay the proof into. Staging holds no PALW state by
/// construction: it is a scratch database that is promoted or discarded, and the guard above says
/// so in its own words ("the one legitimate stateless case").
///
/// It could not recognise it. `db_has_history` was read AFTER `virtual_processor.init()`, and
/// `init` writes `past_pruning_points[0]` on any database that lacks a pruning point — so the row
/// that was supposed to mean "this database carries a chain" was written by the same constructor,
/// three lines earlier, for every consensus that has ever existed. Staging looked like a resuming
/// node, the guard fired, and the node died one second into its first IBD.
///
/// Measured on testnet-11 on 2026-08-26: three fresh hosts, three identical panics, no way to join
/// the network except to restart until the datadir had enough state to pass — which is not a join
/// path, it is a coin flip an operator has to know about.
///
/// This is the staging case; `palw_v2_constructing_a_bundled_consensus_over_an_empty_store_panics`
/// is the resuming one, and both must hold: a guard that cannot tell them apart is either a network
/// with no newcomers or a node whose PALW authority silently fails open.
#[test]
fn palw_v2_a_staging_consensus_without_palw_state_is_allowed() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle(&catalog);
    // Exactly what `ConsensusFactory::new_staging_consensus` builds: the network's own config with
    // genesis skipped, over a database nothing has written to.
    let staging = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .skip_adding_genesis()
        .build();

    let (_lifetime, db) = kaspa_database::create_temp_db!(kaspa_database::prelude::ConnBuilder::default().with_files_limit(10));
    let (sender, _r) = async_channel::unbounded();
    // Constructing it is the whole assertion — the defect was a panic in `Consensus::new`.
    let consensus = crate::consensus::test_consensus::TestConsensus::with_db(db, &staging, sender);
    assert!(
        consensus.virtual_processor().palw_state_v2_store.read().tip_record().unwrap().is_none(),
        "and it is still stateless afterwards — the point is that staging is ALLOWED to be, not that \
         something quietly installed a tip to get past the guard"
    );
}

/// **And the guard actually refuses** — a ConsensusV2 consensus cannot be constructed over an empty
/// PALW store.
///
/// The test above proves the DEGRADED STATE is real; this proves the startup assertion fires in it.
/// Both halves are needed: a guard whose precondition is unreachable is decoration, and a
/// precondition with no guard is the bug.
#[test]
#[should_panic(expected = "PALW state store holds no tip")]
fn palw_v2_constructing_a_bundled_consensus_over_an_empty_store_panics() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle(&catalog);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();

    // One database, opened twice. The first build installs genesis AND the PALW tip; the store is
    // then emptied the way a pruned join or a `reindex_if_stale` leaves it. The second open is a
    // RESTART — `process_genesis` is off, but the database carries `past_pruning_points[0]`, so
    // this is a node resuming its own chain with its PALW state missing. It must refuse.
    let (_lifetime, db) = kaspa_database::create_temp_db!(kaspa_database::prelude::ConnBuilder::default().with_files_limit(10));
    let (sender, _r) = async_channel::unbounded();
    let first = crate::consensus::test_consensus::TestConsensus::with_db(db.clone(), &config, sender.clone());
    let _lt = first.init();
    {
        let mut store = first.virtual_processor().palw_state_v2_store.write();
        store.delete_tip_for_tests().expect("empty the store the way a pruned join leaves it");
    }
    drop(first);

    let resumed = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .skip_adding_genesis()
        .build();
    let _reopened = crate::consensus::test_consensus::TestConsensus::with_db(db, &resumed, sender);
}

/// **Audit M-01: one unauthenticated transaction used to slash a producer and every panel seat.**/// **Audit M-01: one unauthenticated transaction used to slash a producer and every panel seat.**
///
/// `ProducerDefaulted { claim, receipts: [] }` carried no signature of any kind, the acceptance
/// match had no arm for it (`_ => {}`), and the transition folded it — charging every seat
/// `claim.reserved` through `slash_silent_seats` and debiting the producer's bond through
/// `void_and_slash`. `validate_receipt_quorum_v2` — which checks seat membership, the ML-DSA-87
/// signature, dedup, the window AND the quorum — was written, tested, and had no caller anywhere.
///
/// This drives the acceptance layer directly, because that is the layer that was empty. Deleting
/// either half of the fix makes it pass again: the arm, or the exhaustive match that stops a new
/// object kind from slipping past without a decision.
#[tokio::test]
async fn palw_v2_an_unsigned_receipt_set_cannot_slash_anyone() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..2 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor();
    let (tip, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let claim = *state.claims_iter().next().expect("two blocks made two claims").0;
    let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: tip,
        daa_score: ctx.consensus.get_virtual_daa_score(),
        blue_score: 3,
        subsidy: 0,
    };

    // The attack, verbatim: name a real claim, carry nothing.
    for object in [Obj::ProducerDefaulted { claim, receipts: Vec::new() }, Obj::ReceiptLicensed { claim, receipts: Vec::new() }] {
        let err = vp
            .palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(&object))
            .expect_err("an unsigned receipt set must not be able to move a claim");
        assert!(err.contains("quorum"), "and the refusal must name the missing quorum: got {err}");
    }

    // **A retirement is authorised by the bond's own key.** It used to be refused outright, which
    // shut the door and locked every genesis bond's collateral in behind it. Now the check is the
    // one the refusal described, so the two ways of getting it wrong must both still fail: a bond
    // this chain does not have, and a real bond whose owner did not sign.
    let stranger = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0),
        0,
    ));
    let err = vp
        .palw_v2_validate_objects(
            &state,
            &bundle.state,
            &point,
            &[Obj::BondRetireRequested { bond: stranger, signature: vec![0xEE; 8] }],
        )
        .expect_err("a retirement nobody signed must not release anyone's collateral");
    assert!(err.contains("not signed by the key it registered"), "got {err}");

    // And an EMPTY signature is refused a layer earlier, on the ride list, so the two locks are
    // still two — removing either does not open the door on its own.
    assert!(
        kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&Obj::BondRetireRequested {
            bond: stranger,
            signature: Vec::new(),
        })
        .is_err(),
        "an unsigned retirement must not even ride"
    );
}

/// **A gossiped receipt pool assembles into exactly the object a block accepts** (launch
/// blockers: "what is still missing" — the submitter's consensus half, end to end).
///
/// The correspondence that must hold: `palw_v2_receipt_quorum_assemble` is what the panel service
/// submits, `palw_v2_validate_objects` is what the chain runs on the carried object, and both are
/// fed here from the same state — receipts signed with the REAL registry keys, polluted the way a
/// real gossip pool is (garbage signature, duplicate seat), against a claim whose panel the chain
/// derived itself. If the assembled object were ever one acceptance refuses, the submitter would
/// burn fees on transactions the chain drops; this is the test that says it cannot.
#[tokio::test]
async fn palw_v2_a_gossiped_receipt_pool_assembles_the_object_a_block_accepts() {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{
        PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2, validate_palw_lifecycle_tx,
    };
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_panel_v2::{
        PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
    };
    use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2, PalwConsensusObjectV2};

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 64);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    // One claim, then enough chain for its anchor (accepted + 20) to pass and the DERIVED panel
    // binding to fire — the same automatic binding a real network runs.
    for _ in 0..26 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let vp = ctx.consensus.virtual_processor();
    let (_, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let (claim_id, _) = state
        .claims_iter()
        .find(|(_, c)| matches!(c.phase, PalwClaimPhaseV2::PanelBound { .. }))
        .expect("an early claim's panel has bound by now");
    let claim_id = *claim_id;
    let panel = state.panel(&claim_id).expect("a bound claim has a panel");
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let signed_daa = ctx.consensus.get_virtual_daa_score();

    // Sign with the registry keys, resolved the way a seat resolves its own: by matching the
    // bond's REGISTERED key — never by assuming seat order.
    let sign_seat = |seat: &kaspa_consensus_core::palw_state_v2::PalwPanelSeatV2, verdict: PalwReceiptVerdictV2| {
        let bond = state.bond(&seat.bond).expect("a seat's bond is registered");
        let kp = (0..16u64)
            .map(TestConsensus::palw_v2_registry_keypair)
            .find(|kp| kp.verification_key.as_ref() == bond.pubkey.as_slice())
            .expect("a registry seat signs with a registry key");
        let message = palw_receipt_message_v2(network_domain, claim_id, verdict, signed_daa);
        let signature =
            libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT, [0u8; 32])
                .expect("ML-DSA-87 sign")
                .as_ref()
                .to_vec();
        PalwSeatReceiptV2 { claim: claim_id, verdict, seat_bond: seat.bond, signed_daa, signature }
    };
    let honest: Vec<PalwSeatReceiptV2> = panel.seats.iter().take(3).map(|s| sign_seat(s, PalwReceiptVerdictV2::Valid)).collect();

    // The pool as gossip delivers it: a forged receipt first (so a naive assembler chokes on it),
    // a duplicate of an honest seat, then the three honest ones.
    let mut forged = honest[0].clone();
    forged.signature = vec![0u8; forged.signature.len()];
    let pool = vec![forged, honest[0].clone(), honest[0].clone(), honest[1].clone(), honest[2].clone()];

    let object = vp.palw_v2_receipt_quorum_assemble_impl(claim_id, &pool).expect("three honest receipts are a quorum");
    let PalwConsensusObjectV2::ReceiptLicensed { claim, receipts } = &object else {
        panic!("a Valid quorum licenses; got {object:?}");
    };
    assert_eq!(*claim, claim_id);
    assert_eq!(receipts.len(), 3, "the forged receipt and the duplicate were dropped, the quorum kept");

    // The carrier the submitter builds is admissible at the transaction gate…
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() }).unwrap();
    validate_palw_lifecycle_tx(&payload).expect("the assembled object may ride a 0x4b transaction");

    // …and the OBJECT passes the acceptance validator at the same state — the correspondence.
    let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: ctx.consensus.get_sink(),
        daa_score: signed_daa,
        blue_score: 27,
        subsidy: 0,
    };
    vp.palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(&object))
        .expect("what the assembler builds is what acceptance takes");

    // Below quorum: two honest receipts assemble nothing, however clean.
    assert!(
        vp.palw_v2_receipt_quorum_assemble_impl(claim_id, &honest[..2]).is_none(),
        "two receipts are not a quorum and must not become an object"
    );
}

/// **The producer contract reads the LIVE chain, not the genesis bundle** (ADR-0042).
///
/// The harness's own carriage builder reads the class target out of the bundle, which is correct
/// exactly once — at genesis, before anything has been produced. A real producer cannot: the
/// per-class retarget moves the target, the epoch counter moves under every block it lands, and a
/// producer that read either from a file would build attempts the chain refuses and be told only
/// "ticket above target". So the facts come from the state store at virtual's selected parent,
/// and this asserts they MOVE — four blocks in, the epoch counter says four.
///
/// It also asserts the two halves agree on `pwu`, which is the equality admission item 6 turns
/// into a refusal. Nothing else in the tree compares them; they were written a month apart.
#[tokio::test]
async fn palw_v2_producer_facts_track_the_chain_the_blocks_actually_built() {
    use kaspa_consensus_core::api::ConsensusApi;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    let bond = kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0), 0);

    for _ in 0..4 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    let facts = ctx
        .consensus
        .palw_producer_facts_v2(bundle.base_class_id, Some(bond))
        .expect("a ConsensusV2 network answers for its own base class");
    assert_eq!(facts.chain_point, ctx.consensus.get_sink(), "the facts are read where a template would build");
    assert_eq!(facts.epoch_produced_blocks, 4, "the epoch counter moved with the blocks, which a bundle cannot do");
    assert!(facts.has_epoch_room(), "the floor's budget is not spent by four blocks");

    let bond_facts = facts.bond.as_ref().expect("the genesis bond is registered");
    assert_eq!(
        bond_facts.registered_pubkey,
        crate::consensus::test_consensus::TestConsensus::palw_v2_harness_pubkey(),
        "the key a producer must sign with is the one the chain registered"
    );
    assert_eq!(facts.ready_to_produce(&bond_facts.registered_pubkey.clone()), Ok(()), "a fifth block is producible");
    assert!(bond_facts.reserved_exposure > 0, "four open claims reserve exposure — the ceiling is a live number");

    // The two halves, on the same number. The harness computes `pwu` from the bundle; the contract
    // computes it from the state at the tip. On a chain that has not retargeted they must be
    // equal, and the equality is what admission item 6 enforces on every block.
    let carriage_pwu = {
        let header = ctx.build_block_template(0, 0).block.header.clone();
        let wire = ctx.consensus.palw_v2_test_carriage(&header);
        kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&wire)
            .expect("the harness emits a carriage")
            .attempt
            .pwu
    };
    assert_eq!(facts.pwu, carriage_pwu, "the producer contract and the carriage builder derive one pwu");
}

#[tokio::test]
async fn palw_v2_the_exposure_ceiling_bites_when_reservations_are_real() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_at_min_collateral(&catalog);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // As many as the ceiling admits — derived, not typed.
    let fits = palw_v2_claims_that_fit(&bundle);
    assert!(fits >= 2, "the fixture must admit more than one claim for the ceiling to be the thing under test");
    for _ in 0..fits {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let sink_at_ceiling = ctx.consensus.get_sink();
    {
        let store = ctx.consensus.virtual_processor().palw_state_v2_store.read();
        let (_, state) = store.load_tip(&bundle.state).unwrap().unwrap();
        assert_eq!(
            state.claims_iter().count() as u64,
            fits,
            "every admitted claim is still open — nothing has matured to release one"
        );
    }

    // The next does not: its claim would push the bond past `collateral × 500‰`, and no claim
    // has resolved to give the room back. The block exists in the DAG; it is not the chain.
    ctx.build_block_template_row(0..1).validate_and_insert_row().await;
    assert_eq!(
        ctx.consensus.get_sink(),
        sink_at_ceiling,
        "one claim past the ceiling exceeds the bond's exposure, so its block cannot become the sink"
    );
}

/// **ADR-0042 Decision 3c, re-decided: what P0 changed, and what it did not** (audit item).
///
/// The audit reads 3c's deferral as resting on "only the bond holder can mint valid-signature
/// siblings", and concludes that verifying the signature removed the reason. Reading §A2, the
/// load-bearing reason is a different one and it **survives**: with identity = `attempt_id`
/// (signature excluded), any third party flips one signature bit and relays a copy with the SAME
/// block id and an invalid witness. The first-seen copy fails admission and the id lands in
/// known-invalid caches, after which the honest block — same id, arriving second — is refused
/// unseen. One bit censors one block, network-wide, at zero cost.
///
/// Verifying the signature does not remove that primitive; it is what ARMS it. An unverified
/// witness could not fail admission, so the poisoning needed the check to exist. **3c's identity
/// half therefore stays deferred, and its precondition is unchanged: a pipeline path that rejects
/// a witness-mutated carrier WITHOUT marking the block id invalid.**
///
/// What P0 did change is the residual on the identity the tree actually keeps — raw carrier bytes.
/// Before it, ANY third party could mint valid siblings of one solved PoW. Now only the bond
/// holder can, because only it can sign; ML-DSA-87 signing here is hedged with caller-supplied
/// randomness, so it can mint as many as it likes at one signature each. §A2 asserts the bound on
/// that — "self-malleation buys DAG spam ..., never a second paid claim" — and nothing measured
/// it. This does.
#[tokio::test]
async fn palw_v2_a_bond_holders_own_resignature_buys_a_block_but_never_a_second_claim() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle(&catalog);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..2 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    // One honest block, and the SAME attempt signed a second time by the same bonded key. The
    // attempt is untouched, so both carry one `attempt_id` and one solved ticket.
    let template = ctx.build_block_template(11, ctx.simulated_time + 1);
    let honest = template.block.clone();
    let mut sibling = template.block.clone();
    let mut envelope =
        kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&sibling.header.palw_commitment).unwrap();
    let attempt_id = kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&envelope.attempt);
    envelope.signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &TestConsensus::palw_v2_harness_keypair().signing_key,
        attempt_id.as_byte_slice(),
        kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
        // Different hedging randomness — a DIFFERENT signature over the SAME message, and every
        // bit of it verifies.
        [0xA5u8; 32],
    )
    .expect("the bond holder can always sign again")
    .as_ref()
    .to_vec();
    let honest_signature =
        kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&honest.header.palw_commitment).unwrap().signature;
    assert_ne!(envelope.signature, honest_signature, "ML-DSA-87 signing here is hedged — the fixture must actually differ");
    sibling.header.palw_commitment = envelope.encode_wire();
    sibling.header.finalize();

    // Raw-carrier-bytes identity: the sibling is a DIFFERENT block, which is the property §A2
    // chose. (Under 3c's identity half it would share the honest block's id, which is the
    // censorship shape.)
    assert_ne!(sibling.header.hash, honest.header.hash, "the signature is inside block identity, by design");
    let (honest_hash, sibling_hash) = (honest.header.hash, sibling.header.hash);

    ctx.validate_and_insert_block(honest.to_immutable()).await;
    ctx.validate_and_insert_block(sibling.to_immutable()).await;

    // **The bound, measured.** Two blocks exist; one claim does. The second is refused by
    // `DuplicateAttempt` / `DuplicateClaim` — the claim is keyed on `attempt_id`, which the
    // signature is deliberately outside of, so re-signing buys a DAG block and nothing else.
    let vp = ctx.consensus.virtual_processor();
    let (_, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    assert!(state.claim(&attempt_id).is_some(), "the attempt did produce its one claim");
    let for_this_attempt = state.claims_iter().filter(|(id, _)| **id == attempt_id).count();
    assert_eq!(for_this_attempt, 1, "one attempt, one claim, however many signatures were spent on it");

    // And only one of the two can be the sink, because only one of them carries the claim.
    let sink = ctx.consensus.get_sink();
    assert!(sink == honest_hash || sink == sibling_hash);
}

/// **The attempt's signature is checked on the live path — and one solved PoW is one block.**
///
/// The pipeline called `check_palw_attempt_admission_v2`, which takes no verifier and cannot take
/// one: its item 2 compares the carried `executor_pubkey` against the bond record's key, and both
/// are public. So an attempt was admitted on the strength of naming an Active bond, and the
/// `signature` field was bytes nobody read. Two consequences, both asserted here:
///
/// * **anyone could mine under anyone's stake** — the bond outpoint and the key are on chain, so
///   an attacker copies both, writes any bytes into `signature`, and solves the PoW;
/// * **one PoW minted unlimited blocks** — the signature is deliberately outside `attempt_id_v2`
///   and therefore outside the PoW digest (ADR-0042 Decision 3c), while block identity hashes the
///   raw carrier bytes. Unverified, flipping one signature bit yields a different, equally valid
///   block at zero marginal cost.
///
/// The fixture that could not see this was the harness itself: it carried `vec![7u8; 32]` as an
/// ML-DSA-87 public key and `vec![0x5A; ..]` as an ML-DSA-87 signature, and every V2 test passed.
/// It now holds a real key pair, the genesis bond registers that key, and it signs the attempt id
/// after the ticket search — so this test's forgery is a forgery of something real.
#[tokio::test]
async fn palw_v2_a_forged_attempt_signature_cannot_become_the_sink() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle(&catalog);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let mut ctx = TestContext::new(consensus);
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let sink_before = ctx.consensus.get_sink();

    // Everything an honest miner produces — the right bond, the registered key, a derived pwu, a
    // winning class ticket — and ONE flipped signature byte. Nothing else moves, so nothing else
    // can be the reason it is refused.
    let template = ctx.build_block_template(11, ctx.simulated_time + 1);
    let mut forger = template.block.clone();
    let mut envelope =
        kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&forger.header.palw_commitment).unwrap();
    let honest_id = kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&envelope.attempt);
    envelope.signature[0] ^= 0x01;
    forger.header.palw_commitment = envelope.encode_wire();
    forger.header.finalize();
    let forged_hash = forger.header.hash;

    // The attempt itself is untouched: the id the PoW commits to is the honest one, which is
    // exactly why an unverified signature was free to vary.
    let reread = kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&forger.header.palw_commitment).unwrap();
    assert_eq!(
        kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&reread.attempt),
        honest_id,
        "the forgery moves no priced field — only the signature, which is what makes it free"
    );

    // **It is refused at the DOOR now, not at the chain walk** (launch blockers §5).
    //
    // The chain-walk check kept a forgery from becoming the sink, and that was thought sufficient.
    // It was not: the block was still valid to relay, so every peer accepted, stored and forwarded
    // it — and since the signature sits outside `commitment_root_v2` while the block-identity
    // digest hashes the raw carrier bytes, ONE solved proof of work minted an unbounded number of
    // distinct such blocks, a byte flip apiece. Never chain, and never free for anybody else.
    let outcome = ctx.consensus.validate_and_insert_block(forger.to_immutable()).virtual_state_task.await;
    let err = outcome.expect_err("a forged signature must not even enter the DAG");
    assert!(format!("{err}").contains("signature"), "and the refusal must name the signature rather than a digest mismatch: {err}");
    assert_eq!(ctx.consensus.get_sink(), sink_before, "the sink is untouched");
    assert_ne!(forged_hash, sink_before);
}

/// BASE-0's own reachable set, so the fixture cannot certify itself (see
/// `base0_reaches_only_kernels_this_build_adjudicates`).
fn palw_v2_test_catalog() -> kaspa_consensus_core::palw_mode_v2::PalwClassCatalogV2 {
    use kaspa_consensus_core::palw_mode_v2::{PalwClassCatalogEntryV2, PalwClassCatalogV2};
    PalwClassCatalogV2::new(vec![PalwClassCatalogEntryV2 {
        class_id: kaspa_hashes::Hash64::from_u64_word(1),
        artifact_root: kaspa_hashes::Hash64::from_u64_word(0xA7),
        max_step_leaf_count: 1 << 16,
        canonical_step_leaf_count: 4_096,
        // The derived close price the boot gate compares against the ruleset's ceilings. A nominal
        // one here: this fixture exists to exercise the acceptance path, not the cost bound, and a
        // catalog whose entry claimed a real class's cost would be asserting something this
        // fixture's `class_id` does not stand behind.
        court_cost: kaspa_consensus_core::palw_class_admission_v2::PalwCourtCostV1 {
            max_close_bytes: 1,
            max_terminal_macs: 1,
            max_operand_count: 1,
        },
        reachable_kernels: kaspa_consensus_core::palw_step_refute::KDESC_BASE0_ALL
            .iter()
            .map(|d| kaspa_consensus_core::palw_step::kernel_semantics_id_v1(d))
            .collect(),
    }])
    .expect("a well-formed catalog")
}

fn palw_v2_test_bundle(
    catalog: &kaspa_consensus_core::palw_mode_v2::PalwClassCatalogV2,
) -> kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2 {
    // The bond and the class the harness's own attempt carriage names
    // (`TestConsensus::palw_v2_test_carriage`). They must agree: admission refuses an attempt
    // whose bond the chain does not have, and the genesis registration list is the only place a
    // V2 network gets one.
    let mut b = kaspa_consensus_core::palw_fp_devnet_v3::palw_fp_devnet_bundle_v3(
        kaspa_hashes::Hash64::from_u64_word(1),
        catalog.root(),
        kaspa_hashes::Hash64::from_u64_word(0xC0757),
        4_096,
        kaspa_hashes::Hash64::from_u64_word(0xA7),
        {
            // Row 0 is the harness's own bond, carrying its REAL verification key: admission item 2
            // compares the attempt's carried key against this registration's, and the signature
            // check behind it is only meaningful if the key it authorises is the one the chain
            // registered. The rest are the panel's — a registry that cannot seat one is refused by
            // `verify_palw_genesis_v2`, and with one bond no claim could ever be licensed.
            let mut registry = kaspa_consensus_core::palw_fp_devnet_v3::palw_devnet_bond_registry_v1(
                kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1(),
            );
            registry[0].pubkey = crate::consensus::test_consensus::TestConsensus::palw_v2_harness_pubkey();
            registry[0].operator_pubkey = vec![21u8; 8];
            // Every OTHER row gets a real ML-DSA-87 identity too. They carried four-byte
            // placeholders, so no fixture could sign a panel receipt — which is part of why the
            // missing `ReceiptLicensed` edge went unnoticed for so long.
            for (i, row) in registry.iter_mut().enumerate().skip(1) {
                row.pubkey = crate::consensus::test_consensus::TestConsensus::palw_v2_registry_pubkey(i as u64);
            }
            registry
        },
    )
    .expect("the devnet bundle validates");
    b.class_catalog_root = catalog.root();
    b
}

/// The same fixture with a bond sized for a LONGER chain.
///
/// The bundle's own `MIN_COLLATERAL_SOMPI` funds four concurrent claims, which is the ruleset's
/// deliberate rate limit and not a number to edit. It only became visible once the pipeline
/// started folding each block's attempt into the state: until then no claim was ever created, so
/// nothing was ever reserved, so the per-bond exposure ceiling could not be reached however long
/// a fixture mined. The first test to mine five chain blocks found it immediately.
///
/// A test that wants a chain longer than four blocks therefore has to fund it, exactly as an
/// operator would. `palw_v2_the_exposure_ceiling_bites_when_reservations_are_real` keeps the
/// default and asserts the refusal, so raising it here hides nothing.
/// The fixture with every bond at the POLICY FLOOR rather than the derived bind-window figure.
///
/// `palw_fp_devnet_bundle_v3` now sizes a genesis bond to carry a claim through `window_bind`,
/// because a bond that cannot is a chain that stops — but a test that wants to WATCH the exposure
/// ceiling refuse a block needs a deliberately thin one. The floor is a legal declaration (it is
/// `PalwBondParamsV2`'s own minimum), so this is a thin network rather than an invalid one.
fn palw_v2_test_bundle_at_min_collateral(
    catalog: &kaspa_consensus_core::palw_mode_v2::PalwClassCatalogV2,
) -> kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2 {
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
    let mut b = palw_v2_test_bundle(catalog);
    let floor = b.bond.min_collateral_sompi();
    for object in b.genesis_objects.iter_mut() {
        if let PalwConsensusObjectV2::BondRegistered { collateral, .. } = object {
            *collateral = floor;
        }
    }
    b
}

/// How many concurrent claims this fixture's bond can hold before the exposure ceiling refuses
/// one. Derived, because the answer is a function of the pricing and the two tests that asserted a
/// literal "four" both broke the moment the pricing was corrected — which is the fixture asserting
/// an arithmetic accident rather than the rule it is named for.
fn palw_v2_claims_that_fit(bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2) -> u64 {
    use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
    let reserve = bundle
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered {
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
                slash_value_per_pwu,
                ..
            } => Some(*pwu_per_inference as u128 * *slash_value_per_pwu as u128),
            _ => None,
        })
        .expect("the fixture bundle registers its class with a derived pwu rule");
    let collateral = bundle
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::BondRegistered { collateral, .. } => Some(*collateral as u128),
            _ => None,
        })
        .expect("the fixture bundle registers a bond");
    let ceiling = collateral * bundle.admission.max_exposure_ratio_permille() as u128 / 1000;
    (ceiling / reserve) as u64
}

fn palw_v2_test_bundle_funded_for(
    catalog: &kaspa_consensus_core::palw_mode_v2::PalwClassCatalogV2,
    concurrent_claims: u64,
) -> kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2 {
    use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
    let mut b = palw_v2_test_bundle(catalog);
    // Read off the bundle's OWN class registration rather than typed, so a ruleset change moves
    // the fixture with it instead of silently under-funding it. A claim reserves
    // `pwu_per_inference × slash_value_per_pwu` — one inference's worth, NOT the derived `pwu`,
    // which carries an `expected_attempts(target)` factor the ceiling must not depend on
    // (`palw_state_v2::palw_exposure_pwu_v1`).
    let reserve_per_claim = b
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered {
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
                slash_value_per_pwu,
                ..
            } => Some(pwu_per_inference * slash_value_per_pwu),
            _ => None,
        })
        .expect("the fixture bundle registers its class with a derived pwu rule");
    // The ceiling is `collateral × 500‰`, so N concurrent claims need `2 × N × reserve` — but never
    // less than the bundle's own bond floor, which `CollateralBelowMinimum` refuses at genesis. The
    // clamp is not cosmetic: correcting the exposure pricing halved this figure and put the funded
    // fixtures under the floor, so a helper named "funded for N" was producing registries the chain
    // will not accept at all.
    let floor = b.bond.min_collateral_sompi();
    for object in b.genesis_objects.iter_mut() {
        if let PalwConsensusObjectV2::BondRegistered { collateral, .. } = object {
            *collateral = (2 * concurrent_claims * reserve_per_claim).max(floor);
        }
    }
    b
}

#[tokio::test]
async fn antichain_merge_test() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Build a large 32-wide antichain
    ctx.build_block_template_row(0..32)
        .validate_and_insert_row()
        .await
        .assert_tips()
        .assert_virtual_parents_subset()
        .assert_valid_utxo_tip();

    // Mine a long enough chain s.t. the antichain is fully merged
    for _ in 0..32 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    ctx.assert_tips_num(1);
}

/// kaspa-pq Phase 10/11 (ADR-0009/0013): first overlay-ACTIVE integration
/// test. With `dns_params = Some` and `dns_activation_daa_score = 0`, the
/// validator-reward code paths that are dormant on every shipping network —
/// the per-block `ActiveBondView` walk, the §B.4 eligibility check, the
/// coinbase reward fan-out (construction + validation), the cross-block
/// uniqueness walk over the rewarded-keys store, and the template
/// ineligible-shard pre-filter — all RUN here (with empty data, since this
/// chain carries no bonds or attestations). The chain must still mine and
/// validate to a valid UTXO tip, proving that activating the overlay does not
/// break block production or validation and that the empty-reward coinbase is
/// reproduced byte-for-byte by the validation path.
///
/// (A full reward-bearing e2e — a real bond tx, an ML-DSA-signed attestation,
/// and a non-empty reward coinbase — needs funded UTXO-valid overlay txs and
/// is a separate harness effort; the reward/eligibility/uniqueness logic is
/// already unit-tested.)
#[tokio::test]
async fn dns_overlay_active_chain_validates() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            // Activate the DNS overlay from genesis (reuse the self-consistent
            // devnet DNS parameters, with activation pulled down to 0).
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            p.dns_params = Some(dns);
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Mine + validate a chain with the overlay active end-to-end.
    for _ in 0..10 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    ctx.assert_tips_num(1);
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 2): overlay **and** v2-economics ACTIVE integration
/// test. With `dns_activation_daa_score = 0` AND `pos_v2_activation_daa_score = 0` (plus shrunk
/// windows so epochs actually bury within a short chain), the full v2 machinery RUNS on every
/// block: the fence-gated 70/30 participation/quality split, the per-block quality-pool
/// persistence (`block_quality_pool_store`, written non-empty here since the §F carve funds a
/// validator pool even with no attestations), the per-epoch accumulator recompute + finalization
/// (`update_epoch_accumulator`), and the deferred quality-bonus payout
/// (`deferred_quality_bonus_outputs` — incl. the finalization-crossing detection and the φS gate).
///
/// This chain carries no bonds/attestations, so every *reward* set is empty — but the code paths
/// execute, write the stores, and the chain must still mine and validate to a valid UTXO tip.
/// Because the validation path rebuilds the coinbase and rejects any mismatch, reaching a valid
/// UTXO tip proves the v2 economics neither break block production nor desynchronise coinbase
/// construction vs validation. (A reward-bearing e2e — real bonds + attestations paid a non-empty
/// bonus — needs the funded-bond DAG harness, DAG-2.)
#[tokio::test]
async fn pos_v2_active_empty_chain_validates() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            // Activate the v2 economics from genesis and shrink the finalization window
            // (= reward_uniqueness_window + max_reorg_horizon = 4) so epochs bury and the
            // deferred-bonus crossing fires within a short chain.
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 2;
            dns.max_reorg_horizon_blocks = 2;
            p.dns_params = Some(dns);
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // threshold(E) = (E+1)·2 + 4, so by ~daa 12 several epochs have finalized and the deferred
    // quality-bonus path has fired (with empty included sets — exercised, not paid).
    for _ in 0..12 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    ctx.assert_tips_num(1);
}

/// kaspa-pq ADR-0018 §G DAG-2 (funded-bond milestone — retires the "fund a bond
/// from a coinbase UTXO" wall). With the overlay + v2 economics ACTIVE, a real
/// ML-DSA-87 keypair mines a coinbase; after maturity its output is SPENT into a
/// funded stake-bond tx (output-0 = locked stake, input-0 signed over the v2 tx
/// sighash under `MLDSA87_TX_CONTEXT`). The block carrying the bond must reach a
/// valid UTXO tip — proving the script engine (`OpCheckSigMlDsa87`) accepts the
/// real ML-DSA-87 P2PKH spend through full consensus validation, the precondition
/// for the reward-bearing / slashing DAG e2e (DAG-2..6).
#[tokio::test]
async fn pos_v2_funded_bond_chain_validates() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            // Shrink coinbase maturity so the funding coinbase is spendable within a short chain.
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 2;
            dns.max_reorg_horizon_blocks = 2;
            p.dns_params = Some(dns);
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // A known validator/funding key; its coinbase P2PKH spk.
    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // 1) Mine a run of blocks whose coinbase pays K. In Kaspa a block's coinbase
    //    rewards the blocks it MERGES (each merged block's reported miner script),
    //    not its own miner — so K's reward for the funding block b1 (which merges
    //    only genesis → 0 reward) appears in the coinbase of the block that merges
    //    b1 (the harvest block b2).
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let harvest = ctx.mine_block(k_miner.clone(), vec![]).await;
    let coinbase = &harvest.transactions[0];
    let coinbase_id = coinbase.id();
    let coinbase_daa = harvest.header.daa_score;
    let (idx, out) = coinbase
        .outputs
        .iter()
        .enumerate()
        .find(|(_, o)| o.script_public_key == k_spk)
        .expect("the harvest coinbase must pay the known key");
    let coinbase_outpoint = TransactionOutpoint::new(coinbase_id, idx as u32);
    let coinbase_value = out.value;
    assert!(coinbase_value > 200_000, "coinbase value must cover the bond + fee");

    // 2) Mine filler blocks so the harvested coinbase matures (coinbase_maturity = 2).
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    // 3) Spend the matured coinbase into a funded, ML-DSA-87-signed stake-bond tx.
    let amount = coinbase_value - 100_000; // small fee; bond almost the whole coinbase
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _validator_id, _reward_payload) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_outpoint, coinbase_value, coinbase_daa, amount, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();

    // 4) Mine the block carrying the bond tx; it must reach a valid UTXO tip.
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert!(bond_block.transactions.iter().any(|t| t.id() == bond_tx_id), "the funded stake-bond tx must be included in the block");
    assert_eq!(
        ctx.consensus.block_status(bond_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the block carrying the funded ML-DSA-87 stake-bond spend must be UTXO-valid (construction == validation)"
    );
    ctx.assert_valid_utxo_tip();
}

/// ADR-0018 §F bridge wiring — one scenario of the finality-fee e2e: fund a key,
/// spend its matured coinbase into a deposit-lock tx (fee 100_000), mine it, then
/// harvest the next block's coinbase. Returns `(worker_output_value,
/// lock_block_subsidy)` — the worker payout for the block that carried the bridge tx,
/// and that block's subsidy (parsed from its coinbase payload: blue_score u64 LE ‖
/// subsidy u64 LE ‖ …).
///
/// `evm_active` toggles `evm_activation_daa_score` (0 vs u64::MAX). evm-active
/// templates COMMIT to the header timestamp (the EVM execution env derives from it),
/// so in that mode blocks are inserted exactly as templated — no
/// `TestContext::mine_block` timestamp/nonce mutation (the same insertion pattern the
/// EVM lane e2e tests use); the inert mode exercises the ordinary v1 mine path.
async fn finality_fee_bridge_scenario(finality_fence: u64, evm_active: bool) -> (u64, u64) {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            // Shrink coinbase maturity so the funding coinbase is spendable quickly.
            p.coinbase_maturity = 2;
            // The classification is doubly gated: the §F fence AND EVM-lane activation
            // (the bridge only exists on an EVM-active net). MAINNET_PARAMS is EVM-inert
            // (u64::MAX) by default.
            p.evm_activation_daa_score = if evm_active { 0 } else { u64::MAX };
            let mut dns = p.dns_params.clone().unwrap();
            dns.finality_fee_activation_daa_score = finality_fence;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Insert one block: template-as-is when evm-active (timestamp is commitment-bound),
    // else the ordinary simulated-time mine path.
    async fn mine(ctx: &mut TestContext, evm_active: bool, miner: MinerData, txs: Vec<Transaction>) -> Block {
        if evm_active {
            let t =
                ctx.consensus.build_block_template(miner, Box::new(OnetimeTxSelector::new(txs)), TemplateBuildMode::Standard).unwrap();
            let block = t.block.to_immutable();
            ctx.validate_and_insert_block(block.clone()).await;
            block
        } else {
            ctx.mine_block(miner, txs).await
        }
    }

    // Fund: harvest a coinbase paying the known key K (a block's coinbase rewards
    // the blocks it MERGES, so K's reward for b1 appears in the harvest block).
    let seed = [0x5Au8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = mine(&mut ctx, evm_active, k_miner.clone(), vec![]).await;
    let harvest = mine(&mut ctx, evm_active, k_miner.clone(), vec![]).await;
    let coinbase = &harvest.transactions[0];
    let (idx, out) = coinbase
        .outputs
        .iter()
        .enumerate()
        .find(|(_, o)| o.script_public_key == k_spk)
        .expect("the harvest coinbase must pay the known key");
    let coinbase_outpoint = TransactionOutpoint::new(coinbase.id(), idx as u32);
    let (coinbase_value, coinbase_daa) = (out.value, harvest.header.daa_score);
    assert!(coinbase_value > 200_000, "coinbase must cover the lock + fee");

    // Mature the coinbase (coinbase_maturity = 2).
    for _ in 0..5 {
        mine(&mut ctx, evm_active, new_miner_data(), vec![]).await;
    }

    // The bridge tx: matured coinbase → one EVM_DEPOSIT_LOCK output, fee 100_000.
    let lock_tx = dns_harness::funded_signed_deposit_lock_tx(
        seed,
        coinbase_outpoint,
        coinbase_value,
        coinbase_daa,
        ctx.consensus.params().storage_mass_parameter,
    );
    let lock_tx_id = lock_tx.id();

    // Mine it under a distinct miner spk so its worker payout is findable.
    let lock_miner_spk = p2pkh_mldsa87_spk(&[0x33u8; 64]);
    let lock_block = mine(&mut ctx, evm_active, MinerData::new(lock_miner_spk.clone(), vec![]), vec![lock_tx]).await;
    assert!(lock_block.transactions.iter().any(|t| t.id() == lock_tx_id), "the bridge tx must be included");
    assert_eq!(
        ctx.consensus.block_status(lock_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the block carrying the deposit-lock tx must be UTXO-valid (construction == validation)"
    );
    // The lock block's subsidy, from its coinbase payload (blue_score ‖ subsidy ‖ …).
    let payload = &lock_block.transactions[0].payload;
    let subsidy = u64::from_le_bytes(payload[8..16].try_into().unwrap());

    // Harvest: the next block's coinbase pays the lock block's worker share.
    let harvest2 = mine(&mut ctx, evm_active, new_miner_data(), vec![]).await;
    ctx.assert_valid_utxo_tip();
    let worker_out = harvest2.transactions[0]
        .outputs
        .iter()
        .find(|o| o.script_public_key == lock_miner_spk)
        .expect("the next coinbase must pay the lock block's miner")
        .value;
    (worker_out, subsidy)
}

/// kaspa-pq ADR-0018 §F bridge wiring e2e (EVM-active net): an accepted L1 tx that
/// CREATES an `EVM_DEPOSIT_LOCK` output (ADR-0020 §9.2 bridge deposit) is
/// **finality-class** — its fee is split at the validator-primary finality ratios
/// (Worker 25%) instead of the normal-tx ratios (Worker 90%) — through the REAL
/// template→validate coinbase path over a chain (classification at
/// `calculate_utxo_state`, payout via `expected_coinbase_transaction`; every mined
/// block reaching `StatusUTXOValid` proves construction == validation). The fenced
/// twin (`finality_fee_activation_daa_score = u64::MAX`) runs the identical chain
/// shape and pays the Worker the normal 90% — the exact pre-wiring math — proving the
/// §F fence isolates the change. evm-feature-gated: an evm-active template requires
/// the executor (a non-evm build refuses evm-active blocks by design).
#[tokio::test]
#[cfg(feature = "evm")]
async fn finality_fee_bridge_tx_pays_validator_primary_split() {
    use kaspa_consensus_core::dns_finality::{split_block_subsidy, split_finality_fees, split_normal_tx_fees};
    kaspa_core::log::try_init_logger("info");

    // Active fence (0, the PRODUCTION preset value): the 100_000 bridge fee splits at
    // the finality ratios — the Worker gets 25%, the Validator share (75%) funds the
    // §E pool (don't-mint burned here: no bonded validators).
    let (worker_active, subsidy_a) = finality_fee_bridge_scenario(0, true).await;
    // Inert §F fence: the same chain shape pays the normal-tx 90% — the pre-wiring math.
    let (worker_inert, subsidy_b) = finality_fee_bridge_scenario(u64::MAX, true).await;
    assert_eq!(subsidy_a, subsidy_b, "identical chain shape ⇒ identical lock-block subsidy");

    let dns = MAINNET_PARAMS.dns_params.clone().unwrap();
    let fs = &dns.reward_params.fee_split.clone();
    let worker_base = split_block_subsidy(subsidy_a, fs).worker_base_sompi;
    assert_eq!(
        worker_active,
        worker_base + split_finality_fees(100_000, fs).worker_sompi,
        "bridge-tx fee pays the Worker the FINALITY share (25%)"
    );
    assert_eq!(
        worker_inert,
        worker_base + split_normal_tx_fees(100_000, fs).worker_sompi,
        "below the §F fence the same fee pays the Worker the NORMAL share (90%) — byte-identical to pre-wiring"
    );
    assert_eq!(
        worker_inert - worker_active,
        split_normal_tx_fees(100_000, fs).worker_sompi - split_finality_fees(100_000, fs).worker_sompi,
        "the Worker delta is exactly the normal→finality reclassification (the Validator gains it)"
    );
}

/// kaspa-pq ADR-0018 §F bridge wiring — the EVM-activation gate: deposit-lock OUTPUTS
/// are consensus-legal on every net (the output-class exemption is unconditional),
/// but on an EVM-INERT net (`evm_activation_daa_score = u64::MAX` — mainnet today)
/// the classification must NOT fire even with the §F fence at 0: the lock-bearing
/// tx's fee stays normal-class (Worker 90%), byte-identical to the pre-wiring math.
/// Without this gate a miner on an EVM-inert net could self-include a never-claimable
/// lock tx and reroute fees into the §E pool. Runs on the default (non-evm) build —
/// inert nets produce ordinary v1 blocks.
#[tokio::test]
async fn finality_fee_inert_on_evm_inert_net() {
    use kaspa_consensus_core::dns_finality::{split_block_subsidy, split_normal_tx_fees};
    kaspa_core::log::try_init_logger("info");

    // §F fence ACTIVE (0, the production value) but the EVM lane INERT.
    let (worker_out, subsidy) = finality_fee_bridge_scenario(0, false).await;
    let dns = MAINNET_PARAMS.dns_params.clone().unwrap();
    let fs = &dns.reward_params.fee_split.clone();
    assert_eq!(
        worker_out,
        split_block_subsidy(subsidy, fs).worker_base_sompi + split_normal_tx_fees(100_000, fs).worker_sompi,
        "on an EVM-inert net a lock-bearing tx's fee stays NORMAL-class (Worker 90%) — the EVM gate holds"
    );
}

/// kaspa-pq ADR-0018 §G DAG-2 (reward-bearing e2e): the full overlay + v2 reward
/// path over a real BlockDAG. A funded ML-DSA-87 bond is created (as in the funding
/// milestone), then the validator ML-DSA-signs a recent attestation; the block that
/// includes the attestation shard must pay the validator a non-empty §E
/// participation reward in its coinbase AND validate to a UTXO-valid tip — proving
/// the reward fan-out (eligibility → distribution → coinbase) is
/// construction == validation with real bonds + attestations, not just by unit test.
#[tokio::test]
async fn pos_v2_reward_bearing_attestation_validates() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::{Hash64, dns_finality::ready_epoch_from_tip_blue_score};
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            // Reward recency must comfortably cover the canonical anchor, which is buried by
            // attestation_lag + backoff below the tip (blue_score ~ DAA on this linear chain).
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            // DNS v3 blue_score epochs: small so an epoch buries within this short chain.
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // A known validator/funding key.
    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // Fund: b1 funds, then two harvest blocks — h_a pays K for b1, h_b pays K for h_a.
    // coinbase_a funds the bond; coinbase_b funds the attestation shard tx (a 0-input
    // shard tx is rejected by the isolation `NoTxInputs` check, so production funds it).
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _vid, reward_payload) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, value_a - 100_000, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid, "the bond block must be UTXO-valid");
    assert_eq!(reward_payload, k_payload, "rewards pay back to K");

    // Bury several blue_score epochs past the bond so a ready, bond-active canonical anchor
    // exists — DNS v3 pays the §E reward only to an attestation naming the canonical anchor.
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor for the ready epoch")
    };

    // The validator ML-DSA-signs the CANONICAL anchor for a ready epoch (DNS v3). net_id =
    // genesis hash (Addendum A.3); VSC is a domain-separation field only (P-1D zero).
    let att = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        anchor.epoch,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_tx = dns_harness::funded_signed_shard_tx(seed, coinbase_b, value_b, daa_b, att, storage_mass_parameter);

    // The block that includes the attestation shard pays the validator the §E
    // participation reward (to owner_reward_spk_payload == k_spk) and must validate.
    let reward_block = ctx.mine_block(new_miner_data(), vec![shard_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(reward_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the attestation-including block must be UTXO-valid with a non-empty reward coinbase"
    );
    let reward_value = reward_block.transactions[0].outputs.iter().find(|o| o.script_public_key == k_spk).map(|o| o.value);
    assert!(
        reward_value.unwrap_or(0) > 0,
        "the coinbase must pay the validator a non-empty §E participation reward (got {reward_value:?})"
    );
}

/// kaspa-pq ADR-0018 §G (DAG-6): full-consensus equivocation-slashing e2e. A funded,
/// ML-DSA-87-signed bond goes active; the validator then EQUIVOCATES — two signed
/// attestations for the same `(bond, validator, epoch)` but DIFFERENT anchors — and a
/// `SlashingEvidence` tx carries both. The block including it must validate
/// (construction == validation), and as a consensus side-effect must REMOVE the locked
/// stake UTXO (the bond's output-0 leaves the supply) and MINT the reporter reward
/// (`slashing_reporter_reward_bps` = 10%) at `(slashing_tx, 0)`. This proves the slashing
/// economics end-to-end through `mine_block`/validate-and-insert, not just at the
/// `UtxoDiff` unit level (closes the audit's DAG-6 test gap).
#[tokio::test]
async fn pos_v2_slashing_evidence_removes_bond_and_pays_reporter() {
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload},
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // One validator/funding/reporter key suffices to exercise the slashing mechanism.
    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // Fund: h_a pays K (funds the bond), h_b pays K (funds the evidence tx).
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    // Fund + mine the bond — active from activation_daa_score = 0.
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let bond_amount = value_a - 100_000;
    let (bond_tx, _vid, _reward_payload) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, bond_amount, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid, "the bond block must be UTXO-valid");
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    assert!(
        ctx.consensus.get_virtual_utxos(None, 100_000, false).iter().any(|(o, _)| *o == bond_outpoint),
        "the bond's locked-stake UTXO must exist before slashing"
    );
    // Bury the bond so its record is committed into the active bond view the slashing
    // verifier reads (mirrors the burial the reward-bearing e2e does before attesting).
    let mut buried = Vec::new();
    for _ in 0..5 {
        buried.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }

    // Equivocation: two ML-DSA-87-signed attestations, same (bond, validator, epoch),
    // DIFFERENT target_hash (approving two conflicting anchors) — the punishable act.
    let net_id = ctx.consensus.params().genesis.hash;
    let epoch = 1u64;
    // A buried block's DAA: past the bond's (inclusion-set) activation so the bond is
    // Active at the target, and well within `evidence_window_blocks` of the including block.
    let target_daa = buried[1].header.daa_score;
    let att_a = dns_harness::build_signed_attestation(
        &v,
        net_id.as_byte_slice(),
        bond_outpoint,
        epoch,
        Hash64::from_bytes([0xa1u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let att_b = dns_harness::build_signed_attestation(
        &v,
        net_id.as_byte_slice(),
        bond_outpoint,
        epoch,
        Hash64::from_bytes([0xb2u8; 64]),
        target_daa,
        Hash64::default(),
    );
    // Sanity (localizes a signature/net_id mismatch vs a freshness/status rejection): both
    // attestations must verify under the net_id (genesis hash) the consensus slashing
    // verifier reconstructs the digest with.
    for att in [&att_a, &att_b] {
        let msg = kaspa_consensus_core::dns_finality::stake_attestation_message(
            net_id.as_byte_slice(),
            att.epoch,
            att.target_hash,
            att.target_daa_score,
            att.validator_set_commitment,
            att.bond_outpoint,
        );
        assert!(
            kaspa_txscript::verify_mldsa87_with_context(
                &v.pubkey,
                &msg.as_bytes()[..],
                &att.signature,
                kaspa_consensus_core::dns_finality::ATTESTATION_MLDSA87_CONTEXT
            )
            .unwrap(),
            "attestation must self-verify under the consensus net_id"
        );
    }
    let evidence = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint,
        attestation_a: att_a,
        attestation_b: att_b,
        reporter_reward_spk_payload: k_payload,
    };
    let slash_tx = dns_harness::funded_signed_slashing_evidence_tx(seed, coinbase_b, value_b, daa_b, evidence, storage_mass_parameter);
    let slash_tx_id = slash_tx.id();

    // The block including the slashing evidence must validate AND apply the side-effects.
    let slash_block = ctx.mine_block(new_miner_data(), vec![slash_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(slash_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the slashing-evidence block must be UTXO-valid (construction == validation of the slashing side-effects)"
    );

    // Consensus side-effects: the locked stake is REMOVED and the reporter reward is minted.
    let utxos: std::collections::HashMap<_, _> = ctx.consensus.get_virtual_utxos(None, 100_000, false).into_iter().collect();
    assert!(!utxos.contains_key(&bond_outpoint), "the slashed bond's locked stake must be removed from the UTXO set");
    let reporter_mint = TransactionOutpoint::new(slash_tx_id, 0);
    let reporter_bps = ctx.consensus.params().dns_params.clone().unwrap().reward_params.slashing_reporter_reward_bps as u128;
    let expected_reporter = (bond_amount as u128 * reporter_bps / 10_000) as u64;
    let r = utxos.get(&reporter_mint).expect("the reporter reward must be minted at (slashing_tx, 0)");
    assert_eq!(r.amount, expected_reporter, "reporter reward = bond_amount * reporter_bps / 10000");
    assert_eq!(r.script_public_key, k_spk, "the reporter reward pays the declared reporter P2PKH");

    // ── Supply invariant (audit M-01) ──────────────────────────────────────────────────────
    // The 4-way slashing split is value-conserving: reporter + reserve + victim + burn equals the
    // slashed amount EXACTLY (no coins are created or destroyed by slashing), and only the reporter
    // is re-minted into the UTXO set. With a single (self-)validator there is no honest epoch peer,
    // so no victim-compensation output is emitted at (slash_tx, 2); the reserve share is pool-accrued
    // (not a UTXO) and the victim/burn shares leave the supply with the removed locked stake. Hence
    // minted (reporter) ≤ slashed ⇒ slashing cannot inflate supply.
    let rp = ctx.consensus.params().dns_params.clone().unwrap().reward_params;
    let dist = kaspa_consensus_core::dns_finality::compute_slashing_distribution(
        bond_amount,
        rp.slashing_reporter_reward_bps,
        rp.security_reserve_bps,
        rp.victim_epoch_pool_bps,
    );
    assert_eq!(
        dist.reporter_reward_sompi + dist.security_reserve_sompi + dist.victim_epoch_pool_sompi + dist.burned_sompi,
        bond_amount,
        "slashing split conserves value: reporter + reserve + victim + burn == slashed amount"
    );
    assert_eq!(dist.reporter_reward_sompi, expected_reporter, "minted reporter reward == the split's reporter share");
    assert!(dist.reporter_reward_sompi <= bond_amount, "the minted reporter reward never exceeds the slashed amount (no inflation)");
    assert!(
        !utxos.contains_key(&TransactionOutpoint::new(slash_tx_id, 2)),
        "no victim-compensation output is minted with a single (self-)validator"
    );
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2) — victim compensation (audit M-01). TWO validators: A
/// (equivocator) and B (honest). B attests the canonical anchor for a ready epoch E and is rewarded
/// (so it joins epoch E's accumulator `included` set); A then EQUIVOCATES the SAME epoch E (two
/// conflicting attestations) and is slashed. The slashing routes the victim-epoch share of A's
/// slashed stake to epoch E's honest peers = {B} (A is dropped by its `owner_reward_spk_payload`),
/// minting a victim-compensation output to B's reward P2PKH at `(slash_tx, 2)`. Proves the
/// multi-validator victim-compensation economics end-to-end through `mine_block`.
#[tokio::test]
async fn pos_v2_slashing_victim_compensates_honest_peer() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{
            DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload, compute_slashing_distribution, ready_epoch_from_tip_blue_score,
        },
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            // Ensure a non-zero victim pool (the 4-way split): reporter 10% / reserve 40% /
            // victim 40% / burn 10%.
            dns.reward_params.slashing_reporter_reward_bps = 1000;
            dns.reward_params.security_reserve_bps = 4000;
            dns.reward_params.victim_epoch_pool_bps = 4000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let (va, vb) = (dns_harness::harness_validator([0x42u8; 32]), dns_harness::harness_validator([0x43u8; 32]));
    let payload_a: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&va.pubkey).as_bytes();
    let payload_b: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&vb.pubkey).as_bytes();
    let (spk_a, spk_b) = (p2pkh_mldsa87_spk(&payload_a), p2pkh_mldsa87_spk(&payload_b));

    // Fund: A needs two coinbases (bond + slashing-evidence tx), B two (bond + attestation shard).
    // A block's coinbase pays its MERGESET (the previous block's miner), not its own, so mine a
    // batch per miner and SCAN all coinbases for ones paying each validator.
    let (miner_a, miner_b) = (MinerData::new(spk_a.clone(), vec![]), MinerData::new(spk_b.clone(), vec![]));
    let mut blocks = Vec::new();
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_a.clone(), vec![]).await);
    }
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_b.clone(), vec![]).await);
    }
    for _ in 0..5 {
        blocks.push(ctx.mine_block(new_miner_data(), vec![]).await); // mature the coinbases
    }
    let (mut a_funds, mut b_funds) = (Vec::new(), Vec::new());
    for blk in &blocks {
        let cb = &blk.transactions[0];
        for (i, o) in cb.outputs.iter().enumerate() {
            let f = (TransactionOutpoint::new(cb.id(), i as u32), o.value, blk.header.daa_score);
            if o.script_public_key == spk_a {
                a_funds.push(f);
            } else if o.script_public_key == spk_b {
                b_funds.push(f);
            }
        }
    }
    assert!(a_funds.len() >= 2 && b_funds.len() >= 2, "need ≥2 funding coinbases each (a={}, b={})", a_funds.len(), b_funds.len());
    let ((cb_a1, va1, da1), (cb_a2, va2, da2)) = (a_funds[0], a_funds[1]);
    let ((cb_b1, vb1, db1), (cb_b2, vb2, db2)) = (b_funds[0], b_funds[1]);

    // Bond A and B (active from activation_daa_score = 0).
    let storage = ctx.consensus.params().storage_mass_parameter;
    let bond_a_amount = va1 - 100_000;
    let (bond_a_tx, _, _) = dns_harness::funded_signed_bond_tx([0x42u8; 32], cb_a1, va1, da1, bond_a_amount, 0, storage);
    let (bond_b_tx, _, _) = dns_harness::funded_signed_bond_tx([0x43u8; 32], cb_b1, vb1, db1, vb1 - 100_000, 0, storage);
    let (bond_a_id, bond_b_id) = (bond_a_tx.id(), bond_b_tx.id());
    ctx.mine_block(new_miner_data(), vec![bond_a_tx]).await;
    ctx.mine_block(new_miner_data(), vec![bond_b_tx]).await;
    let (bond_a_outpoint, bond_b_outpoint) = (TransactionOutpoint::new(bond_a_id, 0), TransactionOutpoint::new(bond_b_id, 0));

    // Bury so a ready, bond-active canonical anchor exists for a real epoch E.
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor for the ready epoch")
    };
    let epoch_e = anchor.epoch;

    // B HONESTLY attests the canonical anchor for epoch E → B is rewarded ⇒ joins epoch E's
    // accumulator `included` set (keyed by the attestation epoch).
    let att_b = dns_harness::build_signed_attestation(
        &vb,
        genesis_hash.as_byte_slice(),
        bond_b_outpoint,
        epoch_e,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_b = dns_harness::funded_signed_shard_tx([0x43u8; 32], cb_b2, vb2, db2, att_b, storage);
    let reward_block = ctx.mine_block(new_miner_data(), vec![shard_b]).await;
    assert!(
        reward_block.transactions[0].outputs.iter().any(|o| o.script_public_key == spk_b),
        "B must be rewarded for attesting epoch E (so it joins the epoch's included set)"
    );

    // Bury so A's bond is committed into the active bond view the slashing verifier reads.
    let mut buried = Vec::new();
    for _ in 0..5 {
        buried.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }

    // A EQUIVOCATES the SAME epoch E: two conflicting attestations (different anchors).
    let target_daa = buried[1].header.daa_score;
    let att_a1 = dns_harness::build_signed_attestation(
        &va,
        genesis_hash.as_byte_slice(),
        bond_a_outpoint,
        epoch_e,
        Hash64::from_bytes([0xa1u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let att_a2 = dns_harness::build_signed_attestation(
        &va,
        genesis_hash.as_byte_slice(),
        bond_a_outpoint,
        epoch_e,
        Hash64::from_bytes([0xb2u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let evidence = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: bond_a_outpoint,
        attestation_a: att_a1,
        attestation_b: att_a2,
        reporter_reward_spk_payload: payload_a, // reporter paid to A's address (payout is independent of who is slashed)
    };
    let slash_tx = dns_harness::funded_signed_slashing_evidence_tx([0x42u8; 32], cb_a2, va2, da2, evidence, storage);
    let slash_tx_id = slash_tx.id();
    let slash_block = ctx.mine_block(new_miner_data(), vec![slash_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(slash_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the slashing block must validate AND mint the victim-compensation outputs (construction == validation)"
    );

    let utxos: std::collections::HashMap<_, _> = ctx.consensus.get_virtual_utxos(None, 100_000, false).into_iter().collect();
    assert!(!utxos.contains_key(&bond_a_outpoint), "A's slashed locked stake is removed");
    let dist = compute_slashing_distribution(
        bond_a_amount,
        dns.reward_params.slashing_reporter_reward_bps,
        dns.reward_params.security_reserve_bps,
        dns.reward_params.victim_epoch_pool_bps,
    );
    let reporter = utxos.get(&TransactionOutpoint::new(slash_tx_id, 0)).expect("reporter reward minted at (slash_tx, 0)");
    assert_eq!(reporter.amount, dist.reporter_reward_sompi, "reporter = reporter_bps share");
    // VICTIM COMPENSATION: the single honest peer B receives the whole victim pool at (slash_tx, 2).
    let victim = utxos.get(&TransactionOutpoint::new(slash_tx_id, 2)).expect("victim-compensation output minted at (slash_tx, 2)");
    assert_eq!(victim.script_public_key, spk_b, "victim compensation pays the honest peer B");
    assert_eq!(victim.amount, dist.victim_epoch_pool_sompi, "the lone honest peer receives the entire victim pool");
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2) — multiple slashings in ONE block (audit M-01). TWO
/// independently-bonded validators A and B BOTH equivocate and are slashed by SEPARATE evidence
/// transactions carried in the SAME block. Proves the slashing pipeline applies N>1 side-effects
/// atomically and independently: both locked stakes are removed, each reporter reward is minted at
/// its own `(slash_tx, 0)`, each bond's 4-way split conserves value, and — the multi-slash-specific
/// invariant — the block's committed security-reserve accrual is the SUM of both bonds' reserve
/// shares (`apply_slashing_side_effects`'s fold over the resolved effects, persisted by the
/// `parent_balance + reserve_accrual − drip` recurrence). With no honest epoch peer, no
/// victim-compensation output is minted for either.
#[tokio::test]
async fn pos_v2_multi_slashing_in_one_block() {
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload, compute_slashing_distribution},
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            // 4-way split with non-zero reserve + victim shares (so the summed reserve accrual is observable).
            dns.reward_params.slashing_reporter_reward_bps = 1000;
            dns.reward_params.security_reserve_bps = 4000;
            dns.reward_params.victim_epoch_pool_bps = 4000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let (va, vb) = (dns_harness::harness_validator([0x42u8; 32]), dns_harness::harness_validator([0x43u8; 32]));
    let payload_a: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&va.pubkey).as_bytes();
    let payload_b: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&vb.pubkey).as_bytes();
    let (spk_a, spk_b) = (p2pkh_mldsa87_spk(&payload_a), p2pkh_mldsa87_spk(&payload_b));

    // Fund: each validator needs two coinbases (bond + slashing-evidence tx). A block's coinbase pays
    // its MERGESET (the previous block's) miner, so mine a batch per miner and SCAN all coinbases.
    let (miner_a, miner_b) = (MinerData::new(spk_a.clone(), vec![]), MinerData::new(spk_b.clone(), vec![]));
    let mut blocks = Vec::new();
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_a.clone(), vec![]).await);
    }
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_b.clone(), vec![]).await);
    }
    for _ in 0..5 {
        blocks.push(ctx.mine_block(new_miner_data(), vec![]).await); // mature the coinbases
    }
    let (mut a_funds, mut b_funds) = (Vec::new(), Vec::new());
    for blk in &blocks {
        let cb = &blk.transactions[0];
        for (i, o) in cb.outputs.iter().enumerate() {
            let f = (TransactionOutpoint::new(cb.id(), i as u32), o.value, blk.header.daa_score);
            if o.script_public_key == spk_a {
                a_funds.push(f);
            } else if o.script_public_key == spk_b {
                b_funds.push(f);
            }
        }
    }
    assert!(a_funds.len() >= 2 && b_funds.len() >= 2, "need ≥2 funding coinbases each (a={}, b={})", a_funds.len(), b_funds.len());
    let ((cb_a1, va1, da1), (cb_a2, va2, da2)) = (a_funds[0], a_funds[1]);
    let ((cb_b1, vb1, db1), (cb_b2, vb2, db2)) = (b_funds[0], b_funds[1]);

    // Bond A and B (active from activation_daa_score = 0).
    let storage = ctx.consensus.params().storage_mass_parameter;
    let (bond_a_amount, bond_b_amount) = (va1 - 100_000, vb1 - 100_000);
    let (bond_a_tx, _, _) = dns_harness::funded_signed_bond_tx([0x42u8; 32], cb_a1, va1, da1, bond_a_amount, 0, storage);
    let (bond_b_tx, _, _) = dns_harness::funded_signed_bond_tx([0x43u8; 32], cb_b1, vb1, db1, bond_b_amount, 0, storage);
    let (bond_a_id, bond_b_id) = (bond_a_tx.id(), bond_b_tx.id());
    ctx.mine_block(new_miner_data(), vec![bond_a_tx]).await;
    ctx.mine_block(new_miner_data(), vec![bond_b_tx]).await;
    let (bond_a_outpoint, bond_b_outpoint) = (TransactionOutpoint::new(bond_a_id, 0), TransactionOutpoint::new(bond_b_id, 0));

    // Bury so BOTH bonds are committed into the active bond view the slashing verifier reads.
    let mut buried = Vec::new();
    for _ in 0..6 {
        buried.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }

    // Both A and B equivocate: each emits two conflicting attestations for the same epoch (different
    // anchors). With no honest peer the epoch only has to be shared by each validator's own pair.
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let epoch = 1u64;
    let target_daa = buried[1].header.daa_score; // past each bond's activation, within evidence_window of the slash block
    let att_a1 = dns_harness::build_signed_attestation(
        &va,
        genesis_hash.as_byte_slice(),
        bond_a_outpoint,
        epoch,
        Hash64::from_bytes([0xa1u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let att_a2 = dns_harness::build_signed_attestation(
        &va,
        genesis_hash.as_byte_slice(),
        bond_a_outpoint,
        epoch,
        Hash64::from_bytes([0xb2u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let att_b1 = dns_harness::build_signed_attestation(
        &vb,
        genesis_hash.as_byte_slice(),
        bond_b_outpoint,
        epoch,
        Hash64::from_bytes([0xc3u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let att_b2 = dns_harness::build_signed_attestation(
        &vb,
        genesis_hash.as_byte_slice(),
        bond_b_outpoint,
        epoch,
        Hash64::from_bytes([0xd4u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let ev_a = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: bond_a_outpoint,
        attestation_a: att_a1,
        attestation_b: att_a2,
        reporter_reward_spk_payload: payload_a,
    };
    let ev_b = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: bond_b_outpoint,
        attestation_a: att_b1,
        attestation_b: att_b2,
        reporter_reward_spk_payload: payload_b,
    };
    let slash_a = dns_harness::funded_signed_slashing_evidence_tx([0x42u8; 32], cb_a2, va2, da2, ev_a, storage);
    let slash_b = dns_harness::funded_signed_slashing_evidence_tx([0x43u8; 32], cb_b2, vb2, db2, ev_b, storage);
    let (slash_a_id, slash_b_id) = (slash_a.id(), slash_b.id());

    // BOTH slashing-evidence txs in ONE block.
    let slash_block = ctx.mine_block(new_miner_data(), vec![slash_a, slash_b]).await;
    assert_eq!(
        ctx.consensus.block_status(slash_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the block carrying TWO slashing-evidence txs must validate (both side-effects apply atomically)"
    );
    // A block's own transactions (and thus their slashing side-effects + the reserve accrual they
    // commit) are applied to the persisted UTXO state only once the block becomes a SELECTED PARENT.
    // Mine one empty block on top so `slash_block`'s effects settle into a committed chain block
    // (`settle`), whose `reserve_balance_store` row we read below.
    let settle = ctx.mine_block(new_miner_data(), vec![]).await;

    let utxos: std::collections::HashMap<_, _> = ctx.consensus.get_virtual_utxos(None, 100_000, false).into_iter().collect();
    // Both locked stakes removed.
    assert!(!utxos.contains_key(&bond_a_outpoint), "A's slashed locked stake is removed");
    assert!(!utxos.contains_key(&bond_b_outpoint), "B's slashed locked stake is removed");

    let rp = ctx.consensus.params().dns_params.clone().unwrap().reward_params;
    let dist_a = compute_slashing_distribution(
        bond_a_amount,
        rp.slashing_reporter_reward_bps,
        rp.security_reserve_bps,
        rp.victim_epoch_pool_bps,
    );
    let dist_b = compute_slashing_distribution(
        bond_b_amount,
        rp.slashing_reporter_reward_bps,
        rp.security_reserve_bps,
        rp.victim_epoch_pool_bps,
    );
    // Each reporter reward minted independently at its own (slash_tx, 0).
    let ra = utxos.get(&TransactionOutpoint::new(slash_a_id, 0)).expect("A's reporter reward minted at (slash_a, 0)");
    let rb = utxos.get(&TransactionOutpoint::new(slash_b_id, 0)).expect("B's reporter reward minted at (slash_b, 0)");
    assert_eq!((ra.amount, &ra.script_public_key), (dist_a.reporter_reward_sompi, &spk_a), "A's reporter share pays A");
    assert_eq!((rb.amount, &rb.script_public_key), (dist_b.reporter_reward_sompi, &spk_b), "B's reporter share pays B");
    // No honest epoch peer ⇒ no victim-compensation output for either bond.
    assert!(!utxos.contains_key(&TransactionOutpoint::new(slash_a_id, 2)), "no victim output without an honest peer (A)");
    assert!(!utxos.contains_key(&TransactionOutpoint::new(slash_b_id, 2)), "no victim output without an honest peer (B)");

    // Per-bond value conservation: each 4-way split sums back to the slashed amount.
    assert_eq!(
        dist_a.reporter_reward_sompi + dist_a.security_reserve_sompi + dist_a.victim_epoch_pool_sompi + dist_a.burned_sompi,
        bond_a_amount,
        "A's slash split conserves value"
    );
    assert_eq!(
        dist_b.reporter_reward_sompi + dist_b.security_reserve_sompi + dist_b.victim_epoch_pool_sompi + dist_b.burned_sompi,
        bond_b_amount,
        "B's slash split conserves value"
    );

    // MULTI-SLASH INVARIANT: the committed security-reserve accrual is the SUM of both bonds' reserve
    // shares (the fold in `apply_slashing_side_effects`). It commits under `settle` (the block whose
    // selected parent is `slash_block`, so its mergeset carries the two slash txs). `settle`'s parent
    // (`slash_block`) accrued no reserve (balance 0 ⇒ no drip), so the recurrence reduces to
    // `0 + (reserve_a + reserve_b) − 0`.
    let committed_reserve = ctx.consensus.virtual_processor().reserve_balance_store.get(settle.header.hash).unwrap_or(0);
    assert_eq!(
        committed_reserve,
        dist_a.security_reserve_sompi + dist_b.security_reserve_sompi,
        "the block's reserve accrual is the SUM of both slashed bonds' reserve shares"
    );
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4) — security-reserve DRIP (audit M-01). Closes the
/// reserve loop end-to-end: a slashing accrues its reserve share to the pool, and when an epoch the
/// pool can pay finalizes, the reserve DRIPS back out into that block's coinbase, stake-proportionally
/// to the epoch's honest included validators. TWO validators: A is slashed for equivocation (its
/// `security_reserve_bps` share accrues to the reserve pool); B honestly attests the canonical anchor
/// for a ready epoch E and joins `included[E]`. Once epoch E finalizes (its `(E+1)·L + finalization_depth`
/// DAA threshold is crossed), the finalizing block's coinbase pays B the whole reserve (cap set high,
/// B the sole included validator). Proves accrued-in == dripped-out (value conservation).
///
/// NOTE the config sets `epoch_length_blocks == attestation_epoch_length_blue_score`: the drip pays
/// the FINALIZING epoch's `included` set, which `recompute_epoch_tallies` keys by the ATTESTATION
/// epoch, while `epochs_finalized_at` selects epochs by the DAA epoch (`daa_score / epoch_length_blocks`).
/// The two numberings coincide (on a linear chain blue_score ≈ daa_score) only when those two lengths
/// are equal — which is exactly the production reality (both = 100 in GENESIS_ACTIVE/PRODUCTION_DNS_PARAMS).
#[tokio::test]
async fn pos_v2_reserve_drip_pays_finalized_epoch() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{
            DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload, compute_slashing_distribution, ready_epoch_from_tip_blue_score,
        },
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            // epoch_length_blocks == attestation_epoch_length_blue_score (production reality) so the
            // attestation epoch B signs and the DAA epoch the drip finalizes are the same number.
            dns.epoch_length_blocks = 3;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            // Reward recency must comfortably cover the canonical anchor (buried by lag + backoff
            // below the tip); finalization_depth = window + max_reorg_horizon = 52.
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.stake_score_window_blue_score = 10_000;
            // Isolate the drip: participation takes the full validator pool (quality-bonus pool = 0), so
            // the only post-attestation coinbase output to B is the reserve drip.
            dns.reward_params.validator_participation_bps = 10_000;
            dns.reward_params.slashing_reporter_reward_bps = 1000;
            dns.reward_params.security_reserve_bps = 4000; // 40% of the slashed bond accrues to the reserve
            dns.reward_params.victim_epoch_pool_bps = 4000;
            dns.reward_params.reserve_drip_per_epoch_cap_sompi = u64::MAX; // the whole reserve drips in one epoch
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let (va, vb) = (dns_harness::harness_validator([0x42u8; 32]), dns_harness::harness_validator([0x43u8; 32]));
    let payload_a: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&va.pubkey).as_bytes();
    let payload_b: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&vb.pubkey).as_bytes();
    let (spk_a, spk_b) = (p2pkh_mldsa87_spk(&payload_a), p2pkh_mldsa87_spk(&payload_b));

    // Fund: A needs two coinbases (bond + slashing-evidence tx), B two (bond + attestation shard).
    // A block's coinbase pays its MERGESET miner, so mine a batch per miner and SCAN all coinbases.
    let (miner_a, miner_b) = (MinerData::new(spk_a.clone(), vec![]), MinerData::new(spk_b.clone(), vec![]));
    let mut blocks = Vec::new();
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_a.clone(), vec![]).await);
    }
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_b.clone(), vec![]).await);
    }
    for _ in 0..5 {
        blocks.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }
    let (mut a_funds, mut b_funds) = (Vec::new(), Vec::new());
    for blk in &blocks {
        let cb = &blk.transactions[0];
        for (i, o) in cb.outputs.iter().enumerate() {
            let f = (TransactionOutpoint::new(cb.id(), i as u32), o.value, blk.header.daa_score);
            if o.script_public_key == spk_a {
                a_funds.push(f);
            } else if o.script_public_key == spk_b {
                b_funds.push(f);
            }
        }
    }
    assert!(a_funds.len() >= 2 && b_funds.len() >= 2, "need ≥2 funding coinbases each (a={}, b={})", a_funds.len(), b_funds.len());
    let ((cb_a1, va1, da1), (cb_a2, va2, da2)) = (a_funds[0], a_funds[1]);
    let ((cb_b1, vb1, db1), (cb_b2, vb2, db2)) = (b_funds[0], b_funds[1]);

    let storage = ctx.consensus.params().storage_mass_parameter;
    let genesis_hash = ctx.consensus.params().genesis.hash;

    // ── B bonds and HONESTLY attests the ready canonical epoch E ────────────────────────────────
    // B bonds and attests FIRST: A is bonded only later (below), strictly after E's anchor, so A is
    // not part of E's expected-stake denominator — leaving B the sole included validator at E, which
    // makes the drip pay B the WHOLE reserve (a crisp value-conservation assertion). The stake-
    // proportional split when a slashed peer co-existed at the anchor is exercised separately.
    let (bond_b_tx, _, _) = dns_harness::funded_signed_bond_tx([0x43u8; 32], cb_b1, vb1, db1, vb1 - 100_000, 0, storage);
    let bond_b_id = bond_b_tx.id();
    ctx.mine_block(new_miner_data(), vec![bond_b_tx]).await;
    let bond_b_outpoint = TransactionOutpoint::new(bond_b_id, 0);
    // Bury so a ready, bond-active canonical anchor exists for B's epoch E.
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor for the ready epoch")
    };
    let epoch_e = anchor.epoch;
    let att_b = dns_harness::build_signed_attestation(
        &vb,
        genesis_hash.as_byte_slice(),
        bond_b_outpoint,
        epoch_e,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_b = dns_harness::funded_signed_shard_tx([0x43u8; 32], cb_b2, vb2, db2, att_b, storage);
    let reward_block = ctx.mine_block(new_miner_data(), vec![shard_b]).await;
    assert!(
        reward_block.transactions[0].outputs.iter().any(|o| o.script_public_key == spk_b),
        "B must be rewarded for attesting epoch E (so it joins included[E])"
    );

    // ── Accrue the reserve: bond A (AFTER E's anchor) and slash it for equivocation ─────────────
    let bond_a_amount = va1 - 100_000;
    let (bond_a_tx, _, _) = dns_harness::funded_signed_bond_tx([0x42u8; 32], cb_a1, va1, da1, bond_a_amount, 0, storage);
    let bond_a_id = bond_a_tx.id();
    ctx.mine_block(new_miner_data(), vec![bond_a_tx]).await;
    let bond_a_outpoint = TransactionOutpoint::new(bond_a_id, 0);
    // Bury so A's bond is committed into the active bond view the slashing verifier reads.
    let mut buried = Vec::new();
    for _ in 0..6 {
        buried.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }
    // A equivocates an arbitrary epoch (1) DISJOINT from B's epoch E, so A's slash mints no victim
    // output (epoch 1 has no honest included peer) — only the reserve accrues.
    let target_daa = buried[1].header.daa_score;
    let att_a1 = dns_harness::build_signed_attestation(
        &va,
        genesis_hash.as_byte_slice(),
        bond_a_outpoint,
        1,
        Hash64::from_bytes([0xa1u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let att_a2 = dns_harness::build_signed_attestation(
        &va,
        genesis_hash.as_byte_slice(),
        bond_a_outpoint,
        1,
        Hash64::from_bytes([0xb2u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let evidence = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: bond_a_outpoint,
        attestation_a: att_a1,
        attestation_b: att_a2,
        reporter_reward_spk_payload: payload_a,
    };
    let slash_tx = dns_harness::funded_signed_slashing_evidence_tx([0x42u8; 32], cb_a2, va2, da2, evidence, storage);
    let slash_block = ctx.mine_block(new_miner_data(), vec![slash_tx]).await;
    assert_eq!(ctx.consensus.block_status(slash_block.header.hash), BlockStatus::StatusUTXOValid, "the slashing block must validate");
    // Settle so the reserve accrual commits (a block's own txs apply once it becomes a selected parent).
    let reserve_settle = ctx.mine_block(new_miner_data(), vec![]).await;
    let dist = compute_slashing_distribution(
        bond_a_amount,
        dns.reward_params.slashing_reporter_reward_bps,
        dns.reward_params.security_reserve_bps,
        dns.reward_params.victim_epoch_pool_bps,
    );
    let reserve_accrued = ctx.consensus.virtual_processor().reserve_balance_store.get(reserve_settle.header.hash).unwrap_or(0);
    assert_eq!(reserve_accrued, dist.security_reserve_sompi, "A's slash accrues its reserve share to the pool");
    assert!(reserve_accrued > 0, "the reserve must be non-zero to drip");

    // ── Mine until epoch E's DAA-finalization; the drip pays B in that block's coinbase ─────────
    let target_final_daa =
        (epoch_e + 1) * dns.epoch_length_blocks + dns.reward_uniqueness_window_blocks + dns.max_reorg_horizon_blocks;
    let mut drip_block = None;
    for _ in 0..80 {
        let blk = ctx.mine_block(new_miner_data(), vec![]).await;
        // The reserve drip is appended to the coinbase of the block that finalizes epoch E. B got its
        // one-time participation reward at `reward_block` (cross-block dedup blocks re-payment), and the
        // §D worker bounty pays the includer — so the only later coinbase output to B is the drip.
        if blk.transactions[0].outputs.iter().any(|o| o.script_public_key == spk_b) {
            drip_block = Some(blk);
            break;
        }
        if blk.header.daa_score > target_final_daa + 5 {
            break;
        }
    }
    let drip_block = drip_block.expect("a block after the reward must drip the reserve to B at epoch E's finalization");
    let drip_out = drip_block.transactions[0].outputs.iter().find(|o| o.script_public_key == spk_b).expect("drip pays B");
    // The sole included validator B receives the WHOLE reserve (cap is u64::MAX): accrued-in == dripped-out.
    assert_eq!(drip_out.value, reserve_accrued, "the entire accrued reserve drips to the lone included validator B");
}

/// kaspa-pq ADR-0016 §D.2 — the bond-UTXO spend-gate races the slashing side-effect (audit M-01). A
/// validator's locked stake (the bond's output-0) is NOT releasable while the bond is Active, so a
/// block that SPENDS it must be rejected — even when the SAME block also carries a slashing-evidence
/// tx for that bond (which would otherwise remove output-0). The spend-gate wins the race: the block
/// is disqualified (`NonReleasableBondSpendInBlock`), so NEITHER the spend NOR the slash takes effect
/// — the locked stake survives intact and no reporter reward is minted. Proves a validator cannot
/// reclaim locked capital by smuggling a self-spend into a block, and that the spend-gate takes
/// precedence over the slashing side-effect.
#[tokio::test]
async fn pos_v2_spend_gate_rejects_locked_bond_racing_slash() {
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload},
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // One validator/funding/reporter key.
    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // Fund: h_a pays K (funds the bond), h_b pays K (funds the slashing-evidence tx).
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    // Bond: output-0 is the locked stake (a P2PKH to K), Active from activation 0.
    let storage = ctx.consensus.params().storage_mass_parameter;
    let bond_amount = value_a - 100_000;
    let (bond_tx, _vid, _payload) = dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, bond_amount, 0, storage);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid, "the bond block must be UTXO-valid");
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    let bond_daa = bond_block.header.daa_score;

    // Bury so the bond is committed into the active bond view the slashing verifier reads.
    let mut buried = Vec::new();
    for _ in 0..6 {
        buried.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }

    // Equivocation evidence (two conflicting attestations for the same (bond, epoch)).
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let target_daa = buried[1].header.daa_score;
    let att_a = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        1,
        Hash64::from_bytes([0xa1u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let att_b = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        1,
        Hash64::from_bytes([0xb2u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let evidence = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint,
        attestation_a: att_a,
        attestation_b: att_b,
        reporter_reward_spk_payload: k_payload,
    };
    let slash_tx = dns_harness::funded_signed_slashing_evidence_tx(seed, coinbase_b, value_b, daa_b, evidence, storage);
    let slash_tx_id = slash_tx.id();
    // A self-spend of the still-locked bond output-0 (the spend-gate violation).
    let spend_tx = dns_harness::funded_signed_p2pkh_spend(seed, bond_outpoint, bond_amount, bond_daa, storage);

    // ONE block carries BOTH: the slash (which would remove output-0) AND the self-spend of output-0.
    //
    // Since the 2026-08-11 audit P0 the devnet preset activates the MERGESET spend gate at
    // genesis, which replaces the own-body REJECT with an acceptance-time SKIP: the block stays
    // valid and the spend simply is not accepted. That is the deliberate trade — an own-body
    // reject makes an honest miner self-reject for merely MERGING someone else's forbidden
    // spend, while the skip protects the collateral in both cases. What must remain true either
    // way is the property this test exists for: the locked output is NOT spendable while the
    // bond is unreleasable.
    let race_block = ctx.mine_block(new_miner_data(), vec![slash_tx, spend_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(race_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the mergeset skip keeps the block valid — an honest miner must not self-reject over a merged spend"
    );
    // What the SKIP changes, and what it does not. The spend is not accepted — that is the whole
    // point, the collateral cannot leave while the bond is unreleasable — but the block is valid,
    // so everything ELSE in it applies, including the slash. The locked output therefore does
    // leave the UTXO set: removed by the slashing side-effect, not spent by its owner, and the
    // reporter reward that proves which of the two happened IS minted.
    //
    // Under the old own-body REJECT the block was disqualified and neither happened. Both
    // outcomes protect the collateral; only this one lets an honest miner merge the spend without
    // self-rejecting.
    let utxos: std::collections::HashMap<_, _> = ctx.consensus.get_virtual_utxos(None, 100_000, false).into_iter().collect();
    assert!(!utxos.contains_key(&bond_outpoint), "the locked output must not survive as spendable stake");
    assert!(
        utxos.contains_key(&TransactionOutpoint::new(slash_tx_id, 0)),
        "the slash applied and paid its reporter — so the output left by SLASHING, not by the refused spend"
    );
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2) — slashing is REORG-RESISTANT and reorg-SAFE (audit M-01). An
/// equivocator cannot escape its slash by getting the network to reorg onto a heavier branch that
/// omits the evidence. A bond X is buried in a shared prefix; one branch (A) slashes X — committing
/// the side-effect (output-0 removed, reporter minted at `(slash_tx, 0)`) — while a HEAVIER competing
/// branch (B), built by a second consensus instance over the SAME prefix, omits the slash. When B's
/// blocks arrive the node reorgs onto B (the reorg gate is held dormant — Bootstrap stage, since
/// `min_active_validators` is raised so a lone bond never activates it — so selection is pure
/// blue_work). The slash block leaves the SELECTED chain, but branch A is still a DAG tip and is
/// MERGED into the virtual, so the slash side-effect is RECOMPUTED and re-applies deterministically:
/// X stays slashed and the reporter stays minted, exactly once (no double-removal, no panic, supply
/// conserved). This is the economically correct, reorg-safe outcome — the equivocation evidence is
/// permanent in the DAG, so the punishment survives the reorg rather than being stranded or replayed.
#[tokio::test]
async fn pos_v2_slashing_survives_reorg_via_evidence_merge() {
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload},
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            // A large reorg horizon so the fork is within range (the gate is dormant anyway).
            dns.max_reorg_horizon_blocks = 1000;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            // Keep the rollout stage in Bootstrap (one bond can never reach Active), so the reorg gate
            // stays GateInactive and selection is pure blue_work — the heaviest branch wins.
            dns.min_active_validators = 100;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // ── Shared prefix on the honest node: fund + bond X + bury (collected for delivery to the 2nd
    //    instance, so both branches share an identical bond-creation history) ────────────────────
    let mut prefix = Vec::new();
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    prefix.push(ctx.mine_block(k_miner.clone(), vec![]).await);
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    prefix.push(h_a.clone());
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    prefix.push(h_b.clone());
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        prefix.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }
    let storage = ctx.consensus.params().storage_mass_parameter;
    let bond_amount = value_a - 100_000;
    let (bond_tx, _vid, _rp) = dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, bond_amount, 0, storage);
    let bond_tx_id = bond_tx.id();
    prefix.push(ctx.mine_block(new_miner_data(), vec![bond_tx]).await);
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    let mut buried = Vec::new();
    for _ in 0..6 {
        let b = ctx.mine_block(new_miner_data(), vec![]).await;
        buried.push(b.clone());
        prefix.push(b);
    }

    // ── Second instance: replay the SAME prefix, then build a HEAVIER no-slash branch B ─────────
    let mut atk = TestContext::new(TestConsensus::new(&config));
    for b in &prefix {
        atk.validate_and_insert_block(b.clone()).await;
    }
    atk.simulated_time = ctx.simulated_time; // so branch B's timestamps stay ahead of the prefix
    let mut branch_b = Vec::new();
    for _ in 0..12 {
        branch_b.push(atk.mine_block(new_miner_data(), vec![]).await);
    }
    let branch_b_tip = branch_b.last().unwrap().header.hash;

    // ── Honest branch A: slash X (equivocation) and settle so the side-effect is COMMITTED ──────
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let target_daa = buried[1].header.daa_score;
    let att_a = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        1,
        Hash64::from_bytes([0xa1u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let att_b = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        1,
        Hash64::from_bytes([0xb2u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let evidence = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint,
        attestation_a: att_a,
        attestation_b: att_b,
        reporter_reward_spk_payload: k_payload,
    };
    let slash_tx = dns_harness::funded_signed_slashing_evidence_tx(seed, coinbase_b, value_b, daa_b, evidence, storage);
    let slash_tx_id = slash_tx.id();
    let slash_block = ctx.mine_block(new_miner_data(), vec![slash_tx]).await;
    assert_eq!(ctx.consensus.block_status(slash_block.header.hash), BlockStatus::StatusUTXOValid, "branch A's slash block validates");
    ctx.mine_block(new_miner_data(), vec![]).await; // settle so the slash side-effect commits

    // Slash applied on branch A: X's locked stake is gone and the reporter reward is minted.
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let expected_reporter = kaspa_consensus_core::dns_finality::compute_slashing_distribution(
        bond_amount,
        dns.reward_params.slashing_reporter_reward_bps,
        dns.reward_params.security_reserve_bps,
        dns.reward_params.victim_epoch_pool_bps,
    )
    .reporter_reward_sompi;
    let reporter_outpoint = TransactionOutpoint::new(slash_tx_id, 0);
    let pre: std::collections::HashMap<_, _> = ctx.consensus.get_virtual_utxos(None, 100_000, false).into_iter().collect();
    assert!(!pre.contains_key(&bond_outpoint), "branch A: the slashed bond's locked stake is removed");
    assert_eq!(pre.get(&reporter_outpoint).map(|u| u.amount), Some(expected_reporter), "branch A: the reporter reward is minted");

    // ── Deliver branch B → the node reorgs onto the heavier no-slash branch ─────────────────────
    for b in &branch_b {
        ctx.validate_and_insert_block(b.clone()).await;
    }
    assert_eq!(ctx.consensus.get_sink(), branch_b_tip, "the node reorged onto the heavier branch B (gate dormant ⇒ pure blue_work)");

    // ── The slash SURVIVES the reorg: branch A leaves the selected chain but is merged back into the
    //    virtual, so the side-effect re-applies deterministically — X stays slashed, reporter stays
    //    minted EXACTLY ONCE (no double-removal, no double-mint, no panic). ──────────────────────
    let post: std::collections::HashMap<_, _> = ctx.consensus.get_virtual_utxos(None, 100_000, false).into_iter().collect();
    assert!(
        !post.contains_key(&bond_outpoint),
        "after reorg: the equivocator is STILL slashed — its locked stake stays removed (evidence merged back)"
    );
    assert_eq!(
        post.get(&reporter_outpoint).map(|u| u.amount),
        Some(expected_reporter),
        "after reorg: the reporter reward is still minted, exactly once (recomputed over the new selected chain + merge set)"
    );
}

/// kaspa-pq ADR-0018 §F (DAG-3) — STAGED reward-split rollout across the `full_reward_split_daa_score`
/// boundary. The §F carve selects the fee/subsidy split deterministically from the block's DAA score:
/// below `full_reward_split_daa_score` the BOOTSTRAP split (smaller validator carve — worker base
/// 8200bps), at/above it the FULL split (worker base 6200bps; validator 30% — re-genesis raised it
/// from 25%). This mines a constant-miner chain straight across the boundary and asserts (a) EVERY
/// block stays UTXO-valid — the coinbase carve the template builds equals the one validation
/// recomputes, on BOTH sides AND at the crossing block (construction == validation across a staged
/// consensus parameter), and (b) the miner's per-block subsidy share visibly DROPS at the boundary
/// (bootstrap 82% → full 62% of subsidy), proving the split actually changed rather than the stage
/// being inert.
#[tokio::test]
async fn pos_v2_staged_full_reward_split_across_boundary() {
    kaspa_core::log::try_init_logger("info");
    const H: u64 = 20; // full_reward_split_daa_score
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0; // overlay active ⇒ the §F carve applies (Some(split))
            dns.full_reward_split_daa_score = H; // Stage 2 (bootstrap) below H, Stage 3 (full) at/above
            // pos_v2 stays fenced (preset u64::MAX) — §F fee-split staging is independent of the v2 economics.
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // A single, constant miner so every coinbase pays the same spk (the reward is the prev block's
    // carved subsidy — the coinbase pays its mergeset miner).
    let v = dns_harness::harness_validator([0x42u8; 32]);
    let k_spk = p2pkh_mldsa87_spk(&kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes());
    let miner = MinerData::new(k_spk.clone(), vec![]);

    let mut rewards: Vec<(u64, u64)> = Vec::new(); // (block daa_score, miner's coinbase reward)
    for _ in 0..(H + 15) {
        let b = ctx.mine_block(miner.clone(), vec![]).await;
        assert_eq!(
            ctx.consensus.block_status(b.header.hash),
            BlockStatus::StatusUTXOValid,
            "every block stays UTXO-valid across the staged-split boundary (construction == validation)"
        );
        let r: u64 = b.transactions[0].outputs.iter().filter(|o| o.script_public_key == k_spk).map(|o| o.value).sum();
        rewards.push((b.header.daa_score, r));
    }

    // The coinbase of a block at DAA d carves the mergeset (prev block's) subsidy by the split
    // SELECTED FROM d. Sample a block clearly in Stage 2 (bootstrap) and one clearly in Stage 3
    // (full); both adjacent enough that subsidy decay is negligible, so the ratio isolates the carve.
    let stage2 = rewards.iter().rev().find(|(d, r)| *d < H && *r > 0).map(|(_, r)| *r).expect("a Stage-2 reward");
    let stage3 = rewards.iter().find(|(d, r)| *d >= H && *r > 0).map(|(_, r)| *r).expect("a Stage-3 reward");
    // Worker base share drops 8200bps → 6200bps ⇒ ratio ≈ 0.7561. Tolerance absorbs the tiny per-block decay.
    let ratio = stage3 as f64 / stage2 as f64;
    assert!(
        (0.74..=0.77).contains(&ratio),
        "the miner's subsidy share drops at the boundary by the bootstrap→full worker-base carve (8200→6200bps ≈ 0.756); got stage2={stage2} stage3={stage3} ratio={ratio:.4}"
    );
}

/// kaspa-pq ADR-0018 §G (DAG-7) — MULTI-NODE mesh convergence with the DNS overlay ACTIVE. Three
/// independent consensus instances (same overlay-active config) each mine a DIVERGENT chain from
/// genesis; then every block is gossiped to every node. All three must converge on the SAME sink —
/// i.e. the overlay's per-block machinery (epoch accumulator / reserve / rewarded-keys stores) and
/// the reorg gate (dormant here: no attestations ⇒ no confirmed anchor) do NOT break GHOSTDAG's
/// deterministic multi-node convergence. Complements the single-instance wide-DAG anchor-agreement
/// test (which proves divergent VIEWS pick one anchor) with real cross-instance block exchange.
#[tokio::test]
async fn dag7_multi_node_mesh_converges_with_overlay_active() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 8; // enough to merge the divergent tips
            p.mergeset_size_limit = 16;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0; // overlay ACTIVE on every node
            p.dns_params = Some(dns);
        })
        .build();

    // Three nodes, each mining a chain of a DIFFERENT length from genesis (genuinely divergent tips).
    let mut nodes: Vec<TestContext> = (0..3).map(|_| TestContext::new(TestConsensus::new(&config))).collect();
    let lengths = [5usize, 8, 6];
    let mut chains: Vec<Vec<Block>> = Vec::new();
    for i in 0..nodes.len() {
        let mut blocks = Vec::new();
        for _ in 0..lengths[i] {
            blocks.push(nodes[i].mine_block(new_miner_data(), vec![]).await);
        }
        chains.push(blocks);
    }

    // Before gossip the nodes disagree (each sees only its own chain's tip).
    let pre: Vec<_> = nodes.iter().map(|n| n.consensus.get_sink()).collect();
    assert!(pre[0] != pre[1] || pre[1] != pre[2], "pre-gossip the nodes' sinks diverge");

    // Gossip: deliver every OTHER node's chain (parents-first) to each node.
    // Index-based: the inner `i == j` skip needs both indices, and `nodes[i]` is
    // borrowed mutably while `chains[j]` is borrowed immutably in the same body.
    #[allow(clippy::needless_range_loop)]
    for i in 0..nodes.len() {
        for j in 0..chains.len() {
            if i == j {
                continue;
            }
            for b in &chains[j] {
                nodes[i].validate_and_insert_block(b.clone()).await;
            }
        }
    }

    // After gossip every node holds the identical union DAG ⇒ all converge on ONE sink.
    let sinks: Vec<_> = nodes.iter().map(|n| n.consensus.get_sink()).collect();
    assert_eq!(sinks[0], sinks[1], "node 0 and node 1 converge on the same sink ({} vs {})", sinks[0], sinks[1]);
    assert_eq!(sinks[1], sinks[2], "node 1 and node 2 converge on the same sink ({} vs {})", sinks[1], sinks[2]);
    // The converged sink is the heaviest divergent chain's tip (node 1's 8-block chain), and every
    // node's chosen sink is one of the gossiped tips (a real block, not genesis).
    let tips: std::collections::HashSet<_> = chains.iter().map(|c| c.last().unwrap().header.hash).collect();
    assert!(tips.contains(&sinks[0]), "the converged sink is one of the mined chain tips");
}

/// kaspa-pq DNS-finality optional hard inclusion — SELECTIVE attestation CENSORSHIP below φS is invalid when enabled.
///
/// Two equal-stake validators A and B are both bonded. With φS = 60%, a block/template that includes
/// only A's attestation reaches 50% included stake and must be rejected by consensus. Including both
/// reaches 100%, clears the mandatory gate, and the block validates.
#[tokio::test]
async fn dag5_selective_censorship_below_quality_floor_is_rejected() {
    use kaspa_consensus_core::{Hash64, errors::block::RuleError};
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            dns.mandatory_attestation_inclusion_daa_score = 0;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let (va, vb) = (dns_harness::harness_validator([0x42u8; 32]), dns_harness::harness_validator([0x43u8; 32]));
    let payload_a: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&va.pubkey).as_bytes();
    let payload_b: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&vb.pubkey).as_bytes();
    let (spk_a, spk_b) = (p2pkh_mldsa87_spk(&payload_a), p2pkh_mldsa87_spk(&payload_b));

    // Fund: each validator needs one coinbase for the bond and one for the mandatory shard.
    let (miner_a, miner_b) = (MinerData::new(spk_a.clone(), vec![]), MinerData::new(spk_b.clone(), vec![]));
    let mut blocks = Vec::new();
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_a.clone(), vec![]).await);
    }
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_b.clone(), vec![]).await);
    }
    for _ in 0..5 {
        blocks.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }
    let (mut a_funds, mut b_funds) = (Vec::new(), Vec::new());
    for blk in &blocks {
        let cb = &blk.transactions[0];
        for (i, o) in cb.outputs.iter().enumerate() {
            let f = (TransactionOutpoint::new(cb.id(), i as u32), o.value, blk.header.daa_score);
            if o.script_public_key == spk_a {
                a_funds.push(f);
            } else if o.script_public_key == spk_b {
                b_funds.push(f);
            }
        }
    }
    assert!(a_funds.len() >= 2 && b_funds.len() >= 2, "need ≥2 A / ≥2 B funding coinbases (a={}, b={})", a_funds.len(), b_funds.len());
    let ((cb_a_bond, va1, da_bond), (cb_a_att, va_att, da_a_att)) = (a_funds[0], a_funds[1]);
    let ((cb_b_bond, vb1, db_bond), (cb_b_e1, vb_e1, db_b_e1)) = (b_funds[0], b_funds[1]);

    // Bond A and B with EXACTLY equal stake. One validator alone is 50% < φS(60%).
    let storage = ctx.consensus.params().storage_mass_parameter;
    let bond_amount = va1.min(vb1) - 100_000;
    let (bond_a_tx, _, _) = dns_harness::funded_signed_bond_tx([0x42u8; 32], cb_a_bond, va1, da_bond, bond_amount, 0, storage);
    let (bond_b_tx, _, _) = dns_harness::funded_signed_bond_tx([0x43u8; 32], cb_b_bond, vb1, db_bond, bond_amount, 0, storage);
    let (bond_a_id, bond_b_id) = (bond_a_tx.id(), bond_b_tx.id());
    ctx.mine_block(new_miner_data(), vec![bond_a_tx]).await;
    ctx.mine_block(new_miner_data(), vec![bond_b_tx]).await;
    let (bond_a_outpoint, bond_b_outpoint) = (TransactionOutpoint::new(bond_a_id, 0), TransactionOutpoint::new(bond_b_id, 0));

    // Advance until the first ready epoch whose selected-parent chain is under-certified. Empty
    // templates are valid before that point and rejected exactly once the hard inclusion gate opens.
    let missing_epoch = {
        let mut guard = 0;
        loop {
            let res = ctx.consensus.build_block_template(
                new_miner_data(),
                Box::new(OnetimeTxSelector::new(Vec::new())),
                TemplateBuildMode::Standard,
            );
            match res {
                Ok(mut t) => {
                    guard += 1;
                    assert!(guard < 64, "expected the mandatory attestation gate to open");
                    ctx.simulated_time += ctx.consensus.params().target_time_per_block();
                    t.block.header.timestamp = ctx.simulated_time;
                    t.block.header.nonce = ctx.simulated_time;
                    t.block.header.finalize();
                    ctx.validate_and_insert_block(t.block.to_immutable()).await;
                }
                Err(RuleError::MissingMandatoryAttestationInBlock(epoch, included, expected, floor)) => {
                    assert_eq!(included, 0, "the first deficient epoch has no parent-chain attestation yet");
                    assert_eq!(expected, bond_amount.saturating_mul(2));
                    assert_eq!(floor, 6000);
                    break epoch;
                }
                Err(e) => panic!("unexpected template error before mandatory gate: {e:?}"),
            }
        }
    };

    let genesis_hash = ctx.consensus.params().genesis.hash;
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let quality_deficit = ctx
        .consensus
        .get_attestation_quality_deficits()
        .into_iter()
        .find(|deficit| deficit.epoch == missing_epoch)
        .expect("quality-monitoring API reports the under-certified ready epoch");
    assert_eq!(quality_deficit.included_stake, 0);
    assert_eq!(quality_deficit.expected_stake, bond_amount.saturating_mul(2));
    assert_eq!(quality_deficit.required_stake_delta, quality_deficit.required_stake);

    let anchor_at = |ctx: &TestContext, epoch: u64| {
        let vp = ctx.consensus.virtual_processor();
        vp.canonical_anchor_by_blue_score(epoch, ctx.consensus.get_sink(), &dns).expect("canonical anchor")
    };

    let anchor_e1 = anchor_at(&ctx, missing_epoch);
    let att_a1 = dns_harness::build_signed_attestation(
        &va,
        genesis_hash.as_byte_slice(),
        bond_a_outpoint,
        missing_epoch,
        anchor_e1.anchor_hash,
        anchor_e1.anchor_daa_score,
        Hash64::default(),
    );
    let att_b1 = dns_harness::build_signed_attestation(
        &vb,
        genesis_hash.as_byte_slice(),
        bond_b_outpoint,
        missing_epoch,
        anchor_e1.anchor_hash,
        anchor_e1.anchor_daa_score,
        Hash64::default(),
    );
    let shard_a1 = dns_harness::funded_signed_shard_tx([0x42u8; 32], cb_a_att, va_att, da_a_att, att_a1, storage);
    let shard_b1 = dns_harness::funded_signed_shard_tx([0x43u8; 32], cb_b_e1, vb_e1, db_b_e1, att_b1, storage);

    // Selector snapshot regression: the deficits handed to the mining selector must be derived
    // from the same template snapshot as validation, including candidate-accepted txs. If A is
    // already accepted by the virtual candidate set, the selector should see only the remaining
    // stake delta, not the full floor from the selected-parent chain.
    let selector_snapshot_deficit = {
        let vp = ctx.consensus.virtual_processor();
        let bond_view = vp.initial_active_bond_view();
        let deficits = vp.mandatory_attestation_deficits_for_template_snapshot(
            ctx.consensus.get_sink(),
            ctx.consensus.get_virtual_daa_score(),
            &bond_view,
            std::slice::from_ref(&shard_a1),
        );
        deficits.into_iter().find(|deficit| deficit.epoch == missing_epoch).expect("candidate-accepted A leaves a reduced deficit")
    };
    assert_eq!(selector_snapshot_deficit.pre_body_included_stake, bond_amount);
    assert_eq!(
        selector_snapshot_deficit.required_stake_delta,
        selector_snapshot_deficit.required_stake.saturating_sub(bond_amount),
        "selector deficit must be reduced by candidate-accepted stake before body selection"
    );

    // A-only is selective censorship: 50% included stake is below the 60% quality floor, so the
    // template is not produced.
    let only_a = ctx.consensus.build_block_template(
        new_miner_data(),
        Box::new(OnetimeTxSelector::new(vec![shard_a1.clone()])),
        TemplateBuildMode::Standard,
    );
    match only_a {
        Err(RuleError::MissingMandatoryAttestationInBlock(epoch, included, expected, floor)) => {
            assert_eq!(epoch, missing_epoch);
            assert_eq!(included, bond_amount);
            assert_eq!(expected, bond_amount.saturating_mul(2));
            assert_eq!(floor, 6000);
        }
        other => panic!("A-only censorship template must be rejected, got {other:?}"),
    }

    // A+B reaches 100% included stake and validates.
    let block_full = ctx.mine_block(new_miner_data(), vec![shard_a1, shard_b1]).await;
    let reward = |blk: &Block, spk: &kaspa_consensus_core::tx::ScriptPublicKey| -> u64 {
        blk.transactions[0].outputs.iter().filter(|o| o.script_public_key == *spk).map(|o| o.value).sum()
    };
    let (a_reward_e1, b_reward_e1) = (reward(&block_full, &spk_a), reward(&block_full, &spk_b));
    assert!(a_reward_e1 > 0 && b_reward_e1 > 0, "both included validators are rewarded");
    assert_eq!(a_reward_e1, b_reward_e1, "equal stake gives equal participation reward");

    // Hard mandatory child-after-certification regression: a child of the certifying block must
    // not re-demand the epoch that the selected-parent chain already brought above the floor. It
    // may still stop on a later deficient ready epoch if the test chain has already advanced far
    // enough for another backlog item.
    let child_after_cert = ctx.consensus.build_block_template(
        new_miner_data(),
        Box::new(OnetimeTxSelector::new(Vec::new())),
        TemplateBuildMode::Standard,
    );
    match child_after_cert {
        Ok(_) => {}
        Err(RuleError::MissingMandatoryAttestationInBlock(epoch, ..)) => {
            assert_ne!(
                epoch, missing_epoch,
                "child-after-certification must not re-demand the epoch certified by its selected parent"
            );
        }
        other => panic!("child-after-certification must not fail with an unrelated error, got {other:?}"),
    }
}

/// kaspa-pq H-06 (unbond lifecycle): full-consensus unbond-REQUEST e2e + the client-side
/// funded builder (`funded_signed_unbond_tx`). A funded, ML-DSA-87-signed bond goes
/// Active; the owner then submits a funded, signed `StakeUnbondRequest` — the shape an
/// operator's exit tool produces. The including block must validate, exercising the live
/// unbond-authorization rule (`unbond_request_authorized`: bond present, Pending/Active,
/// owner-key binding `validator_id_from_pubkey(owner) == bond.owner_pubkey_hash`, and the
/// ML-DSA-87 signature over the bond-bound `unbond_request_message` under
/// `UNBOND_REQUEST_CONTEXT`). The release-after-`unbonding_period_blocks` spend is covered
/// by the apply-path unit tests (`allows_spend_of_releasable_bond`).
#[tokio::test]
async fn pos_v2_funded_unbond_request_validates() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let bond_amount = value_a - 100_000;
    let (bond_tx, _vid, _rp) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, bond_amount, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid, "the bond block must be UTXO-valid");
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    // Bury the bond so its record is committed into the active bond view the unbond rule reads.
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    // The owner submits a funded, ML-DSA-87-signed unbond request; the block must validate.
    // audit M-04: the authorization binds the network id (genesis hash), as the consensus rule reconstructs it.
    let net_id = ctx.consensus.params().genesis.hash;
    let unbond_tx = dns_harness::funded_signed_unbond_tx(
        seed,
        net_id.as_byte_slice(),
        coinbase_b,
        value_b,
        daa_b,
        bond_outpoint,
        storage_mass_parameter,
    );
    let unbond_block = ctx.mine_block(new_miner_data(), vec![unbond_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(unbond_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the owner-authorized funded unbond request must validate through full consensus"
    );
}

/// kaspa-pq DNS v3 (PR2b): the processor's blue_score canonical-anchor walk
/// (`canonical_anchor_by_blue_score`) feeds the pure core the *real* selected-chain
/// `(hash, blue_score, daa_score)` ancestors, so the anchor it returns is a genuine
/// selected-chain block, most-recent-at-or-below the epoch cutoff, and stable as the tip
/// advances (the v3 position-invariance property). The hot path does not call it yet (PR4
/// wires it into the verifier), so this white-box test is the only thing exercising the
/// store walk until then. A future / unburied epoch must return `None`, never the tip.
#[tokio::test]
async fn dns_v3_canonical_anchor_walk_matches_chain() {
    use std::collections::HashMap;
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            // Tiny blue_score epochs so several bury within a short linear chain.
            // L=3, backoff=1 -> cutoff(E) = (E+1)*3 - 1 - 1 = 3E+1; lag=2.
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));
    // A linear chain (one block per row); each block's only parent is the prior tip, so
    // mergeset_blues = {selected_parent} and blue_score increments by exactly 1 (genesis = 0).
    let miner = new_miner_data();
    let mut by_blue: HashMap<u64, BlockHash> = HashMap::new();
    for _ in 0..20 {
        let b = ctx.mine_block(miner.clone(), vec![]).await;
        by_blue.insert(b.header.blue_score, b.header.hash);
    }

    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let tip = ctx.consensus.get_sink();
    let vp = ctx.consensus.virtual_processor();

    // cutoff(E) = 3E+1 on this dense chain, and every integer blue_score 0..=20 is present
    // exactly once, so the most-recent-at-or-below is the block whose blue_score == cutoff(E).
    let a0 = vp.canonical_anchor_by_blue_score(0, tip, &dns).expect("epoch 0 buried");
    assert_eq!(a0.epoch, 0);
    assert_eq!(a0.cutoff_blue_score, 1);
    assert_eq!(a0.anchor_blue_score, 1);
    assert_eq!(a0.anchor_hash, by_blue[&1], "epoch 0 anchors the real bs=1 block");
    assert!(!a0.duplicate_of_previous_anchor);

    let a1 = vp.canonical_anchor_by_blue_score(1, tip, &dns).expect("epoch 1 buried");
    assert_eq!(a1.cutoff_blue_score, 4);
    assert_eq!(a1.anchor_blue_score, 4);
    assert_eq!(a1.anchor_hash, by_blue[&4], "epoch 1 anchors the real bs=4 block");
    assert!(!a1.duplicate_of_previous_anchor); // distinct anchors on a dense chain

    // Position-invariance: anchor(0) is the SAME block no matter how far the tip advanced
    // (the walk reads blue_score, not the store index) — the core v3 property.
    let mid = by_blue[&10];
    let a0_mid = vp.canonical_anchor_by_blue_score(0, mid, &dns).expect("epoch 0 buried at mid-chain tip");
    assert_eq!(a0_mid.anchor_hash, a0.anchor_hash, "the anchor is independent of the observing tip");

    // A future / unburied epoch has no canonical anchor on this chain (cutoff > tip.blue_score)
    // and must NOT degenerate to returning the tip.
    assert!(vp.canonical_anchor_by_blue_score(1_000_000, tip, &dns).is_none());
}

/// kaspa-pq DNS v3 (PR4) — POSITIVE: an attestation that names THIS chain's canonical
/// lagged anchor for a ready blue_score epoch IS credited by the v3 verifier
/// (`collect_stake_contributions_v2`) with the bond's full stake, the per-epoch
/// denominator is keyed by the CANONICAL anchor DAA, and a ready epoch the validator did
/// NOT attest still appears in the denominator (so a participation gap is visible to φS
/// instead of vanishing — the v1 weakness). Reuses the funded-bond + funded-shard DAG-2
/// harness; the attestation is signed over the canonical `(epoch, anchor_hash,
/// anchor_daa_score)` rather than a free-floating self-reported target.
#[tokio::test]
async fn dns_v3_canonical_attestation_credited() {
    use crate::model::stores::{headers::HeaderStoreReader, stake_bonds::StakeBondsStoreReader};
    use kaspa_consensus_core::{Hash64, dns_finality::ready_epoch_from_tip_blue_score};
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 2;
            dns.max_reorg_horizon_blocks = 2;
            // Small blue_score epochs so several bury within this chain: L=3, backoff=1 ->
            // cutoff(E)=3E+1; lag=2 -> epoch E ready once tip_blue >= 3E+4.
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // Fund a bond (coinbase_a) + a shard-funding coinbase (coinbase_b), same as the e2e.
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _vid, _reward_payload) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, value_a - 100_000, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid, "the bond block is UTXO-valid");
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);

    // Bury several blue_score epochs past the bond so a ready, bond-active anchor exists.
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    // THIS chain's canonical anchor for the latest ready epoch at the current sink.
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let latest_ready =
            ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
                .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(latest_ready, sink, &dns).expect("canonical anchor for the ready epoch")
    };

    // Sign an attestation that names the canonical anchor exactly, fund + include it.
    let att = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        anchor.epoch,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_tx = dns_harness::funded_signed_shard_tx(seed, coinbase_b, value_b, daa_b, att, storage_mass_parameter);
    let reward_block = ctx.mine_block(new_miner_data(), vec![shard_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(reward_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the canonical-attestation block validates"
    );
    // Mine 2 fillers so the shard is MERGED -> accepted by a chain block in past(sink), the
    // view the StakeScore verifier walks (accepted txs, not a block's own body).
    ctx.mine_block(new_miner_data(), vec![]).await;
    ctx.mine_block(new_miner_data(), vec![]).await;

    let new_sink = ctx.consensus.get_sink();
    let (contributions, denom, bond_amount) = {
        let vp = ctx.consensus.virtual_processor();
        let bonds: Vec<_> = vp.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
        let bond_amount = bonds.iter().find(|b| b.bond_outpoint == bond_outpoint).expect("the funded bond is persisted").amount;
        let (c, d) = vp.collect_stake_contributions_v2(
            new_sink,
            None,
            &bonds,
            genesis_hash.as_byte_slice(),
            &dns,
            ContributionWeight::BondedStake,
        );
        (c, d, bond_amount)
    };

    // The canonical attestation is credited with the bond's full stake at its epoch.
    let credited = contributions.iter().find(|c| c.bond_outpoint == bond_outpoint).expect("the canonical attestation is credited");
    assert_eq!(credited.epoch, anchor.epoch, "credited at the canonical epoch");
    assert_eq!(credited.signed_weight, bond_amount as u128, "credited with the bond's full stake");
    // The denominator is keyed by the CANONICAL anchor DAA for that epoch.
    assert_eq!(denom.get(&anchor.epoch).copied(), Some(anchor.anchor_daa_score), "denominator keyed by the canonical anchor DAA");
    // A ready epoch with no attestation still appears in the denominator (visible gap).
    assert!(
        denom.keys().any(|&e| !contributions.iter().any(|c| c.epoch == e)),
        "a ready, un-attested epoch is still in the denominator (got epochs {:?})",
        denom.keys().collect::<Vec<_>>()
    );
}

/// kaspa-pq DNS v3 (PR4) — NEGATIVE: a validly-signed, bonded, reward-eligible attestation
/// for a ready epoch whose `target_hash` is NOT this chain's canonical anchor is NOT
/// credited by the v3 verifier. The including block still validates (the reward path is
/// migrated to the canonical rule in PR5; until then a non-canonical attestation can still
/// earn the v1 reward), which is exactly the divergence PR5 closes — here we prove the
/// StakeScore verifier already refuses the non-canonical target.
#[tokio::test]
async fn dns_v3_noncanonical_attestation_rejected() {
    use crate::model::stores::{headers::HeaderStoreReader, stake_bonds::StakeBondsStoreReader};
    use kaspa_consensus_core::{Hash64, dns_finality::ready_epoch_from_tip_blue_score};
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            // Wide reward recency so the canonical anchor is comfortably in-window: the only
            // reason the bogus-target attestation earns nothing is the v3 canonical gate.
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _vid, _reward_payload) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, value_a - 100_000, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid);
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let latest_ready =
            ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
                .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(latest_ready, sink, &dns).expect("canonical anchor for the ready epoch")
    };

    // Same ready epoch + canonical DAA, but a BOGUS target_hash (not this chain's anchor).
    let bogus_target = Hash64::from_bytes([0xdeu8; 64]);
    assert_ne!(bogus_target, anchor.anchor_hash);
    let att = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        anchor.epoch,
        bogus_target,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_tx = dns_harness::funded_signed_shard_tx(seed, coinbase_b, value_b, daa_b, att, storage_mass_parameter);
    let reward_block = ctx.mine_block(new_miner_data(), vec![shard_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(reward_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the block still validates — the canonical-gated reward fan-out simply pays nothing for the non-canonical attestation (same in construction + validation)"
    );
    // PR5: the §E reward fan-out is canonical-gated, so the non-canonical attestation earns
    // NO coinbase reward (only output to K would be the §E reward; the miner is a different spk).
    let reward_to_validator = reward_block.transactions[0].outputs.iter().find(|o| o.script_public_key == k_spk).map(|o| o.value);
    assert_eq!(reward_to_validator, None, "a non-canonical attestation earns no §E reward (PR5)");

    ctx.mine_block(new_miner_data(), vec![]).await;
    ctx.mine_block(new_miner_data(), vec![]).await;

    let new_sink = ctx.consensus.get_sink();
    let (contributions, denom) = {
        let vp = ctx.consensus.virtual_processor();
        let bonds: Vec<_> = vp.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
        vp.collect_stake_contributions_v2(new_sink, None, &bonds, genesis_hash.as_byte_slice(), &dns, ContributionWeight::BondedStake)
    };

    // The non-canonical attestation also earns NO StakeScore credit (PR4)...
    assert!(contributions.iter().all(|c| c.bond_outpoint != bond_outpoint), "a non-canonical-target attestation must not be credited");
    // ...even though its epoch IS a ready, creditable epoch (present in the denominator).
    assert!(denom.contains_key(&anchor.epoch), "the epoch is ready/creditable; only the non-canonical target is rejected");
}

/// kaspa-pq DNS v3 (PR3) — the signer hands the validator the canonical lagged anchor, NOT
/// the live sink. The singular `get_validator_attestation_target` returns the oldest READY
/// canonical anchor for which the requested bond is Active (matching the hard-inclusion gate's
/// oldest-first backlog order), and the batch `get_validator_attestation_targets` returns every
/// ready, non-duplicate, bond-active epoch ascending up to the latest — so a fallen-behind
/// validator can catch up. Both feed the exact target the PR4 verifier credits.
#[tokio::test]
async fn dns_v3_signer_produces_canonical_ready_targets() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::{Hash64, dns_finality::ready_epoch_from_tip_blue_score};
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let storage = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _, _) = dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, value_a - 100_000, 0, storage);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid);
    let outpoint = TransactionOutpoint::new(bond_tx_id, 0);

    let miner = new_miner_data();
    for _ in 0..20 {
        ctx.mine_block(miner.clone(), vec![]).await;
    }

    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let sink = ctx.consensus.get_sink();

    // The singular target == the first batch target: the oldest ready epoch for which this bond is
    // active at the canonical anchor.
    let target = ctx.consensus.get_validator_attestation_target(outpoint).expect("a ready canonical target");
    let (latest_ready, anchor) = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        (lr, vp.canonical_anchor_by_blue_score(target.epoch, sink, &dns).expect("canonical anchor for the singular target"))
    };
    assert!(target.epoch <= latest_ready, "the singular target is no later than the latest ready epoch");
    assert_eq!(target.target_hash, anchor.anchor_hash, "target is the canonical anchor hash");
    assert_eq!(target.target_daa_score, anchor.anchor_daa_score, "target daa is the canonical anchor daa");
    assert_eq!(target.validator_set_commitment, Hash64::default(), "VSC is a fixed zero (P-1D)");

    // The batch returns every ready, non-duplicate, bond-active epoch ascending up to the latest.
    let targets = ctx.consensus.get_validator_attestation_targets(outpoint, 0, 100);
    assert!(!targets.is_empty());
    assert!(targets.windows(2).all(|w| w[0].epoch < w[1].epoch), "ascending, unique epochs");
    assert_eq!(target.epoch, targets[0].epoch, "singular target follows oldest-first backlog order");
    assert_eq!(targets.last().unwrap().epoch, latest_ready, "the batch reaches the latest ready epoch");
    {
        let vp = ctx.consensus.virtual_processor();
        for t in &targets {
            let a = vp.canonical_anchor_by_blue_score(t.epoch, sink, &dns).expect("each batched epoch has a canonical anchor");
            assert!(!a.duplicate_of_previous_anchor, "duplicate epochs are excluded from the batch");
            assert_eq!(t.target_hash, a.anchor_hash);
            assert_eq!(t.target_daa_score, a.anchor_daa_score);
        }
    }

    // A `from_epoch` past the latest ready epoch yields nothing (no future epochs to sign).
    assert!(ctx.consensus.get_validator_attestation_targets(outpoint, latest_ready + 1, 100).is_empty());
}

/// kaspa-pq DNS v3 (PR6) — high-parallel no-hole: on a WIDE DAG the selected chain's
/// blue_score jumps by the merged-set size, skipping whole epoch [start, end] ranges. Every
/// buried epoch must still resolve to a canonical anchor (the most-recent selected-chain block
/// at-or-below its cutoff — which, for a skipped epoch, is a block below the jump → a
/// correctly-flagged duplicate), NEVER a hole (None / panic). This is the DAG-level analogue of
/// PR2a's pure `no-hole-on-jump` test, exercising the real store walk over a jumpy chain.
#[tokio::test]
async fn dns_v3_high_parallel_blue_score_jump_no_hole() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::dns_finality::ready_epoch_from_tip_blue_score;
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 16;
            p.mergeset_size_limit = 16;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            // Small epochs vs. wide merges (up to 16) so a single merge jumps past whole epochs.
            dns.attestation_epoch_length_blue_score = 5;
            dns.attestation_lag_blue_score = 3;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 100_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Warm up, then alternate WIDE antichains + single merge blocks so the selected chain's
    // blue_score jumps by the merged set size (skipping whole epoch ranges), then settle.
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await;
    }
    for _ in 0..4 {
        ctx.build_block_template_row(0..14).validate_and_insert_row().await; // wide antichain
        ctx.build_block_template_row(0..1).validate_and_insert_row().await; // merge -> blue_score jump
    }
    for _ in 0..6 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await;
    }
    ctx.assert_tips_num(1);

    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let sink = ctx.consensus.get_sink();
    let vp = ctx.consensus.virtual_processor();
    let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
    let latest_ready =
        ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("epochs are ready on a chain this long");
    assert!(latest_ready >= 2, "the chain spans several epochs (latest_ready = {latest_ready})");

    // NO HOLE + monotonic: every buried epoch resolves to a canonical anchor whose blue_score is
    // non-decreasing across epochs, even across the blue_score jumps.
    let mut prev_blue = 0u64;
    let mut distinct = std::collections::HashSet::new();
    for e in 0..=latest_ready {
        let a = vp.canonical_anchor_by_blue_score(e, sink, &dns).unwrap_or_else(|| panic!("epoch {e} has no canonical anchor (hole)"));
        assert!(a.anchor_blue_score >= prev_blue, "anchor blue_score is monotonic across epochs");
        prev_blue = a.anchor_blue_score;
        distinct.insert(a.anchor_hash);
    }
    // The wide merges actually skipped >=1 epoch range: fewer distinct anchors than epochs (some
    // epochs share an anchor) — proving the test exercised real blue_score jumps, not a dense chain.
    assert!(
        distinct.len() <= latest_ready as usize,
        "a blue_score jump made >=1 epoch reuse a prior anchor ({} distinct anchors over {} epochs)",
        distinct.len(),
        latest_ready + 1
    );
}

/// kaspa-pq DNS v3 — the validator FUNCTIONS end-to-end: a bonded validator's canonical
/// attestation drives the StakeScore over `required_stake_depth`, so `update_dns_state`
/// promotes the overlay to the `Active` stage AND records a DNS-confirmed anchor — the
/// precondition the §H reorg gate needs to protect finality. Shrunk params: a single
/// validator is the whole active stake, so one fully-attested ready epoch earns exactly
/// `1·SCALE`, clearing `required_stake_depth = SCALE/2`. (Foundation for the 51%-attack test.)
#[tokio::test]
async fn dns_v3_validator_drives_confirmed_anchor() {
    use crate::model::stores::{dns_state::DnsStateStoreReader, headers::HeaderStoreReader};
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DnsRolloutStage, STAKE_SCORE_SCALE, StakeScore, ready_epoch_from_tip_blue_score},
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap(); // GENESIS_ACTIVE: TwoDimensionalDominance
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            // Confirm on a short chain: work threshold trivial, one fully-attested epoch suffices.
            dns.required_work_depth = kaspa_consensus_core::BlueWorkType::ZERO;
            dns.required_stake_depth = StakeScore(STAKE_SCORE_SCALE / 2);
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // Fund a bond + a shard-funding coinbase.
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _vid, _reward_payload) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, value_a - 100_000, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid);
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);

    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor")
    };
    let att = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        anchor.epoch,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_tx = dns_harness::funded_signed_shard_tx(seed, coinbase_b, value_b, daa_b, att, storage_mass_parameter);
    let reward_block = ctx.mine_block(new_miner_data(), vec![shard_tx]).await;
    assert_eq!(ctx.consensus.block_status(reward_block.header.hash), BlockStatus::StatusUTXOValid);

    // Mine generously so the shard merges (accepted on the selected chain), the attested epoch
    // buries, and update_dns_state recomputes (it throttles to once per blue_score epoch) with
    // the attestation credited -> stake_depth >= required -> the anchor confirms.
    for _ in 0..15 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    let state = {
        let vp = ctx.consensus.virtual_processor();
        vp.dns_state_store.read().get().expect("DnsState is written once the overlay is active")
    };
    assert_eq!(state.rollout_stage, DnsRolloutStage::Active, "one active validator -> Active stage");
    assert!(
        state.stake_depth >= StakeScore(STAKE_SCORE_SCALE / 2),
        "the validator's canonical attestation drove StakeScore over the threshold (got {:?})",
        state.stake_depth
    );
    assert_ne!(
        state.last_dns_confirmed_anchor,
        Hash64::default(),
        "a DNS-confirmed anchor is recorded (the reorg gate now protects it)"
    );
}

/// kaspa-pq DNS-finality (E3/§6.2 template integration) — a refill-capable selector for the
/// template-adoption tests: returns its candidate batches in order (one per
/// `select_transactions` call) and never reports failure on rejection, so a classifier-driven
/// `reject_selection` (an ineligible-shard drop) triggers the builder's refill loop pulling the
/// next batch — exactly the production frontier-selector refill semantics, without a mempool.
struct RefillTxSelector {
    batches: VecDeque<Vec<Transaction>>,
    rejected: Vec<kaspa_consensus_core::tx::TransactionId>,
}

impl RefillTxSelector {
    fn new(batches: Vec<Vec<Transaction>>) -> Self {
        Self { batches: batches.into(), rejected: vec![] }
    }
}

impl TemplateTransactionSelector for RefillTxSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        self.batches.pop_front().unwrap_or_default()
    }
    fn reject_selection(&mut self, tx_id: kaspa_consensus_core::tx::TransactionId) {
        self.rejected.push(tx_id);
    }
    // Infallible from the selector's POV: rejections are refills, not failures. The
    // tests build with `TemplateBuildMode::Infallible` so the rejection never aborts.
    fn is_successful(&self) -> bool {
        true
    }
}

/// Shared setup for the template-adoption tests (E3/§6.2): bond one validator, bury several
/// blue_score epochs so a ready bond-active anchor exists, and return the context plus the
/// validator, the canonical anchor, and TWO matured shard-funding coinbase outpoints (so two
/// distinct funded shards can be built in one test). Mirrors the `dns_v3_*` preamble.
#[cfg(test)]
async fn template_adoption_setup() -> (
    TestContext,
    dns_harness::HarnessValidator,
    TransactionOutpoint,                                            // bond outpoint
    kaspa_consensus_core::dns_finality::CanonicalLaggedEpochAnchor, // canonical anchor for latest ready epoch
    (TransactionOutpoint, u64, u64),                                // shard-funding coinbase #1 (outpoint, value, daa)
    (TransactionOutpoint, u64, u64),                                // shard-funding coinbase #2 (outpoint, value, daa)
    kaspa_consensus_core::dns_finality::DnsParams,
    BlockHash, // genesis hash (net id)
    u64,       // storage_mass_parameter
) {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::dns_finality::ready_epoch_from_tip_blue_score;
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);
    let k_miner = MinerData::new(k_spk.clone(), vec![]);

    // coinbase_a funds the bond; coinbase_b + coinbase_c fund two shards.
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_c = ctx.mine_block(k_miner.clone(), vec![]).await;
    let pick = |h: &Block| {
        let cb = &h.transactions[0];
        let (i, o) = cb.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("pays K");
        (TransactionOutpoint::new(cb.id(), i as u32), o.value, h.header.daa_score)
    };
    let (coinbase_a, value_a, daa_a) = pick(&h_a);
    let cb_b = pick(&h_b);
    let cb_c = pick(&h_c);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _vid, _rp) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, value_a - 100_000, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid);
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor")
    };
    (ctx, v, bond_outpoint, anchor, cb_b, cb_c, dns, genesis_hash, storage_mass_parameter)
}

/// kaspa-pq DNS-finality (E3/§6.2, test T3): a mempool-submitted ELIGIBLE attestation shard
/// appears as a non-coinbase tx in `build_block_template` output (the construction path now
/// classifies + keeps eligible shards at selection time instead of dropping them late).
#[tokio::test]
async fn t3_eligible_shard_in_block_template() {
    use kaspa_consensus_core::Hash64;
    let (ctx, v, bond_outpoint, anchor, cb_b, _cb_c, dns, genesis_hash, smp) = template_adoption_setup().await;
    let att = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        anchor.epoch,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_tx = dns_harness::funded_signed_shard_tx(v.seed, cb_b.0, cb_b.1, cb_b.2, att, smp);
    let shard_id = shard_tx.id();

    let template = ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(vec![shard_tx])), TemplateBuildMode::Standard)
        .expect("template builds with the eligible shard");
    // The eligible shard is included as a non-coinbase tx.
    assert!(
        template.block.transactions.iter().skip(1).any(|t| t.id() == shard_id),
        "the eligible attestation shard must appear in the template"
    );
    // T5 (fee alignment): calculated_fees stays 1:1 with the non-coinbase txs.
    assert_eq!(
        template.calculated_fees.len(),
        template.block.transactions.len() - 1,
        "calculated_fees must be 1:1 with the non-coinbase txs"
    );
    let _ = dns; // params used by setup; silence unused on some builds
}

/// kaspa-pq DNS-finality (E3/§6.2, test T4): an INELIGIBLE shard selected first is rejected
/// (refilled) and an ELIGIBLE shard from the next batch is included instead. Uses the
/// refill-capable selector + `Infallible` build so the classifier drop triggers a refill, not
/// a build failure.
#[tokio::test]
async fn t4_ineligible_shard_refilled_with_eligible() {
    use kaspa_consensus_core::Hash64;
    let (ctx, v, bond_outpoint, anchor, cb_b, cb_c, _dns, genesis_hash, smp) = template_adoption_setup().await;

    // Ineligible: correct bond + signature but a WRONG self-declared validator_id (P-1A
    // mismatch) ⇒ classifier `Drop(ValidatorIdMismatch)`. Still a structurally valid funded tx
    // (so it passes block-template tx validation and reaches the classifier).
    let mut bad_att = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        anchor.epoch,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    bad_att.validator_id = Hash64::from_bytes([0xff; 64]); // ≠ bond.validator_pubkey_hash
    let bad_shard = dns_harness::funded_signed_shard_tx(v.seed, cb_b.0, cb_b.1, cb_b.2, bad_att, smp);
    let bad_id = bad_shard.id();

    // Eligible: correct id + signature + canonical anchor.
    let good_att = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        anchor.epoch,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let good_shard = dns_harness::funded_signed_shard_tx(v.seed, cb_c.0, cb_c.1, cb_c.2, good_att, smp);
    let good_id = good_shard.id();

    // Batch 1 = the ineligible shard (dropped+refilled); batch 2 = the eligible shard (kept).
    let selector = RefillTxSelector::new(vec![vec![bad_shard], vec![good_shard]]);
    let template = ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(selector), TemplateBuildMode::Infallible)
        .expect("template builds (infallible)");

    let ids: Vec<_> = template.block.transactions.iter().skip(1).map(|t| t.id()).collect();
    assert!(!ids.contains(&bad_id), "the ineligible shard must be dropped from the template");
    assert!(ids.contains(&good_id), "the eligible refill shard must be included instead");
    // T5 (fee alignment) after a drop+refill: still 1:1.
    assert_eq!(
        template.calculated_fees.len(),
        template.block.transactions.len() - 1,
        "calculated_fees must stay 1:1 with the non-coinbase txs after a drop+refill"
    );
}

/// kaspa-pq DNS-finality (P1, duplicate-epoch credit regression): the SAME (bond, epoch)
/// attestation accepted TWICE on the selected chain is credited to StakeScore only ONCE
/// (`collect_stake_contributions_v2` dedups by the canonical-anchor gate + the
/// `aggregate_epoch_tallies` per-(bond,epoch) collapse). This pins the existing — and, as the
/// investigation found, already-correct — behavior so a future change cannot start double-crediting.
#[tokio::test]
async fn duplicate_epoch_attestation_credited_once() {
    use crate::model::stores::stake_bonds::StakeBondsStoreReader;
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{aggregate_epoch_tallies, total_active_stake_by_epoch},
    };
    let (mut ctx, v, bond_outpoint, anchor, cb_b, cb_c, dns, genesis_hash, smp) = template_adoption_setup().await;

    // Two shards naming the SAME canonical (epoch, anchor) for the SAME bond, funded from two
    // distinct coinbases so both are structurally valid + can both be mined/accepted.
    let mk = |cb: (TransactionOutpoint, u64, u64)| {
        let att = dns_harness::build_signed_attestation(
            &v,
            genesis_hash.as_byte_slice(),
            bond_outpoint,
            anchor.epoch,
            anchor.anchor_hash,
            anchor.anchor_daa_score,
            Hash64::default(),
        );
        dns_harness::funded_signed_shard_tx(v.seed, cb.0, cb.1, cb.2, att, smp)
    };
    let shard1 = mk(cb_b);
    let shard2 = mk(cb_c);
    let b1 = ctx.mine_block(new_miner_data(), vec![shard1]).await;
    assert_eq!(ctx.consensus.block_status(b1.header.hash), BlockStatus::StatusUTXOValid);
    let b2 = ctx.mine_block(new_miner_data(), vec![shard2]).await;
    assert_eq!(ctx.consensus.block_status(b2.header.hash), BlockStatus::StatusUTXOValid);
    // Bury so both merge onto the selected chain.
    for _ in 0..4 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    let new_sink = ctx.consensus.get_sink();
    let vp = ctx.consensus.virtual_processor();
    let bonds: Vec<_> = vp.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
    let (contributions, denom) =
        vp.collect_stake_contributions_v2(new_sink, None, &bonds, genesis_hash.as_byte_slice(), &dns, ContributionWeight::BondedStake);

    // The (bond, epoch) pair is credited at most once even though two shards carry it.
    let credited_for_epoch = contributions.iter().filter(|c| c.bond_outpoint == bond_outpoint && c.epoch == anchor.epoch).count();
    assert!(credited_for_epoch >= 1, "the canonical attestation is credited at least once");
    // After the per-(bond,epoch) aggregation, the signed stake for this epoch equals the bond's
    // stake exactly ONCE (no double-count from the duplicate shard).
    let totals = total_active_stake_by_epoch(&bonds, &denom, kaspa_consensus_core::dns_finality::InactivityLeakViewV1::none());
    let per_epoch = aggregate_epoch_tallies(&contributions, &totals);
    let bond_amount = bonds.iter().find(|b| b.bond_outpoint == bond_outpoint).expect("bond persisted").amount;
    let tally = per_epoch.iter().find(|t| t.epoch == anchor.epoch).expect("the epoch is tallied");
    assert_eq!(
        tally.signed_weight, bond_amount as u128,
        "the duplicate (bond, epoch) is credited exactly once (signed stake == one bond's stake, got {})",
        tally.signed_weight
    );
}

/// kaspa-pq DNS v3 (§H finality) — **51%-PoW attack is stopped**: a stake-less attacker that
/// out-mines the honest chain (strictly higher blue_work — a PoW majority) CANNOT rewrite a
/// DNS-confirmed anchor. The honest node bonds a validator and reaches a confirmed anchor;
/// a second consensus instance (the attacker, same genesis) mines a longer STAKE-LESS chain;
/// its heavier blocks are delivered to the honest node, whose sink-search runs the
/// `TwoDimensionalDominance` gate (`dns_reorg_allows`): the candidate exits the confirmed
/// prefix and out-Works but does NOT out-Stake (zero attestations) → `DominanceViolation` →
/// soft-reject. The honest sink therefore STILL contains the confirmed anchor, never the
/// heavier attacker tip — PoW surplus does not substitute for a PoS deficit (the
/// non-substitutability finality property). Completes the PR6-deferred 51%-finality-stop sim.
#[tokio::test]
async fn dns_v3_pow_majority_cannot_rewrite_confirmed_anchor() {
    use crate::model::stores::{dns_state::DnsStateStoreReader, ghostdag::GhostdagStoreReader, headers::HeaderStoreReader};
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DnsRolloutStage, STAKE_SCORE_SCALE, StakeScore, ready_epoch_from_tip_blue_score},
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap(); // GENESIS_ACTIVE: TwoDimensionalDominance
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            // Large reorg horizon so a from-genesis fork is GATE-ELIGIBLE (the dominance test
            // runs) instead of being auto-rejected as deeper than the horizon.
            dns.max_reorg_horizon_blocks = 1000;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            dns.required_work_depth = kaspa_consensus_core::BlueWorkType::ZERO;
            dns.required_stake_depth = StakeScore(STAKE_SCORE_SCALE / 2);
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // ---- Honest node: bond a validator, attest, reach a DNS-confirmed anchor. ----
    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _vid, _rp) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, value_a - 100_000, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid);
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor")
    };
    let att = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        anchor.epoch,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_tx = dns_harness::funded_signed_shard_tx(seed, coinbase_b, value_b, daa_b, att, storage_mass_parameter);
    ctx.mine_block(new_miner_data(), vec![shard_tx]).await;
    for _ in 0..15 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    let honest_sink = ctx.consensus.get_sink();
    let (confirmed_anchor, honest_work) = {
        let vp = ctx.consensus.virtual_processor();
        let st = vp.dns_state_store.read().get().expect("DnsState");
        assert_eq!(st.rollout_stage, DnsRolloutStage::Active, "honest node is Active");
        assert_ne!(st.last_dns_confirmed_anchor, Hash64::default(), "honest node has a confirmed anchor");
        (st.last_dns_confirmed_anchor, vp.ghostdag_store.get_blue_work(honest_sink).unwrap())
    };

    // ---- Attacker: a SEPARATE instance (same genesis) mines a longer STAKE-LESS chain. ----
    let mut atk = TestContext::new(TestConsensus::new(&config));
    let mut attacker_blocks = Vec::new();
    for _ in 0..60 {
        attacker_blocks.push(atk.mine_block(new_miner_data(), vec![]).await);
    }
    let attacker_tip = attacker_blocks.last().unwrap().header.hash;
    let attacker_work = { atk.consensus.virtual_processor().ghostdag_store.get_blue_work(attacker_tip).unwrap() };
    assert!(
        attacker_work > honest_work,
        "the attacker is a genuine PoW majority (heavier blue_work): attacker {attacker_work} vs honest {honest_work}"
    );

    // ---- Deliver the attacker's heavier branch to the honest node. ----
    for b in &attacker_blocks {
        ctx.validate_and_insert_block(b.clone()).await;
    }

    // ---- Finality held: the honest sink STILL contains the confirmed anchor, NOT the heavier
    //      attacker tip. PoW surplus could not substitute for the attacker's zero stake. ----
    let new_sink = ctx.consensus.get_sink();
    assert_ne!(new_sink, attacker_tip, "the honest node did NOT reorg onto the heavier stake-less attacker chain");
    {
        let vp = ctx.consensus.virtual_processor();
        assert!(
            vp.reachability_service.is_chain_ancestor_of(confirmed_anchor, new_sink),
            "the DNS-confirmed anchor is still on the selected chain (the reorg gate stopped the 51% attack)"
        );
        // The confirmed anchor is unchanged — finality was not rewritten.
        let st = vp.dns_state_store.read().get().expect("DnsState");
        assert_eq!(st.last_dns_confirmed_anchor, confirmed_anchor, "the confirmed anchor was not rewritten by the attack");
    }
}

/// kaspa-pq DNS v3 — **many validators converge on ONE anchor at the epoch boundary**, the
/// core reason v3 replaces v1 current-sink signing. Under fast mining / a wide DAG, validators
/// transiently observe DIFFERENT sinks (the multi-tip frontier + propagation lag) — v1 had each
/// sign its own differing sink, splitting honest stake below φS. Here we build a wide DAG, take
/// many divergent validator VIEWS (the multiple frontier tips a fast network produces + lagging
/// ancestors at different heights), and show that although their views differ (≥2 distinct
/// blocks → ≥2 distinct v1 sink-targets), every one of them computes the SAME v3 canonical
/// lagged anchor for a buried epoch (exactly 1) — unanimous, so honest stake never splits.
#[tokio::test]
async fn dns_v3_many_validators_agree_on_anchor_under_fast_wide_dag() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::dns_finality::ready_epoch_from_tip_blue_score;
    use std::collections::HashSet;
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 16;
            p.mergeset_size_limit = 16;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            // Moderate epoch + a generous lag so the latest ready epoch's anchor is buried well
            // below the churning multi-tip frontier (where the views diverge) into shared history.
            dns.attestation_epoch_length_blue_score = 5;
            dns.attestation_lag_blue_score = 20;
            dns.attestation_anchor_backoff_blue_score = 2;
            dns.stake_score_window_blue_score = 100_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Fast mining / wide DAG: wide antichains merged repeatedly, ENDING on a wide antichain so the
    // frontier is genuinely multi-tip (the different sinks a fast network's validators observe).
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await;
    }
    for _ in 0..6 {
        ctx.build_block_template_row(0..12).validate_and_insert_row().await; // wide antichain
        ctx.build_block_template_row(0..1).validate_and_insert_row().await; // merge -> blue_score jump
    }
    ctx.build_block_template_row(0..12).validate_and_insert_row().await; // leave a multi-tip frontier

    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let sink = ctx.consensus.get_sink();

    // Collect many divergent VALIDATOR VIEWS: every frontier tip (a fast network's competing
    // sinks) + several RECENT lagging ancestors at different heights (validators a little behind
    // on propagation — but still past the readiness threshold, like real honest nodes).
    let mut views: Vec<BlockHash> = ctx.consensus.get_tips().into_iter().collect();
    {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        for anc in vp.reachability_service.default_backward_chain_iterator(sink) {
            let b = vp.headers_store.get_blue_score(anc).unwrap();
            // Only "slightly behind" validators (recent ancestors); stop once we'd reach nodes too
            // far back to have a ready epoch (a genesis-deep view is not a realistic poll state).
            if sink_blue.saturating_sub(b) > 40
                || ready_epoch_from_tip_blue_score(b, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
                    .is_none()
            {
                break;
            }
            views.push(anc);
            if views.len() >= 24 {
                break;
            }
        }
    }
    views.sort();
    views.dedup();

    let (anchors, blue_scores, buried_epoch) = {
        let vp = ctx.consensus.virtual_processor();
        // A buried epoch every view agrees is ready: the min over views of each view's latest
        // ready epoch (so canonical_anchor_by_blue_score returns Some for every view).
        let buried_epoch = views
            .iter()
            .map(|t| {
                let b = vp.headers_store.get_blue_score(*t).unwrap();
                ready_epoch_from_tip_blue_score(b, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
                    .expect("each view has at least one ready epoch")
            })
            .min()
            .unwrap();
        let anchors: HashSet<BlockHash> = views
            .iter()
            .map(|t| {
                vp.canonical_anchor_by_blue_score(buried_epoch, *t, &dns).expect("every view resolves the buried epoch").anchor_hash
            })
            .collect();
        let blue_scores: HashSet<u64> = views.iter().map(|t| vp.headers_store.get_blue_score(*t).unwrap()).collect();
        (anchors, blue_scores, buried_epoch)
    };

    // The views are genuinely divergent (a fast network: many distinct tips at several heights) —
    // under v1 these would be ≥2 different current-sink targets, splitting honest stake.
    assert!(views.len() >= 5, "many validator views ({})", views.len());
    assert!(blue_scores.len() >= 2, "the views sit at genuinely different positions (would be different v1 sinks)");
    // ...yet under v3 every view computes the SAME canonical anchor for the buried epoch.
    assert_eq!(
        anchors.len(),
        1,
        "all {} validator views must agree on ONE canonical anchor for epoch {} (got {} distinct)",
        views.len(),
        buried_epoch,
        anchors.len()
    );
}

/// kaspa-pq Layer-0 (audit M-3, updated for ADR-0007 Phase 3): a header whose
/// `pow_algo_id` is not the algo the network mandates at its DAA score is
/// rejected by header-in-isolation validation. On the BLAKE2b-SHA3-active mainnet
/// params the mandated id is `3`, so both the wrong-but-known Phase-1 id (`1` —
/// a miner trying the cheap kHeavyHash on a BLAKE2b-SHA3 network) and a garbage id
/// (`99`) must be rejected, before the PoW seed — which consumes algo_id — is
/// even derived.
#[tokio::test]
async fn header_with_unknown_pow_algo_id_is_rejected() {
    use kaspa_consensus_core::errors::block::RuleError;
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    // Establish a virtual chain with one valid (template-built ⇒ correct algo id) block.
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();

    // Corrupt the algo id to the wrong-but-known Phase-1 id and re-finalize.
    let mut t = ctx.build_block_template(0, ctx.simulated_time + 1_000);
    t.block.header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH;
    t.block.header.finalize();
    let res = ctx.consensus.validate_and_insert_block(t.block.to_immutable()).block_task.await;
    assert!(matches!(res, Err(RuleError::UnknownPowAlgoId(1))), "expected UnknownPowAlgoId(1), got {res:?}");

    // A garbage id is rejected the same way.
    let mut t = ctx.build_block_template(0, ctx.simulated_time + 2_000);
    t.block.header.pow_algo_id = 99;
    t.block.header.finalize();
    let res = ctx.consensus.validate_and_insert_block(t.block.to_immutable()).block_task.await;
    assert!(matches!(res, Err(RuleError::UnknownPowAlgoId(99))), "expected UnknownPowAlgoId(99), got {res:?}");
}

// ============================================================================
// kaspa-pq ADR-0018 §G — DNS-overlay DAG integration harness (foundation).
//
// Retires the "ML-DSA-87 signing unavailable in the consensus test crate"
// blocker for the reward-bearing / reorg / slashing DAG tests (DAG-2..7): these
// helpers let a consensus test build stake-bond + attestation-shard txs and
// produce an attestation signature the §B.4 verifier
// (`kaspa_txscript::verify_mldsa87_with_context` under
// `ATTESTATION_MLDSA87_CONTEXT`) accepts. Funding a bond tx from a coinbase UTXO
// (so a full reward-bearing chain validates) is the next harness step (DAG-2).
// ============================================================================
#[cfg(test)]
mod dns_harness {
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{
            ATTESTATION_MLDSA87_CONTEXT, DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload, StakeAttestation, StakeBondPayload,
            StakeUnbondRequestPayload, UNBOND_REQUEST_CONTEXT, attestations_from_accepted_txs, p2pkh_mldsa87_spk,
            single_attestation_shard, stake_attestation_message, stake_attestation_shard_tx, unbond_request_message,
            validator_id_from_pubkey,
        },
        hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash},
        hashing::sighash_type::SIG_HASH_ALL,
        mass::MassCalculator,
        subnets::{
            SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_SLASHING_EVIDENCE, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, SUBNETWORK_ID_STAKE_BOND,
            SUBNETWORK_ID_STAKE_UNBOND,
        },
        tx::{PopulatedTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
    };
    use kaspa_txscript::{MLDSA87_TX_CONTEXT, script_builder::ScriptBuilder};
    use libcrux_ml_dsa::ml_dsa_87 as mldsa;

    /// A test validator: an ML-DSA-87 key (re-derived deterministically from
    /// `seed`) plus its 2592-byte pubkey and overlay `validator_id`.
    pub(super) struct HarnessValidator {
        pub seed: [u8; 32],
        pub pubkey: Vec<u8>,
        pub validator_id: Hash64,
    }

    pub(super) fn harness_validator(seed: [u8; 32]) -> HarnessValidator {
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let validator_id = validator_id_from_pubkey(&pubkey);
        HarnessValidator { seed, pubkey, validator_id }
    }

    /// Build a stake-bond tx (subnetwork `SUBNETWORK_ID_STAKE_BOND`, payload =
    /// borsh `StakeBondPayload`). The funded variant (output-0 = `amount` locked
    /// stake spent from a coinbase UTXO) is the next step; here the tx is
    /// payload-first for shape / borsh checks.
    pub(super) fn build_stake_bond_tx(
        v: &HarnessValidator,
        amount: u64,
        activation_daa_score: u64,
        reward_payload: [u8; 64],
    ) -> Transaction {
        let payload = StakeBondPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            owner_pubkey_hash: v.validator_id,
            validator_pubkey_hash: v.validator_id,
            validator_pubkey: v.pubkey.clone(),
            amount,
            activation_daa_score,
            unbonding_period_blocks: 700,
            owner_reward_spk_payload: reward_payload,
        };
        Transaction::new(
            crate::constants::TX_VERSION,
            vec![],
            vec![],
            0,
            SUBNETWORK_ID_STAKE_BOND,
            0,
            borsh::to_vec(&payload).unwrap(),
        )
    }

    /// kaspa-pq ADR-0018 §G (DAG-2): build a FUNDED, ML-DSA-87-signed stake-bond tx.
    /// Spends the matured coinbase UTXO `coinbase_outpoint` (value `coinbase_value`,
    /// paid to this validator's own P2PKH) into output-0 = `amount` locked stake
    /// (P2PKH to the same key), carrying the `StakeBondPayload`. Input-0 is signed
    /// over `calc_mldsa87_signature_hash(.., SIG_HASH_ALL)` under `MLDSA87_TX_CONTEXT`
    /// — the exact 64-byte digest `OpCheckSigMlDsa87` recomputes — so the block
    /// validates through the full script engine (construction == validation).
    /// Returns `(signed tx, validator_id, owner_reward_spk_payload)`.
    pub(super) fn funded_signed_bond_tx(
        seed: [u8; 32],
        coinbase_outpoint: TransactionOutpoint,
        coinbase_value: u64,
        coinbase_daa_score: u64,
        amount: u64,
        activation_daa_score: u64,
        storage_mass_parameter: u64,
    ) -> (Transaction, Hash64, [u8; 64]) {
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let validator_id = validator_id_from_pubkey(&pubkey);
        // Keyed BLAKE2b-512 address payload (the same digest the spk's OP_BLAKE2B_512
        // recomputes); rewards + the locked-stake output both pay this P2PKH.
        let reward_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes();
        let spk = p2pkh_mldsa87_spk(&reward_payload);

        let payload = StakeBondPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            owner_pubkey_hash: validator_id,
            validator_pubkey_hash: validator_id,
            validator_pubkey: pubkey.clone(),
            amount,
            activation_daa_score,
            unbonding_period_blocks: 700,
            owner_reward_spk_payload: reward_payload,
        };
        // input-0 spends the coinbase; output-0 = the locked stake; fee = coinbase_value - amount.
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(coinbase_outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(amount, spk.clone())],
            0,
            SUBNETWORK_ID_STAKE_BOND,
            0,
            borsh::to_vec(&payload).unwrap(),
        );

        // KIP-9 storage-mass commitment: value-based, so independent of the (still
        // empty) signature_script — committing it now matches the validator's
        // `calc_contextual_masses(..).storage_mass` recheck (else WrongMass).
        let utxo = UtxoEntry::new(coinbase_value, spk, coinbase_daa_score, true);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![utxo.clone()]))
            .expect("contextual mass is computable for the funded bond tx")
            .storage_mass;
        tx.set_mass(storage_mass);

        // Sign input-0 over the SIG_HASH_ALL digest of the (mass-committed) tx.
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sig_hash = {
            let populated = PopulatedTransaction::new(&tx, vec![utxo]);
            calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_ALL, &reused)
        };
        let sig = mldsa::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0x77u8; 32])
            .expect("ML-DSA-87 sign on the 64-byte sighash");
        let mut sig_item = sig.as_ref().to_vec();
        sig_item.push(SIG_HASH_ALL.to_u8());
        let sig_script = ScriptBuilder::new()
            .add_data(&sig_item)
            .expect("ML-DSA-87 signature push fits MAX_SCRIPT_ELEMENT_SIZE")
            .add_data(&pubkey)
            .expect("ML-DSA-87 public-key push fits MAX_SCRIPT_ELEMENT_SIZE")
            .drain();
        tx.inputs[0].signature_script = sig_script;
        (tx, validator_id, reward_payload)
    }

    /// kaspa-pq ADR-0018 §G (DAG-2): build a FUNDED, ML-DSA-87-signed attestation
    /// shard tx — the production shape (`build_funded_shard_tx`). A canonical 0-input
    /// shard tx is rejected by the isolation `NoTxInputs` check, so the shard must
    /// spend a (matured) coinbase like any other tx: one P2PKH change output back to
    /// the same key, with the attestation carried verbatim in the payload on
    /// `SUBNETWORK_ID_STAKE_ATTESTATION_SHARD`. Input-0 is ML-DSA-signed over the v2
    /// tx sighash; the storage mass is committed.
    pub(super) fn funded_signed_shard_tx(
        seed: [u8; 32],
        coinbase_outpoint: TransactionOutpoint,
        coinbase_value: u64,
        coinbase_daa_score: u64,
        attestation: StakeAttestation,
        storage_mass_parameter: u64,
    ) -> Transaction {
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let reward_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes();
        let spk = p2pkh_mldsa87_spk(&reward_payload);
        // The payload is exactly what the canonical zero-input shard builder emits.
        let payload = stake_attestation_shard_tx(&single_attestation_shard(attestation)).payload;
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(coinbase_outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(coinbase_value - 100_000, spk.clone())],
            0,
            SUBNETWORK_ID_STAKE_ATTESTATION_SHARD,
            0,
            payload,
        );
        let utxo = UtxoEntry::new(coinbase_value, spk, coinbase_daa_score, true);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![utxo.clone()]))
            .expect("contextual mass is computable for the funded shard tx")
            .storage_mass;
        tx.set_mass(storage_mass);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sig_hash = {
            let populated = PopulatedTransaction::new(&tx, vec![utxo]);
            calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_ALL, &reused)
        };
        let sig = mldsa::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0x88u8; 32])
            .expect("ML-DSA-87 sign on the 64-byte sighash");
        let mut sig_item = sig.as_ref().to_vec();
        sig_item.push(SIG_HASH_ALL.to_u8());
        let sig_script = ScriptBuilder::new()
            .add_data(&sig_item)
            .expect("ML-DSA-87 signature push fits MAX_SCRIPT_ELEMENT_SIZE")
            .add_data(&pubkey)
            .expect("ML-DSA-87 public-key push fits MAX_SCRIPT_ELEMENT_SIZE")
            .drain();
        tx.inputs[0].signature_script = sig_script;
        tx
    }

    /// kaspa-pq ADR-0018 §G (DAG-6): build a FUNDED, ML-DSA-87-signed slashing-evidence
    /// tx. Spends the matured coinbase `coinbase_outpoint` (paid to this key's P2PKH) with
    /// **no outputs** — the reporter reward is minted by consensus as a side-effect at
    /// `(tx, 0)` (ADR-0013 Addendum C.2), so any declared output would collide with the
    /// mint — carrying the `SlashingEvidencePayload` on `SUBNETWORK_ID_SLASHING_EVIDENCE`.
    /// Input-0 is ML-DSA-87-signed over the v2 sighash and the storage mass is committed,
    /// so the block validates through the full script engine (construction == validation).
    pub(super) fn funded_signed_slashing_evidence_tx(
        seed: [u8; 32],
        coinbase_outpoint: TransactionOutpoint,
        coinbase_value: u64,
        coinbase_daa_score: u64,
        evidence: SlashingEvidencePayload,
        storage_mass_parameter: u64,
    ) -> Transaction {
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let reward_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes();
        let spk = p2pkh_mldsa87_spk(&reward_payload);
        // Evidence tx: input-0 funds it (the entire value becomes fee), NO outputs.
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(coinbase_outpoint, vec![], 0, 1)],
            vec![],
            0,
            SUBNETWORK_ID_SLASHING_EVIDENCE,
            0,
            borsh::to_vec(&evidence).unwrap(),
        );
        let utxo = UtxoEntry::new(coinbase_value, spk, coinbase_daa_score, true);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![utxo.clone()]))
            .expect("contextual mass is computable for the funded slashing-evidence tx")
            .storage_mass;
        tx.set_mass(storage_mass);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sig_hash = {
            let populated = PopulatedTransaction::new(&tx, vec![utxo]);
            calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_ALL, &reused)
        };
        let sig = mldsa::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0x99u8; 32])
            .expect("ML-DSA-87 sign on the 64-byte sighash");
        let mut sig_item = sig.as_ref().to_vec();
        sig_item.push(SIG_HASH_ALL.to_u8());
        let sig_script = ScriptBuilder::new()
            .add_data(&sig_item)
            .expect("ML-DSA-87 signature push fits MAX_SCRIPT_ELEMENT_SIZE")
            .add_data(&pubkey)
            .expect("ML-DSA-87 public-key push fits MAX_SCRIPT_ELEMENT_SIZE")
            .drain();
        tx.inputs[0].signature_script = sig_script;
        tx
    }

    /// kaspa-pq H-06 (unbond lifecycle): build a FUNDED, ML-DSA-87-signed stake-unbond
    /// request tx — the client-side shape an operator submits to exit a bond. Spends the
    /// matured coinbase into one P2PKH change output (subnet `SUBNETWORK_ID_STAKE_UNBOND`),
    /// carrying a `StakeUnbondRequestPayload` whose `signature` is the owner's ML-DSA-87
    /// signature over `unbond_request_message(bond)` under `UNBOND_REQUEST_CONTEXT`
    /// (the digest the stateful `unbond_request_authorized` rule reconstructs). Input-0 is
    /// signed over the v2 tx sighash so the block validates through the script engine.
    pub(super) fn funded_signed_unbond_tx(
        seed: [u8; 32],
        net_id: &[u8],
        coinbase_outpoint: TransactionOutpoint,
        coinbase_value: u64,
        coinbase_daa_score: u64,
        bond_outpoint: TransactionOutpoint,
        storage_mass_parameter: u64,
    ) -> Transaction {
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let reward_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes();
        let spk = p2pkh_mldsa87_spk(&reward_payload);
        // Owner authorization: ML-DSA-87 signature over the network- and bond-bound unbond message (M-04).
        let auth_digest = unbond_request_message(net_id, bond_outpoint);
        let auth_sig = mldsa::sign(&kp.signing_key, &auth_digest.as_bytes()[..], UNBOND_REQUEST_CONTEXT, [0xaau8; 32])
            .expect("ML-DSA-87 unbond authorization sign");
        let payload = borsh::to_vec(&StakeUnbondRequestPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            bond_outpoint,
            owner_pubkey: pubkey.clone(),
            signature: auth_sig.as_ref().to_vec(),
        })
        .unwrap();
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(coinbase_outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(coinbase_value - 100_000, spk.clone())],
            0,
            SUBNETWORK_ID_STAKE_UNBOND,
            0,
            payload,
        );
        let utxo = UtxoEntry::new(coinbase_value, spk, coinbase_daa_score, true);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![utxo.clone()]))
            .expect("contextual mass is computable for the funded unbond tx")
            .storage_mass;
        tx.set_mass(storage_mass);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sig_hash = {
            let populated = PopulatedTransaction::new(&tx, vec![utxo]);
            calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_ALL, &reused)
        };
        let sig = mldsa::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0xabu8; 32])
            .expect("ML-DSA-87 sign on the 64-byte sighash");
        let mut sig_item = sig.as_ref().to_vec();
        sig_item.push(SIG_HASH_ALL.to_u8());
        let sig_script = ScriptBuilder::new()
            .add_data(&sig_item)
            .expect("ML-DSA-87 signature push fits MAX_SCRIPT_ELEMENT_SIZE")
            .add_data(&pubkey)
            .expect("ML-DSA-87 public-key push fits MAX_SCRIPT_ELEMENT_SIZE")
            .drain();
        tx.inputs[0].signature_script = sig_script;
        tx
    }

    /// Build a FUNDED, ML-DSA-87-signed NATIVE spend of a P2PKH UTXO (e.g. a bond's locked
    /// output-0) back to the same key. Exercises the ADR-0016 §D.2 bond-UTXO spend-gate: consensus
    /// must reject a block that spends a still-locked (non-releasable) bond output. The spent output
    /// is a regular (non-coinbase) tx output; the sighash commits its value + spk (both supplied),
    /// so the signature verifies through the full script engine.
    pub(super) fn funded_signed_p2pkh_spend(
        seed: [u8; 32],
        outpoint: TransactionOutpoint,
        value: u64,
        daa_score: u64,
        storage_mass_parameter: u64,
    ) -> Transaction {
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let reward_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes();
        let spk = p2pkh_mldsa87_spk(&reward_payload);
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(value - 100_000, spk.clone())],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let utxo = UtxoEntry::new(value, spk, daa_score, false);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![utxo.clone()]))
            .expect("contextual mass is computable for the bond-output spend")
            .storage_mass;
        tx.set_mass(storage_mass);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sig_hash = {
            let populated = PopulatedTransaction::new(&tx, vec![utxo]);
            calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_ALL, &reused)
        };
        let sig = mldsa::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0xacu8; 32])
            .expect("ML-DSA-87 sign on the 64-byte sighash");
        let mut sig_item = sig.as_ref().to_vec();
        sig_item.push(SIG_HASH_ALL.to_u8());
        let sig_script = ScriptBuilder::new()
            .add_data(&sig_item)
            .expect("ML-DSA-87 signature push fits MAX_SCRIPT_ELEMENT_SIZE")
            .add_data(&pubkey)
            .expect("ML-DSA-87 public-key push fits MAX_SCRIPT_ELEMENT_SIZE")
            .drain();
        tx.inputs[0].signature_script = sig_script;
        tx
    }

    /// ADR-0018 §F bridge wiring: a fully ML-DSA-87-signed BRIDGE tx — spends `seed`'s
    /// P2PKH `outpoint` into a single `EVM_DEPOSIT_LOCK` output (`value − 100_000`; fee
    /// 100_000), whose refund path is the same key's P2PKH. Mirrors
    /// [`funded_signed_p2pkh_spend`]; the lock output makes the tx **finality-class**
    /// past `finality_fee_activation_daa_score`.
    pub(super) fn funded_signed_deposit_lock_tx(
        seed: [u8; 32],
        outpoint: TransactionOutpoint,
        value: u64,
        daa_score: u64,
        storage_mass_parameter: u64,
    ) -> Transaction {
        use kaspa_txscript::script_class::evm_deposit_lock_script;
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let reward_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes();
        let spk = p2pkh_mldsa87_spk(&reward_payload);
        // The deposit lock: 20-byte EVM address, far-future timeout (refund path never taken
        // here), small claim tip; refund = the spender's own ML-DSA P2PKH.
        let lock_spk = evm_deposit_lock_script([0xABu8; 20], 100_000_000, 7, spk.script());
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(value - 100_000, lock_spk)],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let utxo = UtxoEntry::new(value, spk, daa_score, false);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![utxo.clone()]))
            .expect("contextual mass is computable for the deposit-lock spend")
            .storage_mass;
        tx.set_mass(storage_mass);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sig_hash = {
            let populated = PopulatedTransaction::new(&tx, vec![utxo]);
            calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_ALL, &reused)
        };
        let sig = mldsa::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0xadu8; 32])
            .expect("ML-DSA-87 sign on the 64-byte sighash");
        let mut sig_item = sig.as_ref().to_vec();
        sig_item.push(SIG_HASH_ALL.to_u8());
        let sig_script = ScriptBuilder::new()
            .add_data(&sig_item)
            .expect("ML-DSA-87 signature push fits MAX_SCRIPT_ELEMENT_SIZE")
            .add_data(&pubkey)
            .expect("ML-DSA-87 public-key push fits MAX_SCRIPT_ELEMENT_SIZE")
            .drain();
        tx.inputs[0].signature_script = sig_script;
        tx
    }

    /// Build a fully ML-DSA-87-signed attestation for `bond_outpoint`, signing
    /// exactly the digest the §B.4 verifier reconstructs.
    pub(super) fn build_signed_attestation(
        v: &HarnessValidator,
        network_id: &[u8],
        bond_outpoint: TransactionOutpoint,
        epoch: u64,
        target_hash: Hash64,
        target_daa_score: u64,
        validator_set_commitment: Hash64,
    ) -> StakeAttestation {
        let msg = stake_attestation_message(network_id, epoch, target_hash, target_daa_score, validator_set_commitment, bond_outpoint);
        let mb = msg.as_bytes();
        let kp = mldsa::generate_key_pair(v.seed);
        let sig = mldsa::sign(&kp.signing_key, &mb[..], ATTESTATION_MLDSA87_CONTEXT, [0x55u8; 32]).expect("ml-dsa-87 sign");
        StakeAttestation {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: v.validator_id,
            bond_outpoint,
            epoch,
            target_hash,
            target_daa_score,
            validator_set_commitment,
            signature: sig.as_ref().to_vec(),
        }
    }

    /// DAG-harness foundation (ADR-0018 §G): a consensus test can build overlay
    /// txs and produce an attestation signature the §B.4 verifier accepts.
    #[test]
    fn dns_harness_signs_attestations_the_verifier_accepts() {
        let v = harness_validator([0x11u8; 32]);
        assert_eq!(v.pubkey.len(), 2592);
        assert_eq!(v.validator_id, validator_id_from_pubkey(&v.pubkey));

        // Stake-bond tx shape + payload round-trip; validator_pubkey_hash binds the pubkey.
        let bond_tx = build_stake_bond_tx(&v, 10_000_000_000, 0, [0x33u8; 64]);
        assert_eq!(bond_tx.subnetwork_id, SUBNETWORK_ID_STAKE_BOND);
        let bond_outpoint = TransactionOutpoint::new(bond_tx.id(), 0);
        let decoded: StakeBondPayload = borsh::from_slice(&bond_tx.payload).unwrap();
        assert_eq!(decoded.amount, 10_000_000_000);
        assert_eq!(decoded.validator_pubkey_hash, validator_id_from_pubkey(&decoded.validator_pubkey));

        // Signed attestation: the §B.4 verifier (txscript) must accept it.
        let net_id = [0xabu8; 32];
        let target_hash = Hash64::from_bytes([0x44u8; 64]);
        let vsc = Hash64::from_bytes([0x22u8; 64]);
        let att = build_signed_attestation(&v, &net_id, bond_outpoint, 7, target_hash, 700, vsc);
        let msg = stake_attestation_message(
            &net_id,
            att.epoch,
            att.target_hash,
            att.target_daa_score,
            att.validator_set_commitment,
            att.bond_outpoint,
        );
        let mb = msg.as_bytes();
        assert!(
            kaspa_txscript::verify_mldsa87_with_context(&v.pubkey, &mb[..], &att.signature, ATTESTATION_MLDSA87_CONTEXT).unwrap(),
            "the §B.4 verifier must accept the harness-signed attestation"
        );
        // A different key must NOT verify (sanity).
        let v2 = harness_validator([0x99u8; 32]);
        assert!(
            !kaspa_txscript::verify_mldsa87_with_context(&v2.pubkey, &mb[..], &att.signature, ATTESTATION_MLDSA87_CONTEXT).unwrap()
        );

        // Shard tx wraps exactly one extractable attestation.
        let shard_tx = stake_attestation_shard_tx(&single_attestation_shard(att));
        assert_eq!(shard_tx.subnetwork_id, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD);
        assert_eq!(attestations_from_accepted_txs(std::slice::from_ref(&shard_tx)).len(), 1);
    }
}

#[tokio::test]
async fn basic_utxo_disqualified_test() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Mine a valid chain
    for _ in 0..10 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    // Get current sink
    let sink = ctx.consensus.get_sink();

    // Mine a longer disqualified chain
    let disqualified_tip = ctx.build_and_insert_disqualified_chain(vec![config.genesis.hash], 20).await;

    assert_ne!(sink, disqualified_tip);
    assert_eq!(sink, ctx.consensus.get_sink());
    assert_eq!(BlockHashSet::from_iter([sink, disqualified_tip]), BlockHashSet::from_iter(ctx.consensus.get_tips().into_iter()));
    assert!(!ctx.consensus.get_virtual_parents().contains(&disqualified_tip));
}

#[tokio::test]
async fn double_search_disqualified_test() {
    // TODO: add non-coinbase transactions and concurrency in order to complicate the test

    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.min_difficulty_window_size = p.difficulty_window_size;
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Mine 3 valid blocks over genesis
    ctx.build_block_template_row(0..3)
        .validate_and_insert_row()
        .await
        .assert_tips()
        .assert_virtual_parents_subset()
        .assert_valid_utxo_tip();

    // Mark the one expected to remain on virtual chain
    let original_sink = ctx.consensus.get_sink();

    // Find the roots to be used for the disqualified chains
    let mut virtual_parents = ctx.consensus.get_virtual_parents();
    assert!(virtual_parents.remove(&original_sink));
    let mut iter = virtual_parents.into_iter();
    let root_1 = iter.next().unwrap();
    let root_2 = iter.next().unwrap();
    assert_eq!(iter.next(), None);

    // Mine a valid chain
    for _ in 0..10 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    // Get current sink
    let sink = ctx.consensus.get_sink();

    assert!(ctx.consensus.reachability_service().is_chain_ancestor_of(original_sink, sink));

    // Mine a long disqualified chain
    let disqualified_tip_1 = ctx.build_and_insert_disqualified_chain(vec![root_1], 30).await;

    // And another shorter disqualified chain
    let disqualified_tip_2 = ctx.build_and_insert_disqualified_chain(vec![root_2], 20).await;

    assert_eq!(ctx.consensus.get_block_status(root_1), Some(BlockStatus::StatusUTXOValid));
    assert_eq!(ctx.consensus.get_block_status(root_2), Some(BlockStatus::StatusUTXOValid));

    assert_ne!(sink, disqualified_tip_1);
    assert_ne!(sink, disqualified_tip_2);
    assert_eq!(sink, ctx.consensus.get_sink());
    assert_eq!(
        BlockHashSet::from_iter([sink, disqualified_tip_1, disqualified_tip_2]),
        BlockHashSet::from_iter(ctx.consensus.get_tips().into_iter())
    );
    assert!(!ctx.consensus.get_virtual_parents().contains(&disqualified_tip_1));
    assert!(!ctx.consensus.get_virtual_parents().contains(&disqualified_tip_2));

    // Mine a long enough valid chain s.t. both disqualified chains are fully merged
    for _ in 0..30 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    ctx.assert_tips_num(1);
}

fn new_miner_data() -> MinerData {
    // kaspa-pq PQ-only: coinbase outputs must be the standard ML-DSA-87 P2PKH class
    // (enforced with no exemption — see `check_transaction_pq_output_classes`). Use a
    // random 64-byte hash payload: the script is class-valid but unspendable (no
    // preimage), which is all this helper needs, and stays distinct per call.
    let mut payload = [0u8; 64];
    for b in payload.iter_mut() {
        *b = rand::random();
    }
    MinerData::new(p2pkh_mldsa87_spk(&payload), vec![])
}

#[cfg(feature = "evm")]
fn set_fresh_dns_finality(consensus: &TestConsensus) {
    use crate::model::stores::dns_state::DnsStateStore;
    use kaspa_consensus_core::BlueWorkType;
    use kaspa_consensus_core::dns_finality::{DnsHealth, DnsRolloutStage, DnsState, STAKE_SCORE_SCALE, StakeScore};

    consensus
        .virtual_processor()
        .dns_state_store
        .write()
        .set(DnsState {
            selected_chain_anchor: BlockHash::from(77u64),
            anchor_daa_score: 0,
            work_depth: BlueWorkType::from_u64(2_000_000),
            stake_depth: StakeScore(20 * STAKE_SCORE_SCALE),
            last_dns_confirmed_anchor: BlockHash::from(77u64),
            last_dns_confirmed_anchor_daa_score: 0,
            rollout_stage: DnsRolloutStage::Active,
            validator_set_commitment: BlockHash::from(88u64),
            health: DnsHealth::Active,
        })
        .unwrap();
}

/// kaspa-pq EVM Lane v0.4 (ADR-0020) — first EVM-ACTIVE pipeline integration
/// test: with `evm_activation_daa_score = 0`, real blocks inserted through the
/// full pipeline (header → body → virtual) drive the lazy chain-context step:
/// each chain block's mergeset acceptance executes ONCE, its result + state
/// snapshot persist atomically with its UTXO diff, the canonical EVM heads
/// move with the sink, a commitment fault disqualifies the block from the
/// chain (the block stays in the DAG — no poison), and the chain recovers
/// past the disqualified block without re-executing prior EVM results.
#[tokio::test]
#[cfg(feature = "evm")]
async fn evm_active_chain_executes_persists_and_moves_heads() {
    use crate::model::stores::evm::{EvmCanonicalHeadsStoreReader, EvmHeaderStoreReader, EvmPayloadStoreReader, EvmRawTxStoreReader};
    use kaspa_consensus_core::constants::EVM_HEADER_VERSION;
    use kaspa_consensus_core::evm::{EvmAddress, EvmExecutionPayload, EvmStateSnapshot};
    use kaspa_evm::EvmBlockInput;
    use kaspa_hashes::Hash64;

    kaspa_core::log::try_init_logger("info");
    let config =
        ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().edit_consensus_params(|p| p.evm_activation_daa_score = 0).build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let storage = consensus.consensus_clone().storage.clone();
    set_fresh_dns_finality(&consensus);

    let genesis = consensus.params().genesis.hash;
    let miner_data = MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]);

    // ---- b1: empty payload. The §4.3 version rule demands v2 post-activation;
    // the producer (this test) computes the mergeset-acceptance commitment the
    // same way the verifier will (mergeset = [genesis], no payloads ⇒ no
    // accepted txs; EVM parent = none ⇒ genesis state).
    let payload1 = EvmExecutionPayload::default();
    let mut b1 = consensus.build_utxo_valid_block_with_parents(1.into(), vec![genesis], miner_data.clone(), vec![]);
    b1.header.version = EVM_HEADER_VERSION;
    b1.header.evm_payload_hash = payload1.payload_hash();
    let input1 = EvmBlockInput {
        parent: None,
        header_timestamp_ms: b1.header.timestamp,
        selected_parent_hash: genesis.as_bytes(),
        blue_work_be: b1.header.blue_work.to_be_bytes().to_vec(),
        daa_score: b1.header.daa_score,
        payload: &payload1,
        accepted_txs: &[],
        gas_pool_v2_activation_daa_score: u64::MAX,
        f002_withdraw_cap_activation_daa_score: u64::MAX,
        f003_mldsa_verify_activation_daa_score: u64::MAX,
        typed_receipt_root_activation_daa_score: u64::MAX,
    };
    let (exp1, snap1) = kaspa_evm::snapshot::execute_block_from_snapshot(&EvmStateSnapshot::default(), &input1).unwrap();
    b1.header.evm_commitment_root = exp1.header.commitment_root();
    b1.evm_payload = payload1;
    consensus.validate_and_insert_block(b1.to_immutable()).virtual_state_task.await.unwrap();

    assert_eq!(storage.evm_header_store.get(1.into()).unwrap(), exp1.header, "b1's EVM result persisted by the pipeline");
    assert_eq!(exp1.header.evm_number, 1);
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(1u64), "heads moved to the sink");

    // ---- b2: carries its OWN non-empty payload (a declared coinbase + extra
    // data) — proving payload persistence at body commit and EVM state chaining
    // b1 → b2 through the real pipeline. (A real DepositClaim needs a funded
    // EVM_DEPOSIT_LOCK UTXO — P4 claim validation rejects a dangling one; the
    // claim/bridge paths are unit-tested in processes::evm.)
    let payload2 = EvmExecutionPayload {
        evm_coinbase: EvmAddress::from_bytes([0xFE; 20]),
        extra_data: vec![0x4D, 0x53, 0x4B],
        ..Default::default()
    };
    let mut b2 = consensus.build_utxo_valid_block_with_parents(2.into(), vec![1.into()], miner_data.clone(), vec![]);
    b2.header.version = EVM_HEADER_VERSION;
    b2.header.evm_payload_hash = payload2.payload_hash();
    let input2 = EvmBlockInput {
        parent: Some(&exp1.header),
        header_timestamp_ms: b2.header.timestamp,
        selected_parent_hash: BlockHash::from(1u64).as_bytes(),
        blue_work_be: b2.header.blue_work.to_be_bytes().to_vec(),
        daa_score: b2.header.daa_score,
        payload: &payload2,
        accepted_txs: &[], // b1's payload was empty ⇒ nothing to accept
        gas_pool_v2_activation_daa_score: u64::MAX,
        f002_withdraw_cap_activation_daa_score: u64::MAX,
        f003_mldsa_verify_activation_daa_score: u64::MAX,
        typed_receipt_root_activation_daa_score: u64::MAX,
    };
    let (exp2, _snap2) = kaspa_evm::snapshot::execute_block_from_snapshot(&snap1, &input2).unwrap();
    b2.header.evm_commitment_root = exp2.header.commitment_root();
    b2.evm_payload = payload2.clone();
    consensus.validate_and_insert_block(b2.to_immutable()).virtual_state_task.await.unwrap();

    let stored2 = storage.evm_header_store.get(2.into()).unwrap();
    assert_eq!(stored2, exp2.header);
    assert_eq!(stored2.evm_number, 2);
    assert_eq!(stored2.parent_state_root, exp1.header.state_root, "EVM state chains selected-parent-wise");
    assert_eq!(storage.evm_payload_store.get(2.into()).unwrap(), payload2, "own payload persisted at body commit");
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(2u64));

    // ---- b3: a commitment FAULT (producer lied about the acceptance result).
    // The block enters the DAG but is disqualified from the chain — exactly the
    // UTXO-fault shape — and no EVM rows are written for it.
    let payload3 = EvmExecutionPayload::default();
    let mut b3 = consensus.build_utxo_valid_block_with_parents(3.into(), vec![2.into()], miner_data.clone(), vec![]);
    b3.header.version = EVM_HEADER_VERSION;
    b3.header.evm_payload_hash = payload3.payload_hash();
    b3.header.evm_commitment_root = Hash64::from_bytes([0xEE; 64]);
    b3.evm_payload = payload3.clone();
    let _ = consensus.validate_and_insert_block(b3.to_immutable()).virtual_state_task.await;
    assert_eq!(consensus.block_status(3.into()), BlockStatus::StatusDisqualifiedFromChain, "commitment mismatch ⇒ chain-disqualified");
    assert!(!storage.evm_header_store.has(3.into()).unwrap(), "no EVM rows for a disqualified block");
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(2u64), "heads did NOT follow the faulty block");

    // ---- b4: a valid sibling continuation on b2 — the chain recovers past the
    // disqualified b3 (b3 ∉ past(b4), so b3's payload is NOT accepted by b4)
    // and the heads advance. b1/b2 results are reused (their diffs exist ⇒ the
    // KeyNotFound execution arm is never re-entered: no re-execution on reorg).
    let payload4 = EvmExecutionPayload::default();
    let mut b4 = consensus.build_utxo_valid_block_with_parents(4.into(), vec![2.into()], miner_data, vec![]);
    b4.header.version = EVM_HEADER_VERSION;
    b4.header.evm_payload_hash = payload4.payload_hash();
    let input4 = EvmBlockInput {
        parent: Some(&exp2.header),
        header_timestamp_ms: b4.header.timestamp,
        selected_parent_hash: BlockHash::from(2u64).as_bytes(),
        blue_work_be: b4.header.blue_work.to_be_bytes().to_vec(),
        daa_score: b4.header.daa_score,
        payload: &payload4,
        accepted_txs: &[], // b2's payload txs are empty (system ops are not delayed-accepted)
        gas_pool_v2_activation_daa_score: u64::MAX,
        f002_withdraw_cap_activation_daa_score: u64::MAX,
        f003_mldsa_verify_activation_daa_score: u64::MAX,
        typed_receipt_root_activation_daa_score: u64::MAX,
    };
    let snap2 = {
        // Recompute b2's child snapshot the same way the node stored it.
        let (_, s) = kaspa_evm::snapshot::execute_block_from_snapshot(&snap1, &input2).unwrap();
        s
    };
    let (exp4, _) = kaspa_evm::snapshot::execute_block_from_snapshot(&snap2, &input4).unwrap();
    b4.header.evm_commitment_root = exp4.header.commitment_root();
    b4.evm_payload = payload4;
    consensus.validate_and_insert_block(b4.to_immutable()).virtual_state_task.await.unwrap();

    assert_eq!(storage.evm_header_store.get(4.into()).unwrap().evm_number, 3, "b4 is EVM block 3 on the selected chain");
    assert_eq!(
        storage.evm_heads_store.read().get().unwrap().latest,
        BlockHash::from(4u64),
        "heads recovered past the disqualified block"
    );

    // ---- b5: the node's OWN template (§15 producer path) — the builder must
    // declare v2, commit the (empty) payload hash and the REAL acceptance
    // commitment, and the resulting block must validate through the full
    // pipeline. (Template used as-is: on an evm-active net a miner must not
    // mutate the template timestamp — the commitment derives from it.)
    let template = consensus
        .build_block_template(
            MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]),
            Box::new(OnetimeTxSelector::new(Default::default())),
            TemplateBuildMode::Standard,
        )
        .unwrap();
    assert_eq!(template.block.header.version, EVM_HEADER_VERSION, "evm-active template declares v2");
    assert_eq!(template.block.header.evm_payload_hash, EvmExecutionPayload::default().payload_hash());
    assert_ne!(template.block.header.evm_commitment_root, Hash64::default(), "the template committed a real acceptance result");
    let mut b5 = template.block;
    b5.header.hash = 5u64.into(); // test identity (PoW skipped)
    consensus.validate_and_insert_block(b5.to_immutable()).virtual_state_task.await.unwrap();
    assert!(storage.evm_header_store.has(5.into()).unwrap(), "the self-mined block executed + persisted");
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(5u64));

    // ---- b6 (§16-1): a template with an EVM-mempool candidate — the §15
    // step-6 own-payload path. The fixture is a signed EIP-1559 transfer on
    // EVM_CHAIN_ID (regenerate: `cargo test -p kaspa-evm fixture_generator --
    // --ignored --nocapture`); its sender is UNFUNDED, which is irrelevant for
    // inclusion (data-only) and makes acceptance a deterministic class-2 skip.
    const FIXTURE_TX_NONCE0: &str = "02f86b834d534b8080843b9aca008252089400000000000000000000000000000000000000228201f480c001a03244f5d74a96a52bd1c42fa1b9c336f4d3ae5509190ed9a526f17971c7fd743ca07f58e09399b50636b84f0ae4a7634c60a11c6f32427b613ebf6f4a638d6c68c1";
    let mut raw_n0 = vec![0u8; FIXTURE_TX_NONCE0.len() / 2];
    faster_hex::hex_decode(FIXTURE_TX_NONCE0.as_bytes(), &mut raw_n0).unwrap();

    let template = consensus
        .build_block_template_with_evm(
            MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]),
            Box::new(OnetimeTxSelector::new(Default::default())),
            TemplateBuildMode::Standard,
            kaspa_consensus_core::evm::EvmTemplateData {
                evm_coinbase: kaspa_consensus_core::evm::EvmAddress::from_bytes([0xCB; 20]),
                transactions: vec![raw_n0.clone()],
                system_ops: vec![],
            },
        )
        .unwrap();
    assert_eq!(template.block.evm_payload.transactions, vec![raw_n0.clone()], "the candidate landed in the own payload");
    assert_eq!(
        template.block.evm_payload.evm_coinbase,
        kaspa_consensus_core::evm::EvmAddress::from_bytes([0xCB; 20]),
        "the declared fee recipient landed as the payload coinbase (§8.2)"
    );
    assert_eq!(
        template.block.header.evm_payload_hash,
        template.block.evm_payload.payload_hash(),
        "the header commits the NON-empty payload"
    );
    let mut b6 = template.block;
    b6.header.hash = 6u64.into();
    consensus.validate_and_insert_block(b6.to_immutable()).virtual_state_task.await.unwrap();
    assert!(storage.evm_payload_store.has(6.into()).unwrap(), "the non-empty own payload persisted at commit_body");

    // ---- b7: the NEXT template accepts b6's payload (mergeset delayed
    // acceptance): the unfunded sender makes the tx a deterministic class-2
    // skip — counted, no receipt, block valid. This closes the full §16-1
    // loop: pool candidate → template inclusion → wire/body validation →
    // acceptance processing by the selected child.
    let template = consensus
        .build_block_template(
            MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]),
            Box::new(OnetimeTxSelector::new(Default::default())),
            TemplateBuildMode::Standard,
        )
        .unwrap();
    let mut b7 = template.block;
    b7.header.hash = 7u64.into();
    consensus.validate_and_insert_block(b7.to_immutable()).virtual_state_task.await.unwrap();
    let h7 = storage.evm_header_store.get(7.into()).unwrap();
    assert_eq!(h7.skipped_tx_count, 1, "b6's unfunded payload tx was class-2 skipped at acceptance");
    assert_eq!(h7.accepted_tx_count, 0);
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(7u64));

    // §16-3: the tx-lookup index recorded the journey — included in b6 (DA
    // visibility), never accepted, last skip = class 2 (unfunded sender). The
    // exact data misaka_getTxInclusionStatus serves.
    let fixture_hash = kaspa_evm::tx::tx_hash(&{
        let mut raw = vec![0u8; FIXTURE_TX_NONCE0.len() / 2];
        faster_hex::hex_decode(FIXTURE_TX_NONCE0.as_bytes(), &mut raw).unwrap();
        raw
    });
    let row = storage.evm_tx_index_store.get_or_default(fixture_hash).unwrap();
    assert_eq!(row.included_in, vec![BlockHash::from(6u64)], "DA visibility: the payload block carrying the tx");
    assert!(row.accepted_in.is_empty(), "never executed (unfunded)");
    assert_eq!(row.last_skip_class, Some(2));

    // audit R-2: the raw tx is resolvable DIRECTLY by hash (no included_in scan),
    // recorded at body commit of its carrying payload block (b6) — the path the
    // eth_getTransactionByHash/receipt adapter now uses.
    let stored = storage.evm_raw_tx_store.get(fixture_hash).unwrap().expect("raw tx indexed by hash");
    assert_eq!(stored.raw, raw_n0, "raw EIP-2718 bytes round-trip by hash");
    assert_eq!(stored.payload_block, BlockHash::from(6u64), "carrying payload block recorded");
    assert_eq!(
        consensus.consensus_clone().get_evm_raw_tx(fixture_hash).unwrap(),
        Some(raw_n0.clone()),
        "get_evm_raw_tx resolves the tx without the bounded included_in scan"
    );

    consensus.shutdown(wait_handles);
}

/// kaspa-pq EVM Lane v0.4 (§16 RPC / canonical-index fix, R-1): the
/// `evm_number → L1 hash` map is driven by the SELECTED chain at virtual commit,
/// NOT per-block result-commit. A reorg must detach the old canonical block's
/// number and attach the new chain's block at that number; the detached block
/// stays queryable by L1 hash (immutable rows are kept). This exercises
/// `update_evm_canonical_number_map` end-to-end. The conditional-release branch
/// is unit-tested in `model::stores::evm` (`evm_number_store_canonical_*`). The
/// precise sink-search-loser shadow that motivated the fix needs the DNS
/// reorg-gate (overlay-Active); the structural fix prevents it by construction —
/// a non-selected block never writes the map.
#[tokio::test]
#[cfg(feature = "evm")]
async fn evm_active_canonical_number_map_follows_reorg() {
    use crate::model::stores::evm::{EvmCanonicalHeadsStoreReader, EvmHeaderStoreReader, EvmNumberStoreReader};
    use kaspa_consensus_core::constants::EVM_HEADER_VERSION;
    use kaspa_consensus_core::evm::{EvmExecutionPayload, EvmStateSnapshot};
    use kaspa_evm::EvmBlockInput;

    kaspa_core::log::try_init_logger("info");
    let config =
        ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().edit_consensus_params(|p| p.evm_activation_daa_score = 0).build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let storage = consensus.consensus_clone().storage.clone();

    let genesis = consensus.params().genesis.hash;
    let miner_data = MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]);
    let inert = u64::MAX;

    // ---- b1 (#1): empty payload on genesis (mirrors the EVM-active test).
    let payload1 = EvmExecutionPayload::default();
    let mut b1 = consensus.build_utxo_valid_block_with_parents(1.into(), vec![genesis], miner_data.clone(), vec![]);
    b1.header.version = EVM_HEADER_VERSION;
    b1.header.evm_payload_hash = payload1.payload_hash();
    let input1 = EvmBlockInput {
        parent: None,
        header_timestamp_ms: b1.header.timestamp,
        selected_parent_hash: genesis.as_bytes(),
        blue_work_be: b1.header.blue_work.to_be_bytes().to_vec(),
        daa_score: b1.header.daa_score,
        payload: &payload1,
        accepted_txs: &[],
        gas_pool_v2_activation_daa_score: inert,
        f002_withdraw_cap_activation_daa_score: inert,
        f003_mldsa_verify_activation_daa_score: inert,
        typed_receipt_root_activation_daa_score: inert,
    };
    let (exp1, snap1) = kaspa_evm::snapshot::execute_block_from_snapshot(&EvmStateSnapshot::default(), &input1).unwrap();
    b1.header.evm_commitment_root = exp1.header.commitment_root();
    b1.evm_payload = payload1;
    consensus.validate_and_insert_block(b1.to_immutable()).virtual_state_task.await.unwrap();

    // ---- b2 (#2): on b1.
    let payload2 = EvmExecutionPayload::default();
    let mut b2 = consensus.build_utxo_valid_block_with_parents(2.into(), vec![1.into()], miner_data.clone(), vec![]);
    b2.header.version = EVM_HEADER_VERSION;
    b2.header.evm_payload_hash = payload2.payload_hash();
    let input2 = EvmBlockInput {
        parent: Some(&exp1.header),
        header_timestamp_ms: b2.header.timestamp,
        selected_parent_hash: BlockHash::from(1u64).as_bytes(),
        blue_work_be: b2.header.blue_work.to_be_bytes().to_vec(),
        daa_score: b2.header.daa_score,
        payload: &payload2,
        accepted_txs: &[],
        gas_pool_v2_activation_daa_score: inert,
        f002_withdraw_cap_activation_daa_score: inert,
        f003_mldsa_verify_activation_daa_score: inert,
        typed_receipt_root_activation_daa_score: inert,
    };
    let (exp2, snap2) = kaspa_evm::snapshot::execute_block_from_snapshot(&snap1, &input2).unwrap();
    b2.header.evm_commitment_root = exp2.header.commitment_root();
    b2.evm_payload = payload2;
    consensus.validate_and_insert_block(b2.to_immutable()).virtual_state_task.await.unwrap();
    assert_eq!(storage.evm_number_store.get(2).unwrap(), Some(BlockHash::from(2u64)), "b2 claims #2");

    // ---- x3 (#3) on b2 — the initial sink. Hash 9 wins the equal-blue-work
    // tiebreak vs y3 (hash 5), so x3 stays canonical at #3 until y4 reorgs.
    let payloadx = EvmExecutionPayload::default();
    let mut x3 = consensus.build_utxo_valid_block_with_parents(9.into(), vec![2.into()], miner_data.clone(), vec![]);
    x3.header.version = EVM_HEADER_VERSION;
    x3.header.evm_payload_hash = payloadx.payload_hash();
    let inputx = EvmBlockInput {
        parent: Some(&exp2.header),
        header_timestamp_ms: x3.header.timestamp,
        selected_parent_hash: BlockHash::from(2u64).as_bytes(),
        blue_work_be: x3.header.blue_work.to_be_bytes().to_vec(),
        daa_score: x3.header.daa_score,
        payload: &payloadx,
        accepted_txs: &[],
        gas_pool_v2_activation_daa_score: inert,
        f002_withdraw_cap_activation_daa_score: inert,
        f003_mldsa_verify_activation_daa_score: inert,
        typed_receipt_root_activation_daa_score: inert,
    };
    let (expx, _snapx) = kaspa_evm::snapshot::execute_block_from_snapshot(&snap2, &inputx).unwrap();
    assert_eq!(expx.header.evm_number, 3);
    x3.header.evm_commitment_root = expx.header.commitment_root();
    x3.evm_payload = payloadx;
    consensus.validate_and_insert_block(x3.to_immutable()).virtual_state_task.await.unwrap();
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(9u64), "x3 is the sink");
    assert_eq!(storage.evm_number_store.get(3).unwrap(), Some(BlockHash::from(9u64)), "x3 canonical at #3 before the reorg");

    // ---- y3 (#3) on b2 — a sibling of x3. Equal blue work, lower hash (5 < 9)
    // ⇒ x3 keeps the sink; y3 is inserted but not yet selected/validated.
    let payloady3 = EvmExecutionPayload::default();
    let mut y3 = consensus.build_utxo_valid_block_with_parents(5.into(), vec![2.into()], miner_data.clone(), vec![]);
    y3.header.version = EVM_HEADER_VERSION;
    y3.header.evm_payload_hash = payloady3.payload_hash();
    let inputy3 = EvmBlockInput {
        parent: Some(&exp2.header),
        header_timestamp_ms: y3.header.timestamp,
        selected_parent_hash: BlockHash::from(2u64).as_bytes(),
        blue_work_be: y3.header.blue_work.to_be_bytes().to_vec(),
        daa_score: y3.header.daa_score,
        payload: &payloady3,
        accepted_txs: &[],
        gas_pool_v2_activation_daa_score: inert,
        f002_withdraw_cap_activation_daa_score: inert,
        f003_mldsa_verify_activation_daa_score: inert,
        typed_receipt_root_activation_daa_score: inert,
    };
    let (expy3, snapy3) = kaspa_evm::snapshot::execute_block_from_snapshot(&snap2, &inputy3).unwrap();
    y3.header.evm_commitment_root = expy3.header.commitment_root();
    y3.evm_payload = payloady3;
    consensus.validate_and_insert_block(y3.to_immutable()).virtual_state_task.await.unwrap();
    assert_eq!(storage.evm_number_store.get(3).unwrap(), Some(BlockHash::from(9u64)), "x3 still canonical at #3 (y3 not selected)");

    // ---- y4 (#4) on y3 — the heavier branch (2 blocks past b2) reorgs the sink
    // from x3 to y4. The selected chain is now ...b2, y3(#3), y4(#4).
    let payloady4 = EvmExecutionPayload::default();
    let mut y4 = consensus.build_utxo_valid_block_with_parents(6.into(), vec![5.into()], miner_data, vec![]);
    y4.header.version = EVM_HEADER_VERSION;
    y4.header.evm_payload_hash = payloady4.payload_hash();
    let inputy4 = EvmBlockInput {
        parent: Some(&expy3.header),
        header_timestamp_ms: y4.header.timestamp,
        selected_parent_hash: BlockHash::from(5u64).as_bytes(),
        blue_work_be: y4.header.blue_work.to_be_bytes().to_vec(),
        daa_score: y4.header.daa_score,
        payload: &payloady4,
        accepted_txs: &[],
        gas_pool_v2_activation_daa_score: inert,
        f002_withdraw_cap_activation_daa_score: inert,
        f003_mldsa_verify_activation_daa_score: inert,
        typed_receipt_root_activation_daa_score: inert,
    };
    let (expy4, _snapy4) = kaspa_evm::snapshot::execute_block_from_snapshot(&snapy3, &inputy4).unwrap();
    assert_eq!(expy4.header.evm_number, 4);
    y4.header.evm_commitment_root = expy4.header.commitment_root();
    y4.evm_payload = payloady4;
    consensus.validate_and_insert_block(y4.to_immutable()).virtual_state_task.await.unwrap();

    // The reorg detached x3 and attached y3(#3) + y4(#4):
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(6u64), "sink reorged to y4");
    assert_eq!(
        storage.evm_number_store.get(3).unwrap(),
        Some(BlockHash::from(5u64)),
        "#3 now resolves to y3 (canonical), not the detached x3"
    );
    assert_eq!(storage.evm_number_store.get(4).unwrap(), Some(BlockHash::from(6u64)), "y4 claimed #4");
    assert_eq!(storage.evm_number_store.get(2).unwrap(), Some(BlockHash::from(2u64)), "#2 (below the fork) is unchanged");
    assert_ne!(storage.evm_number_store.get(3).unwrap(), Some(BlockHash::from(9u64)), "the detached x3 no longer owns #3");
    // The detached x3 stays queryable by L1 hash (immutable rows survive).
    assert!(storage.evm_header_store.has(9.into()).unwrap(), "x3's immutable EVM rows survive the reorg (hash-queryable)");

    consensus.shutdown(wait_handles);
}

/// kaspa-pq EVM Lane v0.4 §9.2 — producer-side deposit-claim path: a queued
/// `DepositClaim` (resolved from a real EVM_DEPOSIT_LOCK UTXO, the work the
/// `submitEvmDepositClaim` RPC does) lands in the node's OWN template
/// `system_ops` after the template path re-validates it against the live claim
/// view; a claim for a non-existent/stale lock is dropped — so a queued claim
/// can never make the producer's own block invalid. This closes the production
/// half of the bridge: deposits are now both validatable (P4) AND producible.
#[tokio::test]
#[cfg(feature = "evm")]
async fn evm_producer_deposit_claim_fills_and_filters_template_system_ops() {
    use kaspa_consensus_core::evm::{DepositClaim, EvmAddress, EvmSystemOp, EvmTemplateData};
    use kaspa_consensus_core::header::Header;
    use kaspa_consensus_core::muhash::MuHashExtensions;
    use kaspa_consensus_core::tx::UtxoEntry;
    use kaspa_muhash::MuHash;
    use kaspa_txscript::script_class::evm_deposit_lock_script;

    kaspa_core::log::try_init_logger("info");

    // A real EVM_DEPOSIT_LOCK output: 1000 sompi locked to an EVM address, claim
    // tip 7, timeout far in the future; refund = a standard ML-DSA P2PKH.
    let evm_addr = [0xAB; 20];
    let refund_spk = p2pkh_mldsa87_spk(&[0x42; 64]);
    let lock_spk = evm_deposit_lock_script(evm_addr, 1_000_000, 7, refund_spk.script());
    let lock_outpoint = TransactionOutpoint::new(99u64.into(), 0);
    let initial_utxos =
        [(lock_outpoint, UtxoEntry { amount: 1000, script_public_key: lock_spk, block_daa_score: 0, is_coinbase: false })];

    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| p.evm_activation_daa_score = 0)
        .apply_args(|cfg| {
            let mut ms = MuHash::new();
            initial_utxos.iter().for_each(|(op, u)| ms.add_utxo(op, u));
            cfg.params.genesis.utxo_commitment = ms.finalize();
            let genesis_header: Header = (&cfg.params.genesis).into();
            cfg.params.genesis.hash = genesis_header.hash;
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let mut genesis_ms = MuHash::new();
    consensus.append_imported_pruning_point_utxos(&initial_utxos, &mut genesis_ms);
    consensus.import_pruning_point_utxo_set(config.genesis.hash, genesis_ms).unwrap();

    // (1) the valid claim for the real lock; (2) a claim for a non-existent
    // outpoint that re-validation must drop.
    let good_claim = DepositClaim {
        deposit_outpoint: lock_outpoint,
        evm_address: EvmAddress::from_bytes(evm_addr),
        amount_sompi: 1000,
        claim_tip_sompi: 7,
    };
    let bogus_claim = DepositClaim {
        deposit_outpoint: TransactionOutpoint::new(123u64.into(), 0),
        evm_address: EvmAddress::from_bytes([0xCD; 20]),
        amount_sompi: 500,
        claim_tip_sompi: 0,
    };

    let stale_template = consensus
        .build_block_template_with_evm(
            MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]),
            Box::new(OnetimeTxSelector::new(Default::default())),
            TemplateBuildMode::Standard,
            EvmTemplateData {
                evm_coinbase: EvmAddress::from_bytes([0xCB; 20]),
                transactions: vec![],
                system_ops: vec![good_claim.clone(), bogus_claim.clone()],
            },
        )
        .unwrap();
    assert!(
        stale_template.block.evm_payload.is_empty(),
        "without a fresh DNS-confirmed anchor, bridge deposit claims stay out of the template"
    );

    set_fresh_dns_finality(&consensus);

    let template = consensus
        .build_block_template_with_evm(
            MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]),
            Box::new(OnetimeTxSelector::new(Default::default())),
            TemplateBuildMode::Standard,
            EvmTemplateData {
                evm_coinbase: EvmAddress::from_bytes([0xCB; 20]),
                transactions: vec![],
                system_ops: vec![good_claim.clone(), bogus_claim],
            },
        )
        .unwrap();

    assert_eq!(template.block.evm_payload.system_ops.len(), 1, "only the valid claim survives template re-validation");
    assert_eq!(
        template.block.evm_payload.system_ops[0],
        EvmSystemOp::DepositClaim(good_claim),
        "the resolved lock's claim is in the own payload"
    );
    assert_eq!(
        template.block.evm_payload.evm_coinbase,
        EvmAddress::from_bytes([0xCB; 20]),
        "a claim-bearing payload declares the coinbase (the tip routes to it)"
    );
    assert_eq!(template.block.header.evm_payload_hash, template.block.evm_payload.payload_hash(), "header commits the claim payload");

    consensus.shutdown(wait_handles);
}

/// kaspa-pq EVM Lane v0.4 §14.1/§14.3 — Y9 budget independence, pipeline e2e:
/// a template assembled from an OVERSUPPLIED candidate list fills the payload
/// to the byte cap (and no further), the resulting full-cap block keeps its
/// normal UTXO content and passes the complete pipeline (mass rules included),
/// and the next chain block processes the entire payload at acceptance without
/// invalidating or stalling the UTXO lane. Complements the in-isolation mass
/// equality test (`evm_y9_payload_byte_budget_independent_of_utxo_mass_budget`);
/// the λ·D propagation re-validation with measured payload-laden D is Y10 —
/// testnet work and an activation precondition (§14.3), not a unit concern.
#[tokio::test]
#[cfg(feature = "evm")]
async fn evm_y9_full_cap_payload_block_validates_and_executes() {
    use crate::model::stores::evm::{EvmCanonicalHeadsStoreReader, EvmHeaderStoreReader, EvmPayloadStoreReader};
    use kaspa_consensus_core::evm::{EvmAddress, EvmExecutionPayload, EvmTemplateData, MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK};

    kaspa_core::log::try_init_logger("info");
    let config =
        ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().edit_consensus_params(|p| p.evm_activation_daa_score = 0).build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let storage = consensus.consensus_clone().storage.clone();
    set_fresh_dns_finality(&consensus);

    // The class-1-valid §16 fixture, oversupplied: duplication is legal at the
    // body (admission is per-tx) and a deterministic skip at acceptance.
    const FIXTURE_TX_NONCE0: &str = "02f86b834d534b8080843b9aca008252089400000000000000000000000000000000000000228201f480c001a03244f5d74a96a52bd1c42fa1b9c336f4d3ae5509190ed9a526f17971c7fd743ca07f58e09399b50636b84f0ae4a7634c60a11c6f32427b613ebf6f4a638d6c68c1";
    let mut raw = vec![0u8; FIXTURE_TX_NONCE0.len() / 2];
    faster_hex::hex_decode(FIXTURE_TX_NONCE0.as_bytes(), &mut raw).unwrap();
    let base = EvmExecutionPayload::default().payload_bytes().len();
    let per_tx = 4 + raw.len();
    let n = (MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK - base) / per_tx;

    let template = consensus
        .build_block_template_with_evm(
            MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]),
            Box::new(OnetimeTxSelector::new(Default::default())),
            TemplateBuildMode::Standard,
            EvmTemplateData {
                evm_coinbase: EvmAddress::from_bytes([0xCB; 20]),
                transactions: vec![raw.clone(); n + 32], // 32 candidates beyond the cap
                system_ops: vec![],
            },
        )
        .unwrap();
    assert_eq!(template.block.evm_payload.transactions.len(), n, "template fills to the byte cap and not one tx further");
    let assembled = template.block.evm_payload.payload_bytes().len();
    assert!(assembled <= MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK, "assembled payload within the cap");
    assert!(assembled > MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK - per_tx, "assembled payload NEAR the cap");

    let mut b1 = template.block;
    b1.header.hash = 1u64.into();
    consensus.validate_and_insert_block(b1.to_immutable()).virtual_state_task.await.unwrap();
    assert!(storage.evm_payload_store.has(1.into()).unwrap(), "full-cap payload persisted at commit_body");

    // The next chain block accepts b1's payload: every copy is a deterministic
    // skip (unfunded sender), the block stays valid, the heads advance — a
    // payload-maxed DAG block never blocks the UTXO lane (§14.2).
    let template = consensus
        .build_block_template(
            MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]),
            Box::new(OnetimeTxSelector::new(Default::default())),
            TemplateBuildMode::Standard,
        )
        .unwrap();
    let mut b2 = template.block;
    b2.header.hash = 2u64.into();
    consensus.validate_and_insert_block(b2.to_immutable()).virtual_state_task.await.unwrap();
    let h2 = storage.evm_header_store.get(2.into()).unwrap();
    assert_eq!(h2.skipped_tx_count, n as u32, "the full-cap payload was processed: every copy skipped, none accepted");
    assert_eq!(h2.accepted_tx_count, 0);
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(2u64));

    consensus.shutdown(wait_handles);
}

/// The DNS reorg gate's common-ancestor search: the O(log horizon) index binary search must agree
/// with the linear walk **block for block and boundary for boundary**, and the horizon both apply
/// must be the CANONICAL-side rewind depth.
///
/// The metric matters as much as the answer. Bounding the *candidate* side (what the walk did
/// before) measures how far the other branch ran, so an attacker who mines a long secret branch
/// while canonical advances a little would be classified "deeper than the horizon" and the gate
/// would ABSTAIN — handing a deep-reorg attacker the pass the veto exists to deny. Bounding the
/// canonical side asks the question the horizon is named for: how many of MY blocks would this
/// candidate rewind. Here the side branch (8 blocks) is deliberately SHORTER than the canonical
/// rewind it would cause (15), so the two metrics give different verdicts and the test pins the
/// right one.
#[tokio::test]
async fn dns_gate_common_ancestor_search_matches_walk_on_canonical_rewind_depth() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..40 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await;
    }

    const REWIND: usize = 15; // canonical blocks the side branch would rewind
    const SIDE_LEN: usize = 8; // side-branch length — deliberately < REWIND
    let sink = ctx.consensus.get_sink();
    let fork_point = {
        let vp = ctx.consensus.virtual_processor();
        // The iterator yields `sink` itself first, so `nth(REWIND)` is REWIND blocks back.
        vp.reachability_service.default_backward_chain_iterator(sink).nth(REWIND).expect("the chain is longer than REWIND")
    };
    let side_tip = ctx.build_and_insert_disqualified_chain(vec![fork_point], SIDE_LEN).await;

    let vp = ctx.consensus.virtual_processor();
    // At or above the rewind depth both paths find the same fork point.
    for horizon in [REWIND as u64, REWIND as u64 + 1, 40, 10_000] {
        assert_eq!(
            vp.chain_common_ancestor_within(side_tip, sink, horizon),
            Some(fork_point),
            "binary search finds the fork point at horizon {horizon}"
        );
        assert_eq!(vp.chain_common_ancestor_walk(side_tip, sink, horizon), Some(fork_point), "the walk agrees at horizon {horizon}");
    }
    // Below it both abstain — including at exactly one short of the boundary.
    for horizon in [0, 1, REWIND as u64 - 1] {
        assert_eq!(vp.chain_common_ancestor_within(side_tip, sink, horizon), None, "binary search abstains at {horizon}");
        assert_eq!(vp.chain_common_ancestor_walk(side_tip, sink, horizon), None, "the walk agrees at {horizon}");
    }
    // The boundary is the canonical rewind (15), NOT the side-branch length (8): a horizon that
    // covers the side branch but not the rewind must still abstain.
    assert!(SIDE_LEN < REWIND);
    assert_eq!(vp.chain_common_ancestor_within(side_tip, sink, SIDE_LEN as u64), None);
    assert_eq!(vp.chain_common_ancestor_walk(side_tip, sink, SIDE_LEN as u64), None);
    // Degenerate inputs: canonical vs itself, and canonical vs one of its own chain ancestors.
    assert_eq!(vp.chain_common_ancestor_within(sink, sink, 10), Some(sink));
    assert_eq!(vp.chain_common_ancestor_walk(sink, sink, 10), Some(sink));
    assert_eq!(vp.chain_common_ancestor_within(fork_point, sink, 100), Some(fork_point));
    assert_eq!(vp.chain_common_ancestor_walk(fork_point, sink, 100), Some(fork_point));
}

/// Critical-1 regression: **a long secret branch must not buy an abstain.**
///
/// The gate's horizon is a bound on the search, and turning "I stopped looking" into a consensus
/// decision is what made the old candidate-side walk exploitable. Shape of the attack it allowed:
///
/// ```text
///   fork F
///     canonical:  F → C1 … C15          (15 blocks — all this candidate would rewind)
///     secret:     F → A1 … A60          (60 blocks — mined in private, arbitrarily long)
/// ```
///
/// Walking back from the CANDIDATE, a horizon of 20 runs out 40 blocks short of `F`, reports "no
/// common ancestor", and the gate abstains — so an attacker bought a pass out of DNS adjudication
/// purely by making the secret branch longer, which costs nothing to do. Walking back from
/// CANONICAL asks the question the horizon is actually named for — how many of MY blocks does this
/// rewind — and 15 is well inside 20, so the ancestor is found and the gate adjudicates.
///
/// This is the mirror of `dns_gate_common_ancestor_search_matches_walk_on_canonical_rewind_depth`,
/// which pins the same metric from the other side (side branch SHORTER than the rewind). Both
/// asymmetries are needed: one proves a short branch cannot dodge the horizon, this one proves a
/// long branch cannot dodge the gate.
#[tokio::test]
async fn dns_gate_long_secret_branch_cannot_buy_an_abstain() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..40 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await;
    }

    const REWIND: usize = 15; // canonical blocks the secret branch would rewind
    const SECRET_LEN: usize = 60; // secret branch — deliberately MUCH longer than the rewind
    const HORIZON: u64 = 20; // covers the rewind, nowhere near the secret branch
    assert!(REWIND < HORIZON as usize && (HORIZON as usize) < SECRET_LEN, "the horizon must separate the two metrics");

    let sink = ctx.consensus.get_sink();
    let fork_point = {
        let vp = ctx.consensus.virtual_processor();
        vp.reachability_service.default_backward_chain_iterator(sink).nth(REWIND).expect("the chain is longer than REWIND")
    };
    let secret_tip = ctx.build_and_insert_disqualified_chain(vec![fork_point], SECRET_LEN).await;
    // Built up front: extending the branch needs `&mut ctx`, which cannot overlap the `vp` borrow.
    let longer_tip = ctx.build_and_insert_disqualified_chain(vec![secret_tip], SECRET_LEN).await;

    let vp = ctx.consensus.virtual_processor();
    // The ancestor is found: the horizon bounds the canonical rewind (15), not the secret branch (60).
    assert_eq!(
        vp.chain_common_ancestor_within(secret_tip, sink, HORIZON),
        Some(fork_point),
        "a {SECRET_LEN}-block secret branch must not push a {REWIND}-block rewind out of a {HORIZON} horizon"
    );
    assert_eq!(vp.chain_common_ancestor_walk(secret_tip, sink, HORIZON), Some(fork_point), "the reference walk agrees");

    // Lengthening the secret branch further changes nothing — the metric is not on that side.
    assert_eq!(
        vp.chain_common_ancestor_within(longer_tip, sink, HORIZON),
        Some(fork_point),
        "doubling the secret branch must not change the verdict"
    );
    assert_eq!(vp.chain_common_ancestor_walk(longer_tip, sink, HORIZON), Some(fork_point));

    // And the horizon still bites where it should: one short of the canonical rewind, both abstain.
    assert_eq!(vp.chain_common_ancestor_within(secret_tip, sink, REWIND as u64 - 1), None);
    assert_eq!(vp.chain_common_ancestor_walk(secret_tip, sink, REWIND as u64 - 1), None);

    // The bug this pins, stated executably. Swapping the arguments makes the walk measure the
    // SECRET side — exactly what the pre-fix code did — and at the same horizon it reports "no
    // common ancestor", which the gate turns into an abstain. Same DAG, same horizon, opposite
    // verdict: the metric alone is the difference between adjudicating and standing aside.
    assert_eq!(
        vp.chain_common_ancestor_walk(sink, secret_tip, HORIZON),
        None,
        "measuring the secret side abstains — this is the hole, kept here so it cannot come back unnoticed"
    );
}

/// **The boundary of the DNS liveness work, stated executably.** Releasing the DNS veto —
/// horizon, TTL, work override, all of it — cannot converge a fork whose branches disagree about
/// whether each other's blocks are *valid*.
///
/// Every DNS release path acts at sink selection, on candidates that have already passed UTXO
/// validation. When a node judges the other branch UTXO-invalid, those blocks are disqualified
/// **before** the gate is ever consulted, so no amount of gate permissiveness reaches them. That
/// is why the incident reports separate the two layers: testnet-21's split was verdict divergence
/// (layer 1) wearing a relay/IBD closure (layer 2), and testnet-22 still lists overlay-commitment
/// verdict divergence among its unresolved candidate causes.
///
/// Construction is deliberately blunt: a side branch heavier than canonical, built UTXO-invalid,
/// with the DNS gate provably dormant (no bond ⇒ never `Active`) so it cannot be blamed for the
/// non-convergence. The heavier branch is refused anyway.
///
/// Kept so nobody reads the partition-liveness commits as "chain splits are fixed". They are not:
/// what is fixed is DNS turning a split into a permanent one.
#[tokio::test]
async fn verdict_divergent_branch_does_not_converge_even_with_the_dns_gate_dormant() {
    use crate::model::stores::{dns_state::DnsStateStoreReader, ghostdag::GhostdagStoreReader};
    use kaspa_consensus_core::dns_finality::DnsRolloutStage;

    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..20 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await;
    }

    const REWIND: usize = 5; // canonical blocks the side branch forks below
    const SIDE_LEN: usize = 40; // long enough to out-work the canonical tail by a wide margin
    let sink_before = ctx.consensus.get_sink();
    let fork_point = {
        let vp = ctx.consensus.virtual_processor();
        vp.reachability_service.default_backward_chain_iterator(sink_before).nth(REWIND).expect("chain longer than REWIND")
    };
    let side_tip = ctx.build_and_insert_disqualified_chain(vec![fork_point], SIDE_LEN).await;

    // The side branch really is heavier — this is not a work-starved branch being ignored.
    {
        let vp = ctx.consensus.virtual_processor();
        let side_work = vp.ghostdag_store.get_blue_work(side_tip).unwrap();
        let canon_work = vp.ghostdag_store.get_blue_work(sink_before).unwrap();
        assert!(side_work > canon_work, "the side branch must out-work canonical ({side_work} vs {canon_work})");

        // And the DNS gate is dormant: no bond was ever created, so the rollout never reached
        // `Active` and `dns_reorg_outcome` short-circuits to `GateInactive` before any horizon,
        // TTL or override logic runs. Whatever refuses this branch, it is not the DNS veto.
        let stage = vp.dns_state_store.read().get().map(|s| s.rollout_stage).unwrap_or(DnsRolloutStage::Launch);
        assert_ne!(stage, DnsRolloutStage::Active, "the gate must be provably out of the picture for this test to mean anything");
    }

    // Refused anyway: the blocks never became sink candidates, because this node judged them
    // UTXO-invalid. Sink selection — where every DNS release path lives — is downstream of that.
    assert_eq!(ctx.consensus.block_status(side_tip), BlockStatus::StatusDisqualifiedFromChain, "the branch is verdict-rejected");
    assert_eq!(ctx.consensus.get_sink(), sink_before, "a heavier but verdict-rejected branch does not move the sink");

    // The honest reading: DNS bounded ⇒ a split cannot be made permanent BY DNS. A split caused by
    // divergent validation still needs the divergence itself fixed.
    assert!(
        ctx.consensus.virtual_processor().reachability_service.is_chain_ancestor_of(fork_point, ctx.consensus.get_sink()),
        "both branches still share the fork point; nothing converged them"
    );
}

/// Confirmed-anchor TTL (`dns_veto_ttl_daa_score`) end to end on a real DAG — the release path for
/// a node defending an anchor its own branch no longer supports.
///
/// Shape (the testnet-20 dead-branch wedge, and what an isolated node lands in): a validator bonds
/// and attests once, the node confirms an anchor, then attestation stops while the node keeps
/// mining. `advance_dns_confirmation` carries that same anchor forward forever, so before the TTL
/// existed the veto stayed armed on a branch nothing was supporting, and only a >4x work override
/// or a fork deeper than the horizon could ever release it.
///
/// Both directions are asserted from the identical block script, so the TTL is provably the cause:
///   * TTL disabled (`u64::MAX`) → the heavier stake-less branch is REFUSED (this is also the
///     51%-attack property `dns_v3_pow_majority_cannot_rewrite_confirmed_anchor` pins).
///   * TTL exceeded            → the veto releases and the node follows the work-dominant chain.
///
/// The attacker is deliberately kept under the 4x override ratio, so in the control case the
/// refusal comes from the stake dimension and nothing else.
#[tokio::test]
async fn dns_stale_anchor_ttl_releases_a_dead_branch_wedge() {
    use crate::model::stores::{dns_state::DnsStateStoreReader, ghostdag::GhostdagStoreReader, headers::HeaderStoreReader};
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DnsRolloutStage, STAKE_SCORE_SCALE, StakeScore, ready_epoch_from_tip_blue_score},
    };
    kaspa_core::log::try_init_logger("info");

    /// Runs the same script under one TTL and reports whether the confirmed anchor survived on the
    /// honest node's selected chain after the heavier stake-less branch arrived.
    async fn anchor_survives_with_ttl(ttl: u64) -> bool {
        let config = ConfigBuilder::new(MAINNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|p| {
                p.max_block_parents = 4;
                p.mergeset_size_limit = 10;
                p.coinbase_maturity = 2;
                let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap(); // TwoDimensionalDominance
                dns.dns_activation_daa_score = 0;
                dns.pos_v2_activation_daa_score = 0;
                dns.epoch_length_blocks = 2;
                dns.reward_uniqueness_window_blocks = 50;
                // A generous gate horizon so the fork stays gate-eligible: the release under test
                // must be the TTL, never the horizon abstain.
                dns.max_reorg_horizon_blocks = 1000;
                dns.dns_gate_horizon_blocks = 5000;
                dns.dns_veto_ttl_daa_score = ttl;
                dns.attestation_epoch_length_blue_score = 3;
                dns.attestation_lag_blue_score = 2;
                dns.attestation_anchor_backoff_blue_score = 1;
                dns.stake_score_window_blue_score = 10_000;
                dns.required_work_depth = kaspa_consensus_core::BlueWorkType::ZERO;
                dns.required_stake_depth = StakeScore(STAKE_SCORE_SCALE / 2);
                p.dns_params = Some(dns);
            })
            .build();
        let mut ctx = TestContext::new(TestConsensus::new(&config));

        // ---- Honest node: bond a validator, attest once, reach a DNS-confirmed anchor. ----
        let seed = [0x42u8; 32];
        let v = dns_harness::harness_validator(seed);
        let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
        let k_spk = p2pkh_mldsa87_spk(&k_payload);
        let k_miner = MinerData::new(k_spk.clone(), vec![]);
        let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
        let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
        let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
        let cb_a = &h_a.transactions[0];
        let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
        let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
        let cb_b = &h_b.transactions[0];
        let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
        let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
        for _ in 0..5 {
            ctx.mine_block(new_miner_data(), vec![]).await;
        }
        let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
        let (bond_tx, _vid, _rp) =
            dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, value_a - 100_000, 0, storage_mass_parameter);
        let bond_tx_id = bond_tx.id();
        ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
        let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
        for _ in 0..8 {
            ctx.mine_block(new_miner_data(), vec![]).await;
        }
        let dns = ctx.consensus.params().dns_params.clone().unwrap();
        let genesis_hash = ctx.consensus.params().genesis.hash;
        let sink = ctx.consensus.get_sink();
        let anchor = {
            let vp = ctx.consensus.virtual_processor();
            let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
            let lr =
                ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
                    .expect("an epoch is ready");
            vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor")
        };
        let att = dns_harness::build_signed_attestation(
            &v,
            genesis_hash.as_byte_slice(),
            bond_outpoint,
            anchor.epoch,
            anchor.anchor_hash,
            anchor.anchor_daa_score,
            Hash64::default(),
        );
        let shard_tx = dns_harness::funded_signed_shard_tx(seed, coinbase_b, value_b, daa_b, att, storage_mass_parameter);
        ctx.mine_block(new_miner_data(), vec![shard_tx]).await;
        for _ in 0..15 {
            ctx.mine_block(new_miner_data(), vec![]).await;
        }
        let confirmed_anchor = {
            let vp = ctx.consensus.virtual_processor();
            let st = vp.dns_state_store.read().get().expect("DnsState");
            assert_eq!(st.rollout_stage, DnsRolloutStage::Active, "honest node is Active");
            assert_ne!(st.last_dns_confirmed_anchor, Hash64::default(), "honest node has a confirmed anchor");
            st.last_dns_confirmed_anchor
        };

        // ---- Attestation STOPS while the node keeps mining: the anchor freezes, the tip runs on. ----
        for _ in 0..40 {
            ctx.mine_block(new_miner_data(), vec![]).await;
        }
        let (honest_work, anchor_age) = {
            let vp = ctx.consensus.virtual_processor();
            let st = vp.dns_state_store.read().get().expect("DnsState");
            assert_eq!(st.last_dns_confirmed_anchor, confirmed_anchor, "no new confirmation: the same anchor is carried forward");
            let sink = ctx.consensus.get_sink();
            let sink_daa = vp.headers_store.get_daa_score(sink).unwrap();
            (vp.ghostdag_store.get_blue_work(sink).unwrap(), sink_daa.saturating_sub(st.last_dns_confirmed_anchor_daa_score))
        };
        assert!(anchor_age > 20, "the anchor must have aged measurably on the node's own chain (got {anchor_age})");

        // ---- A heavier, stake-less branch arrives — heavier, but under the 4x override ratio. ----
        let mut atk = TestContext::new(TestConsensus::new(&config));
        let mut attacker_blocks = Vec::new();
        for _ in 0..110 {
            attacker_blocks.push(atk.mine_block(new_miner_data(), vec![]).await);
        }
        let attacker_tip = attacker_blocks.last().unwrap().header.hash;
        let attacker_work = { atk.consensus.virtual_processor().ghostdag_store.get_blue_work(attacker_tip).unwrap() };
        assert!(attacker_work > honest_work, "the branch is genuinely heavier ({attacker_work} vs {honest_work})");
        let (four_x, overflowed) = honest_work.overflowing_mul_u64(4);
        assert!(!overflowed && attacker_work < four_x, "and stays UNDER the 4x work override, so only the TTL can explain a release");
        for b in &attacker_blocks {
            ctx.validate_and_insert_block(b.clone()).await;
        }

        let new_sink = ctx.consensus.get_sink();
        let vp = ctx.consensus.virtual_processor();
        let survived = vp.reachability_service.is_chain_ancestor_of(confirmed_anchor, new_sink);
        assert_eq!(
            survived,
            new_sink != attacker_tip,
            "the two ways of asking (anchor still on the chain / sink moved to the branch) must agree"
        );
        survived
    }

    assert!(
        anchor_survives_with_ttl(u64::MAX).await,
        "control: with the TTL disabled the veto never expires, so the heavier stake-less branch is refused"
    );
    assert!(
        !anchor_survives_with_ttl(20).await,
        "with the anchor aged past the TTL the veto releases and the node follows the work-dominant chain"
    );
}

/// **Qwen3.6's own three series, per epoch, on a two-class chain: expected, observed, target.**
///
/// The live testnet cannot answer this — testnet-11 registered no second class (its 2026-08-24
/// registration carrier was never mined), so its only class holds the whole table and the
/// retarget is an exact no-op by construction. This runs the same question against the same
/// consensus code on the two-class RC genesis, where the entrant is the REAL `PALW-QWEN36` class:
/// its own `shape_profile_id`, its own counted `pwu_per_inference`, its own registered target.
///
/// Every block here is a real block: template, attempt, class lottery, ML-DSA-87 signature, and
/// `validate_and_insert_block` through the virtual processor. What is reused rather than recomputed
/// is the EXECUTION behind one class's roots — one forward pass per class, then the nonce moves the
/// challenge. Acceptance never re-runs an execution (that is the panel's and the court's), so this
/// changes nothing the difficulty loop reads.
///
/// Run: `cargo test --release -p kaspa-consensus qwen36_per_epoch -- --ignored --nocapture`
#[tokio::test]
#[ignore = "produces thousands of real blocks; run explicitly with --ignored --nocapture"]
async fn palw_rc_qwen36_per_epoch_expected_observed_target() {
    use kaspa_consensus_core::api::ConsensusApi;
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
        challenge_v2, class_ticket_v3, execution_anchor_v3, palw_network_domain_v2_for,
    };
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    use kaspa_consensus_core::palw_qwen36_profile::{QWEN36_35B_A3B, QWEN36_RC_CANONICAL, qwen36_profile_v1};
    use misaka_palw_base0::produce::{base0_execute_for_attempt_v1, base0_rc_job_anchor_v1, base0_rc_job_v1};
    use misaka_palw_base0::qwen36_backend::Qwen36Backend;

    let epochs: u64 = std::env::var("MISAKA_EPOCHS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);

    // Every registry row gets a REAL key, so claims can be spread over more than one bond: a
    // 1000-block epoch holds ~`window_bind` claims open at once, and one bond is sized for one
    // class's span.
    let keys: Vec<_> = (0..kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1() as u32)
        .map(|i| libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0xB0u8.wrapping_add(i as u8); 32]))
        .collect();
    let registry: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(i, kp)| kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(
                i as u32,
            )),
            pubkey: kp.verification_key.as_ref().to_vec(),
            operator_pubkey: vec![21u8, i as u8, 0, 0, 0, 0, 0, 0],
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11 + i as u64),
        })
        .collect();

    let base_root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's artifact derives");
    let qwen_artifact = misaka_palw_base0::qwen36::qwen36_dev_fixture(4, 8);
    let qwen_root = qwen_artifact.artifact_root();
    let params = kaspa_consensus_core::config::params::palw_rc_params_with_qwen36(base_root, qwen_root, registry)
        .expect("the two-class genesis assembles");
    let bundle = match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("a ConsensusV2 network"),
    };
    let epoch_length = bundle.state.epoch_length();
    let qwen_class_id = qwen36_profile_v1(QWEN36_35B_A3B).expect("projects").shape_profile_id();
    let base_class_id = bundle.base_class_id;

    let config = ConfigBuilder::new(params)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            if !cfg!(feature = "evm") {
                p.evm_activation_daa_score = u64::MAX;
            }
        })
        .build();
    let network_domain = palw_network_domain_v2_for(config.params.net.to_string().as_bytes(), Some(config.params.genesis.hash));
    let ctx = TestContext::new(TestConsensus::new(&config));

    // One execution per class; the nonce does the rest.
    let base_profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
    let base_artifact = misaka_palw_base0::rc::palw_rc_base0_artifact_v1().expect("derives");
    let seed_anchor = base0_rc_job_anchor_v1(
        network_domain,
        kaspa_hashes::Hash64::from_u64_word(1),
        base_class_id,
        &kaspa_consensus_core::config::premine::premine_outpoint(0),
        0,
    );
    let (base_job, base_prompt) =
        base0_rc_job_v1(&base_profile, seed_anchor, base_artifact.shape.vocab, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    let base_run = base0_execute_for_attempt_v1(&base_artifact, &base_profile, &base_job, &base_prompt).expect("the floor runs");
    let qwen_backend = Qwen36Backend::new(
        std::sync::Arc::new(qwen_artifact),
        "Qwen3.6-dev-fixture",
        QWEN36_RC_CANONICAL,
        qwen_class_id,
        config.params.net.to_string().into_bytes(),
    );
    let qwen_anchor = base0_rc_job_anchor_v1(
        network_domain,
        kaspa_hashes::Hash64::from_u64_word(2),
        qwen_class_id,
        &kaspa_consensus_core::config::premine::premine_outpoint(1),
        0,
    );
    let (qwen_job, qwen_prompt) = qwen_backend.job_for_anchor(qwen_anchor).expect("the anchor implies a job");
    let qwen_run = qwen_backend.execute(&qwen_job, &qwen_prompt).expect("a real hybrid forward pass");
    // The four roots and the chunk count are all an attempt carries out of an execution, and the
    // two engines return different types — so the commitment material, not the run, is what the
    // loop below selects between.
    let base_roots =
        (base_run.trace_root, base_run.output_root, base_run.execution_root, base_run.trace_manifest_root, base_run.trace_chunk_count);
    let qwen_roots =
        (qwen_run.trace_root, qwen_run.output_root, qwen_run.execution_root, qwen_run.trace_manifest_root, qwen_run.trace_chunk_count);

    println!();
    println!("=== two-class RC chain: BASE-0 {} + QWEN36 {} ===", &base_class_id.to_string()[..16], &qwen_class_id.to_string()[..16]);
    println!("epoch_length {epoch_length}, entrant share {}‰", bundle.state.min_grantable_share_permille());
    println!(
        "{:>5} {:>7} {:>6} {:>9} {:>9} {:>7} {:>12} {:>16} {:>16}",
        "epoch", "class", "share", "expected", "observed", "budget", "pwu", "target before", "target after"
    );

    // Per epoch: how many blocks the entrant tries to produce.
    let qwen_quota = |epoch: u64| -> u64 { if epoch == 0 { 0 } else { 1 } };
    let mut refused_over_budget = 0u64;
    // The counters roll the moment the boundary block lands, so the closed epoch's numbers have to
    // be taken from inside it — the last read before the crossing is the epoch's final state.
    let mut last_in_epoch: Vec<(&'static str, u16, u64, u64, u64, u128, u64)> = Vec::new();

    for epoch in 0..epochs {
        loop {
            let daa = ctx.consensus.get_virtual_daa_score();
            if daa / epoch_length != epoch {
                break;
            }
            let produced_qwen = ctx
                .consensus
                .palw_producer_facts_v2(qwen_class_id, None)
                .map(|f| if f.epoch_index == epoch { f.epoch_produced_blocks } else { 0 })
                .unwrap_or(0);
            let want_qwen = produced_qwen < qwen_quota(epoch);
            let (class_id, bond_index) = if want_qwen { (qwen_class_id, 1u32) } else { (base_class_id, 0u32) };
            let bond = kaspa_consensus_core::config::premine::premine_outpoint(bond_index);
            let facts = ctx.consensus.palw_producer_facts_v2(class_id, Some(bond)).expect("a V2 network answers");
            let run = if want_qwen { qwen_roots } else { base_roots };
            let mut block = ctx.build_block_template_keeping_time(0).block;
            let timestamp = block.header.timestamp;
            let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
            let mut attempt = PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain,
                challenge: kaspa_hashes::Hash64::default(),
                class_id,
                executor_bond: bond,
                executor_pubkey: keys[bond_index as usize].verification_key.as_ref().to_vec(),
                operator_id: facts.bond.as_ref().expect("a genesis bond").operator_id,
                artifact_root: facts.artifact_root,
                trace_root: run.0,
                output_root: run.1,
                execution_root: run.2,
                pwu: facts.pwu,
                trace_manifest_root: run.3,
                trace_chunk_count: run.4,
                trace_retention_daa: block.header.daa_score.saturating_add(facts.min_trace_retention_daa),
            };
            // The draw walks the bucket (ADR-0072): the ticket is the execution's under the anchor the
            // header derives, so a nonce inside a bucket moves nothing and the next bucket is the next draw.
            let mut won = None;
            for bucket in 0u64..4096 {
                let nonce = bucket << kaspa_consensus_core::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2;
                let anchor = execution_anchor_v3(network_domain, pre_pow, class_id, &bond, nonce);
                if class_ticket_v3(&attempt, anchor) <= facts.class_target {
                    attempt.challenge = challenge_v2(network_domain, pre_pow, timestamp, nonce, class_id, &bond);
                    won = Some(nonce);
                    break;
                }
            }
            let nonce = won.expect("the class target is winnable");
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &keys[bond_index as usize].signing_key,
                attempt_id_v2(&attempt).as_byte_slice(),
                PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
                [0x5Au8; 32],
            )
            .expect("sign")
            .as_ref()
            .to_vec();
            block.header.nonce = nonce;
            block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
            block.header.finalize();
            let inserted = block.to_immutable();
            let status = ctx.consensus.validate_and_insert_block(inserted).virtual_state_task.await;
            match status {
                Ok(_) => {}
                Err(e) => {
                    let text = format!("{e:?}");
                    assert!(
                        text.contains("budget") || text.contains("exposure"),
                        "a block may only be refused for the budget or the bond, got: {text}"
                    );
                    refused_over_budget += 1;
                    if refused_over_budget > 4 {
                        panic!("the chain refused five blocks running: {text}");
                    }
                    break;
                }
            }
            // `palw_producer_facts_v2` answers for the CANDIDATE, so the last read inside an epoch
            // already describes the next one — keep only the reads that still describe this epoch.
            let captured: Vec<_> = [("BASE-0", base_class_id, 0u32), ("QWEN36", qwen_class_id, 1u32)]
                .into_iter()
                .map(|(name, class_id, bond_index)| {
                    let f = ctx
                        .consensus
                        .palw_producer_facts_v2(class_id, Some(kaspa_consensus_core::config::premine::premine_outpoint(bond_index)))
                        .expect("answers");
                    let share = ctx
                        .consensus
                        .palw_v2_class_table()
                        .into_iter()
                        .find(|r| r.class_id == class_id)
                        .and_then(|r| r.share_permille)
                        .unwrap_or(0);
                    (name, share, f.epoch_produced_blocks, f.epoch_budget_blocks, f.pwu, f.class_target, f.epoch_index)
                })
                .collect();
            if captured.iter().all(|row| row.6 == epoch) {
                last_in_epoch = captured;
            }
        }
        // The closed epoch's own numbers, captured inside it.
        let rows = std::mem::take(&mut last_in_epoch);
        let realized: u64 = rows.iter().map(|r| r.2).sum();
        for (name, share, observed, budget, pwu, target, seen_epoch) in rows {
            let expected = (realized * share as u64 + 500) / 1000;
            let class_id = if name == "BASE-0" { base_class_id } else { qwen_class_id };
            let after = ctx.consensus.palw_producer_facts_v2(class_id, None).map(|f| f.class_target).unwrap_or(target);
            println!(
                "{:>5} {:>7} {:>5}‰ {:>9} {:>9} {:>7} {:>12} {:>16.9} {:>16.9}",
                seen_epoch,
                name,
                share,
                expected,
                observed,
                budget,
                pwu,
                (target as f64 + 1.0) / (u128::MAX as f64 + 1.0),
                (after as f64 + 1.0) / (u128::MAX as f64 + 1.0)
            );
        }
    }
    println!("blocks refused for budget/exposure during the run: {refused_over_budget}");
}

/// **ADR-0054 under real block validation: the entrant's share moves at the boundary.**
///
/// `the_real_qwen36_class_earns_share_by_producing` drives the transition directly, which is where
/// the rule lives; this drives the BLOCKS — every one of them templated, ticketed, signed and
/// accepted — so the share the chain reports afterwards is one that survived the whole pipeline,
/// and it is read back through the same `palw_v2_class_table` an operator sees.
///
/// One epoch is 1,000 DAA and one block is one DAA, so this is a thousand real blocks; ignored in
/// CI for that reason.
#[tokio::test]
#[ignore = "produces a full 1000-block epoch; run with --ignored --nocapture"]
async fn palw_rc_qwen36_earns_share_through_real_blocks() {
    use kaspa_consensus_core::api::ConsensusApi;
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
        challenge_v2, class_ticket_v3, execution_anchor_v3, palw_network_domain_v2_for,
    };
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    use kaspa_consensus_core::palw_qwen36_profile::{QWEN36_35B_A3B, QWEN36_RC_CANONICAL, qwen36_profile_v1};
    use misaka_palw_base0::produce::{base0_execute_for_attempt_v1, base0_rc_job_anchor_v1, base0_rc_job_v1};
    use misaka_palw_base0::qwen36_backend::Qwen36Backend;

    let keys: Vec<_> = (0..kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1() as u32)
        .map(|i| libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0xB0u8.wrapping_add(i as u8); 32]))
        .collect();
    let registry: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(i, kp)| kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(
                i as u32,
            )),
            pubkey: kp.verification_key.as_ref().to_vec(),
            operator_pubkey: vec![21u8, i as u8, 0, 0, 0, 0, 0, 0],
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11 + i as u64),
        })
        .collect();
    let base_root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("derives");
    let qwen_artifact = misaka_palw_base0::qwen36::qwen36_dev_fixture(4, 8);
    let qwen_root = qwen_artifact.artifact_root();
    let params = kaspa_consensus_core::config::params::palw_rc_params_with_qwen36(base_root, qwen_root, registry)
        .expect("the two-class genesis assembles");
    let bundle = match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("a ConsensusV2 network"),
    };
    assert!(bundle.state.class_growth_permille() > 0, "the RC bundle carries ADR-0054");
    let epoch_length = bundle.state.epoch_length();
    let base_class_id = bundle.base_class_id;
    let qwen_class_id = qwen36_profile_v1(QWEN36_35B_A3B).expect("projects").shape_profile_id();

    let config = ConfigBuilder::new(params)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            if !cfg!(feature = "evm") {
                p.evm_activation_daa_score = u64::MAX;
            }
        })
        .build();
    let network_domain = palw_network_domain_v2_for(config.params.net.to_string().as_bytes(), Some(config.params.genesis.hash));
    let ctx = TestContext::new(TestConsensus::new(&config));

    // One execution per class; the nonce moves the challenge, which is what the ticket reads.
    let base_profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
    let base_artifact = misaka_palw_base0::rc::palw_rc_base0_artifact_v1().expect("derives");
    let seed = base0_rc_job_anchor_v1(
        network_domain,
        kaspa_hashes::Hash64::from_u64_word(1),
        base_class_id,
        &kaspa_consensus_core::config::premine::premine_outpoint(0),
        0,
    );
    let (base_job, base_prompt) =
        base0_rc_job_v1(&base_profile, seed, base_artifact.shape.vocab, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    let base_run = base0_execute_for_attempt_v1(&base_artifact, &base_profile, &base_job, &base_prompt).expect("the floor runs");
    let backend = Qwen36Backend::new(
        std::sync::Arc::new(qwen_artifact),
        "Qwen3.6-dev-fixture",
        QWEN36_RC_CANONICAL,
        qwen_class_id,
        config.params.net.to_string().into_bytes(),
    );
    let qwen_anchor = base0_rc_job_anchor_v1(
        network_domain,
        kaspa_hashes::Hash64::from_u64_word(2),
        qwen_class_id,
        &kaspa_consensus_core::config::premine::premine_outpoint(1),
        0,
    );
    let (qwen_job, qwen_prompt) = backend.job_for_anchor(qwen_anchor).expect("the anchor implies a job");
    let qwen_run = backend.execute(&qwen_job, &qwen_prompt).expect("a real hybrid forward pass");
    let base_roots =
        (base_run.trace_root, base_run.output_root, base_run.execution_root, base_run.trace_manifest_root, base_run.trace_chunk_count);
    let qwen_roots =
        (qwen_run.trace_root, qwen_run.output_root, qwen_run.execution_root, qwen_run.trace_manifest_root, qwen_run.trace_chunk_count);

    let share_of = |ctx: &TestContext, class_id| {
        ctx.consensus.palw_v2_class_table().into_iter().find(|r| r.class_id == class_id).and_then(|r| r.share_permille)
    };
    let opening_share = share_of(&ctx, qwen_class_id).expect("the entrant is in the table");
    assert_eq!(
        opening_share,
        kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN36_SHARE_PERMILLE,
        "the card funds the hybrid tier at genesis"
    );
    // The budget IS the share, so filling it is the only signal a class has — and the only way to
    // produce that signal is to make every block it is allowed to.
    let quota = ctx.consensus.palw_producer_facts_v2(qwen_class_id, None).expect("answers").epoch_budget_blocks;
    assert!(quota > 1, "a funded tier's allowance is more than one block");

    let mut qwen_made = 0u64;
    // One epoch, plus the block that crosses the boundary — the crossing is what pays.
    while ctx.consensus.get_virtual_daa_score() <= epoch_length {
        let want_qwen = qwen_made < quota;
        let (class_id, bond_index) = if want_qwen { (qwen_class_id, 1u32) } else { (base_class_id, 0u32) };
        let bond = kaspa_consensus_core::config::premine::premine_outpoint(bond_index);
        let facts = ctx.consensus.palw_producer_facts_v2(class_id, Some(bond)).expect("a V2 network answers");
        let run = if want_qwen { qwen_roots } else { base_roots };
        let mut block = ctx.build_block_template_keeping_time(0).block;
        let timestamp = block.header.timestamp;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
        let mut attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: kaspa_hashes::Hash64::default(),
            class_id,
            executor_bond: bond,
            executor_pubkey: keys[bond_index as usize].verification_key.as_ref().to_vec(),
            operator_id: facts.bond.as_ref().expect("a genesis bond").operator_id,
            artifact_root: facts.artifact_root,
            trace_root: run.0,
            output_root: run.1,
            execution_root: run.2,
            pwu: facts.pwu,
            trace_manifest_root: run.3,
            trace_chunk_count: run.4,
            trace_retention_daa: block.header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        // The draw walks the bucket (ADR-0072): the ticket is the execution's under the anchor the
        // header derives, so a nonce inside a bucket moves nothing and the next bucket is the next draw.
        let mut won = None;
        for bucket in 0u64..4096 {
            let nonce = bucket << kaspa_consensus_core::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2;
            let anchor = execution_anchor_v3(network_domain, pre_pow, class_id, &bond, nonce);
            if class_ticket_v3(&attempt, anchor) <= facts.class_target {
                attempt.challenge = challenge_v2(network_domain, pre_pow, timestamp, nonce, class_id, &bond);
                won = Some(nonce);
                break;
            }
        }
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &keys[bond_index as usize].signing_key,
            attempt_id_v2(&attempt).as_byte_slice(),
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
            [0x5Au8; 32],
        )
        .expect("sign")
        .as_ref()
        .to_vec();
        block.header.nonce = won.expect("the class target is winnable");
        block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
        block.header.finalize();
        ctx.consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.expect("the chain accepts it");
        if want_qwen {
            qwen_made += 1;
        }
    }

    {
        // The closed epoch as the rule read it. Kept because the first version of this test passed
        // its own loop counter and failed its assertion: 200 blocks built, 15 accepted, and only
        // the store could say which of those two numbers the chain believed.
        let vp = ctx.consensus.virtual_processor();
        let (_, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
        let counted = state.epoch_counter(&qwen_class_id).map(|c| (c.epoch_index, c.produced_blocks));
        eprintln!("at the crossing: counter {counted:?}");
        assert_eq!(counted, Some((0, quota)), "every block the loop built was accepted and counted");
    }
    let qwen_share = share_of(&ctx, qwen_class_id).expect("the entrant is in the table");
    let base_share = share_of(&ctx, base_class_id).expect("the floor is in the table");
    eprintln!(
        "after one epoch of real blocks: QWEN36 {qwen_share} permille (from {opening_share}, having made {qwen_made} of {quota}), BASE-0 {base_share} permille"
    );
    assert_eq!(qwen_made, quota, "it made every block its allowance permitted");
    let step = (u32::from(opening_share) * 250 / 1000).max(1) as u16;
    assert_eq!(qwen_share, opening_share + step, "it filled its budget, so it took a step from the floor");
    assert_eq!(qwen_share + base_share, 1000, "and the denominator is conserved through the block path");
    let facts = ctx.consensus.palw_producer_facts_v2(qwen_class_id, None).expect("answers");
    assert_eq!(facts.epoch_budget_blocks as u16, qwen_share, "the new epoch's budget follows the new share");
}

/// **ADR-0058 under real block validation: a class that never wins a chain slot still counts.**
///
/// The sibling test above hands the entrant every chain slot its budget allows. This one hands it
/// NONE — every Qwen block is built on a parent four chain blocks stale, so it always loses tip
/// selection by blue work and is only ever MERGED — which is testnet-11 as measured (12 of 12
/// real-inference blocks merged, zero on the selected chain, share frozen at genesis forever).
/// The assertions are the mechanism ADR-0058 restores: the epoch counter fills from the mergeset,
/// the budget cap holds against over-production (three extra blocks are skipped, not counted),
/// the share steps up at the boundary exactly as if the blocks had been chain blocks, and every
/// merged claim names its CARRYING block and escrows nothing.
///
/// One epoch is 1,000 DAA and one block is one DAA; ignored in CI for the same reason as above.
#[tokio::test]
#[ignore = "produces a full 1000-block epoch; run with --ignored --nocapture"]
async fn palw_rc_qwen36_counts_merged_work() {
    kaspa_core::log::try_init_logger("info");
    use kaspa_consensus_core::api::ConsensusApi;
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
        challenge_v2, class_ticket_v3, execution_anchor_v3, palw_network_domain_v2_for,
    };
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    use kaspa_consensus_core::palw_qwen36_profile::{QWEN36_35B_A3B, QWEN36_RC_CANONICAL, qwen36_profile_v1};
    use misaka_palw_base0::produce::{base0_execute_for_attempt_v1, base0_rc_job_anchor_v1, base0_rc_job_v1};
    use misaka_palw_base0::qwen36_backend::Qwen36Backend;
    use std::collections::HashSet;

    let keys: Vec<_> = (0..kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1() as u32)
        .map(|i| libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0xB0u8.wrapping_add(i as u8); 32]))
        .collect();
    let registry: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(i, kp)| kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(
                i as u32,
            )),
            pubkey: kp.verification_key.as_ref().to_vec(),
            operator_pubkey: vec![21u8, i as u8, 0, 0, 0, 0, 0, 0],
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11 + i as u64),
        })
        .collect();
    let base_root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("derives");
    let qwen_artifact = misaka_palw_base0::qwen36::qwen36_dev_fixture(4, 8);
    let qwen_root = qwen_artifact.artifact_root();
    let params = kaspa_consensus_core::config::params::palw_rc_params_with_qwen36(base_root, qwen_root, registry)
        .expect("the two-class genesis assembles");
    let bundle = match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("a ConsensusV2 network"),
    };
    let epoch_length = bundle.state.epoch_length();
    let base_class_id = bundle.base_class_id;
    let qwen_class_id = qwen36_profile_v1(QWEN36_35B_A3B).expect("projects").shape_profile_id();

    let config = ConfigBuilder::new(params)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            if !cfg!(feature = "evm") {
                p.evm_activation_daa_score = u64::MAX;
            }
        })
        .build();
    let network_domain = palw_network_domain_v2_for(config.params.net.to_string().as_bytes(), Some(config.params.genesis.hash));
    let ctx = TestContext::new(TestConsensus::new(&config));

    let base_profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
    let base_artifact = misaka_palw_base0::rc::palw_rc_base0_artifact_v1().expect("derives");
    let seed = base0_rc_job_anchor_v1(
        network_domain,
        kaspa_hashes::Hash64::from_u64_word(1),
        base_class_id,
        &kaspa_consensus_core::config::premine::premine_outpoint(0),
        0,
    );
    let (base_job, base_prompt) =
        base0_rc_job_v1(&base_profile, seed, base_artifact.shape.vocab, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    let base_run = base0_execute_for_attempt_v1(&base_artifact, &base_profile, &base_job, &base_prompt).expect("the floor runs");
    let backend = Qwen36Backend::new(
        std::sync::Arc::new(qwen_artifact),
        "Qwen3.6-dev-fixture",
        QWEN36_RC_CANONICAL,
        qwen_class_id,
        config.params.net.to_string().into_bytes(),
    );
    let qwen_anchor = base0_rc_job_anchor_v1(
        network_domain,
        kaspa_hashes::Hash64::from_u64_word(2),
        qwen_class_id,
        &kaspa_consensus_core::config::premine::premine_outpoint(1),
        0,
    );
    let (qwen_job, qwen_prompt) = backend.job_for_anchor(qwen_anchor).expect("the anchor implies a job");
    let qwen_run = backend.execute(&qwen_job, &qwen_prompt).expect("a real hybrid forward pass");
    let base_roots =
        (base_run.trace_root, base_run.output_root, base_run.execution_root, base_run.trace_manifest_root, base_run.trace_chunk_count);
    let qwen_roots =
        (qwen_run.trace_root, qwen_run.output_root, qwen_run.execution_root, qwen_run.trace_manifest_root, qwen_run.trace_chunk_count);

    let share_of = |ctx: &TestContext, class_id| {
        ctx.consensus.palw_v2_class_table().into_iter().find(|r| r.class_id == class_id).and_then(|r| r.share_permille)
    };
    let opening_share = share_of(&ctx, qwen_class_id).expect("the entrant is in the table");
    let quota = ctx.consensus.palw_producer_facts_v2(qwen_class_id, None).expect("answers").epoch_budget_blocks;
    assert!(quota > 1, "a funded tier's allowance is more than one block");

    // How stale a Qwen block's parent is, in chain blocks. Four floor blocks of blue work is the
    // margin the real network showed (2M–14M ≈ 2–10 blocks), and it guarantees the side block
    // loses tip selection here for the same reason it loses it there.
    const STALE_DEPTH: usize = 4;
    // Three blocks past the budget: the cap must SKIP them (deterministically, every node),
    // not count them — over-production is the attack the budget exists for.
    let overproduce = quota + 3;

    let mut chain_tips: Vec<BlockHash> = vec![config.genesis.hash];
    let mut qwen_blocks: HashSet<BlockHash> = HashSet::new();
    let mut qwen_made = 0u64;
    let sign = |keys: &[libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair], bond_index: u32, attempt: &PalwAttemptUnsignedV2| {
        libcrux_ml_dsa::ml_dsa_87::sign(
            &keys[bond_index as usize].signing_key,
            attempt_id_v2(attempt).as_byte_slice(),
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
            [0x5Au8; 32],
        )
        .expect("sign")
        .as_ref()
        .to_vec()
    };

    while ctx.consensus.get_virtual_daa_score() <= epoch_length {
        // A Qwen SIDE block first, whenever allowance (plus the deliberate excess) remains and
        // the chain is deep enough to have a stale parent for it.
        if qwen_made < overproduce && chain_tips.len() > STALE_DEPTH {
            let stale = chain_tips[chain_tips.len() - 1 - STALE_DEPTH];
            let bond = kaspa_consensus_core::config::premine::premine_outpoint(1);
            let facts = ctx.consensus.palw_producer_facts_v2(qwen_class_id, Some(bond)).expect("a V2 network answers");
            // NOT the UTXO-valid template builder: its pov sink search runs the deep-reorg
            // gate, which correctly refuses to re-anchor virtual on a stale parent (and then
            // walks past genesis into ORIGIN). A side blue needs no UTXO validity — only a
            // chain-block candidate is ever UTXO-validated, and this block must never be one.
            let mut block = ctx.consensus.build_block_with_parents_and_transactions(blockhash::NONE, vec![stale], vec![]);
            let timestamp = block.header.timestamp;
            let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
            let mut attempt = PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain,
                challenge: kaspa_hashes::Hash64::default(),
                class_id: qwen_class_id,
                executor_bond: bond,
                executor_pubkey: keys[1].verification_key.as_ref().to_vec(),
                operator_id: facts.bond.as_ref().expect("a genesis bond").operator_id,
                artifact_root: facts.artifact_root,
                trace_root: qwen_roots.0,
                output_root: qwen_roots.1,
                execution_root: qwen_roots.2,
                pwu: facts.pwu,
                trace_manifest_root: qwen_roots.3,
                trace_chunk_count: qwen_roots.4,
                trace_retention_daa: block.header.daa_score.saturating_add(facts.min_trace_retention_daa),
            };
            // The draw walks the bucket (ADR-0072): the ticket is the execution's under the anchor the
            // header derives, so a nonce inside a bucket moves nothing and the next bucket is the next draw.
            let mut won = None;
            for bucket in 0u64..4096 {
                let nonce = bucket << kaspa_consensus_core::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2;
                let anchor = execution_anchor_v3(network_domain, pre_pow, qwen_class_id, &bond, nonce);
                if class_ticket_v3(&attempt, anchor) <= facts.class_target {
                    attempt.challenge = challenge_v2(network_domain, pre_pow, timestamp, nonce, qwen_class_id, &bond);
                    won = Some(nonce);
                    break;
                }
            }
            let signature = sign(&keys, 1, &attempt);
            block.header.nonce = won.expect("the class target is winnable");
            block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
            block.header.finalize();
            let qwen_hash = block.header.hash;
            ctx.consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.expect("the DAG accepts it");
            qwen_blocks.insert(qwen_hash);
            qwen_made += 1;
        }

        // Then the floor's chain block, which merges whatever the anticone holds.
        {
            let bond = kaspa_consensus_core::config::premine::premine_outpoint(0);
            let facts = ctx.consensus.palw_producer_facts_v2(base_class_id, Some(bond)).expect("a V2 network answers");
            let mut block = ctx.build_block_template_keeping_time(0).block;
            let timestamp = block.header.timestamp;
            let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
            let mut attempt = PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain,
                challenge: kaspa_hashes::Hash64::default(),
                class_id: base_class_id,
                executor_bond: bond,
                executor_pubkey: keys[0].verification_key.as_ref().to_vec(),
                operator_id: facts.bond.as_ref().expect("a genesis bond").operator_id,
                artifact_root: facts.artifact_root,
                trace_root: base_roots.0,
                output_root: base_roots.1,
                execution_root: base_roots.2,
                pwu: facts.pwu,
                trace_manifest_root: base_roots.3,
                trace_chunk_count: base_roots.4,
                trace_retention_daa: block.header.daa_score.saturating_add(facts.min_trace_retention_daa),
            };
            // The draw walks the bucket (ADR-0072): the ticket is the execution's under the anchor the
            // header derives, so a nonce inside a bucket moves nothing and the next bucket is the next draw.
            let mut won = None;
            for bucket in 0u64..4096 {
                let nonce = bucket << kaspa_consensus_core::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2;
                let anchor = execution_anchor_v3(network_domain, pre_pow, base_class_id, &bond, nonce);
                if class_ticket_v3(&attempt, anchor) <= facts.class_target {
                    attempt.challenge = challenge_v2(network_domain, pre_pow, timestamp, nonce, base_class_id, &bond);
                    won = Some(nonce);
                    break;
                }
            }
            let signature = sign(&keys, 0, &attempt);
            block.header.nonce = won.expect("the class target is winnable");
            block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
            block.header.finalize();
            ctx.consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.expect("the chain accepts it");
            chain_tips.push(ctx.consensus.get_sink());
        }
    }

    // 1. Not one Qwen block won a chain slot — the starvation is real, not assumed.
    let chain: HashSet<BlockHash> = ctx
        .consensus
        .get_virtual_chain_from_block(config.genesis.hash, None)
        .expect("the chain walks")
        .added
        .iter()
        .copied()
        .collect();
    assert!(qwen_blocks.iter().all(|q| !chain.contains(q)), "every Qwen block stayed off the selected chain");

    // 1b. And not because they were quietly blue: at ghostdag_k = 1 a four-deep side block's
    // anticone makes it a RED by construction, so this test only proves ADR-0058 if the works
    // it counted rode the red half of the mergeset — which is where every slower-than-the-floor
    // class lives at the frozen cadence.
    {
        use crate::model::stores::ghostdag::GhostdagStoreReader;
        let vp_diag = ctx.consensus.virtual_processor();
        let mut in_blues = 0usize;
        let mut in_reds = 0usize;
        for cb in &chain {
            if let Ok(gd) = vp_diag.ghostdag_store.get_data(*cb) {
                in_blues += gd.mergeset_blues.iter().filter(|b| qwen_blocks.contains(*b)).count();
                in_reds += gd.mergeset_reds.iter().filter(|b| qwen_blocks.contains(*b)).count();
            }
        }
        eprintln!("side blocks: {} total, {in_blues} merged as blues, {in_reds} as reds", qwen_blocks.len());
        assert_eq!(in_blues + in_reds, qwen_blocks.len(), "every side block was merged exactly once");
        assert!(in_reds > 0, "at k = 1 the side blocks land in the red set — the set the old rule never read");
    }

    // 2. The counter filled from the MERGESET, and the budget cap held against the excess.
    let vp = ctx.consensus.virtual_processor();
    let (_, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let counted = state.epoch_counter(&qwen_class_id).map(|c| (c.epoch_index, c.produced_blocks));
    let base_counted = state.epoch_counter(&base_class_id).map(|c| (c.epoch_index, c.produced_blocks));
    let qwen_claims_total = state.claims_iter().filter(|(_, c)| c.class_id == qwen_class_id).count();
    let all_claims_total = state.claims_iter().count();
    eprintln!(
        "at the crossing: qwen counter {counted:?}, base counter {base_counted:?}, qwen claims in state {qwen_claims_total}, all claims {all_claims_total}, side blocks built {qwen_made} (budget {quota})"
    );
    assert_eq!(counted, Some((0, quota)), "merged production filled the budget exactly; the three excess blocks were skipped");

    // 3. Every merged claim names its CARRYING block and escrows nothing (ADR-0058 D4/D5).
    let mut merged_claims = 0usize;
    for (_, claim) in state.claims_iter().filter(|(_, c)| c.class_id == qwen_class_id) {
        assert!(qwen_blocks.contains(&claim.accepted_block), "a merged claim names the blue that carried it");
        assert_eq!(claim.escrowed_reward, 0, "a merged claim escrows nothing — its blue was paid by the coinbase in full");
        merged_claims += 1;
    }
    assert!(merged_claims > 0, "the tip state still holds merged claims");

    // 4. The boundary paid: the share stepped up off merged production alone.
    let qwen_share = share_of(&ctx, qwen_class_id).expect("the entrant is in the table");
    let base_share = share_of(&ctx, base_class_id).expect("the floor is in the table");
    eprintln!(
        "after one epoch of MERGED-ONLY production: QWEN36 {qwen_share} permille (from {opening_share}), BASE-0 {base_share} permille"
    );
    let step = (u32::from(opening_share) * 250 / 1000).max(1) as u16;
    assert_eq!(qwen_share, opening_share + step, "it filled its budget from the anticone, so it took a step from the floor");
    assert_eq!(qwen_share + base_share, 1000, "and the denominator is conserved");
}

/// **A caller's prompt, run on a registered class, opens a claim at the SHIPPED quantum.**
///
/// This test used to assert the opposite — that no registered class could earn a single draw —
/// because the shipped quantum was 1,000 and the widest job a class could hold (BASE-0's n_ctx 12
/// → 705 CU) floored to zero. Lowering the quantum to 100, with `pwu_per_quantum` lowered in step
/// so a given CU total keeps the exact chain weight it had, is what changed: the floor's own
/// maximum job now earns real quanta, and its commitment is taken by the same extraction the
/// virtual processor runs on every accepted block — the shipped parameters, not a rebuilt set.
///
/// The comment above `QUANTUM_CU` records the arithmetic; this is its consequence on the chain
/// path: a person's prompt, executed on the class the chain registered, becomes a claim the chain
/// opens.
#[tokio::test]
async fn a_callers_prompt_on_a_registered_class_opens_a_claim_at_the_shipped_quantum() {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_commitment_v3};
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT, PALW_FP_V3_VERSION, PalwFpCommitmentTxPayloadV3,
        PalwFreePromptJobV3, fp_claim_id_v3,
    };
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
    use kaspa_consensus_core::tx::{Transaction, TransactionId, TransactionOutpoint};
    use kaspa_hashes::Hash64;
    use misaka_palw_base0::backend::Base0Backend;
    use misaka_palw_base0::classes::{canonical_class_by_model_id_v1, resolve_class_v1};

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 64);

    // The floor, resolved the way a node resolves it: from nothing but its registered root.
    let court = bundle.court;
    let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor is a canonical class");
    let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root");
    let backend = Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("the floor resolves"));

    // **The caller's tokens.** Not derived from an anchor — that is the attempt lane's rule, and
    // the whole point of this lane is that a person chooses the input.
    // The largest job this class can hold: one prefill token and the rest decode, which is what
    // maximises `cu` under these weights. It is still short of a quantum, and the assertions below
    // are about that rather than about this particular prompt.
    let ctx_max = backend.profile().n_ctx as usize;
    let prompt: Vec<usize> = vec![11];
    let decode = (ctx_max - prompt.len()) as u32;
    let bond = TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0);
    let keypair = crate::consensus::test_consensus::TestConsensus::palw_v2_harness_keypair();
    let job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            MAINNET_PARAMS.net.to_string().as_bytes(),
            Some(MAINNET_PARAMS.genesis.hash),
        ),
        class_id: entry.class_id(),
        executor_bond: bond,
        executor_pubkey: crate::consensus::test_consensus::TestConsensus::palw_v2_harness_pubkey(),
        operator_id: Hash64::from_u64_word(0x0B),
        anchor_block: MAINNET_PARAMS.genesis.hash,
        anchor_daa: 1,
        job_nonce: [0x5A; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(
            &prompt.iter().map(|t| *t as u32).collect::<Vec<_>>(),
        ),
        prompt_tokens: prompt.len() as u32,
        decode_token_limit: decode,
        max_context_tokens: backend.profile().n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
    };

    let run = backend.execute_free_prompt(&job, &prompt).expect("the floor runs a caller's prompt");
    let class = PalwFpClassFactsV3 {
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: backend.profile().shape_profile_id(),
        cu_ruleset_id: Hash64::default(),
    };
    let commitment =
        palw_fp_commitment_v3(&job, &class, &run, b"misaka-palw-rc", 4_096).expect("a finished run assembles a commitment");

    // Signed over the claim id, which is what the extraction verifies — the bond answering for the
    // work, not a fixture asserting that somebody would have.
    let claim_id_signed = fp_claim_id_v3(&commitment);
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &keypair.signing_key,
        claim_id_signed.as_byte_slice(),
        PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT,
        [0u8; 32],
    )
    .expect("the harness signs")
    .as_ref()
    .to_vec();

    let committed_leaves = commitment.work_leaves;
    let payload = borsh::to_vec(&PalwFpCommitmentTxPayloadV3 {
        version: PALW_FP_V3_VERSION,
        commitment,
        prompt_token_ids: prompt.iter().map(|t| *t as u32).collect(),
        signature,
    })
    .expect("the commitment payload serializes");
    let tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT.clone(), 0, payload);

    // **The shipped extraction, on the shipped bundle.** No parameters rebuilt: this is the
    // function the virtual processor calls, with the quantum the network actually ships.
    let extraction = kaspa_consensus_core::palw_fp_objects_v3::palw_fp_objects_from_accepted_txs_v3(
        std::slice::from_ref(&tx),
        job.network_domain,
        &bundle.freeprompt,
        kaspa_consensus_core::BlockHash::default(),
        |pubkey: &[u8], message: &[u8], context: &[u8], signature: &[u8]| {
            kaspa_txscript::verify_mldsa87_with_context(pubkey, message, context, signature).unwrap_or(false)
        },
    );

    // The floor's widest job clears the quantum now, so its commitment opens a claim.
    // The floor's quantum is an eighth of its canonical job (ADR-0074 Decision 5); this job of a
    // few tokens must earn at least one, or the lane is unreachable on the class it ships with.
    let (canonical_ctx, _) = backend.job_for_anchor(Hash64::from_u64_word(0xF1)).expect("the floor implies a canonical job");
    let canonical_leaves = kaspa_consensus_core::palw_step::step_leaf_count(backend.profile(), &canonical_ctx).expect("counts");
    let quantum = kaspa_consensus_core::palw_freeprompt_v3::fp_class_quantum_leaves_v1(
        canonical_leaves,
        bundle.freeprompt.quanta_per_canonical_job(),
    );
    assert!(
        committed_leaves >= quantum,
        "this job ({committed_leaves} leaves) must reach the floor's {quantum}-leaf quantum (canonical job {canonical_leaves} leaves)"
    );
    assert!(extraction.skipped.is_empty(), "a job that earns a draw is not skipped: {:?}", extraction.skipped);
    let [carried] = &extraction.objects[..] else { panic!("exactly one object rides a commitment") };
    let Obj::FreePromptCommitted { claim, class_id, .. } = &carried.object else {
        panic!("and it commits a free-prompt claim: {:?}", carried.object)
    };
    assert_eq!(*claim, claim_id_signed, "the claim the chain opened is the one the executor signed");
    assert_eq!(*class_id, entry.class_id(), "under the class that ran it");
}

/// **A receipt block carrying a real certified claim passes the header admission gate.**
///
/// The end-to-end admission — a real execution's claim, certified, its quantum spend accepted by
/// `check_palw_receipt_spend_admission_full_v3` — is proven in
/// `misaka_palw_base0::backend::end_to_end_tests`. This adds the piece that lives in the pipeline:
/// the receipt carriage a producer builds, on a real header, gets past the HEADER stage's
/// signature gate — the check `validate_and_insert_block` runs before a block is a block at all.
///
/// It stops there deliberately. Whether the block then becomes the SINK is a stateful question the
/// virtual processor answers on the chain candidate, and reaching it honestly needs the claim
/// certified ON the mined chain — windows of DAA (bind 600, challenge 1,200) the frozen cadence
/// makes into hours and this harness cannot cheaply advance without tripping the epoch-budget and
/// DAA-progress rules that govern a real chain. The admission logic is proven where it is pure;
/// this proves the header gate accepts the producer's carriage, and names what it does not cover.
#[tokio::test]
async fn a_receipt_carriage_passes_the_header_signature_gate() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 64);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();

    // A claim id and bond a producer would name — the harness bond is genesis row 0, whose REAL
    // key the signature verifies against. The claim need not be on chain for the HEADER gate: that
    // gate checks the signature over the header position, and the stateful "is this claim Final"
    // question is asked later, on the candidate (measured in the receipt-gate test above).
    let bond = TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0);
    let claim_id = kaspa_hashes::Hash64::from_u64_word(0xFEED);
    let beacon_block = kaspa_hashes::Hash64::from_u64_word(0xBEAC);

    let honest = ctx.build_block_template(9, ctx.simulated_time + 1);

    // Junk signature — the stateless gate must refuse it, naming the signature.
    let mut forged = honest.block.clone();
    forged.header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3;
    forged.header.palw_commitment =
        ctx.consensus.palw_v3_test_receipt_carriage_for(&forged.header, false, claim_id, 0, bond, beacon_block);
    forged.header.finalize();
    match ctx.consensus.validate_and_insert_block(forged.to_immutable()).virtual_state_task.await {
        Err(kaspa_consensus_core::errors::block::RuleError::BadPalwCarriageAdmission { algo_id: 7, reason }) => {
            assert!(reason.to_lowercase().contains("signature"), "the refusal names the signature: {reason}");
        }
        other => panic!("a junk-signature receipt carriage must be refused at the header stage, got {other:?}"),
    }

    // Real signature over the same spend — the producer's carriage — must get PAST the signature
    // gate. It is refused later on the stateful facts (no such Final claim on this chain), which is
    // a different complaint and deliberately not asserted here.
    let mut signed = honest.block.clone();
    signed.header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3;
    signed.header.palw_commitment =
        ctx.consensus.palw_v3_test_receipt_carriage_for(&signed.header, true, claim_id, 0, bond, beacon_block);
    signed.header.finalize();
    if let Err(kaspa_consensus_core::errors::block::RuleError::BadPalwCarriageAdmission { algo_id: 7, reason }) =
        ctx.consensus.validate_and_insert_block(signed.to_immutable()).virtual_state_task.await
    {
        assert!(
            !reason.to_lowercase().contains("signature"),
            "a correctly signed spend must not be refused for its signature: {reason}"
        );
    }
}
