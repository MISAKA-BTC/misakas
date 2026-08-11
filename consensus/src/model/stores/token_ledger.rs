//! MISAKA Compute Token Program — the Phase A store family
//! (`docs/misaka-compute-token-program-design-v0.1.md` §9.1).
//!
//! One [`DbTokenStore`] bundles the three keyspaces the design names, because
//! they live or die together (one schema version, one reindex):
//!
//! * **ledger** — `(asset_id, owner) → TokenAccount {balance, nonce}` (§4.2).
//! * **supply** — `asset_id → TokenSupply {minted, burned}`, the anchors of the
//!   §4.2 conservation invariant `Σ balance == minted − burned`.
//! * **settlements** — `epoch → TokenEmissionSettlement`, the write-once record
//!   of one epoch's emission (§5). Settlement is computed from the *finalized*
//!   VLT credit rows ([`super::vlt_credits`]), so a settlement row is
//!   branch-invariant for the same reason those are: the epoch it describes is
//!   buried past the challenge window and the reorg horizon, and no branch can
//!   still disagree about it. That is why this store needs no undo machinery.
//!
//! The **ledger** rows are different: transfers and burns apply at acceptance
//! on the live chain, which a reorg can rewind. The rollback strategy
//! (per-block token diffs alongside `utxo_diffs`) is part of the processor
//! wiring (design §9.5 step 2/3), NOT of this file — Phase A PR 1 ships the
//! store primitives only, and nothing writes them while every preset's token
//! fence is `u64::MAX` ([`kaspa_consensus_core::token::TokenParams::INERT`]).
//!
//! Values are count-estimable only, so every access uses an untracked
//! (`Count`) cache policy — never `tracked_bytes` (see [`super::vlt_credits`]).

use std::fmt::Display;
use std::sync::Arc;

use parking_lot::RwLock;

use kaspa_consensus_core::token::{TokenAccount, TokenEmissionSettlement, TokenMintMeta, TokenSupply};
use kaspa_core::info;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash64;
use rocksdb::WriteBatch;

use super::U64Key;

/// The rules the token rows were written under.
///
/// Mirrors [`super::vlt_credits::VLT_CREDITS_SCHEMA_VERSION`]: the settlement
/// rows are derived AND write-once, and the ledger rows are the running result
/// of every application rule ever in force — for both, a rules change must
/// discard and rebuild rather than read old rows as final. Bump on any change
/// to the borsh layouts or to what application/settlement would have produced
/// for history already recorded.
///
/// * 1 — original (Phase A).
/// * 2 — audit-emission v0.2: settlements grew `audit_paid` (borsh layout change), and the
///   settlement values themselves changed shape (one budget now pays exec + audit work), so
///   rows settled under rule 1 record a distribution this build would not produce.
/// * 3 — Phase B: `MintTo` became atomic. Under rule 2 a cap-breaching issuance staged its
///   nonce bump before the cap check and kept it, so a ledger built under those rules records
///   nonces that this build would not have consumed — and every later mint on that asset
///   diverges. Rebuilt from the chain.
pub const TOKEN_LEDGER_SCHEMA_VERSION: u32 = 3;

/// `(asset_id, owner)` as a fixed-width DB key: 8 LE bytes of asset id, then
/// the 64-byte overlay owner id. Asset-major, so a prefix iteration walks one
/// asset's holders contiguously (the §4.2 conservation check is per asset).
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct TokenLedgerKey([u8; 8 + 64]);

impl From<(u64, Hash64)> for TokenLedgerKey {
    fn from((asset_id, owner): (u64, Hash64)) -> Self {
        let mut bytes = [0u8; 72];
        bytes[..8].copy_from_slice(&asset_id.to_le_bytes());
        bytes[8..].copy_from_slice(owner.as_byte_slice());
        Self(bytes)
    }
}

impl AsRef<[u8]> for TokenLedgerKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for TokenLedgerKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let asset = u64::from_le_bytes(self.0[..8].try_into().expect("8 bytes"));
        let owner = Hash64::from_bytes(self.0[8..].try_into().expect("64 bytes"));
        write!(f, "token:{asset}:{owner}")
    }
}

/// The Phase A token store family — see the module docs for what each keyspace
/// holds and which of them need (and do not need) reorg handling.
pub struct DbTokenStore {
    db: Arc<DB>,
    ledger: CachedDbAccess<TokenLedgerKey, TokenAccount>,
    supply: CachedDbAccess<U64Key, TokenSupply>,
    settlements: CachedDbAccess<U64Key, TokenEmissionSettlement>,
    /// Phase B: `asset_id → TokenMintMeta`, written once by the first accepted CreateMint.
    mint_metas: CachedDbAccess<U64Key, TokenMintMeta>,
    version: CachedDbItem<u32>,
    /// Next selected-chain index the ledger fold processes (design §9.2).
    /// `RwLock` because [`CachedDbItem::write`] needs `&mut` and this store is
    /// shared as a bare `Arc` — the lock is cursor-local so ledger reads stay
    /// lock-free.
    fold_cursor: RwLock<CachedDbItem<u64>>,
    /// Next epoch emission settlement considers (design §5.3). Same locking
    /// story as [`Self::fold_cursor`].
    settlement_cursor: RwLock<CachedDbItem<u64>>,
}

impl DbTokenStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            ledger: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::TokenLedger.into()),
            supply: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::TokenSupply.into()),
            settlements: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::TokenEmissionSettlements.into()),
            mint_metas: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::TokenMintMetas.into()),
            version: CachedDbItem::new(Arc::clone(&db), DatabaseStorePrefixes::TokenLedgerSchemaVersion.into()),
            fold_cursor: RwLock::new(CachedDbItem::new(Arc::clone(&db), DatabaseStorePrefixes::TokenLedgerFoldCursor.into())),
            settlement_cursor: RwLock::new(CachedDbItem::new(db, DatabaseStorePrefixes::TokenSettlementCursor.into())),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// Drop every token row not written under the current rules, so the family
    /// is rebuilt from the chain (design §9.1; same escape hatch as
    /// [`super::vlt_credits::DbVltCreditStore::reindex_if_stale`], and the same
    /// caveats: the overlay transactions and credit rows everything here is
    /// derived from are untouched).
    ///
    /// Called once at startup. An absent marker with rows present is version 1
    /// by definition; an absent marker with no rows is a fresh database.
    pub fn reindex_if_stale(&mut self) -> Result<(), StoreError> {
        let stored = match self.version.read() {
            Ok(v) => Some(v),
            Err(StoreError::KeyNotFound(_)) => None,
            Err(e) => return Err(e),
        };
        if stored == Some(TOKEN_LEDGER_SCHEMA_VERSION) {
            return Ok(());
        }
        let had_rows = self.ledger.iterator().next().is_some()
            || self.supply.iterator().next().is_some()
            || self.settlements.iterator().next().is_some();
        if had_rows {
            info!(
                "[token] store rows were written under rules v{} and this build writes v{TOKEN_LEDGER_SCHEMA_VERSION}; \
                 discarding the ledger/supply/settlement rows so they are recomputed from the chain \
                 (no blocks or overlay transactions are affected)",
                stored.unwrap_or(1)
            );
            self.ledger.delete_all(DirectDbWriter::new(&self.db))?;
            self.supply.delete_all(DirectDbWriter::new(&self.db))?;
            self.settlements.delete_all(DirectDbWriter::new(&self.db))?;
            self.mint_metas.delete_all(DirectDbWriter::new(&self.db))?;
            // The cursors describe how far the discarded rows reached; resetting them is what
            // makes the next commit rebuild rather than resume past the wiped span.
            self.fold_cursor.write().write(DirectDbWriter::new(&self.db), &0)?;
            self.settlement_cursor.write().write(DirectDbWriter::new(&self.db), &0)?;
        }
        self.version.write(DirectDbWriter::new(&self.db), &TOKEN_LEDGER_SCHEMA_VERSION)
    }

    // ---- cursors --------------------------------------------------------

    /// The next selected-chain index the ledger fold should process, or `None`
    /// before the fold has ever run (the caller lazily initializes it to the
    /// first chain index past the shadow fence — design §9.2).
    pub fn fold_cursor(&self) -> Result<Option<u64>, StoreError> {
        match self.fold_cursor.read().read() {
            Ok(v) => Ok(Some(v)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_fold_cursor_batch(&self, batch: &mut WriteBatch, next_index: u64) -> Result<(), StoreError> {
        self.fold_cursor.write().write(BatchDbWriter::new(batch), &next_index)
    }

    /// The next epoch settlement should consider, or `None` before the first
    /// settlement pass (lazily initialized to `emission_activation_epoch`).
    pub fn settlement_cursor(&self) -> Result<Option<u64>, StoreError> {
        match self.settlement_cursor.read().read() {
            Ok(v) => Ok(Some(v)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_settlement_cursor_batch(&self, batch: &mut WriteBatch, next_epoch: u64) -> Result<(), StoreError> {
        self.settlement_cursor.write().write(BatchDbWriter::new(batch), &next_epoch)
    }

    // ---- ledger ---------------------------------------------------------

    /// The `(asset, owner)` row, or the default (zero balance, zero nonce) if
    /// absent — an absent row IS the empty account (design §4.2: no
    /// account-creation step).
    pub fn get_account(&self, asset_id: u64, owner: Hash64) -> Result<TokenAccount, StoreError> {
        match self.ledger.read((asset_id, owner).into()) {
            Ok(account) => Ok(account),
            Err(StoreError::KeyNotFound(_)) => Ok(TokenAccount::default()),
            Err(e) => Err(e),
        }
    }

    /// Persist a row into `batch`. The caller applies token semantics
    /// ([`kaspa_consensus_core::token::apply_token_transfer`] /
    /// [`apply_token_burn`]) and writes both touched rows in ONE batch or
    /// neither — a half-applied transfer is a conservation violation.
    ///
    /// [`apply_token_burn`]: kaspa_consensus_core::token::apply_token_burn
    pub fn set_account_batch(
        &self,
        batch: &mut WriteBatch,
        asset_id: u64,
        owner: Hash64,
        account: TokenAccount,
    ) -> Result<(), StoreError> {
        self.ledger.write(BatchDbWriter::new(batch), (asset_id, owner).into(), account)
    }

    // ---- supply ---------------------------------------------------------

    /// The asset's supply counters, defaulting to zero for an asset with no
    /// history (for Phase A that is every asset but [`TOK_ASSET_ID`], and TOK
    /// too until first settlement).
    pub fn get_supply(&self, asset_id: u64) -> Result<TokenSupply, StoreError> {
        match self.supply.read(asset_id.into()) {
            Ok(supply) => Ok(supply),
            Err(StoreError::KeyNotFound(_)) => Ok(TokenSupply::default()),
            Err(e) => Err(e),
        }
    }

    pub fn set_supply_batch(&self, batch: &mut WriteBatch, asset_id: u64, supply: TokenSupply) -> Result<(), StoreError> {
        self.supply.write(BatchDbWriter::new(batch), asset_id.into(), supply)
    }

    // ---- mint metas (Phase B) ------------------------------------------

    /// The asset's immutable mint policy, or `None` for an asset no accepted
    /// `CreateMint` has claimed (and always `None` for TOK, whose only issuance
    /// is emission).
    pub fn get_mint_meta(&self, asset_id: u64) -> Result<Option<TokenMintMeta>, StoreError> {
        match self.mint_metas.read(asset_id.into()) {
            Ok(meta) => Ok(Some(meta)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Write-once by contract: the caller only stages a meta for an asset
    /// [`Self::get_mint_meta`] answered `None` for within the same fold pass.
    pub fn set_mint_meta_batch(&self, batch: &mut WriteBatch, asset_id: u64, meta: TokenMintMeta) -> Result<(), StoreError> {
        self.mint_metas.write(BatchDbWriter::new(batch), asset_id.into(), meta)
    }

    // ---- emission settlements ------------------------------------------

    /// `epoch`'s settled emission, or `KeyNotFound` if it has not settled
    /// (every epoch while the token fence is inert).
    pub fn get_settlement(&self, epoch: u64) -> Result<TokenEmissionSettlement, StoreError> {
        self.settlements.read(epoch.into())
    }

    /// Whether `epoch` already settled — the idempotence gate the wiring
    /// consults before re-running settlement at a virtual commit (design §9.2).
    pub fn has_settlement(&self, epoch: u64) -> Result<bool, StoreError> {
        self.settlements.has(epoch.into())
    }

    /// Persist an epoch's settlement into `batch`, together with the ledger
    /// credits and supply bump it implies — one batch, one atomic step.
    ///
    /// Write-once per epoch: the caller must only settle an epoch
    /// [`has_settlement`] denies, and only from **finalized** credit rows
    /// ([`kaspa_consensus_core::vlt::vlt_epoch_finalized`]) — a settlement from
    /// a live epoch would mint on a branch, which is exactly what the design's
    /// §5.3 pin exists to make impossible.
    ///
    /// [`has_settlement`]: Self::has_settlement
    pub fn set_settlement_batch(
        &self,
        batch: &mut WriteBatch,
        epoch: u64,
        settlement: TokenEmissionSettlement,
    ) -> Result<(), StoreError> {
        self.settlements.write(BatchDbWriter::new(batch), epoch.into(), settlement)
    }

    // ---- direct (non-batched) writes — tests / diagnostics only ---------

    pub fn set_account(&self, asset_id: u64, owner: Hash64, account: TokenAccount) -> Result<(), StoreError> {
        self.ledger.write(DirectDbWriter::new(&self.db), (asset_id, owner).into(), account)
    }

    pub fn set_supply(&self, asset_id: u64, supply: TokenSupply) -> Result<(), StoreError> {
        self.supply.write(DirectDbWriter::new(&self.db), asset_id.into(), supply)
    }

    pub fn set_settlement(&self, epoch: u64, settlement: TokenEmissionSettlement) -> Result<(), StoreError> {
        self.settlements.write(DirectDbWriter::new(&self.db), epoch.into(), settlement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::token::{TOK_ASSET_ID, TokenEmissionReward};
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;

    fn owner(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    #[test]
    fn ledger_key_is_asset_major_and_collision_free() {
        let a = TokenLedgerKey::from((0, owner(1)));
        let b = TokenLedgerKey::from((0, owner(2)));
        let c = TokenLedgerKey::from((1, owner(1)));
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.as_ref().len(), 72);
        assert_eq!(&a.as_ref()[..8], &TOK_ASSET_ID.to_le_bytes());
        assert_eq!(a.to_string(), format!("token:0:{}", owner(1)));
    }

    /// An absent row IS the empty account: reading it must not error, and a
    /// written row must round-trip byte-exactly.
    #[test]
    fn accounts_default_when_absent_and_roundtrip_when_present() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbTokenStore::new(db, CachePolicy::Count(16));

        assert_eq!(store.get_account(TOK_ASSET_ID, owner(1)).unwrap(), TokenAccount::default());
        assert_eq!(store.get_supply(TOK_ASSET_ID).unwrap(), TokenSupply::default());

        store.set_account(TOK_ASSET_ID, owner(1), TokenAccount { balance: 700, nonce: 3 }).unwrap();
        store.set_supply(TOK_ASSET_ID, TokenSupply { minted: 1_000, burned: 300 }).unwrap();
        assert_eq!(store.get_account(TOK_ASSET_ID, owner(1)).unwrap(), TokenAccount { balance: 700, nonce: 3 });
        assert_eq!(store.get_supply(TOK_ASSET_ID).unwrap().circulating(), 700);
        // A different asset's row for the same owner is a different account.
        assert_eq!(store.get_account(1, owner(1)).unwrap(), TokenAccount::default());
    }

    #[test]
    fn settlements_roundtrip_and_gate_idempotence() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbTokenStore::new(db, CachePolicy::Count(16));

        assert!(!store.has_settlement(9).unwrap());
        let settlement = TokenEmissionSettlement {
            budget: 1_000,
            network_compute: 1_001,
            paid_total: 999,
            rewards: vec![TokenEmissionReward { owner: owner(1), amount: 999 }],
            audit_paid: 0,
        };
        store.set_settlement(9, settlement.clone()).unwrap();
        assert!(store.has_settlement(9).unwrap());
        assert_eq!(store.get_settlement(9).unwrap(), settlement);
    }

    /// The family shares one version marker: a rules bump discards all three
    /// keyspaces together (a ledger rebuilt under new rules against settlements
    /// kept from old ones would break conservation silently).
    #[test]
    fn a_rules_change_discards_the_whole_family() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbTokenStore::new(db.clone(), CachePolicy::Count(16));

        store.reindex_if_stale().unwrap();
        store.set_account(TOK_ASSET_ID, owner(1), TokenAccount { balance: 5, nonce: 1 }).unwrap();
        store.set_supply(TOK_ASSET_ID, TokenSupply { minted: 5, burned: 0 }).unwrap();
        store.set_settlement(2, TokenEmissionSettlement::default()).unwrap();

        // Same rules ⇒ untouched.
        store.reindex_if_stale().unwrap();
        assert_eq!(store.get_account(TOK_ASSET_ID, owner(1)).unwrap().balance, 5);

        // Previous rules ⇒ all three keyspaces dropped, marker advanced.
        store.version.write(DirectDbWriter::new(&db), &(TOKEN_LEDGER_SCHEMA_VERSION - 1)).unwrap();
        let mut reopened = DbTokenStore::new(db, CachePolicy::Count(16));
        reopened.reindex_if_stale().unwrap();
        assert_eq!(reopened.get_account(TOK_ASSET_ID, owner(1)).unwrap(), TokenAccount::default());
        assert_eq!(reopened.get_supply(TOK_ASSET_ID).unwrap(), TokenSupply::default());
        assert!(!reopened.has_settlement(2).unwrap());
        assert_eq!(reopened.version.read().unwrap(), TOKEN_LEDGER_SCHEMA_VERSION);
    }
}
