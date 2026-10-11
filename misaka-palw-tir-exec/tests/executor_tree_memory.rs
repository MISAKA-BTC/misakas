//! All executable corpus models reserve executor/tree storage before construction.
//! Requested byte counts and guard lifetime are tested; RSS and fees are not certified.
#![cfg(feature = "node")]
mod node_common;

use kaspa_consensus_core::{
    Hash64,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1},
    palw_tir_court_v1::PalwTirEvidenceStoreV1,
    palw_tir_step_v1::PalwTirStepSpaceV1,
};
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirClassRunnerV1, TirEvidenceV1, TirHeldInventoryV1, TirParamsSourceV1, TirRetainedJobV1, set_tir_memory_reserver_v1,
        tir_executor_storage_need_v1, tir_full_tree_storage_need_v1, tir_retained_storage_need_v1,
    },
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

struct Held {
    role: &'static str,
    bytes: u64,
    rows: Arc<Mutex<BTreeMap<&'static str, u64>>>,
}
impl Drop for Held {
    fn drop(&mut self) {
        *self.rows.lock().unwrap().get_mut(self.role).unwrap() -= self.bytes;
    }
}

// Actual native Vec capacities of the retained payload; class/descriptor metadata
// and allocator overhead are separately allowed by the envelope, not measured here.
fn retained_payload_bytes(job: &TirRetainedJobV1) -> u64 {
    fn vec_bytes<T>(v: &Vec<T>) -> u64 {
        (v.capacity() * std::mem::size_of::<T>()) as u64
    }
    let mut bytes = vec_bytes(&job.prompt)
        + vec_bytes(&job.generated)
        + vec_bytes(&job.leaf_hashes)
        + vec_bytes(&job.logits_rows)
        + vec_bytes(&job.points);
    bytes += job.logits_rows.iter().map(vec_bytes).sum::<u64>();
    for point in &job.points {
        bytes += vec_bytes(&point.fixed) + vec_bytes(&point.hist) + vec_bytes(&point.generated);
        bytes += point.fixed.iter().map(vec_bytes).sum::<u64>();
        for rows in &point.hist {
            bytes += vec_bytes(rows) + rows.iter().map(vec_bytes).sum::<u64>();
        }
    }
    bytes
}

#[test]
fn actual_executor_and_evidence_trees_reserve_refuse_and_release_for_all_models() {
    let rows = Arc::new(Mutex::new(BTreeMap::<&'static str, u64>::new()));
    let refuse_executor = Arc::new(AtomicBool::new(false));
    let refuse_tree = Arc::new(AtomicBool::new(false));
    let request_log = Arc::new(Mutex::new(Vec::new()));
    assert!(set_tir_memory_reserver_v1({
        let (rows, reject_exec, reject_tree, log) = (rows.clone(), refuse_executor.clone(), refuse_tree.clone(), request_log.clone());
        Arc::new(move |role, class, ctx, bytes| {
            assert!(bytes > 0 && class != Hash64::from_bytes([0; 64]) && ctx != Hash64::from_bytes([0; 64]));
            log.lock().unwrap().push((role, bytes));
            if (role == "tir-executor" && reject_exec.load(Ordering::SeqCst))
                || (role == "tir-full-tree" && reject_tree.load(Ordering::SeqCst))
            {
                return Err("probe refusal before allocation".into());
            }
            *rows.lock().unwrap().entry(role).or_default() += bytes;
            Ok(Box::new(Held { role, bytes, rows: rows.clone() }))
        })
    }));
    let held = || rows.lock().unwrap().values().sum::<u64>();
    let (mut cases, mut maximum_exec, mut maximum_tree, mut maximum_retained) = (0, 0, 0, 0);
    for (name, program, params) in
        node_common::programs().into_iter().filter(|(_, p, _)| analyze_ranges(p).is_ok() && !p.params.is_empty())
    {
        for program in [program.clone(), node_common::tiled(program)] {
            let plan = TirPlan::compile(&program).unwrap();
            let tparams = TirParams::from_map(&plan, &params).unwrap();
            let source = TirParamsSourceV1(&tparams);
            let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
            let class = PalwTirClassV1 {
                version: PALW_TIR_CLASS_VERSION_V1,
                program: program.encode(),
                layout: node_common::layout(&program, 7, 2, 2, 64),
                tokenizer_id: Hash64::from_bytes([3; 64]),
            };
            let root = inventory.tree.root();
            let class_id = class.class_id(&root);
            let space = PalwTirStepSpaceV1::new(&class).unwrap();
            let prompt: Vec<u32> = (0..4).map(|i| (i * 5 + 3) % program.token_bound).collect();
            let ctx = node_common::job(&program, 4, 3, class_id, &prompt, PalwPromptIdsFormV1::Flat);
            let runner = TirClassRunnerV1::new(&space, &plan, &tparams, class_id).unwrap();
            let executor_need = tir_executor_storage_need_v1(&space, &plan, &ctx).unwrap();
            refuse_executor.store(true, Ordering::SeqCst);
            let mut emitted = 0;
            assert!(runner.run(&ctx, &prompt, 1 << 26, false, &mut |_| emitted += 1).is_err(), "{name}");
            assert_eq!(emitted, 0, "executor refusal precedes execution");
            assert_eq!(held(), 0);
            refuse_executor.store(false, Ordering::SeqCst);
            let own = runner.retain(&class, root, &ctx, &prompt, 1 << 26).unwrap();
            assert_eq!(request_log.lock().unwrap().last(), Some(&("tir-executor", executor_need)));
            assert_eq!(held(), 0, "working reservation ends after execution/commit");
            let retained_need = tir_retained_storage_need_v1(&space, &class, &ctx, 1 << 26).unwrap();
            assert!(retained_payload_bytes(&own) <= retained_need, "{name}: native retained capacities exceed requested envelope");
            let tree_need = tir_full_tree_storage_need_v1(own.binding.step_leaf_count).unwrap();
            refuse_tree.store(true, Ordering::SeqCst);
            assert!(TirEvidenceV1::own(&runner, &own, &inventory, PalwPromptIdsFormV1::Flat, 1 << 26).is_err());
            assert_eq!(held(), 0);
            refuse_tree.store(false, Ordering::SeqCst);
            {
                let store = TirEvidenceV1::own(&runner, &own, &inventory, PalwPromptIdsFormV1::Flat, 1 << 26).unwrap();
                assert_eq!(held(), tree_need);
                assert!(store.step_leaf(own.binding.step_leaf_count - 1).is_some());
                assert_eq!(held(), tree_need, "replay's working lease releases without releasing the tree");
            }
            assert_eq!(held(), 0, "dropping the evidence store releases its tree");
            let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = runner.run(&ctx, &prompt, 1 << 26, false, &mut |_| panic!("lease unwind probe"));
            }));
            assert!(unwind.is_err());
            assert_eq!(held(), 0, "executor panic releases admitted work");
            maximum_exec = maximum_exec.max(executor_need);
            maximum_tree = maximum_tree.max(tree_need);
            maximum_retained = maximum_retained.max(retained_need);
            cases += 1;
        }
    }
    assert!(cases >= 20);
    println!(
        "{cases} executable corpus cases: maximum requested executor {maximum_exec} B, full tree {maximum_tree} B, retained {maximum_retained} B; native retained Vec capacities checked, not process RSS/fee approval"
    );
}
