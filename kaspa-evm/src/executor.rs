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
//! the caller) invalidate a block (§6.2).

use crate::{env, roots, state, EvmExecError};
use kaspa_consensus_core::evm::{
    DepositClaim, EvmAddress, EvmBloom, EvmExecutionHeader, EvmExecutionPayload, EvmExecutionResult, EvmLog, EvmReceipt,
    EvmSystemOp, EvmU256, EVM_GENESIS_STATE_ROOT, EVM_NATIVE_SCALE, MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK,
    SYSTEM_DEPOSIT_GAS_PER_CLAIM,
};
use kaspa_hashes::EvmH256;
use revm::primitives::{Address, EVMError, ExecutionResult, B256, KECCAK_EMPTY, U256};
use revm::{
    db::{AccountState, CacheDB, EmptyDB},
    Evm,
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
}

#[inline]
fn b256_to_evmh256(b: B256) -> EvmH256 {
    EvmH256::from_bytes(b.0)
}

#[inline]
fn to_revm_address(a: &kaspa_consensus_core::evm::EvmAddress) -> Address {
    Address::from(a.as_bytes())
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

    // 1. Bounded deposit claims, applied before user txs (design §6.2/§7.4):
    //    credit `amount_sompi × EVM_NATIVE_SCALE` wei and charge system gas.
    for op in &input.payload.system_ops {
        match op {
            EvmSystemOp::DepositClaim(claim) => {
                let wei = U256::from(claim.amount_sompi as u128 * EVM_NATIVE_SCALE as u128);
                let addr = to_revm_address(&claim.evm_address);
                // Credit the EVM balance. `load_account` materializes the entry (a
                // new address starts as `NotExisting`, which `basic()` reports as
                // absent); give it a real (EOA) code hash and mark it `Touched` so
                // the credit is visible to execution + the state trie + spendable.
                let acct = state_db.load_account(addr).map_err(|e| EvmExecError::InvalidTx(format!("deposit credit: {e:?}")))?;
                if acct.info.code_hash == B256::ZERO {
                    acct.info.code_hash = KECCAK_EMPTY;
                }
                acct.info.balance = acct.info.balance.saturating_add(wei);
                acct.account_state = AccountState::Touched;
                gas_used = gas_used.saturating_add(SYSTEM_DEPOSIT_GAS_PER_CLAIM);
                applied_claims.push(claim.clone());
            }
        }
    }

    // 2. Class-5 prefix-take (design §7, D4): walk `AcceptedEvmTxs(B)` in
    //    canonical order accumulating DECLARED gas limits; the first tx whose
    //    addition exceeds `MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK` and every tx
    //    after it are deterministically skipped (nonce unchanged — they remain
    //    re-acceptable later). Judging by gas_limit (not gas_used) fixes the
    //    accept set BEFORE execution, so a parallel scheduler's input is
    //    deterministic. An undecodable tx cannot appear in a body-valid payload
    //    (class-1 admission); defense-in-depth maps it to a deterministic skip
    //    so every implementation that reaches it stays consensus-consistent.
    let mut skipped_tx_count: u32 = 0;
    let mut planned: Vec<(revm::primitives::TxEnv, &AcceptedTxCandidate)> = Vec::with_capacity(input.accepted_txs.len());
    let mut cumulative_gas_limit: u64 = 0;
    let mut over_cap = false;
    for cand in input.accepted_txs {
        if over_cap {
            skipped_tx_count += 1; // class 5 (strict prefix: everything after the first over-cap tx)
            continue;
        }
        let txenv = match crate::tx::decode_tx_to_env(&cand.raw) {
            Ok(t) => t,
            Err(_) => {
                skipped_tx_count += 1; // defensive: class-1 material that slipped past admission
                continue;
            }
        };
        cumulative_gas_limit = cumulative_gas_limit.saturating_add(txenv.gas_limit);
        if cumulative_gas_limit > MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK {
            over_cap = true;
            skipped_tx_count += 1; // class 5
            continue;
        }
        planned.push((txenv, cand));
    }

    // 3. Accepted user txs in canonical order.
    let accepting_coinbase = derived.coinbase;
    let mut receipts: Vec<EvmReceipt> = Vec::with_capacity(planned.len());
    let mut executed_raws: Vec<Vec<u8>> = Vec::with_capacity(planned.len());
    let mut burn_this_block: u128 = 0;
    for (txenv, cand) in planned {
        let derived = derived.clone();
        let basefee = derived.base_fee_per_gas;
        // Effective gas price (EIP-1559): legacy txs carry no priority field —
        // their tip is gas_price − basefee; typed txs tip min(max_fee, basefee
        // + max_priority) − basefee. Needed below to reroute the tip.
        let max_fee = txenv.gas_price;
        let effective_gas_price = match txenv.gas_priority_fee {
            Some(priority) => max_fee.min(U256::from(basefee).saturating_add(priority)),
            None => max_fee,
        };
        let tip_per_gas = effective_gas_price.saturating_sub(U256::from(basefee));
        let tx_for_env = txenv.clone();
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
            .modify_tx_env(move |t| *t = tx_for_env)
            .build();

        match evm.transact_commit() {
            Ok(result) => {
                drop(evm);
                let tx_gas = result.gas_used();
                gas_used = gas_used.saturating_add(tx_gas);
                burn_this_block = burn_this_block.saturating_add(basefee.saturating_mul(tx_gas as u128));
                // §8.1 (D3): the priority fee belongs to the PAYLOAD block's
                // declared coinbase. revm credited the accepting coinbase
                // (block.coinbase) during commit; move the tip over. Balance
                // moves WITHIN the EVM lane — supply-neutral.
                let tip = tip_per_gas.saturating_mul(U256::from(tx_gas));
                let payload_cb = to_revm_address(&cand.payload_coinbase);
                if !tip.is_zero() && payload_cb != accepting_coinbase {
                    reroute_balance(&mut state_db, accepting_coinbase, payload_cb, tip)?;
                }
                receipts.push(make_receipt(&result, gas_used));
                executed_raws.push(cand.raw.clone());
            }
            // §6.1 class 2 (and 3 via the nonce rule): acceptance-time invalid
            // (nonce / upfront funds / max_fee < basefee) ⇒ deterministic skip —
            // no receipt, no gas, no nonce change, no trace beyond the counter.
            Err(EVMError::Transaction(_)) => {
                drop(evm);
                skipped_tx_count += 1;
            }
            Err(other) => return Err(EvmExecError::InvalidTx(format!("{other:?}"))),
        }
    }

    // 3. F002 withdrawals — P2 sub-B (the inspector). Empty here.
    let withdrawals = Vec::new();

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
    let total_native_balance = parent_total.saturating_add(deposited).saturating_sub(withdrawn).saturating_sub(burn_this_block);
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
        evm_burn_accumulator: EvmU256::from(parent_burn.saturating_add(burn_this_block)),
    };

    let result = EvmExecutionResult { header, receipts, withdrawals, applied_deposit_claims: applied_claims };
    Ok((result, state_db))
}

/// Move `amount` wei `from → to` directly in the working state (the §8.1 tip
/// reroute). Both accounts are materialized/Touched the same way the deposit
/// credit is, so the move is visible to later txs, the state trie and spending.
fn reroute_balance(db: &mut CacheDB<EmptyDB>, from: Address, to: Address, amount: U256) -> Result<(), EvmExecError> {
    let src = db.load_account(from).map_err(|e| EvmExecError::InvalidTx(format!("tip reroute (debit): {e:?}")))?;
    if src.info.code_hash == B256::ZERO {
        src.info.code_hash = KECCAK_EMPTY;
    }
    // revm just credited `from` with exactly the tip; an under-balance here is
    // impossible, but saturate rather than panic the verifier.
    src.info.balance = src.info.balance.saturating_sub(amount);
    src.account_state = AccountState::Touched;
    let dst = db.load_account(to).map_err(|e| EvmExecError::InvalidTx(format!("tip reroute (credit): {e:?}")))?;
    if dst.info.code_hash == B256::ZERO {
        dst.info.code_hash = KECCAK_EMPTY;
    }
    dst.info.balance = dst.info.balance.saturating_add(amount);
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
        }
    }

    fn cand(raw: Vec<u8>, cb: u8) -> AcceptedTxCandidate {
        AcceptedTxCandidate { raw, payload_coinbase: EvmAddress::from_bytes([cb; 20]) }
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
}
