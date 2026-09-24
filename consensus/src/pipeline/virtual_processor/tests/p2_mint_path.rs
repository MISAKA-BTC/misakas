//! **ADR-0152 Phase 2, P2-2 — the mint path, verified at the processor on testnet-12**
//! (phase2-plan F1/F2, §4: T03, T58 — the plan's T46, renumbered by ADR-0152 v3.1 — T47, T25,
//! T05's UTXO half and T29's queue half).
//!
//! Phase 1 proved the vesting rules through the fold (`palw_state_v2/tests/vesting_fold_v1.rs`:
//! T03's fold half, T30/T58, T47, T82). What only the processor can show is that the rows those
//! rules move reach REAL coinbases: a moved leg is minted by the next block's coinbase, byte for
//! byte on the build and the validate path, whatever lane the block rides. So every block here is
//! a real testnet-12 block — a heartbeat the node's own adapter shaped (algo 8, zero subsidy) or an
//! attempt a harness card signed (algo 6) — built from the node's template and accepted by the
//! node's own validation; a block whose template disagreed with validation would not become the
//! sink, and `T12Chain` demands that it does. That is T58's "template coinbase == validation
//! coinbase", for both kinds of block, on every block.
//!
//! **Two kinds of run.** T03 proper (`p2_t03_…`) plants nothing it measures: two floor claims go
//! through a real panel, real V2 receipts on funded carriers and the challenge window to `Final`;
//! one row matures and is minted, the other is burned by a real data-availability default (a signed
//! `DefaultAccused` nobody answers), a third claim voids `BindTimeout`, and the default's reporter
//! reward is minted to the accuser — every carve read off the coinbase that withheld it. The one
//! plant is the maturing row's re-key (its DAA clock and licences, through the carriage, as T29's
//! conviction twin does), because a real maturity is 3,000 DAA and thirty licences after `Final`.
//! The other runs PLANT rows — latched, maturing on the DAA clock with the second clock's licences
//! in, or held by the second clock — plus reporter awards and market rows into the tip
//! (`set_tip_for_tests`, through the carriage and its consistency load), with the settled-anchor
//! count raised to `SETTLED` so the second clock can be met or not per row, and from there on every
//! block's fold, step 3d and coinbase are the node's; their closing identity is conservation over
//! the planted rows plus the carves of the run's own attempt blocks.
//!
//! Every block is held to `Minting::after`:
//! 0. **the carve** — the coinbase withholds `palw_v2_escrow_withheld_at` of its selected parent,
//!    and for an attempt parent that is read off the coinbase itself: the attempt's miner is paid
//!    its worker base less exactly that (T03's withheld side);
//! 1. the coinbase carries the parent queue's first eight rows contiguously and in key order (by
//!    POSITION, which is how T03 attributes them — one payout script receives round fees, vesting
//!    mints and seat pay alike, phase2-plan §5.3), and every non-market key it mints is one an
//!    earlier step 3d moved a leg onto;
//! 2. the legs step 3d moved are exactly `palw_vesting_next_block_plan_v1` of the committed parent
//!    (the RPC's and this harness's one question, IA-6), and the rows it latched exactly those
//!    `palw_vesting_row_maturity_v1` calls mature — on every block the planner's own contract covers
//!    (no anchor settled, no row written, burned or re-keyed, no award, no market row that could move
//!    V-7's reserve); on the others each moved row moves whole;
//! 3. the queue lemma (the non-market part ≤ 8 after the fold);
//! 4. every moved key sits in the child's first eight at exactly the moved amount, so the next
//!    coinbase mints it;
//! 5. V-3's consistency;
//! 6. the books T03 closes from: the claims each block accepted, the row each `Final` wrote (its
//!    escrow the carve withheld), and the `BuybackAtFinal`, `ReserveCredited` and `Burned` notes.
use super::t12_round_lane_e2e::{
    T12Chain, card_payout_spk, sign_spend, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards,
};
use super::{OnetimeTxSelector, new_miner_data};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::processes::transaction_validator::tx_validation_in_utxo_context::TxValidationFlags;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::errors::tx::TxRuleError;
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::palw_economic_safety_v1::PalwLicenceDoorTagV1;
use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
};
use kaspa_consensus_core::palw_state_v2::{
    PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT, PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX, PALW_V2_MAX_PAYOUTS_PER_BLOCK,
    PALW_V2_MAX_PENDING_PAYOUTS, PalwBlockContextV2, PalwBondKeyV2, PalwCarrierRefundV1, PalwChainStateV2, PalwClaimPhaseV2,
    PalwConsensusObjectV2, PalwPayoutV2, PalwStateCarriageV2, PalwStateParamsV2, PalwVoidReasonV2, palw_da_accusation_message_v2,
    palw_model_refund_payout_key_v1,
};
use kaspa_consensus_core::palw_vesting_v1::{
    PALW_V2_VESTING_MARKET_RESERVE, PalwVestingLegV1, PalwVestingNoteV1, PalwVestingRowV1, PalwVestingSourceV1, PalwVestingStopV1,
    palw_chain_vesting_halted_v1, palw_vesting_consistency_v1, palw_vesting_market_rows_waiting_after_drain_v1,
    palw_vesting_next_block_plan_v1, palw_vesting_non_market_rows_waiting_v1, palw_vesting_notes_of_delta_v1,
    palw_vesting_payout_key_v1, palw_vesting_row_maturity_v1,
};
use kaspa_consensus_core::subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_LIFECYCLE};
use kaspa_consensus_core::tx::{
    MutableTransaction, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_hashes::Hash64;
use std::collections::{BTreeMap, BTreeSet};

/// The settled-anchor count every plant stands at: past testnet-12's depth (30), so a row written
/// at count 0 has its licences and one written at `SETTLED` has none (held by the second clock).
const SETTLED: u64 = 40;
/// Per-row amounts: the producer's leg is `PRODUCER + (row index)`, so no two rows mint the same
/// output; each credited seat `PER_SEAT`; then the reserve, the remainder named nowhere, and the
/// buyback slice (V-3: the row holds at most `escrowed − buyback`).
const PRODUCER: u64 = 3_000_000;
const PER_SEAT: u64 = 400_000;
const RESERVE: u64 = 7;
const UNNAMED: u64 = 13;
const BUYBACK: u64 = 5;
/// What a harness carrier pays in fees, and what a refused market buy sinks.
const CARRIER_FEE: u64 = 1_000_000;
const BUY_SINK: u64 = 100_000_000;

/// How a planted row stands against V-4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Clock {
    /// Latched already (X29): mature, waiting only for V-7's budget — a carried row.
    Latched,
    /// Unlatched; its second clock has its licences, so it latches when its DAA clock runs out.
    Licensed,
    /// Unlatched with no licence since `Final`: its DAA clock alone never matures it (T25).
    SecondClockHolds,
}

/// The facts a plant is written against.
struct Plant {
    /// The DAA the planted tip stands at.
    daa: u64,
    bonds: Vec<PalwBondKeyV2>,
    /// Each genesis card's payout payload.
    payloads: Vec<Hash64>,
}

impl Plant {
    /// A vesting row as `finalize_claim` writes one: `producer` paid `PRODUCER + n`, each of `seats`
    /// `PER_SEAT`, payees fixed at `Final` (I-5).
    fn row(&self, claim_id: Hash64, n: u64, producer: usize, seats: &[usize], expiry_daa: u64, clock: Clock) -> PalwVestingRowV1 {
        let producer_amount = PRODUCER + n;
        let named = producer_amount + PER_SEAT * seats.len() as u64 + RESERVE;
        PalwVestingRowV1 {
            claim_id,
            producer_bond: self.bonds[producer],
            class_id: Hash64::from_u64_word(0xC1A5),
            execution_root: Hash64::from_u64_word(0xE7EC_0000 + n),
            artifact_root: Hash64::default(),
            job_identity: Hash64::default(),
            free_prompt: false,
            trace_root: Hash64::default(),
            segment_count: 0,
            licence_door: PalwLicenceDoorTagV1::Quorum,
            basis_k: 3,
            escrowed_reward: named + UNNAMED + BUYBACK,
            buyback_bound: BUYBACK,
            producer: PalwPayoutV2 { payload: self.payloads[producer], amount: producer_amount },
            seats: seats.iter().map(|s| (self.bonds[*s], PalwPayoutV2 { payload: self.payloads[*s], amount: PER_SEAT })).collect(),
            reserve: RESERVE,
            final_daa: expiry_daa.saturating_sub(3_000),
            expiry_daa,
            settled_at_final: if clock == Clock::SecondClockHolds { SETTLED } else { 0 },
            matured_at: (clock == Clock::Latched).then_some(self.daa),
        }
    }
}

/// A market row's key: the market's `0xFF` prefix over a distinct hash (the refund and fee legs
/// key the same way).
fn market_key(i: u64) -> Hash64 {
    let mut bytes = Hash64::from_u64_word(0x4D00_0000 + i).as_bytes();
    bytes[0] = PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX;
    Hash64::from_bytes(bytes)
}

fn is_market(key: &Hash64) -> bool {
    key.as_byte_slice()[0] == PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX
}

fn output(payout: &PalwPayoutV2) -> TransactionOutput {
    TransactionOutput::new(payout.amount, p2pkh_mldsa87_spk(payout.payload.as_byte_slice()))
}

/// Card `card`'s ML-DSA-87 signature over `message` under `context` (the harness keys the genesis
/// cards carry, `t12_with_harness_cards`).
fn sign(card: usize, message: &[u8], context: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&TestConsensus::palw_v2_registry_keypair(card as u64).signing_key, message, context, [0x03u8; 32])
        .expect("ML-DSA-87 signs")
        .as_ref()
        .to_vec()
}

/// What the chain's coinbases minted, attributed by position against the parent queue's keys.
#[derive(Default, Debug)]
struct Ledger {
    rows: u128,
    reporters: u128,
    market: u128,
    reserve_credited: u128,
}

/// **T03's books**, kept block by block by `Minting::after` — everything the identity closes from,
/// read off real coinbases, real deltas and their notes.
#[derive(Default, Debug)]
struct Books {
    /// Σ what each mined coinbase withheld from its selected parent (`palw_v2_escrow_withheld_at`).
    withheld: u128,
    /// How many of those were read off the coinbase itself (an attempt parent).
    carves_read: usize,
    /// Every claim a block of the run accepted: `(escrowed_reward, accepted block, last phase seen)`.
    claims: BTreeMap<Hash64, (u64, BlockHash, PalwClaimPhaseV2)>,
    /// The row each `Final` of the run wrote, as written.
    finals: BTreeMap<Hash64, PalwVestingRowV1>,
    buyback_notes: u128,
    reserve_notes: u128,
    /// `Burned` rows, by claim, with the offence kind that burned them.
    burned: BTreeMap<Hash64, (u64, kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1)>,
    /// Reporter rewards awarded during the run (funded by slashes, outside the identity).
    awarded: u128,
    /// Market rows (carrier refunds) written during the run, by key (funded by sinks, outside it).
    market_written: BTreeMap<Hash64, PalwPayoutV2>,
    /// Blocks whose step 3d was held to the next-block plan exactly, and those outside its contract.
    exact: usize,
    relaxed: usize,
    /// Blocks in which step 3′ wrote a refund and step 3d moved a row.
    refunds_beside_moves: usize,
}

impl Books {
    /// One line for the log: the counts and totals, not the maps.
    fn summary(&self) -> String {
        let burned: u128 = self.burned.values().map(|(sompi, _)| *sompi as u128).sum();
        format!(
            "withheld {} ({} carves read off coinbases), {} claims, {} Finals, buyback {}, reserve {}, {} rows burned ({burned}), \
             awarded {}, {} refunds, plan exact on {} blocks / relaxed on {}, {} blocks with a refund beside a move",
            self.withheld,
            self.carves_read,
            self.claims.len(),
            self.finals.len(),
            self.buyback_notes,
            self.reserve_notes,
            self.burned.len(),
            self.awarded,
            self.market_written.len(),
            self.exact,
            self.relaxed,
            self.refunds_beside_moves
        )
    }
}

enum Kind {
    Heartbeat(Vec<Transaction>),
    Attempt(usize),
}

fn beat() -> Kind {
    Kind::Heartbeat(Vec::new())
}

/// One block's facts: the committed parent, the block, the child the node committed, the vesting
/// notes of the block's delta, and the claim an attempt block made.
struct Stepped {
    parent: PalwChainStateV2,
    block: Block,
    child: PalwChainStateV2,
    notes: Vec<PalwVestingNoteV1>,
    claim: Option<Hash64>,
}

impl Stepped {
    fn moved(&self) -> Vec<(PalwVestingSourceV1, Vec<PalwVestingLegV1>)> {
        self.notes
            .iter()
            .filter_map(|note| match note {
                PalwVestingNoteV1::Moved { source, legs } => Some((source.clone(), legs.clone())),
                _ => None,
            })
            .collect()
    }

    fn moved_row(&self, claim_id: &Hash64) -> bool {
        self.moved().iter().any(|(source, _)| *source == PalwVestingSourceV1::Row { claim_id: *claim_id })
    }
}

/// **Whether the committed parent's next-block plan must equal this block's step 3d exactly** —
/// `palw_vesting_next_block_plan_v1`'s own contract: the block settles no anchor, writes no row at a
/// `Final`, burns or re-keys none, awards no reporter reward, and writes no market row that could
/// move V-7's market reserve (a new `0xFF` row changes the budget only while fewer than two market
/// rows wait past the drain). Those are the inputs to step 3d only the block itself can move.
fn plan_is_exact(parent: &PalwChainStateV2, child: &PalwChainStateV2, notes: &[PalwVestingNoteV1]) -> bool {
    let parent_awards: BTreeSet<&Hash64> = parent.reporter_rewards_iter().map(|(k, _)| k).collect();
    child.settled_attempt_finals() == parent.settled_attempt_finals()
        && !notes.iter().any(|note| {
            matches!(
                note,
                PalwVestingNoteV1::Burned { .. } | PalwVestingNoteV1::ShareBurned { .. } | PalwVestingNoteV1::BuybackAtFinal { .. }
            ) || matches!(note, PalwVestingNoteV1::Moved { source: PalwVestingSourceV1::Reporter { offence_id }, .. }
                    if !parent_awards.contains(offence_id))
        })
        && child.reporter_rewards_iter().all(|(k, _)| parent_awards.contains(k))
        && child.vesting_iter_by_expiry().all(|row| {
            parent
                .vesting_row(&row.claim_id)
                .is_some_and(|was| (was.expiry_daa, was.settled_at_final) == (row.expiry_daa, row.settled_at_final))
        })
        && (palw_vesting_market_rows_waiting_after_drain_v1(parent) >= PALW_V2_VESTING_MARKET_RESERVE
            || child.pending_payouts_iter().all(|(k, _)| !is_market(k) || parent.pending_payout(k).is_some()))
}

struct Minting {
    chain: T12Chain,
    domain: Hash64,
    /// Each genesis card's payout payload.
    payloads: Vec<Hash64>,
    /// The state the books start from: the last plant, or the tip as mined.
    planted: PalwChainStateV2,
    /// The planted rows (T03's withheld-before-the-run side).
    rows: BTreeMap<Hash64, PalwVestingRowV1>,
    planted_reporters: u128,
    planted_market: u128,
    /// The queue keys a step 3d moved a row's leg onto (A-KEY producer keys, seat keys), and a
    /// reporter reward onto — learned from the `Moved` notes, so every non-market key a coinbase
    /// mints must have been moved first.
    row_keys: BTreeSet<Hash64>,
    reporter_keys: BTreeSet<Hash64>,
    ledger: Ledger,
    books: Books,
    /// Each card's spendable change: its genesis fee float, then each carrier's change.
    wallets: BTreeMap<usize, (TransactionOutpoint, UtxoEntry)>,
    /// The last block, if it was an attempt: its hash and card (its child's coinbase pays that card).
    last_attempt: Option<(BlockHash, usize)>,
    nonce: u64,
}

/// **testnet-12 as shipped (harness cards), one heartbeat in, nothing planted.**
async fn mined() -> Minting {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    assert!(bundle.state.rcore_plus_active_at(0), "testnet-12 arms R-core+ from genesis");
    assert_eq!(config.params.palw_settled_anchor_depth, Some(30), "testnet-12's second clock");
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    chain.heartbeat(config.params.target_time_per_block(), Vec::new()).await;
    let (_, tip) = chain.tip_state();
    assert!(tip.vesting_len() == 0 && tip.pending_payouts_iter().next().is_none());
    let payloads = chain.bonds.iter().map(|b| tip.bond(b).expect("a genesis card").payout_payload).collect();
    Minting {
        chain,
        domain,
        payloads,
        planted: tip,
        rows: BTreeMap::new(),
        planted_reporters: 0,
        planted_market: 0,
        row_keys: BTreeSet::new(),
        reporter_keys: BTreeSet::new(),
        ledger: Ledger::default(),
        books: Books::default(),
        wallets: floats.into_iter().enumerate().collect(),
        last_attempt: None,
        nonce: 0x9E_0000,
    }
}

/// [`mined`], with `edit` planted on the tip at `SETTLED` anchors.
async fn minting(edit: impl FnOnce(&Plant, &mut PalwStateCarriageV2)) -> Minting {
    let mut m = mined().await;
    m.plant(edit);
    m
}

impl Minting {
    fn sp(&self) -> &PalwStateParamsV2 {
        &self.chain.bundle.state
    }

    fn ttpb(&self) -> u64 {
        self.chain.config.params.target_time_per_block()
    }

    fn sink_daa(&self) -> u64 {
        self.chain.daa_of(self.chain.sink())
    }

    fn card_of(&self, bond: &PalwBondKeyV2) -> usize {
        self.chain.bonds.iter().position(|b| b == bond).expect("every bond here is a genesis card")
    }

    /// The raw second-clock depth a block at `daa` folds with (`palw_settled_anchor_depth_at`, I-8).
    fn raw_depth(&self, daa: u64) -> Option<u64> {
        let p = &self.chain.config.params;
        if p.palw_audit_2026_09_23.is_some_and(|f| f.is_active(daa)) { p.palw_settled_anchor_depth } else { None }
    }

    /// **Plant `edit` on the tip at `SETTLED` anchors, and start the books there.** The rows it adds
    /// are T03's withheld-before-the-run side; the reporter awards and market rows it adds are the
    /// planted halves of their own closures.
    fn plant(&mut self, edit: impl FnOnce(&Plant, &mut PalwStateCarriageV2)) {
        let (sink, tip) = self.chain.tip_state();
        let plant = Plant { daa: self.chain.daa_of(sink), bonds: self.chain.bonds.clone(), payloads: self.payloads.clone() };
        let mut carriage = PalwStateCarriageV2::from_state(&tip);
        carriage.settled_attempt_finals = SETTLED;
        let before = carriage.clone();
        edit(&plant, &mut carriage);
        let added: BTreeMap<Hash64, PalwVestingRowV1> =
            carriage.vesting.iter().filter(|(id, _)| !before.vesting.contains_key(id)).map(|(id, row)| (*id, row.clone())).collect();
        // V-3: the counters name what the rows hold.
        carriage.vesting_counters.created += added.values().map(PalwVestingRowV1::total_sompi_u128).sum::<u128>();
        self.planted_reporters += carriage
            .reporter_rewards
            .iter()
            .filter(|(k, _)| !before.reporter_rewards.contains_key(k))
            .map(|(_, p)| p.amount as u128)
            .sum::<u128>();
        self.planted_market += carriage
            .pending_payouts
            .iter()
            .filter(|(k, _)| !before.pending_payouts.contains_key(k))
            .map(|(_, p)| p.amount as u128)
            .sum::<u128>();
        self.rows.extend(added);
        let planted: PalwChainStateV2 =
            carriage.into_state(&self.chain.bundle.state, None).expect("the planted tip is a consistent state");
        palw_vesting_consistency_v1(&planted).expect("the plant satisfies V-3");
        self.chain.vp().palw_state_v2_store.write().set_tip_for_tests(sink, &planted).expect("the plant becomes the tip");
        self.planted = planted;
    }

    /// **Re-key a row on the tip, through the carriage** — the one plant T03 proper makes (see the
    /// module doc): nothing in the books moves, only what `edit` writes.
    fn rekey(&mut self, edit: impl FnOnce(&mut PalwStateCarriageV2)) {
        let (sink, tip) = self.chain.tip_state();
        let mut carriage = PalwStateCarriageV2::from_state(&tip);
        edit(&mut carriage);
        let rekeyed: PalwChainStateV2 = carriage.into_state(&self.chain.bundle.state, None).expect("the re-keyed tip is consistent");
        palw_vesting_consistency_v1(&rekeyed).expect("a re-key keeps V-3");
        self.chain.vp().palw_state_v2_store.write().set_tip_for_tests(sink, &rekeyed).expect("the re-key becomes the tip");
    }

    /// Mine one block of `kind` and hold it to `after`'s checks.
    async fn step(&mut self, kind: Kind) -> Stepped {
        let (parent_hash, parent) = self.chain.tip_state();
        let step = self.ttpb();
        let (block, claim, card) = match kind {
            Kind::Heartbeat(txs) => (self.chain.heartbeat(step, txs).await, None, None),
            Kind::Attempt(card) => {
                let (block, claim) = self.chain.attempt(card, step, Vec::new(), &|_| true).await;
                (block, Some(claim), Some(card))
            }
        };
        let stepped = self.after(parent_hash, parent, block, claim);
        self.last_attempt = card.map(|card| (stepped.block.header.hash, card));
        stepped
    }

    /// The checks of the module doc, on a block already inserted as the sink.
    fn after(&mut self, parent_hash: BlockHash, parent: PalwChainStateV2, block: Block, claim: Option<Hash64>) -> Stepped {
        let hash = block.header.hash;
        let daa = block.header.daa_score;
        let what = format!("block {hash} (DAA {daa}, algo {})", block.header.pow_algo_id);
        let (tip_block, child) = self.chain.tip_state();
        assert_eq!(tip_block, hash, "{what}: the walk left the PALW tip at the block");
        let vp = self.chain.vp();
        assert_eq!(vp.ghostdag_store.get_selected_parent(hash).unwrap(), parent_hash, "{what}: one chain");
        let (_, delta) = vp.palw_state_v2_store.read().delta_of(hash).expect("the block's delta row");
        let stepped = Stepped { parent, block, child, notes: palw_vesting_notes_of_delta_v1(&delta).cloned().collect(), claim };
        let (parent, child) = (&stepped.parent, &stepped.child);
        let outputs = &stepped.block.transactions[0].outputs;

        // (1) The coinbase mints the parent queue's first eight rows, contiguously and in key order;
        //     each output is attributed by its POSITION to the parent key it renders.
        let prefix: Vec<(Hash64, PalwPayoutV2)> =
            parent.pending_payouts_iter().take(PALW_V2_MAX_PAYOUTS_PER_BLOCK).map(|(k, p)| (*k, *p)).collect();
        let mut rendered_at = 0..0;
        if !prefix.is_empty() {
            let rendered: Vec<TransactionOutput> = prefix.iter().map(|(_, p)| output(p)).collect();
            let at = outputs
                .windows(rendered.len())
                .position(|window| window == rendered.as_slice())
                .unwrap_or_else(|| panic!("{what}: the coinbase does not carry the parent queue's first {} rows", rendered.len()));
            rendered_at = at..at + rendered.len();
            for (j, (key, payout)) in prefix.iter().enumerate() {
                assert_eq!(outputs[at + j].value, payout.amount);
                if is_market(key) {
                    self.ledger.market += payout.amount as u128;
                } else if self.reporter_keys.contains(key) {
                    self.ledger.reporters += payout.amount as u128;
                } else {
                    assert!(
                        self.row_keys.contains(key),
                        "{what}: a non-market key is one a step 3d moved a row's leg onto, key {key}"
                    );
                    self.ledger.rows += payout.amount as u128;
                }
            }
        }

        // (0) T03's withheld side: the carve this coinbase withholds from its selected parent — for
        //     an attempt parent, read off the coinbase: the attempt's miner is paid its worker base
        //     less exactly the carve (outside the queue's rendered rows, which may pay it too).
        let withheld = vp.palw_v2_escrow_withheld_at(parent, parent_hash);
        self.books.withheld += withheld as u128;
        if let Some((attempt, card)) = self.last_attempt
            && attempt == parent_hash
        {
            let parent_daa = self.chain.daa_of(parent_hash);
            let split = vp.fee_split_at(parent_daa).expect("the overlay split");
            let base =
                kaspa_consensus_core::dns_finality::split_block_subsidy(vp.coinbase_manager.calc_block_subsidy(parent_daa), &split)
                    .worker_base_sompi;
            let spk = card_payout_spk(card);
            let paid: u64 = outputs
                .iter()
                .enumerate()
                .filter(|(i, o)| !rendered_at.contains(i) && o.script_public_key == spk)
                .map(|(_, o)| o.value)
                .sum();
            assert!(withheld > 0, "{what}: a testnet-12 attempt escrows a carve");
            assert_eq!(paid, base - withheld, "{what}: the coinbase pays card {card}'s attempt its worker base less the carve");
            self.books.carves_read += 1;
        }

        // (2) Step 3d moved exactly what the committed parent predicts (IA-6), and latched exactly
        //     the rows V-4 calls mature at this DAA — wherever the planner's contract covers the
        //     block; elsewhere every moved row moves whole.
        let raw = self.raw_depth(daa);
        if plan_is_exact(parent, child, &stepped.notes) {
            let predicted = palw_vesting_next_block_plan_v1(parent, self.sp(), daa, raw);
            let want: Vec<_> = predicted.moves.iter().map(|m| (m.source.clone(), m.legs.clone())).collect();
            assert_eq!(
                stepped.moved(),
                want,
                "{what}: the next-block plan of the committed parent is the fold's (stopped {:?})",
                predicted.stopped
            );
            let latched: BTreeSet<Hash64> = stepped
                .notes
                .iter()
                .filter_map(|note| match note {
                    PalwVestingNoteV1::Latched { claim_id, matured_at } => {
                        assert_eq!(*matured_at, daa, "{what}: a row latches at its block's DAA");
                        Some(*claim_id)
                    }
                    _ => None,
                })
                .collect();
            let mature: BTreeSet<Hash64> = parent
                .vesting_iter_by_expiry()
                .filter(|row| row.matured_at.is_none() && palw_vesting_row_maturity_v1(parent, self.sp(), row, daa, raw).mature_now)
                .map(|row| row.claim_id)
                .collect();
            assert_eq!(latched, mature, "{what}: step 3d latches exactly the rows V-4 calls mature");
            self.books.exact += 1;
        } else {
            for (source, legs) in stepped.moved() {
                if let PalwVestingSourceV1::Row { claim_id } = source {
                    let row =
                        parent.vesting_row(&claim_id).unwrap_or_else(|| panic!("{what}: moved row {claim_id} stood in the parent"));
                    let moved: u128 = legs.iter().map(|leg| leg.amount as u128).sum();
                    assert_eq!(moved, row.total_sompi_u128(), "{what}: a row moves whole");
                }
            }
            self.books.relaxed += 1;
        }

        // (3) The queue lemma.
        assert!(
            palw_vesting_non_market_rows_waiting_v1(child) <= PALW_V2_MAX_PAYOUTS_PER_BLOCK,
            "{what}: the non-market part of the queue is {} rows",
            palw_vesting_non_market_rows_waiting_v1(child)
        );

        // (4) Every moved key is in the child's first eight, at exactly what this block moved onto
        //     it: the next coinbase mints it, whole.
        let mut owed: BTreeMap<Hash64, (Hash64, u64)> = BTreeMap::new();
        for (source, legs) in stepped.moved() {
            for leg in legs.iter().filter(|leg| leg.takes_budget()) {
                let key = leg.queue_key.expect("a budget-taking leg has a key");
                match source {
                    PalwVestingSourceV1::Row { .. } => self.row_keys.insert(key),
                    PalwVestingSourceV1::Reporter { .. } => self.reporter_keys.insert(key),
                };
                let entry = owed.entry(key).or_insert((leg.payload, 0));
                assert_eq!(entry.0, leg.payload, "{what}: one key, one payee");
                entry.1 += leg.amount;
            }
            self.ledger.reserve_credited +=
                legs.iter().filter(|leg| leg.queue_key.is_none()).map(|leg| leg.amount as u128).sum::<u128>();
        }
        let child_prefix: BTreeSet<Hash64> =
            child.pending_payouts_iter().take(PALW_V2_MAX_PAYOUTS_PER_BLOCK).map(|(k, _)| *k).collect();
        for (key, (payload, amount)) in &owed {
            assert!(child_prefix.contains(key), "{what}: moved key {key} is not in the prefix the next coinbase renders");
            assert_eq!(child.pending_payout(key), Some(&PalwPayoutV2 { payload: *payload, amount: *amount }), "{what}: moved whole");
        }

        // (5) V-3.
        palw_vesting_consistency_v1(child).unwrap_or_else(|why| panic!("{what}: V-3: {why}"));

        // (6) The books.
        self.book(&what, hash, daa, &stepped);
        stepped
    }

    /// (6): what this block accepted, finalized, burned, awarded and wrote to the market.
    fn book(&mut self, what: &str, hash: BlockHash, daa: u64, stepped: &Stepped) {
        let (parent, child) = (&stepped.parent, &stepped.child);
        for (id, claim) in child.claims_iter() {
            if parent.claim(id).is_none() {
                assert_eq!(claim.accepted_block, hash, "{what}: a claim is accepted in its own attempt block");
                self.books.claims.insert(*id, (claim.escrowed_reward, claim.accepted_block, claim.phase.clone()));
            } else if let Some(entry) = self.books.claims.get_mut(id) {
                entry.2 = claim.phase.clone();
            }
        }
        for row in child.vesting_iter_by_expiry() {
            if parent.vesting_row(&row.claim_id).is_none() {
                assert_eq!(row.final_daa, daa, "{what}: a new row is a Final of this block");
                assert!(matches!(child.claim(&row.claim_id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Final { .. })));
                let (escrow, ..) = self.books.claims.get(&row.claim_id).expect("a Final of the run is a claim of the run");
                assert_eq!(row.escrowed_reward, *escrow, "{what}: the row names the escrow its carrying block's child withheld");
                self.books.finals.insert(row.claim_id, row.clone());
            }
        }
        let parent_awards: BTreeSet<&Hash64> = parent.reporter_rewards_iter().map(|(k, _)| k).collect();
        for note in &stepped.notes {
            match note {
                PalwVestingNoteV1::BuybackAtFinal { claim_id, sompi } => {
                    assert_eq!(
                        self.books.finals.get(claim_id).map(|row| row.buyback_bound),
                        Some(*sompi),
                        "{what}: V-3's buyback slice"
                    );
                    self.books.buyback_notes += *sompi as u128;
                }
                PalwVestingNoteV1::ReserveCredited { sompi, .. } => self.books.reserve_notes += *sompi as u128,
                PalwVestingNoteV1::Burned { claim_id, sompi, kind, .. } => {
                    self.books.burned.insert(*claim_id, (*sompi, *kind));
                }
                PalwVestingNoteV1::ShareBurned { claim_id, sompi, .. } => {
                    panic!("{what}: no seat share of {claim_id} burns here ({sompi})")
                }
                // An award written at step 2 and moved by step 3d in the same block never shows in
                // `reporter_rewards`: it is counted from its move.
                PalwVestingNoteV1::Moved { source: PalwVestingSourceV1::Reporter { offence_id }, legs }
                    if !parent_awards.contains(offence_id) =>
                {
                    self.books.awarded += legs.iter().map(|leg| leg.amount as u128).sum::<u128>();
                }
                _ => {}
            }
        }
        self.books.awarded +=
            child.reporter_rewards_iter().filter(|(k, _)| !parent_awards.contains(k)).map(|(_, p)| p.amount as u128).sum::<u128>();
        let mut refunded = false;
        for (key, payout) in child.pending_payouts_iter().filter(|(k, _)| is_market(k) && parent.pending_payout(k).is_none()) {
            self.books.market_written.insert(*key, *payout);
            refunded = true;
        }
        if refunded && stepped.moved().iter().any(|(source, _)| matches!(source, PalwVestingSourceV1::Row { .. })) {
            self.books.refunds_beside_moves += 1;
        }
    }

    /// A heartbeat template the node builds and its adapter shapes — not inserted — so a test can
    /// read the coinbase outpoints of a block before that block exists.
    fn heartbeat_template(&mut self) -> MutableBlock {
        self.chain.ctx.simulated_time += self.ttpb();
        self.nonce += 1;
        let mut t = self
            .chain
            .ctx
            .consensus
            .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
            .expect("a template");
        stamp_harness_time(&self.chain.config.params, &mut t.block.header, self.chain.ctx.simulated_time);
        t.block.header.nonce = self.nonce;
        t.block.header.finalize();
        let (t, _) = self.chain.vp().heartbeat_adapt_block_template(t).expect("the heartbeat lane is open on testnet-12");
        self.chain.ctx.simulated_time = self.chain.ctx.simulated_time.max(t.block.header.timestamp);
        t.block
    }

    /// Insert a block built by [`Self::heartbeat_template`] and hold it to `after`'s checks.
    async fn insert(&mut self, block: MutableBlock) -> Stepped {
        let (parent_hash, parent) = self.chain.tip_state();
        let block = block.to_immutable();
        let hash = block.header.hash;
        self.chain
            .ctx
            .consensus
            .validate_and_insert_block(block.clone())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("block {hash} was refused: {e}"));
        assert_eq!(self.chain.ctx.consensus.block_status(hash), BlockStatus::StatusUTXOValid);
        assert_eq!(self.chain.sink(), hash, "the block is the sink");
        let stepped = self.after(parent_hash, parent, block, None);
        self.last_attempt = None;
        stepped
    }

    /// Whether any virtual UTXO pays `amount` to `payload`.
    fn a_utxo_pays(&self, payout: &PalwPayoutV2) -> bool {
        let spk = p2pkh_mldsa87_spk(payout.payload.as_byte_slice());
        self.chain
            .ctx
            .consensus
            .get_virtual_utxos(None, usize::MAX, false)
            .into_iter()
            .any(|(_, entry)| entry.script_public_key == spk && entry.amount == payout.amount)
    }

    /// **A funded, signed 0x4b carrier of `object`, paid from `payer`'s wallet**, change at output 0
    /// back to the payer's own script and `sink` (a market buy's) at output 1 — as `misaka palw`
    /// builds one. The change becomes the payer's wallet once a block accepts the carrier.
    fn carrier(&mut self, payer: usize, object: PalwConsensusObjectV2, sink: Option<TransactionOutput>) -> Transaction {
        let (outpoint, entry) = self.wallets.remove(&payer).unwrap_or_else(|| panic!("card {payer} has a wallet"));
        let sunk = sink.as_ref().map_or(0, |o| o.value);
        let change = entry.amount - sunk - CARRIER_FEE;
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            std::iter::once(TransactionOutput::new(change, card_payout_spk(payer))).chain(sink).collect(),
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        sign_spend(&mut tx, entry, payer, self.chain.config.params.storage_mass_parameter);
        self.wallets.insert(payer, (TransactionOutpoint::new(tx.id(), 0), UtxoEntry::new(change, card_payout_spk(payer), 0, false)));
        tx
    }

    /// A market buy the fold refuses (`min_units_out = u64::MAX` on the floor class, which has no
    /// market), carried by `payer`: step 3′ of the block that accepts it owes the payer its sink back
    /// (P-B1), keyed `palw_model_refund_payout_key_v1(carrier)`.
    fn refused_buy(&mut self, payer: usize, n: u64) -> Transaction {
        let line = self.chain.bundle.base_class_id;
        let object = PalwConsensusObjectV2::ModelBuy {
            line_id: line,
            holder: Hash64::from_u64_word(0xB0B0 + n),
            msk_in: BUY_SINK,
            min_units_out: u64::MAX,
            sink_index: 1,
        };
        let sink = TransactionOutput::new(BUY_SINK, kaspa_consensus_core::palw_model_market_v1::palw_model_sink_spk_v1(&line));
        self.carrier(payer, object, Some(sink))
    }

    /// Heartbeats until `done` holds of the tip, at most `cap`.
    async fn beat_until(&mut self, cap: u64, what: &str, done: impl Fn(&PalwChainStateV2) -> bool) {
        for _ in 0..cap {
            if done(&self.chain.tip_state().1) {
                return;
            }
            self.step(beat()).await;
        }
        assert!(done(&self.chain.tip_state().1), "{what}: not reached in {cap} heartbeats (sink DAA {})", self.sink_daa());
    }

    /// **The block that binds `claim_id`'s panel** (SW-8, IA-1a): heartbeats to the claim's anchor
    /// slot, then card `card`'s attempt — which binds it and makes a claim of its own, returned.
    async fn attempt_at_the_anchor_slot(&mut self, claim_id: Hash64, card: usize) -> Hash64 {
        let slot = self.chain.tip_state().1.claim(&claim_id).expect("the claim exists").bind_base_daa()
            + self.chain.bundle.panel.anchor_delay();
        while self.sink_daa() < slot {
            self.step(beat()).await;
        }
        let stepped = self.step(Kind::Attempt(card)).await;
        assert!(
            matches!(stepped.child.claim(&claim_id).map(|c| &c.phase), Some(PalwClaimPhaseV2::PanelBound { .. })),
            "claim {claim_id} binds in its anchor block"
        );
        stepped.claim.expect("an attempt block makes a claim")
    }

    /// **A quorum of `claim_id`'s panel signs `Valid`** (V2 receipts, through the node's own
    /// assembler) and the `ReceiptLicensed` rides a carrier paid by `payer`: the block carrying it,
    /// then the block accepting it.
    async fn license(&mut self, claim_id: Hash64, payer: usize) {
        let panel = self.chain.tip_state().1.panel(&claim_id).expect("a bound claim has a panel").clone();
        let signed_daa = self.chain.ctx.consensus.get_virtual_daa_score();
        let message = palw_receipt_message_v2(self.domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
        let receipts: Vec<PalwSeatReceiptV2> = panel
            .seats
            .iter()
            .take(self.chain.bundle.panel.quorum() as usize)
            .map(|seat| PalwSeatReceiptV2 {
                claim: claim_id,
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: seat.bond,
                signed_daa,
                signature: sign(self.card_of(&seat.bond), message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
            })
            .collect();
        let object = self.chain.vp().palw_v2_receipt_quorum_assemble_impl(claim_id, &receipts).expect("a signed quorum assembles");
        let carrier = self.carrier(payer, object, None);
        let carrying = self.step(Kind::Heartbeat(vec![carrier.clone()])).await;
        assert!(carrying.block.transactions.iter().any(|tx| tx.id() == carrier.id()), "the carrier is in the block");
        let accepted = self.step(beat()).await;
        assert!(
            matches!(accepted.child.claim(&claim_id).map(|c| &c.phase), Some(PalwClaimPhaseV2::ReceiptLicensed { .. })),
            "the carried quorum licenses claim {claim_id}"
        );
    }

    /// **T03, the coinbase-level identity** (ADR-0152 V-3, phase2-plan §5.3), closed at the tip:
    ///
    /// `Σ planted rows' escrow + Σ escrow of the claims the run accepted` (the latter what the
    /// run's coinbases withheld, plus the carve of a claim accepted in the last block, which the
    /// next coinbase withholds) `= Σ minted from rows (by coinbase position) + Σ moved and not yet
    /// minted + Σ live rows + Σ burned rows + Σ voided claims' escrow + Σ unresolved claims' escrow
    /// + Σ unnamed + Σ buyback + Σ reserve credited` — with the reporter and market mints OUTSIDE
    /// it (funded by slashes and sinks) and closing on their own.
    fn t03_closes(&self) -> T03 {
        let (sink, end) = self.chain.tip_state();
        let claims: u128 = self.books.claims.values().map(|(escrow, ..)| *escrow as u128).sum();
        let pending: u128 = self.books.claims.values().filter(|(_, at, _)| *at == sink).map(|(escrow, ..)| *escrow as u128).sum();
        assert_eq!(self.books.withheld, claims - pending, "every carve a coinbase withheld is the escrow of a claim (none skipped)");
        let planted: u128 = self.rows.values().map(|row| row.escrowed_reward as u128).sum();
        let rows = || self.rows.values().chain(self.books.finals.values());
        let buyback: u128 = rows().map(|row| row.buyback_bound as u128).sum();
        let unnamed: u128 = rows().map(|row| (row.escrowed_reward - row.buyback_bound) as u128 - row.total_sompi_u128()).sum();
        let finals_buyback: u128 = self.books.finals.values().map(|row| row.buyback_bound as u128).sum();
        assert_eq!(finals_buyback, self.books.buyback_notes, "a Final's buyback slice is its note");
        let live: u128 = end.vesting_iter_by_expiry().map(PalwVestingRowV1::total_sompi_u128).sum();
        let burned = end.vesting_counters().burned - self.planted.vesting_counters().burned;
        assert_eq!(
            burned,
            self.books.burned.values().map(|(sompi, _)| *sompi as u128).sum::<u128>(),
            "every burned row is a Burned note"
        );
        let reserve = (end.panel_reserve_sompi() - self.planted.panel_reserve_sompi()) as u128;
        assert_eq!(reserve, self.ledger.reserve_credited, "the reserve legs moved are what panel_reserve_sompi gained");
        assert_eq!(reserve, self.books.reserve_notes, "…and what the ReserveCredited notes name");
        let in_flight = |keys: &BTreeSet<Hash64>| -> u128 {
            end.pending_payouts_iter().filter(|(k, _)| keys.contains(k)).map(|(_, p)| p.amount as u128).sum()
        };
        let rows_in_flight = in_flight(&self.row_keys);
        let (mut voided, mut unresolved) = (0u128, 0u128);
        for (id, (escrow, _, phase)) in &self.books.claims {
            if self.books.finals.contains_key(id) {
                continue; // its escrow is its row's
            }
            match phase {
                PalwClaimPhaseV2::Voided { .. } => voided += *escrow as u128,
                PalwClaimPhaseV2::Final { .. } => assert_eq!(*escrow, 0, "claim {id}: a Final with an escrow wrote a row"),
                _ => unresolved += *escrow as u128,
            }
        }
        assert_eq!(
            planted + claims,
            self.ledger.rows + rows_in_flight + live + burned + voided + unresolved + unnamed + buyback + reserve,
            "T03: withheld = minted from rows + in flight + live + burned + voided + unresolved + unnamed + buyback + reserve \
             ({:?}, {:?})",
            self.ledger,
            self.books
        );
        let moved = end.vesting_counters().moved - self.planted.vesting_counters().moved;
        assert_eq!(moved, self.ledger.rows + rows_in_flight + reserve, "every sompi moved is minted, in flight, or the reserve");
        // Outside the identity, and closing on their own.
        let reporters_waiting: u128 = end.reporter_rewards_iter().map(|(_, p)| p.amount as u128).sum();
        assert_eq!(
            self.planted_reporters + self.books.awarded,
            self.ledger.reporters + in_flight(&self.reporter_keys) + reporters_waiting,
            "every reporter award is minted, in flight, or waiting"
        );
        let market_left: u128 = end.pending_payouts_iter().filter(|(k, _)| is_market(k)).map(|(_, p)| p.amount as u128).sum();
        let written: u128 = self.books.market_written.values().map(|p| p.amount as u128).sum();
        assert_eq!(self.planted_market + written, self.ledger.market + market_left, "the market rows are minted or still queued");
        T03 { burned, voided, unresolved, reserve }
    }
}

/// The terms of T03 a test asserts by name.
#[derive(Debug)]
struct T03 {
    burned: u128,
    voided: u128,
    unresolved: u128,
    reserve: u128,
}

/// A small deterministic generator (xorshift64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// One randomized chain (see the test). `refunds` carries two refused market buys in each of three
/// heartbeat blocks, so step 3′'s refunds land beside step 3d's moves on mined blocks.
async fn randomized_run(seed: u64, market_rows: u64, refunds: bool) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    // Drawn before the plant so the closure can own them.
    let n_rows = 12 + rng.below(6);
    let mut shapes: Vec<(usize, Vec<usize>, i64, Clock)> = Vec::new();
    for i in 0..n_rows {
        let producer = rng.below(8) as usize;
        let mut pool: Vec<usize> = (0..8).collect();
        let mut seats = Vec::new();
        for _ in 0..rng.below(6) {
            seats.push(pool.remove(rng.below(pool.len() as u64) as usize));
        }
        // A burst of four rows on one DAA; a row the second clock holds in the middle of the order,
        // and two licensed rows behind it (stop, never skip); the rest carried or maturing.
        let (offset, clock) = match i {
            0..=3 => (3, Clock::Licensed),
            4 => (8, Clock::SecondClockHolds),
            5 | 6 => (9, Clock::Licensed),
            _ => match rng.below(3) {
                0 => (-1, Clock::Latched),
                _ => (1 + rng.below(7) as i64, Clock::Licensed),
            },
        };
        shapes.push((producer, seats, offset, clock));
    }
    let n_reporters = 1 + rng.below(3);
    let reporter_payees: Vec<usize> = (0..n_reporters).map(|_| rng.below(8) as usize).collect();
    let mut m = minting(|p, c| {
        for (n, (producer, seats, offset, clock)) in shapes.iter().enumerate() {
            let claim_id = Hash64::from_u64_word((seed << 16) | 0x5E00 | n as u64);
            let expiry = (p.daa as i64 + offset).max(0) as u64;
            c.vesting.insert(claim_id, p.row(claim_id, n as u64, *producer, seats, expiry, *clock));
        }
        for (j, payee) in reporter_payees.iter().enumerate() {
            let offence = Hash64::from_u64_word((seed << 16) | 0x6E00 | j as u64);
            c.reporter_rewards.insert(offence, PalwPayoutV2 { payload: p.payloads[*payee], amount: 250_000 + j as u64 });
        }
        for i in 0..market_rows {
            c.pending_payouts.insert(market_key(i), PalwPayoutV2 { payload: p.payloads[7], amount: 10_000 + i });
        }
    })
    .await;
    // Heartbeats, with two attempt blocks by two cards, and — `refunds` — refused buys in three.
    let attempts = [(4usize, (seed as usize + 1) % 8), (11, (seed as usize + 5) % 8)];
    let carrying: [(usize, [usize; 2]); 3] = [(2, [0, 1]), (5, [2, 3]), (8, [5, 6])];
    let mut carriers: Vec<(Transaction, usize)> = Vec::new();
    let mut steps = Vec::new();
    for b in 0..32 {
        let kind = match (attempts.iter().find(|(at, _)| *at == b), carrying.iter().find(|(at, _)| refunds && *at == b)) {
            (Some((_, card)), _) => Kind::Attempt(*card),
            (None, Some((_, payers))) => {
                let txs: Vec<Transaction> = payers
                    .iter()
                    .map(|payer| {
                        let tx = m.refused_buy(*payer, carriers.len() as u64);
                        carriers.push((tx.clone(), *payer));
                        tx
                    })
                    .collect();
                Kind::Heartbeat(txs)
            }
            (None, None) => beat(),
        };
        steps.push(m.step(kind).await);
    }
    let end = steps.last().expect("a chain").child.clone();

    // Stop, never skip: every row ahead of the held one moved, the held one never latched, and the
    // rows behind it — latched or not — wait behind it.
    let held =
        m.rows.values().find(|row| row.settled_at_final == SETTLED).expect("the plant holds one row on the second clock").clone();
    let head: Vec<Hash64> =
        m.planted.vesting_iter_by_expiry().take_while(|row| row.claim_id != held.claim_id).map(|row| row.claim_id).collect();
    let tail: Vec<Hash64> = m.planted.vesting_iter_by_expiry().skip(head.len() + 1).map(|row| row.claim_id).collect();
    assert!(!head.is_empty() && !tail.is_empty(), "seed {seed:#x}: the plant has rows on both sides of the held one");
    for id in &head {
        assert!(end.vesting_row(id).is_none(), "seed {seed:#x}: row {id} ahead of the held row has moved");
        assert!(steps.iter().any(|s| s.moved_row(id)), "seed {seed:#x}: through a Moved note");
    }
    let row = end.vesting_row(&held.claim_id).expect("the held row stays");
    assert_eq!(row.matured_at, None, "seed {seed:#x}: the second clock holds it past its DAA clock (T25)");
    assert!(end.last_point().expect("a point").daa_score >= held.expiry_daa, "seed {seed:#x}: its DAA clock has run");
    for id in &tail {
        assert!(end.vesting_row(id).is_some(), "seed {seed:#x}: row {id} behind the held row waits");
    }
    assert!(end.reporter_rewards_iter().next().is_none(), "seed {seed:#x}: the reporter awards move first");
    // The lanes both minted: an attempt block's coinbase checks passed like a heartbeat's, and each
    // attempt's carve was read off its child's coinbase.
    assert_eq!(steps.iter().filter(|s| s.block.header.pow_algo_id == steps[4].block.header.pow_algo_id).count(), 2);
    assert_eq!(m.books.carves_read, 2, "seed {seed:#x}: both attempts' carves, off the coinbase");
    // With a market backlog every block renders a full drain, so both attempt coinbases minted.
    if market_rows > 100 {
        for at in [4, 11] {
            assert!(steps[at].parent.pending_payouts_iter().next().is_some(), "seed {seed:#x}: attempt block {at} minted payouts");
        }
    }
    // Every refused buy was paid back its sink, to its payer, by a refund row step 3′ wrote — and
    // at least one of them in a block whose step 3d moved a row, under all of `after`'s checks.
    for (carrier, payer) in &carriers {
        assert_eq!(
            m.books.market_written.get(&palw_model_refund_payout_key_v1(&carrier.id())),
            Some(&PalwPayoutV2 { payload: m.payloads[*payer], amount: BUY_SINK }),
            "seed {seed:#x}: carrier {} is owed its sink back",
            carrier.id()
        );
    }
    assert_eq!(m.books.market_written.len(), carriers.len(), "seed {seed:#x}: the refunds are the only market rows written");
    if refunds {
        assert!(m.books.refunds_beside_moves > 0, "seed {seed:#x}: a refund and a row's move share a mined block");
        assert_eq!(m.books.relaxed, 0, "seed {seed:#x}: with the market reserve binding, every block is inside the plan's contract");
    }
    let t03 = m.t03_closes();
    eprintln!(
        "[p2-t58] seed {seed:#x}: {} rows, {market_rows} market rows, {:?}, {t03:?}; {}",
        m.rows.len(),
        m.ledger,
        m.books.summary()
    );
}

/// **T58 (the plan's T46), and T03's conservation over planted rows, on randomized chains of real
/// blocks.**
///
/// Three plants — a market queue of 1,016 rows (V-7's market reserve binds: two drain slots stay
/// the market's), of 3, and none — each with a maturity burst (four rows on one DAA), carried rows,
/// reporter awards, and a row the second clock holds with licensed rows behind it; then 32 blocks,
/// two of them attempt blocks (algo 6) and the rest heartbeats the node's adapter shaped (algo 8).
/// On the 1,016-row chain six refused market buys ride three of the heartbeats, so step 3′ writes
/// their refunds while step 3d moves rows, on mined blocks. Every block passes `Minting::after` —
/// the carve read off the coinbase, build == validate on both lanes, the next-block plan is the
/// fold's, the latch is V-4's, the queue lemma, every moved leg minted by the next coinbase at its
/// amount — and at the end: every row ahead of the held one moved, the held one never latched
/// although its DAA clock ran (T25's "never on the DAA clock alone"), the rows behind it wait (stop,
/// never skip), every refund was written, and the books close (the planted rows' escrow and the two
/// attempts' carves, against the mints by coinbase position; T03 proper is the next test).
#[tokio::test]
async fn p2_t58_t03_the_queue_lemma_and_the_coinbase_identity_hold_over_randomized_chains() {
    for (seed, market_rows, refunds) in [(0x5801u64, 1_016u64, true), (0x5802, 3, false), (0x5803, 0, false)] {
        randomized_run(seed, market_rows, refunds).await;
    }
}

/// **T03: the coinbase-level identity over real `Final`s, a real conviction and real mints**
/// (ADR-0152 V-3; phase2-plan §4 T03, §5.3; launch gate §8.3 item 5).
///
/// One testnet-12 chain, every window as shipped:
/// * card 0's floor attempt makes claim A; card 7's attempt at A's anchor slot binds A's panel and
///   makes claim B; card 6's at B's slot binds B's and makes claim C, which no attempt ever anchors;
/// * a quorum of each panel signs `Valid` and the licences ride carriers; both claims go `Final`
///   past the short challenge window, and each `Final` writes a row whose escrow is the carve its
///   attempt's child coinbase withheld — read off that coinbase;
/// * A's row is re-keyed to mature (the one plant: its DAA clock and licences), latches and moves
///   in the next block and is minted by the one after;
/// * card 1 accuses B's row of withholding (`DefaultAccused`, the FinalRow stage) and nobody
///   answers: at the deadline the default reverses B's `Final`, burns its whole row (`Burned`,
///   `DaDefault`) and names card 1 the reporter; the award is written when its reveal window closes,
///   moved, and minted — funded by B's producer's slash, which bounds it (T56's reporter half);
/// * C voids `BindTimeout` at its backstop: its carve is withheld and never minted.
///
/// Then the identity closes with its minted, burned, voided and reserve terms all non-zero, and the
/// reporter mint outside it. The floor has no work price and no pair, so its rows leave nothing
/// unnamed and buy nothing back — asserted per row, which pins `finalize_claim`'s amounts: a row
/// naming more or less than the carve its coinbase withheld fails there.
#[tokio::test]
async fn p2_t03_the_coinbase_identity_closes_over_real_finals_a_real_conviction_and_real_mints() {
    let mut m = mined().await;
    let w_receipt = m.sp().window_receipt();
    eprintln!(
        "[p2-t03] testnet-12: anchor delay {}, bind {}, receipt {w_receipt}, challenge {} (short {}), court {}",
        m.chain.bundle.panel.anchor_delay(),
        m.sp().window_bind(),
        m.sp().window_challenge(),
        m.sp().window_challenge_at(0),
        m.sp().window_court()
    );
    // Claims A, B, C; A and B licensed.
    let a = m.step(Kind::Attempt(0)).await.claim.expect("card 0's attempt makes claim A");
    let b = m.attempt_at_the_anchor_slot(a, 7).await;
    m.license(a, 0).await;
    let c = m.attempt_at_the_anchor_slot(b, 6).await;
    m.license(b, 7).await;
    assert_eq!(m.books.carves_read, 3, "each attempt's carve was read off its child's coinbase");
    let is_final = |s: &PalwChainStateV2, id: &Hash64| matches!(s.claim(id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Final { .. }));
    m.beat_until(4 * m.sp().window_challenge_at(0) + 50, "A and B are Final", |s| is_final(s, &a) && is_final(s, &b)).await;
    let (row_a, row_b) = (m.books.finals[&a].clone(), m.books.finals[&b].clone());
    for (id, row) in [(a, &row_a), (b, &row_b)] {
        assert!(
            row.escrowed_reward > 0 && row.producer.amount > 0 && !row.seats.is_empty(),
            "claim {id}'s row names its legs: {row:?}"
        );
        // The floor is paid its escrow whole (no work price) and has no pair (no buyback), so its
        // `Final` names every sompi the coinbase withheld: nothing unnamed, nothing bought back.
        assert_eq!(
            (row.total_sompi(), row.buyback_bound),
            (row.escrowed_reward, 0),
            "claim {id}: a floor row names its whole carve (I-11)"
        );
    }

    // A matures (re-keyed), moves, and is minted.
    let depth = m.chain.config.params.palw_settled_anchor_depth.expect("the second clock");
    let now = m.sink_daa();
    m.rekey(|c| {
        c.settled_attempt_finals = c.settled_attempt_finals.max(depth);
        let row = c.vesting.get_mut(&a).expect("A's row");
        row.expiry_daa = now;
        row.settled_at_final = c.settled_attempt_finals - depth;
    });
    let moved = m.step(beat()).await;
    assert!(moved.moved_row(&a), "A latches and moves at 3d");
    let minted = m.step(beat()).await;
    assert!(minted.block.transactions[0].outputs.contains(&output(&row_a.producer)), "A's producer leg is minted by the next block");
    assert_eq!(m.ledger.rows, row_a.total_sompi_u128() - row_a.reserve as u128, "A's queue legs, minted whole");

    // Card 1 accuses B's row; nobody answers.
    let accuser = m.chain.bonds[1];
    let message = palw_da_accusation_message_v2(m.domain, &b, 0, &accuser);
    let accusation = PalwConsensusObjectV2::DefaultAccused {
        claim: b,
        missing_event_index: 0,
        accuser,
        signature: sign(1, message.as_byte_slice(), PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT),
    };
    let carrier = m.carrier(1, accusation, None);
    m.step(Kind::Heartbeat(vec![carrier])).await;
    let opened = m.step(beat()).await;
    let deadline =
        opened.child.da_sessions_of(&b).find(|(who, _)| **who == accuser).expect("the accusation opens a session").1.deadline_daa;
    let producer_b = m.chain.bonds[7];
    let slashed_before = opened.child.bond(&producer_b).expect("card 7").slashed;
    m.beat_until(2 * (deadline - m.sink_daa()) + 20, "B's default", |s| s.vesting_row(&b).is_none()).await;
    assert_eq!(
        m.books.burned.get(&b),
        Some(&(row_b.total_sompi(), kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::DaDefault)),
        "the default burned B's whole row"
    );
    assert!(matches!(
        m.chain.tip_state().1.claim(&b).map(|c| &c.phase),
        Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. })
    ));

    // The reporter reward: written when its reveal window closes, moved by step 3d, minted to card 1.
    let mut waited = 0;
    while m.ledger.reporters == 0 {
        assert!(waited < 2 * w_receipt + 40, "the default's reporter award is minted after its reveal window");
        m.step(beat()).await;
        waited += 1;
    }
    let awarded = m.books.awarded;
    assert!(awarded > 0 && m.ledger.reporters == awarded, "the award, minted whole ({awarded})");
    let end = m.chain.tip_state().1;
    assert!(m.a_utxo_pays(&PalwPayoutV2 { payload: m.payloads[1], amount: awarded as u64 }), "to the accuser, card 1");
    // T56's reporter half: the mint is funded by the conviction's collection, and bounded by it.
    let collected = end.bond(&producer_b).expect("card 7").slashed - slashed_before;
    assert!(awarded <= collected as u128, "a reporter mint is bounded by what its conviction collected ({awarded} ≤ {collected})");
    assert!(
        matches!(m.books.claims[&c].2, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }),
        "claim C, never anchored, voided at its backstop: {:?}",
        m.books.claims[&c].2
    );

    let t03 = m.t03_closes();
    assert_eq!(t03.burned, row_b.total_sompi_u128(), "the burned term is B's row");
    assert_eq!(t03.voided, m.books.claims[&c].0 as u128, "the voided term is C's carve");
    assert!(t03.voided > 0 && t03.burned > 0 && t03.reserve > 0 && m.ledger.rows > 0, "the terms are all exercised");
    assert_eq!(t03.unresolved, 0, "every claim of the run resolved");
    assert_eq!(t03.reserve, row_a.reserve as u128, "the reserve credited is A's");
    assert_eq!(end.vesting_len(), 0, "no row is left");
    eprintln!("[p2-t03] sink DAA {}: {t03:?}, {:?}; {}", m.sink_daa(), m.ledger, m.books.summary());
}

/// **T47: a claim whose id begins `0xFF`, with the queue full of market rows, is minted by the
/// block after its move.** Keyed by its raw id the producer leg would sort behind every one of the
/// 1,024 market rows and wait for them (M-10); under A-KEY it sorts first. The counterfactual is
/// asserted on the real queues: in both the planted tip and the moving block's child, at least a
/// full drain of market rows sorts below the raw id.
#[tokio::test]
async fn p2_t47_a_claim_id_beginning_0xff_is_minted_next_block_behind_a_full_market() {
    let claim_id = Hash64::from_bytes([0xFF; 64]);
    let mut m = minting(|p, c| {
        c.vesting.insert(claim_id, p.row(claim_id, 0, 3, &[4], p.daa, Clock::Latched));
        for i in 0..PALW_V2_MAX_PENDING_PAYOUTS as u64 {
            c.pending_payouts.insert(market_key(i), PalwPayoutV2 { payload: p.payloads[7], amount: 10_000 + i });
        }
    })
    .await;
    let raw_rank = |s: &PalwChainStateV2| s.pending_payouts_iter().filter(|(k, _)| **k < claim_id).count();
    assert_eq!(raw_rank(&m.planted), PALW_V2_MAX_PENDING_PAYOUTS, "keyed raw, the leg would sort behind all 1,024 market rows");
    let a_key = palw_vesting_payout_key_v1(&claim_id);
    assert_eq!(a_key.as_byte_slice()[0], 0x00, "A-KEY forces 0x00");

    // Block 1: 3d moves the row (two keys, inside the six the market reserve leaves).
    let moved = m.step(beat()).await;
    assert!(moved.moved_row(&claim_id), "the carried row moves in the first block");
    assert_eq!(moved.child.pending_payouts_iter().next().map(|(k, _)| *k), Some(a_key), "A-KEY sorts first");
    assert!(raw_rank(&moved.child) >= PALW_V2_MAX_PAYOUTS_PER_BLOCK, "keyed raw, the next coinbase would not reach it");

    // Block 2: its coinbase mints the producer leg (and the seat's), and the key is drained.
    let producer = m.rows[&claim_id].producer;
    let minted = m.step(beat()).await;
    assert!(minted.block.transactions[0].outputs.contains(&output(&producer)), "minted by the next block");
    assert!(minted.child.pending_payout(&a_key).is_none());
    assert!(m.a_utxo_pays(&producer), "the leg is a UTXO now");
}

/// **T25 and T05's UTXO half: a matured leg exists only once a coinbase mints it, and from then
/// on obeys Mainnet Decision A alone.**
///
/// * **T05** — the row is not a UTXO: no output pays the leg while it vests, nor after step 3d moved
///   it; the next block's coinbase outpoint, read off that block's template before the block
///   exists, is refused as `MissingTxOutpoints` by the mempool and by the block path's check.
/// * **T25** — once minted, the output is a coinbase output like any other. With no DNS-confirmed
///   anchor the mempool refuses it until `coinbase_spend_maturity` (600 DAA) — `ImmatureCoinbaseSpend`
///   at `M + 599`, accepted at `M + 600` — and the block path's own floor is `coinbase_maturity`, the
///   policy-only difference Decision A names. With a DNS-confirmed anchor AT the mint's block the
///   mempool releases it at that floor, 600 DAA early; an anchor one DAA before it has not passed
///   the mint and releases nothing (`coinbase_spend_settled`). Nothing about the row (its clocks,
///   its latch) is asked. And a row whose DAA clock has run out but whose second clock holds never
///   matures; the licence halt's twin is `p2_t25_a_licence_halt_latches_and_moves_nothing_at_the_processor_s_fold`.
#[tokio::test]
async fn p2_t25_t05_a_leg_is_no_utxo_until_minted_and_then_obeys_decision_a_alone() {
    const PAYEE: usize = 2;
    let claim_id = Hash64::from_u64_word(0x25_0005);
    let held_id = Hash64::from_u64_word(0x25_0006);
    let mut m = minting(|p, c| {
        c.vesting.insert(claim_id, p.row(claim_id, 0, PAYEE, &[], p.daa, Clock::Latched));
        // Its DAA clock has run out already; only its second clock holds it.
        c.vesting.insert(held_id, p.row(held_id, 1, 5, &[6], p.daa, Clock::SecondClockHolds));
    })
    .await;
    let producer = m.rows[&claim_id].producer;
    let spk = p2pkh_mldsa87_spk(producer.payload.as_byte_slice());
    assert!(!m.a_utxo_pays(&producer), "T05: a vesting row is not an output");

    // Block 1: step 3d moves the row into the queue — still no output.
    let moved = m.step(beat()).await;
    assert!(moved.moved_row(&claim_id));
    assert!(!m.a_utxo_pays(&producer), "T05: a moved leg is a queue row, not an output");

    // Block 2's template carries the leg; its outpoint does not exist until the block does.
    let template = m.heartbeat_template();
    let coinbase = template.transactions[0].clone();
    let index =
        coinbase.outputs.iter().position(|o| o.script_public_key == spk && o.value == producer.amount).expect("block 2 mints it");
    let outpoint = TransactionOutpoint::new(coinbase.id(), index as u32);
    let mint_daa = template.header.daa_score;
    let entry = UtxoEntry::new(producer.amount, spk.clone(), mint_daa, true);
    let mut spend = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(producer.amount - 20_000, spk.clone())],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        Vec::new(),
    );
    sign_spend(&mut spend, entry.clone(), PAYEE, m.chain.config.params.storage_mass_parameter);
    let vp = m.chain.vp();
    // `None`: the mempool as a peer's relay meets it, at the node's own virtual DAA.
    let mempool = |daa: Option<u64>| {
        let mut tx = MutableTransaction::from_tx(spend.clone());
        match daa {
            Some(daa) => vp.validate_mempool_transaction_at_daa_for_tests(&mut tx, daa),
            None => vp.validate_mempool_transaction(&mut tx, &Default::default()),
        }
    };
    let block_path = |pov: u64| {
        let stores = vp.virtual_stores.read();
        vp.validate_transaction_in_utxo_context(&spend, &stores.utxo_set, pov, TxValidationFlags::Full, None).map(|_| ())
    };
    assert!(matches!(mempool(None), Err(TxRuleError::MissingTxOutpoints)), "T05: the future outpoint is unknown to the mempool");
    assert!(matches!(block_path(mint_daa + 10_000), Err(TxRuleError::MissingTxOutpoints)), "T05: and to the block path");

    let minted = m.insert(template).await;
    assert_eq!(minted.block.transactions[0].id(), coinbase.id(), "the block is its template");
    assert!(m.a_utxo_pays(&producer), "minted");
    assert_eq!(
        m.chain.ctx.consensus.get_virtual_utxo_entry(outpoint).map(|e| (e.amount, e.is_coinbase)),
        Some((producer.amount, true))
    );

    // T25: Decision A — DAA-based coinbase maturity, no DNS anchor on this chain.
    let settlement = vp.dns_coinbase_settlement().expect("testnet-12 runs the coinbase settlement fallback");
    assert_eq!(settlement.confirmed_anchor_daa, None, "no DNS-confirmed anchor");
    let maturity = m.chain.config.params.coinbase_spend_maturity();
    assert_eq!((maturity, settlement.long_maturity_daa), (600, 600));
    match mempool(None) {
        Err(TxRuleError::ImmatureCoinbaseSpend(0, o, at, _, bound)) => assert_eq!((o, at, bound), (outpoint, mint_daa, maturity)),
        other => panic!("the mempool holds a fresh mint, got {other:?}"),
    }
    match mempool(Some(mint_daa + maturity - 1)) {
        Err(TxRuleError::ImmatureCoinbaseSpend(0, o, _, pov, bound)) => assert_eq!((o, pov, bound), (outpoint, mint_daa + 599, 600)),
        other => panic!("M + 599 is one DAA short, got {other:?}"),
    }
    mempool(Some(mint_daa + maturity)).expect("M + 600: a coinbase output spendable on the DAA alone");
    // The block path's floor is the classic maturity; 600 is the policy's (Decision A).
    let floor = m.chain.config.params.coinbase_maturity();
    assert!(floor < maturity, "the block-path floor ({floor}) is below the policy's 600");
    block_path(mint_daa + floor).expect("the block path admits it at its own floor");
    if floor > 0 {
        assert!(matches!(block_path(mint_daa + floor - 1), Err(TxRuleError::ImmatureCoinbaseSpend(..))));
    }

    // T25's DNS half: the node's DnsState names a confirmed anchor (the policy layer reads it,
    // never consensus). At the mint's own block it releases the leg at the block path's floor; one
    // DAA before it has not passed the mint and changes nothing. The node's state is put back after.
    {
        use crate::model::stores::dns_state::{DnsStateStore, DnsStateStoreReader};
        let kept = vp.dns_state_store.read().get().expect("testnet-12 keeps a DnsState");
        let confirm = |anchor_daa: u64| {
            let mut state = kept.clone();
            state.last_dns_confirmed_anchor = minted.block.header.hash;
            state.last_dns_confirmed_anchor_daa_score = anchor_daa;
            vp.dns_state_store.write().set(state).expect("the DnsState is planted");
            assert_eq!(vp.dns_coinbase_settlement().expect("the fallback").confirmed_anchor_daa, Some(anchor_daa));
        };
        confirm(mint_daa - 1);
        assert!(
            matches!(mempool(Some(mint_daa + floor.max(1))), Err(TxRuleError::ImmatureCoinbaseSpend(..))),
            "an anchor before the mint's block has not passed it: the long fallback still applies"
        );
        confirm(mint_daa);
        mempool(Some(mint_daa + floor)).expect("a DNS-final anchor at the mint releases it at the block path's floor");
        if floor > 0 {
            assert!(matches!(mempool(Some(mint_daa + floor - 1)), Err(TxRuleError::ImmatureCoinbaseSpend(..))), "never below it");
        }
        vp.dns_state_store.write().set(kept).expect("the node's DnsState is restored");
    }

    // The held row's DAA clock ran out before block 1; through every block since, its second clock
    // held it.
    let (_, tip) = m.chain.tip_state();
    let held = tip.vesting_row(&held_id).expect("the held row stays").clone();
    let tip_daa = tip.last_point().expect("a point").daa_score;
    let v4 = palw_vesting_row_maturity_v1(&tip, m.sp(), &held, tip_daa, m.raw_depth(tip_daa));
    assert!(v4.daa_clock_met && v4.licences_since_final < 30 && !v4.halted, "the DAA clock has run, the licences have not: {v4:?}");
    assert!(held.matured_at.is_none() && !v4.mature_now, "never on the DAA clock alone");
}

/// **T25's halt twin: during a licence halt nothing latches and nothing moves** — not the row its
/// second clock holds, and not even the row whose licences are in (V-4(b) is chain-wide); once an
/// anchor settles again, the licensed row latches and moves in the next block, and the other still
/// waits on its second clock. The committed parent's next-block plan says the same at every step.
///
/// **At the processor's transition, not on mined blocks.** A halt is `2 × window_court` (6,000 DAA
/// on testnet-12) with no anchor settled, and the anchor ring's floor is DAA 0, so a mined chain must
/// first be 6,000 DAA long — about 12,000 heartbeats, since the clock steps on every second beat —
/// and that run did not finish in ten minutes of a debug build. So the rows are planted on the mined
/// tip and folded through `palw_v2_block_fold_with_refunds_for_tests` — the block path's own fold,
/// at the processor's extras for each point — over consecutive points past DAA 6,000, each fold's
/// result the next one's parent; the end of the halt is an anchor planted into the ring (what a
/// licence's `settle_anchor` writes). The DAA-clock-alone half of T25 on mined blocks is the held
/// row of `p2_t25_t05_…` and T58's held row.
#[tokio::test]
async fn p2_t25_a_licence_halt_latches_and_moves_nothing_at_the_processor_s_fold() {
    let licensed = Hash64::from_u64_word(0x25_1001);
    let short = Hash64::from_u64_word(0x25_1002);
    let mut m = mined().await;
    let w = m.sp().window_court();
    let at = 2 * w + 10;
    m.plant(|p, c| {
        assert!(c.recent_anchor_daas.is_empty(), "no anchor ever settled on this chain");
        c.vesting.insert(licensed, p.row(licensed, 0, 1, &[2], at - 5, Clock::Licensed));
        c.vesting.insert(short, p.row(short, 1, 3, &[4], at - 4, Clock::SecondClockHolds));
    });
    let vp = m.chain.vp();
    let (_, mut state) = m.chain.tip_state();
    let blue = state.last_point().expect("a point").blue_score;
    let point = |i: u64| PalwBlockContextV2 {
        block: Hash64::from_u64_word(0x25_2000 + i),
        daa_score: at + i,
        blue_score: blue + 1 + i,
        subsidy: 0,
    };
    let fold = |state: &PalwChainStateV2, i: u64| {
        vp.palw_v2_block_fold_with_refunds_for_tests(state, m.sp(), &point(i), &[], Vec::new()).expect("an empty block folds")
    };

    for i in 0..4 {
        let daa = point(i).daa_score;
        let raw = m.raw_depth(daa);
        assert!(palw_chain_vesting_halted_v1(&state, raw, daa, w), "no anchor for 2 × window_court: halted");
        let plan = palw_vesting_next_block_plan_v1(&state, m.sp(), daa, raw);
        assert!(plan.moves.is_empty(), "the committed parent's plan moves nothing in a halt: {plan:?}");
        let folded = fold(&state, i);
        for id in [licensed, short] {
            let row = folded.vesting_row(&id).expect("the row stays");
            let v4 = palw_vesting_row_maturity_v1(&folded, m.sp(), row, daa, raw);
            assert!(row.matured_at.is_none() && v4.daa_clock_met && v4.halted && !v4.mature_now, "row {id} in the halt: {v4:?}");
        }
        let v4 = palw_vesting_row_maturity_v1(&folded, m.sp(), folded.vesting_row(&licensed).unwrap(), daa, raw);
        assert!(v4.licences_since_final >= 30, "the licensed row's second clock is spent too, and it still waits: {v4:?}");
        assert!(folded.pending_payouts_iter().next().is_none(), "nothing reached the queue");
        state = folded;
    }

    // An anchor settles: the halt ends. The licensed row latches and moves in the next block; the
    // short one waits on its second clock.
    let mut carriage = PalwStateCarriageV2::from_state(&state);
    carriage.recent_anchor_daas = vec![point(3).daa_score];
    let state = carriage.into_state(m.sp(), None).expect("an anchor in the ring");
    let daa = point(4).daa_score;
    let plan = palw_vesting_next_block_plan_v1(&state, m.sp(), daa, m.raw_depth(daa));
    assert_eq!(plan.moves.len(), 1, "the parent's plan: the licensed row, and only it");
    assert_eq!(plan.moves[0].source, PalwVestingSourceV1::Row { claim_id: licensed });
    let folded = fold(&state, 4);
    assert!(folded.vesting_row(&licensed).is_none(), "the licensed row latched and moved");
    let producer = m.rows[&licensed].producer;
    assert_eq!(folded.pending_payout(&palw_vesting_payout_key_v1(&licensed)), Some(&producer), "its producer leg is queued, whole");
    assert!(folded.vesting_row(&short).expect("the short row waits").matured_at.is_none(), "its second clock still holds it");
    palw_vesting_consistency_v1(&folded).expect("V-3");
}

/// **T29 (processor half, the queue): the acceptance rehearsal and the fold agree with the queue
/// at 1,016 rows, a carried row moving and a row maturing at 3d in the same block.**
///
/// The parent queue holds 1,016 market rows; step 1b drains eight, so the rehearsal's pre-object
/// base — the fold's step 2, mirrored — has room for exactly 16 of the 20 carrier refunds a block
/// of refused market buys owes (refunds count against the cap and are reserved as owed). The fold,
/// handed the rehearsal's list, applies it without error (the filter's one contract) and writes
/// all 16, filling the queue to the cap; THEN step 3d latches the maturing row and moves the carried
/// six-key row — exempt from the cap (M-10), bounded by V-7's budget of 8 − 2 (the market's
/// reserve) — so the committed queue is 1,030 ≤ cap + one drain. The maturing row latched but did
/// not fit (stop at `BudgetFull`), exactly as the committed parent's next-block plan says. Nothing
/// in the rehearsal reserved for 3d, and nothing needed to (phase2-plan F4). The same interplay on
/// mined blocks, refunds carried by real carriers, is T58's 1,016-row chain.
#[tokio::test]
async fn p2_t29_the_rehearsal_and_the_fold_agree_at_a_1016_row_queue_with_maturity_at_3d() {
    let carried = Hash64::from_u64_word(0x29_0001);
    let maturing = Hash64::from_u64_word(0x29_0002);
    let queued = PALW_V2_MAX_PENDING_PAYOUTS as u64 - PALW_V2_MAX_PAYOUTS_PER_BLOCK as u64;
    let m = minting(|p, c| {
        c.vesting.insert(carried, p.row(carried, 0, 0, &[1, 2, 3, 4, 5], p.daa, Clock::Latched));
        c.vesting.insert(maturing, p.row(maturing, 1, 6, &[7], p.daa + 1, Clock::Licensed));
        for i in 0..queued {
            c.pending_payouts.insert(market_key(i), PalwPayoutV2 { payload: p.payloads[7], amount: 10_000 + i });
        }
    })
    .await;
    assert_eq!(queued, 1_016);
    let (_, tip) = m.chain.tip_state();
    let last = *tip.last_point().expect("a point");
    let point = PalwBlockContextV2 {
        block: Hash64::from_u64_word(0x2929),
        daa_score: last.daa_score + 1,
        blue_score: last.blue_score + 1,
        subsidy: 0,
    };
    let line = m.chain.bundle.base_class_id;
    let refused_buy = |i: u64| {
        (
            PalwConsensusObjectV2::ModelBuy {
                line_id: line,
                holder: Hash64::from_u64_word(0xB0B0 + i),
                msk_in: 1_000_000,
                min_units_out: u64::MAX,
                sink_index: 1,
            },
            Some(PalwCarrierRefundV1 {
                carrier: TransactionId::from_u64_word(0x29_1000 + i),
                line_id: line,
                payee: m.payloads[7],
                amount: 1_000_000,
            }),
        )
    };
    let vp = m.chain.vp();
    let (accepted, _, refunds) =
        vp.palw_v2_accepted_refundable_objects_for_tests(&tip, m.sp(), &point, (0..20).map(refused_buy).collect(), point.block);
    assert!(accepted.is_empty(), "every buy is refused (min_units_out = u64::MAX)");
    let room = PALW_V2_MAX_PENDING_PAYOUTS - (queued as usize - PALW_V2_MAX_PAYOUTS_PER_BLOCK);
    assert_eq!(refunds.len(), room, "the rehearsal owes exactly the refunds the post-drain queue has room for");
    assert_eq!(refunds, (0..room as u64).map(|i| refused_buy(i).1.unwrap()).collect::<Vec<_>>(), "the first ones, in order");

    let folded = vp
        .palw_v2_block_fold_with_refunds_for_tests(&tip, m.sp(), &point, &accepted, refunds.clone())
        .expect("what the rehearsal returns, the fold applies — with 3d moving after it");
    assert!(folded.vesting_row(&carried).is_none(), "the carried row moved at 3d");
    let latched = folded.vesting_row(&maturing).expect("the maturing row is carried, not moved");
    assert_eq!(latched.matured_at, Some(point.daa_score), "it latched at 3d, in this block");
    let carried_keys = m.rows[&carried].key_count();
    assert_eq!(carried_keys, 6);
    assert_eq!(folded.pending_payouts_iter().count(), PALW_V2_MAX_PENDING_PAYOUTS + carried_keys, "the cap, plus 3d's exempt moves");
    assert!(folded.pending_payouts_iter().count() <= PALW_V2_MAX_PENDING_PAYOUTS + PALW_V2_MAX_PAYOUTS_PER_BLOCK);
    assert!(palw_vesting_non_market_rows_waiting_v1(&folded) <= PALW_V2_MAX_PAYOUTS_PER_BLOCK, "the queue lemma");
    let plan = palw_vesting_next_block_plan_v1(&tip, m.sp(), point.daa_score, m.raw_depth(point.daa_score));
    assert_eq!(plan.moves.len(), 1, "the committed parent predicts one move");
    assert_eq!(plan.moves[0].source, PalwVestingSourceV1::Row { claim_id: carried });
    assert_eq!(
        (plan.stopped, plan.stopped_at),
        (PalwVestingStopV1::BudgetFull, Some(PalwVestingSourceV1::Row { claim_id: maturing }))
    );
    palw_vesting_consistency_v1(&folded).expect("V-3");
}

/// **ADR-0152 Phase 2, P2-3: the vesting rows across a reorg and a pruned sync** (T48, T49, and the
/// undecodable-delta test extended to the vesting variants) — a child of this suite, so it drives the
/// same `Minting` on more than one node and holds every block a builder mines to `after`.
#[path = "p2_reorg_and_ibd.rs"]
mod p2_reorg_and_ibd;

/// **ADR-0152 Phase 2, P2-4: the EVM twin** (T50) — the mint path on testnet-12 with the EVM lane as
/// shipped, every block the node's own template at this host's clock. A child of this suite so it
/// holds every block to `after`; only a build with the `evm` feature can build a template for an
/// active lane.
#[cfg(feature = "evm")]
#[path = "p2_evm_twin.rs"]
mod p2_evm_twin;
