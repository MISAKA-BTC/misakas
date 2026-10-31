//! Independent byte/identity/preallocation checks, not native accepted-history
//! or Court/payment/credit authority. Owner resolvers still inspect raw bodies.
use misaka_palw_positive_contract::{identity::*, journal::*, rows::*};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}
struct Counting;
fn mark() {
    if TRACK.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
    }
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        mark();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        mark();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, n: usize) -> *mut u8 {
        mark();
        unsafe { System.realloc(p, layout, n) }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
fn measured<T>(f: impl FnOnce() -> T) -> (T, u64) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TRACK.with(|n| n.set(false));
        }
    }
    ALLOCATIONS.with(|n| n.set(0));
    TRACK.with(|n| n.set(true));
    let reset = Reset;
    let result = f();
    drop(reset);
    (result, ALLOCATIONS.with(Cell::get))
}
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}
fn digest(s: &str) -> Digest {
    Digest(unhex(s).try_into().unwrap())
}
fn vectors() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../docs/design/palw/positive-court/case-journal-vectors-r1.json")).unwrap()
}
fn limits() -> CaseJournalLimits {
    CaseJournalLimits {
        max_record_bytes: 1 << 20,
        max_payload_bytes: 1 << 18,
        max_aux_entries: 4096,
        max_aux_preimage_bytes: 1 << 18,
        max_total_preimage_bytes: 1 << 20,
        max_logical_heap_bytes: 1 << 22,
    }
}
fn fixture(index: usize) -> (CaseJournalRecord, Vec<u8>) {
    let data = vectors();
    let v = &data["vectors"][index];
    let header = CaseJournalHeader {
        version: 2,
        revision: 1,
        scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
        case: CaseId(d(4)),
        result: ResultId(d(5)),
        sequence: v["sequence"].as_u64().unwrap(),
        previous: digest(v["previous_hex"].as_str().unwrap()),
        origin: borsh::from_slice(&unhex(v["origin_hex"].as_str().unwrap())).unwrap(),
        kind: match v["kind"].as_u64().unwrap() {
            0 => CaseJournalEventKind::NativeBody,
            1 => CaseJournalEventKind::NativeTransaction,
            _ => unreachable!(),
        },
    };
    let record = CaseJournalRecord {
        header,
        payload: unhex(v["payload_hex"].as_str().unwrap()),
        aux: v["aux"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| CaseJournalAux {
                material: borsh::from_slice(&unhex(a["material_hex"].as_str().unwrap())).unwrap(),
                preimage: unhex(a["preimage_hex"].as_str().unwrap()),
            })
            .collect(),
    };
    (record, unhex(v["record_hex"].as_str().unwrap()))
}
fn count_offset(record: &CaseJournalRecord) -> usize {
    617 + record.payload.len()
}
fn expected_refs(record: &CaseJournalRecord) -> Vec<AcceptedMaterial> {
    record.aux.iter().map(|a| a.material).collect()
}

#[test]
fn independent_python_vectors_pin_full_records_and_both_keyed_hash_domains() {
    let data = vectors();
    assert_eq!(CASE_JOURNAL_HEADER_BYTES, 613);
    assert_eq!(CASE_JOURNAL_AUX_MATERIAL_BYTES, 280);
    for index in 0..3 {
        let v = &data["vectors"][index];
        let (record, bytes) = fixture(index);
        assert_eq!(record.encode(limits()).unwrap(), bytes);
        assert_eq!(bytes.len() as u64, v["record_bytes"].as_u64().unwrap());
        let view = CaseJournalRecordView::decode(&bytes, limits()).unwrap();
        assert_eq!(view.header(), record.header);
        assert_eq!(view.payload(), record.payload);
        assert_eq!(view.aux_count() as usize, record.aux.len());
        assert_eq!(view.head(), digest(v["head_hex"].as_str().unwrap()));
        assert_eq!(
            view.history(),
            AcceptedMaterial { origin: record.header.origin, digest: digest(v["history_digest_hex"].as_str().unwrap()) }
        );
        assert_eq!(borsh::to_vec(&view.history()).unwrap(), unhex(v["history_material_hex"].as_str().unwrap()));
        assert_ne!(view.head(), view.history().digest);
        view.require_exact_refs(&expected_refs(&record)).unwrap();
        for (decoded, expected) in view.aux().zip(&record.aux) {
            assert_eq!(decoded.material(), expected.material);
            assert_eq!(decoded.material_bytes(), borsh::to_vec(&expected.material).unwrap());
            assert_eq!(decoded.preimage(), expected.preimage);
            assert!(bytes.as_ptr_range().contains(&decoded.material_bytes().as_ptr()));
            assert!(bytes.as_ptr_range().contains(&decoded.preimage().as_ptr()));
        }
        assert!(bytes.as_ptr_range().contains(&view.payload().as_ptr()));
        // A Vec length prefix is not part of the agreed raw-record hash input.
        let wrapped = borsh::to_vec(&bytes).unwrap();
        let wrong = blake2b_simd::Params::new().hash_length(64).key(data["event_domain"].as_str().unwrap().as_bytes()).hash(&wrapped);
        assert_ne!(view.head().0.as_slice(), wrong.as_bytes());
    }
}

#[test]
fn borrowed_decode_refs_iteration_and_identity_hashing_allocate_nothing() {
    let (record, bytes) = fixture(1);
    let refs = expected_refs(&record);
    let _ = CaseJournalRecordView::decode(&bytes, limits()).unwrap().head();
    let ((head, history, aux_bytes, quote, refs_ok), allocations) = measured(|| {
        let view = CaseJournalRecordView::decode(std::hint::black_box(&bytes), limits()).unwrap();
        let aux_bytes = view.aux().map(|a| a.preimage().len()).sum::<usize>();
        (view.head(), view.history(), aux_bytes, view.quote(), view.require_exact_refs(&refs).is_ok())
    });
    assert_eq!(allocations, 0);
    assert_eq!(aux_bytes, 8);
    assert!(refs_ok);
    assert_ne!(head, history.digest);
    assert_eq!(quote.record_bytes, bytes.len() as u64);
    assert_eq!(quote.payload_bytes, record.payload.len() as u64);
    assert_eq!(quote.aux_entries, 2);
    assert_eq!(quote.aux_preimage_bytes, 8);
    assert_eq!(quote.total_preimage_bytes, record.payload.len() as u64 + 8);
    assert_eq!(quote.logical_heap_bytes, quote.total_preimage_bytes + 2 * std::mem::size_of::<CaseJournalAux>() as u64);
}

#[test]
fn every_truncated_prefix_trailing_byte_and_unknown_header_is_rejected_before_allocation() {
    for index in 0..3 {
        let (_, bytes) = fixture(index);
        for end in 0..bytes.len() {
            let (is_error, allocations) = measured(|| CaseJournalRecordView::decode(&bytes[..end], limits()).is_err());
            assert!(is_error, "vector {index}, prefix {end}");
            assert_eq!(allocations, 0, "vector {index}, prefix {end}");
        }
        for (offset, replacement) in [(0, 3), (2, 2), (612, 2), (612, 255)] {
            let mut bad = bytes.clone();
            bad[offset] = replacement;
            let (is_error, allocations) = measured(|| CaseJournalRecordView::decode(&bad, limits()).is_err());
            assert!(is_error);
            assert_eq!(allocations, 0, "vector {index}, offset {offset}, replacement {replacement}");
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(CaseJournalRecordView::decode(&trailing, limits()).is_err());
    }
}

#[test]
fn hostile_payload_aux_counts_and_lengths_cannot_allocate_or_overflow_offsets() {
    let (record, bytes) = fixture(1);
    let count = count_offset(&record);
    let first_preimage_len = count + 4 + 280;
    for offset in [613, count, first_preimage_len] {
        let mut hostile = bytes.clone();
        hostile[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let (is_error, allocations) = measured(|| CaseJournalRecordView::decode(&hostile, limits()).is_err());
        assert!(is_error, "length/count at {offset}");
        assert_eq!(allocations, 0);
        let extreme = CaseJournalLimits {
            max_record_bytes: u64::MAX,
            max_payload_bytes: u64::MAX,
            max_aux_entries: u32::MAX,
            max_aux_preimage_bytes: u64::MAX,
            max_total_preimage_bytes: u64::MAX,
            max_logical_heap_bytes: u64::MAX,
        };
        let (is_error, allocations) = measured(|| CaseJournalRecordView::decode(&hostile, extreme).is_err());
        assert!(is_error, "EOF still bounds hostile count/length under wide local caps");
        assert_eq!(allocations, 0);
    }
}

#[test]
fn initial_sequence_and_nonempty_payload_material_rules_are_enforced() {
    let (mut record, bytes) = fixture(0);
    let mut zero_sequence = bytes.clone();
    zero_sequence[324..332].fill(0);
    assert!(CaseJournalRecordView::decode(&zero_sequence, limits()).is_err());
    record.header.sequence = 0;
    assert!(record.encode(limits()).is_err());
    record.header.sequence = 1;
    record.header.previous = d(1);
    assert!(record.encode(limits()).is_err());
    let mut nonzero_previous = bytes;
    nonzero_previous[332] = 1;
    assert!(CaseJournalRecordView::decode(&nonzero_previous, limits()).is_err());
    for kind in [CaseJournalEventKind::NativeBody, CaseJournalEventKind::NativeTransaction] {
        let (mut record, mut bytes) = fixture(0);
        record.header.kind = kind;
        record.payload.clear();
        assert!(record.encode(limits()).is_err());
        bytes[612] = match kind {
            CaseJournalEventKind::NativeBody => 0,
            CaseJournalEventKind::NativeTransaction => 1,
        };
        bytes[613..617].fill(0);
        let payload_len = bytes.len() - 621;
        bytes.drain(617..617 + payload_len);
        assert!(CaseJournalRecordView::decode(&bytes, limits()).is_err());
    }
    let (_, bytes) = fixture(0);
    for (offset, replacement) in [(617, 3), (619, 2), (621, 8), (621, 255)] {
        let mut bad = bytes.clone();
        bad[offset] = replacement;
        let (is_error, allocations) = measured(|| CaseJournalRecordView::decode(&bad, limits()).is_err());
        assert!(is_error, "body prefix byte {offset}");
        assert_eq!(allocations, 0);
    }
    let (mut record, _) = fixture(1);
    record.aux[0].preimage.clear();
    assert!(record.encode(limits()).is_err());
    let (record, bytes) = fixture(1);
    let len_at = count_offset(&record) + 4 + 280;
    let first_len = record.aux[0].preimage.len();
    let mut empty = bytes;
    empty[len_at..len_at + 4].fill(0);
    empty.drain(len_at + 4..len_at + 4 + first_len);
    assert!(CaseJournalRecordView::decode(&empty, limits()).is_err());
    // Zero auxiliaries are allowed even though every configured limit is finite/nonzero.
    let (_, bytes) = fixture(0);
    let view = CaseJournalRecordView::decode(&bytes, limits()).unwrap();
    assert_eq!(view.aux_count(), 0);
    view.require_exact_refs(&[]).unwrap();
}

#[test]
fn auxiliary_materials_use_strict_serialized_byte_order_and_reject_duplicates() {
    let (mut record, bytes) = fixture(1);
    assert_eq!(record.aux[0].material.origin.accepted_tx_ordinal, 256);
    assert_eq!(record.aux[1].material.origin.accepted_tx_ordinal, 1);
    record.aux.reverse();
    assert!(record.encode(limits()).is_err());
    let (mut record, _) = fixture(1);
    record.aux[1].material = record.aux[0].material;
    assert!(record.encode(limits()).is_err());
    let (record, _) = fixture(1);
    let start = count_offset(&record) + 4;
    let second = start + 280 + 4 + record.aux[0].preimage.len();
    let mut reversed = bytes[..start].to_vec();
    reversed.extend_from_slice(&bytes[second..]);
    reversed.extend_from_slice(&bytes[start..second]);
    assert!(CaseJournalRecordView::decode(&reversed, limits()).is_err());
    let mut duplicate = bytes;
    let first_material: [u8; 280] = duplicate[start..start + 280].try_into().unwrap();
    duplicate[second..second + 280].copy_from_slice(&first_material);
    let (is_error, allocations) = measured(|| CaseJournalRecordView::decode(&duplicate, limits()).is_err());
    assert!(is_error);
    assert_eq!(allocations, 0);
}

#[test]
fn independently_expected_references_reject_missing_extra_reordered_duplicate_or_substituted_material() {
    let (record, bytes) = fixture(1);
    let view = CaseJournalRecordView::decode(&bytes, limits()).unwrap();
    let refs = expected_refs(&record);
    view.require_exact_refs(&refs).unwrap();
    for missing in [&refs[..0], &refs[..1], &refs[1..]] {
        assert!(view.require_exact_refs(missing).is_err());
    }
    let mut reversed = refs.clone();
    reversed.reverse();
    assert!(view.require_exact_refs(&reversed).is_err());
    let mut extra = refs.clone();
    extra.push(AcceptedMaterial { digest: d(99), ..refs[1] });
    assert!(view.require_exact_refs(&extra).is_err());
    let duplicate = [refs[0], refs[0]];
    assert!(view.require_exact_refs(&duplicate).is_err());
    for origin_changed in [false, true] {
        let mut substituted = refs.clone();
        if origin_changed {
            substituted[0].origin.accepted_daa += 1;
        } else {
            substituted[0].digest.0[0] ^= 1;
        }
        let (is_error, allocations) = measured(|| view.require_exact_refs(&substituted).is_err());
        assert!(is_error);
        assert_eq!(allocations, 0);
    }
}

#[test]
fn every_independent_byte_count_and_logical_heap_limit_accepts_t_and_refuses_t_minus_one() {
    let (record, bytes) = fixture(1);
    let quote = CaseJournalRecordView::decode(&bytes, limits()).unwrap().quote();
    for cap in 0..6 {
        let mut exact = limits();
        let boundary = match cap {
            0 => quote.record_bytes,
            1 => quote.payload_bytes,
            2 => u64::from(quote.aux_entries),
            3 => record.aux.iter().map(|a| a.preimage.len() as u64).max().unwrap(),
            4 => quote.total_preimage_bytes,
            _ => quote.logical_heap_bytes,
        };
        let set = |l: &mut CaseJournalLimits, value| match cap {
            0 => l.max_record_bytes = value,
            1 => l.max_payload_bytes = value,
            2 => l.max_aux_entries = u32::try_from(value).unwrap(),
            3 => l.max_aux_preimage_bytes = value,
            4 => l.max_total_preimage_bytes = value,
            _ => l.max_logical_heap_bytes = value,
        };
        set(&mut exact, boundary);
        CaseJournalRecordView::decode(&bytes, exact).unwrap();
        assert_eq!(record.encode(exact).unwrap(), bytes);
        let mut short = exact;
        set(&mut short, boundary - 1);
        let (is_error, allocations) = measured(|| CaseJournalRecordView::decode(&bytes, short).is_err());
        assert!(is_error, "cap {cap}");
        assert_eq!(allocations, 0, "cap {cap}");
        let (encoded, allocations) = measured(|| record.encode(short));
        assert!(encoded.is_err(), "encoder cap {cap}");
        assert_eq!(allocations, 0, "encoder allocated before cap {cap} refusal");
        let mut zero = exact;
        set(&mut zero, 0);
        assert!(CaseJournalRecordView::decode(&bytes, zero).is_err(), "zero limit {cap} is an invalid configuration");
        assert!(record.encode(zero).is_err());
    }
}

#[test]
fn every_full_record_byte_is_committed_when_the_mutated_envelope_remains_decodable() {
    let (_, bytes) = fixture(1);
    let original = CaseJournalRecordView::decode(&bytes, limits()).unwrap();
    for offset in 0..bytes.len() {
        let mut changed = bytes.clone();
        changed[offset] ^= 128;
        if let Ok(view) = CaseJournalRecordView::decode(&changed, limits()) {
            assert_ne!(view.head(), original.head(), "uncommitted event byte {offset}");
            assert_ne!(view.history().digest, original.history().digest, "uncommitted history byte {offset}");
        }
    }
    // Leaf decoding makes no claim that this opaque transaction or any aux
    // preimage was accepted; those literal bytes must reach the native resolver.
    let (_, native_bytes) = fixture(2);
    let native = CaseJournalRecordView::decode(&native_bytes, limits()).unwrap();
    assert_eq!(native.header().kind, CaseJournalEventKind::NativeTransaction);
    assert!(native.payload().ends_with(b"opaque-native-transaction-fixture"));
}
