//! Independent recovery regressions: expected work, fees and terminal state are
//! asserted directly, rather than by comparing two calls to the same decoder.
use super::*;
use crate::palw_positive_court_v2::{
    coordinator::transport::MaterialTransportLimitsV2,
    gate::ClaimFormV2,
    service::{CHUNK_BYTES, chunk_hash_v2},
    tests::{Signed, parts, policy, result},
};
use crate::palw_step_leg::{step_merkle_root_v1, step_opening_v1};
use crate::palw_tir_court_v1::test_support::{TinyExecution, tiny_execution};
use std::cell::Cell;

fn panel() -> (Hash64, Vec<u8>) {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([32; 32]);
    let bytes = key.verification_key.as_ref().to_vec();
    (mldsa87_key_id(&bytes), bytes)
}
fn fixture(x: &TinyExecution, work: u64) -> (Signed, ServicePolicyV2, Vec<u8>) {
    let bytes = vec![17; CHUNK_BYTES];
    let mut r = result(x);
    let service = ServicePolicyV2 {
        version: 2,
        case: r.claim,
        source: step_merkle_root_v1(&[chunk_hash_v2(&r.claim, 0, &bytes)]).unwrap(),
        chunks: 1,
        max_rounds: 1,
        max_unique_bytes: CHUNK_BYTES as u64,
        max_transmitted_bytes: CHUNK_BYTES as u64 * 8,
        work_per_chunk: 1,
        max_work: work,
        retention_until: 120,
        deadline: 100,
        price_per_chunk: 10,
        hold: 20,
    };
    r.service_policy = service.id();
    (Signed::new(x, r, policy()), service, bytes)
}
fn fresh(x: &TinyExecution, s: &Signed, p: ServicePolicyV2) -> PositiveCoordinatorV2 {
    PositiveCoordinatorV2::admit(
        AdmissionScopeV2 {
            network: s.result.network,
            branch: s.result.branch,
            ruleset: s.result.ruleset,
            registered_class_id: x.binding.job_context.shape_profile_id,
            required_form: ClaimFormV2::MinerResultV2,
        },
        s.result.clone(),
        s.miner_signature.clone(),
        s.miner_key.clone(),
        x.binding.class.clone(),
        s.policy,
        PalwPromptIdsFormV1::Flat,
        PanelRosterV2::new(vec![panel().1], MLDSA87_PUBKEY_LEN as u64).unwrap(),
        p,
        CoordinatorLimitsV2 {
            max_attempts: 2,
            max_reserved_elements: 1 << 45,
            max_reserved_terms: 1 << 45,
            max_reserved_witness_bytes: 256 << 20,
            max_history_bytes: 1 << 20,
            max_batch_parts: 1 << 20,
            max_batch_bytes: 64 << 20,
        },
        1,
    )
    .unwrap()
}
fn caps() -> RecoveryLimitsV2 {
    RecoveryLimitsV2 { max_records: 64, max_encoded_bytes: 128 << 20, max_decoded_heap: 128 << 20 }
}
fn origin(daa: u64) -> EventOriginV2 {
    EventOriginV2 { carrier: Hash64::from_u64_word(23), daa }
}
fn authenticate(o: EventOriginV2, _: &RecoveryEventV2) -> bool {
    o.carrier == origin(0).carrier
}
fn record(j: &mut JournaledCoordinatorV2, event: RecoveryEventV2, daa: u64) -> Result<(), ErrorV2> {
    match j.record(origin(daa), event, authenticate, |_, _| Ok(())) {
        Ok(result) => result,
        Err(RecoveryErrorV2::Invalid(error)) => Err(error),
        Err(error) => panic!("unexpected persistence error: {error:?}"),
    }
}
fn request(j: &JournaledCoordinatorV2) -> RecoveryEventV2 {
    let id = j.state().unwrap().request_id(WorkRequestV2::Source { first: 0, count: 1 }).unwrap();
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([32; 32]);
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, id.as_byte_slice(), SOURCE_REQUEST_CONTEXT, [14; 32]).unwrap();
    RecoveryEventV2::Request { panel: panel().0, first: 0, count: 1, signature: signature.as_ref().to_vec() }
}
fn response(j: &JournaledCoordinatorV2, bytes: &[u8]) -> RecoveryEventV2 {
    let leaf = chunk_hash_v2(&j.state().unwrap().result().claim, 0, bytes);
    RecoveryEventV2::Response { index: 0, bytes: bytes.to_vec(), opening: step_opening_v1(&[leaf], 0).unwrap() }
}

#[test]
fn exhausted_journal_cannot_prevent_authenticated_clock_expiry() {
    let x = tiny_execution(None);
    let (s, p, _) = fixture(&x, 4);
    let limits = RecoveryLimitsV2 { max_records: 1, ..caps() };
    let mut j = JournaledCoordinatorV2::new(fresh(&x, &s, p.clone()), 1, limits).unwrap();
    assert_eq!(record(&mut j, RecoveryEventV2::Finish, 2), Err(ErrorV2::Order));
    assert_eq!(record(&mut j, RecoveryEventV2::Expire, 100), Err(ErrorV2::Budget));
    // Expiry is a deterministic projection of a separately authenticated native
    // clock. A full input archive must not pin resources or an unrefunded hold.
    j.advance_clock(100, |daa| daa == 100).unwrap();
    assert_eq!(j.state().unwrap().completion().claim, ClaimStatusV2::EndedUnverified);
    let meter = j.state().unwrap().service_meter();
    assert_eq!((meter.paid, meter.refunded, meter.remaining), (0, 20, 0));
    assert!(j.advance_clock(99, |_| true).is_err());
    let r = JournaledCoordinatorV2::restore(fresh(&x, &s, p), 1, j.encoded().unwrap(), j.head().unwrap(), limits, 100, authenticate)
        .unwrap();
    assert_eq!(r.state().unwrap().completion().claim, ClaimStatusV2::EndedUnverified);
    assert_eq!(r.state().unwrap().service_meter().refunded, 20);
}

#[test]
fn clock_projection_needs_authentication_and_cannot_unpoison_failed_persistence() {
    let x = tiny_execution(None);
    let (s, p, _) = fixture(&x, 4);
    let mut j = JournaledCoordinatorV2::new(fresh(&x, &s, p), 1, caps()).unwrap();
    assert!(j.advance_clock(100, |_| false).is_err());
    assert_eq!(j.state().unwrap().completion().claim, ClaimStatusV2::Checking);
    assert!(j.record(origin(2), RecoveryEventV2::Finish, authenticate, |_, _| Err(io::Error::other("disk failure"))).is_err());
    assert!(j.advance_clock(100, |_| true).is_err());
    assert!(j.state().is_err());
    assert!(j.head().is_err());
    assert!(j.encoded().is_err());
}

#[test]
fn configured_transport_is_not_a_pristine_journal_admission() {
    let x = tiny_execution(None);
    let (s, p, _) = fixture(&x, 4);
    let mut c = fresh(&x, &s, p);
    c.configure_material_transport(MaterialTransportLimitsV2 {
        max_chunk_bytes: 4096,
        max_buffer_bytes: 1 << 20,
        max_wire_bytes: 8192,
        max_packets: 8,
    })
    .unwrap();
    // Otherwise the initial archive is identical to an unconfigured admission
    // while the in-memory reservation differs and disappears on restart.
    assert!(JournaledCoordinatorV2::new(c, 1, caps()).is_err());
}

#[test]
fn one_bad_source_response_exhausts_work_even_if_restart_precedes_valid_retry() {
    let x = tiny_execution(None);
    let (s, p, bytes) = fixture(&x, 1);
    let mut j = JournaledCoordinatorV2::new(fresh(&x, &s, p.clone()), 1, caps()).unwrap();
    let req = request(&j);
    record(&mut j, req, 2).unwrap();
    let mut bad = response(&j, &bytes);
    if let RecoveryEventV2::Response { bytes, .. } = &mut bad {
        bytes[0] ^= 1;
    }
    assert_eq!(record(&mut j, bad, 3), Err(ErrorV2::Binding));
    let mut r = JournaledCoordinatorV2::restore(fresh(&x, &s, p), 1, j.encoded().unwrap(), j.head().unwrap(), caps(), 4, authenticate)
        .unwrap();
    assert_eq!(r.state().unwrap().service_meter().work, 1);
    assert_eq!(r.state().unwrap().service_meter().paid, 0);
    assert_eq!(r.state().unwrap().service_meter().remaining, 20);
    let good = response(&r, &bytes);
    assert_eq!(record(&mut r, good, 5), Err(ErrorV2::Budget));
    assert_eq!(r.state().unwrap().completion().miner_data, DataResponseStatusV2::Pending);
}

#[test]
fn trusted_head_pins_same_daa_event_order_and_origin_authentication_can_check_ordinal() {
    let x = tiny_execution(None);
    let (s, p, bytes) = fixture(&x, 4);
    let mut j = JournaledCoordinatorV2::new(fresh(&x, &s, p.clone()), 1, caps()).unwrap();
    let req = request(&j);
    record(&mut j, req, 2).unwrap();
    let reply = response(&j, &bytes);
    record(&mut j, reply, 2).unwrap();
    assert_eq!(j.state().unwrap().service_meter().paid, 10);
    let mut image: Image = borsh::from_slice(j.encoded().unwrap()).unwrap();
    image.records.swap(0, 1);
    let swapped = borsh::to_vec(&image).unwrap();
    assert!(
        JournaledCoordinatorV2::restore(fresh(&x, &s, p.clone()), 1, &swapped, j.head().unwrap(), caps(), 3, authenticate).is_err()
    );
    let ordinal = Cell::new(0usize);
    let r = JournaledCoordinatorV2::restore(fresh(&x, &s, p), 1, j.encoded().unwrap(), j.head().unwrap(), caps(), 3, |o, e| {
        let i = ordinal.get();
        ordinal.set(i + 1);
        authenticate(o, e) && matches!((i, e), (0, RecoveryEventV2::Request { .. }) | (1, RecoveryEventV2::Response { .. }))
    })
    .unwrap();
    assert_eq!(ordinal.get(), 2);
    assert_eq!(r.state().unwrap().service_meter().paid, 10);
}

#[test]
fn revoked_pass_cannot_be_restored_by_omitting_the_revocation_tail() {
    let x = tiny_execution(None);
    let (s, p, _) = fixture(&x, 4);
    let mut j = JournaledCoordinatorV2::new(fresh(&x, &s, p.clone()), 1, caps()).unwrap();
    record(
        &mut j,
        RecoveryEventV2::Candidate { panel: panel().0, witness: s.witness.clone(), signature: s.panel_signature.0.clone() },
        2,
    )
    .unwrap();
    let ps = parts(&x, s.policy).into_iter().map(|(cone, trace)| SavedPartV2 { cone, trace }).collect();
    record(&mut j, RecoveryEventV2::Batch { witness: s.witness.id(), first: 0, parts: ps }, 3).unwrap();
    record(&mut j, RecoveryEventV2::Finish, 4).unwrap();
    let old = j.encoded().unwrap().to_vec();
    record(&mut j, RecoveryEventV2::Revoke, 5).unwrap();
    assert!(JournaledCoordinatorV2::restore(fresh(&x, &s, p.clone()), 1, &old, j.head().unwrap(), caps(), 6, authenticate).is_err());
    let restored =
        JournaledCoordinatorV2::restore(fresh(&x, &s, p), 1, j.encoded().unwrap(), j.head().unwrap(), caps(), 6, authenticate)
            .unwrap();
    assert_eq!(restored.state().unwrap().completion().claim, ClaimStatusV2::Revoked);
    assert_eq!(restored.state().unwrap().service_meter().refunded, 20);
}

#[test]
fn untrusted_lengths_and_interrupted_reader_never_allocate_a_decoded_history() {
    struct FailingReader<'a> {
        bytes: &'a [u8],
        successful: usize,
    }
    impl Read for FailingReader<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if self.successful == 0 {
                return Err(io::Error::other("broken import"));
            }
            let count = out.len().min(self.bytes.len()).min(self.successful);
            out[..count].copy_from_slice(&self.bytes[..count]);
            self.bytes = &self.bytes[count..];
            self.successful -= count;
            Ok(count)
        }
    }
    let x = tiny_execution(None);
    let (s, p, _) = fixture(&x, 4);
    let j = JournaledCoordinatorV2::new(fresh(&x, &s, p), 1, caps()).unwrap();
    assert_eq!(read_generation(FailingReader { bytes: j.encoded().unwrap(), successful: 17 }, caps()), Err(ErrorV2::Wire));
    let mut bad = j.encoded().unwrap().to_vec();
    bad[66..70].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(read_generation(&bad[..], caps()), Err(ErrorV2::Budget));
    let mut nested = j.encoded().unwrap().to_vec();
    nested[66..70].copy_from_slice(&1u32.to_le_bytes());
    nested.extend_from_slice(origin(2).carrier.as_byte_slice());
    nested.extend_from_slice(&2u64.to_le_bytes());
    nested.push(0); // Request with an impossible signature vector length.
    nested.extend_from_slice(panel().0.as_byte_slice());
    nested.extend_from_slice(&0u64.to_le_bytes());
    nested.extend_from_slice(&1u64.to_le_bytes());
    nested.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(read_generation(&nested[..], caps()).is_err());
}

// An actual process dies after the immutable generation and selected head are
// synced, before record() can apply any in-memory service change.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "invoked by process_death_after_durable_selection_replays_once"]
fn durable_crash_child() {
    let directory = std::env::var_os("MISAKA_A6_AUDIT_CRASH_DIR").expect("child directory");
    let x = tiny_execution(None);
    let (s, p, _) = fixture(&x, 4);
    let mut j = JournaledCoordinatorV2::new(fresh(&x, &s, p), 1, caps()).unwrap();
    let req = request(&j);
    let _ = j.record(origin(2), req, authenticate, |bytes, h| {
        persist_generation(Path::new(&directory), bytes, h)?;
        let mut selected = std::fs::File::create(Path::new(&directory).join("selected"))?;
        selected.write_all(&h.records.to_le_bytes())?;
        selected.write_all(h.digest.as_byte_slice())?;
        selected.sync_all()?;
        std::fs::File::open(&directory)?.sync_all()?;
        std::process::exit(86);
    });
    panic!("crash must happen inside persistence acknowledgement");
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn process_death_after_durable_selection_replays_once() {
    let dir = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "palw_positive_court_v2::coordinator::recovery::adversarial_tests::durable_crash_child"])
        .env("MISAKA_A6_AUDIT_CRASH_DIR", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(86), "child stderr: {}", String::from_utf8_lossy(&output.stderr));
    let selected = std::fs::read(dir.path().join("selected")).unwrap();
    assert_eq!(selected.len(), 72);
    let h = RecoveryHeadV2 {
        records: u64::from_le_bytes(selected[..8].try_into().unwrap()),
        digest: Hash64::from_bytes(selected[8..].try_into().unwrap()),
    };
    let path = dir.path().join(format!("{}-{}.pcj", h.records, h.digest));
    let archive = read_generation(std::fs::File::open(path).unwrap(), caps()).unwrap();
    let x = tiny_execution(None);
    let (s, p, bytes) = fixture(&x, 4);
    let mut r = JournaledCoordinatorV2::restore(fresh(&x, &s, p), 1, &archive, h, caps(), 3, authenticate).unwrap();
    assert_eq!(r.state().unwrap().service_meter().rounds, 1);
    assert_eq!(r.state().unwrap().service_meter().work, 0);
    assert_eq!(r.state().unwrap().service_meter().remaining, 20);
    let reply = response(&r, &bytes);
    record(&mut r, reply.clone(), 4).unwrap();
    record(&mut r, reply, 5).unwrap();
    assert_eq!(r.state().unwrap().service_meter().paid, 10);
    assert_eq!(r.state().unwrap().service_meter().work, 2);
}

#[test]
fn persistence_unwind_cannot_leave_a_reusable_half_recorded_coordinator() {
    let x = tiny_execution(None);
    let (s, p, _) = fixture(&x, 4);
    let mut j = JournaledCoordinatorV2::new(fresh(&x, &s, p), 1, caps()).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = j.record(origin(2), RecoveryEventV2::Finish, authenticate, |_, _| panic!("storage transaction unwound"));
    }));
    assert!(result.is_err());
    assert!(j.state().is_err());
    assert!(j.head().is_err());
    assert!(j.encoded().is_err());
    assert!(j.record(origin(3), RecoveryEventV2::Expire, authenticate, |_, _| Ok(())).is_err());
}
