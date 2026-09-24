//! **ADR-0152 Phase 2, P2-2 — the mint path, verified at the processor on testnet-12**
//! (phase2-plan F1/F2, §4: T58 — the plan's T46, renumbered by ADR-0152 v3.1 — T03, T47, T25,
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
//! **The rows are planted, not earned.** A real `Final` on testnet-12 is a panel, a quorum of
//! receipts and a challenge window, and a real maturity is 3,000 DAA and thirty licences after it
//! (`t12_round_lane_e2e` walks the first half at full length). The mint path starts where the row
//! exists, so the harness writes rows — latched, maturing on the DAA clock with the second clock's
//! licences in, or held by the second clock — plus reporter awards and market rows into the tip
//! the chain stands at (`set_tip_for_tests`, through the carriage and its consistency load), with
//! the chain's settled-anchor count raised to `SETTLED` so the second clock can be met or not per
//! row. From there on nothing is planted: every block's fold, step 3d and coinbase are the node's.
//!
//! Every block is held to five things (`Minting::after`): its coinbase carries the parent queue's
//! first eight rows contiguously and in key order (by POSITION, which is how T03 attributes them —
//! one payout script receives round fees, vesting mints and seat pay alike, phase2-plan §5.3); the
//! legs its step 3d moved are exactly `palw_vesting_next_block_plan_v1` of the committed parent
//! (the RPC's and this harness's one question, IA-6), and the rows it latched are exactly those
//! `palw_vesting_row_maturity_v1` calls mature; the queue lemma (the non-market part ≤ 8 after the
//! fold); every moved key sits in the child's first eight at exactly the moved amount, so the next
//! coinbase mints it; and V-3's consistency.
use super::t12_round_lane_e2e::{T12Chain, sign_spend, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards};
use super::{OnetimeTxSelector, new_miner_data};
use crate::processes::transaction_validator::tx_validation_in_utxo_context::TxValidationFlags;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::errors::tx::TxRuleError;
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::palw_economic_safety_v1::PalwLicenceDoorTagV1;
use kaspa_consensus_core::palw_state_v2::{
    PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX, PALW_STATE_V2_PANEL_PAYOUT_KEY_PREFIX, PALW_V2_MAX_PAYOUTS_PER_BLOCK,
    PALW_V2_MAX_PENDING_PAYOUTS, PalwBlockContextV2, PalwBondKeyV2, PalwCarrierRefundV1, PalwChainStateV2, PalwConsensusObjectV2,
    PalwPayoutV2, PalwStateCarriageV2, PalwStateParamsV2,
};
use kaspa_consensus_core::palw_vesting_v1::{
    PalwVestingLegV1, PalwVestingNoteV1, PalwVestingRowV1, PalwVestingSourceV1, PalwVestingStopV1, palw_reporter_payout_key_v1,
    palw_vesting_consistency_v1, palw_vesting_next_block_plan_v1, palw_vesting_non_market_rows_waiting_v1,
    palw_vesting_notes_of_delta_v1, palw_vesting_payout_key_v1, palw_vesting_row_maturity_v1,
};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
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

fn output(payout: &PalwPayoutV2) -> TransactionOutput {
    TransactionOutput::new(payout.amount, p2pkh_mldsa87_spk(payout.payload.as_byte_slice()))
}

/// What the chain's coinbases minted, attributed by position against the parent queue's keys.
#[derive(Default, Debug)]
struct Ledger {
    rows: u128,
    reporters: u128,
    market: u128,
    reserve_credited: u128,
}

enum Kind {
    Heartbeat,
    Attempt(usize),
}

/// One block's facts: the committed parent, the block, the child the node committed, and the
/// vesting notes of the block's delta.
struct Stepped {
    parent: PalwChainStateV2,
    block: Block,
    child: PalwChainStateV2,
    notes: Vec<PalwVestingNoteV1>,
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

struct Minting {
    chain: T12Chain,
    plant: Plant,
    planted: PalwChainStateV2,
    /// The planted rows (T03's withheld side) and the queue keys their producer legs land on.
    rows: BTreeMap<Hash64, PalwVestingRowV1>,
    producer_keys: BTreeSet<Hash64>,
    /// The planted reporter awards, by the queue key their move lands on.
    reporter_keys: BTreeMap<Hash64, u64>,
    ledger: Ledger,
    nonce: u64,
}

/// **testnet-12 as shipped (harness cards), one heartbeat in, with `edit` planted on the tip.**
async fn minting(edit: impl FnOnce(&Plant, &mut PalwStateCarriageV2)) -> Minting {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    assert!(bundle.state.rcore_plus_active_at(0), "testnet-12 arms R-core+ from genesis");
    assert_eq!(config.params.palw_settled_anchor_depth, Some(30), "testnet-12's second clock");
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    chain.heartbeat(config.params.target_time_per_block(), Vec::new()).await;
    let (sink, tip) = chain.tip_state();
    assert!(tip.settled_attempt_finals() <= SETTLED && tip.vesting_len() == 0 && tip.pending_payouts_iter().next().is_none());
    let plant = Plant {
        daa: chain.daa_of(sink),
        bonds: chain.bonds.clone(),
        payloads: chain.bonds.iter().map(|b| tip.bond(b).expect("a genesis card").payout_payload).collect(),
    };
    let mut carriage = PalwStateCarriageV2::from_state(&tip);
    carriage.settled_attempt_finals = SETTLED;
    edit(&plant, &mut carriage);
    // V-3: the counters name what the rows hold.
    carriage.vesting_counters.created = carriage.vesting.values().map(PalwVestingRowV1::total_sompi_u128).sum();
    let rows = carriage.vesting.clone();
    let reporter_keys =
        carriage.reporter_rewards.iter().map(|(offence, payout)| (palw_reporter_payout_key_v1(offence), payout.amount)).collect();
    let planted: PalwChainStateV2 = carriage.into_state(&chain.bundle.state, None).expect("the planted tip is a consistent state");
    palw_vesting_consistency_v1(&planted).expect("the plant satisfies V-3");
    chain.vp().palw_state_v2_store.write().set_tip_for_tests(sink, &planted).expect("the plant becomes the tip");
    let producer_keys = rows.keys().map(palw_vesting_payout_key_v1).collect();
    Minting { chain, plant, planted, rows, producer_keys, reporter_keys, ledger: Ledger::default(), nonce: 0x9E_0000 }
}

impl Minting {
    fn sp(&self) -> &PalwStateParamsV2 {
        &self.chain.bundle.state
    }

    /// The raw second-clock depth a block at `daa` folds with (`palw_settled_anchor_depth_at`, I-8).
    fn raw_depth(&self, daa: u64) -> Option<u64> {
        let p = &self.chain.config.params;
        if p.palw_audit_2026_09_23.is_some_and(|f| f.is_active(daa)) { p.palw_settled_anchor_depth } else { None }
    }

    /// Mine one block of `kind` and hold it to the five checks (module doc).
    async fn step(&mut self, kind: Kind) -> Stepped {
        let (_, parent) = self.chain.tip_state();
        let step = self.chain.config.params.target_time_per_block();
        let block = match kind {
            Kind::Heartbeat => self.chain.heartbeat(step, Vec::new()).await,
            Kind::Attempt(card) => self.chain.attempt(card, step, Vec::new(), &|_| true).await.0,
        };
        self.after(parent, block)
    }

    /// The five checks, on a block already inserted as the sink.
    fn after(&mut self, parent: PalwChainStateV2, block: Block) -> Stepped {
        let hash = block.header.hash;
        let daa = block.header.daa_score;
        let what = format!("block {hash} (DAA {daa}, algo {})", block.header.pow_algo_id);
        let (tip_block, child) = self.chain.tip_state();
        assert_eq!(tip_block, hash, "{what}: the walk left the PALW tip at the block");
        let (_, delta) = self.chain.vp().palw_state_v2_store.read().delta_of(hash).expect("the block's delta row");
        let stepped = Stepped { parent, block, child, notes: palw_vesting_notes_of_delta_v1(&delta).cloned().collect() };
        let (parent, child) = (&stepped.parent, &stepped.child);

        // (1) The coinbase mints the parent queue's first eight rows, contiguously and in key order;
        //     each output is attributed by its POSITION to the parent key it renders.
        let prefix: Vec<(Hash64, PalwPayoutV2)> =
            parent.pending_payouts_iter().take(PALW_V2_MAX_PAYOUTS_PER_BLOCK).map(|(k, p)| (*k, *p)).collect();
        if !prefix.is_empty() {
            let rendered: Vec<TransactionOutput> = prefix.iter().map(|(_, p)| output(p)).collect();
            let outputs = &stepped.block.transactions[0].outputs;
            let at = outputs
                .windows(rendered.len())
                .position(|window| window == rendered.as_slice())
                .unwrap_or_else(|| panic!("{what}: the coinbase does not carry the parent queue's first {} rows", rendered.len()));
            for (j, (key, payout)) in prefix.iter().enumerate() {
                assert_eq!(outputs[at + j].value, payout.amount);
                let first = key.as_byte_slice()[0];
                if first == PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX {
                    self.ledger.market += payout.amount as u128;
                } else if self.reporter_keys.contains_key(key) {
                    self.ledger.reporters += payout.amount as u128;
                } else {
                    assert!(
                        self.producer_keys.contains(key) || first == PALW_STATE_V2_PANEL_PAYOUT_KEY_PREFIX,
                        "{what}: a non-market key is a moved producer (A-KEY) or seat key, key {key}"
                    );
                    self.ledger.rows += payout.amount as u128;
                }
            }
        }

        // (2) Step 3d moved exactly what the committed parent predicts (IA-6), and latched exactly
        //     the rows V-4 calls mature at this DAA.
        let raw = self.raw_depth(daa);
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

        // (3) The queue lemma.
        assert!(
            palw_vesting_non_market_rows_waiting_v1(child) <= PALW_V2_MAX_PAYOUTS_PER_BLOCK,
            "{what}: the non-market part of the queue is {} rows",
            palw_vesting_non_market_rows_waiting_v1(child)
        );

        // (4) Every moved key is in the child's first eight, at exactly what this block moved onto
        //     it: the next coinbase mints it, whole.
        let mut owed: BTreeMap<Hash64, (Hash64, u64)> = BTreeMap::new();
        for (_, legs) in stepped.moved() {
            for leg in legs.iter().filter(|leg| leg.takes_budget()) {
                let entry = owed.entry(leg.queue_key.expect("a budget-taking leg has a key")).or_insert((leg.payload, 0));
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
        stepped
    }

    /// A heartbeat template the node builds and its adapter shapes — not inserted — so a test can
    /// read the coinbase outpoints of a block before that block exists.
    fn heartbeat_template(&mut self) -> MutableBlock {
        self.chain.ctx.simulated_time += self.chain.config.params.target_time_per_block();
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

    /// Insert a block built by [`Self::heartbeat_template`] and hold it to the five checks.
    async fn insert(&mut self, block: MutableBlock) -> Stepped {
        let (_, parent) = self.chain.tip_state();
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
        self.after(parent, block)
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

    /// **T03, the coinbase-level identity over the run** (V-3 as restated by phase2-plan §5.3):
    /// Σ withheld (the planted rows' escrow) = Σ minted from rows (by coinbase position) + Σ moved
    /// and not yet minted + Σ live + Σ burned + Σ unnamed + Σ buyback + Σ reserve credited — with the
    /// reporter and market mints OUTSIDE it (funded by slashes and sinks) and closing on their own.
    fn t03_closes(&self, end: &PalwChainStateV2) {
        let withheld: u128 = self.rows.values().map(|row| row.escrowed_reward as u128).sum();
        let unnamed: u128 =
            self.rows.values().map(|row| (row.escrowed_reward - row.buyback_bound) as u128 - row.total_sompi_u128()).sum();
        let buyback: u128 = self.rows.values().map(|row| row.buyback_bound as u128).sum();
        let live: u128 = end.vesting_iter_by_expiry().map(PalwVestingRowV1::total_sompi_u128).sum();
        let burned = end.vesting_counters().burned - self.planted.vesting_counters().burned;
        let reserve_credited = (end.panel_reserve_sompi() - self.planted.panel_reserve_sompi()) as u128;
        assert_eq!(reserve_credited, self.ledger.reserve_credited, "the reserve legs moved are what panel_reserve_sompi gained");
        let in_flight = |class: fn(&Self, &Hash64) -> bool| -> u128 {
            end.pending_payouts_iter().filter(|(k, _)| class(self, k)).map(|(_, p)| p.amount as u128).sum()
        };
        let row_key =
            |m: &Self, k: &Hash64| k.as_byte_slice()[0] != PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX && !m.reporter_keys.contains_key(k);
        let rows_in_flight = in_flight(row_key);
        assert_eq!(
            withheld,
            self.ledger.rows + rows_in_flight + live + burned + unnamed + buyback + reserve_credited,
            "T03: withheld = minted from rows + in flight + live + burned + unnamed + buyback + reserve ({:?})",
            self.ledger
        );
        let moved = end.vesting_counters().moved - self.planted.vesting_counters().moved;
        assert_eq!(
            moved,
            self.ledger.rows + rows_in_flight + reserve_credited,
            "every sompi moved is minted, in flight, or the reserve"
        );
        // Outside the identity, and closing on their own.
        let reporters: u128 = self.reporter_keys.values().map(|a| *a as u128).sum();
        let reporters_waiting: u128 = end.reporter_rewards_iter().map(|(_, p)| p.amount as u128).sum();
        assert_eq!(reporters, self.ledger.reporters + in_flight(|m, k| m.reporter_keys.contains_key(k)) + reporters_waiting);
        let market: u128 = self.planted.pending_payouts_iter().map(|(_, p)| p.amount as u128).sum();
        let market_left = in_flight(|_, k| k.as_byte_slice()[0] == PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX);
        assert_eq!(market, self.ledger.market + market_left, "the planted market rows are minted or still queued");
    }
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

/// One randomized chain (see the test).
async fn randomized_run(seed: u64, market_rows: u64) {
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
    // Heartbeats, with two attempt blocks by two cards.
    let attempts = [(4usize, (seed as usize + 1) % 8), (11, (seed as usize + 5) % 8)];
    let mut steps = Vec::new();
    for b in 0..32 {
        let kind = match attempts.iter().find(|(at, _)| *at == b) {
            Some((_, card)) => Kind::Attempt(*card),
            None => Kind::Heartbeat,
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
    // The lanes both minted: an attempt block's coinbase checks passed like a heartbeat's.
    assert_eq!(steps.iter().filter(|s| s.block.header.pow_algo_id == steps[4].block.header.pow_algo_id).count(), 2);
    // With a market backlog every block renders a full drain, so both attempt coinbases minted.
    if market_rows > 100 {
        for at in [4, 11] {
            assert!(steps[at].parent.pending_payouts_iter().next().is_some(), "seed {seed:#x}: attempt block {at} minted payouts");
        }
    }
    m.t03_closes(&end);
    eprintln!("[p2-t58] seed {seed:#x}: {} rows, {market_rows} market rows, ledger {:?}", m.rows.len(), m.ledger);
}

/// **T58 (the plan's T46) and T03 at the processor, over randomized chains of real blocks.**
///
/// Three plants — a market queue of 1,016 rows (V-7's market reserve binds: two drain slots stay
/// the market's), of 3, and none — each with a maturity burst (four rows on one DAA), carried rows,
/// reporter awards, and a row the second clock holds with licensed rows behind it; then 32 blocks,
/// two of them attempt blocks (algo 6) and the rest heartbeats the node's adapter shaped (algo 8).
/// Every block passes the five checks of `Minting::after` — build == validate on both lanes, the
/// next-block plan is the fold's, the latch is V-4's, the queue lemma, every moved leg minted by the
/// next coinbase at its amount — and at the end: every row ahead of the held one moved, the held
/// one never latched although its DAA clock ran (T25's "never on the DAA clock alone"), the rows
/// behind it wait (stop, never skip), and T03's identity closes by coinbase position.
#[tokio::test]
async fn p2_t58_t03_the_queue_lemma_and_the_coinbase_identity_hold_over_randomized_chains() {
    for (seed, market_rows) in [(0x5801u64, 1_016u64), (0x5802, 3), (0x5803, 0)] {
        randomized_run(seed, market_rows).await;
    }
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
    let moved = m.step(Kind::Heartbeat).await;
    assert!(moved.moved_row(&claim_id), "the carried row moves in the first block");
    assert_eq!(moved.child.pending_payouts_iter().next().map(|(k, _)| *k), Some(a_key), "A-KEY sorts first");
    assert!(raw_rank(&moved.child) >= PALW_V2_MAX_PAYOUTS_PER_BLOCK, "keyed raw, the next coinbase would not reach it");

    // Block 2: its coinbase mints the producer leg (and the seat's), and the key is drained.
    let producer = m.rows[&claim_id].producer;
    let minted = m.step(Kind::Heartbeat).await;
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
/// * **T25** — once minted, the output is a coinbase output like any other: with no DNS-confirmed
///   anchor the mempool refuses it until `coinbase_spend_maturity` (600 DAA) — `ImmatureCoinbaseSpend`
///   at `M + 599`, accepted at `M + 600` — and the block path's own floor is `coinbase_maturity`,
///   the policy-only difference Decision A names; nothing about the row (its clocks, its latch) is
///   asked. And a row whose DAA clock has run out but whose second clock holds never matures.
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
    let moved = m.step(Kind::Heartbeat).await;
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

    // The held row's DAA clock ran out before block 1; through every block since, its second clock
    // held it.
    let (_, tip) = m.chain.tip_state();
    let held = tip.vesting_row(&held_id).expect("the held row stays").clone();
    let tip_daa = tip.last_point().expect("a point").daa_score;
    let v4 = palw_vesting_row_maturity_v1(&tip, m.sp(), &held, tip_daa, m.raw_depth(tip_daa));
    assert!(v4.daa_clock_met && v4.licences_since_final < 30 && !v4.halted, "the DAA clock has run, the licences have not: {v4:?}");
    assert!(held.matured_at.is_none() && !v4.mature_now, "never on the DAA clock alone");
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
/// in the rehearsal reserved for 3d, and nothing needed to (phase2-plan F4).
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
                payee: m.plant.payloads[7],
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
