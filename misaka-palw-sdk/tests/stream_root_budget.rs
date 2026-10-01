//! **The streamed inventory root holds a leaf, not a tensor** (RFC-0002 Part II). A synthetic
//! `PALWTIR1` container of ~100 MB (tables of 33 MB each — the consensus function holds one whole
//! instance at a time, so it would hold ≥ 33 MB) has its root derived by
//! `palw_tir_inventory_root_streamed_v1` under a counting global allocator; the peak above what the
//! process held before must stay under 8 MiB, and the root equals the consensus function's (computed
//! outside the window). One test only: the allocator counts the whole process.

use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_root_v1;
use misaka_palw_sdk::tir_manifest::PalwTirContainerSourceV1;
use misaka_palw_sdk::tir_stream::{ContainerRanges, palw_tir_inventory_root_streamed_v1};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{DType, Ref, TensorType, TirProgramV1};
use misaka_palw_tir_artifact::{PalwTirContainerV1, tensor_bytes_v1, write_container_v1_streamed};
use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Counting;
static CUR: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn add(n: usize) {
    let cur = CUR.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(cur, Relaxed);
}

// SAFETY: every call forwards to the system allocator and only counts sizes on the side.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            add(l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() {
            add(l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) };
        CUR.fetch_sub(l.size(), Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, new) };
        if !q.is_null() {
            if new >= l.size() {
                add(new - l.size());
            } else {
                CUR.fetch_sub(l.size() - new, Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

const V: u32 = 32768;
const H: u32 = 512;

fn program() -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(V, HISTORY_BOUND_V1_SMALL);
    let table = pb.param("embed.table", DType::I16, &[V, H], false);
    let w = pb.param("blk.w", DType::I8, &[H, H], true);
    let head = pb.param("head.w", DType::I16, &[V, H], false);
    let carry = vec![TensorType::fixed(DType::I32, &[H])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let row = b.gather(table, Ref::Input(0), 0, 0);
        let row = b.cast(row, DType::I32);
        b.finish(&[row])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let x = b.reshape_fixed(Ref::CarryIn(0), &[H, 1]);
        let acc = b.matmul(w, x, DType::I64);
        let acc = b.reshape_fixed(acc, &[H]);
        let y = b.clamp(acc, -30_000, 30_000, DType::I32);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb.block("post", carry.clone());
        let x = b.reshape_fixed(Ref::CarryIn(0), &[H, 1]);
        let l = b.matmul(head, x, DType::I64);
        let l = b.reshape_fixed(l, &[V]);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    pb.finish(pre, vec![layer; 40], post, logits)
}

#[test]
fn the_streamed_root_of_a_large_container_stays_inside_its_budget() {
    let p = program();
    let path = std::env::temp_dir().join(format!("tir-root-budget-{}.palwtir", std::process::id()));
    // Tensors from a generator, streamed in pieces: the writer never holds one either.
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    write_container_v1_streamed(&path, &p, Vec::new(), [1u8; 64], "{}".into(), &mut |j, _l, out| {
        let mut left = tensor_bytes_v1(&p, j) as usize;
        let mut piece = vec![0u8; 1 << 20];
        while left > 0 {
            let n = left.min(piece.len());
            for b in &mut piece[..n] {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x >> 24) as u8;
            }
            out.write_all(&piece[..n]).map_err(|e| e.to_string())?;
            left -= n;
        }
        Ok(())
    })
    .expect("container");
    let c = PalwTirContainerV1::open(&path).expect("opens");
    let file_mib = c.file_len as f64 / (1 << 20) as f64;
    let ranges = ContainerRanges::open(&c).expect("ranges");
    let base = CUR.load(Relaxed);
    PEAK.store(base, Relaxed);
    let (root, count) = palw_tir_inventory_root_streamed_v1(&c.program, &ranges).expect("streamed root");
    let peak = PEAK.load(Relaxed).saturating_sub(base);
    eprintln!("container {file_mib:.0} MiB, {count} leaves; streamed root peak {:.2} MiB", peak as f64 / (1 << 20) as f64);
    assert!(peak < 8 << 20, "the streamed root held {peak} bytes");
    // Outside the window: the consensus function (one whole instance resident) agrees.
    let want = palw_tir_inventory_root_v1(&c.program, &PalwTirContainerSourceV1(&c)).expect("consensus root");
    assert_eq!((root, count), want);
    let _ = std::fs::remove_file(&path);
}
