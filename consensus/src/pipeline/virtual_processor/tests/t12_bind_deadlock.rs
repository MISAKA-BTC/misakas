//! **The producer-ceiling bind deadlock on testnet-12, measured and closed** (post-launch lane
//! `rcore/f1-bind-deadlock`, fence `palw_anchor_at_ceiling`).
//!
//! Past `palw_rcore_plus` a claim binds its panel only in its own anchor block (ADR-0152 SW-8), and
//! the anchor is the first ATTEMPT block on the selected chain at or past `bind_base + anchor_delay`
//! (heartbeats never anchor, M4 finding 2). An attempt block is admitted against its parent state,
//! and admission item 8 refuses an attempt whose bond has no room for one more claim
//! (`ExposureCeilingExceeded`) — which the walk turns into `StatusDisqualifiedFromChain`. So when
//! every attempt-capable producer is at its ceiling, no attempt block can exist, no anchor exists,
//! no `Provisional` claim binds, and the room those claims hold is freed only by the `BindTimeout`
//! void at `bind_base + window_bind` — no reward, no forfeit — after which the cycle can repeat.
//! Found live on a salted t12 drill with the shipping build (audit-lifecycle T12-052).
//!
//! * `m0`: the ledger of ONE floor claim on shipped testnet-12 — what it commits on its producer and
//!   its seats at accept, bind, licence and Final, and the windows that date each release. The
//!   fleet arithmetic in the lane report is built from these printed figures.
//! * `r1`: the deadlock itself on the released rule (the fence dormant): one producer fills its
//!   ceiling with `Provisional` claims; past their slot its attempt block is disqualified from the
//!   chain, so nothing binds; the claims void `BindTimeout` at the backstop, unpaid.
//! * `f1`: the fix across its fence — below `palw_anchor_at_ceiling` the at-ceiling attempt is still
//!   disqualified; from it the same producer's attempt is a BINDER: it anchors and binds every claim
//!   due at it, carries no claim of its own (the fold's finding-17 skip) and is paid no worker carve
//!   (on testnet-12 the carve is the whole worker share, so a binder is paid nothing but its fees);
//!   a second binder with nothing left to bind is disqualified as before.
//! * `f2`: the binder beside lane A (`palw_operator_anchor`, with lane F1, all three fences armed):
//!   only an operator's OWN at-ceiling attempt binds. A non-operator's at-ceiling attempt that merges a
//!   displaced operator attempt anchors under lane A — but only the slots that attempt reached, which
//!   are below the claims due at it — and it is disqualified as before (the node is told no binder is
//!   due for a non-operator); the operator's at-ceiling attempt then binds every claim due at it, on
//!   its own execution's seed.
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_producer_v2::{PALW_NOT_READY_EXPOSURE_FULL_V2, PalwProducerFactsV2};
use kaspa_consensus_core::palw_state_v2::{PalwChainStateV2, PalwClaimPhaseV2, PalwVoidReasonV2};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

const MSK: f64 = 1e8;

fn msk(sompi: u128) -> f64 {
    sompi as f64 / MSK
}

fn card_pubkey(card: usize) -> Vec<u8> {
    TestConsensus::palw_v2_registry_keypair(card as u64).verification_key.as_ref().to_vec()
}

/// testnet-12 with harness cards, the post-launch fence at `fence` (`None` = the released rule).
fn t12_at(fence: Option<u64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    assert_eq!(config.params.palw_anchor_at_ceiling, None, "the fence is dormant on testnet-12 as shipped");
    let Some(at) = fence else { return (config, bundle, premine, floats) };
    let mut params = config.params.clone();
    params.palw_anchor_at_ceiling = Some(kaspa_consensus_core::config::params::ForkActivation::new(at));
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("testnet-12 with the fence armed is a runnable ruleset");
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    let bundle = bundle.clone();
    (config, bundle, premine, floats)
}

/// The cards `f2`'s lane-A rule trusts; 6 and 7 stand outside it (as in lane A's own processor test).
const OPERATORS: [usize; 6] = [0, 1, 2, 3, 4, 5];

/// testnet-12 with harness cards and the three post-launch fences this lane composes with armed at one
/// height `h`: lane A (`palw_operator_anchor`, over cards 0–5), its prerequisite lane F1
/// (`palw_panel_seed_execution`) and this lane's `palw_anchor_at_ceiling`.
fn t12_with_lane_a(h: u64) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    use kaspa_consensus_core::config::params::ForkActivation;
    use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    assert_eq!(config.params.palw_operator_anchor, None, "testnet-12 ships lane A's fence dormant");
    let cards: Vec<PalwBondKeyV2> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::BondRegistered { bond, .. } => Some(*bond),
            _ => None,
        })
        .collect();
    let mut operators: Vec<PalwBondKeyV2> = OPERATORS.iter().map(|i| cards[*i]).collect();
    operators.sort();
    let mut params = config.params.clone();
    params.palw_operator_anchor =
        Some(kaspa_consensus_core::palw_operator_anchor_v1::PalwOperatorAnchorV1 { activation: ForkActivation::new(h), operators });
    params.palw_panel_seed_execution = Some(ForkActivation::new(h));
    params.palw_anchor_at_ceiling = Some(ForkActivation::new(h));
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("testnet-12 with lane A, F1 and the binder armed is a runnable ruleset");
    let PalwConsensusMode::ConsensusV2(armed) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    assert_eq!(armed, &bundle, "the fences are Params fields: the bundle does not move");
    (config, bundle, premine, floats)
}

/// The harness chain plus an attempt builder that does NOT refuse an unready card — the node's
/// pre-check is exactly what a binder has to get past, so the test must be able to build one.
struct Fleet {
    chain: T12Chain,
    domain: Hash64,
    nonce: u64,
}

impl Fleet {
    fn new(fence: Option<u64>) -> Self {
        let (config, bundle, premine, floats) = t12_at(fence);
        Self::on(config, bundle, premine, floats)
    }

    fn on(config: Config, bundle: PalwConsensusParamsV2, premine: Premine, floats: Premine) -> Self {
        kaspa_core::log::try_init_logger("warn");
        let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        // Nonces far from the harness's own counter: the execution key reads the nonce.
        Fleet { chain, domain, nonce: 1 << 40 }
    }

    fn ttpb(&self) -> u64 {
        self.chain.config.params.target_time_per_block()
    }

    fn sink_daa(&self) -> u64 {
        self.chain.daa_of(self.chain.sink())
    }

    fn state(&self) -> PalwChainStateV2 {
        self.chain.tip_state().1
    }

    fn facts(&self, card: usize) -> PalwProducerFactsV2 {
        self.chain
            .ctx
            .consensus
            .palw_producer_facts_v2(self.chain.bundle.base_class_id, Some(self.chain.bonds[card].0))
            .expect("testnet-12 answers for its floor")
    }

    /// The producer's own verdict, as kaspad's loop asks it (`ready_to_produce_v3` at the candidate).
    fn ready(&self, card: usize) -> Result<(), &'static str> {
        let facts = self.facts(card);
        facts.ready_to_produce_v3(&card_pubkey(card), self.chain.config.params.palw_rcore_plus_active_at(facts.daa_score))
    }

    fn committed(&self, card: usize) -> u128 {
        self.facts(card).bond.expect("a genesis card").committed
    }

    /// Card `card`'s floor attempt on the node's own template, with a winning class ticket, signed —
    /// [`T12Chain::attempt`]'s carriage without its readiness assertion.
    fn build_attempt(&mut self, card: usize) -> (MutableBlock, Hash64) {
        use kaspa_consensus_core::palw_attempt_v2::{
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
            PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
        };
        let ttpb = self.ttpb();
        self.chain.ctx.simulated_time += ttpb;
        self.nonce += 1;
        let bond = self.chain.bonds[card];
        let mut t = self
            .chain
            .ctx
            .consensus
            .build_block_template(
                MinerData::new(card_payout_spk(card), vec![]),
                Box::new(super::OnetimeTxSelector::new(Vec::new())),
                TemplateBuildMode::Standard,
            )
            .expect("a template");
        assert!(kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(t.block.header.pow_algo_id));
        super::t12_round_lane_e2e::stamp_harness_time(&self.chain.config.params, &mut t.block.header, self.chain.ctx.simulated_time);
        t.block.header.nonce = self.nonce;
        let facts = self.facts(card);
        let bond_facts = facts.bond.as_ref().expect("a genesis card").clone();
        let header = &t.block.header;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let mut attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: self.domain,
            challenge: challenge_v2(self.domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
            class_id: facts.class_id,
            executor_bond: bond.0,
            executor_pubkey: card_pubkey(card),
            operator_id: bond_facts.operator_id,
            artifact_root: facts.artifact_root,
            trace_root: Hash64::default(),
            output_root: Hash64::from_u64_word(0x0070_0000_0000_0000 ^ self.nonce),
            execution_root: Hash64::from_u64_word(0xE7EC_0000_0000_0000 ^ self.nonce),
            pwu: facts.pwu,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        let anchor = execution_anchor_v3(self.domain, pre_pow, facts.class_id, &bond.0, header.nonce);
        let won = (0u64..4_000_000).any(|draw| {
            attempt.trace_root = Hash64::from_u64_word((self.nonce << 20) ^ draw ^ 0x7B00_0000_0000_0000);
            attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
            class_ticket_v3(&attempt, anchor) <= facts.class_target
        });
        assert!(won, "the floor's class lottery is winnable");
        let claim_id = attempt_id_v2(&attempt);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &TestConsensus::palw_v2_registry_keypair(card as u64).signing_key,
            claim_id.as_byte_slice(),
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
            [0x5Bu8; 32],
        )
        .expect("ML-DSA-87 sign")
        .as_ref()
        .to_vec();
        t.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
        t.block.header.finalize();
        (t.block, claim_id)
    }

    /// Insert `block` and report what the node made of it.
    async fn insert(&mut self, block: MutableBlock) -> (BlockHash, BlockStatus) {
        let block = block.to_immutable();
        let hash = block.header.hash;
        self.chain.ctx.consensus.validate_and_insert_block(block).virtual_state_task.await.expect("the block is structurally valid");
        (hash, self.chain.ctx.consensus.block_status(hash))
    }

    /// Card `card`'s attempt, which must become the sink.
    async fn attempt(&mut self, card: usize) -> (BlockHash, Hash64) {
        let (block, claim_id) = self.build_attempt(card);
        let (hash, status) = self.insert(block).await;
        assert_eq!(status, BlockStatus::StatusUTXOValid, "card {card}'s attempt {hash} is a valid chain block");
        assert_eq!(self.chain.sink(), hash, "card {card}'s attempt {hash} is the sink");
        (hash, claim_id)
    }

    async fn beat_to(&mut self, daa: u64) {
        let ttpb = self.ttpb();
        while self.sink_daa() < daa {
            self.chain.heartbeat(ttpb, Vec::new()).await;
        }
    }

    /// Card `card` makes floor claims until its own pre-check holds on the exposure ceiling — the
    /// state a producer at its ceiling is in. Returns the claims, in order.
    async fn fill_the_ceiling(&mut self, card: usize) -> Vec<Hash64> {
        let mut claims = Vec::new();
        while self.ready(card).is_ok() {
            let (_, claim_id) = self.attempt(card).await;
            claims.push(claim_id);
            assert!(claims.len() < 1_000, "a genesis card's ceiling holds a bounded number of floor claims");
        }
        assert_eq!(self.ready(card), Err(PALW_NOT_READY_EXPOSURE_FULL_V2), "card {card} holds on its exposure ceiling");
        claims
    }
}

fn phases(state: &PalwChainStateV2, claims: &[Hash64]) -> std::collections::BTreeMap<String, usize> {
    let mut out = std::collections::BTreeMap::new();
    for claim_id in claims {
        let phase = match state.claim(claim_id).map(|c| &c.phase) {
            None => "absent".to_string(),
            Some(PalwClaimPhaseV2::Voided { reason, .. }) => format!("Voided({reason:?})"),
            Some(phase) => format!("{phase:?}").split([' ', '{', '(']).next().unwrap_or_default().to_string(),
        };
        *out.entry(phase).or_default() += 1;
    }
    out
}

/// **`m0`: one floor claim's ledger on testnet-12 as shipped** — the figures the lane's steady-state
/// arithmetic uses, read off the chain rather than restated.
#[tokio::test]
async fn t12_bind_deadlock_m0_the_ledger_of_one_floor_claim() {
    use kaspa_consensus_core::palw_panel_v2::{
        PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
    };
    use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutput};
    let (config, bundle, _, floats) = t12_at(None);
    let mut f = Fleet::new(None);
    let sp = &bundle.state;
    let vp = f.chain.vp();
    let anchor_delay = bundle.panel.anchor_delay();
    println!(
        "[m0] testnet-12: anchor_delay {anchor_delay}; window_bind {}; window_receipt {}; window_challenge {} (at DAA 0: {}); \
         window_court {}; seats {} / quorum {}; settled-anchor depth {:?}; subsidy at DAA 1 {:.2} MSK, at DAA 129 {:.2} MSK",
        sp.window_bind(),
        sp.window_receipt(),
        sp.window_challenge(),
        sp.window_challenge_at(0),
        sp.window_court(),
        bundle.panel.seat_count(),
        bundle.panel.quorum(),
        config.params.palw_settled_anchor_depth,
        vp.coinbase_manager.calc_block_subsidy(1) as f64 / MSK,
        vp.coinbase_manager.calc_block_subsidy(129) as f64 / MSK,
    );
    let facts = f.facts(0);
    let bond = facts.bond.clone().unwrap();
    println!(
        "[m0] card 0: collateral {:.2} MSK, ceiling {:.2} MSK, per-claim commitment {:.2} MSK → {} floor claims fit an empty bond",
        bond.collateral as f64 / MSK,
        msk(bond.exposure_ceiling),
        msk(bond.claim_exposure),
        bond.exposure_ceiling / bond.claim_exposure.max(1),
    );

    // 1. Accept.
    f.chain.heartbeat(f.ttpb(), Vec::new()).await;
    let before: Vec<u128> = (0..8).map(|c| f.committed(c)).collect();
    let (_, claim_id) = f.attempt(0).await;
    let state = f.state();
    let claim = state.claim(&claim_id).expect("the claim").clone();
    let full = kaspa_consensus_core::palw_state_v2::palw_claim_bond_reservation_v1(sp, &claim).unwrap();
    let escrow = sp.claim_escrow_reservation_v1(claim.accepted_daa, claim.escrowed_reward);
    println!(
        "[m0] 1. accepted at DAA {}: full commitment {:.4} MSK = escrow {:.4} + weight/other {:.4}; card 0 committed +{:.4}",
        claim.accepted_daa,
        msk(full),
        msk(escrow),
        msk(full - escrow),
        msk(f.committed(0) - before[0])
    );

    // 2. Bind: the first attempt block at or past the slot (card 7's), SW-8.
    f.beat_to(claim.accepted_daa + anchor_delay).await;
    assert_eq!(f.state().claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Provisional, "heartbeats never anchor");
    let before: Vec<u128> = (0..8).map(|c| f.committed(c)).collect();
    let (anchor, card7_claim) = f.attempt(7).await;
    let state = f.state();
    let PalwClaimPhaseV2::PanelBound { bound_daa } = state.claim(&claim_id).unwrap().phase else { panic!("bound in its anchor") };
    let panel = state.panel(&claim_id).unwrap().clone();
    let seat_cards: Vec<usize> = panel.seats.iter().map(|s| f.chain.bonds.iter().position(|b| *b == s.bond).unwrap()).collect();
    let duty = state.panel_duty_row_of(&claim_id).map(|row| row.seat_exposure).unwrap_or(0);
    println!(
        "[m0] 2. bound at DAA {bound_daa} in card 7's attempt {anchor}; seats {seat_cards:?}; duty row seat_exposure {:.4} MSK; \
         committed deltas {:?} (card 7 also took its own claim {card7_claim})",
        msk(duty),
        (0..8).map(|c| format!("{:.4}", msk(f.committed(c)) - msk(before[c]))).collect::<Vec<_>>()
    );

    // 3. Licence: all five seats' Valid receipts on a carrier funded by card 0's fee float.
    let before: Vec<u128> = (0..8).map(|c| f.committed(c)).collect();
    let signed_daa = f.chain.ctx.consensus.get_virtual_daa_score();
    let receipts: Vec<PalwSeatReceiptV2> = panel
        .seats
        .iter()
        .zip(&seat_cards)
        // Every seat answers, as the eight live operator seats do: SR-1 releases the escrow at the
        // licence only when the whole panel has served (`palw_rcore_release_record_holds_v1`).
        .map(|(seat, card)| {
            let message = palw_receipt_message_v2(f.domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &TestConsensus::palw_v2_registry_keypair(*card as u64).signing_key,
                message.as_byte_slice(),
                PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                [0x11u8; 32],
            )
            .expect("sign")
            .as_ref()
            .to_vec();
            PalwSeatReceiptV2 { claim: claim_id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: seat.bond, signed_daa, signature }
        })
        .collect();
    let object = f.chain.vp().palw_v2_receipt_quorum_assemble_impl(claim_id, &receipts).expect("a signed quorum assembles");
    let carrier = {
        use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        let payload =
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
        let (float_outpoint, float_entry) = floats[0].clone();
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(float_outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(float_entry.amount - 300_000, card_payout_spk(0))],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        super::t12_round_lane_e2e::sign_spend(&mut tx, float_entry, 0, config.params.storage_mass_parameter);
        tx
    };
    f.chain.heartbeat(f.ttpb(), vec![carrier]).await;
    f.chain.heartbeat(f.ttpb(), Vec::new()).await;
    let state = f.state();
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = state.claim(&claim_id).unwrap().phase else {
        panic!("licensed: {:?}", state.claim(&claim_id).unwrap().phase)
    };
    let bonds = f.chain.bonds.clone();
    let lock = |state: &PalwChainStateV2, card: usize| {
        state.slashable_locks_of(&bonds[card]).find(|((_, c), _)| *c == claim_id).map(|(_, lock)| *lock)
    };
    println!(
        "[m0] 3. licensed at DAA {licensed_daa}; committed deltas {:?}; seat locks {:?}",
        (0..8).map(|c| format!("{:.4}", msk(f.committed(c)) - msk(before[c]))).collect::<Vec<_>>(),
        seat_cards.iter().map(|c| lock(&state, *c).map(|l| (format!("{:.4}", msk(l.amount)), l.expiry_daa))).collect::<Vec<_>>(),
    );

    // 4. Final.
    let before: Vec<u128> = (0..8).map(|c| f.committed(c)).collect();
    f.beat_to(licensed_daa + sp.window_challenge_at(licensed_daa) + 1).await;
    let state = f.state();
    let PalwClaimPhaseV2::Final { final_daa } = state.claim(&claim_id).unwrap().phase else {
        panic!("Final: {:?}", state.claim(&claim_id).unwrap().phase)
    };
    println!(
        "[m0] 4. Final at DAA {final_daa} (accept→Final {} DAA); committed deltas {:?}; card 0 committed now {:.4}; seat locks {:?}",
        final_daa - claim.accepted_daa,
        (0..8).map(|c| format!("{:.4}", msk(f.committed(c)) - msk(before[c]))).collect::<Vec<_>>(),
        msk(f.committed(0)),
        seat_cards
            .iter()
            .map(|c| lock(&state, *c).map(|l| (format!("{:.4}", msk(l.amount)), l.expiry_daa, l.settled_at_final)))
            .collect::<Vec<_>>(),
    );
    assert!(final_daa > bound_daa && bound_daa >= claim.accepted_daa + anchor_delay);
}

/// **`r1`: the deadlock on the released rule.** Card 7 fills its ceiling at one DAA; past the slot its
/// attempt block is disqualified from the chain (admission item 8), so none of its claims binds; the
/// claims hold the whole ceiling until the `BindTimeout` backstop voids every one, unpaid, and only then
/// can card 7 produce again.
#[tokio::test]
async fn t12_bind_deadlock_r1_a_producer_at_its_ceiling_cannot_anchor_its_own_claims() {
    let mut f = Fleet::new(None);
    f.chain.heartbeat(f.ttpb(), Vec::new()).await;
    let started = std::time::Instant::now();
    let claims = f.fill_the_ceiling(7).await;
    let facts = f.facts(7);
    let bond = facts.bond.clone().unwrap();
    let accepted = f.state().claim(&claims[0]).unwrap().accepted_daa;
    println!(
        "[r1] card 7 at its ceiling after {} floor claims at DAA {accepted} ({:?}): committed {:.2} / ceiling {:.2} MSK, one more needs {:.2}",
        claims.len(),
        started.elapsed(),
        msk(bond.committed),
        msk(bond.exposure_ceiling),
        msk(bond.claim_exposure)
    );
    let slot = accepted + f.chain.bundle.panel.anchor_delay();
    f.beat_to(slot).await;
    assert_eq!(phases(&f.state(), &claims).get("Provisional"), Some(&claims.len()), "past the slot, nothing has anchored");

    // Card 7's attempt at the slot: the only producer, and at its ceiling.
    let sink = f.chain.sink();
    let (block, _) = f.build_attempt(7);
    let (hash, status) = f.insert(block).await;
    assert_eq!(status, BlockStatus::StatusDisqualifiedFromChain, "the at-ceiling attempt is disqualified (admission item 8)");
    assert_eq!(f.chain.sink(), sink, "the chain does not take it");
    println!("[r1] card 7's attempt {hash} at DAA {} past the slot: {status:?}", f.sink_daa() + 1);

    // Only heartbeats can follow: through the whole bind window nothing binds.
    let deadline = f.state().claim(&claims[0]).unwrap().bind_base_daa() + f.chain.bundle.state.window_bind();
    f.beat_to(deadline).await;
    let state = f.state();
    assert_eq!(phases(&state, &claims).get("Provisional"), Some(&claims.len()), "at the deadline still nothing has bound");
    assert!(f.ready(7).is_err(), "and card 7 is still held");
    f.beat_to(deadline + 1).await;
    let state = f.state();
    let voided = claims
        .iter()
        .filter(|c| {
            matches!(state.claim(c).map(|c| &c.phase), Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }))
                || state.claim(c).is_none()
        })
        .count();
    println!(
        "[r1] DAA {}: {:?}; card 7 committed {:.2} MSK, slashed {}; ready {:?}",
        f.sink_daa(),
        phases(&state, &claims),
        msk(f.committed(7)),
        state.bond(&f.chain.bonds[7]).unwrap().slashed,
        f.ready(7)
    );
    assert_eq!(voided, claims.len(), "the backstop voids every one BindTimeout");
    assert_eq!(state.bond(&f.chain.bonds[7]).unwrap().slashed, 0, "without forfeit (S0)");
    assert_eq!(f.ready(7), Ok(()), "and only then may card 7 produce again");
}

/// **`f1`: the binder, across its fence.** The same scenario as `r1` with `palw_anchor_at_ceiling` at
/// `3 × anchor_delay`: below the fence card 7's at-ceiling attempt is disqualified exactly as released;
/// from the fence the same producer's attempt is the claims' anchor — every one of the 146 binds in it,
/// the attempt itself carries no claim (card 7's commitment does not move), its worker carve is withheld,
/// and a second binder with nothing left to anchor is disqualified as before.
#[tokio::test]
async fn t12_bind_deadlock_f1_past_the_fence_an_at_ceiling_attempt_binds_the_claims_due_at_it() {
    let anchor_delay = t12_at(None).1.panel.anchor_delay();
    let fence = 3 * anchor_delay;
    let mut f = Fleet::new(Some(fence));
    assert!(f.chain.config.params.palw_anchor_at_ceiling_active_at(fence));
    assert!(!f.chain.config.params.palw_anchor_at_ceiling_active_at(fence - 1));
    f.chain.heartbeat(f.ttpb(), Vec::new()).await;
    let claims = f.fill_the_ceiling(7).await;
    let accepted = f.state().claim(&claims[0]).unwrap().accepted_daa;
    let committed_at_ceiling = f.committed(7);
    let slot = accepted + anchor_delay;
    println!("[f1] card 7 at its ceiling after {} claims at DAA {accepted}; slot {slot}; fence {fence}", claims.len());

    // Below the fence: the released rule, at the slot and at the last DAA before the fence.
    for below in [slot, fence - 1] {
        f.beat_to(below).await;
        assert!(!f.facts(7).binder_due, "below the fence the node is told no binder is due (it would be disqualified)");
        let sink = f.chain.sink();
        let (block, _) = f.build_attempt(7);
        let daa = block.header.daa_score;
        assert!(daa < fence, "built below the fence (DAA {daa})");
        let (hash, status) = f.insert(block).await;
        assert_eq!(status, BlockStatus::StatusDisqualifiedFromChain, "below the fence the at-ceiling attempt {hash} is disqualified");
        assert_eq!(f.chain.sink(), sink);
        assert_eq!(phases(&f.state(), &claims).get("Provisional"), Some(&claims.len()));
        println!("[f1] DAA {daa} (below the fence): card 7's attempt {status:?}; all {} claims Provisional", claims.len());
    }

    // From the fence: the binder. The node's own facts say so: the only hold is the ceiling, and a
    // claim is due at the candidate — kaspad's producer mines on exactly this pair.
    f.beat_to(fence).await;
    assert_eq!(f.ready(7), Err(PALW_NOT_READY_EXPOSURE_FULL_V2), "card 7 still holds on its ceiling");
    assert!(f.facts(7).binder_due, "and past the fence a binder is due at the candidate");
    let parent = f.state();
    let (block, binder_attempt) = f.build_attempt(7);
    let daa = block.header.daa_score;
    assert!(daa >= fence, "built at or past the fence (DAA {daa})");
    let (binder, status) = f.insert(block).await;
    assert_eq!(status, BlockStatus::StatusUTXOValid, "past the fence the at-ceiling attempt {binder} is a valid chain block");
    assert_eq!(f.chain.sink(), binder, "and the chain takes it");
    let state = f.state();
    let bound = phases(&state, &claims);
    println!("[f1] DAA {daa} (past the fence): binder {binder}: {bound:?}");
    assert_eq!(bound.get("PanelBound"), Some(&claims.len()), "every claim due at the binder binds in it");
    for claim_id in &claims {
        let panel = state.panel(claim_id).expect("a bound claim has a panel");
        assert_eq!(panel.seats.len(), f.chain.bundle.panel.seat_count() as usize, "a full jury");
        assert!(panel.seats.iter().all(|seat| seat.bond != f.chain.bonds[7]), "the executor never sits on its own panel");
        assert!(matches!(state.claim(claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { bound_daa } if bound_daa == daa));
    }
    assert!(parent.claim(&binder_attempt).is_none() && state.claim(&binder_attempt).is_none(), "the binder's attempt carries no claim");
    assert_eq!(f.committed(7), committed_at_ceiling, "card 7's commitment does not move: the binder reserved nothing");
    let withheld = f.chain.vp().palw_v2_escrow_withheld_at(&state, binder);
    let vp = f.chain.vp();
    let carve = f.chain.bundle.state.worker_carve_at(vp.coinbase_manager.calc_block_subsidy(daa), vp.palw_escrow_carve_at(daa, daa));
    println!("[f1] the binder's worker carve withheld: {:.4} MSK (carve {:.4})", withheld as f64 / MSK, carve as f64 / MSK);
    assert!(withheld > 0 && withheld == carve, "the binder's carve is withheld as a skipped own attempt's is");

    // Nothing left to anchor at this DAA: the node is told so, and a second binder is disqualified.
    assert!(!f.facts(7).binder_due, "every due claim is bound: no binder is due");
    let (block, _) = f.build_attempt(7);
    let (hash, status) = f.insert(block).await;
    assert_eq!(status, BlockStatus::StatusDisqualifiedFromChain, "a binder with nothing due ({hash}) is disqualified");
    assert_eq!(f.chain.sink(), binder);
    // Nor does a heartbeat bind anything; the claims wait for receipts as any bound claim does. The
    // heartbeat's coinbase pays the binder its block's worker share LESS the withheld carve — what any
    // attempt block is paid at once (the carve is what a claim would have escrowed to its Final).
    let child = f.chain.heartbeat(f.ttpb(), Vec::new()).await;
    assert_eq!(phases(&f.state(), &claims).get("PanelBound"), Some(&claims.len()));
    let paid: u64 =
        child.transactions[0].outputs.iter().filter(|out| out.script_public_key == card_payout_spk(7)).map(|out| out.value).sum();
    println!(
        "[f1] the merging heartbeat pays card 7 {:.4} MSK at once for the binder (subsidy {:.4}, carve withheld {:.4})",
        paid as f64 / MSK,
        vp.coinbase_manager.calc_block_subsidy(daa) as f64 / MSK,
        withheld as f64 / MSK
    );
    assert_eq!(paid, 0, "testnet-12 escrows the whole worker share, so a binder (no fees here) is paid nothing: a duty, not an income");
}

/// **`f2`: the binder beside lane A** (`palw_operator_anchor` over cards 0–5, lane F1 and
/// `palw_anchor_at_ceiling`, all armed at DAA 2). Past lane A a chain block anchors a claim iff it IS or
/// MERGES an operator attempt at or past the claim's slot, and it anchors only the slots at or below the
/// latest such attempt (`sw8_anchor_reach`). The binder is kept only for an operator's OWN attempt:
///
/// 1. card 7 (a non-operator) fills its ceiling with claims whose slot is `s7`; card 0 (an operator)
///    fills its own a DAA later;
/// 2. card 1's attempt O, a DAA below `s7`, loses the selected-parent race to a heartbeat sibling and
///    arrives only after that chain has reached `s7`;
/// 3. card 7's at-ceiling attempt N, past `s7`, merges O: under lane A it anchors (it merges an
///    operator attempt), but only up to O's DAA — below every claim due at it. The binder refuses a
///    non-operator (a rule keyed on the block's own DAA would have kept N on the chain binding nothing):
///    N is disqualified as before, nothing binds or voids, and the node told card 7 no binder was due;
/// 4. card 0's at-ceiling attempt B — an operator's own, merging O as well — is a binder: every one of
///    card 7's claims binds in it with a full jury, on B's own execution (O is below the slot, so it
///    seeds nothing), B carries no claim and card 0's commitment does not move; card 0's own claims,
///    whose slot is one DAA later, stay `Provisional`.
#[tokio::test]
async fn t12_bind_deadlock_f2_beside_lane_a_only_an_operators_own_at_ceiling_attempt_binds() {
    use crate::model::stores::ghostdag::GhostdagStoreReader;
    use kaspa_consensus_core::palw_panel_v2::{palw_panel_anchor_execution_v1, palw_panel_draw_seed_v1};
    use kaspa_consensus_core::palw_state_v2::PalwBlockContextV2;
    let h = 2;
    let (config, bundle, premine, floats) = t12_with_lane_a(h);
    let anchor_delay = bundle.panel.anchor_delay();
    let mut f = Fleet::on(config, bundle, premine, floats);
    f.beat_to(h).await;

    // 1. Card 7 (non-operator) at its ceiling, then card 0 (operator) a DAA later.
    let claims_7 = f.fill_the_ceiling(7).await;
    let s7 = f.state().claim(&claims_7[0]).unwrap().bind_base_daa() + anchor_delay;
    f.beat_to(f.sink_daa() + 1).await;
    let claims_0 = f.fill_the_ceiling(0).await;
    let s0 = f.state().claim(&claims_0[0]).unwrap().bind_base_daa() + anchor_delay;
    assert!(s7 < s0, "card 7's claims fall due first ({s7} < {s0})");
    let committed_0 = f.committed(0);
    println!("[f2] card 7 (non-operator): {} claims, slot {s7}; card 0 (operator): {} claims, slot {s0}", claims_7.len(), claims_0.len());

    // 2. O (card 1, an operator with room) just below s7 — within the merge depth of what follows —
    //    displaced by a heartbeat sibling and held back.
    f.beat_to(s7 - 1).await;
    let (o, _) = f.build_attempt(1);
    let o = o.to_immutable();
    let sibling = f.chain.heartbeat(f.ttpb(), Vec::new()).await;
    assert_eq!(o.header.direct_parents(), sibling.header.direct_parents(), "siblings on the same parents");
    assert!(o.header.daa_score < s7, "O stands below card 7's slot ({} < {s7})", o.header.daa_score);
    f.beat_to(s7).await;
    f.chain.ctx.consensus.validate_and_insert_block(o.clone()).virtual_state_task.await.expect("the operator's attempt is valid");
    assert_ne!(f.chain.sink(), o.header.hash, "O arrives late and is not the chain");
    assert_eq!(phases(&f.state(), &claims_7).get("Provisional"), Some(&claims_7.len()), "nothing has anchored card 7's claims");

    // 3. Card 7's at-ceiling attempt N merges O.
    assert_eq!(f.ready(7), Err(PALW_NOT_READY_EXPOSURE_FULL_V2), "card 7 holds on its ceiling");
    assert!(!f.facts(7).binder_due, "past lane A the node tells a non-operator no binder is due, though claims are");
    let sink = f.chain.sink();
    let (n, _) = f.build_attempt(7);
    let n_header = n.header.clone();
    assert!(n_header.daa_score >= s7, "N stands past card 7's slot ({} >= {s7})", n_header.daa_score);
    let (n_hash, status) = f.insert(n).await;
    let merged = f.chain.vp().ghostdag_store.get_data(n_hash).expect("ghostdag data");
    assert!(merged.mergeset_blues.contains(&o.header.hash) || merged.mergeset_reds.contains(&o.header.hash), "N merges O");
    let vp = f.chain.vp();
    let point = PalwBlockContextV2 { block: n_hash, daa_score: n_header.daa_score, blue_score: n_header.blue_score, subsidy: 0 };
    assert_eq!(vp.palw_sw8_anchor_delay_for(&point), Some(anchor_delay), "under lane A N anchors: it merges an operator attempt");
    assert_eq!(vp.palw_anchor_reach_of_v1(n_hash, &n_header), Some(o.header.daa_score), "but only up to O's DAA, below s7");
    println!("[f2] N (card 7, DAA {}) merges O (card 1, DAA {}): reach {}; {status:?}", n_header.daa_score, o.header.daa_score, o.header.daa_score);
    assert_eq!(status, BlockStatus::StatusDisqualifiedFromChain, "a non-operator's at-ceiling attempt is no binder past lane A");
    assert_eq!(f.chain.sink(), sink, "the chain does not take it");
    assert_eq!(phases(&f.state(), &claims_7).get("Provisional"), Some(&claims_7.len()), "nothing bound, nothing voided");

    // 4. Card 0's at-ceiling attempt B — an operator's own — binds every claim due at it.
    assert_eq!(f.ready(0), Err(PALW_NOT_READY_EXPOSURE_FULL_V2), "card 0 holds on its ceiling");
    assert!(f.facts(0).binder_due, "an operator at its ceiling is told a binder is due");
    let (b, b_attempt) = f.build_attempt(0);
    let b_header = b.header.clone();
    let (b_hash, status) = f.insert(b).await;
    assert_eq!(status, BlockStatus::StatusUTXOValid, "the operator's at-ceiling attempt {b_hash} is a binder");
    assert_eq!(f.chain.sink(), b_hash);
    let merged = f.chain.vp().ghostdag_store.get_data(b_hash).expect("ghostdag data");
    let merges_o = merged.mergeset_blues.contains(&o.header.hash) || merged.mergeset_reds.contains(&o.header.hash);
    let state = f.state();
    let bound = phases(&state, &claims_7);
    println!("[f2] B (card 0, DAA {}, merges O: {merges_o}): card 7's claims {bound:?}; card 0's {:?}", b_header.daa_score, phases(&state, &claims_0));
    assert_eq!(bound.get("PanelBound"), Some(&claims_7.len()), "every claim due at the operator's binder binds in it");
    let network = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        f.chain.config.params.net.to_string().as_bytes(),
        Some(f.chain.config.params.genesis.hash),
    );
    let execution = palw_panel_anchor_execution_v1(network, &b_header).expect("an attempt's execution");
    for claim_id in &claims_7 {
        let panel = state.panel(claim_id).expect("a bound claim has a panel");
        assert_eq!(panel.seats.len(), f.chain.bundle.panel.seat_count() as usize, "a full jury");
        assert_eq!(panel.anchor, palw_panel_draw_seed_v1(&execution, claim_id), "on the binder's own execution (lane F1)");
        assert!(matches!(state.claim(claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { bound_daa } if bound_daa == b_header.daa_score));
    }
    if b_header.daa_score < s0 {
        assert_eq!(phases(&state, &claims_0).get("Provisional"), Some(&claims_0.len()), "card 0's own claims are not due yet");
    }
    assert!(state.claim(&b_attempt).is_none(), "the binder's attempt carries no claim");
    // Card 0 is not the executor of card 7's claims, so the draw may seat it in what room its ceiling
    // leaves (less than one claim's): its commitment moves by those seat duties alone — the binder
    // reserved nothing of its own.
    let card_0 = f.chain.bonds[0];
    let seat_duties: u128 = claims_7
        .iter()
        .filter(|c| state.panel(c).is_some_and(|panel| panel.seats.iter().any(|seat| seat.bond == card_0)))
        .map(|c| state.panel_duty_row_of(c).map_or(0, |row| row.seat_exposure))
        .sum();
    println!("[f2] card 0 committed {:.4} → {:.4} MSK (seat duties {:.4})", msk(committed_0), msk(f.committed(0)), msk(seat_duties));
    assert_eq!(f.committed(0), committed_0 + seat_duties, "card 0's commitment moves by its seat duties only: the binder reserved nothing");
    assert!(!f.facts(7).binder_due, "and card 7 is still told no binder is due");
}
