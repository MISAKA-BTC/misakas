//! A6 production storage boundary for checked, node-local Positive histories.
//! Full computation replay precedes publication; archives, resource/funding/
//! liability row deltas and undo commit in the SAME synchronous RocksDB batch.
//! This does not allocate consensus wire IDs, select a pruning trust policy,
//! implement missing native coin authority, or grant consensus credit.
//! Projection callbacks only PREPARE rows; they must not mutate balances or
//! caches. Native integrations hold the existing chain-state exclusive writer
//! guard across anchor resolution, replay, current branch/generation/clock
//! validation, projection + commit, including all direct DB writers.
use kaspa_consensus_core::{
    Hash64,
    palw_positive_court_v2::{
        ErrorV2, MinerResultV2,
        coordinator::{
            PositiveCoordinatorV2,
            recovery::{
                JournaledCoordinatorV2, PreparedRecoveryUpdateV2, RecoveryContextV2, RecoveryErrorV2, RecoveryHeadV2, RecoveryLimitsV2,
            },
        },
        recovery_sync::{CheckedRecoveryImportV2, RecoveryImportScopeV2, RecoveryRetentionV2},
    },
};
use kaspa_database::palw_positive_recovery::{
    CommitEffect, PositiveRecoveryStore, RecoveryBinding, RecoveryCommit, RecoveryHead, RecoveryStoreError, RelatedRow,
};

#[derive(Debug)]
pub enum PersistenceErrorV2 {
    Core(ErrorV2),
    Runtime(RecoveryErrorV2),
    Store(RecoveryStoreError),
    Missing,
    Binding,
    Clock,
}
impl From<ErrorV2> for PersistenceErrorV2 {
    fn from(e: ErrorV2) -> Self {
        Self::Core(e)
    }
}
impl From<RecoveryErrorV2> for PersistenceErrorV2 {
    fn from(e: RecoveryErrorV2) -> Self {
        Self::Runtime(e)
    }
}
impl From<RecoveryStoreError> for PersistenceErrorV2 {
    fn from(e: RecoveryStoreError) -> Self {
        Self::Store(e)
    }
}
fn h(h: Hash64) -> [u8; 64] {
    *h.as_byte_slice()
}
fn db_head(head: RecoveryHeadV2) -> RecoveryHead {
    RecoveryHead { records: head.records, digest: h(head.digest) }
}
fn binding(result: &MinerResultV2) -> RecoveryBinding {
    RecoveryBinding { branch: h(result.branch), ruleset: h(result.ruleset), result: h(result.id()) }
}
fn scope(result: &MinerResultV2) -> RecoveryImportScopeV2 {
    RecoveryImportScopeV2 {
        network: result.network,
        branch: result.branch,
        ruleset: result.ruleset,
        case: result.claim,
        result: result.id(),
    }
}
fn transaction(result: &MinerResultV2, head: RecoveryHeadV2, now: u64, rows: &[RelatedRow]) -> [u8; 64] {
    let mut hash = blake2b_simd::Params::new().hash_length(64).key(b"misaka/node-recovery-transaction/v1").to_state();
    for id in [result.id(), result.claim, result.branch, result.ruleset, head.digest] {
        hash.update(id.as_byte_slice());
    }
    hash.update(&head.records.to_le_bytes());
    hash.update(&now.to_le_bytes());
    for row in rows {
        hash.update(&(row.key.len() as u64).to_le_bytes());
        hash.update(&row.key);
        for value in [&row.expected, &row.value] {
            match value {
                Some(v) => {
                    hash.update(&[1]);
                    hash.update(&(v.len() as u64).to_le_bytes());
                    hash.update(v);
                }
                None => {
                    hash.update(&[0]);
                }
            }
        }
    }
    hash.finalize().as_bytes().try_into().expect("64")
}
fn write_checked(
    store: &PositiveRecoveryStore,
    state: &PositiveCoordinatorV2,
    archive: &[u8],
    head: RecoveryHeadV2,
    previous: Option<RecoveryHeadV2>,
    now: u64,
    related: Vec<RelatedRow>,
) -> Result<CommitEffect, PersistenceErrorV2> {
    store
        .commit(RecoveryCommit {
            case: h(state.result().claim),
            binding: binding(state.result()),
            expected_head: previous.map(db_head),
            head: db_head(head),
            observed_daa: now,
            transaction: transaction(state.result(), head, now, &related),
            archive: archive.to_vec(),
            related,
        })
        .map_err(Into::into)
}
/// A quote for THIS whole-image implementation, including replay plus archive
/// re-encoding, retained before-image, and synchronous WriteBatch material.
/// It bounds explicit logical archive buffers (including undo/WriteBatch
/// copies), not allocator slack, decoded heap, native rows or RocksDB RSS.
/// It is an admission accounting quote, not a wall-clock performance claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryWorkQuoteV2 {
    pub replay_records: u64,
    pub replay_bytes: u64,
    pub encode_bytes: u64,
    pub peak_archive_bytes: u64,
}
pub fn quote_update(current_bytes: u64, current_records: u64, max_next_bytes: u64) -> Result<RecoveryWorkQuoteV2, ErrorV2> {
    if max_next_bytes < current_bytes {
        return Err(ErrorV2::Budget);
    }
    let peak_archive_bytes = current_bytes
        .checked_mul(8)
        .and_then(|n| max_next_bytes.checked_mul(16).and_then(|m| n.checked_add(m)))
        .ok_or(ErrorV2::Budget)?;
    Ok(RecoveryWorkQuoteV2 {
        replay_records: current_records.checked_add(1).ok_or(ErrorV2::Budget)?,
        replay_bytes: current_bytes,
        encode_bytes: max_next_bytes,
        peak_archive_bytes,
    })
}
/// Worst-case whole-history quote which an embedding A4 resource reservation
/// must include before accepting this bounded profile. Decoded heap and native
/// related-row memory remain separate explicit reservations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryAdmissionQuoteV2 {
    pub max_reencoded_bytes: u64,
    pub max_replayed_elements: u64,
    pub max_replayed_terms: u64,
    pub max_archive_peak_bytes: u64,
}
pub fn quote_admission(state: &PositiveCoordinatorV2, limits: RecoveryLimitsV2) -> Result<RecoveryAdmissionQuoteV2, ErrorV2> {
    let count = limits.max_records.checked_add(1).ok_or(ErrorV2::Budget)?;
    let reserved = state.attempts();
    Ok(RecoveryAdmissionQuoteV2 {
        max_reencoded_bytes: limits.max_encoded_bytes.checked_mul(count).ok_or(ErrorV2::Budget)?,
        max_replayed_elements: reserved.reserved_elements.checked_mul(count).ok_or(ErrorV2::Budget)?,
        max_replayed_terms: reserved.reserved_terms.checked_mul(count).ok_or(ErrorV2::Budget)?,
        max_archive_peak_bytes: limits.max_encoded_bytes.checked_mul(24).ok_or(ErrorV2::Budget)?,
    })
}

/// Initial publication. `project` is the embedding A4/A5 authority: it must
/// resolve native funding, capacity, consent and liabilities from ONE current
/// execution view. No permissive native projector is provided by this crate.
pub fn commit_initial(
    store: &PositiveRecoveryStore,
    mut journal: JournaledCoordinatorV2,
    now: u64,
    authenticate_clock: impl FnOnce(u64) -> bool,
    project: impl FnOnce(&PositiveCoordinatorV2) -> Result<Vec<RelatedRow>, PersistenceErrorV2>,
) -> Result<JournaledCoordinatorV2, PersistenceErrorV2> {
    journal.advance_clock(now, authenticate_clock)?;
    let state = journal.state()?;
    let related = project(state)?;
    write_checked(store, state, journal.encoded()?, journal.head()?, None, now, related)?;
    Ok(journal)
}
/// Full computation and budget effects have already been checked by the
/// consumed proposal. Native row preparation sees that checked state, then
/// all effects commit together. Any I/O ambiguity consumes the runtime.
/// `current_daa` must read the authoritative native clock while holding the
/// exclusive view guard. A proposal from an earlier view cannot be published.
/// That guard/projector must also revalidate current branch/generation/consent;
/// there is deliberately no permissive native implementation.
pub fn commit_prepared(
    store: &PositiveRecoveryStore,
    update: PreparedRecoveryUpdateV2,
    current_daa: impl FnOnce() -> u64,
    project: impl FnOnce(&PositiveCoordinatorV2) -> Result<Vec<RelatedRow>, PersistenceErrorV2>,
) -> Result<(JournaledCoordinatorV2, Result<(), ErrorV2>), PersistenceErrorV2> {
    let now = update.observed_daa();
    if current_daa() != now {
        return Err(PersistenceErrorV2::Clock);
    }
    let related = project(update.checked_state())?;
    update
        .commit(|previous, head, archive, state| {
            write_checked(store, state, archive, head, Some(previous), now, related)
                .map(|_| ())
                .map_err(|e| std::io::Error::other(format!("{e:?}")))
        })
        .map_err(Into::into)
}
/// A local DB head is still not computation authority: authenticate it against
/// the external selected-history anchor and replay every material input.
fn restore_view<C: RecoveryContextV2>(
    store: &PositiveRecoveryStore,
    context: &C,
    anchor: &C::TrustedAnchor,
    limits: RecoveryLimitsV2,
) -> Result<(JournaledCoordinatorV2, u64), PersistenceErrorV2> {
    let admitted = context.resolve(anchor)?;
    let now = context.current_daa();
    let result = admitted.coordinator.result();
    let saved = store.load(&h(result.claim))?.ok_or(PersistenceErrorV2::Missing)?;
    if saved.binding != binding(result) || saved.head != db_head(admitted.expected) {
        return Err(PersistenceErrorV2::Binding);
    }
    if saved.observed_daa > now {
        return Err(PersistenceErrorV2::Clock);
    }
    let restored = JournaledCoordinatorV2::restore(
        admitted.coordinator,
        admitted.admitted_at,
        &saved.archive,
        admitted.expected,
        limits,
        now,
        |origin, event| context.authenticate(anchor, origin, event),
    )
    .map_err(PersistenceErrorV2::from)?;
    Ok((restored, saved.observed_daa))
}
/// Read-only rechecked view. Expiry derived from the current native clock may
/// be newer than durable financial rows; native publication must use
/// restore_and_commit below before exposing fee/resource side effects.
pub fn restore_selected<C: RecoveryContextV2>(
    store: &PositiveRecoveryStore,
    context: C,
    anchor: &C::TrustedAnchor,
    limits: RecoveryLimitsV2,
) -> Result<JournaledCoordinatorV2, PersistenceErrorV2> {
    restore_view(store, &context, anchor, limits).map(|(state, _)| state)
}
/// Startup with clock-driven fees/resources projected and committed before the
/// restored runtime is published. The projection is read-only preparation of
/// deltas and must hold the native execution view's exclusive writer guard.
/// Never debit/hold/publish anything inside the projection callback itself.
pub fn restore_and_commit<C: RecoveryContextV2>(
    store: &PositiveRecoveryStore,
    context: C,
    anchor: &C::TrustedAnchor,
    limits: RecoveryLimitsV2,
    project: impl FnOnce(&PositiveCoordinatorV2) -> Result<Vec<RelatedRow>, PersistenceErrorV2>,
) -> Result<JournaledCoordinatorV2, PersistenceErrorV2> {
    let (journal, saved_clock) = restore_view(store, &context, anchor, limits)?;
    let now = journal.observed_daa()?;
    if now == saved_clock {
        if context.current_daa() != now {
            return Err(PersistenceErrorV2::Clock);
        }
        return Ok(journal);
    }
    let prepared = journal.prepare_clock(now, |daa| daa == now)?;
    commit_prepared(store, prepared, || context.current_daa(), project).map(|(journal, _)| journal)
}

/// Only an actual rechecked bounded import can select a new archive generation.
/// Peer-selected transport hashes or serialized Pass flags cannot construct the
/// token. Native row projection remains a separate, trusted A4/A5 obligation.
pub fn publish_import(
    store: &PositiveRecoveryStore,
    checked: CheckedRecoveryImportV2,
    previous: Option<RecoveryHeadV2>,
    current_daa: impl FnOnce() -> u64,
    project: impl FnOnce(&PositiveCoordinatorV2) -> Result<Vec<RelatedRow>, PersistenceErrorV2>,
) -> Result<JournaledCoordinatorV2, PersistenceErrorV2> {
    let now = checked.state().observed_daa()?;
    if current_daa() != now {
        return Err(PersistenceErrorV2::Clock);
    }
    let journal = checked.state();
    let state = journal.state()?;
    if scope(state.result()) != checked.scope() || journal.head()? != checked.head() {
        return Err(PersistenceErrorV2::Binding);
    }
    let related = project(state)?;
    write_checked(store, state, checked.encoded(), checked.head(), previous, now, related)?;
    Ok(checked.into_state())
}

/// Remove obsolete undo/receipt material only after a rechecked replacement and
/// every rooted retention horizon permit it. Current archive and native rows
/// remain present; logical deletion does not claim immediate disk reclamation.
pub fn prune_retired(
    store: &PositiveRecoveryStore,
    checked: &CheckedRecoveryImportV2,
    retention: RecoveryRetentionV2,
    max_rows: u64,
    max_bytes: u64,
) -> Result<kaspa_database::palw_positive_recovery::PurgeEffect, PersistenceErrorV2> {
    let now = checked.observed_daa();
    if !retention.may_prune(now, checked) {
        return Err(PersistenceErrorV2::Binding);
    }
    let state = checked.state().state()?;
    let saved = store.load(&h(state.result().claim))?.ok_or(PersistenceErrorV2::Missing)?;
    if scope(state.result()) != checked.scope()
        || saved.binding != binding(state.result())
        || saved.head != db_head(checked.head())
        || saved.observed_daa != now
    {
        return Err(PersistenceErrorV2::Binding);
    }
    store.purge_before_images(&h(state.result().claim), saved.head, now, max_rows, max_bytes).map_err(Into::into)
}
