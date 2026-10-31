use super::*;
use crate::palw_positive_court_v2::{
    gate::ClaimFormV2,
    service::{CHUNK_BYTES, chunk_hash_v2},
    tests::{Signed, parts, policy, result},
};
use crate::palw_step_leg::{step_merkle_root_v1, step_opening_v1};
use crate::palw_tir_court_v1::test_support::{TinyExecution, tiny_execution};

fn panel(seed: u8) -> (Hash64, Vec<u8>) {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32]);
    let bytes = key.verification_key.as_ref().to_vec();
    (mldsa87_key_id(&bytes), bytes)
}
fn sign(seed: u8, id: Hash64, context: &[u8]) -> SignatureV2 {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32]);
    SignatureV2(
        libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, id.as_byte_slice(), context, [seed.wrapping_add(1); 32])
            .unwrap()
            .as_ref()
            .to_vec(),
    )
}
fn setup(x: &TinyExecution) -> (Signed, ServicePolicyV2, Vec<u8>, CoordinatorLimitsV2) {
    let bytes = vec![7; CHUNK_BYTES];
    let mut result = result(x);
    let service = ServicePolicyV2 {
        version: 2,
        case: result.claim,
        source: step_merkle_root_v1(&[chunk_hash_v2(&result.claim, 0, &bytes)]).unwrap(),
        chunks: 1,
        max_rounds: 1,
        max_unique_bytes: CHUNK_BYTES as u64,
        max_transmitted_bytes: CHUNK_BYTES as u64 * 5,
        work_per_chunk: 1,
        max_work: 4,
        retention_until: 120,
        deadline: 100,
        price_per_chunk: 10,
        hold: 20,
    };
    result.service_policy = service.id();
    let signed = Signed::new(x, result, policy());
    let limits = CoordinatorLimitsV2 {
        max_attempts: 2,
        max_reserved_elements: 1 << 45,
        max_reserved_terms: 1 << 45,
        max_reserved_witness_bytes: 128 << 20,
        max_history_bytes: 1 << 20,
        max_batch_parts: 1 << 20,
        max_batch_bytes: 64 << 20,
    };
    (signed, service, bytes, limits)
}
fn coordinator(x: &TinyExecution, signed: &Signed, service: ServicePolicyV2, limits: CoordinatorLimitsV2) -> PositiveCoordinatorV2 {
    PositiveCoordinatorV2::admit(
        AdmissionScopeV2 {
            network: signed.result.network,
            branch: signed.result.branch,
            ruleset: signed.result.ruleset,
            registered_class_id: x.binding.job_context.shape_profile_id,
            required_form: ClaimFormV2::MinerResultV2,
        },
        signed.result.clone(),
        signed.miner_signature.clone(),
        signed.miner_key.clone(),
        x.binding.class.clone(),
        signed.policy,
        PalwPromptIdsFormV1::Flat,
        PanelRosterV2::new(vec![panel(32).1, panel(42).1], 2 * MLDSA87_PUBKEY_LEN as u64).unwrap(),
        service,
        limits,
        1,
    )
    .unwrap()
}
fn caps() -> RecoveryLimitsV2 {
    RecoveryLimitsV2 { max_records: 100, max_encoded_bytes: 128 << 20, max_decoded_heap: 128 << 20 }
}
fn fresh(x: &TinyExecution, s: &Signed, p: ServicePolicyV2, l: CoordinatorLimitsV2) -> JournaledCoordinatorV2 {
    JournaledCoordinatorV2::new(coordinator(x, s, p, l), 1, caps()).unwrap()
}
fn origin(daa: u64) -> EventOriginV2 {
    EventOriginV2 { carrier: Hash64::from_bytes([8; 64]), daa }
}
fn canonical(o: EventOriginV2, _: &RecoveryEventV2) -> bool {
    o.carrier == origin(0).carrier
}
fn record(j: &mut JournaledCoordinatorV2, e: RecoveryEventV2, daa: u64) -> Result<(), ErrorV2> {
    match j.record(origin(daa), e, canonical, |_, _| Ok(())) {
        Ok(outcome) => outcome,
        Err(RecoveryErrorV2::Invalid(error)) => Err(error),
        Err(error) => panic!("unexpected persistence failure: {error:?}"),
    }
}
fn source_request(j: &JournaledCoordinatorV2, seed: u8) -> RecoveryEventV2 {
    let request = WorkRequestV2::Source { first: 0, count: 1 };
    let id = j.state().unwrap().request_id(request).unwrap();
    RecoveryEventV2::Request { panel: panel(seed).0, first: 0, count: 1, signature: sign(seed, id, SOURCE_REQUEST_CONTEXT).0 }
}
fn source_response(j: &JournaledCoordinatorV2, bytes: &[u8]) -> RecoveryEventV2 {
    let leaf = chunk_hash_v2(&j.state().unwrap().result().claim, 0, bytes);
    RecoveryEventV2::Response { index: 0, bytes: bytes.to_vec(), opening: step_opening_v1(&[leaf], 0).unwrap() }
}
fn candidate(s: &Signed) -> RecoveryEventV2 {
    RecoveryEventV2::Candidate { panel: panel(32).0, witness: s.witness.clone(), signature: s.panel_signature.0.clone() }
}
fn batch(x: &TinyExecution, s: &Signed) -> RecoveryEventV2 {
    RecoveryEventV2::Batch {
        witness: s.witness.id(),
        first: 0,
        parts: parts(x, s.policy).into_iter().map(|(cone, trace)| SavedPartV2 { cone, trace }).collect(),
    }
}
fn restore(
    x: &TinyExecution,
    s: &Signed,
    p: ServicePolicyV2,
    l: CoordinatorLimitsV2,
    j: &JournaledCoordinatorV2,
    now: u64,
) -> JournaledCoordinatorV2 {
    JournaledCoordinatorV2::restore(coordinator(x, s, p, l), 1, j.encoded().unwrap(), j.head().unwrap(), caps(), now, canonical)
        .unwrap()
}

#[test]
fn restart_preserves_all_panel_budgets_paid_once_and_resource_reservations() {
    let x = tiny_execution(None);
    let (s, p, bytes, l) = setup(&x);
    let mut j = fresh(&x, &s, p.clone(), l);
    let req = source_request(&j, 32);
    record(&mut j, req, 2).unwrap();
    let mut bad = source_response(&j, &bytes);
    if let RecoveryEventV2::Response { bytes, .. } = &mut bad {
        bytes[0] ^= 1;
    }
    assert_eq!(record(&mut j, bad, 3), Err(ErrorV2::Binding));
    let reply = source_response(&j, &bytes);
    record(&mut j, reply.clone(), 4).unwrap();
    let before = j.state().unwrap().service_meter();
    assert_eq!(before.paid, 10);
    assert_eq!(before.work, 2);
    let reserved = j.state().unwrap().attempts();
    let mut r = restore(&x, &s, p, l, &j, 4);
    assert_eq!(r.state().unwrap().service_meter(), before);
    assert_eq!(r.state().unwrap().attempts(), reserved);
    let req = source_request(&r, 42);
    record(&mut r, req, 5).unwrap();
    assert_eq!(r.state().unwrap().service_meter().rounds, 1);
    record(&mut r, reply.clone(), 6).unwrap();
    record(&mut r, reply.clone(), 7).unwrap();
    assert_eq!(record(&mut r, reply, 8), Err(ErrorV2::Budget));
    let meter = r.state().unwrap().service_meter();
    assert_eq!(meter.paid, 10);
    assert_eq!(meter.work, 4);
    assert_eq!(meter.paid + meter.refunded + meter.remaining, 20);
}

#[test]
fn rejected_candidate_and_partial_batch_failure_survive_restart_and_panel_handoff() {
    let x = tiny_execution(None);
    let (s, p, _, l) = setup(&x);
    let mut j = fresh(&x, &s, p.clone(), l);
    record(&mut j, candidate(&s), 2).unwrap();
    let mut b = batch(&x, &s);
    if let RecoveryEventV2::Batch { parts, .. } = &mut b {
        parts.truncate(2);
        parts[1].cone.clear();
        parts[1].trace = None;
    }
    assert_eq!(record(&mut j, b, 3), Err(ErrorV2::Wire));
    let before = j.state().unwrap().attempts();
    let charged = j.state().unwrap().current().unwrap().charged_bytes();
    let mut r = restore(&x, &s, p.clone(), l, &j, 4);
    assert_eq!(r.state().unwrap().attempts(), before);
    assert_eq!(r.state().unwrap().current().unwrap().charged_bytes(), charged);
    assert_eq!(r.state().unwrap().current().unwrap().cursor(), 1);
    assert_eq!(r.state().unwrap().completion().witness, Some(WitnessStatusV2::Rejected));
    let mut retry = s.witness.clone();
    retry.attempt = 1;
    let e = RecoveryEventV2::Candidate { panel: panel(42).0, signature: sign(42, retry.id(), WITNESS_CONTEXT).0, witness: retry };
    record(&mut r, e, 5).unwrap();
    assert_eq!(r.state().unwrap().attempts().attempts, 2);
    assert_eq!(r.state().unwrap().history().len(), 1);
    let rr = restore(&x, &s, p, l, &r, 6);
    assert_eq!(rr.state().unwrap().attempts().attempts, 2);
    assert_eq!(rr.state().unwrap().history(), r.state().unwrap().history());
}

#[test]
fn signed_malformed_candidate_consumes_attempt_across_restart() {
    let x = tiny_execution(None);
    let (s, p, _, l) = setup(&x);
    let mut j = fresh(&x, &s, p.clone(), l);
    let mut bad = s.witness.clone();
    bad.binding.step_leaf_count += 1;
    let e = RecoveryEventV2::Candidate { panel: panel(32).0, signature: sign(32, bad.id(), WITNESS_CONTEXT).0, witness: bad };
    assert_eq!(record(&mut j, e, 2), Err(ErrorV2::Binding));
    let mut r = restore(&x, &s, p, l, &j, 3);
    assert_eq!(r.state().unwrap().attempts().attempts, 1);
    assert_eq!(record(&mut r, candidate(&s), 4), Err(ErrorV2::Order));
    assert_eq!(r.state().unwrap().completion().claim, ClaimStatusV2::Checking);
}

#[test]
fn full_history_rechecks_computation_even_after_deadline_and_reorg_removes_all_effects() {
    let x = tiny_execution(None);
    let (s, p, bytes, l) = setup(&x);
    let mut j = fresh(&x, &s, p.clone(), l);
    let before = j.encoded().unwrap().to_vec();
    let old_head = j.head().unwrap();
    let req = source_request(&j, 32);
    record(&mut j, req, 2).unwrap();
    let reply = source_response(&j, &bytes);
    record(&mut j, reply, 3).unwrap();
    record(&mut j, candidate(&s), 4).unwrap();
    record(&mut j, batch(&x, &s), 5).unwrap();
    record(&mut j, RecoveryEventV2::Finish, 6).unwrap();
    let r = restore(&x, &s, p.clone(), l, &j, 200);
    assert_eq!(r.state().unwrap().completion().claim, ClaimStatusV2::Passed);
    assert_eq!(r.state().unwrap().current().unwrap().cursor(), x.binding.step_leaf_count);
    assert_eq!(r.state().unwrap().service_meter(), j.state().unwrap().service_meter());
    // Selected history rollback supplies its OWN trusted head; requests,
    // attempts, payment, responsibility and cursor all roll back together.
    let rollback =
        JournaledCoordinatorV2::restore(coordinator(&x, &s, p.clone(), l), 1, &before, old_head, caps(), 7, canonical).unwrap();
    assert_eq!(rollback.state().unwrap().completion().claim, ClaimStatusV2::Checking);
    assert_eq!(rollback.state().unwrap().attempts().attempts, 0);
    assert_eq!(rollback.state().unwrap().service_meter().paid, 0);
    assert_eq!(rollback.state().unwrap().service_meter().remaining, 20);
    assert!(rollback.state().unwrap().current().is_none());
    assert!(JournaledCoordinatorV2::restore(coordinator(&x, &s, p, l), 1, &before, j.head().unwrap(), caps(), 7, canonical).is_err());
}

#[test]
fn downtime_expires_pending_duties_without_extending_deadline_or_double_refund() {
    let x = tiny_execution(None);
    let (s, p, _, l) = setup(&x);
    let mut j = fresh(&x, &s, p.clone(), l);
    let req = source_request(&j, 32);
    record(&mut j, req, 2).unwrap();
    record(&mut j, candidate(&s), 3).unwrap();
    let mut r = restore(&x, &s, p.clone(), l, &j, 100);
    assert_eq!(r.state().unwrap().completion().claim, ClaimStatusV2::EndedUnverified);
    assert_eq!(r.state().unwrap().service_meter().refunded, 20);
    record(&mut r, RecoveryEventV2::Expire, 101).unwrap();
    let rr = restore(&x, &s, p, l, &r, 102);
    assert_eq!(rr.state().unwrap().service_meter().refunded, 20);
    assert_eq!(record(&mut r, candidate(&s), 99), Err(ErrorV2::Binding));
}

#[test]
fn forged_finish_flag_only_snapshot_missing_material_and_unselected_origins_never_pass() {
    let x = tiny_execution(None);
    let (s, p, _, l) = setup(&x);
    let mut j = fresh(&x, &s, p.clone(), l);
    record(&mut j, candidate(&s), 2).unwrap();
    assert_eq!(record(&mut j, RecoveryEventV2::Finish, 3), Err(ErrorV2::Incomplete));
    assert_ne!(restore(&x, &s, p.clone(), l, &j, 4).state().unwrap().completion().claim, ClaimStatusV2::Passed);
    assert!(
        JournaledCoordinatorV2::restore(
            coordinator(&x, &s, p.clone(), l),
            1,
            b"PositivePassed=true",
            j.head().unwrap(),
            caps(),
            4,
            canonical
        )
        .is_err()
    );
    let image: Image = borsh::from_slice(j.encoded().unwrap()).unwrap();
    let cut = Image { schema: SCHEMA, admission: image.admission, records: vec![] };
    let cut = borsh::to_vec(&cut).unwrap();
    assert!(
        JournaledCoordinatorV2::restore(coordinator(&x, &s, p.clone(), l), 1, &cut, j.head().unwrap(), caps(), 4, canonical).is_err()
    );
    assert!(
        JournaledCoordinatorV2::restore(
            coordinator(&x, &s, p.clone(), l),
            1,
            j.encoded().unwrap(),
            j.head().unwrap(),
            caps(),
            4,
            |_, _| false
        )
        .is_err()
    );
    let mut changed = l;
    changed.max_attempts = 1;
    assert!(
        JournaledCoordinatorV2::restore(
            coordinator(&x, &s, p.clone(), changed),
            1,
            j.encoded().unwrap(),
            j.head().unwrap(),
            caps(),
            4,
            canonical
        )
        .is_err()
    );
    let mut different = s.result.clone();
    different.branch = Hash64::from_bytes([99; 64]);
    let signed = Signed::new(&x, different, s.policy);
    assert!(
        JournaledCoordinatorV2::restore(
            coordinator(&x, &signed, p, l),
            1,
            j.encoded().unwrap(),
            j.head().unwrap(),
            caps(),
            4,
            canonical
        )
        .is_err()
    );
}

#[test]
fn archive_limits_and_allocation_preflight_reject_bad_lengths_and_suffixes() {
    let x = tiny_execution(None);
    let (s, p, _, l) = setup(&x);
    let j = fresh(&x, &s, p.clone(), l);
    for n in 0..j.encoded().unwrap().len() {
        assert!(
            JournaledCoordinatorV2::restore(
                coordinator(&x, &s, p.clone(), l),
                1,
                &j.encoded().unwrap()[..n],
                j.head().unwrap(),
                caps(),
                2,
                canonical
            )
            .is_err()
        );
    }
    let mut huge = j.encoded().unwrap().to_vec();
    huge[66..70].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(preflight(&huge, caps()), Err(ErrorV2::Budget));
    let mut suffix = j.encoded().unwrap().to_vec();
    suffix.push(0);
    assert!(preflight(&suffix, caps()).is_err());
    let tiny = RecoveryLimitsV2 { max_records: 0, ..caps() };
    let mut j = JournaledCoordinatorV2::new(coordinator(&x, &s, p, l), 1, tiny).unwrap();
    let h = j.head().unwrap();
    assert!(matches!(
        j.record(origin(2), RecoveryEventV2::Expire, canonical, |_, _| panic!("must not persist")),
        Err(RecoveryErrorV2::Invalid(ErrorV2::Budget))
    ));
    assert_eq!(j.head().unwrap(), h);
}

#[test]
fn real_generation_restart_and_indeterminate_write_fail_closed() {
    let x = tiny_execution(None);
    let (s, p, _, l) = setup(&x);
    let dir = tempfile::tempdir().unwrap();
    let mut j = fresh(&x, &s, p.clone(), l);
    let req = source_request(&j, 32);
    let mut selected = None;
    j.record(origin(2), req, canonical, |bytes, h| {
        selected = Some((persist_generation(dir.path(), bytes, h)?, h));
        Ok(())
    })
    .unwrap()
    .unwrap();
    let (path, h) = selected.unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(persist_generation(dir.path(), &bytes, h).unwrap(), path);
    let mut r = JournaledCoordinatorV2::restore(coordinator(&x, &s, p.clone(), l), 1, &bytes, h, caps(), 3, canonical).unwrap();
    assert_eq!(r.state().unwrap().service_meter().rounds, 1);
    // Simulate disk commit succeeding followed by acknowledgement failure.
    let mut uncertain = None;
    assert!(matches!(
        r.record(origin(4), candidate(&s), canonical, |bytes, h| {
            uncertain = Some((persist_generation(dir.path(), bytes, h)?, h));
            Err(io::Error::other("lost commit acknowledgement"))
        }),
        Err(RecoveryErrorV2::Persistence(_))
    ));
    assert!(matches!(r.state(), Err(RecoveryErrorV2::Indeterminate)));
    assert!(matches!(r.record(origin(5), RecoveryEventV2::Finish, canonical, |_, _| Ok(())), Err(RecoveryErrorV2::Indeterminate)));
    let (path, h) = uncertain.unwrap();
    let bytes = std::fs::read(path).unwrap();
    let resolved = JournaledCoordinatorV2::restore(coordinator(&x, &s, p, l), 1, &bytes, h, caps(), 5, canonical).unwrap();
    assert_eq!(resolved.state().unwrap().attempts().attempts, 1);
    assert_eq!(resolved.state().unwrap().service_meter().rounds, 1);
}

#[test]
fn pending_horizons_and_missing_archive_prevent_pruning() {
    let r = RetentionV2 {
        pending: false,
        evidence_until: 10,
        liability_until: 11,
        fee_until: 12,
        reorg_until: 13,
        archive_available: true,
    };
    assert!(!r.may_prune_for_self_verifying_import(13));
    assert!(r.may_prune_for_self_verifying_import(14));
    assert!(!RetentionV2 { pending: true, ..r }.may_prune_for_self_verifying_import(100));
    assert!(!RetentionV2 { archive_available: false, ..r }.may_prune_for_self_verifying_import(100));
}

#[test]
fn a0_recovery_port_uses_resolved_head_and_bounded_archive_reader() {
    use misaka_palw_positive_contract::ports::Recovery;
    struct Context<'a> {
        x: &'a TinyExecution,
        s: &'a Signed,
        p: ServicePolicyV2,
        l: CoordinatorLimitsV2,
        head: RecoveryHeadV2,
    }
    impl RecoveryContextV2 for Context<'_> {
        type TrustedAnchor = Hash64;
        fn resolve(&self, anchor: &Hash64) -> Result<RecoveryAdmissionV2, ErrorV2> {
            if *anchor != self.s.result.branch {
                return Err(ErrorV2::Binding);
            }
            Ok(RecoveryAdmissionV2 {
                coordinator: coordinator(self.x, self.s, self.p.clone(), self.l),
                admitted_at: 1,
                expected: self.head,
            })
        }
        fn authenticate(&self, _: &Hash64, origin: EventOriginV2, event: &RecoveryEventV2) -> bool {
            canonical(origin, event)
        }
        fn current_daa(&self) -> u64 {
            4
        }
    }
    let x = tiny_execution(None);
    let (s, p, _, l) = setup(&x);
    let mut j = fresh(&x, &s, p.clone(), l);
    let req = source_request(&j, 32);
    record(&mut j, req, 2).unwrap();
    record(&mut j, candidate(&s), 3).unwrap();
    let bytes = read_generation(j.encoded().unwrap(), caps()).unwrap();
    let driver = RecoveryAdapterV2::new(Context { x: &x, s: &s, p, l, head: j.head().unwrap() }, caps());
    let restored = driver.replay(&s.result.branch, bytes.clone()).unwrap();
    assert_eq!(restored.state().unwrap().service_meter(), j.state().unwrap().service_meter());
    assert_eq!(restored.state().unwrap().attempts(), j.state().unwrap().attempts());
    assert!(driver.replay(&Hash64::from_bytes([77; 64]), bytes.clone()).is_err());
    assert!(driver.replay(&s.result.branch, bytes[..bytes.len() - 1].to_vec()).is_err());
    let small = RecoveryLimitsV2 { max_encoded_bytes: bytes.len() as u64 - 1, ..caps() };
    assert_eq!(read_generation(&bytes[..], small), Err(ErrorV2::Budget));
}

#[test]
fn fragmented_material_restarts_with_partial_bytes_and_failed_packet_budget_intact() {
    use super::super::transport::MaterialDescriptorV2;
    let x = tiny_execution(None);
    let (s, p, _, mut l) = setup(&x);
    l.max_reserved_witness_bytes += 1 << 20;
    let mut j = fresh(&x, &s, p.clone(), l);
    record(
        &mut j,
        RecoveryEventV2::ConfigureMaterial {
            max_chunk_bytes: 1 << 16,
            max_buffer_bytes: 1 << 20,
            max_wire_bytes: 1 << 20,
            max_packets: 100,
        },
        2,
    )
    .unwrap();
    record(&mut j, candidate(&s), 3).unwrap();
    let (cone, trace) = parts(&x, s.policy).remove(0);
    let descriptor = MaterialDescriptorV2::for_part(s.witness.id(), 0, &cone, trace.as_deref());
    let mut payload = cone.clone();
    payload.extend_from_slice(trace.as_deref().unwrap_or(&[]));
    let event = |offset, bytes: Vec<u8>| RecoveryEventV2::MaterialChunk {
        witness: descriptor.witness,
        index: descriptor.index,
        cone_bytes: descriptor.cone_bytes,
        trace_bytes: descriptor.trace_bytes,
        payload: descriptor.payload,
        offset,
        bytes,
    };
    record(&mut j, event(0, payload[..17].to_vec()), 4).unwrap();
    assert_eq!(record(&mut j, event(18, vec![7]), 4), Err(ErrorV2::Order));
    record(&mut j, event(0, payload[..17].to_vec()), 4).unwrap();
    let before = j.state().unwrap().material_transport_meter().unwrap();
    assert_eq!(before.packets, 3);
    let mut restored = restore(&x, &s, p.clone(), l, &j, 5);
    assert_eq!(restored.state().unwrap().material_transport_meter(), Some(before));
    assert_eq!(restored.state().unwrap().attempts(), j.state().unwrap().attempts());
    record(&mut restored, event(17, payload[17..].to_vec()), 6).unwrap();
    assert_eq!(restored.state().unwrap().current().unwrap().cursor(), 1);
    assert_eq!(restored.state().unwrap().material_transport_meter().unwrap().checked_parts, 1);
    let restored = restore(&x, &s, p, l, &restored, 7);
    assert_eq!(restored.state().unwrap().current().unwrap().cursor(), 1);
    assert_eq!(restored.state().unwrap().material_transport_meter().unwrap().packets, 4);
}

#[test]
fn legacy_schema_one_replays_but_cannot_smuggle_transport_configuration() {
    let x = tiny_execution(None);
    let (s, p, _, l) = setup(&x);
    let j = fresh(&x, &s, p.clone(), l);
    let mut old = j.encoded().unwrap().to_vec();
    old[..2].copy_from_slice(&1u16.to_le_bytes());
    let mut r =
        JournaledCoordinatorV2::restore(coordinator(&x, &s, p.clone(), l), 1, &old, head(&old, 0), caps(), 2, canonical).unwrap();
    record(&mut r, candidate(&s), 3).unwrap();
    assert_eq!(&r.encoded().unwrap()[..2], &SCHEMA.to_le_bytes());
    let mut l = l;
    l.max_reserved_witness_bytes += 1 << 20;
    let mut r = fresh(&x, &s, p.clone(), l);
    record(
        &mut r,
        RecoveryEventV2::ConfigureMaterial {
            max_chunk_bytes: 1 << 16,
            max_buffer_bytes: 1 << 20,
            max_wire_bytes: 1 << 20,
            max_packets: 100,
        },
        2,
    )
    .unwrap();
    let mut disguised = r.encoded().unwrap().to_vec();
    disguised[..2].copy_from_slice(&1u16.to_le_bytes());
    assert!(
        JournaledCoordinatorV2::restore(coordinator(&x, &s, p, l), 1, &disguised, head(&disguised, 1), caps(), 3, canonical).is_err()
    );
}
