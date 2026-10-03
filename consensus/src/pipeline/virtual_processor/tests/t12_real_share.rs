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
//! signed by a card, and folded by the real pipeline; the DAG, the colouring, the fold and the idle ledger are
//! the node's own.
//!
//! * `…red_under_floors_blue_under_heartbeats…` — **before the fence**: a slow REAL attempt with two or three
//!   floors in its anticone goes RED (the live finding, reproduced), and with any number of heartbeats it is
//!   the selected parent (BLUE).
//! * `…a_busy_chain_refuses_floors…` — **past the fence**: once a REAL attempt is accepted, the floor is
//!   refused by name (`FloorNotIdle`) at the producer's pre-check and by the fold; only heartbeats extend the
//!   chain under the slow attempt, and it is BLUE and accepted, renewing the idle ledger.
//! * `…a_rogue_floor…` — the residual, stated: a producer that ignores the pre-check still mines floor-lane
//!   blocks, which colour classically (the colouring is header-stage and cannot read the class or the ledger),
//!   so the slow attempt goes RED — but those blocks carry NO claim (no reward, no weight), and the RED
//!   attempt is STILL applied by the merging block's fold and renews the ledger (**reds count**).
//! * `…after_an_idle_stretch_longer_than_k…` — the first REAL attempt after more than K quiet slots can go RED
//!   (the floor resumed and extended the chain under it), is accepted all the same, and the NEXT is BLUE.
//! * `…carries_its_slots_tick…` — B: with no heartbeat in the chain a REAL attempt carries its slot's tick,
//!   one tick a slot however many attempt-lane blocks the slot holds; the control (the fence unarmed) ticks
//!   not at all.
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards};
use super::{OnetimeTxSelector, new_miner_data};
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
use kaspa_consensus_core::palw_real_share_v1::{PALW_REAL_IDLE_K_SLOTS_V1 as K, PALW_T12_USEFUL_WORK_FENCES_V1, palw_real_last_accept_v1};
use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
use kaspa_hashes::Hash64;
use libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair;

/// The 8k producer's inference, as P2 measured it on the live chain (p50 342 s): the drill's default delay.
const INFERENCE_MS: u64 = 340_000;
/// One clock slot.
const SLOT: u64 = kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS;

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
}

/// testnet-12 as launched, with the Useful Work Transition's two fences armed from genesis when `fences`.
fn config_of(fences: bool) -> (Config, PalwConsensusParamsV2, Vec<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)>, Vec<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)>) {
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
        kaspa_core::log::try_init_logger("warn");
        let (config, bundle, premine, floats) = config_of(fences);
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
        Rig { c, k8, nonce: 1 << 45 }
    }

    fn floor(&self) -> Hash64 {
        self.c.bundle.base_class_id
    }

    /// **The 8k class planted `Active` on eight ready seats at the sink** — `t12_rcore_s6_producer_share`'s
    /// plant. Idempotent, and done right BEFORE a REAL attempt's template (never between a template and its
    /// landing: the attempt's own fold runs on the state it was built on).
    fn plant(&self) {
        let vp = self.c.vp();
        let (sink, tip) = vp.palw_state_v2_store.read().load_tip(&self.c.bundle.state).unwrap().expect("the tip loads");
        let now = self.c.ctx.consensus.get_virtual_daa_score();
        let mut carriage = PalwStateCarriageV2::from_state(&tip);
        carriage.model_lifecycles.get_mut(&self.k8).expect("the 8k row").state = PalwModelLifecycleV1::Active;
        for bond in &self.c.bonds {
            carriage.seat_readiness.insert(
                (*bond, self.k8),
                PalwSeatReadinessRowV1 { proved_daa: now, proved_span: now, leaf_index: 0, proof_version: 2, chunks: 8 },
            );
        }
        let planted = carriage.into_state(&self.c.bundle.state, None).expect("the planted tip is a consistent state");
        vp.palw_state_v2_store.write().set_tip_for_tests(sink, &planted).expect("the planted tip becomes the tip");
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

    fn last_real_accept(&self) -> Option<u64> {
        palw_real_last_accept_v1(self.state().real_work_ledger())
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

/// **Past the fence, on a busy chain: floors are refused, the slow REAL attempt is BLUE and accepted.**
#[tokio::test]
async fn real_share_past_the_fence_a_busy_chain_refuses_floors_and_the_slow_real_attempt_is_blue_and_accepted() {
    let mut rig = Rig::new(true).await;
    // Idle at the fence (no REAL attempt yet): the floor is the bonded fallback and is admitted.
    rig.assert_floors_free("before any REAL attempt");
    // R0: the first REAL attempt, a chain block. Accepted at once (its own fold), it makes the chain busy.
    rig.plant();
    let (r0, r0_id) = rig.real_now(1, 1_000).await;
    let state = rig.state();
    assert!(state.claim(&r0_id).is_some(), "R0 is accepted");
    assert_eq!(rig.last_real_accept(), Some(rig.c.daa_of(r0.header.hash)), "the idle ledger holds R0's DAA");
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
    // The idle ledger holds the DAA of the block that ACCEPTED it: its own fold when it is the chain's
    // selected parent, the merging block's when it was merged.
    let accepted_at = if colour == Colour::SelectedParent { rig.c.daa_of(d.header.hash) } else { rig.c.daa_of(merger.header.hash) };
    assert_eq!(rig.last_real_accept(), Some(accepted_at), "the idle ledger is renewed");
    rig.assert_floors_held("after the slow attempt landed");
}

/// **The residual, stated: a producer that ignores the pre-check can still push a REAL attempt RED — and earns
/// nothing for it; and a RED REAL attempt still renews the idle ledger.** The colouring is header-stage and
/// reads neither the class nor the ledger, so two floor-lane blocks in the slow attempt's anticone colour it RED
/// whoever mined them. What A″ changes is what those blocks are: valid blocks the fold skips — no claim, no
/// weight, no reward (the worker carve is withheld and burned) — and the RED attempt is merged and APPLIED by
/// the block that merges it (reds are applied, `palw_v2_merged_works`), so the ledger still hears of it.
#[tokio::test]
async fn real_share_a_rogue_floor_still_colours_classically_but_earns_nothing_and_a_red_real_attempt_still_renews_the_ledger() {
    let mut rig = Rig::new(true).await;
    rig.plant();
    rig.real_now(1, 1_000).await;
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
    assert_eq!(after_floors.real_work_ledger(), before.real_work_ledger(), "and the ledger does not move for a floor");
    rig.c.ctx.simulated_time = rig.c.ctx.simulated_time.max(templated_at + INFERENCE_MS);
    let (merger, colour) = rig.land(&d).await;
    eprintln!("[real-share] past the fence, two rogue floors under the slow attempt: {colour:?}");
    assert_eq!(colour, Colour::Red, "the colouring is classic against a floor-lane block, however the fold treats it");
    let state = rig.state();
    let claim = state.claim(&d_id).expect("the RED attempt is applied by the merging block's fold");
    assert_eq!(claim.accepted_block, d.header.hash);
    assert_eq!(rig.last_real_accept(), Some(rig.c.daa_of(merger.header.hash)), "reds count: the ledger holds the merging block's DAA");
    assert!(state.bounded_immature() > after_floors.bounded_immature(), "and the RED attempt's weight is credited");
    rig.assert_floors_held("the RED attempt renewed the ledger");
}

/// **The first REAL attempt after an idle stretch longer than K can go RED — and is the last to.** K slots after
/// the last REAL acceptance the bonded floor resumes; a REAL attempt whose template was taken then meets the
/// floors that extended the chain under it and goes RED (the honest limit of K), is accepted all the same, and
/// its acceptance holds the floor again — so the next slow attempt is BLUE.
#[tokio::test]
async fn real_share_the_first_real_attempt_after_an_idle_stretch_longer_than_k_can_go_red_and_the_next_is_blue() {
    let mut rig = Rig::new(true).await;
    rig.plant();
    let (r0, _) = rig.real_now(1, 1_000).await;
    let d0 = rig.c.daa_of(r0.header.hash);
    // K - 1 slots after the last REAL acceptance the floor is still held; at K it is free.
    loop {
        let now = rig.c.ctx.consensus.get_virtual_daa_score();
        if now >= d0 + K {
            break;
        }
        if now + 1 == d0 + K {
            rig.assert_floors_held("one slot short of K");
        }
        rig.beat(1).await;
    }
    assert_eq!(rig.c.ctx.consensus.get_virtual_daa_score(), d0 + K, "the walk stops exactly at K");
    rig.assert_floors_free("K slots after the last REAL acceptance: the weak stretch is over and the bonded floor resumes");

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
    rig.c.ctx.simulated_time = rig.c.ctx.simulated_time.max(templated_at + INFERENCE_MS);
    let (merger, colour) = rig.land(&d).await;
    eprintln!("[real-share] the first REAL attempt after more than K idle slots, three floors under it: {colour:?}");
    assert_eq!(colour, Colour::Red, "the floors that resumed extended the chain under it");
    assert!(rig.state().claim(&d_id).is_some(), "accepted all the same");
    assert_eq!(rig.last_real_accept(), Some(rig.c.daa_of(merger.header.hash)));
    rig.assert_floors_held("its acceptance holds the floor again");

    // The next slow attempt meets heartbeats only.
    rig.plant();
    let (d2, d2_id) = rig.real(6, 1_000);
    let templated_at = d2.header.timestamp;
    let beats = rig.beat_until(templated_at + INFERENCE_MS).await;
    rig.assert_floors_held("while the next attempt is being drawn");
    let (_m2, colour2) = rig.land(&d2).await;
    eprintln!("[real-share] the next REAL attempt, {beats} heartbeats under it: {colour2:?}");
    assert!(matches!(colour2, Colour::SelectedParent | Colour::Blue), "the next attempt is BLUE: {colour2:?}");
    assert!(rig.state().claim(&d2_id).is_some());
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
