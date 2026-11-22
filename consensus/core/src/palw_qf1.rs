//! RFC-0066 logical QF1 encodings. A digest identifies the supplied material.
//! Verifying its origin and execution remains the responsibility of the caller.
use crate::{Hash64, palw_tb1::releases::Tb1BeaconSeed, palw_tir_step_v1::PalwTirStepSpaceV1};
use blake2b_simd::{Params, State};
use misaka_palw_tir::{Block, DType, Dim, Prim, Ref, TensorType};

pub mod readout_chunks;
use readout_chunks::{ChunkStream, HashChunk};

pub const VOCAB: u32 = 151936;
pub const CONTEXT: u32 = 8192;
pub const MAX_NEW: u32 = 1024;
pub const WIDTH: usize = 4096;
pub const ROLES: usize = 37;

fn hash(size: usize, domain: &[u8]) -> State {
    let mut h = Params::new().hash_length(size).to_state();
    h.update(domain);
    h
}

/// Committed producers in prediction-position order, and the token decision
/// producer. This is derived from the validated TIR program and layout.
pub struct ReadoutBinding {
    producers: [(u8, u16); ROLES],
    argmax: (u8, u16),
}

impl ReadoutBinding {
    pub fn from_step_space(space: &PalwTirStepSpaceV1, final_norm: u16) -> Result<Self, &'static str> {
        let p = &space.program;
        let layout = &space.layout;
        if p.token_bound != VOCAB || layout.max_context != CONTEXT || p.schedule.layers.len() != ROLES - 1 {
            return Err("QF1 vocabulary, positions or depth differ");
        }
        if layout.commit_tiles.iter().any(|&lanes| lanes != 64) {
            return Err("QF1 producer replay requires the registered 64-lane commit layout");
        }
        let vector = TensorType::new(DType::I16, vec![Dim::Fixed(WIDTH as u32)]);
        let mut producers = [(0, 0); ROLES];
        for (role, &b) in p.schedule.layers.iter().enumerate() {
            let block = &p.blocks[usize::from(b)];
            let [out] = block.carry_out[..] else { return Err("QF1 requires a single residual carry") };
            let node = &block.nodes[usize::from(out)];
            if !node.commit || node.out != vector || !depends(block, out, Ref::CarryIn(0), None) {
                return Err("QF1 layer residual is not the committed A16 carry");
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
            return Err("QF1 final norm must carry the last residual into the committed i32 head");
        }
        let mut projections =
            post.nodes.iter().enumerate().filter(|(_, node)| {
                node.commit && node.prim == (Prim::TopK { axis: 0, k: 1 }) && node.inputs == [Ref::Node(p.logits)]
            });
        let (Some((argmax, _)), None) = (projections.next(), projections.next()) else {
            return Err("QF1 requires one committed TopK(1) on the full logits");
        };
        producers[ROLES - 1] = (p.schedule.post, final_norm);
        Ok(Self { producers, argmax: (p.schedule.post, argmax as u16) })
    }

    pub fn producers(&self) -> &[(u8, u16); ROLES] {
        &self.producers
    }

    pub fn argmax(&self) -> (u8, u16) {
        self.argmax
    }

    pub fn instance_descriptor(&self, manifest: Hash64, class: Hash64, scales: &[Hash64; ROLES]) -> Hash64 {
        let mut h = hash(64, b"MISAKA/private/QF1/local-instance/v1\0");
        h.update(manifest.as_byte_slice()).update(class.as_byte_slice());
        for (role, (&(block, node), scale)) in self.producers.iter().zip(scales).enumerate() {
            h.update(&[role as u8, block]).update(&node.to_le_bytes()).update(scale.as_byte_slice());
        }
        Hash64::from_bytes(*h.finalize().as_array())
    }

    /// The caller authenticates these roots and this Readout against the same
    /// immutable registration and program. For a split artifact, supply the
    /// authenticated ordinary inventory of the same weights.
    pub fn work_model(&self, graph: Hash64, ordinary_inventory: Hash64) -> WorkModel {
        let mut h = hash(64, b"MISAKA/private/QF1/work-model/v1\0");
        h.update(graph.as_byte_slice()).update(ordinary_inventory.as_byte_slice());
        for &(block, node) in &self.producers {
            h.update(&[block]).update(&node.to_le_bytes());
        }
        WorkModel(Hash64::from_bytes(*h.finalize().as_array()))
    }
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

/// Identity of the model's numerical program, weights and Readout.
pub struct WorkModel(Hash64);

impl WorkModel {
    pub fn work_key(&self, input: WorkInput) -> Hash64 {
        let mut h = hash(64, b"MISAKA/private/QF1/computational-work/v1\0");
        h.update(self.0.as_byte_slice()).update(input.token_root.as_byte_slice()).update(&input.limit.to_le_bytes());
        Hash64::from_bytes(*h.finalize().as_array())
    }
}

/// The actual ordered input token IDs and the fixed generation limit. The
/// caller authenticates these against the EXEC registration and beacon.
pub struct WorkInput {
    token_root: Hash64,
    limit: u32,
}

impl WorkInput {
    pub fn new(prompt: &[u32], limit: u32) -> Result<Self, String> {
        let len = u32::try_from(prompt.len()).map_err(|_| "QF1 input length")?;
        check_capacity(len, limit)?;
        if prompt.iter().any(|&id| id >= VOCAB) {
            return Err("QF1 input ID out of range".into());
        }
        let token_root = crate::palw_prompt_ids_v1::prompt_token_ids_root_v1(prompt).map_err(|e| e.to_string())?;
        Ok(Self { token_root, limit })
    }
}

pub fn attempt(context: &[u8], seed: &Tb1BeaconSeed) -> Result<[u8; 32], &'static str> {
    let n = u32::try_from(context.len()).map_err(|_| "QF1 context length")?;
    let mut h = hash(32, b"misaka/palw/qf1/attempt/v1\0");
    h.update(&n.to_le_bytes()).update(context).update(&32u32.to_le_bytes()).update(seed.as_bytes());
    Ok(h.finalize().as_bytes().try_into().unwrap())
}

pub fn check_capacity(input_len: u32, limit: u32) -> Result<(), &'static str> {
    if input_len == 0 || !(1..=MAX_NEW).contains(&limit) || input_len > CONTEXT - (limit - 1) {
        return Err("QF1 context or generation limit");
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Eos,
    Limit,
}

pub fn is_eos(id: u32) -> bool {
    matches!(id, 151643 | 151645)
}

pub fn stop_reason(prompt_len: u32, answer: &[u32], limit: u32) -> Result<Stop, &'static str> {
    check_capacity(prompt_len, limit)?;
    let (&last, before) = answer.split_last().ok_or("QF1 empty answer")?;
    if answer.len() > limit as usize || answer.iter().any(|&t| t >= VOCAB) || before.iter().copied().any(is_eos) {
        return Err("invalid QF1 answer");
    }
    if is_eos(last) {
        Ok(Stop::Eos)
    } else if answer.len() == limit as usize {
        Ok(Stop::Limit)
    } else {
        Err("QF1 premature stop")
    }
}

/// Feed residuals in layer order and the final normalized vector at each
/// prediction position. Validate the entire step before advancing the state.
pub struct Readout {
    chunks: ChunkStream,
    steps: u32,
}

impl Default for Readout {
    fn default() -> Self {
        Self { chunks: ChunkStream::new(), steps: 0 }
    }
}

impl Readout {
    pub fn step(&mut self, vectors: &[&[i16]; ROLES]) -> Result<(), &'static str> {
        if self.steps >= MAX_NEW || vectors.iter().any(|v| v.len() != WIDTH || v.contains(&i16::MIN)) {
            return Err("invalid QF1 readout step");
        }
        self.chunks.push(&[1]);
        self.chunks.push(&self.steps.to_le_bytes());
        let mut row = [0u8; WIDTH * 2];
        for vector in vectors {
            for (dst, value) in row.chunks_exact_mut(2).zip(*vector) {
                dst.copy_from_slice(&value.to_le_bytes());
            }
            self.chunks.push(&row);
        }
        self.steps += 1;
        Ok(())
    }
    pub fn finish(mut self, prompt_len: u32, answer: &[u32], limit: u32) -> Result<([u8; 64], Vec<HashChunk>), &'static str> {
        stop_reason(prompt_len, answer, limit)?;
        if answer.len() != self.steps as usize {
            return Err("QF1 answer/readout count mismatch");
        }
        self.chunks.push(&[255]);
        self.chunks.push(&self.steps.to_le_bytes());
        Ok(self.chunks.finish())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TreeKind {
    Input = 1,
    Answer = 2,
    Readout = 3,
    Trace = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeRoot {
    kind: TreeKind,
    attempt: [u8; 32],
    count: u64,
    pub digest: [u8; 64],
}

impl TreeRoot {
    fn hash(&self, tag: u8) -> State {
        let mut h = hash(64, b"misaka/palw/qf1/merkle/v1\0");
        h.update(&[tag]).update(&self.attempt).update(&[self.kind as u8]).update(&self.count.to_le_bytes());
        h
    }

    fn leaf(&self, index: u64, bytes: &[u8]) -> Result<[u8; 64], &'static str> {
        match self.kind {
            TreeKind::Input | TreeKind::Answer => {
                let t = u32::from_le_bytes(bytes.try_into().map_err(|_| "QF1 token leaf width")?);
                if t >= VOCAB {
                    return Err("QF1 token outside vocabulary");
                }
            }
            TreeKind::Readout if bytes.len() != 64 => return Err("QF1 readout leaf width"),
            TreeKind::Trace if bytes.is_empty() => return Err("QF1 empty trace record"),
            _ => {}
        }
        let size = u32::try_from(bytes.len()).map_err(|_| "QF1 leaf size")?;
        Ok(*self.hash(0).update(&index.to_le_bytes()).update(&size.to_le_bytes()).update(bytes).finalize().as_array())
    }

    fn parent(&self, height: u32, index: u64, left: &[u8; 64], right: &[u8; 64]) -> [u8; 64] {
        *self.hash(2).update(&height.to_le_bytes()).update(&index.to_le_bytes()).update(left).update(right).finalize().as_array()
    }

    /// Caller bounds and checks trace grammar at the source. Auxiliary memory
    /// for the tree is logarithmic in the already reserved number of leaves.
    pub fn build<'a>(
        attempt: [u8; 32],
        kind: TreeKind,
        leaves: impl ExactSizeIterator<Item = &'a [u8]>,
        budget: u64,
    ) -> Result<Self, &'static str> {
        let count = u64::try_from(leaves.len()).map_err(|_| "QF1 tree length")?;
        let mut tree = TreeBuilder::new(attempt, kind, count, budget, None)?;
        for leaf in leaves {
            tree.push(leaf)?;
        }
        Ok(tree.finish()?.0)
    }

    /// Interpret the digest asserted by an Original with externally expected
    /// kind, attempt and length. Authenticating a value still needs its opening.
    pub fn from_claimed_digest(attempt: [u8; 32], kind: TreeKind, count: u64, digest: [u8; 64]) -> Result<Self, &'static str> {
        if count == 0 || count.checked_next_power_of_two().is_none() || (kind == TreeKind::Readout && count != 1) {
            return Err("QF1 claimed tree shape");
        }
        Ok(Self { kind, attempt, count, digest })
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn verify_opening(&self, index: u64, value: &[u8], siblings: &[[u8; 64]]) -> bool {
        if index >= self.count || siblings.len() != self.count.next_power_of_two().ilog2() as usize {
            return false;
        }
        let Ok(mut node) = self.leaf(index, value) else {
            return false;
        };
        let mut pos = index;
        for (depth, sibling) in siblings.iter().enumerate() {
            let (left, right) = if pos & 1 == 1 { (sibling, &node) } else { (&node, sibling) };
            pos >>= 1;
            node = self.parent(depth as u32 + 1, pos, left, right);
        }
        node == self.digest
    }
}

/// Stream one tree with a fixed number of requested openings. Each path uses
/// logarithmic space and shares the leaf and parent hashes with all other paths.
pub struct TreeBuilder<const N: usize = 1> {
    root: TreeRoot,
    padded: u64,
    next: u64,
    path_for: [Option<u64>; N],
    stack: Vec<[u8; 64]>,
    paths: [Vec<[u8; 64]>; N],
}

impl TreeBuilder {
    pub fn new(attempt: [u8; 32], kind: TreeKind, count: u64, budget: u64, path_for: Option<u64>) -> Result<Self, &'static str> {
        Self::with_openings(attempt, kind, count, budget, [path_for])
    }

    pub fn finish(self) -> Result<(TreeRoot, Vec<[u8; 64]>), &'static str> {
        let (root, [path]) = self.finish_with_openings()?;
        Ok((root, path))
    }
}

impl<const N: usize> TreeBuilder<N> {
    pub fn with_openings(
        attempt: [u8; 32],
        kind: TreeKind,
        count: u64,
        budget: u64,
        path_for: [Option<u64>; N],
    ) -> Result<Self, &'static str> {
        if count == 0 || count > budget || (kind == TreeKind::Readout && count != 1) || path_for.iter().flatten().any(|&i| i >= count)
        {
            return Err("QF1 tree length or opening index");
        }
        Ok(Self {
            root: TreeRoot { kind, attempt, count, digest: [0; 64] },
            padded: count.checked_next_power_of_two().ok_or("QF1 tree size overflow")?,
            next: 0,
            path_for,
            stack: Vec::new(),
            paths: std::array::from_fn(|_| Vec::new()),
        })
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<(), &'static str> {
        if self.next >= self.root.count {
            return Err("QF1 extra leaf");
        }
        self.append_hash(self.root.leaf(self.next, bytes)?);
        Ok(())
    }

    fn append_hash(&mut self, mut node: [u8; 64]) {
        let mut pos = self.next;
        let mut height = 0;
        while pos & 1 == 1 {
            let left = self.stack.pop().expect("completed left sibling");
            for (target, path) in self.path_for.iter().zip(&mut self.paths) {
                let Some(target) = target else { continue };
                let owner = target >> height;
                if owner == pos {
                    path.push(left);
                }
                if owner == pos - 1 {
                    path.push(node);
                }
            }
            pos >>= 1;
            height += 1;
            node = self.root.parent(height, pos, &left, &node);
        }
        self.stack.push(node);
        self.next += 1;
    }

    pub fn finish_with_openings(mut self) -> Result<(TreeRoot, [Vec<[u8; 64]>; N]), &'static str> {
        if self.next != self.root.count {
            return Err("QF1 missing leaf");
        }
        while self.next < self.padded {
            self.append_hash(*self.root.hash(1).update(&self.next.to_le_bytes()).finalize().as_array());
        }
        self.root.digest = self.stack.pop().expect("nonempty tree");
        Ok((self.root, self.paths))
    }
}

/// Logical ticket only. Admission, target, native acceptance and one-time
/// accounting are enforced by the owning mining transition.
pub fn ticket(roots: [&TreeRoot; 4]) -> Result<[u8; 32], &'static str> {
    let attempt = roots[0].attempt;
    let mut h = hash(32, b"misaka/palw/qf1/ticket/v1\0");
    h.update(&attempt);
    for (i, r) in roots.into_iter().enumerate() {
        if r.kind as usize != i + 1 || r.attempt != attempt {
            return Err("mixed QF1 tree kind or attempt");
        }
        h.update(&r.count.to_le_bytes()).update(&r.digest);
    }
    Ok(h.finalize().as_bytes().try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streamed_disclosures_verify_for_every_index_in_ragged_trees() {
        for count in 1usize..18 {
            let leaves: Vec<_> = (0..count as u32).map(u32::to_le_bytes).collect();
            let reference = TreeRoot::build([8; 32], TreeKind::Answer, leaves.iter().map(|l| l.as_slice()), 20).unwrap();
            for i in 0..count {
                let mut tree = TreeBuilder::new([8; 32], TreeKind::Answer, count as u64, 20, Some(i as u64)).unwrap();
                for leaf in &leaves {
                    tree.push(leaf).unwrap();
                }
                assert!(tree.push(&[0; 4]).is_err());
                let (root, path) = tree.finish().unwrap();
                assert_eq!(root, reference);
                assert!(root.verify_opening(i as u64, &leaves[i], &path));
                assert!(!root.verify_opening(i as u64, &[255; 4], &path));
            }
        }
        let tree = TreeBuilder::new([0; 32], TreeKind::Trace, 1, 1, None).unwrap();
        assert!(tree.finish().is_err());
    }

    #[test]
    fn qf1_native_readout_matches_supplied_vector_and_generation_boundaries() {
        let vectors: Vec<Vec<i16>> =
            (0..ROLES).map(|r| (0..WIDTH).map(|c| (((257 + r * 31 + c * 17) % 65535) as i32 - 32767) as i16).collect()).collect();
        let views = std::array::from_fn(|r| vectors[r].as_slice());
        let mut h = Readout::default();
        let mut invalid = views;
        invalid[36] = &[];
        assert!(h.step(&invalid).is_err());
        h.step(&views).unwrap();
        let (z, chunks) = h.finish(3, &[151645], 8).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/design/palw/models/qf1/conformance-vectors.json")).unwrap();
        assert_eq!(faster_hex::hex_string(&z), v["synthetic_one_step"]["z_hex"]);
        let mut message = readout_chunks::DOMAIN.to_vec();
        message.push(1);
        message.extend_from_slice(&0u32.to_le_bytes());
        for vector in &vectors {
            for value in vector {
                message.extend_from_slice(&value.to_le_bytes());
            }
        }
        message.push(255);
        message.extend_from_slice(&1u32.to_le_bytes());
        assert_eq!(message.len(), 303141);
        assert_eq!(chunks.len(), 75);
        for (i, (data, chunk)) in message.chunks(4096).zip(&chunks).enumerate() {
            let previous = i.checked_sub(1).and_then(|j| chunks.get(j));
            assert!(chunk.verify(1, i as u32, previous, data));
            let mut changed = chunk.clone();
            changed.chain_after[0] ^= 1;
            assert!(!changed.verify(1, i as u32, previous, data));
        }
        let attempt = std::array::from_fn(|i| i as u8);
        let token_bytes = [1u32.to_le_bytes(), 2u32.to_le_bytes(), 3u32.to_le_bytes()];
        let x = TreeRoot::build(attempt, TreeKind::Input, token_bytes.iter().map(|v| v.as_slice()), 3).unwrap();
        let y = TreeRoot::build(attempt, TreeKind::Answer, std::iter::once(151645u32.to_le_bytes().as_slice()), 1).unwrap();
        let readout = TreeRoot::build(attempt, TreeKind::Readout, std::iter::once(z.as_slice()), 1).unwrap();
        let trace = TreeRoot::build(attempt, TreeKind::Trace, std::iter::once(b"SYNTHETIC-NOT-A-VALID-TRACE".as_slice()), 1).unwrap();
        let roots = [&x, &y, &readout, &trace];
        for (i, r) in roots.iter().enumerate() {
            assert_eq!(faster_hex::hex_string(&r.digest), v["synthetic_one_step"]["roots_hex"][i]);
        }
        assert!(readout.verify_opening(0, &z, &[]));
        assert!(!readout.verify_opening(1, &z, &[]));
        assert_eq!(faster_hex::hex_string(&ticket(roots).unwrap()), v["synthetic_one_step"]["ticket_hex"]);
        assert!(ticket([&y, &x, &readout, &trace]).is_err());
        assert_eq!(stop_reason(8192, &[151643], 1), Ok(Stop::Eos));
        assert_eq!(stop_reason(8192, &[17], 1), Ok(Stop::Limit));
        assert!(stop_reason(8192, &[151645], 2).is_err());
        assert!(stop_reason(2, &[151645, 0], 2).is_err());
        assert!(stop_reason(2, &[0], 2).is_err());
    }
}
