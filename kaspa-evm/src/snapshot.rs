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
use revm::primitives::{AccountInfo, Address, Bytecode, Bytes, KECCAK_EMPTY, B256, U256};

#[inline]
fn to_u256(v: EvmU256) -> U256 {
    U256::from_be_bytes(v.to_be_bytes())
}

#[inline]
fn from_u256(v: U256) -> EvmU256 {
    EvmU256::from_be_bytes(v.to_be_bytes::<32>())
}

/// Validate that a snapshot is in the EXACT canonical form [`snapshot_from_cachedb`]
/// produces, BEFORE seeding (audit EVM-01 / EVM-03). The state root commits to
/// `code_hash` (not the code bytes) and collapses duplicates/ordering, so without
/// this an attacker-supplied pruning-point snapshot could reproduce the committed
/// `state_root` while smuggling in: missing bytecode (`code_hash != KECCAK_EMPTY`
/// with empty `code` — later execution would lack the code), mismatched code,
/// unsorted/duplicate accounts or storage slots, zero-valued slots, or EIP-161
/// empty accounts (DB bloat / ambiguity / future-trie migration hazard). All are
/// rejected here with a deterministic ERROR (never `panic!` — a consensus/import
/// path; the error propagates to a block-validity / IBD failure).
fn validate_snapshot_canonical(snapshot: &EvmStateSnapshot) -> Result<(), crate::EvmExecError> {
    let empty_code_hash = EvmH256::from_bytes(KECCAK_EMPTY.0);
    let mut prev_addr: Option<[u8; 20]> = None;
    for acc in &snapshot.accounts {
        let addr = acc.address.as_bytes();
        // Strictly ascending by address (== the snapshot_from_cachedb sort) ⇒ sorted + unique.
        if let Some(prev) = prev_addr {
            if prev >= addr {
                return Err(crate::EvmExecError::InvariantViolation(
                    "EVM snapshot corruption: accounts are not strictly sorted/unique by address".into(),
                ));
            }
        }
        prev_addr = Some(addr);

        // EVM-01: code ⇔ code_hash must be consistent in BOTH directions.
        if acc.code.is_empty() {
            if acc.code_hash != empty_code_hash {
                return Err(crate::EvmExecError::InvariantViolation(format!(
                    "EVM snapshot corruption: account 0x{} has non-empty code_hash {:?} but empty code bytes",
                    hex(&addr),
                    acc.code_hash
                )));
            }
        } else {
            let computed = EvmH256::from_bytes(revm::primitives::keccak256(&acc.code).0);
            if computed != acc.code_hash {
                return Err(crate::EvmExecError::InvariantViolation(format!(
                    "EVM snapshot corruption: account 0x{} code_hash {:?} != keccak256(code) {:?}",
                    hex(&addr),
                    acc.code_hash,
                    computed
                )));
            }
        }

        // EIP-161 empty accounts are excluded by snapshot_from_cachedb ⇒ reject them on import.
        if acc.nonce == 0 && acc.balance.is_zero() && acc.code.is_empty() && acc.storage.is_empty() && acc.code_hash == empty_code_hash {
            return Err(crate::EvmExecError::InvariantViolation(format!(
                "EVM snapshot corruption: account 0x{} is empty (EIP-161) — must not be persisted",
                hex(&addr)
            )));
        }

        // Storage: strictly ascending by slot (== the sort), unique, no zero values
        // (zero slots are excluded by snapshot_from_cachedb).
        let mut prev_slot: Option<[u8; 32]> = None;
        for (slot, val) in &acc.storage {
            if val.is_zero() {
                return Err(crate::EvmExecError::InvariantViolation(format!(
                    "EVM snapshot corruption: account 0x{} has a zero-valued storage slot",
                    hex(&addr)
                )));
            }
            let slot_be = slot.to_be_bytes();
            if let Some(prev) = prev_slot {
                if prev >= slot_be {
                    return Err(crate::EvmExecError::InvariantViolation(format!(
                        "EVM snapshot corruption: account 0x{} storage is not strictly sorted/unique by slot",
                        hex(&addr)
                    )));
                }
            }
            prev_slot = Some(slot_be);
        }
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Seed a fresh `CacheDB` from a persisted parent state snapshot, after a strict
/// canonicality + code/code_hash check ([`validate_snapshot_canonical`]). Seeding
/// of a locally-produced snapshot always passes (it came from
/// [`snapshot_from_cachedb`]); the check fails closed only on a corrupt store or a
/// malicious pruning-point import snapshot (audit EVM-01 / EVM-03).
pub fn seed_cachedb(snapshot: &EvmStateSnapshot) -> Result<CacheDB<EmptyDB>, crate::EvmExecError> {
    validate_snapshot_canonical(snapshot)?;
    let mut db = CacheDB::new(EmptyDB::default());
    for acc in &snapshot.accounts {
        let addr = Address::from(acc.address.as_bytes());
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
    // O12 (IBD catch-up): empty-acceptance fast path — no accepted txs and no
    // system ops means the state transition is the identity, so skip revm, the
    // keccak-MPT root recompute and the snapshot extraction entirely. The header
    // is produced by the same derivation functions as the full path (byte-equal
    // commitment; see executor::empty_acceptance_result + its equivalence test).
    if input.accepted_txs.is_empty() && input.payload.system_ops.is_empty() {
        return Ok((crate::executor::empty_acceptance_result(input), parent_snapshot.clone()));
    }
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
            gas_pool_v2_activation_daa_score: u64::MAX,
            f002_withdraw_cap_activation_daa_score: u64::MAX,
            f003_mldsa_verify_activation_daa_score: u64::MAX,
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

    fn acc(addr: u8, nonce: u64, balance: u128, code: Vec<u8>, code_hash: EvmH256, storage: Vec<(EvmU256, EvmU256)>) -> EvmAccountSnapshot {
        EvmAccountSnapshot { address: EvmAddress::from_bytes([addr; 20]), nonce, balance: EvmU256::from(balance), code_hash, code, storage }
    }
    fn snap(accounts: Vec<EvmAccountSnapshot>) -> EvmStateSnapshot {
        EvmStateSnapshot { accounts }
    }

    /// audit EVM-01: a snapshot account with a real `code_hash` but EMPTY `code`
    /// bytes reproduces the committed state root (root commits to `code_hash`) yet
    /// would leave the contract uncallable. `seed_cachedb` must reject it.
    #[test]
    fn seed_rejects_empty_code_with_non_empty_code_hash() {
        let real_code = vec![0x60u8, 0x00, 0x60, 0x00, 0xf3]; // some non-empty runtime
        let real_hash = EvmH256::from_bytes(revm::primitives::keccak256(&real_code).0);
        // code_hash of real code, but code bytes dropped.
        let bad = snap(vec![acc(0x11, 1, 0, vec![], real_hash, vec![])]);
        assert!(seed_cachedb(&bad).is_err(), "empty code with non-empty code_hash must be rejected");
        // The honest forms both pass: empty code + empty hash, and matching code + hash.
        let empty_hash = EvmH256::from_bytes(KECCAK_EMPTY.0);
        assert!(seed_cachedb(&snap(vec![acc(0x11, 1, 0, vec![], empty_hash, vec![])])).is_ok());
        assert!(seed_cachedb(&snap(vec![acc(0x11, 1, 0, real_code.clone(), real_hash, vec![])])).is_ok());
        // A code/code_hash MISMATCH (non-empty code, wrong hash) is also rejected.
        assert!(seed_cachedb(&snap(vec![acc(0x11, 1, 0, real_code, empty_hash, vec![])])).is_err());
    }

    /// audit EVM-03: a non-canonical snapshot (unsorted/duplicate accounts, zero
    /// storage slots, unsorted storage, or EIP-161 empty accounts) can collapse to
    /// the committed root in the CacheDB but is rejected on import.
    #[test]
    fn seed_rejects_non_canonical_snapshot() {
        let empty_hash = EvmH256::from_bytes(KECCAK_EMPTY.0);
        let one = EvmU256::from(1u128);
        let two = EvmU256::from(2u128);
        let zero = EvmU256::from(0u128);
        // Unsorted / non-unique accounts (0x22 then 0x11).
        assert!(seed_cachedb(&snap(vec![
            acc(0x22, 1, 5, vec![], empty_hash, vec![]),
            acc(0x11, 1, 5, vec![], empty_hash, vec![]),
        ]))
        .is_err());
        // Duplicate account address.
        assert!(seed_cachedb(&snap(vec![
            acc(0x11, 1, 5, vec![], empty_hash, vec![]),
            acc(0x11, 2, 6, vec![], empty_hash, vec![]),
        ]))
        .is_err());
        // Zero-valued storage slot.
        assert!(seed_cachedb(&snap(vec![acc(0x11, 1, 5, vec![], empty_hash, vec![(one, zero)])])).is_err());
        // Unsorted storage slots (slot 2 before slot 1).
        assert!(seed_cachedb(&snap(vec![acc(0x11, 1, 5, vec![], empty_hash, vec![(two, one), (one, one)])])).is_err());
        // EIP-161 empty account.
        assert!(seed_cachedb(&snap(vec![acc(0x11, 0, 0, vec![], empty_hash, vec![])])).is_err());
        // The canonical form passes.
        assert!(seed_cachedb(&snap(vec![acc(0x11, 1, 5, vec![], empty_hash, vec![(one, one), (two, one)])])).is_ok());
    }
}

