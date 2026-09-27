//! **Claim-capacity profiler for testnet-12 (audit/claim-capacity, 2026-09-26).**
//!
//! Not a security test: a MEASUREMENT harness. It drives testnet-12 as shipped (`0e8ec984e`, the
//! public release) through the real pipeline — the node's own templates, the heartbeat adapter, the
//! fold, the stake draw that binds panels, the node's own receipt assemblers, real ML-DSA-87
//! signatures by the seats the chain drew — and records, DAA by DAA, how many claims a producer bond
//! of a given collateral can open, when each claim licenses, when its escrow term leaves the bond's
//! ledger, when it reaches Final, and why the producer is held when it is held.
//!
//! The chain is `t12_round_lane_e2e`'s harness (eight genesis cards with harness keys, the premine
//! imported, the EVM lane inert). What this file adds, all by the test's hand and all stated:
//!
//! * **Subject bonds are planted** on the tip through the carriage (as `t12_rcore_s6_producer_share`
//!   and `p2_mint_path` plant): a registered, active bond with the collateral under test, its own
//!   harness key (registry keypair 8 + j), a unique operator id, and NO capable classes — so the
//!   draw never seats it and its ledger holds only its own claims (a pure producer).
//! * **For the 8k row, readiness is planted** for the eight cards (`proof_version 2`, proved at the
//!   planting DAA, refreshed every 16 DAA), and the row is put in `Probation` if the fold holds it
//!   below that (the harness cards cannot run the real 8k readiness proof). Probation admits claims;
//!   its 50‰ only prices the production share, not the room (`admission_permille`).
//! * **Background anchors**: past `palw_rcore_plus` a panel binds only in an attempt block at or past
//!   its anchor slot, so one genesis card (rotating) makes one floor attempt per DAA — the live
//!   network's floor producers do the same. Its claims are licensed like everyone's.
//! * **Receipts**: every drawn seat signs its duty receipt (V3, its assigned segment mask) and the
//!   node's coverage assembler builds the licence (`all5`), or the first three seats sign V2 full-mask
//!   receipts and the quorum assembler builds a V1 licence (`quorum3`, escrow NOT released: SR-1 needs
//!   every seat served). A licence rides a funded 0x4b carrier in the next block. The delay between
//!   the bind and the signatures is a scenario parameter (`CAP_LIC_DELAY`, DAA).
//!
//! Every run is `#[ignore]` (minutes to an hour of a debug build) and is steered by environment
//! variables; the results are JSON lines appended to `CAP_OUT`.
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, sign_spend, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::palw_model_registry_v1::{PalwModelLifecycleV1, PalwSeatReadinessRowV1};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PALW_RECEIPT_V3_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3,
    palw_receipt_message_v2, palw_receipt_message_v3,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBondKeyV2, PalwBondStateV2, PalwBondStatusV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj,
    PalwStateCarriageV2,
};
use kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2;
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

const SOMPI: u64 = 100_000_000;

fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn out_line(line: String) {
    eprintln!("[cap] {line}");
    if let Ok(path) = std::env::var("CAP_OUT") {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("CAP_OUT opens");
        writeln!(f, "{line}").expect("CAP_OUT writes");
    }
}

/// The key a planted subject signs with: registry keypair `8 + j` (the cards use 0..7).
fn subject_key(j: usize) -> &'static libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair {
    TestConsensus::palw_v2_registry_keypair(8 + j as u64)
}

fn subject_pubkey(j: usize) -> Vec<u8> {
    subject_key(j).verification_key.as_ref().to_vec()
}

fn subject_payload(j: usize) -> Hash64 {
    Hash64::from_bytes(kaspa_hashes::blake2b_512_address_payload(&subject_pubkey(j)).as_bytes())
}

fn card_key(i: usize) -> &'static libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair {
    TestConsensus::palw_v2_registry_keypair(i as u64)
}

/// Who signs an attempt: a genesis card (a seat too) or a planted subject (a pure producer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Producer {
    Card(usize),
    Subject(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LicencePolicy {
    /// Every seat signs its V3 duty receipt; the coverage assembler licenses with all five.
    All5,
    /// The first three seats sign V2 full-mask receipts; the V1 quorum door licenses (k = 3) and
    /// the escrow stays (SR-1 needs every seat served) until Final.
    Quorum3,
    /// Nobody signs: claims sit bound until the receipt window voids them.
    None,
    /// The full-replay seat and one partial sign V3; the node's optimistic assembler licenses
    /// (`OptimisticLicensed`, S2, basis_k = 1): counted unlicensed, escrow held, due at bound + 601.
    S2,
    /// Each claim draws one of all5 / quorum3 / s2 with these permilles (the live mix).
    Mix(u16, u16, u16),
}

pub(super) struct Subject {
    label: String,
    pub(super) bond: PalwBondKeyV2,
    collateral: u64,
    claims: BTreeSet<Hash64>,
    accepted_total: u64,
    skipped_total: u64,
}

pub(super) struct CapSim {
    pub(super) chain: T12Chain,
    network_domain: Hash64,
    nonce: u64,
    class_id: Hash64,
    base_class: Hash64,
    k8_class: Option<Hash64>,
    pub(super) subjects: Vec<Subject>,
    /// Spendable carrier funding: (outpoint, entry, card whose key signs it, usable from block #).
    wallet: Vec<(TransactionOutpoint, UtxoEntry, usize, u64)>,
    blocks: u64,
    queued_carriers: Vec<Transaction>,
    /// Claims bound and waiting for their receipts: claim -> DAA the seats sign at.
    receipts_due: BTreeMap<Hash64, u64>,
    licensed_sent: BTreeSet<Hash64>,
    background_card: usize,
    policy: LicencePolicy,
    lic_delay: u64,
    /// Every claim's life, for the latency tables: accepted, bound, licensed, released, final, void.
    life: BTreeMap<Hash64, [Option<u64>; 6]>,
    run: String,
    logged_mass: bool,
    assembler_checked: std::cell::Cell<bool>,
    /// `CAP_LIC_DELAY=live:<class>`: bind -> licence delays drawn from live public t12 (DAA 140-170),
    /// one entry per observed licence; the harness's carriage adds its own ~1 DAA.
    pub(super) lic_delays: Option<Vec<u64>>,
}

impl CapSim {
    fn vp(&self) -> std::sync::Arc<crate::pipeline::virtual_processor::VirtualStateProcessor> {
        self.chain.vp()
    }

    fn daa(&self) -> u64 {
        self.chain.ctx.consensus.get_virtual_daa_score()
    }

    fn state(&self) -> std::sync::Arc<PalwChainStateV2> {
        self.vp().palw_state_v2_store.read().load_tip_cached(&self.chain.bundle.state).unwrap().expect("the tip loads").1
    }

    /// Insert a block and demand it became the sink.
    async fn insert(&mut self, block: MutableBlock, what: &str) -> Block {
        let block = block.to_immutable();
        let hash = block.header.hash;
        let status = self
            .chain
            .ctx
            .consensus
            .validate_and_insert_block(block.clone())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("{what} {hash} was refused: {e}"));
        assert!(status.has_block_body(), "{what} has a body");
        assert_eq!(self.chain.ctx.consensus.block_status(hash), BlockStatus::StatusUTXOValid, "{what} {hash} is UTXO-valid");
        assert_eq!(self.chain.sink(), hash, "{what} {hash} is the sink");
        self.blocks += 1;
        block
    }

    fn producer_key(&self, p: Producer) -> (&'static libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair, PalwBondKeyV2, Vec<u8>, Hash64) {
        match p {
            Producer::Card(i) => {
                let key = card_key(i);
                let payload = Hash64::from_bytes(kaspa_hashes::blake2b_512_address_payload(key.verification_key.as_ref()).as_bytes());
                (key, self.chain.bonds[i], key.verification_key.as_ref().to_vec(), payload)
            }
            Producer::Subject(j) => (subject_key(j), self.subjects[j].bond, subject_pubkey(j), subject_payload(j)),
        }
    }

    /// Ask the chain whether `p` may make one more claim of `class`, as its producer asks.
    fn ready(&self, p: Producer, class: Hash64) -> Result<(), String> {
        let (_, bond, pubkey, _) = self.producer_key(p);
        let facts = self.chain.ctx.consensus.palw_producer_facts_v2(class, Some(bond.0)).expect("a V2 network answers");
        match facts.ready_to_produce_v3(&pubkey, true) {
            Ok(()) => Ok(()),
            Err(why) => {
                let detail = facts.class_admission_refusal.clone().unwrap_or_default();
                let b = facts.bond.as_ref();
                Err(format!(
                    "{why}{}{} [committed {} + claim {} vs ceiling {}; share {:?}]",
                    if detail.is_empty() { "" } else { ": " },
                    detail,
                    b.map(|b| b.committed).unwrap_or(0),
                    b.map(|b| b.claim_exposure).unwrap_or(0),
                    b.map(|b| b.exposure_ceiling).unwrap_or(0),
                    facts.bond_class_share
                ))
            }
        }
    }

    /// An attempt block by `p` for `class`, built on the node's own template and inserted as the sink.
    /// Returns the claim id and whether the fold created the claim (a skipped own attempt leaves the
    /// block standing without one).
    async fn attempt(&mut self, p: Producer, class: Hash64, txs: Vec<Transaction>) -> (Hash64, bool) {
        use kaspa_consensus_core::palw_attempt_v2::{
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
            PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
        };
        let ttpb = self.chain.config.params.target_time_per_block();
        // Attempt blocks do not move testnet-12's clock; a small step keeps timestamps increasing.
        self.chain.ctx.simulated_time += ttpb / 50;
        self.nonce += 1;
        let (key, bond, pubkey, payload) = self.producer_key(p);
        let mut t = self
            .chain
            .ctx
            .consensus
            .build_block_template(
                MinerData::new(p2pkh_mldsa87_spk(payload.as_byte_slice()), vec![]),
                Box::new(super::OnetimeTxSelector::new(txs)),
                TemplateBuildMode::Standard,
            )
            .expect("a template");
        assert!(kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(t.block.header.pow_algo_id), "the attempt lane");
        stamp_harness_time(&self.chain.config.params, &mut t.block.header, self.chain.ctx.simulated_time);
        t.block.header.nonce = self.nonce;
        let facts = self.chain.ctx.consensus.palw_producer_facts_v2(class, Some(bond.0)).expect("facts");
        let bond_facts = facts.bond.as_ref().expect("a registered bond").clone();
        let header = &t.block.header;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let mut attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: self.network_domain,
            challenge: challenge_v2(self.network_domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
            class_id: facts.class_id,
            executor_bond: bond.0,
            executor_pubkey: pubkey,
            operator_id: bond_facts.operator_id,
            artifact_root: facts.artifact_root,
            trace_root: Hash64::default(),
            output_root: Hash64::from_u64_word(0x0CA0_0000_0000_0000 | self.nonce),
            execution_root: Hash64::from_u64_word(0xCA7E_0000_0000_0000 | self.nonce),
            pwu: facts.pwu,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        let anchor = execution_anchor_v3(self.network_domain, pre_pow, facts.class_id, &bond.0, header.nonce);
        let mut won = false;
        for draw in 0u64..4_000_000 {
            attempt.trace_root = Hash64::from_u64_word((self.nonce << 24) ^ draw ^ 0x7C00_0000_0000_0000);
            attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
            if class_ticket_v3(&attempt, anchor) <= facts.class_target {
                won = true;
                break;
            }
        }
        assert!(won, "the class lottery is winnable");
        let claim_id = attempt_id_v2(&attempt);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, claim_id.as_byte_slice(), PALW_ATTEMPT_V2_MLDSA87_CONTEXT, [0x5Bu8; 32])
            .expect("sign")
            .as_ref()
            .to_vec();
        t.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
        t.block.header.finalize();
        self.insert(t.block, &format!("{p:?}'s attempt")).await;
        let created = self.state().claim(&claim_id).is_some();
        (claim_id, created)
    }

    /// At most ten queued carriers: a block's transient storage mass (500,000) holds eleven 5-receipt
    /// licences (~41.9k each, measured: twelve were refused at 503,072).
    fn take_carriers(&mut self) -> Vec<Transaction> {
        // Measured: a 5-receipt coverage licence carrier is ~125.8k transient mass (size x 4), so a
        // block holds three; take by the block's transient-mass budget, not by count.
        let calc = kaspa_consensus_core::mass::MassCalculator::new_with_consensus_params(&self.chain.config.params);
        let budget = self.chain.config.params.max_block_mass.saturating_sub(20_000);
        let mut used = 0u64;
        let mut taken = Vec::new();
        while let Some(tx) = self.queued_carriers.first() {
            let m = calc.calc_non_contextual_masses(tx).transient_mass;
            if !self.logged_mass && !tx.payload.is_empty() {
                self.logged_mass = true;
                out_line(format!(
                    "{{\"run\":\"{}\",\"carrier_transient_mass\":{m},\"payload_bytes\":{},\"max_block_mass\":{}}}",
                    self.run,
                    tx.payload.len(),
                    self.chain.config.params.max_block_mass
                ));
            }
            if used + m > budget {
                break;
            }
            used += m;
            taken.push(self.queued_carriers.remove(0));
        }
        taken
    }

    /// Heartbeats until the DAA moves by one, carrying the queued carriers in the first.
    async fn advance_one_daa(&mut self) {
        let before = self.daa();
        let ttpb = self.chain.config.params.target_time_per_block();
        for _ in 0..6 {
            let txs = self.take_carriers();
            let block = self.chain.heartbeat(ttpb, txs).await;
            self.blocks += 1;
            let _ = block;
            if self.daa() > before {
                return;
            }
        }
        panic!("six heartbeats did not move the DAA past {before}");
    }

    /// A 0x4b carrier for `object`, funded by the first ready wallet entry; its change joins the wallet.
    fn carrier(&mut self, object: Obj) -> Option<Transaction> {
        use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        let idx = self.wallet.iter().position(|(_, _, _, ready)| *ready <= self.blocks)?;
        let (outpoint, entry, card, _) = self.wallet.remove(idx);
        let payload =
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
        let change = entry.amount - 400_000;
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(change, card_payout_spk(card))],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        sign_spend(&mut tx, entry, card, self.chain.config.params.storage_mass_parameter);
        let next = TransactionOutpoint::new(tx.id(), 0);
        // Spendable once the carrying block's transactions are accepted (the next chain block) — two
        // blocks of margin.
        self.wallet.push((next, UtxoEntry::new(change, card_payout_spk(card), 0, false), card, self.blocks + 3));
        Some(tx)
    }

    /// Split each card's genesis fee float into five, so a block can carry many licences.
    fn fan_out_floats(&mut self, floats: &[(TransactionOutpoint, UtxoEntry)]) -> Vec<Transaction> {
        let mut txs = Vec::new();
        for (card, (outpoint, entry)) in floats.iter().enumerate() {
            let part = (entry.amount - 1_000_000) / 5;
            let outputs = (0..5).map(|_| TransactionOutput::new(part, card_payout_spk(card))).collect();
            let mut tx = Transaction::new(
                crate::constants::TX_VERSION,
                vec![TransactionInput::new(*outpoint, vec![], 0, 1)],
                outputs,
                0,
                kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
                0,
                vec![],
            );
            sign_spend(&mut tx, entry.clone(), card, self.chain.config.params.storage_mass_parameter);
            for i in 0..5 {
                self.wallet.push((
                    TransactionOutpoint::new(tx.id(), i),
                    UtxoEntry::new(part, card_payout_spk(card), 0, false),
                    card,
                    self.blocks + 3,
                ));
            }
            txs.push(tx);
        }
        txs
    }

    /// The licence object for a bound claim under `policy`, signed now by the seats the chain drew.
    fn licence_object(&self, claim_id: Hash64, policy: LicencePolicy) -> Option<Obj> {
        let state = self.state();
        let panel = state.panel(&claim_id)?.clone();
        let signed_daa = self.daa();
        let vp = self.vp();
        let policy = match policy {
            LicencePolicy::Mix(a, q, _) => {
                let draw = (u64::from_le_bytes(claim_id.as_byte_slice()[..8].try_into().unwrap()) % 1000) as u16;
                if draw < a { LicencePolicy::All5 } else if draw < a + q { LicencePolicy::Quorum3 } else { LicencePolicy::S2 }
            }
            other => other,
        };
        match policy {
            LicencePolicy::None => None,
            LicencePolicy::Mix(..) => unreachable!(),
            LicencePolicy::S2 => {
                let assignment = palw_segment_assignment_v2(panel.anchor, claim_id, panel.seats.len() as u16);
                let full = assignment.full_seat as usize;
                let partial = if full == 0 { 1 } else { 0 };
                let receipts: Vec<PalwSeatReceiptV3> = [full, partial]
                    .iter()
                    .map(|&i| {
                        let seat = &panel.seats[i];
                        let card = self.chain.bonds.iter().position(|b| *b == seat.bond).expect("every seat is a genesis card");
                        let segments = assignment.mask_of(i as u16);
                        let message =
                            palw_receipt_message_v3(self.network_domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa, segments);
                        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                            &card_key(card).signing_key,
                            message.as_byte_slice(),
                            PALW_RECEIPT_V3_MLDSA87_CONTEXT,
                            [0x36u8; 32],
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
                vp.palw_v2_optimistic_assemble_impl(claim_id, &receipts)
            }
            LicencePolicy::All5 => {
                let assignment = palw_segment_assignment_v2(panel.anchor, claim_id, panel.seats.len() as u16);
                let receipts: Vec<PalwSeatReceiptV3> = panel
                    .seats
                    .iter()
                    .enumerate()
                    .map(|(i, seat)| {
                        let card = self.chain.bonds.iter().position(|b| *b == seat.bond).expect("every seat is a genesis card");
                        let segments = assignment.mask_of(i as u16);
                        let message =
                            palw_receipt_message_v3(self.network_domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa, segments);
                        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                            &card_key(card).signing_key,
                            message.as_byte_slice(),
                            PALW_RECEIPT_V3_MLDSA87_CONTEXT,
                            [0x34u8; 32],
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
                // The node's coverage assembler returns exactly this set, in this order, for a
                // five-Valid pool (`palw_select_coverage_licence_v2`: arrival order within a rank).
                // It clones the tip per candidate, which a run with thousands of claims cannot
                // afford per licence, so it is asked once per run and asserted equal.
                let object = Obj::ReceiptLicensedV2 { claim: claim_id, receipts: receipts.clone() };
                if !self.assembler_checked.get() {
                    self.assembler_checked.set(true);
                    let assembled = vp.palw_v2_receipt_coverage_assemble_impl(claim_id, &receipts);
                    assert_eq!(assembled.as_ref(), Some(&object), "the node's own coverage assembler builds the same licence");
                }
                Some(object)
            }
            LicencePolicy::Quorum3 => {
                let receipts: Vec<PalwSeatReceiptV2> = panel
                    .seats
                    .iter()
                    .take(3)
                    .map(|seat| {
                        let card = self.chain.bonds.iter().position(|b| *b == seat.bond).expect("every seat is a genesis card");
                        let message = palw_receipt_message_v2(self.network_domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
                        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                            &card_key(card).signing_key,
                            message.as_byte_slice(),
                            PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                            [0x35u8; 32],
                        )
                        .expect("sign")
                        .as_ref()
                        .to_vec();
                        PalwSeatReceiptV2 { claim: claim_id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: seat.bond, signed_daa, signature }
                    })
                    .collect();
                vp.palw_v2_receipt_quorum_assemble_impl(claim_id, &receipts)
            }
        }
    }

    /// Record every claim's transitions (accepted, bound, licensed, escrow released, final, void) at
    /// this tip, and queue receipts for newly bound claims.
    fn observe(&mut self) {
        let state = self.state();
        let now = self.daa();
        let tracked: Vec<Hash64> = self.life.keys().copied().collect();
        for claim_id in tracked {
            let Some(claim) = state.claim(&claim_id) else { continue };
            let entry = self.life.get_mut(&claim_id).unwrap();
            match &claim.phase {
                PalwClaimPhaseV2::PanelBound { bound_daa } => {
                    entry[1].get_or_insert(*bound_daa);
                }
                PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => {
                    entry[2].get_or_insert(*licensed_daa);
                }
                PalwClaimPhaseV2::Final { final_daa } => {
                    entry[4].get_or_insert(*final_daa);
                }
                PalwClaimPhaseV2::Voided { voided_daa, .. } => {
                    entry[5].get_or_insert(*voided_daa);
                }
                _ => {}
            }
            if let Some(panel) = state.panel(&claim_id) {
                entry[1].get_or_insert(panel.bound_daa);
            }
            if claim.rcore.escrow_released {
                entry[3].get_or_insert(now);
            }
            if entry[1].is_some() && !self.licensed_sent.contains(&claim_id) && !self.receipts_due.contains_key(&claim_id) {
                if matches!(claim.phase, PalwClaimPhaseV2::PanelBound { .. }) {
                    let delay = match &self.lic_delays {
                        Some(table) => table[(u64::from_le_bytes(claim_id.as_byte_slice()[8..16].try_into().unwrap()) as usize) % table.len()],
                        None => self.lic_delay,
                    };
                    self.receipts_due.insert(claim_id, now + delay);
                }
            }
        }
    }

    /// Sign and queue every licence that is due.
    fn send_due_licences(&mut self) {
        let now = self.daa();
        let due: Vec<Hash64> = self.receipts_due.iter().filter(|(_, d)| **d <= now).map(|(c, _)| *c).collect();
        let mut sent = 0;
        for claim_id in due {
            if sent >= 12 {
                break;
            }
            let object = match self.licence_object(claim_id, self.policy) {
                Some(o) => o,
                None => {
                    self.receipts_due.remove(&claim_id);
                    self.licensed_sent.insert(claim_id);
                    continue;
                }
            };
            match self.carrier(object) {
                Some(tx) => {
                    self.queued_carriers.push(tx);
                    self.receipts_due.remove(&claim_id);
                    self.licensed_sent.insert(claim_id);
                    sent += 1;
                }
                None => break,
            }
        }
    }

    /// Plant subject bonds (and, for the 8k row, readiness on every card and an admitting state).
    fn plant(&mut self, subjects: &[(String, u64)], k8: bool) {
        let vp = self.vp();
        let bundle = self.chain.bundle.clone();
        let (sink, tip) = self.chain.tip_state();
        let now = self.daa();
        let mut carriage = PalwStateCarriageV2::from_state(&tip);
        for (j, (label, msk)) in subjects.iter().enumerate() {
            let bond = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xCA9A_C170_0000_0000 | j as u64), 0));
            let operator_id = Hash64::from_u64_word(0x0FE7_A70E_0000_0000 | j as u64);
            carriage.bonds.insert(
                bond,
                PalwBondStateV2 {
                    pubkey: subject_pubkey(j),
                    operator_id,
                    collateral: msk * SOMPI,
                    slashed: 0,
                    status: PalwBondStatusV2::Active,
                    registered_daa: now,
                    payout_payload: subject_payload(j),
                    capable_classes: BTreeSet::new(),
                },
            );
            self.subjects.push(Subject {
                label: label.clone(),
                bond,
                collateral: msk * SOMPI,
                claims: BTreeSet::new(),
                accepted_total: 0,
                skipped_total: 0,
            });
        }
        if k8 {
            let k8_class = self.k8_class.expect("an 8k row");
            if let Some(row) = carriage.model_lifecycles.get_mut(&k8_class) {
                if !row.state.admits_claims() {
                    row.state = PalwModelLifecycleV1::Probation { probes_passed: 0 };
                }
            }
            for card in &self.chain.bonds {
                carriage
                    .seat_readiness
                    .insert((*card, k8_class), PalwSeatReadinessRowV1 { proved_daa: now, proved_span: now, leaf_index: 0, proof_version: 2, chunks: 8 });
            }
        }
        let planted: PalwChainStateV2 = carriage.into_state(&bundle.state, None).expect("the planted tip is a consistent state");
        vp.palw_state_v2_store.write().set_tip_for_tests(sink, &planted).expect("the planted tip becomes the tip");
    }

    /// Refresh the 8k readiness rows (the harness cannot run the real proof).
    fn refresh_readiness(&mut self) {
        let Some(k8_class) = self.k8_class else { return };
        let vp = self.vp();
        let bundle = self.chain.bundle.clone();
        let (sink, tip) = self.chain.tip_state();
        let now = self.daa();
        let mut carriage = PalwStateCarriageV2::from_state(&tip);
        for card in &self.chain.bonds {
            carriage
                .seat_readiness
                .insert((*card, k8_class), PalwSeatReadinessRowV1 { proved_daa: now, proved_span: now, leaf_index: 0, proof_version: 2, chunks: 8 });
        }
        if let Some(row) = carriage.model_lifecycles.get_mut(&k8_class) {
            if !row.state.admits_claims() {
                row.state = PalwModelLifecycleV1::Probation { probes_passed: 0 };
            }
        }
        let planted: PalwChainStateV2 = carriage.into_state(&bundle.state, None).expect("consistent");
        vp.palw_state_v2_store.write().set_tip_for_tests(sink, &planted).expect("the refresh becomes the tip");
    }

    /// One JSON line per subject for this DAA.
    fn report(&self, rel: u64, accepted_now: &[u64], why: &[String]) {
        let state = self.state();
        for (j, s) in self.subjects.iter().enumerate() {
            let mut by_phase = BTreeMap::<&str, u64>::new();
            let mut escrow_held_licensed = 0u64;
            let mut unlicensed_counted = 0u64;
            for c in &s.claims {
                let Some(claim) = state.claim(c) else {
                    *by_phase.entry("retired").or_default() += 1;
                    continue;
                };
                let tag = match &claim.phase {
                    PalwClaimPhaseV2::Provisional => "prov",
                    PalwClaimPhaseV2::PanelBound { .. } => "bound",
                    PalwClaimPhaseV2::ReceiptLicensed { .. } => "lic",
                    PalwClaimPhaseV2::Final { .. } => "final",
                    PalwClaimPhaseV2::Voided { .. } => "void",
                    _ => "other",
                };
                *by_phase.entry(tag).or_default() += 1;
                if matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) && !claim.rcore.escrow_released {
                    escrow_held_licensed += 1;
                }
                if !claim.phase.is_terminal() && !kaspa_consensus_core::palw_state_v2::palw_rcore_counts_licensed_v1(claim) {
                    unlicensed_counted += 1;
                }
            }
            let facts = self.chain.ctx.consensus.palw_producer_facts_v2(self.class_id, Some(s.bond.0)).expect("facts");
            let b = facts.bond.as_ref().unwrap();
            out_line(format!(
                "{{\"run\":\"{}\",\"subject\":\"{}\",\"collateral_msk\":{},\"rel\":{},\"daa\":{},\"accepted_now\":{},\"accepted_cum\":{},\"skipped_cum\":{},\"phases\":{},\"escrow_held_licensed\":{},\"unlicensed_t2c\":{},\"committed\":{},\"claim_exposure\":{},\"ceiling\":{},\"share\":{},\"hold\":{}}}",
                self.run,
                s.label,
                s.collateral / SOMPI,
                rel,
                self.daa(),
                accepted_now[j],
                s.accepted_total,
                s.skipped_total,
                serde_json_like(&by_phase),
                escrow_held_licensed,
                unlicensed_counted,
                b.committed,
                b.claim_exposure,
                b.exposure_ceiling,
                facts.bond_class_share.map(|(a, b)| format!("[{a},{b}]")).unwrap_or_else(|| "null".to_string()),
                json_str(&why[j])
            ));
        }
    }

    /// The eight cards' (seats') committed ledgers, one line per DAA: the panel side's capital.
    fn report_seats(&self, rel: u64) {
        let mut parts = Vec::new();
        let st = self.state();
        let now = self.daa();
        for (i, card) in self.chain.bonds.iter().enumerate() {
            let facts = self.chain.ctx.consensus.palw_producer_facts_v2(self.base_class, Some(card.0)).expect("facts");
            let b = facts.bond.as_ref().unwrap();
            // Decomposition: own claims' commitments, duty rows, lock excess over duty.
            let own: u128 = st
                .claims_iter()
                .filter(|(_, c)| c.bond == *card)
                .map(|(_, c)| kaspa_consensus_core::palw_state_v2::palw_claim_commitment_v1(&self.chain.bundle.state, c, now).unwrap_or(0))
                .sum();
            let own_held = st
                .claims_iter()
                .filter(|(_, c)| c.bond == *card && matches!(c.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) && !c.rcore.escrow_released)
                .count();
            let (duties, duty_rows): (u128, usize) = st
                .panel_duty_rows_iter()
                .filter(|(_, row)| row.seats.contains_key(card))
                .fold((0, 0), |(a, n), (_, row)| (a + row.seat_exposure, n + 1));
            parts.push(format!("[{i},{},{},{own},{own_held},{duties},{duty_rows}]", b.committed, b.exposure_ceiling));
        }
        let live = st.claims_iter().filter(|(_, c)| !c.phase.is_terminal()).count();
        out_line(format!(
            "{{\"run\":\"{}\",\"seats\":1,\"rel\":{rel},\"daa\":{},\"live_claims\":{live},\"cards\":[{}]}}",
            self.run,
            self.daa(),
            parts.join(",")
        ));
    }

    /// Latencies of every subject claim, as one JSON line each.
    fn report_lives(&self) {
        for s in &self.subjects {
            for c in &s.claims {
                let l = self.life.get(c).copied().unwrap_or_default();
                out_line(format!(
                    "{{\"run\":\"{}\",\"life\":\"{}\",\"subject\":\"{}\",\"accepted\":{},\"bound\":{},\"licensed\":{},\"released\":{},\"final\":{},\"void\":{}}}",
                    self.run, c, s.label, json_opt(l[0]), json_opt(l[1]), json_opt(l[2]), json_opt(l[3]), json_opt(l[4]), json_opt(l[5])
                ));
            }
        }
    }
}

fn serde_json_like(m: &BTreeMap<&str, u64>) -> String {
    let parts: Vec<String> = m.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
    format!("{{{}}}", parts.join(","))
}

fn json_opt(v: Option<u64>) -> String {
    v.map(|x| x.to_string()).unwrap_or_else(|| "null".to_string())
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// testnet-12 with harness cards, heartbeats past the registry grace, the floats fanned out.
pub(super) async fn sim(run: &str, class: &str, subjects: &[(String, u64)], policy: LicencePolicy, lic_delay: u64) -> CapSim {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let ttpb = config.params.target_time_per_block();
    let base_class = bundle.base_class_id;
    let (_, genesis_state) = chain.tip_state();
    let k8_class = genesis_state
        .model_lifecycles_iter()
        .find(|(id, row)| **id != base_class && !kaspa_consensus_core::palw_work_target_v1::palw_panel_held_to_final_v1(row))
        .map(|(id, _)| *id);
    let class_id = match class {
        "floor" => base_class,
        "8k" => k8_class.expect("testnet-12's 8k row"),
        other => panic!("unknown class {other}"),
    };
    chain.heartbeat(ttpb, Vec::new()).await;
    let mut s = CapSim {
        chain,
        network_domain,
        nonce: 1 << 40,
        class_id,
        base_class,
        k8_class,
        subjects: Vec::new(),
        wallet: Vec::new(),
        blocks: 0,
        queued_carriers: Vec::new(),
        receipts_due: BTreeMap::new(),
        licensed_sent: BTreeSet::new(),
        background_card: 0,
        policy,
        lic_delay,
        life: BTreeMap::new(),
        run: run.to_string(),
        logged_mass: false,
        assembler_checked: std::cell::Cell::new(false),
        lic_delays: None,
    };
    let fan = s.fan_out_floats(&floats);
    s.queued_carriers = fan;
    let past_grace: u64 = env_or("CAP_START_DAA", 34);
    while s.daa() < past_grace {
        s.advance_one_daa().await;
    }
    s.plant(subjects, class == "8k");
    s
}

/// The measurement loop: `daa_len` DAA; at each, one background floor attempt (the anchor source),
/// then every subject makes claims of the class under test until its producer facts hold it (at
/// most `max_per_daa`), then licences are signed and queued.
pub(super) async fn drive(s: &mut CapSim, daa_len: u64, max_per_daa: u64) {
    let start = s.daa();
    let started = std::time::Instant::now();
    let k8 = s.k8_class == Some(s.class_id);
    for rel in 0..=daa_len {
        if k8 && rel > 0 && rel % 16 == 0 {
            s.refresh_readiness();
        }
        // The anchor source: one floor attempt by a rotating genesis card.
        let card = s.background_card % 8;
        s.background_card += 1;
        if s.ready(Producer::Card(card), s.base_class).is_ok() {
            let txs = s.take_carriers();
            let (claim_id, created) = s.attempt(Producer::Card(card), s.base_class, txs).await;
            if created {
                s.life.insert(claim_id, [Some(s.daa()), None, None, None, None, None]);
            }
        }
        let mut accepted_now = vec![0u64; s.subjects.len()];
        let mut why = vec![String::new(); s.subjects.len()];
        for j in 0..s.subjects.len() {
            for _ in 0..max_per_daa {
                match s.ready(Producer::Subject(j), s.class_id) {
                    Ok(()) => {
                        let txs = s.take_carriers();
                        let (claim_id, created) = s.attempt(Producer::Subject(j), s.class_id, txs).await;
                        if created {
                            s.subjects[j].claims.insert(claim_id);
                            s.subjects[j].accepted_total += 1;
                            accepted_now[j] += 1;
                            s.life.insert(claim_id, [Some(s.daa()), None, None, None, None, None]);
                        } else {
                            s.subjects[j].skipped_total += 1;
                            why[j] = "fold skipped an attempt the facts admitted".to_string();
                            break;
                        }
                    }
                    Err(reason) => {
                        why[j] = reason;
                        break;
                    }
                }
            }
        }
        s.observe();
        s.send_due_licences();
        s.report(rel, &accepted_now, &why);
        if rel % 5 == 0 {
            s.report_seats(rel);
        }
        s.advance_one_daa().await;
        s.observe();
        if rel % 20 == 0 {
            eprintln!("[cap] {} rel {rel} daa {} blocks {} elapsed {:?}", s.run, s.daa(), s.blocks, started.elapsed());
        }
    }
    let _ = start;
    s.report_lives();
}

/// A histogram (delay, count) as a table of delays, the harness's one-DAA carriage taken off.
pub(super) fn expand(hist: &[(u64, usize)]) -> Vec<u64> {
    hist.iter().flat_map(|(d, n)| std::iter::repeat(d.saturating_sub(1)).take(*n)).collect()
}

fn subjects_from_env() -> Vec<(String, u64)> {
    let spec = std::env::var("CAP_BONDS").unwrap_or_else(|_| "13000".to_string());
    spec.split(',').map(|b| (format!("{b}"), b.parse::<u64>().expect("a bond in MSK"))).collect()
}

fn policy_from_env() -> LicencePolicy {
    match std::env::var("CAP_POLICY").unwrap_or_else(|_| "all5".to_string()).as_str() {
        "all5" => LicencePolicy::All5,
        "quorum3" => LicencePolicy::Quorum3,
        "none" => LicencePolicy::None,
        "s2" => LicencePolicy::S2,
        other if other.starts_with("mix:") => {
            let v: Vec<u16> = other[4..].split(',').map(|x| x.parse().expect("permille")).collect();
            LicencePolicy::Mix(v[0], v[1], v[2])
        }
        other => panic!("unknown policy {other}"),
    }
}

/// **The capacity run the environment names**: `CAP_CLASS` (floor | 8k), `CAP_BONDS` (MSK, comma
/// separated — each a planted subject on the SAME chain), `CAP_POLICY`, `CAP_LIC_DELAY`,
/// `CAP_DAA` (length), `CAP_MAX_PER_DAA`, `CAP_RUN` (label), `CAP_OUT` (JSON lines).
#[tokio::test]
#[ignore = "a measurement run, minutes to an hour; steer it with CAP_* and run with --ignored"]
async fn t12_capacity_run() {
    let class = std::env::var("CAP_CLASS").unwrap_or_else(|_| "floor".to_string());
    let run = std::env::var("CAP_RUN").unwrap_or_else(|_| format!("{class}-run"));
    let subjects = subjects_from_env();
    let policy = policy_from_env();
    let lic_spec = std::env::var("CAP_LIC_DELAY").unwrap_or_else(|_| "0".to_string());
    let lic_delay: u64 = lic_spec.parse().unwrap_or(0);
    let daa_len: u64 = env_or("CAP_DAA", 60);
    let max_per_daa: u64 = env_or("CAP_MAX_PER_DAA", 64);
    let mut s = sim(&run, &class, &subjects, policy, lic_delay).await;
    // Live public testnet-12 bind -> licence delays (DAA 140-170, 309 floor / 11 8k licences),
    // less the harness's own carriage DAA (a licence lands the DAA after it is signed).
    s.lic_delays = match lic_spec.as_str() {
        "live:floor" => Some(expand(&[(0, 66), (1, 118), (2, 50), (3, 11), (4, 14), (5, 11), (6, 4), (7, 4), (8, 2), (9, 4), (10, 4), (11, 3), (13, 7), (14, 2), (16, 1), (21, 1), (24, 1), (25, 1), (32, 1), (39, 1), (51, 1), (57, 1), (66, 1)])),
        "live:8k" => Some(expand(&[(2, 5), (3, 3), (4, 1), (7, 1), (8, 1)])),
        _ => None,
    };
    // The parameters every row reads, once.
    {
        let bundle = &s.chain.bundle;
        let st = s.state();
        out_line(format!(
            "{{\"run\":\"{run}\",\"params\":1,\"anchor_delay\":{},\"window_bind\":{},\"window_receipt\":{},\"window_challenge_at_start\":{},\"window_court\":{},\"seat_count\":{},\"quorum\":{},\"exposure_ratio_permille\":{},\"min_collateral\":{},\"start_daa\":{}}}",
            bundle.panel.anchor_delay(),
            bundle.state.window_bind(),
            bundle.state.window_receipt(),
            bundle.state.window_challenge_at(s.daa()),
            bundle.state.window_court(),
            bundle.panel.seat_count(),
            bundle.panel.quorum(),
            bundle.state.fp_max_exposure_ratio_permille(),
            bundle.state.min_collateral_sompi(),
            s.daa()
        ));
        for (id, row) in st.model_lifecycles_iter() {
            out_line(format!(
                "{{\"run\":\"{run}\",\"class_row\":\"{id}\",\"state\":\"{:?}\",\"verification_ccu\":{},\"economic_ccu\":{},\"window_spans\":{},\"max_inflight\":{},\"required_ready\":{}}}",
                row.state,
                row.work.verification_ccu,
                row.work.economic_ccu_per_claim,
                row.profile.verification_window_spans,
                row.profile.max_inflight_claims,
                row.profile.required_ready_seats
            ));
        }
    }
    drive(&mut s, daa_len, max_per_daa).await;
}

// ---------------------------------------------------------------------------------------------------
// The instantaneous sweep: the chain's own gates (producer facts = the fold's reads) on planted tips.
// ---------------------------------------------------------------------------------------------------

/// A planted claim of `class` by `bond`, `Provisional`, with the reservation the fold would record
/// (`reserved`, `escrowed_reward` copied from a real claim of the class or given).
fn planted_claim(
    class: Hash64,
    bond: PalwBondKeyV2,
    now: u64,
    accepted_block: kaspa_consensus_core::BlockHash,
    i: u64,
    reserved: u128,
    escrowed_reward: u64,
) -> kaspa_consensus_core::palw_state_v2::PalwClaimStateV2 {
    kaspa_consensus_core::palw_state_v2::PalwClaimStateV2 {
        source: kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2::Attempt,
        class_id: class,
        bond,
        pwu: 1,
        accepted_daa: now,
        rebound_daa: None,
        accepted_blue_score: now,
        accepted_block,
        trace_root: Hash64::from_u64_word(0x7100_0000 + i),
        output_root: Hash64::from_u64_word(0x7200_0000 + i),
        execution_root: Hash64::from_u64_word(0x7300_0000 + i),
        trace_chunk_count: 4,
        trace_retention_daa: 999_999,
        reserved,
        immature_contribution: 0,
        escrowed_reward,
        work_leaves: 0,
        work_id: None,
        phase: PalwClaimPhaseV2::Provisional,
        rights_reserved: 0,
        job_identity: Hash64::default(),
        rcore: Default::default(),
    }
}

impl CapSim {
    /// Plant, on the CURRENT tip, `claims` extra Provisional claims per (class, bond, reserved, escrow),
    /// then ask the facts for `ask_class` / `ask_bond`. Restores the tip afterwards.
    fn with_planted<R>(
        &mut self,
        claims: &[(Hash64, PalwBondKeyV2, u64, u128, u64)],
        extra_rows: &[(Hash64, kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleRowV1)],
        read: impl FnOnce(&Self) -> R,
    ) -> R {
        let vp = self.vp();
        let bundle = self.chain.bundle.clone();
        let (sink, tip) = self.chain.tip_state();
        let now = self.daa();
        let mut carriage = PalwStateCarriageV2::from_state(&tip);
        let accepted_block = tip.last_point().map(|p| p.block).unwrap_or(sink);
        for (id, row) in extra_rows {
            carriage.model_lifecycles.insert(*id, row.clone());
            for card in &self.chain.bonds {
                carriage.seat_readiness.insert(
                    (*card, *id),
                    PalwSeatReadinessRowV1 { proved_daa: now, proved_span: now, leaf_index: 0, proof_version: 2, chunks: 8 },
                );
            }
        }
        let mut i = 0u64;
        for (class, bond, n, reserved, escrow) in claims {
            for _ in 0..*n {
                i += 1;
                let claim = planted_claim(*class, *bond, now, accepted_block, i, *reserved, *escrow);
                let held = kaspa_consensus_core::palw_state_v2::palw_claim_bond_reservation_v1(&bundle.state, &claim).expect("a reservation");
                *carriage.reserved_exposure.entry(*bond).or_insert(0) += held;
                carriage.claims.insert(Hash64::from_u64_word(0x7F00_0000_0000 + i), claim);
            }
        }
        let planted: PalwChainStateV2 = carriage.into_state(&bundle.state, None).expect("the planted tip is consistent");
        vp.palw_state_v2_store.write().set_tip_for_tests(sink, &planted).expect("plant");
        let r = read(self);
        vp.palw_state_v2_store.write().set_tip_for_tests(sink, &tip).expect("restore");
        r
    }

    fn verdict(&self, class: Hash64, j: usize) -> (bool, String, Option<(u64, u32)>) {
        let facts = self.chain.ctx.consensus.palw_producer_facts_v2(class, Some(self.subjects[j].bond.0)).expect("facts");
        let v = facts.ready_to_produce_v3(&subject_pubkey(j), true);
        let why = match &v {
            Ok(()) => String::new(),
            Err(w) => format!("{w} {}", facts.class_admission_refusal.clone().unwrap_or_default()),
        };
        (v.is_ok(), why, facts.bond_class_share)
    }
}

/// **N_instant by the chain's own gates**: for each class and bond, the largest number of
/// unlicensed claims the subject can hold with the producer facts still admitting one more, and the
/// refusal that stops it; then cross-model interference (other classes' owed claims) and the split.
#[tokio::test]
#[ignore = "a measurement run; run with --ignored"]
async fn t12_capacity_instant_sweep() {
    let bonds: Vec<u64> = vec![13_000, 20_000, 26_000, 50_000, 100_000, 250_000, 500_000, 1_000_000];
    let subjects: Vec<(String, u64)> = bonds.iter().map(|b| (format!("{b}"), *b)).collect();
    let mut s = sim("instant", "8k", &subjects, LicencePolicy::All5, 0).await;
    let base = s.base_class;
    let k8 = s.k8_class.expect("8k");
    let (_, st) = s.chain.tip_state();
    let two_m = st
        .model_lifecycles_iter()
        .find(|(id, row)| **id != base && kaspa_consensus_core::palw_work_target_v1::palw_panel_held_to_final_v1(row))
        .map(|(id, _)| *id)
        .expect("the 2M row");
    // One real claim of each admitting class, by card 0, for the reservation the fold records.
    let (floor_claim, _) = s.attempt(Producer::Card(0), base, Vec::new()).await;
    let (k8_claim, _) = s.attempt(Producer::Card(0), k8, Vec::new()).await;
    let st = s.state();
    let fc = st.claim(&floor_claim).expect("a floor claim").clone();
    let kc = st.claim(&k8_claim).expect("an 8k claim").clone();
    out_line(format!(
        "{{\"run\":\"instant\",\"template\":\"floor\",\"reserved\":{},\"escrowed_reward\":{},\"commitment\":{}}}",
        fc.reserved,
        fc.escrowed_reward,
        kaspa_consensus_core::palw_state_v2::palw_claim_bond_reservation_v1(&s.chain.bundle.state, &fc).unwrap()
    ));
    out_line(format!(
        "{{\"run\":\"instant\",\"template\":\"8k\",\"reserved\":{},\"escrowed_reward\":{},\"commitment\":{}}}",
        kc.reserved,
        kc.escrowed_reward,
        kaspa_consensus_core::palw_state_v2::palw_claim_bond_reservation_v1(&s.chain.bundle.state, &kc).unwrap()
    ));
    let facts_2m = s.chain.ctx.consensus.palw_producer_facts_v2(two_m, Some(s.subjects[7].bond.0)).expect("facts");
    let e = fc.escrowed_reward;
    let w_2m = facts_2m.bond.as_ref().unwrap().claim_exposure - e as u128;
    out_line(format!(
        "{{\"run\":\"instant\",\"template\":\"2M\",\"claim_exposure\":{},\"reserved_derived\":{},\"refusal\":{}}}",
        facts_2m.bond.as_ref().unwrap().claim_exposure,
        w_2m,
        json_str(&facts_2m.class_admission_refusal.clone().unwrap_or_default())
    ));
    // Measure on a FRESH chain: the template claims above would sit in the 8k room.
    drop(s);
    let mut s = sim("instant", "8k", &subjects, LicencePolicy::All5, 0).await;
    let classes: Vec<(&str, Hash64, u128, u64)> =
        vec![("floor", base, fc.reserved, fc.escrowed_reward), ("8k", k8, kc.reserved, kc.escrowed_reward), ("2M", two_m, w_2m, e)];
    for (name, class, reserved, escrow) in &classes {
        for j in 0..s.subjects.len() {
            let bond = s.subjects[j].bond;
            let mut n = 0u64;
            let (ok0, why0, share0) = s.with_planted(&[], &[], |s| s.verdict(*class, j));
            let mut last_why = why0.clone();
            let mut share = share0;
            if ok0 {
                loop {
                    n += 1;
                    let (ok, why, sh) = s.with_planted(&[(*class, bond, n, *reserved, *escrow)], &[], |s| s.verdict(*class, j));
                    share = sh;
                    if !ok {
                        last_why = why;
                        break;
                    }
                    if n > 400 {
                        last_why = "stopped at 400".into();
                        break;
                    }
                }
            }
            out_line(format!(
                "{{\"run\":\"instant\",\"class\":\"{name}\",\"bond_msk\":{},\"n_instant\":{n},\"share\":{},\"stop\":{}}}",
                s.subjects[j].collateral / SOMPI,
                share.map(|(a, b)| format!("[{a},{b}]")).unwrap_or_else(|| "null".into()),
                json_str(&last_why)
            ));
        }
    }
    // Cross-model: the 13k and 100k subjects' 8k capacity with other classes' owed claims planted
    // (the filler is card 7, a 939k genesis bond): n 8k claims by card 7 (the same class), a 2M
    // claim (C7, owed to Final), and synthetic rows B (an 8k clone) and H (half the 8k window cost).
    let filler = s.chain.bonds[7];
    let k8_row = s.state().model_lifecycle(&k8).expect("8k row").clone();
    let mut b_row = k8_row.clone();
    b_row.state = PalwModelLifecycleV1::Active;
    let b_id = Hash64::from_u64_word(0xB0B0_0000_0000_0001);
    let mut h_row = k8_row.clone();
    h_row.state = PalwModelLifecycleV1::Active;
    h_row.work.economic_ccu_per_claim /= 2;
    h_row.work.verification_ccu /= 2;
    let h_id = Hash64::from_u64_word(0xB0B0_0000_0000_0002);
    let mut big_row = k8_row.clone();
    big_row.state = PalwModelLifecycleV1::Active;
    big_row.work.economic_ccu_per_claim *= 4;
    big_row.work.verification_ccu *= 4;
    big_row.profile.verification_window_spans = 7;
    let big_id = Hash64::from_u64_word(0xB0B0_0000_0000_0003);
    let rows = vec![(b_id, b_row), (h_id, h_row), (big_id, big_row)];
    let scenarios: Vec<(&str, Vec<(Hash64, PalwBondKeyV2, u64, u128, u64)>)> = vec![
        ("8k-alone", vec![]),
        ("8k+card7-8k-x1", vec![(k8, filler, 1, kc.reserved, kc.escrowed_reward)]),
        ("8k+card7-8k-x2", vec![(k8, filler, 2, kc.reserved, kc.escrowed_reward)]),
        ("8k+card7-8k-x3", vec![(k8, filler, 3, kc.reserved, kc.escrowed_reward)]),
        ("8k+2M-x1", vec![(two_m, filler, 1, w_2m, e)]),
        ("8k+B(8k-clone)-x1", vec![(b_id, filler, 1, kc.reserved, kc.escrowed_reward)]),
        ("8k+B(8k-clone)-x2", vec![(b_id, filler, 2, kc.reserved, kc.escrowed_reward)]),
        ("8k+B-x1+H(half)-x2", vec![(b_id, filler, 1, kc.reserved, kc.escrowed_reward), (h_id, filler, 2, kc.reserved, kc.escrowed_reward)]),
        ("8k+BIG(4x,window7)-x1", vec![(big_id, filler, 1, kc.reserved, kc.escrowed_reward)]),
    ];
    for (label, others) in &scenarios {
        for j in [0usize, 4] {
            let bond = s.subjects[j].bond;
            let mut n = 0u64;
            let last_why;
            loop {
                let mut planted = others.clone();
                if n > 0 {
                    planted.push((k8, bond, n, kc.reserved, kc.escrowed_reward));
                }
                let (ok, why, _) = s.with_planted(&planted, &rows, |s| s.verdict(k8, j));
                if !ok {
                    last_why = why;
                    break;
                }
                n += 1;
                if n > 50 {
                    last_why = "stopped at 50".into();
                    break;
                }
            }
            out_line(format!(
                "{{\"run\":\"instant\",\"interference\":\"{label}\",\"bond_msk\":{},\"n_instant_8k\":{n},\"stop\":{}}}",
                s.subjects[j].collateral / SOMPI,
                json_str(&last_why)
            ));
        }
    }
    // The split: 26,000 MSK as one bond (subject 2) vs two 13,000 bonds (subjects 0 and 1 — 13k and
    // 20k here, so the second is read at 13k-equivalent by planting its first two claims): how many 8k
    // claims the pair holds at once, the room shared.
    for (label, j_first, j_second) in [("13k+20k", 0usize, 1usize), ("13k+26k", 0, 2), ("100k+250k", 4, 5)] {
        let b1 = s.subjects[j_first].bond;
        let b2 = s.subjects[j_second].bond;
        let mut n1 = 0u64;
        loop {
            let planted = if n1 > 0 { vec![(k8, b1, n1, kc.reserved, kc.escrowed_reward)] } else { vec![] };
            let (ok, _, _) = s.with_planted(&planted, &rows, |s| s.verdict(k8, j_first));
            if !ok || n1 > 50 {
                break;
            }
            n1 += 1;
        }
        let mut n2 = 0u64;
        let why2;
        loop {
            let mut planted = vec![(k8, b1, n1, kc.reserved, kc.escrowed_reward)];
            if n2 > 0 {
                planted.push((k8, b2, n2, kc.reserved, kc.escrowed_reward));
            }
            let (ok, why, _) = s.with_planted(&planted, &rows, |s| s.verdict(k8, j_second));
            if !ok {
                why2 = why;
                break;
            }
            n2 += 1;
            if n2 > 50 {
                why2 = "stopped".into();
                break;
            }
        }
        out_line(format!(
            "{{\"run\":\"instant\",\"split\":\"{label}\",\"first\":{n1},\"second_after_first\":{n2},\"stop\":{}}}",
            json_str(&why2)
        ));
    }
}

// ---------------------------------------------------------------------------------------------------
// The 2M collateral decomposition and the 1/10 – 1/100 study (coordinator's added scope, 2026-09-26).
// ---------------------------------------------------------------------------------------------------

/// How many claims of `class` the subject `j` may hold at once by the EXPOSURE gate alone
/// (`PalwProducerBondFactsV2::has_committed_room`, the fold's `apply_attempt` / admission item 8
/// arithmetic), whatever the class's lifecycle says: planted Provisional claims at the reservation the
/// facts price (`claim_exposure`), counted until the room refuses.
fn exposure_only_n(s: &mut CapSim, class: Hash64, j: usize, claim_exposure: u128, escrow: u64, extra_rows: &[(Hash64, kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleRowV1)]) -> u64 {
    let bond = s.subjects[j].bond;
    let reserved = claim_exposure.saturating_sub(escrow as u128);
    let mut n = 0u64;
    loop {
        let planted = if n > 0 { vec![(class, bond, n, reserved, escrow)] } else { vec![] };
        let ok = s.with_planted(&planted, extra_rows, |s| {
            let facts = s.chain.ctx.consensus.palw_producer_facts_v2(class, Some(bond.0)).expect("facts");
            facts.bond.as_ref().map(|b| b.has_committed_room()).unwrap_or(false)
        });
        if !ok || n > 2_000 {
            return n;
        }
        n += 1;
    }
}

/// **Each class's per-claim collateral, term by term, on the shipped release; then the same with the
/// class's PWU (its registry work, the draw every exposure and weight is priced on) cut to 1/10 and
/// 1/100, and with `slash_value_per_pwu` cut to 1 (1/5, the smallest non-zero value an integer sompi
/// allows) on every class.** Each variant is a planted tip, read through the producer facts (the
/// fold's own exposure and room arithmetic).
#[tokio::test]
#[ignore = "a measurement run; run with --ignored"]
async fn t12_capacity_collateral_study() {
    let bonds: Vec<u64> = vec![13_000, 26_000, 100_000, 1_000_000];
    let subjects: Vec<(String, u64)> = bonds.iter().map(|b| (format!("{b}"), *b)).collect();
    let mut s = sim("study", "8k", &subjects, LicencePolicy::All5, 0).await;
    let base = s.base_class;
    let k8 = s.k8_class.expect("8k");
    let st = s.state();
    let two_m = st
        .model_lifecycles_iter()
        .find(|(id, row)| **id != base && kaspa_consensus_core::palw_work_target_v1::palw_panel_held_to_final_v1(row))
        .map(|(id, _)| *id)
        .expect("the 2M row");
    let daa = s.daa();
    let subsidy = s.vp().coinbase_manager.calc_block_subsidy(daa);
    let carve = s.chain.bundle.state.worker_carve_permille();
    let escrow = s.chain.bundle.state.worker_carve_at(subsidy, None);
    let beta = s.chain.bundle.state.beta_permille();
    let base_declared = match st.class(&base).expect("floor").pwu_rule {
        kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => pwu_per_inference,
        kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(cap) => cap,
    };
    let base_canonical = st.model_lifecycle(&base).map(|r| r.work.economic_ccu_per_claim).unwrap_or(0);
    out_line(format!(
        "{{\"run\":\"study\",\"globals\":1,\"daa\":{daa},\"subsidy\":{subsidy},\"worker_carve_permille\":{carve},\"escrow\":{escrow},\"beta_permille\":{beta},\"base_declared\":{base_declared},\"base_canonical\":{base_canonical}}}"
    ));
    for (name, class) in [("floor", base), ("8k", k8), ("2M", two_m)] {
        let cs = st.class(&class).expect("a class").clone();
        let row = st.model_lifecycle(&class).cloned();
        let facts = s.chain.ctx.consensus.palw_producer_facts_v2(class, Some(s.subjects[0].bond.0)).expect("facts");
        let b = facts.bond.as_ref().unwrap();
        let eccu = row.as_ref().map(|r| r.work.economic_ccu_per_claim).unwrap_or(0);
        let attempts = kaspa_consensus_core::palw_pwu::palw_claim_attempts_v1(facts.pwu, (eccu > 0).then_some(eccu.min(u64::MAX as u128) as u64));
        let exposure_pwu = if base_canonical > 0 { eccu * base_declared as u128 / base_canonical } else { 0 };
        out_line(format!(
            "{{\"run\":\"study\",\"decompose\":\"{name}\",\"slash_value_per_pwu\":{},\"pwu_rule\":\"{:?}\",\"economic_ccu\":{eccu},\"verification_ccu\":{},\"claim_pwu\":{},\"attempts\":{attempts},\"exposure_pwu\":{exposure_pwu},\"w\":{},\"escrow\":{escrow},\"claim_exposure\":{},\"immature_contribution\":{},\"refusal\":{}}}",
            cs.slash_value_per_pwu,
            cs.pwu_rule,
            row.as_ref().map(|r| r.work.verification_ccu).unwrap_or(0),
            facts.pwu,
            b.claim_exposure.saturating_sub(escrow as u128),
            b.claim_exposure,
            kaspa_consensus_core::palw_state_v2::immature_contribution_v2(&s.chain.bundle.state, facts.pwu),
            json_str(&facts.class_admission_refusal.clone().unwrap_or_default())
        ));
    }
    // Variants. PWU: the class's registry work (economic and verification CCU) / f — every exposure,
    // weight, credit and replay-cost reader follows it. SVP: every class's slash_value_per_pwu = 1.
    let variants: Vec<(&str, u128, Option<u64>)> = vec![("shipped", 1, None), ("pwu/10", 10, None), ("pwu/100", 100, None), ("svp=1", 1, Some(1))];
    for (vname, div, svp) in &variants {
        for (name, class) in [("floor", base), ("8k", k8), ("2M", two_m)] {
            // The variant's tip: the class row's work divided; slash values replaced.
            let vp = s.vp();
            let bundle = s.chain.bundle.clone();
            let (sink, tip) = s.chain.tip_state();
            let mut carriage = PalwStateCarriageV2::from_state(&tip);
            if *div > 1 && class != base {
                if let Some(row) = carriage.model_lifecycles.get_mut(&class) {
                    row.work.economic_ccu_per_claim /= *div;
                    row.work.verification_ccu /= *div;
                    // The registry re-derives the profile from the work (palw_model_registry_v1.rs:173, 229).
                    let g = kaspa_consensus_core::palw_model_registry_v1::palw_registry_globals_of_bundle_v1(&bundle);
                    row.profile.verification_window_spans = kaspa_consensus_core::palw_model_registry_v1::palw_verification_window_spans_v1(&row.work, &g);
                    row.profile.max_inflight_claims = kaspa_consensus_core::palw_model_registry_v1::palw_max_inflight_claims_v1(&row.work, &g, true);
                }
            }
            if let Some(v) = svp {
                for cs in carriage.classes.values_mut() {
                    cs.slash_value_per_pwu = *v;
                }
            }
            let planted: PalwChainStateV2 = match carriage.into_state(&bundle.state, None) {
                Ok(p) => p,
                Err(e) => {
                    out_line(format!("{{\"run\":\"study\",\"variant\":\"{vname}\",\"class\":\"{name}\",\"error\":{}}}", json_str(&format!("{e:?}"))));
                    continue;
                }
            };
            vp.palw_state_v2_store.write().set_tip_for_tests(sink, &planted).expect("variant");
            let facts = s.chain.ctx.consensus.palw_producer_facts_v2(class, Some(s.subjects[0].bond.0)).expect("facts");
            let ce = facts.bond.as_ref().unwrap().claim_exposure;
            let share = facts.bond_class_share;
            let mut ns = Vec::new();
            for j in 0..s.subjects.len() {
                ns.push(exposure_only_n(&mut s, class, j, ce, escrow, &[]));
            }
            // The class gate (share / room) for the 100k subject, alone.
            let mut gate_n = 0u64;
            let gate_why;
            loop {
                let planted_claims = if gate_n > 0 { vec![(class, s.subjects[2].bond, gate_n, ce.saturating_sub(escrow as u128), escrow)] } else { vec![] };
                let (ok, why, _) = s.with_planted(&planted_claims, &[], |s| s.verdict(class, 2));
                if !ok || gate_n > 400 {
                    gate_why = why;
                    break;
                }
                gate_n += 1;
            }
            let row = s.state().model_lifecycle(&class).cloned();
            out_line(format!(
                "{{\"run\":\"study\",\"variant\":\"{vname}\",\"class\":\"{name}\",\"claim_exposure\":{ce},\"w\":{},\"n_exposure_13k_26k_100k_1M\":{:?},\"share\":{},\"gate_n_100k\":{gate_n},\"gate_stop\":{},\"window_spans\":{},\"claim_pwu\":{}}}",
                ce.saturating_sub(escrow as u128),
                ns,
                share.map(|(a, b)| format!("[{a},{b}]")).unwrap_or_else(|| "null".into()),
                json_str(&gate_why),
                row.as_ref().map(|r| r.profile.verification_window_spans).unwrap_or(0),
                facts.pwu
            ));
            vp.palw_state_v2_store.write().set_tip_for_tests(sink, &tip).expect("restore");
        }
    }
}
