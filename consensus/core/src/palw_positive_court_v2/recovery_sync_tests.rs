use super::*;
use crate::palw_positive_court_v2::{
    AdmissionScopeV2,
    coordinator::recovery::{EventOriginV2, RecoveryAdmissionV2, RecoveryEventV2, SavedPartV2},
    coordinator::{CoordinatorLimitsV2, PanelRosterV2, PositiveCoordinatorV2},
    gate::ClaimFormV2,
    service::{CHUNK_BYTES, ServicePolicyV2, chunk_hash_v2},
    tests::{Signed, parts, policy, result},
};
use crate::palw_step_leg::step_merkle_root_v1;
use crate::palw_tir_court_v1::test_support::{TinyExecution, tiny_execution};
use std::io::{Cursor, Read};

fn limits() -> RecoveryLimitsV2 {
    RecoveryLimitsV2 { max_records: 100, max_encoded_bytes: 128 << 20, max_decoded_heap: 128 << 20 }
}
fn transfer() -> RecoveryTransferLimitsV2 {
    RecoveryTransferLimitsV2 { chunk_bytes: 4096, max_chunks: 1 << 18, max_packets: (1 << 18) + 8, max_wire_bytes: 256 << 20 }
}
struct Fixture {
    x: TinyExecution,
    signed: Signed,
    service: ServicePolicyV2,
    attempts: CoordinatorLimitsV2,
}
impl Fixture {
    fn new() -> Self {
        let x = tiny_execution(None);
        let mut r = result(&x);
        let bytes = vec![7; CHUNK_BYTES];
        let service = ServicePolicyV2 {
            version: 2,
            case: r.claim,
            source: step_merkle_root_v1(&[chunk_hash_v2(&r.claim, 0, &bytes)]).unwrap(),
            chunks: 1,
            max_rounds: 1,
            max_unique_bytes: CHUNK_BYTES as u64,
            max_transmitted_bytes: 8 * CHUNK_BYTES as u64,
            work_per_chunk: 1,
            max_work: 4,
            retention_until: 120,
            deadline: 100,
            price_per_chunk: 10,
            hold: 20,
        };
        r.service_policy = service.id();
        let signed = Signed::new(&x, r, policy());
        let attempts = CoordinatorLimitsV2 {
            max_attempts: 2,
            max_reserved_elements: 1 << 45,
            max_reserved_terms: 1 << 45,
            max_reserved_witness_bytes: 128 << 20,
            max_history_bytes: 1 << 20,
            max_batch_parts: 1 << 20,
            max_batch_bytes: 64 << 20,
        };
        Self { x, signed, service, attempts }
    }
    fn fresh(&self) -> PositiveCoordinatorV2 {
        let s = &self.signed;
        PositiveCoordinatorV2::admit(
            AdmissionScopeV2 {
                network: s.result.network,
                branch: s.result.branch,
                ruleset: s.result.ruleset,
                registered_class_id: s.result.job.shape_profile_id,
                required_form: ClaimFormV2::MinerResultV2,
            },
            s.result.clone(),
            s.miner_signature.clone(),
            s.miner_key.clone(),
            self.x.binding.class.clone(),
            s.policy,
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            PanelRosterV2::new(vec![s.panel_key.clone()], 1 << 20).unwrap(),
            self.service.clone(),
            self.attempts,
            1,
        )
        .unwrap()
    }
    fn journal(&self, finish: bool) -> JournaledCoordinatorV2 {
        let mut j = JournaledCoordinatorV2::new(self.fresh(), 1, limits()).unwrap();
        if finish {
            let panel = crate::mldsa87_primitives::mldsa87_key_id(&self.signed.panel_key);
            let events = [
                RecoveryEventV2::Candidate {
                    panel,
                    witness: self.signed.witness.clone(),
                    signature: self.signed.panel_signature.0.clone(),
                },
                RecoveryEventV2::Batch {
                    witness: self.signed.witness.id(),
                    first: 0,
                    parts: parts(&self.x, self.signed.policy).into_iter().map(|(cone, trace)| SavedPartV2 { cone, trace }).collect(),
                },
                RecoveryEventV2::Finish,
            ];
            for (i, e) in events.into_iter().enumerate() {
                j.record(EventOriginV2 { carrier: Hash64::from_bytes([8; 64]), daa: 2 + i as u64 }, e, |_, _| true, |_, _| Ok(()))
                    .unwrap()
                    .unwrap();
            }
        }
        j
    }
    fn scope(&self) -> RecoveryImportScopeV2 {
        let r = &self.signed.result;
        RecoveryImportScopeV2 { network: r.network, branch: r.branch, ruleset: r.ruleset, case: r.claim, result: r.id() }
    }
    fn driver(&self, head: RecoveryHeadV2) -> RecoveryAdapterV2<Context<'_>> {
        RecoveryAdapterV2::new(Context { fixture: self, head }, limits())
    }
}
struct Context<'a> {
    fixture: &'a Fixture,
    head: RecoveryHeadV2,
}
impl RecoveryContextV2 for Context<'_> {
    type TrustedAnchor = Hash64;
    fn resolve(&self, anchor: &Hash64) -> Result<RecoveryAdmissionV2, ErrorV2> {
        if *anchor != self.fixture.signed.result.branch {
            return Err(ErrorV2::Binding);
        }
        Ok(RecoveryAdmissionV2 { coordinator: self.fixture.fresh(), admitted_at: 1, expected: self.head })
    }
    fn authenticate(&self, _: &Hash64, origin: EventOriginV2, _: &RecoveryEventV2) -> bool {
        origin.carrier == Hash64::from_bytes([8; 64])
    }
    fn current_daa(&self) -> u64 {
        6
    }
}
fn receiver(f: &Fixture, j: &JournaledCoordinatorV2) -> RecoveryChunkImportV2<Cursor<Vec<u8>>> {
    RecoveryChunkImportV2::new(
        f.scope(),
        j.head().unwrap(),
        j.encoded().unwrap().len() as u64,
        transfer(),
        limits(),
        Cursor::new(Vec::new()),
    )
    .unwrap()
}
fn send<W: Write>(rx: &mut RecoveryChunkImportV2<W>, scope: RecoveryImportScopeV2, head: RecoveryHeadV2, bytes: &[u8]) {
    for (i, b) in bytes.chunks(transfer().chunk_bytes as usize).enumerate() {
        assert_eq!(rx.accept(scope, head, i as u64, b).unwrap(), RecoveryChunkEffectV2::Appended);
    }
}
fn checked(f: &Fixture, finished: bool) -> CheckedRecoveryImportV2 {
    let j = f.journal(finished);
    let h = j.head().unwrap();
    let mut rx = receiver(f, &j);
    send(&mut rx, f.scope(), h, j.encoded().unwrap());
    rx.finish(&f.driver(h), &f.scope().branch, |c, _| Ok(c.into_inner())).unwrap()
}

#[test]
fn complete_transfer_rechecks_real_court_and_returns_immutable_publish_token() {
    let f = Fixture::new();
    let import = checked(&f, true);
    assert_eq!(import.scope(), f.scope());
    assert_eq!(import.observed_daa(), 6);
    let c = import.state().state().unwrap();
    assert_eq!(c.completion().claim, ClaimStatusV2::Passed);
    assert_eq!(c.current().unwrap().cursor(), f.x.binding.step_leaf_count);
    assert_eq!(import.head(), import.state().head().unwrap());
    assert_eq!(import.encoded(), import.state().encoded().unwrap());
}

#[test]
fn duplicate_resume_counts_packets_without_replaying_bytes_or_resetting_budget() {
    let f = Fixture::new();
    let j = f.journal(true);
    let h = j.head().unwrap();
    let bytes = j.encoded().unwrap();
    let mut rx = receiver(&f, &j);
    let first = &bytes[..4096];
    assert_eq!(rx.accept(f.scope(), h, 0, first).unwrap(), RecoveryChunkEffectV2::Appended);
    let before = rx.meter();
    assert_eq!(rx.accept(f.scope(), h, 0, first).unwrap(), RecoveryChunkEffectV2::Duplicate);
    assert_eq!(rx.meter().accepted_bytes, before.accepted_bytes);
    assert_eq!(rx.meter().packets, before.packets + 1);
    assert_eq!(rx.meter().wire_bytes, before.wire_bytes + ENVELOPE_BYTES + 4096);
    // The exact same receiver resumes after another relay is selected.
    for (i, b) in bytes.chunks(4096).enumerate().skip(1) {
        rx.accept(f.scope(), h, i as u64, b).unwrap();
    }
    let done = rx.finish(&f.driver(h), &f.scope().branch, |c, _| Ok(c.into_inner())).unwrap();
    assert_eq!(done.state().state().unwrap().completion().claim, ClaimStatusV2::Passed);
}

#[test]
fn holes_changed_scope_changed_head_and_changed_duplicate_poison_the_transfer() {
    let f = Fixture::new();
    let j = f.journal(true);
    let h = j.head().unwrap();
    let bytes = j.encoded().unwrap();
    let mut hole = receiver(&f, &j);
    assert!(matches!(hole.accept(f.scope(), h, 1, &bytes[4096..8192]), Err(RecoveryImportErrorV2::Invalid(ErrorV2::Order))));
    assert!(matches!(hole.accept(f.scope(), h, 0, &bytes[..4096]), Err(RecoveryImportErrorV2::Poisoned)));
    let mut changed = receiver(&f, &j);
    let mut scope = f.scope();
    scope.branch = Hash64::from_bytes([91; 64]);
    assert!(matches!(changed.accept(scope, h, 0, &bytes[..4096]), Err(RecoveryImportErrorV2::Invalid(ErrorV2::Binding))));
    let mut changed = receiver(&f, &j);
    let mut head = h;
    head.records += 1;
    assert!(changed.accept(f.scope(), head, 0, &bytes[..4096]).is_err());
    let mut changed = receiver(&f, &j);
    changed.accept(f.scope(), h, 0, &bytes[..4096]).unwrap();
    let mut bad = bytes[..4096].to_vec();
    bad[0] ^= 1;
    assert!(matches!(changed.accept(f.scope(), h, 0, &bad), Err(RecoveryImportErrorV2::Invalid(ErrorV2::Binding))));
}

#[test]
fn missing_tail_corrupt_payload_and_wrong_readback_never_create_a_checked_token() {
    let f = Fixture::new();
    let j = f.journal(true);
    let h = j.head().unwrap();
    let bytes = j.encoded().unwrap();
    let mut missing = receiver(&f, &j);
    missing.accept(f.scope(), h, 0, &bytes[..4096]).unwrap();
    assert!(matches!(
        missing.finish(&f.driver(h), &f.scope().branch, |c, _| Ok(c.into_inner())),
        Err(RecoveryImportErrorV2::Invalid(ErrorV2::Incomplete))
    ));
    let mut corrupt = bytes.to_vec();
    let n = corrupt.len();
    corrupt[n - 1] ^= 1;
    let mut rx = receiver(&f, &j);
    send(&mut rx, f.scope(), h, &corrupt);
    assert!(matches!(
        rx.finish(&f.driver(h), &f.scope().branch, |c, _| Ok(c.into_inner())),
        Err(RecoveryImportErrorV2::Invalid(ErrorV2::Binding))
    ));
    let mut rx = receiver(&f, &j);
    send(&mut rx, f.scope(), h, bytes);
    assert!(matches!(
        rx.finish(&f.driver(h), &f.scope().branch, |c, _| {
            let mut b = c.into_inner();
            b.push(0);
            Ok(b)
        }),
        Err(RecoveryImportErrorV2::Invalid(ErrorV2::Wire))
    ));
}

#[test]
fn unknown_anchor_and_flag_only_archive_do_not_bypass_replay() {
    let f = Fixture::new();
    let j = f.journal(true);
    let h = j.head().unwrap();
    let mut rx = receiver(&f, &j);
    send(&mut rx, f.scope(), h, j.encoded().unwrap());
    assert!(matches!(
        rx.finish(&f.driver(h), &Hash64::from_bytes([1; 64]), |c, _| Ok(c.into_inner())),
        Err(RecoveryImportErrorV2::Invalid(ErrorV2::Binding))
    ));
    let bytes = b"PositivePassed=true";
    let mut hash = journal_hasher(bytes.len() as u64).unwrap();
    hash.update(bytes);
    let head = RecoveryHeadV2 { records: 0, digest: Hash64::from_bytes(hash.finalize().as_bytes().try_into().unwrap()) };
    let mut rx =
        RecoveryChunkImportV2::new(f.scope(), head, bytes.len() as u64, transfer(), limits(), Cursor::new(Vec::new())).unwrap();
    rx.accept(f.scope(), head, 0, bytes).unwrap();
    assert!(rx.finish(&f.driver(head), &f.scope().branch, |c, _| Ok(c.into_inner())).is_err());
}

#[test]
fn transport_caps_bound_packets_and_canonical_last_chunk_before_write() {
    let f = Fixture::new();
    let j = f.journal(true);
    let h = j.head().unwrap();
    let bytes = j.encoded().unwrap();
    let count = (bytes.len() as u64).div_ceil(4096);
    let tight = RecoveryTransferLimitsV2 { max_packets: count, ..transfer() };
    let mut rx = RecoveryChunkImportV2::new(f.scope(), h, bytes.len() as u64, tight, limits(), Cursor::new(Vec::new())).unwrap();
    for _ in 0..count {
        rx.accept(f.scope(), h, 0, &bytes[..4096]).unwrap();
    }
    assert!(matches!(rx.accept(f.scope(), h, 0, &bytes[..4096]), Err(RecoveryImportErrorV2::Invalid(ErrorV2::Budget))));
    let mut rx = receiver(&f, &j);
    assert!(matches!(rx.accept(f.scope(), h, 0, &bytes[..4095]), Err(RecoveryImportErrorV2::Invalid(ErrorV2::Wire))));
    assert!(
        RecoveryChunkImportV2::new(
            f.scope(),
            h,
            bytes.len() as u64,
            RecoveryTransferLimitsV2 { max_chunks: 1, ..transfer() },
            limits(),
            Cursor::new(Vec::new())
        )
        .is_err()
    );
}

#[test]
fn retention_requires_the_same_rechecked_archive_and_all_rooted_horizons() {
    let f = Fixture::new();
    let import = checked(&f, true);
    let horizons = RetentionV2 {
        pending: false,
        evidence_until: 10,
        liability_until: 11,
        fee_until: 12,
        reorg_until: 13,
        archive_available: true,
    };
    let pin = RecoveryRetentionV2::new(import.scope(), import.head(), horizons);
    assert!(!pin.may_prune(13, &import));
    assert!(pin.may_prune(14, &import));
    let mut wrong = import.scope();
    wrong.case = Hash64::from_bytes([77; 64]);
    assert!(!RecoveryRetentionV2::new(wrong, import.head(), horizons).may_prune(14, &import));
    assert!(
        !RecoveryRetentionV2::new(import.scope(), import.head(), RetentionV2 { pending: true, ..horizons }).may_prune(100, &import)
    );
    assert!(
        !RecoveryRetentionV2::new(import.scope(), import.head(), RetentionV2 { archive_available: false, ..horizons })
            .may_prune(100, &import)
    );
    let pending = checked(&f, false);
    assert!(!RecoveryRetentionV2::new(pending.scope(), pending.head(), horizons).may_prune(100, &pending));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn managed_spill_reservation_is_released_after_checked_import_or_rejected_bytes() {
    use kaspa_database::storage_generations::{reserve::FixedBudgetReserver, spill::SpillArea};
    use std::sync::Arc;
    let f = Fixture::new();
    let j = f.journal(true);
    let h = j.head().unwrap();
    let bytes = j.encoded().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let cap = bytes.len() as u64;
    let budget = Arc::new(FixedBudgetReserver::new(cap));
    let area = SpillArea::open(dir.path(), cap, 1, budget.clone()).unwrap();
    let mut rx = RecoveryChunkImportV2::new(f.scope(), h, cap, transfer(), limits(), area.acquire().unwrap()).unwrap();
    send(&mut rx, f.scope(), h, bytes);
    let done = rx
        .finish(&f.driver(h), &f.scope().branch, |mut spill, cap| {
            let mut bytes = Vec::new();
            spill.reader()?.take(cap.checked_add(1).unwrap()).read_to_end(&mut bytes)?;
            Ok(bytes)
        })
        .unwrap();
    assert_eq!(done.state().state().unwrap().completion().claim, ClaimStatusV2::Passed);
    assert_eq!(area.accounting().files, 0);
    assert_eq!(area.accounting().bytes, 0);
    assert!(area.accounting().high_water <= cap);
    assert!(budget.high_water() <= cap);
    let mut rx = RecoveryChunkImportV2::new(f.scope(), h, cap, transfer(), limits(), area.acquire().unwrap()).unwrap();
    let mut bad = bytes.to_vec();
    bad[0] ^= 1;
    send(&mut rx, f.scope(), h, &bad);
    assert!(rx.finish(&f.driver(h), &f.scope().branch, |_, _| panic!("wrong-root bytes must not reach decode")).is_err());
    assert_eq!(area.accounting().files, 0);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn every_declared_scope_field_is_compared_to_the_replayed_admission() {
    let f = Fixture::new();
    let j = f.journal(true);
    let h = j.head().unwrap();
    let bytes = j.encoded().unwrap();
    for field in 0..6 {
        let mut scope = f.scope();
        let other = Hash64::from_bytes([99; 64]);
        match field {
            0 => scope.network = other,
            1 => scope.branch = other,
            2 => scope.ruleset = other,
            3 => scope.case = other,
            4 => scope.result = other,
            _ => (),
        }
        let mut expected = h;
        if field == 5 {
            expected.records += 1;
        }
        let mut rx =
            RecoveryChunkImportV2::new(scope, expected, bytes.len() as u64, transfer(), limits(), Cursor::new(Vec::new())).unwrap();
        send(&mut rx, scope, expected, bytes);
        assert!(matches!(
            rx.finish(&f.driver(h), &f.scope().branch, |c, _| Ok(c.into_inner())),
            Err(RecoveryImportErrorV2::Invalid(ErrorV2::Binding))
        ));
    }
}

#[test]
fn future_carrier_clock_and_same_length_readback_mutation_cannot_publish() {
    let f = Fixture::new();
    let mut future = f.journal(false);
    let panel = crate::mldsa87_primitives::mldsa87_key_id(&f.signed.panel_key);
    future
        .record(
            EventOriginV2 { carrier: Hash64::from_bytes([8; 64]), daa: 7 },
            RecoveryEventV2::Candidate { panel, witness: f.signed.witness.clone(), signature: f.signed.panel_signature.0.clone() },
            |_, _| true,
            |_, _| Ok(()),
        )
        .unwrap()
        .unwrap();
    let h = future.head().unwrap();
    let mut rx = receiver(&f, &future);
    send(&mut rx, f.scope(), h, future.encoded().unwrap());
    assert!(matches!(
        rx.finish(&f.driver(h), &f.scope().branch, |c, _| Ok(c.into_inner())),
        Err(RecoveryImportErrorV2::Invalid(ErrorV2::Binding))
    ));
    let j = f.journal(true);
    let h = j.head().unwrap();
    let mut rx = receiver(&f, &j);
    send(&mut rx, f.scope(), h, j.encoded().unwrap());
    assert!(
        rx.finish(&f.driver(h), &f.scope().branch, |c, _| {
            let mut bytes = c.into_inner();
            bytes[2] ^= 1;
            Ok(bytes)
        })
        .is_err()
    );
    let mut rx = receiver(&f, &j);
    send(&mut rx, f.scope(), h, j.encoded().unwrap());
    assert!(matches!(
        rx.finish(&f.driver(h), &f.scope().branch, |_, _| Err(io::Error::other("readback failed"))),
        Err(RecoveryImportErrorV2::Storage(_))
    ));
}

#[test]
fn a_partial_sink_write_cannot_be_retried_or_finished_and_releases_the_sink() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct FailAfterPrefix {
        prefix: bool,
        dropped: Arc<AtomicBool>,
    }
    impl Write for FailAfterPrefix {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.prefix {
                return Err(io::Error::other("partial disk write"));
            }
            self.prefix = true;
            Ok(bytes.len().min(7))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Drop for FailAfterPrefix {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }
    let f = Fixture::new();
    let j = f.journal(true);
    let h = j.head().unwrap();
    let bytes = j.encoded().unwrap();
    let dropped = Arc::new(AtomicBool::new(false));
    let mut rx = RecoveryChunkImportV2::new(
        f.scope(),
        h,
        bytes.len() as u64,
        transfer(),
        limits(),
        FailAfterPrefix { prefix: false, dropped: dropped.clone() },
    )
    .unwrap();
    assert!(matches!(rx.accept(f.scope(), h, 0, &bytes[..4096]), Err(RecoveryImportErrorV2::Storage(_))));
    assert_eq!(rx.meter().accepted_bytes, 0);
    assert!(matches!(rx.accept(f.scope(), h, 0, &bytes[..4096]), Err(RecoveryImportErrorV2::Poisoned)));
    assert!(matches!(
        rx.finish(&f.driver(h), &f.scope().branch, |_, _| panic!("partial spill must not reach readback")),
        Err(RecoveryImportErrorV2::Poisoned)
    ));
    assert!(dropped.load(Ordering::SeqCst));
}
