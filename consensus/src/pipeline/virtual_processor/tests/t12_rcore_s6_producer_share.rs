//! **ADR-0152 v3.1 S-6 (T-2(a)) at the processor, on testnet-12**: the two reads a producer takes
//! before it spends an inference — `palw_producer_facts_v2`'s `class_admission_refusal`, which
//! `ready_to_produce` reads, and the free-prompt price a gateway (or the node's canonical rail) reads
//! before it writes a commitment — carry the fold's per-bond share, so a bond at its share of a class
//! is held before the inference, not skipped after it (the S-6 review's finding 1: a skipped own
//! attempt's worker carve is withheld and burned, `palw_v2_skipped_own_attempt_carve`).
//!
//! The chain is testnet-12 with harness cards, heartbeats until the virtual is past the registry's
//! grace (30 spans of one DAA), then a tip planted through the carriage: the short-window row
//! `Active` on eight ready cards (room 5, so a share of ⌈5 / 2⌉ = 3) and card 1 holding three
//! unlicensed claims of it. Card 2 holds none. The fence-off twin (`palw_rcore_plus` unset) plants
//! the same tip and reads no share.
use super::t12_round_lane_e2e::{stamp_harness_time, t12_with_harness_cards};
use super::{OnetimeTxSelector, TestContext, new_miner_data};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::TemplateBuildMode;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_model_registry_v1::{PalwModelLifecycleV1, PalwSeatReadinessRowV1};
use kaspa_consensus_core::palw_producer_v2::PALW_NOT_READY_CLASS_NOT_ADMITTING_V2;
use kaspa_consensus_core::palw_state_v2::{
    PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwClaimStateV2, PalwConsensusObjectV2 as Obj,
    PalwStateCarriageV2, PalwStateV2Error, palw_claim_bond_reservation_v1,
};
use kaspa_hashes::Hash64;
use kaspa_muhash::MuHash;

/// The virtual DAA the heartbeats reach: past the registry's grace (activation 0 + 30 spans × 1 DAA),
/// so the room governs and the share is a rule.
const PAST_THE_GRACE_DAA: u64 = 40;

struct Planted {
    ctx: TestContext,
    short: Hash64,
    cards: Vec<(PalwBondKeyV2, Vec<u8>)>,
}

/// testnet-12 with harness cards (`armed = false`: `palw_rcore_plus` unset, the mirrors re-synced),
/// heartbeats past the grace, and the tip planted as the module doc says.
async fn planted(armed: bool) -> Planted {
    let (config, _harness_bundle, premine, _floats) = t12_with_harness_cards();
    let config: Config = if armed {
        config
    } else {
        let mut params = config.params.clone();
        params.palw_rcore_plus = None;
        params.palw_rcore_conservative_classes = &[];
        params.sync_palw_rcore_plus();
        ConfigBuilder::new(params).skip_proof_of_work().build()
    };
    config.params.validate_palw_v2().expect("the fixture is a runnable ruleset");
    let bundle: PalwConsensusParamsV2 = match &config.params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => {
            assert_eq!(b.state.rcore_plus_active_at(0), armed, "the fold's mirror follows the fence");
            b.clone()
        }
        _ => unreachable!("testnet-12 is ConsensusV2"),
    };
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

    // Heartbeats — testnet-12's clock — until the virtual is past the grace. A slot takes two beats
    // (the one stamped into the open slot and the one that steps), so the loop counts DAA, not beats.
    let mut nonce = 0u64;
    while ctx.consensus.get_virtual_daa_score() < PAST_THE_GRACE_DAA {
        assert!(nonce < 20 * PAST_THE_GRACE_DAA, "the heartbeats did not reach DAA {PAST_THE_GRACE_DAA} in {nonce} beats");
        nonce += 1;
        ctx.simulated_time += config.params.target_time_per_block();
        let mut t = ctx
            .consensus
            .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
            .expect("a template");
        stamp_harness_time(&config.params, &mut t.block.header, ctx.simulated_time);
        t.block.header.nonce = nonce;
        t.block.header.finalize();
        let (t, _) = ctx.consensus.virtual_processor().heartbeat_adapt_block_template(t).expect("the heartbeat lane is open");
        ctx.simulated_time = ctx.simulated_time.max(t.block.header.timestamp);
        let block = t.block.to_immutable();
        ctx.consensus.validate_and_insert_block(block).virtual_state_task.await.expect("a heartbeat");
    }

    let vp = ctx.consensus.virtual_processor().clone();
    let (sink, tip) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let now = ctx.consensus.get_virtual_daa_score();
    let cards: Vec<(PalwBondKeyV2, Vec<u8>)> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            Obj::BondRegistered { bond, pubkey, .. } => Some((*bond, pubkey.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(cards.len(), 8, "testnet-12 registers eight cards");
    let short = tip
        .model_lifecycles_iter()
        .find(|(id, row)| **id != bundle.base_class_id && !kaspa_consensus_core::palw_work_target_v1::palw_panel_held_to_final_v1(row))
        .map(|(id, _)| *id)
        .expect("testnet-12's short-window row");

    // The planted tip: the short row admitting on eight fresh cards, and card 1's three unlicensed
    // claims of it — each with the reservation the ledger would hold, so the load's consistency
    // check (claims ⇔ `reserved_exposure`) stands.
    let mut carriage = PalwStateCarriageV2::from_state(&tip);
    carriage.model_lifecycles.get_mut(&short).expect("a model row").state = PalwModelLifecycleV1::Active;
    for (card, _) in &cards {
        carriage.seat_readiness.insert(
            (*card, short),
            PalwSeatReadinessRowV1 { proved_daa: now, proved_span: now, leaf_index: 0, proof_version: 2, chunks: 8 },
        );
    }
    let accepted_block = tip.last_point().map(|p| p.block).unwrap_or(sink);
    for i in 0..3u64 {
        let claim = PalwClaimStateV2 {
            source: PalwClaimSourceV2::Attempt,
            class_id: short,
            bond: cards[1].0,
            pwu: 1,
            accepted_daa: now - 1,
            rebound_daa: None,
            accepted_blue_score: now - 1,
            accepted_block,
            trace_root: Hash64::from_u64_word(0x5601 + i),
            output_root: Hash64::from_u64_word(0x5611 + i),
            execution_root: Hash64::from_u64_word(0x5621 + i),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            reserved: 1,
            immature_contribution: 0,
            escrowed_reward: 0,
            work_leaves: 0,
            work_id: None,
            phase: PalwClaimPhaseV2::Provisional,
            rights_reserved: 0,
            job_identity: Hash64::default(),
            rcore: Default::default(),
        };
        let held = palw_claim_bond_reservation_v1(&bundle.state, &claim).expect("a reservation");
        *carriage.reserved_exposure.entry(cards[1].0).or_insert(0) += held;
        carriage.claims.insert(Hash64::from_u64_word(0x5600 + i), claim);
    }
    let planted: PalwChainStateV2 = carriage.into_state(&bundle.state, None).expect("the planted tip is a consistent state");
    vp.palw_state_v2_store.write().set_tip_for_tests(sink, &planted).expect("the planted tip becomes the tip");
    Planted { ctx, short, cards }
}

impl Planted {
    /// The free-prompt price answer for card `card` on the short row, as a gateway asks it.
    fn fp_price(&self, card: usize) -> Result<(), PalwStateV2Error> {
        self.ctx
            .consensus
            .virtual_processor()
            .palw_fp_commitment_price_impl(self.short, &[], 1, 1, 1, Some(self.cards[card].0.0))
            .expect("a V2 network answers")
            .price
            .map(|_| ())
    }
}

/// **At its share, the producer's own pre-check holds the bond** — `ready_to_produce` answers
/// `PALW_NOT_READY_CLASS_NOT_ADMITTING_V2` with the share named, while the class itself still admits
/// (card 2, holding none, is not held by it); and the free-prompt price is the fold's
/// `BondClassShareExceeded`, the refusal the commitment arm reaches first.
#[tokio::test]
async fn s6_a_bond_at_its_share_is_held_by_the_producer_precheck_and_the_fp_price() {
    let p = planted(true).await;
    let facts = p.ctx.consensus.palw_producer_facts_v2(p.short, Some(p.cards[1].0.0)).expect("a V2 network answers");
    let why = facts.class_admission_refusal.as_deref().expect("the share refuses card 1's fourth claim");
    assert!(why.contains("holds 3 unlicensed claims") && why.contains("its share of the class is 3"), "{why}");
    assert_eq!(facts.ready_to_produce(&p.cards[1].1), Err(PALW_NOT_READY_CLASS_NOT_ADMITTING_V2), "held before the inference");

    let other = p.ctx.consensus.palw_producer_facts_v2(p.short, Some(p.cards[2].0.0)).expect("a V2 network answers");
    assert_eq!(other.class_admission_refusal, None, "the class admits, and card 2's share is its own");

    match p.fp_price(1) {
        Err(PalwStateV2Error::BondClassShareExceeded { unlicensed: 3, share: 3, .. }) => {}
        other => panic!("card 1's free-prompt price is the share's refusal, got {other:?}"),
    }
    assert!(
        !matches!(p.fp_price(2), Err(PalwStateV2Error::BondClassShareExceeded { .. })),
        "card 2's price is whatever the lane prices, not the share"
    );
}

/// **The fence-off twin: no share, so neither read holds card 1** on the same planted tip — the
/// class gate alone answers (three claims in a room of five), exactly the pre-R-core+ reads.
#[tokio::test]
async fn s6_fence_off_twin_the_precheck_reads_no_share() {
    let p = planted(false).await;
    let facts = p.ctx.consensus.palw_producer_facts_v2(p.short, Some(p.cards[1].0.0)).expect("a V2 network answers");
    assert_eq!(facts.class_admission_refusal, None, "below palw_rcore_plus there is no share to read");
    assert!(!matches!(p.fp_price(1), Err(PalwStateV2Error::BondClassShareExceeded { .. })));
}
