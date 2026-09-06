//! Groups transaction validations that depend on the containing header and/or
//! its past headers (but do not depend on UTXO state or other transactions in
//! the containing block)

use super::{
    TransactionValidator,
    errors::{TxResult, TxRuleError},
};
use crate::constants::LOCK_TIME_THRESHOLD;
use kaspa_consensus_core::tx::Transaction;

pub(crate) enum LockTimeType {
    Finalized,
    DaaScore,
    Time,
}

pub(crate) enum LockTimeArg {
    Finalized,
    DaaScore(u64),
    MedianTime(u64),
}

impl TransactionValidator {
    pub(crate) fn validate_tx_in_header_context_with_args(
        &self,
        tx: &Transaction,
        ctx_daa_score: u64,
        ctx_block_time: u64,
    ) -> TxResult<()> {
        self.validate_tx_in_header_context(
            tx,
            match Self::get_lock_time_type(tx) {
                LockTimeType::Finalized => LockTimeArg::Finalized,
                LockTimeType::DaaScore => LockTimeArg::DaaScore(ctx_daa_score),
                LockTimeType::Time => LockTimeArg::MedianTime(ctx_block_time),
            },
            ctx_daa_score,
        )
    }

    /// `ctx_daa_score` is the containing block's own DAA score, ALWAYS — independently of the
    /// lock-time arm, which is `Finalized` for most transactions and carries no score. ADR-0087
    /// Decision 6 is a height rule and needs the height whatever the lock time says.
    pub(crate) fn validate_tx_in_header_context(
        &self,
        tx: &Transaction,
        lock_time_arg: LockTimeArg,
        ctx_daa_score: u64,
    ) -> TxResult<()> {
        self.check_tx_is_finalized(tx, lock_time_arg)?;
        self.check_model_sink_outputs_in_context(tx, ctx_daa_score)
    }

    /// **ADR-0087 Decision 6, the height-indexed half of the sink carve-out** (mainnet audit
    /// 2026-09-06, M-9).
    ///
    /// Decision 6: "below the activation the objects are refused and no market exists". The
    /// isolation door (`check_transaction_pq_output_classes`) is context-free and can only ask
    /// whether this ruleset declares the market AT ALL, so on a build that schedules the fence for
    /// a future score it let a sink output through from genesis — a consensus transaction rule
    /// relaxed H blocks before the market exists, and a one-directional validity disagreement with
    /// every peer that has not scheduled it (`flow_context.rs` keeps such a peer, with a warning).
    ///
    /// This is the same remedy `check_transaction_pq_output_classes`' own M-06 note names for a
    /// non-genesis PQ activation: thread the activation score into a context-bearing check.
    ///
    /// Order matters and is deliberate: isolation stays PERMISSIVE (it must, or no sink could ever
    /// be valid), and this refuses before the transaction reaches the UTXO walk, so nothing is
    /// committed on the strength of the permissive answer.
    ///
    /// Inert on every shipped preset: with the fence `None`, `model_sink_outputs_allowed` is
    /// `false`, isolation has already refused the output, and this returns on its first line.
    fn check_model_sink_outputs_in_context(&self, tx: &Transaction, ctx_daa_score: u64) -> TxResult<()> {
        if !self.model_sink_outputs_allowed {
            // The ruleset does not declare the market: isolation refused the form already, and
            // this walk would be a per-transaction cost that buys nothing.
            return Ok(());
        }
        if matches!(self.palw_model_market_fence, Some(fence) if fence.is_active(ctx_daa_score)) {
            return Ok(());
        }
        for (i, output) in tx.outputs.iter().enumerate() {
            if kaspa_consensus_core::palw_model_market_v1::palw_model_sink_class_v1(&output.script_public_key).is_some() {
                return Err(TxRuleError::ModelSinkBeforeMarketActivation(i, ctx_daa_score));
            }
        }
        Ok(())
    }

    pub(crate) fn get_lock_time_type(tx: &Transaction) -> LockTimeType {
        match tx.lock_time {
            // Lock time of zero means the transaction is finalized.
            0 => LockTimeType::Finalized,

            // The lock time field of a transaction is either a block DAA score at
            // which the transaction is finalized or a timestamp depending on if the
            // value is before the LOCK_TIME_THRESHOLD. When it is under the
            // threshold it is a DAA score
            t if t < LOCK_TIME_THRESHOLD => LockTimeType::DaaScore,

            // ..and when equal or above the threshold it represents time
            _t => LockTimeType::Time,
        }
    }

    fn check_tx_is_finalized(&self, tx: &Transaction, lock_time_arg: LockTimeArg) -> TxResult<()> {
        let block_time_or_daa_score = match lock_time_arg {
            LockTimeArg::Finalized => return Ok(()),
            LockTimeArg::DaaScore(ctx_daa_score) => ctx_daa_score,
            LockTimeArg::MedianTime(ctx_block_time) => ctx_block_time,
        };

        if tx.lock_time < block_time_or_daa_score {
            return Ok(());
        }

        // At this point, the transaction's lock time hasn't occurred yet, but
        // the transaction might still be finalized if the sequence number
        // for all transaction inputs is maxed out.
        for (i, input) in tx.inputs.iter().enumerate() {
            if input.sequence != u64::MAX {
                return Err(TxRuleError::NotFinalized(i));
            }
        }

        Ok(())
    }
}
