//! **Lane LG14-A on the real node: the legacy V2 Panel route under G14** — the dispute reservation (tags 154–155), the reserved DA
//! budget and the common filer's localizer, reached through the mempool, the node's own block template and the chain block's fold.
//!
//! **The scenario of every test.** The producer (card 0) and EVERY seat of its claim's panel collude: every seat signs `Valid`. ONE
//! bond registered AFTER genesis through a real `0x4b` carrier — the newcomer, declaring no class capability, so it is never drawn as
//! a seat, never a genesis card, never an operator — prosecutes from public material only: what the node's read API returns
//! (`palw_legacy_dispute_v1`: the claim's roots, the job it recorded, its retention and deadlines), the carriers of the accepted
//! blocks (the producer's answers, authenticated by the fold against the claim's roots before they count), and its own copy of the
//! floor's registered model (ADR-0177: the property is conditional on that copy). It never reads the producer's capture, heap or
//! keys. The producer's answers are built from the producer's own capture — its obligation, not the verifier's material.
//!
//! **The fence is armed WITHOUT its validation.** `palw_legacy_public_filer_v1` is refused on every real height
//! (`Params::validate_palw_legacy_public_filer_v1`), so nothing here runs on a network; the config is the harness's testnet-12 with the
//! fence and its bundle mirror set directly (as `g14_kernel_route_e2e` arms its own fence).
//!
//! **Harness differences, none of them a rule**: testnet-12 as launched with harness keys on the eight genesis cards
//! (`t12_round_lane_e2e`); the main premine output re-addressed to the newcomer's harness key (the genesis commitment recomputed, as
//! `t12_seat_maturity_fence` does); proof of work skipped; `palw_reorg_strict_economic_win` armed so a heavier branch wins a reorg
//! deterministically. The lying attempt is a real floor execution with one step tile moved (`execute_with_injected_fault`), whose
//! class lottery is won the way a producer wins it: another nonce bucket, another job, another inference.
use super::OnetimeTxSelector;
use super::g14_registration_e2e::{arrive, chain_blocks, root_at};
use super::t12_round_lane_e2e::{
    T12Chain, card_payout_spk, sign_spend, stamp_harness_time, t12_genesis_chain_on, t12_reopened_chain, t12_with_harness_cards,
};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::premine::{MAIN_PREMINE_INDEX, PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI, premine_outpoint_for};
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::muhash::MuHashExtensions;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
    challenge_v2, class_ticket_v3, execution_anchor_v3, palw_attempt_job_v1,
};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1};
use kaspa_consensus_core::palw_da_rcore_v1::{
    PalwDaAnswerV1, PalwDaUnitV1, palw_da_answer_object_v1, palw_da_held_answer_v1, palw_da_held_disclosure_from_capture_v1,
    palw_da_step_leaf_is_fused_v1,
};
use kaspa_consensus_core::palw_held_da_v1::{PalwHeldDisclosureV1, PalwHeldMissingV1};
use kaspa_consensus_core::palw_legacy_public_filer_v1::{
    PALW_DISPUTE_RESERVATION_VERSION_V1, PalwDisputeReservationV1, PalwFilerActionV1, PalwFilerClaimFactsV1, PalwFilerPhaseV1,
    PalwFilerRoleV1, PalwLegacyBisectV1, PalwLegacyProbeV1, palw_dispute_released_object_v1, palw_dispute_reserved_object_v1,
    palw_fraud_filer_demand_object_v1, palw_fraud_filer_facts_v1, palw_fraud_filer_learn_v1, palw_fraud_filer_next_v1,
    palw_fraud_filer_reservation_v1,
};
use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
};
use kaspa_consensus_core::palw_state_v2::{
    PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT, PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT, PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT,
    PalwBondKeyV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwLegacyDisputeViewV1, palw_accuser_exposure_v1,
    palw_bond_registration_message_v2, palw_bond_text_v1, palw_da_accusation_message_v2, palw_da_event_index_v1,
    palw_operator_possession_message_v1,
};
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use kaspa_consensus_core::tx::{Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use kaspa_muhash::MuHash;
use misaka_palw_base0::backend::Base0Backend;
use kaspa_consensus_core::palw_ruling_fences_v1::{
    PalwClaimHardDeadlineFenceV1, palw_claim_deadlines_v1, palw_claim_effective_deadline_v1, palw_claim_hard_deadline_of_v1,
    palw_claim_pursuit_may_start_v1, palw_x3_hard_deadline_at_v1,
};
use std::collections::BTreeMap;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// Card 0 produces every claim; the newcomer is registry row 8, the first row no genesis card uses.
const EXECUTOR: usize = 0;
const NEWCOMER: usize = 8;
/// The card whose attempt at the anchor slot binds the claim's panel (it is never the producer).
const BINDER: usize = 7;
const FEE: u64 = 5_000_000;
/// The registration carrier's collateral output.
const COLLATERAL_INDEX: u32 = 1;

// ---- the network -----------------------------------------------------------------------------------------------------------------

/// testnet-12 as launched, harness cards, the main premine on the newcomer's key; the public filer armed from genesis (on its mirror,
/// WITHOUT its validation) when `armed`.
fn lg14a_config(armed: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine, (TransactionOutpoint, UtxoEntry)) {
    lg14a_config_with(armed, false, false)
}

/// `d2` / `x3`: the 2026-10-12 ruling fences armed from genesis on their mirror, WITHOUT their validation. D2 is `palw_court_model_scope_v1`.
fn lg14a_config_with(armed: bool, d2: bool, x3: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine, (TransactionOutpoint, UtxoEntry)) {
    let (config, _, mut premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    let main = premine_outpoint_for(params.net, MAIN_PREMINE_INDEX);
    let funding = {
        let (outpoint, entry) = premine.iter_mut().find(|(o, _)| *o == main).expect("testnet-12 mints a main premine output");
        entry.script_public_key = card_payout_spk(NEWCOMER);
        assert!(entry.amount > 2 * PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI, "the main output funds a genesis-card collateral");
        (*outpoint, entry.clone())
    };
    let mut multiset = MuHash::new();
    for (outpoint, entry) in &premine {
        multiset.add_utxo(outpoint, entry);
    }
    params.genesis.utxo_commitment = multiset.finalize();
    params.genesis.hash = kaspa_consensus_core::header::Header::from(&params.genesis).hash;
    if armed {
        params.palw_legacy_public_filer_v1 = Some(ForkActivation::new(0));
        params.sync_palw_legacy_public_filer_v1();
        assert!(params.validate_palw_v2().is_err(), "the real validation still refuses the fence; only this harness bypasses it");
    }
    if d2 || x3 {
        use kaspa_consensus_core::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1;
        if d2 {
            params.palw_court_model_scope_v1 = Some(ForkActivation::new(0));
        }
        if x3 {
            // The composition (ADR-0188): resolve = B_disclose + B_court + Delta, i.e. the pursuit budget. The probe's own resolve (200)
            // is shorter than that budget, so no pursuit is admitted at all; the test sets resolve to the composed value.
            let mut fence = PalwClaimHardDeadlineFenceV1::unapproved_probe_at(ForkActivation::new(0));
            let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { unreachable!("testnet-12 is ConsensusV2") };
            fence.policy.resolve_daa = kaspa_consensus_core::palw_ruling_fences_v1::palw_claim_pursuit_budget_v1(&bundle.state, &fence.policy);
            params.palw_claim_hard_deadline_v1 = Some(fence);
            // X3 governs the held-DA pursuit too (REQ-X3-5's open session is a reserved held session): arm LG14-B with it.
            if armed {
                params.palw_legacy_held_da_v2 = Some(ForkActivation::new(0));
            }
        }
        params.sync_palw_2026_10_12_ruling_fences_v1();
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { unreachable!("testnet-12 is ConsensusV2") };
        assert_eq!(bundle.state.claim_hard_deadline_v1().is_some(), x3, "the X3 mirror");
        assert_eq!(bundle.state.court_model_scope_from_daa().is_some(), d2, "the D2 mirror");
    }
    params.palw_reorg_strict_economic_win = Some(ForkActivation::new(0));
    params.skip_proof_of_work = true;
    let config = Config::new(params);
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("testnet-12 is ConsensusV2") };
    let bundle = bundle.clone();
    assert_eq!(bundle.state.legacy_public_filer_active_at(0), armed, "the mirror the fold reads");
    (config, bundle, premine, floats, funding)
}

fn sign(card: usize, message: &[u8], context: &[u8], rnd: u8) -> Vec<u8> {
    let key = TestConsensus::palw_v2_registry_keypair(card as u64);
    libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [rnd; 32]).expect("ML-DSA-87 signs").as_ref().to_vec()
}

fn pubkey(card: usize) -> Vec<u8> {
    TestConsensus::palw_v2_registry_keypair(card as u64).verification_key.as_ref().to_vec()
}

/// What a producer committed, and what it keeps to answer from (ITS obligation — the verifier never reads it).
#[derive(Clone)]
struct Produced {
    claim_id: Hash64,
    /// The producer's retained capture and prompt.
    material: Vec<u8>,
    prompt: Vec<u32>,
    roots: PalwClaimRootsV1,
    /// The step the lie is at (`None` for an honest run).
    lie_leaf: Option<u64>,
}

/// One network: a node, its funding, the newcomer.
struct Lg {
    chain: T12Chain,
    config: Config,
    bundle: PalwConsensusParamsV2,
    premine: Premine,
    floats: Premine,
    /// Each actor's next funding output (a change chain), cards 0..8 and the newcomer.
    funding: BTreeMap<usize, (TransactionOutpoint, UtxoEntry)>,
    domain: Hash64,
    backend: Base0Backend,
    newcomer: Option<PalwBondKeyV2>,
    nonce: u64,
    rnd: u8,
    _keep: Vec<Box<dyn std::any::Any>>,
}

fn backend_of(config: &Config, bundle: &PalwConsensusParamsV2) -> Base0Backend {
    use misaka_palw_base0::classes::resolve_class_v1;
    let artifact_root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root");
    Base0Backend::new(resolve_class_v1(&bundle.court, bundle.base_class_id, artifact_root, &[]).expect("the floor resolves"))
        .with_step_ladder_cap(bundle.court.max_step_leaf_count())
        .with_prompt_ids_form(config.params.palw_prompt_ids_form_v1())
}

impl Lg {
    fn new(armed: bool) -> Lg {
        Lg::over(armed, TestConsensus::new)
    }

    fn over(armed: bool, make: impl FnOnce(&Config) -> TestConsensus) -> Lg {
        Lg::over_with(armed, false, false, make)
    }

    fn over_with(armed: bool, d2: bool, x3: bool, make: impl FnOnce(&Config) -> TestConsensus) -> Lg {
        let (config, bundle, premine, floats, newcomer_funding) = lg14a_config_with(armed, d2, x3);
        let chain = t12_genesis_chain_on(make(&config), &config, &bundle, &premine, &floats);
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        let mut funding: BTreeMap<usize, (TransactionOutpoint, UtxoEntry)> = floats.iter().cloned().enumerate().collect();
        funding.insert(NEWCOMER, newcomer_funding);
        let backend = backend_of(&config, &bundle);
        Lg { chain, config, bundle, premine, floats, funding, domain, backend, newcomer: None, nonce: 0, rnd: 0, _keep: Vec::new() }
    }

    fn ttpb(&self) -> u64 {
        self.config.params.target_time_per_block()
    }

    fn daa(&self) -> u64 {
        self.chain.daa_of(self.chain.sink())
    }

    fn bond(&self, card: usize) -> PalwBondKeyV2 {
        if card == NEWCOMER { self.newcomer.expect("the newcomer is registered") } else { self.chain.bonds[card] }
    }

    fn card_of(&self, bond: &PalwBondKeyV2) -> usize {
        if Some(*bond) == self.newcomer {
            return NEWCOMER;
        }
        self.chain.bonds.iter().position(|b| b == bond).expect("a known bond")
    }

    fn signed(&mut self, card: usize, message: &[u8], context: &[u8]) -> Vec<u8> {
        self.rnd = self.rnd.wrapping_add(1);
        sign(card, message, context, self.rnd)
    }

    fn view(&self, claim: Hash64) -> PalwLegacyDisputeViewV1 {
        self.chain.ctx.consensus.palw_legacy_dispute_v1(claim).expect("the node's read API serves the claim")
    }

    fn collateral(&self, card: usize) -> u64 {
        self.chain.tip_state().1.bond(&self.bond(card)).expect("the bond").collateral
    }

    fn exposure(&self, card: usize) -> u128 {
        palw_accuser_exposure_v1(&self.chain.tip_state().1, &self.bond(card))
    }

    /// A `0x4b` carrier for `object`, funded by `card`'s change chain (which it advances).
    fn carrier(&mut self, card: usize, object: &Obj) -> Transaction {
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() })
            .expect("serializes");
        let (outpoint, entry) = self.funding[&card].clone();
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(entry.amount - FEE, card_payout_spk(card))],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        sign_spend(&mut tx, entry, card, self.config.params.storage_mass_parameter);
        self.funding.insert(
            card,
            (TransactionOutpoint::new(tx.id(), 0), UtxoEntry::new(tx.outputs[0].value, card_payout_spk(card), 0, false)),
        );
        tx
    }

    fn mempool(&self, tx: &Transaction) -> Result<(), kaspa_consensus_core::errors::tx::TxRuleError> {
        self.chain
            .vp()
            .validate_mempool_transaction(&mut kaspa_consensus_core::tx::MutableTransaction::from_tx(tx.clone()), &Default::default())
    }

    /// **Send objects**: each through the mempool, into the node's own template, folded by the block after the one that carries it
    /// (`g14_kernel_route_e2e`'s waves: an actor's carriers chain, the template takes only confirmed outputs). Returns the carrying
    /// blocks and the folding block.
    async fn send(&mut self, items: Vec<(usize, Obj)>) -> (Vec<Block>, Block) {
        let mut waves: Vec<Vec<Transaction>> = Vec::new();
        let mut seen: BTreeMap<usize, usize> = BTreeMap::new();
        for (card, object) in &items {
            let tx = self.carrier(*card, object);
            let wave = *seen.entry(*card).and_modify(|n| *n += 1).or_insert(0);
            if waves.len() <= wave {
                waves.resize(wave + 1, Vec::new());
            }
            waves[wave].push(tx);
        }
        let ttpb = self.ttpb();
        let mut carrying = Vec::new();
        for txs in &waves {
            for tx in txs {
                self.mempool(tx).unwrap_or_else(|e| panic!("the mempool takes the carrier: {e}"));
            }
            let block = self.chain.heartbeat(ttpb, txs.clone()).await;
            for tx in txs {
                assert!(block.transactions.iter().any(|t| t.id() == tx.id()), "the node's template carries the carrier");
            }
            carrying.push(block);
        }
        let folding = self.chain.heartbeat(ttpb, Vec::new()).await;
        (carrying, folding)
    }

    async fn beat(&mut self, n: u64) {
        let ttpb = self.ttpb();
        for _ in 0..n {
            self.chain.heartbeat(ttpb, Vec::new()).await;
        }
    }

    async fn beat_to(&mut self, daa: u64) {
        let ttpb = self.ttpb();
        while self.daa() < daa {
            self.chain.heartbeat(ttpb, Vec::new()).await;
        }
    }

    /// **The newcomer registers through a real `0x4b` carrier** (as `--palw-register-bond` carries one): the collateral at output 1,
    /// paid to the payout the registration names; both signatures under this chain's domain; NO class capability — a verifier, never
    /// drawn as a seat.
    async fn register_newcomer(&mut self) -> PalwBondKeyV2 {
        let signed_bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::default(), COLLATERAL_INDEX));
        let collateral = PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
        let key = pubkey(NEWCOMER);
        let payout = Hash64::from_bytes(kaspa_hashes::blake2b_512_address_payload(&key).as_bytes());
        let classes = std::collections::BTreeSet::new();
        let message = palw_bond_registration_message_v2(self.domain, &signed_bond, &key, &key, collateral, &payout, &classes);
        let possession = palw_operator_possession_message_v1(self.domain, &signed_bond, &key, &key);
        let mut signature = self.signed(NEWCOMER, message.as_byte_slice(), PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT);
        signature.extend(self.signed(NEWCOMER, possession.as_byte_slice(), PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT));
        let object = Obj::BondRegistered {
            bond: signed_bond,
            pubkey: key.clone(),
            operator_pubkey: key,
            collateral,
            payout_payload: payout,
            capable_classes: classes,
            signature,
        };
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
        let (outpoint, entry) = self.funding[&NEWCOMER].clone();
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![
                TransactionOutput::new(entry.amount - collateral - 1_000_000_000, card_payout_spk(NEWCOMER)),
                TransactionOutput::new(collateral, p2pkh_mldsa87_spk(payout.as_byte_slice())),
            ],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        sign_spend(&mut tx, entry, NEWCOMER, self.config.params.storage_mass_parameter);
        self.mempool(&tx).unwrap_or_else(|e| panic!("the mempool takes the registration: {e}"));
        let ttpb = self.ttpb();
        self.chain.heartbeat(ttpb, vec![tx.clone()]).await;
        self.chain.heartbeat(ttpb, Vec::new()).await;
        self.funding.insert(
            NEWCOMER,
            (TransactionOutpoint::new(tx.id(), 0), UtxoEntry::new(tx.outputs[0].value, card_payout_spk(NEWCOMER), 0, false)),
        );
        let bond = PalwBondKeyV2(TransactionOutpoint::new(tx.id(), COLLATERAL_INDEX));
        let (_, state) = self.chain.tip_state();
        let record = state.bond(&bond).expect("the chain registered the newcomer");
        assert!(record.registered_daa > 0, "registered after genesis");
        assert!(record.capable_classes.is_empty(), "a verifier, declaring no class");
        assert!(!self.chain.bonds.contains(&bond), "not a genesis card");
        self.newcomer = Some(bond);
        bond
    }

    /// **Card `card`'s attempt over a REAL floor execution** — honest, or with one step tile moved at a leaf the court can try in
    /// one move (non-fused, in the capture) — whose class lottery it wins the way a producer does: another nonce bucket is another
    /// anchor, another job, another inference. Inserted through `validate_and_insert_block` (header, lottery, admission, fold).
    async fn real_attempt(&mut self, card: usize, lie: bool) -> Produced {
        use kaspa_consensus_core::hashing::header::pre_pow_hash_64;
        use misaka_palw_base0::produce::base0_material_decode_v1;
        let bond = self.chain.bonds[card];
        let prefill_draw = true;
        self.chain.ctx.simulated_time += self.ttpb();
        let mut template = self
            .chain
            .ctx
            .consensus
            .build_block_template(
                kaspa_consensus_core::coinbase::MinerData::new(card_payout_spk(card), vec![]),
                Box::new(OnetimeTxSelector::new(Vec::new())),
                kaspa_consensus_core::block::TemplateBuildMode::Standard,
            )
            .expect("a template");
        stamp_harness_time(&self.config.params, &mut template.block.header, self.chain.ctx.simulated_time);
        for attempt in 0u64..4_000 {
            // Another nonce bucket: another anchor, another job, another inference (ADR-0072).
            self.nonce += 1;
            template.block.header.nonce =
                (0x47_0000 + self.nonce) << kaspa_consensus_core::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2;
            let facts =
                self.chain.ctx.consensus.palw_producer_facts_v2(self.bundle.base_class_id, Some(bond.0)).expect("the floor's facts");
            let header = &template.block.header;
            assert_eq!(self.config.params.palw_prefill_draw_active_at(header.daa_score), prefill_draw, "testnet-12 draws one forward");
            let pre_pow = pre_pow_hash_64(header);
            let anchor = execution_anchor_v3(self.domain, pre_pow, facts.class_id, &bond.0, header.nonce);
            let (canonical, prompt) = self.backend.job_for_anchor(anchor).expect("the floor implies a job");
            let job = palw_attempt_job_v1(canonical, prefill_draw);
            let honest = self.backend.execute(&job, &prompt).expect("the floor runs its job");
            let (run, lie_leaf): (PalwExecutionOutcomeV1, Option<u64>) = if lie {
                let (binding, tiles, ..) = base0_material_decode_v1(&honest.material).expect("the capture decodes");
                let held: std::collections::BTreeSet<u64> = tiles.iter().map(|(i, _)| *i).collect();
                let n = binding.step_leaf_count;
                let leaf = (n / 2..n)
                    .find(|leaf| {
                        held.contains(leaf)
                            && kaspa_consensus_core::palw_step::canonical_step_coordinates(
                                &binding.shape_profile,
                                &binding.job_context,
                                *leaf,
                            )
                            .is_some()
                            && !palw_da_step_leaf_is_fused_v1(&binding, *leaf)
                    })
                    .expect("an openable, non-fused leaf");
                (self.backend.execute_with_injected_fault(&job, &prompt, leaf).expect("the drill's fault runs"), Some(leaf))
            } else {
                (honest, None)
            };
            let bond_facts = facts.bond.as_ref().expect("a registered bond");
            let unsigned = PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain: self.domain,
                challenge: challenge_v2(self.domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
                class_id: facts.class_id,
                executor_bond: bond.0,
                executor_pubkey: pubkey(card),
                operator_id: bond_facts.operator_id,
                artifact_root: facts.artifact_root,
                trace_root: run.trace_root,
                output_root: run.output_root,
                execution_root: run.execution_root,
                pwu: facts.pwu,
                trace_manifest_root: run.trace_manifest_root,
                trace_chunk_count: run.trace_chunk_count,
                trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
            };
            if class_ticket_v3(&unsigned, anchor) > facts.class_target {
                continue;
            }
            let claim_id = attempt_id_v2(&unsigned);
            let signature = self.signed(card, claim_id.as_byte_slice(), PALW_ATTEMPT_V2_MLDSA87_CONTEXT);
            let mut mined = template.block.clone();
            mined.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt: unsigned, signature }.encode_wire();
            mined.header.finalize();
            let block = mined.to_immutable();
            let hash = block.header.hash;
            self.chain
                .ctx
                .consensus
                .validate_and_insert_block(block.clone())
                .virtual_state_task
                .await
                .unwrap_or_else(|e| panic!("the attempt block {hash} was refused: {e}"));
            assert_eq!(self.chain.sink(), hash, "the attempt block is the sink");
            eprintln!(
                "[lg14a] card {card}'s {} attempt won its lottery on try {attempt}: claim {claim_id}",
                if lie { "lying" } else { "honest" }
            );
            let roots = PalwClaimRootsV1 {
                execution_root: run.execution_root,
                trace_root: run.trace_root,
                anchor,
                attempt_draw: Some(prefill_draw),
                output_root: None,
                job_pin: None,
            };
            let prompt: Vec<u32> = prompt.iter().map(|id| u32::try_from(*id).expect("a u32 id")).collect();
            let (_, state) = self.chain.tip_state();
            let record = state.claim(&claim_id).expect("the chain opened the claim");
            assert_eq!(record.job_identity, anchor, "the claim records the job its block asked for");
            let _ = block;
            return Produced { claim_id, material: run.material, prompt, roots, lie_leaf };
        }
        panic!("the floor's class lottery is winnable");
    }

    /// The chain binds the claim's panel (an attempt at its anchor slot); the newcomer is never a seat.
    async fn bind(&mut self, claim: Hash64) -> Vec<usize> {
        self.chain.attempt_at_the_anchor_slot(claim, BINDER).await;
        let (_, state) = self.chain.tip_state();
        assert!(matches!(state.claim(&claim).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "the anchor block binds it");
        let panel = state.panel(&claim).expect("a bound panel").clone();
        let seats: Vec<usize> = panel.seats.iter().map(|seat| self.card_of(&seat.bond)).collect();
        if let Some(newcomer) = self.newcomer {
            assert!(panel.seats.iter().all(|seat| seat.bond != newcomer), "the newcomer is never a seat");
        }
        assert!(!seats.contains(&EXECUTOR));
        seats
    }

    /// **Every seat colludes**: each signs `Valid`; the node's own assembler licenses on a funded carrier.
    async fn license(&mut self, claim: Hash64, seats: &[usize]) -> u64 {
        let signed_daa = self.chain.ctx.consensus.get_virtual_daa_score();
        let receipts: Vec<PalwSeatReceiptV2> = seats
            .iter()
            .map(|card| {
                let message = palw_receipt_message_v2(self.domain, claim, PalwReceiptVerdictV2::Valid, signed_daa);
                let signature = sign(*card, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT, 0x11);
                PalwSeatReceiptV2 {
                    claim,
                    verdict: PalwReceiptVerdictV2::Valid,
                    seat_bond: self.chain.bonds[*card],
                    signed_daa,
                    signature,
                }
            })
            .collect();
        let object = self.chain.vp().palw_v2_receipt_quorum_assemble_impl(claim, &receipts).expect("a Valid set assembles");
        self.send(vec![(seats[0], object)]).await;
        let (_, state) = self.chain.tip_state();
        match state.claim(&claim).unwrap().phase {
            PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => licensed_daa,
            ref other => panic!("every seat's Valid licenses the claim: {other:?}"),
        }
    }

    /// **The colluding producer answers every unit it owes** (it cannot do otherwise without defaulting): the duty list its node
    /// reads (`palw_disclosure_duties_v1`), each unit opened from ITS capture by the node's builders. `withhold` names units it keeps
    /// silent on. Returns whether anything was sent.
    async fn producer_answers(&mut self, produced: &Produced, withhold: &dyn Fn(&PalwDaUnitV1) -> bool) -> bool {
        let duties = self.chain.ctx.consensus.palw_disclosure_duties_v1(vec![self.chain.bonds[EXECUTOR]]);
        let form = self.config.params.palw_prompt_ids_form_v1();
        let work_leaves = self.view(produced.claim_id).work_leaves;
        let mut objects = Vec::new();
        for duty in duties.duties.iter().filter(|d| d.claim_id == produced.claim_id && !withhold(&d.unit)) {
            let answer = match duty.unit {
                PalwDaUnitV1::Event { row, tile } => PalwDaAnswerV1::Event(
                    self.backend.disclose_trace_event(&produced.material, row, tile).expect("the capture opens the event"),
                ),
                PalwDaUnitV1::Held(missing) => {
                    let (binding, disclosure) = palw_da_held_disclosure_from_capture_v1(
                        &self.backend,
                        &produced.material,
                        &produced.prompt,
                        produced.roots,
                        work_leaves,
                        missing,
                        form,
                        || self.backend.disclose_trace_event(&produced.material, u32::MAX, u8::MAX).map(|d| d.binding().clone()),
                    )
                    .unwrap_or_else(|e| panic!("the producer's capture answers {missing:?}: {e}"));
                    palw_da_held_answer_v1(produced.claim_id, missing, binding, disclosure)
                }
                other => panic!("a unit the legacy floor does not owe: {other:?}"),
            };
            self.rnd = self.rnd.wrapping_add(1);
            let rnd = self.rnd;
            let object = palw_da_answer_object_v1(
                &self.domain,
                produced.claim_id,
                duty.unit,
                answer,
                duty.discloser,
                self.bundle.court.max_close_bytes(),
                |message, context| Some(sign(EXECUTOR, message, context, rnd)),
            )
            .unwrap_or_else(|e| panic!("the node's builder builds the answer: {e}"));
            objects.push((EXECUTOR, object));
        }
        if objects.is_empty() {
            return false;
        }
        self.send(objects).await;
        true
    }

    /// The accepted lifecycle objects of the last `depth` chain blocks — what any node reads off its own chain.
    fn recent_objects(&self, depth: usize) -> Vec<Obj> {
        let depth = depth.max(1);
        let vp = self.chain.vp();
        let genesis = self.config.params.genesis.hash;
        let mut at = self.chain.sink();
        let mut objects = Vec::new();
        for _ in 0..depth {
            if at == genesis {
                break;
            }
            let block = self.chain.ctx.consensus.get_block(at).expect("the node holds its chain");
            for tx in
                block.transactions.iter().filter(|tx| tx.subnetwork_id == kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE)
            {
                if let Ok(payload) = borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload) {
                    objects.push(payload.object);
                }
            }
            at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
        }
        objects
    }
}

// ---- the verifier: the common filer's engine over public reads only -------------------------------------------------------------

/// **The newcomer's filer** — `palw_fraud_filer_next_v1` (the engine every role runs) over what the node's read API and accepted
/// blocks show, plus the verifier's own replay of the claim's job on its own model copy.
struct Verifier {
    card: usize,
    claim: Hash64,
    /// Its own honest run of the claim's job (the verifier's material, never the producer's).
    honest: PalwExecutionOutcomeV1,
    prompt: Vec<u32>,
    roots: PalwClaimRootsV1,
    binding: Option<PalwStepBindingV2>,
    bisect: PalwLegacyBisectV1,
    demands: u32,
}

impl Verifier {
    /// **Discover → Check**: the claim's public facts, the job it recorded, an honest replay on the verifier's own model.
    fn check(lg: &Lg, card: usize, claim: Hash64) -> Verifier {
        let view = lg.view(claim);
        let (canonical, prompt) = lg.backend.job_for_anchor(view.job_identity).expect("the floor implies the recorded job");
        let job = palw_attempt_job_v1(canonical, true);
        let honest = lg.backend.execute(&job, &prompt).expect("the verifier replays the job");
        let roots = PalwClaimRootsV1 {
            execution_root: honest.execution_root,
            trace_root: honest.trace_root,
            anchor: view.job_identity,
            attempt_draw: Some(true),
            output_root: None,
            job_pin: None,
        };
        let prompt = prompt.iter().map(|id| u32::try_from(*id).expect("a u32 id")).collect();
        Verifier { card, claim, honest, prompt, roots, binding: None, bisect: PalwLegacyBisectV1::new(0), demands: 0 }
    }

    fn mismatch(&self, lg: &Lg) -> bool {
        lg.view(self.claim).execution_root != self.honest.execution_root
    }

    /// The engine's facts, by the ONE read the node's filer runs (`palw_fraud_filer_facts_v1`) over the node's read API.
    fn facts(&self, lg: &Lg) -> PalwFilerClaimFactsV1 {
        let view = lg.view(self.claim);
        let bond = lg.bond(self.card);
        let reservable = lg
            .chain
            .ctx
            .consensus
            .palw_legacy_dispute_reservation_check_v1(palw_fraud_filer_reservation_v1(&view, bond))
            .is_some_and(|r| r.is_ok());
        palw_fraud_filer_facts_v1(Some(&view), &bond, reservable, self.binding.is_some())
    }

    /// **The answer to `unit` on chain**, read off the accepted blocks and counted only once the fold marked the unit answered.
    fn answer_on_chain(&self, lg: &Lg, unit: &PalwDaUnitV1) -> Option<PalwDaAnswerV1> {
        if !lg.view(self.claim).answered.contains(unit) {
            return None;
        }
        lg.recent_objects(512).into_iter().find_map(|object| match object {
            Obj::MaterialDisclosedV2 { claim, unit: answered, answer, .. }
                if claim == self.claim
                    && answered == *unit
                    && kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(
                        unit,
                        &answer,
                        &lg.view(self.claim).execution_root,
                    ) =>
            {
                Some(answer)
            }
            _ => None,
        })
    }

    /// The demand object for `probe`, signed by the verifier's bond — the ONE builder the node's filer runs
    /// (`palw_fraud_filer_demand_object_v1`: P2-6's event builder, DA-3's held builder with the fold's stateless checks).
    fn demand(&self, lg: &mut Lg, probe: PalwLegacyProbeV1) -> Obj {
        let accuser = lg.bond(self.card);
        let execution_root = lg.view(self.claim).execution_root;
        lg.rnd = lg.rnd.wrapping_add(1);
        let (card, rnd) = (self.card, lg.rnd);
        palw_fraud_filer_demand_object_v1(
            &lg.domain,
            self.claim,
            &execution_root,
            probe,
            self.binding.as_ref(),
            accuser,
            lg.config.params.palw_prompt_ids_form_at(lg.daa()),
            |message, context| Some(sign(card, message, context, rnd)),
        )
        .expect("the builder takes the demand")
    }

    /// **Read what the last demand disclosed** (only authenticated answers) by the ONE reader the node's filer runs
    /// (`palw_fraud_filer_learn_v1`): the binding, or a range's committed leaf hashes against the verifier's own.
    fn learn(&mut self, lg: &Lg, probe: PalwLegacyProbeV1) -> bool {
        if matches!(probe, PalwLegacyProbeV1::Terminal { .. }) {
            return false;
        }
        let Some(answer) = self.answer_on_chain(lg, &probe.unit()) else { return false };
        let execution_root = lg.view(self.claim).execution_root;
        let (mut binding, mut bisect) = (self.binding.clone(), self.bisect);
        palw_fraud_filer_learn_v1(probe, &answer, &execution_root, &mut binding, &mut bisect, |first, count| {
            Ok(self.own_range(lg, first, count))
        })
        .expect("an authenticated answer reads");
        if let (PalwLegacyProbeV1::Binding { .. }, Some(read)) = (probe, binding.as_ref()) {
            assert_eq!(read.committed_execution_root, execution_root, "authenticated against the claim's root");
        }
        (self.binding, self.bisect) = (binding, bisect);
        true
    }

    /// The verifier's own leaf hashes of `[first, first + count)`, from its own run, by the node's own builder.
    fn own_range(&self, lg: &Lg, first: u64, count: u32) -> Vec<Hash64> {
        let (_, disclosure) = palw_da_held_disclosure_from_capture_v1(
            &lg.backend,
            &self.honest.material,
            &self.prompt,
            self.roots,
            lg.view(self.claim).work_leaves,
            PalwHeldMissingV1::StepRange { first, count },
            lg.config.params.palw_prompt_ids_form_v1(),
            || lg.backend.disclose_trace_event(&self.honest.material, u32::MAX, u8::MAX).map(|d| d.binding().clone()),
        )
        .expect("the verifier's own run opens the range");
        let PalwHeldDisclosureV1::StepRange { opening } = disclosure else { panic!("a range") };
        opening.leaf_hashes
    }

    /// **One step of the engine**: the next move, taken through the chain (and the producer's answers, which are its obligation).
    /// Returns the engine's verdict once the case is over.
    async fn step(&mut self, lg: &mut Lg, produced: &Produced, withhold: &dyn Fn(&PalwDaUnitV1) -> bool) -> Option<PalwFilerPhaseV1> {
        let facts = self.facts(lg);
        let binding = self.binding.clone();
        let bisect = self.bisect;
        let demandable = |leaf: u64| binding.as_ref().is_some_and(|b| !palw_da_step_leaf_is_fused_v1(b, leaf));
        match palw_fraud_filer_next_v1(PalwFilerRoleV1::PublicBond, &facts, self.mismatch(lg), &bisect, demandable) {
            PalwFilerActionV1::Done(phase) => Some(phase),
            PalwFilerActionV1::Wait => {
                // A demand of ours is open: the producer owes the answer.
                if !lg.producer_answers(produced, withhold).await {
                    let bond = lg.bond(self.card);
                    let deadline = lg
                        .view(self.claim)
                        .sessions
                        .iter()
                        .filter(|(accuser, session)| *accuser == bond && session.units.iter().any(withhold))
                        .map(|(_, session)| session.deadline_daa)
                        .min();
                    if let Some(deadline) = deadline {
                        // Mine through the actual deadline. A step cap is not a DAA clock (the harness may need two blocks per DAA).
                        lg.beat_to(deadline + 2).await;
                    } else {
                        lg.beat(1).await;
                    }
                }
                None
            }
            PalwFilerActionV1::Reserve => {
                let view = lg.view(self.claim);
                let bond = lg.bond(self.card);
                let reservation = palw_fraud_filer_reservation_v1(&view, bond);
                let card = self.card;
                lg.rnd = lg.rnd.wrapping_add(1);
                let rnd = lg.rnd;
                let object = palw_dispute_reserved_object_v1(lg.domain, reservation, |m| {
                    sign(card, m, kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1, rnd)
                });
                lg.send(vec![(card, object)]).await;
                assert!(lg.view(self.claim).record.is_some_and(|r| r.live.contains_key(&bond)), "the reservation landed");
                None
            }
            PalwFilerActionV1::Demand(probe) => {
                // **Shared progress (RFC-0014 §7.4)**: a unit the chain already holds the answer to — this verifier's before a
                // restart, or anybody's — is read, never demanded again.
                if self.learn(lg, probe) {
                    return None;
                }
                let object = self.demand(lg, probe);
                lg.send(vec![(self.card, object)]).await;
                self.demands += 1;
                if !matches!(probe, PalwLegacyProbeV1::Terminal { .. }) {
                    // The producer answers (its obligation), then the verifier reads what the chain authenticated.
                    for _ in 0..3 {
                        if self.learn(lg, probe) {
                            break;
                        }
                        if !lg.producer_answers(produced, withhold).await {
                            lg.beat(1).await;
                        }
                    }
                }
                None
            }
            PalwFilerActionV1::HeldRoute { leaf } => {
                panic!("leaf {leaf} is fused: LG14-B's route (the fixture picks a non-fused lie)")
            }
        }
    }

    /// The engine run to its verdict, at most `cap` steps.
    async fn pursue(
        &mut self,
        lg: &mut Lg,
        produced: &Produced,
        withhold: &dyn Fn(&PalwDaUnitV1) -> bool,
        cap: usize,
    ) -> PalwFilerPhaseV1 {
        for _ in 0..cap {
            if let Some(verdict) = self.step(lg, produced, withhold).await {
                return verdict;
            }
        }
        panic!(
            "the pursuit of {} reached no verdict in {cap} steps ({} demands), localizer {:?}, facts {:?}, view {:?}",
            self.claim,
            self.demands,
            self.bisect,
            self.facts(lg),
            lg.view(self.claim)
        );
    }
}

// ---- the tests ------------------------------------------------------------------------------------------------------------------

/// The claim's whole pre-pursuit life: the newcomer registered after genesis, the lying (or honest) attempt, the colluding seats'
/// binding and licence.
async fn licensed_claim(lg: &mut Lg, lie: bool) -> (Produced, Vec<usize>, u64) {
    lg.beat(3).await;
    lg.register_newcomer().await;
    let produced = lg.real_attempt(EXECUTOR, lie).await;
    let seats = lg.bind(produced.claim_id).await;
    let licensed = lg.license(produced.claim_id, &seats).await;
    (produced, seats, licensed)
}

/// **G14 C1 / C5 / C7 / C8, before Final**: a non-seat bond registered after genesis convicts a self-consistent computation lie that
/// every seat signed `Valid`. It reserves before the panel is bound (the bind, receipt and challenge clocks are then held), reads the
/// claim's binding off the producer's answer, compares consecutive authenticated ranges with its own replay, and demands the
/// first divergent leaf — whose disclosure convicts the producer in the fold. The claim never reaches `Final` though its challenge
/// window passes many times over; every deposit and refuted exposure is returned; a node replaying the chain reaches the same roots.
#[tokio::test]
async fn lg14a_a_non_seat_newcomer_convicts_a_computation_lie_before_final() {
    conviction_before_final(false).await
}

/// **D2 (REQ-T6-4) on the node**: the same full collusion, on a claim accepted with D2 (and X3, with its probe policy) armed from
/// genesis: producer and every seat sign `Valid`, and the non-seat convicts from public material only, before Final.
#[tokio::test]
#[ignore = "BLOCKED (ADR-0188 R-D2-NODE): a new claim's held StepLeaf accusation is refused at the mempool (ADR-0177 D2); the replacement route is not wired into node admission"]
async fn d2_a_full_collusion_on_a_new_claim_is_convicted_before_final_on_the_node() {
    conviction_before_final(true).await
}

async fn conviction_before_final(d2: bool) {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::over_with(true, d2, false, TestConsensus::new);
    lg.beat(3).await;
    lg.register_newcomer().await;
    let produced = lg.real_attempt(EXECUTOR, true).await;
    let id = produced.claim_id;
    // Discover → Check: a mismatch from the public read and the verifier's own replay; Reserve, while the claim is still Provisional.
    let newcomer = lg.bond(NEWCOMER);
    let candidates = lg.chain.ctx.consensus.palw_fraud_filer_candidates_v1(newcomer);
    assert!(candidates.iter().any(|c| c.claim_id == id && !c.seat), "the node's filer read offers the claim to the non-seat");
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    assert!(v.mismatch(&lg), "the verifier's own replay does not reproduce the committed root");
    let before = lg.collateral(NEWCOMER);
    assert_eq!(v.step(&mut lg, &produced, &|_| false).await, None, "the engine reserves first");
    assert_eq!(lg.view(id).deadline_daa, None, "held: the claim owes no bind deadline");
    // The public reads RPC 204-206 serve: the claim's dispute view and the reserver's standing.
    let status = lg.chain.ctx.consensus.palw_fraud_filer_status_v1(newcomer).expect("the newcomer's standing");
    assert!(status.active && status.live.iter().any(|row| row.claim_id == id), "op 206 shows the live reservation");
    assert_eq!(lg.chain.ctx.consensus.palw_legacy_disputes_v1(Some(newcomer), 16), vec![id], "op 205 lists it");
    let json = kaspa_consensus_core::palw_state_v2::PalwLegacyDisputeObservationV1::of(&lg.view(id)).to_json();
    assert!(json.contains("\"held\":true") && json.contains(&palw_bond_text_v1(&newcomer)), "op 204's document: {json}");
    let seats = lg.bind(id).await;
    let licensed = lg.license(id, &seats).await;
    assert_eq!(lg.view(id).deadline_daa, None, "held: no Final deadline");
    lg.beat_to(licensed + lg.bundle.state.window_challenge_at(licensed) + 2).await;
    assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the reservation holds Final past its floor");
    let verdict = v.pursue(&mut lg, &produced, &|_| false, 400).await;
    let view = lg.view(id);
    eprintln!(
        "[lg14a] verdict {verdict:?} after {} demands; lie at leaf {:?}; claim {:?}; licensed at {licensed}, now {}",
        v.demands,
        produced.lie_leaf,
        view.phase,
        lg.daa()
    );
    assert_eq!(verdict, PalwFilerPhaseV1::Convicted);
    assert!(view.court_convicted || view.executor_refuted, "a recorded conviction");
    assert_eq!(v.bisect.located(), produced.lie_leaf, "the located leaf is the lie");
    assert!(lg.daa() > licensed + lg.bundle.state.window_challenge_at(licensed) + 1, "past the Final floor, never Final");
    assert!(view.record.is_none(), "every deposit refunded");
    assert_eq!(lg.exposure(NEWCOMER), 0, "every refuted exposure refunded with the conviction");
    assert_eq!(lg.collateral(NEWCOMER), before, "the honest verifier nets zero");
    // A node replaying the chain (IBD) reaches the same roots, block by block.
    let z = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    for block in chain_blocks(&lg.chain, lg.chain.sink()) {
        let hash = block.header.hash;
        arrive(&z, block, "A's block").await;
        assert_eq!(root_at(&z, hash), root_at(&lg.chain, hash), "the replaying node's root at {hash}");
    }
    // RFC-0014 §6.3 on the node started after the claim: ops 204-206, through the ops' own parsers and builders and the RPC's JSON wire
    // form, serve the same public documents as the producer's node.
    use kaspa_rpc_core::convert::palw_legacy as rpc;
    let (fresh_node, main_node) = (z.ctx.consensus.consensus_clone(), lg.chain.ctx.consensus.consensus_clone());
    let wire = |response: &kaspa_rpc_core::GetPalwLegacyDisputeResponse| -> kaspa_rpc_core::GetPalwLegacyDisputeResponse {
        serde_json::from_str(&serde_json::to_string(response).expect("to the wire")).expect("off the wire")
    };
    let ask = kaspa_rpc_core::GetPalwLegacyDisputeRequest { claim_id: id.to_string() };
    let claim = rpc::palw_legacy_dispute_request_v1(&ask).expect("a well-formed id");
    let fresh = wire(&rpc::palw_legacy_dispute_response_v1(fresh_node.as_ref(), claim));
    assert!(fresh.available && fresh.json.contains("Voided"), "op 204 on the fresh node: {}", fresh.json);
    assert_eq!(fresh, wire(&rpc::palw_legacy_dispute_response_v1(main_node.as_ref(), claim)), "one document on both nodes");
    let status_ask = kaspa_rpc_core::GetPalwFraudFilerStatusRequest { bond: palw_bond_text_v1(&newcomer) };
    let bond = rpc::palw_fraud_filer_status_request_v1(&status_ask).expect("a well-formed bond");
    let status = rpc::palw_fraud_filer_status_response_v1(fresh_node.as_ref(), bond);
    assert!(status.available && status.json.contains("\"live\":[]"), "op 206: nothing live after the conviction: {}", status.json);
    assert!(rpc::palw_legacy_dispute_request_v1(&kaspa_rpc_core::GetPalwLegacyDisputeRequest { claim_id: "zz".into() }).is_err());
}

/// **G14 C5, the DA default**: the producer withholds the first divergent leaf (a unit it owes): the reserved session defaults at
/// its deadline — `ProducerWithholding`, the producer charged — while the reservation holds the claim; the newcomer is made whole.
#[tokio::test]
async fn lg14a_a_withheld_unit_defaults() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::new(true);
    let (produced, _, _) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    let lie = produced.lie_leaf.unwrap();
    let withhold = move |unit: &PalwDaUnitV1| *unit == PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf: lie });
    let producer_before = lg.collateral(EXECUTOR);
    let verdict = v.pursue(&mut lg, &produced, &withhold, 2_000).await;
    assert_eq!(verdict, PalwFilerPhaseV1::DaDefault, "silence on an owed unit is the DA default, never a fraud finding");
    assert!(lg.view(id).da_defaulted);
    assert!(lg.collateral(EXECUTOR) < producer_before, "the withholding producer is charged");
    assert_eq!(lg.exposure(NEWCOMER), 0);
}

/// **G14 C5 after Final**: the claim reached `Final` before anyone looked; the newcomer reserves it at the post-`Final` stage
/// (its vesting row unmatured), localizes the same way, and the guilty disclosure reverses the `Final`.
#[tokio::test]
async fn lg14a_a_lie_is_convicted_after_final() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::new(true);
    let (produced, _, licensed) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    lg.beat_to(licensed + lg.bundle.state.window_challenge_at(licensed) + 2).await;
    assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::Final { .. }), "nobody looked: Final");
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    let verdict = v.pursue(&mut lg, &produced, &|_| false, 400).await;
    assert_eq!(verdict, PalwFilerPhaseV1::Convicted, "the Final is reversed by the guilty disclosure");
    assert!(lg.view(id).record.is_none());
    assert_eq!(lg.exposure(NEWCOMER), 0);
}

/// **G14 C5, the dismissal**: an honest claim; the newcomer (mistaken, or malicious) reserves, reads the binding, then finds its own
/// replay agrees and releases. The claim reaches `Final`, its producer uncharged; the newcomer's deposit is held until the claim
/// retires (where it burns).
#[tokio::test]
async fn lg14a_an_honest_claims_challenge_is_dismissed() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::new(true);
    let (produced, _, _) = licensed_claim(&mut lg, false).await;
    let id = produced.claim_id;
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    assert!(!v.mismatch(&lg), "the honest root reproduces");
    // A reservation filed anyway (the engine would not: a challenger that is wrong pays).
    let view = lg.view(id);
    let bond = lg.bond(NEWCOMER);
    let reservation = PalwDisputeReservationV1 {
        version: PALW_DISPUTE_RESERVATION_VERSION_V1,
        claim: id,
        execution_root: view.execution_root,
        trace_root: view.trace_root,
        reserver: bond,
    };
    let object = palw_dispute_reserved_object_v1(lg.domain, reservation, |m| {
        sign(NEWCOMER, m, kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1, 0x77)
    });
    lg.send(vec![(NEWCOMER, object)]).await;
    let deposit = lg.view(id).record.expect("held").live[&bond].deposit;
    // The binding read, demanded by hand (the engine, whose replay agrees, files nothing): answered, so the session is refuted.
    let binding_read = PalwLegacyProbeV1::Binding { row: 0, tile: 0 };
    let object = v.demand(&mut lg, binding_read);
    lg.send(vec![(NEWCOMER, object)]).await;
    for _ in 0..3 {
        if v.learn(&lg, binding_read) {
            break;
        }
        if !lg.producer_answers(&produced, &|_| false).await {
            lg.beat(1).await;
        }
    }
    assert!(v.binding.is_some(), "the binding, read off the producer's authenticated answer");
    assert_eq!(
        palw_fraud_filer_next_v1(PalwFilerRoleV1::PublicBond, &v.facts(&lg), v.mismatch(&lg), &v.bisect, |_| true),
        PalwFilerActionV1::Done(PalwFilerPhaseV1::Honest)
    );
    let producer_before = lg.collateral(EXECUTOR);
    let object = palw_dispute_released_object_v1(lg.domain, id, bond, |m| {
        sign(NEWCOMER, m, kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1, 0x78)
    });
    lg.send(vec![(NEWCOMER, object)]).await;
    let record = lg.view(id).record.expect("the deposit is held");
    assert_eq!(record.dismissed_held, vec![(bond, deposit)]);
    let deadline = lg.view(id).deadline_daa.expect("release rearms the claim's own Final deadline");
    lg.beat_to(deadline + 2).await;
    assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::Final { .. }), "the honest claim reaches Final at its deadline");
    assert_eq!(lg.collateral(EXECUTOR), producer_before, "the honest producer is not charged");
    assert!(lg.exposure(NEWCOMER) >= deposit, "the wrong challenger's deposit is held until retirement");
}

/// **G14 C6, no pre-emption**: a colluding seat (the producer's Sybil) reserves first and does nothing, and a bystander card holds a
/// plain non-seat session open on the claim; the honest newcomer's reservation and its reserved sessions are admitted regardless — on
/// its own budget — and it convicts inside the claim's own hard deadline, which neither of them moved. A direct proof lands over an
/// open court as well (the fold suite, `lg14a_a_direct_proof_is_judged_over_an_open_court`: testnet-12's held regime opens a court on a
/// floor claim only through a fused leaf's dissection).
#[tokio::test]
async fn lg14a_a_sybil_reservation_and_sessions_pre_empt_nothing() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::new(true);
    let (produced, seats, _) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    let sybil = seats[0];
    let view = lg.view(id);
    let reservation = PalwDisputeReservationV1 {
        version: PALW_DISPUTE_RESERVATION_VERSION_V1,
        claim: id,
        execution_root: view.execution_root,
        trace_root: view.trace_root,
        reserver: lg.bond(sybil),
    };
    let object = palw_dispute_reserved_object_v1(lg.domain, reservation, |m| {
        sign(sybil, m, kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1, 0x55)
    });
    lg.send(vec![(sybil, object)]).await;
    let bystander = (1..8).find(|card| !seats.contains(card) && *card != BINDER && *card != EXECUTOR).unwrap_or(BINDER);
    let bystander_bond = lg.bond(bystander);
    let message = palw_da_accusation_message_v2(lg.domain, &id, palw_da_event_index_v1(1, 0), &bystander_bond);
    let signature = sign(bystander, message.as_byte_slice(), PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT, 0x56);
    lg.send(vec![(
        bystander,
        Obj::DefaultAccused { claim: id, missing_event_index: palw_da_event_index_v1(1, 0), accuser: bystander_bond, signature },
    )])
    .await;
    assert!(lg.view(id).record.is_some_and(|r| r.live.contains_key(&lg.bond(sybil))), "the Sybil holds the first reservation");
    let hard = lg.view(id).hard_deadline_daa;
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    let verdict = v.pursue(&mut lg, &produced, &|_| false, 400).await;
    assert_eq!(verdict, PalwFilerPhaseV1::Convicted, "the Sybil's first reservation pre-empts nothing");
    assert!(lg.daa() <= hard, "inside the claim's own hard deadline, which no reservation moved");
}

/// **G14 C8, recovery**: the node is stopped mid-pursuit (a reservation live, sessions refuted, the binding read) and reopened over
/// the same database; the engine re-derives its case from the chain alone and convicts; every delta row survived.
#[tokio::test]
async fn lg14a_the_pursuit_survives_a_node_restart() {
    restart_mid_pursuit(false).await
}

/// The X3 order: the engine reserves while the claim is still Provisional (before its open cutoff), then binds and licenses it.
async fn reserved_then_licensed(lg: &mut Lg, lie: bool) -> (Produced, u64) {
    lg.beat(3).await;
    lg.register_newcomer().await;
    let produced = lg.real_attempt(EXECUTOR, lie).await;
    let id = produced.claim_id;
    let mut v = Verifier::check(&*lg, NEWCOMER, id);
    assert_eq!(v.step(lg, &produced, &|_| false).await, None, "the engine reserves before the bind");
    let seats = lg.bind(id).await;
    let licensed = lg.license(id, &seats).await;
    (produced, licensed)
}

/// **D2 restart identity on the node (2026-10-18)**: the same stop-and-reopen with D2 armed; the pursuit survives and a fresh node
/// replays the same roots.
#[tokio::test]
#[ignore = "BLOCKED (ADR-0188 R-D2-NODE): same as the D2 conviction node test (the held accusation is refused by D2)"]
async fn d2_the_pursuit_survives_a_restart() {
    restart_mid_pursuit(true).await
}

async fn restart_mid_pursuit(d2: bool) {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut lg = Lg::over_with(true, d2, false, |c| TestConsensus::with_db(db.clone(), c, sender));
    lg._keep.push(Box::new(receiver));
    let produced = licensed_claim(&mut lg, true).await.0;
    let id = produced.claim_id;
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    for _ in 0..4 {
        v.step(&mut lg, &produced, &|_| false).await;
    }
    assert!(lg.view(id).record.is_some(), "the reservation is live at the stop");
    let (sink, root) = (lg.chain.sink(), lg.chain.tip_state().1.state_root());
    let deltas: Vec<Hash64> = chain_blocks(&lg.chain, sink).iter().map(|b| root_at(&lg.chain, b.header.hash)).collect();
    // ---- stop, reopen on the same database ----
    let Lg { chain, config, bundle, premine, floats, funding, domain, backend, newcomer, nonce, rnd, _keep } = lg;
    let (simulated_time, heartbeat_nonce) = (chain.ctx.simulated_time, chain.nonce_for_reopen());
    drop(chain);
    let mut resumed = config.clone();
    resumed.process_genesis = false;
    let (sender, receiver) = async_channel::unbounded();
    let second = TestConsensus::with_db(db.clone(), &resumed, sender);
    let chain = t12_reopened_chain(second, &resumed, &bundle, simulated_time, heartbeat_nonce);
    let mut keep = _keep;
    keep.push(Box::new(receiver));
    let mut lg = Lg { chain, config, bundle, premine, floats, funding, domain, backend, newcomer, nonce, rnd, _keep: keep };
    assert_eq!(lg.chain.sink(), sink);
    assert_eq!(lg.chain.tip_state().1.state_root(), root, "the PALW tip, off disk, with the reservation");
    for (b, want) in chain_blocks(&lg.chain, sink).iter().zip(&deltas) {
        assert_eq!(root_at(&lg.chain, b.header.hash), *want, "the delta row of {} survived", b.header.hash);
    }
    // The engine re-derives everything from the chain: a fresh verifier (no local state) carries the case on.
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    let verdict = v.pursue(&mut lg, &produced, &|_| false, 400).await;
    assert_eq!(verdict, PalwFilerPhaseV1::Convicted, "convicted after the restart");
}

/// **G14 C8, reorg**: an admissible shallow fork omits a post-Final reservation; switching back restores its exact rows and hold.
/// Both forks are judged under the shipped strict-economic-win rule, with roots checked against independent replay.
#[tokio::test]
async fn lg14a_a_reorg_undoes_and_returns_the_reservation_exactly() {
    reorg_undoes_the_reservation(false).await
}

/// **D2 reorg identity on the node (2026-10-18)**: the same reorg with D2 armed.
#[tokio::test]
async fn d2_a_reorg_undoes_and_returns_the_reservation_exactly() {
    reorg_undoes_the_reservation(true).await
}

async fn reorg_undoes_the_reservation(d2: bool) {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::over_with(true, d2, false, TestConsensus::new);
    let (produced, _, licensed) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    // Reserve in the post-Final stage. Both branches retain the same Final work, so a shallow GHOSTDAG win may reorg either
    // way. A pre-Final held branch is an economic LOSS against an unheld Final branch and cannot return by heartbeat mining.
    lg.beat_to(licensed + lg.bundle.state.window_challenge_at(licensed) + 3).await;
    assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::Final { .. }));
    let fork = lg.chain.sink();
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    v.step(&mut lg, &produced, &|_| false).await; // the reservation
    assert!(lg.view(id).record.is_some());
    let a_tip_root = lg.chain.tip_state().1.state_root();
    // Z follows A.
    let z = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    for block in chain_blocks(&lg.chain, lg.chain.sink()) {
        arrive(&z, block, "A's block").await;
    }
    assert_eq!(z.tip_state().1.state_root(), a_tip_root);
    // B mines a heavier branch from the fork that never carries the reservation, past the claim's Final floor.
    let mut b = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    let up_to_fork = chain_blocks(&lg.chain, fork);
    let fork_timestamp = up_to_fork.last().unwrap().header.timestamp;
    for block in up_to_fork {
        arrive(&b, block, "a block up to the fork").await;
    }
    b.ctx.simulated_time = fork_timestamp;
    let ttpb = lg.ttpb();
    let a_work = lg.chain.ctx.consensus.get_block(lg.chain.sink()).unwrap().header.blue_work;
    let mut b_blocks = Vec::new();
    while b.ctx.consensus.get_block(b.sink()).unwrap().header.blue_work <= a_work {
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    let shallow = kaspa_consensus_core::palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_DAA_V1;
    assert!(b.daa_of(b.sink()) - b.daa_of(fork) <= shallow, "B must remain inside the real shallow-tie window");
    for block in &b_blocks {
        arrive(&z, block.clone(), "B's block").await;
    }
    assert_eq!(z.sink(), b.sink(), "Z reorgs onto B");
    assert_eq!(z.tip_state().1.state_root(), b.tip_state().1.state_root(), "Z on B: B's own root");
    assert!(z.tip_state().1.legacy_dispute_v1(&id).is_none(), "no reservation on B");
    assert!(matches!(z.tip_state().1.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "unheld on B: Final");
    // A wins the same shallow economic tie by GHOSTDAG blue work; the reservation returns.
    let old_len = chain_blocks(&lg.chain, lg.chain.sink()).len();
    let b_work = b.ctx.consensus.get_block(b.sink()).unwrap().header.blue_work;
    while lg.chain.ctx.consensus.get_block(lg.chain.sink()).unwrap().header.blue_work <= b_work {
        lg.beat(1).await;
    }
    for block in chain_blocks(&lg.chain, lg.chain.sink()).into_iter().skip(old_len) {
        arrive(&z, block, "A's later block").await;
    }
    assert_eq!(z.sink(), lg.chain.sink(), "Z back on A");
    assert_eq!(z.tip_state().1.state_root(), lg.chain.tip_state().1.state_root(), "A's root, with the reservation");
    assert!(z.tip_state().1.legacy_dispute_v1(&id).is_some_and(|r| r.holds()), "held again");
    assert!(matches!(z.tip_state().1.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
}

/// **The unarmed twin (live int-12's rules)**: the same reservation carrier is a valid transaction — the live build tolerates its
/// bytes — and its object is dropped by name (A-2): no record, no hold, nothing charged; the PALW roots are those of the same chain
/// without the carrier, and the claim reaches `Final` at its floor while the non-seat's session is still open (the gap this lane
/// closes: G14C's matrix §3.9, C8).
#[tokio::test]
async fn lg14a_unarmed_the_reservation_is_dropped_by_name_and_the_bystander_is_outrun() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::new(false);
    let (produced, _, licensed) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    let view = lg.view(id);
    let bond = lg.bond(NEWCOMER);
    let reservation = PalwDisputeReservationV1 {
        version: PALW_DISPUTE_RESERVATION_VERSION_V1,
        claim: id,
        execution_root: view.execution_root,
        trace_root: view.trace_root,
        reserver: bond,
    };
    let object = palw_dispute_reserved_object_v1(lg.domain, reservation, |m| {
        sign(NEWCOMER, m, kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1, 0x99)
    });
    let collateral = lg.collateral(NEWCOMER);
    let (carrying, _) = lg.send(vec![(NEWCOMER, object)]).await;
    assert_eq!(carrying.len(), 1, "the carrier rides a block the node accepted");
    assert!(lg.view(id).record.is_none(), "dropped by name: no record");
    assert_eq!(lg.exposure(NEWCOMER), 0, "nothing reserved");
    assert_eq!(lg.collateral(NEWCOMER), collateral);
    // The bystander's non-seat session does not hold the claim (V3S-08): Final at its floor while it is open.
    let v = Verifier::check(&lg, NEWCOMER, id);
    let object = v.demand(&mut lg, PalwLegacyProbeV1::Binding { row: 0, tile: 0 });
    lg.send(vec![(NEWCOMER, object)]).await;
    assert!(lg.view(id).sessions.iter().any(|(accuser, session)| *accuser == bond && !session.accuser_is_seat));
    lg.beat_to(licensed + lg.bundle.state.window_challenge_at(licensed) + 2).await;
    assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::Final { .. }), "outrun: Final under an open non-seat session");
}

// Lane G14-R: the pursuit resumes across a reorg that removes the reservation, a full IBD node mid-descent and a pruned import:
// `lg14a_legacy_filer_e2e/g14r_resume.rs`.
mod g14r_resume;

// Lane g14-close-legacy (2026-10-16): LG14-A + LG14-B + option (b) armed through `Params` that pass the whole validation with test
// arm-readiness records (never a mirror-only bypass): full collusion convicted by an outside bond, held-DA withholding defaulted.
mod g14_close_legacy;

// ---- X3 on the node (REQ-X3-1..5, 2026-10-18): the probe policy, armed from genesis, on claims accepted under it -------------------

/// **REQ-X3-1, 2, 3 on the node**: acceptance fixes the hard deadline at the composed windows; the effective deadline is the minimum
/// of the proposed and the hard one; no later block (no extension, no regrant) moves a claim's deadline.
#[tokio::test]
async fn x3_req_1_2_3_the_hard_deadline_is_fixed_at_acceptance_on_the_node() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::over_with(false, false, true, TestConsensus::new);
    let (produced, _, _) = licensed_claim(&mut lg, false).await;
    let id = produced.claim_id;
    let claim = lg.chain.tip_state().1.claim(&id).expect("the claim is on the tip").clone();
    let policy = lg.bundle.state.claim_hard_deadline_v1().expect("armed").policy;
    let hard = palw_claim_hard_deadline_of_v1(&lg.bundle.state, &claim).expect("X3 governs the claim");
    assert_eq!(hard, palw_x3_hard_deadline_at_v1(&policy, claim.accepted_daa), "REQ-X3-1: fixed at acceptance by the composed windows");
    assert_eq!(palw_claim_effective_deadline_v1(hard + 500, Some(hard)), hard, "REQ-X3-2: min(proposed, hard)");
    assert_eq!(palw_claim_effective_deadline_v1(hard - 1, Some(hard)), hard - 1, "REQ-X3-2: a proposed deadline inside the hard one stands");
    lg.beat(30).await;
    let later = lg.chain.tip_state().1.claim(&id).expect("still on the tip").clone();
    assert_eq!(later.accepted_daa, claim.accepted_daa);
    assert_eq!(palw_claim_hard_deadline_of_v1(&lg.bundle.state, &later), Some(hard), "REQ-X3-3: no extension by later blocks");
}

/// **REQ-X3-4 on the node**: a pursuit starts only at or before the open cutoff and only if `start + B_remaining <= hard`, so the set of
/// admissible starts is an interval from acceptance to a latest start; a node block flow past that start opens no reservation.
#[tokio::test]
async fn x3_req_4_a_pursuit_starts_only_inside_its_budget_on_the_node() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::over_with(true, false, true, TestConsensus::new);
    lg.beat(3).await;
    lg.register_newcomer().await;
    let produced = lg.real_attempt(EXECUTOR, true).await;
    let id = produced.claim_id;
    let claim = lg.chain.tip_state().1.claim(&id).expect("on the tip").clone();
    let deadlines = palw_claim_deadlines_v1(&lg.bundle.state, &claim).expect("X3 governs the claim");
    let admitted: Vec<u64> = (claim.accepted_daa..=deadlines.open_cutoff_daa + 1)
        .filter(|s| palw_claim_pursuit_may_start_v1(&lg.bundle.state, &claim, *s).is_ok())
        .collect();
    let latest = *admitted.last().expect("some start is admitted");
    assert_eq!(admitted, (claim.accepted_daa..=latest).collect::<Vec<_>>(), "admitted starts form an interval");
    assert!(latest <= deadlines.open_cutoff_daa, "never past the open cutoff");
    lg.beat_to(latest + 1).await;
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    v.step(&mut lg, &produced, &|_| false).await;
    assert!(lg.view(id).record.is_none(), "REQ-X3-4 on the node: no reservation opens past the admitted start");
}

/// **REQ-X3-5 on the node**: an unresolved claim whose DA session is OPEN (the session holds the claim's Final under X3) is not Final
/// at its hard deadline; reaching the deadline alone never Finals it. (Test premise corrected 2026-10-19, ADR-0188 §3.1.2: a plain
/// reservation without a session does not hold under X3 — see `x3_req_5b_a_reservation_without_a_session_finals_at_its_effective_deadline`.)
#[tokio::test]
async fn x3_req_5_reaching_the_deadline_alone_never_finals_an_unresolved_claim_on_the_node() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::over_with(true, false, true, TestConsensus::new);
    let (produced, licensed) = reserved_then_licensed(&mut lg, true).await;
    let id = produced.claim_id;
    let newcomer = lg.bond(NEWCOMER);
    // The reserved outsider reads the binding (answered), then demands the descent's first held unit; the producer withholds it, so
    // a reserved DA session stays open on the claim (the same pursuit as `legacy_v2_attempt_held_da_withholding_ends_in_da_default`).
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    let withhold = |unit: &PalwDaUnitV1| matches!(unit, PalwDaUnitV1::LegacyHeldV2(_));
    for _ in 0..8 {
        if v.binding.is_some() {
            break;
        }
        v.step(&mut lg, &produced, &withhold).await;
    }
    let binding = v.binding.clone().expect("the binding, read off the producer's authenticated answer");
    let height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(binding.step_leaf_count);
    let unit = kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2::StepNode { level: height, index: 0 };
    let object = v.demand(&mut lg, PalwLegacyProbeV1::HeldNode { unit });
    lg.send(vec![(NEWCOMER, object)]).await;
    assert!(
        lg.view(id).sessions.iter().any(|(accuser, session)| *accuser == newcomer && session.units.contains(&PalwDaUnitV1::LegacyHeldV2(unit))),
        "the reserved held session is open"
    );
    let claim = lg.chain.tip_state().1.claim(&id).expect("on the tip").clone();
    let hard = palw_claim_hard_deadline_of_v1(&lg.bundle.state, &claim).expect("X3 governs the claim");
    assert!(hard > licensed, "the hard deadline lies past the license");
    // Walk past the hard deadline one beat at a time: while the session is open the claim is never Final; the withheld unit ends in
    // the objective DA default (Voided), never in a deadline-made Final.
    let mut daa = lg.daa();
    while daa <= hard + 2 {
        let phase = lg.view(id).phase.clone();
        assert!(!matches!(phase, PalwClaimPhaseV2::Final { .. }), "REQ-X3-5: an unresolved claim is never Final by a deadline (daa {daa}, hard {hard}, phase {phase:?})");
        if matches!(phase, PalwClaimPhaseV2::Voided { .. }) {
            break;
        }
        lg.beat(1).await;
        daa = lg.daa();
    }
}

/// **REQ-X3-2 on the node (premise of REQ-X3-5, 2026-10-19)**: a plain reservation with NO DA session does not hold a claim under X3;
/// it Finals at the effective deadline min(proposed, hard), which is the claim's window floor here (below the hard deadline).
#[tokio::test]
async fn x3_req_5b_a_reservation_without_a_session_finals_at_its_effective_deadline() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::over_with(true, false, true, TestConsensus::new);
    let (produced, _licensed) = reserved_then_licensed(&mut lg, true).await;
    let id = produced.claim_id;
    let claim = lg.chain.tip_state().1.claim(&id).expect("on the tip").clone();
    let hard = palw_claim_hard_deadline_of_v1(&lg.bundle.state, &claim).expect("X3 governs the claim");
    assert!(lg.view(id).record.is_some(), "the reservation is live");
    assert!(lg.chain.tip_state().1.da_claim(&id).map_or(0, |r| r.open_sessions()) == 0, "no DA session");
    lg.beat_to(hard + 2).await;
    match lg.view(id).phase {
        // REQ-X3-2: effective = min(proposed, hard); a plain reservation (no session) does not hold the claim past it.
        PalwClaimPhaseV2::Final { final_daa } => assert!(final_daa <= hard, "never past the hard deadline ({final_daa} > {hard})"),
        other => panic!("a plain reservation without a session does not hold under X3: {other:?} (hard {hard})"),
    }
}

/// **REQ-X3-1 across a restart (2026-10-18)**: the claim's fixed hard deadline is the same after the stop and the reopen, the PALW tip
/// and every delta row are the same, and a fresh node replays the chain to the same roots.
#[tokio::test]
async fn x3_the_fixed_deadline_survives_a_restart_and_replays() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut lg = Lg::over_with(false, false, true, |c| TestConsensus::with_db(db.clone(), c, sender));
    lg._keep.push(Box::new(receiver));
    let (produced, _, _) = licensed_claim(&mut lg, false).await;
    let id = produced.claim_id;
    let claim = lg.chain.tip_state().1.claim(&id).expect("on the tip").clone();
    let hard = palw_claim_hard_deadline_of_v1(&lg.bundle.state, &claim).expect("X3 governs the claim");
    let (sink, root) = (lg.chain.sink(), lg.chain.tip_state().1.state_root());
    let deltas: Vec<Hash64> = chain_blocks(&lg.chain, sink).iter().map(|b| root_at(&lg.chain, b.header.hash)).collect();
    // ---- stop, reopen on the same database ----
    let Lg { chain, config, bundle, premine, floats, funding, domain, backend, newcomer, nonce, rnd, _keep } = lg;
    let (simulated_time, heartbeat_nonce) = (chain.ctx.simulated_time, chain.nonce_for_reopen());
    drop(chain);
    let mut resumed = config.clone();
    resumed.process_genesis = false;
    let (sender, receiver) = async_channel::unbounded();
    let second = TestConsensus::with_db(db.clone(), &resumed, sender);
    let chain = t12_reopened_chain(second, &resumed, &bundle, simulated_time, heartbeat_nonce);
    let mut keep = _keep;
    keep.push(Box::new(receiver));
    let lg = Lg { chain, config, bundle, premine, floats, funding, domain, backend, newcomer, nonce, rnd, _keep: keep };
    assert_eq!(lg.chain.sink(), sink);
    assert_eq!(lg.chain.tip_state().1.state_root(), root, "the PALW tip, off disk");
    for (b, want) in chain_blocks(&lg.chain, sink).iter().zip(&deltas) {
        assert_eq!(root_at(&lg.chain, b.header.hash), *want, "the delta row of {} survived", b.header.hash);
    }
    let after = lg.chain.tip_state().1.claim(&id).expect("the claim survives the restart").clone();
    assert_eq!(palw_claim_hard_deadline_of_v1(&lg.bundle.state, &after), Some(hard), "REQ-X3-1: the fixed deadline survives the restart");
    // A fresh node replays the chain and reaches the same roots, block by block.
    let z = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    for block in chain_blocks(&lg.chain, lg.chain.sink()) {
        let hash = block.header.hash;
        arrive(&z, block, "A's block").await;
        assert_eq!(root_at(&z, hash), root_at(&lg.chain, hash), "the replaying node's root at {hash}");
    }
}

/// **REQ-X3-5 reorg on the node (2026-10-19)**: the reservation and its open DA session are created on branch A after the fork point
/// and before the open cutoff. A heavier branch B from that fork carries neither and reorgs past the claim's floor (the claim is Final
/// there, unheld). Then A returns. On every tip the reservation, the open-session count and the claim's phase follow the winning branch
/// exactly: no stale hold on B, no lost hold on the return to A.
#[tokio::test]
#[ignore = "UNVERIFIED: never completed. Branch B must out-weigh A's post-fork blocks, so its stamps run hours past the wall clock and the node's 132 s clock-lead cap refuses them; waiting real time does not finish. Needs a harness clock the node reads (not a relaxed cap)."]
async fn x3_req_5c_a_reorg_moves_the_open_session_hold_exactly_with_the_branch() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::over_with(true, false, true, TestConsensus::new);
    lg.beat(3).await;
    lg.register_newcomer().await;
    let produced = lg.real_attempt(EXECUTOR, true).await;
    let id = produced.claim_id;
    // The fork point: the claim is accepted, no reservation yet, and the open cutoff is still ahead.
    let fork = lg.chain.sink();
    let claim = lg.chain.tip_state().1.claim(&id).expect("on the tip").clone();
    let deadlines = palw_claim_deadlines_v1(&lg.bundle.state, &claim).expect("X3 governs the claim");
    assert!(lg.chain.daa_of(fork) <= deadlines.open_cutoff_daa, "the fork is before the open cutoff");
    // Branch A: the reservation, the bind and the license, then an open held DA session (the same pursuit as REQ-X3-5).
    let seats = lg.bind(id).await;
    let _licensed = lg.license(id, &seats).await;
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    let withhold = |unit: &PalwDaUnitV1| matches!(unit, PalwDaUnitV1::LegacyHeldV2(_));
    for _ in 0..8 {
        if v.binding.is_some() {
            break;
        }
        v.step(&mut lg, &produced, &withhold).await;
    }
    let binding = v.binding.clone().expect("the binding, read off the producer's authenticated answer");
    let height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(binding.step_leaf_count);
    let unit = kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2::StepNode { level: height, index: 0 };
    let object = v.demand(&mut lg, PalwLegacyProbeV1::HeldNode { unit });
    lg.send(vec![(NEWCOMER, object)]).await;
    let newcomer = lg.bond(NEWCOMER);
    assert!(lg.view(id).record.is_some(), "A: the reservation is live");
    assert!(
        lg.view(id).sessions.iter().any(|(accuser, session)| *accuser == newcomer && session.units.contains(&PalwDaUnitV1::LegacyHeldV2(unit))),
        "A: the reserved held session is open"
    );
    let hold = |c: &T12Chain| {
        let state = c.tip_state().1;
        (
            state.legacy_dispute_v1(&id).is_some_and(|r| r.holds()),
            state.da_claim(&id).map_or(0, |r| r.open_sessions()),
            std::mem::discriminant(&state.claim(&id).expect("on the tip").phase),
        )
    };
    let a_hold = hold(&lg.chain);
    assert_eq!(a_hold.1, 1, "A: one open session");
    // Z follows A.
    let z = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    for block in chain_blocks(&lg.chain, lg.chain.sink()) {
        arrive(&z, block, "A's block").await;
    }
    assert_eq!(z.tip_state().1.state_root(), lg.chain.tip_state().1.state_root(), "Z on A");
    assert_eq!(hold(&z), a_hold, "Z on A: the hold and the session follow A");
    // Branch B: from the fork, without the reservation or the session, heavier and past the floor.
    let mut b = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    let up_to_fork = chain_blocks(&lg.chain, fork);
    let fork_timestamp = up_to_fork.last().unwrap().header.timestamp;
    for block in up_to_fork {
        arrive(&b, block, "a block up to the fork").await;
    }
    b.ctx.simulated_time = fork_timestamp;
    let ttpb = lg.ttpb();
    let a_work = lg.chain.ctx.consensus.get_block(lg.chain.sink()).unwrap().header.blue_work;
    let mut b_blocks = Vec::new();
    while b.ctx.consensus.get_block(b.sink()).unwrap().header.blue_work <= a_work {
        // B needs more blocks past the fork than A has, so its stamps run ahead of the wall clock; wait them inside the real
        // clock-lead cap (132 s) instead of relaxing it.
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        let lead = (b.ctx.simulated_time + ttpb).saturating_sub(now + 100_000);
        if lead > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(lead)).await;
        }
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    // B stays inside the real shallow-tie window: a deeper branch is not a reorg the node takes (and outruns the clock lead cap).
    let shallow = kaspa_consensus_core::palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_DAA_V1;
    assert!(b.daa_of(b.sink()) - b.daa_of(fork) <= shallow, "B must remain inside the real shallow-tie window");
    for block in &b_blocks {
        arrive(&z, block.clone(), "B's block").await;
    }
    assert_eq!(z.sink(), b.sink(), "Z reorgs onto B");
    assert_eq!(z.tip_state().1.state_root(), b.tip_state().1.state_root(), "Z on B: B's own root");
    let b_hold = hold(&z);
    assert_eq!(b_hold.0, false, "B: no reservation, no stale hold");
    assert_eq!(b_hold.1, 0, "B: no open session, no stale hold");
    // Branch A returns: heavier again, with the same reservation and session.
    let old_len = chain_blocks(&lg.chain, lg.chain.sink()).len();
    let b_work = b.ctx.consensus.get_block(b.sink()).unwrap().header.blue_work;
    while lg.chain.ctx.consensus.get_block(lg.chain.sink()).unwrap().header.blue_work <= b_work {
        lg.beat(1).await;
    }
    for block in chain_blocks(&lg.chain, lg.chain.sink()).into_iter().skip(old_len) {
        arrive(&z, block, "A's later block").await;
    }
    assert_eq!(z.sink(), lg.chain.sink(), "Z back on A");
    assert_eq!(z.tip_state().1.state_root(), lg.chain.tip_state().1.state_root(), "Z on A: A's root");
    assert_eq!(hold(&z), hold(&lg.chain), "Z back on A: the hold and the session equal A's exactly");
    assert_eq!(hold(&z).1, 1, "the open session returns with A");
}
