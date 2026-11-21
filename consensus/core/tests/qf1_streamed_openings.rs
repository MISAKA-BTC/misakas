//! The independently requested tile and the retained Stop use the same canonical tree.
use kaspa_consensus_core::palw_qf1::{TreeBuilder, TreeKind, TreeRoot};

#[test]
fn collecting_two_paths_preserves_each_opening_including_padding_and_coincident_indices() {
    for count in [1u64, 2, 3, 7, 8, 9, 31] {
        let records: Vec<_> = (0..count).map(u64::to_le_bytes).collect();
        let expected = TreeRoot::build([17; 32], TreeKind::Trace, records.iter().map(|r| r.as_slice()), count).unwrap();
        let last = count - 1;
        for first in [None, Some(0), Some(last / 2), Some(last)] {
            let mut tree = TreeBuilder::with_openings([17; 32], TreeKind::Trace, count, count, [first, Some(last)]).unwrap();
            for record in &records {
                tree.push(record).unwrap();
            }
            let (root, [tile, stop]) = tree.finish_with_openings().unwrap();
            assert_eq!(root, expected);
            assert!(expected.verify_opening(last, &records[last as usize], &stop));
            match first {
                Some(index) => assert!(expected.verify_opening(index, &records[index as usize], &tile)),
                None => assert!(tile.is_empty()),
            }
        }
        assert!(TreeBuilder::with_openings([17; 32], TreeKind::Trace, count, count, [Some(0), Some(count)]).is_err());
    }
}
