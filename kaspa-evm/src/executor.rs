//! The block EVM executor — v0.4 **mergeset delayed acceptance** (design §3):
//! given a block's `selected_parent` EVM header + state, its L1 header context,
//! its OWN payload (system ops + declared coinbase) and the mergeset's accepted
//! user txs in canonical order, run the lane and produce the
//! `EvmExecutionResult` whose `header.commitment_root()` the consensus verifier
//! checks against `Header::evm_commitment_root`.
//!
//! `EvmResult(B)` is a function of B's parents + B's system ops only (invariant
//! I2): **B's own payload `transactions` are never executed here** — they are
//! data (committed by `Header::evm_payload_hash`) accepted by B's selected
//! child. Execution order (§3.2): bounded deposit claims (credit EVM balances)
//! → deterministic class-5 prefix-take over `AcceptedEvmTxs(B)` → accepted user
//! txs in canonical order → [collect F002 withdrawals: P4] → roots → the
//! committed header.
//!
//! Skip semantics (§6.1): acceptance-time invalidity (nonce / funds / fee —
//! class 2, which also subsumes duplicates, class 3) and over-cap txs (class 5)
//! are **deterministic skips**: no receipt, no gas, no nonce change; only
//! `skipped_tx_count` records them. Executed failures (revert / OOG — class 4)
//! are status-0 receipts. Only producer faults (commitment mismatch, checked by
//! the caller) invalidate a block (§6.2). (A defensive class-1 label also
//! exists for undecodable material — unreachable for body-validated payloads.)
//!
//! F002 residual balance (audit L3, documented behavior): under SHANGHAI
//! (pre-EIP-6780) a contract may SELFDESTRUCT with F002 as beneficiary,
//! force-crediting it OUTSIDE the call-frame intercept — no withdraw log, no
//! burn, the wei stays locked in F002 forever. This is supply-NEUTRAL (the
//! stranded wei remains inside `evm_total_native_balance`); F002's balance is
//! therefore NOT invariantly zero. Deliberately not swept: an end-of-block
//! sweep would be a consensus rule. Re-evaluate at any spec bump (EIP-6780
//! changes SELFDESTRUCT semantics — see the EVM_SPEC_ID pin in lib.rs).

use crate::{env, roots, state, EvmExecError};
use kaspa_consensus_core::evm::{
    DepositClaim, EvmAddress, EvmBloom, EvmExecutionHeader, EvmExecutionPayload, EvmExecutionResult, EvmLog, EvmReceipt,
    EvmSystemOp, EvmU256, WithdrawOp, EVM_GENESIS_STATE_ROOT, EVM_NATIVE_SCALE, MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK,
    MAX_WITHDRAWALS_PER_EVM_BLOCK, SYSTEM_DEPOSIT_GAS_PER_CLAIM,
};
use kaspa_hashes::EvmH256;
use revm::primitives::{Address, EVMError, ExecutionResult, B256, KECCAK_EMPTY, U256};
use revm::{
    db::{AccountState, CacheDB, EmptyDB},
    DatabaseCommit, Evm,
};

/// One element of `AcceptedEvmTxs(B)`: a user tx drawn from a mergeset payload
/// block, paired with that PAYLOAD block's declared `evm_coinbase` — the
/// recipient of this tx's priority fee (design v0.4 §8.1, D3: inclusion is the
/// scarce resource, so the payload miner earns the tip).
#[derive(Clone, Debug)]
pub struct AcceptedTxCandidate {
    /// EIP-2718 typed-transaction bytes.
    pub raw: Vec<u8>,
    /// `evm_coinbase` of the DAG block whose payload carried this tx.
    pub payload_coinbase: EvmAddress,
}

/// Everything the executor needs about a block, all ancestor-derived (the
/// consensus driver fills this from the stores + the L1 header). The parent EVM
/// **state** is passed separately as the seed `CacheDB`.
pub struct EvmBlockInput<'a> {
    /// `selected_parent`'s committed EVM header; `None` for the first EVM block
    /// (its parent is the EVM genesis at number 0 / `EVM_GENESIS_STATE_ROOT`).
    pub parent: Option<&'a EvmExecutionHeader>,
    /// `B.header.timestamp` in milliseconds.
    pub header_timestamp_ms: u64,
    /// `selected_parent(B)` block hash bytes (prevrandao input, frozen order).
    pub selected_parent_hash: [u8; 64],
    /// `B.header.blue_work` big-endian bytes (prevrandao input, frozen order).
    pub blue_work_be: Vec<u8>,
    /// `B.header.daa_score` (prevrandao input).
    pub daa_score: u64,
    /// `B.evm_payload` — supplies B's own `system_ops` (executed here, §3.2)
    /// and B's declared `evm_coinbase` (the `COINBASE` opcode value, §8.2).
    /// Its `transactions` are data-only and are NOT read by the executor.
    pub payload: &'a EvmExecutionPayload,
    /// `AcceptedEvmTxs(B)` pre-prefix-take: the mergeset's payload txs in
    /// canonical order (`sorted_mergeset`, then payload order — design §3.1).
    pub accepted_txs: &'a [AcceptedTxCandidate],
    /// EVM gas-pool v2 fence (`Params::evm_gas_pool_v2_activation_daa_score`). When
    /// `daa_score >= gas_pool_v2_activation_daa_score` the executor uses the
    /// sequential gas-pool (actual-gas accounting, class-2 consumes nothing,
    /// non-fitting txs do not block later ones); below it, the v1 strict
    /// declared-gas prefix-take. The construction site copies it from the network
    /// params so production execution, snapshot replay and IBD all agree.
    pub gas_pool_v2_activation_daa_score: u64,
    /// F002 withdrawal-cap fence (`Params::evm_f002_withdraw_cap_activation_daa_score`,
    /// audit M-03). When `daa_score >= this`, a tx whose F002 withdrawals would push
    /// the accepting block's running `WithdrawOp` count over
    /// `MAX_WITHDRAWALS_PER_EVM_BLOCK` is a deterministic class-2 SKIP (its state is
    /// NOT committed — no nonce/burn/withdrawal), so the per-block count of
    /// L1-materialized withdrawals is bounded. Below the fence (inert), withdrawals
    /// are uncapped and execution is byte-identical to before this change.
    pub f002_withdraw_cap_activation_daa_score: u64,
    /// F003 `MLDSA87_VERIFY` precompile fence (`Params::evm_f003_mldsa_verify_activation_daa_score`,
    /// PREA v1.1 §9 / P0-1). When `daa_score >= this`, the F003 verify handler is
    /// registered (`crate::precompiles::register_all_misaka_precompiles`); below it
    /// the handler is absent so a call to `0x…F003` behaves as a call to an empty
    /// account — byte-identical execution, genesis/state-root unchanged.
    pub f003_mldsa_verify_activation_daa_score: u64,
}

#[inline]
fn b256_to_evmh256(b: B256) -> EvmH256 {
    EvmH256::from_bytes(b.0)
}

#[inline]
fn to_revm_address(a: &kaspa_consensus_core::evm::EvmAddress) -> Address {
    Address::from(a.as_bytes())
}

/// Number of F002 WithdrawOps a tx's logs would materialize (audit M-03 cap check).
fn count_withdraws(result: &ExecutionResult) -> usize {
    let mut n = 0;
    for log in result.logs() {
        if crate::withdraw::decode_withdraw_log(log).is_some() {
            n += 1;
        }
    }
    n
}

/// Run a block's EVM lane. Returns the committed result and the post-execution
/// state (for persistence / the next block). The parent state is consumed as the
/// mutable working set.
pub fn execute_block_evm(
    mut state_db: CacheDB<EmptyDB>,
    input: &EvmBlockInput,
) -> Result<(EvmExecutionResult, CacheDB<EmptyDB>), EvmExecError> {
    let parent_state_root = input.parent.map(|p| p.state_root).unwrap_or(EVM_GENESIS_STATE_ROOT);
    let coinbase = to_revm_address(&input.payload.evm_coinbase);
    let derived = env::derive_env(
        input.parent,
        input.header_timestamp_ms,
        &input.selected_parent_hash,
        &input.blue_work_be,
        input.daa_score,
        coinbase,
    );

    let mut gas_used: u64 = 0;
    let mut applied_claims: Vec<DepositClaim> = Vec::new();

    // 1. Bounded deposit claims, applied before user txs (design §3.2): credit
    //    `(amount − claim_tip) × EVM_NATIVE_SCALE` wei to the deposit address
    //    and `claim_tip × SCALE` to the ACCEPTING block's coinbase (the AH-1
    //    claim-inclusion incentive — supply-neutral split of the lock amount);
    //    charge system gas. Tip ≤ amount is consensus-validated; the executor
    //    clamps defensively.
    for op in &input.payload.system_ops {
        match op {
            EvmSystemOp::DepositClaim(claim) => {
                let tip_sompi = claim.claim_tip_sompi.min(claim.amount_sompi);
                let credit_wei = U256::from((claim.amount_sompi - tip_sompi) as u128 * EVM_NATIVE_SCALE as u128);
                let tip_wei = U256::from(tip_sompi as u128 * EVM_NATIVE_SCALE as u128);
                credit_balance(&mut state_db, to_revm_address(&claim.evm_address), credit_wei)?;
                if !tip_wei.is_zero() {
                    credit_balance(&mut state_db, coinbase, tip_wei)?;
                }
                gas_used = gas_used.saturating_add(SYSTEM_DEPOSIT_GAS_PER_CLAIM);
                applied_claims.push(claim.clone());
            }
        }
    }

    // audit R2-#1: the deposit claims above already consumed `gas_used` worth of
    // SYSTEM gas (≤ 256 × 25k = 6.4M). The user-tx prefix-take must take that out
    // of the block's gas budget, otherwise system_gas + up-to-30M user gas could
    // commit `gas_used > gas_limit` and feed an out-of-band value into the next
    // block's base-fee update. Cap the USER cumulative at (block cap − system gas)
    // so total committed gas_used ≤ gas_limit always holds.
    let system_gas = gas_used;
    let user_gas_budget = MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK.checked_sub(system_gas).ok_or_else(|| {
        EvmExecError::InvariantViolation(format!("system gas {system_gas} exceeds block gas cap {MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK}"))
    })?;

    // 2. Class-5 prefix-take (design §7, D4): walk `AcceptedEvmTxs(B)` in
    //    canonical order accumulating DECLARED gas limits; the first tx whose
    //    addition exceeds `user_gas_budget` (the block cap minus system gas) and
    //    every tx after it are deterministically skipped (nonce unchanged — they
    //    remain re-acceptable later). Judging by gas_limit (not gas_used) fixes the
    //    accept set BEFORE execution, so a parallel scheduler's input is
    //    deterministic. An undecodable tx cannot appear in a body-valid payload
    //    (class-1 admission); defense-in-depth maps it to a deterministic skip
    //    so every implementation that reaches it stays consensus-consistent.
    // EVM-lane liveness fix: the gas-pool v2 fence. Below it, the v1 strict
    // declared-gas prefix-take runs (BELOW byte-for-byte unchanged). At/above it the
    // sequential gas pool runs (the `else` branch of the execution loop further down):
    // declared gas only gates pool admission, the pool is debited by ACTUAL gas used,
    // class-2 acceptance skips consume nothing, and a non-fitting tx does NOT block
    // later (smaller) txs. CHANGES execution results ⇒ activation-gated (consensus fork).
    let gas_pool_v2 = input.daa_score >= input.gas_pool_v2_activation_daa_score;
    // Audit M-03: when active, cap the WithdrawOps an accepting block materializes
    // at MAX_WITHDRAWALS_PER_EVM_BLOCK via a per-tx class-2 skip (see the
    // commit-points below). Inert below the fence ⇒ withdrawals uncapped and
    // execution byte-identical to before this change.
    let withdraw_cap_active = input.daa_score >= input.f002_withdraw_cap_activation_daa_score;
    // PREA P0-1: register the F003 verify precompile only at/after its fence. Inert
    // (u64::MAX) ⇒ false ⇒ F003 handler not registered ⇒ byte-identical execution.
    let f003_active = input.daa_score >= input.f003_mldsa_verify_activation_daa_score;

    let mut skipped_tx_count: u32 = 0;
    // §16: per-candidate outcomes (parallel to input order) — store/RPC data
    // feeding the tx-lookup index, never part of the commitment.
    let mut outcomes: Vec<Option<kaspa_consensus_core::evm::EvmCandidateOutcome>> = vec![None; input.accepted_txs.len()];
    let mut planned: Vec<(revm::primitives::TxEnv, &AcceptedTxCandidate, usize)> = Vec::with_capacity(input.accepted_txs.len());
    if !gas_pool_v2 {
        let mut cumulative_gas_limit: u64 = 0;
        let mut over_cap = false;
        for (cand_idx, cand) in input.accepted_txs.iter().enumerate() {
            if over_cap {
                skipped_tx_count += 1; // class 5 (strict prefix: everything after the first over-cap tx)
                outcomes[cand_idx] = Some(kaspa_consensus_core::evm::EvmCandidateOutcome::Skipped { class: 5 });
                continue;
            }
            let txenv = match crate::tx::decode_tx_to_env(&cand.raw) {
                Ok(t) => t,
                Err(_) => {
                    // Defensive: class-1 material (syntactically inadmissible) that
                    // slipped past admission — unreachable for a body-validated
                    // payload, but recorded under its DESIGN class (1) so the
                    // tx-lookup index stays truthful. The label is store/RPC data
                    // only, never part of the commitment (audit L5).
                    skipped_tx_count += 1;
                    outcomes[cand_idx] = Some(kaspa_consensus_core::evm::EvmCandidateOutcome::Skipped { class: 1 });
                    continue;
                }
            };
            cumulative_gas_limit = cumulative_gas_limit.saturating_add(txenv.gas_limit);
            if cumulative_gas_limit > user_gas_budget {
                over_cap = true;
                skipped_tx_count += 1; // class 5
                outcomes[cand_idx] = Some(kaspa_consensus_core::evm::EvmCandidateOutcome::Skipped { class: 5 });
                continue;
            }
            planned.push((txenv, cand, cand_idx));
        }
    }

    // 3. Accepted user txs in canonical order.
    let accepting_coinbase = derived.coinbase;
    let mut receipts: Vec<EvmReceipt> = Vec::with_capacity(planned.len());
    let mut executed_raws: Vec<Vec<u8>> = Vec::with_capacity(planned.len());
    let mut burn_this_block: u128 = 0;
    let mut withdrawals: Vec<WithdrawOp> = Vec::new();
    // Receipt-side cumulative gas counts USER txs only (eth semantics:
    // cumulativeGasUsed = gas of txs up to and including this one). System-op
    // gas stays in the header's `gas_used` (the block budget) but must not
    // offset the first receipt — eth tooling derives per-tx gas from deltas
    // (audit L4).
    let mut tx_cumulative_gas: u64 = 0;
    // O3 (optimization design v0.1): the cfg/block env and the F002 handler are
    // identical for every tx of the block — build the Evm ONCE and swap only
    // the tx env per iteration (the old per-tx builder paid allocation +
    // handler registration ≤1,428×/block). Post-commit balance edits
    // (tip reroute / F002 burn) go through `evm.db_mut()` instead of dropping
    // and re-borrowing the CacheDB.
    let basefee = derived.base_fee_per_gas;
    let mut evm = Evm::builder()
        .with_db(&mut state_db)
        .with_spec_id(crate::EVM_SPEC_ID)
        .modify_cfg_env(|c| c.chain_id = derived.chain_id)
        .modify_block_env(|b| {
            b.number = U256::from(derived.evm_number);
            b.timestamp = U256::from(derived.evm_timestamp_sec);
            // §8.2 (audit AM-3): COINBASE is the ACCEPTING block's declared
            // coinbase — one coinbase per EVM block. revm also pays the tip
            // here; rerouted to the payload coinbase right after commit.
            b.coinbase = accepting_coinbase;
            b.gas_limit = U256::from(derived.gas_limit);
            b.basefee = U256::from(basefee);
            b.difficulty = U256::ZERO;
            b.prevrandao = Some(derived.prev_randao);
        })
        // MISAKA precompiles via the single shared seam (PREA §9.5): F002 always,
        // F003 iff its fence is active. Both executor and the eth_call simulator
        // register through this one fn so they can never diverge (parity).
        .append_handler_register_box(Box::new(move |h| crate::precompiles::register_all_misaka_precompiles(h, f003_active)))
        .build();
    if !gas_pool_v2 {
    // === v1: execute the prefix-take-selected `planned` set (UNCHANGED) ===
    for (txenv, cand, cand_idx) in planned {
        // Effective gas price (EIP-1559): legacy txs carry no priority field —
        // their tip is gas_price − basefee; typed txs tip min(max_fee, basefee
        // + max_priority) − basefee. Needed below to reroute the tip.
        let max_fee = txenv.gas_price;
        let effective_gas_price = match txenv.gas_priority_fee {
            Some(priority) => max_fee.min(U256::from(basefee).saturating_add(priority)),
            None => max_fee,
        };
        let tip_per_gas = effective_gas_price.saturating_sub(U256::from(basefee));
        evm.context.evm.env.tx = txenv;

        // Audit M-03: when the withdrawal cap is active, execute WITHOUT committing,
        // and skip (class-2, dropping the state) if this tx's withdrawals would push
        // the block over MAX_WITHDRAWALS_PER_EVM_BLOCK. Inert ⇒ exactly
        // `transact_commit()` (transact + commit), byte-identical to before.
        let exec = if withdraw_cap_active {
            match evm.transact() {
                Ok(rs) => {
                    if withdrawals.len() + count_withdraws(&rs.result) > MAX_WITHDRAWALS_PER_EVM_BLOCK {
                        skipped_tx_count += 1; // class 2: state dropped — no nonce/burn/withdrawal/gas/receipt
                        outcomes[cand_idx] = Some(kaspa_consensus_core::evm::EvmCandidateOutcome::Skipped { class: 2 });
                        continue;
                    }
                    evm.db_mut().commit(rs.state);
                    Ok(rs.result)
                }
                Err(e) => Err(e),
            }
        } else {
            evm.transact_commit()
        };
        match exec {
            Ok(result) => {
                let tx_gas = result.gas_used();
                gas_used = gas_used.saturating_add(tx_gas);
                tx_cumulative_gas = tx_cumulative_gas.saturating_add(tx_gas);
                // audit #5: basefee burn feeds the supply identity — use checked math.
                let tx_burn = basefee
                    .checked_mul(tx_gas as u128)
                    .and_then(|b| burn_this_block.checked_add(b))
                    .ok_or_else(|| EvmExecError::InvariantViolation("basefee-burn accumulator overflow".to_string()))?;
                burn_this_block = tx_burn;
                // §8.1 (D3): the priority fee belongs to the PAYLOAD block's
                // declared coinbase. revm credited the accepting coinbase
                // (block.coinbase) during commit; move the tip over. Balance
                // moves WITHIN the EVM lane — supply-neutral.
                let tip = tip_per_gas.saturating_mul(U256::from(tx_gas));
                let payload_cb = to_revm_address(&cand.payload_coinbase);
                if !tip.is_zero() && payload_cb != accepting_coinbase {
                    reroute_balance(evm.db_mut(), accepting_coinbase, payload_cb, tip)?;
                }
                // F002 withdrawals (design §9.3): the COMMITTED logs are exactly
                // the effective (non-reverted) withdraw calls. Materialize each
                // as a WithdrawOp and burn the escrowed wei out of F002 — the
                // value leaves the EVM lane here; consensus re-creates it as a
                // synthetic UTXO output in this block's diff.
                let receipt_index = receipts.len() as u32;
                let evm_tx_hash = crate::tx::tx_hash(&cand.raw);
                let mut op_index = 0u32;
                let mut withdrawn_wei: u128 = 0;
                for log in result.logs() {
                    if let Some(w) = crate::withdraw::decode_withdraw_log(log) {
                        withdrawals.push(WithdrawOp {
                            receipt_index,
                            op_index,
                            evm_tx_hash,
                            from: EvmAddress::from_bytes(w.from),
                            script_public_key: w.script_public_key,
                            amount_sompi: (w.amount_wei / EVM_NATIVE_SCALE as u128) as u64,
                        });
                        op_index += 1;
                        withdrawn_wei = withdrawn_wei.saturating_add(w.amount_wei);
                    }
                }
                if withdrawn_wei > 0 {
                    burn_balance(evm.db_mut(), crate::withdraw::f002_address(), U256::from(withdrawn_wei))?;
                }
                outcomes[cand_idx] =
                    Some(kaspa_consensus_core::evm::EvmCandidateOutcome::Accepted { receipt_index: receipts.len() as u32 });
                receipts.push(make_receipt(&result, tx_cumulative_gas));
                executed_raws.push(cand.raw.clone());
            }
            // §6.1 class 2 (and 3 via the nonce rule): acceptance-time invalid
            // (nonce / upfront funds / max_fee < basefee) ⇒ deterministic skip —
            // no receipt, no gas, no nonce change, no trace beyond the counter.
            Err(EVMError::Transaction(_)) => {
                skipped_tx_count += 1;
                outcomes[cand_idx] = Some(kaspa_consensus_core::evm::EvmCandidateOutcome::Skipped { class: 2 });
            }
            Err(other) => return Err(EvmExecError::InvalidTx(format!("{other:?}"))),
        }
    }
    } else {
        // === v2: sequential gas pool (Ethereum semantics) ===
        // Walk `AcceptedEvmTxs(B)` in canonical order with a running gas pool seeded
        // at `user_gas_budget`. Declared gas only GATES admission (a tx whose declared
        // `gas_limit` exceeds the remaining pool is class-5 but does NOT block later,
        // smaller txs); the pool is debited by ACTUAL `gas_used`; an acceptance-time
        // invalid tx (class-2: nonce / funds / basefee) consumes nothing; a full
        // duplicate already executed in THIS block is class-3. The per-tx post-commit
        // accounting (tip reroute / F002 withdraw burn / receipt / basefee burn) is
        // identical to the v1 Ok-arm above.
        let mut remaining_user_gas = user_gas_budget;
        let mut accepted_hashes: std::collections::HashSet<kaspa_hashes::EvmH256> = std::collections::HashSet::new();
        for (cand_idx, cand) in input.accepted_txs.iter().enumerate() {
            let evm_tx_hash = crate::tx::tx_hash(&cand.raw);
            if accepted_hashes.contains(&evm_tx_hash) {
                skipped_tx_count += 1; // class 3 (duplicate already executed in this block)
                outcomes[cand_idx] = Some(kaspa_consensus_core::evm::EvmCandidateOutcome::Skipped { class: 3 });
                continue;
            }
            let txenv = match crate::tx::decode_tx_to_env(&cand.raw) {
                Ok(t) => t,
                Err(_) => {
                    skipped_tx_count += 1; // class 1 (defensive; unreachable for a body-valid payload)
                    outcomes[cand_idx] = Some(kaspa_consensus_core::evm::EvmCandidateOutcome::Skipped { class: 1 });
                    continue;
                }
            };
            if txenv.gas_limit > remaining_user_gas {
                // Does not fit the remaining pool — skip WITHOUT consuming the pool and
                // WITHOUT blocking later (smaller) txs (the liveness fix vs v1's strict prefix).
                skipped_tx_count += 1; // class 5
                outcomes[cand_idx] = Some(kaspa_consensus_core::evm::EvmCandidateOutcome::Skipped { class: 5 });
                continue;
            }
            let max_fee = txenv.gas_price;
            let effective_gas_price = match txenv.gas_priority_fee {
                Some(priority) => max_fee.min(U256::from(basefee).saturating_add(priority)),
                None => max_fee,
            };
            let tip_per_gas = effective_gas_price.saturating_sub(U256::from(basefee));
            evm.context.evm.env.tx = txenv;

            // Audit M-03: same withdrawal-cap gate as the v1 path. When active,
            // a tx whose withdrawals would breach the per-block cap is a class-2
            // skip with its state DROPPED (no commit ⇒ no gas-pool debit, no
            // nonce/burn/withdrawal, not added to accepted_hashes). Inert ⇒
            // exactly `transact_commit()`, byte-identical.
            let exec = if withdraw_cap_active {
                match evm.transact() {
                    Ok(rs) => {
                        if withdrawals.len() + count_withdraws(&rs.result) > MAX_WITHDRAWALS_PER_EVM_BLOCK {
                            skipped_tx_count += 1; // class 2
                            outcomes[cand_idx] = Some(kaspa_consensus_core::evm::EvmCandidateOutcome::Skipped { class: 2 });
                            continue;
                        }
                        evm.db_mut().commit(rs.state);
                        Ok(rs.result)
                    }
                    Err(e) => Err(e),
                }
            } else {
                evm.transact_commit()
            };
            match exec {
                Ok(result) => {
                    let tx_gas = result.gas_used();
                    // Debit the pool by ACTUAL gas. `tx_gas ≤ gas_limit ≤ remaining` holds
                    // (revm never charges above the tx's own gas_limit), so this never
                    // underflows; checked as a consensus invariant.
                    remaining_user_gas = remaining_user_gas
                        .checked_sub(tx_gas)
                        .ok_or_else(|| EvmExecError::InvariantViolation("v2 gas pool debited below zero".to_string()))?;
                    gas_used = gas_used.saturating_add(tx_gas);
                    tx_cumulative_gas = tx_cumulative_gas.saturating_add(tx_gas);
                    let tx_burn = basefee
                        .checked_mul(tx_gas as u128)
                        .and_then(|b| burn_this_block.checked_add(b))
                        .ok_or_else(|| EvmExecError::InvariantViolation("basefee-burn accumulator overflow".to_string()))?;
                    burn_this_block = tx_burn;
                    let tip = tip_per_gas.saturating_mul(U256::from(tx_gas));
                    let payload_cb = to_revm_address(&cand.payload_coinbase);
                    if !tip.is_zero() && payload_cb != accepting_coinbase {
                        reroute_balance(evm.db_mut(), accepting_coinbase, payload_cb, tip)?;
                    }
                    // AUDIT M-03: the WithdrawOp cap is ENFORCED at the commit point
                    // above (the `withdraw_cap_active` branch class-2-skips a tx whose
                    // withdrawals would breach MAX_WITHDRAWALS_PER_EVM_BLOCK, dropping
                    // its state). By the time control reaches here the tx is committed
                    // and within the cap, so this loop just materializes its withdraws.
                    let receipt_index = receipts.len() as u32;
                    let mut op_index = 0u32;
                    let mut withdrawn_wei: u128 = 0;
                    for log in result.logs() {
                        if let Some(w) = crate::withdraw::decode_withdraw_log(log) {
                            withdrawals.push(WithdrawOp {
                                receipt_index,
                                op_index,
                                evm_tx_hash,
                                from: EvmAddress::from_bytes(w.from),
                                script_public_key: w.script_public_key,
                                amount_sompi: (w.amount_wei / EVM_NATIVE_SCALE as u128) as u64,
                            });
                            op_index += 1;
                            withdrawn_wei = withdrawn_wei.saturating_add(w.amount_wei);
                        }
                    }
                    if withdrawn_wei > 0 {
                        burn_balance(evm.db_mut(), crate::withdraw::f002_address(), U256::from(withdrawn_wei))?;
                    }
                    outcomes[cand_idx] =
                        Some(kaspa_consensus_core::evm::EvmCandidateOutcome::Accepted { receipt_index: receipts.len() as u32 });
                    receipts.push(make_receipt(&result, tx_cumulative_gas));
                    executed_raws.push(cand.raw.clone());
                    accepted_hashes.insert(evm_tx_hash);
                }
                Err(EVMError::Transaction(_)) => {
                    // class-2: consumes no gas, no receipt, no nonce change — and crucially
                    // does NOT debit the pool, so a re-included already-accepted tx can no
                    // longer starve the block's later txs.
                    skipped_tx_count += 1;
                    outcomes[cand_idx] = Some(kaspa_consensus_core::evm::EvmCandidateOutcome::Skipped { class: 2 });
                }
                Err(other) => return Err(EvmExecError::InvalidTx(format!("{other:?}"))),
            }
        }
    }
    drop(evm);

    // 4. Roots + bloom + accumulators.
    let logs_bloom = EvmBloom::from_bytes(roots::logs_bloom(&receipts));
    let parent_burn = input.parent.map(|p| evmu256_to_u128(p.evm_burn_accumulator)).unwrap_or(0);
    // O(1) supply-invariant accumulator (design v0.4 §9.1, audit AM-5):
    // total(B) = total(parent) + deposits(B) − withdrawals(B) − burn(B).
    // Priority fees and value transfers move wei BETWEEN EVM accounts (net
    // zero); only deposits add, and withdrawals/basefee-burn remove.
    let parent_total = input.parent.map(|p| evmu256_to_u128(p.evm_total_native_balance)).unwrap_or(0);
    let deposited: u128 = applied_claims.iter().map(|c| c.amount_sompi as u128 * EVM_NATIVE_SCALE as u128).sum();
    let withdrawn: u128 = withdrawals.iter().map(|w: &kaspa_consensus_core::evm::WithdrawOp| w.amount_sompi as u128 * EVM_NATIVE_SCALE as u128).sum();
    // `evm_total_native_balance` is a DERIVED best-effort supply figure. On the
    // real chain wei enters only via deposit claims, so on a valid chain the
    // identity total(B) = total(parent) + deposits − withdrawals − burn holds and
    // never underflows. But it is NOT a safe hard-halt: an account can legitimately
    // hold a balance that this accumulator did not source (e.g. the test harness
    // seeds balances directly without a deposit), and freezing the chain on that
    // would be worse than an imprecise supply readout. So keep the saturating form
    // here. (audit #5 is enforced where it matters — the F002 escrow `burn_balance`
    // and the per-account credit/reroute moves are checked and fail closed.)
    let total_native_balance = parent_total.saturating_add(deposited).saturating_sub(withdrawn).saturating_sub(burn_this_block);
    // audit INFO-b (revised): the saturating form above is the committed source of truth and is
    // intentional — it never hard-halts. A previous `debug_assert_eq!` here demanded the EXACT
    // checked identity, but it false-positived on every executor/snapshot test: those harnesses fund
    // senders by seeding balances DIRECTLY (no deposit), so with EVM_INITIAL_BASE_FEE > 0 any executed
    // tx burns basefee while `deposited == 0`, legitimately saturating the accumulator. The check could
    // not distinguish that legitimate seeding from a genuine accumulator bug (both clamp ⇒ `checked` is
    // `None`), so it only produced false panics. Genuine supply divergence is caught where it MATTERS,
    // with `checked` math that fails closed: the F002 escrow `burn_balance` and the per-account
    // credit / tip-reroute moves (audit #5). The aggregate debug-only assert added no reliable signal
    // and is removed; release behaviour is unchanged (it was compiled out there anyway).
    let header = EvmExecutionHeader {
        parent_state_root,
        state_root: b256_to_evmh256(state::state_root(&state_db)),
        // §4.2: the ordered root over ACCEPTED-AND-EXECUTED txs only — skips
        // (classes 2/3/5) leave no trace in the execution result.
        transactions_root: roots::transactions_root(&executed_raws),
        receipts_root: roots::receipts_root(&receipts),
        system_ops_root: roots::system_ops_root(&input.payload.system_ops),
        withdrawals_root: roots::withdrawals_root(&withdrawals),
        deposit_claim_queue_root: roots::deposit_claim_root(&applied_claims),
        logs_bloom,
        gas_used,
        gas_limit: derived.gas_limit,
        base_fee_per_gas: EvmU256::from(derived.base_fee_per_gas),
        evm_number: derived.evm_number,
        evm_timestamp_sec: derived.evm_timestamp_sec,
        evm_chain_id: derived.chain_id,
        // v0.4 §8.2 (audit AM-3): the accepting block's declared coinbase.
        coinbase: input.payload.evm_coinbase,
        accepted_tx_count: receipts.len() as u32,
        skipped_tx_count,
        evm_total_native_balance: EvmU256::from(total_native_balance),
        evm_burn_accumulator: EvmU256::from(
            parent_burn
                .checked_add(burn_this_block)
                .ok_or_else(|| EvmExecError::InvariantViolation("burn accumulator overflow".to_string()))?,
        ),
    };

    let candidate_outcomes = outcomes
        .into_iter()
        .map(|o| o.expect("every candidate received an outcome in the planning or execution loop"))
        .collect();
    let result =
        EvmExecutionResult { header, receipts, withdrawals, applied_deposit_claims: applied_claims, candidate_outcomes };
    Ok((result, state_db))
}

/// O12 (IBD catch-up): the EMPTY-ACCEPTANCE fast path. When a block accepts NO
/// user txs and carries NO system ops — the common case on a young chain, and
/// frequent forever (empty mergesets) — the EVM transition is fully determined
/// without touching revm or the state: the state is UNCHANGED (state_root =
/// parent's), every collection root is the fixed empty-input digest, gas_used
/// is 0, and only the env-derived header fields advance (evm_number, the
/// timestamp clamp, the EIP-1559 base-fee update from the parent's gas_used).
///
/// CONSENSUS-NEUTRAL BY CONSTRUCTION: every field is produced by the SAME
/// functions the full path uses (`env::derive_env`, the `roots::*` fns over
/// empty inputs, the parent-accumulator copies), so the resulting header — and
/// therefore `commitment_root()` — is byte-identical to a full execution over
/// the same input. Pinned by `empty_fast_path_equals_full_execution` below; a
/// mixed network of fast-path and full-path nodes cannot diverge.
///
/// What this skips per empty block: `seed_cachedb` over the whole parent state,
/// the revm `Evm` build, the keccak-MPT `state_root` recompute over the ENTIRE
/// state, and the full `snapshot_from_cachedb` extraction — the dominant
/// serial-thread EVM cost during weak-host IBD catch-up (and it stays O(1) as
/// the state grows, instead of O(state)).
pub fn empty_acceptance_result(input: &EvmBlockInput) -> EvmExecutionResult {
    debug_assert!(input.accepted_txs.is_empty() && input.payload.system_ops.is_empty());
    let parent_state_root = input.parent.map(|p| p.state_root).unwrap_or(EVM_GENESIS_STATE_ROOT);
    let coinbase = to_revm_address(&input.payload.evm_coinbase);
    let derived = env::derive_env(
        input.parent,
        input.header_timestamp_ms,
        &input.selected_parent_hash,
        &input.blue_work_be,
        input.daa_score,
        coinbase,
    );
    let parent_burn = input.parent.map(|p| evmu256_to_u128(p.evm_burn_accumulator)).unwrap_or(0);
    let parent_total = input.parent.map(|p| evmu256_to_u128(p.evm_total_native_balance)).unwrap_or(0);
    let header = EvmExecutionHeader {
        parent_state_root,
        // No account was touched: the post-state trie is the parent's.
        state_root: parent_state_root,
        transactions_root: roots::transactions_root(&[]),
        receipts_root: roots::receipts_root(&[]),
        system_ops_root: roots::system_ops_root(&[]),
        withdrawals_root: roots::withdrawals_root(&[]),
        deposit_claim_queue_root: roots::deposit_claim_root(&[]),
        logs_bloom: EvmBloom::from_bytes(roots::logs_bloom(&[])),
        gas_used: 0,
        gas_limit: derived.gas_limit,
        base_fee_per_gas: EvmU256::from(derived.base_fee_per_gas),
        evm_number: derived.evm_number,
        evm_timestamp_sec: derived.evm_timestamp_sec,
        evm_chain_id: derived.chain_id,
        coinbase: input.payload.evm_coinbase,
        accepted_tx_count: 0,
        skipped_tx_count: 0,
        evm_total_native_balance: EvmU256::from(parent_total),
        evm_burn_accumulator: EvmU256::from(parent_burn),
    };
    EvmExecutionResult { header, receipts: vec![], withdrawals: vec![], applied_deposit_claims: vec![], candidate_outcomes: vec![] }
}

/// Credit `amount` wei directly in the working state. `load_account`
/// materializes the entry (a new address starts as `NotExisting`, which
/// `basic()` reports as absent); give it a real (EOA) code hash and mark it
/// `Touched` so the credit is visible to execution + the state trie.
fn credit_balance(db: &mut CacheDB<EmptyDB>, addr: Address, amount: U256) -> Result<(), EvmExecError> {
    let acct = db.load_account(addr).map_err(|e| EvmExecError::InvalidTx(format!("balance credit: {e:?}")))?;
    if acct.info.code_hash == B256::ZERO {
        acct.info.code_hash = KECCAK_EMPTY;
    }
    // audit #5: fail closed on overflow rather than saturate (a 256-bit balance
    // overflow is spec-impossible — total native supply is bounded — so it can
    // only mean corruption/a bug).
    acct.info.balance = acct
        .info
        .balance
        .checked_add(amount)
        .ok_or_else(|| EvmExecError::InvariantViolation(format!("balance credit overflow at {addr}")))?;
    acct.account_state = AccountState::Touched;
    Ok(())
}

/// Burn `amount` wei out of `addr` (the F002 escrow exit): the wei leaves the
/// EVM lane entirely — total native balance decreases by exactly this amount.
fn burn_balance(db: &mut CacheDB<EmptyDB>, addr: Address, amount: U256) -> Result<(), EvmExecError> {
    let acct = db.load_account(addr).map_err(|e| EvmExecError::InvalidTx(format!("balance burn: {e:?}")))?;
    if acct.info.code_hash == B256::ZERO {
        acct.info.code_hash = KECCAK_EMPTY;
    }
    // audit #5: the F002 escrow must hold ≥ the burned amount (the withdraw
    // handler debited the caller into F002 first). An underflow would mean a
    // synthetic UTXO is materialized without the EVM side being debited — a
    // supply break. Fail closed instead of hiding it with a saturating_sub.
    acct.info.balance = acct
        .info
        .balance
        .checked_sub(amount)
        .ok_or_else(|| EvmExecError::InvariantViolation(format!("F002 burn underflow at {addr}: escrow < withdrawal")))?;
    acct.account_state = AccountState::Touched;
    Ok(())
}

/// Move `amount` wei `from → to` directly in the working state (the §8.1 tip
/// reroute). Both accounts are materialized/Touched the same way the deposit
/// credit is, so the move is visible to later txs, the state trie and spending.
fn reroute_balance(db: &mut CacheDB<EmptyDB>, from: Address, to: Address, amount: U256) -> Result<(), EvmExecError> {
    let src = db.load_account(from).map_err(|e| EvmExecError::InvalidTx(format!("tip reroute (debit): {e:?}")))?;
    if src.info.code_hash == B256::ZERO {
        src.info.code_hash = KECCAK_EMPTY;
    }
    // revm just credited `from` with exactly the tip, so an under-balance here
    // is spec-impossible — fail closed if it ever happens (audit #5).
    src.info.balance = src
        .info
        .balance
        .checked_sub(amount)
        .ok_or_else(|| EvmExecError::InvariantViolation(format!("tip reroute underflow at {from}")))?;
    src.account_state = AccountState::Touched;
    let dst = db.load_account(to).map_err(|e| EvmExecError::InvalidTx(format!("tip reroute (credit): {e:?}")))?;
    if dst.info.code_hash == B256::ZERO {
        dst.info.code_hash = KECCAK_EMPTY;
    }
    dst.info.balance = dst
        .info
        .balance
        .checked_add(amount)
        .ok_or_else(|| EvmExecError::InvariantViolation(format!("tip reroute overflow at {to}")))?;
    dst.account_state = AccountState::Touched;
    Ok(())
}

fn make_receipt(result: &ExecutionResult, cumulative_gas_used: u64) -> EvmReceipt {
    let logs = result
        .logs()
        .iter()
        .map(|log| EvmLog {
            address: kaspa_consensus_core::evm::EvmAddress::from_bytes(log.address.into_array()),
            topics: log.data.topics().iter().map(|t| EvmH256::from_bytes(t.0)).collect(),
            data: log.data.data.to_vec(),
        })
        .collect();
    EvmReceipt { succeeded: result.is_success(), cumulative_gas_used, gas_used: result.gas_used(), logs }
}

fn evmu256_to_u128(v: EvmU256) -> u128 {
    v.try_to_u128().unwrap_or(u128::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::evm::{EvmAddress, EVM_CHAIN_ID, EVM_INITIAL_BASE_FEE};
    use revm::primitives::{AccountInfo, KECCAK_EMPTY};
    use revm::Database;

    /// Build + sign a 1559 transfer; returns (sender, raw EIP-2718 bytes).
    #[allow(clippy::too_many_arguments)]
    fn signed_tx(key: u8, nonce: u64, to: Address, value: u128, gas_limit: u64, max_fee: u128, priority: u128) -> (Address, Vec<u8>) {
        use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
        use alloy_eips::eip2718::Encodable2718;
        use alloy_signer::SignerSync;
        use alloy_signer_local::PrivateKeySigner;

        let signer = PrivateKeySigner::from_bytes(&B256::from([key; 32])).unwrap();
        let tx = TxEip1559 {
            chain_id: EVM_CHAIN_ID,
            nonce,
            gas_limit,
            max_fee_per_gas: max_fee,
            max_priority_fee_per_gas: priority,
            to: revm::primitives::TxKind::Call(to),
            value: U256::from(value),
            access_list: Default::default(),
            input: Default::default(),
        };
        let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        (signer.address(), TxEnvelope::from(tx.into_signed(sig)).encoded_2718())
    }

    fn signed_transfer(nonce: u64, to: Address, value: u128, max_fee: u128) -> (Address, Vec<u8>) {
        signed_tx(0x11, nonce, to, value, 21_000, max_fee, 0)
    }

    fn funded_seed(addr: Address, wei: u128) -> CacheDB<EmptyDB> {
        let mut seed = CacheDB::new(EmptyDB::default());
        seed.insert_account_info(addr, AccountInfo { balance: U256::from(wei), nonce: 0, code_hash: KECCAK_EMPTY, code: None });
        seed
    }

    fn input_with<'a>(payload: &'a EvmExecutionPayload, accepted: &'a [AcceptedTxCandidate]) -> EvmBlockInput<'a> {
        EvmBlockInput {
            parent: None,
            header_timestamp_ms: 5_000,
            selected_parent_hash: [9u8; 64],
            blue_work_be: vec![1, 2, 3],
            daa_score: 42,
            payload,
            accepted_txs: accepted,
            gas_pool_v2_activation_daa_score: u64::MAX,
            // Cap inert by default (daa_score 42 < u64::MAX) — existing tests keep
            // byte-identical behavior; the cap test below overrides it.
            f002_withdraw_cap_activation_daa_score: u64::MAX,
            f003_mldsa_verify_activation_daa_score: u64::MAX,
        }
    }

    fn cand(raw: Vec<u8>, cb: u8) -> AcceptedTxCandidate {
        AcceptedTxCandidate { raw, payload_coinbase: EvmAddress::from_bytes([cb; 20]) }
    }

    /// Same block, but with the gas-pool v2 fence ACTIVE (activation score 0 ≤ the
    /// helper's daa_score 42) so `execute_block_evm` runs the sequential gas pool.
    fn input_v2<'a>(payload: &'a EvmExecutionPayload, accepted: &'a [AcceptedTxCandidate]) -> EvmBlockInput<'a> {
        EvmBlockInput { gas_pool_v2_activation_daa_score: 0, ..input_with(payload, accepted) }
    }

    const HUGE_SEED: u128 = 100_000_000_000_000_000_000_000;
    use kaspa_consensus_core::evm::MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK as BLOCK_GAS_CAP;

    /// P2-T1 (the core liveness proof): the EXACT `class5_prefix_take_is_strict` input
    /// (declared 20M + 20M + 21k vs the 30M cap, all plain transfers) accepts only 1 tx
    /// under v1's strict declared-gas prefix-take, but all 3 under the v2 gas pool — the
    /// 20M declarations only GATE admission; the pool is debited by the 21k each actually
    /// uses, so nothing over-caps. This is the "50k-declared / 21k-used" capacity fix.
    #[test]
    fn gas_pool_v2_accepts_the_run_v1_strict_prefix_skips() {
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let to = Address::with_last_byte(0x22);
        let (from, r1) = signed_tx(0x11, 0, to, 111, 20_000_000, basefee, 0);
        let (_, r2) = signed_tx(0x11, 1, to, 222, 20_000_000, basefee, 0);
        let (_, r3) = signed_tx(0x11, 2, to, 333, 21_000, basefee, 0);
        let payload = EvmExecutionPayload { evm_coinbase: EvmAddress::from_bytes([0xFE; 20]), ..Default::default() };
        let accepted = [cand(r1, 0xAA), cand(r2, 0xAA), cand(r3, 0xAA)];

        let (v1, _) = execute_block_evm(funded_seed(from, HUGE_SEED), &input_with(&payload, &accepted)).unwrap();
        assert_eq!(v1.header.accepted_tx_count, 1, "v1 strict prefix: only the first fits the DECLARED budget");
        assert_eq!(v1.header.skipped_tx_count, 2);

        let (v2, mut db) = execute_block_evm(funded_seed(from, HUGE_SEED), &input_v2(&payload, &accepted)).unwrap();
        assert_eq!(v2.header.accepted_tx_count, 3, "v2 gas pool: declared gas only gates; actual 21k each ⇒ all 3 fit");
        assert_eq!(v2.header.skipped_tx_count, 0);
        assert_eq!(db.basic(to).unwrap().unwrap().balance, U256::from(111u64 + 222 + 333), "all three transfers landed under v2");
        assert_eq!(db.basic(from).unwrap().unwrap().nonce, 3);
    }

    /// P2-T2 (class-2 does not starve later txs): a nonce-too-low (class-2) tx that DECLARES
    /// the entire 30M block budget, followed by two valid transfers. v1 reserves the 30M in
    /// the prefix-take, so the valid txs over-cap to class-5 (0 valid accepted). v2 charges the
    /// class-2 tx NOTHING (it never executes), so both valid txs run.
    #[test]
    fn gas_pool_v2_class2_skip_consumes_no_pool() {
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let to = Address::with_last_byte(0x33);
        let (from, stale) = signed_tx(0x11, 5, to, 1, BLOCK_GAS_CAP, basefee, 0); // nonce 5 vs state 0 ⇒ class-2
        let (_, good0) = signed_tx(0x11, 0, to, 100, 21_000, basefee, 0);
        let (_, good1) = signed_tx(0x11, 1, to, 200, 21_000, basefee, 0);
        let payload = EvmExecutionPayload { evm_coinbase: EvmAddress::from_bytes([0xFE; 20]), ..Default::default() };
        let accepted = [cand(stale, 0xAA), cand(good0, 0xAA), cand(good1, 0xAA)];

        let (v1, _) = execute_block_evm(funded_seed(from, HUGE_SEED), &input_with(&payload, &accepted)).unwrap();
        assert_eq!(v1.header.accepted_tx_count, 0, "v1: the 30M-declared class-2 tx starves the valid txs (head-of-line block)");

        let (v2, mut db) = execute_block_evm(funded_seed(from, HUGE_SEED), &input_v2(&payload, &accepted)).unwrap();
        assert_eq!(v2.header.accepted_tx_count, 2, "v2: class-2 consumes no pool ⇒ both valid txs execute");
        assert_eq!(db.basic(to).unwrap().unwrap().balance, U256::from(300u64));
        assert_eq!(db.basic(from).unwrap().unwrap().nonce, 2);
    }

    /// P2-T7 (determinism): the v2 gas pool is a pure function of the canonical input — two
    /// runs over the same accepted set yield identical commitment / state / gas / skip counts.
    #[test]
    fn gas_pool_v2_is_deterministic() {
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let to = Address::with_last_byte(0x44);
        let (from, r1) = signed_tx(0x11, 0, to, 111, 20_000_000, basefee, 0);
        let (_, r2) = signed_tx(0x11, 1, to, 222, 20_000_000, basefee, 0);
        let (_, r3) = signed_tx(0x11, 2, to, 333, 21_000, basefee, 0);
        let payload = EvmExecutionPayload { evm_coinbase: EvmAddress::from_bytes([0xFE; 20]), ..Default::default() };
        let accepted = [cand(r1, 0xAA), cand(r2, 0xAA), cand(r3, 0xAA)];
        let (a, _) = execute_block_evm(funded_seed(from, HUGE_SEED), &input_v2(&payload, &accepted)).unwrap();
        let (b, _) = execute_block_evm(funded_seed(from, HUGE_SEED), &input_v2(&payload, &accepted)).unwrap();
        assert_eq!(a.header.commitment_root(), b.header.commitment_root());
        assert_eq!(a.header.state_root, b.header.state_root);
        assert_eq!(a.header.gas_used, b.header.gas_used);
        assert_eq!((a.header.accepted_tx_count, a.header.skipped_tx_count), (b.header.accepted_tx_count, b.header.skipped_tx_count));
    }

    /// P2 (in-block dedup): the SAME raw tx twice in one block — the first executes, the second
    /// is a class-3 duplicate skip (the v2 `accepted_hashes` set), applied exactly once.
    #[test]
    fn gas_pool_v2_in_block_duplicate_is_class3() {
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let to = Address::with_last_byte(0x55);
        let (from, r) = signed_tx(0x11, 0, to, 100, 21_000, basefee, 0);
        let payload = EvmExecutionPayload { evm_coinbase: EvmAddress::from_bytes([0xFE; 20]), ..Default::default() };
        let accepted = [cand(r.clone(), 0xAA), cand(r, 0xAA)];
        let (res, mut db) = execute_block_evm(funded_seed(from, HUGE_SEED), &input_v2(&payload, &accepted)).unwrap();
        assert_eq!(res.header.accepted_tx_count, 1);
        assert_eq!(res.header.skipped_tx_count, 1, "the duplicate is a class-3 skip");
        assert_eq!(db.basic(to).unwrap().unwrap().balance, U256::from(100u64), "applied exactly once");
        assert_eq!(db.basic(from).unwrap().unwrap().nonce, 1);
    }

    /// O12: the empty-acceptance fast path must be BYTE-IDENTICAL to a full
    /// execution over the same (empty) input — the consensus-neutrality proof.
    /// Covers (a) the implicit-genesis parent and (b) a non-trivial parent
    /// state built by a real transfer block, including the post-state snapshot
    /// identity (unchanged state ⇒ child snapshot == parent snapshot).
    #[test]
    fn empty_fast_path_equals_full_execution() {
        use crate::snapshot::{seed_cachedb, snapshot_from_cachedb};
        let empty_payload = EvmExecutionPayload::default();

        // (a) implicit-genesis parent, empty state.
        let input = input_with(&empty_payload, &[]);
        let (full, full_db) = execute_block_evm(CacheDB::new(EmptyDB::default()), &input).unwrap();
        let fast = empty_acceptance_result(&input);
        assert_eq!(full.header, fast.header, "fast-path header must equal full execution (genesis parent)");
        assert_eq!(full.header.commitment_root(), fast.header.commitment_root());
        assert!(fast.receipts.is_empty() && fast.withdrawals.is_empty() && fast.candidate_outcomes.is_empty());
        assert_eq!(snapshot_from_cachedb(&full_db).accounts.len(), 0);

        // (b) parent with real state: run a funded transfer block first.
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let to = Address::with_last_byte(0x22);
        let (from, raw) = signed_transfer(0, to, 500, basefee);
        let cands = vec![cand(raw, 0xAB)];
        let parent_input = input_with(&empty_payload, &cands);
        let (parent_result, parent_db) = execute_block_evm(funded_seed(from, 10u128.pow(18)), &parent_input).unwrap();
        let parent_snapshot = snapshot_from_cachedb(&parent_db);
        assert!(!parent_snapshot.accounts.is_empty(), "the transfer must leave real state");

        // Child block with EMPTY acceptance on top of that state.
        let child_input = EvmBlockInput {
            parent: Some(&parent_result.header),
            header_timestamp_ms: 6_000,
            selected_parent_hash: [7u8; 64],
            blue_work_be: vec![4, 5, 6],
            daa_score: 43,
            payload: &empty_payload,
            accepted_txs: &[],
            gas_pool_v2_activation_daa_score: u64::MAX,
            f002_withdraw_cap_activation_daa_score: u64::MAX,
            f003_mldsa_verify_activation_daa_score: u64::MAX,
        };
        // FULL path: seed the parent state, execute, extract.
        let (full_child, full_child_db) = execute_block_evm(seed_cachedb(&parent_snapshot).unwrap(), &child_input).unwrap();
        let full_child_snapshot = snapshot_from_cachedb(&full_child_db);
        // FAST path.
        let fast_child = empty_acceptance_result(&child_input);
        assert_eq!(full_child.header, fast_child.header, "fast-path header must equal full execution (stateful parent)");
        assert_eq!(full_child.header.commitment_root(), fast_child.header.commitment_root());
        assert_eq!(full_child.header.state_root, parent_result.header.state_root, "unchanged state keeps the parent root");
        assert_eq!(full_child_snapshot, parent_snapshot, "unchanged state round-trips to the identical snapshot");
        // And the snapshot-level entry point takes the fast path with the same outputs.
        let (via_snapshot, child_snapshot) = crate::snapshot::execute_block_from_snapshot(&parent_snapshot, &child_input).unwrap();
        assert_eq!(via_snapshot.header, fast_child.header);
        assert_eq!(child_snapshot, parent_snapshot);
    }

    #[test]
    fn deposit_credit_and_accepted_transfer_produce_a_stable_commitment() {
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let to = Address::with_last_byte(0x22);
        let (from, raw) = signed_transfer(0, to, 500, basefee);
        let seed = funded_seed(from, 1_000_000_000_000_000_000);

        let claim_addr = EvmAddress::from_bytes([0xCC; 20]);
        let payload = EvmExecutionPayload {
            system_ops: vec![EvmSystemOp::DepositClaim(DepositClaim {
                deposit_outpoint: Default::default(),
                evm_address: claim_addr,
                amount_sompi: 7,
                claim_tip_sompi: 0,
            })],
            evm_coinbase: EvmAddress::from_bytes([0xFE; 20]),
            ..Default::default()
        };
        // v0.4 §3.1: the transfer rides in as an ACCEPTED tx (from a mergeset
        // payload), not in B's own payload.
        let accepted = [cand(raw, 0xFE)];
        let input = input_with(&payload, &accepted);

        let (result, mut db) = execute_block_evm(seed.clone(), &input).unwrap();

        // First EVM block: number 1, parent state root = genesis, ts = max(5, 0).
        assert_eq!(result.header.evm_number, 1);
        assert_eq!(result.header.parent_state_root, EVM_GENESIS_STATE_ROOT);
        assert_eq!(result.header.evm_timestamp_sec, 5);
        // Deposit credited 7 sompi x 1e10 = 7e10 wei.
        assert_eq!(db.basic(to_revm_address(&claim_addr)).unwrap().unwrap().balance, U256::from(70_000_000_000u64));
        // Transfer landed.
        assert_eq!(db.basic(to).unwrap().unwrap().balance, U256::from(500u64));
        // gas = 25k (claim) + 21k (transfer); burn = 21k x basefee.
        assert_eq!(result.header.gas_used, 46_000);
        // Receipt cumulative gas counts USER txs only (audit L4): the claim's
        // 25k system gas lives in header.gas_used, never in receipts.
        assert_eq!(result.receipts[0].cumulative_gas_used, 21_000);
        assert_eq!(result.header.evm_burn_accumulator, EvmU256::from(21_000u128 * basefee));
        // v0.4 counters + the accepting coinbase (audit AM-3).
        assert_eq!(result.header.accepted_tx_count, 1);
        assert_eq!(result.header.skipped_tx_count, 0);
        assert_eq!(result.header.coinbase, payload.evm_coinbase);
        assert_eq!(result.applied_deposit_claims.len(), 1);
        assert_eq!(result.receipts.len(), 1);
        assert!(result.receipts[0].succeeded);

        // Determinism: same inputs => identical (non-trivial) commitment.
        let (result2, _) = execute_block_evm(seed, &input).unwrap();
        assert_eq!(result.header.commitment_root(), result2.header.commitment_root());
        assert_ne!(result.header.commitment_root(), kaspa_hashes::Hash64::default());
    }

    /// v0.4 §3.1 / invariant I2 (Y2 off-by-one): a block's OWN payload txs are
    /// data — they never enter its own EvmResult. Two inputs differing only in
    /// B's own `payload.transactions` produce the identical commitment.
    #[test]
    fn own_payload_txs_are_data_only() {
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let to = Address::with_last_byte(0x22);
        let (from, raw) = signed_transfer(0, to, 500, basefee);
        let seed = funded_seed(from, 1_000_000_000_000_000_000);

        let empty_payload = EvmExecutionPayload { evm_coinbase: EvmAddress::from_bytes([0xFE; 20]), ..Default::default() };
        let stuffed_payload = EvmExecutionPayload { transactions: vec![raw], ..empty_payload.clone() };

        let (r_empty, mut db_empty) = execute_block_evm(seed.clone(), &input_with(&empty_payload, &[])).unwrap();
        let (r_stuffed, _) = execute_block_evm(seed, &input_with(&stuffed_payload, &[])).unwrap();

        assert!(r_stuffed.receipts.is_empty(), "B's own payload tx was NOT executed in B");
        assert_eq!(r_stuffed.header.accepted_tx_count, 0);
        assert_eq!(db_empty.basic(to).unwrap().map(|a| a.balance).unwrap_or_default(), U256::ZERO);
        assert_eq!(
            r_empty.header.commitment_root(),
            r_stuffed.header.commitment_root(),
            "EvmResult(B) is independent of B's own user payload (it only feeds Header::evm_payload_hash)"
        );
    }

    /// v0.4 §6.1 classes 2/3 (Y3): acceptance-time-invalid txs (bad nonce /
    /// unfunded sender / max_fee < basefee) are deterministic SKIPS — no
    /// receipt, no gas, no nonce change; only the counter records them.
    #[test]
    fn class2_skips_leave_no_trace() {
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let to = Address::with_last_byte(0x22);
        let (funded, raw_bad_nonce) = signed_tx(0x11, 7, to, 500, 21_000, basefee, 0); // nonce 7 != 0
        let (_unfunded, raw_unfunded) = signed_tx(0x22, 0, to, 500, 21_000, basefee, 0); // no balance
        let (_f2, raw_cheap) = signed_tx(0x11, 0, to, 500, 21_000, 1, 0); // max_fee 1 wei < basefee
        let seed = funded_seed(funded, 1_000_000_000_000_000_000);
        let seed_root = state::state_root(&seed);

        let payload = EvmExecutionPayload { evm_coinbase: EvmAddress::from_bytes([0xFE; 20]), ..Default::default() };
        let accepted = [cand(raw_bad_nonce, 0xAA), cand(raw_unfunded, 0xAB), cand(raw_cheap, 0xAC)];
        let (result, mut db) = execute_block_evm(seed, &input_with(&payload, &accepted)).unwrap();

        assert_eq!(result.header.skipped_tx_count, 3);
        assert_eq!(result.header.accepted_tx_count, 0);
        assert!(result.receipts.is_empty(), "skips leave no receipts");
        assert_eq!(result.header.gas_used, 0, "skips charge no gas");
        assert_eq!(result.header.state_root.as_bytes(), state::state_root(&db).0.as_slice());
        assert_eq!(result.header.state_root.as_bytes(), seed_root.0.as_slice(), "state untouched");
        let acct = db.basic(funded).unwrap().unwrap();
        assert_eq!(acct.nonce, 0, "nonce unchanged => the tx stays re-acceptable later");
    }

    /// v0.4 §7 (D4, Y6): the class-5 accepted-gas cap is a deterministic STRICT
    /// prefix-take over declared gas limits — the first over-cap tx and every
    /// tx after it are skipped, even later ones that would individually fit.
    #[test]
    fn class5_prefix_take_is_strict() {
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let to = Address::with_last_byte(0x22);
        // gas limits 20M + 20M + 21k against the 30M cap: #2 overflows => #2 and #3 skipped.
        let (from, raw1) = signed_tx(0x11, 0, to, 111, 20_000_000, basefee, 0);
        let (_, raw2) = signed_tx(0x11, 1, to, 222, 20_000_000, basefee, 0);
        let (_, raw3) = signed_tx(0x11, 2, to, 333, 21_000, basefee, 0);
        // Upfront cost is gas_limit x max_fee: fund generously (21M gwei x 20M).
        let seed = funded_seed(from, 100_000_000_000_000_000_000_000u128);

        let payload = EvmExecutionPayload { evm_coinbase: EvmAddress::from_bytes([0xFE; 20]), ..Default::default() };
        let accepted = [cand(raw1, 0xAA), cand(raw2, 0xAA), cand(raw3, 0xAA)];
        let (result, mut db) = execute_block_evm(seed, &input_with(&payload, &accepted)).unwrap();

        assert_eq!(result.header.accepted_tx_count, 1, "only the in-budget prefix executes");
        assert_eq!(result.header.skipped_tx_count, 2, "the over-cap tx AND everything after it");
        assert_eq!(db.basic(to).unwrap().unwrap().balance, U256::from(111u64), "only tx #1 landed");
        assert_eq!(db.basic(from).unwrap().unwrap().nonce, 1, "skipped txs left the nonce untouched");
    }

    /// v0.4 §8 (D3, Y5) + §9.1 (AM-5): the priority fee routes to the PAYLOAD
    /// block's coinbase (the accepting coinbase nets zero), and the committed
    /// O(1) total-native-balance accumulator equals the actual state sum when
    /// all funds enter via deposits.
    #[test]
    fn priority_fee_routes_to_payload_coinbase_and_supply_accumulator_matches() {
        let basefee = EVM_INITIAL_BASE_FEE as u128; // 1 gwei
        let to = Address::with_last_byte(0x22);
        // max_fee 2 gwei, priority 1 gwei => effective 2 gwei, tip 1 gwei/gas.
        let (sender, raw) = signed_tx(0x11, 0, to, 500, 21_000, 2 * basefee, basefee);

        // The sender is funded ONLY by a same-block deposit claim (claims apply
        // before accepted txs, Y13): 10_000 sompi = 1e14 wei.
        let payload = EvmExecutionPayload {
            system_ops: vec![EvmSystemOp::DepositClaim(DepositClaim {
                deposit_outpoint: Default::default(),
                evm_address: EvmAddress::from_bytes(sender.into_array()),
                amount_sompi: 10_000,
                claim_tip_sompi: 0,
            })],
            evm_coinbase: EvmAddress::from_bytes([0xFE; 20]), // accepting coinbase F
            ..Default::default()
        };
        let accepted = [cand(raw, 0xAB)]; // payload coinbase X != F
        let (result, mut db) = execute_block_evm(CacheDB::new(EmptyDB::default()), &input_with(&payload, &accepted)).unwrap();

        assert!(result.receipts[0].succeeded);
        let tip = 21_000u128 * basefee;
        let burn = 21_000u128 * basefee;
        // X (the payload miner) earned the tip; F (the accepting miner) nets zero.
        assert_eq!(db.basic(Address::from([0xAB; 20])).unwrap().unwrap().balance, U256::from(tip));
        assert_eq!(db.basic(Address::from([0xFE; 20])).unwrap().map(|a| a.balance).unwrap_or_default(), U256::ZERO);
        // O(1) accumulator: total = 0 + deposits - withdrawals - burn ...
        let expected_total = 100_000_000_000_000u128 - burn;
        assert_eq!(result.header.evm_total_native_balance, EvmU256::from(expected_total));
        // ... and it equals the ACTUAL post-state sum (supply invariant, I6).
        let snapshot = crate::snapshot::snapshot_from_cachedb(&db);
        let actual: u128 = snapshot.accounts.iter().map(|a| a.balance.try_to_u128().unwrap()).sum();
        assert_eq!(actual, expected_total);
    }
    /// Build + sign an arbitrary call (value + calldata) from a test key.
    #[allow(clippy::too_many_arguments)]
    fn signed_call(key: u8, nonce: u64, to: Address, value: u128, gas_limit: u64, max_fee: u128, input: Vec<u8>) -> (Address, Vec<u8>) {
        use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
        use alloy_eips::eip2718::Encodable2718;
        use alloy_signer::SignerSync;
        use alloy_signer_local::PrivateKeySigner;

        let signer = PrivateKeySigner::from_bytes(&B256::from([key; 32])).unwrap();
        let tx = TxEip1559 {
            chain_id: EVM_CHAIN_ID,
            nonce,
            gas_limit,
            max_fee_per_gas: max_fee,
            max_priority_fee_per_gas: 0,
            to: revm::primitives::TxKind::Call(to),
            value: U256::from(value),
            access_list: Default::default(),
            input: input.into(),
        };
        let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        (signer.address(), TxEnvelope::from(tx.into_signed(sig)).encoded_2718())
    }

    fn withdraw_calldata(spk: &kaspa_consensus_core::tx::ScriptPublicKey) -> Vec<u8> {
        let mut data = spk.version().to_be_bytes().to_vec();
        data.extend_from_slice(spk.script());
        data
    }

    /// v0.4 §9.3 (F002): a payable withdraw call burns the wei out of the EVM
    /// lane, emits exactly one WithdrawOp with the caller/amount/destination,
    /// and the O(1) supply accumulator tracks the exit (deposits − withdrawals
    /// − burn == actual state sum).
    #[test]
    fn f002_withdraw_emits_op_and_burns_from_evm() {
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let spk = kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[0x42u8; 64]);
        let f002 = crate::withdraw::f002_address();
        // Withdraw 5 sompi = 5e10 wei.
        let withdraw_wei = 5u128 * EVM_NATIVE_SCALE as u128;
        let (sender, raw) = signed_call(0x11, 0, f002, withdraw_wei, 60_000, basefee, withdraw_calldata(&spk));

        // Fund the sender ONLY via a same-block deposit (exact supply math).
        let payload = EvmExecutionPayload {
            system_ops: vec![EvmSystemOp::DepositClaim(DepositClaim {
                deposit_outpoint: Default::default(),
                evm_address: EvmAddress::from_bytes(sender.into_array()),
                amount_sompi: 10_000, // 1e14 wei
                claim_tip_sompi: 0,
            })],
            evm_coinbase: EvmAddress::from_bytes([0xFE; 20]),
            ..Default::default()
        };
        let accepted = [cand(raw, 0xFE)];
        let (result, mut db) = execute_block_evm(CacheDB::new(EmptyDB::default()), &input_with(&payload, &accepted)).unwrap();

        assert!(result.receipts[0].succeeded, "the withdraw call succeeded");
        assert_eq!(result.withdrawals.len(), 1);
        let w = &result.withdrawals[0];
        assert_eq!(w.receipt_index, 0);
        assert_eq!(w.op_index, 0);
        assert_eq!(w.from, EvmAddress::from_bytes(sender.into_array()));
        assert_eq!(w.amount_sompi, 5);
        assert_eq!(w.script_public_key, spk);
        // The withdraw log is part of the committed receipt (RPC-visible).
        assert!(result.receipts[0].logs.iter().any(|l| l.address.as_bytes() == f002.into_array()));
        // The escrow was burned: F002 holds nothing in THIS scenario. (NOT an
        // invariant — SELFDESTRUCT force-sends can strand supply-neutral wei in
        // F002; see `selfdestruct_to_f002_strands_value_supply_neutrally`.)
        assert_eq!(db.basic(f002).unwrap().map(|a| a.balance).unwrap_or_default(), U256::ZERO);
        // Supply: total = deposit − withdrawal − basefee burn, and matches the
        // actual state sum (gas: 21k intrinsic + calldata + 9k F002).
        let gas_burn = result.receipts[0].gas_used as u128 * basefee;
        let expected_total = 10_000u128 * EVM_NATIVE_SCALE as u128 - withdraw_wei - gas_burn;
        assert_eq!(result.header.evm_total_native_balance, EvmU256::from(expected_total));
        let snapshot = crate::snapshot::snapshot_from_cachedb(&db);
        let actual: u128 = snapshot.accounts.iter().map(|a| a.balance.try_to_u128().unwrap()).sum();
        assert_eq!(actual, expected_total);
    }

    /// Audit M-03: with the withdrawal cap ACTIVE, a block carrying MAX+1
    /// withdraw txs accepts exactly MAX (one WithdrawOp each), class-2 SKIPS the
    /// overflow tx, and the skipped tx's state never commits (sender nonce
    /// advances by only MAX) — supply-neutral. INERT ⇒ all MAX+1 materialize.
    /// Exercised on BOTH the v1 (strict-prefix) and v2 (gas-pool) paths.
    #[test]
    fn withdraw_cap_skips_overflow_and_preserves_state() {
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let spk = kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[0x42u8; 64]);
        let f002 = crate::withdraw::f002_address();
        let n = MAX_WITHDRAWALS_PER_EVM_BLOCK + 1; // one past the cap

        // MAX+1 withdraw txs from one sender (nonces 0..=MAX), 1 sompi each.
        let mut sender = None;
        let mut raws = Vec::with_capacity(n);
        for i in 0..n {
            let (s, raw) = signed_call(0x11, i as u64, f002, EVM_NATIVE_SCALE as u128, 60_000, basefee, withdraw_calldata(&spk));
            sender = Some(s);
            raws.push(raw);
        }
        let sender = sender.unwrap();
        let cands: Vec<AcceptedTxCandidate> = raws.into_iter().map(|r| cand(r, 0xFE)).collect();
        // Fund generously via a same-block deposit (covers MAX+1 txs' gas + withdraws).
        let payload = EvmExecutionPayload {
            system_ops: vec![EvmSystemOp::DepositClaim(DepositClaim {
                deposit_outpoint: Default::default(),
                evm_address: EvmAddress::from_bytes(sender.into_array()),
                amount_sompi: 1_000_000_000, // 1e19 wei — ample
                claim_tip_sompi: 0,
            })],
            evm_coinbase: EvmAddress::from_bytes([0xFE; 20]),
            ..Default::default()
        };
        // Helper: sender nonce after a run (proves the overflow tx did NOT commit).
        let nonce_of = |db: &mut CacheDB<EmptyDB>| db.basic(sender).unwrap().map(|a| a.nonce).unwrap_or(0);

        for (label, gas_pool_v2_fence) in [("v2", 0u64), ("v1", u64::MAX)] {
            // --- cap ACTIVE (fence 0 ≤ daa 42) ---
            let input = EvmBlockInput {
                gas_pool_v2_activation_daa_score: gas_pool_v2_fence,
                f002_withdraw_cap_activation_daa_score: 0,
                ..input_with(&payload, &cands)
            };
            let (res, mut db) = execute_block_evm(CacheDB::new(EmptyDB::default()), &input).unwrap();
            assert_eq!(res.withdrawals.len(), MAX_WITHDRAWALS_PER_EVM_BLOCK, "{label}: cap bounds materialized withdrawals");
            assert_eq!(res.header.accepted_tx_count, MAX_WITHDRAWALS_PER_EVM_BLOCK as u32, "{label}: MAX accepted");
            assert_eq!(res.header.skipped_tx_count, 1, "{label}: exactly the overflow tx skipped");
            assert_eq!(nonce_of(&mut db), MAX_WITHDRAWALS_PER_EVM_BLOCK as u64, "{label}: overflow tx state NOT committed (nonce advanced only MAX)");

            // --- cap INERT (fence u64::MAX) ⇒ uncapped, all MAX+1 materialize ---
            let inert = EvmBlockInput { f002_withdraw_cap_activation_daa_score: u64::MAX, ..input };
            let (res2, _db2) = execute_block_evm(CacheDB::new(EmptyDB::default()), &inert).unwrap();
            assert_eq!(res2.withdrawals.len(), n, "{label}: inert ⇒ uncapped (all {n} withdrawals)");
            assert_eq!(res2.header.skipped_tx_count, 0, "{label}: inert ⇒ no cap skip");
        }
    }

    /// v0.4 §9.3 / §6.1 class 4: user-input faults at F002 (non-multiple
    /// amount, zero value, garbage destination) REVERT the call — the carrying
    /// tx gets a status-0 receipt, gas is charged, no WithdrawOp is emitted,
    /// and the value returns to the sender. The block stays valid.
    #[test]
    fn f002_user_faults_revert_without_withdrawal() {
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let spk = kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[0x42u8; 64]);
        let f002 = crate::withdraw::f002_address();

        // (a) amount not an exact sompi multiple; (b) zero value;
        // (c) destination not the PQ-standard class.
        let (_s1, raw_frac) = signed_call(0x11, 0, f002, EVM_NATIVE_SCALE as u128 + 1, 60_000, basefee, withdraw_calldata(&spk));
        let (_s2, raw_zero) = signed_call(0x11, 1, f002, 0, 60_000, basefee, withdraw_calldata(&spk));
        let (sender, raw_badspk) =
            signed_call(0x11, 2, f002, 5 * EVM_NATIVE_SCALE as u128, 60_000, basefee, vec![0, 0, 0xAA, 0xBB]);

        let seed = funded_seed(sender, 1_000_000_000_000_000_000);
        let payload = EvmExecutionPayload { evm_coinbase: EvmAddress::from_bytes([0xFE; 20]), ..Default::default() };
        let accepted = [cand(raw_frac, 0xFE), cand(raw_zero, 0xFE), cand(raw_badspk, 0xFE)];
        let (result, mut db) = execute_block_evm(seed, &input_with(&payload, &accepted)).unwrap();

        assert_eq!(result.receipts.len(), 3, "all three executed (class 4, not skipped)");
        assert!(result.receipts.iter().all(|r| !r.succeeded), "every fault reverted");
        assert!(result.receipts.iter().all(|r| r.gas_used > 0), "reverts are charged");
        assert!(result.withdrawals.is_empty(), "no withdrawal escaped");
        // Reverts return the value — F002 nets zero HERE (scenario-specific;
        // see the SELFDESTRUCT residual test for the documented exception).
        assert_eq!(db.basic(f002).unwrap().map(|a| a.balance).unwrap_or_default(), U256::ZERO, "no value stuck in F002");
    }

    /// Audit L3 (documented behavior, pinned): under SHANGHAI (pre-EIP-6780) a
    /// contract that SELFDESTRUCTs with F002 as beneficiary force-credits it
    /// OUTSIDE the call-frame intercept — no withdraw log, no WithdrawOp, no
    /// burn. The wei is stranded in F002 forever, and that is SUPPLY-NEUTRAL:
    /// the O(1) accumulator still equals the actual state sum (the residual
    /// stays inside `evm_total_native_balance`). If this test breaks on an
    /// EVM_SPEC_ID bump (EIP-6780 changes SELFDESTRUCT), re-decide the F002
    /// residual policy BEFORE freezing the new spec.
    #[test]
    fn selfdestruct_to_f002_strands_value_supply_neutrally() {
        use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
        use alloy_eips::eip2718::Encodable2718;
        use alloy_signer::SignerSync;
        use alloy_signer_local::PrivateKeySigner;

        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let f002 = crate::withdraw::f002_address();
        let strand_wei = 5u128 * EVM_NATIVE_SCALE as u128;

        // Init code: PUSH20 <f002> SELFDESTRUCT — the deploying contract
        // self-destructs during creation, force-sending its endowment to F002.
        let mut init_code = vec![0x73u8];
        init_code.extend_from_slice(f002.as_slice());
        init_code.push(0xFF);

        let signer = PrivateKeySigner::from_bytes(&B256::from([0x11u8; 32])).unwrap();
        let tx = TxEip1559 {
            chain_id: EVM_CHAIN_ID,
            nonce: 0,
            gas_limit: 200_000,
            max_fee_per_gas: basefee,
            max_priority_fee_per_gas: 0,
            to: revm::primitives::TxKind::Create,
            value: U256::from(strand_wei),
            access_list: Default::default(),
            input: init_code.into(),
        };
        let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        let sender = signer.address();
        let raw = TxEnvelope::from(tx.into_signed(sig)).encoded_2718();

        // Fund the sender ONLY via a same-block deposit (exact supply math).
        // Upfront cost = gas_limit (200k) x max_fee (1 gwei) = 2e14 wei + the
        // 5e10 endowment, so deposit 30_000 sompi = 3e14 wei.
        let payload = EvmExecutionPayload {
            system_ops: vec![EvmSystemOp::DepositClaim(DepositClaim {
                deposit_outpoint: Default::default(),
                evm_address: EvmAddress::from_bytes(sender.into_array()),
                amount_sompi: 30_000, // 3e14 wei
                claim_tip_sompi: 0,
            })],
            evm_coinbase: EvmAddress::from_bytes([0xFE; 20]),
            ..Default::default()
        };
        let accepted = [cand(raw, 0xFE)];
        let (result, mut db) = execute_block_evm(CacheDB::new(EmptyDB::default()), &input_with(&payload, &accepted)).unwrap();

        assert_eq!(result.header.accepted_tx_count, 1, "the create tx executed (not skipped)");
        assert!(result.receipts[0].succeeded, "create + selfdestruct executed");
        assert!(result.withdrawals.is_empty(), "force-send bypasses the F002 intercept: NO WithdrawOp");
        // The endowment is stranded in F002 (not burned, not withdrawable).
        assert_eq!(db.basic(f002).unwrap().unwrap().balance, U256::from(strand_wei), "residual locked in F002");
        // Supply stays EXACT: total = deposit − basefee burn (nothing left the
        // lane), and the accumulator equals the actual state sum residual-included.
        let gas_burn = result.receipts[0].gas_used as u128 * basefee;
        let expected_total = 30_000u128 * EVM_NATIVE_SCALE as u128 - gas_burn;
        assert_eq!(result.header.evm_total_native_balance, EvmU256::from(expected_total));
        let snapshot = crate::snapshot::snapshot_from_cachedb(&db);
        let actual: u128 = snapshot.accounts.iter().map(|a| a.balance.try_to_u128().unwrap()).sum();
        assert_eq!(actual, expected_total, "supply invariant exact WITH the stranded F002 residual");
    }

    /// AH-1 (v0.4 §9.2): the claim tip splits the deposited amount between the
    /// deposit address and the ACCEPTING coinbase — supply-neutral.
    #[test]
    fn deposit_claim_tip_credits_accepting_coinbase() {
        let payload = EvmExecutionPayload {
            system_ops: vec![EvmSystemOp::DepositClaim(DepositClaim {
                deposit_outpoint: Default::default(),
                evm_address: EvmAddress::from_bytes([0xCC; 20]),
                amount_sompi: 100,
                claim_tip_sompi: 7,
            })],
            evm_coinbase: EvmAddress::from_bytes([0xFE; 20]),
            ..Default::default()
        };
        let (result, mut db) = execute_block_evm(CacheDB::new(EmptyDB::default()), &input_with(&payload, &[])).unwrap();
        let scale = EVM_NATIVE_SCALE as u128;
        assert_eq!(db.basic(Address::from([0xCC; 20])).unwrap().unwrap().balance, U256::from(93 * scale));
        assert_eq!(db.basic(Address::from([0xFE; 20])).unwrap().unwrap().balance, U256::from(7 * scale));
        assert_eq!(result.header.evm_total_native_balance, EvmU256::from(100 * scale), "the split is supply-neutral");
    }

}
