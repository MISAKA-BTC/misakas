//! Public, unknown-coordinate localization and honest bounds; these artificial
//! directories do not establish execution, fidelity, native Final or coverage.
use misaka_palw_kernel::{
    flat_detect5::{FlatPublicReadsV5, FlatReadV5},
    merkle5::FlatLeafHashV5,
    seg::{NodeOpeningV1, SegmentedCommitmentsV1},
};
use misaka_palw_sdk::flat_root_locator_v9::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
};

struct Public<'a> {
    directory: &'a SegmentedCommitmentsV1,
    available: RefCell<BTreeSet<(u32, u16, u16)>>,
    pending: Cell<Option<FlatReadV5>>,
    delay: bool,
    malformed: bool,
    calls: Cell<u32>,
}
impl FlatPublicReadsV5 for Public<'_> {
    fn node(&self, u: FlatReadV5) -> Option<NodeOpeningV1> {
        self.calls.set(self.calls.get() + 1);
        if self.delay && !self.available.borrow().contains(&(u.position, u.occurrence, u.node)) {
            self.pending.set(Some(u));
            return None;
        }
        let mut opening = self.directory.node_opening(u.position, u.occurrence, u.node)?;
        if self.malformed {
            opening.commitment[0] ^= 1;
        }
        Some(opening)
    }
    fn hash(&self, _: FlatReadV5) -> Option<FlatLeafHashV5> {
        None
    }
}
fn own(positions: usize, nodes: usize) -> SegmentedCommitmentsV1 {
    SegmentedCommitmentsV1::new(
        (0..positions)
            .map(|q| {
                vec![
                    (0..nodes)
                        .map(|i| {
                            let mut d = [0; 64];
                            d[..8].copy_from_slice(&(q as u64).to_le_bytes());
                            d[8..16].copy_from_slice(&(i as u64).to_le_bytes());
                            d
                        })
                        .collect(),
                ]
            })
            .collect(),
    )
}
fn trial(positions: usize, nodes: usize, faults: &[(usize, usize)], delayed: bool) {
    let own = own(positions, nodes);
    let mut commitments = own.commitments.clone();
    for &(q, n) in faults {
        commitments[q][0][n][40] ^= 1;
    }
    let directory = SegmentedCommitmentsV1::new(commitments);
    let roots = directory.segment_roots();
    let public = Public {
        directory: &directory,
        available: RefCell::new(BTreeSet::new()),
        pending: Cell::new(None),
        delay: delayed,
        malformed: false,
        calls: Cell::new(0),
    };
    let mut locator = FlatRootLocatorV9::new(&own, &roots).unwrap();
    loop {
        match locator.advance(&public) {
            RootFindingV9::Demand(u) => {
                assert!(delayed);
                public.available.borrow_mut().insert((u.position, u.occurrence, u.node));
            }
            RootFindingV9::Clean => {
                assert!(faults.is_empty());
                break;
            }
            RootFindingV9::Divergent(o) => {
                let first = *faults.iter().min().unwrap();
                assert_eq!((o.position as usize, o.node as usize), first);
                assert_eq!(o.occurrence, 0);
                assert!(locator.successful_reads() <= root_reads_bound_v9(positions as u32, nodes as u64).unwrap());
                let before = locator.successful_reads();
                assert_eq!(locator.advance(&public), RootFindingV9::Divergent(o));
                assert_eq!(before, locator.successful_reads());
                break;
            }
            RootFindingV9::Inconsistent(e) => panic!("{positions}/{nodes}/{faults:?}: {e}"),
        }
    }
}
#[test]
fn all_first_last_and_interior_faults_in_carried_odd_trees_are_bounded() {
    for p in [1, 2, 3, 5, 13, 26] {
        for n in [1, 2, 3, 7, 16, 29] {
            for q in 0..p {
                for i in 0..n {
                    trial(p, n, &[(q, i)], false);
                }
            }
        }
    }
}
#[test]
fn multiple_faults_choose_the_first_in_immutable_graph_order() {
    trial(26, 47, &[(25, 46), (17, 0), (17, 39), (0, 38), (0, 12)], false);
}
#[test]
fn delayed_public_answers_resume_without_rescanning_the_prefix() {
    trial(26, 47, &[(25, 46)], true);
}
#[test]
fn public_segment_roots_skip_earlier_clean_segments_without_da() {
    trial(1030, 7, &[(1029, 6)], true);
}
#[test]
fn checkpoint_sized_last_fault_needs_at_most_nineteen_root_reads() {
    assert_eq!(root_reads_bound_v9(26, 7658), Some(19));
    trial(26, 7658, &[(25, 7657)], true);
}
#[test]
fn clean_whole_claim_requires_no_public_value_callback() {
    trial(26, 47, &[], false);
}
#[test]
fn malformed_opening_stays_a_demand_and_cannot_advance_or_convict() {
    let own = own(3, 7);
    let mut c = own.commitments.clone();
    c[2][0][6][40] ^= 1;
    let changed = SegmentedCommitmentsV1::new(c);
    let roots = changed.segment_roots();
    let public = Public {
        directory: &changed,
        available: RefCell::default(),
        pending: Cell::new(None),
        delay: false,
        malformed: true,
        calls: Cell::new(0),
    };
    let mut locator = FlatRootLocatorV9::new(&own, &roots).unwrap();
    assert!(matches!(locator.advance(&public), RootFindingV9::Demand(_)));
    assert_eq!(locator.successful_reads(), 0);
}
#[test]
fn wrong_owned_roots_and_public_segment_counts_are_rejected() {
    let mut own = own(3, 7);
    let roots = own.segment_roots();
    assert!(FlatRootLocatorV9::new(&own, &[]).is_err());
    own.position_roots[0][0] ^= 1;
    assert!(FlatRootLocatorV9::new(&own, &roots).is_err());
}
