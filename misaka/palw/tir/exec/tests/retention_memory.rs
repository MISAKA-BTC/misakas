//! Real backend retention with a scoped ledger probe; no public activation or RSS claim.
#![cfg(feature = "node")]
mod node_common;

use kaspa_consensus_core::{Hash64, palw_backend::PalwExecutionBackendV1, palw_prompt_ids_v1::PalwPromptIdsFormV1};
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir_exec::node::{TirArtifactV1, TirBackendV1, TirMemoryReserverV1, tir_retained_storage_need_v1};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
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
fn retained_memo_reservation_lives_until_last_arc_even_after_replacement_and_eviction() {
    let (name, program, params) = node_common::programs()
        .into_iter()
        .find(|(_, program, _)| analyze_ranges(program).is_ok() && !program.params.is_empty())
        .unwrap();
    let path = std::env::temp_dir().join(format!("tir-retention-memory-{}.palwtir", std::process::id()));
    let mut tensor = |j, layer| params.tensors.get(&(j, layer)).map(|t| t.to_le_bytes()).ok_or_else(|| "missing tensor".into());
    misaka_palw_tir_artifact::write_container_v1(
        &path,
        &program,
        borsh::to_vec(&node_common::layout(&program, 7, 2, 2, 64)).unwrap(),
        [3; 64],
        name.clone(),
        &mut tensor,
    )
    .unwrap();
    let artifact = Arc::new(TirArtifactV1::open(&path).unwrap());
    let class = artifact.class().unwrap();
    let (root, _) = artifact.inventory_root().unwrap();
    let class_id = class.class_id(&root);
    let canonical = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class_id, (4, 3)).unwrap();
    let held = Arc::new(AtomicU64::new(0));
    let calls = Arc::new(AtomicU64::new(0));
    let reserver: TirMemoryReserverV1 = {
        let held = held.clone();
        let calls = calls.clone();
        Arc::new(move |role, named_class, _, bytes| {
            assert_eq!(role, "tir-retained-job");
            assert_eq!(named_class, class_id);
            calls.fetch_add(1, Ordering::SeqCst);
            held.fetch_add(bytes, Ordering::SeqCst);
            Ok(Box::new(Held { bytes, held: held.clone() }))
        })
    };
    let backend = TirBackendV1::new(name, artifact, root, canonical, PalwPromptIdsFormV1::Flat, 1 << 26)
        .unwrap()
        .with_memory_reserver_v1(reserver);
    let (first, first_prompt) = backend.job_for_anchor(Hash64::from_bytes([4; 64])).unwrap();
    let (second, second_prompt) = backend.job_for_anchor(Hash64::from_bytes([5; 64])).unwrap();
    let first_prompt: Vec<u32> = first_prompt.into_iter().map(|id| u32::try_from(id).unwrap()).collect();
    let second_prompt: Vec<u32> = second_prompt.into_iter().map(|id| u32::try_from(id).unwrap()).collect();
    let need1 = tir_retained_storage_need_v1(backend.space(), &class, &first, 1 << 26).unwrap();
    let need2 = tir_retained_storage_need_v1(backend.space(), &class, &second, 1 << 26).unwrap();
    let a = backend.retain_memo(&first, &first_prompt).unwrap();
    let same = backend.retain_memo(&first, &first_prompt).unwrap();
    assert!(Arc::ptr_eq(&a, &same));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(held.load(Ordering::SeqCst), need1);
    let b = backend.retain_memo(&second, &second_prompt).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(held.load(Ordering::SeqCst), need1 + need2, "replacement cannot release an in-flight first job");
    drop(a);
    assert_eq!(held.load(Ordering::SeqCst), need1 + need2);
    drop(same);
    assert_eq!(held.load(Ordering::SeqCst), need2);
    backend.evict_retained_memo_v1();
    assert_eq!(held.load(Ordering::SeqCst), need2, "eviction preserves an in-flight second job");
    drop(b);
    assert_eq!(held.load(Ordering::SeqCst), 0);
    let backend = backend.with_memory_reserver_v1(Arc::new(|_, _, _, _| Err("refused before re-execution".into())));
    assert_eq!(backend.retain_memo(&first, &first_prompt).unwrap_err(), "refused before re-execution");
    assert_eq!(held.load(Ordering::SeqCst), 0, "refusal produces no data/lease");
    std::fs::remove_file(path).unwrap();
}
