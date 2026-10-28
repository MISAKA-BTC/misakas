//! **RFC-0018 §10 T-1/T-2, in process, on testnet-12's own GHOSTDAG (k = 1): three models, three miners, sibling attempts from the
//! same parents — all retained, merged, and each claim recorded and attributed on its own, whatever order the blocks arrive in.**
//!
//! The chain is testnet-12 as launched (the harness cards). Every genesis model row that is not the floor is planted `Active` on eight
//! ready seats (`t12_real_share`'s plant), so three classes are open: the floor and the two model rows. Three cards each build an
//! attempt of a DIFFERENT class on the SAME virtual (three templates before any is inserted), the three land as siblings, and the next
//! heartbeat merges them — one is the selected parent, the others are merged blue or red (k = 1).
//!
//! * `…all_retained_and_each_recorded…` — every sibling is in the merging block's past (selected parent, blue or red) and every claim
//!   is in the state with its own bond, class and carrying block (attribution is per claim, never per round or per parent).
//! * `…arrival_order_does_not_matter…` — a second node receives the same blocks in the reverse order: the same sink, the same
//!   GHOSTDAG colouring and the same PALW state root (DAG interpretation and adopted claims converge).
//! * `…with_the_sibling_fence…` — the same two runs with `palw_dag_sibling_attribution_v1` armed from genesis (a FIXTURE: the fence is
//!   refused by validation on every network; the harness does not validate): unchanged retention, and the canonical order yields the
//!   same state on both arrival orders.
//!
//! **Unverified source (2026-10-15): written, not built and not run.** Real-process versions: `scripts/drills/rfc0018_dag_drill.py`.
#![allow(warnings)]
use super::OnetimeTxSelector;
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::{PalwModelLifecycleV1, PalwSeatReadinessRowV1};
use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
use kaspa_hashes::Hash64;

type Premine = Vec<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)>;
type Parts = (Config, PalwConsensusParamsV2, Premine, Premine);

fn card_pubkey(i: usize) -> Vec<u8> {
    TestConsensus::palw_v2_registry_keypair(i as u64).verification_key.as_ref().to_vec()
}

/// testnet-12 as launched; with `sibling_fence`, RFC-0018's sibling attribution armed from genesis (fixture only).
fn parts(sibling_fence: bool) -> Parts {
    let (config, _bundle, premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    // testnet-12's second model row is the 2M row, which takes no claim without a measured verification row (ADR-0152 §4-quater
    // V2, U-D1: `ClassDeadlineUnmeasured`, derived 13,995 DAA > 600). Validation 2026-10-17 found this source's card-2 sibling
    // refused for exactly that (its claim unrecorded, and as a chain candidate its block disqualified). Open it as on its flag day
    // with the same measured row the R-core door-gate suite uses, so all three rows take claims.
    params.palw_class_verify_rows = Box::leak(Box::new([super::t12_rcore_sr10_door_gate::t12_2m_flag_day_row(&params)]));
    params.sync_palw_class_verify_deadline();
    if sibling_fence {
        params.palw_dag_sibling_attribution_v1 = Some(ForkActivation::always());
        params.sync_palw_rfc0018_v1();
    }
    // Armed the way every kernel-route harness arms a dormant fence: `ConfigBuilder::build` runs `validate_palw_v2`, which refuses
    // every RFC-0018 fence by design, so the config is constructed directly (the shipping validation still refuses it).
    params.skip_proof_of_work = true;
    if sibling_fence {
        assert!(params.validate_palw_v2().is_err(), "the real validation still refuses the sibling fence");
    }
    let config = Config::new(params);
    let bundle = match &config.params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => unreachable!("testnet-12 is ConsensusV2"),
    };
    assert_eq!(
        bundle.state.rfc0018_v1().is_some_and(|m| m.sibling_attribution_at(0)),
        sibling_fence,
        "the fold's mirror follows the fixture fence"
    );
    (config, bundle, premine, floats)
}

struct Rig {
    c: T12Chain,
    /// The three open classes: the floor first, then the planted model rows.
    classes: Vec<Hash64>,
    nonce: u64,
    /// The blocks this rig mined before the siblings (a replica replays them: the harness's heartbeat coinbase is random).
    history: Vec<Block>,
}

impl Rig {
    async fn new((config, bundle, premine, floats): Parts) -> Rig {
        kaspa_core::log::try_init_logger("warn");
        let mut c = t12_genesis_chain(&config, &bundle, &premine, &floats);
        let mut history = Vec::new();
        for _ in 0..20 {
            history.push(c.heartbeat(1_000, Vec::new()).await);
        }
        Rig::finish(c, &bundle, history)
    }

    /// **A replica of `of`**: a fresh node on the same genesis that receives `of`'s pre-sibling blocks, then the same plant — so it
    /// starts at the same sink with the same state, and differs only in what arrives next and in what order.
    async fn replica((config, bundle, premine, floats): Parts, of: &Rig) -> Rig {
        let mut c = t12_genesis_chain(&config, &bundle, &premine, &floats);
        for block in &of.history {
            c.ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await.expect("a replayed block is valid");
            c.ctx.simulated_time = c.ctx.simulated_time.max(block.header.timestamp);
        }
        Rig::finish(c, &bundle, of.history.clone())
    }

    fn finish(c: T12Chain, bundle: &PalwConsensusParamsV2, history: Vec<Block>) -> Rig {
        let mut classes = vec![bundle.base_class_id];
        {
            let (_, tip) = c.tip_state();
            classes.extend(tip.model_lifecycles_iter().map(|(id, _)| *id).filter(|id| *id != bundle.base_class_id).take(2));
        }
        assert_eq!(classes.len(), 3, "testnet-12's genesis holds the floor and two model rows");
        let mut rig = Rig { c, classes, nonce: 1 << 46, history };
        rig.plant();
        rig
    }

    /// Every non-floor class `Active` on eight ready seats at the sink (`t12_real_share`'s plant, for each row).
    fn plant(&mut self) {
        let vp = self.c.vp();
        let (sink, tip) = vp.palw_state_v2_store.read().load_tip(&self.c.bundle.state).unwrap().expect("the tip loads");
        let now = self.c.ctx.consensus.get_virtual_daa_score();
        let mut carriage = PalwStateCarriageV2::from_state(&tip);
        for class in self.classes.iter().skip(1) {
            let row = carriage.model_lifecycles.get_mut(class).expect("the model row");
            row.state = PalwModelLifecycleV1::Active;
            row.profile.max_inflight_claims = row.profile.max_inflight_claims.max(16);
            // Inside the launched receipt window (the class verify deadline, ADR on measured rows): a fixture's model row whose derived
            // verify deadline exceeds the window has no measured row and every attempt of it is refused (`ClassDeadlineUnmeasured`).
            row.work.verification_ccu = row.work.verification_ccu.min(1);
            for bond in &self.c.bonds {
                carriage.seat_readiness.insert(
                    (*bond, *class),
                    PalwSeatReadinessRowV1 { proved_daa: now, proved_span: now, leaf_index: 0, proof_version: 2, chunks: 8 },
                );
            }
        }
        let planted = carriage.into_state(&self.c.bundle.state, None).expect("the planted tip is a consistent state");
        vp.palw_state_v2_store.write().set_tip_for_tests(sink, &planted).expect("the planted tip becomes the tip");
    }

    /// **An attempt of `class` by `card` on the CURRENT virtual, built and not inserted** (`t12_real_share::Rig::build_of`).
    fn build_of(&mut self, class: Hash64, card: usize) -> (MutableBlock, Hash64) {
        use kaspa_consensus_core::palw_attempt_v2::{
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
            PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
            palw_network_domain_v2_for,
        };
        self.c.ctx.simulated_time += 1_000;
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
        stamp_harness_time(&self.c.config.params, &mut t.block.header, self.c.ctx.simulated_time);
        t.block.header.nonce = self.nonce;
        let facts = self.c.ctx.consensus.palw_producer_facts_v2(class, Some(bond.0)).expect("a V2 network answers");
        let bond_facts = facts.bond.as_ref().expect("a genesis card is a registered bond").clone();
        let network_domain =
            palw_network_domain_v2_for(self.c.config.params.net.to_string().as_bytes(), Some(self.c.config.params.genesis.hash));
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
            output_root: Hash64::from_u64_word(0x0018_0000_0000_0000 | self.nonce),
            execution_root: Hash64::from_u64_word(0x18EC_0000_0000_0000 | self.nonce),
            pwu: facts.pwu,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        let anchor = execution_anchor_v3(network_domain, pre_pow, facts.class_id, &bond.0, header.nonce);
        let mut won = false;
        for draw in 0u64..4_000_000 {
            attempt.trace_root = Hash64::from_u64_word((self.nonce << 32) ^ draw ^ 0x1800_0000_0000_0000);
            attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
            if class_ticket_v3(&attempt, anchor) <= facts.class_target {
                won = true;
                break;
            }
        }
        assert!(won, "class {class}'s lottery is winnable");
        let claim_id = attempt_id_v2(&attempt);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &TestConsensus::palw_v2_registry_keypair(card as u64).signing_key,
            claim_id.as_byte_slice(),
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
            [0x5Au8; 32],
        )
        .expect("ML-DSA-87 sign over a 64-byte attempt id")
        .as_ref()
        .to_vec();
        let envelope = PalwAttemptEnvelopeV2 { attempt, signature };
        t.block.header.palw_commitment = envelope.encode_wire();
        t.block.header.finalize();
        // Diagnostic: the block's own attempt folded on the tip as the pipeline folds it (a refusal names why it is not merged).
        {
            let (_, tip) = self.c.tip_state();
            let params = self.c.vp().palw_state_params_v2.clone().expect("ConsensusV2");
            let last = tip.last_point().expect("a folded tip");
            let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                block: t.block.header.hash,
                daa_score: t.block.header.daa_score,
                blue_score: last.blue_score + 1,
                subsidy: 0,
            };
            if let Err(why) = self.c.vp().palw_v2_fold_attempt_for_tests(&tip, &params, &point, &[], &envelope, &t.block.header) {
                eprintln!("rfc18 sibling diagnostic: card {card}'s attempt of class {class} is refused by the fold: {why:?}");
            }
        }
        (t.block, claim_id)
    }

    async fn insert(&mut self, block: &Block) {
        self.c
            .ctx
            .consensus
            .validate_and_insert_block(block.clone())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("{} was refused: {e}", block.header.hash));
        self.c.ctx.simulated_time = self.c.ctx.simulated_time.max(block.header.timestamp);
    }
}

/// One sibling: its block, its claim, the card (miner) and the class (model).
struct Sibling {
    block: Block,
    claim: Hash64,
    card: usize,
    class: Hash64,
}

/// Three models × three miners from the same parents, inserted in `order`, merged by one heartbeat. Returns the siblings and the merger.
async fn three_models_from_the_same_parents(rig: &mut Rig) -> (Vec<Sibling>, Block) {
    let mut built = Vec::new();
    for (card, class) in [(1usize, rig.classes[0]), (2, rig.classes[1]), (3, rig.classes[2])] {
        let (block, claim) = rig.build_of(class, card);
        built.push(Sibling { block: block.to_immutable(), claim, card, class });
    }
    let parents: Vec<Vec<BlockHash>> = built.iter().map(|s| s.block.header.direct_parents().to_vec()).collect();
    assert!(parents.windows(2).all(|w| w[0] == w[1]), "the three siblings share their parents");
    for s in &built {
        rig.insert(&s.block).await;
    }
    let merger = rig.c.heartbeat(1_000, Vec::new()).await;
    (built, merger)
}

fn assert_retained_and_attributed(tag: &str, rig: &Rig, siblings: &[Sibling], merger: BlockHash) {
    let gd = rig.c.vp().ghostdag_store.get_data(merger).unwrap();
    for s in siblings {
        let hash = s.block.header.hash;
        let merged = gd.selected_parent == hash || gd.mergeset_blues.contains(&hash) || gd.mergeset_reds.contains(&hash);
        if !merged {
            let merger_header = rig.c.ctx.consensus.get_header(merger).unwrap();
            eprintln!(
                "[rfc18-diag] {tag}: card {} sibling {hash} status {:?}; merger parents {:?}; sibling daa {} merger daa {}; tips {:?}",
                s.card,
                rig.c.ctx.consensus.get_block_status(hash),
                merger_header.direct_parents(),
                s.block.header.daa_score,
                merger_header.daa_score,
                rig.c.ctx.consensus.get_tips()
            );
        }
        assert!(merged, "{tag}: sibling {hash} (card {}) is in the merger's past as selected parent, blue or red", s.card);
    }
    let (_, state) = rig.c.tip_state();
    let mut seen_classes = std::collections::BTreeSet::new();
    for s in siblings {
        let claim = state.claim(&s.claim).unwrap_or_else(|| panic!("{tag}: card {}'s claim {} is recorded", s.card, s.claim));
        assert_eq!(claim.bond, rig.c.bonds[s.card], "{tag}: attributed to its own miner's bond");
        assert_eq!(claim.class_id, s.class, "{tag}: recorded under its own model");
        assert_eq!(claim.accepted_block, s.block.header.hash, "{tag}: carried by its own block, blue or red");
        seen_classes.insert(claim.class_id);
    }
    assert_eq!(seen_classes.len(), 3, "{tag}: three models, three claims");
}

async fn run_all_retained(sibling_fence: bool) {
    let tag = if sibling_fence { "sibling fence" } else { "launched" };
    let mut rig = Rig::new(parts(sibling_fence)).await;
    let (siblings, merger) = three_models_from_the_same_parents(&mut rig).await;
    assert_retained_and_attributed(tag, &rig, &siblings, merger.header.hash);
}

/// The same blocks, delivered to fresh nodes in EVERY permutation of the three siblings' arrival (3! = 6), give the same sink,
/// colouring, PALW state root (adopted claims, caps consumed, escrows), safe frontier and ADR-0170 seed anchor (RFC-0018 §4.2: under
/// the fence the anchor is the least-priority admitted attempt of the span, never the last write).
async fn run_arrival_order(sibling_fence: bool) {
    let tag = if sibling_fence { "sibling fence" } else { "launched" };
    let mut a = Rig::new(parts(sibling_fence)).await;
    let start = a.c.sink();
    let (siblings, merger) = three_models_from_the_same_parents(&mut a).await;
    let (ga, sa) = (a.c.vp().ghostdag_store.get_data(merger.header.hash).unwrap(), a.c.tip_state().1);
    for order in [[0usize, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
        let mut b = Rig::replica(parts(sibling_fence), &a).await;
        // The two rigs start byte-identical (same genesis, the same replayed blocks, the same plant).
        assert_eq!(b.c.sink(), start, "{tag}: identical starts");
        for i in order {
            b.insert(&siblings[i].block).await;
        }
        b.insert(&merger).await;
        let tag = format!("{tag}, arrival {order:?}");
        assert_eq!(a.c.sink(), b.c.sink(), "{tag}: one sink");
        let gb = b.c.vp().ghostdag_store.get_data(merger.header.hash).unwrap();
        assert_eq!(ga.selected_parent, gb.selected_parent, "{tag}: one selected parent");
        assert_eq!(ga.mergeset_blues, gb.mergeset_blues, "{tag}: one colouring (blues)");
        assert_eq!(ga.mergeset_reds, gb.mergeset_reds, "{tag}: one colouring (reds)");
        let sb = b.c.tip_state().1;
        assert_eq!(sa.state_root(), sb.state_root(), "{tag}: the adopted claims, caps and escrows are one");
        assert_eq!(sa.safe_frontier(), sb.safe_frontier(), "{tag}: one fork-choice standing");
        assert_eq!(sa.round_seed_anchor(), sb.round_seed_anchor(), "{tag}: one ADR-0170 seed anchor");
        assert_retained_and_attributed(&tag, &b, &siblings, merger.header.hash);
    }
}

#[tokio::test]
async fn rfc18_three_models_three_miners_from_the_same_parents_are_all_retained_and_each_recorded() {
    run_all_retained(false).await;
}

#[tokio::test]
async fn rfc18_arrival_order_does_not_matter_for_dag_claims_or_state() {
    run_arrival_order(false).await;
}

#[tokio::test]
async fn rfc18_with_the_sibling_fence_retention_and_arrival_order_independence_hold() {
    run_all_retained(true).await;
    run_arrival_order(true).await;
}
