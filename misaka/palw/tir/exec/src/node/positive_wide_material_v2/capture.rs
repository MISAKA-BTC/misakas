use super::*;
use crate::node::{
    run::TirRunObserverV1,
    run_wide_q24_v2::{TirClassRunnerWideQ24V2, TirLeafOutWideQ24V2, TirWideRunLimitsV2},
};
use crate::{NodeValue, Slice};
use kaspa_consensus_core::{
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1},
    palw_step_leg::{PalwStepTileLeafV1, step_tile_leaf_hash_v1},
    palw_tir_step_v1::{palw_tir_execution_root_v1, palw_tir_leaf_values_v2},
};
use misaka_palw_positive_contract::{identity::Digest, public_one_honest::OriginalCommit};
use misaka_palw_tir::{Prim, demand::history_length_v1};
use std::{cell::Cell, rc::Rc};
/// Compare canonical original program bytes without allocating an encoded Vec.
/// A foreign expected class may be much smaller than the independently held
/// space; its claimed length must never size a clone of that other program.
fn program_matches(program: &misaka_palw_tir::TirProgramV1, expected: &[u8]) -> bool {
    use borsh::BorshSerialize;
    struct Match<'a>(&'a [u8]);
    impl std::io::Write for Match<'_> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if !self.0.starts_with(b) {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
            self.0 = &self.0[b.len()..];
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut out = Match(expected);
    program.serialize(&mut out).is_ok() && out.0.is_empty()
}
/// Metadata comes from the actual registry/job owner, not a peer capture. This
/// borrowed DTO is no native authorization and does not invent an assignment.
pub struct CaptureExpectedV2<'a> {
    pub class: &'a PalwTirClassV1,
    pub artifact: Hash64,
    pub job: &'a PalwJobContextV2,
    pub space: &'a PalwTirStepSpaceV2,
    pub prompt: &'a [u32],
}
struct Leaf {
    version: u16,
    coord: kaspa_consensus_core::palw_step::PalwStepCoordinateV1,
    count: u32,
    offset: u64,
    bytes: u64,
}
pub struct WidePanelCaptureV2<'a> {
    pub(super) expected: CaptureExpectedV2<'a>,
    pub(super) context: Hash64,
    pub(super) run: TirJobRunWideQ24V2,
    pub(super) records: Vec<Record>,
    leaves: Vec<Leaf>,
    pub(super) tape: Tape,
    pub(super) session: Rc<Session>,
    pub(super) builder_live: Cell<bool>,
    pub(super) retained_heap: u64,
    _lease: TirMemoryLeaseV1,
}
impl WidePanelCaptureV2<'_> {
    pub fn meter(&self) -> WidePanelMeterV2 {
        *self.session.meter.borrow()
    }
    pub fn run(&self) -> &TirJobRunWideQ24V2 {
        &self.run
    }

    /// Set the original's trace and output from this single observed run.
    /// The native execution root binds both the step tree and logits trace.
    /// Call before constructing the manifest and signing the original.
    pub fn bind_public_original(&self, original: &mut OriginalCommit) -> Result<(), Error> {
        let expected = &self.expected;
        let job = expected.job;
        if original.manifest_root != Digest([0; 64])
            || original.job.0 != job.job_id.as_bytes()
            || original.execution.0 != self.context.as_bytes()
            || original.registered_class.0 != job.shape_profile_id.as_bytes()
            || original.artifact.0 != expected.artifact.as_bytes()
            || original.input_tokens.0 != job.prompt_token_ids_hash.as_bytes()
            || original.tokenizer.0 != job.tokenizer_id.as_bytes()
            || original.execution_rules.0 != job.cu_ruleset_id.as_bytes()
            || original.generation_seed != job.execution_seed
            || original.prefill != job.declared_prefill_tokens
            || original.decode != job.exact_decode_tokens
            || original.max_context != job.max_context_tokens
        {
            return Err("public original does not match the captured job".into());
        }
        self.session.charge(Charge::Hash, count(expected.class.program.len())?)?;
        if original.program.0 != expected.class.graph_ir_root().as_bytes() {
            return Err("public original does not match the captured program".into());
        }
        original.trace_root = Digest(self.run.execution_root().as_bytes());
        original.output = Digest(self.run.output_root().as_bytes());
        Ok(())
    }
    /// Owned metadata export. The caller MUST acquire an independent export
    /// guard before this call, additive to all earlier copies, and retain it
    /// until the returned binding/copies are dropped, even after capture Drop.
    /// The capture lease and copy-work counter do not own that returned lifetime.
    pub fn binding(&self) -> Result<PalwTirStepBindingV1, Error> {
        let c = self.expected.class;
        let bytes = add(
            count(c.program.len())?,
            add(
                mul(count(c.layout.commit_tiles.len() + c.layout.state_tiles.len())?, 4)?,
                add(count(self.expected.job.network_id.len())?, 1024)?,
            )?,
        )?;
        self.session.charge(Charge::Copy, bytes)?;
        let e = &self.expected;
        Ok(PalwTirStepBindingV1 {
            version: 1,
            class: e.class.clone(),
            artifact_root: e.artifact,
            job_context: e.job.clone(),
            full_logits_trace_root: self.run.trace_root(),
            step_leaf_count: self.run.leaf_count(),
            step_merkle_root: self.run.step_merkle_root(),
            committed_execution_root: self.run.execution_root(),
        })
    }
    pub(super) fn record(&self, key: Key) -> Result<&Record, Error> {
        self.session.charge(Charge::Visit, 64)?;
        let i = self.records.binary_search_by_key(&key, |r| r.key).map_err(|_| "wide Panel missing record")?;
        Ok(&self.records[i])
    }
    pub(super) fn value(&self, key: Key, lane: usize) -> Result<i128, Error> {
        let r = self.record(key)?;
        if r.shape().iter().product::<usize>() as u64 != r.lanes {
            return Err("wide tape metadata shape changed".into());
        }
        if count(lane)? >= r.lanes {
            return Err("wide Panel lane range".into());
        }
        match r.storage {
            Storage::Bytes(at) => {
                let w = r.dtype.width();
                let mut b = [0; 16];
                self.tape.read(add(at, mul(count(lane)?, w as u64)?)?, &mut b[..w], &self.session)?;
                Ok(decode(r.dtype, &b[..w]))
            }
            Storage::StepView { position, occurrence, node } => {
                let space = self.expected.space;
                let (block, _) = space.occurrences()[occurrence as usize];
                let tile = space.commit_tile_len(block, node).ok_or("wide committed view tile")?;
                let slot = space.node_slot(occurrence as usize, node).ok_or("wide committed view slot")?;
                let job = space.job_shape(self.expected.job).map_err(|e| e.to_string())?;
                let (call_index, p) = job.call_position(position);
                let coord = kaspa_consensus_core::palw_step::PalwStepCoordinateV1 {
                    call_index,
                    node_slot: slot,
                    position: p,
                    tile_index: (lane / tile as usize) as u32,
                };
                self.session.charge(Charge::Visit, mul(add(count(self.expected.class.program.len())?, 1024)?, 128)?)?;
                let index = space.leaf_index(self.expected.job, &coord).ok_or("wide committed view source")?;
                let image = self.preimage(index)?;
                let width = if r.dtype == DType::I64 { 8 } else { 4 };
                let at = (lane % tile as usize) * width;
                let bytes = image.values_le.get(at..at + width).ok_or("wide committed view lane")?;
                Ok(if width == 8 {
                    i64::from_le_bytes(bytes.try_into().map_err(|_| "wide lane8")?) as i128
                } else if r.dtype == DType::Idx {
                    u32::from_le_bytes(bytes.try_into().map_err(|_| "wide lane4")?) as i128
                } else {
                    i32::from_le_bytes(bytes.try_into().map_err(|_| "wide lane4")?) as i128
                })
            }
            Storage::HistoryView { state, layer, first, row_lanes } => {
                let row = count(lane)? / row_lanes;
                let pos = u32::try_from(add(u64::from(first), row)?).map_err(|_| "wide history position")?;
                let r = self.record(Key::History(pos, state, layer))?;
                let Storage::Bytes(at) = r.storage else { return Err("wide history nested view".into()) };
                let w = r.dtype.width();
                let mut b = [0; 16];
                self.tape.read(add(at, mul(count(lane)? % row_lanes, w as u64)?)?, &mut b[..w], &self.session)?;
                Ok(decode(r.dtype, &b[..w]))
            }
        }
    }
    /// Bounded contiguous claim extraction. Native i64/i128 lanes remain exact;
    /// no full-node/window clone and no per-lane disk syscall in the common path.
    pub(super) fn values(&self, key: Key, first: u64, n: u32) -> Result<Vec<i128>, Error> {
        let r = self.record(key)?;
        if first.checked_add(u64::from(n)).is_none_or(|end| end > r.lanes)
            || mul(u64::from(n), 16)? > self.session.limits.max_claim_bytes
        {
            return Err("wide claim slice extent/cap".into());
        }
        let mut out = vec_exact(n as usize)?;
        match r.storage {
            Storage::Bytes(at) => self.read_values(at, r.dtype, first, n, &mut out)?,
            Storage::HistoryView { state, layer, first: row0, row_lanes } => {
                let end = first + u64::from(n);
                let mut lane = first;
                while lane < end {
                    let row = lane / row_lanes;
                    let within = lane % row_lanes;
                    let take = (end - lane).min(row_lanes - within);
                    let position = u32::try_from(add(u64::from(row0), row)?).map_err(|_| "wide history position width")?;
                    let record = self.record(Key::History(position, state, layer))?;
                    let Storage::Bytes(at) = record.storage else {
                        return Err("wide raw append row is not stored once".into());
                    };
                    self.read_values(
                        at,
                        record.dtype,
                        within,
                        u32::try_from(take).map_err(|_| "wide history claim width")?,
                        &mut out,
                    )?;
                    lane += take;
                }
            }
            Storage::StepView { .. } => {
                for lane in first..first + u64::from(n) {
                    out.push(self.value(key, lane as usize)?);
                }
            }
        }
        Ok(out)
    }
    fn read_values(&self, at: u64, dtype: DType, first: u64, n: u32, out: &mut Vec<i128>) -> Result<(), Error> {
        let width = dtype.width();
        let mut done = 0usize;
        let mut chunk = [0u8; 4096];
        while done < n as usize {
            let take = (n as usize - done).min(chunk.len() / width);
            let bytes = take * width;
            let offset = add(at, mul(add(first, count(done)?)?, width as u64)?)?;
            self.tape.read(offset, &mut chunk[..bytes], &self.session)?;
            out.extend(chunk[..bytes].chunks_exact(width).map(|b| dtype.decode_le(b)));
            done += take;
        }
        Ok(())
    }
    pub(super) fn preimage_bytes(&self, index: u64) -> Result<u64, Error> {
        self.session.charge(Charge::Visit, 1)?;
        self.leaves.get(usize::try_from(index).map_err(|_| "wide leaf index")?).map(|l| l.bytes).ok_or("wide leaf absent".into())
    }
    pub(super) fn preimage(&self, index: u64) -> Result<PalwStepTileLeafV1, Error> {
        self.session.charge(Charge::Visit, 1)?;
        let l = self.leaves.get(usize::try_from(index).map_err(|_| "wide leaf index")?).ok_or("wide leaf absent")?;
        if l.bytes > self.session.limits.max_material_bytes {
            return Err("wide leaf body cap".into());
        }
        let mut bytes = vec_exact(usize::try_from(l.bytes).map_err(|_| "wide leaf width")?)?;
        bytes.resize(l.bytes as usize, 0);
        self.tape.read(l.offset, &mut bytes, &self.session)?;
        let image = PalwStepTileLeafV1 { version: l.version, coord: l.coord, value_count: l.count, values_le: bytes };
        self.session.charge(Charge::Hash, add(l.bytes, 1)?)?;
        if step_tile_leaf_hash_v1(&self.context, &self.expected.job.shape_profile_id, &image)
            != *self.run.leaf_hashes().get(index as usize).ok_or("wide original leaf hash absent")?
        {
            return Err("wide tape leaf differs from original run".into());
        }
        Ok(image)
    }
}
struct Build<'a> {
    e: CaptureExpectedV2<'a>,
    class_id: Hash64,
    context: Hash64,
    tape: Tape,
    session: Rc<Session>,
    records: Vec<Record>,
    leaves: Vec<Leaf>,
    position: u32,
    occ: usize,
    node: usize,
    outputs: u32,
}
impl Build<'_> {
    fn push(&mut self, key: Key, dtype: DType, shape: &[usize], data: Slice<'_>, from: usize, n: usize) -> Result<(), Error> {
        if self.records.len() as u64 >= self.session.limits.max_records || from.checked_add(n).is_none_or(|e| e > data.len()) {
            return Err("wide record cap/extent".into());
        }
        self.session.charge(Charge::Visit, count(n)?)?;
        let shape = Shape::new(shape)?;
        let lanes = shape.0[..shape.1].iter().try_fold(1u64, |n, d| mul(n, *d as u64))?;
        if lanes != count(n)? {
            return Err("wide record shape".into());
        }
        let at = self.tape.len;
        let w = dtype.width();
        let total = mul(count(n)?, w as u64)?;
        if add(at, total)? > self.session.limits.max_tape_bytes {
            return Err("wide record tape preflight".into());
        }
        let mut chunk = [0u8; 4096];
        let mut used = 0;
        for i in from..from + n {
            let v = data.get(i);
            if !dtype.contains(v) {
                return Err("wide original dtype value".into());
            }
            chunk[used..used + w].copy_from_slice(&v.to_le_bytes()[..w]);
            used += w;
            if used == chunk.len() {
                self.tape.append(&chunk, &self.session)?;
                used = 0;
            }
        }
        if used > 0 {
            self.tape.append(&chunk[..used], &self.session)?;
        }
        self.records.push(Record { key, dtype, rank: shape.1, shape: shape.0, lanes, storage: Storage::Bytes(at) });
        Ok(())
    }
    fn normalize_cursor(&mut self) -> Result<(), Error> {
        let job = self.e.space.job_shape(self.e.job).map_err(|e| e.to_string())?;
        while self.position < job.positions {
            if self.occ >= self.e.space.occurrences().len() {
                self.position += 1;
                self.occ = 0;
                self.node = 0;
                continue;
            }
            if (self.occ + 1 == self.e.space.occurrences().len() && !job.runs_post(self.position))
                || self.node >= self.e.space.program().blocks[self.e.space.occurrences()[self.occ].0 as usize].nodes.len()
            {
                self.occ += 1;
                self.node = 0;
                continue;
            }
            break;
        }
        Ok(())
    }
    fn leaf(&mut self, out: &TirLeafOutWideQ24V2<'_>) -> Result<(), Error> {
        if out.index != self.leaves.len() as u64 || out.index >= self.session.limits.max_step_leaves {
            return Err("wide leaf order".into());
        }
        if out.preimage.values_le.len() as u64 > self.session.limits.max_material_bytes {
            return Err("wide emitted leaf scratch cap".into());
        }
        self.session.charge(Charge::Visit, mul(add(count(self.e.class.program.len())?, 1024)?, 128)?)?;
        let canonical = self.e.space.leaf_at(self.e.job, out.index).ok_or("wide extra leaf")?;
        self.session.charge(Charge::Hash, add(count(out.preimage.values_le.len())?, 1)?)?;
        drop(palw_tir_leaf_values_v2(self.e.space, self.e.job, &canonical, out.preimage).map_err(|e| e.to_string())?);
        if step_tile_leaf_hash_v1(&self.context, &self.class_id, out.preimage) != out.hash {
            return Err("wide emitted leaf hash".into());
        }
        let at = self.tape.append(&out.preimage.values_le, &self.session)?;
        self.leaves.push(Leaf {
            version: out.preimage.version,
            coord: out.preimage.coord,
            count: out.preimage.value_count,
            offset: at,
            bytes: count(out.preimage.values_le.len())?,
        });
        Ok(())
    }
}
struct Observer<'a>(Rc<RefCell<Build<'a>>>);
impl TirRunObserverV1 for Observer<'_> {
    fn node(&mut self, position: u32, v: &NodeValue<'_>) -> Result<(), String> {
        let mut b = self.0.borrow_mut();
        let geometry_bound = mul(add(count(b.e.class.program.len())?, 1024)?, 8)?;
        b.session.charge(Charge::Visit, geometry_bound)?;
        b.normalize_cursor()?;
        if position != b.position {
            return Err("wide node position/order".into());
        }
        let (block, layer) = b.e.space.occurrences()[b.occ];
        let space = b.e.space;
        let node = &space.program().blocks[block as usize].nodes[b.node];
        let h = history_length_v1(b.e.space.info(), block, position).ok_or("wide node history")?;
        let shape = node.out.resolve(h);
        let slot = b.e.space.occurrences()[..b.occ].iter().try_fold(0u32, |a, (block, _)| {
            a.checked_add(b.e.space.program().blocks[*block as usize].nodes.len() as u32).ok_or("wide slot")
        })? + b.node as u32;
        if (v.block, v.layer, v.node, v.slot, v.commit, v.dtype) != (block, layer, b.node as u16, slot, node.commit, node.out.dtype)
            || v.shape != shape
            || v.data.len() != shape.iter().product::<usize>()
        {
            return Err("wide node original geometry".into());
        }
        let key = Key::Node(position, b.occ as u16, b.node as u16);
        let dtype = node.out.dtype;
        let hist = if let Prim::HistAppend { state } = node.prim { Some(state) } else { None };
        if let Some(state) = hist {
            let decl = &b.e.space.program().states[state as usize];
            let row = decl.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
            let n = row.iter().product::<usize>();
            let layer = decl.per_layer.then_some(layer).flatten();
            b.push(Key::History(position, state, layer), dtype, &row, v.data, v.data.len() - n, n)?;
            let s = Shape::new(&shape)?;
            if b.records.len() as u64 >= b.session.limits.max_records {
                return Err("wide history metadata cap".into());
            }
            b.records.push(Record {
                key,
                dtype,
                rank: s.1,
                shape: s.0,
                lanes: count(v.data.len())?,
                storage: Storage::HistoryView { state, layer, first: position + 1 - h as u32, row_lanes: count(n)? },
            });
        } else if v.commit {
            // The original step callback writes committed tiles once. Keep an
            // indexed view instead of cloning the same whole native tensor.
            if b.records.len() as u64 >= b.session.limits.max_records {
                return Err("wide commit-view metadata cap".into());
            }
            let shape = Shape::new(&shape)?;
            let occurrence = b.occ as u16;
            b.records.push(Record {
                key,
                dtype,
                rank: shape.1,
                shape: shape.0,
                lanes: count(v.data.len())?,
                storage: Storage::StepView { position, occurrence, node: v.node },
            });
        } else {
            b.push(key, dtype, &shape, v.data, 0, v.data.len())?;
        }
        b.node += 1;
        Ok(())
    }
    fn state(
        &mut self,
        position: u32,
        state: u16,
        layer: Option<u16>,
        history: Option<u32>,
        dtype: DType,
        shape: &[usize],
        data: Slice<'_>,
    ) -> Result<(), String> {
        if history.is_some() {
            return Ok(());
        }
        let mut b = self.0.borrow_mut();
        let inst = b.e.space.fixed_instances().iter().find(|i| i.state == state && i.layer == layer).ok_or("wide unknown Fixed")?;
        if inst.dtype != dtype
            || shape != b.e.space.program().states[state as usize].shape.iter().map(|n| *n as usize).collect::<Vec<_>>()
            || data.len() as u64 != inst.elements
        {
            return Err("wide Fixed metadata".into());
        }
        b.push(Key::Fixed(position, state, layer), dtype, shape, data, 0, data.len())
    }
    fn last_output(&mut self, position: u32, _dtype: DType, _shape: &[usize], _data: Slice<'_>, _selected: u32) -> Result<(), String> {
        let mut b = self.0.borrow_mut();
        if position + 1 != b.e.job.declared_prefill_tokens + b.outputs {
            return Err("wide output order".into());
        }
        b.outputs += 1;
        Ok(())
    }
}
/// Own one empty tape and one actual reservation before callbacks allocate or
/// write. The caller must include original model/plan/input/space owners in the
/// external envelope; this is an operational lease, never consensus authority.
#[allow(clippy::too_many_arguments)]
pub fn capture_one_pass_v2<'a>(
    expected: CaptureExpectedV2<'a>,
    runner: &TirClassRunnerWideQ24V2<'_>,
    run_limits: TirWideRunLimitsV2,
    work: &WidePanelWorkV2,
    file: File,
    reserver: &TirMemoryReserverV1,
) -> Result<WidePanelCaptureV2<'a>, Error> {
    let limits = work.inner.limits;
    let session = work.inner.clone();
    // Cheap format/shape/length bounds precede EVERY context/program/prompt hash.
    // Work construction accepts fixed Hash64 IDs only and performs no job hash.
    if expected.job.version != kaspa_consensus_core::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2
        || expected.job.network_id.is_empty()
        || expected.job.network_id.len() > kaspa_consensus_core::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES
        || expected.job.declared_prefill_tokens == 0
        || expected.job.exact_decode_tokens == 0
        || expected
            .job
            .declared_prefill_tokens
            .checked_add(expected.job.exact_decode_tokens)
            .is_none_or(|n| n > expected.job.max_context_tokens)
        || expected.prompt.len() as u64 != u64::from(expected.job.declared_prefill_tokens)
        || expected.class.program.len() > misaka_palw_tir::program::MAX_PROGRAM_BYTES
        || expected.class.layout.commit_tiles.len()
            > misaka_palw_tir::program::MAX_BLOCKS * misaka_palw_tir::program::MAX_NODES_PER_BLOCK
        || expected.class.layout.state_tiles.len() > misaka_palw_tir::program::MAX_STATES
    {
        return Err("wide foreign metadata shape/length cap".into());
    }
    if work.started.replace(true) {
        return Err("wide attempt CPU run already started; spent work is retained".into());
    }
    // Only immutable lengths/checked arithmetic and the runner's pure quote
    // occur before this mandatory capture lease. The ledger key is the owner's
    // pre-existing fixed expectation, not a freshly hashed delivered DTO.
    // The opaque actual space was independently decoded under the format cap;
    // bound metadata quote/equality work even when the foreign expected bytes
    // are tiny. No numerical execution or program Vec allocation occurs here.
    session.charge(Charge::Visit, mul(add(misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64, 1024)?, 128)?)?;
    let run_quote = runner.quote(expected.job, run_limits)?;
    let handoff_peak = run_quote.handoff_peak_requests()?;
    if handoff_peak > run_limits.max_requested_bytes {
        return Err("wide required output handoff ceiling".into());
    }
    let records_cap = usize::try_from(limits.max_records).map_err(|_| "wide records index")?;
    let leaves_cap = usize::try_from(limits.max_step_leaves).map_err(|_| "wide leaves index")?;
    let layout_bytes = add(32, mul(count(expected.class.layout.commit_tiles.len() + expected.class.layout.state_tiles.len())?, 4)?)?;
    let context_bytes = add(1024, count(expected.job.network_id.len())?)?;
    // Metadata request envelope: original program re-encoding, layout Borsh
    // hashing and context Borsh hashing, allowing Vec growth/coexisting writers.
    // Include FOREIGN layout length explicitly, even before layout equality.
    let metadata_scratch = mul(add(add(count(expected.class.program.len())?, layout_bytes)?, context_bytes)?, 4)?;
    // A callback first decodes one mixed-width leaf to i128 (<=4x its bytes),
    // DROPS that Vec, then hashes/writes the borrowed preimage. Runner-owned
    // native tile/preimage/hash buffers are separately in run_quote.
    let callback_scratch = mul(limits.max_material_bytes, 4)?;
    // Fixed local rank/scalar/hash/4096-byte IO/control request envelope. This
    // is not process RSS; original stack/allocator/OS owners remain external.
    let control_scratch = 65536u64;
    let heap = [
        limits.external_retained_bytes,
        mul(limits.max_records, size_of::<Record>() as u64)?,
        mul(limits.max_step_leaves, size_of::<Leaf>() as u64)?,
        metadata_scratch,
        callback_scratch,
        control_scratch,
    ]
    .into_iter()
    .try_fold(0, add)?;
    if add(heap, handoff_peak)? > limits.max_live_heap_bytes {
        return Err("wide collector simultaneous heap cap".into());
    }
    let lease = reserver("positive-wide-panel-capture-v2", work.class, work.execution, heap)?;
    // Hash work accounting is additive to the RAM guards, never a substitute.
    session.charge(Charge::Hash, context_bytes)?;
    let execution = expected.job.context_hash();
    if work.class != expected.job.shape_profile_id || work.execution != execution {
        return Err("wide work belongs to another original job/class".into());
    }
    session.charge(
        Charge::Hash,
        add(mul(add(count(expected.class.program.len())?, layout_bytes)?, 8)?, mul(count(expected.prompt.len())?, 4)?)?,
    )?;
    let class_id = expected.class.class_id(&expected.artifact);
    if expected.class.program.len() > misaka_palw_tir::program::MAX_PROGRAM_BYTES
        || class_id != expected.job.shape_profile_id
        || expected.class.layout != *expected.space.layout()
        || !program_matches(expected.space.program(), &expected.class.program)
        || runner.original_space().program() != expected.space.program()
        || runner.original_space().layout() != expected.space.layout()
        || !prompt_token_ids_match_v1(PalwPromptIdsFormV1::Flat, expected.prompt, &expected.job.prompt_token_ids_hash)
    {
        return Err("wide collector independent original metadata".into());
    }
    let shared = Rc::new(RefCell::new(Build {
        class_id,
        context: execution,
        e: expected,
        tape: Tape::empty(file)?,
        session,
        records: vec_exact(records_cap)?,
        leaves: vec_exact(leaves_cap)?,
        position: 0,
        occ: 0,
        node: 0,
        outputs: 0,
    }));
    let mut observer = Observer(shared.clone());
    let target = shared.clone();
    let mut leaves = move |out: &TirLeafOutWideQ24V2<'_>| target.borrow_mut().leaf(out);

    let (job, prompt, execution) = {
        let b = shared.borrow();
        (b.e.job, b.e.prompt, b.context)
    };
    let run = runner.run_observed_with_retained_output_v2(job, prompt, execution, run_limits, &mut leaves, &mut observer, reserver)?;
    drop(leaves);
    drop(observer);
    let mut b = Rc::try_unwrap(shared).map_err(|_| "wide collector leaked callback")?.into_inner();
    b.normalize_cursor()?;
    let shape = b.e.space.job_shape(b.e.job).map_err(|e| e.to_string())?;
    if b.position != shape.positions
        || b.outputs != b.e.job.exact_decode_tokens
        || b.leaves.len() as u64 != run.leaf_count()
        || run.leaf_hashes().len() != b.leaves.len()
    {
        return Err("wide collector incomplete execution".into());
    }
    b.session.charge(Charge::Visit, mul(count(b.records.len())?, 64)?)?;
    b.records.sort_unstable_by_key(|r| r.key);
    if b.records.windows(2).any(|w| w[0].key == w[1].key) {
        return Err("wide duplicate record".into());
    }
    let fixed = count(b.e.space.fixed_instances().len())?;
    b.session.charge(Charge::Visit, mul(u64::from(shape.positions), mul(fixed, 64)?)?)?;
    for p in 0..shape.positions {
        for inst in b.e.space.fixed_instances() {
            if b.records.binary_search_by_key(&Key::Fixed(p, inst.state, inst.layer), |r| r.key).is_err() {
                return Err("wide missing Fixed observation".into());
            }
        }
    }
    if palw_tir_execution_root_v1(&execution, &run.trace_root(), &b.class_id, run.leaf_count(), &run.step_merkle_root())
        != run.execution_root()
    {
        return Err("wide collector run root join".into());
    }
    Ok(WidePanelCaptureV2 {
        expected: b.e,
        context: b.context,
        run,
        records: b.records,
        leaves: b.leaves,
        tape: b.tape,
        session: b.session,
        builder_live: Cell::new(false),
        retained_heap: heap,
        _lease: lease,
    })
}
