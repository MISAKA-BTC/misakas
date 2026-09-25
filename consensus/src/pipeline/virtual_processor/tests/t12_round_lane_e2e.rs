//! **testnet-12's execution lane, end to end, through the real pipeline.**
//!
//! One chain, built block by block on testnet-12's shipped ruleset, carries a floor attempt from the
//! block that makes it all the way to a round block whose fee is paid to the bond that earned it:
//!
//! ```text
//! attempt (algo 6) ──► claim ──► PanelBound (derived by the chain) ──► ReceiptLicensed (3 signed
//!   receipts on a funded 0x4b carrier) ──► Final ──► round_finals ──► snapshot for span
//!   F + 2 + maturity ──► seed anchor (an attempt block in the span before) ──► schedule with
//!   tickets on the span's own rounds ──► round block (algo 10) signed for a ticket's round,
//!   anchored in that span ──► merging chain block: palw_round_verdicts_v1 permits it, the fold
//!   records the permit, and the coinbase pays the round block's fee to the bond's payout
//! ```
//!
//! **What is testnet-12 here, and what is not.** The params are `Params::from(testnet-12)` — every
//! fence, window, cadence, lane shape and class row exactly as shipped (`palw_economic_safety` and
//! `palw_audit_2026_09_23` included), and NO window is shortened. Three things differ, all forced by
//! the harness and none a rule:
//!
//! * **The eight genesis cards carry harness keys.** Each `BondRegistered` keeps its outpoint,
//!   collateral and operator key; its bond key and payout address are replaced by
//!   `TestConsensus::palw_v2_registry_keypair(i)` and that key's own address — the real ones are
//!   operator-held, and without them no attempt, receipt or round block can be signed.
//! * **The genesis premine is imported, with each bond's 100 MSK fee float paid to that harness
//!   payout** — the same UTXOs at the same outpoints and amounts, so the float the t12 genesis gives
//!   each bond (`PALW_RC_BOND_FEE_FLOAT_SOMPI`) is the one that funds the receipt carrier here, as
//!   it must on the live chain. The genesis `utxo_commitment` and hash are recomputed from that set
//!   exactly as `set_genesis_utxo_commitment_from_config` does for a node, and the set is imported
//!   through the node's own `import_pruning_point_utxo_set`.
//! * **The EVM lane is inert — on every build**: `evm_activation_daa_score` is `u64::MAX` and
//!   `palw_model_evm` `None` (testnet-12 ships the lane active from DAA 0, asserted below). This
//!   harness is testnet-12's clock: it stamps every template with a simulated time, because the
//!   chain it needs is hundreds of 120 s slots long and the wall clock cannot drive that. On an
//!   EVM-active network a template stamped after the build is a different block from the one the
//!   builder committed — `evm_commitment_root` is executed against the header's timestamp — and
//!   the node disqualifies it from the chain (the rule `TestContext::build_block_template` asserts;
//!   every re-stamp here asserts it too). The lane used to be left on under the `evm` feature, which
//!   `kaspad`'s default features switch on for this crate whenever the two are tested together, and
//!   every harness block was then disqualified. The lane is orthogonal to everything asserted here;
//!   the path a node actually mines — its own template, the heartbeat adapter, no re-stamp, the lane
//!   as shipped — is `t12_clock_floor::t12_the_node_s_own_beats_tick_from_genesis_with_the_evm_lane_as_shipped`.
//!
//! PoW difficulty is skipped (`skip_proof_of_work`), as in every harness test; the class lottery
//! (the attempt's `class_ticket_v3` under the class target) is NOT — every attempt here wins it.
//!
//! **One producer choice is made on purpose: the seed anchor's draw.** A floor Final's credit is
//! 7,708 CanonicalWork against a quantum of 100,000, so whether it mints a ticket (~7.7%) is decided
//! by the span seed, and the seed is a function of the seed anchor's execution key. The anchor's
//! producer picks, among its winning class draws, one whose seed gives the outcome a test needs —
//! the same choice any producer has (a draw is an inference, and which winning inference it carries
//! is its own) — and the seed the chain writes is asserted equal to the one predicted.
//!
//! The executions are not real inferences: the trace/output/execution roots are fixtures, the way
//! the harness carriage has always made them. Consensus never re-executes an attempt; the panel's
//! receipts are what certify it, and here they are real ML-DSA-87 signatures by the seats the chain
//! drew.
use super::{OnetimeTxSelector, TestContext, new_miner_data};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::headers::HeaderStoreReader;
use crate::model::stores::virtual_state::VirtualStateStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::config::{Config, ConfigBuilder, params::Params};
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::muhash::MuHashExtensions;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj};
use kaspa_consensus_core::tx::{
    PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_hashes::Hash64;
use kaspa_muhash::MuHash;
use libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair;
use std::sync::Arc;

const T12_GENESIS_CARDS: usize = 8;

/// Card `i`'s harness key — row `i` of the registry fixture (row 0 is the harness identity).
fn card_key(i: usize) -> &'static MLDSA87KeyPair {
    TestConsensus::palw_v2_registry_keypair(i as u64)
}

fn card_pubkey(i: usize) -> Vec<u8> {
    card_key(i).verification_key.as_ref().to_vec()
}

/// The P2PKH owner payload of card `i`'s harness key: what its payout and its fee float pay to.
fn card_payout_payload(i: usize) -> Hash64 {
    Hash64::from_bytes(kaspa_hashes::blake2b_512_address_payload(&card_pubkey(i)).as_bytes())
}

pub(super) fn card_payout_spk(i: usize) -> ScriptPublicKey {
    p2pkh_mldsa87_spk(card_payout_payload(i).as_byte_slice())
}

/// **testnet-12 as shipped, with harness keys on its eight genesis cards** — see the module doc for
/// the three differences and why each is forced. Returns the config, the bundle, the genesis UTXO
/// set the harness imports and each card's fee float in that set. The EVM lane is inert: every
/// harness that stamps its own clock onto templates uses this one.
pub(super) fn t12_with_harness_cards()
-> (Config, PalwConsensusParamsV2, Vec<(TransactionOutpoint, UtxoEntry)>, Vec<(TransactionOutpoint, UtxoEntry)>) {
    t12_with_harness_cards_and_evm(false)
}

/// [`t12_with_harness_cards`], with `keep_evm` leaving the EVM lane exactly as testnet-12 ships it
/// (active from DAA 0). Only a build with the `evm` feature can build a template for an active lane,
/// and only a caller that never moves a template's timestamp after the build may keep it.
pub(super) fn t12_with_harness_cards_and_evm(
    keep_evm: bool,
) -> (Config, PalwConsensusParamsV2, Vec<(TransactionOutpoint, UtxoEntry)>, Vec<(TransactionOutpoint, UtxoEntry)>) {
    use kaspa_consensus_core::config::params::PALW_T12_GENESIS_BONDS;
    use kaspa_consensus_core::config::premine::{PALW_RC_BOND_FEE_FLOAT_SOMPI, genesis_premine_utxos_for, premine_outpoint_for};
    assert!(!keep_evm || cfg!(feature = "evm"), "a build without the `evm` feature cannot build a template for an active EVM lane");
    let shipped = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    let mut params = shipped.clone();
    assert_eq!(PALW_T12_GENESIS_BONDS.len(), T12_GENESIS_CARDS);

    // The cards: keys and payouts only.
    {
        let PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
        let mut row = 0usize;
        for object in bundle.genesis_objects.iter_mut() {
            if let Obj::BondRegistered { bond, pubkey, payout_payload, .. } = object {
                assert_eq!(
                    bond.0,
                    premine_outpoint_for(shipped.net, PALW_T12_GENESIS_BONDS[row].premine_index),
                    "card {row} keeps its collateral outpoint"
                );
                *pubkey = card_pubkey(row);
                *payout_payload = card_payout_payload(row);
                row += 1;
            }
        }
        assert_eq!(row, T12_GENESIS_CARDS, "testnet-12 registers eight genesis bonds");
    }

    // The premine: each card's fee float, re-addressed to that card's harness payout.
    let mut utxos: Vec<(TransactionOutpoint, UtxoEntry)> = genesis_premine_utxos_for(params.net).into_iter().collect();
    let mut floats: Vec<Option<(TransactionOutpoint, UtxoEntry)>> = vec![None; T12_GENESIS_CARDS];
    for (outpoint, entry) in utxos.iter_mut() {
        for (i, card) in PALW_T12_GENESIS_BONDS.iter().enumerate() {
            if entry.amount == PALW_RC_BOND_FEE_FLOAT_SOMPI && entry.script_public_key == p2pkh_mldsa87_spk(&card.payout_payload) {
                assert!(floats[i].is_none(), "one float per card");
                entry.script_public_key = card_payout_spk(i);
                floats[i] = Some((*outpoint, entry.clone()));
            }
        }
    }
    let floats: Vec<_> =
        floats.into_iter().enumerate().map(|(i, f)| f.unwrap_or_else(|| panic!("card {i} has a fee float"))).collect();
    let mut multiset = MuHash::new();
    for (outpoint, entry) in &utxos {
        multiset.add_utxo(outpoint, entry);
    }
    params.genesis.utxo_commitment = multiset.finalize();
    params.genesis.hash = kaspa_consensus_core::header::Header::from(&params.genesis).hash;

    // The divergence is stated, not assumed: testnet-12 ships the lane at genesis.
    assert_eq!(shipped.evm_activation_daa_score, 0, "testnet-12 ships the EVM lane active from genesis");
    let config = ConfigBuilder::new(params)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            if !keep_evm {
                p.evm_activation_daa_score = u64::MAX;
                // ADR-0089 Decision 9: the market's EVM face may not be armed on a lane made inert.
                p.palw_model_evm = None;
            }
        })
        .build();
    config.params.validate_palw_v2().expect("testnet-12 with harness cards is a runnable ruleset");
    assert_eq!(config.params.is_evm_active(0), keep_evm, "the EVM lane is as asked");

    // What this test leans on is testnet-12's, and is asserted rather than assumed.
    assert_eq!(config.params.palw_execution_lane, shipped.palw_execution_lane, "the lane as shipped");
    assert_eq!(config.params.palw_execution_quanta, shipped.palw_execution_quanta);
    assert!(config.params.palw_economic_safety.is_some_and(|f| f.is_active(0)), "ADR-0151's bundle is armed from genesis");
    assert!(config.params.palw_audit_2026_09_23.is_some_and(|f| f.is_active(0)), "the 2026-09-23 audit fence is armed from genesis");
    assert_eq!(config.params.target_time_per_block(), shipped.target_time_per_block());
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    let PalwConsensusMode::ConsensusV2(shipped_bundle) = &shipped.palw_consensus_mode else { unreachable!() };
    assert_eq!(bundle.state, shipped_bundle.state, "every window as shipped");
    assert_eq!(bundle.panel, shipped_bundle.panel, "the panel as shipped");
    let bundle = bundle.clone();
    (config, bundle, utxos, floats)
}

/// **Stamp a built template with the harness's clock** — legal only where the EVM lane is inert at
/// the template's score. The builder executed `evm_commitment_root` against ITS timestamp (in whole
/// seconds), so a template stamped over it carries a commitment for another block and the node
/// disqualifies it from the chain; a node's own miner never re-stamps (the heartbeat adapter
/// re-commits the lane when it stamps a beat for its slot).
pub(super) fn stamp_harness_time(params: &Params, header: &mut kaspa_consensus_core::header::Header, timestamp: u64) {
    assert!(
        !params.is_evm_active(header.daa_score),
        "re-stamping a template whose EVM lane is active invalidates its evm_commitment_root — the harness runs with the lane inert"
    );
    header.timestamp = timestamp;
}

/// Sign input 0 of `tx`, which spends `utxo`, under card `i`'s key — the P2PKH-ML-DSA-87 spend
/// `adr0127_round_burst` builds.
pub(super) fn sign_spend(tx: &mut Transaction, utxo: UtxoEntry, i: usize, storage_mass_parameter: u64) {
    use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
    use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
    use kaspa_consensus_core::mass::MassCalculator;
    use kaspa_txscript::{MLDSA87_TX_CONTEXT, script_builder::ScriptBuilder};
    let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
        .calc_contextual_masses(&PopulatedTransaction::new(tx, vec![utxo.clone()]))
        .expect("contextual mass is computable")
        .storage_mass;
    tx.set_mass(storage_mass);
    let reused = Mldsa87SigHashReusedValuesUnsync::new();
    let sig_hash = calc_mldsa87_signature_hash(&PopulatedTransaction::new(tx, vec![utxo]), 0, SIG_HASH_ALL, &reused);
    let sig =
        libcrux_ml_dsa::ml_dsa_87::sign(&card_key(i).signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0x12u8; 32])
            .expect("ML-DSA-87 sign on the 64-byte sighash");
    let mut sig_item = sig.as_ref().to_vec();
    sig_item.push(SIG_HASH_ALL.to_u8());
    tx.inputs[0].signature_script =
        ScriptBuilder::new().add_data(&sig_item).expect("the signature fits").add_data(&card_pubkey(i)).expect("the key fits").drain();
}

/// The chain under construction, with the handful of facts every step needs. `pub(super)` so the
/// stake-draw integration suite (`t12_stake_draw_integration`, ADR-0152 T89) builds the same chain.
pub(super) struct T12Chain {
    pub(super) ctx: TestContext,
    pub(super) config: Config,
    pub(super) bundle: PalwConsensusParamsV2,
    pub(super) bonds: Vec<PalwBondKeyV2>,
    network_domain: Hash64,
    nonce: u64,
    heartbeats: u64,
    attempts: u64,
}

impl T12Chain {
    pub(super) fn vp(&self) -> Arc<crate::pipeline::virtual_processor::VirtualStateProcessor> {
        self.ctx.consensus.virtual_processor().clone()
    }

    pub(super) fn tip_state(&self) -> (BlockHash, PalwChainStateV2) {
        self.vp().palw_state_v2_store.read().load_tip(&self.bundle.state).unwrap().expect("the tip loads")
    }

    pub(super) fn sink(&self) -> BlockHash {
        self.ctx.consensus.get_sink()
    }

    pub(super) fn daa_of(&self, block: BlockHash) -> u64 {
        self.vp().headers_store.get_daa_score(block).unwrap()
    }

    fn sink_daa(&self) -> u64 {
        self.daa_of(self.sink())
    }

    fn span_daa(&self) -> u64 {
        self.config.params.palw_execution_lane.expect("t12 opens the lane").schedule_span_daa_at(self.sink_daa())
    }

    fn round_of(&self, timestamp_ms: u64) -> u64 {
        kaspa_consensus_core::palw_execution_lane_v1::palw_execution_round_v1(timestamp_ms, self.config.params.genesis.timestamp)
    }

    /// Insert a block this test built and demand it became the sink — a chain block the node
    /// refused would otherwise surface a hundred blocks later as a wrong number.
    async fn insert_chain_block(&mut self, block: MutableBlock, what: &str) -> Block {
        let block = block.to_immutable();
        let hash = block.header.hash;
        let status = self
            .ctx
            .consensus
            .validate_and_insert_block(block.clone())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("{what} {hash} was refused: {e}"));
        assert!(status.has_block_body(), "{what} has a body");
        assert_eq!(self.ctx.consensus.block_status(hash), BlockStatus::StatusUTXOValid, "{what} {hash} is UTXO-valid");
        assert_eq!(self.sink(), hash, "{what} {hash} is the sink");
        block
    }

    /// **A heartbeat (algo 8)**, `step_ms` after the simulated clock — the lane testnet-12's clock
    /// runs on: past `palw_anchor_clock` no other lane moves the DAA score.
    pub(super) async fn heartbeat(&mut self, step_ms: u64, txs: Vec<Transaction>) -> Block {
        self.ctx.simulated_time += step_ms;
        self.nonce += 1;
        let mut t = self
            .ctx
            .consensus
            .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(txs)), TemplateBuildMode::Standard)
            .expect("a template");
        stamp_harness_time(&self.config.params, &mut t.block.header, self.ctx.simulated_time);
        t.block.header.nonce = self.nonce;
        t.block.header.finalize();
        let (t, _) = self.vp().heartbeat_adapt_block_template(t).expect("the heartbeat lane is open on testnet-12");
        assert_eq!(t.block.header.pow_algo_id, kaspa_consensus_core::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1);
        self.ctx.simulated_time = self.ctx.simulated_time.max(t.block.header.timestamp);
        self.heartbeats += 1;
        self.insert_chain_block(t.block, "a heartbeat").await
    }

    /// **An attempt block (the attempt lane) by genesis card `card`**: the template the node builds,
    /// stamped with a carriage built from `palw_producer_facts_v2` at the template's own point — the
    /// class, target, pwu, operator id, artifact root and retention the chain will demand — whose
    /// class ticket wins, signed by the card's key. Returns the block and its claim id.
    ///
    /// `keep` is asked of each winning draw's execution key (`execution_commitment_v3` under the
    /// header's anchor — what the processor records as a seed anchor's `execution_key`); the first
    /// draw it keeps is carried. A producer varies its execution the same way: a draw is an
    /// inference, and which winning inference it carries is its own choice.
    pub(super) async fn attempt(
        &mut self,
        card: usize,
        step_ms: u64,
        txs: Vec<Transaction>,
        keep: &dyn Fn(Hash64) -> bool,
    ) -> (Block, Hash64) {
        let (block, claim_id) = self.build_attempt(card, step_ms, txs, keep);
        let block = self.insert_chain_block(block, &format!("card {card}'s attempt block")).await;
        (block, claim_id)
    }

    /// [`Self::attempt`]'s block, built and not inserted — so two can be built on the same parents.
    fn build_attempt(
        &mut self,
        card: usize,
        step_ms: u64,
        txs: Vec<Transaction>,
        keep: &dyn Fn(Hash64) -> bool,
    ) -> (MutableBlock, Hash64) {
        use kaspa_consensus_core::palw_attempt_v2::{
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
            PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
            execution_commitment_v3,
        };
        self.ctx.simulated_time += step_ms;
        self.nonce += 1;
        let bond = self.bonds[card];
        let mut t = self
            .ctx
            .consensus
            .build_block_template(
                MinerData::new(card_payout_spk(card), vec![]),
                Box::new(OnetimeTxSelector::new(txs)),
                TemplateBuildMode::Standard,
            )
            .expect("a template");
        assert!(
            kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(t.block.header.pow_algo_id),
            "a ConsensusV2 template declares the attempt lane"
        );
        stamp_harness_time(&self.config.params, &mut t.block.header, self.ctx.simulated_time);
        t.block.header.nonce = self.nonce;
        let facts = self
            .ctx
            .consensus
            .palw_producer_facts_v2(self.bundle.base_class_id, Some(bond.0))
            .expect("testnet-12 answers for its floor");
        let bond_facts = facts.bond.as_ref().expect("a genesis card is a registered bond").clone();
        assert_eq!(bond_facts.registered_pubkey, card_pubkey(card), "the chain registered card {card}'s harness key");
        facts.ready_to_produce(&card_pubkey(card)).unwrap_or_else(|why| panic!("card {card} is not ready to produce: {why}"));
        let header = &t.block.header;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let execution = Hash64::from_u64_word(0xE7EC_0000_0000_0000 | self.nonce);
        let mut attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: self.network_domain,
            challenge: challenge_v2(self.network_domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
            class_id: facts.class_id,
            executor_bond: bond.0,
            executor_pubkey: card_pubkey(card),
            operator_id: bond_facts.operator_id,
            artifact_root: facts.artifact_root,
            trace_root: Hash64::default(),
            output_root: Hash64::from_u64_word(0x0070_0000_0000_0000 | self.nonce),
            execution_root: execution,
            pwu: facts.pwu,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        let anchor = execution_anchor_v3(self.network_domain, pre_pow, facts.class_id, &bond.0, header.nonce);
        let mut won = false;
        for draw in 0u64..4_000_000 {
            attempt.trace_root = Hash64::from_u64_word((self.nonce << 32) ^ draw ^ 0x7A00_0000_0000_0000);
            attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
            if class_ticket_v3(&attempt, anchor) <= facts.class_target && keep(execution_commitment_v3(&attempt, anchor)) {
                won = true;
                break;
            }
        }
        assert!(won, "the floor's class lottery is winnable (with the draw kept)");
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
        self.attempts += 1;
        (t.block, claim_id)
    }

    /// Heartbeats until `done` holds of the tip, at most `cap`; the answer is `done`'s last value.
    async fn beat_until(&mut self, cap: u64, what: &str, mut done: impl FnMut(&Self) -> bool) {
        let start = self.sink_daa();
        for n in 0..cap {
            if done(self) {
                eprintln!("[t12-e2e] {what}: reached after {n} heartbeats (DAA {start} -> {})", self.sink_daa());
                return;
            }
            self.heartbeat(self.config.params.target_time_per_block(), Vec::new()).await;
            if n > 0 && n % 500 == 0 {
                eprintln!("[t12-e2e]   … {what}: {n} heartbeats, sink DAA {}", self.sink_daa());
            }
        }
        assert!(done(self), "{what}: not reached in {cap} heartbeats (DAA {start} -> {})", self.sink_daa());
    }

    /// **The block that binds `claim_id`'s panel on testnet-12** (ADR-0152 SW-8 and the M4 review's
    /// finding 2): heartbeats until the chain's DAA reaches the claim's anchor slot
    /// (`bind_base_daa() + anchor_delay`), then card `card`'s attempt. Past `palw_rcore_plus` a panel
    /// anchors only on an attempt block — a heartbeat's hash costs `2^24` hashes to re-roll, an
    /// attempt's an inference — so the heartbeats at the slot bind nothing (asserted), and this
    /// attempt, the first attempt block at or past the slot, is the claim's anchor and binds it in its
    /// own acceptance. The attempt makes a claim of its own for `card`, which the caller's chain simply
    /// carries. Returns the anchor block.
    pub(super) async fn attempt_at_the_anchor_slot(&mut self, claim_id: Hash64, card: usize) -> Block {
        let slot = {
            let (_, state) = self.tip_state();
            state.claim(&claim_id).expect("the claim exists").bind_base_daa() + self.bundle.panel.anchor_delay()
        };
        self.beat_until(4 * self.bundle.panel.anchor_delay() + 400, "the claim's anchor slot", |c| c.sink_daa() >= slot).await;
        let (_, state) = self.tip_state();
        assert_eq!(
            state.claim(&claim_id).expect("the claim stays").phase,
            PalwClaimPhaseV2::Provisional,
            "a heartbeat at the slot does not anchor a panel past palw_rcore_plus"
        );
        let (block, _) = self.attempt(card, self.config.params.target_time_per_block(), Vec::new(), &|_| true).await;
        assert!(block.header.daa_score >= slot, "the attempt stands at or past the slot");
        block
    }
}

/// **Genesis, as a node starts it**: the premine imported through the node's own
/// `import_pruning_point_utxo_set` against the recomputed commitment, each card's harness key and
/// payout asserted registered, and card 0's fee float in the virtual UTXO set.
pub(super) fn t12_genesis_chain(
    config: &Config,
    bundle: &PalwConsensusParamsV2,
    premine: &[(TransactionOutpoint, UtxoEntry)],
    floats: &[(TransactionOutpoint, UtxoEntry)],
) -> T12Chain {
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
    let bonds: Vec<PalwBondKeyV2> = {
        let (_, state) =
            ctx.consensus.virtual_processor().palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("genesis state");
        let keys: Vec<PalwBondKeyV2> = bundle
            .genesis_objects
            .iter()
            .filter_map(|o| match o {
                Obj::BondRegistered { bond, .. } => Some(*bond),
                _ => None,
            })
            .collect();
        for (i, key) in keys.iter().enumerate() {
            let record = state.bond(key).expect("a genesis card is registered");
            assert_eq!(record.pubkey, card_pubkey(i));
            assert_eq!(record.payout_payload, card_payout_payload(i));
        }
        keys
    };
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let chain =
        T12Chain { ctx, config: config.clone(), bundle: bundle.clone(), bonds, network_domain, nonce: 0, heartbeats: 0, attempts: 0 };
    {
        let utxos: std::collections::HashMap<_, _> =
            chain.ctx.consensus.get_virtual_utxos(None, 1_000_000, false).into_iter().collect();
        assert_eq!(
            utxos.get(&floats[0].0).map(|e| e.amount),
            Some(floats[0].1.amount),
            "card 0's fee float is in the virtual UTXO set"
        );
    }
    chain
}

/// Which seed anchor the chain is given — see [`a_floor_final_scheduled`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SeedDraw {
    /// An anchor whose seed lets the floor Final mint a ticket.
    MintsATicket,
    /// An anchor whose seed leaves the floor Final without one.
    MintsNoTicket,
}

/// The chain standing at the first block of the span a floor Final's snapshot targets, with the
/// schedule that block seeded.
struct AtTheTargetSpan {
    chain: T12Chain,
    lane: kaspa_consensus_core::config::params::PalwExecutionLaneV1,
    claim_id: Hash64,
    executor: PalwBondKeyV2,
    final_row: kaspa_consensus_core::palw_execution_lane_v1::PalwExecFinalV1,
    span_daa: u64,
    target: u64,
    opening: BlockHash,
    open_round: u64,
    window: u64,
    schedule: kaspa_consensus_core::palw_execution_lane_v1::PalwExecScheduleV1,
    carrier: Transaction,
}

/// **Stages 1–7 on testnet-12, every window as shipped**: card 0's floor attempt → its claim → the
/// panel the chain derives → a quorum's `Valid` receipts on a 0x4b carrier funded by card 0's genesis
/// float → `Final` → the snapshot one span later, for `span + 1 + maturity` → 120 spans of
/// heartbeats → card 1's attempt block in the span before the target (the seed anchor) → the
/// target span's first block, which seeds the schedule.
///
/// **Only the seed anchor's draw is chosen.** A floor Final's credit on testnet-12 is 7,708
/// CanonicalWork against a quantum of 100,000, so it mints one ticket with probability ~7.7%, decided
/// by the span seed — which is a function of the anchor's execution key, the span and the safe
/// frontier (`palw_execution_span_seed_v1`). Card 1 picks, among its winning class draws, the first
/// whose seed gives the answer `draw` asks for, and the seed the chain then writes is asserted equal
/// to the one predicted. Every other block is what it would be anyway.
async fn a_floor_final_scheduled(draw: SeedDraw) -> AtTheTargetSpan {
    use kaspa_consensus_core::palw_execution_lane_v1::{PalwExecSeedAnchorV1, palw_execution_span_seed_v1, palw_execution_span_v1};
    use kaspa_consensus_core::palw_execution_quanta_v1::{
        PALW_EXECUTION_QUANTUM_V1, palw_execution_quantum_count_v1, palw_execution_span_rounds_v1,
    };
    use kaspa_consensus_core::palw_panel_v2::{
        PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
    };
    kaspa_core::log::try_init_logger("warn");

    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let lane = config.params.palw_execution_lane.expect("t12 opens the execution lane");
    let ttpb = config.params.target_time_per_block();
    // testnet-12's maturity is the challenge window it applies (user decision 2026-09-25): 120 DAA,
    // the short window every licence here gets from DAA 0 — not the lattice's unshortened 1,200.
    let maturity_daa = config.params.palw_exec_quantum_maturity_v1();
    assert_eq!(maturity_daa, 120, "testnet-12 states the maturity it serves");
    assert_eq!(maturity_daa, bundle.state.window_challenge_at(0), "and it is the challenge window it applies");
    eprintln!(
        "[t12-e2e {draw:?}] testnet-12: target time {ttpb} ms; span {} DAA ({} rounds); anchor delay {}; receipt window {}; \
         challenge window {} (short {} at DAA 0); court window {}; quanta maturity {maturity_daa} DAA; seats {}/quorum {}; \
         lane width {} max/mergeset {}; quanta armed {:?}",
        lane.schedule_span_daa,
        palw_execution_span_rounds_v1(lane.schedule_span_daa, ttpb),
        bundle.panel.anchor_delay(),
        bundle.state.window_receipt(),
        bundle.state.window_challenge(),
        bundle.state.window_challenge_at(0),
        bundle.state.window_court(),
        bundle.panel.seat_count(),
        bundle.panel.quorum(),
        lane.permits_per_round,
        lane.max_per_mergeset,
        config.params.palw_execution_quanta,
    );

    // ---- genesis: the node's own premine import, against the recomputed commitment ----------------
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let network_domain = chain.network_domain;

    // ---- 1. the floor attempt ---------------------------------------------------------------------
    chain.heartbeat(ttpb, Vec::new()).await;
    let (attempt_block, claim_id) = chain.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let executor = chain.bonds[0];
    let accepted_daa = {
        let (_, state) = chain.tip_state();
        let claim = state.claim(&claim_id).expect("the attempt block created its claim");
        assert_eq!(claim.bond, executor);
        assert_eq!(claim.class_id, bundle.base_class_id, "a floor claim");
        eprintln!(
            "[t12-e2e {draw:?}] 1. attempt {} (algo {}) at DAA {} made floor claim {claim_id}: phase {:?}, pwu {}, reserved {}",
            attempt_block.header.hash,
            attempt_block.header.pow_algo_id,
            attempt_block.header.daa_score,
            claim.phase,
            claim.pwu,
            claim.reserved
        );
        claim.accepted_daa
    };

    // ---- 2. the chain derives and binds its panel, in its anchor block (an attempt, SW-8) --------------
    let anchor_block = chain.attempt_at_the_anchor_slot(claim_id, 7).await;
    let (_, state) = chain.tip_state();
    assert!(
        matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }),
        "the anchor block binds the claim: {:?}",
        state.claim(&claim_id).unwrap().phase
    );
    assert_eq!(state.panel(&claim_id).unwrap().anchor, anchor_block.header.hash, "anchored on the attempt block");
    let panel = state.panel(&claim_id).expect("a bound claim has a panel").clone();
    let PalwClaimPhaseV2::PanelBound { bound_daa } = state.claim(&claim_id).unwrap().phase.clone() else { unreachable!() };
    assert_eq!(bound_daa, anchor_block.header.daa_score, "bound at its anchor block");
    assert!(bound_daa >= accepted_daa + bundle.panel.anchor_delay(), "the anchor is at or past the anchor delay after acceptance");
    assert_eq!(panel.seats.len(), bundle.panel.seat_count() as usize, "a full jury");
    assert!(panel.seats.iter().all(|s| s.bond != executor), "the executor never sits on its own panel");
    let seat_cards: Vec<usize> =
        panel.seats.iter().map(|s| chain.bonds.iter().position(|b| *b == s.bond).expect("every seat is a genesis card")).collect();
    eprintln!("[t12-e2e {draw:?}] 2. panel bound at DAA {bound_daa}: seats are cards {seat_cards:?}");

    // ---- 3. a quorum of seats signs Valid; the receipts ride a 0x4b carrier funded by card 0's float -
    let signed_daa = chain.ctx.consensus.get_virtual_daa_score();
    let receipts: Vec<PalwSeatReceiptV2> = panel
        .seats
        .iter()
        .zip(&seat_cards)
        .take(bundle.panel.quorum() as usize)
        .map(|(seat, card)| {
            let message = palw_receipt_message_v2(network_domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &card_key(*card).signing_key,
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
    // The node's own assembler, as its panel service submits.
    let object = chain.vp().palw_v2_receipt_quorum_assemble_impl(claim_id, &receipts).expect("a signed quorum assembles");
    assert!(matches!(object, Obj::ReceiptLicensed { .. }), "a Valid quorum licenses the floor claim: {object:?}");
    let carrier = {
        use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() })
            .expect("serializes");
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
        sign_spend(&mut tx, float_entry, 0, config.params.storage_mass_parameter);
        tx
    };
    let carrying = chain.heartbeat(ttpb, vec![carrier.clone()]).await;
    assert!(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()), "the carrier is in the block");
    chain.heartbeat(ttpb, Vec::new()).await; // accepts the carrying block's transactions
    let (_, state) = chain.tip_state();
    let licensed_daa = match state.claim(&claim_id).unwrap().phase.clone() {
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => licensed_daa,
        other => panic!("the carried quorum licenses the claim; it is {other:?}"),
    };
    eprintln!("[t12-e2e {draw:?}] 3. licensed at DAA {licensed_daa} by {} receipts signed at DAA {signed_daa}", receipts.len());

    // ---- 4. the challenge window passes and the claim is Final -----------------------------------
    chain
        .beat_until(4 * bundle.state.window_challenge() + 400, "the claim reaches Final", |c| {
            let (_, state) = c.tip_state();
            state.claim(&claim_id).is_some_and(|c| matches!(c.phase, PalwClaimPhaseV2::Final { .. }))
        })
        .await;
    let (_, state) = chain.tip_state();
    let PalwClaimPhaseV2::Final { final_daa } = state.claim(&claim_id).unwrap().phase.clone() else { unreachable!() };
    assert_eq!(
        final_daa,
        licensed_daa + bundle.state.window_challenge_at(licensed_daa) + 1,
        "Final at the first chain block past testnet-12's short challenge window"
    );
    let span_daa = chain.span_daa();
    let final_span = palw_execution_span_v1(final_daa, span_daa);
    let (finals_span, finals) = state.round_finals();
    let final_row = *finals.get(&claim_id).expect("a Final attempt is a credit in its span");
    assert_eq!(finals_span, final_span);
    assert_eq!((final_row.bond, final_row.claim_id), (executor, claim_id));
    eprintln!("[t12-e2e {draw:?}] 4. Final at DAA {final_daa} (span {final_span}); credit {} CanonicalWork", final_row.credit);

    // ---- 5. the next span snapshots it for span + 1 + maturity ------------------------------------
    chain
        .beat_until(8, "the Final is snapshotted", |c| {
            let (_, state) = c.tip_state();
            state.round_pending_snapshots().values().any(|s| s.finals.iter().any(|f| f.claim_id == claim_id))
        })
        .await;
    let (_, state) = chain.tip_state();
    let target = *state
        .round_pending_snapshots()
        .iter()
        .find(|(_, s)| s.finals.iter().any(|f| f.claim_id == claim_id))
        .map(|(t, _)| t)
        .unwrap();
    let snap_span = palw_execution_span_v1(chain.sink_daa(), span_daa);
    let maturity_spans = maturity_daa.div_ceil(span_daa.max(1));
    assert_eq!(target, snap_span + 1 + maturity_spans, "past ADR-0151's bundle the maturity delays the snapshot, not the tickets");
    assert_eq!(maturity_spans, 120, "one-DAA spans: the 120-DAA maturity is 120 spans");
    assert!(
        target > final_span + maturity_spans && target <= final_span + 2 + maturity_spans + 1,
        "the first permit's span follows the Final by the maturity plus the snapshot's span rounding: final {final_span}, target {target}"
    );
    eprintln!(
        "[t12-e2e {draw:?}] 5. snapshotted at span {snap_span} for span {target} ({maturity_spans} spans of maturity; {} DAA after Final)",
        target * span_daa - final_daa
    );

    // ---- 6. the maturity runs; an attempt block in the span before the target records the anchor --
    chain
        .beat_until(2 * (target * span_daa) + 400, "the span before the target", |c| {
            palw_execution_span_v1(c.sink_daa(), span_daa) + 1 >= target
        })
        .await;
    assert_eq!(palw_execution_span_v1(chain.sink_daa(), span_daa) + 1, target, "the chain stands in the span before the target");
    let (_, state) = chain.tip_state();
    let (frontier_blue_score, frontier) = state.safe_frontier();
    let seed_of = move |execution_key: Hash64| {
        let anchor = PalwExecSeedAnchorV1 { span: target - 1, block: Hash64::default(), execution_key };
        palw_execution_span_seed_v1(&anchor, target, frontier_blue_score, frontier)
    };
    let tickets_under = move |execution_key: Hash64| {
        palw_execution_quantum_count_v1(
            u128::from(final_row.credit),
            u128::from(PALW_EXECUTION_QUANTUM_V1),
            seed_of(execution_key),
            claim_id,
        )
    };
    let keep = move |execution_key: Hash64| match draw {
        SeedDraw::MintsATicket => tickets_under(execution_key) >= 1,
        SeedDraw::MintsNoTicket => tickets_under(execution_key) == 0,
    };
    let (anchor_block, _) = chain.attempt(1, ttpb, Vec::new(), &keep).await;
    let (_, state) = chain.tip_state();
    let anchor = *state.round_seed_anchor().expect("an attempt-carrying chain block records the seed anchor");
    assert_eq!((anchor.span, anchor.block), (target - 1, anchor_block.header.hash), "recorded in the span before the target");
    assert!(state.round_schedule(target).is_none(), "no schedule before the span opens");

    // ---- 7. the target span opens: the snapshot is seeded as its schedule --------------------------
    chain.beat_until(8, "the target span opens", |c| palw_execution_span_v1(c.sink_daa(), span_daa) >= target).await;
    let opening = chain.sink();
    assert_eq!(palw_execution_span_v1(chain.sink_daa(), span_daa), target);
    let (_, state) = chain.tip_state();
    assert!(state.round_pending_snapshots().get(&target).is_none(), "the snapshot is spent");
    let schedule = state.round_schedule(target).expect("the target span's first block seeds its schedule").clone();
    assert_eq!(schedule.seed, seed_of(anchor.execution_key), "the chain seeded the span from the anchor and frontier predicted");
    assert!(schedule.finals.iter().any(|f| f.claim_id == claim_id), "the schedule is the Final's");
    let open_round = chain.round_of(chain.vp().headers_store.get_timestamp(opening).unwrap());
    let window = palw_execution_span_rounds_v1(span_daa, ttpb);
    eprintln!(
        "[t12-e2e {draw:?}] 7. span {target} opened at {opening} (round {open_round}); schedule: {} finals, {} tickets on rounds {:?}",
        schedule.finals.len(),
        schedule.quanta.len(),
        schedule.quanta.iter().map(|q| q.scheduled_round).collect::<Vec<_>>()
    );
    AtTheTargetSpan { chain, lane, claim_id, executor, final_row, span_daa, target, opening, open_round, window, schedule, carrier }
}

/// Stage 8 and 9: a chain block inside the target span (so the sink's selected parent — a round
/// block's anchor — is the opening block), then a round block for `round` under `permit_index`,
/// signed by the executor's key, carrying a spend of the receipt carrier's change that pays
/// `fee`. Returns the round block and the spend.
async fn a_round_block_in_the_target_span(
    at: &mut AtTheTargetSpan,
    round: u64,
    permit_index: u16,
    fee: u64,
) -> (MutableBlock, Transaction) {
    use kaspa_consensus_core::palw_execution_lane_v1::{
        PALW_EXEC_ENVELOPE_VERSION_V1, PALW_EXEC_MLDSA87_CONTEXT, PalwExecEnvelopeV1, palw_exec_signing_message_v1,
        palw_execution_span_v1,
    };
    // One second on, a block that does not move the clock, so the opening block becomes the sink's
    // selected parent — the anchor a round block hangs from — and the span does not move. It used to
    // be a heartbeat stamped a second after the span opened, which earned no DAA only because it was
    // stamped BEFORE its slot. The heartbeat audit's H1 stamps a beat for its slot and H3 refuses one
    // before it (a beat that can never tick is the 89% the audit measured), so a beat here would hold
    // the slot and the next block would step out of the span. An attempt block moves no clock on
    // testnet-12; card 3 is not otherwise used by these scenarios.
    at.chain.attempt(3, 1_000, Vec::new(), &|_| true).await;
    assert_eq!(palw_execution_span_v1(at.chain.sink_daa(), at.span_daa), at.target, "still the target span");
    assert_eq!(at.chain.vp().ghostdag_store.get_selected_parent(at.chain.sink()).unwrap(), at.opening);

    let (change, change_entry) = {
        let out = TransactionOutpoint::new(at.carrier.id(), 0);
        let utxos: std::collections::HashMap<_, _> =
            at.chain.ctx.consensus.get_virtual_utxos(None, 1_000_000, false).into_iter().collect();
        (out, utxos.get(&out).cloned().expect("the carrier's change is unspent"))
    };
    let paying = {
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(change, vec![], 0, 1)],
            vec![TransactionOutput::new(change_entry.amount - fee, card_payout_spk(0))],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        sign_spend(&mut tx, change_entry, 0, at.chain.config.params.storage_mass_parameter);
        tx
    };
    let template = at
        .chain
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(vec![paying.clone()])), TemplateBuildMode::Standard)
        .expect("a template carrying the spend");
    let mut block =
        at.chain.vp().round_adapt_block_template(template, round, card_payout_spk(0)).expect("the lane adapts a template").block;
    assert_eq!(block.header.pow_algo_id, kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1);
    assert_eq!(block.header.direct_parents(), &[at.opening], "hung from the opening block, the sink's selected parent");
    assert!(block.transactions.iter().any(|tx| tx.id() == paying.id()), "the round block carries the spend");
    assert_eq!(at.chain.round_of(block.header.timestamp), round, "stamped at the start of its round");
    block.header.nonce = 0;
    block.header.palw_commitment = Vec::new();
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
    let message = palw_exec_signing_message_v1(
        at.chain.network_domain,
        pre_pow,
        block.header.timestamp,
        block.header.nonce,
        round,
        permit_index,
        &at.executor,
    );
    let signature =
        libcrux_ml_dsa::ml_dsa_87::sign(&card_key(0).signing_key, message.as_byte_slice(), PALW_EXEC_MLDSA87_CONTEXT, [0u8; 32])
            .expect("ML-DSA-87 sign")
            .as_ref()
            .to_vec();
    block.header.palw_commitment = PalwExecEnvelopeV1 {
        version: PALW_EXEC_ENVELOPE_VERSION_V1,
        network_domain: at.chain.network_domain,
        round,
        permit_index,
        bond: at.executor,
        pubkey: card_pubkey(0),
        signature,
    }
    .encode();
    block.header.finalize();
    let round_hash = block.header.hash;
    let sink_before = at.chain.sink();
    at.chain
        .ctx
        .consensus
        .validate_and_insert_block(block.clone().to_immutable())
        .virtual_state_task
        .await
        .expect("a signed round block is valid at the header");
    assert_eq!(at.chain.sink(), sink_before, "a round block never moves the sink");
    assert_eq!(at.chain.vp().ghostdag_store.get_selected_parent(round_hash).unwrap(), at.opening, "anchored in the target span");
    (block, paying)
}

/// What the merging block did with one round block.
struct Merged {
    permitted: bool,
    uses: Vec<(u64, u64, u16)>,
    permit_recorded: bool,
    accepted_in_span: u64,
    spend_accepted: bool,
    fee_paid_to_payout: bool,
    merging: Block,
}

/// Stage 10: the verdict `palw_round_verdicts_v1` gives the round block against the tip state, then
/// the next chain block (card 2's attempt) that merges it, and what that block's fold and coinbase did.
async fn merge_the_round_block(
    at: &mut AtTheTargetSpan,
    round_block: &MutableBlock,
    paying: &Transaction,
    round: u64,
    fee: u64,
) -> Merged {
    use kaspa_consensus_core::palw_execution_lane_v1::{PALW_EXEC_ROUND_MS, PalwExecEnvelopeV1, palw_execution_span_v1};
    let round_hash = round_block.header.hash;
    let permit_index = PalwExecEnvelopeV1::decode(&round_block.header.palw_commitment).expect("an envelope").permit_index;
    at.chain.ctx.simulated_time =
        at.chain.ctx.simulated_time.max(at.chain.config.params.genesis.timestamp + (round + 1) * PALW_EXEC_ROUND_MS);
    let verdicts = {
        let vp = at.chain.vp();
        let virtual_state = vp.virtual_stores.read().state.get().unwrap();
        assert!(virtual_state.ghostdag_data.mergeset_reds.contains(&round_hash), "virtual merges the round block as a red");
        let (_, parent_state) = at.chain.tip_state();
        vp.palw_round_verdicts_v1(&parent_state, &virtual_state.ghostdag_data, virtual_state.daa_score).expect("the lane is open")
    };
    assert!(verdicts.round_blocks.contains(&round_hash));
    let (merging, _) = at.chain.attempt(2, 1_000, Vec::new(), &|_| true).await;
    let merging_hash = merging.header.hash;
    let data = at.chain.vp().ghostdag_store.get_data(merging_hash).unwrap();
    assert!(data.mergeset_reds.contains(&round_hash), "the merging block merges the round block as a red");
    assert_eq!(palw_execution_span_v1(merging.header.daa_score, at.span_daa), at.target, "in the target span");
    let (_, merged) = at.chain.tip_state();
    let accepted: Vec<_> = at
        .chain
        .ctx
        .consensus
        .get_block_acceptance_data(merging_hash)
        .unwrap()
        .iter()
        .flat_map(|m| m.accepted_transactions.iter().map(|e| e.transaction_id).collect::<Vec<_>>())
        .collect();
    let payout = card_payout_spk(0);
    Merged {
        permitted: verdicts.permitted.contains(&round_hash),
        uses: verdicts.uses.iter().map(|u| (u.span, u.round, u.permit_index)).collect(),
        permit_recorded: merged.round_permit_used(at.target, round, permit_index),
        accepted_in_span: merged.round_permits_accepted(at.target),
        spend_accepted: accepted.contains(&paying.id()),
        fee_paid_to_payout: merging.transactions[0].outputs.iter().any(|o| o.value == fee && o.script_public_key == payout),
        merging,
    }
}

/// **A floor attempt on testnet-12 becomes a permitted algo-10 block whose fee pays its bond.**
///
/// The seed anchor is drawn so the floor Final mints its ticket (see [`a_floor_final_scheduled`]);
/// everything else is the chain doing what testnet-12 does. Asserted along the way: the claim binds
/// a full panel at `accepted + anchor_delay`, is licensed by a carried quorum, is `Final` after the
/// short challenge window, is a credit in its span, is snapshotted for `span + 1 + 120` (the maturity is
/// that same short window), and the
/// target span's first block seeds a schedule whose tickets sit on that span's own rounds
/// (`[open + 3, open + 3 + 120)`). Then a round block signed for the first ticket's round, anchored
/// at the opening block: `palw_round_verdicts_v1` permits it and names the permit, the merging block
/// merges it as a red, the fold records the permit, the spend it carried is accepted, and the merging
/// block's coinbase pays that spend's fee to card 0's registered payout.
#[tokio::test]
async fn t12_a_floor_final_becomes_a_permitted_round_block_whose_fee_pays_its_bond() {
    use kaspa_consensus_core::palw_execution_lane_v1::palw_execution_permits_v1;
    use kaspa_consensus_core::palw_execution_quanta_v1::PALW_EXEC_TICKET_LEAD_ROUNDS_V1;
    const ROUND_FEE: u64 = 500_000;
    let started = std::time::Instant::now();
    let mut at = a_floor_final_scheduled(SeedDraw::MintsATicket).await;

    // The tickets: the Final's, the executor's, on the span's own rounds, consecutively from the lead.
    let mine: Vec<_> = at.schedule.quanta.iter().filter(|q| q.final_id == at.claim_id).copied().collect();
    assert!(!mine.is_empty(), "the Final mints a ticket under this seed");
    assert_eq!(mine.len(), at.schedule.quanta.len(), "and the span's only tickets are its");
    let first = at.open_round + PALW_EXEC_TICKET_LEAD_ROUNDS_V1;
    let mut rounds: Vec<u64> = mine.iter().map(|q| q.scheduled_round).collect();
    rounds.sort();
    assert_eq!(rounds, (first..first + mine.len() as u64).collect::<Vec<_>>(), "consecutive from the opening round plus the lead");
    assert!(
        rounds.iter().all(|r| *r < at.open_round + PALW_EXEC_TICKET_LEAD_ROUNDS_V1 + at.window),
        "inside the span's {} rounds",
        at.window
    );
    assert!(mine.iter().all(|q| q.bond == at.executor), "every ticket is the executor's");

    let ticket = mine.iter().min_by_key(|q| q.scheduled_round).copied().unwrap();
    let round = ticket.scheduled_round;
    let width = at.lane.width_of_span_len(at.target, at.span_daa);
    let permit = palw_execution_permits_v1(&at.schedule, round, width)
        .into_iter()
        .find(|p| p.bond == at.executor)
        .expect("the ticket's round is the executor's permit");
    assert_eq!(permit.quantum_id, ticket.quantum_id, "traceable to the Final that earned it");
    let untended = rounds.last().unwrap() + 1;
    assert!(palw_execution_permits_v1(&at.schedule, untended, width).is_empty(), "a round no ticket holds has no permit");

    let (round_block, paying) = a_round_block_in_the_target_span(&mut at, round, permit.index, ROUND_FEE).await;
    let view = at.chain.vp().palw_round_view_v1(round).expect("the lane is open");
    assert_eq!(view.span, at.target, "a round block built now is judged by the target span's schedule");
    assert!(view.permits.iter().any(|p| p.bond == at.executor && p.index == permit.index), "and the node's own view lists the permit");

    let merged = merge_the_round_block(&mut at, &round_block, &paying, round, ROUND_FEE).await;
    assert!(merged.permitted, "palw_round_verdicts_v1 permits the round block");
    assert_eq!(merged.uses, vec![(at.target, round, permit.index)], "and names the permit it spends");
    assert!(merged.permit_recorded, "the fold records the permit as used");
    assert_eq!(merged.accepted_in_span, 1, "one permit accepted in the span");
    assert!(merged.spend_accepted, "the permitted round block's spend is accepted");
    assert!(
        merged.fee_paid_to_payout,
        "the merging block pays the round block's {ROUND_FEE}-sompi fee to the bond's registered payout: {:?}",
        merged.merging.transactions[0].outputs.iter().map(|o| o.value).collect::<Vec<_>>()
    );
    // Route-matrix #7: the claim's own row reads the whole way.
    let (_, state) = at.chain.tip_state();
    let row = kaspa_consensus_core::palw_producer_v2::palw_claim_exec_lane_v1(&state, &at.claim_id).expect("the claim is in the lane");
    assert_eq!((row.stage, row.span, row.tickets as usize, row.tickets_spent), ("scheduled", at.target, mine.len(), 1));
    assert_eq!((row.first_round, row.last_round), (Some(first), rounds.last().copied()));
    eprintln!(
        "[t12-e2e] done: round block {} permitted and paid by {}; {} heartbeats + {} attempt blocks, sink DAA {}, {:.1} s",
        round_block.header.hash,
        merged.merging.header.hash,
        at.chain.heartbeats,
        at.chain.attempts,
        at.chain.sink_daa(),
        started.elapsed().as_secs_f64()
    );
}

/// **Route-matrix #2 — a Final that mints no ticket holds no round permit** (formerly a FINDING:
/// it handed its bond the whole domain lottery instead).
///
/// Past `palw_execution_quanta` a Final's rights are its tickets: `palw_execution_mint_quanta_windowed_v1`
/// mints `credit / quantum` of them with the remainder drawn against the seed, and ADR-0151 prices a
/// Final's realizable rights as those tickets. The schedule could not say "armed, and zero minted":
/// `palw_execution_permits_v1` reads an EMPTY `quanta` as "the ADR-0125 lottery still draws this
/// span", so a span whose Finals all minted nothing — on testnet-12 that is ~92% of floor Finals
/// (credit 7,708 against a quantum of 100,000) — granted the domain's listed bonds a permit on every
/// round of the domain's parity (60 of the span's 120, measured here before the fix, each paid). The
/// same fallback ran for a Final whose execution was convicted after its snapshot.
///
/// This test builds that chain for real — the same stages, with the seed anchor drawn so the Final
/// mints nothing — and signs a round block for a round the OLD rule granted the executor. Past
/// ADR-0151's bundle the chain applies `palw_execution_permits_v2(.., tickets_only = true)`: the node's
/// own view lists no permit, the merging block's verdict refuses the round block, no permit is
/// recorded, and neither the spend nor its fee is accepted.
#[tokio::test]
async fn t12_a_final_that_mints_no_ticket_holds_no_round_permit() {
    use kaspa_consensus_core::palw_execution_lane_v1::{palw_execution_permits_v1, palw_execution_permits_v2};
    use kaspa_consensus_core::palw_execution_quanta_v1::PALW_EXEC_TICKET_LEAD_ROUNDS_V1;
    const ROUND_FEE: u64 = 500_000;
    let mut at = a_floor_final_scheduled(SeedDraw::MintsNoTicket).await;
    assert!(at.schedule.quanta.is_empty(), "under this seed the floor Final mints no ticket");
    assert!(
        at.schedule.finals.iter().any(|f| f.claim_id == at.claim_id && f.credit == at.final_row.credit),
        "though it is the schedule's Final"
    );

    // The rounds of the span's own window the OLD rule (the lottery fallback) granted the executor,
    // and what the chain's rule grants now.
    let width = at.lane.width_of_span_len(at.target, at.span_daa);
    let first = at.open_round + PALW_EXEC_TICKET_LEAD_ROUNDS_V1;
    let lottery: Vec<u64> = (first..first + at.window)
        .filter(|r| palw_execution_permits_v1(&at.schedule, *r, width).iter().any(|p| p.bond == at.executor))
        .collect();
    let granted: Vec<u64> = (first..first + at.window)
        .filter(|r| palw_execution_permits_v2(&at.schedule, *r, width, true).iter().any(|p| p.bond == at.executor))
        .collect();
    eprintln!(
        "[t12-e2e MintsNoTicket] with 0 tickets the lottery fallback would grant the executor {} of the span's {} rounds; \
         the tickets-only rule grants {}",
        lottery.len(),
        at.window,
        granted.len()
    );
    assert!(granted.is_empty(), "no ticket, no permit — ADR-0151 prices a Final's rights as its tickets");
    let &round = lottery.first().expect("the fallback this test guards against would have granted a round");
    let old_permit = palw_execution_permits_v1(&at.schedule, round, width).into_iter().find(|p| p.bond == at.executor).unwrap();
    assert_eq!(old_permit.quantum_id, Hash64::default(), "the fallback's permit was a lottery permit, traceable to no Final");

    // The chain, end to end: a round block signed for that round under the old permit.
    let (round_block, paying) = a_round_block_in_the_target_span(&mut at, round, old_permit.index, ROUND_FEE).await;
    let view = at.chain.vp().palw_round_view_v1(round).expect("the lane is open");
    assert_eq!(view.span, at.target, "judged by the target span's schedule");
    assert!(view.permits.is_empty(), "the node's own view lists no permit for a round no ticket holds");
    let merged = merge_the_round_block(&mut at, &round_block, &paying, round, ROUND_FEE).await;
    let facts = format!(
        "a Final with credit {} minted 0 tickets (quantum {}); the round block for round {round} was permitted={}, its \
         permit recorded={}, its spend accepted={}, and its {ROUND_FEE}-sompi fee paid to the bond={}",
        at.final_row.credit,
        kaspa_consensus_core::palw_execution_quanta_v1::PALW_EXECUTION_QUANTUM_V1,
        merged.permitted,
        merged.permit_recorded,
        merged.spend_accepted,
        merged.fee_paid_to_payout,
    );
    eprintln!("[t12-e2e MintsNoTicket] {facts}");
    assert!(
        !merged.permitted && merged.uses.is_empty() && !merged.permit_recorded && merged.accepted_in_span == 0,
        "the verdict refuses a round block no ticket holds: {facts}"
    );
    assert!(!merged.spend_accepted && !merged.fee_paid_to_payout, "and nothing it carried is accepted or paid: {facts}");
}

/// **The 2026-09-23 re-audit of finding 17: a chain block's own attempt that the fold skips is not
/// paid its carve.**
///
/// Past `palw_audit_2026_09_23` step 4 of the transition SKIPS a block's own attempt that the live
/// exposure ceiling refuses (`AttemptExposureCeiling`), and records no claim. The coinbase of the
/// block's selected-chain child withholds the parent's worker carve by reading
/// `palw_v2_escrow_withheld_at` — which summed claims only, so a skipped attempt withheld nothing and
/// its whole worker share was paid for work nobody could examine. The skip itself cannot be staged
/// in this harness without filling a 939,063-MSK card's ceiling, so the state the fold would have
/// left is taken as the attempt block's PARENT state (which holds no claim for it): the withhold
/// read against it must be the carve `apply_attempt` would have escrowed, and against the state the
/// fold actually wrote it must be the claim's own escrow, not twice it. A heartbeat withholds nothing
/// either way. Both call sites (validation and template) read this one function.
#[tokio::test]
async fn t12_a_skipped_own_attempt_has_its_carve_withheld_from_the_child_coinbase() {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let ttpb = config.params.target_time_per_block();
    let beat = chain.heartbeat(ttpb, Vec::new()).await;
    let (attempt_block, claim_id) = chain.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let vp = chain.vp();
    let attempt_hash = attempt_block.header.hash;
    let (_, admitted_state) = chain.tip_state();
    let claim = admitted_state.claim(&claim_id).expect("the attempt was admitted").clone();
    assert_eq!(claim.accepted_block, attempt_hash);
    assert!(claim.escrowed_reward > 0, "a testnet-12 attempt escrows a carve");
    let skipped_state = {
        let (_, delta) = vp.palw_state_v2_store.read().delta_of(attempt_hash).expect("a chain block has a delta");
        kaspa_consensus_core::palw_state_v2::revert_delta_v2(&admitted_state, &delta, &bundle.state).expect("the delta reverts")
    };
    assert!(skipped_state.claim(&claim_id).is_none(), "the state a skip leaves holds no claim for the attempt");

    assert_eq!(
        vp.palw_v2_escrow_withheld_at(&admitted_state, attempt_hash),
        claim.escrowed_reward,
        "an admitted attempt is withheld from its claim's record, once"
    );
    assert_eq!(
        vp.palw_v2_escrow_withheld_at(&skipped_state, attempt_hash),
        claim.escrowed_reward,
        "a skipped attempt is withheld the carve its claim would have escrowed, and it is never released"
    );
    assert_eq!(vp.palw_v2_escrow_withheld_at(&skipped_state, beat.header.hash), 0, "a heartbeat carries no attempt");

    // And the child's coinbase, built by the node's own template path against the admitted state,
    // pays the attempt block's worker base less exactly that escrow — the figure the skipped state
    // now withholds as well.
    let child = chain.heartbeat(ttpb, Vec::new()).await;
    let paid_to_card0: u64 = child.transactions[0]
        .outputs
        .iter()
        .filter(|o| o.script_public_key == card_payout_spk(0))
        .map(|o| o.value)
        .sum();
    let split = vp.fee_split_at(attempt_block.header.daa_score).expect("the overlay split");
    let parts = kaspa_consensus_core::dns_finality::split_block_subsidy(
        vp.coinbase_manager.calc_block_subsidy(attempt_block.header.daa_score),
        &split,
    );
    assert_eq!(paid_to_card0, parts.worker_base_sompi - claim.escrowed_reward, "the child pays the worker base less the escrow");
}

/// **The licence stall (2026-09-24), through the node's own assemblers on testnet-12's ruleset.**
///
/// About half the floor claims of testnet-12 stayed `PanelBound`: the processor's assemblers added
/// V3 receipts in arrival order and dropped any whose addition was not `NoQuorum`, so a pool froze
/// at its first two `Valid`s — the optimistic licence formed only when the full-replay seat was one
/// of them, and the coverage licence never. Here the chain derives a floor claim's five-seat panel,
/// every seat signs its duty receipt over the V3 message with its assigned mask (real ML-DSA-87, by
/// the seats the chain drew), and the full seat's receipt arrives LAST:
///
/// * the whole panel assembles a `ReceiptLicensedV2` of all five, and the acceptance layer takes it;
/// * the pool claim d0815709 held when it stalled — three partials, then the full seat — assembles an
///   `OptimisticLicensed` of the full seat and one partial, and coverage assembles nothing;
/// * that optimistic object rides a funded 0x4b carrier and the claim is `ReceiptLicensed`: the
///   acceptance layer, the audit fence's fold filter and the fold itself all agree with the builder.
#[tokio::test]
async fn t12_a_late_full_seat_licenses_through_the_node_s_own_assemblers() {
    use kaspa_consensus_core::palw_panel_v2::{
        PALW_RECEIPT_V3_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3, palw_receipt_message_v3,
    };
    use kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2;
    kaspa_core::log::try_init_logger("warn");

    let (config, bundle, premine, floats) = t12_with_harness_cards();
    assert!(config.params.palw_verification_v2_at(0) && config.params.palw_verification_s2_at(0), "t12 arms S1 and S2 at genesis");
    let ttpb = config.params.target_time_per_block();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let network_domain = chain.network_domain;

    chain.heartbeat(ttpb, Vec::new()).await;
    let (_, claim_id) = chain.attempt(0, ttpb, Vec::new(), &|_| true).await;
    chain.attempt_at_the_anchor_slot(claim_id, 7).await;
    let (_, state) = chain.tip_state();
    assert!(matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "bound in its anchor block");
    let panel = state.panel(&claim_id).expect("a bound claim has a panel").clone();
    assert_eq!(panel.seats.len(), 5, "the shipped jury");
    let assignment = palw_segment_assignment_v2(panel.anchor, claim_id, panel.seats.len() as u16);
    let signed_daa = chain.ctx.consensus.get_virtual_daa_score();
    let by_duty: Vec<PalwSeatReceiptV3> = panel
        .seats
        .iter()
        .enumerate()
        .map(|(i, seat)| {
            let card = chain.bonds.iter().position(|b| *b == seat.bond).expect("every seat is a genesis card");
            let segments = assignment.mask_of(i as u16);
            let message = palw_receipt_message_v3(network_domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa, segments);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &card_key(card).signing_key,
                message.as_byte_slice(),
                PALW_RECEIPT_V3_MLDSA87_CONTEXT,
                [0x33u8; 32],
            )
            .expect("sign")
            .as_ref()
            .to_vec();
            PalwSeatReceiptV3 {
                receipt: PalwSeatReceiptV2 {
                    claim: claim_id,
                    verdict: PalwReceiptVerdictV2::Valid,
                    seat_bond: seat.bond,
                    signed_daa,
                    signature,
                },
                segments,
            }
        })
        .collect();
    let full = by_duty[assignment.full_seat as usize].clone();
    let partials: Vec<PalwSeatReceiptV3> =
        by_duty.iter().enumerate().filter(|(i, _)| *i != assignment.full_seat as usize).map(|(_, r)| r.clone()).collect();
    let vp = chain.vp();

    // The whole panel, the full seat's receipt last: coverage licenses with all five.
    let mut whole = partials.clone();
    whole.push(full.clone());
    let coverage = vp.palw_v2_receipt_coverage_assemble_impl(claim_id, &whole).expect("the whole panel covers");
    let Obj::ReceiptLicensedV2 { receipts, .. } = &coverage else { panic!("coverage builds a ReceiptLicensedV2: {coverage:?}") };
    assert_eq!(receipts.len(), 5);
    let (tip_block, state) = vp.palw_state_v2_store.read().load_tip_cached(&bundle.state).unwrap().expect("the tip loads");
    let virtual_state = vp.lkg_virtual_state.load();
    let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: tip_block,
        daa_score: virtual_state.daa_score,
        blue_score: virtual_state.ghostdag_data.blue_score,
        subsidy: 0,
    };
    vp.palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(&coverage))
        .expect("the acceptance layer takes the coverage licence the node built");

    // d0815709's pool: three partials, then the full seat.
    let stalled: Vec<PalwSeatReceiptV3> = partials[..3].iter().cloned().chain(std::iter::once(full.clone())).collect();
    assert!(vp.palw_v2_receipt_coverage_assemble_impl(claim_id, &stalled).is_none(), "four receipts cannot cover");
    let optimistic = vp.palw_v2_optimistic_assemble_impl(claim_id, &stalled).expect("the late full seat still licenses");
    let Obj::OptimisticLicensed { receipts, .. } = &optimistic else { panic!("an OptimisticLicensed: {optimistic:?}") };
    assert_eq!(receipts.as_slice(), &[full.clone(), partials[0].clone()], "the full seat first, then the first partial to arrive");
    vp.palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(&optimistic))
        .expect("the acceptance layer takes the optimistic licence the node built");

    // Carried, it licenses the claim.
    let carrier = {
        use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: optimistic.clone() })
            .expect("serializes");
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
        sign_spend(&mut tx, float_entry, 0, config.params.storage_mass_parameter);
        tx
    };
    let carrying = chain.heartbeat(ttpb, vec![carrier.clone()]).await;
    assert!(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()), "the carrier is in the block");
    chain.heartbeat(ttpb, Vec::new()).await; // accepts the carrying block's transactions
    let (_, state) = chain.tip_state();
    let phase = state.claim(&claim_id).unwrap().phase.clone();
    assert!(matches!(phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the carried optimistic licence licenses the claim: {phase:?}");
}

/// **T-THREAD (ADR-0152 v3.1 J-1), through the pipeline on testnet-12: a claim records the
/// execution anchor of the header that carried it** — `execution_anchor_v3(network domain, the
/// header's pre-PoW hash, class, bond, nonce)`, the job its attempt had to answer:
///
/// * **own work**: card 0's attempt block, built by this node and inserted as the sink;
/// * **merged work**: card 1's attempt block built on the SAME parents as card 2's, both inserted,
///   and a heartbeat that merges whichever lost the tip — its claim, folded from the anticone,
///   records the anchor of its OWN header (never the merging block's, never the attempt id).
#[tokio::test]
async fn t12_a_claim_records_the_anchor_of_the_header_that_carried_it() {
    use kaspa_consensus_core::palw_attempt_v2::{PalwAttemptEnvelopeV2, execution_anchor_v3};
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    assert!(config.params.palw_offence_attribution_active_at(0), "testnet-12 records job identities from genesis");
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let ttpb = config.params.target_time_per_block();
    chain.heartbeat(ttpb, Vec::new()).await;
    let anchor_of = |chain: &T12Chain, block: &Block| {
        let envelope = PalwAttemptEnvelopeV2::decode_wire(&block.header.palw_commitment).expect("an attempt carriage");
        execution_anchor_v3(
            chain.network_domain,
            kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header),
            envelope.attempt.class_id,
            &envelope.attempt.executor_bond,
            block.header.nonce,
        )
    };

    // Own work.
    let (own_block, own_claim) = chain.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let (_, state) = chain.tip_state();
    let recorded = state.claim(&own_claim).expect("the attempt opened its claim").job_identity;
    assert_eq!(recorded, anchor_of(&chain, &own_block), "own work records its own header's anchor");
    assert_ne!(recorded, own_claim, "the anchor, never the claim id");

    // Two attempt blocks on the same parents; the heartbeat merges the one that lost the tip.
    let (a, claim_a) = chain.build_attempt(1, ttpb, Vec::new(), &|_| true);
    let (b, claim_b) = chain.build_attempt(2, 0, Vec::new(), &|_| true);
    assert_eq!(a.header.direct_parents(), b.header.direct_parents(), "siblings");
    let (a, b) = (a.to_immutable(), b.to_immutable());
    for block in [&a, &b] {
        chain.ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await.expect("an attempt block is valid");
    }
    chain.heartbeat(ttpb, Vec::new()).await;
    let (_, state) = chain.tip_state();
    for (block, claim) in [(&a, claim_a), (&b, claim_b)] {
        let record = state.claim(&claim).unwrap_or_else(|| panic!("claim {claim} was admitted (chain or merged)"));
        assert_eq!(record.accepted_block, block.header.hash, "the claim's carrying block is its own");
        assert_eq!(record.job_identity, anchor_of(&chain, block), "each claim records the anchor of its OWN header");
    }
}
