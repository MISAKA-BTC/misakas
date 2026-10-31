//! Concrete unknown-tail forgery against the local journal checksum chain.
//! The checksum is not authentication. Only an EXACT protected anchor may
//! authorize recovered preparation cost/union state.
use super::*;
use crate::miner::positive_v2::tests::Fixture;
use kaspa_consensus_core::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, open_artifact_leaf_v1};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Default)]
struct ProtectedFixture {
    anchor: Rc<RefCell<Option<JournalAnchorV2>>>,
    commits: Rc<Cell<u64>>,
}
impl ProtectedAnchorStoreV2 for ProtectedFixture {
    fn load(&self) -> Result<Option<JournalAnchorV2>, MinerErrorV2> {
        Ok(*self.anchor.borrow())
    }
    fn commit(&mut self, expected: Option<JournalAnchorV2>, next: JournalAnchorV2) -> Result<(), MinerErrorV2> {
        if *self.anchor.borrow() != expected {
            return Err(journal_error("tail-test protected CAS mismatch"));
        }
        *self.anchor.borrow_mut() = Some(next);
        self.commits.set(self.commits.get() + 1);
        Ok(())
    }
}
struct TempJournal(PathBuf);
impl TempJournal {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let suffix = NEXT.fetch_add(1, Ordering::Relaxed);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("misaka-a2-tail-{}-{now}-{suffix}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> PathBuf {
        self.0.join("journal")
    }
}
impl Drop for TempJournal {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Three genuine 4-KiB source units make the counter discount observable.
/// Tests use either sufficient signed budget for exact fresh-unit accounting,
/// or a lower budget that must reject a genuinely fresh second unit.
struct LargeUnits {
    units: Vec<StoredUnitV2>,
    calls: Cell<u64>,
}
impl StoredProviderV2 for LargeUnits {
    fn unit(&self, kind: SourceKind, _: Digest, index: u64) -> Result<&StoredUnitV2, String> {
        assert_eq!(kind, SourceKind::ArtifactInventory);
        self.calls.set(self.calls.get() + 1);
        self.units.get(index as usize).ok_or_else(|| "no source unit".into())
    }
}
struct LargeCase {
    signed: Fixture,
    provider: LargeUnits,
}
impl LargeCase {
    fn new() -> Self {
        Self::with_unique_budget(16384)
    }
    fn with_unique_budget(unique_bytes: u64) -> Self {
        let mut signed = Fixture::new();
        let operands: Vec<_> = (0..3)
            .map(|index| PalwArtifactOperandV1 {
                tensor_name: "large-row".into(),
                layer: None,
                row_start: index * 4096,
                bytes: vec![index as u8; 4096],
            })
            .collect();
        let root = artifact_root_v1(&operands.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
        signed.result.artifact = Digest(root.as_bytes());
        signed.admission.artifact = signed.result.artifact;
        signed.contract.budget.unique_source_bytes = unique_bytes;
        signed.result.service_contract = signed.contract.id();
        let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([61; 32]);
        signed.signature = *libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key, &signed.result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32])
            .unwrap()
            .as_ref();
        let units = (0..3).map(|index| StoredUnitV2::Artifact(open_artifact_leaf_v1(&operands, index).unwrap())).collect();
        Self { signed, provider: LargeUnits { units, calls: Cell::new(0) } }
    }
    fn duty(&self, index: u64) -> LargeDuty {
        LargeDuty {
            request: self.signed.request(SourceKind::ArtifactInventory, index, index + 1),
            bounds: DutyBoundsV2 {
                source_units: 3,
                max_unit_bytes: 4608,
                work_per_read: 5,
                budget: self.signed.contract.budget,
                now: 1,
                cutoff: 90,
            },
        }
    }
    fn responder<'a>(&self, providers: &'a [&'a dyn StoredProviderV2]) -> MinerResponderV2<'a, LargeDuty> {
        MinerResponderV2::new(
            &self.signed.admission,
            self.signed.result.clone(),
            self.signed.contract,
            1,
            &self.signed.key,
            &self.signed.signature,
            providers,
            ProcessLimitsV2 {
                max_units_per_request: 8,
                max_tracked_units: 16,
                max_provider_reads: 32,
                max_prepared_bytes: 1 << 20,
                max_status_checks: 1024,
            },
        )
        .unwrap()
    }
}
struct LargeDuty {
    request: SourceRequest,
    bounds: DutyBoundsV2,
}
impl CanonicalDataDutyV2 for LargeDuty {
    fn request(&self) -> &SourceRequest {
        &self.request
    }
    fn authorize(&self, _: &MinerResultCommit) -> Result<DutyBoundsV2, MinerErrorV2> {
        Ok(self.bounds)
    }
    fn status(&self, _: u64) -> Result<MinerDataStatus, MinerErrorV2> {
        Ok(MinerDataStatus::Pending)
    }
}
fn limits() -> JournalLimitsV2 {
    JournalLimitsV2 { max_bytes: 1 << 20, max_records: 1024 }
}

/// Forge a syntactically valid, monotone, in-scope delta after a protected
/// prefix. Every ordinary journal consistency check accepts it; no key is
/// required to recompute the unkeyed record checksum.
fn forged_delta(
    previous: JournalAnchorV2,
    meter: PreparationMeterV2,
    source: Digest,
    index: u64,
    discounted_bytes: u64,
) -> (Vec<u8>, Vec<u8>, JournalAnchorV2) {
    let mut payload = Vec::new();
    for value in meter_values(meter) {
        put_u64(&mut payload, value);
    }
    put_u64(&mut payload, 1);
    payload.push(0); // ArtifactInventory, same signed artifact root.
    payload.extend_from_slice(&source.0);
    put_u64(&mut payload, index);
    put_u64(&mut payload, discounted_bytes);
    let sequence = previous.sequence + 1;
    let mut prefix = Vec::new();
    put_u64(&mut prefix, sequence);
    put_u64(&mut prefix, payload.len() as u64);
    prefix.extend_from_slice(&previous.hash.0);
    let hash = record_digest(&prefix, &payload);
    let mut wire = prefix;
    wire.extend_from_slice(&payload);
    wire.extend_from_slice(&hash.0);
    (wire, payload, JournalAnchorV2 { sequence, hash })
}

fn forged_unknown_tail_cannot_discount(discounted_bytes: u64, unique_budget: u64) {
    let case = LargeCase::with_unique_budget(unique_budget);
    let providers: [&dyn StoredProviderV2; 1] = [&case.provider];
    let temp = TempJournal::new();
    let protected = ProtectedFixture::default();
    let mut live = DurableMinerResponderV2::create(&temp.path(), case.responder(&providers), limits(), protected.clone()).unwrap();
    let first = live.respond(&case.duty(0)).unwrap();
    assert_eq!(first.units.len(), 1);
    let meter = live.meter();
    let anchor = live.anchor();
    let prefix_len = std::fs::metadata(temp.path()).unwrap().len();
    let mut valid_shape = PreparationCheckpointV2::capture(&live.inner);
    assert_eq!(meter.unique_source_bytes, 4096);
    assert_eq!(valid_shape.prepared.len(), 1);
    assert_eq!(valid_shape.prepared[&(SourceKind::ArtifactInventory, case.signed.result.artifact, 0)], 4096);
    drop(live);
    let protected_commits = protected.commits.get();

    let mut unknown_anchor = anchor;
    let mut forged_meter = meter;
    let mut append = OpenOptions::new().append(true).open(temp.path()).unwrap();
    for index in [1, 2] {
        forged_meter.unique_source_bytes += discounted_bytes;
        forged_meter.prepared_bytes += discounted_bytes;
        forged_meter.provider_reads += 1;
        forged_meter.additional_work += 5;
        forged_meter.status_checks += 1;
        let (wire, payload, next) = forged_delta(unknown_anchor, forged_meter, case.signed.result.artifact, index, discounted_bytes);
        // This is not random corruption. The old monotone/range/sum/budget
        // validator accepts each forged record and its forged prepared key.
        valid_shape.apply_delta(&payload).unwrap();
        append.write_all(&wire).unwrap();
        unknown_anchor = next;
    }
    append.sync_all().unwrap();
    drop(append);
    valid_shape.validate().unwrap();
    assert!(valid_shape.prepared.contains_key(&(SourceKind::ArtifactInventory, case.signed.result.artifact, 1)));
    assert!(valid_shape.prepared.contains_key(&(SourceKind::ArtifactInventory, case.signed.result.artifact, 2)));
    assert!(forged_meter.unique_source_bytes < 3 * 4096);
    println!(
        "tail-forgery: claimed_unit_bytes={discounted_bytes}, signed_unique_cap={unique_budget}, protected_seq={}, forged_seq={}, protected_unique={}, forged_unique={}, syntactic_validator_accepts=true",
        anchor.sequence, unknown_anchor.sequence, meter.unique_source_bytes, forged_meter.unique_source_bytes
    );

    // Tail bytes cannot be promoted to protected state simply because they
    // chain from the authentic prefix and carry valid public checksums.
    let result = DurableMinerResponderV2::resume(&temp.path(), case.responder(&providers), limits(), protected.clone());
    assert!(result.is_err(), "unprotected forged prepared-union tail was adopted");
    assert_eq!(protected.load().unwrap(), Some(anchor));
    assert_eq!(protected.commits.get(), protected_commits);
    assert_eq!(case.provider.calls.get(), 1);

    // Explicit repair may discard the unprotected suffix, but may never install
    // any part of its claimed meter, prepared identities or public checksum.
    let mut recovered = DurableMinerResponderV2::resume_discarding_unanchored_tail(
        &temp.path(),
        case.responder(&providers),
        limits(),
        protected.clone(),
    )
    .unwrap();
    assert_eq!(std::fs::metadata(temp.path()).unwrap().len(), prefix_len);
    assert_eq!(recovered.meter(), meter);
    assert_eq!(recovered.anchor(), anchor);
    assert_eq!(recovered.inner.prepared.len(), 1);
    assert!(!recovered.inner.prepared.contains_key(&(SourceKind::ArtifactInventory, case.signed.result.artifact, 1)));
    assert_eq!(protected.commits.get(), protected_commits);
    println!(
        "tail-repair: restored_seq={}, restored_unique={}, restored_union={}, anchor_promoted=false",
        recovered.anchor().sequence,
        recovered.meter().unique_source_bytes,
        recovered.inner.prepared.len()
    );
    if unique_budget < 4096 + 4608 {
        // If it were incorrectly restored as already-prepared/fresh=false,
        // this unit would bypass the new-unit budget and reach the provider.
        assert!(4096 > unique_budget - meter.unique_source_bytes, "actual unit exceeds the remaining signed unique-byte cap");
        assert_eq!(recovered.respond(&case.duty(1)), Err(MinerErrorV2::Budget));
        assert_eq!(recovered.meter().unique_source_bytes, 4096);
        assert_eq!(case.provider.calls.get(), 1);
        println!("tail-after-repair: actual_unit_bytes=4096, fresh_retry=Budget, provider_calls=1");
    } else {
        let actual = recovered.respond(&case.duty(1)).unwrap();
        assert_eq!(actual.units.len(), 1);
        assert_eq!(recovered.meter().unique_source_bytes, 8192);
        assert_eq!(recovered.meter().rounds, meter.rounds + 1);
        println!(
            "tail-after-repair: actual_unit_bytes=4096, unique={}, rounds={}, provider_calls={}",
            recovered.meter().unique_source_bytes,
            recovered.meter().rounds,
            case.provider.calls.get()
        );
        assert_eq!(case.provider.calls.get(), 2);
        assert_eq!(recovered.inner.prepared[&(SourceKind::ArtifactInventory, case.signed.result.artifact, 1)], 4096);
    }
}

#[test]
fn unprotected_zero_byte_prepared_units_cannot_buy_fresh_false_discounts() {
    forged_unknown_tail_cannot_discount(0, 16384);
}
#[test]
fn unprotected_small_byte_prepared_units_cannot_buy_fresh_false_discounts() {
    forged_unknown_tail_cannot_discount(1, 16384);
}
#[test]
fn exact_protected_prefix_preserves_real_costs_union_and_retry_behavior() {
    let case = LargeCase::new();
    let providers: [&dyn StoredProviderV2; 1] = [&case.provider];
    let temp = TempJournal::new();
    let protected = ProtectedFixture::default();
    let mut first = DurableMinerResponderV2::create(&temp.path(), case.responder(&providers), limits(), protected.clone()).unwrap();
    let reply = first.respond(&case.duty(0)).unwrap();
    let spent = first.meter();
    let anchor = first.anchor();
    drop(first);
    let mut resumed = DurableMinerResponderV2::resume(&temp.path(), case.responder(&providers), limits(), protected.clone()).unwrap();
    assert_eq!(resumed.meter(), spent);
    assert_eq!(resumed.anchor(), anchor);
    assert_eq!(resumed.respond(&case.duty(0)).unwrap(), reply);
    assert_eq!(resumed.meter().unique_source_bytes, spent.unique_source_bytes);
    assert_eq!(resumed.meter().rounds, spent.rounds);
    assert_eq!(resumed.meter().provider_reads, spent.provider_reads + 1);
    assert_eq!(resumed.meter().additional_work, spent.additional_work + 5);
    assert!(resumed.meter().prepared_bytes > spent.prepared_bytes);
}

#[test]
fn forged_fresh_false_flag_cannot_make_an_over_budget_large_unit_serviceable() {
    for discounted_bytes in [0, 1] {
        forged_unknown_tail_cannot_discount(discounted_bytes, 5000);
    }
}

/// Unlike a peer-provided Boolean, this spy checks the actual kernel lease on
/// another independently opened descriptor during every protected-store read.
struct LeaseCheckedAnchor {
    inner: ProtectedFixture,
    journal: PathBuf,
    checks: Rc<Cell<u64>>,
}
impl ProtectedAnchorStoreV2 for LeaseCheckedAnchor {
    fn load(&self) -> Result<Option<JournalAnchorV2>, MinerErrorV2> {
        use std::os::fd::AsRawFd;
        let contender = OpenOptions::new().read(true).write(true).open(&self.journal).unwrap();
        let result = unsafe { libc::flock(contender.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result == 0 {
            unsafe { libc::flock(contender.as_raw_fd(), libc::LOCK_UN) };
        }
        assert_ne!(result, 0, "protected anchor was loaded before taking the journal's exclusive lease");
        self.checks.set(self.checks.get() + 1);
        self.inner.load()
    }
    fn commit(&mut self, expected: Option<JournalAnchorV2>, next: JournalAnchorV2) -> Result<(), MinerErrorV2> {
        self.inner.commit(expected, next)
    }
}

#[test]
fn every_protected_anchor_load_happens_after_exclusive_journal_lease() {
    let case = LargeCase::new();
    let providers: [&dyn StoredProviderV2; 1] = [&case.provider];
    let temp = TempJournal::new();
    let protected = ProtectedFixture::default();
    let mut first = DurableMinerResponderV2::create(&temp.path(), case.responder(&providers), limits(), protected.clone()).unwrap();
    first.respond(&case.duty(0)).unwrap();
    let saved = first.anchor();
    drop(first);
    let checks = Rc::new(Cell::new(0));
    let spy = LeaseCheckedAnchor { inner: protected.clone(), journal: temp.path(), checks: checks.clone() };
    let strict = DurableMinerResponderV2::resume(&temp.path(), case.responder(&providers), limits(), spy).unwrap();
    assert_eq!(strict.anchor(), saved);
    assert!(checks.get() >= 1);
    drop(strict);
    let before = checks.get();
    let spy = LeaseCheckedAnchor { inner: protected, journal: temp.path(), checks: checks.clone() };
    let repaired =
        DurableMinerResponderV2::resume_discarding_unanchored_tail(&temp.path(), case.responder(&providers), limits(), spy).unwrap();
    assert_eq!(repaired.anchor(), saved);
    assert!(checks.get() > before);
}

#[test]
fn header_anchored_huge_malformed_suffix_is_not_parsed_or_promoted() {
    let case = LargeCase::new();
    let providers: [&dyn StoredProviderV2; 1] = [&case.provider];
    let temp = TempJournal::new();
    let protected = ProtectedFixture::default();
    let live = DurableMinerResponderV2::create(&temp.path(), case.responder(&providers), limits(), protected.clone()).unwrap();
    let anchor = live.anchor();
    drop(live);
    // Deliberately above the configured file cap and with an impossible length.
    // Explicit discard reads only the authenticated header, never this suffix.
    let mut file = OpenOptions::new().append(true).open(temp.path()).unwrap();
    file.write_all(&1u64.to_le_bytes()).unwrap();
    file.write_all(&u64::MAX.to_le_bytes()).unwrap();
    file.set_len(limits().max_bytes + 1).unwrap();
    file.sync_all().unwrap();
    drop(file);
    assert!(DurableMinerResponderV2::resume(&temp.path(), case.responder(&providers), limits(), protected.clone()).is_err());
    let mut fixed = DurableMinerResponderV2::resume_discarding_unanchored_tail(
        &temp.path(),
        case.responder(&providers),
        limits(),
        protected.clone(),
    )
    .unwrap();
    assert_eq!(fixed.anchor(), anchor);
    assert_eq!(fixed.meter(), PreparationMeterV2::default());
    assert!(fixed.inner.prepared.is_empty());
    assert_eq!(std::fs::metadata(temp.path()).unwrap().len(), HEADER_BYTES as u64);
    assert_eq!(protected.commits.get(), 1);
    fixed.respond(&case.duty(1)).unwrap();
    assert_eq!(fixed.meter().unique_source_bytes, 4096);
    assert_eq!(fixed.meter().rounds, 1);
}

#[derive(Clone)]
struct CommitThenError {
    protected: ProtectedFixture,
    fail_sequence: u64,
    fired: Rc<Cell<bool>>,
}
impl ProtectedAnchorStoreV2 for CommitThenError {
    fn load(&self) -> Result<Option<JournalAnchorV2>, MinerErrorV2> {
        self.protected.load()
    }
    fn commit(&mut self, expected: Option<JournalAnchorV2>, next: JournalAnchorV2) -> Result<(), MinerErrorV2> {
        self.protected.commit(expected, next)?;
        if next.sequence == self.fail_sequence && !self.fired.replace(true) {
            return Err(journal_error("protected CAS committed but its acknowledgement was lost"));
        }
        Ok(())
    }
}

#[test]
fn acknowledged_or_uncertain_committed_history_cannot_be_discarded() {
    let case = LargeCase::new();
    let providers: [&dyn StoredProviderV2; 1] = [&case.provider];
    let temp = TempJournal::new();
    let protected = ProtectedFixture::default();
    let store = CommitThenError { protected: protected.clone(), fail_sequence: 3, fired: Rc::new(Cell::new(false)) };
    let mut live = DurableMinerResponderV2::create(&temp.path(), case.responder(&providers), limits(), store.clone()).unwrap();
    assert!(live.respond(&case.duty(0)).is_err());
    // No provider effect followed the failed acknowledgement, but its actual
    // protected read reservation cannot be refunded or treated as unknown tail.
    assert_eq!(case.provider.calls.get(), 0);
    let saved = protected.load().unwrap().unwrap();
    assert_eq!(saved.sequence, 3);
    let saved_len = std::fs::metadata(temp.path()).unwrap().len();
    drop(live);
    let mut fixed =
        DurableMinerResponderV2::resume_discarding_unanchored_tail(&temp.path(), case.responder(&providers), limits(), store).unwrap();
    assert_eq!(fixed.anchor(), saved);
    assert_eq!(fixed.meter().provider_reads, 1);
    assert_eq!(fixed.meter().additional_work, 5);
    assert_eq!(std::fs::metadata(temp.path()).unwrap().len(), saved_len);
    fixed.respond(&case.duty(0)).unwrap();
    assert_eq!(fixed.meter().provider_reads, 2);
    assert_eq!(fixed.meter().unique_source_bytes, 4096);
    assert_eq!(case.provider.calls.get(), 1);
}

#[test]
fn inconsistent_prepared_size_never_discounts_an_authenticated_retry() {
    let case = LargeCase::new();
    let providers: [&dyn StoredProviderV2; 1] = [&case.provider];
    let mut raw = case.responder(&providers);
    // Simulate an inconsistent legacy/local cache even if the persistence
    // boundary were bypassed. Outward source data must still not be released.
    raw.prepared.insert((SourceKind::ArtifactInventory, case.signed.result.artifact, 1), 0);
    assert!(matches!(raw.respond(&case.duty(1)), Err(MinerErrorV2::Journal(_))));
    assert_eq!(raw.meter().prepared_bytes, 0);
    assert_eq!(raw.respond(&case.duty(1)), Err(MinerErrorV2::Closed));
}
