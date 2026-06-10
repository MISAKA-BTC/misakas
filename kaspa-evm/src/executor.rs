//! The block EVM executor (design §6.2): given a block's `selected_parent` EVM
//! header + state, its L1 header context, and its EVM payload, run the lane and
//! produce the `EvmExecutionResult` whose `header.commitment_root()` the
//! consensus verifier checks against `Header::evm_commitment_root`.
//!
//! Execution order (§6.2): apply bounded deposit claims (credit EVM balances) →
//! run user txs in payload order → [collect F002 withdrawals: P2 sub-B] →
//! compute roots → assemble the committed header. User tx failures are status-0
//! receipts (block stays valid, §6.3); only producer faults (checked by the
//! caller against the commitment) invalidate a block.

use crate::{env, roots, state, EvmExecError};
use kaspa_consensus_core::evm::{
    DepositClaim, EvmBloom, EvmExecutionHeader, EvmExecutionPayload, EvmExecutionResult, EvmLog, EvmReceipt, EvmSystemOp,
    EvmU256, EVM_GENESIS_STATE_ROOT, EVM_NATIVE_SCALE, SYSTEM_DEPOSIT_GAS_PER_CLAIM,
};
use kaspa_hashes::EvmH256;
use revm::primitives::{Address, EVMError, ExecutionResult, B256, KECCAK_EMPTY, U256};
use revm::{
    db::{AccountState, CacheDB, EmptyDB},
    Evm,
};

/// Everything the executor needs about a block, all ancestor-derived (the P3
/// hook fills this from the stores + the L1 header). The parent EVM **state** is
/// passed separately as the seed `CacheDB`.
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
    /// `B.evm_payload` (system ops + user txs + fee recipient + extra data).
    pub payload: &'a EvmExecutionPayload,
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
    let coinbase = to_revm_address(&input.payload.fee_recipient);
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

    // 2. User txs in payload order.
    let mut receipts: Vec<EvmReceipt> = Vec::with_capacity(input.payload.transactions.len());
    let mut burn_this_block: u128 = 0;
    for raw in &input.payload.transactions {
        let txenv = crate::tx::decode_tx_to_env(raw).map_err(EvmExecError::TxDecode)?;
        let derived = derived.clone();
        let basefee = derived.base_fee_per_gas;
        let mut evm = Evm::builder()
            .with_db(&mut state_db)
            .with_spec_id(crate::EVM_SPEC_ID)
            .modify_cfg_env(|c| c.chain_id = derived.chain_id)
            .modify_block_env(|b| {
                b.number = U256::from(derived.evm_number);
                b.timestamp = U256::from(derived.evm_timestamp_sec);
                b.coinbase = derived.coinbase;
                b.gas_limit = U256::from(derived.gas_limit);
                b.basefee = U256::from(basefee);
                b.difficulty = U256::ZERO;
                b.prevrandao = Some(derived.prev_randao);
            })
            .modify_tx_env(move |t| *t = txenv)
            .build();

        match evm.transact_commit() {
            Ok(result) => {
                let tx_gas = result.gas_used();
                gas_used = gas_used.saturating_add(tx_gas);
                burn_this_block = burn_this_block.saturating_add(basefee.saturating_mul(tx_gas as u128));
                receipts.push(make_receipt(&result, gas_used));
                drop(evm);
            }
            // Design §6.3: a user-caused pre-execution failure (nonce / funds /
            // basefee) is a status-0 receipt, NOT a block fault.
            Err(EVMError::Transaction(_)) => {
                drop(evm);
                receipts.push(EvmReceipt { succeeded: false, cumulative_gas_used: gas_used, gas_used: 0, logs: Vec::new() });
            }
            Err(other) => return Err(EvmExecError::InvalidTx(format!("{other:?}"))),
        }
    }

    // 3. F002 withdrawals — P2 sub-B (the inspector). Empty here.
    let withdrawals = Vec::new();

    // 4. Roots + bloom.
    let logs_bloom = EvmBloom::from_bytes(roots::logs_bloom(&receipts));
    let parent_burn = input.parent.map(|p| evmu256_to_u128(p.evm_burn_accumulator)).unwrap_or(0);
    let header = EvmExecutionHeader {
        parent_state_root,
        state_root: b256_to_evmh256(state::state_root(&state_db)),
        transactions_root: roots::transactions_root(&input.payload.transactions),
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
        evm_burn_accumulator: EvmU256::from(parent_burn.saturating_add(burn_this_block)),
    };

    let result = EvmExecutionResult { header, receipts, withdrawals, applied_deposit_claims: applied_claims };
    Ok((result, state_db))
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

    /// Build + sign a 1559 transfer from a fixed test key; returns (sender, raw).
    fn signed_transfer(nonce: u64, to: Address, value: u128, max_fee: u128, chain_id: u64) -> (Address, Vec<u8>) {
        use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
        use alloy_eips::eip2718::Encodable2718;
        use alloy_signer::SignerSync;
        use alloy_signer_local::PrivateKeySigner;

        let signer = PrivateKeySigner::from_bytes(&B256::from([0x11u8; 32])).unwrap();
        let tx = TxEip1559 {
            chain_id,
            nonce,
            gas_limit: 21_000,
            max_fee_per_gas: max_fee,
            max_priority_fee_per_gas: 0,
            to: revm::primitives::TxKind::Call(to),
            value: U256::from(value),
            access_list: Default::default(),
            input: Default::default(),
        };
        let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        (signer.address(), TxEnvelope::from(tx.into_signed(sig)).encoded_2718())
    }

    #[test]
    fn deposit_credit_and_transfer_produce_a_stable_commitment() {
        let chain_id = EVM_CHAIN_ID;
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let to = Address::with_last_byte(0x22);
        let (from, raw) = signed_transfer(0, to, 500, basefee, chain_id);

        let mut seed = CacheDB::new(EmptyDB::default());
        seed.insert_account_info(
            from,
            AccountInfo { balance: U256::from(1_000_000_000_000_000_000u128), nonce: 0, code_hash: KECCAK_EMPTY, code: None },
        );

        let claim_addr = EvmAddress::from_bytes([0xCC; 20]);
        let payload = EvmExecutionPayload {
            system_ops: vec![EvmSystemOp::DepositClaim(DepositClaim {
                deposit_outpoint: Default::default(),
                evm_address: claim_addr,
                amount_sompi: 7,
            })],
            transactions: vec![raw],
            fee_recipient: EvmAddress::from_bytes([0xFE; 20]),
            extra_data: vec![],
        };
        let input = EvmBlockInput {
            parent: None,
            header_timestamp_ms: 5_000,
            selected_parent_hash: [9u8; 64],
            blue_work_be: vec![1, 2, 3],
            daa_score: 42,
            payload: &payload,
        };

        let (result, mut db) = execute_block_evm(seed.clone(), &input).unwrap();

        // First EVM block: number 1, parent state root = genesis, ts = max(5, 0+1).
        assert_eq!(result.header.evm_number, 1);
        assert_eq!(result.header.parent_state_root, EVM_GENESIS_STATE_ROOT);
        assert_eq!(result.header.evm_timestamp_sec, 5);
        // Deposit credited 7 sompi × 1e10 = 7e10 wei.
        assert_eq!(db.basic(to_revm_address(&claim_addr)).unwrap().unwrap().balance, U256::from(70_000_000_000u64));
        // Transfer landed.
        assert_eq!(db.basic(to).unwrap().unwrap().balance, U256::from(500u64));
        // gas = 25k (claim) + 21k (transfer); burn = 21k × basefee.
        assert_eq!(result.header.gas_used, 46_000);
        assert_eq!(result.header.evm_burn_accumulator, EvmU256::from(21_000u128 * basefee));
        assert_eq!(result.applied_deposit_claims.len(), 1);
        assert_eq!(result.receipts.len(), 1);
        assert!(result.receipts[0].succeeded);

        // Determinism: same inputs ⇒ identical (non-trivial) commitment.
        let (result2, _) = execute_block_evm(seed, &input).unwrap();
        assert_eq!(result.header.commitment_root(), result2.header.commitment_root());
        assert_ne!(result.header.commitment_root(), kaspa_hashes::Hash64::default());
    }
}
