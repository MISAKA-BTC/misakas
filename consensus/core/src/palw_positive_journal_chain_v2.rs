//! Bounded identity/link diagnostics for the shared per-case journal.
//!
//! Neither a pin nor successful link checking authenticates native accepted
//! provenance, material domains, a court outcome, payment, or credit. Native
//! owners must independently authorize and replay the complete accepted prefix.
use misaka_palw_positive_contract::{
    ContractError,
    identity::{CaseId, ChainScope, Digest, ResultId},
    journal::{CaseJournalEventKind, CaseJournalLimits, CaseJournalRecordView},
    rows::{AcceptedMaterial, JournalHeadRow},
};
use std::mem::size_of;

/// An implementation lends one pinned record for the duration of `visit`.
/// It must enforce `max_bytes` before copying/allocating a record. The checker
/// independently rechecks that length. Missing records remain source errors.
pub trait CaseJournalSourceV2 {
    type Error;
    fn with_record<T>(&self, head: Digest, max_bytes: u64, visit: impl FnOnce(&[u8]) -> T) -> Result<T, Self::Error>;
}

/// Public data, not accepted-state authority. The native owner chooses the pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseJournalPinV2 {
    pub scope: ChainScope,
    pub case: CaseId,
    pub result: ResultId,
    pub head: JournalHeadRow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseJournalChainLimitsV2 {
    pub record_limits: CaseJournalLimits,
    pub max_events: u64,
    /// Aggregate input bytes for BOTH the backwalk and one forward visitation.
    pub max_total_record_bytes: u64,
    /// Both identity domains on BOTH passes: four times the unique input bytes.
    pub max_hash_bytes: u64,
    pub max_index_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseJournalInvalidV2 {
    Codec(ContractError),
    Pin,
    Scope,
    Sequence,
    Identity,
    LatestHead,
    InitialEvent,
    ChangedOnReread,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseJournalBudgetV2 {
    Limits,
    Events,
    Index,
    Record,
    TotalRecordBytes,
    HashBytes,
}
#[derive(Debug, PartialEq, Eq)]
pub enum CaseJournalChainErrorV2<E> {
    Source(E),
    Invalid(CaseJournalInvalidV2),
    LocalBudget(CaseJournalBudgetV2),
}
#[derive(Debug, PartialEq, Eq)]
pub enum CaseJournalVisitErrorV2<S, V> {
    Chain(CaseJournalChainErrorV2<S>),
    Visitor(V),
}

/// Work for one complete check followed by one complete forward visit. These
/// are reservation terms, not an estimate of physical RSS or a chain receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseJournalLinkQuoteV2 {
    pub events: u64,
    pub record_reads: u64,
    pub record_decodes: u64,
    pub total_record_bytes: u64,
    pub hash_bytes: u64,
    pub index_bytes: u64,
    pub retained_inline_bytes: u64,
    pub decode_workspace_bytes: u64,
}

/// Sealed identity/link result. It contains only reverse-ordered digests and
/// bounded work metadata; it cannot be used as a native provenance capability.
#[derive(Debug)]
pub struct CheckedCaseJournalLinksV2 {
    pin: CaseJournalPinV2,
    limits: CaseJournalChainLimitsV2,
    reverse_heads: Vec<Digest>,
    quote: CaseJournalLinkQuoteV2,
}
impl CheckedCaseJournalLinksV2 {
    pub fn pin(&self) -> &CaseJournalPinV2 {
        &self.pin
    }
    pub fn quote(&self) -> CaseJournalLinkQuoteV2 {
        self.quote
    }
    pub fn record_limits(&self) -> CaseJournalLimits {
        self.limits.record_limits
    }

    /// Reacquire each pinned record, oldest first, and recheck identity, scope,
    /// sequence and links BEFORE invoking the callback for that record.
    /// A source may change between reads. A later error can therefore follow
    /// earlier callbacks: callers MUST stage their effects privately and expose
    /// no partial replay/result until this entire method returns success.
    pub fn visit_forward<S: CaseJournalSourceV2, V>(
        &self,
        source: &S,
        mut visitor: impl FnMut(CaseJournalRecordView<'_>) -> Result<(), V>,
    ) -> Result<(), CaseJournalVisitErrorV2<S::Error, V>> {
        let mut raw = 0u64;
        let mut workspace = 0u64;
        for (offset, &head) in self.reverse_heads.iter().rev().enumerate() {
            let sequence = u64::try_from(offset)
                .ok()
                .and_then(|n| n.checked_add(1))
                .ok_or(CaseJournalVisitErrorV2::Chain(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Events)))?;
            let previous = if offset == 0 { Digest([0; 64]) } else { self.reverse_heads[self.reverse_heads.len() - offset] };
            source
                .with_record(head, self.limits.record_limits.max_record_bytes, |bytes| {
                    account_record(bytes, &mut raw, self.limits).map_err(CaseJournalVisitErrorV2::Chain)?;
                    let (record, history) = checked_record(bytes, head, sequence, self.pin, self.limits.record_limits, &mut workspace)
                        .map_err(CaseJournalVisitErrorV2::Chain)?;
                    if record.header().previous != previous {
                        return Err(CaseJournalVisitErrorV2::Chain(CaseJournalChainErrorV2::Invalid(
                            CaseJournalInvalidV2::ChangedOnReread,
                        )));
                    }
                    if sequence == self.pin.head.sequence {
                        check_latest(record, history, self.pin).map_err(CaseJournalVisitErrorV2::Chain)?;
                    }
                    visitor(record).map_err(CaseJournalVisitErrorV2::Visitor)
                })
                .map_err(|error| CaseJournalVisitErrorV2::Chain(CaseJournalChainErrorV2::Source(error)))??;
        }
        if raw.checked_mul(2) != Some(self.quote.total_record_bytes) || workspace != self.quote.decode_workspace_bytes {
            return Err(CaseJournalVisitErrorV2::Chain(CaseJournalChainErrorV2::Invalid(CaseJournalInvalidV2::ChangedOnReread)));
        }
        Ok(())
    }
}

/// Check at most `pin.head.sequence` linked records, retaining only their
/// digests. All count/index arithmetic is checked BEFORE reserving the Vec.
pub fn check_case_journal_links_v2<S: CaseJournalSourceV2>(
    source: &S,
    pin: CaseJournalPinV2,
    limits: CaseJournalChainLimitsV2,
) -> Result<CheckedCaseJournalLinksV2, CaseJournalChainErrorV2<S::Error>> {
    limits.record_limits.validate().map_err(|_| CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Limits))?;
    if limits.max_events == 0 || limits.max_total_record_bytes == 0 || limits.max_hash_bytes == 0 || limits.max_index_bytes == 0 {
        return Err(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Limits));
    }
    if pin.head.case != pin.case || pin.head.sequence == 0 || pin.head.head == pin.head.history.digest {
        return Err(CaseJournalChainErrorV2::Invalid(CaseJournalInvalidV2::Pin));
    }
    let events = pin.head.sequence;
    if events > limits.max_events {
        return Err(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Events));
    }
    let index_bytes = events
        .checked_mul(size_of::<Digest>() as u64)
        .filter(|&n| n <= limits.max_index_bytes)
        .ok_or(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Index))?;
    let count = usize::try_from(events).map_err(|_| CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Index))?;
    let reads = events.checked_mul(2).ok_or(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Events))?;
    let mut reverse_heads = Vec::new();
    reverse_heads.try_reserve_exact(count).map_err(|_| CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Index))?;
    let actual_index = u64::try_from(reverse_heads.capacity())
        .ok()
        .and_then(|n| n.checked_mul(size_of::<Digest>() as u64))
        .filter(|&n| n <= limits.max_index_bytes)
        .ok_or(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Index))?;
    debug_assert!(actual_index >= index_bytes);
    let mut raw = 0u64;
    let mut workspace = 0u64;
    let mut expected = pin.head.head;
    for sequence in (1..=events).rev() {
        let previous = source
            .with_record(expected, limits.record_limits.max_record_bytes, |bytes| {
                account_record(bytes, &mut raw, limits)?;
                let (record, history) = checked_record(bytes, expected, sequence, pin, limits.record_limits, &mut workspace)?;
                if sequence == events {
                    check_latest(record, history, pin)?;
                }
                Ok(record.header().previous)
            })
            .map_err(CaseJournalChainErrorV2::Source)??;
        reverse_heads.push(expected);
        expected = previous;
    }
    let quote = CaseJournalLinkQuoteV2 {
        events,
        record_reads: reads,
        record_decodes: reads,
        total_record_bytes: raw.checked_mul(2).ok_or(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::TotalRecordBytes))?,
        hash_bytes: raw.checked_mul(4).ok_or(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::HashBytes))?,
        index_bytes: actual_index,
        retained_inline_bytes: size_of::<CheckedCaseJournalLinksV2>() as u64,
        decode_workspace_bytes: workspace,
    };
    Ok(CheckedCaseJournalLinksV2 { pin, limits, reverse_heads, quote })
}

fn account_record<E>(bytes: &[u8], raw: &mut u64, limits: CaseJournalChainLimitsV2) -> Result<(), CaseJournalChainErrorV2<E>> {
    let n = u64::try_from(bytes.len()).map_err(|_| CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Record))?;
    if n > limits.record_limits.max_record_bytes {
        return Err(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Record));
    }
    let next = raw.checked_add(n).ok_or(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::TotalRecordBytes))?;
    if next.checked_mul(2).is_none_or(|n| n > limits.max_total_record_bytes) {
        return Err(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::TotalRecordBytes));
    }
    // Check the cost of BOTH domains on BOTH passes before invoking either
    // shared identity hash. No uncharged hash of a peer's oversized record.
    if next.checked_mul(4).is_none_or(|n| n > limits.max_hash_bytes) {
        return Err(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::HashBytes));
    }
    *raw = next;
    Ok(())
}

fn checked_record<'a, E>(
    bytes: &'a [u8],
    expected: Digest,
    sequence: u64,
    pin: CaseJournalPinV2,
    limits: CaseJournalLimits,
    workspace: &mut u64,
) -> Result<(CaseJournalRecordView<'a>, AcceptedMaterial), CaseJournalChainErrorV2<E>> {
    // Bounds were checked before this borrowed decode by account_record. The
    // per-record profile is supplied by the caller, never invented here.
    let record = CaseJournalRecordView::decode(bytes, limits).map_err(|error| {
        if error == ContractError::Budget {
            CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Record)
        } else {
            CaseJournalChainErrorV2::Invalid(CaseJournalInvalidV2::Codec(error))
        }
    })?;
    let header = record.header();
    if header.scope != pin.scope || header.case != pin.case || header.result != pin.result {
        return Err(CaseJournalChainErrorV2::Invalid(CaseJournalInvalidV2::Scope));
    }
    if header.sequence != sequence {
        return Err(CaseJournalChainErrorV2::Invalid(CaseJournalInvalidV2::Sequence));
    }
    if sequence == 1
        && (header.previous != Digest([0; 64])
            || header.kind != CaseJournalEventKind::NativeBody
            || record.payload().get(4) != Some(&0))
    {
        return Err(CaseJournalChainErrorV2::Invalid(CaseJournalInvalidV2::InitialEvent));
    }
    let head = record.head();
    let history = record.history();
    if head != expected || head == history.digest {
        return Err(CaseJournalChainErrorV2::Invalid(CaseJournalInvalidV2::Identity));
    }
    *workspace = (*workspace).max(record.quote().decode_workspace_bytes);
    Ok((record, history))
}

fn check_latest<E>(
    record: CaseJournalRecordView<'_>,
    history: AcceptedMaterial,
    pin: CaseJournalPinV2,
) -> Result<(), CaseJournalChainErrorV2<E>> {
    if record.header().previous != pin.head.previous || history != pin.head.history {
        return Err(CaseJournalChainErrorV2::Invalid(CaseJournalInvalidV2::LatestHead));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_positive_contract::{
        CONTRACT_REVISION, CONTRACT_VERSION,
        journal::{CaseJournalHeader, CaseJournalRecord},
        rows::{AcceptedOrigin, NativeHash64},
    };
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
    };

    #[derive(Default)]
    struct Source {
        records: RefCell<BTreeMap<Digest, Vec<u8>>>,
        reads: Cell<u64>,
    }
    impl CaseJournalSourceV2 for Source {
        type Error = &'static str;
        fn with_record<T>(&self, head: Digest, max_bytes: u64, visit: impl FnOnce(&[u8]) -> T) -> Result<T, Self::Error> {
            self.reads.set(self.reads.get() + 1);
            let records = self.records.borrow();
            let record = records.get(&head).ok_or("missing")?;
            if record.len() as u64 > max_bytes {
                return Err("oversize");
            }
            Ok(visit(record))
        }
    }
    fn limits() -> CaseJournalChainLimitsV2 {
        CaseJournalChainLimitsV2 {
            record_limits: CaseJournalLimits {
                max_record_bytes: 4096,
                max_payload_bytes: 1024,
                max_aux_entries: 4,
                max_aux_preimage_bytes: 1024,
                max_total_preimage_bytes: 2048,
                max_logical_heap_bytes: 4096,
            },
            max_events: 8,
            max_total_record_bytes: 65_536,
            max_hash_bytes: 131_072,
            max_index_bytes: 512,
        }
    }
    fn scope() -> ChainScope {
        ChainScope { network: Digest([1; 64]), ruleset: Digest([2; 64]), branch: Digest([3; 64]) }
    }
    fn record(sequence: u64, previous: Digest, kind: CaseJournalEventKind, operation: u8) -> Vec<u8> {
        let header = CaseJournalHeader {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            scope: scope(),
            case: CaseId(Digest([4; 64])),
            result: ResultId(Digest([5; 64])),
            sequence,
            previous,
            origin: AcceptedOrigin {
                accepting_block: NativeHash64([6; 64]),
                carrier_block: NativeHash64([7; 64]),
                carrier_transaction: NativeHash64([8; 64]),
                accepted_tx_ordinal: 0,
                event_ordinal: sequence as u32,
                accepted_daa: sequence,
                accepted_blue_score: sequence,
            },
            kind,
        };
        let payload = match kind {
            CaseJournalEventKind::NativeBody => {
                let mut payload = Vec::from(CONTRACT_VERSION.to_le_bytes());
                payload.extend_from_slice(&CONTRACT_REVISION.to_le_bytes());
                payload.push(operation);
                payload
            }
            CaseJournalEventKind::NativeTransaction => vec![operation],
        };
        CaseJournalRecord { header, payload, aux: vec![] }.encode(limits().record_limits).unwrap()
    }
    fn fixture(n: u64) -> (Source, CaseJournalPinV2, Vec<Digest>) {
        let source = Source::default();
        let mut previous = Digest([0; 64]);
        let mut heads = vec![];
        let mut pin = None;
        for sequence in 1..=n {
            let bytes = record(sequence, previous, CaseJournalEventKind::NativeBody, if sequence == 1 { 0 } else { 5 });
            let view = CaseJournalRecordView::decode(&bytes, limits().record_limits).unwrap();
            let head = view.head();
            pin = Some(CaseJournalPinV2 {
                scope: scope(),
                case: view.header().case,
                result: view.header().result,
                head: JournalHeadRow { case: view.header().case, sequence, previous, head, history: view.history() },
            });
            source.records.borrow_mut().insert(head, bytes);
            heads.push(head);
            previous = head;
        }
        (source, pin.unwrap(), heads)
    }
    fn pin_single(source: &Source, bytes: Vec<u8>) -> CaseJournalPinV2 {
        let view = CaseJournalRecordView::decode(&bytes, limits().record_limits).unwrap();
        let header = view.header();
        let pin = CaseJournalPinV2 {
            scope: header.scope,
            case: header.case,
            result: header.result,
            head: JournalHeadRow {
                case: header.case,
                sequence: header.sequence,
                previous: header.previous,
                head: view.head(),
                history: view.history(),
            },
        };
        source.records.borrow_mut().insert(pin.head.head, bytes);
        pin
    }

    #[test]
    fn bounded_backwalk_and_forward_recheck_charge_two_passes() {
        let (source, pin, _) = fixture(3);
        let raw: u64 = source.records.borrow().values().map(|b| b.len() as u64).sum();
        let checked = check_case_journal_links_v2(&source, pin, limits()).unwrap();
        assert_eq!(source.reads.get(), 3);
        assert_eq!(checked.quote().record_reads, 6);
        assert_eq!(checked.quote().record_decodes, 6);
        assert_eq!(checked.quote().total_record_bytes, raw * 2);
        assert_eq!(checked.quote().hash_bytes, raw * 4);
        assert!(checked.quote().index_bytes >= 3 * 64);
        let mut sequences = vec![];
        checked
            .visit_forward(&source, |record| {
                sequences.push(record.header().sequence);
                Ok::<_, ()>(())
            })
            .unwrap();
        assert_eq!(sequences, [1, 2, 3]);
        assert_eq!(source.reads.get(), 6);
        // Exact aggregate boundaries pass, one byte below either refuses.
        for policy in [CaseJournalChainLimitsV2 { max_total_record_bytes: raw * 2, max_hash_bytes: raw * 4, ..limits() }] {
            assert!(check_case_journal_links_v2(&source, pin, policy).is_ok());
        }
        assert!(matches!(
            check_case_journal_links_v2(&source, pin, CaseJournalChainLimitsV2 { max_total_record_bytes: raw * 2 - 1, ..limits() }),
            Err(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::TotalRecordBytes))
        ));
        assert!(matches!(
            check_case_journal_links_v2(&source, pin, CaseJournalChainLimitsV2 { max_hash_bytes: raw * 4 - 1, ..limits() }),
            Err(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::HashBytes))
        ));
    }

    #[test]
    fn event_and_index_limits_refuse_before_source_access() {
        let (source, mut pin, _) = fixture(1);
        pin.head.sequence = u64::MAX;
        assert!(matches!(
            check_case_journal_links_v2(&source, pin, limits()),
            Err(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Events))
        ));
        pin.head.sequence = 2;
        assert!(matches!(
            check_case_journal_links_v2(&source, pin, CaseJournalChainLimitsV2 { max_index_bytes: 127, ..limits() }),
            Err(CaseJournalChainErrorV2::LocalBudget(CaseJournalBudgetV2::Index))
        ));
        assert_eq!(source.reads.get(), 0);
    }

    #[test]
    fn missing_truncation_scope_tail_and_history_alias_are_rejected() {
        let (source, pin, heads) = fixture(2);
        source.records.borrow_mut().remove(&heads[0]);
        assert!(matches!(check_case_journal_links_v2(&source, pin, limits()), Err(CaseJournalChainErrorV2::Source("missing"))));
        let (source, pin, heads) = fixture(2);
        source.records.borrow_mut().get_mut(&heads[0]).unwrap().pop();
        assert!(matches!(check_case_journal_links_v2(&source, pin, limits()), Err(CaseJournalChainErrorV2::Invalid(_))));
        let (source, mut pin, _) = fixture(2);
        pin.scope.branch = Digest([9; 64]);
        assert!(matches!(
            check_case_journal_links_v2(&source, pin, limits()),
            Err(CaseJournalChainErrorV2::Invalid(CaseJournalInvalidV2::Scope))
        ));
        let (source, mut pin, heads) = fixture(2);
        let newer = source.records.borrow().get(&heads[1]).unwrap().clone();
        source.records.borrow_mut().insert(heads[0], newer);
        assert!(matches!(check_case_journal_links_v2(&source, pin, limits()), Err(CaseJournalChainErrorV2::Invalid(_))));
        pin.head.history.digest = pin.head.head;
        assert!(matches!(
            check_case_journal_links_v2(&source, pin, limits()),
            Err(CaseJournalChainErrorV2::Invalid(CaseJournalInvalidV2::Pin))
        ));
    }

    #[test]
    fn initial_event_and_cyclic_malformed_source_fail_without_unbounded_walk() {
        for (kind, op) in [(CaseJournalEventKind::NativeBody, 5), (CaseJournalEventKind::NativeTransaction, 0)] {
            let source = Source::default();
            let pin = pin_single(&source, record(1, Digest([0; 64]), kind, op));
            assert!(matches!(
                check_case_journal_links_v2(&source, pin, limits()),
                Err(CaseJournalChainErrorV2::Invalid(CaseJournalInvalidV2::InitialEvent))
            ));
        }
        let source = Source::default();
        let pin = pin_single(&source, record(2, Digest([10; 64]), CaseJournalEventKind::NativeBody, 5));
        // A source returning the same latest record for its predecessor cannot
        // manufacture a sequence-decreasing cycle even before hash identity.
        let bytes = source.records.borrow().get(&pin.head.head).unwrap().clone();
        source.records.borrow_mut().insert(Digest([10; 64]), bytes);
        assert!(matches!(
            check_case_journal_links_v2(&source, pin, limits()),
            Err(CaseJournalChainErrorV2::Invalid(CaseJournalInvalidV2::Sequence))
        ));
        assert_eq!(source.reads.get(), 2);
    }

    #[test]
    fn reread_change_and_visitor_failure_stop_before_later_callbacks() {
        let (source, pin, heads) = fixture(3);
        let checked = check_case_journal_links_v2(&source, pin, limits()).unwrap();
        let mut calls = 0;
        assert_eq!(
            checked.visit_forward(&source, |_| {
                calls += 1;
                Err("visitor")
            }),
            Err(CaseJournalVisitErrorV2::Visitor("visitor"))
        );
        assert_eq!(calls, 1);
        assert_eq!(source.reads.get(), 4);
        source.records.borrow_mut().get_mut(&heads[1]).unwrap().push(0);
        let mut private_staged = vec![];
        assert!(matches!(
            checked.visit_forward(&source, |record| {
                private_staged.push(record.header().sequence);
                Ok::<_, ()>(())
            }),
            Err(CaseJournalVisitErrorV2::Chain(CaseJournalChainErrorV2::Invalid(_)))
        ));
        assert_eq!(private_staged, [1]); // Must be discarded by the caller.
    }
}
