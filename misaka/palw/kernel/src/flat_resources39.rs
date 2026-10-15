//! Independent maximum/actual quotes from the owning39 body. Values are bounded
//! allowances, not a measured CPU/RSS claim. Every axis is derived; caller
//! vectors, input shapes and post-claim length expansion are not accepted.
use crate::{
    flat_prepared39::PreparedV39,
    flat_resources10::{ResourceAxisV10 as A, ResourceQuantitiesV10},
    flat_source39::CheckedSourceV39,
    hash::{Digest, object_id},
    merkle3::LayoutV3,
};
use borsh::BorshSerialize;
use misaka_palw_tir::{Prim, program_v39::NodePrimV39};
pub const COURT39_MAX_BYTES: u64 = (64 << 20) - 75;
pub const COURT39_MAX_LEAVES: u64 = 65536;
pub const COURT39_MAX_SCRATCH: u64 = 1 << 30;
pub const COURT39_MAX_WORK: u64 = 1 << 38;
fn add(a: u128, b: u128) -> Result<u128, String> {
    a.checked_add(b).ok_or("resource39 addition overflow".into())
}
fn mul(a: u128, b: u128) -> Result<u128, String> {
    a.checked_mul(b).ok_or("resource39 multiplication overflow".into())
}
fn product(s: &[usize]) -> Result<u128, String> {
    s.iter().try_fold(1u128, |n, d| mul(n, *d as u128))
}
fn setadd(q: &mut ResourceQuantitiesV10, a: A, n: u128) -> Result<(), String> {
    q.set(a, add(q.get(a), n)?);
    Ok(())
}
pub fn ordinary_court_id_v39() -> Digest {
    crate::hash::id(b"misaka-palw/kernel/ordinary-full-history/v39",b"canonical39;scalar-element;original-wiring-and-real-PC;actual-min(H,cap)-selection;greedy-low-index;trace-header;realPC-header;source39;64MiB-total-including-salt64-length4-wrapper7;65536-leaves;1GiB;2^38;u16x2048;state-continuity")
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatCourtScopeV39 {
    pub version: u16,
    pub source: Digest,
    pub profile: Digest,
    pub program: Digest,
    pub parameters: Digest,
    pub inventory: Digest,
    pub positions: u32,
    pub court: Digest,
    pub maximum_bytes: u64,
    pub maximum_leaves: u64,
    pub maximum_work: u64,
    pub maximum_scratch: u64,
    pub parts: u16,
}
impl FlatCourtScopeV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/court-scope/v39", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatResourceVectorV39 {
    pub version: u16,
    pub source: Digest,
    pub profile: Digest,
    pub positions: u32,
    pub scope: Digest,
    pub quantities: ResourceQuantitiesV10,
}
impl FlatResourceVectorV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/reference-resource/v39", self)
    }
}
pub struct CheckedFlatResourcesV39 {
    scope: FlatCourtScopeV39,
    vector: FlatResourceVectorV39,
}
impl CheckedFlatResourcesV39 {
    pub fn scope(&self) -> &FlatCourtScopeV39 {
        &self.scope
    }
    pub fn vector(&self) -> &FlatResourceVectorV39 {
        &self.vector
    }
    pub fn root(&self) -> Digest {
        self.vector.root()
    }
}
/// Full line dependency count. Structural/pointwise relations read one element
/// per operand; contractions/selection read their ORIGINAL axis in full.
pub fn scalar_reads_v39(pr: &PreparedV39<'_>, p: u32, s: u16, n: u16) -> Result<Vec<u64>, String> {
    let node = pr.node(s, n)?;
    let sh = pr.shapes(p, s, n)?;
    let mut reads = vec![1; sh.inputs.len()];
    match &node.prim {
        NodePrimV39::HistorySelect { axis, .. }
        | NodePrimV39::Frozen(Prim::TopK { axis, .. } | Prim::ReduceSum { axis } | Prim::ReduceMax { axis }) => {
            reads[0] = sh.inputs[0][*axis as usize] as u64
        }
        NodePrimV39::Frozen(Prim::MatMul) => {
            reads[0] = *sh.inputs[0].last().ok_or("resource39 MatMul rank")? as u64;
            reads[1] = sh.inputs[1][sh.inputs[1].len() - 2] as u64;
        }
        _ => {}
    }
    Ok(reads)
}
pub fn derive_flat_resources_v39(source: &CheckedSourceV39, positions: u32) -> Result<CheckedFlatResourcesV39, String> {
    let pr = PreparedV39::new(source, positions)?;
    let program = source.program();
    let mut q = ResourceQuantitiesV10::default();
    let mut court_bytes = 0u128;
    let mut court_work = 0u128;
    let mut court_scratch = 0u128;
    let mut court_leaves = 0u128;
    let mut live = 0u128;
    let mut tree_peak = 0u128;
    let mut reference_payload = 0u128;
    let body = source.wire().program_bytes.len() as u128;
    let metadata = add(mul(body, 1024)?, mul(pr.node_count() as u128, 256)?)?;
    for (s, (b, _)) in pr.occurrences().iter().enumerate() {
        let block = &program.blocks[*b as usize];
        let mut last: Vec<usize> = (0..block.nodes.len()).collect();
        for (n, node) in block.nodes.iter().enumerate() {
            for r in &node.inputs {
                if let misaka_palw_tir::Ref::Node(j) = r {
                    last[*j as usize] = n;
                }
            }
        }
        for n in &block.carry_out {
            last[*n as usize] = block.nodes.len();
        }
        let h = pr.history(positions - 1, s as u16)?;
        let sizes: Vec<u128> = block.nodes.iter().map(|n| n.out.elements_at(h).map(|n| n as u128 * 16)).collect::<Result<_, _>>()?;
        let carry = block.carry_in.iter().try_fold(0u128, |n, t| add(n, mul(t.elements_at(h)? as u128, 16)?))?;
        let mut release = vec![0u128; block.nodes.len()];
        for (j, end) in last.iter().enumerate() {
            if *end < release.len() {
                release[*end] = add(release[*end], sizes[j])?;
            }
        }
        let mut retained = 0u128;
        for (n, node) in block.nodes.iter().enumerate() {
            let sh = pr.shapes(positions - 1, s as u16, n as u16)?;
            let out = product(&sh.out)?;
            let mut input = 0u128;
            let mut copies = out;
            for (i, shape) in sh.inputs.iter().enumerate() {
                // The implemented39 direct parameter Gather reads only selected
                // contiguous source ranges, not the full vocabulary/expert bank.
                let lazy = i == 0
                    && matches!(node.prim, NodePrimV39::Frozen(Prim::Gather { .. }))
                    && matches!(node.inputs.first(), Some(misaka_palw_tir::Ref::Param(_)));
                input = add(input, if lazy { out } else { product(shape)? })?;
            }
            let mut scalar = mul(add(input, out)?, 16)?;
            let mut macs = 0u128;
            match &node.prim {
                NodePrimV39::Frozen(Prim::MatMul) => {
                    macs = mul(out, *sh.inputs[0].last().ok_or("resource39 matmul rank")? as u128)?;
                    scalar = mul(macs, 16)?;
                }
                NodePrimV39::HistorySelect { axis, .. } | NodePrimV39::Frozen(Prim::TopK { axis, .. }) => {
                    let len = sh.inputs[0][*axis as usize] as u128;
                    let log = (128 - len.max(1).leading_zeros()) as u128;
                    scalar = mul(mul(input, log)?, 64)?;
                }
                _ => {}
            }
            // Same original node last-use schedule as Interpreter39. Include old
            // carry and conservative arithmetic/broadcast temporaries, but never
            // multiply all forty layer intermediates into simultaneously live RAM.
            let backend = add(add(retained, carry)?, mul(add(input, out)?, 128)?)?;
            live = live.max(backend);
            reference_payload = reference_payload.max(add(add(retained, carry)?, mul(add(input, out)?, 16)?)?);
            retained = add(retained, sizes[n])?;
            retained = retained.checked_sub(release[n]).ok_or("resource39 live ownership underflow")?;
            copies = add(copies, input)?;
            for (a, v) in [
                (A::Macs, macs),
                (A::ScalarSteps, scalar),
                (A::IndexSteps, mul(add(input, out)?, 32)?),
                (A::ValidatedElements, add(input, out)?),
                (A::CopiedReferenceBytes, mul(copies, 32)?),
            ] {
                setadd(&mut q, a, mul(v, positions as u128)?)?;
            }
            {
                let l = LayoutV3::try_of(&sh.out).ok_or("resource39 committed geometry")?;
                let leaves = add(l.leaves(0) as u128, l.leaves(1) as u128)?;
                let hashed = add(mul(out, 2 * node.out.dtype.width() as u128)?, mul(leaves, 512)?)?;
                setadd(&mut q, A::TraceHashBytes, mul(hashed, positions as u128)?)?;
                setadd(
                    &mut q,
                    A::StoredEvidenceBytes,
                    mul(add(add(mul(leaves, 128)?, mul(out, node.out.dtype.width() as u128)?)?, 128)?, positions as u128)?,
                )?;
                tree_peak = tree_peak.max(add(131072, mul((128 - leaves.max(1).leading_zeros()) as u128, 256)?)?);
            }
            let reads = scalar_reads_v39(&pr, positions - 1, s as u16, n as u16)?;
            let mut bytes = add(4096, mul(pr.node_count() as u128, 0)?)?;
            let mut leaves = 1u128;
            let mut work = metadata;
            let mut scratch = 8u128 << 20;
            // Conservative one row/column leaf per read works for strided axes too;
            // contiguous lines use the exact 4096-cell PC tile bound.
            for (i, r) in reads.iter().enumerate() {
                let shape = &sh.inputs[i];
                let layout = LayoutV3::try_of(shape).ok_or("resource39 scalar operand geometry")?;
                let contiguous = match &node.prim {
                    NodePrimV39::Frozen(Prim::MatMul) => true,
                    NodePrimV39::HistorySelect { axis, .. }
                    | NodePrimV39::Frozen(Prim::TopK { axis, .. } | Prim::ReduceSum { axis } | Prim::ReduceMax { axis }) => {
                        *axis as usize + 2 >= shape.len()
                    }
                    _ => false,
                };
                let amount = if contiguous { r.div_ceil(4096) + 1 } else { (*r).min(layout.leaves(0)) };
                leaves = add(leaves, amount as u128)?;
                // Original large parameter trees can exceed sixteen levels;
                // bind the exact largest row/column path before Fund.
                let path = crate::merkle::depth(layout.leaves(0).max(layout.leaves(1))) as u128;
                let framing = add(add(128, mul(shape.len() as u128, 8)?)?, mul(path, 64)?)?;
                let leaf = add(framing, mul(4096, sh.in_dtypes[i].width() as u128)?)?;
                bytes = add(bytes, mul(amount as u128, leaf)?)?;
                work = add(work, mul(*r as u128, 1024)?)?;
                scratch = add(scratch, add(mul(*r as u128, 64)?, mul(mul(amount as u128, 4096)?, 128)?)?)?;
            }
            bytes = add(bytes, 80 << 10)?;
            scratch = add(scratch, bytes)?;
            court_bytes = court_bytes.max(bytes).max(256 << 10);
            court_leaves = court_leaves.max(leaves);
            court_work = court_work.max(work);
            court_scratch = court_scratch.max(scratch);
        }
    }
    // Weight/hash storage is based on the original declarations, independent of
    // job positions or selected history. A shorter job never shrinks a weight.
    for (j, d) in program.params.iter().enumerate() {
        let instances = if d.per_layer { program.schedule.layers.len() } else { 1 };
        let shape: Vec<_> = d.shape.iter().map(|d| *d as usize).collect();
        let cells = product(&shape)?;
        let raw = mul(cells, d.dtype.width() as u128)?;
        let l = LayoutV3::try_of(&shape).ok_or("resource39 real parameter geometry")?;
        let leaves = add(l.leaves(0) as u128, l.leaves(1) as u128)?;
        setadd(&mut q, A::RawParameterStorage, mul(raw, instances as u128)?)?;
        setadd(&mut q, A::ParameterHashBytes, mul(add(mul(raw, 2)?, mul(leaves, 512)?)?, instances as u128)?)?;
        let _ = j;
        tree_peak = tree_peak.max(add(131072, mul((128 - leaves.max(1).leading_zeros()) as u128, 256)?)?);
    }
    setadd(&mut q, A::ParameterCalls, mul(mul(pr.node_count() as u128, program.params.len() as u128)?, positions as u128)?)?;
    let mut states = 0u128;
    for s in &program.states {
        let cells = s.shape.iter().try_fold(1u128, |n, d| mul(n, *d as u128))?;
        let history = match s.kind {
            misaka_palw_tir::StateKind::Fixed { .. } => 1,
            misaka_palw_tir::StateKind::Hist { window } => positions.min(window),
        };
        let instances = if s.per_layer { program.schedule.layers.len() } else { 1 };
        states = add(states, mul(mul(mul(cells, history as u128)?, instances as u128)?, 32)?)?;
    }
    let input =
        program.inputs.iter().try_fold(0u128, |n, d| add(n, mul(d.shape.iter().try_fold(1u128, |n, d| mul(n, *d as u128))?, 16)?))?;
    let mut input_hash = 0u128;
    let mut input_pc_work = 0u128;
    for d in &program.inputs {
        let cells = d.shape.iter().try_fold(1u128, |n, d| mul(n, *d as u128))?;
        input_hash = add(input_hash, mul(cells, 2 * d.dtype.width() as u128)?)?;
        input_pc_work = add(input_pc_work, mul(cells, 2048)?)?;
    }
    setadd(&mut q, A::ParameterHashBytes, mul(input_hash, positions as u128)?)?;
    setadd(&mut q, A::ParameterCalls, mul(mul(input, 2)?, positions as u128)?)?;
    setadd(&mut q, A::MetadataWork, mul(input_pc_work, positions as u128)?)?;
    court_work = add(court_work, input_pc_work)?;
    let owned = add(add(add(live, states)?, mul(input, 2)?)?, tree_peak)?;
    let metadata_owned = add(add(mul(body, 8)?, mul(pr.node_count() as u128, 4096)?)?, mul(positions as u128, 256)?)?;
    for (a, v) in [
        (A::ReferenceTensorPayload, add(reference_payload, states)?),
        (A::ContainerAllocatorAllowance, metadata_owned),
        (A::DeliveryBytes, q.get(A::StoredEvidenceBytes)),
        (A::ReplyCalls, pr.node_count() as u128),
        (A::Attempts, 2),
        (A::CourtWork, court_work),
        (A::CourtBytes, court_bytes),
        (A::CourtScratchBytes, court_scratch),
        (A::MetadataWork, add(metadata, mul(input_pc_work, positions as u128)?)?),
        (A::ParameterTreeScratch, tree_peak),
        (A::OwnedPeakBytes, add(owned, metadata_owned)?),
    ] {
        q.set(a, v);
    }
    if court_bytes > COURT39_MAX_BYTES as u128
        || court_leaves > COURT39_MAX_LEAVES as u128
        || court_work > COURT39_MAX_WORK as u128
        || court_scratch > COURT39_MAX_SCRATCH as u128
    {
        return Err("resource39 complete original relation exceeds owning court profile".into());
    }
    let scope = FlatCourtScopeV39 {
        version: 39,
        source: source.root(),
        profile: source.profile_root(),
        program: source.program_root(),
        parameters: source.parameters().root(),
        inventory: source.inventory_root(),
        positions,
        court: ordinary_court_id_v39(),
        maximum_bytes: court_bytes as u64,
        maximum_leaves: court_leaves as u64,
        maximum_work: court_work as u64,
        maximum_scratch: court_scratch as u64,
        parts: u16::try_from(
            (court_bytes as u64).checked_add(75).ok_or("resource39 whole filing wrapper overflow")?.div_ceil(32 << 10),
        )
        .map_err(|_| "resource39 court page count")?,
    };
    let vector = FlatResourceVectorV39 {
        version: 39,
        source: source.root(),
        profile: source.profile_root(),
        positions,
        scope: scope.root(),
        quantities: q,
    };
    Ok(CheckedFlatResourcesV39 { scope, vector })
}
