//! RFC-0037 §5.3 / lane F: resume at a collection boundary re-authenticates the stored spill, re-charges the byte
//! budget, drops only a torn tail, and refuses damaged, duplicated or other-generation spills. Coverage refusal of
//! missing / extra / mislocated collections. Node-local verifier only; no wire.
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_row_stream_v1::*;
use kaspa_consensus_core::palw_state_v2::{palw_collection_root_of_entries_v1, palw_state_root_of_preimage_v1};

type Rows = Vec<(Vec<u8>, Vec<u8>)>;
struct Committed {
    preimage: Vec<u8>,
    root: Hash64,
    collections: Vec<(Vec<u8>, Rows, usize)>,
}
fn rows(label: &str, n: usize, width: usize) -> Rows {
    (0..n).map(|i| (format!("{label}-{i:06}").into_bytes(), vec![(i % 251) as u8; width])).collect()
}
fn committed(spec: &[(&str, usize, usize)]) -> Committed {
    let mut preimage = 5u16.to_le_bytes().to_vec();
    let mut collections = Vec::new();
    for (i, (label, n, width)) in spec.iter().enumerate() {
        preimage.extend_from_slice(&[i as u8; 3]);
        let r = rows(label, *n, *width);
        let root = palw_collection_root_of_entries_v1(label.as_bytes(), r.len(), r.iter().cloned());
        let offset = preimage.len();
        preimage.extend_from_slice(root.as_byte_slice());
        collections.push((label.as_bytes().to_vec(), r, offset));
    }
    let root = palw_state_root_of_preimage_v1(&preimage);
    Committed { preimage, root, collections }
}
fn three() -> Committed {
    committed(&[("bonds", 6, 40), ("classes", 5, 30), ("claims", 7, 20)])
}
fn limits() -> PalwRowSpillLimitsV1 {
    PalwRowSpillLimitsV1 { max_bytes: 1 << 20, max_collections: 16, max_rows: 1024, max_label_bytes: 64, max_key_bytes: 64, max_value_bytes: 256 }
}
fn emit(v: &mut PalwRowStreamVerifierV1<Vec<u8>>, c: &Committed, i: usize, upto: Option<usize>) {
    let (label, rows, offset) = &c.collections[i];
    v.begin_collection(label, rows.len() as u64, *offset).unwrap();
    for (k, val) in rows.iter().take(upto.unwrap_or(rows.len())) {
        v.row(k, val).unwrap();
    }
    if upto.is_none() {
        v.end_collection().unwrap();
    }
}
fn full_spill(c: &Committed) -> Vec<u8> {
    let mut v = PalwRowStreamVerifierV1::new(c.root, c.preimage.clone(), Vec::new(), 1 << 20).unwrap();
    for i in 0..c.collections.len() {
        emit(&mut v, c, i, None);
    }
    v.finish().unwrap().into_sink()
}
fn collection_end(c: &Committed, count: usize) -> usize {
    let mut v = PalwRowStreamVerifierV1::new(c.root, c.preimage.clone(), Vec::new(), 1 << 20).unwrap();
    for i in 0..count {
        emit(&mut v, c, i, None);
    }
    v.finish().unwrap().into_sink().len()
}
fn resume(c: &Committed, spill: &[u8], budget: u64) -> Result<PalwRowStreamResumedV1<Vec<u8>>, PalwRowStreamErrorV1> {
    let owned = spill.to_vec();
    palw_resume_row_stream_v1(c.root, c.preimage.clone(), spill, limits(), budget, |_| None, move |at| {
        let mut sink = owned;
        sink.truncate(at as usize);
        Ok(sink)
    })
}

#[test]
fn torn_tail_is_dropped_to_the_last_boundary_and_the_finished_spill_is_byte_identical() {
    let c = three();
    let whole = full_spill(&c);
    // Interrupted: two collections complete, the third cut mid-row.
    let mut partial = whole[..collection_end(&c, 2) + 20].to_vec();
    partial.truncate(collection_end(&c, 2) + 20);
    let resumed = resume(&c, &partial, 1 << 20).unwrap();
    assert!(resumed.torn_tail);
    assert_eq!(resumed.collections_replayed, 2);
    assert_eq!(resumed.boundary_bytes as usize, collection_end(&c, 2));
    let mut v = resumed.verifier;
    emit(&mut v, &c, 2, None);
    let layout: Vec<(&[u8], usize)> = c.collections.iter().map(|(l, _, o)| (l.as_slice(), *o)).collect();
    let verified = v.finish_covering(&layout).unwrap();
    assert_eq!(verified.collections().len(), 3);
    assert_eq!(verified.into_sink(), whole, "a resumed stream writes exactly what an uninterrupted one writes");
}

#[test]
fn a_clean_boundary_resumes_without_a_torn_tail_and_every_cut_point_is_safe() {
    let c = three();
    let whole = full_spill(&c);
    for cut in 0..=whole.len() {
        match resume(&c, &whole[..cut], 1 << 20) {
            Ok(r) => {
                assert!(r.boundary_bytes as usize <= cut);
                assert_eq!(r.torn_tail, r.boundary_bytes as usize != cut);
                // Whatever survives is exactly a complete-collection prefix of the uninterrupted spill.
                let ends = [0, collection_end(&c, 1), collection_end(&c, 2), collection_end(&c, 3)];
                assert!(ends.contains(&(r.boundary_bytes as usize)), "cut {cut} left boundary {}", r.boundary_bytes);
            }
            Err(e) => panic!("a plain prefix is never damage (cut {cut}): {e}"),
        }
    }
}

#[test]
fn replayed_rows_are_reauthenticated_a_flipped_byte_in_a_completed_collection_refuses_the_spill() {
    let c = three();
    let mut spill = full_spill(&c);
    let inside_first = collection_end(&c, 1) - 5;
    spill[inside_first] ^= 1;
    assert!(matches!(resume(&c, &spill, 1 << 20), Err(PalwRowStreamErrorV1::ResumeSpill(_))));
}

#[test]
fn another_generation_is_refused_by_preimage_and_by_collection() {
    let c = three();
    let spill = full_spill(&c);
    let other = committed(&[("bonds", 6, 41), ("classes", 5, 30), ("claims", 7, 20)]);
    // The other state's preimage/root against this spill: the first collection no longer sits in the preimage.
    assert!(matches!(resume(&other, &spill, 1 << 20), Err(PalwRowStreamErrorV1::ResumeSpill(_))));
    // A preimage that does not hash to the header-derived root is refused before the spill is read.
    let err = palw_resume_row_stream_v1(other.root, c.preimage.clone(), &spill[..], limits(), 1 << 20, |_| None, |_| Ok(Vec::new()))
        .err()
        .unwrap();
    assert_eq!(err, PalwRowStreamErrorV1::PreimageDoesNotMatchRoot(other.root));
}

#[test]
fn a_repeated_collection_in_the_spill_is_refused() {
    let c = three();
    let one = full_spill(&committed(&[("bonds", 6, 40)]));
    let mut spill = one.clone();
    spill.extend_from_slice(&one);
    let single = committed(&[("bonds", 6, 40)]);
    assert!(matches!(resume(&single, &spill, 1 << 20), Err(PalwRowStreamErrorV1::ResumeSpill(_))));
    let _ = c;
}

#[test]
fn the_byte_budget_is_recharged_for_replayed_rows_and_carries_into_the_resumed_stream() {
    let c = three();
    let whole = full_spill(&c);
    let prefix = whole[..collection_end(&c, 2)].to_vec();
    let rows_two: u64 = c.collections[..2].iter().flat_map(|(_, r, _)| r.iter()).map(|(k, v)| (k.len() + v.len()) as u64 + 9).sum();
    // A budget smaller than the replayed prefix cannot be escaped by resuming.
    assert_eq!(resume(&c, &prefix, rows_two - 1).err(), Some(PalwRowStreamErrorV1::ByteBudget(rows_two - 1)));
    // Exactly the prefix: accepted, and already charged, so the next row has no room.
    let resumed = resume(&c, &prefix, rows_two).unwrap();
    assert_eq!(resumed.verifier.row_bytes(), rows_two);
    let mut v = resumed.verifier;
    v.begin_collection(&c.collections[2].0, c.collections[2].1.len() as u64, c.collections[2].2).unwrap();
    let (k, val) = &c.collections[2].1[0];
    assert_eq!(v.row(k, val), Err(PalwRowStreamErrorV1::ByteBudget(rows_two)));
}

#[test]
fn local_spill_limits_are_a_local_refusal_not_a_peer_fault() {
    let c = three();
    let whole = full_spill(&c);
    let mut small = limits();
    small.max_bytes = 16;
    let err = palw_resume_row_stream_v1(c.root, c.preimage.clone(), &whole[..], small, 1 << 20, |_| None, |_| Ok(Vec::new())).err().unwrap();
    assert!(matches!(err, PalwRowStreamErrorV1::ResumeLocalLimit(_)), "{err:?}");
}

#[test]
fn coverage_refuses_missing_extra_mislocated_and_duplicated_layouts() {
    let c = three();
    let build = |names: &[usize]| {
        let mut v = PalwRowStreamVerifierV1::new(c.root, c.preimage.clone(), Vec::new(), 1 << 20).unwrap();
        for i in names {
            emit(&mut v, &c, *i, None);
        }
        v
    };
    let layout: Vec<(&[u8], usize)> = c.collections.iter().map(|(l, _, o)| (l.as_slice(), *o)).collect();
    // Omitting `claims` passes every per-collection root check but is a typed coverage fault here.
    assert_eq!(build(&[0, 1]).finish_covering(&layout).err(), Some(PalwRowStreamErrorV1::MissingCollection(b"claims".to_vec())));
    assert!(build(&[0, 1]).finish().is_ok(), "plain finish cannot see an omission; this is why coverage exists");
    // An extra collection the layout does not name.
    assert_eq!(build(&[0, 1, 2]).finish_covering(&layout[..2]).err(), Some(PalwRowStreamErrorV1::UnexpectedCollection(b"claims".to_vec())));
    // Right label at the wrong preimage offset.
    let mut moved = layout.clone();
    moved[0].1 += 1;
    assert!(matches!(build(&[0, 1, 2]).finish_covering(&moved), Err(PalwRowStreamErrorV1::CollectionOffsetDiffers { .. })));
    // A layout naming one collection twice.
    let mut dup = layout.clone();
    dup.push(layout[0]);
    assert!(matches!(build(&[0, 1, 2]).finish_covering(&dup), Err(PalwRowStreamErrorV1::RequiredLayoutDuplicate(_))));
    assert!(build(&[0, 1, 2]).finish_covering(&layout).is_ok());
}
