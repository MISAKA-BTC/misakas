//! Persisted EVM state snapshot ↔ revm in-memory `CacheDB` (design §11, P3).
//!
//! Lets the consensus stores hold a block's full EVM state (secp-free borsh,
//! [`EvmStateSnapshot`]) and the executor seed from the parent snapshot / extract
//! the child snapshot. This append-only `(parent_state, block) -> child_state`
//! chaining is what lets an EVM result be computed once and never re-executed on
//! a virtual reorg (design §2.1/§10.1) — the basis for P3's no-replay rule.

use kaspa_consensus_core::evm::{EvmAccountSnapshot, EvmAddress, EvmExecutionResult, EvmStateSnapshot, EvmU256};
use kaspa_hashes::EvmH256;
use revm::db::{CacheDB, EmptyDB};
use revm::primitives::{AccountInfo, Address, Bytecode, Bytes, B256, U256};

#[inline]
fn to_u256(v: EvmU256) -> U256 {
    U256::from_be_bytes(v.to_be_bytes())
}

#[inline]
fn from_u256(v: U256) -> EvmU256 {
    EvmU256::from_be_bytes(v.to_be_bytes::<32>())
}

/// Seed a fresh `CacheDB` from a persisted parent state snapshot.
///
/// audit #10 / R2-#4: when an account carries bytecode, verify
/// `code_hash == keccak256(code)`. The state root commits to `code_hash`, not the
/// code bytes, so a corrupt/migrated store with mismatched code would otherwise
/// execute against the wrong code while still reproducing the committed root for
/// callers that don't touch it. Seeding is local (no attacker input), so a
/// mismatch is store corruption — fail closed with a deterministic ERROR (R2-#4:
/// a consensus/template path must not `panic!`; the error propagates up to a
/// block-validity / template-build failure).
pub fn seed_cachedb(snapshot: &EvmStateSnapshot) -> Result<CacheDB<EmptyDB>, crate::EvmExecError> {
    let mut db = CacheDB::new(EmptyDB::default());
    for acc in &snapshot.accounts {
        let addr = Address::from(acc.address.as_bytes());
        if !acc.code.is_empty() {
            let computed = revm::primitives::keccak256(&acc.code);
            if computed.0 != acc.code_hash.as_bytes() {
                return Err(crate::EvmExecError::InvariantViolation(format!(
                    "EVM snapshot corruption: account {addr} code_hash {:?} != keccak256(code) {:?}",
                    acc.code_hash, computed
                )));
            }
        }
        let code = if acc.code.is_empty() { None } else { Some(Bytecode::new_raw(Bytes::from(acc.code.clone()))) };
        db.insert_account_info(
            addr,
            AccountInfo { balance: to_u256(acc.balance), nonce: acc.nonce, code_hash: B256::from(acc.code_hash.as_bytes()), code },
        );
        for (slot, val) in &acc.storage {
            db.insert_account_storage(addr, to_u256(*slot), to_u256(*val)).expect("seed storage on a just-inserted account");
        }
    }
    Ok(db)
}

/// Extract a deterministic full-state snapshot from a post-execution `CacheDB`
/// (EIP-161 empty accounts and zero storage slots excluded; accounts sorted by
/// address, slots by key).
pub fn snapshot_from_cachedb(db: &CacheDB<EmptyDB>) -> EvmStateSnapshot {
    let mut accounts: Vec<EvmAccountSnapshot> = db
        .accounts
        .iter()
        .filter(|(_, a)| !a.info.is_empty())
        .map(|(addr, a)| {
            let mut storage: Vec<(EvmU256, EvmU256)> =
                a.storage.iter().filter(|(_, v)| !v.is_zero()).map(|(s, v)| (from_u256(*s), from_u256(*v))).collect();
            storage.sort_unstable_by(|x, y| x.0.to_be_bytes().cmp(&y.0.to_be_bytes()));
            EvmAccountSnapshot {
                address: EvmAddress::from_bytes(addr.into_array()),
                nonce: a.info.nonce,
                balance: from_u256(a.info.balance),
                code_hash: EvmH256::from_bytes(a.info.code_hash.0),
                code: a.info.code.as_ref().map(|c| c.original_bytes().to_vec()).unwrap_or_default(),
                storage,
            }
        })
        .collect();
    accounts.sort_unstable_by(|x, y| x.address.as_bytes().cmp(&y.address.as_bytes()));
    EvmStateSnapshot { accounts }
}

/// Execute a block from a persisted parent state snapshot, returning the
/// committed result and the child state snapshot to persist. A pure function of
/// `(parent_snapshot, block)` — re-running yields an identical result, so the
/// consensus layer stores it once and never re-executes on reorg.
pub fn execute_block_from_snapshot(
    parent_snapshot: &EvmStateSnapshot,
    input: &crate::EvmBlockInput,
) -> Result<(EvmExecutionResult, EvmStateSnapshot), crate::EvmExecError> {
    let db = seed_cachedb(parent_snapshot)?;
    let (result, post_db) = crate::execute_block_evm(db, input)?;
    Ok((result, snapshot_from_cachedb(&post_db)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EvmBlockInput;
    use kaspa_consensus_core::evm::{EvmExecutionHeader, EvmExecutionPayload, EVM_CHAIN_ID, EVM_INITIAL_BASE_FEE};
    use revm::primitives::{TxKind, KECCAK_EMPTY};

    fn signed_transfer(nonce: u64, to: Address, value: u128, max_fee: u128) -> (Address, Vec<u8>) {
        use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
        use alloy_eips::eip2718::Encodable2718;
        use alloy_signer::SignerSync;
        use alloy_signer_local::PrivateKeySigner;
        let signer = PrivateKeySigner::from_bytes(&B256::from([0x11u8; 32])).unwrap();
        let tx = TxEip1559 {
            chain_id: EVM_CHAIN_ID,
            nonce,
            gas_limit: 21_000,
            max_fee_per_gas: max_fee,
            max_priority_fee_per_gas: 0,
            to: TxKind::Call(to),
            value: U256::from(value),
            access_list: Default::default(),
            input: Default::default(),
        };
        let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        (signer.address(), TxEnvelope::from(tx.into_signed(sig)).encoded_2718())
    }

    fn input<'a>(
        payload: &'a EvmExecutionPayload,
        accepted: &'a [crate::AcceptedTxCandidate],
        parent: Option<&'a EvmExecutionHeader>,
    ) -> EvmBlockInput<'a> {
        EvmBlockInput {
            parent,
            header_timestamp_ms: 10_000,
            selected_parent_hash: [7u8; 64],
            blue_work_be: vec![0, 1],
            daa_score: 1,
            payload,
            accepted_txs: accepted,
        }
    }

    fn cand(raw: Vec<u8>) -> crate::AcceptedTxCandidate {
        crate::AcceptedTxCandidate { raw, payload_coinbase: EvmAddress::from_bytes([0xEE; 20]) }
    }

    #[test]
    fn snapshot_chaining_is_append_only_and_deterministic() {
        let basefee = EVM_INITIAL_BASE_FEE as u128;
        let to = Address::with_last_byte(0x55);
        let (from, raw1) = signed_transfer(0, to, 500, basefee);

        // Genesis-state snapshot: the sender funded.
        let snap0 = EvmStateSnapshot {
            accounts: vec![EvmAccountSnapshot {
                address: EvmAddress::from_bytes(from.into_array()),
                nonce: 0,
                balance: EvmU256::from(1_000_000_000_000_000_000u128),
                code_hash: EvmH256::from_bytes(KECCAK_EMPTY.0),
                code: vec![],
                storage: vec![],
            }],
        };

        // v0.4 §3.1: user txs enter as ACCEPTED txs (mergeset payloads), not as
        // the block's own payload.
        let p = EvmExecutionPayload::default();
        let a1 = [cand(raw1)];
        let (r1, snap1) = execute_block_from_snapshot(&snap0, &input(&p, &a1, None)).unwrap();
        assert_eq!(r1.header.evm_number, 1);
        assert!(snap1.accounts.iter().any(|a| a.address.as_bytes() == to.into_array() && a.balance == EvmU256::from(500u128)), "recipient credited in child snapshot");

        // Re-running block 1 from snap0 is identical (the no-replay basis).
        let (r1b, snap1b) = execute_block_from_snapshot(&snap0, &input(&p, &a1, None)).unwrap();
        assert_eq!(r1.header.commitment_root(), r1b.header.commitment_root());
        assert_eq!(snap1, snap1b);

        // Block 2 chains on block 1: parent_state_root = block1's state_root, number 2.
        let (_from2, raw2) = signed_transfer(1, to, 300, basefee);
        let a2 = [cand(raw2)];
        let (r2, _snap2) = execute_block_from_snapshot(&snap1, &input(&p, &a2, Some(&r1.header))).unwrap();
        assert_eq!(r2.header.parent_state_root, r1.header.state_root);
        assert_eq!(r2.header.evm_number, 2);
    }
}

