//! Frozen node-cost parity, including the old allocating reference, with
//! borrowed traversal allocation measurement. No admission/credit is asserted.
use misaka_palw_tir::{
    DType, Dim, Prim, Ref, TensorType, TirProgramV1,
    admit::{CostV1, node_cost},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use std::{borrow::Cow, path::PathBuf};
struct AllocationProbe;
thread_local! {static PROBE_ACTIVE:Cell<bool>=const{Cell::new(false)};static PROBE_CALLS:Cell<usize>=const{Cell::new(0)};}
fn record_allocation() {
    let _ = PROBE_ACTIVE.try_with(|active| {
        if active.get() {
            let _ = PROBE_CALLS.try_with(|n| n.set(n.get() + 1));
        }
    });
}
// SAFETY: every operation forwards the original pointers/layout unchanged to
// System. Thread-local integer counters do not allocate and never affect them.
unsafe impl GlobalAlloc for AllocationProbe {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        // SAFETY: forward the unchanged valid allocation layout to System.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        // SAFETY: forward the unchanged valid zeroed allocation layout.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forward the original allocated pointer and matching layout.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        // SAFETY: forward the original pointer/layout and requested new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: AllocationProbe = AllocationProbe;
fn allocations<T>(run: impl FnOnce() -> T) -> (T, usize) {
    PROBE_ACTIVE.with(|a| assert!(!a.get()));
    PROBE_CALLS.with(|n| n.set(0));
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            PROBE_ACTIVE.with(|a| a.set(false));
        }
    }
    PROBE_ACTIVE.with(|a| a.set(true));
    let stop = Stop;
    let out = run();
    let count = PROBE_CALLS.with(Cell::get);
    drop(stop);
    (out, count)
}

// Frozen pre-refactor reference: copied from the cc22 source, not a new recipe.
fn ref_type(p: &TirProgramV1, block: usize, r: Ref) -> Cow<'_, TensorType> {
    let b = &p.blocks[block];
    match r {
        Ref::Node(j) => Cow::Borrowed(&b.nodes[j as usize].out),
        Ref::CarryIn(k) => Cow::Borrowed(&b.carry_in[k as usize]),
        Ref::Param(j) => Cow::Owned(TensorType::fixed(p.params[j as usize].dtype, &p.params[j as usize].shape)),
        Ref::Const(j) => Cow::Owned(TensorType::fixed(p.consts[j as usize].dtype, &p.consts[j as usize].shape)),
        Ref::State(j) => Cow::Owned(TensorType::fixed(p.states[j as usize].dtype, &p.states[j as usize].shape)),
        Ref::Input(_) => Cow::Owned(TensorType::scalar(DType::Idx)),
    }
}

fn elems(t: &TensorType, h: u64) -> u64 {
    t.elements_at(h)
}

fn bytes(t: &TensorType, h: u64) -> u64 {
    t.elements_at(h).saturating_mul(t.dtype.width() as u64)
}

fn dim_at(d: Dim, h: u64) -> u64 {
    match d {
        Dim::Fixed(n) => n as u64,
        Dim::H => h,
    }
}

/// The contraction extent of a `MatMul` node: `a.shape[−1]`.
fn contraction(p: &TirProgramV1, block: usize, node: usize, h: u64) -> u64 {
    let n = &p.blocks[block].nodes[node];
    let a = ref_type(p, block, n.inputs[0]);
    dim_at(a.shape[a.rank() - 1], h)
}

// ---------------------------------------------------------------------------------------------
// §8: the cost of a node
// ---------------------------------------------------------------------------------------------

/// A node's §8 cost at `H = h`.
fn legacy_node_cost(p: &TirProgramV1, block: usize, node: usize, h: u64) -> CostV1 {
    let n = &p.blocks[block].nodes[node];
    let out_e = elems(&n.out, h);
    let out_b = bytes(&n.out, h);
    let ins: Vec<Cow<'_, TensorType>> = n.inputs.iter().map(|r| ref_type(p, block, *r)).collect();
    let in_b: u64 = ins.iter().map(|t| bytes(t, h)).fold(0u64, u64::saturating_add);
    let mut c = CostV1 { bytes_written: out_b, ..Default::default() };
    match &n.prim {
        Prim::Iota { .. } => c.elementwise = out_e,
        Prim::Gather { .. } => {
            c.elementwise = out_e;
            c.bytes_read = out_e.saturating_mul(ins[0].dtype.width() as u64).saturating_add(bytes(&ins[1], h));
        }
        Prim::MatMul => {
            c.macs = out_e.saturating_mul(contraction(p, block, node, h));
            c.bytes_read = in_b;
        }
        Prim::ReduceSum { .. } | Prim::ReduceMax { .. } => {
            c.elementwise = elems(&ins[0], h);
            c.bytes_read = in_b;
        }
        Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => {
            c.transcendentals = out_e;
            c.bytes_read = in_b;
        }
        Prim::TopK { k, .. } => {
            c.elementwise = elems(&ins[0], h).saturating_mul(*k as u64);
            c.bytes_read = in_b;
        }
        Prim::HistAppend { .. } => {
            c.elementwise = out_e;
            c.bytes_read = out_b;
            c.bytes_written = bytes(&ins[0], h);
        }
        _ => {
            c.elementwise = out_e;
            c.bytes_read = in_b;
        }
    }
    c
}

fn programs() -> Vec<TirProgramV1> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../consensus-vectors/tir-v1/programs");
    let mut files = std::fs::read_dir(dir)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect::<Vec<_>>();
    files.sort();
    assert!(files.len() >= 7);
    files
        .iter()
        .map(|path| {
            let json: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            let text = json["program_borsh_hex"].as_str().unwrap();
            let bytes = (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect::<Vec<_>>();
            TirProgramV1::decode_canonical(&bytes).unwrap()
        })
        .collect()
}
#[test]
fn frozen_cost_vectors_match_all_corpus_nodes_refs_and_symbolic_saturation_without_allocating() {
    let (sentinel, count) = allocations(|| std::hint::black_box(vec![0u8; 128]));
    assert!(count > 0);
    drop(sentinel);
    let programs = programs();
    let mut refs = 0u8;
    let mut saturated = false;
    let mut nodes = 0usize;
    for p in &programs {
        for (block, b) in p.blocks.iter().enumerate() {
            for (index, n) in b.nodes.iter().enumerate() {
                for r in &n.inputs {
                    refs |= 1
                        << match r {
                            Ref::Node(_) => 0,
                            Ref::CarryIn(_) => 1,
                            Ref::Param(_) => 2,
                            Ref::Const(_) => 3,
                            Ref::State(_) => 4,
                            Ref::Input(_) => 5,
                        };
                }
                for h in [0, 1, 2, 17, u64::from(u32::MAX), u64::MAX] {
                    let expected = legacy_node_cost(p, block, index, h);
                    let (actual, allocs) = allocations(|| node_cost(std::hint::black_box(p), block, index, h));
                    assert_eq!(actual, expected, "block {block} node {index} H {h}");
                    assert_eq!(allocs, 0, "block {block} node {index} H {h}");
                    saturated |= [actual.macs, actual.elementwise, actual.transcendentals, actual.bytes_read, actual.bytes_written]
                        .contains(&u64::MAX);
                    nodes += 1;
                }
            }
        }
    }
    assert_eq!(refs, 0b11_1111, "corpus must exercise every Ref kind");
    assert!(saturated);
    assert!(nodes > 100);
}
