//! ADR-0037 Decision 2 + Decision 8 / ADR-0038 Decision G: the pruning-surviving job
//! ledger, the per-executor rate state, and the epoch budget valve.
//!
//! [`PalwJobLedgerV3`] is the state object that retires the per-block challenge-horizon
//! walk: one [`crate::palw_job_state::PalwJobStateV3`] per `job_id`, keyed in a `BTreeMap`
//! so equal job sets serialize byte-identically regardless of insertion order (I14's
//! determinism, structurally). The ledger owns exactly three facts the spine does not:
//!
//! * **Admission is first-accepted-wins** — a `job_id` already present refuses a second
//!   insert ([`PalwJobLedgerError::DuplicateJob`]), the sibling of I3's credit-once rule.
//! * **Missing is an error, never an empty answer** (I7) — [`PalwJobLedgerV3::get`] on an
//!   unknown id returns [`PalwJobLedgerError::MissingJob`]; there is no lookup in this
//!   module that silently defaults, because the audited fail-opens were exactly such
//!   lookups answering "no history" where the truth was "history you failed to load".
//! * **A claim bit sets exactly once** (I3) — [`PalwJobLedgerV3::claim_reward`] records
//!   the payment fact per claimant bit; whether a claim is *payable* is the credit
//!   consumer's gate ([`crate::palw_job_state::PalwJobStatusV3::creditable`]), this ledger
//!   only makes double-payment inexpressible.
//!
//! [`PalwExecutorCreditStateV3`] is ADR-0037 Decision 8 made state: no telemetry-derived
//! `P_check` enters any safety inequality, so the binding limits are the per-class credit
//! interval and the bond-exposure cap `exposure ≤ bond × limit‰ / 1000`, both checked (and
//! recorded) *at credit time*. The u128 intermediate in the cap is the structural fix for
//! the measured `max_leverage` 11,655× violation — the old code's u64 product wrapped and
//! the cap evaporated. Per-identity limits remain fairness aids (Sybil-splittable), which
//! is why [`PalwEpochBudgetV3`] exists: ADR-0037 Decision 7 / ADR-0038 Decision G make the
//! global block and epoch budgets the real valve — `palw_reward_block ≤ block_cap` and
//! `palw_reward_epoch ≤ epoch_cap`, refused atomically (a refusal records nothing).
//!
//! Everything here is arithmetic over the caller's facts — no store handle, no clock, no
//! registry — same shape as [`crate::palw_credit`] and the spine itself. Consensus-inert:
//! nothing constructs any of these types on any shipped network; the Track-C change set
//! wires them, and per ADR-0037 partial *activation* is forbidden.

use crate::palw_job_state::{PalwJobEventV3, PalwJobStateV3, PalwJobStatusV3, PalwJobTransitionError};
use kaspa_hashes::Hash64;
use std::collections::BTreeMap;

/// Refusals of the ledger, the rate state and the budget. Every arm names the exact
/// numbers that refused — a budget or leverage rejection must be auditable from the error
/// alone, without replaying the state that produced it.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwJobLedgerError {
    /// I7: the job is not in the ledger. Missing is an ERROR, never an empty answer —
    /// the audited fail-opens were lookups answering "no history" for "not loaded".
    #[error("PALW job {job_id} is not in the ledger (I7: missing is an error, not an empty answer)")]
    MissingJob { job_id: Hash64 },
    /// First-accepted-wins (I3's admission sibling): the `job_id` is already present, or
    /// the offered job is not `Open` (a non-`Open` job has history, so it is not NEW).
    #[error("PALW job {job_id} is already admitted (first-accepted-wins)")]
    DuplicateJob { job_id: Hash64 },
    /// The spine refused the event — illegal in the job's current status.
    #[error(transparent)]
    Transition(#[from] PalwJobTransitionError),
    /// I3: this claimant's bit is already set — a job credits at most once per claimant.
    #[error("PALW job {job_id} reward bit {claimant_bit} is already claimed (I3: credit-once)")]
    RewardAlreadyClaimed { job_id: Hash64, claimant_bit: u8 },
    /// The claim bitmap carries 32 bits (executor bit 0 plus panel members); no bit ≥ 32.
    #[error("PALW claimant bit {got} is out of range (the claim bitmap carries bits 0..32)")]
    ClaimantBitOutOfRange { got: u8 },
    /// ADR-0037 Decision 8: the per-class credit interval has not elapsed.
    #[error(
        "PALW credit interval not elapsed for class {class_id}: last credited at DAA {last_daa}, \
         current DAA {current_daa}, minimum interval {min_interval}"
    )]
    CreditIntervalNotElapsed { class_id: Hash64, last_daa: u64, current_daa: u64, min_interval: u64 },
    /// ADR-0037 Decision 8: `exposure + addition` would exceed `bond × limit‰ / 1000`.
    #[error(
        "PALW executor leverage exceeded: exposure {exposure} + addition {addition} breaks \
         bond {bond_amount} × {limit_permille}‰ (the 11,655× violation's structural fence)"
    )]
    ExecutorLeverageExceeded { exposure: u64, addition: u64, bond_amount: u64, limit_permille: u16 },
    /// ADR-0037 Decision 7: one block's PALW payout exceeds the per-block cap.
    #[error("PALW block budget exceeded: addition {addition} > block cap {cap} (epoch spent so far: {spent})")]
    BlockBudgetExceeded { spent: u64, addition: u64, cap: u64 },
    /// ADR-0037 Decision 7: the epoch's accumulated PALW payout would exceed the epoch cap.
    #[error("PALW epoch budget exceeded: spent {spent} + addition {addition} > epoch cap {cap}")]
    EpochBudgetExceeded { spent: u64, addition: u64, cap: u64 },
    /// A u64 accumulator would wrap. Checked math everywhere — wrapping accumulators are
    /// how the leverage violation happened once already.
    #[error("PALW amount arithmetic overflowed u64")]
    AmountOverflow,
}

/// The pruning-surviving job ledger: one state object per `job_id` (ADR-0037 Decision 2).
/// A `BTreeMap` on purpose — deterministic iteration and serialization independent of
/// insertion order, so two nodes that admitted the same jobs in different DAG orders hold
/// byte-identical ledgers.
#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwJobLedgerV3 {
    pub jobs: BTreeMap<Hash64, PalwJobStateV3>,
}

impl PalwJobLedgerV3 {
    /// Admit a NEW job. First-accepted-wins: a `job_id` already present refuses with
    /// [`PalwJobLedgerError::DuplicateJob`], and so does a job whose status is not
    /// [`PalwJobStatusV3::Open`] — a job with history is not new, and admitting it would
    /// smuggle transitions past the spine's closed algebra.
    pub fn open(&mut self, job: PalwJobStateV3) -> Result<(), PalwJobLedgerError> {
        if job.status != PalwJobStatusV3::Open || self.jobs.contains_key(&job.job_id) {
            return Err(PalwJobLedgerError::DuplicateJob { job_id: job.job_id });
        }
        self.jobs.insert(job.job_id, job);
        Ok(())
    }

    /// I7 lookup: a missing job is an error, never an empty answer.
    pub fn get(&self, job_id: &Hash64) -> Result<&PalwJobStateV3, PalwJobLedgerError> {
        self.jobs.get(job_id).ok_or(PalwJobLedgerError::MissingJob { job_id: *job_id })
    }

    /// Apply a lifecycle event through the spine's closed algebra. Legality is entirely
    /// [`PalwJobStateV3::apply`]'s; a refusal leaves the job untouched (the spine only
    /// writes the successor status on success).
    pub fn apply(&mut self, job_id: &Hash64, event: PalwJobEventV3) -> Result<(), PalwJobLedgerError> {
        let job = self.jobs.get_mut(job_id).ok_or(PalwJobLedgerError::MissingJob { job_id: *job_id })?;
        job.apply(event)?;
        Ok(())
    }

    /// I3: set claim bit `claimant_bit` (0 = executor, 1.. = panel members) exactly once.
    /// Refuses a bit ≥ 32, a missing job, or an already-set bit. Deliberately does NOT
    /// check status: whether the claim is *payable* is the credit consumer's gate
    /// ([`PalwJobStatusV3::creditable`]); this ledger records the claim FACT, and makes
    /// recording it twice inexpressible.
    pub fn claim_reward(&mut self, job_id: &Hash64, claimant_bit: u8) -> Result<(), PalwJobLedgerError> {
        if claimant_bit >= 32 {
            return Err(PalwJobLedgerError::ClaimantBitOutOfRange { got: claimant_bit });
        }
        let job = self.jobs.get_mut(job_id).ok_or(PalwJobLedgerError::MissingJob { job_id: *job_id })?;
        let mask = 1u32 << claimant_bit;
        if job.reward_claimed_bitmap & mask != 0 {
            return Err(PalwJobLedgerError::RewardAlreadyClaimed { job_id: *job_id, claimant_bit });
        }
        job.reward_claimed_bitmap |= mask;
        Ok(())
    }
}

/// ADR-0037 Decision 8: per-executor rate state, enforced at credit time — never
/// telemetry. Whether a validator "really replayed" is unobservable, so nothing here
/// derives from self-declared capacity; the binding facts are what this state records:
/// when each class last credited, how much this epoch has credited, and how much credit
/// is outstanding against the bond.
#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecutorCreditStateV3 {
    /// Last DAA score at which this executor credited, per execution class.
    pub last_credited_daa_by_class: BTreeMap<Hash64, u64>,
    /// Total credited this epoch (sompi); zeroed by [`Self::roll_epoch`].
    pub credited_amount_this_epoch: u64,
    /// Credit issued but not yet finalized or voided (sompi) — the leverage numerator.
    pub active_unfinalized_exposure: u64,
}

impl PalwExecutorCreditStateV3 {
    /// The Decision 8 gate, check-then-record in one atomic step:
    ///
    /// 1. **Interval** — `current_daa − last ≥ min_credit_interval` for the class (no
    ///    prior credit passes vacuously). The comparison saturates on the `last +
    ///    interval` side, so a near-`u64::MAX` interval refuses rather than wrapping open.
    /// 2. **Leverage** — `active_unfinalized_exposure + addition ≤ bond_amount ×
    ///    leverage_limit_permille / 1000`, computed in u128: the structural fix for the
    ///    measured 11,655× violation, where the u64 product wrapped and the cap vanished.
    ///
    /// On success, and only then, ALL THREE facts record: `last_credited_daa_by_class`,
    /// `credited_amount_this_epoch` (checked add), `active_unfinalized_exposure` (checked
    /// add). A refusal — including an [`PalwJobLedgerError::AmountOverflow`] — records
    /// nothing.
    pub fn check_and_record_credit(
        &mut self,
        class_id: Hash64,
        current_daa: u64,
        min_credit_interval: u64,
        addition: u64,
        bond_amount: u64,
        leverage_limit_permille: u16,
    ) -> Result<(), PalwJobLedgerError> {
        if let Some(&last_daa) = self.last_credited_daa_by_class.get(&class_id)
            && current_daa < last_daa.saturating_add(min_credit_interval)
        {
            return Err(PalwJobLedgerError::CreditIntervalNotElapsed {
                class_id,
                last_daa,
                current_daa,
                min_interval: min_credit_interval,
            });
        }
        let cap = (bond_amount as u128) * (leverage_limit_permille as u128) / 1000;
        if (self.active_unfinalized_exposure as u128) + (addition as u128) > cap {
            return Err(PalwJobLedgerError::ExecutorLeverageExceeded {
                exposure: self.active_unfinalized_exposure,
                addition,
                bond_amount,
                limit_permille: leverage_limit_permille,
            });
        }
        // Compute every successor value before writing any, so a late overflow refusal
        // leaves the state exactly as it was (atomicity, same posture as the budget).
        let new_epoch_amount =
            self.credited_amount_this_epoch.checked_add(addition).ok_or(PalwJobLedgerError::AmountOverflow)?;
        let new_exposure =
            self.active_unfinalized_exposure.checked_add(addition).ok_or(PalwJobLedgerError::AmountOverflow)?;
        self.last_credited_daa_by_class.insert(class_id, current_daa);
        self.credited_amount_this_epoch = new_epoch_amount;
        self.active_unfinalized_exposure = new_exposure;
        Ok(())
    }

    /// A job finalized (or voided): release its exposure. Saturating on purpose —
    /// releasing more than is held clamps to zero rather than wrapping the leverage
    /// numerator to ~u64::MAX, a defensive posture the tests pin.
    pub fn release_exposure(&mut self, amount: u64) {
        self.active_unfinalized_exposure = self.active_unfinalized_exposure.saturating_sub(amount);
    }

    /// Epoch rollover: the epoch amount resets; exposure does NOT — a job unfinalized at
    /// the epoch edge still leverages the bond.
    pub fn roll_epoch(&mut self) {
        self.credited_amount_this_epoch = 0;
    }
}

/// ADR-0037 Decision 7 / ADR-0038 Decision G: the block and epoch budgets. Per-identity
/// rate limits are fairness aids — Sybil-splittable — so THIS is the real valve:
/// `palw_reward_block ≤ block_cap` and `palw_reward_epoch ≤ epoch_cap`, inside the
/// scheduled emission (the carve, never an append; I6/I15).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwEpochBudgetV3 {
    /// The most one block's total PALW payout may be (sompi).
    pub block_cap: u64,
    /// The most one epoch's accumulated PALW payout may be (sompi).
    pub epoch_cap: u64,
    /// Accumulated payout this epoch (sompi); zeroed by [`Self::roll_epoch`].
    pub spent_this_epoch: u64,
}

impl PalwEpochBudgetV3 {
    /// Charge one block's total PALW payout: `addition ≤ block_cap` AND
    /// `spent_this_epoch + addition ≤ epoch_cap`, checked math. Records on success only —
    /// a refusal leaves `spent_this_epoch` untouched.
    pub fn charge_block(&mut self, addition: u64) -> Result<(), PalwJobLedgerError> {
        if addition > self.block_cap {
            return Err(PalwJobLedgerError::BlockBudgetExceeded {
                spent: self.spent_this_epoch,
                addition,
                cap: self.block_cap,
            });
        }
        let new_spent = self.spent_this_epoch.checked_add(addition).ok_or(PalwJobLedgerError::AmountOverflow)?;
        if new_spent > self.epoch_cap {
            return Err(PalwJobLedgerError::EpochBudgetExceeded {
                spent: self.spent_this_epoch,
                addition,
                cap: self.epoch_cap,
            });
        }
        self.spent_this_epoch = new_spent;
        Ok(())
    }

    /// Epoch rollover: the spent accumulator resets; the caps are parameters, untouched.
    pub fn roll_epoch(&mut self) {
        self.spent_this_epoch = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_job_state::{PalwDeadlinesV3, PalwDualDeadlineV3};
    use crate::tx::TransactionOutpoint;
    use PalwJobEventV3 as E;
    use PalwJobStatusV3 as S;

    /// A well-formed `Open` job with the given id word; the same literal shape as the
    /// spine's own tests.
    fn job(id: u64) -> PalwJobStateV3 {
        PalwJobStateV3 {
            job_id: Hash64::from_u64_word(id),
            job_context_hash: Hash64::from_u64_word(2),
            model_band_id: Hash64::from_u64_word(3),
            execution_class_id: Hash64::from_u64_word(4),
            requester_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(5), 0),
            executor_bond_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(6), 0),
            executor_validator_id: Hash64::from_u64_word(7),
            commitment_root: Hash64::from_u64_word(8),
            trace_root: Hash64::from_u64_word(9),
            output_root: Hash64::from_u64_word(10),
            commitment_anchor_hash: Hash64::from_u64_word(11),
            eligible_set_snapshot_id: Hash64::from_u64_word(12),
            selected_verifier_bond_outpoints: vec![TransactionOutpoint::new(Hash64::from_u64_word(13), 1)],
            status: S::Open,
            deadlines: PalwDeadlinesV3 {
                commit_by: PalwDualDeadlineV3 { due_daa: 10, due_mtp: 10 },
                quorum_by: PalwDualDeadlineV3 { due_daa: 20, due_mtp: 20 },
                challenge_close: PalwDualDeadlineV3 { due_daa: 30, due_mtp: 30 },
            },
            user_escrow_amount: 1_000,
            max_inflation_credit: 500,
            reward_claimed_bitmap: 0,
        }
    }

    /// First-accepted-wins: a second `open()` with the same id refuses with
    /// `DuplicateJob`, and the first admission survives; a different id admits fine.
    #[test]
    fn open_is_first_accepted_wins() {
        let mut ledger = PalwJobLedgerV3::default();
        ledger.open(job(1)).unwrap();
        let mut second = job(1);
        second.user_escrow_amount = 9_999; // A would-be overwrite must not land.
        assert_eq!(ledger.open(second), Err(PalwJobLedgerError::DuplicateJob { job_id: Hash64::from_u64_word(1) }));
        assert_eq!(ledger.get(&Hash64::from_u64_word(1)).unwrap().user_escrow_amount, 1_000);
        ledger.open(job(2)).unwrap();
        assert_eq!(ledger.jobs.len(), 2);
    }

    /// I7: `get()` on an unknown id is `MissingJob` — never a silent default.
    #[test]
    fn get_missing_is_an_error() {
        let ledger = PalwJobLedgerV3::default();
        assert_eq!(
            ledger.get(&Hash64::from_u64_word(42)).unwrap_err(),
            PalwJobLedgerError::MissingJob { job_id: Hash64::from_u64_word(42) }
        );
    }

    /// `apply()` drives the spine: `Open --CommitmentAccepted--> Committed` is visible
    /// through `get()`; an illegal event refuses with a `Transition` error and the state
    /// is unchanged.
    #[test]
    fn apply_delegates_to_the_spine() {
        let mut ledger = PalwJobLedgerV3::default();
        let id = Hash64::from_u64_word(1);
        ledger.open(job(1)).unwrap();
        ledger.apply(&id, E::CommitmentAccepted).unwrap();
        assert_eq!(ledger.get(&id).unwrap().status, S::Committed);
        // Illegal in `Committed`: the spine refuses, and the status stays put.
        assert_eq!(
            ledger.apply(&id, E::ExactConviction),
            Err(PalwJobLedgerError::Transition(PalwJobTransitionError {
                status: S::Committed,
                event: E::ExactConviction
            }))
        );
        assert_eq!(ledger.get(&id).unwrap().status, S::Committed);
        // And an unknown job is I7, not a spine question.
        assert_eq!(
            ledger.apply(&Hash64::from_u64_word(9), E::CommitmentAccepted),
            Err(PalwJobLedgerError::MissingJob { job_id: Hash64::from_u64_word(9) })
        );
    }

    /// I3 on the claim bitmap: bit 0 then bit 1 set fine; bit 0 again is
    /// `RewardAlreadyClaimed`; bit 32 is out of range; an unknown job is `MissingJob`.
    #[test]
    fn claim_reward_sets_each_bit_exactly_once() {
        let mut ledger = PalwJobLedgerV3::default();
        let id = Hash64::from_u64_word(1);
        ledger.open(job(1)).unwrap();
        ledger.claim_reward(&id, 0).unwrap();
        ledger.claim_reward(&id, 1).unwrap();
        assert_eq!(ledger.get(&id).unwrap().reward_claimed_bitmap, 0b11);
        assert_eq!(
            ledger.claim_reward(&id, 0),
            Err(PalwJobLedgerError::RewardAlreadyClaimed { job_id: id, claimant_bit: 0 })
        );
        assert_eq!(ledger.claim_reward(&id, 32), Err(PalwJobLedgerError::ClaimantBitOutOfRange { got: 32 }));
        assert_eq!(
            ledger.claim_reward(&Hash64::from_u64_word(9), 0),
            Err(PalwJobLedgerError::MissingJob { job_id: Hash64::from_u64_word(9) })
        );
        // The refusals recorded nothing.
        assert_eq!(ledger.get(&id).unwrap().reward_claimed_bitmap, 0b11);
    }

    /// Decision 8 interval: a first credit passes with no history; a second inside the
    /// interval refuses; at exactly `last + interval` it passes; and class A's history
    /// never gates class B.
    #[test]
    fn credit_interval_gates_per_class() {
        let class_a = Hash64::from_u64_word(0xA);
        let class_b = Hash64::from_u64_word(0xB);
        let mut state = PalwExecutorCreditStateV3::default();
        let credit = |state: &mut PalwExecutorCreditStateV3, class, daa| {
            state.check_and_record_credit(class, daa, 100, 10, 1_000_000, 1000)
        };
        credit(&mut state, class_a, 1_000).unwrap();
        assert_eq!(
            credit(&mut state, class_a, 1_099),
            Err(PalwJobLedgerError::CreditIntervalNotElapsed {
                class_id: class_a,
                last_daa: 1_000,
                current_daa: 1_099,
                min_interval: 100
            })
        );
        // The refusal recorded nothing: last stays 1_000, so exactly last+interval passes.
        credit(&mut state, class_a, 1_100).unwrap();
        // Class B is independent — same DAA neighborhood, no history of its own.
        credit(&mut state, class_b, 1_100).unwrap();
        assert_eq!(state.last_credited_daa_by_class[&class_a], 1_100);
        assert_eq!(state.last_credited_daa_by_class[&class_b], 1_100);
    }

    /// Decision 8 leverage: bond 1000 at 2000‰ caps exposure at 2000; 1500 + 600 refuses,
    /// 1500 + 500 lands exactly on the cap and records. The u128 intermediate keeps
    /// `u64::MAX × 1000‰` from wrapping — the 11,655× violation's exact failure mode.
    #[test]
    fn leverage_cap_holds_and_does_not_wrap() {
        let class = Hash64::from_u64_word(0xA);
        let mut state = PalwExecutorCreditStateV3 { active_unfinalized_exposure: 1_500, ..Default::default() };
        assert_eq!(
            state.check_and_record_credit(class, 10, 1, 600, 1_000, 2_000),
            Err(PalwJobLedgerError::ExecutorLeverageExceeded {
                exposure: 1_500,
                addition: 600,
                bond_amount: 1_000,
                limit_permille: 2_000
            })
        );
        // The refusal recorded nothing.
        assert_eq!(state.credited_amount_this_epoch, 0);
        assert!(state.last_credited_daa_by_class.is_empty());
        state.check_and_record_credit(class, 10, 1, 500, 1_000, 2_000).unwrap();
        assert_eq!(state.active_unfinalized_exposure, 2_000);
        assert_eq!(state.credited_amount_this_epoch, 500);
        assert_eq!(state.last_credited_daa_by_class[&class], 10);
        // u128 path: a u64::MAX bond at 1000‰ is a u64::MAX cap, and a max-sized addition
        // against a fresh state neither wraps the cap nor the accumulators.
        let mut huge = PalwExecutorCreditStateV3::default();
        huge.check_and_record_credit(class, 10, 1, u64::MAX, u64::MAX, 1_000).unwrap();
        assert_eq!(huge.active_unfinalized_exposure, u64::MAX);
    }

    /// `release_exposure` saturates at zero (never wraps the leverage numerator open),
    /// and `roll_epoch` zeroes the epoch amount but carries exposure across.
    #[test]
    fn release_saturates_and_epoch_roll_keeps_exposure() {
        let mut state = PalwExecutorCreditStateV3 {
            credited_amount_this_epoch: 700,
            active_unfinalized_exposure: 300,
            ..Default::default()
        };
        state.release_exposure(1_000);
        assert_eq!(state.active_unfinalized_exposure, 0);
        state.active_unfinalized_exposure = 300;
        state.roll_epoch();
        assert_eq!(state.credited_amount_this_epoch, 0);
        assert_eq!(state.active_unfinalized_exposure, 300, "exposure must survive the epoch edge");
    }

    /// Decision 7 budgets: a single block over `block_cap` refuses; accumulation crossing
    /// `epoch_cap` refuses with NOTHING recorded (atomicity); `roll_epoch` resets spend.
    #[test]
    fn budget_valve_refuses_atomically() {
        let mut budget = PalwEpochBudgetV3 { block_cap: 100, epoch_cap: 150, spent_this_epoch: 0 };
        assert_eq!(
            budget.charge_block(101),
            Err(PalwJobLedgerError::BlockBudgetExceeded { spent: 0, addition: 101, cap: 100 })
        );
        budget.charge_block(100).unwrap();
        assert_eq!(budget.spent_this_epoch, 100);
        assert_eq!(
            budget.charge_block(100),
            Err(PalwJobLedgerError::EpochBudgetExceeded { spent: 100, addition: 100, cap: 150 })
        );
        assert_eq!(budget.spent_this_epoch, 100, "a refusal must record nothing");
        budget.charge_block(50).unwrap();
        assert_eq!(budget.spent_this_epoch, 150);
        budget.roll_epoch();
        assert_eq!(budget.spent_this_epoch, 0);
        budget.charge_block(100).unwrap();
    }

    /// Borsh roundtrip of all three state types — the ledger with a live job, the credit
    /// state with per-class history, the budget mid-epoch.
    #[test]
    fn borsh_roundtrips_all_three_types() {
        let mut ledger = PalwJobLedgerV3::default();
        ledger.open(job(1)).unwrap();
        ledger.apply(&Hash64::from_u64_word(1), E::CommitmentAccepted).unwrap();
        let ledger2: PalwJobLedgerV3 = borsh::from_slice(&borsh::to_vec(&ledger).unwrap()).unwrap();
        assert_eq!(ledger, ledger2);

        let mut credit = PalwExecutorCreditStateV3::default();
        credit.check_and_record_credit(Hash64::from_u64_word(0xA), 10, 1, 5, 1_000, 1_000).unwrap();
        let credit2: PalwExecutorCreditStateV3 = borsh::from_slice(&borsh::to_vec(&credit).unwrap()).unwrap();
        assert_eq!(credit, credit2);

        let budget = PalwEpochBudgetV3 { block_cap: 100, epoch_cap: 150, spent_this_epoch: 40 };
        let budget2: PalwEpochBudgetV3 = borsh::from_slice(&borsh::to_vec(&budget).unwrap()).unwrap();
        assert_eq!(budget, budget2);
    }

    /// Determinism: the same job set admitted in different orders serializes
    /// byte-identically — the BTreeMap makes admission order unobservable in state.
    #[test]
    fn insertion_order_is_unobservable_in_serialized_state() {
        let mut forward = PalwJobLedgerV3::default();
        for id in [1u64, 2, 3] {
            forward.open(job(id)).unwrap();
        }
        let mut backward = PalwJobLedgerV3::default();
        for id in [3u64, 2, 1] {
            backward.open(job(id)).unwrap();
        }
        assert_eq!(forward, backward);
        assert_eq!(borsh::to_vec(&forward).unwrap(), borsh::to_vec(&backward).unwrap());
    }
}
