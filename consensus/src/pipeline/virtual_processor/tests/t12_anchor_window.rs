//! **ADR-0170 on testnet-12's own GHOSTDAG: with floors held and REAL attempts flowing, a Candidate class is admitted within two periods —
//! and without the window it is not.**
//!
//! The chain is testnet-12 as launched (the harness cards, every window as shipped) with ADR-0165's reserve armed from genesis, so honest
//! floor producers HOLD (none is mined here) while REAL attempts flow: the 8k class planted `Active` on eight ready seats the way
//! `t12_real_share` plants it, each attempt a real block of that class, drawn for 340 s (so it lands as a side block and the next heartbeat
//! MERGES it — blue or red, never a chain block), exactly the regime the Useful Work Transition aims at. A second genesis model row (the
//! held 2M one) is planted `Candidate` with fresh readiness proofs on all eight seats, so a jury of the network's operators finds a majority
//! holding it whenever one SITS — which needs the seed anchor of the span before its audit.
//!
//! Under the reserve alone that anchor never exists: floors are refused, heartbeats carry no attempt, and a REAL attempt is merged, never the
//! chain block that records an anchor. The class stays `Candidate` through every audit. With `palw_anchor_window_v1` armed beside it, the
//! merged REAL attempts anchor (M1), the anchor survives the span boundaries (M2), the audit reads the latest anchor of its window (M3), and the
//! class leaves `Candidate` AT ITS FIRST AUDIT.
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards};
use super::OnetimeTxSelector;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_anchor_window_v1::{PALW_ANCHOR_WINDOW_SPANS_V1 as W, PALW_T12_ANCHOR_WINDOW_FENCES_V1};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::{PalwModelLifecycleV1, PalwSeatReadinessRowV1};
use kaspa_consensus_core::palw_real_share_v1::PALW_T12_USEFUL_WORK_FENCES_V1;
use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
use kaspa_hashes::Hash64;
use libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair;

/// The 8k producer's inference, as P2 measured it on the live chain (p50 342 s).
const INFERENCE_MS: u64 = 340_000;
/// One audit period on testnet-12, in DAA (one-DAA spans).
const PERIOD: u64 = 100;

fn card_key(i: usize) -> &'static MLDSA87KeyPair {
    TestConsensus::palw_v2_registry_keypair(i as u64)
}

fn card_pubkey(i: usize) -> Vec<u8> {
    card_key(i).verification_key.as_ref().to_vec()
}

/// testnet-12 as launched with the reserve armed from genesis, and the window too when `window`.
fn config_of(window: bool) -> (Config, PalwConsensusParamsV2, Vec<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)>, Vec<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)>) {
    let (config, _bundle, premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    for fence in PALW_T12_USEFUL_WORK_FENCES_V1 {
        (fence.set)(&mut params, Some(ForkActivation::always()));
    }
    if window {
        for fence in PALW_T12_ANCHOR_WINDOW_FENCES_V1 {
            (fence.set)(&mut params, Some(ForkActivation::always()));
        }
    }
    params.validate_palw_v2().expect("the reserve (and the window) validate on testnet-12 from genesis");
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    let bundle = match &config.params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => unreachable!("testnet-12 is ConsensusV2"),
    };
    assert_eq!(bundle.state.anchor_window_active_at(0), window, "the fold's mirror follows the fence");
    (config, bundle, premine, floats)
}

/// One honest heartbeat slot, as the H1 miner mines it.
async fn honest_slot(c: &mut T12Chain) -> Vec<Block> {
    let start = c.daa_of(c.sink());
    let mut out = vec![c.heartbeat(1_000, Vec::new()).await];
    while c.daa_of(c.sink()) == start {
        assert!(out.len() < 4, "an honest slot steps within four beats");
        out.push(c.heartbeat(1_000, Vec::new()).await);
    }
    out
}

struct Rig {
    c: T12Chain,
    /// The 8k class: the genesis model row that is neither the floor nor held to Final — the REAL attempts' class.
    k8: Hash64,
    /// The Candidate: the other model row (the held 2M one), planted `Candidate`.
    head: Hash64,
    head_planted: bool,
    nonce: u64,
}

impl Rig {
    async fn new(window: bool) -> Rig {
        kaspa_core::log::try_init_logger("warn");
        let (config, bundle, premine, floats) = config_of(window);
        let mut c = t12_genesis_chain(&config, &bundle, &premine, &floats);
        let (k8, head) = {
            let (_, tip) = c.tip_state();
            let rows: Vec<(Hash64, bool)> = tip
                .model_lifecycles_iter()
                .filter(|(id, _)| **id != bundle.base_class_id)
                .map(|(id, row)| (*id, kaspa_consensus_core::palw_work_target_v1::palw_panel_held_to_final_v1(row)))
                .collect();
            let k8 = rows.iter().find(|(_, held)| !held).map(|(id, _)| *id).expect("testnet-12's short-window (8k) row");
            let head = rows.iter().find(|(id, _)| *id != k8).map(|(id, _)| *id).expect("a second model row to plant as the Candidate");
            (k8, head)
        };
        for _ in 0..10 {
            honest_slot(&mut c).await;
        }
        Rig { c, k8, head, head_planted: false, nonce: 1 << 46 }
    }

    fn daa(&self) -> u64 {
        self.c.ctx.consensus.get_virtual_daa_score()
    }

    /// **The 8k class `Active` on eight ready seats, and the head class `Candidate` (once) with fresh readiness proofs on the same eight** — at
    /// the sink, right BEFORE a REAL attempt's template (never between a template and its landing). The head's rows are re-dated every time, so
    /// they are fresh whenever an audit comes (a row stands 24 spans).
    fn plant(&mut self) {
        let vp = self.c.vp();
        let (sink, tip) = vp.palw_state_v2_store.read().load_tip(&self.c.bundle.state).unwrap().expect("the tip loads");
        let now = self.daa();
        let mut carriage = PalwStateCarriageV2::from_state(&tip);
        {
            let row = carriage.model_lifecycles.get_mut(&self.k8).expect("the 8k row");
            row.state = PalwModelLifecycleV1::Active;
            // The harness runs no panel, so a REAL claim never resolves and would hold one of the registry's in-flight places for good (5
            // here); the chain's own verification keeps its cap far from full. `t12_real_share` raises it the same way.
            row.profile.max_inflight_claims = 64;
        }
        if !self.head_planted {
            carriage.model_lifecycles.get_mut(&self.head).expect("the head row").state = PalwModelLifecycleV1::Candidate;
            self.head_planted = true;
        }
        for bond in &self.c.bonds {
            for class in [self.k8, self.head] {
                carriage.seat_readiness.insert(
                    (*bond, class),
                    PalwSeatReadinessRowV1 { proved_daa: now, proved_span: now, leaf_index: 0, proof_version: 2, chunks: 8 },
                );
            }
        }
        let planted = carriage.into_state(&self.c.bundle.state, None).expect("the planted tip is a consistent state");
        vp.palw_state_v2_store.write().set_tip_for_tests(sink, &planted).expect("the planted tip becomes the tip");
    }

    /// A REAL attempt (the 8k class) by `card`, built and not inserted — `t12_real_share`'s construction.
    fn real(&mut self, card: usize, step_ms: u64) -> (MutableBlock, Hash64) {
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
        let class = self.k8;
        let facts = self.c.ctx.consensus.palw_producer_facts_v2(class, Some(bond.0)).expect("a V2 network answers");
        let rcore_plus = self.c.config.params.palw_rcore_plus_fence().is_some_and(|f| f.is_active(facts.daa_score));
        facts
            .ready_to_produce_v3(&card_pubkey(card), rcore_plus)
            .unwrap_or_else(|why| panic!("card {card} is not ready to produce for class {class}: {why} ({:?})", facts.class_admission_refusal));
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

    /// **The slow producer's attempt lands**: `d` is inserted as a side block (its parents are where its template was), then the next heartbeat
    /// merges it.
    async fn land(&mut self, d: &MutableBlock) -> Block {
        let d = d.clone().to_immutable();
        self.c.ctx.simulated_time = self.c.ctx.simulated_time.max(d.header.timestamp);
        self.c
            .ctx
            .consensus
            .validate_and_insert_block(d.clone())
            .virtual_state_task
            .await
            .expect("a slow attempt is a valid block when it lands");
        self.c.heartbeat(1_000, Vec::new()).await
    }

    /// Heartbeat slots until the simulated clock reaches `until_ms`.
    async fn beat_until(&mut self, until_ms: u64) {
        while self.c.ctx.simulated_time < until_ms {
            honest_slot(&mut self.c).await;
        }
    }

    /// One REAL cycle: plant, template, 340 s of heartbeats, land. Never a floor.
    async fn real_cycle(&mut self, card: usize) {
        self.plant();
        let started = self.c.daa_of(self.c.sink());
        let (d, _) = self.real(card, 1_000);
        let templated_at = d.header.timestamp;
        self.beat_until(templated_at + INFERENCE_MS).await;
        self.land(&d).await;
        let (sink, anchor) = (self.c.daa_of(self.c.sink()), self.c.tip_state().1.round_seed_anchor().map(|a| a.span));
        eprintln!("[anchor-window] REAL by card {card}: templated at DAA {started}, merged at DAA {sink}, seed anchor span {anchor:?}");
    }

    fn head_state(&self) -> PalwModelLifecycleV1 {
        self.c.tip_state().1.model_lifecycle(&self.head).expect("the head row").state
    }

    /// The audit spans of the head class strictly after `after` — the fold's own predicate (`admission_jury_v1`): the staggered one past the
    /// Activation Pool, every multiple of the period otherwise.
    fn audit_spans(&self, after: u64, n: usize) -> Vec<u64> {
        let pool = self.c.config.params.palw_activation_pool.is_some();
        (after + 1..)
            .filter(|span| {
                if pool {
                    kaspa_consensus_core::palw_activation_pool_v1::palw_admission_audit_due_staggered_v1(&self.head, *span, PERIOD)
                } else {
                    kaspa_consensus_core::palw_model_registry_v1::palw_admission_audit_due_v1(*span, PERIOD)
                }
            })
            .take(n)
            .collect()
    }
}

/// **Walk a chain with floors held and REAL attempts flowing through `n` audit periods of the head class.** The first audit is the first
/// due span at or after DAA 60 (past the registry's grace); a REAL attempt is mined each time the clock is 20 and 9 slots before an audit
/// (so inside the window, and never as a chain block; a cycle spends about four slots, so the last lands well before the audit span). Two a
/// period, not the live chain's twenty: the harness runs no panel, so a REAL claim never resolves and the panel's replay room (five claims of
/// this class) is what bounds the traffic — a sparser anchor supply than the live one, which only makes the case harder for the window.
/// Returns, per audit, `(its span, the head's state after the block that opens it, the span of the seed anchor then)`.
async fn walk(window: bool, n: usize) -> Vec<(u64, PalwModelLifecycleV1, Option<u64>)> {
    let mut rig = Rig::new(window).await;
    assert_eq!(rig.c.config.params.palw_admission_audit_period_daa, Some(PERIOD), "testnet-12's audit period");
    let audits = rig.audit_spans(60, n);
    if n > 1 {
        assert_eq!(audits[1], audits[0] + PERIOD, "one audit a period");
    }
    let real_at: Vec<u64> = audits.iter().flat_map(|a| [a - 20, a - 9]).collect();
    let mut out = Vec::new();
    let mut card = 0usize;
    for audit in audits {
        loop {
            // The sink's DAA: the block that opened span `audit` is in the chain once it reaches it, and the tip state is that block's.
            let now = rig.c.daa_of(rig.c.sink());
            if now >= audit {
                break;
            }
            if real_at.contains(&now) {
                card = (card + 1) % rig.c.bonds.len();
                rig.real_cycle(card).await;
                // A REAL cycle spends ~3 slots (the clock has moved on); the targets are far apart, so none is repeated or skipped.
            } else {
                honest_slot(&mut rig.c).await;
            }
        }
        let state = rig.c.tip_state().1;
        assert_eq!(rig.c.daa_of(rig.c.sink()), audit, "the walk stops at the block that opens the audit span");
        out.push((audit, rig.head_state(), state.round_seed_anchor().map(|a| a.span)));
    }
    out
}

#[tokio::test]
async fn anchor_window_with_floors_held_and_real_flowing_the_head_class_is_admitted_at_its_first_audit() {
    // The window: a merged REAL attempt anchors (M1), the anchor survives the span boundaries (M2), the audit reads the latest anchor of its
    // window (M3) — the jury sits at the FIRST audit and the class leaves Candidate.
    let with = walk(true, 1).await;
    let (first, state, anchor) = with[0];
    eprintln!("[anchor-window] with the window: audit {first}: head {state:?}, anchor span {anchor:?}");
    assert_ne!(state, PalwModelLifecycleV1::Candidate, "the head class is admitted at its first audit (span {first}) with the window");
    let anchor = anchor.expect("a merged REAL attempt anchored");
    assert!(anchor < first && first - anchor <= W, "the anchor is of the window S − {W} … S − 1: {anchor} for the audit at {first}");
}

#[tokio::test]
async fn anchor_window_the_reserve_alone_leaves_no_anchor_and_the_head_class_stays_candidate_through_two_audits() {
    // Without it: floors refused, heartbeats carry no attempt, a REAL attempt is merged and never the chain block that records an anchor — no
    // anchor at any audit, so no jury sits: the int-11 drill's skipped audit, on every period of the same REAL traffic.
    let without = walk(false, 2).await;
    for (span, state, anchor) in &without {
        eprintln!("[anchor-window] reserve only: audit {span}: head {state:?}, anchor span {anchor:?}");
    }
    for (span, state, anchor) in &without {
        assert_eq!(*state, PalwModelLifecycleV1::Candidate, "no anchor, no jury at the audit at span {span}");
        assert_eq!(*anchor, None, "and no seed anchor at all under the reserve alone (audit {span})");
    }
}
