//! Bounded compatibility vectors only. Production stream path never creates
//! Vec<Vec<i128>>; this test uses it to pin the pre-existing signed bytes.
use super::*;
use crate::palw_positive_check_v2::full_relation::wide_v2::stream::*;
use std::{
    cell::RefCell,
    io::{Cursor, Read},
    rc::Rc,
};
fn caps() -> StreamRecordLimitsV2 {
    StreamRecordLimitsV2 {
        max_record_bytes: 1 << 20,
        max_claim_values: 64,
        max_read_bytes: 1 << 24,
        max_read_calls: 1 << 20,
        max_extra_heap_bytes: 1 << 30,
        metadata_auth_bytes: 1 << 27,
        signature_workspace_bytes: 1 << 20,
        external_retained_bytes: 1 << 20,
    }
}
fn reserve(bytes: u64) -> Result<StreamRecordLeaseV2, FullRelationError> {
    assert!(bytes > 0);
    Ok(Box::new(()))
}
fn signed(f: &Fixture, s: &Signed, claims: &[Vec<i128>]) -> (StreamPanelRecordV2, Vec<u8>, [u8; PUBKEY_BYTES], [u8; SIGNATURE_BYTES]) {
    let prep = s.prep(f, limits());
    let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([94; 32]);
    let author = principal(panel.verification_key.as_ref());
    let bytes = borsh::to_vec(claims).unwrap();
    let mut reader = Cursor::new(bytes.clone());
    let record = prep.freeze_stream_record(f.binding.clone(), f.generated.clone(), author, &mut reader, caps(), &reserve).unwrap();
    let original_tuple = (
        &s.result.id(),
        &author,
        &f.binding,
        &f.generated,
        &limits().words(),
        &PalwPromptIdsFormV1::Flat,
        &b"fixture-wide-exact-sign".as_slice(),
        &claims,
    );
    let legacy_bytes = borsh::to_vec(&original_tuple).unwrap();
    let digest = Digest(
        blake2b_simd::Params::new().hash_length(64).key(b"fixture-wide-exact").hash(&legacy_bytes).as_bytes().try_into().unwrap(),
    );
    assert_eq!(record.signing_digest(), digest);
    let resident = prep.freeze_record(f.binding.clone(), f.generated.clone(), author, claims.to_vec()).unwrap();
    assert_eq!(record.signing_digest(), resident.signing_digest());
    let key = *panel.verification_key.as_ref();
    let sig = *libcrux_ml_dsa::ml_dsa_87::sign(&panel.signing_key, &record.signing_digest().0, record.signature_context(), [0; 32])
        .unwrap()
        .as_ref();
    (record, bytes, key, sig)
}
fn begin<R: ClaimsReaderV2>(
    f: &Fixture,
    s: &Signed,
    record: &StreamPanelRecordV2,
    reader: R,
    key: [u8; PUBKEY_BYTES],
    sig: [u8; SIGNATURE_BYTES],
) -> StreamFullRelationCheckerV2<R> {
    s.prep(f, limits())
        .begin_stream(
            record.binding().clone(),
            record.generated().to_vec(),
            record.author(),
            record.signing_digest(),
            reader,
            key,
            sig,
            caps(),
            &reserve,
        )
        .unwrap()
}
fn submit<R: ClaimsReaderV2>(f: &Fixture, node: &mut StreamFullRelationCheckerV2<R>) -> Result<StreamSubmitV2, FullRelationError> {
    let r = node.preparation().plan().relation_at(node.checked_relations()).unwrap();
    let m = material(f, &r);
    let t = target(f, &r);
    node.submit(&m.view(), t.as_ref().map(Target::view))
}
#[test]
fn old_record_bytes_match_stream_signature_and_every_wide_relation_is_required_before_eof_outcome() {
    let f = fixture(false);
    let s = Signed::new(&f, false);
    let values = claims(&f, s.prep(&f, limits()).plan());
    let (record, bytes, key, sig) = signed(&f, &s, &values);
    let mut node = begin(&f, &s, &record, Cursor::new(bytes), key, sig);
    assert_eq!(node.finish(), Err(FullRelationError::Incomplete));
    let total = node.preparation().plan().summary().relations;
    for i in 0..total {
        let result = submit(&f, &mut node).unwrap();
        assert_eq!(node.checked_relations(), i + 1);
        if i + 1 < total {
            assert_eq!(result, StreamSubmitV2::Continue);
            assert_eq!(node.finish(), Err(FullRelationError::Incomplete));
        } else {
            assert_eq!(result, StreamSubmitV2::NeedEof);
        }
    }
    assert_eq!(node.checked_legacy_frontier(), f.binding.step_leaf_count);
    assert_eq!(node.finish().unwrap(), CheckOutcome::Matched);
    assert_eq!(node.finish(), Err(FullRelationError::Closed));
}
#[test]
fn signed_bad_panel_claim_is_only_attributed_after_eof_and_never_turns_into_miner_output_fault() {
    let f = fixture(false);
    let s = Signed::new(&f, false);
    let plan = s.prep(&f, limits());
    let mut values = claims(&f, plan.plan());
    let index = plan.plan().iter().find(|r| matches!(r.kind, ExpandedRelationKind::RecomputeNode { .. })).unwrap().index as usize;
    values[index][0] += 1;
    let (record, bytes, key, sig) = signed(&f, &s, &values);
    let mut node = begin(&f, &s, &record, Cursor::new(bytes), key, sig);
    loop {
        if submit(&f, &mut node).unwrap() == StreamSubmitV2::NeedEof {
            break;
        }
    }
    assert_eq!(node.checked_relations(), index as u64);
    assert_eq!(node.finish().unwrap(), CheckOutcome::WitnessRejected);
    s.result.authenticate(&s.key, &s.signature).unwrap();
}
#[test]
fn wrong_signed_miner_output_waits_for_complete_claims_source_coverage_and_eof_signature() {
    let f = fixture(false);
    let s = Signed::new(&f, true);
    let values = claims(&f, s.prep(&f, limits()).plan());
    let (record, bytes, key, sig) = signed(&f, &s, &values);
    let mut node = begin(&f, &s, &record, Cursor::new(bytes), key, sig);
    while submit(&f, &mut node).unwrap() != StreamSubmitV2::NeedEof {}
    assert_eq!(node.checked_relations(), node.preparation().plan().summary().relations);
    assert_eq!(node.finish().unwrap(), CheckOutcome::OutputMismatch);
}
struct Changing {
    bytes: Rc<RefCell<Vec<u8>>>,
    position: usize,
}
impl Read for Changing {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let data = self.bytes.borrow();
        let n = out.len().min(data.len() - self.position);
        out[..n].copy_from_slice(&data[self.position..self.position + n]);
        self.position += n;
        Ok(n)
    }
}
impl ClaimsReaderV2 for Changing {
    fn retained_request_bytes(&self) -> Option<u64> {
        Some(self.bytes.borrow().capacity() as u64 + 128)
    }
    fn scratch_request_bytes(&self) -> Option<u64> {
        Some(0)
    }
}
#[test]
fn post_begin_changed_unread_tape_with_local_mismatch_has_no_panel_or_miner_verdict_on_wrong_root() {
    let f = fixture(false);
    let s = Signed::new(&f, false);
    let values = claims(&f, s.prep(&f, limits()).plan());
    let (record, bytes, key, sig) = signed(&f, &s, &values);
    let storage = Rc::new(RefCell::new(bytes));
    let reader = Changing { bytes: storage.clone(), position: 0 };
    let mut node = begin(&f, &s, &record, reader, key, sig);
    assert_eq!(submit(&f, &mut node).unwrap(), StreamSubmitV2::Continue);
    let next_value = 4 + 4 + values[0].len() * 16 + 4;
    storage.borrow_mut()[next_value] ^= 1;
    loop {
        if submit(&f, &mut node).unwrap() == StreamSubmitV2::NeedEof {
            break;
        }
    }
    assert_eq!(node.finish(), Err(FullRelationError::Delivery));
    assert_eq!(node.finish(), Err(FullRelationError::Closed));
}
struct DiskFail {
    inner: Cursor<Vec<u8>>,
    left: usize,
}
impl Read for DiskFail {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.left == 0 {
            return Err(std::io::Error::other("injected middle disk failure"));
        }
        let n = out.len().min(self.left);
        let got = self.inner.read(&mut out[..n])?;
        self.left -= got;
        Ok(got)
    }
}
impl ClaimsReaderV2 for DiskFail {
    fn retained_request_bytes(&self) -> Option<u64> {
        Some(self.inner.get_ref().capacity() as u64 + 128)
    }
    fn scratch_request_bytes(&self) -> Option<u64> {
        Some(0)
    }
}
#[test]
fn middle_disk_failure_or_invalid_signature_cannot_expose_a_computation_outcome() {
    let f = fixture(false);
    let s = Signed::new(&f, false);
    let values = claims(&f, s.prep(&f, limits()).plan());
    let (record, bytes, key, sig) = signed(&f, &s, &values);
    let mut node = begin(&f, &s, &record, DiskFail { inner: Cursor::new(bytes.clone()), left: 11 }, key, sig);
    assert_eq!(submit(&f, &mut node), Err(FullRelationError::Delivery));
    assert_eq!(node.checked_relations(), 0);
    assert_eq!(node.finish(), Err(FullRelationError::Closed));
    let mut node = begin(&f, &s, &record, Cursor::new(bytes), key, [0; SIGNATURE_BYTES]);
    while submit(&f, &mut node).unwrap() != StreamSubmitV2::NeedEof {}
    assert_eq!(node.finish(), Err(FullRelationError::Signature));
}
#[test]
fn source_delivery_retry_uses_same_claim_without_re_read_or_coverage_advance() {
    let f = fixture(false);
    let s = Signed::new(&f, false);
    let values = claims(&f, s.prep(&f, limits()).plan());
    let (record, bytes, key, sig) = signed(&f, &s, &values);
    let mut node = begin(&f, &s, &record, Cursor::new(bytes), key, sig);
    let r = node.preparation().plan().relation_at(0).unwrap();
    let m = material(&f, &r);
    let mut t = target(&f, &r).unwrap();
    t.preimage.values_le[0] ^= 1;
    assert_eq!(node.submit(&m.view(), Some(t.view())), Err(FullRelationError::Delivery));
    let meter = node.read_meter();
    assert_eq!(node.checked_relations(), 0);
    assert_eq!(meter.relations_read, 1);
    submit(&f, &mut node).unwrap();
    assert_eq!(node.read_meter().read_bytes, meter.read_bytes);
    while node.checked_relations() < node.preparation().plan().summary().relations {
        submit(&f, &mut node).unwrap();
    }
    assert_eq!(node.finish().unwrap(), CheckOutcome::Matched);
}

struct UnknownReader;
impl Read for UnknownReader {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("unknown reader must be refused before IO")
    }
}
impl ClaimsReaderV2 for UnknownReader {}
#[test]
fn unknown_provider_owners_or_denied_mandatory_lease_refuse_before_any_stream_read() {
    let f = fixture(false);
    let s = Signed::new(&f, false);
    let mut unknown = UnknownReader;
    assert!(matches!(
        s.prep(&f, limits()).freeze_stream_record(
            f.binding.clone(),
            f.generated.clone(),
            PrincipalId(Digest([1; 64])),
            &mut unknown,
            caps(),
            &reserve,
        ),
        Err(FullRelationError::Budget)
    ));
    let values = claims(&f, s.prep(&f, limits()).plan());
    let (record, bytes, key, sig) = signed(&f, &s, &values);
    let deny = |_: u64| -> Result<StreamRecordLeaseV2, FullRelationError> { Err(FullRelationError::Budget) };
    let calls = std::cell::Cell::new(0);
    struct NoRead<'a>(&'a std::cell::Cell<u32>);
    impl Read for NoRead<'_> {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            self.0.set(self.0.get() + 1);
            panic!("lease denied")
        }
    }
    impl ClaimsReaderV2 for NoRead<'_> {
        fn retained_request_bytes(&self) -> Option<u64> {
            Some(0)
        }
        fn scratch_request_bytes(&self) -> Option<u64> {
            Some(0)
        }
    }
    assert!(matches!(
        s.prep(&f, limits()).begin_stream(
            record.binding().clone(),
            record.generated().to_vec(),
            record.author(),
            record.signing_digest(),
            NoRead(&calls),
            key,
            sig,
            caps(),
            &deny,
        ),
        Err(FullRelationError::Budget)
    ));
    assert_eq!(calls.get(), 0);
    drop(bytes);
}
#[test]
fn noncanonical_outer_or_claim_lengths_and_trailing_bytes_cannot_reach_an_outcome() {
    let f = fixture(false);
    let s = Signed::new(&f, false);
    let values = claims(&f, s.prep(&f, limits()).plan());
    let (record, bytes, key, sig) = signed(&f, &s, &values);
    let mut outer = bytes.clone();
    outer[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        s.prep(&f, limits()).begin_stream(
            record.binding().clone(),
            record.generated().to_vec(),
            record.author(),
            record.signing_digest(),
            Cursor::new(outer),
            key,
            sig,
            caps(),
            &reserve,
        ),
        Err(FullRelationError::Delivery)
    ));
    let mut bad_claim = bytes.clone();
    bad_claim[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut node = begin(&f, &s, &record, Cursor::new(bad_claim), key, sig);
    assert_eq!(submit(&f, &mut node), Err(FullRelationError::Delivery));
    assert_eq!(node.checked_relations(), 0);
    let mut trailing = bytes;
    trailing.push(0);
    let mut node = begin(&f, &s, &record, Cursor::new(trailing), key, sig);
    while submit(&f, &mut node).unwrap() != StreamSubmitV2::NeedEof {}
    assert_eq!(node.finish(), Err(FullRelationError::Delivery));
}
