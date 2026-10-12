//! Lazy mapped/resident inventory construction uses lasting host admission, with retry.
#![cfg(feature = "node")]
mod node_common;

use kaspa_consensus_core::{Hash64, palw_artifact::verify_artifact_opening_v1};
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir_exec::node::{
    TirArtifactV1, TirParamOpenerV1, TirResidencyPolicyV1, memory::tir_inventory_tree_storage_need_v1, set_tir_memory_reserver_v1,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

struct Held {
    bytes: u64,
    held: Arc<AtomicU64>,
}
impl Drop for Held {
    fn drop(&mut self) {
        self.held.fetch_sub(self.bytes, Ordering::SeqCst);
    }
}

#[test]
fn inventory_host_refusal_retries_and_last_artifact_owner_releases_for_mapped_and_resident() {
    let (held, calls, refused) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)), Arc::new(AtomicBool::new(false)));
    assert!(set_tir_memory_reserver_v1({
        let (held, calls, refused) = (held.clone(), calls.clone(), refused.clone());
        Arc::new(move |role, class, root, bytes| {
            assert_eq!(role, "tir-inventory-tree");
            assert_ne!(class, Hash64::from_bytes([0; 64]));
            assert_ne!(root, Hash64::from_bytes([0; 64]));
            calls.fetch_add(1, Ordering::SeqCst);
            if refused.load(Ordering::SeqCst) {
                return Err("inventory host refusal before construction".into());
            }
            held.fetch_add(bytes, Ordering::SeqCst);
            Ok(Box::new(Held { bytes, held: held.clone() }))
        })
    }));
    let mut cases = 0;
    for (name, program, params) in
        node_common::programs().into_iter().filter(|(_, p, _)| analyze_ranges(p).is_ok() && !p.params.is_empty())
    {
        let path = std::env::temp_dir().join(format!("tir-inventory-memory-{}.palwtir", std::process::id()));
        let mut tensor = |j, layer| params.tensors.get(&(j, layer)).map(|t| t.to_le_bytes()).ok_or_else(|| "missing tensor".into());
        misaka_palw_tir_artifact::write_container_v1(
            &path,
            &program,
            borsh::to_vec(&node_common::layout(&program, 7, 2, 2, 64)).unwrap(),
            [3; 64],
            name,
            &mut tensor,
        )
        .unwrap();
        for resident in [false, true] {
            let policy = if resident { TirResidencyPolicyV1::Bytes(1 << 20) } else { TirResidencyPolicyV1::PageCache };
            let artifact = Arc::new(TirArtifactV1::open_with_residency(&path, policy).unwrap());
            assert_eq!(!artifact.is_mapped(), resident);
            let (root, count) = artifact.inventory_root().unwrap();
            let need = tir_inventory_tree_storage_need_v1(&program, resident).unwrap();
            refused.store(true, Ordering::SeqCst);
            let before = calls.load(Ordering::SeqCst);
            let reads = artifact.residency_stats().map_or(0, |s| s.bytes_read);
            assert_eq!(artifact.inventory_tree().err().unwrap(), "inventory host refusal before construction");
            assert_eq!(calls.load(Ordering::SeqCst), before + 1);
            assert_eq!(held.load(Ordering::SeqCst), 0);
            assert_eq!(artifact.residency_stats().map_or(0, |s| s.bytes_read), reads, "refusal must precede the full hash-tree pass");
            refused.store(false, Ordering::SeqCst);
            let tree = artifact.inventory_tree().unwrap();
            assert_eq!((tree.root(), tree.leaf_count()), (root, count));
            assert_eq!(held.load(Ordering::SeqCst), need);
            let ptr = tree as *const _;
            assert_eq!(artifact.inventory_tree().unwrap() as *const _, ptr);
            assert_eq!(calls.load(Ordering::SeqCst), before + 2, "cached tree keeps its existing lease");
            let opening = artifact.param_opening(0).unwrap();
            verify_artifact_opening_v1(&opening, root).unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), before + 2);
            let final_owner = artifact.clone();
            drop(artifact);
            assert_eq!(held.load(Ordering::SeqCst), need, "eviction cannot release a live artifact tree");
            assert_eq!(final_owner.inventory_tree().unwrap().root(), root);
            drop(final_owner);
            assert_eq!(held.load(Ordering::SeqCst), 0);
            println!(
                "{resident}: {count} inventory leaves, requested {need} B; actual mapped/resident root/opening preserved, not RSS/fee approval"
            );
        }
        let mut hostile = program.clone();
        hostile.params[0].shape = vec![u32::MAX; 6];
        assert!(tir_inventory_tree_storage_need_v1(&hostile, false).is_err());
        assert!(tir_inventory_tree_storage_need_v1(&hostile, true).is_err());
        std::fs::remove_file(path).unwrap();
        cases += 1;
    }
    assert!(cases >= 10);
    println!(
        "{cases} executable corpus classes, each mapped and resident; requested envelopes and real canonical openings checked, no RSS/fee claim"
    );
}
