//! All executable corpus models reserve executor/tree storage before construction.
//! Requested byte counts and guard lifetime are tested; RSS and fees are not certified.
#![cfg(feature = "node")]
#![allow(warnings)]
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
        TirClassRunnerV1, TirEvidenceV1, TirHeldInventoryV1, TirParamOpenerV1, TirParamsSourceV1, TirRetainedJobV1,
        set_tir_memory_reserver_v1, tir_executor_storage_need_v1, tir_full_tree_storage_need_v1, tir_replay_position_cost_v1,
        tir_retained_storage_need_v1,
    },
};
use std::{
    cell::Cell,
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

struct ObservedOpener<'a> {
    inner: &'a dyn TirParamOpenerV1,
    calls: Cell<u64>,
    report_scratch: bool,
}
impl TirParamOpenerV1 for ObservedOpener<'_> {
    fn param_scratch_storage_v1(&self, leaves: &[u32]) -> Option<u64> {
        self.report_scratch.then(|| self.inner.param_scratch_storage_v1(leaves)).flatten()
    }
    fn param_opening(&self, leaf: u32) -> Option<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1> {
        self.calls.set(self.calls.get() + 1);
        self.inner.param_opening(leaf)
    }
    fn param_multiproof(&self, leaves: &[u32]) -> Option<kaspa_consensus_core::palw_artifact::PalwArtifactMultiproofV1> {
        self.calls.set(self.calls.get() + 1);
        self.inner.param_multiproof(leaves)
    }
}

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
    let refuse_scratch = Arc::new(AtomicBool::new(false));
    let refuse_copies = Arc::new(AtomicBool::new(false));
    let refuse_replay = Arc::new(AtomicBool::new(false));
    let request_log = Arc::new(Mutex::new(Vec::new()));
    assert!(set_tir_memory_reserver_v1({
        let (rows, reject_exec, reject_tree, reject_replay, reject_copy, reject_scratch, log) = (
            rows.clone(),
            refuse_executor.clone(),
            refuse_tree.clone(),
            refuse_replay.clone(),
            refuse_copies.clone(),
            refuse_scratch.clone(),
            request_log.clone(),
        );
        Arc::new(move |role, class, ctx, bytes| {
            assert!(bytes > 0 && class != Hash64::from_bytes([0; 64]) && ctx != Hash64::from_bytes([0; 64]));
            log.lock().unwrap().push((role, bytes));
            if (role == "tir-executor" && reject_exec.load(Ordering::SeqCst))
                || (role == "tir-full-tree" && reject_tree.load(Ordering::SeqCst))
                || (role == "tir-replay-output" && reject_replay.load(Ordering::SeqCst))
                || (role == "tir-carried-copies" && reject_copy.load(Ordering::SeqCst))
                || (role == "tir-param-scratch" && reject_scratch.load(Ordering::SeqCst))
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
                let last = space.job_shape(&ctx).unwrap().positions - 1;
                let replay_need = store.replay_requested_storage_v1(last).unwrap() as u64;
                refuse_replay.store(true, Ordering::SeqCst);
                assert!(store.step_leaf(own.binding.step_leaf_count - 1).is_none());
                assert_eq!(held(), tree_need, "refused replay creates no cache reservation/data");
                refuse_replay.store(false, Ordering::SeqCst);
                store.begin_builder_move_v1();
                assert!(store.step_leaf(own.binding.step_leaf_count - 1).is_some());
                assert_eq!(
                    held(),
                    tree_need + replay_need + store.carried_copy_reserved_bytes_v1(),
                    "the target cache and carried copy keep admission after executor work releases"
                );
                let calls = request_log.lock().unwrap().len();
                let copied_before = store.carried_copy_bytes_used_v1();
                assert!(store.step_leaf(own.binding.step_leaf_count - 1).is_some());
                assert_eq!(request_log.lock().unwrap().len(), calls, "small cache-hit copies reuse already reserved capacity");
                assert!(store.carried_copy_bytes_used_v1() > copied_before, "cache hits still charge each owned copy");
                let symbolic = store.replay_requested_cost_v1(last).unwrap();
                assert_eq!(store.replay_cost_used_v1(), symbolic);
                store.begin_builder_move_v1();
                assert_eq!(store.replay_cost_used_v1(), Default::default());
                assert!(store.step_leaf(own.binding.step_leaf_count - 1).is_some());
                assert_eq!(store.replay_cost_used_v1(), Default::default(), "cache hits do not replay or reset a running meter");
            }
            assert_eq!(held(), 0, "dropping the evidence store releases its tree");
            {
                let make_store = || TirEvidenceV1::own(&runner, &own, &inventory, PalwPromptIdsFormV1::Flat, 1 << 26).unwrap();
                let probe = make_store();
                let leaf = probe.step_leaf(0).unwrap();
                let need = probe.carried_copy_bytes_used_v1();
                assert!(need >= leaf.values_le.capacity() as u64 + std::mem::size_of_val(&leaf) as u64);
                drop(probe);
                let exact = make_store().with_carried_copy_limits_v1(need, 2 * need);
                let first_copy = exact.step_leaf(0).unwrap();
                assert_eq!(first_copy, leaf);
                assert_eq!(exact.carried_copy_bytes_used_v1(), need);
                assert!(exact.step_leaf(0).is_none(), "a cache hit cannot bypass the same-move copy cap");
                let copy_reserved = exact.carried_copy_reserved_bytes_v1();
                exact.begin_builder_move_v1();
                assert_eq!(exact.carried_copy_bytes_used_v1(), 0);
                assert_eq!(exact.carried_copy_reserved_bytes_v1(), copy_reserved, "reset retains prior issued-copy guards");
                assert_eq!(exact.step_leaf(0).unwrap(), first_copy);
                exact.begin_builder_move_v1();
                assert!(exact.step_leaf(0).is_none(), "reset cannot bypass the store-lifetime copy cap");
                assert_eq!(exact.carried_copy_lifetime_bytes_v1(), 2 * need);
                drop(exact);
                let one_less = make_store().with_carried_copy_limits_v1(need - 1, need);
                assert!(one_less.step_leaf(0).is_none());
                assert_eq!(one_less.carried_copy_bytes_used_v1(), 0);
                drop(one_less);
                refuse_copies.store(true, Ordering::SeqCst);
                let refused = make_store();
                assert!(refused.step_leaf(0).is_none(), "copy host refusal returns no owned leaf");
                assert_eq!(refused.carried_copy_bytes_used_v1(), 0);
                assert_eq!(refused.carried_copy_reserved_bytes_v1(), 0);
                drop(refused);
                refuse_copies.store(false, Ordering::SeqCst);
                let observed = ObservedOpener { inner: &inventory, calls: Cell::new(0), report_scratch: true };
                let param_store = TirEvidenceV1::own(&runner, &own, &observed, PalwPromptIdsFormV1::Flat, 1 << 26).unwrap();
                let opening = param_store.param_opening(0).unwrap();
                assert_eq!(opening, inventory.param_opening(0).unwrap());
                assert_eq!(observed.calls.get(), 1);
                assert_eq!(
                    rows.lock().unwrap().get("tir-param-scratch").copied().unwrap_or(0),
                    0,
                    "provider transient scratch releases after the opening"
                );
                refuse_scratch.store(true, Ordering::SeqCst);
                assert!(param_store.param_opening(0).is_none());
                assert_eq!(observed.calls.get(), 1, "scratch refusal precedes provider tensor materialization");
                refuse_scratch.store(false, Ordering::SeqCst);
                let param_need = param_store.carried_copy_bytes_used_v1() / 2; // successful + refused attempted copy
                drop(param_store);
                let param_exact = TirEvidenceV1::own(&runner, &own, &observed, PalwPromptIdsFormV1::Flat, 1 << 26)
                    .unwrap()
                    .with_carried_copy_limits_v1(param_need, param_need);
                assert_eq!(param_exact.param_opening(0).unwrap(), opening);
                assert!(param_exact.param_opening(0).is_none());
                drop(param_exact);
                let before = observed.calls.get();
                let param_below = TirEvidenceV1::own(&runner, &own, &observed, PalwPromptIdsFormV1::Flat, 1 << 26)
                    .unwrap()
                    .with_carried_copy_limits_v1(param_need - 1, param_need);
                assert!(param_below.param_opening(0).is_none());
                assert_eq!(observed.calls.get(), before, "one-less copy cap precedes provider access");
                drop(param_below);
                let unknown = ObservedOpener { inner: &inventory, calls: Cell::new(0), report_scratch: false };
                let unknown_store = TirEvidenceV1::own(&runner, &own, &unknown, PalwPromptIdsFormV1::Flat, 1 << 26).unwrap();
                assert!(unknown_store.param_opening(0).is_none());
                assert_eq!(unknown.calls.get(), 0, "unknown provider scratch is never assumed absent");
            }
            assert_eq!(held(), 0);
            {
                let _position_cost = tir_replay_position_cost_v1(&space).unwrap();
                let store = TirEvidenceV1::own(&runner, &own, &inventory, PalwPromptIdsFormV1::Flat, 1 << 26).unwrap();
                let shape = space.job_shape(&ctx).unwrap();
                let first = space.leaf_at(&ctx, 0).unwrap().position;
                let next_position = (first + 1..shape.positions).find(|a| space.leaves_at_position(&shape, *a) > 0).unwrap();
                let next = space.running_total(&shape, next_position) as u64;
                let first_cost = store.replay_requested_cost_v1(first).unwrap();
                let next_cost = store.replay_requested_cost_v1(next_position).unwrap();
                let limit = misaka_palw_tir::admit::CostV1 {
                    macs: first_cost.macs.max(next_cost.macs),
                    elementwise: first_cost.elementwise.max(next_cost.elementwise),
                    transcendentals: first_cost.transcendentals.max(next_cost.transcendentals),
                    bytes_read: first_cost.bytes_read.max(next_cost.bytes_read),
                    bytes_written: first_cost.bytes_written.max(next_cost.bytes_written),
                };
                let store = store.with_replay_cost_limits_v1(limit);
                assert!(store.step_leaf(0).is_some());
                assert_eq!(store.replay_cost_used_v1(), first_cost);
                assert!(store.step_leaf(next).is_none(), "each source replay must fit the shared whole-move symbolic budget");
                assert_eq!(store.replay_cost_used_v1(), first_cost);
                store.begin_builder_move_v1();
                assert!(store.step_leaf(next).is_some(), "reset starts a new bounded move");
                assert_eq!(store.replay_cost_used_v1(), next_cost);
                let mut one_less = first_cost;
                assert!(one_less.bytes_written > 0);
                one_less.bytes_written -= 1;
                let store = store.with_replay_cost_limits_v1(one_less);
                assert!(store.step_leaf(0).is_none(), "one-less symbolic written-byte budget refuses before replay");
                assert_eq!(store.replay_cost_used_v1(), Default::default());
            }
            assert_eq!(held(), 0);
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
