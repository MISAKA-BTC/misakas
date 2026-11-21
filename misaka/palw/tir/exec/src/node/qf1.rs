//! RFC-0066 prediction-position replay for an explicitly pinned, local TIR instance.
//! A committed producer location identifies a value; accepting its intended Qwen
//! semantics requires the separate lowering/instance assessment.
pub mod cost_review;
mod trace;
use super::{TirArtifactV1, memory, positive_court::greedy_index};
use crate::{NoSink, NodeValue, Slice, StepSink, TirExecutor};
use kaspa_consensus_core::{
    Hash64,
    palw_qf1::{self, ROLES, Readout, Stop, VOCAB, WIDTH},
    palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_job_context_v1},
    palw_tir_step_v1::PalwTirStepSpaceV1,
};
use misaka_palw_tir::{Block, DType, Dim, Prim, Ref, TensorType};
use trace::Trace;
pub use trace::{LocalTrace, TraceOpening, TraceRequest};

/// The manifest digest binds source inventory and conversion recipe. Scale
/// references are in POST_BLOCK[0..36], FINAL_NORM order in that manifest.
pub struct LocalInstance<'a> {
    artifact: &'a TirArtifactV1,
    space: PalwTirStepSpaceV1,
    producers: [(u8, u16); ROLES],
    identity: Identity,
    argmax: (u8, u16),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Identity {
    pub descriptor: Hash64,
    pub inventory: Hash64,
    pub class: Hash64,
    pub tokenizer: Hash64,
}

pub struct LocalAnswer {
    pub tokens: Vec<u32>,
    pub stop: Stop,
    pub processed_positions: u32,
    pub z: [u8; 64],
}

/// Check dataflow in the validated SSA block, optionally cutting a producer.
fn depends(block: &Block, output: u16, source: Ref, cut: Option<u16>) -> bool {
    let mut reachable = Vec::with_capacity(usize::from(output) + 1);
    for (idx, node) in block.nodes.iter().enumerate().take(usize::from(output) + 1) {
        let found = Some(idx as u16) != cut
            && node.inputs.iter().any(|r| *r == source || matches!(r, Ref::Node(n) if reachable[usize::from(*n)]));
        reachable.push(found);
    }
    reachable[usize::from(output)]
}

impl<'a> LocalInstance<'a> {
    pub fn bind(artifact: &'a TirArtifactV1, manifest: Hash64, final_norm: u16, scales: &[Hash64; ROLES]) -> Result<Self, String> {
        Self::bind_inventory(artifact, manifest, final_norm, scales, false)
    }

    /// Evaluate the full portable PALWTIR1 represented by these tensor bytes.
    /// The full class commits to the ordinary inventory tree. Even when the
    /// bytes are locally held in sections, no composite root or opening is
    /// used for this numerical replay or for the signed QF1 descriptor.
    pub fn bind_full_weights(
        artifact: &'a TirArtifactV1,
        manifest: Hash64,
        final_norm: u16,
        scales: &[Hash64; ROLES],
    ) -> Result<Self, String> {
        Self::bind_inventory(artifact, manifest, final_norm, scales, true)
    }

    fn bind_inventory(
        artifact: &'a TirArtifactV1,
        manifest: Hash64,
        final_norm: u16,
        scales: &[Hash64; ROLES],
        full_weights: bool,
    ) -> Result<Self, String> {
        let class = artifact.class()?;
        let space = PalwTirStepSpaceV1::new(&class).map_err(|e| e.to_string())?;
        let p = &space.program;
        if p.token_bound != VOCAB || class.layout.max_context != palw_qf1::CONTEXT || p.schedule.layers.len() != ROLES - 1 {
            return Err("QF1 vocabulary, positions or depth differ".into());
        }
        if class.layout.commit_tiles.iter().any(|&lanes| lanes != 64) {
            return Err("QF1 producer replay requires the registered 64-lane commit layout".into());
        }
        let vector = TensorType::new(DType::I16, vec![Dim::Fixed(WIDTH as u32)]);
        let mut producers = [(0, 0); ROLES];
        for (role, &b) in p.schedule.layers.iter().enumerate() {
            let block = &p.blocks[usize::from(b)];
            let [out] = block.carry_out[..] else { return Err("QF1 requires a single residual carry".into()) };
            let node = &block.nodes[usize::from(out)];
            if !node.commit || node.out != vector || !depends(block, out, Ref::CarryIn(0), None) {
                return Err("QF1 layer residual is not the committed A16 carry".into());
            }
            producers[role] = (b, out);
        }
        let post = &p.blocks[usize::from(p.schedule.post)];
        let logits = &post.nodes[usize::from(p.logits)];
        if !post.nodes.get(usize::from(final_norm)).is_some_and(|n| n.commit && n.out == vector)
            || logits.out != TensorType::new(DType::I32, vec![Dim::Fixed(VOCAB)])
            || !depends(post, final_norm, Ref::CarryIn(0), None)
            || !depends(post, p.logits, Ref::Node(final_norm), None)
            || depends(post, p.logits, Ref::CarryIn(0), Some(final_norm))
        {
            return Err("QF1 final norm must carry the last residual into the committed i32 head".into());
        }
        let mut projections =
            post.nodes.iter().enumerate().filter(|(_, node)| {
                node.commit && node.prim == (Prim::TopK { axis: 0, k: 1 }) && node.inputs == [Ref::Node(p.logits)]
            });
        let (Some((argmax, _)), None) = (projections.next(), projections.next()) else {
            return Err("QF1 requires one committed TopK(1) on the full logits".into());
        };
        producers[ROLES - 1] = (p.schedule.post, final_norm);
        artifact.check_weights_unmodified()?;
        let (inventory, _) = if full_weights && artifact.composite_ref().is_some() {
            kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_root_v1(p, artifact).map_err(|e| e.to_string())?
        } else {
            artifact.inventory_root()?
        };
        artifact.check_weights_unmodified()?;
        let class_id = class.class_id(&inventory);
        let mut hash = blake2b_simd::Params::new().hash_length(64).to_state();
        hash.update(b"MISAKA/private/QF1/local-instance/v1\0").update(manifest.as_byte_slice()).update(class_id.as_byte_slice());
        // Class ID includes layout, program, weights and tokenizer. Role 0..35
        // implies the scheduled layer; 36 identifies the post occurrence.
        for (role, (&(block, node), scale)) in producers.iter().zip(scales).enumerate() {
            hash.update(&[role as u8, block]).update(&node.to_le_bytes()).update(scale.as_byte_slice());
        }
        let descriptor = Hash64::from_bytes(*hash.finalize().as_array());
        Ok(Self {
            artifact,
            argmax: (p.schedule.post, argmax as u16),
            space,
            producers,
            identity: Identity { descriptor, inventory, class: class_id, tokenizer: class.tokenizer_id },
        })
    }

    pub fn identity(&self) -> Identity {
        self.identity
    }

    /// Non-paying replay from an empty KV history. The context supplies the
    /// pre-registered leaf budget and this attempt's local memory attribution.
    pub fn replay(&self, prompt: &[u32], limit: u32, max_leaves: u64, job: Hash64, seed: [u8; 32]) -> Result<LocalAnswer, String> {
        self.drive(prompt, limit, max_leaves, job, seed, None, None)
    }

    pub fn replay_trace(
        &self,
        prompt: &[u32],
        limit: u32,
        max_leaves: u64,
        job: Hash64,
        seed: [u8; 32],
        request: TraceRequest,
    ) -> Result<LocalTrace, String> {
        let mut trace = Trace::begin(self, prompt, limit, max_leaves, seed, request)?;
        let answer = self.drive(prompt, limit, max_leaves, job, seed, Some(&mut trace), None)?;
        trace.finish(answer)
    }

    fn drive(
        &self,
        prompt: &[u32],
        limit: u32,
        max_leaves: u64,
        job: Hash64,
        seed: [u8; 32],
        mut trace: Option<&mut Trace>,
        verified_prefix: Option<&misaka_palw_tir::RunState>,
    ) -> Result<LocalAnswer, String> {
        self.artifact.check_weights_unmodified()?;
        let len = u32::try_from(prompt.len()).map_err(|_| "QF1 input length")?;
        palw_qf1::check_capacity(len, limit)?;
        if prompt.iter().any(|&id| id >= VOCAB) {
            return Err("QF1 input ID out of range".into());
        }
        let class = self.artifact.class()?;
        let facts = PalwTirJobFactsV1::of(&class, &self.space.program, self.identity.class);
        let mut ctx = palw_tir_job_context_v1(&facts, (len, limit));
        ctx.job_id = job;
        ctx.execution_seed = seed;
        ctx.prompt_token_ids_hash =
            kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_root_v1(prompt).map_err(|e| e.to_string())?;
        self.space.leaf_count_capped(&ctx, max_leaves).map_err(|e| e.to_string())?;
        let plan = self.artifact.plan();
        let need = memory::tir_executor_storage_need_v1(&self.space, plan, &ctx)?;
        // One position of i16 rows + chunk records (including Vec growth),
        // compression scratch, and the canonical token result.
        let row_bytes = (2 * ROLES * WIDTH) as u64;
        let chunks = (64 + (5 + row_bytes) * u64::from(limit)).div_ceil(4096);
        let extra = row_bytes + 32768 + 2 * chunks * size_of::<palw_qf1::readout_chunks::HashChunk>() as u64;
        let _lease = memory::reserve_v1(
            &memory::memory_reserver_v1(),
            "tir-qf1-local",
            self.identity.class,
            &ctx,
            need.checked_add(extra).ok_or("QF1 memory quote overflow")?,
        )?;
        let mut exec = TirExecutor::new(plan, self.artifact.params()).map_err(|e| e.to_string())?;
        // The typed executor only fuses regions preserving every committed output.
        // An observer that needs internal nodes still selects the unfused route.
        exec.set_fused(true);
        let (&last, prefix) = prompt.split_last().ok_or("QF1 empty input")?;
        // Only the offline paired replay supplies this state. Its prefix values
        // have already been entered into each original's independent trace.
        if let Some(state) = verified_prefix {
            exec.import_state(state).map_err(|e| e.to_string())?;
        }
        let mut token = last;
        for &id in prefix.get(exec.pos() as usize..).ok_or("QF1 checkpoint beyond prefill")? {
            trace::step(&mut exec, id, &mut NoSink, false, trace.as_deref_mut())?;
        }
        let mut sink = Prediction {
            argmax: self.argmax,
            selected: None,
            producers: &self.producers,
            next: 0,
            rows: std::array::from_fn(|_| Vec::with_capacity(WIDTH)),
        };
        let mut readout = Readout::default();
        let mut tokens = Vec::with_capacity(limit as usize);
        loop {
            sink.next = 0;
            sink.selected = None;
            trace::step(&mut exec, token, &mut sink, true, trace.as_deref_mut())?;
            if sink.next != ROLES {
                return Err("QF1 incomplete producer sequence".into());
            }
            readout.step(&std::array::from_fn(|role| sink.rows[role].as_slice()))?;
            let (shape, data) = exec.logits();
            if shape != [VOCAB as usize] {
                return Err("QF1 logits width".into());
            }
            token = match data {
                Slice::I8(v) => greedy_index(v),
                Slice::I16(v) => greedy_index(v),
                Slice::I32(v) => greedy_index(v),
                _ => return Err("QF1 logits storage".into()),
            }
            .ok_or("QF1 empty logits")? as u32;
            if sink.selected != Some(token) {
                return Err("QF1 committed TopK differs from greedy selection".into());
            }
            if let Some(trace) = trace.as_mut() {
                let max = i32::try_from(data.get(token as usize)).map_err(|_| "QF1 winning logit outside i32")?;
                trace.decision(tokens.len() as u32, exec.pos() - 1, token, max, self.argmax.1)?;
            }
            tokens.push(token);
            if palw_qf1::is_eos(token) || tokens.len() == limit as usize {
                break;
            }
        }
        let stop = palw_qf1::stop_reason(len, &tokens, limit)?;
        let (z, chunks) = readout.finish(len, &tokens, limit)?;
        let answer = LocalAnswer { tokens, stop, processed_positions: exec.pos(), z };
        if let Some(trace) = trace {
            trace.resolve_length(self, answer.tokens.len() as u32, max_leaves)?;
            trace.end(&answer, &chunks)?;
        }
        self.artifact.check_weights_unmodified()?;
        Ok(answer)
    }
}

struct Prediction<'a> {
    argmax: (u8, u16),
    selected: Option<u32>,
    producers: &'a [(u8, u16); ROLES],
    next: usize,
    rows: [Vec<i16>; ROLES],
}
impl StepSink for Prediction<'_> {
    fn node(&mut self, v: &NodeValue<'_>) {
        if v.layer.is_none() && (v.block, v.node) == self.argmax && v.data.len() == 1 {
            self.selected = u32::try_from(v.data.get(0)).ok();
        }
        let role = v.layer.map(usize::from).unwrap_or(ROLES - 1);
        if role != self.next || self.producers[role] != (v.block, v.node) {
            return;
        }
        let row = &mut self.rows[role];
        row.clear();
        match v.data {
            Slice::I8(values) => row.extend(values.iter().copied().map(i16::from)),
            Slice::I16(values) => row.extend_from_slice(values),
            _ => return,
        }
        self.next += 1;
    }
}
