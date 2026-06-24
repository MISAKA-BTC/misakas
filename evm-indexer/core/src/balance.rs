//! §10.4 / §10.5 — materialized balances/ownership + reorg inverse delta.
//!
//! Current balances are a fold of every canonical transfer. On a reorg the
//! indexer must UNDO a detached block's transfers exactly, so every `apply` has
//! an exact `revert`, and a block is reverted by reverting its transfers in
//! REVERSE order (so a token moved `A→B→C` within one block unwinds `C→B→A`).
//! A mint is `from == 0x0`, a burn is `to == 0x0` (§10.4); the zero address is
//! never credited/debited a real balance.

use std::collections::HashMap;

use alloy_primitives::U256;

use crate::event::{TokenStandard, TokenTransfer};

const ZERO: [u8; 20] = [0u8; 20];

/// Materialized token state (§10.4): ERC-20 balances, ERC-721 ownership, and
/// ERC-1155 balances. Absent key ⇒ zero balance / no owner (the maps never hold
/// a zero entry, so `apply`+`revert` round-trips back to the exact same map).
#[derive(Debug, Default, Clone)]
pub struct Balances {
    /// (token, owner) → balance.
    erc20: HashMap<([u8; 20], [u8; 20]), U256>,
    /// (collection, token_id) → current owner.
    erc721: HashMap<([u8; 20], U256), [u8; 20]>,
    /// (collection, token_id, owner) → balance.
    erc1155: HashMap<([u8; 20], U256, [u8; 20]), U256>,
}

/// Add `amount` to `map[key]` (no-op for zero; never stores a zero).
fn credit<K: Eq + std::hash::Hash>(map: &mut HashMap<K, U256>, key: K, amount: U256) {
    if amount.is_zero() {
        return;
    }
    let bal = map.entry(key).or_insert(U256::ZERO);
    *bal = bal.saturating_add(amount);
}

/// Subtract `amount` from `map[key]`, removing the key when it reaches zero. A
/// debit of an absent/short balance saturates at zero (a debit below zero only
/// happens on inconsistent input — clamp rather than panic).
fn debit<K: Eq + std::hash::Hash>(map: &mut HashMap<K, U256>, key: K, amount: U256) {
    if amount.is_zero() {
        return;
    }
    if let Some(bal) = map.get_mut(&key) {
        *bal = bal.saturating_sub(amount);
        if bal.is_zero() {
            map.remove(&key);
        }
    }
}

impl Balances {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one transfer into current state (§10.4).
    pub fn apply(&mut self, t: &TokenTransfer) {
        match t.standard {
            TokenStandard::Erc20 => {
                if t.from != ZERO {
                    debit(&mut self.erc20, (t.token, t.from), t.amount);
                }
                if t.to != ZERO {
                    credit(&mut self.erc20, (t.token, t.to), t.amount);
                }
            }
            TokenStandard::Erc721 => {
                let id = t.token_id.unwrap_or(U256::ZERO);
                if t.to == ZERO {
                    self.erc721.remove(&(t.token, id)); // burn
                } else {
                    self.erc721.insert((t.token, id), t.to);
                }
            }
            TokenStandard::Erc1155 => {
                let id = t.token_id.unwrap_or(U256::ZERO);
                if t.from != ZERO {
                    debit(&mut self.erc1155, (t.token, id, t.from), t.amount);
                }
                if t.to != ZERO {
                    credit(&mut self.erc1155, (t.token, id, t.to), t.amount);
                }
            }
        }
    }

    /// Undo one transfer — the exact inverse of [`apply`](Self::apply) (§10.5).
    /// For ERC-721 the owner reverts to `from` (or is removed if `from` was the
    /// zero address, i.e. the transfer was a mint).
    pub fn revert(&mut self, t: &TokenTransfer) {
        match t.standard {
            TokenStandard::Erc20 => {
                if t.to != ZERO {
                    debit(&mut self.erc20, (t.token, t.to), t.amount);
                }
                if t.from != ZERO {
                    credit(&mut self.erc20, (t.token, t.from), t.amount);
                }
            }
            TokenStandard::Erc721 => {
                let id = t.token_id.unwrap_or(U256::ZERO);
                if t.from == ZERO {
                    self.erc721.remove(&(t.token, id)); // undo a mint
                } else {
                    self.erc721.insert((t.token, id), t.from);
                }
            }
            TokenStandard::Erc1155 => {
                let id = t.token_id.unwrap_or(U256::ZERO);
                if t.to != ZERO {
                    debit(&mut self.erc1155, (t.token, id, t.to), t.amount);
                }
                if t.from != ZERO {
                    credit(&mut self.erc1155, (t.token, id, t.from), t.amount);
                }
            }
        }
    }

    /// Apply a block's transfers in order (the block attaches, §10.5).
    pub fn apply_block(&mut self, transfers: &[TokenTransfer]) {
        for t in transfers {
            self.apply(t);
        }
    }

    /// Revert a detached block's transfers — in REVERSE order, the exact inverse
    /// of [`apply_block`](Self::apply_block). Reverse matters: a token moved
    /// `A→B→C` within one block must unwind `C→B→A` (§10.5 inverse delta).
    pub fn revert_block(&mut self, transfers: &[TokenTransfer]) {
        for t in transfers.iter().rev() {
            self.revert(t);
        }
    }

    /// ERC-20 balance of `owner` for `token` (zero if absent).
    pub fn erc20_balance(&self, token: [u8; 20], owner: [u8; 20]) -> U256 {
        self.erc20.get(&(token, owner)).copied().unwrap_or(U256::ZERO)
    }

    /// Current ERC-721 owner of `(collection, token_id)`, if any.
    pub fn erc721_owner(&self, collection: [u8; 20], token_id: U256) -> Option<[u8; 20]> {
        self.erc721.get(&(collection, token_id)).copied()
    }

    /// ERC-1155 balance of `owner` for `(collection, token_id)` (zero if absent).
    pub fn erc1155_balance(&self, collection: [u8; 20], token_id: U256, owner: [u8; 20]) -> U256 {
        self.erc1155.get(&(collection, token_id, owner)).copied().unwrap_or(U256::ZERO)
    }

    /// Number of non-zero materialized entries (diagnostics / round-trip tests).
    pub fn entry_count(&self) -> usize {
        self.erc20.len() + self.erc721.len() + self.erc1155.len()
    }

    // --- seed setters (a persistent backend seeds these from storage before
    //     folding a block's transfers, then reads the results back to write) ---

    /// Seed/overwrite an ERC-20 balance (a zero value clears the entry, keeping
    /// the "no entry == zero" invariant).
    pub fn set_erc20(&mut self, token: [u8; 20], owner: [u8; 20], balance: U256) {
        if balance.is_zero() {
            self.erc20.remove(&(token, owner));
        } else {
            self.erc20.insert((token, owner), balance);
        }
    }

    /// Seed/overwrite ERC-721 ownership (`None` clears it).
    pub fn set_erc721(&mut self, collection: [u8; 20], token_id: U256, owner: Option<[u8; 20]>) {
        match owner {
            Some(o) => {
                self.erc721.insert((collection, token_id), o);
            }
            None => {
                self.erc721.remove(&(collection, token_id));
            }
        }
    }

    /// Seed/overwrite an ERC-1155 balance (a zero value clears the entry).
    pub fn set_erc1155(&mut self, collection: [u8; 20], token_id: U256, owner: [u8; 20], balance: U256) {
        if balance.is_zero() {
            self.erc1155.remove(&(collection, token_id, owner));
        } else {
            self.erc1155.insert((collection, token_id, owner), balance);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: [u8; 20] = [0xAA; 20];
    const B: [u8; 20] = [0xBB; 20];
    const C: [u8; 20] = [0xCC; 20];
    const TOK: [u8; 20] = [0x11; 20];

    fn erc20(from: [u8; 20], to: [u8; 20], amount: u64) -> TokenTransfer {
        TokenTransfer { standard: TokenStandard::Erc20, token: TOK, operator: None, from, to, token_id: None, amount: U256::from(amount) }
    }
    fn erc721(from: [u8; 20], to: [u8; 20], id: u64) -> TokenTransfer {
        TokenTransfer {
            standard: TokenStandard::Erc721,
            token: TOK,
            operator: None,
            from,
            to,
            token_id: Some(U256::from(id)),
            amount: U256::from(1u64),
        }
    }
    fn erc1155(from: [u8; 20], to: [u8; 20], id: u64, amount: u64) -> TokenTransfer {
        TokenTransfer {
            standard: TokenStandard::Erc1155,
            token: TOK,
            operator: Some(A),
            from,
            to,
            token_id: Some(U256::from(id)),
            amount: U256::from(amount),
        }
    }

    #[test]
    fn erc20_mint_transfer_burn_and_revert() {
        let mut bal = Balances::new();
        bal.apply(&erc20(ZERO, A, 100)); // mint 100 → A
        assert_eq!(bal.erc20_balance(TOK, A), U256::from(100u64));
        bal.apply(&erc20(A, B, 30)); // A → B 30
        assert_eq!(bal.erc20_balance(TOK, A), U256::from(70u64));
        assert_eq!(bal.erc20_balance(TOK, B), U256::from(30u64));

        // Revert the A→B transfer exactly.
        bal.revert(&erc20(A, B, 30));
        assert_eq!(bal.erc20_balance(TOK, A), U256::from(100u64));
        assert_eq!(bal.erc20_balance(TOK, B), U256::ZERO, "zeroed balance leaves no entry");

        // Revert the mint → A back to zero, no stray entries.
        bal.revert(&erc20(ZERO, A, 100));
        assert_eq!(bal.erc20_balance(TOK, A), U256::ZERO);
        assert_eq!(bal.entry_count(), 0);
    }

    #[test]
    fn erc721_ownership_and_revert() {
        let mut bal = Balances::new();
        bal.apply(&erc721(ZERO, A, 7)); // mint #7 → A
        assert_eq!(bal.erc721_owner(TOK, U256::from(7u64)), Some(A));
        bal.apply(&erc721(A, B, 7)); // A → B
        assert_eq!(bal.erc721_owner(TOK, U256::from(7u64)), Some(B));

        bal.revert(&erc721(A, B, 7)); // undo → owner back to A
        assert_eq!(bal.erc721_owner(TOK, U256::from(7u64)), Some(A));
        bal.revert(&erc721(ZERO, A, 7)); // undo mint → no owner
        assert_eq!(bal.erc721_owner(TOK, U256::from(7u64)), None);
        assert_eq!(bal.entry_count(), 0);

        // A burn removes ownership.
        bal.apply(&erc721(ZERO, A, 9));
        bal.apply(&erc721(A, ZERO, 9)); // burn
        assert_eq!(bal.erc721_owner(TOK, U256::from(9u64)), None);
    }

    #[test]
    fn erc1155_balance_and_revert() {
        let mut bal = Balances::new();
        bal.apply(&erc1155(ZERO, A, 1, 5)); // mint 5 of #1 → A
        bal.apply(&erc1155(A, B, 1, 2)); // A → B 2
        assert_eq!(bal.erc1155_balance(TOK, U256::from(1u64), A), U256::from(3u64));
        assert_eq!(bal.erc1155_balance(TOK, U256::from(1u64), B), U256::from(2u64));
        bal.revert(&erc1155(A, B, 1, 2));
        assert_eq!(bal.erc1155_balance(TOK, U256::from(1u64), A), U256::from(5u64));
        assert_eq!(bal.erc1155_balance(TOK, U256::from(1u64), B), U256::ZERO);
    }

    /// The reorg property: applying a block then reverting it (in reverse) is the
    /// identity — INCLUDING a token moved twice within the same block (A→B→C),
    /// which only unwinds correctly because revert_block iterates in reverse.
    #[test]
    fn block_apply_then_revert_is_identity_with_intra_block_chain() {
        let mut bal = Balances::new();
        // Pre-state: A holds 50, owns NFT #1; mint set up out-of-band.
        bal.apply(&erc20(ZERO, A, 50));
        bal.apply(&erc721(ZERO, A, 1));
        let snapshot = bal.clone();

        // A block that chains a transfer: 20 A→B then B→C; and NFT #1 A→B→C.
        let block = vec![erc20(A, B, 20), erc20(B, C, 20), erc721(A, B, 1), erc721(B, C, 1)];
        bal.apply_block(&block);
        assert_eq!(bal.erc20_balance(TOK, C), U256::from(20u64));
        assert_eq!(bal.erc20_balance(TOK, A), U256::from(30u64));
        assert_eq!(bal.erc721_owner(TOK, U256::from(1u64)), Some(C));

        // Detach the block (reorg): reverse-order revert restores the snapshot.
        bal.revert_block(&block);
        assert_eq!(bal.erc20_balance(TOK, A), U256::from(50u64));
        assert_eq!(bal.erc20_balance(TOK, B), U256::ZERO);
        assert_eq!(bal.erc20_balance(TOK, C), U256::ZERO);
        assert_eq!(bal.erc721_owner(TOK, U256::from(1u64)), Some(A));
        assert_eq!(bal.entry_count(), snapshot.entry_count());
    }
}
