//! §12 archive — the pure state-diff / reconstruction engine (design §12.3/§12.4).
//!
//! The current EVM lane persists a FULL [`EvmStateSnapshot`] per block (prefix
//! 206) so a result is computed once and never re-executed on a virtual reorg
//! (design §11). That O(state) per-block form is fine inside the hot reorg
//! window but is not a long-term archive format. §12 adds a compact long-term
//! form: a forward [`EvmStateDiffV2`] per canonical block (prefix 220) anchored
//! by periodic full [`EvmStateCheckpointV1`]s (prefix 221), with bytecode kept
//! once in a content-addressed code store (prefix 222).
//!
//! This module is the secp-free, revm-free CORE of that machinery — pure
//! functions over the two `EvmStateSnapshot`s the executor already produces:
//!
//!   * [`compute_state_diff`] — `(parent, child) -> EvmStateDiffV2` (the writer).
//!   * [`recon_from_snapshot`] / [`apply_state_diff`] / [`recon_to_snapshot`] —
//!     seed from a checkpoint and replay forward diffs to reconstruct a
//!     historical state (the reader). Reconstruction is checked: a forward diff
//!     whose `before` view disagrees with the accumulated state is rejected as
//!     corruption ([`StateDiffError::Inconsistent`]) — never silently applied.
//!
//! The `evm`-feature node layer (design §12.4) additionally recomputes the
//! reconstructed state's keccak-MPT root and fails the query on any mismatch
//! with the block's committed header (no empty-state fallback). That root check
//! needs revm and so lives in `kaspa-evm`; everything here is offline-testable.

use super::{
    AccountChange, AccountCore, EvmAccountSnapshot, EvmAddress, EvmStateCheckpointV1, EvmStateDiffV2, EvmStateSnapshot, EvmU256,
    StorageChange,
};
use kaspa_hashes::{EvmH256, Hash64, blake2b_256_keyed};
use std::collections::BTreeMap;

/// `keccak256("")` — the `code_hash` of an account with no bytecode (an EOA). A
/// secp-free mirror of revm's `KECCAK_EMPTY`; `kaspa-evm` guards that the two
/// stay equal (`evm_empty_code_hash_matches_revm`). An [`AccountCore`] with this
/// hash has no entry in the content-addressed code store.
pub const EVM_EMPTY_CODE_HASH: EvmH256 = EvmH256::from_bytes([
    0xc5, 0xd2, 0x46, 0x01, 0x86, 0xf7, 0x23, 0x3c, 0x92, 0x7e, 0x7d, 0xb2, 0xdc, 0xc7, 0x03, 0xc0, 0xe5, 0x00, 0xb6, 0x53, 0xca,
    0x82, 0x27, 0x3b, 0x7b, 0xfa, 0xd8, 0x04, 0x5d, 0x85, 0xa4, 0x70,
]);

/// Default canonical-block interval between full checkpoints (design §12.3). A
/// checkpoint is also written at every pruning-point advance regardless of this.
pub const EVM_CHECKPOINT_INTERVAL: u64 = 2048;

/// Domain-separated key for the checkpoint-snapshot checksum (design §12.3) — a
/// keyed BLAKE2b-256 over the opaque snapshot encoding, distinct from every
/// other MISAKA keyed-blake context.
const EVM_CHECKPOINT_CHECKSUM_CONTEXT: &[u8] = b"misaka-evm-checkpoint-v1";

/// A reconstruction failure (design §12.4) — fail closed; never fall back to an
/// empty / partial state for a historical query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateDiffError {
    /// A forward diff's `before` view disagrees with the accumulated state, or a
    /// checkpoint's checksum/encoding is bad: the diff/checkpoint chain is corrupt.
    Inconsistent(String),
    /// A reconstructed account references bytecode (`code_hash != EVM_EMPTY_CODE_HASH`)
    /// that the content-addressed code store does not hold.
    MissingCode(EvmH256),
}

impl std::fmt::Display for StateDiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateDiffError::Inconsistent(m) => write!(f, "EVM state-diff inconsistency: {m}"),
            StateDiffError::MissingCode(h) => write!(f, "EVM state reconstruction: missing code for hash {h}"),
        }
    }
}

impl std::error::Error for StateDiffError {}

/// One account during reconstruction: its non-storage core plus its full set of
/// non-zero storage slots. Code bytes are resolved separately from the
/// content-addressed code store by `core.code_hash`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReconAccount {
    pub core: AccountCore,
    /// Non-zero storage slots, keyed by 32-byte big-endian slot (sorted by `BTreeMap`).
    pub storage: BTreeMap<[u8; 32], [u8; 32]>,
}

/// Reconstructed EVM state at a block: address -> account, ordered by address so
/// it serializes back to a canonical [`EvmStateSnapshot`] without re-sorting.
pub type ReconState = BTreeMap<[u8; 20], ReconAccount>;

#[inline]
fn core_of(a: &EvmAccountSnapshot) -> AccountCore {
    AccountCore { nonce: a.nonce, balance: a.balance, code_hash: a.code_hash }
}

/// An account is EIP-161 empty (and so excluded from a canonical snapshot) when
/// it has zero nonce, zero balance, no code, and no storage.
#[inline]
fn is_eip161_empty(core: &AccountCore, storage: &BTreeMap<[u8; 32], [u8; 32]>) -> bool {
    core.nonce == 0 && core.balance.is_zero() && core.code_hash == EVM_EMPTY_CODE_HASH && storage.is_empty()
}

/// Compute the forward state diff of `child` over `parent` (design §12.3). Both
/// are full canonical snapshots (accounts sorted/unique by address, storage
/// sorted/unique/non-zero), as produced by `kaspa_evm::snapshot::snapshot_from_cachedb`.
/// The diff lists only accounts whose core or storage changed, with `before`/`after`
/// cores (`None` ⇒ absent) and only the changed storage slots. Applying it to a
/// reconstruction of `parent` yields `child` ([`apply_state_diff`]).
pub fn compute_state_diff(parent: &EvmStateSnapshot, child: &EvmStateSnapshot, block: Hash64, parent_hash: Hash64) -> EvmStateDiffV2 {
    let p: BTreeMap<[u8; 20], &EvmAccountSnapshot> = parent.accounts.iter().map(|a| (a.address.as_bytes(), a)).collect();
    let c: BTreeMap<[u8; 20], &EvmAccountSnapshot> = child.accounts.iter().map(|a| (a.address.as_bytes(), a)).collect();

    // Union of addresses in ascending order (both maps are already address-ordered).
    let mut addrs: Vec<[u8; 20]> = Vec::with_capacity(p.len() + c.len());
    addrs.extend(p.keys().copied());
    addrs.extend(c.keys().copied());
    addrs.sort_unstable();
    addrs.dedup();

    let mut account_changes = Vec::new();
    for addr in addrs {
        let pa = p.get(&addr).copied();
        let ca = c.get(&addr).copied();
        let before = pa.map(core_of);
        let after = ca.map(core_of);
        let storage_changes = diff_storage(pa, ca);
        if before != after || !storage_changes.is_empty() {
            account_changes.push(AccountChange { address: EvmAddress::from_bytes(addr), before, after, storage_changes });
        }
    }
    EvmStateDiffV2 { block, parent: parent_hash, account_changes }
}

/// Diff two accounts' storage. Each snapshot stores only non-zero slots sorted by
/// slot, so a slot present in one and absent in the other changed to/from zero.
fn diff_storage(pa: Option<&EvmAccountSnapshot>, ca: Option<&EvmAccountSnapshot>) -> Vec<StorageChange> {
    let ps: BTreeMap<[u8; 32], EvmU256> =
        pa.map(|a| a.storage.iter().map(|(s, v)| (s.to_be_bytes(), *v)).collect()).unwrap_or_default();
    let cs: BTreeMap<[u8; 32], EvmU256> =
        ca.map(|a| a.storage.iter().map(|(s, v)| (s.to_be_bytes(), *v)).collect()).unwrap_or_default();

    let mut slots: Vec<[u8; 32]> = Vec::with_capacity(ps.len() + cs.len());
    slots.extend(ps.keys().copied());
    slots.extend(cs.keys().copied());
    slots.sort_unstable();
    slots.dedup();

    let mut out = Vec::new();
    for slot in slots {
        let before = ps.get(&slot).copied().unwrap_or(EvmU256::ZERO);
        let after = cs.get(&slot).copied().unwrap_or(EvmU256::ZERO);
        if before != after {
            out.push(StorageChange { slot: EvmU256::from_be_bytes(slot), before, after });
        }
    }
    out
}

/// New bytecode deployed by `diff` (design §12.3): for every account whose
/// `code_hash` went to a non-empty value this block, the `(code_hash, code)` to
/// write into the content-addressed code store (prefix 222). The code bytes come
/// from `child` (which holds the full account). Content-addressed, so the store
/// dedups; this returns only the freshly-deployed entries (bounded per block, not
/// the whole state).
pub fn diff_code_entries<'a>(diff: &EvmStateDiffV2, child: &'a EvmStateSnapshot) -> Vec<(EvmH256, &'a [u8])> {
    let c: BTreeMap<[u8; 20], &EvmAccountSnapshot> = child.accounts.iter().map(|a| (a.address.as_bytes(), a)).collect();
    let mut out = Vec::new();
    for ch in &diff.account_changes {
        let Some(after) = &ch.after else { continue };
        if after.code_hash == EVM_EMPTY_CODE_HASH {
            continue;
        }
        let deployed = match &ch.before {
            None => true,
            Some(before) => before.code_hash != after.code_hash,
        };
        if !deployed {
            continue;
        }
        if let Some(acc) = c.get(&ch.address.as_bytes()).filter(|a| !a.code.is_empty()) {
            out.push((after.code_hash, acc.code.as_slice()));
        }
    }
    out
}

/// Seed a reconstruction accumulator from a full snapshot (a decoded checkpoint).
pub fn recon_from_snapshot(snap: &EvmStateSnapshot) -> ReconState {
    snap.accounts
        .iter()
        .map(|a| {
            let storage: BTreeMap<[u8; 32], [u8; 32]> = a.storage.iter().map(|(s, v)| (s.to_be_bytes(), v.to_be_bytes())).collect();
            (a.address.as_bytes(), ReconAccount { core: core_of(a), storage })
        })
        .collect()
}

/// Apply one forward diff to the accumulator (design §12.4, step 3). Checked: the
/// diff's `before` view must agree with the accumulated state, else the chain is
/// corrupt ([`StateDiffError::Inconsistent`]). An account that becomes EIP-161
/// empty is removed, keeping the accumulator byte-identical to the canonical
/// snapshot at that block.
pub fn apply_state_diff(state: &mut ReconState, diff: &EvmStateDiffV2) -> Result<(), StateDiffError> {
    for ch in &diff.account_changes {
        let addr = ch.address.as_bytes();

        // Verify `before` against the accumulated state (corruption tripwire).
        let current = state.get(&addr);
        let current_core = current.map(|a| a.core.clone());
        if ch.before != current_core {
            return Err(StateDiffError::Inconsistent(format!(
                "account 0x{} before-core mismatch (diff expected {:?}, have {:?})",
                hex20(&addr),
                ch.before,
                current_core
            )));
        }

        match &ch.after {
            None => {
                // Destroyed: every prior storage slot must be cleared by the diff
                // (compute_state_diff emits before->0 for each), so the account is
                // simply dropped. Verify the slot befores matched above already.
                state.remove(&addr);
            }
            Some(core) => {
                let entry = state.entry(addr).or_default();
                entry.core = core.clone();
                for sc in &ch.storage_changes {
                    let slot = sc.slot.to_be_bytes();
                    let have = entry.storage.get(&slot).copied().map(EvmU256::from_be_bytes).unwrap_or(EvmU256::ZERO);
                    if sc.before != have {
                        return Err(StateDiffError::Inconsistent(format!(
                            "account 0x{} slot before mismatch (diff expected {:?}, have {:?})",
                            hex20(&addr),
                            sc.before,
                            have
                        )));
                    }
                    if sc.after.is_zero() {
                        entry.storage.remove(&slot);
                    } else {
                        entry.storage.insert(slot, sc.after.to_be_bytes());
                    }
                }
                // Defensive: an account left EIP-161 empty is not in canonical form.
                if is_eip161_empty(&entry.core, &entry.storage) {
                    state.remove(&addr);
                }
            }
        }
    }
    Ok(())
}

/// Apply one forward diff in REVERSE (C-01 Stage 1, slice S5 — the inverse-delta
/// engine used to re-base the flat state when the canonical head moves). Given a
/// reconstruction AT the diff's block (the child), this reverts it to the diff's
/// parent: each account's core goes `after → before` (removed if `before` is
/// `None`, i.e. the block created it) and each storage slot goes `after → before`.
/// Checked: the diff's `after` view must agree with the accumulated state (the
/// corruption tripwire, symmetric to [`apply_state_diff`]). Exact inverse:
/// `apply_state_diff` then `apply_inverse_state_diff` with the same diff is the
/// identity (for canonical diffs, where an account never has an EIP-161-empty
/// `after` — such an account is absent in the child snapshot, so the change is a
/// destroy `after = None`, not an empty core).
pub fn apply_inverse_state_diff(state: &mut ReconState, diff: &EvmStateDiffV2) -> Result<(), StateDiffError> {
    for ch in &diff.account_changes {
        let addr = ch.address.as_bytes();

        // Verify `after` against the accumulated state (we are at the child).
        let current_core = state.get(&addr).map(|a| a.core.clone());
        if ch.after != current_core {
            return Err(StateDiffError::Inconsistent(format!(
                "inverse: account 0x{} after-core mismatch (diff expected {:?}, have {:?})",
                hex20(&addr),
                ch.after,
                current_core
            )));
        }

        match &ch.before {
            None => {
                // The block created this account ⇒ undo the creation (drop it).
                state.remove(&addr);
            }
            Some(core) => {
                let entry = state.entry(addr).or_default();
                entry.core = core.clone();
                for sc in &ch.storage_changes {
                    let slot = sc.slot.to_be_bytes();
                    let have = entry.storage.get(&slot).copied().map(EvmU256::from_be_bytes).unwrap_or(EvmU256::ZERO);
                    if sc.after != have {
                        return Err(StateDiffError::Inconsistent(format!(
                            "inverse: account 0x{} slot after mismatch (diff expected {:?}, have {:?})",
                            hex20(&addr),
                            sc.after,
                            have
                        )));
                    }
                    if sc.before.is_zero() {
                        entry.storage.remove(&slot);
                    } else {
                        entry.storage.insert(slot, sc.before.to_be_bytes());
                    }
                }
                // Symmetric defensive: a reverted account left EIP-161 empty is not canonical.
                if is_eip161_empty(&entry.core, &entry.storage) {
                    state.remove(&addr);
                }
            }
        }
    }
    Ok(())
}

/// Serialize a reconstructed state back to a canonical [`EvmStateSnapshot`]
/// (design §12.4, step 4 input). Bytecode is resolved from the content-addressed
/// code store via `code_resolver`; a missing code hash is a hard error (no empty
/// fallback). The output is in the exact canonical form (sorted accounts/storage,
/// no empty accounts) that `kaspa_evm` validates before seeding the executor.
pub fn recon_to_snapshot(
    state: &ReconState,
    mut code_resolver: impl FnMut(&EvmH256) -> Option<Vec<u8>>,
) -> Result<EvmStateSnapshot, StateDiffError> {
    let mut accounts = Vec::with_capacity(state.len());
    for (addr, acc) in state {
        let code = if acc.core.code_hash == EVM_EMPTY_CODE_HASH {
            Vec::new()
        } else {
            code_resolver(&acc.core.code_hash).ok_or(StateDiffError::MissingCode(acc.core.code_hash))?
        };
        let storage: Vec<(EvmU256, EvmU256)> =
            acc.storage.iter().map(|(s, v)| (EvmU256::from_be_bytes(*s), EvmU256::from_be_bytes(*v))).collect();
        accounts.push(EvmAccountSnapshot {
            address: EvmAddress::from_bytes(*addr),
            nonce: acc.core.nonce,
            balance: acc.core.balance,
            code_hash: acc.core.code_hash,
            code,
            storage,
        });
    }
    Ok(EvmStateSnapshot { accounts })
}

/// Opaque encoding of a checkpoint's full snapshot (design §12.3). Currently
/// borsh; the field is versioned/opaque so a compressor can wrap this later
/// without a format break. Pairs with [`decode_checkpoint_snapshot`].
pub fn encode_checkpoint_snapshot(snap: &EvmStateSnapshot) -> Vec<u8> {
    borsh::to_vec(snap).expect("EvmStateSnapshot is infallibly borsh-serializable")
}

/// Decode a checkpoint's opaque snapshot encoding (inverse of [`encode_checkpoint_snapshot`]).
pub fn decode_checkpoint_snapshot(bytes: &[u8]) -> Result<EvmStateSnapshot, StateDiffError> {
    borsh::from_slice(bytes).map_err(|e| StateDiffError::Inconsistent(format!("checkpoint snapshot decode: {e}")))
}

/// The checkpoint checksum: keyed BLAKE2b-256 over the opaque snapshot encoding.
pub fn checkpoint_checksum(encoded: &[u8]) -> [u8; 32] {
    blake2b_256_keyed(EVM_CHECKPOINT_CHECKSUM_CONTEXT, encoded)
}

impl EvmStateCheckpointV1 {
    /// Build a checkpoint at `block` from its full snapshot (design §12.3). The
    /// `state_root` is the block's committed EVM state root (reconstruction
    /// verifies the decoded snapshot reproduces it).
    pub fn build(block: Hash64, evm_number: u64, state_root: EvmH256, snapshot: &EvmStateSnapshot) -> Self {
        let compressed_snapshot = encode_checkpoint_snapshot(snapshot);
        let checksum = checkpoint_checksum(&compressed_snapshot);
        EvmStateCheckpointV1 { block, evm_number, state_root, compressed_snapshot, checksum }
    }

    /// Decode this checkpoint's snapshot, verifying the checksum first (design
    /// §12.3/§12.4 — a bad checksum is corruption, fail closed).
    pub fn decode_snapshot(&self) -> Result<EvmStateSnapshot, StateDiffError> {
        if checkpoint_checksum(&self.compressed_snapshot) != self.checksum {
            return Err(StateDiffError::Inconsistent(format!("checkpoint {} checksum mismatch", self.block)));
        }
        decode_checkpoint_snapshot(&self.compressed_snapshot)
    }
}

#[inline]
fn hex20(b: &[u8; 20]) -> String {
    let mut s = String::with_capacity(40);
    for byte in b {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u256(n: u64) -> EvmU256 {
        EvmU256::from_u128(n as u128)
    }

    fn addr(b: u8) -> EvmAddress {
        EvmAddress::from_bytes([b; 20])
    }

    fn code_hash(b: u8) -> EvmH256 {
        EvmH256::from_bytes([b; 32])
    }

    /// Build a canonical snapshot from (addr, nonce, balance, code_hash, code, [(slot,val)]).
    fn snap(accounts: &[(u8, u64, u64, EvmH256, &[u8], &[(u64, u64)])]) -> EvmStateSnapshot {
        let mut accs: Vec<EvmAccountSnapshot> = accounts
            .iter()
            .map(|(a, nonce, bal, ch, code, storage)| {
                let mut st: Vec<(EvmU256, EvmU256)> = storage.iter().map(|(s, v)| (u256(*s), u256(*v))).collect();
                st.sort_unstable_by(|x, y| x.0.to_be_bytes().cmp(&y.0.to_be_bytes()));
                EvmAccountSnapshot {
                    address: addr(*a),
                    nonce: *nonce,
                    balance: u256(*bal),
                    code_hash: *ch,
                    code: code.to_vec(),
                    storage: st,
                }
            })
            .collect();
        accs.sort_unstable_by(|x, y| x.address.as_bytes().cmp(&y.address.as_bytes()));
        EvmStateSnapshot { accounts: accs }
    }

    fn eoa(a: u8, nonce: u64, bal: u64) -> (u8, u64, u64, EvmH256, &'static [u8], &'static [(u64, u64)]) {
        (a, nonce, bal, EVM_EMPTY_CODE_HASH, &[], &[])
    }

    /// The diff round-trips: applying compute(parent,child) to a reconstruction
    /// of parent reproduces child exactly — over a multi-block synthetic chain.
    #[test]
    fn diff_round_trips_over_a_chain() {
        // A code blob deployed at block 2; its hash is content-addressed.
        let code_a: &[u8] = &[0x60, 0x00, 0x60, 0x00, 0xfd];
        let ch_a = code_hash(0xAA);

        let chain = vec![
            EvmStateSnapshot::default(),                    // S0: genesis
            snap(&[eoa(0x01, 1, 1000), eoa(0x02, 0, 500)]), // S1
            snap(&[
                eoa(0x01, 2, 800),
                eoa(0x02, 0, 500),
                (0x03, 1, 0, ch_a, code_a, &[(1, 7), (2, 9)]), // contract deployed
            ]), // S2
            snap(&[
                eoa(0x01, 2, 800),
                (0x03, 1, 0, ch_a, code_a, &[(1, 7), (3, 4)]), // slot 2 cleared, slot 3 set; 0x02 self-destructed
            ]), // S3
        ];

        // Code store the reconstruction resolves against, filled by diff_code_entries.
        let mut code_store: BTreeMap<[u8; 32], Vec<u8>> = BTreeMap::new();

        let mut recon = recon_from_snapshot(&chain[0]);
        for i in 1..chain.len() {
            let parent_hash = Hash64::from_bytes([(i - 1) as u8; 64]);
            let block = Hash64::from_bytes([i as u8; 64]);
            let diff = compute_state_diff(&chain[i - 1], &chain[i], block, parent_hash);

            // Writer side: stash newly-deployed code.
            for (h, code) in diff_code_entries(&diff, &chain[i]) {
                code_store.insert(h.as_bytes(), code.to_vec());
            }

            // Reader side: apply forward, then materialize and compare.
            apply_state_diff(&mut recon, &diff).expect("consistent diff applies");
            let rebuilt = recon_to_snapshot(&recon, |h| code_store.get(&h.as_bytes()).cloned()).expect("code resolves");
            assert_eq!(rebuilt, chain[i], "reconstruction of S{i} must equal the canonical snapshot");
        }
    }

    /// The inverse engine (S5) walks a chain back DOWN: forward to the tip, then
    /// `apply_inverse_state_diff` each diff in reverse reproduces every ancestor
    /// state exactly, ending at empty genesis. (Exercises self-destruct undo,
    /// contract-deploy undo, and slot set/clear undo.)
    #[test]
    fn inverse_diff_walks_a_chain_back_down() {
        let code_a: &[u8] = &[0x60, 0x00, 0x60, 0x00, 0xfd];
        let ch_a = code_hash(0xAA);
        let chain = [
            EvmStateSnapshot::default(),
            snap(&[eoa(0x01, 1, 1000), eoa(0x02, 0, 500)]),
            snap(&[eoa(0x01, 2, 800), eoa(0x02, 0, 500), (0x03, 1, 0, ch_a, code_a, &[(1, 7), (2, 9)])]),
            snap(&[eoa(0x01, 2, 800), (0x03, 1, 0, ch_a, code_a, &[(1, 7), (3, 4)])]),
        ];
        let diffs: Vec<_> = (1..chain.len())
            .map(|i| {
                compute_state_diff(
                    &chain[i - 1],
                    &chain[i],
                    Hash64::from_bytes([i as u8; 64]),
                    Hash64::from_bytes([(i - 1) as u8; 64]),
                )
            })
            .collect();

        let mut recon = recon_from_snapshot(&chain[0]);
        for d in &diffs {
            apply_state_diff(&mut recon, d).unwrap();
        }
        assert_eq!(recon, recon_from_snapshot(&chain[3]), "forward reaches the tip");

        for i in (1..chain.len()).rev() {
            apply_inverse_state_diff(&mut recon, &diffs[i - 1]).expect("inverse applies");
            assert_eq!(recon, recon_from_snapshot(&chain[i - 1]), "inverse to S{} matches canonical", i - 1);
        }
        assert!(recon.is_empty(), "fully reverted back to empty genesis");
    }

    /// The inverse tripwire fires when applied to the wrong (non-child) state.
    #[test]
    fn inverse_diff_rejects_wrong_after_view() {
        let s0 = snap(&[eoa(0x01, 1, 1000)]);
        let s1 = snap(&[eoa(0x01, 2, 900)]);
        let diff = compute_state_diff(&s0, &s1, Hash64::from_bytes([1; 64]), Hash64::from_bytes([0; 64]));
        // Reverting must be applied to the child (s1); applying it to s0 mismatches `after`.
        let mut recon = recon_from_snapshot(&s0);
        assert!(matches!(apply_inverse_state_diff(&mut recon, &diff), Err(StateDiffError::Inconsistent(_))));
    }

    /// An empty (genesis→genesis) transition produces an empty diff.
    #[test]
    fn no_change_is_empty_diff() {
        let s = snap(&[eoa(0x01, 1, 1000)]);
        let diff = compute_state_diff(&s, &s, Hash64::from_bytes([2; 64]), Hash64::from_bytes([1; 64]));
        assert!(diff.account_changes.is_empty());
    }

    /// A forward diff whose `before` view is wrong is rejected (corruption),
    /// not silently applied.
    #[test]
    fn inconsistent_diff_is_rejected() {
        let s0 = snap(&[eoa(0x01, 1, 1000)]);
        let s1 = snap(&[eoa(0x01, 2, 900)]);
        let good = compute_state_diff(&s0, &s1, Hash64::from_bytes([1; 64]), Hash64::from_bytes([0; 64]));

        // Apply the good diff to the WRONG seed (empty) → before-mismatch.
        let mut recon = recon_from_snapshot(&EvmStateSnapshot::default());
        assert!(matches!(apply_state_diff(&mut recon, &good), Err(StateDiffError::Inconsistent(_))));
    }

    /// Reconstruction fails closed when bytecode is missing from the code store.
    #[test]
    fn missing_code_fails_closed() {
        let code: &[u8] = &[0x01, 0x02];
        let ch = code_hash(0xCD);
        let s = snap(&[(0x05, 1, 0, ch, code, &[])]);
        let recon = recon_from_snapshot(&s);
        // Empty resolver → MissingCode.
        let err = recon_to_snapshot(&recon, |_| None).unwrap_err();
        assert_eq!(err, StateDiffError::MissingCode(ch));
    }

    /// Checkpoint encode → decode round-trips and the checksum catches tampering.
    #[test]
    fn checkpoint_roundtrip_and_checksum() {
        let s = snap(&[eoa(0x01, 3, 42), (0x02, 1, 0, code_hash(0x11), &[0xaa, 0xbb], &[(9, 9)])]);
        let cp = EvmStateCheckpointV1::build(Hash64::from_bytes([7; 64]), 100, code_hash(0x99), &s);
        assert_eq!(cp.decode_snapshot().unwrap(), s);

        // Tamper with the encoding → checksum mismatch (fail closed).
        let mut bad = cp.clone();
        bad.compressed_snapshot[0] ^= 0xff;
        assert!(matches!(bad.decode_snapshot(), Err(StateDiffError::Inconsistent(_))));
    }

    /// diff_code_entries returns a deployed contract's code exactly once, keyed by
    /// its content hash, and nothing for pure balance/nonce churn.
    #[test]
    fn diff_code_entries_only_new_deployments() {
        let code: &[u8] = &[0xde, 0xad];
        let ch = code_hash(0x44);
        let s0 = snap(&[eoa(0x01, 0, 100)]);
        let s1 = snap(&[eoa(0x01, 0, 100), (0x02, 1, 0, ch, code, &[])]);
        let diff = compute_state_diff(&s0, &s1, Hash64::from_bytes([1; 64]), Hash64::from_bytes([0; 64]));
        let entries = diff_code_entries(&diff, &s1);
        assert_eq!(entries, vec![(ch, code)]);

        // A later block that only changes balance deploys no new code.
        let s2 = snap(&[eoa(0x01, 1, 50), (0x02, 1, 0, ch, code, &[(1, 1)])]);
        let diff2 = compute_state_diff(&s1, &s2, Hash64::from_bytes([2; 64]), Hash64::from_bytes([1; 64]));
        assert!(diff_code_entries(&diff2, &s2).is_empty());
    }
}
