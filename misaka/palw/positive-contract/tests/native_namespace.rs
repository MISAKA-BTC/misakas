use misaka_palw_positive_contract::{identity::*, native::*, *};
#[test]
fn fixed_native_keys_are_versioned_distinct_and_bounded() {
    let d = Digest([1; 64]);
    let c = CaseId(d);
    let keys = [
        NativeRowKey::Result(ResultId(d)),
        NativeRowKey::ClaimIndex(ClaimId(d)),
        NativeRowKey::Case(c),
        NativeRowKey::Witness(WitnessId(d)),
        NativeRowKey::Coverage(WitnessId(d)),
        NativeRowKey::SourceUnion { case: c, kind: messages::SourceKind::ArtifactInventory, source: d },
        NativeRowKey::ServiceReceipt { case: c, kind: messages::SourceKind::ArtifactInventory, source: d, unit: 0 },
        NativeRowKey::Funding(c),
        NativeRowKey::PacketReceipt { witness: WitnessId(d), first_relation: 0 },
        NativeRowKey::JournalHead(c),
    ];
    let mut all = std::collections::BTreeSet::new();
    for (i, key) in keys.iter().enumerate() {
        let b = key.encode();
        assert_eq!(&b[..4], &[2, 0, 1, 0]);
        assert_eq!(b[4], i as u8);
        assert!(b.len() <= 142);
        assert!(all.insert(b));
    }
    assert_ne!(ruleset_id(d, d), ruleset_id(d, Digest([2; 64])));
    assert_eq!(require_native_activation(), Err(ContractError::NativeAdapterUnassigned));
}
#[test]
fn central_object_reservation_does_not_overlap_an_existing_range() {
    let text = include_str!("../../../../consensus/core/src/palw_lifecycle_objects_v2.rs");
    let table = text.split("pub const PALW_A2_TAG_ALLOCATIONS_V1:").nth(1).unwrap().split("\n];").next().unwrap();
    let ranges: Vec<(u8, u8)> = table
        .lines()
        .filter_map(|l| {
            let line = l.trim().strip_prefix('(')?;
            let mut parts = line.split(',');
            let lo = parts.next()?.trim().parse().ok()?;
            let hi = parts.next()?.trim().parse().ok()?;
            Some((lo, hi))
        })
        .collect();
    assert_eq!(ranges.iter().filter(|(lo, hi)| *lo <= LIFECYCLE_OBJECT_TAG && LIFECYCLE_OBJECT_TAG <= *hi).count(), 1);
    assert!(table.contains("RFC-0044 Positive Court R2"));
    for (i, (lo, hi)) in ranges.iter().enumerate() {
        assert!(lo <= hi);
        for (a, b) in &ranges[i + 1..] {
            assert!(hi < a || b < lo);
        }
    }
    assert_eq!(KERNEL_AUX_TABLE, 151);
    assert_eq!(CERTIFICATION_SUBJECT_PREFIX, b'P');
}
