//! A composite's full-subtree temporary refold is distinct from its persistent tree.
#![cfg(feature = "node")]
mod node_common;
mod residency_common;
use node_common::tircommon;

use kaspa_consensus_core::{
    Hash64,
    palw_improve_composite_v1::{palw_tir_composite_carriage_v1, palw_tir_composite_ref_v1},
};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirArtifactV1, TirFileMirrorV1, TirParamOpenerV1, TirParamsSourceV1, TirResidencyPolicyV1, fetch_shard_params_v1,
        set_tir_memory_reserver_v1,
    },
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

struct Held {
    role: &'static str,
    bytes: u64,
    held: Arc<Mutex<BTreeMap<&'static str, u64>>>,
}
impl Drop for Held {
    fn drop(&mut self) {
        *self.held.lock().unwrap().get_mut(self.role).unwrap() -= self.bytes;
    }
}

#[test]
fn mapped_and_resident_composite_carriage_refold_refuses_before_copy_and_preserves_legacy_bytes() {
    let held = Arc::new(Mutex::new(BTreeMap::new()));
    let limit = Arc::new(AtomicU64::new(u64::MAX));
    let calls = Arc::new(Mutex::new(Vec::new()));
    assert!(set_tir_memory_reserver_v1({
        let (held, limit, calls) = (held.clone(), limit.clone(), calls.clone());
        Arc::new(move |role, class, root, bytes| {
            assert!(matches!(role, "tir-inventory-tree" | "tir-composite-carriage"));
            assert_ne!(class, Hash64::from_bytes([0; 64]));
            assert_ne!(root, Hash64::from_bytes([0; 64]));
            calls.lock().unwrap().push((role, bytes));
            if role == "tir-composite-carriage" && bytes > limit.load(Ordering::SeqCst) {
                return Err("full-subtree scratch refused".into());
            }
            *held.lock().unwrap().entry(role).or_default() += bytes;
            Ok(Box::new(Held { role, bytes, held: held.clone() }))
        })
    }));
    let dir = std::env::temp_dir().join(format!("tir-composite-memory-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (parent, gens) = residency_common::many_expert_moe(64, 2, 2);
    let parent = node_common::flat(parent);
    let (candidate, cgens) = residency_common::many_expert_moe_candidate(64, 2, 2);
    let candidate = node_common::flat(candidate);
    let pparams = tircommon::models::materialize(&parent, &gens, 41);
    let mut cparams = tircommon::models::materialize(&candidate, &cgens, 41);
    for (key, tensor) in &pparams.tensors {
        cparams.tensors.insert(*key, tensor.clone());
    }
    let ppath = dir.join("parent.palwtir");
    misaka_palw_tir_artifact::write_container_v1(
        &ppath,
        &parent,
        borsh::to_vec(&node_common::layout(&parent, 5, 2, 2, 64)).unwrap(),
        [2; 64],
        "parent".into(),
        &mut |j, l| pparams.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| "missing parent".into()),
    )
    .unwrap();
    let parent_file = TirArtifactV1::open(&ppath).unwrap();
    let (parent_root, split) = parent_file.inventory_root().unwrap();
    let parent_class = parent_file.class().unwrap().class_id(&parent_root);
    let plan = TirPlan::compile(&candidate).unwrap();
    let params = TirParams::from_map(&plan, &cparams).unwrap();
    let src = TirParamsSourceV1(&params);
    let r = palw_tir_composite_ref_v1(parent_class, &candidate, parent.params.len() as u32, &src).unwrap();
    assert_eq!(r.parent_root, parent_root);
    let spath = dir.join("candidate.palwtirs");
    misaka_palw_tir_artifact::write_section_v1(
        &spath,
        &candidate,
        parent.params.len() as u32,
        borsh::to_vec(&node_common::layout(&candidate, 5, 2, 2, 64)).unwrap(),
        [2; 64],
        "candidate".into(),
        &mut |j, l| cparams.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| "missing candidate".into()),
    )
    .unwrap();

    for resident in [false, true] {
        let policy = if resident { TirResidencyPolicyV1::Bytes(1 << 20) } else { TirResidencyPolicyV1::PageCache };
        let artifact = Arc::new(TirArtifactV1::open_composite_with_residency(&ppath, &spath, &r, Some(parent_root), policy).unwrap());
        assert_eq!(!artifact.is_mapped(), resident);
        let draws = [vec![0], vec![split], vec![0, split]];
        let before = calls.lock().unwrap().len();
        let reads = artifact.residency_stats().map_or(0, |s| s.bytes_read);
        let first_need = artifact.composite_carriage_storage_need_v1(&draws[0]).unwrap();
        limit.store(first_need - 1, Ordering::SeqCst);
        assert!(artifact.param_carriage(&draws[0]).is_none());
        assert_eq!(
            &calls.lock().unwrap()[before..],
            &[("tir-composite-carriage", first_need)],
            "refusal precedes persistent-tree construction and piece reads"
        );
        assert_eq!(held.lock().unwrap().values().sum::<u64>(), 0);
        assert_eq!(artifact.residency_stats().map_or(0, |s| s.bytes_read), reads);
        limit.store(u64::MAX, Ordering::SeqCst);
        let tree = artifact.inventory_tree().unwrap();
        let persistent = *held.lock().unwrap().get("tir-inventory-tree").unwrap();
        for draw in &draws {
            let need = artifact.composite_carriage_storage_need_v1(draw).unwrap();
            let opened: Vec<_> = draw.iter().map(|i| (*i, artifact.param_opening(*i).unwrap().operand)).collect();
            let (parent_hashes, adapter_hashes) = tree.leaves().split_at(split as usize);
            let expected = palw_tir_composite_carriage_v1(&r, split, parent_hashes, adapter_hashes, &opened).unwrap();
            limit.store(need - 1, Ordering::SeqCst);
            assert!(artifact.param_carriage(draw).is_none());
            assert_eq!(held.lock().unwrap().values().sum::<u64>(), persistent);
            limit.store(need, Ordering::SeqCst);
            let actual = artifact.param_carriage(draw).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(borsh::to_vec(&actual).unwrap(), borsh::to_vec(&expected).unwrap());
            assert_eq!(calls.lock().unwrap().last(), Some(&("tir-composite-carriage", need)));
            assert_eq!(
                held.lock().unwrap().get("tir-composite-carriage").copied().unwrap_or(0),
                0,
                "refold scratch releases before returning proof"
            );
            assert_eq!(held.lock().unwrap().values().sum::<u64>(), persistent);
        }
        let keep = artifact.clone();
        drop(artifact);
        assert_eq!(held.lock().unwrap().values().sum::<u64>(), persistent);
        drop(keep);
        assert_eq!(held.lock().unwrap().values().sum::<u64>(), 0);
        limit.store(u64::MAX, Ordering::SeqCst);
    }
    // More than one 256-row request reuses ONE immutable mirror/tree session.
    // The returned rows have their caller's separate lasting admission.
    assert!(split > 256);
    let before = calls.lock().unwrap().len();
    let holding =
        fetch_shard_params_v1(&parent_file.class().unwrap(), parent_class, parent_root, 1, 0, &TirFileMirrorV1(ppath.clone()))
            .unwrap();
    assert_eq!(holding.leaves, u64::from(split));
    assert_eq!(calls.lock().unwrap().len(), before + 1, "multi-chunk fetch hashes/reserves the inventory only once");
    assert_eq!(calls.lock().unwrap()[before].0, "tir-inventory-tree");
    assert_eq!(held.lock().unwrap().values().sum::<u64>(), 0, "fetch-local artifact/tree releases after copied holding is assembled");
    drop(holding);
    drop(parent_file);
    std::fs::remove_file(ppath).unwrap();
    std::fs::remove_file(spath).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
