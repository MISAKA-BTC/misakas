//! RFC-0066 logical records, streamed directly from the TIR executor. Since
//! every leaf commits the final count, the producer supplies T from its first
//! execution. A checker can use the declared T; any disagreement fails replay.
use super::{LocalAnswer, LocalInstance};
use crate::{NodeValue, StepSink, TirExecutor};
use kaspa_consensus_core::{
    Hash64,
    palw_qf1::{
        self, Stop, TreeBuilder, TreeKind, TreeRoot,
        readout_chunks::{HashChunk, record_count},
    },
    palw_tir_step_v1::PalwTirJobShapeV1,
};
use misaka_palw_tir::{DType, TirError, TirErrorKind, TirResult};

pub struct LocalTrace {
    pub answer: LocalAnswer,
    pub roots: [TreeRoot; 4],
    pub ticket: [u8; 32],
    pub opening: Option<TraceOpening>,
    /// Authenticated terminal record retained with the original for direct Court disclosure.
    pub stop_opening: TraceOpening,
}

pub struct TraceOpening {
    pub index: u64,
    pub record: Vec<u8>,
    pub siblings: Vec<[u8; 64]>,
}

#[derive(Clone, Copy)]
pub struct TraceRequest {
    pub context_digest: Hash64,
    pub answer_len: u32,
    /// Disclosure at one absolute logical trace index, if requested.
    pub open: Option<u64>,
}

fn tokens_root(s: [u8; 32], kind: TreeKind, tokens: &[u32]) -> Result<TreeRoot, &'static str> {
    let n = tokens.len() as u64;
    let mut tree = TreeBuilder::new(s, kind, n, n, None)?;
    for &id in tokens {
        tree.push(&id.to_le_bytes())?;
    }
    Ok(tree.finish()?.0)
}

struct Geometry {
    tiles: u64,
    count: u64,
    chunks: u32,
}

impl Geometry {
    fn new(instance: &LocalInstance<'_>, len: u32, steps: u32, cap: u64) -> Result<Self, String> {
        palw_qf1::check_capacity(len, steps)?;
        let positions = len + steps - 1;
        let shape = PalwTirJobShapeV1 { prefill: len, positions };
        let tiles = instance.space.running_commit_tiles(&shape, positions);
        let chunks = record_count(steps)?;
        let count = tiles.saturating_add(u128::from(chunks) + u128::from(steps) + 3);
        if count > u128::from(cap) {
            return Err("QF1 logical trace exceeds registered leaf budget".into());
        }
        Ok(Self { tiles: tiles as u64, count: count as u64, chunks })
    }
}

impl LocalInstance<'_> {
    /// Logical TRACE length for a disclosure on this concrete layout.
    pub fn trace_leaf_count(&self, prompt_len: u32, answer_len: u32, cap: u64) -> Result<u64, String> {
        Ok(Geometry::new(self, prompt_len, answer_len, cap)?.count)
    }
}

pub(super) struct Trace {
    s: [u8; 32],
    len: u32,
    steps: u32,
    layout: Geometry,
    input: TreeRoot,
    tree: TreeBuilder<2>,
    stop_record: Vec<u8>,
    next: u64,
    tiles: u64,
    opening: Option<(u64, Vec<u8>)>,
    post: u8,
    // Reuse the envelope buffer for every committed tile.
    record: Vec<u8>,
    tail: Option<(TreeRoot, TreeRoot)>,
}

impl Trace {
    pub(super) fn begin(
        instance: &LocalInstance<'_>,
        prompt: &[u32],
        limit: u32,
        cap: u64,
        s: [u8; 32],
        request: TraceRequest,
    ) -> Result<Self, String> {
        let len = u32::try_from(prompt.len()).map_err(|_| "QF1 prompt length")?;
        // Reserve worst-case capacity before doing any numerical work; a short
        // claimed answer does not loosen the original registration budget.
        Geometry::new(instance, len, limit, cap)?;
        let steps = request.answer_len;
        if steps > limit {
            return Err("QF1 trace answer exceeds original limit".into());
        }
        let layout = Geometry::new(instance, len, steps, cap)?;
        let stop_index = layout.count - 2 - u64::from(layout.chunks);
        let tree = TreeBuilder::with_openings(s, TreeKind::Trace, layout.count, cap, [request.open, Some(stop_index)])?;
        let input = tokens_root(s, TreeKind::Input, prompt)?;
        let mut trace = Self {
            s,
            len,
            steps,
            layout,
            input,
            tree,
            stop_record: Vec::new(),
            next: 0,
            tiles: 0,
            opening: request.open.map(|i| (i, Vec::new())),
            post: instance.space.program.schedule.post,
            record: Vec::with_capacity(320),
            tail: None,
        };
        let rec = trace.start(0);
        rec.extend_from_slice(request.context_digest.as_byte_slice());
        rec.extend_from_slice(instance.identity.descriptor.as_byte_slice());
        rec.extend_from_slice(&s);
        rec.extend_from_slice(&input.digest);
        rec.extend_from_slice(&len.to_le_bytes());
        rec.extend_from_slice(&(limit as u16).to_le_bytes());
        trace.push()?;
        Ok(trace)
    }

    fn start(&mut self, kind: u8) -> &mut Vec<u8> {
        self.record.clear();
        self.record.extend_from_slice(&[kind, 0, 0, 0, 0]);
        &mut self.record
    }

    fn push(&mut self) -> Result<(), &'static str> {
        let len = u32::try_from(self.record.len() - 5).map_err(|_| "QF1 record length")?;
        self.record[1..5].copy_from_slice(&len.to_le_bytes());
        self.tree.push(&self.record)?;
        if let Some((_, value)) = self.opening.as_mut().filter(|(i, _)| *i == self.next) {
            value.clone_from(&self.record);
        }
        self.next += 1;
        Ok(())
    }

    fn node_value(&mut self, pos: u32, v: &NodeValue<'_>) -> Result<(), &'static str> {
        if !v.commit || !v.dtype.committable() {
            return Err("QF1 invalid commit type");
        }
        let stage = if v.layer.is_some() {
            1
        } else if v.block == self.post {
            2
        } else {
            0
        };
        for start in (0..v.data.len()).step_by(64) {
            let lanes = 64.min(v.data.len() - start);
            let record = self.start(1);
            record.extend_from_slice(&pos.to_le_bytes());
            record.push(stage);
            record.extend_from_slice(&v.layer.unwrap_or(u16::MAX).to_le_bytes());
            record.extend_from_slice(&u32::from(v.node).to_le_bytes());
            record.extend_from_slice(&u32::try_from(start).map_err(|_| "QF1 tile index")?.to_le_bytes());
            record.extend_from_slice(&(lanes as u16).to_le_bytes());
            record.push(v.dtype.tag());
            for i in start..start + lanes {
                let value = v.data.get(i);
                if !v.dtype.contains(value) || (v.dtype == DType::I16 && value == i128::from(i16::MIN)) {
                    return Err("QF1 committed lane outside declared type");
                }
                v.dtype.encode_le(value, record);
            }
            self.push()?;
            self.tiles += 1;
        }
        Ok(())
    }

    pub(super) fn decision(&mut self, j: u32, pos: u32, id: u32, max: i32, argmax: u16) -> Result<(), String> {
        if j >= self.steps || pos != self.len - 1 + j {
            return Err("QF1 decision outside original trace length".into());
        }
        let record = self.start(2);
        for value in [j, pos, id] {
            record.extend_from_slice(&value.to_le_bytes());
        }
        record.extend_from_slice(&max.to_le_bytes());
        record.extend_from_slice(&u32::from(argmax).to_le_bytes());
        Ok(self.push()?)
    }

    pub(super) fn end(&mut self, answer: &LocalAnswer, chunks: &[HashChunk]) -> Result<(), String> {
        let t = answer.tokens.len() as u32;
        let n = answer.processed_positions;
        if t != self.steps || n != self.len + t - 1 || self.tiles != self.layout.tiles || chunks.len() != self.layout.chunks as usize {
            return Err("QF1 executed trace geometry differs from Original".into());
        }
        let rec = self.start(3);
        for value in [t, n, *answer.tokens.last().ok_or("empty QF1 result")?] {
            rec.extend_from_slice(&value.to_le_bytes());
        }
        rec.push(match answer.stop {
            Stop::Eos => 0,
            Stop::Limit => 1,
        });
        self.push()?;
        self.stop_record.clone_from(&self.record);
        for c in chunks {
            let rec = self.start(4);
            rec.extend_from_slice(&c.index.to_le_bytes());
            rec.extend_from_slice(&c.consumed.to_le_bytes());
            rec.extend_from_slice(&c.length.to_le_bytes());
            rec.push(u8::from(c.final_flag));
            rec.extend_from_slice(&c.chain_after);
            self.push()?;
        }
        let y = tokens_root(self.s, TreeKind::Answer, &answer.tokens)?;
        let z = TreeRoot::build(self.s, TreeKind::Readout, std::iter::once(answer.z.as_slice()), 1)?;
        let [rx, ry, rz] = [self.input.digest, y.digest, z.digest];
        let (l, tiles, h, d) = (self.len, self.tiles, self.layout.chunks, self.layout.count);
        let rec = self.start(5);
        for root in [rx, ry, rz] {
            rec.extend_from_slice(&root);
        }
        for value in [l, t, n] {
            rec.extend_from_slice(&value.to_le_bytes());
        }
        rec.extend_from_slice(&tiles.to_le_bytes());
        rec.extend_from_slice(&t.to_le_bytes());
        rec.extend_from_slice(&h.to_le_bytes());
        rec.extend_from_slice(&d.to_le_bytes());
        rec.extend_from_slice(&answer.z);
        self.push()?;
        self.tail = Some((y, z));
        Ok(())
    }

    pub(super) fn finish(self, answer: LocalAnswer) -> Result<LocalTrace, String> {
        let (y, z) = self.tail.ok_or("missing QF1 End")?;
        let (trace, [siblings, stop_path]) = self.tree.finish_with_openings()?;
        let roots = [self.input, y, z, trace];
        let ticket = palw_qf1::ticket(roots.each_ref())?;
        let opening = self.opening.map(|(index, record)| TraceOpening { index, record, siblings });
        let stop_opening = TraceOpening {
            index: self.layout.count - 2 - u64::from(self.layout.chunks),
            record: self.stop_record,
            siblings: stop_path,
        };
        Ok(LocalTrace { answer, roots, ticket, opening, stop_opening })
    }
}

struct Observed<'a> {
    pos: u32,
    trace: &'a mut Trace,
    inner: &'a mut dyn StepSink,
}

impl StepSink for Observed<'_> {
    fn node(&mut self, _: &NodeValue<'_>) {
        unreachable!("executor calls the fallible observer")
    }
    fn try_node(&mut self, v: &NodeValue<'_>) -> TirResult<()> {
        self.trace.node_value(self.pos, v).map_err(|e| TirError::new(TirErrorKind::Operand, e))?;
        self.inner.try_node(v)
    }
}

pub(super) fn step(
    exec: &mut TirExecutor<'_>,
    id: u32,
    sink: &mut dyn StepSink,
    post: bool,
    trace: Option<&mut Trace>,
) -> Result<(), String> {
    let result = if let Some(trace) = trace {
        let pos = exec.pos();
        exec.step_opt(id, &mut Observed { pos, trace, inner: sink }, post)
    } else {
        exec.step_opt(id, sink, post)
    };
    result.map_err(|e| e.to_string())
}
