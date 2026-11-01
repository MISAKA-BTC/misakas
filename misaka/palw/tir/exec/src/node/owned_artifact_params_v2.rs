//! Fully owned resident parameters from an already admitted PALWTIR1 source.
//! No loader startup, program decoding/validation/compile, unsafe/mmap/lazy data,
//! model qualification, native activation or computation-credit decision.
//! Uses the shared public Vec policy from Core9c010109d.
use super::memory::{self, TirMemoryLeaseV1, TirMemoryReserverV1};
use crate::{Buf, ParamData, Slice, TirParams, TirPlan, params::quote_owned_params_table_v2};
use kaspa_consensus_core::{
    Hash64,
    palw_artifact::{PalwArtifactLeafHasherV1, PalwArtifactMerkleFrontierV1, artifact_leaf_parts_v1},
    palw_native_resource_v1::{btree_node_envelope_bytes_v1, growing_vec_request_capacity_v1},
    palw_tir_artifact_v1::{PALW_TIR_ROW_PIECE_BYTES_V1, PalwTirInventoryRowV1, palw_tir_visit_inventory_rows_v1},
    palw_tir_step_v1::{PalwTirStepBindingV1, PalwTirVerifiedBindingV2},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::{
    DType, Ref,
    program::{MAX_NODE_INPUTS, MAX_NODES_PER_BLOCK},
};
use misaka_palw_tir_artifact::bounded_v1::PalwTirCheckedHeaderV1;
use std::{
    collections::BTreeSet,
    io::{Read, Seek, SeekFrom},
    mem::size_of,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnedParamErrorV2 {
    UnsupportedPolicy,
    Overflow,
    Limit,
    Pins,
    Geometry,
    Read,
    Allocation,
    Inventory,
    Root,
    Guard,
}
use OwnedParamErrorV2 as E;
fn add(a: u64, b: u64) -> Result<u64, E> {
    a.checked_add(b).ok_or(E::Overflow)
}
fn mul(a: u64, b: u64) -> Result<u64, E> {
    a.checked_mul(b).ok_or(E::Overflow)
}
fn n(a: usize) -> Result<u64, E> {
    u64::try_from(a).map_err(|_| E::Overflow)
}
fn limit(a: u64, b: u64) -> Result<(), E> {
    if a > b { Err(E::Limit) } else { Ok(()) }
}
fn array<T>(count: u64) -> Result<u64, E> {
    mul(count, n(size_of::<T>())?)
}
fn growing<T>(count: u64) -> Result<u64, E> {
    array::<T>(growing_vec_request_capacity_v1::<T>(count).ok_or(E::Overflow)?)
}
fn exact<T>(count: usize) -> Result<Vec<T>, E> {
    let mut out = Vec::new();
    out.try_reserve_exact(count).map_err(|_| E::Allocation)?;
    if out.capacity() != count {
        return Err(E::Allocation);
    }
    Ok(out)
}

/// These independently admitted owners/associations are caller preconditions.
/// In particular `verified` must come from THIS original expected binding and
/// context; no arbitrary mutable compiled-plan fields are authenticated here.
/// `checked` was obtained from this SAME opened descriptor, remains unchanged,
/// and borrows the independently bounded canonical normal-form program owner.
#[derive(Clone, Copy)]
pub struct OwnedParamInputsV2<'a> {
    pub checked: &'a PalwTirCheckedHeaderV1<'a>,
    pub plan: &'a TirPlan,
    pub expected: &'a PalwTirStepBindingV1,
    pub verified: &'a PalwTirVerifiedBindingV2,
    pub original_context: &'a PalwJobContextV2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedParamLimitsV2 {
    pub max_tensor_bytes: u64,
    pub max_payload_bytes: u64,
    pub max_chunk_bytes: u64,
    pub max_metadata_bytes: u64,
    pub max_native_bytes: u64,
    pub max_inventory_leaves: u64,
    pub max_work_per_attempt: u64,
    pub max_attempts: u64,
    pub max_total_read_bytes: u64,
    pub max_total_inventory_leaves: u64,
    pub max_total_work: u64,
    pub max_operation_bytes: u64,
    pub external_retained_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedParamQuoteV2 {
    pub payload_bytes: u64,
    pub largest_tensor_bytes: u64,
    pub params_metadata_bytes: u64,
    pub visitor_metadata_bytes: u64,
    pub frontier_bytes: u64,
    pub chunk_bytes: u64,
    pub fixed_scratch_bytes: u64,
    pub leaves: u64,
    pub metadata_work: u64,
    pub data_work: u64,
    pub retained_bytes: u64,
    pub new_owner_peak_bytes: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OwnedParamUsageV2 {
    pub attempts: u64,
    pub read_bytes: u64,
    pub leaves: u64,
    pub work: u64,
    pub retained_claims: u64,
}
/// No reset/refund. Even refused guards and failed reads keep their admitted
/// charges. Retained claims conservatively coexist until this budget is dropped.
pub struct OwnedParamBudgetV2 {
    limits: OwnedParamLimitsV2,
    usage: OwnedParamUsageV2,
}
impl OwnedParamBudgetV2 {
    pub fn new(limits: OwnedParamLimitsV2) -> Self {
        Self { limits, usage: OwnedParamUsageV2::default() }
    }
    pub fn usage(&self) -> OwnedParamUsageV2 {
        self.usage
    }
    fn start(&mut self, inputs: OwnedParamInputsV2<'_>) -> Result<u64, E> {
        if !cfg!(target_pointer_width = "64") {
            return Err(E::UnsupportedPolicy);
        }
        let p = inputs.checked.program();
        // O(1) format cardinalities precede any nested scans. Symbolic metadata
        // work counts conservative reference/tree/param×layer visits, not cycles.
        let refs = mul(mul(n(p.blocks.len())?, MAX_NODES_PER_BLOCK as u64)?, MAX_NODE_INPUTS as u64)?;
        let joins = mul(mul(n(p.params.len())?, add(n(p.schedule.layers.len())?, 1)?)?, add(refs, 1)?)?;
        let work = add(mul(mul(refs, add(refs, 1)?)?, 2)?, mul(joins, 3)?)?;
        let work = add(work, add(n(inputs.checked.header().program.len())?, n(inputs.checked.header().tensors.len())?)?)?;
        let attempts = add(self.usage.attempts, 1)?;
        let total = add(self.usage.work, work)?;
        limit(attempts, self.limits.max_attempts)?;
        limit(work, self.limits.max_work_per_attempt)?;
        limit(total, self.limits.max_total_work)?;
        self.usage.attempts = attempts;
        self.usage.work = total;
        Ok(work)
    }
    fn admit(&mut self, q: OwnedParamQuoteV2) -> Result<(), E> {
        let next = OwnedParamUsageV2 {
            attempts: self.usage.attempts,
            read_bytes: add(self.usage.read_bytes, q.payload_bytes)?,
            leaves: add(self.usage.leaves, q.leaves)?,
            work: add(self.usage.work, q.data_work)?,
            retained_claims: add(self.usage.retained_claims, q.retained_bytes)?,
        };
        limit(add(q.metadata_work, q.data_work)?, self.limits.max_work_per_attempt)?;
        limit(next.read_bytes, self.limits.max_total_read_bytes)?;
        limit(next.leaves, self.limits.max_total_inventory_leaves)?;
        limit(next.work, self.limits.max_total_work)?;
        limit(
            add(add(q.new_owner_peak_bytes, self.usage.retained_claims)?, self.limits.external_retained_bytes)?,
            self.limits.max_operation_bytes,
        )?;
        self.usage = next;
        Ok(())
    }
}

fn quote(inputs: OwnedParamInputsV2<'_>, l: OwnedParamLimitsV2, metadata_work: u64) -> Result<OwnedParamQuoteV2, E> {
    let p = inputs.checked.program();
    let table = quote_owned_params_table_v2(p).map_err(|_| E::Overflow)?;
    let mut refs = 0;
    for block in &p.blocks {
        for node in &block.nodes {
            for r in &node.inputs {
                if matches!(r, Ref::Param(_)) {
                    refs = add(refs, 1)?;
                }
            }
        }
    }
    // One Core instance projection: readers exact P sets, tree membership upper
    // bound by actual Param references, outer Vec and per-declaration instances.
    let mut visitor = array::<BTreeSet<u8>>(n(p.params.len())?)?;
    visitor = add(visitor, mul(refs, n(btree_node_envelope_bytes_v1::<u8, ()>())?)?)?;
    visitor = add(visitor, growing::<Vec<Option<u16>>>(n(p.params.len())?)?)?;
    for d in &p.params {
        visitor =
            add(visitor, if d.per_layer { growing::<Option<u16>>(n(p.schedule.layers.len())?)? } else { array::<Option<u16>>(1)? })?;
    }
    // Core visitor calls inventory count then constructs its iteration projection;
    // these do not coexist, but each needs the above envelope and work admission.
    let mut total = 0;
    let mut largest = 0;
    let mut leaves = 0;
    let mut chunk = 0;
    let mut elements = 0;
    let mut hash_headers = 0;
    for e in &inputs.checked.header().tensors {
        let d = p.params.get(e.param as usize).ok_or(E::Geometry)?;
        if d.dtype == DType::I128 {
            return Err(E::Geometry);
        }
        let width = n(d.dtype.width())?;
        let count = d.shape.iter().try_fold(1, |a, b| mul(a, u64::from(*b)))?;
        let bytes = mul(count, width)?;
        if bytes != e.bytes || bytes == 0 || bytes > u32::MAX as u64 {
            return Err(E::Geometry);
        }
        limit(bytes, l.max_tensor_bytes)?;
        total = add(total, bytes)?;
        largest = largest.max(bytes);
        elements = add(elements, count)?;
        let row = if d.shape.len() >= 2 { mul(d.shape[1..].iter().try_fold(1, |a, b| mul(a, u64::from(*b)))?, width)? } else { bytes };
        if row == 0 || bytes % row != 0 {
            return Err(E::Geometry);
        }
        let count = mul(bytes / row, row.div_ceil(PALW_TIR_ROW_PIECE_BYTES_V1))?;
        leaves = add(leaves, count)?;
        chunk = chunk.max(row.min(PALW_TIR_ROW_PIECE_BYTES_V1)).max(width);
        hash_headers = add(hash_headers, mul(count, add(n(d.name.len())?, 16)?)?)?;
    }
    if leaves == 0 || leaves > u32::MAX as u64 {
        return Err(E::Geometry);
    }
    limit(total, l.max_payload_bytes)?;
    limit(leaves, l.max_inventory_leaves)?;
    limit(chunk, l.max_chunk_bytes)?;
    let depth = u64::from(u64::BITS - leaves.leading_zeros());
    let frontier = add(n(size_of::<PalwArtifactMerkleFrontierV1>())?, growing::<Option<Hash64>>(depth)?)?;
    let fixed = add(
        n(size_of::<PalwArtifactLeafHasherV1>())?,
        add(
            add(n(size_of::<blake2b_simd::State>())?, n(size_of::<blake2b_simd::Hash>())?)?,
            add(array::<Hash64>(4)?, n(size_of::<PalwTirInventoryRowV1>())?)?,
        )?,
    )?;
    let retained = add(n(size_of::<OwnedArtifactParamsV2<'_>>())?, add(table.heap_bytes, total)?)?;
    let params_metadata = add(table.root_bytes, table.heap_bytes)?;
    let metadata = add(params_metadata, visitor)?;
    limit(metadata, l.max_metadata_bytes)?;
    let peak = add(retained, add(chunk, add(visitor, add(frontier, fixed)?)?)?)?;
    limit(peak, l.max_native_bytes)?;
    // Explicit symbolic currency: source read bytes, decoded native elements,
    // re-encoded hash bytes/headers and at most2*leaves internal 128-byte hashes.
    let data_work = add(add(mul(total, 2)?, elements)?, add(hash_headers, mul(mul(leaves, 2)?, 128)?)?)?;
    Ok(OwnedParamQuoteV2 {
        payload_bytes: total,
        largest_tensor_bytes: largest,
        params_metadata_bytes: params_metadata,
        visitor_metadata_bytes: visitor,
        frontier_bytes: frontier,
        chunk_bytes: chunk,
        fixed_scratch_bytes: fixed,
        leaves,
        metadata_work,
        data_work,
        retained_bytes: retained,
        new_owner_peak_bytes: peak,
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnedParamLayoutOriginV2 {
    DeclaredInFile,
    IndependentlyExpectedOnly,
}
// This is a literal consistency check of caller-admitted associations, not a
// fresh class-id/context-hash admission. In particular, do not add unquoted
// CanonicalWriter hashing here. A future Loaded entry must establish the whole
// expected/verified/original-context association through one retained owner.
fn check_pins(i: OwnedParamInputsV2<'_>) -> Result<OwnedParamLayoutOriginV2, E> {
    let h = i.checked.header();
    let e = i.expected;
    let s = i.verified.space();
    let c = i.original_context;
    if &e.job_context != c
        || c.shape_profile_id != i.verified.class_id()
        || h.program != e.class.program
        || i.checked.program() != s.program()
        || i.checked.program() != &i.plan.program
        || s.layout() != &e.class.layout
        || h.tokenizer_id != e.class.tokenizer_id.as_bytes()
        || i.verified.job() != s.job_shape(c).map_err(|_| E::Pins)?
    {
        return Err(E::Pins);
    }
    let Some(view) = i.checked.layout().map_err(|_| E::Pins)? else {
        // Empty means undeclared. The admitted original expected layout is the
        // only source; no file/default/max-context rewrite or derivation occurs.
        return Ok(OwnedParamLayoutOriginV2::IndependentlyExpectedOnly);
    };
    let x = &e.class.layout;
    if view.version != x.version
        || view.max_context != x.max_context
        || view.checkpoint_interval != x.checkpoint_interval
        || view.h_tile != x.h_tile
        || !view.commit_tiles().eq(x.commit_tiles.iter().copied())
        || !view.state_tiles().eq(x.state_tiles.iter().copied())
    {
        return Err(E::Pins);
    }
    Ok(OwnedParamLayoutOriginV2::DeclaredInFile)
}

/// Owns all native parameter Vecs, while borrowing the independently admitted
/// source/program/plan/pins owners. This owner has no Clone or move-out getter.
/// Borrowed TirParams still supports deep Clone and further slice exports;
/// arbitrary copies require the caller's separate reservation/external inventory.
/// This owner's guard does not follow those copies. Native params drop before
/// the optional new-owner host lease.
pub struct OwnedArtifactParamsV2<'a> {
    params: TirParams<'static>,
    inputs: OwnedParamInputsV2<'a>,
    root: Hash64,
    layout_origin: OwnedParamLayoutOriginV2,
    quote: OwnedParamQuoteV2,
    _memory: Option<TirMemoryLeaseV1>,
}
impl OwnedArtifactParamsV2<'_> {
    /// Borrow tied to this owner. A deep cloned/exported copy is caller-owned
    /// and separately admitted; this guard is not transferred with a copy.
    pub fn params(&self) -> &TirParams<'_> {
        &self.params
    }
    pub fn layout_origin(&self) -> OwnedParamLayoutOriginV2 {
        self.layout_origin
    }
    pub fn artifact_root(&self) -> Hash64 {
        self.root
    }
    pub fn quote(&self) -> OwnedParamQuoteV2 {
        self.quote
    }
    pub fn original_context(&self) -> &PalwJobContextV2 {
        self.inputs.original_context
    }
}
fn read_native<R: Read>(reader: &mut R, dtype: DType, elements: usize, chunk: &mut [u8]) -> Result<Buf, E> {
    macro_rules! read {
        ($t:ty,$v:ident) => {{
            let width = size_of::<$t>();
            if chunk.len() < width {
                return Err(E::Limit);
            }
            let mut out = exact::<$t>(elements)?;
            while out.len() < elements {
                let count = (chunk.len() / width).min(elements - out.len());
                let bytes = count.checked_mul(width).ok_or(E::Overflow)?;
                reader.read_exact(&mut chunk[..bytes]).map_err(|_| E::Read)?;
                for raw in chunk[..bytes].chunks_exact(width) {
                    out.push(<$t>::from_le_bytes(raw.try_into().map_err(|_| E::Geometry)?));
                }
            }
            Buf::$v(out)
        }};
    }
    Ok(match dtype {
        DType::I8 => read!(i8, I8),
        DType::I16 => read!(i16, I16),
        DType::I32 => read!(i32, I32),
        DType::I64 => read!(i64, I64),
        DType::Idx => read!(u32, Idx),
        DType::I128 => return Err(E::Geometry),
    })
}
fn encode_piece(data: Slice<'_>, start: usize, len: usize, out: &mut [u8]) -> Result<(), E> {
    let width = data.dtype().width();
    if start % width != 0 || len % width != 0 || len > out.len() {
        return Err(E::Geometry);
    }
    let first = start / width;
    let count = len / width;
    let end = first.checked_add(count).ok_or(E::Overflow)?;
    macro_rules! encode {
        ($v:expr) => {{
            let v = $v.get(first..end).ok_or(E::Geometry)?;
            for (value, bytes) in v.iter().zip(out[..len].chunks_exact_mut(width)) {
                bytes.copy_from_slice(&value.to_le_bytes());
            }
        }};
    }
    match data {
        Slice::I8(v) => encode!(v),
        Slice::I16(v) => encode!(v),
        Slice::I32(v) => encode!(v),
        Slice::I64(v) => encode!(v),
        Slice::Idx(v) => encode!(v),
        Slice::I128(_) => return Err(E::Geometry),
    }
    Ok(())
}
/// Caller supplies the SAME already-open header-checked descriptor. It owns
/// source stamp/mutation checks and bounded provider internals. No path reopen.
/// The expected/verified/context association was independently admitted already.
/// Optional reserver is the existing Node host-reserver contract; its bookkeeping
/// is external, and its new-owner lease is retained through the returned owner.
/// Failed accepted attempts are not refunded. No model eligibility/credit exists.
pub fn load_owned_artifact_params_v2<'a, R: Read + Seek>(
    reader: &mut R,
    inputs: OwnedParamInputsV2<'a>,
    budget: &mut OwnedParamBudgetV2,
    reserver: Option<&TirMemoryReserverV1>,
) -> Result<OwnedArtifactParamsV2<'a>, E> {
    let metadata_work = budget.start(inputs)?;
    let q = quote(inputs, budget.limits, metadata_work)?;
    budget.admit(q)?;
    let layout_origin = check_pins(inputs)?;
    let lease = match reserver {
        Some(r) => Some(
            r("tir-owned-artifact-params-v2", inputs.verified.class_id(), inputs.verified.context_hash(), q.new_owner_peak_bytes)
                .map_err(|_| E::Guard)?,
        ),
        None => None,
    };
    let mut params = TirParams::new(inputs.plan);
    let mut chunk = exact::<u8>(usize::try_from(q.chunk_bytes).map_err(|_| E::Overflow)?)?;
    chunk.resize(q.chunk_bytes as usize, 0);
    for entry in &inputs.checked.header().tensors {
        let d = &inputs.checked.program().params[entry.param as usize];
        let count = usize::try_from(entry.bytes / (d.dtype.width() as u64)).map_err(|_| E::Overflow)?;
        reader.seek(SeekFrom::Start(entry.offset)).map_err(|_| E::Read)?;
        let buf = read_native(reader, d.dtype, count, &mut chunk)?;
        params
            .insert(inputs.plan, entry.param, entry.layer, ParamData::from_buf(buf).map_err(|_| E::Geometry)?)
            .map_err(|_| E::Geometry)?;
    }
    params.check_complete(inputs.plan).map_err(|_| E::Geometry)?;
    let mut frontier = PalwArtifactMerkleFrontierV1::new();
    let mut failed = None;
    palw_tir_visit_inventory_rows_v1(inputs.checked.program(), &mut |row| {
        if failed.is_some() {
            return;
        }
        let attempt = (|| -> Result<(), E> {
            let data = params.get(row.param, row.layer).ok_or(E::Geometry)?;
            encode_piece(data, row.row_start as usize, row.len as usize, &mut chunk)?;
            frontier.push(artifact_leaf_parts_v1(
                &inputs.checked.program().params[row.param as usize].name,
                row.layer,
                row.row_start,
                &chunk[..row.len as usize],
            ));
            Ok(())
        })();
        if let Err(error) = attempt {
            failed = Some(error)
        }
    })
    .map_err(|_| E::Inventory)?;
    if let Some(error) = failed {
        return Err(error);
    }
    if frontier.leaf_count() != q.leaves {
        return Err(E::Inventory);
    }
    let root = frontier.root().ok_or(E::Inventory)?;
    if root != inputs.expected.artifact_root {
        return Err(E::Root);
    }
    Ok(OwnedArtifactParamsV2 { params, inputs, root, layout_origin, quote: q, _memory: lease })
}
/// Convenience for the existing optional global host-reserver, with no change
/// to its flags or admission semantics. No guard is fabricated when absent.
pub fn load_owned_artifact_params_with_node_guard_v2<'a, R: Read + Seek>(
    reader: &mut R,
    inputs: OwnedParamInputsV2<'a>,
    budget: &mut OwnedParamBudgetV2,
) -> Result<OwnedArtifactParamsV2<'a>, E> {
    let reserver = memory::memory_reserver_v1();
    load_owned_artifact_params_v2(reader, inputs, budget, reserver.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
        palw_tir_step_v1::{PalwTirStepSpaceV2, palw_tir_execution_root_v1, verify_tir_binding_v2},
    };
    use misaka_palw_tir::{TirProgramV1, builder::ProgramBuilder, program::LOGITS_I64_Q24_SCHEME_V2};
    use misaka_palw_tir_artifact::{
        bounded_v1::{PalwTirHeaderLimitsV1, read_header_bounded_v1},
        write_container_v1,
    };
    use std::{
        fs::OpenOptions,
        io::Write,
        sync::{
            Arc,
            atomic::{AtomicU64, AtomicUsize, Ordering},
        },
    };
    fn limits() -> OwnedParamLimitsV2 {
        OwnedParamLimitsV2 {
            max_tensor_bytes: 1 << 20,
            max_payload_bytes: 1 << 20,
            max_chunk_bytes: 32768,
            max_metadata_bytes: 1 << 20,
            max_native_bytes: 1 << 20,
            max_inventory_leaves: 1024,
            max_work_per_attempt: 1 << 32,
            max_attempts: 1,
            max_total_read_bytes: 1 << 20,
            max_total_inventory_leaves: 1024,
            max_total_work: 1 << 34,
            max_operation_bytes: 1 << 24,
            external_retained_bytes: 1 << 22,
        }
    }
    fn fixture(empty_layout: bool, mode: &str, test: impl FnOnce(&mut std::fs::File, OwnedParamInputsV2<'_>)) {
        let mut b = ProgramBuilder::new(2, 1 << 18);
        let a = b.param("signed", DType::I8, &[2], false);
        let x = b.param("index", DType::Idx, &[2], false);
        let y = b.param("logits", DType::I64, &[2], false);
        let mut pre = b.block("pre", vec![]);
        let a = pre.reshape_fixed(a, &[2]);
        pre.commit(a);
        let x = pre.reshape_fixed(x, &[2]);
        pre.commit(x);
        let pre = pre.finish(&[]);
        let mut post = b.block("post", vec![]);
        let y = post.reshape_fixed(y, &[2]);
        post.commit(y);
        let Ref::Node(y) = y else { unreachable!() };
        let post = post.finish(&[]);
        let mut program = b.finish(pre, vec![], post, y);
        program.logits_scheme_id = LOGITS_I64_Q24_SCHEME_V2;
        // Trusted test-only decode/compile, outside the loader's subset/admission.
        let program = TirProgramV1::decode_canonical(&program.encode()).unwrap();
        let plan = TirPlan::compile(&program).unwrap();
        let data = [
            vec![128, 127],
            vec![0, 0, 0, 0, 255, 255, 255, 255],
            vec![0, 0, 0, 0, 0, 0, 0, 128, 255, 255, 255, 255, 255, 255, 255, 127],
        ];
        let mut root = PalwArtifactMerkleFrontierV1::new();
        for (j, bytes) in data.iter().enumerate() {
            root.push(artifact_leaf_parts_v1(&program.params[j].name, None, 0, bytes));
        }
        let artifact = root.root().unwrap();
        let class = PalwTirClassV1 {
            version: 1,
            program: program.encode(),
            layout: PalwTirLayoutV1 {
                version: 1,
                max_context: 16,
                checkpoint_interval: 1,
                h_tile: 1,
                commit_tiles: vec![4; 3],
                state_tiles: vec![],
            },
            tokenizer_id: Hash64::from_u64_word(18),
        };
        let zero = Hash64::from_u64_word(0);
        let ctx = PalwJobContextV2 {
            version: 2,
            network_id: b"testnet-12".to_vec(),
            job_id: Hash64::from_u64_word(42),
            job_nullifier: zero,
            assignment_id: zero,
            execution_seed: [3; 32],
            model_profile_id: zero,
            runtime_manifest_hash: zero,
            runtime_class_id: zero,
            shape_profile_id: class.class_id(&artifact),
            trace_scheme_id: Hash64::from_bytes(LOGITS_I64_Q24_SCHEME_V2),
            cu_ruleset_id: zero,
            tokenizer_id: zero,
            prompt_token_ids_hash: zero,
            declared_prefill_tokens: 1,
            exact_decode_tokens: 1,
            max_context_tokens: 16,
        };
        let space = PalwTirStepSpaceV2::new(&class).unwrap();
        let count = space.leaf_count_capped(&ctx, 1024).unwrap();
        let trace = Hash64::from_u64_word(61);
        let step = Hash64::from_u64_word(62);
        let binding = PalwTirStepBindingV1 {
            version: 1,
            job_context: ctx.clone(),
            class: class.clone(),
            artifact_root: artifact,
            full_logits_trace_root: trace,
            step_leaf_count: count,
            step_merkle_root: step,
            committed_execution_root: palw_tir_execution_root_v1(&ctx.context_hash(), &trace, &ctx.shape_profile_id, count, &step),
        };
        // Synthetic coherent execution-root fields pin data association only;
        // no model execution, registration receipt or computation pass is claimed.
        let verified = verify_tir_binding_v2(&binding, 1024).unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("owned.palwtir");
        write_container_v1(
            &path,
            &program,
            if empty_layout { vec![] } else { borsh::to_vec(&class.layout).unwrap() },
            class.tokenizer_id.as_bytes(),
            "literal-test-provenance".into(),
            &mut |j, _| Ok(data[j as usize].clone()),
        )
        .unwrap();
        let mut file = OpenOptions::new().read(true).write(true).open(&path).unwrap();
        let len = file.metadata().unwrap().len();
        let h = read_header_bounded_v1(
            &mut file,
            len,
            PalwTirHeaderLimitsV1 {
                max_file_bytes: 1 << 20,
                max_header_bytes: 1 << 20,
                max_program_bytes: 1 << 18,
                max_layout_bytes: 1 << 16,
                max_tensors: 32,
                max_meta_bytes: 1024,
                max_native_bytes: 1 << 20,
                max_copy_bytes: 1 << 20,
                max_work: 1 << 32,
                max_operation_bytes: 1 << 24,
                external_retained_bytes: 1 << 22,
            },
        )
        .unwrap();
        let checked = h
            .validate_with_program(
                &program,
                PalwTirHeaderLimitsV1 {
                    max_file_bytes: 1 << 20,
                    max_header_bytes: 1 << 20,
                    max_program_bytes: 1 << 18,
                    max_layout_bytes: 1 << 16,
                    max_tensors: 32,
                    max_meta_bytes: 1024,
                    max_native_bytes: 1 << 20,
                    max_copy_bytes: 1 << 20,
                    max_work: 1 << 32,
                    max_operation_bytes: 1 << 24,
                    external_retained_bytes: 1 << 22,
                },
            )
            .unwrap();
        if mode == "bad-root" {
            file.seek(SeekFrom::Start(checked.header().tensors[0].offset)).unwrap();
            file.write_all(&[0]).unwrap();
        }
        if mode == "truncate" {
            let last = checked.header().tensors.last().unwrap();
            file.set_len(last.offset + last.bytes - 1).unwrap();
        }
        test(
            &mut file,
            OwnedParamInputsV2 { checked: &checked, plan: &plan, expected: &binding, verified: &verified, original_context: &ctx },
        );
    }
    struct Guard(Arc<AtomicU64>, u64);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.fetch_sub(self.1, Ordering::SeqCst);
        }
    }
    fn reserver(held: Arc<AtomicU64>, calls: Arc<AtomicUsize>) -> TirMemoryReserverV1 {
        Arc::new(move |role, _, _, bytes| {
            assert_eq!(role, "tir-owned-artifact-params-v2");
            calls.fetch_add(1, Ordering::SeqCst);
            held.fetch_add(bytes, Ordering::SeqCst);
            Ok(Box::new(Guard(held.clone(), bytes)))
        })
    }
    #[test]
    fn native_owned_endpoints_idx_root_context_and_guard_stay_together() {
        for empty in [false, true] {
            fixture(empty, "good", |file, inputs| {
                let held = Arc::new(AtomicU64::new(0));
                let calls = Arc::new(AtomicUsize::new(0));
                let reserve = reserver(held.clone(), calls.clone());
                let mut budget = OwnedParamBudgetV2::new(limits());
                let owner = load_owned_artifact_params_v2(file, inputs, &mut budget, Some(&reserve)).unwrap();
                assert_eq!(owner.params().get(0, None).unwrap().to_i128s(), vec![-128, 127]);
                assert_eq!(owner.params().get(1, None).unwrap().to_i128s(), vec![0, u32::MAX as i128]);
                assert_eq!(owner.params().get(2, None).unwrap().to_i128s(), vec![i64::MIN as i128, i64::MAX as i128]);
                assert!(owner.params().row_source().is_none());
                assert_eq!(owner.artifact_root(), inputs.expected.artifact_root);
                assert_eq!(owner.quote().leaves, 3);
                assert_eq!(owner.original_context(), inputs.original_context);
                assert_ne!(inputs.expected.class.tokenizer_id, owner.original_context().tokenizer_id);
                assert_eq!(
                    owner.layout_origin(),
                    if empty { OwnedParamLayoutOriginV2::IndependentlyExpectedOnly } else { OwnedParamLayoutOriginV2::DeclaredInFile }
                );
                assert_eq!(inputs.checked.header().layout.is_empty(), empty);
                assert_eq!(held.load(Ordering::SeqCst), owner.quote().new_owner_peak_bytes);
                let before = budget.usage();
                assert_eq!(load_owned_artifact_params_v2(file, inputs, &mut budget, Some(&reserve)).err(), Some(E::Limit));
                assert_eq!(budget.usage(), before);
                assert_eq!(calls.load(Ordering::SeqCst), 1);
                drop(owner);
                assert_eq!(held.load(Ordering::SeqCst), 0);
            });
        }
    }
    #[test]
    fn root_mismatch_and_truncation_release_guard_without_returning_partial_owner() {
        for (mode, want) in [("bad-root", E::Root), ("truncate", E::Read)] {
            fixture(false, mode, |file, inputs| {
                let held = Arc::new(AtomicU64::new(0));
                let calls = Arc::new(AtomicUsize::new(0));
                let reserve = reserver(held.clone(), calls.clone());
                let mut budget = OwnedParamBudgetV2::new(limits());
                assert_eq!(load_owned_artifact_params_v2(file, inputs, &mut budget, Some(&reserve)).err(), Some(want));
                assert_eq!(held.load(Ordering::SeqCst), 0);
                assert_eq!(calls.load(Ordering::SeqCst), 1);
                assert_eq!(budget.usage().attempts, 1);
                assert_eq!(budget.usage().read_bytes, 26); // Full accepted read admission is not refunded.
            });
        }
    }
    #[test]
    fn exact_and_one_below_native_chunk_payload_work_and_io_caps_are_preallocation() {
        fixture(false, "good", |file, inputs| {
            let mut probe = OwnedParamBudgetV2::new(limits());
            let work = probe.start(inputs).unwrap();
            let q = quote(inputs, limits(), work).unwrap();
            let mut exact = limits();
            exact.max_tensor_bytes = q.largest_tensor_bytes;
            exact.max_payload_bytes = q.payload_bytes;
            exact.max_chunk_bytes = q.chunk_bytes;
            exact.max_metadata_bytes = q.params_metadata_bytes + q.visitor_metadata_bytes;
            exact.max_native_bytes = q.new_owner_peak_bytes;
            exact.max_inventory_leaves = q.leaves;
            exact.max_total_read_bytes = q.payload_bytes;
            exact.max_work_per_attempt = q.metadata_work + q.data_work;
            exact.max_operation_bytes = exact.external_retained_bytes + q.new_owner_peak_bytes;
            for field in 0..9 {
                let mut cap = exact;
                match field {
                    0 => cap.max_tensor_bytes -= 1,
                    1 => cap.max_payload_bytes -= 1,
                    2 => cap.max_chunk_bytes -= 1,
                    3 => cap.max_metadata_bytes -= 1,
                    4 => cap.max_native_bytes -= 1,
                    5 => cap.max_inventory_leaves -= 1,
                    6 => cap.max_total_read_bytes -= 1,
                    7 => cap.max_work_per_attempt -= 1,
                    _ => cap.max_operation_bytes -= 1,
                }
                let before = file.stream_position().unwrap();
                let mut budget = OwnedParamBudgetV2::new(cap);
                assert_eq!(load_owned_artifact_params_v2(file, inputs, &mut budget, None).err(), Some(E::Limit), "field{field}");
                assert_eq!(file.stream_position().unwrap(), before); // No parameter seek/read before cap refusal.
            }
            let mut budget = OwnedParamBudgetV2::new(exact);
            let owner = load_owned_artifact_params_v2(file, inputs, &mut budget, None).unwrap();
            assert_eq!(owner.artifact_root(), inputs.expected.artifact_root);
        });
    }

    #[test]
    fn i16_per_layer_and_i32_multi_piece_values_keep_authoritative_root_and_metadata() {
        let mut b = ProgramBuilder::new(2, 1 << 18);
        let layered = b.param("layer.i16", DType::I16, &[2, 16385], true);
        let vector = b.param("global.i32", DType::I32, &[8193], false);
        let logits = b.param("logits", DType::I64, &[2], false);
        let mut pre = b.block("pre", vec![]);
        let vector = pre.reshape_fixed(vector, &[8193]);
        pre.commit(vector);
        let pre = pre.finish(&[]);
        let mut layer = b.block("layer", vec![]);
        let layered = layer.reshape_fixed(layered, &[2, 16385]);
        layer.commit(layered);
        let layer = layer.finish(&[]);
        let mut post = b.block("post", vec![]);
        let logits = post.reshape_fixed(logits, &[2]);
        post.commit(logits);
        let Ref::Node(logits) = logits else { unreachable!() };
        let post = post.finish(&[]);
        let mut program = b.finish(pre, vec![layer, layer], post, logits);
        program.logits_scheme_id = LOGITS_I64_Q24_SCHEME_V2;
        let program = TirProgramV1::decode_canonical(&program.encode()).unwrap();
        let plan = TirPlan::compile(&program).unwrap();
        // Independent native source literals, established BEFORE creating the
        // candidate file. No Tensor or full i128 mirror is constructed.
        let mut first = vec![0i16; 32770];
        for (at, value) in [(0, i16::MIN), (16383, i16::MAX), (16384, -1234), (16385, 2345), (32768, -2345), (32769, i16::MAX)] {
            first[at] = value;
        }
        let mut second = vec![0i16; 32770];
        for (at, value) in [(0, i16::MAX), (16383, i16::MIN), (16384, 1234), (16385, -2345), (32768, 2345), (32769, i16::MIN)] {
            second[at] = value;
        }
        let mut global = vec![0i32; 8193];
        global[0] = i32::MIN;
        global[8191] = -20261102;
        global[8192] = i32::MAX;
        let raw_first: Vec<u8> = first.iter().flat_map(|v| v.to_le_bytes()).collect();
        let raw_second: Vec<u8> = second.iter().flat_map(|v| v.to_le_bytes()).collect();
        let raw_global: Vec<u8> = global.iter().flat_map(|v| v.to_le_bytes()).collect();
        let raw_logits: Vec<u8> = [i64::MIN, i64::MAX].into_iter().flat_map(i64::to_le_bytes).collect();
        let raw = |param: u16, layer: Option<u16>| -> &[u8] {
            match (param, layer) {
                (0, Some(0)) => &raw_first,
                (0, Some(1)) => &raw_second,
                (1, None) => &raw_global,
                (2, None) => &raw_logits,
                _ => panic!("unexpected literal instance"),
            }
        };
        let mut expected_frontier = PalwArtifactMerkleFrontierV1::new();
        let mut expected_leaves = 0;
        // Authoritative Core geometry, independent native source bytes. Never
        // derive callerExpected from the candidate descriptor/header contents.
        palw_tir_visit_inventory_rows_v1(&program, &mut |row| {
            let bytes = raw(row.param, row.layer);
            let start = row.row_start as usize;
            let end = start + row.len as usize;
            expected_frontier.push(artifact_leaf_parts_v1(
                &program.params[row.param as usize].name,
                row.layer,
                row.row_start,
                &bytes[start..end],
            ));
            expected_leaves += 1;
        })
        .unwrap();
        assert_eq!(expected_leaves, 11); // 2layers*2rows*2pieces +2 I32 pieces +1 I64 piece.
        let artifact = expected_frontier.root().unwrap();
        let class = PalwTirClassV1 {
            version: 1,
            program: program.encode(),
            layout: PalwTirLayoutV1 {
                version: 1,
                max_context: 16,
                checkpoint_interval: 1,
                h_tile: 1,
                commit_tiles: vec![32; 3],
                state_tiles: vec![],
            },
            tokenizer_id: Hash64::from_u64_word(18),
        };
        let zero = Hash64::from_u64_word(0);
        let ctx = PalwJobContextV2 {
            version: 2,
            network_id: b"testnet-12".to_vec(),
            job_id: Hash64::from_u64_word(43),
            job_nullifier: zero,
            assignment_id: zero,
            execution_seed: [4; 32],
            model_profile_id: zero,
            runtime_manifest_hash: zero,
            runtime_class_id: zero,
            shape_profile_id: class.class_id(&artifact),
            trace_scheme_id: Hash64::from_bytes(LOGITS_I64_Q24_SCHEME_V2),
            cu_ruleset_id: zero,
            tokenizer_id: zero,
            prompt_token_ids_hash: zero,
            declared_prefill_tokens: 1,
            exact_decode_tokens: 1,
            max_context_tokens: 16,
        };
        let space = PalwTirStepSpaceV2::new(&class).unwrap();
        let count = space.leaf_count_capped(&ctx, 1 << 16).unwrap();
        let trace = Hash64::from_u64_word(71);
        let step = Hash64::from_u64_word(72);
        let binding = PalwTirStepBindingV1 {
            version: 1,
            job_context: ctx.clone(),
            class: class.clone(),
            artifact_root: artifact,
            full_logits_trace_root: trace,
            step_leaf_count: count,
            step_merkle_root: step,
            committed_execution_root: palw_tir_execution_root_v1(&ctx.context_hash(), &trace, &ctx.shape_profile_id, count, &step),
        };
        let verified = verify_tir_binding_v2(&binding, 1 << 16).unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("multi-piece.palwtir");
        write_container_v1(
            &path,
            &program,
            borsh::to_vec(&class.layout).unwrap(),
            class.tokenizer_id.as_bytes(),
            "independent-native-endpoints".into(),
            &mut |j, l| Ok(raw(j, l).to_vec()),
        )
        .unwrap();
        let mut file = OpenOptions::new().read(true).write(true).open(&path).unwrap();
        let file_len = file.metadata().unwrap().len();
        let header_limits = PalwTirHeaderLimitsV1 {
            max_file_bytes: 1 << 20,
            max_header_bytes: 1 << 20,
            max_program_bytes: 1 << 18,
            max_layout_bytes: 1 << 16,
            max_tensors: 32,
            max_meta_bytes: 1024,
            max_native_bytes: 1 << 20,
            max_copy_bytes: 1 << 20,
            max_work: 1 << 32,
            max_operation_bytes: 1 << 24,
            external_retained_bytes: 1 << 22,
        };
        let metadata = read_header_bounded_v1(&mut file, file_len, header_limits).unwrap();
        let checked = metadata.validate_with_program(&program, header_limits).unwrap();
        assert_eq!(checked.header().tensors.len(), 4);
        let inputs =
            OwnedParamInputsV2 { checked: &checked, plan: &plan, expected: &binding, verified: &verified, original_context: &ctx };
        let held = Arc::new(AtomicU64::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let reserve = reserver(held.clone(), calls.clone());
        let mut budget = OwnedParamBudgetV2::new(limits());
        let owner = load_owned_artifact_params_v2(&mut file, inputs, &mut budget, Some(&reserve)).unwrap();
        let Slice::I16(actual_first) = owner.params().get(0, Some(0)).unwrap() else { panic!("I16 layer0") };
        let Slice::I16(actual_second) = owner.params().get(0, Some(1)).unwrap() else { panic!("I16 layer1") };
        let Slice::I32(actual_global) = owner.params().get(1, None).unwrap() else { panic!("I32 global") };
        assert_eq!(actual_first, first.as_slice());
        assert_eq!(actual_second, second.as_slice());
        assert_eq!(actual_global, global.as_slice());
        assert_eq!(
            (actual_first[16383], actual_first[16384], actual_first[16385], actual_first[32769]),
            (i16::MAX, -1234, 2345, i16::MAX)
        );
        assert_eq!(
            (actual_second[16383], actual_second[16384], actual_second[16385], actual_second[32769]),
            (i16::MIN, 1234, -2345, i16::MIN)
        );
        assert_eq!((actual_global[0], actual_global[8191], actual_global[8192]), (i32::MIN, -20261102, i32::MAX));
        let q = owner.quote();
        let table = quote_owned_params_table_v2(&program).unwrap();
        assert_eq!(table.layer_slots, 4); // Params::new allocates2+1+1 original slots.
        assert_eq!(q.params_metadata_bytes, table.root_bytes + table.heap_bytes);
        assert_eq!(q.payload_bytes, 2 * (2 * 16385 * 2) + 8193 * 4 + 2 * 8);
        assert_eq!(q.payload_bytes, 163868);
        assert_eq!(q.largest_tensor_bytes, 65540);
        assert_eq!(q.chunk_bytes, 32768);
        assert_eq!(q.leaves, 11);
        assert_eq!(budget.usage().read_bytes, 163868);
        assert_eq!(budget.usage().leaves, 11);
        assert_eq!(budget.usage().work, q.metadata_work + q.data_work);
        assert_eq!(owner.artifact_root(), artifact);
        assert_eq!(owner.layout_origin(), OwnedParamLayoutOriginV2::DeclaredInFile);
        assert_eq!(owner.original_context(), &ctx);
        assert_eq!(ctx.tokenizer_id, zero);
        assert_ne!(class.tokenizer_id, ctx.tokenizer_id);
        assert_eq!(class.layout.max_context, 16);
        assert_eq!(ctx.max_context_tokens, 16);
        assert_eq!(held.load(Ordering::SeqCst), q.new_owner_peak_bytes);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        drop(owner);
        assert_eq!(held.load(Ordering::SeqCst), 0);
    }
}
