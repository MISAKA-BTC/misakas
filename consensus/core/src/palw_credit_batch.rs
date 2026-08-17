//! ADR-0037 Decision 7 / ADR-0038 Decision G: the budgeted deterministic credit batch —
//! the mint side of the async job machine, as a pure function.
//!
//! This module succeeds the unbounded `Vec<TransactionOutput>` producer shape of
//! [`crate::palw_credit`]'s consumers: a finalized job no longer appends outputs to
//! whatever coinbase happens to observe it. Instead, every creditable terminal
//! ([`crate::palw_job_state::PalwJobStatusV3::creditable`]) leaves ONE
//! [`PalwFinalizedCreditRecordV3`] in a pruning-surviving index, and each crediting block
//! drains a *budgeted, prefix-mandatory* batch from that index in the pinned order
//! `(finalized_daa, job_id)`:
//!
//! * **Prefix-mandatory** — the walk includes records in key order and STOPS at the first
//!   record that does not fit the remaining budget or output slots. Skipping a too-big
//!   record to cherry-pick a later one would be miner discretion over payee ordering —
//!   i.e. censorship — so it is not expressible here (ADR-0037 Decision 7).
//! * **Budgeted** — `consumed_budget ≤ block_budget` and the output count ≤ `max_outputs`
//!   hold by construction; the mint is a carve of scheduled subsidy, never an append
//!   (I6/I15).
//! * **Credit-once (I3)** — the index pairs `pending` with a `consumed` set;
//!   [`PalwFinalizedCreditIndexV3::record`] refuses a job_id it has ever seen, and
//!   [`PalwFinalizedCreditIndexV3::consume`] moves ids one way, so no reorg replay or
//!   double-observation mints twice.
//! * **Exact payees (I4)** — a payee resolves ONLY through the caller's
//!   `resolve_spk(bond_outpoint)` closure, and every minted script is
//!   [`crate::dns_finality::p2pkh_mldsa87_spk`] of that payload — never a
//!   `validator_pubkey_hash` lookup.
//! * **Missing is never empty (I7)** — a resolver miss on any payee of an
//!   otherwise-includable record is an ERROR aborting the whole batch, not a silently
//!   shorter one.
//!
//! Everything here is arithmetic over the caller's facts — no store handle, no clock;
//! construction and validation of a crediting block compute byte-identical batches, and a
//! reorg that changes the index changes the batch identically on every node. Same shape
//! as [`crate::palw_credit`] and [`crate::palw_job_state`].
//!
//! Consensus-inert: nothing constructs [`PalwFinalizedCreditIndexV3`] on any shipped
//! network; the Track-C change set (ADR-0037 §Implementation order) wires it.

use crate::tx::{TransactionOutpoint, TransactionOutput};
use kaspa_hashes::Hash64;
use std::collections::{BTreeMap, BTreeSet};

/// Version pin for the batch rule; a rule change is a new version, never a silent edit.
pub const PALW_CREDIT_BATCH_VERSION_V1: u16 = 1;

/// One finalized, not-yet-consumed credit: the mint-side residue of a job/block whose
/// status reached a creditable terminal (`PalwJobStatusV3::creditable()`). Amounts are
/// sompi, fixed at finalization; payees stay as bond outpoints (I4) and become scripts
/// only inside [`compute_palw_credit_batch_v3`].
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFinalizedCreditRecordV3 {
    pub job_id: Hash64,
    pub finalized_daa: u64,
    pub executor_bond_outpoint: TransactionOutpoint,
    pub executor_amount: u64,
    /// (verifier bond outpoint, award) pairs — receipt earners.
    pub verifier_awards: Vec<(TransactionOutpoint, u64)>,
}

impl PalwFinalizedCreditRecordV3 {
    /// The record's total mint (executor + every verifier award), or `None` on u64
    /// overflow — an overflowing record is a construction bug upstream and must surface
    /// as [`PalwCreditBatchError::RecordOverflow`], never wrap into a small "fit".
    fn total_amount(&self) -> Option<u64> {
        self.verifier_awards.iter().try_fold(self.executor_amount, |acc, (_, award)| acc.checked_add(*award))
    }

    /// How many actual outputs this record mints: zero-amount awards mint no output
    /// (a zero-value UTXO is dust the ledger never needs) but still consume with the
    /// record, so they occupy no slot against `max_outputs`.
    fn output_count(&self) -> usize {
        usize::from(self.executor_amount > 0) + self.verifier_awards.iter().filter(|(_, award)| *award > 0).count()
    }
}

/// The pruning-surviving index of finalized credits awaiting mint, plus the consumed set
/// that makes credit-once (I3) hold across blocks and reorgs.
#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFinalizedCreditIndexV3 {
    /// Keyed (finalized_daa, job_id) — THE pinned consumption order.
    pub pending: BTreeMap<(u64, Hash64), PalwFinalizedCreditRecordV3>,
    pub consumed: BTreeSet<Hash64>,
}

/// A batch refusal or an index refusal. Every variant is a caller bug or an attack —
/// never a silent no-op (the [`crate::palw_job_state`] closure discipline).
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwCreditBatchError {
    /// `record()` saw a job_id already pending — a job credits at most once (I3).
    #[error("PALW credit record for job {job_id} is already pending (I3)")]
    DuplicateJobId { job_id: Hash64 },

    /// `record()` saw a job_id already consumed, or `consume()` was asked to consume the
    /// same id twice — consumption is one-way (I3).
    #[error("PALW credit for job {job_id} was already consumed (I3)")]
    AlreadyConsumed { job_id: Hash64 },

    /// `consume()` was handed an id with no pending record — the crediting block named a
    /// credit this chain view never finalized.
    #[error("PALW credit id {job_id} is not pending in this index")]
    UnknownCreditId { job_id: Hash64 },

    /// The resolver had no payout script for a bond outpoint of an includable record.
    /// Missing is never empty (I7): the whole batch aborts rather than mint a subset.
    #[error("no payout script resolves for bond outpoint {bond_outpoint} (I7: missing is never empty)")]
    PayeeUnresolvable { bond_outpoint: TransactionOutpoint },

    /// Summing one record's amounts overflowed u64 — an upstream construction bug that
    /// must never be interpreted as a small (wrapped) amount.
    #[error("PALW credit record for job {job_id} overflows u64 summing its amounts")]
    RecordOverflow { job_id: Hash64 },

    /// A pending record is keyed under one job_id but carries another — index corruption,
    /// refused before any arithmetic trusts the record.
    #[error("PALW credit index key names job {keyed} but the record carries job {carried}")]
    KeyRecordMismatch { keyed: Hash64, carried: Hash64 },
}

impl PalwFinalizedCreditIndexV3 {
    /// Record a finalized credit. Errors if the job_id is already pending or consumed
    /// (I3) — a job's mint-side residue exists at most once, ever, under ANY
    /// `finalized_daa` (a replayed finalization at a different DAA is still the same
    /// job and still refuses).
    pub fn record(&mut self, r: PalwFinalizedCreditRecordV3) -> Result<(), PalwCreditBatchError> {
        if self.consumed.contains(&r.job_id) {
            return Err(PalwCreditBatchError::AlreadyConsumed { job_id: r.job_id });
        }
        if self.pending.keys().any(|(_, id)| *id == r.job_id) {
            return Err(PalwCreditBatchError::DuplicateJobId { job_id: r.job_id });
        }
        self.pending.insert((r.finalized_daa, r.job_id), r);
        Ok(())
    }

    /// Consume the batch's ids (called when the crediting block is accepted). Errors on
    /// unknown id. All-or-nothing: the ids are validated as a set BEFORE any mutation,
    /// so a bad batch leaves the index byte-identical (no partial consumption to unwind
    /// on the error path).
    pub fn consume(&mut self, ids: &[Hash64]) -> Result<(), PalwCreditBatchError> {
        let mut keys = Vec::with_capacity(ids.len());
        let mut seen = BTreeSet::new();
        for id in ids {
            if self.consumed.contains(id) || !seen.insert(*id) {
                return Err(PalwCreditBatchError::AlreadyConsumed { job_id: *id });
            }
            let Some(key) = self.pending.keys().find(|(_, pending_id)| pending_id == id).copied() else {
                return Err(PalwCreditBatchError::UnknownCreditId { job_id: *id });
            };
            keys.push(key);
        }
        for (daa, id) in keys {
            self.pending.remove(&(daa, id));
            self.consumed.insert(id);
        }
        Ok(())
    }
}

/// One block's credit batch: what [`compute_palw_credit_batch_v3`] decided the coinbase
/// mints, which pending ids that consumes, and how much of the block budget it spent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCreditBatchV3 {
    pub outputs: Vec<TransactionOutput>,
    pub consumed_credit_ids: Vec<Hash64>,
    pub consumed_budget: u64,
}

/// Deterministic, prefix-mandatory batch: walk `pending` in key order; a record is
/// included iff it is not consumed, its `finalized_daa <= current_daa`, and the whole
/// record (executor + all verifier awards) fits the remaining budget and remaining
/// `max_outputs`. The FIRST record that does not fit STOPS the walk (no skipping —
/// skipping would be miner discretion / censorship). Payees resolve ONLY through
/// `resolve_spk(bond_outpoint)`; a resolver miss is an ERROR (I7: missing is never
/// empty), aborting the whole batch. Zero-amount awards mint no output but still count
/// as consumed with the record.
///
/// A record's outputs mint executor-first, then verifier awards in recorded order, so
/// construction and validation produce byte-identical output vectors.
pub fn compute_palw_credit_batch_v3<F>(
    index: &PalwFinalizedCreditIndexV3,
    current_daa: u64,
    block_budget: u64,
    max_outputs: usize,
    mut resolve_spk: F,
) -> Result<PalwCreditBatchV3, PalwCreditBatchError>
where
    F: FnMut(&TransactionOutpoint) -> Option<[u8; 64]>,
{
    let mut batch = PalwCreditBatchV3 { outputs: Vec::new(), consumed_credit_ids: Vec::new(), consumed_budget: 0 };
    for ((_, keyed_id), record) in &index.pending {
        if *keyed_id != record.job_id {
            return Err(PalwCreditBatchError::KeyRecordMismatch { keyed: *keyed_id, carried: record.job_id });
        }
        // Defense in depth: `record`/`consume` keep pending and consumed disjoint, but a
        // deserialized index is the caller's bytes — a consumed id never mints (I3) and
        // never stops the walk (it is not a "does not fit", it is a "does not exist").
        if index.consumed.contains(&record.job_id) {
            continue;
        }
        // Future-dated records are not yet due. They sort as a suffix of the key order,
        // but exclusion is by the predicate, not the position — they never stop the walk.
        if record.finalized_daa > current_daa {
            continue;
        }
        let total = record.total_amount().ok_or(PalwCreditBatchError::RecordOverflow { job_id: record.job_id })?;
        // Whole-record fit, both meters. The first miss ends the batch: everything after
        // this record in key order waits for a block with room (prefix-mandatory).
        let fits_budget = total <= block_budget - batch.consumed_budget;
        let fits_outputs = record.output_count() <= max_outputs - batch.outputs.len();
        if !fits_budget || !fits_outputs {
            break;
        }
        // I4/I7: every payee of the includable record must resolve — zero-amount awards
        // included, so a dangling bond outpoint surfaces here, not when the award grows.
        let executor_payload = resolve_spk(&record.executor_bond_outpoint)
            .ok_or(PalwCreditBatchError::PayeeUnresolvable { bond_outpoint: record.executor_bond_outpoint })?;
        if record.executor_amount > 0 {
            batch.outputs.push(TransactionOutput::new(
                record.executor_amount,
                crate::dns_finality::p2pkh_mldsa87_spk(&executor_payload),
            ));
        }
        for (bond_outpoint, award) in &record.verifier_awards {
            let payload =
                resolve_spk(bond_outpoint).ok_or(PalwCreditBatchError::PayeeUnresolvable { bond_outpoint: *bond_outpoint })?;
            if *award > 0 {
                batch.outputs.push(TransactionOutput::new(*award, crate::dns_finality::p2pkh_mldsa87_spk(&payload)));
            }
        }
        batch.consumed_credit_ids.push(record.job_id);
        batch.consumed_budget += total;
    }
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns_finality::p2pkh_mldsa87_spk;

    fn h64(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn outpoint(byte: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(h64(byte), 0)
    }

    /// A record minting `executor` to bond 0xB0+n and one verifier award to 0xC0+n.
    fn record(job: u8, finalized_daa: u64, executor: u64, verifier: u64) -> PalwFinalizedCreditRecordV3 {
        PalwFinalizedCreditRecordV3 {
            job_id: h64(job),
            finalized_daa,
            executor_bond_outpoint: outpoint(0xB0 + job),
            executor_amount: executor,
            verifier_awards: vec![(outpoint(0xC0 + job), verifier)],
        }
    }

    /// A resolver that knows every bond outpoint: the payload is the outpoint's own
    /// transaction-id bytes, so distinct payees get distinct scripts.
    fn resolve_all(bond: &TransactionOutpoint) -> Option<[u8; 64]> {
        Some(bond.transaction_id.as_bytes())
    }

    fn index_of(records: Vec<PalwFinalizedCreditRecordV3>) -> PalwFinalizedCreditIndexV3 {
        let mut index = PalwFinalizedCreditIndexV3::default();
        for r in records {
            index.record(r).unwrap();
        }
        index
    }

    /// Determinism: the same index and inputs produce byte-identical batches, and the
    /// iteration order is (finalized_daa, job_id) regardless of insertion order.
    #[test]
    fn determinism_iteration_order_is_key_order_not_insertion_order() {
        let a = record(0x01, 200, 10, 1);
        let b = record(0x02, 100, 20, 2);
        let c = record(0x03, 100, 30, 3);
        let forward = index_of(vec![a.clone(), b.clone(), c.clone()]);
        let reversed = index_of(vec![c, b, a]);
        let batch = compute_palw_credit_batch_v3(&forward, 1_000, u64::MAX, usize::MAX, resolve_all).unwrap();
        let batch2 = compute_palw_credit_batch_v3(&reversed, 1_000, u64::MAX, usize::MAX, resolve_all).unwrap();
        assert_eq!(batch, batch2, "insertion order must not leak into the batch");
        // (100, 0x02), (100, 0x03), (200, 0x01): DAA first, job_id breaking the tie.
        assert_eq!(batch.consumed_credit_ids, vec![h64(0x02), h64(0x03), h64(0x01)]);
        assert_eq!(batch.consumed_budget, 66);
        assert_eq!(batch.outputs.len(), 6);
    }

    /// Prefix-mandatory: with a budget that fits records 1 and 3 but not 2, the batch
    /// contains ONLY record 1 — the walk stops at 2 and 3 is NOT cherry-picked, because
    /// skipping is exactly the miner discretion this rule exists to forbid.
    #[test]
    fn prefix_mandatory_first_misfit_stops_the_walk() {
        let index = index_of(vec![record(0x01, 100, 10, 0), record(0x02, 200, 500, 0), record(0x03, 300, 10, 0)]);
        let batch = compute_palw_credit_batch_v3(&index, 1_000, 25, usize::MAX, resolve_all).unwrap();
        assert_eq!(batch.consumed_credit_ids, vec![h64(0x01)], "record 3 fits the leftover budget but must NOT be cherry-picked");
        assert_eq!(batch.consumed_budget, 10);
        assert_eq!(batch.outputs.len(), 1);
    }

    /// Budget ceiling: consumed_budget ≤ block_budget always, and a first record whose
    /// own sum exceeds the whole budget yields an EMPTY batch — an oversized record is a
    /// wait, not an error.
    #[test]
    fn budget_ceiling_holds_and_an_oversized_record_yields_an_empty_batch() {
        let index = index_of(vec![record(0x01, 100, 900, 200), record(0x02, 200, 1, 1)]);
        let batch = compute_palw_credit_batch_v3(&index, 1_000, 1_000, usize::MAX, resolve_all).unwrap();
        assert_eq!(batch, PalwCreditBatchV3 { outputs: vec![], consumed_credit_ids: vec![], consumed_budget: 0 });

        // Exact fit is a fit; the ceiling is ≤, not <.
        let index = index_of(vec![record(0x01, 100, 900, 100)]);
        let batch = compute_palw_credit_batch_v3(&index, 1_000, 1_000, usize::MAX, resolve_all).unwrap();
        assert_eq!(batch.consumed_budget, 1_000);
        assert_eq!(batch.consumed_credit_ids, vec![h64(0x01)]);
    }

    /// max_outputs is a hard cap counted in ACTUAL outputs: zero-amount awards occupy no
    /// slot, so a record with a zero verifier award fits a one-slot block, and the next
    /// two-output record stops the walk.
    #[test]
    fn max_outputs_counts_actual_outputs_and_zero_awards_are_free() {
        let index = index_of(vec![record(0x01, 100, 10, 0), record(0x02, 200, 10, 5)]);
        let batch = compute_palw_credit_batch_v3(&index, 1_000, u64::MAX, 1, resolve_all).unwrap();
        assert_eq!(batch.outputs.len(), 1, "the zero verifier award of record 1 mints nothing and counts nothing");
        assert_eq!(batch.consumed_credit_ids, vec![h64(0x01)]);

        // Two slots admit record 2's pair as well.
        let batch = compute_palw_credit_batch_v3(&index, 1_000, u64::MAX, 3, resolve_all).unwrap();
        assert_eq!(batch.outputs.len(), 3);
        assert_eq!(batch.consumed_credit_ids, vec![h64(0x01), h64(0x02)]);

        // An all-zero record consumes with zero outputs and zero budget.
        let index = index_of(vec![record(0x07, 100, 0, 0)]);
        let batch = compute_palw_credit_batch_v3(&index, 1_000, 0, 0, resolve_all).unwrap();
        assert_eq!(batch.outputs, vec![]);
        assert_eq!(batch.consumed_credit_ids, vec![h64(0x07)], "zero-amount records still consume (I3)");
        assert_eq!(batch.consumed_budget, 0);
    }

    /// I3: record() rejects a job_id already pending (even under a different
    /// finalized_daa) or already consumed; consume() then record() again also rejects,
    /// and consume() refuses a second consumption or an unknown id.
    #[test]
    fn i3_credit_once_across_record_and_consume() {
        let mut index = index_of(vec![record(0x01, 100, 10, 1)]);
        let mut replay = record(0x01, 999, 10, 1); // same job, different DAA — still the same job
        assert_eq!(index.record(replay.clone()), Err(PalwCreditBatchError::DuplicateJobId { job_id: h64(0x01) }));

        index.consume(&[h64(0x01)]).unwrap();
        assert!(index.pending.is_empty());
        assert!(index.consumed.contains(&h64(0x01)));
        replay.finalized_daa = 100;
        assert_eq!(index.record(replay), Err(PalwCreditBatchError::AlreadyConsumed { job_id: h64(0x01) }));
        assert_eq!(index.consume(&[h64(0x01)]), Err(PalwCreditBatchError::AlreadyConsumed { job_id: h64(0x01) }));
        assert_eq!(index.consume(&[h64(0x02)]), Err(PalwCreditBatchError::UnknownCreditId { job_id: h64(0x02) }));

        // A duplicate inside ONE consume() call is the same double-mint, and the failed
        // call leaves the index untouched (all-or-nothing).
        let mut index = index_of(vec![record(0x03, 100, 10, 1)]);
        let before = index.clone();
        assert_eq!(index.consume(&[h64(0x03), h64(0x03)]), Err(PalwCreditBatchError::AlreadyConsumed { job_id: h64(0x03) }));
        assert_eq!(index, before, "a refused consume() mutates nothing");
    }

    /// I7: a resolver miss on ANY payee of an otherwise-includable record returns
    /// Err(PayeeUnresolvable) naming that outpoint — never a partial batch.
    #[test]
    fn i7_a_resolver_miss_aborts_the_whole_batch() {
        let index = index_of(vec![record(0x01, 100, 10, 1), record(0x02, 200, 10, 1)]);
        // Record 1 resolves fully; record 2's VERIFIER outpoint (0xC2) is the miss.
        let missing = outpoint(0xC0 + 0x02);
        let err = compute_palw_credit_batch_v3(&index, 1_000, u64::MAX, usize::MAX, |bond| {
            (*bond != missing).then(|| bond.transaction_id.as_bytes())
        })
        .unwrap_err();
        assert_eq!(err, PalwCreditBatchError::PayeeUnresolvable { bond_outpoint: missing });
    }

    /// I4: every output's script equals p2pkh_mldsa87_spk of the resolver's payload for
    /// the RIGHT outpoint — two distinct payloads land on their own outputs, executor
    /// first, then awards in recorded order.
    #[test]
    fn i4_scripts_are_the_resolved_payloads_per_outpoint() {
        let index = index_of(vec![record(0x01, 100, 10, 7)]);
        let executor_payload = [0xEE; 64];
        let verifier_payload = [0x77; 64];
        let batch = compute_palw_credit_batch_v3(&index, 1_000, u64::MAX, usize::MAX, |bond| {
            if *bond == outpoint(0xB1) {
                Some(executor_payload)
            } else if *bond == outpoint(0xC1) {
                Some(verifier_payload)
            } else {
                None
            }
        })
        .unwrap();
        assert_eq!(batch.outputs.len(), 2);
        assert_eq!(batch.outputs[0], TransactionOutput::new(10, p2pkh_mldsa87_spk(&executor_payload)));
        assert_eq!(batch.outputs[1], TransactionOutput::new(7, p2pkh_mldsa87_spk(&verifier_payload)));
    }

    /// Future-dated records (finalized_daa > current_daa) are excluded and never stop
    /// the walk: with a due record sorted BETWEEN two future-dated positions impossible
    /// (DAA is the major key), the future set is a suffix — a budget stop inside the due
    /// prefix and the future suffix's exclusion are separate effects, shown separately.
    #[test]
    fn future_dated_records_are_excluded_not_walk_stoppers() {
        // All due except the (500, ..) suffix; the walk consumes the whole due prefix.
        let index = index_of(vec![record(0x01, 100, 10, 1), record(0x02, 200, 10, 1), record(0x03, 500, 10, 1)]);
        let batch = compute_palw_credit_batch_v3(&index, 250, u64::MAX, usize::MAX, resolve_all).unwrap();
        assert_eq!(batch.consumed_credit_ids, vec![h64(0x01), h64(0x02)], "the future suffix is excluded, not an error");
        assert_eq!(batch.consumed_budget, 22);

        // The stop reason is only ever budget/max_outputs: an oversized record at the
        // head stops the walk BEFORE the due record behind it, future suffix regardless.
        let index = index_of(vec![record(0x01, 100, 900, 200), record(0x02, 200, 1, 1), record(0x03, 500, 1, 1)]);
        let batch = compute_palw_credit_batch_v3(&index, 250, 1_000, usize::MAX, resolve_all).unwrap();
        assert_eq!(batch, PalwCreditBatchV3 { outputs: vec![], consumed_credit_ids: vec![], consumed_budget: 0 });

        // A future-dated record alone yields an empty batch and no resolver call at all.
        let index = index_of(vec![record(0x04, 500, 10, 1)]);
        let batch = compute_palw_credit_batch_v3(&index, 250, u64::MAX, usize::MAX, |_| -> Option<[u8; 64]> {
            panic!("a not-yet-due record must not resolve payees")
        })
        .unwrap();
        assert_eq!(batch.consumed_credit_ids, vec![]);
    }

    /// A record whose amounts overflow u64 is an error, never a wrapped "fit".
    #[test]
    fn record_overflow_is_an_error_not_a_wrap() {
        let mut r = record(0x01, 100, u64::MAX, 0);
        r.verifier_awards = vec![(outpoint(0xC1), 1)];
        let index = index_of(vec![r]);
        let err = compute_palw_credit_batch_v3(&index, 1_000, u64::MAX, usize::MAX, resolve_all).unwrap_err();
        assert_eq!(err, PalwCreditBatchError::RecordOverflow { job_id: h64(0x01) });
    }

    /// Borsh roundtrip of the index: pending (with its tuple keys) and consumed survive
    /// serialization byte-for-byte — the pruning-surviving representation is the struct.
    #[test]
    fn borsh_roundtrip_of_the_index() {
        let mut index = index_of(vec![record(0x01, 100, 10, 1), record(0x02, 200, 20, 2)]);
        index.consume(&[h64(0x01)]).unwrap();
        let bytes = borsh::to_vec(&index).unwrap();
        let back: PalwFinalizedCreditIndexV3 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, index);
        assert_eq!(borsh::to_vec(&back).unwrap(), bytes, "re-serialization is byte-identical");
    }
}
