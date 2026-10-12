//! Local mapped replay followed by bounded ordinary9 public acquisition.
//!
//! The caller obtains a complete op-211 snapshot and separately authenticates
//! its anchor. This consumer receives only that checked view and independently
//! acquired model bytes. No producer coordinates, private tensors or prepared
//! prosecution are accepted. It does not grant execution/Final rights or prove
//! completeness for TopK, derived history, encoder or decode special courts.
use crate::tir_stream::{ContainerRanges, palw_tir_model_inventory_root_streamed_v2};
use kaspa_consensus_core::{
    Hash64,
    palw_flat_model_vector_v9::FlatModelVectorViewV9,
    palw_model_artifact_v2::ModelArtifactBindingHeaderV2,
    palw_tir_artifact_v1::{PALW_TIR_ROW_PIECE_BYTES_V1, PalwTirModelInventoryV2, palw_tir_row_bytes_v1, palw_tir_tensor_bytes_v1},
};
use misaka_palw_kernel::{
    descriptor::k2_tir_v4_descriptor,
    flat_court5::flat_profile_commitment_v5,
    flat_detect5::{FlatPublicReadsV5, FlatReadV5},
    flat_detect9::{FlatFindingV9, FlatHeaderReadV9, FlatPublicReadsV9, FlatValueReadV9, locate_ordinary_relation_v9},
    flat_replay9::flat_replay_envelope_v9,
    flat_resources10::{ResourceQuantitiesV10, derive_flat_resources_v10},
    hash::{Digest, object_id},
    merkle3_stream::tensor_commitment_streamed_v3,
    merkle5::FlatLeafHashV5,
    seg::{NodeOpeningV1, SegmentedCommitmentsV1},
    trace::{SourceV1, WiringV1},
};
use misaka_palw_tir::{DType, Prim, Tensor, TirProgramV1};
use misaka_palw_tir_exec::{NodeValue, StepSink, TirExecutor, node::TirArtifactV1};
use sha2::{Digest as _, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io::Read,
    path::Path,
};

#[derive(Clone, Debug)]
pub struct MappedFlatReplayLimitsV9 {
    /// Independently derived reference10 quantities must fit all 21 axes before
    /// any model file is opened. Mapped storage does not weaken those ceilings.
    pub reference: ResourceQuantitiesV10,
    pub retained_tensor_bytes: u128,
    pub raw_artifact_bytes: u64,
    pub parameter_workspace_bytes: u64,
    pub selected_parameter_bytes: usize,
    pub selected_parameter_total_bytes: u128,
    pub attempts: u32,
    pub public_reads: u64,
    pub public_reply_bytes: u64,
    pub scope_checker_work: u128,
    pub parameter_hash_bytes: u128,
    pub trace_hash_bytes: u128,
    pub artifact_hash_bytes: u128,
    /// Separate complete model-inventory R pass, in exact descriptor/program
    /// order. This local tariff never substitutes for native PWU or rewards.
    pub inventory: MappedInventoryLimitsV9,
}
#[derive(Clone, Debug)]
pub struct MappedInventoryLimitsV9 {
    pub leaves: u32,
    pub raw_bytes: u128,
    pub hash_bytes: u128,
    pub work: u128,
    pub workspace_bytes: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MappedInventoryQuoteV9 {
    pub leaves: u32,
    pub raw_bytes: u128,
    /// Includes exact leaf metadata and all interior-node message bytes.
    pub hash_bytes: u128,
    /// Local units: 4096 per conservative Blake2b block, one per raw copied
    /// byte, 256 per range request and 256 per parameter instance quotation.
    pub work: u128,
    /// One 32KiB leaf plus an exact-request read window, at most 64 frontier
    /// slots and geometry-derived inventory bookkeeping; no model byte vector.
    pub workspace_bytes: u64,
}
/// Derive all R-pass quotas from canonical typed geometry before opening the
/// model. No row enumeration, weight allocation or declared root is trusted.
pub fn mapped_model_inventory_quote_v9(p: &TirProgramV1) -> Result<MappedInventoryQuoteV9, String> {
    let descriptor = k2_tir_v4_descriptor();
    let inventory = PalwTirModelInventoryV2::new(&descriptor, p)?;
    let leaves = inventory.leaf_count();
    if leaves == 0 {
        return Err("model inventory must contain a model parameter".into());
    }
    let mut raw_bytes = 0u128;
    let mut leaf_hash_bytes = 0u128;
    let mut blocks = 0u128;
    let mut instances = 0u128;
    let mut quoted_leaves = 0u128;
    for (param, layers) in inventory.instances().iter().enumerate() {
        let d = &p.params[param];
        // Reuse the frozen axis-0 grammar, including whole rank-0/1 tensors.
        let raw = palw_tir_tensor_bytes_v1(p, param as u16) as u128;
        let row = palw_tir_row_bytes_v1(p, param as u16) as u128;
        let rows = raw / row;
        let full = row / PALW_TIR_ROW_PIECE_BYTES_V1 as u128;
        let rem = row % PALW_TIR_ROW_PIECE_BYTES_V1 as u128;
        let metadata = 16u128 + d.name.len() as u128;
        let per_row_blocks = full * (2 + (metadata + PALW_TIR_ROW_PIECE_BYTES_V1 as u128).div_ceil(128))
            + if rem > 0 { 2 + (metadata + rem).div_ceil(128) } else { 0 };
        let per_instance_leaves = rows * row.div_ceil(PALW_TIR_ROW_PIECE_BYTES_V1 as u128);
        let count = layers.len() as u128;
        raw_bytes = raw_bytes.checked_add(raw * count).ok_or("R raw bytes overflow")?;
        leaf_hash_bytes = leaf_hash_bytes.checked_add((raw + per_instance_leaves * metadata) * count).ok_or("R leaf hash overflow")?;
        blocks = blocks.checked_add(per_row_blocks * rows * count).ok_or("R hash blocks overflow")?;
        instances += count;
        quoted_leaves += per_instance_leaves * count;
    }
    if quoted_leaves != leaves as u128 {
        return Err("R inventory geometry differs from frozen leaf count".into());
    }
    let interiors = leaves as u128 - 1;
    let hash_bytes = leaf_hash_bytes.checked_add(interiors * 128).ok_or("R tree hash overflow")?;
    // Key block + message block + one conservative finalization block.
    blocks = blocks.checked_add(interiors * 3).ok_or("R tree blocks overflow")?;
    let work = blocks
        .checked_mul(4096)
        .and_then(|n| n.checked_add(raw_bytes))
        .and_then(|n| n.checked_add(leaves as u128 * 256))
        .and_then(|n| n.checked_add(instances * 256))
        .ok_or("R work overflow")?;
    // The visitor derives per-parameter reader sets and instance vectors. Cover
    // each possible block reader with a full tree-node allocation and vector
    // growth as well as all actual instance entries, rather than only buffers.
    let metadata_bytes = p.params.len() as u128 * (128 + p.blocks.len() as u128 * 256) + instances * 32;
    let workspace_bytes = (PALW_TIR_ROW_PIECE_BYTES_V1 as u128 * 2 + 64 * 80 + 1024)
        .checked_add(metadata_bytes)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or("R inventory workspace overflow")?;
    Ok(MappedInventoryQuoteV9 { leaves, raw_bytes, hash_bytes, work, workspace_bytes })
}
#[derive(Clone, Debug, Default)]
pub struct MappedFlatReplayUsageV9 {
    pub reference: ResourceQuantitiesV10,
    pub authenticated_parameter_instances: usize,
    pub raw_parameter_bytes_read: u128,
    pub retained_tensor_bytes: u128,
    pub independently_executed_nodes: usize,
    pub attempts: u32,
    pub public_reads: u64,
    /// Worst allowed wire bytes are reserved before every callback, including
    /// missing, malformed and borrowed replies. This is not actual delivery.
    pub reserved_reply_bytes: u64,
    pub scope_checker_work: u128,
    pub parameter_hash_bytes: u128,
    pub trace_hash_bytes: u128,
    pub artifact_hash_bytes: u128,
    pub selected_parameter_bytes: u128,
    pub model_inventory: MappedInventoryQuoteV9,
    pub authenticated_model_inventory_root: Option<Hash64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // a value type kept by value on purpose; boxing changes every match site
pub enum MappedFlatReplayResultV9 {
    Finding(FlatFindingV9),
    Limit(&'static str),
    Inconsistent(String),
    /// Compose with the existing specialized detector and its independently
    /// paid scope/replay contract. Ordinary-only replay cannot declare Clean.
    NeedsSpecial {
        unit: FlatReadV5,
        reason: MappedSpecialCourtV9,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MappedSpecialCourtV9 {
    TopK,
    DerivedHistory,
    Decode { index: u32 },
    StreamedParameter,
}
pub struct MappedFlatReplayV9 {
    artifact: TirArtifactV1,
    subject: Digest,
    values: Vec<Vec<Vec<Tensor>>>,
    roots: SegmentedCommitmentsV1,
    tokens: Vec<u32>,
    decode_mismatch: Option<u32>,
    authenticated_prefix: Cell<u64>,
    selected_preparation_limit_hit: Cell<bool>,
    limits: MappedFlatReplayLimitsV9,
    usage: RefCell<MappedFlatReplayUsageV9>,
    pub artifact_sha256: String,
}
fn subject(v: &FlatModelVectorViewV9) -> Digest {
    object_id(
        b"misaka-palw/sdk/mapped-replay-subject/v9",
        &(
            v.profile,
            v.scope.root(),
            v.replay.root(),
            v.trace.program.encode(),
            v.trace.params.root(),
            v.trace.positions,
            v.trace.segment_roots.clone(),
            v.trace.job.class_binding_id,
            v.trace.job.prompt_len,
            v.trace.job.prompt_root,
            v.trace.job.inline_prompt.clone(),
            v.trace.job.max_new_tokens,
            v.trace.job.decode,
            v.trace.job.complete,
            v.trace.generated.clone(),
            v.trace.encoder.map(|e| (e.first_input, e.l)),
        ),
    )
}
fn hash_file(path: &Path, cap: u64) -> Result<String, String> {
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > cap {
        return Err("mapped artifact exceeds local byte cap".into());
    }
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut b = vec![0; 1 << 20];
    let mut seen = 0u64;
    loop {
        let n = file.read(&mut b).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        seen = seen.checked_add(n as u64).filter(|n| *n <= cap).ok_or("artifact grew beyond cap")?;
        h.update(&b[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
fn blank(p: &TirProgramV1) -> Vec<Vec<Option<Tensor>>> {
    p.occurrences().iter().map(|(b, _)| (0..p.blocks[*b as usize].nodes.len()).map(|_| None).collect()).collect()
}
struct Capture<'a> {
    map: &'a BTreeMap<(u8, Option<u16>), u16>,
    values: Vec<Vec<Option<Tensor>>>,
    prior_bytes: u128,
    cap: u128,
    bytes: u128,
    seen: usize,
    error: Option<String>,
}
impl StepSink for Capture<'_> {
    fn every_node(&self) -> bool {
        true
    }
    fn node(&mut self, v: &NodeValue<'_>) {
        if self.error.is_some() {
            return;
        }
        let Some(&s) = self.map.get(&(v.block, v.layer)) else {
            self.error = Some("unmapped occurrence".into());
            return;
        };
        let Some(bytes) = (v.data.len() as u128).checked_mul(16).and_then(|n| n.checked_add(self.bytes)) else {
            self.error = Some("retained tensor byte overflow".into());
            return;
        };
        if self.prior_bytes.checked_add(bytes).is_none_or(|n| n > self.cap) {
            self.error = Some("retained tensor byte cap".into());
            return;
        }
        let Some(slot) = self.values.get_mut(s as usize).and_then(|s| s.get_mut(v.node as usize)) else {
            self.error = Some("own executor emitted a foreign node".into());
            return;
        };
        if slot.is_some() {
            self.error = Some("duplicate own node".into());
            return;
        }
        self.bytes = bytes;
        self.seen += 1;
        *slot = Some(v.to_tensor());
    }
}
impl MappedFlatReplayV9 {
    /// Both `binding` and `binding_id` must come from the same separately
    /// anchored snapshot whose candidate supplied `v`. This verifies actual
    /// local R and PC bytes; it does not establish publisher/source fidelity or
    /// authorize an execution class, Final or conformance rewards.
    pub fn prepare(
        v: &FlatModelVectorViewV9,
        binding: &ModelArtifactBindingHeaderV2,
        binding_id: Hash64,
        artifact_path: &Path,
        limits: MappedFlatReplayLimitsV9,
    ) -> Result<Self, String> {
        if v.trace.encoder.is_some() || v.trace.positions == 0 || !v.trace.job.complete || limits.attempts == 0 {
            return Err("mapped ordinary replay requires an open complete decoder job and attempts".into());
        }
        let prompt = v.trace.job.inline_prompt.as_ref().ok_or("mapped replay requires authenticated inline prompt")?;
        if prompt.len() != v.trace.job.prompt_len as usize
            || v.trace.generated.is_empty()
            || !misaka_palw_kernel::job::generation_length_matches_v1(v.trace.job.max_new_tokens, v.trace.generated.len())
            || misaka_palw_kernel::seg::prompt_root_of_ids_v1(prompt) != v.trace.job.prompt_root
            || prompt.iter().chain(&v.trace.generated).any(|t| *t >= v.trace.program.token_bound)
            || prompt.len().checked_add(v.trace.generated.len() - 1) != Some(v.trace.positions as usize)
        {
            return Err("exact public fixed-count decoder job required".into());
        }
        let bytes = v.trace.program.encode();
        if binding.id() != binding_id
            || binding.refuted
            || binding.descriptor != k2_tir_v4_descriptor().digest()
            || binding.program_root != misaka_palw_kernel::public::program_root_v1(&bytes)
            || binding.program_bytes_len as usize != bytes.len()
            || binding.kernel_param_root != Hash64::from_bytes(v.trace.params.root())
            || binding.pc_instances as usize != v.trace.params.by_instance.len()
        {
            return Err("public model binding identity/descriptor/program/PC differs".into());
        }
        let replay = flat_replay_envelope_v9(&v.trace.program, &v.scope, None).map_err(|e| format!("{e:?}"))?;
        if replay != v.replay || replay.scope_checker_work_per_attempt > limits.scope_checker_work {
            return Err("independent replay declaration/checker quota differs".into());
        }
        let resources = derive_flat_resources_v10(&v.trace.program, &v.trace.params, v.trace.positions, None)?;
        resources.vector.quantities.within(&limits.reference).map_err(|a| format!("local reference10 axis {a:?}"))?;
        let retained =
            replay.reference.position_tensor_bytes.checked_mul(v.trace.positions as u128).ok_or("retained preflight overflow")?;
        if retained > limits.retained_tensor_bytes {
            return Err("retained position tensor preflight cap".into());
        }
        let inventory_quote = mapped_model_inventory_quote_v9(&v.trace.program)?;
        if inventory_quote.leaves > limits.inventory.leaves
            || inventory_quote.raw_bytes > limits.inventory.raw_bytes
            || inventory_quote.hash_bytes > limits.inventory.hash_bytes
            || inventory_quote.work > limits.inventory.work
            || inventory_quote.workspace_bytes > limits.inventory.workspace_bytes
        {
            return Err("complete model inventory R preflight quota".into());
        }
        if limits.parameter_workspace_bytes == 0 || limits.raw_artifact_bytes == 0 || limits.selected_parameter_bytes == 0 {
            return Err("positive source budgets required".into());
        }
        let file_len = std::fs::metadata(artifact_path).map_err(|e| e.to_string())?.len();
        if file_len > limits.raw_artifact_bytes {
            return Err("raw mapped artifact preflight cap".into());
        }
        // Reserve two complete capped passes. A changing file can consume the
        // cap even when its original metadata declared fewer bytes.
        let artifact_hash_bytes = (limits.raw_artifact_bytes as u128)
            .checked_mul(2)
            .filter(|n| *n <= limits.artifact_hash_bytes)
            .ok_or("source file hash quota")?;
        let parameter_hash_bytes =
            resources.vector.quantities.get(misaka_palw_kernel::flat_resources10::ResourceAxisV10::ParameterHashBytes);
        let trace_hash_bytes = resources.vector.quantities.get(misaka_palw_kernel::flat_resources10::ResourceAxisV10::TraceHashBytes);
        if parameter_hash_bytes > limits.parameter_hash_bytes || trace_hash_bytes > limits.trace_hash_bytes {
            return Err("source/reference hash preflight quota".into());
        }
        let before = hash_file(artifact_path, limits.raw_artifact_bytes)?;
        let artifact = TirArtifactV1::open(artifact_path)?;
        let p = &artifact.plan().program;
        if p != &v.trace.program || artifact.plan().param_instances.len() != v.trace.params.by_instance.len() {
            return Err("independent mapped source is not the authenticated canonical program/instance set".into());
        }
        let mut usage = MappedFlatReplayUsageV9 {
            reference: resources.vector.quantities,
            scope_checker_work: replay.scope_checker_work_per_attempt,
            parameter_hash_bytes,
            trace_hash_bytes,
            artifact_hash_bytes,
            model_inventory: inventory_quote.clone(),
            ..Default::default()
        };
        let descriptor = k2_tir_v4_descriptor();
        let inventory = PalwTirModelInventoryV2::new(&descriptor, p)?;
        // A one-byte configured read-ahead makes every window exactly the
        // requested leaf size. Thus physical reads cannot silently expand a
        // 1536-byte row into a 4MiB window or allocate all 1.8GB model bytes.
        let src = ContainerRanges::open_with_window(artifact.container(), 1)?;
        let (root, count) = palw_tir_model_inventory_root_streamed_v2(&inventory, &src)?;
        if root != binding.model_inventory_root || count != inventory_quote.leaves {
            return Err("independent complete model inventory R root differs".into());
        }
        usage.authenticated_model_inventory_root = Some(root);
        drop(src);
        for &(j, layer) in &artifact.plan().param_instances {
            let d = &p.params[j as usize];
            let source = artifact.params().get(j, layer).ok_or("mapped parameter missing")?;
            let width = d.dtype.width();
            let shape = d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
            let (root, _) = tensor_commitment_streamed_v3(d.dtype, &shape, limits.parameter_workspace_bytes, &mut |offset, out| {
                if !(offset as usize).is_multiple_of(width) || out.len() % width != 0 {
                    return Err("raw parameter alignment".into());
                }
                let start = usize::try_from(offset).map_err(|e| e.to_string())? / width;
                let count = out.len() / width;
                misaka_palw_tir_exec::with_slice!(source, x => {
                    for (value, raw) in x.get(start..start+count).ok_or("raw parameter range")?.iter().zip(out.chunks_exact_mut(width)) {
                        raw.copy_from_slice(&value.to_le_bytes());
                    }
                });
                usage.raw_parameter_bytes_read =
                    usage.raw_parameter_bytes_read.checked_add(out.len() as u128).ok_or("raw byte overflow")?;
                Ok(())
            })?;
            if v.trace.params.by_instance.get(&(j, layer)) != Some(&root) {
                return Err(format!("mapped PC instance {j}/{layer:?} differs"));
            }
            usage.authenticated_parameter_instances += 1;
        }
        let mut map = BTreeMap::new();
        for (s, key) in p.occurrences().iter().enumerate() {
            if map.insert(*key, s as u16).is_some() {
                return Err("mapped replay requires unique block/layer occurrences".into());
            }
        }
        let mut values = Vec::new();
        let mut decode_mismatch = None;
        let tokens = prompt.iter().chain(&v.trace.generated[..v.trace.generated.len() - 1]).copied().collect::<Vec<_>>();
        let mut executor = TirExecutor::new(artifact.plan(), artifact.params()).map_err(|e| e.to_string())?;
        for (q, token) in tokens.iter().enumerate() {
            let mut c = Capture {
                map: &map,
                values: blank(p),
                prior_bytes: usage.retained_tensor_bytes,
                cap: limits.retained_tensor_bytes,
                bytes: 0,
                seen: 0,
                error: None,
            };
            executor.step(*token, &mut c).map_err(|e| e.to_string())?;
            if let Some(e) = c.error {
                return Err(e);
            }
            let pos = c
                .values
                .into_iter()
                .map(|s| s.into_iter().map(|t| t.ok_or("missing own node")).collect::<Result<Vec<_>, _>>())
                .collect::<Result<Vec<_>, _>>()?;
            if q + 1 >= prompt.len() {
                let output = &pos[pos.len() - 1][p.logits as usize];
                if v.trace.job.decode.select(output) != Some(v.trace.generated[q + 1 - prompt.len()]) {
                    decode_mismatch.get_or_insert((q + 1 - prompt.len()) as u32);
                }
            }
            usage.retained_tensor_bytes += c.bytes;
            usage.independently_executed_nodes += c.seen;
            values.push(pos);
        }
        let roots = SegmentedCommitmentsV1::new(
            values
                .iter()
                .map(|p| {
                    p.iter()
                        .map(|s| s.iter().map(flat_profile_commitment_v5).collect::<Result<Vec<_>, _>>())
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?,
        );
        drop(executor);
        if hash_file(artifact_path, limits.raw_artifact_bytes)? != before {
            return Err("mapped source changed during independent replay".into());
        }
        Ok(Self {
            subject: subject(v),
            artifact,
            values,
            roots,
            tokens,
            decode_mismatch,
            authenticated_prefix: Cell::new(0),
            selected_preparation_limit_hit: Cell::new(false),
            limits,
            usage: RefCell::new(usage),
            artifact_sha256: before,
        })
    }
    pub fn usage(&self) -> MappedFlatReplayUsageV9 {
        self.usage.borrow().clone()
    }
    pub fn authenticated_matching_prefix_nodes(&self) -> u64 {
        self.authenticated_prefix.get()
    }
    pub fn check(&mut self, v: &FlatModelVectorViewV9, public: &dyn FlatPublicReadsV9) -> MappedFlatReplayResultV9 {
        if self.subject != subject(v) {
            return MappedFlatReplayResultV9::Inconsistent("fresh snapshot changed the cached job/model/scope".into());
        }
        if self.selected_preparation_limit_hit.get() {
            return MappedFlatReplayResultV9::Limit("selected parameter preparation bytes");
        }
        let mut usage = self.usage.borrow_mut();
        if usage.attempts >= self.limits.attempts {
            return MappedFlatReplayResultV9::Limit("attempts");
        }
        let Some(work) = usage
            .scope_checker_work
            .checked_add(v.replay.scope_checker_work_per_attempt)
            .filter(|n| *n <= self.limits.scope_checker_work)
        else {
            return MappedFlatReplayResultV9::Limit("scope checker work");
        };
        let Some(param_hash) = usage
            .parameter_hash_bytes
            .checked_add(v.replay.reference.parameter_hash_bytes_per_attempt)
            .filter(|n| *n <= self.limits.parameter_hash_bytes)
        else {
            return MappedFlatReplayResultV9::Limit("parameter hash bytes");
        };
        let Some(trace_hash) = usage
            .trace_hash_bytes
            .checked_add(v.replay.reference.trace_hash_bytes_per_attempt)
            .filter(|n| *n <= self.limits.trace_hash_bytes)
        else {
            return MappedFlatReplayResultV9::Limit("trace hash bytes");
        };
        usage.attempts += 1;
        usage.scope_checker_work = work;
        usage.parameter_hash_bytes = param_hash;
        usage.trace_hash_bytes = trace_hash;
        drop(usage);
        // The complete owned run matched every trace root and every fixed-count
        // delivered id. No producer value/header callback is needed for Clean.
        if self.roots.segment_roots() == v.trace.segment_roots {
            if let Some(index) = self.decode_mismatch {
                return MappedFlatReplayResultV9::NeedsSpecial {
                    unit: FlatReadV5 {
                        position: v.trace.job.prompt_len - 1 + index,
                        occurrence: (v.trace.program.occurrences().len() - 1) as u16,
                        node: v.trace.program.logits,
                        leaf: None,
                    },
                    reason: MappedSpecialCourtV9::Decode { index },
                };
            }
            return MappedFlatReplayResultV9::Finding(FlatFindingV9::Clean);
        }
        let hit = Cell::new(None);
        let bounded = Bounded { replay: self, public, hit: &hit };
        let p = &v.trace.program;
        let w = match WiringV1::new(p) {
            Ok(w) => w,
            Err(e) => return MappedFlatReplayResultV9::Inconsistent(e.to_string()),
        };
        let count = p.occurrences().iter().map(|(b, _)| p.blocks[*b as usize].nodes.len() as u64).sum::<u64>();
        for (q, position) in self.values.iter().enumerate() {
            let mut index = 0u64;
            for (s, values) in position.iter().enumerate() {
                for (n, expected) in values.iter().enumerate() {
                    let flat_index = q as u64 * count + index;
                    if flat_index < self.authenticated_prefix.get() {
                        index += 1;
                        continue;
                    }
                    let unit = FlatReadV5 { position: q as u32, occurrence: s as u16, node: n as u16, leaf: None };
                    let Some(node) = bounded.node(unit).filter(|o| {
                        (o.position, o.occurrence, o.node) == (unit.position, unit.occurrence, unit.node)
                            && o.authenticates(&v.trace.segment_roots, v.trace.positions, index, count)
                    }) else {
                        return match hit.get() {
                            Some(e) => MappedFlatReplayResultV9::Limit(e),
                            None => MappedFlatReplayResultV9::Finding(FlatFindingV9::Demand(unit)),
                        };
                    };
                    index += 1;
                    if node.commitment == self.roots.commitments[q][s][n] {
                        self.authenticated_prefix.set(flat_index + 1);
                        continue;
                    }
                    if matches!(w.node(s as u16, n as u16).prim, Prim::TopK { .. } | Prim::HistAppend { .. })
                        || w.is_derived(s as u16, n as u16)
                    {
                        return MappedFlatReplayResultV9::NeedsSpecial {
                            unit,
                            reason: if matches!(w.node(s as u16, n as u16).prim, Prim::TopK { .. }) {
                                MappedSpecialCourtV9::TopK
                            } else {
                                MappedSpecialCourtV9::DerivedHistory
                            },
                        };
                    }
                    for r in &w.node(s as u16, n as u16).inputs {
                        if let misaka_palw_tir::Ref::Param(j) = r {
                            let decl = &p.params[*j as usize];
                            let size = decl.shape.iter().try_fold(16usize, |a, b| a.checked_mul(*b as usize));
                            if size.is_none_or(|n| n > self.limits.selected_parameter_bytes) {
                                return MappedFlatReplayResultV9::NeedsSpecial {
                                    unit,
                                    reason: MappedSpecialCourtV9::StreamedParameter,
                                };
                            }
                        }
                    }
                    let own = || self.operands(&w, q as u32, s as u16, n as u16);
                    let found = locate_ordinary_relation_v9(&v.context(), &bounded, unit, expected, &own, &|| None);
                    return match hit.get() {
                        Some(e) => MappedFlatReplayResultV9::Limit(e),
                        None if self.selected_preparation_limit_hit.get() => {
                            MappedFlatReplayResultV9::Limit("selected parameter preparation bytes")
                        }
                        None => MappedFlatReplayResultV9::Finding(found),
                    };
                }
            }
        }
        MappedFlatReplayResultV9::Inconsistent("all authenticated node roots agree but aggregate trace differs".into())
    }
    fn operands(&self, w: &WiringV1<'_>, q: u32, s: u16, n: u16) -> Result<Vec<Tensor>, String> {
        // Ordinary inputs come from committed prior nodes, immutable parameters,
        // program constants or initial zeros. Token/position-only relations
        // have their inputs represented by SourceV1::Public below.
        let tokens = &self.tokens;
        (0..w.node(s, n).inputs.len())
            .map(|i| match w.input_source(tokens, q, s, n, i).map_err(|e| e.to_string())? {
                SourceV1::Node { position, occurrence, node } => self
                    .values
                    .get(position as usize)
                    .and_then(|p| p.get(occurrence as usize))
                    .and_then(|s| s.get(node as usize))
                    .cloned()
                    .ok_or_else(|| "missing independently replayed operand".into()),
                SourceV1::Param { index, layer } => self.reference_parameter(index, layer),
                SourceV1::Const(j) => {
                    let c = w.program.consts.get(j as usize).ok_or("missing program const")?;
                    Tensor::from_le_bytes(c.dtype, &c.shape.iter().map(|d| *d as usize).collect::<Vec<_>>(), &c.data)
                        .map_err(|e| e.to_string())
                }
                SourceV1::Zeros { dtype, shape } => Ok(Tensor::zeros(dtype, &shape)),
                SourceV1::Public(v) => Tensor::scalar(DType::Idx, v as i128).map_err(|e| e.to_string()),
                SourceV1::Input { .. } => Err("encoder/pipeline inputs require their explicit public input consumer".into()),
            })
            .collect()
    }
    /// Inputs for a specialized detector remain independently owned, job-bound
    /// reference material. This is not an authenticated producer opening.
    pub fn reference_node(&self, unit: FlatReadV5) -> Option<&Tensor> {
        self.values.get(unit.position as usize)?.get(unit.occurrence as usize)?.get(unit.node as usize)
    }
    pub fn reference_parameter(&self, index: u16, layer: Option<u16>) -> Result<Tensor, String> {
        let p = &self.artifact.plan().program;
        let d = p.params.get(index as usize).ok_or("unknown source parameter")?;
        let len = d.shape.iter().try_fold(1usize, |a, b| a.checked_mul(*b as usize)).ok_or("source extent overflow")?;
        if len.checked_mul(16).is_none_or(|b| b > self.limits.selected_parameter_bytes) {
            return Err("selected parameter payload cap".into());
        }
        let mut usage = self.usage.borrow_mut();
        let Some(next) =
            usage.selected_parameter_bytes.checked_add(len as u128 * 16).filter(|n| *n <= self.limits.selected_parameter_total_bytes)
        else {
            self.selected_preparation_limit_hit.set(true);
            return Err("selected parameter preparation total cap".into());
        };
        usage.selected_parameter_bytes = next;
        drop(usage);
        let source = self.artifact.params().get(index, layer).ok_or("source instance missing")?;
        Tensor::new(d.dtype, d.shape.iter().map(|n| *n as usize).collect(), source.to_i128s()).map_err(|e| e.to_string())
    }
}
struct Bounded<'a> {
    replay: &'a MappedFlatReplayV9,
    public: &'a dyn FlatPublicReadsV9,
    hit: &'a Cell<Option<&'static str>>,
}
impl Bounded<'_> {
    fn reserve(&self, bytes: u64) -> bool {
        let mut u = self.replay.usage.borrow_mut();
        if u.public_reads >= self.replay.limits.public_reads {
            self.hit.set(Some("public reads"));
            return false;
        }
        let Some(total) = u.reserved_reply_bytes.checked_add(bytes).filter(|n| *n <= self.replay.limits.public_reply_bytes) else {
            self.hit.set(Some("public reply bytes"));
            return false;
        };
        u.public_reads += 1;
        u.reserved_reply_bytes = total;
        true
    }
}
impl FlatPublicReadsV5 for Bounded<'_> {
    fn node(&self, u: FlatReadV5) -> Option<NodeOpeningV1> {
        self.reserve(8192).then(|| self.public.node(u)).flatten().filter(|x| x.byte_len() <= 8192)
    }
    fn hash(&self, u: FlatReadV5) -> Option<FlatLeafHashV5> {
        self.reserve(8192).then(|| self.public.hash(u)).flatten().filter(|x| x.shape.len() <= 4 && x.siblings.len() <= 64)
    }
}
impl FlatPublicReadsV9 for Bounded<'_> {
    fn header(&self, u: FlatHeaderReadV9) -> Option<Vec<u8>> {
        self.reserve(512).then(|| self.public.header(u)).flatten().filter(|b| b.len() <= 512)
    }
    fn value(&self, u: FlatValueReadV9) -> Option<Vec<u8>> {
        self.reserve(16384).then(|| self.public.value(u)).flatten().filter(|b| b.len() <= 16384)
    }
}
