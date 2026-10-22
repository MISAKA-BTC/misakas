//! **RFC-0036 (Proposed) — row-level PALW state access, dormant.** The seam a later wiring switches backends behind: consensus code
//! that reads rows through [`PalwRowSourceV1`] and writes them through [`PalwRowOverlayV1`] runs unchanged whether the rows come from
//! the full in-memory state ([`PalwFullStateRowSourceV1`], today's behaviour, byte for byte) or from verified, fetched rows of an
//! authenticated map (`crate::palw_state_auth_map_v1`) held by a pruned node.
//!
//! Three rules this layer makes structural:
//! * **Pinned.** Every source reads ONE state, named by its root ([`PalwRowPinV1`]): the parent state of the block being applied.
//! * **"Unavailable" is not "absent".** [`PalwRowReadV1::Unavailable`] is its own answer, and the typed reads return it as `Err`,
//!   never as `Ok(None)`. A caller that cannot fetch a row stops (sync pauses); it never decides the row is not there, and so a local
//!   fetch failure can never make a legal block invalid.
//! * **A cache is not state.** [`PalwCachedRowSourceV1`] keeps verified answers keyed by (root, table, key) under a byte budget and
//!   forgets them freely; it never answers for a root it was not filled from, never caches an unavailable read, and evicting an
//!   entry changes no answer — only where the next one comes from.
//!
//! Nothing in consensus uses this module yet; the table numbering below is the RFC's draft registry.

use crate::Hash64;
use crate::config::params::Params;
use crate::palw_mode_v2::PalwModeV2Error;
use crate::palw_state_auth_map_v1::{
    AUTH_MAP_MAX_KEY_LEN_V1, AUTH_MAP_MAX_VALUE_LEN_V1, AuthMapErrorV1, AuthMapV1, AuthProofLimitsV1, AuthStatePinV1,
    AuthStateRowProofV1, AuthTableSpecV1, KeyRangeV1, RowUpdateV1, VerifiedRowsV1, verify_state_rows_v1,
};
use crate::palw_state_v2::{PalwBondKeyV2, PalwBondStateV2, PalwChainStateV2, PalwClaimStateV2, PalwClassStateV2};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

/// **The tables this layer names** (RFC-0036 draft registry; a subset of the state families — the rest follow the same codecs).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum PalwStateTableV1 {
    Bonds,
    Classes,
    Claims,
    ConsumedOffences,
    CourtSessions,
    PendingPayouts,
    ClassShares,
    RewardPending,
    ReporterRewards,
    ModelLifecycles,
    TirShardClaims,
    /// The kernel route's aux rows: key `table_byte ‖ aux_key`, value the raw row (what `aux-root/v1` hashes).
    KernelRouteAux,
}

impl PalwStateTableV1 {
    pub const ALL: [Self; 12] = [
        Self::Bonds,
        Self::Classes,
        Self::Claims,
        Self::ConsumedOffences,
        Self::CourtSessions,
        Self::PendingPayouts,
        Self::ClassShares,
        Self::RewardPending,
        Self::ReporterRewards,
        Self::ModelLifecycles,
        Self::TirShardClaims,
        Self::KernelRouteAux,
    ];

    pub const fn table_id(self) -> u32 {
        match self {
            Self::Bonds => 1,
            Self::Classes => 2,
            Self::Claims => 3,
            Self::ConsumedOffences => 4,
            Self::CourtSessions => 5,
            Self::PendingPayouts => 6,
            Self::ClassShares => 7,
            Self::RewardPending => 8,
            Self::ReporterRewards => 9,
            Self::ModelLifecycles => 10,
            Self::TirShardClaims => 11,
            Self::KernelRouteAux => 12,
        }
    }

    /// The table's authenticated-map spec: its key length (an outpoint key is a 64-byte txid and a 4-byte big-endian index; every
    /// other typed key a 64-byte hash) and the module's value ceiling.
    pub const fn spec(self) -> AuthTableSpecV1 {
        let key = match self {
            Self::Bonds => 68,
            Self::KernelRouteAux => AUTH_MAP_MAX_KEY_LEN_V1,
            _ => 64,
        };
        AuthTableSpecV1::new(self.table_id(), 1, key, AUTH_MAP_MAX_VALUE_LEN_V1)
    }
}

impl PalwStateTableV1 {
    /// Is `key` a key this table can hold at all (a 68-byte outpoint, a 64-byte hash, or a table byte and an aux key)? Every backend
    /// refuses any other shape the same way ([`PalwRowUnavailableV1::MalformedKey`]) instead of answering it absent.
    pub fn key_shape_ok(self, key: &[u8]) -> bool {
        match self {
            Self::Bonds => key.len() == 68,
            Self::KernelRouteAux => !key.is_empty() && key.len() <= AUTH_MAP_MAX_KEY_LEN_V1 as usize,
            _ => key.len() == 64,
        }
    }
}

/// **The order-preserving key of a bond**: txid then the output index big-endian — bytewise order is `PalwBondKeyV2`'s order.
pub fn palw_bond_row_key_v1(key: &PalwBondKeyV2) -> Vec<u8> {
    let mut k = key.0.transaction_id.as_bytes().to_vec();
    k.extend_from_slice(&key.0.index.to_be_bytes());
    k
}

pub fn palw_bond_of_row_key_v1(bytes: &[u8]) -> Option<PalwBondKeyV2> {
    let (txid, index) = (bytes.get(..64)?, bytes.get(64..)?);
    let index: [u8; 4] = index.try_into().ok()?;
    Some(PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_bytes(txid.try_into().ok()?), u32::from_be_bytes(index))))
}

fn hash_of_row_key(bytes: &[u8]) -> Option<Hash64> {
    Some(Hash64::from_bytes(bytes.try_into().ok()?))
}

/// The state a source reads, by root.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PalwRowPinV1 {
    pub state_root: Hash64,
}

/// Why a row could not be answered. None of these is "absent".
#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
pub enum PalwRowUnavailableV1 {
    #[error("the row is not held locally")]
    NotLocal,
    #[error("no supplier could serve the row")]
    SupplierUnavailable,
    #[error("the supplied row's proof did not verify against the pin")]
    ProofRejected,
    #[error("the key is not a key of this table")]
    MalformedKey,
    #[error("the stored row does not decode")]
    Undecodable,
}

/// One row read.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PalwRowReadV1 {
    Present(Vec<u8>),
    Absent,
    Unavailable(PalwRowUnavailableV1),
}

impl PalwRowReadV1 {
    /// `Ok(Some)` present, `Ok(None)` absent, `Err` unavailable.
    pub fn into_result(self) -> Result<Option<Vec<u8>>, PalwRowUnavailableV1> {
        match self {
            Self::Present(v) => Ok(Some(v)),
            Self::Absent => Ok(None),
            Self::Unavailable(why) => Err(why),
        }
    }
}

/// A range read: the first `limit` rows of the range, complete up to the last one returned; `more` when the limit cut the range.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PalwRowRangeV1 {
    pub rows: Vec<(Vec<u8>, Vec<u8>)>,
    pub more: bool,
}

fn decode<T: borsh::BorshDeserialize>(bytes: &[u8]) -> Result<T, PalwRowUnavailableV1> {
    let mut slice = bytes;
    let v = T::deserialize(&mut slice).map_err(|_| PalwRowUnavailableV1::Undecodable)?;
    slice.is_empty().then_some(v).ok_or(PalwRowUnavailableV1::Undecodable)
}

/// **A parent-root-pinned row source.**
pub trait PalwRowSourceV1 {
    fn pin(&self) -> PalwRowPinV1;
    fn read_row(&self, table: PalwStateTableV1, key: &[u8]) -> PalwRowReadV1;
    fn read_range(&self, table: PalwStateTableV1, range: &KeyRangeV1, limit: usize) -> Result<PalwRowRangeV1, PalwRowUnavailableV1>;

    fn read_typed<T: borsh::BorshDeserialize>(&self, table: PalwStateTableV1, key: &[u8]) -> Result<Option<T>, PalwRowUnavailableV1>
    where
        Self: Sized,
    {
        self.read_row(table, key).into_result()?.map(|v| decode(&v)).transpose()
    }

    fn bond(&self, key: &PalwBondKeyV2) -> Result<Option<PalwBondStateV2>, PalwRowUnavailableV1>
    where
        Self: Sized,
    {
        self.read_typed(PalwStateTableV1::Bonds, &palw_bond_row_key_v1(key))
    }

    fn class(&self, id: &Hash64) -> Result<Option<PalwClassStateV2>, PalwRowUnavailableV1>
    where
        Self: Sized,
    {
        self.read_typed(PalwStateTableV1::Classes, id.as_byte_slice())
    }

    fn claim(&self, id: &Hash64) -> Result<Option<PalwClaimStateV2>, PalwRowUnavailableV1>
    where
        Self: Sized,
    {
        self.read_typed(PalwStateTableV1::Claims, id.as_byte_slice())
    }

    /// Has this offence been consumed? `Err` when unknown — never `Ok(false)` for a row that could not be read.
    fn offence_consumed(&self, offence: &Hash64) -> Result<bool, PalwRowUnavailableV1>
    where
        Self: Sized,
    {
        Ok(self.read_row(PalwStateTableV1::ConsumedOffences, offence.as_byte_slice()).into_result()?.is_some())
    }
}

impl<S: PalwRowSourceV1 + ?Sized> PalwRowSourceV1 for &S {
    fn pin(&self) -> PalwRowPinV1 {
        (**self).pin()
    }
    fn read_row(&self, table: PalwStateTableV1, key: &[u8]) -> PalwRowReadV1 {
        (**self).read_row(table, key)
    }
    fn read_range(&self, table: PalwStateTableV1, range: &KeyRangeV1, limit: usize) -> Result<PalwRowRangeV1, PalwRowUnavailableV1> {
        (**self).read_range(table, range, limit)
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// The full-state adapter

/// **Today's state, read as rows.** A row lookup is a keyed lookup into the in-memory collection; a range is the collection's own
/// ordered iteration, filtered. Values are the borsh bytes the v1 collection root hashes (for the aux table, the raw row).
pub struct PalwFullStateRowSourceV1<'a> {
    state: &'a PalwChainStateV2,
    pin: PalwRowPinV1,
}

impl<'a> PalwFullStateRowSourceV1<'a> {
    /// Pin to the state's own (v1) root — computed once, here.
    pub fn new(state: &'a PalwChainStateV2) -> Self {
        Self { pin: PalwRowPinV1 { state_root: state.state_root() }, state }
    }

    /// Pin to a root the caller already holds for this state.
    pub fn new_pinned(state: &'a PalwChainStateV2, state_root: Hash64) -> Self {
        Self { state, pin: PalwRowPinV1 { state_root } }
    }

    /// Every row of `table` in key order (the authenticated stream's content).
    pub fn table_rows(&self, table: PalwStateTableV1) -> Vec<(Vec<u8>, Vec<u8>)> {
        fn enc<V: borsh::BorshSerialize>(k: &Hash64, v: &V) -> (Vec<u8>, Vec<u8>) {
            (k.as_byte_slice().to_vec(), borsh::to_vec(v).expect("state rows are borsh-serializable"))
        }
        let s = self.state;
        match table {
            PalwStateTableV1::Bonds => s
                .bonds_iter()
                .map(|(k, v)| (palw_bond_row_key_v1(k), borsh::to_vec(v).expect("state rows are borsh-serializable")))
                .collect(),
            PalwStateTableV1::Classes => s.classes_iter().map(|(k, v)| enc(k, v)).collect(),
            PalwStateTableV1::Claims => s.claims_iter().map(|(k, v)| enc(k, v)).collect(),
            PalwStateTableV1::ConsumedOffences => s.consumed_offences_iter().map(|(k, v)| enc(k, v)).collect(),
            PalwStateTableV1::CourtSessions => s.court_sessions_iter().map(|(k, v)| enc(k, v)).collect(),
            PalwStateTableV1::PendingPayouts => s.pending_payouts_iter().map(|(k, v)| enc(k, v)).collect(),
            PalwStateTableV1::ClassShares => s.class_shares_iter().map(|(k, v)| enc(k, v)).collect(),
            PalwStateTableV1::RewardPending => s.reward_pending_iter().map(|(k, v)| enc(k, v)).collect(),
            PalwStateTableV1::ReporterRewards => s.reporter_rewards_iter().map(|(k, v)| enc(k, v)).collect(),
            PalwStateTableV1::ModelLifecycles => s.model_lifecycles_iter().map(|(k, v)| enc(k, v)).collect(),
            PalwStateTableV1::TirShardClaims => s.tir_shard_claims_iter().map(|(k, v)| enc(k, v)).collect(),
            PalwStateTableV1::KernelRouteAux => s
                .kernel_route_state_v1()
                .map(|r| {
                    r.aux
                        .iter()
                        .map(|((t, k), v)| {
                            let mut key = vec![*t];
                            key.extend_from_slice(k);
                            (key, v.clone())
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

impl PalwRowSourceV1 for PalwFullStateRowSourceV1<'_> {
    fn pin(&self) -> PalwRowPinV1 {
        self.pin
    }

    fn read_row(&self, table: PalwStateTableV1, key: &[u8]) -> PalwRowReadV1 {
        fn row<V: borsh::BorshSerialize>(v: Option<&V>) -> PalwRowReadV1 {
            v.map_or(PalwRowReadV1::Absent, |v| PalwRowReadV1::Present(borsh::to_vec(v).expect("state rows are borsh-serializable")))
        }
        let s = self.state;
        let malformed = PalwRowReadV1::Unavailable(PalwRowUnavailableV1::MalformedKey);
        if !table.key_shape_ok(key) {
            return malformed;
        }
        if table == PalwStateTableV1::Bonds {
            return match palw_bond_of_row_key_v1(key) {
                Some(bond) if key.len() == 68 => row(s.bond(&bond)),
                _ => malformed,
            };
        }
        if table == PalwStateTableV1::KernelRouteAux {
            let Some((t, k)) = key.split_first() else { return malformed };
            return match s.kernel_route_state_v1().and_then(|r| r.aux.get(&(*t, k.to_vec()))) {
                Some(v) => PalwRowReadV1::Present(v.clone()),
                None => PalwRowReadV1::Absent,
            };
        }
        let Some(id) = hash_of_row_key(key) else { return malformed };
        match table {
            PalwStateTableV1::Classes => row(s.class(&id)),
            PalwStateTableV1::Claims => row(s.claim(&id)),
            PalwStateTableV1::ConsumedOffences => row(s.consumed_offence(&id)),
            PalwStateTableV1::CourtSessions => row(s.court_session(&id)),
            PalwStateTableV1::PendingPayouts => row(s.pending_payout(&id)),
            // no keyed getter is exposed for the share table: the adapter finds the row (this adapter is a reference, not a backend)
            PalwStateTableV1::ClassShares => row(s.class_shares_iter().find(|(k, _)| **k == id).map(|(_, v)| v)),
            PalwStateTableV1::RewardPending => row(s.reward_pending(&id)),
            PalwStateTableV1::ReporterRewards => row(s.reporter_reward(&id)),
            PalwStateTableV1::ModelLifecycles => row(s.model_lifecycle(&id)),
            PalwStateTableV1::TirShardClaims => row(s.tir_shard_claim(&id)),
            PalwStateTableV1::Bonds | PalwStateTableV1::KernelRouteAux => unreachable!("handled above"),
        }
    }

    fn read_range(&self, table: PalwStateTableV1, range: &KeyRangeV1, limit: usize) -> Result<PalwRowRangeV1, PalwRowUnavailableV1> {
        let mut rows: Vec<_> =
            self.table_rows(table).into_iter().filter(|(k, _)| range.contains(k)).take(limit.saturating_add(1)).collect();
        let more = rows.len() > limit;
        rows.truncate(limit);
        Ok(PalwRowRangeV1 { rows, more })
    }
}

/// **The authenticated map of one table of a source** (read as a complete range). `Err` if any read is unavailable.
pub fn palw_auth_map_of_table_v1<S: PalwRowSourceV1>(source: &S, table: PalwStateTableV1) -> Result<AuthMapV1, PalwRowUnavailableV1> {
    let mut map = AuthMapV1::new(table.spec()).expect("registry specs are in bounds");
    let range = source.read_range(table, &KeyRangeV1::all(), usize::MAX)?;
    for (k, v) in range.rows {
        map.insert(k, v).map_err(|_| PalwRowUnavailableV1::Undecodable)?;
    }
    Ok(map)
}

// ---------------------------------------------------------------------------------------------------------------------------
// The pruned backend: rows fetched from a supplier and verified against the pin

/// Whoever serves rows with proofs (a peer, a local archive). `None` is "could not serve" — never "no such row".
pub trait PalwRowSupplierV1 {
    fn fetch(&self, table: PalwStateTableV1, ranges: &[KeyRangeV1]) -> Option<AuthStateRowProofV1>;
}

impl<T: PalwRowSupplierV1 + ?Sized> PalwRowSupplierV1 for &T {
    fn fetch(&self, table: PalwStateTableV1, ranges: &[KeyRangeV1]) -> Option<AuthStateRowProofV1> {
        (**self).fetch(table, ranges)
    }
}

/// **Rows a pruned node holds no copy of**, fetched and verified against an authenticated-map pin (`AuthStatePinV1`). A supplier that
/// is silent gives [`PalwRowUnavailableV1::SupplierUnavailable`]; one that lies gives [`PalwRowUnavailableV1::ProofRejected`];
/// neither is ever "absent". A table the pinned directory proves it does not hold answers every key absent (that IS proven).
pub struct PalwProvenRowSourceV1<F> {
    pin: AuthStatePinV1,
    supplier: F,
    limits: AuthProofLimitsV1,
}

impl<F: PalwRowSupplierV1> PalwProvenRowSourceV1<F> {
    pub fn new(pin: AuthStatePinV1, supplier: F, limits: AuthProofLimitsV1) -> Self {
        Self { pin, supplier, limits }
    }

    fn verified(&self, table: PalwStateTableV1, ranges: &[KeyRangeV1]) -> Result<Option<VerifiedRowsV1>, PalwRowUnavailableV1> {
        let proof = self.supplier.fetch(table, ranges).ok_or(PalwRowUnavailableV1::SupplierUnavailable)?;
        match verify_state_rows_v1(&self.pin, &table.spec(), &proof, ranges, &self.limits) {
            Ok(rows) => Ok(Some(rows)),
            Err(AuthMapErrorV1::TableAbsent) => Ok(None),
            Err(_) => Err(PalwRowUnavailableV1::ProofRejected),
        }
    }
}

impl<F: PalwRowSupplierV1> PalwRowSourceV1 for PalwProvenRowSourceV1<F> {
    fn pin(&self) -> PalwRowPinV1 {
        PalwRowPinV1 { state_root: self.pin.state_root }
    }

    fn read_row(&self, table: PalwStateTableV1, key: &[u8]) -> PalwRowReadV1 {
        if !table.key_shape_ok(key) {
            return PalwRowReadV1::Unavailable(PalwRowUnavailableV1::MalformedKey);
        }
        match self.verified(table, &[KeyRangeV1::point(key)]) {
            Err(why) => PalwRowReadV1::Unavailable(why),
            Ok(None) => PalwRowReadV1::Absent,
            Ok(Some(rows)) => match rows.get(key) {
                Ok(Some(v)) => PalwRowReadV1::Present(v.to_vec()),
                Ok(None) => PalwRowReadV1::Absent,
                Err(_) => PalwRowReadV1::Unavailable(PalwRowUnavailableV1::ProofRejected),
            },
        }
    }

    fn read_range(&self, table: PalwStateTableV1, range: &KeyRangeV1, limit: usize) -> Result<PalwRowRangeV1, PalwRowUnavailableV1> {
        let Some(rows) = self.verified(table, std::slice::from_ref(range))? else {
            return Ok(PalwRowRangeV1 { rows: Vec::new(), more: false });
        };
        let all = rows.range(range).map_err(|_| PalwRowUnavailableV1::ProofRejected)?;
        let more = all.len() > limit;
        Ok(PalwRowRangeV1 { rows: all.into_iter().take(limit).map(|(k, v)| (k.to_vec(), v.to_vec())).collect(), more })
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Writes: a change set over a pinned source

/// One row write: the value the writer read under the pin, and the value it leaves.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PalwRowWriteV1 {
    pub old: Option<Vec<u8>>,
    pub new: Option<Vec<u8>>,
}

/// **The writes of one block**, against one pin. Only changes are held — never a copy of the parent.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PalwRowChangeSetV1 {
    pub pin: PalwRowPinV1,
    pub writes: BTreeMap<(PalwStateTableV1, Vec<u8>), PalwRowWriteV1>,
}

impl PalwRowChangeSetV1 {
    /// The writes of `table` as authenticated-map updates (what an update proof verifies; old → new root).
    pub fn auth_updates(&self, table: PalwStateTableV1) -> Vec<RowUpdateV1> {
        self.writes
            .iter()
            .filter(|((t, _), _)| *t == table)
            .map(|((_, k), w)| RowUpdateV1 { key: k.clone(), old: w.old.clone(), new: w.new.clone() })
            .collect()
    }

    /// The inverse change set (child → parent): what a reorg applies to undo the block.
    pub fn inverse(&self, child_pin: PalwRowPinV1) -> Self {
        Self {
            pin: child_pin,
            writes: self.writes.iter().map(|(k, w)| (k.clone(), PalwRowWriteV1 { old: w.new.clone(), new: w.old.clone() })).collect(),
        }
    }
}

/// **A source with a block's writes on top.** Reads see the writes; writes record the pinned old value, so a write of a row the
/// base cannot serve is refused (`Err`), not guessed.
pub struct PalwRowOverlayV1<S> {
    base: S,
    changes: PalwRowChangeSetV1,
}

impl<S: PalwRowSourceV1> PalwRowOverlayV1<S> {
    pub fn new(base: S) -> Self {
        let pin = base.pin();
        Self { base, changes: PalwRowChangeSetV1 { pin, writes: BTreeMap::new() } }
    }

    pub fn write_row(&mut self, table: PalwStateTableV1, key: &[u8], new: Option<Vec<u8>>) -> Result<(), PalwRowUnavailableV1> {
        if let Some(w) = self.changes.writes.get_mut(&(table, key.to_vec())) {
            w.new = new;
            return Ok(());
        }
        let old = self.base.read_row(table, key).into_result()?;
        self.changes.writes.insert((table, key.to_vec()), PalwRowWriteV1 { old, new });
        Ok(())
    }

    pub fn changes(&self) -> &PalwRowChangeSetV1 {
        &self.changes
    }

    pub fn into_changes(self) -> PalwRowChangeSetV1 {
        self.changes
    }
}

impl<S: PalwRowSourceV1> PalwRowSourceV1 for PalwRowOverlayV1<S> {
    /// The pin of the BASE: an overlay is "the parent plus these writes", not a new committed state.
    fn pin(&self) -> PalwRowPinV1 {
        self.changes.pin
    }

    fn read_row(&self, table: PalwStateTableV1, key: &[u8]) -> PalwRowReadV1 {
        match self.changes.writes.get(&(table, key.to_vec())) {
            Some(PalwRowWriteV1 { new: Some(v), .. }) => PalwRowReadV1::Present(v.clone()),
            Some(PalwRowWriteV1 { new: None, .. }) => PalwRowReadV1::Absent,
            None => self.base.read_row(table, key),
        }
    }

    fn read_range(&self, table: PalwStateTableV1, range: &KeyRangeV1, limit: usize) -> Result<PalwRowRangeV1, PalwRowUnavailableV1> {
        let touched: Vec<_> = self.changes.writes.iter().filter(|((t, k), _)| *t == table && range.contains(k)).collect();
        // Each write can hide at most one base row, so `limit + writes` base rows always leave `limit` merged rows.
        let base = self.base.read_range(table, range, limit.saturating_add(touched.len()))?;
        let last_base = base.rows.last().map(|(k, _)| k.clone());
        let mut merged: BTreeMap<Vec<u8>, Vec<u8>> = base.rows.into_iter().collect();
        for ((_, k), w) in touched {
            match &w.new {
                Some(v) => merged.insert(k.clone(), v.clone()),
                None => merged.remove(k),
            };
        }
        if base.more {
            // Past the last base row the base was not read: a written row there may have unread base rows before it.
            let cut = last_base.expect("a cut range returned rows");
            merged.retain(|k, _| *k <= cut);
        }
        let mut rows: Vec<_> = merged.into_iter().collect();
        let more = base.more || rows.len() > limit;
        rows.truncate(limit);
        Ok(PalwRowRangeV1 { rows, more })
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// The bounded cache

type CacheKey = (Hash64, PalwStateTableV1, Vec<u8>);

/// What the cache did, for tests and metrics.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PalwRowCacheStatsV1 {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub used_bytes: usize,
    pub entries: usize,
}

/// **Verified answers, LRU, under a byte budget.** Keys carry the root, so an answer is reused only for the state it was read from.
pub struct PalwRowCacheV1 {
    budget_bytes: usize,
    used_bytes: usize,
    tick: u64,
    entries: HashMap<CacheKey, (Option<Vec<u8>>, u64)>,
    lru: BTreeMap<u64, CacheKey>,
    stats: PalwRowCacheStatsV1,
}

const ENTRY_OVERHEAD: usize = 128;

fn entry_cost(key: &CacheKey, value: &Option<Vec<u8>>) -> usize {
    ENTRY_OVERHEAD + key.2.len() + value.as_ref().map_or(0, Vec::len)
}

impl PalwRowCacheV1 {
    pub fn new(budget_bytes: usize) -> Self {
        Self { budget_bytes, used_bytes: 0, tick: 0, entries: HashMap::new(), lru: BTreeMap::new(), stats: Default::default() }
    }

    pub fn stats(&self) -> PalwRowCacheStatsV1 {
        PalwRowCacheStatsV1 { used_bytes: self.used_bytes, entries: self.entries.len(), ..self.stats }
    }

    fn get(&mut self, key: &CacheKey) -> Option<Option<Vec<u8>>> {
        self.tick += 1;
        let tick = self.tick;
        let (value, at) = self.entries.get_mut(key)?;
        self.lru.remove(at);
        *at = tick;
        self.lru.insert(tick, key.clone());
        Some(value.clone())
    }

    fn put(&mut self, key: CacheKey, value: Option<Vec<u8>>) {
        let cost = entry_cost(&key, &value);
        if cost > self.budget_bytes {
            return;
        }
        if let Some((old, at)) = self.entries.remove(&key) {
            self.lru.remove(&at);
            self.used_bytes -= entry_cost(&key, &old);
        }
        while self.used_bytes + cost > self.budget_bytes {
            let (_, victim) = self.lru.pop_first().expect("over budget implies an entry");
            let (old, _) = self.entries.remove(&victim).expect("lru and entries agree");
            self.used_bytes -= entry_cost(&victim, &old);
            self.stats.evictions += 1;
        }
        self.tick += 1;
        self.used_bytes += cost;
        self.lru.insert(self.tick, key.clone());
        self.entries.insert(key, (value, self.tick));
    }

    /// Forget everything (a restart, a budget cut): answers come from the source again.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.lru.clear();
        self.used_bytes = 0;
    }
}

/// **A source behind the bounded cache.**
pub struct PalwCachedRowSourceV1<S> {
    inner: S,
    cache: RefCell<PalwRowCacheV1>,
}

impl<S: PalwRowSourceV1> PalwCachedRowSourceV1<S> {
    pub fn new(inner: S, budget_bytes: usize) -> Self {
        Self { inner, cache: RefCell::new(PalwRowCacheV1::new(budget_bytes)) }
    }

    pub fn stats(&self) -> PalwRowCacheStatsV1 {
        self.cache.borrow().stats()
    }

    pub fn clear(&self) {
        self.cache.borrow_mut().clear()
    }

    pub fn inner(&self) -> &S {
        &self.inner
    }
}

impl<S: PalwRowSourceV1> PalwRowSourceV1 for PalwCachedRowSourceV1<S> {
    fn pin(&self) -> PalwRowPinV1 {
        self.inner.pin()
    }

    fn read_row(&self, table: PalwStateTableV1, key: &[u8]) -> PalwRowReadV1 {
        let ck = (self.inner.pin().state_root, table, key.to_vec());
        let hit = self.cache.borrow_mut().get(&ck);
        if let Some(hit) = hit {
            self.cache.borrow_mut().stats.hits += 1;
            return hit.map_or(PalwRowReadV1::Absent, PalwRowReadV1::Present);
        }
        self.cache.borrow_mut().stats.misses += 1;
        let read = self.inner.read_row(table, key);
        match &read {
            PalwRowReadV1::Present(v) => self.cache.borrow_mut().put(ck, Some(v.clone())),
            PalwRowReadV1::Absent => self.cache.borrow_mut().put(ck, None),
            PalwRowReadV1::Unavailable(_) => {} // never cached: the next read asks again
        }
        read
    }

    fn read_range(&self, table: PalwStateTableV1, range: &KeyRangeV1, limit: usize) -> Result<PalwRowRangeV1, PalwRowUnavailableV1> {
        self.inner.read_range(table, range, limit)
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// The fence

impl Params {
    /// **`palw_state_row_map_v1` (RFC-0036 / ADR-0203, Proposed — DORMANT).** `Ok` while the fence is not armed (every preset).
    /// Armed, it is refused: no consensus path reads rows through this layer or commits a state root of the authenticated form, so a
    /// height would arm nothing — and the RFC is not adopted.
    pub fn validate_palw_state_row_map_v1(&self) -> Result<(), PalwModeV2Error> {
        if self.palw_state_row_map_v1_fence().is_some() {
            return Err(PalwModeV2Error::Invalid(
                "palw_state_row_map_v1 cannot be armed: RFC-0036 is Proposed and no consensus path reads or commits the row map",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests;
