//! **ADR-0165 on testnet-12's own GHOSTDAG: the slow REAL attempt, what colours it, and what carries its
//! tick** — the 2026-10-03 live finding (evidence `lanes/evidence/8k-red-1003/`): in the last 300 DAA, 72
//! attempts of the 8k class, **71 RED and 1 BLUE, every RED caused by floor attempts** (141 counted peers,
//! all floor; heartbeats counted 0 — ADR-0105's `LaneColoring::Weighted` makes them invisible to a bonded
//! candidate). The 8k producer infers for ~340 s (p50; p95 418 s) while the floors — 3 a slot — extend the
//! chain, and at `ghostdag_k = 1` two floors in an attempt's anticone make it red.
//!
//! The chain is testnet-12 as launched (the harness cards, EVM inert, every window as shipped), with the 8k
//! class — the genesis model row that is neither the floor nor held to Final — planted `Active` on eight ready
//! seats the way `t12_rcore_s6_producer_share` plants its short row (the registry would take ~30 spans of
//! readiness proofs to do it; a planted tip is the harness's way). A REAL attempt here is a real block of that
//! class, built exactly as the floor's are (`T12Chain::build_attempt_drawn`'s construction, class-generic),
//! signed by a card, and folded by the real pipeline; the DAG, the colouring, the fold and the floor state
//! machine are the node's own.
//!
//! * `…red_under_floors_blue_under_heartbeats…` — **before the fence**: a slow REAL attempt with two or three
//!   floors in its anticone goes RED (the live finding, reproduced), and with any number of heartbeats it is
//!   the selected parent (BLUE).
//! * `…a_busy_chain_refuses_floors…` — **past the fence**: a REAL attempt accepted BLUE makes the floor state
//!   Normal, and the floor is refused by name (`FloorNotIdle`) at the producer's pre-check and by the fold; only
//!   heartbeats extend the chain under the slow attempt, and it is BLUE and accepted, refreshing Normal.
//! * `…a_rogue_floor…` — the residual, stated: a producer that ignores the pre-check still mines floor-lane
//!   blocks, which colour classically (the colouring is header-stage and cannot read the class or the state),
//!   so the slow attempt goes RED — but those blocks carry NO claim (no reward, no weight), and the RED
//!   attempt is STILL applied by the merging block's fold, yet **a RED attempt never extends Normal**.
//! * `…a_red_first_attempt_opens_a_probe…` — after Normal has run out the floor resumes, so the first REAL attempt
//!   after an idle stretch can go RED; it is accepted all the same and **opens a Probe** (floors refused for
//!   `probe_slots`), under which the NEXT slow attempt is BLUE and makes the state Normal; a Probe nobody answers
//!   expires and the floor resumes.
//! * `…a_pruned_join_inside_a_normal_stretch…`, `…_inside_a_probe…` — a node that imports its pruning point inside
//!   Normal or a Probe decides the floor, block for block, as the archival node does (the floor state travels in the
//!   carriage, T49-style).
//! * `…carries_its_slots_tick…` — B: with no heartbeat in the chain a REAL attempt carries its slot's tick,
//!   one tick a slot however many attempt-lane blocks the slot holds; the control (the fence unarmed) ticks
//!   not at all.
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards};
use super::OnetimeTxSelector;
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::headers::HeaderStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::{PalwModelLifecycleV1, PalwSeatReadinessRowV1};
use kaspa_consensus_core::palw_real_share_v1::PalwFloorModeV1::{Idle, Normal, Probe};
use kaspa_consensus_core::palw_real_share_v1::{
    PALW_FLOOR_ANCHOR_DUTY_AFTER_SLOTS_V1 as DUTY_AFTER, PALW_FLOOR_ANCHOR_DUTY_STAGGER_SLOTS_V1 as DUTY_STAGGER, PALW_FLOOR_IDLE_SLOTS_V1 as IDLE,
    PALW_FLOOR_PROBE_SLOTS_V1 as PROBE, PALW_T12_USEFUL_WORK_FENCES_V1, PalwFloorAnchorDutyInputV1, PalwFloorAnchorDutyWaitV1,
    PalwFloorStateV1, palw_floor_anchor_duty_stagger_v1, palw_floor_anchor_duty_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
use kaspa_hashes::Hash64;
use libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair;

/// The 8k producer's inference, as P2 measured it on the live chain (p50 342 s): the drill's default delay.
const INFERENCE_MS: u64 = 340_000;
/// One clock slot.
const SLOT: u64 = kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS;

/// A harness chain's ingredients: the config, the PALW bundle, the premine and the fee floats.
type Premine = Vec<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)>;
type Parts = (Config, PalwConsensusParamsV2, Premine, Premine);

fn card_key(i: usize) -> &'static MLDSA87KeyPair {
    TestConsensus::palw_v2_registry_keypair(i as u64)
}

fn card_pubkey(i: usize) -> Vec<u8> {
    card_key(i).verification_key.as_ref().to_vec()
}

/// How a block stands in a merging block's GHOSTDAG data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Colour {
    SelectedParent,
    Blue,
    Red,
    NotMerged,
}

struct Rig {
    c: T12Chain,
    /// The 8k class: the genesis model row that is neither the floor nor held to Final.
    k8: Hash64,
    /// Our own nonce space (the chain's is private), clear of the harness's.
    nonce: u64,
    /// Every plant made, with the sink it was made on: the test's hand, which a peer cannot send — a second node following
    /// this chain installs each at the same block before the next one arrives (`follower`).
    plants: Vec<(BlockHash, kaspa_consensus_core::palw_state_v2::PalwChainStateV2)>,
}

/// testnet-12 as launched, with the Useful Work Transition's two fences armed from genesis when `fences`.
fn config_of(fences: bool) -> Parts {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    if !fences {
        return (config, bundle, premine, floats);
    }
    let mut params = config.params.clone();
    for fence in PALW_T12_USEFUL_WORK_FENCES_V1 {
        (fence.set)(&mut params, Some(ForkActivation::always()));
    }
    params.validate_palw_v2().expect("the Useful Work Transition validates on testnet-12 from genesis");
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    let bundle = match &config.params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => unreachable!("testnet-12 is ConsensusV2"),
    };
    assert!(bundle.state.floor_reserve_active_at(0), "the fold's mirror follows the fence");
    (config, bundle, premine, floats)
}

/// **testnet-12 with the Useful Work Transition AND lane A** (the operator-anchored panel, with lane F1's seed under it, and the
/// bind-deadlock fence whose `binder_due` the anchor duty reads) — the post-launch fences a REAL claim's binding and the round lane's
/// anchors answer to — armed from DAA 1 through their own entries, the operator set cut to the first `operators` genesis cards: those
/// are the operators (the floor producers of the fleet), the rest are non-operator bonds (an external REAL producer). With `window`,
/// ADR-0170's `palw_anchor_window_v1` too (the DAA-5,300 release arms it with the rest). The release's other fences stay dormant: this
/// isolates what A″ does to anchors, which is the question the 2026-10-03 head-admission finding asks.
fn config_lane_a(operators: usize, window: bool) -> Parts {
    use kaspa_consensus_core::config::params::PALW_T12_POST_LAUNCH_FENCES_V1;
    use kaspa_consensus_core::palw_anchor_window_v1::PALW_T12_ANCHOR_WINDOW_FENCES_V1;
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
    let (config, bundle, premine, floats) = config_of(true);
    let mut params = config.params.clone();
    for fence in PALW_T12_POST_LAUNCH_FENCES_V1
        .iter()
        .filter(|f| matches!(f.name, "palw_panel_seed_execution" | "palw_operator_anchor" | "palw_anchor_at_ceiling"))
    {
        (fence.set)(&mut params, Some(ForkActivation::new(1)));
    }
    if window {
        for fence in PALW_T12_ANCHOR_WINDOW_FENCES_V1 {
            (fence.set)(&mut params, Some(ForkActivation::new(1)));
        }
    }
    let cards: Vec<_> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            Obj::BondRegistered { bond, .. } => Some(*bond),
            _ => None,
        })
        .collect();
    let rule = params.palw_operator_anchor.as_mut().expect("lane A is armed");
    let mut operators: Vec<_> = cards.iter().take(operators).copied().collect();
    operators.sort();
    rule.operators = operators;
    params.validate_palw_v2().expect("lane A over a subset of the genesis bonds validates on testnet-12");
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    let bundle = match &config.params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => unreachable!("testnet-12 is ConsensusV2"),
    };
    assert_eq!(bundle.state.anchor_window_active_at(1), window, "the fold's mirror of the window follows the fence");
    (config, bundle, premine, floats)
}

/// One honest heartbeat slot, as the H1 miner mines it: a beat a second after the last block, beats a second
/// apart until one steps the clock.
async fn honest_slot(c: &mut T12Chain) -> Vec<Block> {
    let start = c.daa_of(c.sink());
    let mut out = vec![c.heartbeat(1_000, Vec::new()).await];
    while c.daa_of(c.sink()) == start {
        assert!(out.len() < 4, "an honest slot steps within four beats");
        out.push(c.heartbeat(1_000, Vec::new()).await);
    }
    out
}

impl Rig {
    async fn new(fences: bool) -> Rig {
        Rig::over(config_of(fences)).await
    }

    /// The rig over testnet-12 with the Useful Work Transition and lane A, `operators` of the eight cards being operators, and ADR-0170's
    /// anchor window when `window`.
    async fn new_lane_a(operators: usize, window: bool) -> Rig {
        Rig::over(config_lane_a(operators, window)).await
    }

    async fn over((config, bundle, premine, floats): Parts) -> Rig {
        kaspa_core::log::try_init_logger("warn");
        let mut c = t12_genesis_chain(&config, &bundle, &premine, &floats);
        let k8 = {
            let (_, tip) = c.tip_state();
            tip.model_lifecycles_iter()
                .find(|(id, row)| {
                    **id != bundle.base_class_id && !kaspa_consensus_core::palw_work_target_v1::palw_panel_held_to_final_v1(row)
                })
                .map(|(id, _)| *id)
                .expect("testnet-12's short-window (8k) row")
        };
        // Ten slots of heartbeats: a reference for the clock, and room in the epoch's released budget.
        for _ in 0..10 {
            honest_slot(&mut c).await;
        }
        Rig { c, k8, nonce: 1 << 45, plants: Vec::new() }
    }

    fn floor(&self) -> Hash64 {
        self.c.bundle.base_class_id
    }

    /// **The 8k class planted `Active` on eight ready seats at the sink** — `t12_rcore_s6_producer_share`'s
    /// plant. Idempotent, and done right BEFORE a REAL attempt's template (never between a template and its
    /// landing: the attempt's own fold runs on the state it was built on).
    fn plant(&mut self) {
        self.plant_with(None);
    }

    /// [`Self::plant`], with the 8k class's in-flight cap (the registry's `max_inflight_claims`, 5 here) raised to `inflight_cap`
    /// where the test needs more REAL claims open than the harness's missing panel receipts would ever let resolve — the chain's own
    /// verification keeps its cap far from full, the harness's does not run.
    fn plant_with(&mut self, inflight_cap: Option<u32>) {
        let vp = self.c.vp();
        let (sink, tip) = vp.palw_state_v2_store.read().load_tip(&self.c.bundle.state).unwrap().expect("the tip loads");
        let now = self.c.ctx.consensus.get_virtual_daa_score();
        let mut carriage = PalwStateCarriageV2::from_state(&tip);
        {
            let row = carriage.model_lifecycles.get_mut(&self.k8).expect("the 8k row");
            row.state = PalwModelLifecycleV1::Active;
            if let Some(cap) = inflight_cap {
                row.profile.max_inflight_claims = cap;
            }
        }
        for bond in &self.c.bonds {
            carriage.seat_readiness.insert(
                (*bond, self.k8),
                PalwSeatReadinessRowV1 { proved_daa: now, proved_span: now, leaf_index: 0, proof_version: 2, chunks: 8 },
            );
        }
        let planted = carriage.into_state(&self.c.bundle.state, None).expect("the planted tip is a consistent state");
        vp.palw_state_v2_store.write().set_tip_for_tests(sink, &planted).expect("the planted tip becomes the tip");
        self.plants.push((sink, planted));
    }

    /// **An attempt block of `class` by card `card`, built and not inserted** — `T12Chain::build_attempt_drawn`'s
    /// construction with the class a parameter. `check_ready` runs the producer's own pre-check first (a REAL
    /// attempt and an honest floor pass it); `false` is a producer that ignores it — the rogue floor.
    fn build_of(&mut self, class: Hash64, card: usize, step_ms: u64, check_ready: bool) -> (MutableBlock, Hash64) {
        use kaspa_consensus_core::palw_attempt_v2::{
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
            PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
            palw_network_domain_v2_for,
        };
        self.c.ctx.simulated_time += step_ms;
        self.nonce += 1;
        let bond = self.c.bonds[card];
        let mut t = self
            .c
            .ctx
            .consensus
            .build_block_template(
                MinerData::new(card_payout_spk(card), vec![]),
                Box::new(OnetimeTxSelector::new(Vec::new())),
                TemplateBuildMode::Standard,
            )
            .expect("a template");
        assert!(kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(t.block.header.pow_algo_id), "a V2 template declares the attempt lane");
        stamp_harness_time(&self.c.config.params, &mut t.block.header, self.c.ctx.simulated_time);
        t.block.header.nonce = self.nonce;
        let facts = self.c.ctx.consensus.palw_producer_facts_v2(class, Some(bond.0)).expect("a V2 network answers");
        if check_ready {
            let rcore_plus = self.c.config.params.palw_rcore_plus_fence().is_some_and(|f| f.is_active(facts.daa_score));
            facts
                .ready_to_produce_v3(&card_pubkey(card), rcore_plus)
                .unwrap_or_else(|why| panic!("card {card} is not ready to produce for class {class}: {why} ({:?})", facts.class_admission_refusal));
        }
        let bond_facts = facts.bond.as_ref().expect("a genesis card is a registered bond").clone();
        let network_domain = palw_network_domain_v2_for(self.c.config.params.net.to_string().as_bytes(), Some(self.c.config.params.genesis.hash));
        let header = &t.block.header;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let mut attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
            class_id: facts.class_id,
            executor_bond: bond.0,
            executor_pubkey: card_pubkey(card),
            operator_id: bond_facts.operator_id,
            artifact_root: facts.artifact_root,
            trace_root: Hash64::default(),
            output_root: Hash64::from_u64_word(0x0070_0000_0000_0000 | self.nonce),
            execution_root: Hash64::from_u64_word(0xE7EC_0000_0000_0000 | self.nonce),
            pwu: facts.pwu,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        let anchor = execution_anchor_v3(network_domain, pre_pow, facts.class_id, &bond.0, header.nonce);
        let mut won = false;
        for draw in 0u64..4_000_000 {
            attempt.trace_root = Hash64::from_u64_word((self.nonce << 32) ^ draw ^ 0x7A00_0000_0000_0000);
            attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
            if class_ticket_v3(&attempt, anchor) <= facts.class_target {
                won = true;
                break;
            }
        }
        assert!(won, "the class lottery is winnable");
        let claim_id = attempt_id_v2(&attempt);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &card_key(card).signing_key,
            claim_id.as_byte_slice(),
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
            [0x5Au8; 32],
        )
        .expect("ML-DSA-87 sign over a 64-byte attempt id")
        .as_ref()
        .to_vec();
        t.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
        t.block.header.finalize();
        (t.block, claim_id)
    }

    /// A REAL attempt (the 8k class) by `card`, built and not inserted: the slow producer's template.
    fn real(&mut self, card: usize, step_ms: u64) -> (MutableBlock, Hash64) {
        let class = self.k8;
        self.build_of(class, card, step_ms, true)
    }

    /// Insert `block` as the next chain block and demand it became the sink.
    async fn chain_block(&mut self, block: MutableBlock, what: &str) -> Block {
        let block = block.to_immutable();
        let hash = block.header.hash;
        self.c
            .ctx
            .consensus
            .validate_and_insert_block(block.clone())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("{what} {hash} was refused: {e}"));
        assert_eq!(self.c.ctx.consensus.block_status(hash), BlockStatus::StatusUTXOValid, "{what} {hash} is UTXO-valid");
        assert_eq!(self.c.sink(), hash, "{what} {hash} is the sink");
        self.c.ctx.simulated_time = self.c.ctx.simulated_time.max(block.header.timestamp);
        block
    }

    /// A REAL attempt as a chain block, mined at once.
    async fn real_now(&mut self, card: usize, step_ms: u64) -> (Block, Hash64) {
        let (block, id) = self.real(card, step_ms);
        (self.chain_block(block, &format!("card {card}'s REAL attempt")).await, id)
    }

    /// A floor attempt that ignores the producer's pre-check (a rogue producer), as a chain block.
    async fn rogue_floor(&mut self, card: usize, step_ms: u64) -> (Block, Hash64) {
        let class = self.floor();
        let (block, id) = self.build_of(class, card, step_ms, false);
        (self.chain_block(block, &format!("card {card}'s rogue floor")).await, id)
    }

    /// An honest floor attempt (the producer's pre-check passes), as a chain block.
    async fn honest_floor(&mut self, card: usize, step_ms: u64) -> (Block, Hash64) {
        let class = self.floor();
        let (block, id) = self.build_of(class, card, step_ms, true);
        (self.chain_block(block, &format!("card {card}'s floor")).await, id)
    }

    /// `slots` honest heartbeat slots.
    async fn beat(&mut self, slots: u64) -> usize {
        let mut beats = 0;
        for _ in 0..slots {
            beats += honest_slot(&mut self.c).await.len();
        }
        beats
    }

    /// Heartbeat slots until the simulated clock reaches `until_ms`.
    async fn beat_until(&mut self, until_ms: u64) -> usize {
        let mut beats = 0;
        while self.c.ctx.simulated_time < until_ms {
            beats += honest_slot(&mut self.c).await.len();
        }
        beats
    }

    /// **The slow producer's attempt lands**: `d` is inserted as a side block (its parents are where its template
    /// was), then the next heartbeat merges it. Returns that heartbeat and how `d` stands in its GHOSTDAG data.
    async fn land(&mut self, d: &MutableBlock) -> (Block, Colour) {
        let d = d.clone().to_immutable();
        self.c.ctx.simulated_time = self.c.ctx.simulated_time.max(d.header.timestamp);
        self.c
            .ctx
            .consensus
            .validate_and_insert_block(d.clone())
            .virtual_state_task
            .await
            .expect("a slow attempt is a valid block when it lands");
        let merger = self.c.heartbeat(1_000, Vec::new()).await;
        let colour = self.colour(merger.header.hash, d.header.hash);
        (merger, colour)
    }

    fn colour(&self, merger: BlockHash, block: BlockHash) -> Colour {
        let gd = self.c.vp().ghostdag_store.get_data(merger).unwrap();
        if gd.selected_parent == block {
            Colour::SelectedParent
        } else if gd.mergeset_blues.contains(&block) {
            Colour::Blue
        } else if gd.mergeset_reds.contains(&block) {
            Colour::Red
        } else {
            Colour::NotMerged
        }
    }

    fn state(&self) -> kaspa_consensus_core::palw_state_v2::PalwChainStateV2 {
        self.c.tip_state().1
    }

    /// The rooted floor state at the sink.
    fn floor_state(&self) -> PalwFloorStateV1 {
        self.state().floor_state_v1()
    }

    /// The floor's producer pre-check for card `card`, as `palw_producer_facts_v2` answers it at the virtual.
    fn floor_precheck(&self, card: usize) -> (Option<String>, Result<(), &'static str>) {
        let facts = self
            .c
            .ctx
            .consensus
            .palw_producer_facts_v2(self.floor(), Some(self.c.bonds[card].0))
            .expect("a V2 network answers for its floor");
        let rcore_plus = self.c.config.params.palw_rcore_plus_fence().is_some_and(|f| f.is_active(facts.daa_score));
        let ready = facts.ready_to_produce_v3(&card_pubkey(card), rcore_plus);
        (facts.class_admission_refusal.clone(), ready)
    }

    fn assert_floors_held(&self, why: &str) {
        for card in 0..self.c.bonds.len() {
            let (refusal, ready) = self.floor_precheck(card);
            let refusal = refusal.unwrap_or_else(|| panic!("{why}: card {card}'s floor pre-check names no refusal"));
            assert!(refusal.contains("idle-only fallback"), "{why}: the refusal is the idle gate's: {refusal}");
            assert_eq!(
                ready,
                Err(kaspa_consensus_core::palw_producer_v2::PALW_NOT_READY_CLASS_NOT_ADMITTING_V2),
                "{why}: card {card} holds before the inference"
            );
        }
    }

    fn assert_floors_free(&self, why: &str) {
        for card in 0..self.c.bonds.len() {
            let (refusal, ready) = self.floor_precheck(card);
            assert_eq!(refusal, None, "{why}: card {card}'s floor pre-check is clear");
            assert_eq!(ready, Ok(()), "{why}: card {card} is ready to produce the floor");
        }
    }
}

/// `n` floors extend the chain while a REAL attempt (its template taken before them) is drawn, and it lands
/// `INFERENCE_MS` after its template. Returns the colour the merging heartbeat gave it.
async fn slow_real_under_floors(n: usize) -> Colour {
    let mut rig = Rig::new(false).await;
    rig.plant();
    let (d, d_id) = rig.real(1, 1_000);
    let templated_at = d.header.timestamp;
    for i in 0..n {
        rig.honest_floor(2 + i % 4, 1_000).await;
    }
    rig.c.ctx.simulated_time = rig.c.ctx.simulated_time.max(templated_at + INFERENCE_MS);
    let (merger, colour) = rig.land(&d).await;
    let gd = rig.c.vp().ghostdag_store.get_data(merger.header.hash).unwrap();
    eprintln!(
        "[real-share] pre-fence, {n} floor(s) under the slow REAL attempt: {colour:?} (merger's selected parent is a floor: {}, blues {}, reds {})",
        gd.selected_parent != d.header.hash,
        gd.mergeset_blues.len(),
        gd.mergeset_reds.len()
    );
    // The claim is still folded, red or not — a red's work is applied by the merging block.
    assert!(rig.state().claim(&d_id).is_some(), "the REAL attempt's claim is applied wherever it stands");
    colour
}

/// `slots` honest heartbeat slots pass while a REAL attempt (its template taken before them) is drawn.
async fn slow_real_under_heartbeats(slots: u64) -> (Colour, usize) {
    let mut rig = Rig::new(false).await;
    rig.plant();
    let (d, d_id) = rig.real(1, 1_000);
    let beats = rig.beat(slots).await;
    let (_merger, colour) = rig.land(&d).await;
    eprintln!("[real-share] pre-fence, {beats} heartbeat(s) under the slow REAL attempt: {colour:?}");
    assert!(rig.state().claim(&d_id).is_some(), "and its claim is applied");
    (colour, beats)
}

/// **Before the fence: the live finding reproduced.** A slow REAL attempt with two or three floors in its
/// anticone goes RED at `ghostdag_k = 1`; with the heartbeats of two and three slots instead (ADR-0105) it is
/// the selected parent — BLUE, whatever their number.
#[tokio::test]
async fn real_share_a_slow_real_attempt_is_red_under_floors_and_blue_under_heartbeats_before_the_fence() {
    for n in [2usize, 3] {
        assert_eq!(slow_real_under_floors(n).await, Colour::Red, "{n} floors in the anticone of a slow REAL attempt: RED (the live finding)");
    }
    for slots in [1u64, 2, 3] {
        let (colour, beats) = slow_real_under_heartbeats(slots).await;
        assert!(beats >= 2 * slots as usize, "{slots} slot(s): the heartbeats were minted ({beats})");
        assert!(
            matches!(colour, Colour::SelectedParent | Colour::Blue),
            "{beats} heartbeats in the anticone of a slow REAL attempt do not count against it: {colour:?}"
        );
    }
}

/// **Past the fence, on a busy chain: floors are refused, the slow REAL attempt is BLUE and accepted.** The first REAL
/// attempt (a chain block, BLUE by construction) makes the floor state Normal; the floor producers hold; only heartbeats
/// extend the chain under the slow attempt, which lands BLUE and refreshes Normal.
#[tokio::test]
async fn real_share_past_the_fence_a_busy_chain_refuses_floors_and_the_slow_real_attempt_is_blue_and_accepted() {
    let mut rig = Rig::new(true).await;
    // Idle at the fence (no REAL attempt yet): the floor is the bonded fallback and is admitted.
    rig.assert_floors_free("before any REAL attempt");
    assert_eq!(rig.floor_state(), PalwFloorStateV1::default(), "nothing is rooted before a REAL attempt");
    // R0: the first REAL attempt, a chain block. Accepted at once (its own fold) and BLUE: Normal, straight from Idle.
    rig.plant();
    let (r0, r0_id) = rig.real_now(1, 1_000).await;
    let state = rig.state();
    assert!(state.claim(&r0_id).is_some(), "R0 is accepted");
    let r0_daa = rig.c.daa_of(r0.header.hash);
    assert_eq!(rig.floor_state().mode, Normal { last_blue: r0_daa }, "a BLUE REAL attempt makes the chain Normal at once, with no probe");
    rig.assert_floors_held("right after a REAL attempt");

    // The slow producer takes its template now and infers for 340 s; honest floor producers hold, the
    // heartbeats go on.
    rig.plant();
    let (d, d_id) = rig.real(2, 1_000);
    let templated_at = d.header.timestamp;
    let beats = rig.beat_until(templated_at + INFERENCE_MS).await;
    assert!(beats >= 4, "two slots of heartbeats were minted under the slow attempt ({beats})");
    rig.assert_floors_held("while the slow REAL attempt is being drawn");
    let (merger, colour) = rig.land(&d).await;
    eprintln!("[real-share] past the fence, busy chain, {beats} heartbeats under the slow attempt: {colour:?}");
    assert!(
        matches!(colour, Colour::SelectedParent | Colour::Blue),
        "with floors refused only heartbeats stand in its anticone: BLUE (selected parent or blue), got {colour:?}"
    );
    let state = rig.state();
    let claim = state.claim(&d_id).expect("the slow REAL attempt is accepted");
    assert_eq!(claim.class_id, rig.k8);
    assert_eq!(claim.accepted_block, d.header.hash, "the claim's carrying block is the slow attempt");
    // Normal is refreshed to the DAA of the block that ACCEPTED it: its own fold when it is the chain's selected parent,
    // the merging block's when it was merged.
    let accepted_at = if colour == Colour::SelectedParent { rig.c.daa_of(d.header.hash) } else { rig.c.daa_of(merger.header.hash) };
    assert_eq!(rig.floor_state().mode, Normal { last_blue: accepted_at }, "a BLUE REAL attempt refreshes Normal");
    assert!(accepted_at >= r0_daa, "to a DAA no earlier than the first attempt's (equal when the slow attempt is the selected parent)");
    rig.assert_floors_held("after the slow attempt landed");
}

/// **The residual, stated: a producer that ignores the pre-check can still push a REAL attempt RED — and earns
/// nothing for it; and a RED REAL attempt never extends Normal.** The colouring is header-stage and reads neither
/// the class nor the state, so two floor-lane blocks in the slow attempt's anticone colour it RED whoever mined
/// them. What the fold-level rule changes is what those blocks are: valid blocks the fold skips — no claim, no
/// weight, no reward (the worker carve is withheld and burned) — and the RED attempt is merged and APPLIED by
/// the block that merges it (reds are applied, `palw_v2_merged_works`), but as a RED one it does not refresh
/// Normal: the machine moves on BLUE verified success only (ADR-0165 §00.2).
#[tokio::test]
async fn real_share_a_rogue_floor_still_colours_classically_but_earns_nothing_and_a_red_real_attempt_never_extends_normal() {
    let mut rig = Rig::new(true).await;
    rig.plant();
    let (r0, _) = rig.real_now(1, 1_000).await;
    let r0_daa = rig.c.daa_of(r0.header.hash);
    rig.assert_floors_held("busy");
    rig.plant();
    let (d, d_id) = rig.real(2, 1_000);
    let templated_at = d.header.timestamp;
    let before = rig.state();
    let (f1, f1_id) = rig.rogue_floor(3, 1_000).await;
    let (f2, f2_id) = rig.rogue_floor(4, 1_000).await;
    let after_floors = rig.state();
    for (f, id) in [(&f1, f1_id), (&f2, f2_id)] {
        assert!(after_floors.claim(&id).is_none(), "the rogue floor {} carries no claim: skipped by the fold", f.header.hash);
    }
    assert_eq!(
        (after_floors.safe_weight(), after_floors.bounded_immature()),
        (before.safe_weight(), before.bounded_immature()),
        "and no weight"
    );
    assert_eq!(after_floors.floor_state_v1(), before.floor_state_v1(), "and the floor state does not move for a floor");
    rig.c.ctx.simulated_time = rig.c.ctx.simulated_time.max(templated_at + INFERENCE_MS);
    let (merger, colour) = rig.land(&d).await;
    eprintln!("[real-share] past the fence, two rogue floors under the slow attempt: {colour:?}");
    assert_eq!(colour, Colour::Red, "the colouring is classic against a floor-lane block, however the fold treats it");
    let state = rig.state();
    let claim = state.claim(&d_id).expect("the RED attempt is applied by the merging block's fold");
    assert_eq!(claim.accepted_block, d.header.hash);
    assert!(state.bounded_immature() > after_floors.bounded_immature(), "and the RED attempt's weight is credited");
    // The honest limit, as the machine sees it: a RED attempt in Normal changes nothing — Normal still ends IDLE slots
    // after the BLUE one (R0), not after this one.
    assert_eq!(rig.floor_state().mode, Normal { last_blue: r0_daa }, "a RED REAL attempt never extends Normal");
    assert!(rig.c.daa_of(merger.header.hash) >= r0_daa, "the merging block is no earlier than R0");
    rig.assert_floors_held("still Normal: it is the BLUE attempt that holds the floor");
}

/// **The first REAL attempt after an idle stretch can go RED — it opens a Probe, and the NEXT attempt is BLUE.** Once
/// Normal has run out (`floor_idle_slots` after the last BLUE attempt) the bonded floor resumes; a REAL attempt whose
/// template was taken then meets the floors that extended the chain under it and goes RED (the honest limit of the idle
/// window), is accepted all the same — and, accepted RED in Idle, **opens a Probe**: floors are refused for `probe_slots`,
/// so the next slow attempt meets heartbeats only, lands BLUE and makes the state Normal. A Probe nobody answers
/// expires at `until` and the floor resumes.
#[tokio::test]
async fn real_share_a_red_first_attempt_opens_a_probe_the_next_is_blue_and_an_unanswered_probe_expires() {
    let mut rig = Rig::new(true).await;
    rig.plant();
    let (r0, _) = rig.real_now(1, 1_000).await;
    let d0 = rig.c.daa_of(r0.header.hash);
    // Normal ends once `daa - last_blue > IDLE`: the floor is still held at `d0 + IDLE` and free at `d0 + IDLE + 1`.
    loop {
        let now = rig.c.ctx.consensus.get_virtual_daa_score();
        if now > d0 + IDLE {
            break;
        }
        if now == d0 + IDLE {
            rig.assert_floors_held("exactly floor_idle_slots after the last BLUE REAL attempt");
        }
        rig.beat(1).await;
    }
    assert_eq!(rig.c.ctx.consensus.get_virtual_daa_score(), d0 + IDLE + 1, "the walk stops at the first free slot");
    rig.assert_floors_free("past floor_idle_slots: the weak stretch is over and the bonded floor resumes");

    // The floor producers are back; a REAL attempt is templated now and drawn for 340 s under them.
    rig.plant();
    let (d, d_id) = rig.real(2, 1_000);
    let templated_at = d.header.timestamp;
    let mut floors = Vec::new();
    for i in 0..3 {
        floors.push(rig.honest_floor(3 + i, 1_000).await.1);
    }
    let state = rig.state();
    assert!(floors.iter().all(|id| state.claim(id).is_some()), "the resumed floors are accepted: bonded fallback weight again");
    assert_eq!(state.floor_state_v1().mode, Idle, "and a floor attempt is no REAL event");
    rig.c.ctx.simulated_time = rig.c.ctx.simulated_time.max(templated_at + INFERENCE_MS);
    let (merger, colour) = rig.land(&d).await;
    eprintln!("[real-share] the first REAL attempt after more than floor_idle_slots idle slots, three floors under it: {colour:?}");
    assert_eq!(colour, Colour::Red, "the floors that resumed extended the chain under it");
    assert!(rig.state().claim(&d_id).is_some(), "accepted all the same");
    let probe_until = rig.c.daa_of(merger.header.hash) + PROBE;
    assert_eq!(rig.floor_state().mode, Probe { until: probe_until }, "accepted RED in Idle: it opens a Probe");
    rig.assert_floors_held("inside the probe");

    // The next slow attempt meets heartbeats only, inside the probe, and is BLUE: Normal.
    rig.plant();
    let (d2, d2_id) = rig.real(6, 1_000);
    let templated_at = d2.header.timestamp;
    let beats = rig.beat_until(templated_at + INFERENCE_MS).await;
    assert!(rig.c.ctx.consensus.get_virtual_daa_score() < probe_until, "the attempt lands inside the probe");
    rig.assert_floors_held("while the next attempt is being drawn");
    let (m2, colour2) = rig.land(&d2).await;
    eprintln!("[real-share] the next REAL attempt, {beats} heartbeats under it: {colour2:?}");
    assert!(matches!(colour2, Colour::SelectedParent | Colour::Blue), "the next attempt is BLUE: {colour2:?}");
    assert!(rig.state().claim(&d2_id).is_some());
    let accepted_at = if colour2 == Colour::SelectedParent { rig.c.daa_of(d2.header.hash) } else { rig.c.daa_of(m2.header.hash) };
    assert_eq!(rig.floor_state().mode, Normal { last_blue: accepted_at }, "BLUE inside the probe: Normal");
    rig.assert_floors_held("Normal after the probe was answered");
}

/// **A Probe nobody answers expires at `until` — and the floor is the fallback again.** A RED REAL attempt in Idle opens a
/// Probe; with no REAL attempt behind it the heartbeats run the clock past `until`, the state is Idle at the first block at or
/// past it (`last_probe_end` recorded), and the producers' pre-check is clear.
#[tokio::test]
async fn real_share_an_unanswered_probe_expires_and_the_floor_resumes() {
    let mut rig = Rig::new(true).await;
    rig.plant();
    // A REAL attempt, templated now, lands RED under two rogue floors in Idle (the fence is armed from genesis, no Normal).
    let (d, d_id) = rig.real(1, 1_000);
    let templated_at = d.header.timestamp;
    rig.rogue_floor(2, 1_000).await;
    rig.rogue_floor(3, 1_000).await;
    rig.c.ctx.simulated_time = rig.c.ctx.simulated_time.max(templated_at + INFERENCE_MS);
    let (merger, colour) = rig.land(&d).await;
    assert_eq!(colour, Colour::Red);
    assert!(rig.state().claim(&d_id).is_some());
    let until = rig.c.daa_of(merger.header.hash) + PROBE;
    assert_eq!(rig.floor_state().mode, Probe { until }, "a RED REAL attempt accepted in Idle opens a probe");
    rig.assert_floors_held("inside the probe");
    // Heartbeats run the clock to `until`: one slot short it is still a probe, and at `until` it is Idle again.
    while rig.c.ctx.consensus.get_virtual_daa_score() + 1 < until {
        rig.beat(1).await;
    }
    rig.assert_floors_held("one slot short of `until`");
    rig.beat(1).await;
    assert!(rig.c.ctx.consensus.get_virtual_daa_score() >= until);
    rig.assert_floors_free("at `until` the probe has run out");
    // The state is rooted as Idle once a block has folded the expiry; the next block does.
    rig.beat(1).await;
    assert_eq!(rig.floor_state(), PalwFloorStateV1 { mode: Idle, last_probe_end: Some(until) }, "Idle, remembering when the probe ended");
}

/// The DAA scores along an attempts-only chain (no heartbeat after the reference): a REAL attempt stamped into
/// the open slot, the REAL attempt after it, two more attempt-lane blocks in the same slot, then the next slot's
/// two REAL attempts.
async fn attempts_only(fences: bool) -> (u64, Vec<(&'static str, u64)>) {
    let mut rig = Rig::new(fences).await;
    rig.plant();
    let daa0 = rig.c.daa_of(rig.c.sink());
    let mut seen = Vec::new();
    // Slot 1: `ra` is stamped past the cursor (the reference is the last heartbeat step, 120 s of slot after it).
    let (ra, _) = rig.real_now(1, SLOT + 5_000).await;
    seen.push(("ra: the REAL attempt in the open slot", rig.c.daa_of(ra.header.hash)));
    let (m, _) = rig.real_now(2, 1_000).await;
    seen.push(("m: the block after it", rig.c.daa_of(m.header.hash)));
    let (x1, _) = rig.rogue_floor(3, 1_000).await;
    seen.push(("x1: another attempt-lane block, same slot", rig.c.daa_of(x1.header.hash)));
    let (x2, _) = rig.rogue_floor(4, 1_000).await;
    seen.push(("x2: and another", rig.c.daa_of(x2.header.hash)));
    // Slot 2.
    let (y, _) = rig.real_now(5, SLOT).await;
    seen.push(("y: a REAL attempt in the next slot", rig.c.daa_of(y.header.hash)));
    let (n, _) = rig.real_now(6, 1_000).await;
    seen.push(("n: the block after it", rig.c.daa_of(n.header.hash)));
    (daa0, seen)
}

/// **B: a REAL attempt carries its slot's tick, once, with no heartbeat in the chain.** The attempt stamped into
/// the open slot is the tick's source; the next block (any lane) is the step and carries it; attempt-lane blocks
/// after it in the same slot tick nothing (one tick a slot, however many attempts it holds); the next slot's
/// attempt and the block after it tick once more. The control — the fence unarmed — ticks not at all: on
/// testnet-12 as launched only a heartbeat moves the clock.
#[tokio::test]
async fn real_share_a_real_attempt_carries_its_slots_tick_and_a_slot_ticks_once() {
    let (daa0, armed) = attempts_only(true).await;
    for (what, daa) in &armed {
        eprintln!("[real-share] B, armed: {what}: DAA {daa} (reference {daa0})");
    }
    let at = |i: usize| armed[i].1;
    assert_eq!(at(0), daa0, "the attempt in the open slot ticks nothing itself");
    assert_eq!(at(1), daa0 + 1, "the block after it carries the tick the REAL attempt held — no heartbeat anywhere");
    assert_eq!(at(2), daa0 + 1, "a second attempt-lane block in the slot ticks nothing");
    assert_eq!(at(3), daa0 + 1, "nor a third: one tick a slot");
    assert_eq!(at(4), daa0 + 1, "the next slot's attempt ticks nothing itself");
    assert_eq!(at(5), daa0 + 2, "and the block after it carries that slot's tick");

    let (daa0, control) = attempts_only(false).await;
    for (what, daa) in &control {
        assert_eq!(*daa, daa0, "control, fence unarmed: {what} — an attempt-lane block moves no clock");
    }
}

// ---- the pruned join (T49-style): the floor state crosses the pruning boundary in the carriage -------------------------

/// Every block of `chain`'s history from genesis (exclusive) to `upto` (inclusive), in the order a peer could deliver them:
/// each selected-chain block preceded by the side blocks it merges (a REAL attempt that landed beside the chain arrives before
/// the block that merges it), oldest first.
fn blocks_through(chain: &T12Chain, upto: BlockHash) -> Vec<Block> {
    let vp = chain.vp();
    let genesis = chain.config.params.genesis.hash;
    let mut spine = Vec::new();
    let mut at = upto;
    while at != genesis {
        spine.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    spine.reverse();
    let mut out = Vec::new();
    for hash in spine {
        let gd = vp.ghostdag_store.get_data(hash).expect("ghostdag data");
        let mut side: Vec<BlockHash> =
            gd.mergeset_blues.iter().chain(gd.mergeset_reds.iter()).copied().filter(|b| *b != gd.selected_parent).collect();
        side.sort_by_key(|b| (vp.headers_store.get_blue_score(*b).expect("a header"), *b));
        for b in side.into_iter().chain(std::iter::once(hash)) {
            out.push(chain.ctx.consensus.get_block(b).expect("the node holds every block of its history"));
        }
    }
    out
}

/// `block` arrives at `chain` as a peer's block does.
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

/// **A second node on `rig`'s chain, through `upto`** — testnet-12 with the same harness cards and the same fences, fed the
/// rig's blocks in order with the rig's plants installed at the block each was made on (the plant is a test's hand, so a peer
/// cannot send it; every block after it is validated by this node against the same planted state).
async fn follower(rig: &Rig, upto: BlockHash) -> T12Chain {
    let (config, bundle, premine, floats) = config_of(true);
    let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    assert_eq!(chain.config.params.genesis.hash, rig.c.config.params.genesis.hash, "one genesis");
    for block in blocks_through(&rig.c, upto) {
        let hash = block.header.hash;
        arrive(&chain, block, "a block of the followed chain").await;
        for (at, planted) in rig.plants.iter().filter(|(at, _)| *at == hash) {
            chain.vp().palw_state_v2_store.write().set_tip_for_tests(*at, planted).expect("the same plant, at the same block");
        }
    }
    assert_eq!(chain.sink(), upto, "the follower walks the followed chain to the point");
    chain
}

fn root_of(chain: &T12Chain, block: BlockHash) -> Hash64 {
    chain.vp().palw_state_v2_store.read().state_root_of(block).expect("a delta row")
}

/// **The pruned join, for one pruning point**: the archival rig has reached `p` (its sink) with the floor state `at_p`. A
/// second node follows it through `p`, sees the header of `p`'s selected-chain child `T` (the witness the import checks the
/// carriage's root against), is then left as a pruned join leaves a node — no PALW tip and no delta row at or below `p` — and
/// installs the carriage the archival node serves, as the borsh bytes `PruningPointPalwState` carries. From then on everything
/// the importer knows about the floor state came through the carriage.
///
/// The archival node then carries on: a policy-ignoring floor attempt (the fold refuses it while the state says so), heartbeat
/// slots until `expires_at` (the first DAA at which the machine has run out), and an honest floor attempt (accepted). The
/// importer is fed the same blocks and must agree on every one: the sink, the PALW root, the floor state, and the decision on
/// each floor attempt (a claim or none). Returns the floor states along the way.
async fn pruned_join(mut rig: Rig, at_p: PalwFloorStateV1, expires_at: u64, what: &str) -> Vec<(u64, PalwFloorStateV1)> {
    let p = rig.c.sink();
    assert_eq!(rig.floor_state(), at_p, "{what}: the archival node holds the state at P");
    let planted = rig.state();
    let p_root = planted.state_root();

    // The importing node: the archival node's blocks through P, validated by its own pipeline.
    let importer = follower(&rig, p).await;
    let below: Vec<BlockHash> = blocks_through(&rig.c, p).iter().map(|b| b.header.hash).collect();

    // T, P's selected-chain child: mined by the archival node, its header seen first by the importer.
    let t = rig.c.heartbeat(1_000, Vec::new()).await;
    arrive(&importer, Block::from_header_arc(t.header.clone()), "T's header").await;

    // The archival node serves P, as `RequestPruningPointPalwState` answers.
    let vp = rig.c.vp();
    vp.capture_pruning_point_palw_state(p);
    let wire = borsh::to_vec(&vp.pruning_point_palw_state(p).expect("the captured point is servable")).expect("serializes");
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&wire).expect("the wire bytes decode");
    assert_eq!(carriage.floor_state, at_p.canonical(), "{what}: the carriage names the floor state ({} bytes)", wire.len());

    // The importer as a pruned join leaves it — then the import.
    {
        let vp = importer.vp();
        let mut store = vp.palw_state_v2_store.write();
        store.delete_tip_for_tests().expect("no PALW tip");
        for block in std::iter::once(importer.config.params.genesis.hash).chain(below.iter().copied()) {
            store.delete_delta_for_tests(block).expect("no delta row at or below the pruning point");
        }
    }
    importer.vp().import_pruning_point_palw_state(p, carriage).expect("the served carriage installs against T's committed root");
    let (at, imported) = importer.tip_state();
    assert_eq!((at, imported.state_root()), (p, p_root), "{what}: the imported state is P's, root for root");
    assert_eq!(imported.floor_state_v1(), at_p, "{what}: and its floor state is the archival node's");

    // The archival node carries on from T: a policy-ignoring floor inside the state, heartbeats to the expiry, an honest floor.
    let mut script: Vec<(BlockHash, &'static str, Option<Hash64>)> = vec![(t.header.hash, "T", None)];
    let (f1, f1_id) = rig.rogue_floor(3, 1_000).await;
    script.push((f1.header.hash, "a policy-ignoring floor, refused by the fold", Some(f1_id)));
    while rig.c.ctx.consensus.get_virtual_daa_score() < expires_at {
        for b in honest_slot(&mut rig.c).await {
            script.push((b.header.hash, "a heartbeat", None));
        }
    }
    rig.assert_floors_free(&format!("{what}: past the expiry"));
    let (f2, f2_id) = rig.honest_floor(4, 1_000).await;
    script.push((f2.header.hash, "an honest floor, accepted", Some(f2_id)));
    let archival = rig.state();
    assert!(archival.claim(&f1_id).is_none(), "{what}: the archival node refused the floor inside the state");
    assert!(archival.claim(&f2_id).is_some(), "{what}: and took the honest floor once the machine had run out");

    // The importer is fed the same blocks, one by one.
    let mut states = Vec::new();
    let mut seen_states = 0usize;
    for (i, (hash, kind, floor_id)) in script.iter().enumerate() {
        let block = rig.c.ctx.consensus.get_block(*hash).expect("the archival node holds its chain");
        arrive(&importer, block, "the archival block").await;
        assert_eq!(importer.sink(), *hash, "{what}: block #{i} ({kind}) is the importer's sink");
        assert_eq!(root_of(&importer, *hash), root_of(&rig.c, *hash), "{what}: block #{i} ({kind}): the same PALW root");
        let theirs = importer.tip_state().1;
        let mine = {
            let vp = rig.c.vp();
            // The state the archival node folded at this block: the importer's must equal it.
            vp.palw_state_v2_store.read().state_root_of(*hash).expect("a delta row")
        };
        assert_eq!(theirs.state_root(), mine, "{what}: block #{i} ({kind}): the importer's state is the archival node's");
        if let Some(id) = floor_id {
            let decision_here = archival.claim(id).is_some();
            assert_eq!(theirs.claim(id).is_some(), decision_here, "{what}: block #{i} ({kind}): the same decision on the floor attempt");
        }
        let daa = rig.c.daa_of(*hash);
        let floor = theirs.floor_state_v1();
        if states.last().map(|(_, f): &(u64, PalwFloorStateV1)| *f) != Some(floor) {
            states.push((daa, floor));
            seen_states += 1;
        }
    }
    assert_eq!(importer.sink(), rig.c.sink(), "{what}: the importer ends on the archival node's sink");
    assert_eq!(importer.tip_state().1.floor_state_v1(), rig.floor_state(), "{what}: with the archival node's floor state");
    assert!(seen_states >= 2, "{what}: the floor state moved on the importer ({states:?})");
    eprintln!(
        "[real-share-pruned] {what}: carriage at P {} bytes; {} blocks replayed identically; floor states {:?}",
        wire.len(),
        script.len(),
        states.iter().map(|(daa, f)| format!("DAA {daa}: {f}")).collect::<Vec<_>>()
    );
    states
}

/// **A node that imports its pruning point inside a NORMAL stretch decides the floor as an archival node does** — the state
/// (`Normal { last_blue }`) crosses in the carriage; the importer refuses the policy-ignoring floor inside it, sees Normal run
/// out `floor_idle_slots` after the last BLUE attempt, and takes the honest floor after it, block for block with the archival
/// node.
#[tokio::test]
async fn real_share_a_pruned_join_inside_a_normal_stretch_decides_the_floor_as_an_archival_node_does() {
    let mut rig = Rig::new(true).await;
    rig.plant();
    let (r0, _) = rig.real_now(1, 1_000).await;
    let last_blue = rig.c.daa_of(r0.header.hash);
    rig.beat(3).await;
    let at_p = PalwFloorStateV1 { mode: Normal { last_blue }, last_probe_end: None };
    let states = pruned_join(rig, at_p, last_blue + IDLE + 1, "Normal at P").await;
    assert_eq!(states.first().map(|(_, f)| f.mode), Some(Normal { last_blue }), "{states:?}");
    assert_eq!(states.last().map(|(_, f)| *f), Some(PalwFloorStateV1::default()), "Normal ran out on the importer, rooted as nothing: {states:?}");
}

/// **A node that imports its pruning point inside a PROBE decides the floor as an archival node does** — the probe's `until`
/// crosses in the carriage; the importer refuses the floor until it, records when the probe ended unanswered, and takes the
/// honest floor after it.
#[tokio::test]
async fn real_share_a_pruned_join_inside_a_probe_decides_the_floor_as_an_archival_node_does() {
    let mut rig = Rig::new(true).await;
    rig.plant();
    let (d, d_id) = rig.real(1, 1_000);
    let templated_at = d.header.timestamp;
    rig.honest_floor(2, 1_000).await;
    rig.honest_floor(3, 1_000).await;
    rig.c.ctx.simulated_time = rig.c.ctx.simulated_time.max(templated_at + INFERENCE_MS);
    let (merger, colour) = rig.land(&d).await;
    assert_eq!(colour, Colour::Red, "the first REAL attempt under two floors is RED");
    assert!(rig.state().claim(&d_id).is_some());
    let until = rig.c.daa_of(merger.header.hash) + PROBE;
    let at_p = PalwFloorStateV1 { mode: Probe { until }, last_probe_end: None };
    let states = pruned_join(rig, at_p, until, "Probe at P").await;
    assert_eq!(states.first().map(|(_, f)| f.mode), Some(Probe { until }), "{states:?}");
    assert_eq!(
        states.last().map(|(_, f)| *f),
        Some(PalwFloorStateV1 { mode: Idle, last_probe_end: Some(until) }),
        "the probe ended unanswered on the importer, remembered for the cooldown: {states:?}"
    );
}

// ---- the head-admission finding (P2, 2026-10-03): what anchors survive Normal with the floor held --------------------------
//
// Every anchor the chain had was made by a block that carries an ADMITTED attempt, and the floor made most of them (three a slot).
// Once REAL work holds the floor refused, the producers that mined those attempts hold, and two consumers lose their source — and the
// two fixes that ship with the Useful Work Transition are measured here, in the pipeline, against the gap each closes:
//
// * **lane A's panel binding** — past lane A a claim binds only in an anchor block, which must be or merge an OPERATOR's attempt at or
//   past its slot (`palw_chain_block_as_anchor_v1`). With the operators' floor producers holding and the REAL work coming from
//   non-operators, no such attempt exists. ANCHOR DUTY (ADR-0165 §00.9, a producer policy: `palw_floor_anchor_duty_v1`) closes it: an
//   operator's floor producer that holds only for the idle-only policy mines ONE binder floor when a claim has waited 30 slots (plus a
//   stagger) — an attempt the fold refuses, and an operator attempt by its header. The tests run consensus-core's rule on the real
//   chain's own producer facts, pass by pass, exactly as the kaspad worker does;
// * **the round lane's seed anchor** (ADR-0130), read by the next span's schedule and by the ADR-0147 admission jury — recorded by a
//   chain block whose OWN attempt is admitted, once per span, and cleared at the span boundary. ADR-0170's `palw_anchor_window_v1`
//   (lane P2, the same release) closes it: a MERGED admitted attempt records the anchor (M1), it survives span boundaries (M2) and the
//   readers take the latest of the last 24 spans (M3). `…_the_audit_…` below measures the anchor's coverage; the Candidate class's
//   admission itself is `t12_anchor_window`'s.

/// What the round lane's seed anchor looks like from outside, sample by sample: the spans the walk crossed, the spans in which the
/// anchor was RECORDED (the rule as it was: an anchor of the sink's own span), and the spans after which an audit at the NEXT span
/// finds a seed in its window ([`palw_anchor_window_admits_v1`]). The ADR-0147 jury sits at an audit span only then.
#[derive(Default, Debug)]
struct AnchorMeter {
    spans: std::collections::BTreeSet<u64>,
    anchored: std::collections::BTreeSet<u64>,
    covered: std::collections::BTreeSet<u64>,
}

impl AnchorMeter {
    /// Sample the sink: a span's anchor stands from the chain block that recorded it until the FIRST block of the next span (the
    /// rotation resets it without the window, and the slot's tick-carrying beat is that block), so it is sampled after every block,
    /// in the sink's own span — never only at the end of a slot, which is already the next span.
    fn sample(&mut self, rig: &Rig) {
        let daa = rig.c.daa_of(rig.c.sink());
        let lane = rig.c.config.params.palw_execution_lane.expect("testnet-12 opens the execution lane");
        let span = kaspa_consensus_core::palw_execution_lane_v1::palw_execution_span_v1(daa, lane.schedule_span_daa_at(daa));
        self.spans.insert(span);
        if let Some(anchor) = rig.state().round_seed_anchor() {
            if anchor.span == span {
                self.anchored.insert(span);
            }
            if kaspa_consensus_core::palw_anchor_window_v1::palw_anchor_window_admits_v1(
                anchor.span,
                span + 1,
                kaspa_consensus_core::palw_anchor_window_v1::PALW_ANCHOR_WINDOW_SPANS_V1,
            ) {
                self.covered.insert(span);
            }
        }
    }

    fn share(&self) -> (usize, usize) {
        (self.anchored.len(), self.spans.len())
    }

    /// The spans from `from` on, and how many of them an audit at the next span would find a seed after.
    fn covered_from(&self, from: u64) -> (usize, usize) {
        (self.covered.range(from..).count(), self.spans.range(from..).count())
    }
}

/// [`honest_slot`], sampling the anchor meter after every beat (see [`AnchorMeter::sample`]).
async fn honest_slot_sampled(rig: &mut Rig, meter: &mut AnchorMeter) {
    let start = rig.c.daa_of(rig.c.sink());
    for beats in 1.. {
        rig.c.heartbeat(1_000, Vec::new()).await;
        meter.sample(rig);
        if rig.c.daa_of(rig.c.sink()) != start {
            break;
        }
        assert!(beats < 4, "an honest slot steps within four beats");
    }
}

/// **The operators' floor producers, one pass each** — what the kaspad worker's loop does with consensus-core's anchor-duty rule: the
/// real producer facts at the virtual DAA (`palw_producer_facts_v2` for the operator's bond), the readiness verdict, the wait tracker
/// (`PalwFloorAnchorDutyWaitV1`) and `palw_floor_anchor_duty_v1` with the bond's stagger. A pass that fires mines ONE floor block
/// ignoring the hold (`rogue_floor`: the producer's draw for its duty) and restarts that operator's wait.
struct Operators {
    waits: Vec<PalwFloorAnchorDutyWaitV1>,
    /// `(card, virtual DAA)` of every binder an operator mined.
    fired: Vec<(usize, u64)>,
    /// The wait, in slots (the release's unless a test shortens it).
    after: u64,
}

impl Operators {
    fn new(operators: usize) -> Operators {
        Operators { waits: vec![PalwFloorAnchorDutyWaitV1::default(); operators], fired: Vec::new(), after: DUTY_AFTER }
    }

    /// One pass of every operator's producer loop at the current virtual DAA.
    async fn pass(&mut self, rig: &mut Rig) {
        for card in 0..self.waits.len() {
            let bond = rig.c.bonds[card];
            let facts = rig
                .c
                .ctx
                .consensus
                .palw_producer_facts_v2(rig.floor(), Some(bond.0))
                .expect("a V2 network answers for its floor");
            let rcore_plus = rig.c.config.params.palw_rcore_plus_fence().is_some_and(|f| f.is_active(facts.daa_score));
            let ready = facts.ready_to_produce_v3(&card_pubkey(card), rcore_plus);
            let due_slots = self.waits[card].observe(facts.binder_due, facts.daa_score);
            let fire = palw_floor_anchor_duty_v1(&PalwFloorAnchorDutyInputV1 {
                binder_due: facts.binder_due,
                is_base_class: facts.is_base_class,
                hold_is_class_not_admitting: ready == Err(kaspa_consensus_core::palw_producer_v2::PALW_NOT_READY_CLASS_NOT_ADMITTING_V2),
                class_refusal: facts.class_admission_refusal.as_deref(),
                due_slots,
                after_slots: self.after,
                stagger: palw_floor_anchor_duty_stagger_v1(bond.0.transaction_id.as_byte_slice()[0]),
            });
            if fire {
                self.fired.push((card, facts.daa_score));
                self.waits[card].fired(facts.daa_score);
                rig.rogue_floor(card, 1_000).await;
            }
        }
    }
}

/// An honest slot, sampled, then the operators' pass.
async fn slot_with_operators(rig: &mut Rig, meter: &mut AnchorMeter, ops: &mut Operators) {
    honest_slot_sampled(rig, meter).await;
    ops.pass(rig).await;
}

/// **A non-operator's REAL claim under Normal, the floor held.** Card 5 (not an operator) lands a REAL attempt as a chain block — BLUE,
/// so the chain is Normal and the operators' floor producers hold. Its claim's anchor slot is `anchor_delay` DAA later. Then one
/// REAL attempt per `producers` entry is templated fourteen slots after the last one, drawn for 340 s under heartbeats only and merged
/// beside the chain, the way an 8k producer's are (each lands inside `floor_idle_slots` of the one before, so the chain stays Normal),
/// and the run goes on until the claim is no longer `Provisional` or `slots_past` slots past its anchor slot are reached. The
/// operators' producers run their passes throughout (`ops`). Returns the rig, the tracked claim, its slot and what the seed-anchor meter saw. (Few attempts: the 8k
/// class's in-flight cap is 5 and its panel budget about five open claims, and the harness runs no panel to resolve them.)
async fn non_operator_claim_under_normal(
    producers: &[usize],
    window: bool,
    ops: &mut Operators,
    slots_past: u64,
) -> (Rig, Hash64, u64, AnchorMeter) {
    let mut rig = Rig::new_lane_a(4, window).await;
    let delay = rig.c.bundle.panel.anchor_delay();
    rig.plant_with(Some(10_000));
    let (r0, claim_id) = rig.real_now(5, 1_000).await;
    let x0 = rig.c.daa_of(r0.header.hash);
    let claim = rig.state().claim(&claim_id).expect("the non-operator's REAL attempt is accepted").clone();
    assert!(matches!(claim.phase, kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::Provisional));
    let slot = claim.bind_base_daa() + delay;
    let until_daa = slot + slots_past;
    assert!(matches!(rig.floor_state().mode, Normal { .. }), "a BLUE REAL attempt: the chain is Normal and the floor is held");
    let mut meter = AnchorMeter::default();
    meter.sample(&rig);
    let bound = |rig: &Rig| !matches!(rig.state().claim(&claim_id).map(|c| c.phase.clone()), Some(kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::Provisional));
    for (k, card) in producers.iter().enumerate() {
        while rig.c.ctx.consensus.get_virtual_daa_score() < x0 + 14 * (k as u64 + 1) {
            slot_with_operators(&mut rig, &mut meter, ops).await;
            if bound(&rig) || rig.c.ctx.consensus.get_virtual_daa_score() >= until_daa {
                return (rig, claim_id, slot, meter);
            }
        }
        rig.plant_with(Some(10_000));
        let (d, _id) = rig.real(*card, 1_000);
        let templated_at = d.header.timestamp;
        while rig.c.ctx.simulated_time < templated_at + INFERENCE_MS {
            slot_with_operators(&mut rig, &mut meter, ops).await;
        }
        let (_merger, colour) = rig.land(&d).await;
        assert!(matches!(colour, Colour::SelectedParent | Colour::Blue), "with the floor held only heartbeats stand in its anticone: {colour:?}");
        meter.sample(&rig);
        assert!(matches!(rig.floor_state().mode, Normal { .. }), "REAL work keeps the chain Normal");
    }
    while !bound(&rig) && rig.c.ctx.consensus.get_virtual_daa_score() < until_daa {
        slot_with_operators(&mut rig, &mut meter, ops).await;
    }
    (rig, claim_id, slot, meter)
}

/// **FIXED: with Normal holding the floor and every REAL producer a non-operator, a REAL claim binds — through ANCHOR DUTY.** Four of
/// the eight genesis cards are the operators; card 5's REAL attempt makes the chain Normal; REAL attempts by cards 6 and 7 keep it
/// Normal; no operator makes an attempt of its own, because their floor producers HOLD. The claim waits — its slot passes and nothing
/// anchors it — until an operator's producer, having seen it due for 30 slots plus its bond's stagger, mines ONE binder floor: the fold
/// refuses it (`FloorNotIdle`: no claim, no weight) and it is still an operator attempt by its header, so it binds the claim in its own
/// acceptance. Asserted: nobody fires before the wait is over; the first binder falls inside `[slot + 30, slot + 30 + 7 + slack]`; it is
/// exactly one binder (the others see the claim bound and their waits break); the claim is bound by it; the chain is still Normal.
#[tokio::test]
async fn real_share_under_normal_a_non_operator_real_claim_binds_through_anchor_duty() {
    use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2 as Phase;
    let mut ops = Operators::new(4);
    let (rig, claim_id, slot, meter) = non_operator_claim_under_normal(&[6, 7, 4], true, &mut ops, 60).await;
    let phase = rig.state().claim(&claim_id).expect("the claim stays").phase.clone();
    let daa = rig.c.ctx.consensus.get_virtual_daa_score();
    eprintln!(
        "[real-share] anchor duty: non-operator REAL claim, Normal, floors held, slot DAA {slot}: now DAA {daa}: {phase:?}; binders mined {:?}; \
         seed-anchor coverage {:?}",
        ops.fired,
        meter.covered_from(0)
    );
    assert_eq!(ops.fired.len(), 1, "exactly one binder is mined: {:?}", ops.fired);
    let (card, at) = ops.fired[0];
    assert!(card < 4, "an operator's");
    assert!(at >= slot + DUTY_AFTER, "nobody fires before the claim has waited {DUTY_AFTER} slots: fired at DAA {at}, slot {slot}");
    assert!(
        at <= slot + DUTY_AFTER + DUTY_STAGGER + 3,
        "within the wait plus the stagger (and the slots a pass lags the DAA by): fired at DAA {at}, slot {slot}"
    );
    assert!(!matches!(phase, Phase::Provisional), "the binder anchors the claim: {phase:?}");
    assert!(matches!(rig.floor_state().mode, Normal { .. }), "REAL work still holds the floor: the binder was the only floor");
}

/// **The control: the same claim binds with no duty when an OPERATOR is the REAL producer** — an operator's REAL attempt, merged beside
/// the chain, is an operator attempt like its floors were; the claim is bound inside the wait, so no producer fires.
#[tokio::test]
async fn real_share_control_an_operators_real_attempts_anchor_a_non_operators_claim_with_no_duty() {
    use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2 as Phase;
    let mut ops = Operators::new(4);
    let (rig, claim_id, slot, meter) = non_operator_claim_under_normal(&[0, 1], true, &mut ops, 60).await;
    let phase = rig.state().claim(&claim_id).expect("the claim stays").phase.clone();
    eprintln!(
        "[real-share] control: operator REAL producers, Normal, floors held: slot DAA {slot}, now DAA {}: {phase:?}; binders mined {:?}",
        rig.c.ctx.consensus.get_virtual_daa_score(),
        ops.fired
    );
    assert!(!matches!(phase, Phase::Provisional), "an operator's REAL attempt anchors the claim: {phase:?}");
    assert!(ops.fired.is_empty(), "…inside the wait, so no operator's duty fired: {:?}", ops.fired);
    let _ = meter;
}

/// **Anchor duty is rare: a claim that binds is bound for good, and nothing else fires.** The same non-operator claim as the duty test,
/// run on past its binding for thirty more slots: no second binder is mined (the claims left Provisional, `binder_due` is false, every
/// operator's wait is broken).
#[tokio::test]
async fn real_share_anchor_duty_fires_once_and_then_the_floor_stays_held() {
    let mut ops = Operators::new(4);
    let (mut rig, claim_id, _slot, mut meter) = non_operator_claim_under_normal(&[6, 7, 4], true, &mut ops, 60).await;
    assert_eq!(ops.fired.len(), 1);
    let bound_at = rig.c.ctx.consensus.get_virtual_daa_score();
    // REAL work keeps flowing (an operator's attempt now), the operators keep passing: 30 more slots, one more REAL attempt to hold Normal.
    rig.plant_with(Some(10_000));
    let (d, _) = rig.real(0, 1_000);
    let templated_at = d.header.timestamp;
    while rig.c.ctx.simulated_time < templated_at + INFERENCE_MS {
        slot_with_operators(&mut rig, &mut meter, &mut ops).await;
    }
    rig.land(&d).await;
    while rig.c.ctx.consensus.get_virtual_daa_score() < bound_at + 30 {
        slot_with_operators(&mut rig, &mut meter, &mut ops).await;
    }
    assert_eq!(ops.fired.len(), 1, "no second binder: {:?}", ops.fired);
    assert!(
        !matches!(rig.state().claim(&claim_id).map(|c| c.phase.clone()), Some(kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::Provisional)),
        "the claim stays bound"
    );
}

/// **The round seed anchor, under Normal with the floor held — with the window (M1 + M2 + M3) and without it.** Normal: a REAL attempt
/// as a chain block, then four REAL attempts by four producers, one in flight at a time, each drawn for 340 s under heartbeats and
/// merged beside the chain (the 8k class's panel budget admits five open claims in a harness that resolves none), the floor
/// producers holding. With `palw_anchor_window_v1` armed: **each merged admitted attempt records the anchor** (M1: the block is the
/// attempt's own, the span the merging block's), **it survives span boundaries** (M2: it is still the same anchor at the next slot),
/// and **every span from the first merged attempt on is followed by an audit that finds a seed in its 24-span window** (M3: the
/// anchors are at most fourteen slots apart). Without the window — the control — the anchor is the rule as it was, recorded by a chain
/// block's own attempt and cleared at the span boundary: one span of sixteen has one, against fifteen of sixteen with the floor
/// producers mining every slot (Idle), the reason the fence exists.
#[tokio::test]
async fn real_share_the_round_seed_anchor_is_recorded_from_merged_attempts_and_survives_to_the_audit_window() {
    use kaspa_consensus_core::palw_anchor_window_v1::PALW_ANCHOR_WINDOW_SPANS_V1 as W;
    let mut rig = Rig::new_lane_a(4, true).await;
    rig.plant_with(Some(10_000));
    rig.real_now(5, 1_000).await;
    let mut meter = AnchorMeter::default();
    meter.sample(&rig);
    let mut first_merged_span = None;
    let mut last_anchor_block = None;
    for turn in 1..=4usize {
        rig.plant_with(Some(10_000));
        let (d, _id) = rig.real(turn % 8, 1_000);
        let templated_at = d.header.timestamp;
        while rig.c.ctx.simulated_time < templated_at + INFERENCE_MS {
            honest_slot_sampled(&mut rig, &mut meter).await;
        }
        let (merger, colour) = rig.land(&d).await;
        assert!(matches!(colour, Colour::Blue | Colour::SelectedParent), "{colour:?}");
        meter.sample(&rig);
        let anchor = *rig.state().round_seed_anchor().expect("M1: a merged admitted attempt recorded the anchor");
        if colour == Colour::Blue {
            assert_eq!(anchor.block, d.header.hash, "M1: the anchor is the merged attempt's own block (turn {turn})");
        }
        let merger_span = {
            let lane = rig.c.config.params.palw_execution_lane.expect("the execution lane");
            let daa = rig.c.daa_of(merger.header.hash);
            kaspa_consensus_core::palw_execution_lane_v1::palw_execution_span_v1(daa, lane.schedule_span_daa_at(daa))
        };
        assert_eq!(anchor.span, merger_span, "M1: recorded in the span of the block that folded the merge (turn {turn})");
        first_merged_span.get_or_insert(merger_span);
        // M2: the next slot's heartbeats cross a span boundary and the anchor is still this one.
        honest_slot_sampled(&mut rig, &mut meter).await;
        assert_eq!(
            rig.state().round_seed_anchor().map(|a| (a.block, a.span)),
            Some((anchor.block, anchor.span)),
            "M2: the anchor survives the span boundary (turn {turn})"
        );
        last_anchor_block = Some(anchor.block);
    }
    let first = first_merged_span.expect("a merged attempt landed");
    let (covered, spans) = meter.covered_from(first);
    eprintln!(
        "[real-share] seed anchors with the window: from span {first}, an audit at the next span finds a seed in {covered} of {spans} spans \
         (W = {W}); last anchor {last_anchor_block:?}"
    );
    assert_eq!(covered, spans, "M3: every span from the first merged attempt on is followed by an audit that finds a seed in its window");
}

/// **The control, without the window: the gap as it stood** (the reason the fence exists). Same REAL flow, `palw_anchor_window_v1`
/// dormant: the anchor is recorded only by a chain block's own attempt and cleared at the span boundary, so most spans end with none
/// — against fifteen of sixteen with the floor producers mining every slot.
#[tokio::test]
async fn real_share_control_without_the_window_most_spans_have_no_anchor_against_honest_floors() {
    // Normal: REAL attempts only, merged beside the chain; the floor producers hold.
    let mut rig = Rig::new_lane_a(4, false).await;
    rig.plant_with(Some(10_000));
    rig.real_now(5, 1_000).await;
    let mut real = AnchorMeter::default();
    real.sample(&rig);
    let mut landed = (0u32, 0u32); // (the selected parent of the block that merged it, merged beside the chain)
    for turn in 1..=4usize {
        rig.plant_with(Some(10_000));
        let (d, _id) = rig.real(turn % 8, 1_000);
        let templated_at = d.header.timestamp;
        while rig.c.ctx.simulated_time < templated_at + INFERENCE_MS {
            honest_slot_sampled(&mut rig, &mut real).await;
        }
        match rig.land(&d).await.1 {
            Colour::SelectedParent => landed.0 += 1,
            _ => landed.1 += 1,
        }
        real.sample(&rig);
        assert!(matches!(rig.floor_state().mode, Normal { .. }), "REAL work keeps the chain Normal");
    }
    // Idle, honest floors, one a slot, chain blocks — the fallback's own anchors — over the same number of spans.
    let mut rig = Rig::new_lane_a(4, false).await;
    let mut floors = AnchorMeter::default();
    let mut slot = 0usize;
    while floors.spans.len() < real.spans.len() {
        rig.honest_floor(slot % 8, 1_000).await;
        floors.sample(&rig);
        honest_slot_sampled(&mut rig, &mut floors).await;
        slot += 1;
    }
    assert_eq!(rig.floor_state().mode, Idle, "honest floors are no REAL event");
    let (fa, fs) = floors.share();
    let (ra, rs) = real.share();
    eprintln!(
        "[real-share] seed anchors WITHOUT the window, over {fs} spans: honest floors {fa} of {fs} anchored; Normal with REAL attempts only {ra} of {rs} \
         anchored ({} of the 4 REAL attempts were the selected parent of the block that merged them, {} merged beside the chain)",
        landed.0, landed.1
    );
    assert!(fa * 10 >= fs * 9, "with floors mined every slot almost every span is anchored: {fa} of {fs}");
    assert!(ra * 2 < rs, "without the window, with the floor held and REAL work merged beside the chain, most spans end with no anchor: {ra} of {rs}");
}
