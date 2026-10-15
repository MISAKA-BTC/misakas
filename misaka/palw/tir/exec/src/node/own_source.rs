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
                if tiled {
                    use super::super::evidence::{TirEvidenceV1, TirTraceV1};
                    use super::super::inventory::TirParamOpenerV1;
                    use kaspa_consensus_core::palw_artifact::{PalwArtifactMultiproofV1, PalwArtifactOpeningV1};
                    struct NoParams;
                    impl TirParamOpenerV1 for NoParams {
                        fn param_opening(&self, _: u32) -> Option<PalwArtifactOpeningV1> {
                            None
                        }
                        fn param_multiproof(&self, _: &[u32]) -> Option<PalwArtifactMultiproofV1> {
                            None
                        }
                    }
                    let binding = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1 {
                        version: 1,
                        job_context: original.job_context.clone(),
                        class: class.clone(),
                        artifact_root: artifact,
                        full_logits_trace_root: run.trace_root,
                        step_leaf_count: run.leaf_count,
                        step_merkle_root: run.step_merkle_root,
                        committed_execution_root: run.execution_root,
                    };
                    let inputs = super::super::cell::TirRunInputsV1::default();
                    let mut wrong_width = run.logits_rows.clone();
                    wrong_width[0].push(0);
                    let rows = TirTraceV1::Rows { rows: &wrong_width, generated: &run.generated };
                    let error = TirEvidenceV1::public_runs_v1(
                        &runner,
                        &binding,
                        &prompt,
                        rows,
                        &inputs,
                        &NoParams,
                        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                        u64::MAX,
                    )
                    .err()
                    .unwrap();
                    assert!(error.contains("registered logits width"));
                    let mut pin = kaspa_consensus_core::palw_step_refute::tiled_decode_pin_v1(
                        &binding.job_context,
                        &run.logits_rows,
                        &run.generated,
                        0,
                        0,
                    )
                    .unwrap();
                    pin.row_root = Hash64::from_u64_word(991);
                    let pins = [pin];
                    let summary =
                        TirTraceV1::Summary { rows_root: source.info.rows_root.unwrap(), generated: &run.generated, pins: &pins };
                    let error = TirEvidenceV1::public_runs_v1(
                        &runner,
                        &binding,
                        &prompt,
                        summary,
                        &inputs,
                        &NoParams,
                        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                        u64::MAX,
                    )
                    .err()
                    .unwrap();
                    assert!(error.contains("unauthenticated trace pins"));
                }
            }
        }
        assert!(std::fs::read_dir(directory.path()).unwrap().next().is_none());
    }

    #[test]
    fn public_sparse_originals_reach_native_cone_kernel_and_missing_never_acquits() {
        use super::super::{artifact::TirArtifactV1, backend::TirBackendV1};
        use kaspa_consensus_core::{
            palw_backend::{PalwExecutionBackendV1, PalwTirPublicConeNeedV1, PalwTirPublicConeProbeV1},
            palw_court_v2::PalwCourtVerdictProofV2,
            palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1},
            palw_mode_v2::PalwCourtParamsV2,
            palw_prompt_ids_v1::PalwPromptIdsFormV1,
            palw_step_refute::{PalwStepRefuteError, PalwTiledDecodeTokensV1, tiled_logits_rows_root_v1},
            palw_tir_court_v1::{PalwTirBuilderWorkLimitsV1, check_tir_cone_refutation_v1},
        };
        let directory = tempfile::tempdir().unwrap();
        let (name, original, params) = test_common::programs()
            .into_iter()
            .find(|(_, p, _)| !p.params.is_empty() && misaka_palw_tir::interval::analyze_ranges(p).is_ok())
            .unwrap();
        let program = test_common::tiled(original);
        let layout = test_common::layout(&program, 5, 2, 2, 64);
        let path = directory.path().join("public.palwtir");
        let mut tensor =
            |j: u16, l: Option<u16>| params.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or("missing fixture parameter".into());
        misaka_palw_tir_artifact::write_container_v1(
            &path,
            &program,
            borsh::to_vec(&layout).unwrap(),
            [2; 64],
            name.clone(),
            &mut tensor,
        )
        .unwrap();
        let artifact = std::sync::Arc::new(TirArtifactV1::open(&path).unwrap());
        let (root, _) = artifact.inventory_root().unwrap();
        let class = artifact.class().unwrap();
        let canonical =
            kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class.class_id(&root), (4, 3)).unwrap();
        let backend = TirBackendV1::new(name, artifact, root, canonical, PalwPromptIdsFormV1::Flat, 1 << 26).unwrap();
        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(31)).unwrap();
        let prompt_ids: Vec<u32> = prompt.iter().map(|id| *id as u32).collect();
        let rules = backend.court_rules(
            &PalwCourtParamsV2::new(kaspa_consensus_core::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT, 4, 2).unwrap(),
        );
        let limits = PalwTirBuilderWorkLimitsV1 { evaluation: rules.limits, max_target_requests: 1 << 20, max_source_reads: 1 << 24 };
        let copies = 1 << 28;
        for fault in [false, true] {
            // Producer fixture construction is separated from the verifier below.
            let produced = if fault {
                backend.execute_with_injected_fault(&job, &prompt, 3).unwrap()
            } else {
                backend.execute(&job, &prompt).unwrap()
            };
            let capture = super::super::backend::TirCaptureV1::decode(&produced.material).unwrap();
            let header = PalwPublicTirBindingHeaderV1::of_binding_v1(&capture.binding);
            let trace = PalwTiledDecodeTokensV1 {
                rows_root: tiled_logits_rows_root_v1(&job, &capture.logits_rows).unwrap(),
                generated_token_ids: capture.generated.clone(),
            };
            let mut public_runs = Vec::new();
            for first in (0..header.step_leaf_count).step_by(256) {
                let count = (header.step_leaf_count - first).min(256) as u32;
                let PalwDaAnswerV1::TirStepRun(run) =
                    backend.step_unit_answer(&produced.material, PalwDaUnitV1::TirStepRun { first, count }).unwrap()
                else {
                    panic!("fixture did not publish original run")
                };
                public_runs.push(*run);
            }
            // The actual verifier receives only public authenticated originals and
            // its registered artifact; it has no producer capture or private state.
            assert!(matches!(
                backend.probe_tir_public_cone_v1(&header, &prompt_ids, Some(&trace), &[], 3, &rules, limits, copies).unwrap(),
                PalwTirPublicConeProbeV1::Need(
                    PalwTirPublicConeNeedV1::StepPreimage { .. } | PalwTirPublicConeNeedV1::StepOpening { .. }
                )
            ));
            let (proof, _) = backend
                .build_tir_public_cone_bounded_v1(&header, &prompt_ids, Some(&trace), &public_runs, 3, &rules, limits, copies)
                .unwrap();
            let PalwCourtVerdictProofV2::TirCone { refutation } = proof else { panic!("not native cone") };
            let verdict = check_tir_cone_refutation_v1(&refutation, &rules);
            if fault {
                assert!(verdict.is_ok());
            } else {
                assert_eq!(verdict, Err(PalwStepRefuteError::NoFaultFound));
            }
            let mut tampered = public_runs.clone();
            tampered[0].range.leaf_hashes[0] = Hash64::from_u64_word(999);
            assert!(
                backend.probe_tir_public_cone_v1(&header, &prompt_ids, Some(&trace), &tampered, 3, &rules, limits, copies).is_err()
            );
            assert!(
                matches!(
                    backend.probe_tir_public_cone_v1(&header, &prompt_ids, Some(&trace), &public_runs, 3, &rules, limits, 0).unwrap(),
                    PalwTirPublicConeProbeV1::Refused { .. }
                ),
                "held operands refused by copy admission must not create a fresh DA duty"
            );
            let zero = PalwTirBuilderWorkLimitsV1 { evaluation: rules.limits, max_target_requests: 0, max_source_reads: 0 };
            assert!(matches!(
                backend.probe_tir_public_cone_v1(&header, &prompt_ids, Some(&trace), &[], 3, &rules, zero, copies).unwrap(),
                PalwTirPublicConeProbeV1::Refused { .. }
            ));
            if !fault {
                let mut found_decode = false;
                for index in 0..header.step_leaf_count {
                    let Ok((PalwCourtVerdictProofV2::TirCone { refutation }, _)) = backend.build_tir_public_cone_bounded_v1(
                        &header,
                        &prompt_ids,
                        Some(&trace),
                        &public_runs,
                        index,
                        &rules,
                        limits,
                        copies,
                    ) else {
                        continue;
                    };
                    if refutation.decode_tokens.is_none() {
                        continue;
                    }
                    found_decode = true;
                    assert!(matches!(
                        backend
                            .probe_tir_public_cone_v1(&header, &prompt_ids, None, &public_runs, index, &rules, limits, copies)
                            .unwrap(),
                        PalwTirPublicConeProbeV1::Need(PalwTirPublicConeNeedV1::DecodePin)
                    ));
                    let mut missing = *refutation;
                    missing.decode_tokens = None;
                    assert_ne!(check_tir_cone_refutation_v1(&missing, &rules), Err(PalwStepRefuteError::NoFaultFound));
                    break;
                }
                assert!(found_decode, "fixture must exercise an actual generated-token dependency");
                let mut invalid_ids = trace.clone();
                *invalid_ids.generated_token_ids.last_mut().unwrap() = u32::MAX;
                let mut invalid_header = header.clone();
                invalid_header.full_logits_trace_root = tiled_logits_outer_root_v1(
                    &job,
                    u64::from(job.exact_decode_tokens),
                    &invalid_ids.rows_root,
                    &invalid_ids.generated_token_ids,
                );
                invalid_header.committed_execution_root = palw_tir_execution_root_v1(
                    &job.context_hash(),
                    &invalid_header.full_logits_trace_root,
                    &job.shape_profile_id,
                    header.step_leaf_count,
                    &header.step_merkle_root,
                );
                let refused = backend
                    .probe_tir_public_cone_v1(&invalid_header, &prompt_ids, Some(&invalid_ids), &[], 3, &rules, limits, copies)
                    .unwrap_err();
                assert!(refused.contains("separate output-validity predicate"));
            }
        }
    }
}
