//! **ADR-0160 lane verify on a real testnet-12 chain**: F-B (`palw_capacity_batch_licence`, the batch
//! licence) and F-R (`palw_capacity_verify_room`, room v2) crossed at a low height, the batch held to
//! the single licence it batches, and the lane's ≥ 10× step measured.
//!
//! The chain is `t12_round_lane_e2e`'s (eight genesis cards with harness keys, the premine imported,
//! the EVM lane inert); the fences are set through their `PALW_T12_POST_LAUNCH_FENCES_V1` entries (the
//! field and the fold's mirror), exactly as `--palw-drill-fence-at` and the release set them. Every
//! receipt and every window root is a real ML-DSA-87 signature by the key the chain registered.
//!
//! * [`adr0160_batch_and_room_v2_cross_their_fence_at_a_low_height`] (the crossing, not ignored):
//!   below `H` a batch licence rides a valid block and folds nothing — the armed node and a node
//!   running testnet-12 as released, fed the same blocks, fold the same PALW root at every block —
//!   and a single licence licenses as ever; past `H` a batch licenses its claims, the floor room reads
//!   the seats' capital, and a second armed node fed the whole chain reaches the same root.
//! * [`adr0160_vt1_a_batch_licenses_exactly_as_its_single_licences`] (V-T1, V-I1, not ignored): from
//!   one tip, the claims' single `ReceiptLicensedV2`s and one `ReceiptLicensedBatchV1` of the same
//!   receipts are both admitted by the gate and fold to the same state root; a tampered path, a
//!   foreign root, another anchor's leaves (the cross-fork import) are refused or inert.
//! * the measurement runs (`#[ignore]`, steered by `CAP_*`, JSON lines to `CAP_OUT`): V-T2 carriage,
//!   V-T3 c_8k at 16 ready seats, V-T4 split and junk, V-T5 the floor room under a flood, and V4's
//!   attempt lane and bind cost.
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, sign_spend, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::config::params::{ForkActivation, PALW_T12_POST_LAUNCH_FENCES_V1, Params};
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::palw_batch_licence_v1::{
    PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT, PalwBatchLicenceEntryV1, PalwSeatWindowRootV1, PalwSeatWindowV1, PalwWindowLeafV1,
};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::{PalwModelLifecycleV1, PalwSeatReadinessRowV1};
use kaspa_consensus_core::palw_panel_v2::{PALW_RECEIPT_V3_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3, palw_receipt_message_v3};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwBondStateV2, PalwBondStatusV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj,
    PalwStateCarriageV2,
};
use kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2;
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

const SOMPI: u64 = 100_000_000;

type Utxos = Vec<(TransactionOutpoint, UtxoEntry)>;

fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn out_line(line: String) {
    eprintln!("[cap-verify] {line}");
    if let Ok(path) = std::env::var("CAP_OUT") {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("CAP_OUT opens");
        writeln!(f, "{line}").expect("CAP_OUT writes");
    }
}

/// The key of signer `n`: the registry harness keys for `n < 16` (cards 0..7, subjects 8..15), and a
/// key of this file's own for every planted seat past them.
fn key(n: u64) -> &'static MLDSA87KeyPair {
    if n < 16 {
        return TestConsensus::palw_v2_registry_keypair(n);
    }
    static KEYS: std::sync::OnceLock<Vec<MLDSA87KeyPair>> = std::sync::OnceLock::new();
    let all = KEYS.get_or_init(|| {
        (0..160u64)
            .map(|i| {
                let mut seed = [0xC5u8; 32];
                seed[0] = 0x40u8.wrapping_add(i as u8);
                seed[1] = 0x16;
                libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed)
            })
            .collect()
    });
    &all[(n - 16) as usize]
}

fn pubkey(n: u64) -> Vec<u8> {
    key(n).verification_key.as_ref().to_vec()
}

fn payload_of(n: u64) -> Hash64 {
    Hash64::from_bytes(kaspa_hashes::blake2b_512_address_payload(&pubkey(n)).as_bytes())
}

fn sign(n: u64, message: &[u8], context: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&key(n).signing_key, message, context, [0x3Cu8; 32]).expect("sign").as_ref().to_vec()
}

/// **testnet-12 with harness cards, F-B and F-R set to `at`** through their own post-launch entries
/// (field and mirror) — `None` is testnet-12 as released. `edit` touches the params after, for a run
/// that also needs something else.
fn verify_ruleset(at: Option<u64>) -> (Config, PalwConsensusParamsV2, Utxos, Utxos) {
    let (config, released_bundle, premine, floats) = t12_with_harness_cards();
    let Some(at) = at else { return (config, released_bundle, premine, floats) };
    let mut params: Params = config.params.clone();
    for name in ["palw_capacity_batch_licence", "palw_capacity_verify_room"] {
        let entry = PALW_T12_POST_LAUNCH_FENCES_V1.iter().find(|f| f.name == name).expect("lane verify's fences are listed");
        (entry.set)(&mut params, Some(ForkActivation::new(at)));
    }
    params.validate_palw_v2().expect("testnet-12 with F-B and F-R armed is a runnable ruleset");
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    assert_eq!(bundle.state.capacity_batch_from_daa(), Some(at), "the fold's F-B mirror followed");
    assert_eq!(bundle.state.capacity_room_from_daa(), Some(at), "the fold's F-R mirror followed");
    assert_eq!(bundle.panel, released_bundle.panel, "the fences are Params fields: the panel does not move");
    let bundle = bundle.clone();
    (config, bundle, premine, floats)
}

/// Who signs an attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Who {
    Card(usize),
    /// A planted bond: its key index.
    Planted(u64),
}

struct Sim {
    chain: T12Chain,
    domain: Hash64,
    nonce: u64,
    base: Hash64,
    k8: Option<Hash64>,
    /// Every bond's signer key index (cards 0..7, then planted bonds).
    keys: BTreeMap<PalwBondKeyV2, u64>,
    /// Spendable carrier funding: (outpoint, entry, card whose key signs it, usable from block #).
    wallet: Vec<(TransactionOutpoint, UtxoEntry, usize, u64)>,
    blocks: u64,
    queued: Vec<Transaction>,
    /// Every block inserted, in order (the crossing replays them into a twin).
    inserted: Vec<Block>,
    /// Planted seats beyond the cards whose 8k readiness is refreshed.
    planted_seats: Vec<PalwBondKeyV2>,
}

impl Sim {
    async fn new(at: Option<u64>) -> Self {
        kaspa_core::log::try_init_logger("warn");
        let (config, bundle, premine, floats) = verify_ruleset(at);
        let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        let base = bundle.base_class_id;
        let (_, genesis_state) = chain.tip_state();
        let k8 = genesis_state
            .model_lifecycles_iter()
            .find(|(id, row)| **id != base && !kaspa_consensus_core::palw_work_target_v1::palw_panel_held_to_final_v1(row))
            .map(|(id, _)| *id);
        let keys = chain.bonds.iter().enumerate().map(|(i, b)| (*b, i as u64)).collect();
        let first = chain.heartbeat(config.params.target_time_per_block(), Vec::new()).await;
        let mut s = Sim {
            chain,
            domain,
            nonce: 1 << 44,
            base,
            k8,
            keys,
            wallet: Vec::new(),
            blocks: 1,
            queued: Vec::new(),
            inserted: vec![first],
            planted_seats: Vec::new(),
        };
        s.queued = s.fan_out(&floats, 24);
        s
    }

    fn vp(&self) -> std::sync::Arc<crate::pipeline::virtual_processor::VirtualStateProcessor> {
        self.chain.vp()
    }

    fn daa(&self) -> u64 {
        self.chain.ctx.consensus.get_virtual_daa_score()
    }

    fn state(&self) -> std::sync::Arc<PalwChainStateV2> {
        self.vp().palw_state_v2_store.read().load_tip_cached(&self.chain.bundle.state).unwrap().expect("the tip loads").1
    }

    fn root(&self) -> Hash64 {
        self.state().state_root()
    }

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
        self.inserted.push(block.clone());
        block
    }

    /// Queued carriers by the block's transient-mass budget (the unchanged block mass rule, V-I2).
    fn take_carriers(&mut self) -> Vec<Transaction> {
        let calc = kaspa_consensus_core::mass::MassCalculator::new_with_consensus_params(&self.chain.config.params);
        let budget = self.chain.config.params.max_block_mass.saturating_sub(20_000);
        let (mut used, mut stored) = (0u64, 0u64);
        let mut taken = Vec::new();
        while let Some(tx) = self.queued.first() {
            let m = calc.calc_non_contextual_masses(tx).transient_mass;
            // The storage mass the transaction commits to (a fan-out's many outputs weigh here).
            if used + m > budget || stored + tx.mass() > budget {
                break;
            }
            used += m;
            stored += tx.mass();
            taken.push(self.queued.remove(0));
        }
        taken
    }

    fn transient_mass(&self, tx: &Transaction) -> u64 {
        kaspa_consensus_core::mass::MassCalculator::new_with_consensus_params(&self.chain.config.params)
            .calc_non_contextual_masses(tx)
            .transient_mass
    }

    /// Heartbeats until the DAA moves by one, carrying the queued carriers.
    async fn beat(&mut self) {
        let before = self.daa();
        let ttpb = self.chain.config.params.target_time_per_block();
        for _ in 0..6 {
            let txs = self.take_carriers();
            let block = self.chain.heartbeat(ttpb, txs).await;
            self.blocks += 1;
            self.inserted.push(block);
            if self.daa() > before {
                return;
            }
        }
        panic!("six heartbeats did not move the DAA past {before}");
    }

    async fn beat_to(&mut self, daa: u64) {
        while self.daa() < daa {
            self.beat().await;
        }
    }

    fn who(&self, w: Who) -> (u64, PalwBondKeyV2) {
        match w {
            Who::Card(i) => (i as u64, self.chain.bonds[i]),
            Who::Planted(n) => (n, *self.keys.iter().find(|(_, k)| **k == n).expect("a planted bond").0),
        }
    }

    fn facts(&self, w: Who, class: Hash64) -> kaspa_consensus_core::palw_producer_v2::PalwProducerFactsV2 {
        let (_, bond) = self.who(w);
        self.chain.ctx.consensus.palw_producer_facts_v2(class, Some(bond.0)).expect("a V2 network answers")
    }

    /// Whether `w` may make one more claim of `class`, as its producer asks.
    fn ready(&self, w: Who, class: Hash64) -> Result<(), String> {
        let (n, _) = self.who(w);
        let facts = self.facts(w, class);
        facts.ready_to_produce_v3(&pubkey(n), true).map_err(|why| format!("{why}: {}", facts.class_admission_refusal.unwrap_or_default()))
    }

    /// An attempt block by `w` for `class` on the node's own template, inserted as the sink. Returns the
    /// claim id and whether the fold created the claim (a skipped own attempt leaves the block standing
    /// without one — it still anchors).
    async fn attempt(&mut self, w: Who, class: Hash64, txs: Vec<Transaction>) -> (Hash64, bool) {
        let (block, claim_id) = self.build_attempt(w, class, txs);
        self.insert(block, &format!("{w:?}'s attempt")).await;
        let created = self.state().claim(&claim_id).is_some();
        (claim_id, created)
    }

    fn build_attempt(&mut self, w: Who, class: Hash64, txs: Vec<Transaction>) -> (MutableBlock, Hash64) {
        use kaspa_consensus_core::palw_attempt_v2::{
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
            PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
        };
        let ttpb = self.chain.config.params.target_time_per_block();
        self.chain.ctx.simulated_time += ttpb / 50;
        self.nonce += 1;
        let (n, bond) = self.who(w);
        let miner = match w {
            Who::Card(i) => card_payout_spk(i),
            Who::Planted(n) => p2pkh_mldsa87_spk(payload_of(n).as_byte_slice()),
        };
        let mut t = self
            .chain
            .ctx
            .consensus
            .build_block_template(MinerData::new(miner, vec![]), Box::new(super::OnetimeTxSelector::new(txs)), TemplateBuildMode::Standard)
            .expect("a template");
        assert!(kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(t.block.header.pow_algo_id), "the attempt lane");
        stamp_harness_time(&self.chain.config.params, &mut t.block.header, self.chain.ctx.simulated_time);
        t.block.header.nonce = self.nonce;
        let facts = self.facts(w, class);
        let bond_facts = facts.bond.as_ref().expect("a registered bond").clone();
        let header = &t.block.header;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let mut attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: self.domain,
            challenge: challenge_v2(self.domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
            class_id: facts.class_id,
            executor_bond: bond.0,
            executor_pubkey: pubkey(n),
            operator_id: bond_facts.operator_id,
            artifact_root: facts.artifact_root,
            trace_root: Hash64::default(),
            output_root: Hash64::from_u64_word(0x0CA1_0000_0000_0000 | self.nonce),
            execution_root: Hash64::from_u64_word(0xCA71_0000_0000_0000 | self.nonce),
            pwu: facts.pwu,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        let anchor = execution_anchor_v3(self.domain, pre_pow, facts.class_id, &bond.0, header.nonce);
        let mut won = false;
        for draw in 0u64..4_000_000 {
            attempt.trace_root = Hash64::from_u64_word((self.nonce << 20) ^ draw ^ 0x7D00_0000_0000_0000);
            attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
            if class_ticket_v3(&attempt, anchor) <= facts.class_target {
                won = true;
                break;
            }
        }
        assert!(won, "the class lottery is winnable");
        let claim_id = attempt_id_v2(&attempt);
        let signature = sign(n, claim_id.as_byte_slice(), PALW_ATTEMPT_V2_MLDSA87_CONTEXT);
        t.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
        t.block.header.finalize();
        (t.block, claim_id)
    }

    /// Split each card's genesis fee float into `parts`, so a block can carry many carriers.
    fn fan_out(&mut self, floats: &[(TransactionOutpoint, UtxoEntry)], parts: u32) -> Vec<Transaction> {
        let mut txs = Vec::new();
        for (card, (outpoint, entry)) in floats.iter().enumerate() {
            let part = (entry.amount - 1_000_000) / parts as u64;
            let outputs = (0..parts).map(|_| TransactionOutput::new(part, card_payout_spk(card))).collect();
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
            for i in 0..parts {
                self.wallet.push((TransactionOutpoint::new(tx.id(), i), UtxoEntry::new(part, card_payout_spk(card), 0, false), card, self.blocks + 3));
            }
            txs.push(tx);
        }
        txs
    }

    /// A 0x4b carrier for `object`, funded by the first ready wallet entry; its change joins the wallet.
    fn carrier(&mut self, object: Obj) -> Option<Transaction> {
        use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        let idx = self.wallet.iter().position(|(_, _, _, ready)| *ready <= self.blocks)?;
        let (outpoint, entry, card, _) = self.wallet.remove(idx);
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
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
        self.wallet.push((TransactionOutpoint::new(tx.id(), 0), UtxoEntry::new(change, card_payout_spk(card), 0, false), card, self.blocks + 3));
        Some(tx)
    }

    /// Every seat's V3 `Valid` on `claim`, with the mask the panel assigned it, signed now.
    fn v3_receipts(&self, claim: Hash64) -> Vec<PalwSeatReceiptV3> {
        let state = self.state();
        let panel = state.panel(&claim).expect("a bound panel").clone();
        let signed_daa = self.daa();
        let assignment = palw_segment_assignment_v2(panel.anchor, claim, panel.seats.len() as u16);
        panel
            .seats
            .iter()
            .enumerate()
            .map(|(i, seat)| {
                let segments = assignment.mask_of(i as u16);
                let message = palw_receipt_message_v3(self.domain, claim, PalwReceiptVerdictV2::Valid, signed_daa, segments);
                let n = self.keys[&seat.bond];
                PalwSeatReceiptV3 {
                    receipt: PalwSeatReceiptV2 {
                        claim,
                        verdict: PalwReceiptVerdictV2::Valid,
                        seat_bond: seat.bond,
                        signed_daa,
                        signature: sign(n, message.as_byte_slice(), PALW_RECEIPT_V3_MLDSA87_CONTEXT),
                    },
                    segments,
                }
            })
            .collect()
    }

    /// The single coverage licence of `claim` — the node's own assembler's answer, asserted.
    fn single_licence(&self, claim: Hash64) -> Obj {
        let receipts = self.v3_receipts(claim);
        let object = Obj::ReceiptLicensedV2 { claim, receipts: receipts.clone() };
        let assembled = self.vp().palw_v2_receipt_coverage_assemble_impl(claim, &receipts);
        assert_eq!(assembled.as_ref(), Some(&object), "the node's coverage assembler builds the same licence");
        object
    }

    /// **Each seat's window over its `Valid`s on `claims`, signed now**: one root per seat, one leaf a
    /// (seat, claim), each leaf bound to the claim's panel anchor.
    fn windows(&self, claims: &[Hash64]) -> Vec<PalwSeatWindowV1> {
        let state = self.state();
        let now = self.daa();
        let mut leaves: BTreeMap<PalwBondKeyV2, Vec<PalwWindowLeafV1>> = BTreeMap::new();
        for claim in claims {
            let Some(panel) = state.panel(claim) else { continue };
            let assignment = palw_segment_assignment_v2(panel.anchor, *claim, panel.seats.len() as u16);
            for (i, seat) in panel.seats.iter().enumerate() {
                leaves.entry(seat.bond).or_default().push(PalwWindowLeafV1 {
                    claim: *claim,
                    anchor_hash: panel.anchor,
                    verdict: PalwReceiptVerdictV2::Valid,
                    signed_daa: now,
                    mask: assignment.mask_of(i as u16),
                });
            }
        }
        leaves
            .into_iter()
            .map(|(seat, leaves)| {
                let n = self.keys[&seat];
                PalwSeatWindowV1::sign(self.domain, seat, now, now, leaves, |message| sign(n, message, PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT))
                    .expect("a non-empty window")
            })
            .collect()
    }

    /// The node's batch assembler over the seats' windows of `claims` (in the order given).
    fn batch_licence(&self, claims: &[Hash64], max_bytes: usize) -> Option<Obj> {
        self.vp().palw_v2_batch_licence_assemble_impl(&self.windows(claims), claims, max_bytes)
    }

    fn point(&self) -> PalwBlockContextV2 {
        let virtual_state = self.vp().lkg_virtual_state.load();
        PalwBlockContextV2 { block: self.chain.sink(), daa_score: virtual_state.daa_score, blue_score: virtual_state.ghostdag_data.blue_score, subsidy: 0 }
    }

    fn bound_claims(&self) -> Vec<Hash64> {
        let state = self.state();
        let mut bound: Vec<(u64, Hash64)> = state
            .claims_iter()
            .filter_map(|(id, c)| match c.phase {
                PalwClaimPhaseV2::PanelBound { bound_daa } => Some((bound_daa, *id)),
                _ => None,
            })
            .collect();
        bound.sort();
        bound.into_iter().map(|(_, id)| id).collect()
    }

    /// Plant bonds on the tip (the carriage, as `t12_claim_capacity` plants): `n` bonds of `msk` each,
    /// keys `first_key..`, each its own operator; `seat` bonds declare the floor and the 8k class and
    /// hold a fresh 8k readiness row. Returns their keys.
    fn plant_bonds(&mut self, first_key: u64, count: u64, msk: u64, seat: bool) -> Vec<PalwBondKeyV2> {
        let vp = self.vp();
        let bundle = self.chain.bundle.clone();
        let (sink, tip) = self.chain.tip_state();
        let now = self.daa();
        let mut carriage = PalwStateCarriageV2::from_state(&tip);
        let mut planted = Vec::new();
        for j in 0..count {
            let n = first_key + j;
            let bond = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xCA9A_5EA7_0000_0000 | n), 0));
            let mut classes = BTreeSet::new();
            if seat {
                classes.insert(self.base);
                if let Some(k8) = self.k8 {
                    classes.insert(k8);
                }
            }
            carriage.bonds.insert(
                bond,
                PalwBondStateV2 {
                    pubkey: pubkey(n),
                    operator_id: Hash64::from_u64_word(0x0FE7_A70E_5EA7_0000 | n),
                    collateral: msk * SOMPI,
                    slashed: 0,
                    status: PalwBondStatusV2::Active,
                    // Registered long enough ago for every maturity floor (planted at the tip).
                    registered_daa: 0,
                    payout_payload: payload_of(n),
                    capable_classes: classes,
                },
            );
            if seat && let Some(k8) = self.k8 {
                carriage
                    .seat_readiness
                    .insert((bond, k8), PalwSeatReadinessRowV1 { proved_daa: now, proved_span: now, leaf_index: 0, proof_version: 2, chunks: 8 });
            }
            self.keys.insert(bond, n);
            planted.push(bond);
        }
        if seat {
            self.planted_seats.extend(planted.iter().copied());
        }
        let state: PalwChainStateV2 = carriage.into_state(&bundle.state, None).expect("the planted tip is a consistent state");
        vp.palw_state_v2_store.write().set_tip_for_tests(sink, &state).expect("the planted tip becomes the tip");
        planted
    }

    /// The 8k row admitting and every seat's readiness fresh (the harness cannot run the real proof).
    fn refresh_8k(&mut self) {
        let Some(k8) = self.k8 else { return };
        let vp = self.vp();
        let bundle = self.chain.bundle.clone();
        let (sink, tip) = self.chain.tip_state();
        let now = self.daa();
        let mut carriage = PalwStateCarriageV2::from_state(&tip);
        for seat in self.chain.bonds.iter().chain(self.planted_seats.iter()) {
            carriage
                .seat_readiness
                .insert((*seat, k8), PalwSeatReadinessRowV1 { proved_daa: now, proved_span: now, leaf_index: 0, proof_version: 2, chunks: 8 });
        }
        if let Some(row) = carriage.model_lifecycles.get_mut(&k8)
            && !row.state.admits_claims()
        {
            row.state = PalwModelLifecycleV1::Probation { probes_passed: 0 };
        }
        let state: PalwChainStateV2 = carriage.into_state(&bundle.state, None).expect("consistent");
        vp.palw_state_v2_store.write().set_tip_for_tests(sink, &state).expect("the refresh becomes the tip");
    }
}

/// **The crossing** (the memory rule "a flag day needs a drill that crosses it"): F-B and F-R at
/// `H`, a chain from genesis to well past it with claims bound and licensed on both sides.
#[tokio::test]
async fn adr0160_batch_and_room_v2_cross_their_fence_at_a_low_height() {
    const H: u64 = 70;
    let mut s = Sim::new(Some(H)).await;
    let base = s.base;
    let anchor_delay = s.chain.bundle.panel.anchor_delay();
    // Claims below the fence, bound below it: one licensed by a single licence, one offered a batch.
    s.beat_to(34).await;
    let mut below = Vec::new();
    for card in 0..4 {
        let (claim, created) = s.attempt(Who::Card(card), base, Vec::new()).await;
        assert!(created, "card {card}'s floor claim below the fence");
        below.push(claim);
    }
    // To the anchor slot (and past it, where an attempt block binds them), still below H.
    s.beat_to(34 + anchor_delay).await;
    let (anchor_claim, _) = s.attempt(Who::Card(4), base, Vec::new()).await;
    let bound = s.bound_claims();
    assert!(below.iter().all(|c| bound.contains(c)), "the four bound at their anchor ({bound:?})");
    assert!(s.daa() < H, "still below the fence at DAA {}", s.daa());
    // Below the fence the assembler offers nothing and a hand-built batch is dropped with its block
    // standing; the single licence licenses as ever.
    assert_eq!(s.batch_licence(&below[1..], 480_000 / 4), None, "no batch below F-B at the virtual's DAA");
    // The assembler is dormant, so the batch is built by hand from the seats' signed windows.
    let windows = s.windows(&below[1..2]);
    let entry_batch = hand_batch(&s, &windows, &below[1..2]);
    let single = s.single_licence(below[0]);
    let single_tx = s.carrier(single).expect("funding");
    let batch_tx = s.carrier(entry_batch).expect("funding");
    s.queued.push(single_tx);
    s.queued.push(batch_tx);
    s.beat().await;
    s.beat().await;
    let st = s.state();
    assert!(matches!(st.claim(&below[0]).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the single licence licensed below H");
    assert!(matches!(st.claim(&below[1]).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "the batch below H folded nothing");
    assert!(s.daa() < H);
    // Across the fence: more claims, bound past it, licensed by one batch.
    s.beat_to(H - 2).await;
    let mut above = Vec::new();
    for card in 0..6 {
        let (claim, created) = s.attempt(Who::Card(card % 8), base, Vec::new()).await;
        assert!(created, "a floor claim straddling the fence");
        above.push(claim);
    }
    s.beat_to(H - 2 + anchor_delay).await;
    s.attempt(Who::Card(7), base, Vec::new()).await;
    assert!(s.daa() >= H, "past the fence");
    let bound = s.bound_claims();
    let due: Vec<Hash64> = bound.iter().copied().filter(|c| above.contains(c) || below[1..].contains(c)).collect();
    assert!(due.len() >= 6, "the straddling claims and the two left below are bound: {due:?}");
    let batch = s.batch_licence(&due, 480_000 / 4).expect("past F-B the assembler batches the due claims");
    let Obj::ReceiptLicensedBatchV1 { roots, entries } = &batch else { unreachable!() };
    assert_eq!(entries.len(), due.len(), "every due claim in one batch");
    assert!(roots.len() <= 8, "one root per seat: {}", roots.len());
    let tx = s.carrier(batch.clone()).expect("funding");
    out_line(format!("{{\"crossing\":1,\"entries\":{},\"roots\":{},\"transient_mass\":{}}}", entries.len(), roots.len(), s.transient_mass(&tx)));
    s.queued.push(tx);
    s.beat().await;
    s.beat().await;
    let st = s.state();
    for claim in &due {
        assert!(
            matches!(st.claim(claim).map(|c| c.phase.clone()), Some(PalwClaimPhaseV2::ReceiptLicensed { .. })),
            "claim {claim} licensed by the batch past H"
        );
    }
    // The floor room reads the seats' capital past the fence and admits honest traffic.
    s.ready(Who::Card(1), base).expect("a card may still produce past F-R");
    let (_, created) = s.attempt(Who::Card(1), base, Vec::new()).await;
    assert!(created, "a floor claim past F-R");
    let _ = anchor_claim;
    s.beat_to(H + 30).await;

    // **Below the fence an armed node IS a released node.**
    let twin_chain = {
        let (config, bundle, premine, floats) = verify_ruleset(None);
        t12_genesis_chain(&config, &bundle, &premine, &floats)
    };
    let armed_blocks = s.inserted.clone();
    let mut compared = 0;
    for block in &armed_blocks {
        if block.header.daa_score >= H {
            break;
        }
        twin_chain
            .ctx
            .consensus
            .validate_and_insert_block(block.clone())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("the released node refuses an armed block below H: {e}"));
        let tip = block.header.hash;
        let armed_root = s.vp().palw_state_v2_store.read().state_root_of(tip).expect("the armed node's root of the block");
        let released_root = twin_chain.vp().palw_state_v2_store.read().state_root_of(tip).expect("the released node's root of the block");
        assert_eq!(released_root, armed_root, "block {tip} at DAA {}: one root below the fence", block.header.daa_score);
        compared += 1;
    }
    assert!(compared > 40, "{compared} blocks compared below the fence");
    // **The whole chain is chain data**: a second armed node fed every block reaches the same root.
    let second_chain = {
        let (config, bundle, premine, floats) = verify_ruleset(Some(H));
        t12_genesis_chain(&config, &bundle, &premine, &floats)
    };
    for block in &armed_blocks {
        second_chain.ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await.expect("a second armed node takes the chain");
    }
    assert_eq!(second_chain.sink(), s.chain.sink(), "the same sink");
    assert_eq!(second_chain.tip_state().1.state_root(), s.root(), "the same PALW root on a second armed node");
}

/// A batch built by hand from `windows` for `claims` (the assembler is dormant below the fence).
fn hand_batch(s: &Sim, windows: &[PalwSeatWindowV1], claims: &[Hash64]) -> Obj {
    let state = s.state();
    let mut roots: Vec<PalwSeatWindowRootV1> = Vec::new();
    let mut entries = Vec::new();
    for claim in claims {
        let panel = state.panel(claim).expect("bound").clone();
        let mut seats = Vec::new();
        for (i, seat) in panel.seats.iter().enumerate() {
            let window = windows.iter().find(|w| w.root.seat_bond == seat.bond).expect("the seat's window");
            let leaf = window.leaves.iter().position(|l| l.claim == *claim).expect("the seat's leaf");
            let root_index = match roots.iter().position(|r| *r == window.root) {
                Some(r) => r,
                None => {
                    roots.push(window.root.clone());
                    roots.len() - 1
                }
            };
            let hashes: Vec<Hash64> = window.leaves.iter().map(|l| l.leaf(s.domain)).collect();
            seats.push(kaspa_consensus_core::palw_batch_licence_v1::PalwBatchSeatReceiptV1 {
                seat_index: i as u8,
                root_index: root_index as u16,
                verdict: window.leaves[leaf].verdict,
                mask: window.leaves[leaf].mask,
                signed_daa: window.leaves[leaf].signed_daa,
                leaf_index: leaf as u32,
                path: kaspa_consensus_core::palw_batch_licence_v1::palw_receipt_window_path_v1(&hashes, leaf).unwrap(),
            });
        }
        entries.push(PalwBatchLicenceEntryV1 { claim: *claim, anchor_hash: panel.anchor, seats });
    }
    Obj::ReceiptLicensedBatchV1 { roots, entries }
}

/// **V-T1 / V-I1: a batch entry licenses exactly as the single object carrying the same receipts
/// would.** From one tip with six bound claims: six `ReceiptLicensedV2`s and one
/// `ReceiptLicensedBatchV1` of the same seats' `Valid`s both pass the gate and fold to one state root.
/// A path that does not fold, a root re-signed by another bond, and a leaf bound to another anchor
/// (a receipt imported from another fork) never license.
#[tokio::test]
async fn adr0160_vt1_a_batch_licenses_exactly_as_its_single_licences() {
    let mut s = Sim::new(Some(0)).await;
    let base = s.base;
    let anchor_delay = s.chain.bundle.panel.anchor_delay();
    s.beat_to(34).await;
    let mut claims = Vec::new();
    for card in 0..6 {
        let (claim, created) = s.attempt(Who::Card(card), base, Vec::new()).await;
        assert!(created);
        claims.push(claim);
    }
    s.beat_to(34 + anchor_delay).await;
    s.attempt(Who::Card(6), base, Vec::new()).await;
    let bound = s.bound_claims();
    let due: Vec<Hash64> = bound.iter().copied().filter(|c| claims.contains(c)).collect();
    assert_eq!(due.len(), 6, "six bound claims");
    let singles: Vec<Obj> = due.iter().map(|c| s.single_licence(*c)).collect();
    let batch = s.batch_licence(&due, 480_000 / 4).expect("the assembler batches them");
    let point = s.point();
    let tip = s.state();
    let sp = &s.chain.bundle.state;
    let vp = s.vp();
    let accepted_singles = vp.palw_v2_accepted_objects_for_tests(&tip, sp, &point, singles.clone(), s.chain.sink());
    assert_eq!(accepted_singles.len(), 6, "the gate takes every single licence");
    let accepted_batch = vp.palw_v2_accepted_objects_for_tests(&tip, sp, &point, vec![batch.clone()], s.chain.sink());
    assert_eq!(accepted_batch, vec![batch.clone()], "the gate takes the batch");
    let by_singles = vp.palw_v2_fold_accepted_for_tests(&tip, sp, &point, &accepted_singles).expect("the singles fold");
    let by_batch = vp.palw_v2_fold_accepted_for_tests(&tip, sp, &point, &accepted_batch).expect("the batch folds");
    for claim in &due {
        assert!(matches!(by_batch.claim(claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "claim {claim} licensed");
    }
    assert_eq!(by_batch.state_root(), by_singles.state_root(), "V-I1: one root whichever carriage");
    out_line(format!("{{\"vt1\":1,\"claims\":6,\"root\":\"{}\"}}", by_batch.state_root()));

    let Obj::ReceiptLicensedBatchV1 { roots, entries } = &batch else { unreachable!() };
    assert!(roots.iter().any(|r| r.leaves.len() == r.count as usize), "the assembler carries a root's full list where it is cheaper");
    for e in entries {
        for seat in &e.seats {
            assert_eq!(seat.path.is_empty(), !roots[seat.root_index as usize].leaves.is_empty(), "a path exactly where the root rides without its list");
        }
    }
    // The same licences in the path form (the seats' windows, a Merkle path a receipt) license alike.
    let by_paths = hand_batch(&s, &s.windows(&due), &due);
    let accepted_paths = vp.palw_v2_accepted_objects_for_tests(&tip, sp, &point, vec![by_paths.clone()], s.chain.sink());
    assert_eq!(accepted_paths, vec![by_paths.clone()], "the gate takes the path form");
    let folded_paths = vp.palw_v2_fold_accepted_for_tests(&tip, sp, &point, &accepted_paths).expect("the path form folds");
    assert_eq!(folded_paths.state_root(), by_singles.state_root(), "V-I1 in the path form too");
    // A path that does not fold: the whole object is refused.
    let Obj::ReceiptLicensedBatchV1 { roots: path_roots, entries: path_entries } = &by_paths else { unreachable!() };
    let mut bad_path = path_entries.clone();
    bad_path[0].seats[0].path[0] = Hash64::from_u64_word(0xBAD);
    assert!(vp.palw_v2_accepted_objects_for_tests(&tip, sp, &point, vec![Obj::ReceiptLicensedBatchV1 { roots: path_roots.clone(), entries: bad_path }], s.chain.sink()).is_empty(), "a path that does not fold is refused");
    // A carried list that does not hash to its signed root: refused.
    let mut bad_list = roots.clone();
    bad_list[0].leaves[0] = Hash64::from_u64_word(0xBAD);
    assert!(vp.palw_v2_accepted_objects_for_tests(&tip, sp, &point, vec![Obj::ReceiptLicensedBatchV1 { roots: bad_list, entries: entries.clone() }], s.chain.sink()).is_empty(), "a list that is not the root's is refused");
    // A receipt naming another leaf of a listed root: refused.
    let mut wrong_leaf = entries.clone();
    wrong_leaf[0].seats[0].leaf_index = (wrong_leaf[0].seats[0].leaf_index + 1) % roots[wrong_leaf[0].seats[0].root_index as usize].count;
    assert!(vp.palw_v2_accepted_objects_for_tests(&tip, sp, &point, vec![Obj::ReceiptLicensedBatchV1 { roots: roots.clone(), entries: wrong_leaf }], s.chain.sink()).is_empty(), "another leaf of the list is refused");
    // A root signed by another bond's key: refused.
    let mut forged_roots = roots.clone();
    forged_roots[0].signature = sign(15, b"not the window message", PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT);
    assert!(vp.palw_v2_accepted_objects_for_tests(&tip, sp, &point, vec![Obj::ReceiptLicensedBatchV1 { roots: forged_roots, entries: entries.clone() }], s.chain.sink()).is_empty(), "a root not signed by its seat is refused");
    // Cross-fork import: the same seats' leaves bound to ANOTHER anchor — valid signatures over a real
    // window, every path folding — license nothing here: the entry is inert, the claims stay bound.
    let foreign_windows: Vec<PalwSeatWindowV1> = {
        let windows = s.windows(&due);
        windows
            .into_iter()
            .map(|w| {
                let n = s.keys[&w.root.seat_bond];
                let leaves: Vec<PalwWindowLeafV1> = w.leaves.iter().map(|l| PalwWindowLeafV1 { anchor_hash: Hash64::from_u64_word(0xF0E1), ..*l }).collect();
                PalwSeatWindowV1::sign(s.domain, w.root.seat_bond, w.root.from_daa, w.root.to_daa, leaves, |m| sign(n, m, PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT)).unwrap()
            })
            .collect()
    };
    let foreign = {
        let Obj::ReceiptLicensedBatchV1 { roots: _, entries: real_entries } = hand_batch(&s, &s.windows(&due), &due) else { unreachable!() };
        let Obj::ReceiptLicensedBatchV1 { roots: f_roots, entries: mut f_entries } = hand_batch(&s, &foreign_windows, &due) else { unreachable!() };
        for (f, r) in f_entries.iter_mut().zip(real_entries.iter()) {
            assert_eq!(f.claim, r.claim);
            f.anchor_hash = Hash64::from_u64_word(0xF0E1);
        }
        Obj::ReceiptLicensedBatchV1 { roots: f_roots, entries: f_entries }
    };
    let accepted_foreign = vp.palw_v2_accepted_objects_for_tests(&tip, sp, &point, vec![foreign], s.chain.sink());
    let folded = vp.palw_v2_fold_accepted_for_tests(&tip, sp, &point, &accepted_foreign).expect("folds");
    for claim in &due {
        assert!(matches!(folded.claim(claim).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "another anchor's receipts license nothing");
    }
    // Inert entries do not drop a live one: a batch re-offering a licensed claim beside a bound one.
    let _ = anchor_delay;
}

// ---------------------------------------------------------------------------------------------------
// The measurement runs (ignored: minutes of a debug build; steer with CAP_*, JSON lines to CAP_OUT).
// ---------------------------------------------------------------------------------------------------

impl Sim {
    /// Plant `n` extra `Provisional` claims of `class` on `bond` at `accepted_daa`, each reserving what
    /// `template` reserves (a real claim of the class), on the current tip.
    fn plant_claims(&mut self, class: Hash64, bond: PalwBondKeyV2, n: u64, accepted_daa: u64, template: &kaspa_consensus_core::palw_state_v2::PalwClaimStateV2, salt: u64) -> Vec<Hash64> {
        let vp = self.vp();
        let bundle = self.chain.bundle.clone();
        let (sink, tip) = self.chain.tip_state();
        let mut carriage = PalwStateCarriageV2::from_state(&tip);
        let mut ids = Vec::new();
        for i in 0..n {
            let mut claim = kaspa_consensus_core::palw_state_v2::palw_claim_template_v1(class, bond, accepted_daa, template.reserved, template.escrowed_reward);
            claim.pwu = template.pwu;
            claim.accepted_block = template.accepted_block;
            claim.trace_root = Hash64::from_u64_word(0x7100_0000 ^ (salt << 24) ^ i);
            claim.output_root = Hash64::from_u64_word(0x7200_0000 ^ (salt << 24) ^ i);
            claim.execution_root = Hash64::from_u64_word(0x7300_0000 ^ (salt << 24) ^ i);
            claim.trace_chunk_count = template.trace_chunk_count;
            claim.trace_retention_daa = template.trace_retention_daa;
            // No immature weight (the planted tip's `bounded_immature` is not re-summed here): the
            // gates measured read none of it.
            claim.immature_contribution = 0;
            let held = kaspa_consensus_core::palw_state_v2::palw_claim_bond_reservation_v1(&bundle.state, &claim).expect("a reservation");
            *carriage.reserved_exposure.entry(bond).or_insert(0) += held;
            let id = Hash64::from_u64_word(0x7F00_0000_0000 ^ (salt << 32) ^ i);
            carriage.claims.insert(id, claim);
            ids.push(id);
        }
        let state: PalwChainStateV2 = carriage.into_state(&bundle.state, None).expect("the planted tip is consistent");
        vp.palw_state_v2_store.write().set_tip_for_tests(sink, &state).expect("plant");
        ids
    }

    /// Everything bound whose receipts are due at `now` (bound at least `delay` DAA ago), oldest bind
    /// first, not yet sent.
    fn due(&self, delay: u64, sent: &BTreeSet<Hash64>) -> Vec<Hash64> {
        let state = self.state();
        let now = self.daa();
        let mut bound: Vec<(u64, Hash64)> = state
            .claims_iter()
            .filter_map(|(id, c)| match c.phase {
                PalwClaimPhaseV2::PanelBound { bound_daa } if bound_daa + delay <= now && !sent.contains(id) => Some((bound_daa, *id)),
                _ => None,
            })
            .collect();
        bound.sort();
        bound.into_iter().map(|(_, id)| id).collect()
    }
}

fn percentile(values: &mut [u64], p: usize) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[((values.len() - 1) * p) / 100]
}

/// **V-T2: carriage ≥ 30 coverage licences a block, real ML-DSA-87** — and the licence-latency tail
/// under batching. A 1M-MSK pure producer opens up to `CAP_PER_DAA` floor claims a DAA (default 40)
/// for `CAP_DAA` DAA (default 40); a genesis card anchors one attempt a DAA; the seats' windows are
/// signed each DAA and the node's batch assembler (`palw_v2_batch_licence_assemble_impl`) licenses every
/// due claim, oldest bind first, in carriers of ≤ 480k transient mass; each block takes carriers by
/// its 500k budget. `CAP_POLICY=single` runs the same chain with single coverage licences instead.
#[tokio::test]
#[ignore = "a measurement run (ADR-0160 V-T2): minutes; run with --ignored"]
async fn adr0160_vt2_batch_carriage_per_block() {
    let batch = std::env::var("CAP_POLICY").map(|p| p != "single").unwrap_or(true);
    let daa_len: u64 = env_or("CAP_DAA", 40);
    let per_daa: u64 = env_or("CAP_PER_DAA", 40);
    let lic_delay: u64 = env_or("CAP_LIC_DELAY", 1);
    let run = if batch { "vt2-batch" } else { "vt2-single" };
    let extra_seats: u64 = env_or("CAP_EXTRA_SEATS", 0);
    let mut s = Sim::new(Some(0)).await;
    let base = s.base;
    s.beat_to(34).await;
    if extra_seats > 0 {
        // More floor seats: every batch then carries more roots (one a seat touched).
        s.plant_bonds(32, extra_seats, 939_063, true);
    }
    let bond_msk: u64 = env_or("CAP_BOND_MSK", 1_000_000);
    let subject = s.plant_bonds(8, 1, bond_msk, false)[0];
    let started = std::time::Instant::now();
    let mut sent: BTreeSet<Hash64> = BTreeSet::new();
    let mut accepted: BTreeMap<Hash64, u64> = BTreeMap::new();
    let mut max_entries = 0usize;
    let mut per_block: Vec<(u64, usize, u64)> = Vec::new();
    let calc = kaspa_consensus_core::mass::MassCalculator::new_with_consensus_params(&s.chain.config.params);
    for rel in 0..daa_len {
        let card = (rel % 8) as usize;
        let txs = s.take_carriers();
        s.attempt(Who::Card(card), base, txs).await;
        for _ in 0..per_daa {
            if s.ready(Who::Planted(8), base).is_err() {
                break;
            }
            let txs = s.take_carriers();
            let (claim, created) = s.attempt(Who::Planted(8), base, txs).await;
            if created {
                accepted.insert(claim, s.daa());
            }
        }
        // Licences: every due claim, oldest bind first; the seats sign one window over each group of
        // `CAP_WINDOW` due claims (a seat's receipts of one replay round), the collector batches it.
        let mut due = s.due(lic_delay, &sent);
        let window_size: usize = env_or("CAP_WINDOW", 64);
        while !due.is_empty() {
            if batch {
                let group: Vec<Hash64> = due.iter().copied().take(window_size).collect();
                let Some(object) = s.batch_licence(&group, 480_000 / 4 - 8_000) else { break };
                let Obj::ReceiptLicensedBatchV1 { entries, .. } = &object else { unreachable!() };
                let taken: BTreeSet<Hash64> = entries.iter().map(|e| e.claim).collect();
                let Some(tx) = s.carrier(object.clone()) else { break };
                sent.extend(taken.iter().copied());
                due.retain(|c| !taken.contains(c));
                s.queued.push(tx);
            } else {
                let claim = due.remove(0);
                let object = s.single_licence(claim);
                let Some(tx) = s.carrier(object) else { break };
                sent.insert(claim);
                s.queued.push(tx);
            }
        }
        // One DAA of heartbeats, each block carrying what its mass holds; count what each carried.
        let before = s.daa();
        for _ in 0..8 {
            let txs = s.take_carriers();
            let carried: usize = txs
                .iter()
                .map(|tx| {
                    let payload: Option<kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2> = borsh::from_slice(&tx.payload).ok();
                    match payload.map(|p| p.object) {
                        Some(Obj::ReceiptLicensedBatchV1 { entries, .. }) => entries.len(),
                        Some(Obj::ReceiptLicensedV2 { .. }) => 1,
                        _ => 0,
                    }
                })
                .sum();
            let mass: u64 = txs.iter().map(|tx| calc.calc_non_contextual_masses(tx).transient_mass).sum();
            let block = s.chain.heartbeat(s.chain.config.params.target_time_per_block(), txs).await;
            s.blocks += 1;
            s.inserted.push(block);
            if carried > 0 {
                per_block.push((s.daa(), carried, mass));
                max_entries = max_entries.max(carried);
            }
            if s.daa() > before {
                break;
            }
        }
        if rel % 5 == 0 {
            eprintln!("[cap-verify] {run} rel {rel} daa {} accepted {} sent {} elapsed {:?}", s.daa(), accepted.len(), sent.len(), started.elapsed());
        }
    }
    // The latency tail: accept → licence and bind → licence, from the chain's own records.
    let st = s.state();
    let mut accept_to_licence = Vec::new();
    let mut licensed = 0usize;
    for (claim, at) in &accepted {
        if let Some(c) = st.claim(claim)
            && let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = c.phase
        {
            licensed += 1;
            accept_to_licence.push(licensed_daa - at);
        }
    }
    let mut blocks_entries: Vec<u64> = per_block.iter().map(|(_, n, _)| *n as u64).collect();
    out_line(format!(
        "{{\"run\":\"{run}\",\"extra_seats\":{extra_seats},\"daa\":{daa_len},\"accepted\":{},\"licensed\":{licensed},\"max_licences_a_block\":{max_entries},\"p50_licences_a_block\":{},\"blocks_carrying\":{},\"max_mass\":{},\"accept_to_licence_p50\":{},\"p90\":{},\"p99\":{},\"max\":{},\"subject\":\"{subject:?}\",\"elapsed_s\":{}}}",
        accepted.len(),
        percentile(&mut blocks_entries, 50),
        per_block.len(),
        per_block.iter().map(|(_, _, m)| *m).max().unwrap_or(0),
        percentile(&mut accept_to_licence.clone(), 50),
        percentile(&mut accept_to_licence.clone(), 90),
        percentile(&mut accept_to_licence.clone(), 99),
        accept_to_licence.iter().max().copied().unwrap_or(0),
        started.elapsed().as_secs()
    ));
    for (daa, n, mass) in &per_block {
        out_line(format!("{{\"run\":\"{run}\",\"block_daa\":{daa},\"licences\":{n},\"transient_mass\":{mass}}}"));
    }
    if batch {
        assert!(max_entries >= 30, "V-T2: {max_entries} coverage licences in the fullest block");
    }
}

/// **V-T3: c_8k at 16 ready seats, the measured speed and `k = 2`**, read through the chain's own
/// gates (the producer facts = the fold's reads): how many 8k claims the network admits at once
/// (every one by one uncontended 1M bond — past F-R its share is the whole room), with and without
/// F-R, over the eight genesis cards alone, with eight more 130k seats (SW-9's `ready_eff` counts them
/// by capped weight: ≈ 9), and with eight more genesis-sized seats (`ready_eff` 16).
#[tokio::test]
#[ignore = "a measurement run (ADR-0160 V-T3); run with --ignored"]
async fn adr0160_vt3_c8k_at_sixteen_ready_seats() {
    for armed in [false, true] {
        for (label, extra, msk) in [("8 cards", 0u64, 0u64), ("+8 x 130k", 8, 130_000), ("+8 x 939,063", 8, 939_063)] {
            let mut s = Sim::new(armed.then_some(0)).await;
            let k8 = s.k8.expect("testnet-12's 8k row");
            s.beat_to(34).await;
            if extra > 0 {
                s.plant_bonds(32, extra, msk, true);
            }
            s.refresh_8k();
            let subjects = s.plant_bonds(8, 2, 1_000_000, false);
            // One real 8k claim for the reservation the fold records (it holds a room slot too).
            let (template_id, created) = s.attempt(Who::Card(0), k8, Vec::new()).await;
            assert!(created, "a card's 8k claim");
            let template = s.state().claim(&template_id).expect("the template").clone();
            // Both subjects take claims, alternating, until neither may: the network's room is what
            // they hold plus the card's claim, and the last refusal names what bound.
            let mut held = [0u64; 2];
            let mut last = String::new();
            for round in 0..400u64 {
                let mut any = false;
                for (j, bond) in subjects.iter().enumerate() {
                    match s.ready(Who::Planted(8 + j as u64), k8) {
                        Ok(()) => {
                            s.plant_claims(k8, *bond, 1, s.daa(), &template, 0x8C00 + round * 4 + j as u64);
                            held[j] += 1;
                            any = true;
                        }
                        Err(why) => last = why,
                    }
                }
                if !any {
                    break;
                }
            }
            // Then the card that holds the template claim takes what its own share leaves: past F-R the
            // cap reserves each active holder's share, so the room is full only once every holder has
            // taken its part.
            let mut card_held = 1u64;
            for i in 0..400u64 {
                match s.ready(Who::Card(0), k8) {
                    Ok(()) => {
                        s.plant_claims(k8, s.chain.bonds[0], 1, s.daa(), &template, 0x9C00 + i);
                        card_held += 1;
                    }
                    Err(why) => {
                        last = why;
                        break;
                    }
                }
            }
            let facts = s.facts(Who::Planted(8), k8);
            let room_exhausted = last.contains("has no room");
            out_line(format!(
                "{{\"run\":\"vt3\",\"armed\":{armed},\"seats\":\"{label}\",\"c8k_network\":{},\"held\":{held:?},\"card_held\":{card_held},\"share_first\":{:?},\"room_bound\":{room_exhausted},\"last_refusal\":{:?}}}",
                held[0] + held[1] + card_held,
                facts.bond_class_share,
                last.chars().take(260).collect::<String>()
            ));
        }
    }
}

/// **V-T4: the split and the junk DoS on the 8k room**, past F-R at 16 ready seats: an honest 1M bond
/// against an attacker of 988,000 MSK as ONE bond, then as 76 bonds of 13,000 — the attacker fills
/// first, uncontended claims taken one at a time, each piece while its cap allows. Recorded: what the
/// attacker holds, what the honest bond may then hold, and the room. Pass: the honest bond reaches
/// `⌊s × room⌋` and the split holds no more than the whole plus one unit per piece.
#[tokio::test]
#[ignore = "a measurement run (ADR-0160 V-T4); run with --ignored"]
async fn adr0160_vt4_split_and_junk_on_the_8k_room() {
    for split in [false, true] {
        let mut s = Sim::new(Some(0)).await;
        let k8 = s.k8.expect("8k");
        s.beat_to(34).await;
        s.plant_bonds(32, 8, 939_063, true);
        s.refresh_8k();
        let honest = s.plant_bonds(8, 1, 1_000_000, false)[0];
        let attackers: Vec<(u64, PalwBondKeyV2)> = if split {
            s.plant_bonds(40, 76, 13_000, false).into_iter().enumerate().map(|(i, b)| (40 + i as u64, b)).collect()
        } else {
            s.plant_bonds(9, 1, 988_000, false).into_iter().map(|b| (9, b)).collect()
        };
        let (template_id, _) = s.attempt(Who::Card(0), k8, Vec::new()).await;
        let template = s.state().claim(&template_id).expect("template").clone();
        // The honest bond is active first with one claim (it won one race), then the attacker fills.
        s.plant_claims(k8, honest, 1, s.daa(), &template, 0x4000);
        let mut attacker_held = 0u64;
        for round in 0..200u64 {
            let mut any = false;
            for (i, (key, bond)) in attackers.iter().enumerate() {
                if s.ready(Who::Planted(*key), k8).is_ok() {
                    s.plant_claims(k8, *bond, 1, s.daa(), &template, 0x5000 + round * 256 + i as u64);
                    attacker_held += 1;
                    any = true;
                }
            }
            if !any {
                break;
            }
        }
        let mut honest_held = 1u64;
        while s.ready(Who::Planted(8), k8).is_ok() && honest_held < 400 {
            s.plant_claims(k8, honest, 1, s.daa(), &template, 0x6000 + honest_held);
            honest_held += 1;
        }
        let why = s.ready(Who::Planted(8), k8).err().unwrap_or_default();
        out_line(format!(
            "{{\"run\":\"vt4\",\"split\":{split},\"attacker_bonds\":{},\"attacker_msk\":988000,\"attacker_held\":{attacker_held},\"honest_msk\":1000000,\"honest_held\":{honest_held},\"card_template_claims\":1,\"stop\":{:?}}}",
            attackers.len(),
            why
        ));
    }
}

/// **V-T5: the floor room stops a flood before the seats saturate** (the capacity map's
/// `K_floor_big`), with F-R and without: a 1M pure producer floods floor claims (`CAP_PER_DAA`, 64)
/// while an honest 13k producer makes one a DAA; a card anchors every DAA; every due claim is licensed
/// (batch past F-B). Recorded per run: the flood's refusals by reason, every honest claim's fate —
/// pass: no honest `BindTimeout` with F-R.
#[tokio::test]
#[ignore = "a measurement run (ADR-0160 V-T5): tens of minutes; run with --ignored"]
async fn adr0160_vt5_the_floor_room_stops_the_flood() {
    let daa_len: u64 = env_or("CAP_DAA", 90);
    let per_daa: u64 = env_or("CAP_PER_DAA", 64);
    let armed = std::env::var("CAP_ARMED").map(|v| v != "0").unwrap_or(true);
    let run = if armed { "vt5-armed" } else { "vt5-released" };
    let mut s = Sim::new(armed.then_some(0)).await;
    let base = s.base;
    s.beat_to(34).await;
    // The flood's bond is large enough to outrun the seats' capital (10M MSK holds ≈ 1,560 unlicensed
    // floor claims; eight genesis seats bind ≈ 1,170 at once).
    s.plant_bonds(8, 1, env_or("CAP_BOND_MSK", 10_000_000), false);
    s.plant_bonds(9, 1, 13_000, false);
    let started = std::time::Instant::now();
    let mut sent: BTreeSet<Hash64> = BTreeSet::new();
    let mut honest: Vec<Hash64> = Vec::new();
    let mut flood_accepted = 0u64;
    let mut refusals: BTreeMap<String, u64> = BTreeMap::new();
    for rel in 0..daa_len {
        let card = (rel % 8) as usize;
        let txs = s.take_carriers();
        s.attempt(Who::Card(card), base, txs).await;
        match s.ready(Who::Planted(9), base) {
            Ok(()) => {
                let (claim, created) = s.attempt(Who::Planted(9), base, Vec::new()).await;
                if created {
                    honest.push(claim);
                }
            }
            Err(why) => *refusals.entry(format!("honest: {}", why.split(" (").next().unwrap_or(""))).or_default() += 1,
        }
        for _ in 0..per_daa {
            match s.ready(Who::Planted(8), base) {
                Ok(()) => {
                    let txs = s.take_carriers();
                    let (_, created) = s.attempt(Who::Planted(8), base, txs).await;
                    flood_accepted += created as u64;
                }
                Err(why) => {
                    let key = why.split(" (").next().unwrap_or("").chars().take(120).collect::<String>();
                    *refusals.entry(format!("flood: {key}")).or_default() += 1;
                    break;
                }
            }
        }
        let mut due = s.due(1, &sent);
        while !due.is_empty() {
            let object = if armed {
                match s.batch_licence(&due, 480_000 / 4 - 8_000) {
                    Some(o) => o,
                    None => break,
                }
            } else {
                s.single_licence(due[0])
            };
            let taken: Vec<Hash64> = match &object {
                Obj::ReceiptLicensedBatchV1 { entries, .. } => entries.iter().map(|e| e.claim).collect(),
                Obj::ReceiptLicensedV2 { claim, .. } => vec![*claim],
                _ => unreachable!(),
            };
            let Some(tx) = s.carrier(object) else { break };
            sent.extend(taken.iter().copied());
            due.retain(|c| !taken.contains(c));
            s.queued.push(tx);
        }
        s.beat().await;
        if rel % 10 == 0 {
            eprintln!("[cap-verify] {run} rel {rel} daa {} flood {flood_accepted} honest {} elapsed {:?}", s.daa(), honest.len(), started.elapsed());
        }
    }
    let st = s.state();
    let mut fates: BTreeMap<String, u64> = BTreeMap::new();
    for claim in &honest {
        let fate = match st.claim(claim).map(|c| c.phase.clone()) {
            Some(PalwClaimPhaseV2::Voided { reason, .. }) => format!("void {reason:?}"),
            Some(PalwClaimPhaseV2::Final { .. }) | None => "final/retired".to_string(),
            Some(PalwClaimPhaseV2::ReceiptLicensed { .. }) => "licensed".to_string(),
            Some(other) => format!("{other:?}").chars().take(20).collect(),
        };
        *fates.entry(fate).or_default() += 1;
    }
    let bind_timeouts: u64 = fates.iter().filter(|(k, _)| k.contains("BindTimeout")).map(|(_, v)| *v).sum();
    out_line(format!(
        "{{\"run\":\"{run}\",\"daa\":{daa_len},\"flood_accepted\":{flood_accepted},\"honest\":{},\"honest_fates\":{:?},\"refusals\":{:?},\"elapsed_s\":{}}}",
        honest.len(),
        fates,
        refusals,
        started.elapsed().as_secs()
    ));
    if armed {
        assert_eq!(bind_timeouts, 0, "V-T5: no honest BindTimeout past F-R");
    }
}

/// **V4: the attempt lane with parallel producers.** Each round, `n` producers (the cards and planted
/// producers) build their attempts on the SAME parents — siblings, as producers racing the same tip
/// are — all are inserted, and the next chain block merges them. testnet-12's two-minute profile runs
/// `ghostdag_k = 1`, so at most one sibling beside the selected parent is blue: counted, the claims the
/// chain made per round and per DAA at `n` = 4, 8 and 16 (`CAP_ROUNDS` rounds a DAA, default 4 — the
/// live 4.3–4.6 blocks a DAA).
#[tokio::test]
#[ignore = "a measurement run (ADR-0160 V4); run with --ignored"]
async fn adr0160_v4_attempt_lane_with_parallel_producers() {
    let rounds_per_daa: u64 = env_or("CAP_ROUNDS", 4);
    let daa_len: u64 = env_or("CAP_DAA", 6);
    for n in [4usize, 8, 16, 32] {
        let mut s = Sim::new(Some(0)).await;
        let base = s.base;
        s.beat_to(34).await;
        s.plant_bonds(8, 26, 1_000_000, false);
        // Card 0 merges each round; the racing producers are the other cards and planted bonds.
        let producers: Vec<Who> = (1..=n).map(|i| if i < 8 { Who::Card(i) } else { Who::Planted(i as u64) }).collect();
        let before = s.state().claims_iter().count();
        let start_daa = s.daa();
        let mut siblings_total = 0u64;
        for _ in 0..daa_len {
            for _ in 0..rounds_per_daa {
                let built: Vec<(MutableBlock, Hash64)> = producers.iter().map(|w| s.build_attempt(*w, base, Vec::new())).collect();
                for (block, _) in built {
                    let block = block.to_immutable();
                    s.chain.ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await.expect("a sibling attempt is valid");
                    s.blocks += 1;
                    siblings_total += 1;
                }
                // The next chain block merges the round: an attempt block (which does not move
                // testnet-12's clock), its own claim counted too. A block names at most
                // `max_block_parents` tips; the rest are merged by the next.
                s.attempt(Who::Card(0), base, Vec::new()).await;
            }
            s.beat().await;
        }
        let made = s.state().claims_iter().count() - before;
        let daa = s.daa() - start_daa;
        out_line(format!(
            "{{\"run\":\"v4-attempt-lane\",\"producers\":{n},\"rounds_per_daa\":{rounds_per_daa},\"daa\":{daa},\"attempts\":{siblings_total},\"claims\":{made},\"claims_per_round\":{:.2},\"claims_per_daa\":{:.2}}}",
            made as f64 / (daa_len * rounds_per_daa) as f64,
            made as f64 / daa.max(1) as f64
        ));
    }
}

/// **V4: the bind cost at 100 and 1,000 binds in one anchor block** — `n` floor claims planted
/// `Provisional` at one acceptance DAA (their anchor slot is one block), then the one attempt block
/// that binds them all, timed end to end (template, validation, the processor's derivation of every
/// panel, the fold). A debug build: the absolute times are an upper bound; the ratio is the point.
#[tokio::test]
#[ignore = "a measurement run (ADR-0160 V4); run with --ignored"]
async fn adr0160_v4_bind_cost_per_anchor_block() {
    for n in [100u64, 1_000] {
        let mut s = Sim::new(Some(0)).await;
        let base = s.base;
        s.beat_to(34).await;
        let subject = s.plant_bonds(8, 1, 10_000_000, false)[0];
        let (template_id, _) = s.attempt(Who::Card(0), base, Vec::new()).await;
        let template = s.state().claim(&template_id).expect("template").clone();
        let slot_base = s.daa();
        s.plant_claims(base, subject, n, slot_base, &template, 0xB1ED);
        let anchor_delay = s.chain.bundle.panel.anchor_delay();
        s.beat_to(slot_base + anchor_delay).await;
        let t0 = std::time::Instant::now();
        s.attempt(Who::Card(1), base, Vec::new()).await;
        let elapsed = t0.elapsed();
        let bound = s.state().claims_iter().filter(|(_, c)| matches!(c.phase, PalwClaimPhaseV2::PanelBound { .. })).count();
        out_line(format!(
            "{{\"run\":\"v4-bind-cost\",\"planted\":{n},\"bound_after_the_anchor\":{bound},\"anchor_block_ms\":{},\"per_bind_us\":{}}}",
            elapsed.as_millis(),
            elapsed.as_micros() / n.max(1) as u128
        ));
    }
}

/// **V-T3's throughput half: the 8k network at the ×10 room**, past F-R with 16 genesis-sized ready
/// seats (the eight cards and eight planted seats, each proving 8k readiness): `CAP_BONDS` pure
/// producers (default two of 1M) make 8k claims while the chain admits them for `CAP_DAA` DAA
/// (default 80); a card anchors every DAA; every due 8k claim is licensed by batch at bind +
/// `CAP_LIC_DELAY` (default 3, live testnet-12's 8k median). Counted: 8k claims accepted per 80 DAA
/// network-wide (the ADR's target ≈ 174 at c = 50) and what held the producers.
#[tokio::test]
#[ignore = "a measurement run (ADR-0160 V-T3 throughput): tens of minutes; run with --ignored"]
async fn adr0160_vt3_the_8k_network_at_the_x10_room() {
    let daa_len: u64 = env_or("CAP_DAA", 80);
    let lic_delay: u64 = env_or("CAP_LIC_DELAY", 3);
    let bonds: u64 = env_or("CAP_BONDS", 2);
    let armed = std::env::var("CAP_ARMED").map(|v| v != "0").unwrap_or(true);
    let mut s = Sim::new(armed.then_some(0)).await;
    let base = s.base;
    let k8 = s.k8.expect("8k");
    s.beat_to(34).await;
    s.plant_bonds(32, 8, 939_063, true);
    s.refresh_8k();
    s.plant_bonds(8, bonds, 1_000_000, false);
    let started = std::time::Instant::now();
    let mut sent: BTreeSet<Hash64> = BTreeSet::new();
    let mut accepted = 0u64;
    let mut holds: BTreeMap<String, u64> = BTreeMap::new();
    for rel in 0..daa_len {
        if rel % 12 == 0 {
            s.refresh_8k();
        }
        let txs = s.take_carriers();
        s.attempt(Who::Card((rel % 8) as usize), base, txs).await;
        for j in 0..bonds {
            for _ in 0..16 {
                match s.ready(Who::Planted(8 + j), k8) {
                    Ok(()) => {
                        let txs = s.take_carriers();
                        let (_, created) = s.attempt(Who::Planted(8 + j), k8, txs).await;
                        accepted += created as u64;
                    }
                    Err(why) => {
                        *holds.entry(why.split(" (").next().unwrap_or("").chars().take(100).collect()).or_default() += 1;
                        break;
                    }
                }
            }
        }
        let mut due = s.due(lic_delay, &sent);
        while !due.is_empty() {
            let Some(object) = s.batch_licence(&due, 480_000 / 4 - 8_000) else { break };
            let Obj::ReceiptLicensedBatchV1 { entries, .. } = &object else { unreachable!() };
            let taken: Vec<Hash64> = entries.iter().map(|e| e.claim).collect();
            let Some(tx) = s.carrier(object.clone()) else { break };
            sent.extend(taken.iter().copied());
            due.retain(|c| !taken.contains(c));
            s.queued.push(tx);
        }
        s.beat().await;
        if rel % 10 == 0 {
            eprintln!("[cap-verify] vt3-8k rel {rel} daa {} accepted {accepted} elapsed {:?}", s.daa(), started.elapsed());
        }
    }
    let per_80 = accepted as f64 * 80.0 / daa_len as f64;
    out_line(format!(
        "{{\"run\":\"vt3-8k-network\",\"armed\":{armed},\"daa\":{daa_len},\"bonds\":{bonds},\"lic_delay\":{lic_delay},\"accepted_8k\":{accepted},\"per_80_daa\":{per_80:.1},\"holds\":{holds:?},\"elapsed_s\":{}}}",
        started.elapsed().as_secs()
    ));
}
