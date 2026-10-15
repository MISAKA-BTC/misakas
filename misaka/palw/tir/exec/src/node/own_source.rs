//! Disposable independent original-job computation. Hashes are written to an
//! anonymous bounded disk tree, not an N-leaf resident capture or private snapshot.
use super::run::TirClassRunnerV1;
#[cfg(test)]
#[path = "../../tests/node_common/mod.rs"]
mod test_common;
use kaspa_consensus_core::{
    Hash64,
    palw_backend::{PalwTirOwnStepSourceInfoV1, PalwTirOwnStepSourceV1},
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_step_leg::{step_merkle_leaf_v1, step_merkle_node_v1},
    palw_step_refute::{
        PALW_BASE0_DOMAIN_LOGITS_TRACE, PALW_LOGITS_TILE_LANES, flat_logits_scheme_id_v1, tiled_logits_outer_root_v1,
        tiled_logits_scheme_id_v1, tiled_logits_tile_leaf_v1,
    },
    palw_tir_step_v1::palw_tir_execution_root_v1,
    palw_v2::PalwJobContextV2,
};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::Mutex,
};

const READ_FRONTIER: u32 = 1024;

#[derive(Default)]
struct Frontier {
    count: u64,
    nodes: Vec<Option<Hash64>>,
}
impl Frontier {
    fn push(&mut self, raw: Hash64) -> Result<(), String> {
        let mut hash = step_merkle_leaf_v1(self.count, &raw);
        self.count = self.count.checked_add(1).ok_or("own Merkle count overflow")?;
        let mut level = 0;
        loop {
            if level == self.nodes.len() {
                self.nodes.push(None);
            }
            let Some(left) = self.nodes[level].take() else {
                self.nodes[level] = Some(hash);
                break;
            };
            hash = step_merkle_node_v1(&left, &hash);
            level += 1;
        }
        Ok(())
    }
    fn root(&self) -> Result<Hash64, String> {
        let mut tail = None;
        for node in &self.nodes {
            if let Some(left) = node {
                tail = Some(match tail {
                    None => *left,
                    Some(right) => step_merkle_node_v1(left, &right),
                });
            }
        }
        tail.ok_or("empty own Merkle tree".into())
    }
}

pub(crate) fn tree_disk_bytes_v1(count: u64) -> Result<u64, String> {
    let (_, _, bytes) = DiskTree::geometry(count)?;
    Ok(bytes)
}

struct DiskTree {
    file: File,
    widths: Vec<u64>,
    offsets: Vec<u64>,
    emitted: u64,
    root: Option<Hash64>,
}
impl DiskTree {
    fn geometry(count: u64) -> Result<(Vec<u64>, Vec<u64>, u64), String> {
        if count == 0 {
            return Err("empty own disk tree".into());
        }
        let (mut widths, mut offsets, mut bytes, mut width) = (Vec::new(), Vec::new(), 0u64, count);
        loop {
            widths.push(width);
            offsets.push(bytes);
            bytes = bytes.checked_add(width.checked_mul(64).ok_or("own tree size overflow")?).ok_or("own tree size overflow")?;
            if width == 1 {
                break;
            }
            width = width.div_ceil(2);
        }
        Ok((widths, offsets, bytes))
    }
    fn new(directory: &Path, count: u64) -> Result<Self, String> {
        let (widths, offsets, bytes) = Self::geometry(count)?;
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        // No named cache survives process death or can become a restart credential.
        let file = tempfile::tempfile_in(directory).map_err(|e| e.to_string())?;
        file.set_len(bytes).map_err(|e| e.to_string())?;
        Ok(Self { file, widths, offsets, emitted: 0, root: None })
    }
    fn location(&self, level: usize, index: u64) -> Result<u64, String> {
        if self.widths.get(level).is_none_or(|width| index >= *width) {
            return Err("own tree coordinate outside original geometry".into());
        }
        self.offsets[level]
            .checked_add(index.checked_mul(64).ok_or("own tree offset overflow")?)
            .ok_or("own tree offset overflow".into())
    }
    fn write(&mut self, level: usize, index: u64, hash: Hash64) -> Result<(), String> {
        let at = self.location(level, index)?;
        self.file.seek(SeekFrom::Start(at)).and_then(|_| self.file.write_all(hash.as_byte_slice())).map_err(|e| e.to_string())
    }
    fn raw(&mut self, level: usize, index: u64) -> Result<Hash64, String> {
        let at = self.location(level, index)?;
        let mut bytes = [0; 64];
        self.file.seek(SeekFrom::Start(at)).and_then(|_| self.file.read_exact(&mut bytes)).map_err(|e| e.to_string())?;
        Ok(Hash64::from_bytes(bytes))
    }
    fn node(&mut self, level: usize, index: u64) -> Result<Hash64, String> {
        let raw = self.raw(level, index)?;
        Ok(if level == 0 { step_merkle_leaf_v1(index, &raw) } else { raw })
    }
    fn push(&mut self, raw: Hash64) -> Result<(), String> {
        let mut index = self.emitted;
        self.write(0, index, raw)?;
        self.emitted += 1;
        let mut level = 0;
        let mut hash = step_merkle_leaf_v1(index, &raw);
        while index & 1 == 1 {
            let left = self.node(level, index - 1)?;
            hash = step_merkle_node_v1(&left, &hash);
            index /= 2;
            level += 1;
            self.write(level, index, hash)?;
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<Hash64, String> {
        if self.emitted != self.widths[0] {
            return Err("own streamed tree is incomplete".into());
        }
        // Only the ragged final parent per level may not yet have been emitted.
        // Native indexed-leaf/node helpers retain the original promote-odd codec.
        for level in 0..self.widths.len() - 1 {
            let width = self.widths[level];
            let parent = (width - 1) / 2;
            let left = self.node(level, parent * 2)?;
            let hash = if parent * 2 + 1 < width { step_merkle_node_v1(&left, &self.node(level, parent * 2 + 1)?) } else { left };
            self.write(level + 1, parent, hash)?;
        }
        let root = self.node(self.widths.len() - 1, 0)?;
        self.root = Some(root);
        Ok(root)
    }
    fn checked(&mut self, level: usize, index: u64) -> Result<(Hash64, Hash64), String> {
        let raw = self.raw(level, index)?;
        let original = if level == 0 { step_merkle_leaf_v1(index, &raw) } else { raw };
        let (mut hash, mut at, mut depth) = (original, index, level);
        while depth + 1 < self.widths.len() {
            if (at ^ 1) < self.widths[depth] {
                let sibling = self.node(depth, at ^ 1)?;
                hash = if at & 1 == 0 { step_merkle_node_v1(&hash, &sibling) } else { step_merkle_node_v1(&sibling, &hash) };
            }
            at /= 2;
            depth += 1;
        }
        if Some(hash) != self.root {
            return Err("own disk subtree no longer authenticates computed root".into());
        }
        Ok((original, raw))
    }
    fn frontier(&mut self, level: u8, index: u64) -> Result<Vec<Hash64>, String> {
        self.checked(level as usize, index)?;
        let descend = level.min(10);
        let child_level = level - descend;
        let first = index.checked_shl(u32::from(descend)).ok_or("own frontier offset overflow")?;
        let end = first.checked_add(1 << descend).ok_or("own frontier end overflow")?.min(self.widths[child_level as usize]);
        (first..end).map(|at| self.checked(child_level as usize, at).map(|(hash, _)| hash)).collect()
    }
}

struct Trace {
    context: PalwJobContextV2,
    flat: Option<blake2b_simd::State>,
    tiled_rows: Option<DiskTree>,
    rows: u32,
}
impl Trace {
    fn new(directory: &Path, ctx: &PalwJobContextV2, scheme: Hash64) -> Result<Self, String> {
        let (flat, tiled_rows) = if scheme == tiled_logits_scheme_id_v1() {
            (None, Some(DiskTree::new(directory, u64::from(ctx.exact_decode_tokens))?))
        } else if scheme == flat_logits_scheme_id_v1() {
            let mut hash = blake2b_simd::Params::new().hash_length(64).key(PALW_BASE0_DOMAIN_LOGITS_TRACE).to_state();
            hash.update(ctx.context_hash().as_byte_slice());
            for size in [ctx.declared_prefill_tokens, ctx.exact_decode_tokens, ctx.exact_decode_tokens] {
                hash.update(&u64::from(size).to_le_bytes());
            }
            (Some(hash), None)
        } else {
            return Err("unknown original TIR logits codec".into());
        };
        Ok(Self { context: ctx.clone(), flat, tiled_rows, rows: 0 })
    }
    fn row(&mut self, index: u32, values: &[i32]) -> Result<(), String> {
        if index != self.rows || values.is_empty() || index >= self.context.exact_decode_tokens {
            return Err("own trace row changed original order/geometry".into());
        }
        if let Some(hash) = self.flat.as_mut() {
            hash.update(&(values.len() as u64).to_le_bytes());
            for value in values {
                hash.update(&value.to_le_bytes());
            }
        } else {
            let mut tiles = Frontier::default();
            for (tile, lanes) in values.chunks(PALW_LOGITS_TILE_LANES).enumerate() {
                tiles.push(tiled_logits_tile_leaf_v1(&self.context.context_hash(), index, tile as u32, lanes))?;
            }
            self.tiled_rows.as_mut().ok_or("own row tree absent")?.push(tiles.root()?)?;
        }
        self.rows += 1;
        Ok(())
    }
    fn finish(&mut self, ids: &[u32]) -> Result<(Hash64, Option<Hash64>), String> {
        if self.rows != self.context.exact_decode_tokens || ids.len() != self.rows as usize {
            return Err("own trace did not reach exact original EOF".into());
        }
        if let Some(hash) = self.flat.take() {
            let mut hash = hash;
            hash.update(&(ids.len() as u64).to_le_bytes());
            for id in ids {
                hash.update(&id.to_le_bytes());
            }
            Ok((Hash64::from_bytes(hash.finalize().as_bytes().try_into().expect("64")), None))
        } else {
            let root = self.tiled_rows.as_mut().ok_or("own row tree absent")?.finish()?;
            Ok((tiled_logits_outer_root_v1(&self.context, u64::from(self.rows), &root, ids), Some(root)))
        }
    }
}

pub(crate) struct OwnStepSource {
    info: PalwTirOwnStepSourceInfoV1,
    steps: Mutex<DiskTree>,
    rows: Option<Mutex<DiskTree>>,
}
impl OwnStepSource {
    pub(crate) fn build(
        directory: &Path,
        original: &PalwPublicTirBindingHeaderV1,
        runner: &TirClassRunnerV1<'_>,
        count: u64,
        cap: u64,
        prompt: &[u32],
    ) -> Result<Self, String> {
        let mut steps = DiskTree::new(directory, count)?;
        let mut trace = Trace::new(directory, &original.job_context, Hash64::from_bytes(runner.space.program.logits_scheme_id))?;
        let mut failure = None;
        let ids = runner.stream_v1(
            &original.job_context,
            prompt,
            cap,
            &mut |leaf| {
                if failure.is_none() {
                    if leaf.index != steps.emitted {
                        failure = Some("own step stream changed original order".into());
                    } else if let Err(error) = steps.push(leaf.hash) {
                        failure = Some(error);
                    }
                }
            },
            &mut |index, row| trace.row(index, row),
        )?;
        if let Some(failure) = failure {
            return Err(failure);
        }
        let mut header = original.clone();
        header.step_leaf_count = count;
        header.step_merkle_root = steps.finish()?;
        let (trace_root, rows_root) = trace.finish(&ids)?;
        header.full_logits_trace_root = trace_root;
        header.committed_execution_root = palw_tir_execution_root_v1(
            &header.job_context.context_hash(),
            &trace_root,
            &runner.class_id,
            count,
            &header.step_merkle_root,
        );
        let output_root = kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(&header.job_context, &ids);
        Ok(Self {
            info: PalwTirOwnStepSourceInfoV1 { header, generated_ids: ids, output_root, rows_root },
            steps: Mutex::new(steps),
            rows: trace.tiled_rows.map(Mutex::new),
        })
    }
}
impl PalwTirOwnStepSourceV1 for OwnStepSource {
    fn info_v1(&self) -> &PalwTirOwnStepSourceInfoV1 {
        &self.info
    }
    fn node_v1(&self, level: u8, index: u64) -> Result<Hash64, String> {
        self.steps.lock().map_err(|_| "own step tree poisoned")?.checked(level as usize, index).map(|(hash, _)| hash)
    }
    fn frontier_v1(&self, level: u8, index: u64) -> Result<Vec<Hash64>, String> {
        self.steps.lock().map_err(|_| "own step tree poisoned")?.frontier(level, index)
    }
    fn leaf_hashes_v1(&self, first: u64, count: u32) -> Result<Vec<Hash64>, String> {
        if count == 0 || count > READ_FRONTIER {
            return Err("own raw hash request exceeds bounded frontier".into());
        }
        let mut tree = self.steps.lock().map_err(|_| "own step tree poisoned")?;
        let end = first.checked_add(u64::from(count)).ok_or("own hash range overflow")?;
        if end > tree.widths[0] {
            return Err("own raw hash range outside original job".into());
        }
        (first..end).map(|at| tree.checked(0, at).map(|(_, raw)| raw)).collect()
    }
    fn row_node_v1(&self, level: u8, index: u64) -> Result<Hash64, String> {
        self.rows
            .as_ref()
            .ok_or("original flat trace has no row Merkle tree")?
            .lock()
            .map_err(|_| "own row tree poisoned")?
            .checked(level as usize, index)
            .map(|(hash, _)| hash)
    }
    fn row_frontier_v1(&self, level: u8, index: u64) -> Result<Vec<Hash64>, String> {
        self.rows
            .as_ref()
            .ok_or("original flat trace has no row Merkle tree")?
            .lock()
            .map_err(|_| "own row tree poisoned")?
            .frontier(level, index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_step_leg::step_merkle_root_capped_v1;

    #[test]
    fn streamed_frontier_and_disk_tree_match_native_promote_odd_at_every_shape() {
        let directory = tempfile::tempdir().unwrap();
        for count in 1..=130 {
            let leaves: Vec<_> = (0..count).map(|i| Hash64::from_u64_word(i + 1)).collect();
            let expected = step_merkle_root_capped_v1(&leaves, 130).unwrap();
            let mut frontier = Frontier::default();
            let mut disk = DiskTree::new(directory.path(), count).unwrap();
            for leaf in &leaves {
                frontier.push(*leaf).unwrap();
                disk.push(*leaf).unwrap();
            }
            assert_eq!(frontier.root().unwrap(), expected);
            assert_eq!(disk.finish().unwrap(), expected);
            assert_eq!(disk.file.metadata().unwrap().len(), tree_disk_bytes_v1(count).unwrap());
            let top = disk.widths.len() as u8 - 1;
            assert_eq!(disk.checked(top as usize, 0).unwrap().0, expected);
            let children = disk.frontier(top, 0).unwrap();
            assert!(children.len() <= READ_FRONTIER as usize);
            for (index, leaf) in leaves.iter().enumerate() {
                assert_eq!(disk.checked(0, index as u64).unwrap().1, *leaf);
            }
        }
        assert!(tree_disk_bytes_v1(0).is_err());
        assert!(tree_disk_bytes_v1(u64::MAX).is_err());
    }
    #[test]
    fn anonymous_tree_authenticates_each_read_and_drops_without_restart_artifacts() {
        let directory = tempfile::tempdir().unwrap();
        let mut disk = DiskTree::new(directory.path(), 3).unwrap();
        for i in 0..3 {
            disk.push(Hash64::from_u64_word(i + 1)).unwrap();
        }
        disk.finish().unwrap();
        disk.write(0, 1, Hash64::from_u64_word(91)).unwrap();
        assert!(disk.checked(0, 1).is_err());
        assert!(disk.frontier(2, 0).is_err());
        assert!(disk.checked(3, 0).is_err());
        drop(disk);
        assert!(std::fs::read_dir(directory.path()).unwrap().next().is_none());
    }
    #[test]
    fn streamed_original_flat_and_tiled_trace_roots_equal_existing_whole_rows_codec() {
        let directory = tempfile::tempdir().unwrap();
        for decode in [1, 3, 9] {
            let z = Hash64::default();
            let context = PalwJobContextV2 {
                version: 2,
                network_id: b"testnet-12".to_vec(),
                job_id: Hash64::from_u64_word(7),
                job_nullifier: z,
                assignment_id: z,
                execution_seed: [0; 32],
                model_profile_id: z,
                runtime_manifest_hash: z,
                runtime_class_id: z,
                shape_profile_id: z,
                trace_scheme_id: flat_logits_scheme_id_v1(),
                cu_ruleset_id: z,
                tokenizer_id: z,
                prompt_token_ids_hash: z,
                declared_prefill_tokens: 3,
                exact_decode_tokens: decode,
                max_context_tokens: 64,
            };
            for width in [1, 4097, 8195] {
                let rows: Vec<Vec<i32>> = (0..decode).map(|row| (0..width).map(|lane| lane as i32 - row as i32).collect()).collect();
                let ids: Vec<u32> = (0..decode).map(|i| i % width).collect();
                for scheme in [flat_logits_scheme_id_v1(), tiled_logits_scheme_id_v1()] {
                    let mut trace = Trace::new(directory.path(), &context, scheme).unwrap();
                    for (index, row) in rows.iter().enumerate() {
                        trace.row(index as u32, row).unwrap();
                    }
                    let (actual, rows_root) = trace.finish(&ids).unwrap();
                    let expected = if scheme == flat_logits_scheme_id_v1() {
                        kaspa_consensus_core::palw_step_refute::base0_logits_trace_root_v1(&context, &rows, &ids)
                    } else {
                        kaspa_consensus_core::palw_step_refute::tiled_logits_trace_root_v1(&context, &rows, &ids).unwrap()
                    };
                    assert_eq!(actual, expected);
                    assert_eq!(rows_root.is_some(), scheme == tiled_logits_scheme_id_v1());
                }
            }
        }
    }

    #[test]
    fn independent_stream_matches_materialized_original_run_without_copying_accused_roots() {
        use crate::{TirParams, TirPlan};
        use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
        use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1;
        let directory = tempfile::tempdir().unwrap();
        for (_, original_program, params) in test_common::programs().into_iter().take(3) {
            for tiled in [false, true] {
                let program =
                    if tiled { test_common::tiled(original_program.clone()) } else { test_common::flat(original_program.clone()) };
                let plan = TirPlan::compile(&program).unwrap();
                let params = TirParams::from_map(&plan, &params).unwrap();
                let class = PalwTirClassV1 {
                    version: PALW_TIR_CLASS_VERSION_V1,
                    program: program.encode(),
                    layout: test_common::layout(&program, 7, 2, 2, 64),
                    tokenizer_id: Hash64::from_u64_word(9),
                };
                let artifact = Hash64::from_u64_word(10);
                let class_id = class.class_id(&artifact);
                let space = PalwTirStepSpaceV1::new(&class).unwrap();
                let prompt: Vec<u32> = (0..3).map(|i| i % program.token_bound).collect();
                let context = test_common::job(
                    &program,
                    3,
                    4,
                    class_id,
                    &prompt,
                    kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                );
                // Deliberately not the attempt's implicit greedy selector. Streaming
                // must preserve whatever exact original-job rule the backend supplied.
                let runner = TirClassRunnerV1::new(&space, &plan, &params, class_id)
                    .unwrap()
                    .with_selector(std::sync::Arc::new(|_, row, index| index % row.len() as u32));
                let run = runner.run(&context, &prompt, u64::MAX, false, &mut |_| {}).unwrap();
                let original = PalwPublicTirBindingHeaderV1 {
                    version: 1,
                    job_context: context,
                    artifact_root: artifact,
                    step_leaf_count: run.leaf_count,
                    step_merkle_root: Hash64::from_u64_word(91),
                    full_logits_trace_root: Hash64::from_u64_word(92),
                    committed_execution_root: Hash64::from_u64_word(93),
                };
                let source = OwnStepSource::build(directory.path(), &original, &runner, run.leaf_count, u64::MAX, &prompt).unwrap();
                assert_eq!(source.info.generated_ids, run.generated);
                assert_eq!(source.info.header.step_merkle_root, run.step_merkle_root);
                assert_eq!(source.info.header.full_logits_trace_root, run.trace_root);
                assert_eq!(source.info.header.committed_execution_root, run.execution_root);
                assert_eq!(source.info.output_root, run.output_root);
                let count = u32::try_from(run.leaf_count.min(1024)).unwrap();
                assert_eq!(source.leaf_hashes_v1(0, count).unwrap(), run.leaf_hashes[..count as usize]);
                assert!(source.leaf_hashes_v1(0, 1025).is_err());
            }
        }
        assert!(std::fs::read_dir(directory.path()).unwrap().next().is_none());
    }
}
