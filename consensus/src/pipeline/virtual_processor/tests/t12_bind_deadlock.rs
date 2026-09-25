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
//!   due at it, carries no claim of its own (the fold's finding-17 skip) and is paid no worker carve;
//!   a second binder with nothing left to bind is disqualified as before.
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
    let Some(_at) = fence else { return (config, bundle, premine, floats) };
    let params = config.params.clone();
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("testnet-12 with the fence armed is a runnable ruleset");
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    let bundle = bundle.clone();
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
        kaspa_core::log::try_init_logger("warn");
        let (config, bundle, premine, floats) = t12_at(fence);
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
