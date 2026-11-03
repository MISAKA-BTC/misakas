//! Read-only native accepted-material data lookup. Selected-chain membership,
//! header/body identity and the exact acceptance ordinal are cross-checked.
//! This is DATA ONLY: no accepted Case, qualified spend, UTXO, court, payment
//! or credit capability is issued, and no native object tag is activated.
use crate::model::stores::{
    acceptance_data::{AcceptanceDataStoreReader, DbAcceptanceDataStore},
    block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore},
    headers::{DbHeadersStore, HeaderStoreReader},
    selected_chain::{DbSelectedChainStore, SelectedChainStoreReader},
};
use kaspa_consensus_core::{
    BlockHash, Hash64,
    config::params::Params,
    hashing,
    header::Header,
    merkle::{calc_hash_merkle_root, calc_hash_merkle_root_pre_crescendo},
    palw_native_resource_v1::reserve_native_requests_v1,
    tx::Transaction,
};
use kaspa_database::prelude::StoreError;
use misaka_palw_positive_contract::rows::AcceptedOrigin;
use parking_lot::RwLockReadGuard;
use std::{
    io::{self, Write},
    mem::size_of,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveJournalAcceptedReadLimitsV2 {
    pub max_mergeset: u64,
    pub max_accepted: u64,
    pub max_carrier_txs: u64,
    /// Bounds each header and the sum of canonical transaction Borsh bytes in
    /// the carrier body; includes cached mass/id in the existing native codec.
    pub max_canonical_bytes: u64,
    pub max_merkle_workspace_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositiveJournalAcceptedInvalidV2 {
    SelectedPoint,
    HeaderIdentity,
    AcceptedClock,
    AcceptedOrdinal,
    CarrierIdentity,
    BodyIndex,
    TransactionIdentity,
    MerkleRoot,
    CanonicalBytes,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositiveJournalAcceptedBudgetV2 {
    Limits,
    Mergeset,
    Accepted,
    CarrierTransactions,
    CanonicalBytes,
    MerkleWorkspace,
}
#[derive(Debug)]
pub enum PositiveJournalAcceptedReadErrorV2 {
    SourceUnavailable(StoreError),
    Invalid(PositiveJournalAcceptedInvalidV2),
    LocalBudget(PositiveJournalAcceptedBudgetV2),
}
#[derive(Debug)]
pub enum PositiveJournalAcceptedVisitErrorV2<E> {
    Read(PositiveJournalAcceptedReadErrorV2),
    Visitor(E),
}
type ReadResult<T> = Result<T, PositiveJournalAcceptedReadErrorV2>;
fn invalid(reason: PositiveJournalAcceptedInvalidV2) -> PositiveJournalAcceptedReadErrorV2 {
    PositiveJournalAcceptedReadErrorV2::Invalid(reason)
}
fn budget(reason: PositiveJournalAcceptedBudgetV2) -> PositiveJournalAcceptedReadErrorV2 {
    PositiveJournalAcceptedReadErrorV2::LocalBudget(reason)
}

/// Caller owns the actual PALW/pruning/read leases and budgets all cache intake,
/// header/acceptance/body ownership BEFORE constructing/using this view. Store
/// loaders may allocate on a miss. This reader reserves ONLY its additional
/// Merkle vectors. Callback cloning/retention and native decoding need their
/// own environment reservation. Callbacks are read-only, including cached mass.
/// The borrowed selected-chain read guard must remain held for this entire
/// lifetime, preventing a selected-chain ABA while tip freshness is checked.
pub struct PositiveJournalAcceptedReaderV2<'a, 'guard> {
    headers: &'a DbHeadersStore,
    acceptance: &'a DbAcceptanceDataStore,
    transactions: &'a DbBlockTransactionsStore,
    selected: &'a RwLockReadGuard<'guard, DbSelectedChainStore>,
    params: &'a Params,
    limits: PositiveJournalAcceptedReadLimitsV2,
    tip: (u64, BlockHash),
}
impl<'a, 'guard> PositiveJournalAcceptedReaderV2<'a, 'guard> {
    pub fn new(
        headers: &'a DbHeadersStore,
        acceptance: &'a DbAcceptanceDataStore,
        transactions: &'a DbBlockTransactionsStore,
        selected: &'a RwLockReadGuard<'guard, DbSelectedChainStore>,
        params: &'a Params,
        limits: PositiveJournalAcceptedReadLimitsV2,
    ) -> ReadResult<Self> {
        if [
            limits.max_mergeset,
            limits.max_accepted,
            limits.max_carrier_txs,
            limits.max_canonical_bytes,
            limits.max_merkle_workspace_bytes,
        ]
        .contains(&0)
        {
            return Err(budget(PositiveJournalAcceptedBudgetV2::Limits));
        }
        let tip = selected.get_tip().map_err(PositiveJournalAcceptedReadErrorV2::SourceUnavailable)?;
        Ok(Self { headers, acceptance, transactions, selected, params, limits, tip })
    }
    pub fn pinned_tip(&self) -> (u64, BlockHash) {
        self.tip
    }
    pub fn require_fresh_tip(&self) -> ReadResult<()> {
        if self.selected.get_tip().map_err(PositiveJournalAcceptedReadErrorV2::SourceUnavailable)? != self.tip {
            return Err(invalid(PositiveJournalAcceptedInvalidV2::SelectedPoint));
        }
        Ok(())
    }
    pub fn with_transaction<T, E>(
        &self,
        origin: AcceptedOrigin,
        visit: impl FnOnce(&Transaction) -> Result<T, E>,
    ) -> Result<T, PositiveJournalAcceptedVisitErrorV2<E>> {
        let read = PositiveJournalAcceptedVisitErrorV2::Read;
        self.require_fresh_tip().map_err(read)?;
        let accepting = BlockHash::from_bytes(origin.accepting_block.0);
        let carrier = BlockHash::from_bytes(origin.carrier_block.0);
        let index =
            self.selected.get_by_hash(accepting).map_err(|e| read(PositiveJournalAcceptedReadErrorV2::SourceUnavailable(e)))?;
        let selected_hash =
            self.selected.get_by_index(index).map_err(|e| read(PositiveJournalAcceptedReadErrorV2::SourceUnavailable(e)))?;
        if index > self.tip.0 || selected_hash != accepting {
            return Err(read(invalid(PositiveJournalAcceptedInvalidV2::SelectedPoint)));
        }
        let accepting_header =
            self.headers.get_header(accepting).map_err(|e| read(PositiveJournalAcceptedReadErrorV2::SourceUnavailable(e)))?;
        self.require_header_identity(&accepting_header, accepting).map_err(read)?;
        if accepting_header.daa_score != origin.accepted_daa || accepting_header.blue_score != origin.accepted_blue_score {
            return Err(read(invalid(PositiveJournalAcceptedInvalidV2::AcceptedClock)));
        }
        let acceptance = self.acceptance.get(accepting).map_err(|e| read(PositiveJournalAcceptedReadErrorV2::SourceUnavailable(e)))?;
        if acceptance.len() as u64 > self.limits.max_mergeset {
            return Err(read(budget(PositiveJournalAcceptedBudgetV2::Mergeset)));
        }
        let mut total = 0u64;
        for block in acceptance.iter() {
            total = total
                .checked_add(block.accepted_transactions.len() as u64)
                .filter(|&n| n <= self.limits.max_accepted)
                .ok_or_else(|| read(budget(PositiveJournalAcceptedBudgetV2::Accepted)))?;
        }
        if u64::from(origin.accepted_tx_ordinal) >= total {
            return Err(read(invalid(PositiveJournalAcceptedInvalidV2::AcceptedOrdinal)));
        }
        // Include EVERY accepted transaction, including coinbase; neither an
        // empty mergeset nor missing entries can be silently skipped.
        let mut ordinal = u64::from(origin.accepted_tx_ordinal);
        let mut found = None;
        for block in acceptance.iter() {
            let length = block.accepted_transactions.len() as u64;
            if ordinal < length {
                found = Some((block.block_hash, &block.accepted_transactions[ordinal as usize]));
                break;
            }
            ordinal -= length;
        }
        let (accepted_carrier, entry) = found.ok_or_else(|| read(invalid(PositiveJournalAcceptedInvalidV2::AcceptedOrdinal)))?;
        if accepted_carrier != carrier || entry.transaction_id.as_bytes() != origin.carrier_transaction.0 {
            return Err(read(invalid(PositiveJournalAcceptedInvalidV2::CarrierIdentity)));
        }
        let carrier_header =
            self.headers.get_header(carrier).map_err(|e| read(PositiveJournalAcceptedReadErrorV2::SourceUnavailable(e)))?;
        self.require_header_identity(&carrier_header, carrier).map_err(read)?;
        let body = self.transactions.get(carrier).map_err(|e| read(PositiveJournalAcceptedReadErrorV2::SourceUnavailable(e)))?;
        if body.is_empty() || body.len() as u64 > self.limits.max_carrier_txs {
            return Err(read(budget(PositiveJournalAcceptedBudgetV2::CarrierTransactions)));
        }
        let mut canonical = CountWriter { bytes: 0, limit: self.limits.max_canonical_bytes };
        for tx in body.iter() {
            borsh::to_writer(&mut canonical, tx).map_err(|_| read(budget(PositiveJournalAcceptedBudgetV2::CanonicalBytes)))?;
        }
        let workspace = positive_journal_merkle_workspace_bytes_v2(body.len() as u64).map_err(read)?;
        if workspace > self.limits.max_merkle_workspace_bytes {
            return Err(read(budget(PositiveJournalAcceptedBudgetV2::MerkleWorkspace)));
        }
        let _workspace_lease =
            reserve_native_requests_v1(workspace).map_err(|_| read(budget(PositiveJournalAcceptedBudgetV2::MerkleWorkspace)))?;
        // Native fork choice is determined by the carrier's own DAA, never by
        // the accepting block, current tip, or accepting either root variant.
        let root = if self.params.crescendo_activation.is_active(carrier_header.daa_score) {
            calc_hash_merkle_root(body.iter())
        } else {
            calc_hash_merkle_root_pre_crescendo(body.iter())
        };
        if root != carrier_header.hash_merkle_root {
            return Err(read(invalid(PositiveJournalAcceptedInvalidV2::MerkleRoot)));
        }
        let tx =
            body.get(entry.index_within_block as usize).ok_or_else(|| read(invalid(PositiveJournalAcceptedInvalidV2::BodyIndex)))?;
        if tx.id() != entry.transaction_id || !tx.id_is_current() {
            return Err(read(invalid(PositiveJournalAcceptedInvalidV2::TransactionIdentity)));
        }
        let result = visit(tx);
        // Even a callback error cannot suppress the final selected-tip check.
        self.require_fresh_tip().map_err(read)?;
        result.map_err(PositiveJournalAcceptedVisitErrorV2::Visitor)
    }
    fn require_header_identity(&self, header: &Header, expected: BlockHash) -> ReadResult<()> {
        let mut count = CountWriter { bytes: 0, limit: self.limits.max_canonical_bytes };
        borsh::to_writer(&mut count, header).map_err(|_| budget(PositiveJournalAcceptedBudgetV2::CanonicalBytes))?;
        if header.hash != expected || hashing::header::hash(header) != expected {
            return Err(invalid(PositiveJournalAcceptedInvalidV2::HeaderIdentity));
        }
        Ok(())
    }
    /// Compare the existing native transaction's entire canonical Borsh stream,
    /// including cached mass and id. No peer Transaction deserialization or
    /// whole-transaction output Vec is created. Qualification remains A5 work.
    pub fn require_canonical_transaction(&self, origin: AcceptedOrigin, expected_raw: &[u8]) -> ReadResult<()> {
        if expected_raw.len() as u64 > self.limits.max_canonical_bytes {
            return Err(budget(PositiveJournalAcceptedBudgetV2::CanonicalBytes));
        }
        let result = self.with_transaction(origin, |tx| {
            let mut compare = CompareWriter { remaining: expected_raw };
            borsh::to_writer(&mut compare, tx).map_err(|_| invalid(PositiveJournalAcceptedInvalidV2::CanonicalBytes))?;
            if !compare.remaining.is_empty() {
                return Err(invalid(PositiveJournalAcceptedInvalidV2::CanonicalBytes));
            }
            Ok::<_, PositiveJournalAcceptedReadErrorV2>(())
        });
        match result {
            Ok(()) => Ok(()),
            Err(PositiveJournalAcceptedVisitErrorV2::Read(error) | PositiveJournalAcceptedVisitErrorV2::Visitor(error)) => Err(error),
        }
    }
}

/// Pinned existing native Merkle implementation allocates N Hash64 leaves and
/// `(2*next_power_of_two(N)-1)` Option<Hash64> tree slots concurrently. Include
/// both Vec owners too. All arithmetic precedes allocation AND transaction hash.
pub fn positive_journal_merkle_workspace_bytes_v2(count: u64) -> ReadResult<u64> {
    let n = usize::try_from(count).map_err(|_| budget(PositiveJournalAcceptedBudgetV2::MerkleWorkspace))?;
    if n == 0 {
        return Err(budget(PositiveJournalAcceptedBudgetV2::CarrierTransactions));
    }
    let nodes = n
        .checked_next_power_of_two()
        .and_then(|p| p.checked_mul(2))
        .and_then(|p| p.checked_sub(1))
        .ok_or_else(|| budget(PositiveJournalAcceptedBudgetV2::MerkleWorkspace))?;
    n.checked_mul(size_of::<Hash64>())
        .and_then(|b| b.checked_add(nodes.checked_mul(size_of::<Option<Hash64>>())?))
        .and_then(|b| b.checked_add(2 * size_of::<Vec<Hash64>>()))
        .and_then(|b| u64::try_from(b).ok())
        .ok_or_else(|| budget(PositiveJournalAcceptedBudgetV2::MerkleWorkspace))
}
struct CountWriter {
    bytes: u64,
    limit: u64,
}
impl Write for CountWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .filter(|&n| n <= self.limit)
            .ok_or_else(|| io::Error::from(io::ErrorKind::OutOfMemory))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct CompareWriter<'a> {
    remaining: &'a [u8],
}
impl Write for CompareWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.remaining.get(..bytes.len()) != Some(bytes) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.remaining = &self.remaining[bytes.len()..];
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::stores::{
        acceptance_data::AcceptanceDataStore, block_transactions::BlockTransactionsStore, headers::HeaderStore,
        selected_chain::SelectedChainStore,
    };
    use kaspa_consensus_core::{
        acceptance_data::{AcceptedTxEntry, MergesetBlockAcceptanceData},
        config::params::MAINNET_PARAMS,
    };
    use kaspa_database::{create_temp_db, prelude::*};
    use misaka_palw_positive_contract::rows::NativeHash64;
    use parking_lot::RwLock;
    use std::sync::Arc;

    fn limits() -> PositiveJournalAcceptedReadLimitsV2 {
        PositiveJournalAcceptedReadLimitsV2 {
            max_mergeset: 8,
            max_accepted: 16,
            max_carrier_txs: 16,
            max_canonical_bytes: 65_536,
            max_merkle_workspace_bytes: 65_536,
        }
    }
    fn header(root: Hash64, daa: u64) -> Header {
        Header::new_finalized(
            0,
            Default::default(),
            root,
            Default::default(),
            Default::default(),
            1,
            1,
            0,
            1,
            daa,
            Default::default(),
            daa + 1,
            Default::default(),
        )
    }
    fn fixture(
        db: Arc<DB>,
        bad_root: bool,
        stale_id: bool,
    ) -> (DbHeadersStore, DbAcceptanceDataStore, DbBlockTransactionsStore, RwLock<DbSelectedChainStore>, AcceptedOrigin, Vec<u8>) {
        let headers = DbHeadersStore::new(db.clone(), CachePolicy::Count(4), CachePolicy::Count(4));
        let acceptance = DbAcceptanceDataStore::new(db.clone(), CachePolicy::Count(4));
        let bodies = DbBlockTransactionsStore::new(db.clone(), CachePolicy::Count(4));
        let mut tx = Transaction::new(0, vec![], vec![], 0, kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE, 0, vec![1, 2, 3]);
        if stale_id {
            tx.payload.push(4);
        }
        let raw = borsh::to_vec(&tx).unwrap();
        let coinbase = Transaction::new(0, vec![], vec![], 0, kaspa_consensus_core::subnets::SUBNETWORK_ID_COINBASE, 0, vec![0]);
        let txs = vec![coinbase, tx];
        let root = calc_hash_merkle_root_pre_crescendo(txs.iter());
        let carrier = header(if bad_root { Hash64::from_u64_word(99) } else { root }, 1);
        let accepting = header(Default::default(), 2);
        let origin = AcceptedOrigin {
            accepting_block: NativeHash64(accepting.hash.as_bytes()),
            carrier_block: NativeHash64(carrier.hash.as_bytes()),
            carrier_transaction: NativeHash64(txs[1].id().as_bytes()),
            accepted_tx_ordinal: 1,
            event_ordinal: 0,
            accepted_daa: accepting.daa_score,
            accepted_blue_score: accepting.blue_score,
        };
        acceptance
            .insert(
                accepting.hash,
                Arc::new(vec![MergesetBlockAcceptanceData {
                    block_hash: carrier.hash,
                    accepted_transactions: vec![
                        AcceptedTxEntry { transaction_id: txs[0].id(), index_within_block: 0 },
                        AcceptedTxEntry { transaction_id: txs[1].id(), index_within_block: 1 },
                    ],
                }]),
            )
            .unwrap();
        bodies.insert(carrier.hash, txs.into()).unwrap();
        headers.insert(carrier.hash, Arc::new(carrier), 0).unwrap();
        let selected = RwLock::new(DbSelectedChainStore::new(db.clone(), CachePolicy::Count(4)));
        let mut batch = rocksdb::WriteBatch::default();
        selected.write().init_with_pruning_point(&mut batch, accepting.hash).unwrap();
        db.write(batch).unwrap();
        headers.insert(accepting.hash, Arc::new(accepting), 0).unwrap();
        (headers, acceptance, bodies, selected, origin, raw)
    }
    #[test]
    fn merkle_workspace_checks_actual_typed_slots_and_overflow() {
        for n in 1..=17u64 {
            let nodes = 2 * n.next_power_of_two() - 1;
            assert_eq!(
                positive_journal_merkle_workspace_bytes_v2(n).unwrap(),
                n * size_of::<Hash64>() as u64 + nodes * size_of::<Option<Hash64>>() as u64 + 2 * size_of::<Vec<Hash64>>() as u64
            );
        }
        assert!(positive_journal_merkle_workspace_bytes_v2(0).is_err());
        assert!(positive_journal_merkle_workspace_bytes_v2(u64::MAX).is_err());
    }
    #[test]
    fn canonical_compare_stream_checks_truncation_trailing_and_cached_fields() {
        let tx = Transaction::new(0, vec![], vec![], 0, kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE, 0, vec![1, 2, 3]);
        let bytes = borsh::to_vec(&tx).unwrap();
        let mut compare = CompareWriter { remaining: &bytes };
        borsh::to_writer(&mut compare, &tx).unwrap();
        assert!(compare.remaining.is_empty());
        let mut compare = CompareWriter { remaining: &bytes[..bytes.len() - 1] };
        assert!(borsh::to_writer(&mut compare, &tx).is_err());
        let mut extra = bytes.clone();
        extra.push(0);
        let mut compare = CompareWriter { remaining: &extra };
        borsh::to_writer(&mut compare, &tx).unwrap();
        assert!(!compare.remaining.is_empty());
        tx.set_mass(100);
        let mut compare = CompareWriter { remaining: &bytes };
        assert!(borsh::to_writer(&mut compare, &tx).is_err());
        let mut count = CountWriter { bytes: 0, limit: (bytes.len() - 1) as u64 };
        assert!(borsh::to_writer(&mut count, &tx).is_err());
    }

    #[test]
    fn real_db_exact_accepted_ordinal_includes_coinbase_and_raw_eof_is_strict() {
        let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let (headers, acceptance, bodies, selected, origin, raw) = fixture(db, false, false);
        let guard = selected.read();
        let reader = PositiveJournalAcceptedReaderV2::new(&headers, &acceptance, &bodies, &guard, &MAINNET_PARAMS, limits()).unwrap();
        reader.require_canonical_transaction(origin, &raw).unwrap();
        assert!(reader.require_canonical_transaction(origin, &raw[..raw.len() - 1]).is_err());
        let mut extra = raw.clone();
        extra.push(0);
        assert!(reader.require_canonical_transaction(origin, &extra).is_err());
        let mut wrong = origin;
        wrong.accepted_tx_ordinal = 0;
        assert!(matches!(
            reader.with_transaction(wrong, |_| Ok::<_, ()>(())),
            Err(PositiveJournalAcceptedVisitErrorV2::Read(PositiveJournalAcceptedReadErrorV2::Invalid(
                PositiveJournalAcceptedInvalidV2::CarrierIdentity
            )))
        ));
        wrong = origin;
        wrong.accepted_daa += 1;
        assert!(matches!(
            reader.with_transaction(wrong, |_| Ok::<_, ()>(())),
            Err(PositiveJournalAcceptedVisitErrorV2::Read(PositiveJournalAcceptedReadErrorV2::Invalid(
                PositiveJournalAcceptedInvalidV2::AcceptedClock
            )))
        ));
        wrong = origin;
        wrong.accepted_tx_ordinal = 2;
        assert!(reader.with_transaction(wrong, |_| Ok::<_, ()>(())).is_err());
        let reader = PositiveJournalAcceptedReaderV2::new(
            &headers,
            &acceptance,
            &bodies,
            &guard,
            &MAINNET_PARAMS,
            PositiveJournalAcceptedReadLimitsV2 { max_merkle_workspace_bytes: 1, ..limits() },
        )
        .unwrap();
        assert!(matches!(
            reader.with_transaction(origin, |_| Ok::<_, ()>(())),
            Err(PositiveJournalAcceptedVisitErrorV2::Read(PositiveJournalAcceptedReadErrorV2::LocalBudget(
                PositiveJournalAcceptedBudgetV2::MerkleWorkspace
            )))
        ));
    }
    #[test]
    fn real_db_bad_merkle_and_stale_id_never_reach_callback() {
        for (bad_root, stale_id, expected) in [
            (true, false, PositiveJournalAcceptedInvalidV2::MerkleRoot),
            (false, true, PositiveJournalAcceptedInvalidV2::TransactionIdentity),
        ] {
            let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let (headers, acceptance, bodies, selected, origin, _) = fixture(db, bad_root, stale_id);
            let guard = selected.read();
            let reader =
                PositiveJournalAcceptedReaderV2::new(&headers, &acceptance, &bodies, &guard, &MAINNET_PARAMS, limits()).unwrap();
            let mut called = false;
            assert!(matches!(reader.with_transaction(origin, |_| { called = true; Ok::<_, ()>(()) }),
                Err(PositiveJournalAcceptedVisitErrorV2::Read(PositiveJournalAcceptedReadErrorV2::Invalid(actual))) if actual == expected));
            assert!(!called);
        }
    }
    #[test]
    fn real_db_missing_selected_point_or_body_never_uses_an_empty_fallback() {
        let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let (headers, acceptance, bodies, selected, origin, _) = fixture(db, false, false);
        let guard = selected.read();
        let reader = PositiveJournalAcceptedReaderV2::new(&headers, &acceptance, &bodies, &guard, &MAINNET_PARAMS, limits()).unwrap();
        let mut wrong = origin;
        wrong.accepting_block = NativeHash64([99; 64]);
        assert!(matches!(
            reader.with_transaction(wrong, |_| Ok::<_, ()>(())),
            Err(PositiveJournalAcceptedVisitErrorV2::Read(PositiveJournalAcceptedReadErrorV2::SourceUnavailable(_)))
        ));
        bodies.delete(BlockHash::from_bytes(origin.carrier_block.0)).unwrap();
        assert!(matches!(
            reader.with_transaction(origin, |_| Ok::<_, ()>(())),
            Err(PositiveJournalAcceptedVisitErrorV2::Read(PositiveJournalAcceptedReadErrorV2::SourceUnavailable(_)))
        ));
    }
}
