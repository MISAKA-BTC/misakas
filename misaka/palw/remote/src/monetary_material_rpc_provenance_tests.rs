//! Real op242 pager/header/aux authentication with injected finality only. No
//! fixture header here demonstrates PoW, native finality or computation credit.
use super::*;
use crate::monetary_material_gc::{MonetaryMaterialGcOracleV1, RemoteFinalityAuthorityV1, RemoteProofGcOracleV2};
use kaspa_consensus_core::{
    header::Header,
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_route_policy_v1},
    palw_state_v2::{PalwChainStateV2, PalwStateCarriageV2, PalwStateParamsV2},
};
fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn header(parents: Vec<Hash64>, nonce: u64) -> Header {
    Header::new_finalized(
        1,
        vec![parents].try_into().unwrap(),
        h(0),
        h(0),
        h(0),
        0,
        0,
        nonce,
        kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID,
        0,
        0.into(),
        0,
        h(0),
    )
}
pub(crate) fn empty_page(tag: u64) -> PalwMonetaryAuxPageV1 {
    let params = PalwStateParamsV2::new(100, 100, 10, 20, 50, 1000, h(1), 4, 1000, 100, 1000, 0).unwrap();
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    carriage.kernel_route = Some(PalwKernelRouteStateV1::new(palw_kernel_route_policy_v1(h(tag), h(2)), None, Default::default()));
    let state = carriage.into_state_v2(&params, None, false).unwrap();
    let parent_header = header(vec![h(3)], tag);
    let parent = kaspa_consensus_core::hashing::header::hash(&parent_header);
    let other = kaspa_consensus_core::hashing::header::hash(&header(vec![h(4)], tag + 100));
    let header = header(vec![parent, other], tag).with_palw_state_root(state.state_root());
    let (preimage, offset) = state.state_root_preimage_located_v1();
    let (aux_root, total_rows, next_row, next_offset, fragments) = monetary_aux_fragments_v1(&state, 0, 0, 1024).unwrap();
    PalwMonetaryAuxPageV1 {
        header,
        parent_header,
        state_preimage: preimage,
        fork_choice: None,
        kernel_route_offset: offset.unwrap() as u64,
        aux_root,
        total_rows,
        start_row: 0,
        start_offset: 0,
        next_row,
        next_offset,
        fragments,
    }
}
pub(crate) fn finish(page: &PalwMonetaryAuxPageV1) -> ProvenMonetarySnapshotV2 {
    let pin = kaspa_consensus_core::hashing::header::hash(&page.header);
    let mut pager = MonetaryMaterialProofPagerV1::new(pin, None, page, 1 << 20, 16 << 20, 4).unwrap();
    pager.push(page).unwrap();
    pager.finish_with_recovery_v2().unwrap()
}
pub(crate) struct Finality {
    pub pin: Hash64,
    pub parent: Hash64,
    pub moved: bool,
    pub reads: usize,
}
impl RemoteFinalityAuthorityV1 for Finality {
    fn finalized_pin(&mut self) -> Result<Hash64, String> {
        self.reads += 1;
        Ok(if self.moved && self.reads > 1 { h(999) } else { self.pin })
    }
    fn selected_parent(&mut self, _: Hash64) -> Result<Hash64, String> {
        Ok(self.parent)
    }
    fn is_chain_ancestor_of(&mut self, _: Hash64, _: Hash64) -> Result<bool, String> {
        Err("history unavailable in this fixture".into())
    }
    fn chain_after(&mut self, _: Hash64, _: usize) -> Result<Vec<Hash64>, String> {
        Err("history unavailable in this fixture".into())
    }
}
pub(crate) fn finality(page: &PalwMonetaryAuxPageV1) -> Finality {
    Finality {
        pin: kaspa_consensus_core::hashing::header::hash(&page.header),
        parent: kaspa_consensus_core::hashing::header::hash(&page.parent_header),
        moved: false,
        reads: 0,
    }
}
#[test]
fn completed_aux_proof_keeps_the_exact_pinned_header_and_parent() {
    let page = empty_page(10);
    let snapshot = finish(&page);
    assert_eq!(snapshot.pinned_block(), kaspa_consensus_core::hashing::header::hash(&page.header));
    assert_eq!(snapshot.parent_block(), kaspa_consensus_core::hashing::header::hash(&page.parent_header));
    let mut pager = MonetaryMaterialProofPagerV1::new(snapshot.pinned_block(), None, &page, 1 << 20, 16 << 20, 4).unwrap();
    let mut changed = page.clone();
    changed.parent_header = header(vec![h(4)], 110);
    assert!(
        page.header.parents_by_level.get(0).unwrap().contains(&kaspa_consensus_core::hashing::header::hash(&changed.parent_header))
    );
    assert!(pager.push(&changed).is_err(), "a later page cannot substitute another valid DAG parent");
}
#[test]
fn remote_gc_requires_the_proof_of_its_current_pin_and_selected_parent() {
    let current = empty_page(10);
    let old = empty_page(11);
    let mut oracle = RemoteProofGcOracleV2 { authority: finality(&current), proven_snapshot: |_| Ok(finish(&old)) };
    assert!(oracle.covenant_rows().is_err());
    assert!(oracle.terminal_receipt(h(8), h(9)).is_err());
    assert!(oracle.release_finalized(h(8), h(9), h(7), h(6)).is_err());
    let mut native = finality(&current);
    native.parent = current.header.parents_by_level.get(0).unwrap()[1];
    let mut oracle = RemoteProofGcOracleV2 { authority: native, proven_snapshot: |_| Ok(finish(&current)) };
    assert!(oracle.covenant_rows().is_err(), "proof parent must be selected, not simply in the DAG parent list");
    let mut oracle = RemoteProofGcOracleV2 { authority: finality(&current), proven_snapshot: |_| Ok(finish(&current)) };
    assert_eq!(oracle.covenant_rows().unwrap().0, finality(&current).parent);
    assert!(oracle.terminal_receipt(h(8), h(9)).unwrap().is_none());
}
#[test]
fn finality_movement_during_authenticated_snapshot_read_is_unknown() {
    let current = empty_page(10);
    let mut native = finality(&current);
    native.moved = true;
    let mut oracle = RemoteProofGcOracleV2 { authority: native, proven_snapshot: |_| Ok(finish(&current)) };
    assert!(oracle.covenant_rows().is_err());
}
