//! Bounded projection of retained ORIGINAL TIR capture bytes. Anonymous indexes
//! are disposable acceleration, not verifier credentials or restart authority.
use super::own_source::{DiskTree, Trace, tree_disk_bytes_v1};
use kaspa_consensus_core::{
    Hash64,
    palw_backend::{PalwOriginalCaptureReaderV1, PalwTirPublicCaptureNeedV1, PalwTirPublicCaptureReaderV1},
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1},
    palw_step::PALW_STEP_MAX_TILE_LEN,
    palw_step_leg::{PalwStepTileLeafV1, step_tile_leaf_hash_v1},
    palw_step_refute::{
        PALW_BASE0_DOMAIN_LOGITS_TRACE, PALW_LOGITS_TILE_LANES, tiled_logits_outer_root_v1, tiled_logits_tile_leaf_v1,
    },
    palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Request, PalwTirPublicResponseV1 as Response, PalwTirPublicSourceV1},
    palw_tir_step_v1::PalwTirStepBindingV1,
};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{Arc, Mutex},
};

const READ: u32 = 16_384;
const MAX_LEAF: u64 = PALW_STEP_MAX_TILE_LEN as u64 * 4;
const LEAF_HEAD: u64 = 26;

pub(crate) fn need_v1(
    binding: &PalwTirStepBindingV1,
    public: &PalwTirPublicSourceV1,
    length: u64,
    replay_ram: u64,
) -> Result<PalwTirPublicCaptureNeedV1, String> {
    let n = public.leaf_count_v1();
    let d = u64::from(public.decode_count_v1());
    let tiles = u64::from(public.vocab_v1()).div_ceil(PALW_LOGITS_TILE_LANES as u64);
    let forest = if public.supports_tiled_trace_v1() {
        tree_disk_bytes_v1(tiles)?
            .checked_mul(d)
            // Original rows tree plus the independently recomputed fold trace.
            .and_then(|n| n.checked_add(tree_disk_bytes_v1(d).ok()?.checked_mul(2)?))
            .ok_or("TIR capture forest quote overflow")?
    } else {
        0
    };
    let disk_bytes = tree_disk_bytes_v1(n)?.checked_add(forest)
        .and_then(|b| b.checked_add(n.checked_mul(8)?))
        // Reserved before opening even when a dense capture needs no replay spool.
        .and_then(|b| b.checked_add(n.checked_mul(MAX_LEAF + LEAF_HEAD)?))
        .ok_or("TIR capture disk quote overflow")?;
    let metadata = borsh::object_length(binding).map_err(|e| e.to_string())? as u64;
    let resident_ram_bytes = metadata
        .checked_mul(8)
        .and_then(|b| b.checked_add(u64::from(binding.job_context.declared_prefill_tokens).checked_mul(16)?))
        .and_then(|b| b.checked_add(d.checked_mul(32)?))
        // Current continuation, restored executor and replacement state coexist.
        .and_then(|b| b.checked_add(replay_ram.checked_mul(2)?))
        .and_then(|b| b.checked_add(2_097_152))
        .ok_or("TIR capture RAM quote overflow")?;
    let maximum_prepare_io_bytes = length
        .checked_mul(2)
        .and_then(|n| n.checked_add(disk_bytes.checked_mul(8)?))
        .and_then(|n| n.checked_add(metadata.checked_mul(4)?))
        .ok_or("TIR capture IO quote overflow")?;
    let positions = u64::from(binding.job_context.declared_prefill_tokens)
        .checked_add(d)
        .and_then(|n| n.checked_sub(1))
        .ok_or("TIR replay position quote overflow")?;
    let maximum_replay_id_words =
        positions.checked_add(1).and_then(|n| n.checked_mul(d)).and_then(|n| n.checked_mul(4)).ok_or("TIR replay ID work overflow")?;
    Ok(PalwTirPublicCaptureNeedV1 {
        resident_ram_bytes,
        disk_bytes,
        maximum_prepare_io_bytes,
        maximum_replay_leaves: n,
        maximum_replay_positions: positions,
        maximum_replay_id_words,
        // Up to256 raw leaves or1024 frontier children, each read verified
        // through at most64 native Merkle levels (including path siblings).
        maximum_response_io_bytes: 256 * (MAX_LEAF + LEAF_HEAD) + 4 * 1024 * 64 * 64,
        maximum_response_tree_hashes: 4 * 1024 * 64,
    })
}

#[derive(Clone, Copy)]
enum Phase {
    Prefix(usize),
    PromptCount,
    Prompt,
    RowCount,
    Rows,
    IdCount,
    Ids,
    LeavesCount,
    Leaves,
    Replay,
    Ready,
    Failed,
}

pub(crate) struct CaptureReader {
    source: Arc<dyn PalwOriginalCaptureReaderV1>,
    binding: PalwTirStepBindingV1,
    public: PalwTirPublicSourceV1,
    expected: Vec<u8>,
    form: PalwPromptIdsFormV1,
    at: u64,
    phase: Phase,
    prompt: Vec<u32>,
    ids: Vec<u32>,
    row: u32,
    tile: u32,
    rows_start: u64,
    row_started: bool,
    rows_root: Option<Hash64>,
    flat: Option<blake2b_simd::State>,
    steps: DiskTree,
    rows: Option<DiskTree>,
    forest: Option<File>,
    tile_stride: u64,
    active_tiles: Option<DiskTree>,
    offsets: File,
    leaf: u64,
    replay: Option<FoldReplayV1>,
    replay_point: Option<super::run::TirResumePointV1>,
    replay_ids: Option<Arc<[u32]>>,
    replay_trace: Option<Trace>,
    spool: Option<Arc<Spool>>,
    directory: std::path::PathBuf,
}

pub(crate) struct FoldReplayV1 {
    pub(crate) artifact: Arc<super::artifact::TirArtifactV1>,
    pub(crate) space: kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1,
    pub(crate) fused: bool,
    pub(crate) cap: u64,
}
struct Spool(Mutex<(File, u64)>);
impl Spool {
    fn append(&self, leaf: &PalwStepTileLeafV1) -> Result<u64, String> {
        let mut held = self.0.lock().map_err(|_| "TIR spool poisoned")?;
        let at = held.1;
        held.0.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
        borsh::to_writer(&mut held.0, leaf).map_err(|e| e.to_string())?;
        held.1 = at.checked_add(LEAF_HEAD + leaf.values_le.len() as u64).ok_or("TIR spool overflow")?;
        Ok(at)
    }
}
impl PalwOriginalCaptureReaderV1 for Spool {
    fn len_v1(&self) -> u64 {
        self.0.lock().map(|held| held.1).unwrap_or(0)
    }
    fn read_v1(&self, at: u64, length: u32) -> Result<Vec<u8>, String> {
        let mut held = self.0.lock().map_err(|_| "TIR spool poisoned")?;
        if length > READ || at.checked_add(u64::from(length)).is_none_or(|end| end > held.1) {
            return Err("TIR spool read outside canonical original leaves".into());
        }
        let mut bytes = vec![0; length as usize];
        held.0.seek(SeekFrom::Start(at)).and_then(|_| held.0.read_exact(&mut bytes)).map_err(|e| e.to_string())?;
        Ok(bytes)
    }
}

impl CaptureReader {
    pub(crate) fn open_v1(
        source: Arc<dyn PalwOriginalCaptureReaderV1>,
        binding: PalwTirStepBindingV1,
        public: PalwTirPublicSourceV1,
        form: PalwPromptIdsFormV1,
        directory: &Path,
        replay: Option<FoldReplayV1>,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let n = public.leaf_count_v1();
        let mut expected = super::backend::TIR_CAPTURE_MAGIC_V1.to_vec();
        borsh::to_writer(&mut expected, &binding).map_err(|e| e.to_string())?;
        let tiled = public.supports_tiled_trace_v1();
        let d = u64::from(public.decode_count_v1());
        let tiles = u64::from(public.vocab_v1()).div_ceil(PALW_LOGITS_TILE_LANES as u64);
        let tile_stride = tree_disk_bytes_v1(tiles)?;
        let forest = if tiled {
            let file = tempfile::tempfile_in(directory).map_err(|e| e.to_string())?;
            file.set_len(tile_stride.checked_mul(d).ok_or("TIR forest size overflow")?).map_err(|e| e.to_string())?;
            Some(file)
        } else {
            None
        };
        let rows = if tiled { Some(DiskTree::new(directory, d)?) } else { None };
        let offsets = tempfile::tempfile_in(directory).map_err(|e| e.to_string())?;
        offsets.set_len(n.checked_mul(8).ok_or("TIR offset index overflow")?).map_err(|e| e.to_string())?;
        let flat = if tiled {
            None
        } else {
            let mut hash = blake2b_simd::Params::new().hash_length(64).key(PALW_BASE0_DOMAIN_LOGITS_TRACE).to_state();
            hash.update(binding.job_context.context_hash().as_byte_slice());
            for size in [binding.job_context.declared_prefill_tokens, public.decode_count_v1(), public.decode_count_v1()] {
                hash.update(&u64::from(size).to_le_bytes());
            }
            Some(hash)
        };
        Ok(Self {
            source,
            binding,
            public,
            expected,
            form,
            at: 0,
            phase: Phase::Prefix(0),
            prompt: Vec::new(),
            ids: Vec::new(),
            row: 0,
            tile: 0,
            rows_start: 0,
            row_started: false,
            rows_root: None,
            flat,
            steps: DiskTree::new(directory, n)?,
            rows,
            forest,
            tile_stride,
            active_tiles: None,
            offsets,
            leaf: 0,
            replay,
            replay_point: None,
            replay_ids: None,
            replay_trace: None,
            spool: None,
            directory: directory.to_path_buf(),
        })
    }

    fn read(&self, at: u64, length: u32) -> Result<Vec<u8>, String> {
        if length > READ || at.checked_add(u64::from(length)).is_none_or(|end| end > self.source.len_v1()) {
            return Err("TIR original read exceeds bounded exact capture range".into());
        }
        let bytes = self.source.read_v1(at, length)?;
        if bytes.len() != length as usize {
            return Err("TIR original capture returned a short range".into());
        }
        Ok(bytes)
    }
    fn word(&mut self) -> Result<u32, String> {
        let word = u32::from_le_bytes(self.read(self.at, 4)?.try_into().expect("four"));
        self.at = self.at.checked_add(4).ok_or("capture offset overflow")?;
        Ok(word)
    }
    fn words(&self, at: u64, count: u32) -> Result<Vec<u32>, String> {
        let bytes = self.read(at, count.checked_mul(4).ok_or("TIR word read overflow")?)?;
        Ok(bytes.chunks_exact(4).map(|b| u32::from_le_bytes(b.try_into().expect("four"))).collect())
    }
    fn leaf_read(&self, at: u64, length: u32) -> Result<Vec<u8>, String> {
        match self.spool.as_ref() {
            Some(spool) => spool.read_v1(at, length),
            None => self.read(at, length),
        }
    }
    fn raw_leaf(&self, at: u64) -> Result<PalwStepTileLeafV1, String> {
        let bytes = self.leaf_read(at, LEAF_HEAD as u32)?;
        let mut wire = bytes.as_slice();
        use borsh::BorshDeserialize;
        let version = u16::deserialize_reader(&mut wire).map_err(|e| e.to_string())?;
        let coord = kaspa_consensus_core::palw_step::PalwStepCoordinateV1::deserialize_reader(&mut wire).map_err(|e| e.to_string())?;
        let value_count = u32::deserialize_reader(&mut wire).map_err(|e| e.to_string())?;
        let length = u32::deserialize_reader(&mut wire).map_err(|e| e.to_string())?;
        if u64::from(length) > MAX_LEAF {
            return Err("TIR original leaf exceeds format before allocation".into());
        }
        let mut values_le = Vec::new();
        values_le.try_reserve_exact(length as usize).map_err(|e| e.to_string())?;
        let start = at.checked_add(LEAF_HEAD).ok_or("TIR leaf offset overflow")?;
        while values_le.len() < length as usize {
            let part = (length as usize - values_le.len()).min(READ as usize) as u32;
            values_le.extend(self.leaf_read(start + values_le.len() as u64, part)?);
        }
        Ok(PalwStepTileLeafV1 { version, coord, value_count, values_le })
    }

    fn advance(&mut self) -> Result<(), String> {
        match self.phase {
            Phase::Prefix(first) => {
                let count = (self.expected.len() - first).min(READ as usize);
                if self.read(self.at, count as u32)? != self.expected[first..first + count] {
                    return Err("retained TIR capture changes original authenticated binding".into());
                }
                self.at += count as u64;
                self.phase = if first + count == self.expected.len() {
                    self.expected.clear();
                    Phase::PromptCount
                } else {
                    Phase::Prefix(first + count)
                };
            }
            Phase::PromptCount => {
                if self.word()? != self.binding.job_context.declared_prefill_tokens {
                    return Err("TIR capture prompt count differs".into());
                }
                self.phase = Phase::Prompt;
            }
            Phase::Prompt => {
                let remaining = self.binding.job_context.declared_prefill_tokens as usize - self.prompt.len();
                let count = remaining.min(READ as usize / 4) as u32;
                let words = self.words(self.at, count)?;
                self.prompt.extend(words);
                self.at += u64::from(count) * 4;
                if self.prompt.len() == self.binding.job_context.declared_prefill_tokens as usize {
                    if !prompt_token_ids_match_v1(self.form, &self.prompt, &self.binding.job_context.prompt_token_ids_hash) {
                        return Err("TIR original public prompt hash differs".into());
                    }
                    self.phase = Phase::RowCount;
                }
            }
            Phase::RowCount => {
                if self.word()? != self.public.decode_count_v1() {
                    return Err("TIR capture decode row count differs".into());
                }
                self.rows_start = self.at;
                self.phase = Phase::Rows;
            }
            Phase::Rows => {
                if !self.row_started {
                    if self.word()? != self.public.vocab_v1() {
                        return Err("TIR original row changes registered vocabulary width".into());
                    }
                    if let Some(hash) = &mut self.flat {
                        hash.update(&u64::from(self.public.vocab_v1()).to_le_bytes());
                    }
                    if let Some(file) = &self.forest {
                        self.active_tiles = Some(DiskTree::slice(
                            file,
                            u64::from(self.row) * self.tile_stride,
                            u64::from(self.public.vocab_v1()).div_ceil(PALW_LOGITS_TILE_LANES as u64),
                            None,
                        )?);
                    }
                    self.row_started = true;
                }
                let count = (self.public.vocab_v1() - self.tile * PALW_LOGITS_TILE_LANES as u32).min(PALW_LOGITS_TILE_LANES as u32);
                let bytes = self.read(self.at, count * 4)?;
                if let Some(hash) = &mut self.flat {
                    hash.update(&bytes);
                }
                if let Some(tree) = &mut self.active_tiles {
                    let lanes: Vec<i32> = bytes.chunks_exact(4).map(|b| i32::from_le_bytes(b.try_into().expect("four"))).collect();
                    tree.push(tiled_logits_tile_leaf_v1(&self.binding.job_context.context_hash(), self.row, self.tile, &lanes))?;
                }
                self.at += u64::from(count) * 4;
                self.tile += 1;
                if u64::from(self.tile) * PALW_LOGITS_TILE_LANES as u64 >= u64::from(self.public.vocab_v1()) {
                    if let Some(mut tree) = self.active_tiles.take() {
                        self.rows.as_mut().ok_or("rows tree absent")?.push(tree.finish()?)?;
                    }
                    self.row += 1;
                    self.tile = 0;
                    self.row_started = false;
                    if self.row == self.public.decode_count_v1() {
                        if let Some(tree) = &mut self.rows {
                            self.rows_root = Some(tree.finish()?);
                        }
                        self.phase = Phase::IdCount;
                    }
                }
            }
            Phase::IdCount => {
                if self.word()? != self.public.decode_count_v1() {
                    return Err("TIR original generated ID count differs".into());
                }
                self.phase = Phase::Ids;
            }
            Phase::Ids => {
                let count = (self.public.decode_count_v1() as usize - self.ids.len()).min(1024) as u32;
                let words = self.words(self.at, count)?;
                self.ids.extend(words);
                self.at += u64::from(count) * 4;
                if self.ids.len() == self.public.decode_count_v1() as usize {
                    let root = if let Some(mut hash) = self.flat.take() {
                        hash.update(&(self.ids.len() as u64).to_le_bytes());
                        for id in &self.ids {
                            hash.update(&id.to_le_bytes());
                        }
                        Hash64::from_bytes(hash.finalize().as_bytes().try_into().expect("64"))
                    } else {
                        tiled_logits_outer_root_v1(
                            &self.binding.job_context,
                            self.ids.len() as u64,
                            &self.rows_root.ok_or("rows root absent")?,
                            &self.ids,
                        )
                    };
                    if root != self.binding.full_logits_trace_root {
                        return Err("TIR retained rows/IDs do not reach original trace EOF".into());
                    }
                    self.phase = Phase::LeavesCount;
                }
            }
            Phase::LeavesCount => {
                let count = u64::from(self.word()?);
                if count == 0 {
                    if self.at != self.source.len_v1() {
                        return Err("folded original TIR capture has trailing bytes".into());
                    }
                    let replay =
                        self.replay.as_ref().ok_or("folded TIR capture requires a registered public model replay provider")?;
                    if self.prompt.iter().any(|id| *id >= replay.space.program.token_bound)
                        || self.ids.iter().take(self.ids.len().saturating_sub(1)).any(|id| *id >= replay.space.program.token_bound)
                    {
                        return Err("folded TIR original IDs need their separate output-validity predicate before replay".into());
                    }
                    self.replay_ids = Some(Arc::from(self.ids.clone()));
                    self.replay_trace = Some(Trace::new(
                        &self.directory,
                        &self.binding.job_context,
                        Hash64::from_bytes(replay.space.program.logits_scheme_id),
                    )?);
                    self.spool =
                        Some(Arc::new(Spool(Mutex::new((tempfile::tempfile_in(&self.directory).map_err(|e| e.to_string())?, 0)))));
                    self.phase = Phase::Replay;
                    return Ok(());
                }
                if count != self.public.leaf_count_v1() {
                    return Err("TIR dense capture changes original step count".into());
                }
                self.phase = Phase::Leaves;
            }
            Phase::Leaves => {
                let preimage = self.raw_leaf(self.at)?;
                self.offsets
                    .seek(SeekFrom::Start(self.leaf * 8))
                    .and_then(|_| self.offsets.write_all(&self.at.to_le_bytes()))
                    .map_err(|e| e.to_string())?;
                self.steps.push(step_tile_leaf_hash_v1(
                    &self.binding.job_context.context_hash(),
                    &self.binding.job_context.shape_profile_id,
                    &preimage,
                ))?;
                self.at = self.at.checked_add(LEAF_HEAD + preimage.values_le.len() as u64).ok_or("TIR leaf cursor overflow")?;
                self.leaf += 1;
                if self.leaf == self.public.leaf_count_v1() {
                    if self.at != self.source.len_v1() || self.steps.finish()? != self.binding.step_merkle_root {
                        return Err("TIR dense original step EOF/root differs".into());
                    }
                    self.prompt = Vec::new();
                    self.phase = Phase::Ready;
                }
            }
            Phase::Replay => self.replay_position()?,
            Phase::Ready => {}
            Phase::Failed => return Err("original TIR source previously failed; reopen exact original range".into()),
        }
        Ok(())
    }

    fn replay_position(&mut self) -> Result<(), String> {
        let replay = self.replay.as_ref().ok_or("TIR public replay provider absent")?;
        let ids = self.replay_ids.as_ref().ok_or("TIR original IDs not sealed")?.clone();
        let runner = super::run::TirClassRunnerV1::new(
            &replay.space,
            replay.artifact.plan(),
            replay.artifact.params(),
            self.binding.job_context.shape_profile_id,
        )?
        .with_fused(replay.fused)
        .with_selector(Arc::new(move |_, _, index| ids[index as usize]));
        let spool = self.spool.as_ref().ok_or("TIR replay spool absent")?;
        let (steps, offsets, count) = (&mut self.steps, &mut self.offsets, &mut self.leaf);
        let trace = self.replay_trace.as_mut().ok_or("TIR replay trace absent")?;
        let mut failed = None;
        let point = runner.replay_streamed_position_v1(
            &self.binding.job_context,
            &self.prompt,
            replay.cap,
            self.replay_point.as_ref(),
            &mut |leaf| {
                if failed.is_some() {
                    return;
                }
                let result = (|| {
                    if leaf.index != *count || leaf.preimage.values_le.len() as u64 > MAX_LEAF {
                        return Err("TIR replay changed original leaf order or format".into());
                    }
                    let at = spool.append(leaf.preimage)?;
                    offsets
                        .seek(SeekFrom::Start(*count * 8))
                        .and_then(|_| offsets.write_all(&at.to_le_bytes()))
                        .map_err(|e| e.to_string())?;
                    steps.push(leaf.hash)?;
                    *count += 1;
                    Ok::<_, String>(())
                })();
                if let Err(error) = result {
                    failed = Some(error);
                }
            },
            &mut |row, lanes| trace.row(row, lanes),
        )?;
        if let Some(error) = failed {
            return Err(error);
        }
        if point.generated.len() > self.ids.len() || point.generated != self.ids[..point.generated.len()] {
            return Err("TIR public replay changed authenticated original ID prefix".into());
        }
        let positions = replay.space.job_shape(&self.binding.job_context).map_err(|e| e.to_string())?.positions;
        if point.position.checked_add(1) == Some(positions) {
            let (trace_root, _) = trace.finish(&self.ids)?;
            let step_root = steps.finish()?;
            let execution_root = kaspa_consensus_core::palw_tir_step_v1::palw_tir_execution_root_v1(
                &self.binding.job_context.context_hash(),
                &trace_root,
                &self.binding.job_context.shape_profile_id,
                *count,
                &step_root,
            );
            if *count != self.public.leaf_count_v1()
                || trace_root != self.binding.full_logits_trace_root
                || step_root != self.binding.step_merkle_root
                || execution_root != self.binding.committed_execution_root
            {
                return Err("public reconstructed fold does not match ORIGINAL trace/step/execution roots".into());
            }
            self.prompt = Vec::new();
            self.replay_point = None;
            self.replay_ids = None;
            self.replay_trace = None;
            self.replay = None;
            self.phase = Phase::Ready;
        } else {
            self.replay_point = Some(point);
        }
        Ok(())
    }

    fn preimage(&mut self, index: u64) -> Result<PalwStepTileLeafV1, String> {
        let mut bytes = [0; 8];
        self.offsets.seek(SeekFrom::Start(index * 8)).and_then(|_| self.offsets.read_exact(&mut bytes)).map_err(|e| e.to_string())?;
        let preimage = self.raw_leaf(u64::from_le_bytes(bytes))?;
        if step_tile_leaf_hash_v1(&self.binding.job_context.context_hash(), &self.binding.job_context.shape_profile_id, &preimage)
            != self.steps.checked(0, index)?.1
        {
            return Err("retained TIR preimage changed after original-root indexing".into());
        }
        Ok(preimage)
    }
    fn tile_tree(&mut self, row: u32) -> Result<(Hash64, kaspa_consensus_core::palw_step_leg::PalwStepOpeningV1, DiskTree), String> {
        let rows = self.rows.as_mut().ok_or("original Flat trace has no row/tile tree")?;
        let row_root = rows.checked(0, u64::from(row))?.1;
        let opening = rows.opening(u64::from(row))?;
        let tree = DiskTree::slice(
            self.forest.as_ref().ok_or("original tile forest absent")?,
            u64::from(row) * self.tile_stride,
            u64::from(self.public.vocab_v1()).div_ceil(PALW_LOGITS_TILE_LANES as u64),
            Some(row_root),
        )?;
        Ok((row_root, opening, tree))
    }
    fn answer(&mut self, request: &Request) -> Result<Response, String> {
        let bound = self.public.unit_bound_v1(request)?.maximum_response_bytes;
        let response = match *request {
            Request::StepLeaf { index } => Response::StepLeaf { preimage: self.preimage(index)?, opening: self.steps.opening(index)? },
            Request::StepRun { first, count } => {
                let range = self.steps.range(first, count)?;
                let mut used = 5 + borsh::object_length(&range).map_err(|e| e.to_string())? as u64;
                let mut preimages = Vec::new();
                for index in first..first + u64::from(count) {
                    // One format-bounded leaf scratch is part of preparation RAM.
                    // Never accumulate malformed leaves beyond this native unit.
                    let preimage = self.preimage(index)?;
                    used = used.checked_add(LEAF_HEAD + preimage.values_le.len() as u64).ok_or("TIR response size overflow")?;
                    if used > bound {
                        return Err("original TIR run exceeds native requested unit allocation".into());
                    }
                    preimages.push(preimage);
                }
                Response::StepRun { preimages, range }
            }
            Request::StepNode { level, index } => {
                Response::StepNode { frontier: self.steps.frontier(level, index)?, siblings: self.steps.siblings(level, index)? }
            }
            Request::RowsNode { level, index } => {
                let tree = self.rows.as_mut().ok_or("rows tree absent")?;
                Response::RowsNode { frontier: tree.frontier(level, index)?, siblings: tree.siblings(level, index)? }
            }
            Request::TileNode { row, level, index } => {
                let (row_root, row_opening, mut tree) = self.tile_tree(row)?;
                Response::TileNode {
                    row_root,
                    row_opening,
                    frontier: tree.frontier(level, index)?,
                    siblings: tree.siblings(level, index)?,
                }
            }
            Request::TraceTile { row, tile } => {
                let (row_root, row_opening, mut tree) = self.tile_tree(row)?;
                let first = u64::from(tile) * PALW_LOGITS_TILE_LANES as u64;
                let count = (u64::from(self.public.vocab_v1()) - first).min(PALW_LOGITS_TILE_LANES as u64) as u32;
                let at = self.rows_start + u64::from(row) * (4 + u64::from(self.public.vocab_v1()) * 4) + 4 + first * 4;
                let lanes: Vec<i32> = self.words(at, count)?.into_iter().map(|word| word as i32).collect();
                if tiled_logits_tile_leaf_v1(&self.binding.job_context.context_hash(), row, tile, &lanes)
                    != tree.checked(0, u64::from(tile))?.1
                {
                    return Err("retained TIR tile changed after original-root indexing".into());
                }
                Response::TraceTile { row_root, row_opening, lanes, opening: tree.opening(u64::from(tile))? }
            }
            Request::Ids { offset } => Response::Ids {
                rows_root: self.rows_root.ok_or("original trace has no tiled ID source")?,
                ids: self.ids[offset as usize..(offset as usize + 1024).min(self.ids.len())].to_vec(),
            },
        };
        if borsh::object_length(&response).map_err(|e| e.to_string())? as u64 > bound {
            return Err("original TIR response exceeds native requested unit allocation".into());
        }
        Ok(response)
    }
}
impl PalwTirPublicCaptureReaderV1 for CaptureReader {
    fn poll_v1(&mut self, request: &Request) -> Result<Option<Response>, String> {
        self.public.check_request_v1(request)?;
        if !matches!(self.phase, Phase::Ready) {
            if let Err(error) = self.advance() {
                self.phase = Phase::Failed;
                return Err(error);
            }
            return Ok(None);
        }
        self.answer(request).map(Some)
    }
}

#[cfg(test)]
#[path = "../../tests/node_common/mod.rs"]
mod test_common;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    };
    struct Original {
        bytes: Mutex<Vec<u8>>,
        largest: AtomicU32,
    }
    impl PalwOriginalCaptureReaderV1 for Original {
        fn len_v1(&self) -> u64 {
            self.bytes.lock().unwrap().len() as u64
        }
        fn read_v1(&self, at: u64, length: u32) -> Result<Vec<u8>, String> {
            self.largest.fetch_max(length, Ordering::SeqCst);
            if length > READ {
                return Err("oversized retained read".into());
            }
            self.bytes
                .lock()
                .unwrap()
                .get(at as usize..at as usize + length as usize)
                .map(|v| v.to_vec())
                .ok_or("short original".into())
        }
    }
    fn fixture(tiled: bool) -> (PalwTirStepBindingV1, PalwTirPublicSourceV1, Arc<Original>) {
        use kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1;
        let (_, original, params) = test_common::programs()
            .into_iter()
            .find(|(_, p, _)| !p.params.is_empty() && misaka_palw_tir::interval::analyze_ranges(p).is_ok())
            .unwrap();
        let program = if tiled { test_common::tiled(original) } else { test_common::flat(original) };
        let class = PalwTirClassV1 {
            version: 1,
            program: program.encode(),
            layout: test_common::layout(&program, 5, 2, 2, 64),
            tokenizer_id: Hash64::from_u64_word(9),
        };
        let artifact = Hash64::from_u64_word(10);
        let class_id = class.class_id(&artifact);
        let space = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(&class).unwrap();
        let plan = crate::TirPlan::compile(&program).unwrap();
        let params = crate::TirParams::from_map(&plan, &params).unwrap();
        let prompt: Vec<u32> = (0..3).map(|i| i % program.token_bound).collect();
        let job = test_common::job(&program, 3, 4, class_id, &prompt, PalwPromptIdsFormV1::Flat);
        let runner = super::super::run::TirClassRunnerV1::new(&space, &plan, &params, class_id).unwrap();
        let mut leaves = Vec::new();
        let run = runner.run(&job, &prompt, 1 << 26, false, &mut |leaf| leaves.push(leaf.preimage.clone())).unwrap();
        let binding = PalwTirStepBindingV1 {
            version: 1,
            job_context: job,
            class: class.clone(),
            artifact_root: artifact,
            full_logits_trace_root: run.trace_root,
            step_leaf_count: run.leaf_count,
            step_merkle_root: run.step_merkle_root,
            committed_execution_root: run.execution_root,
        };
        let header = kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&binding);
        let (registered, _) = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(&class, &artifact).unwrap();
        let public =
            PalwTirPublicSourceV1::new_v1(&registered, &class.layout, &header, run.execution_root, run.trace_root, 1 << 26).unwrap();
        let capture = super::super::backend::TirCaptureV1 {
            binding: binding.clone(),
            prompt,
            logits_rows: run.logits_rows,
            generated: run.generated,
            leaves,
        };
        (binding, public, Arc::new(Original { bytes: Mutex::new(capture.encode()), largest: AtomicU32::new(0) }))
    }
    fn ready(reader: &mut CaptureReader, request: &Request) -> Response {
        for _ in 0..1_000_000 {
            if let Some(answer) = reader.poll_v1(request).unwrap() {
                return answer;
            }
        }
        panic!("bounded fixture preparation did not finish");
    }
    #[test]
    fn bounded_original_capture_projects_raw_units_and_reauthenticates_mutated_bytes() {
        use kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicIdsAssemblerV1;
        for tiled in [false, true] {
            let (binding, public, original) = fixture(tiled);
            let directory = tempfile::tempdir().unwrap();
            let need = need_v1(&binding, &public, original.len_v1(), 0).unwrap();
            assert!(need.disk_bytes > binding.step_leaf_count * 8);
            assert!(need.maximum_prepare_io_bytes >= original.len_v1());
            let mut reader = CaptureReader::open_v1(
                original.clone(),
                binding.clone(),
                public.clone(),
                PalwPromptIdsFormV1::Flat,
                directory.path(),
                None,
            )
            .unwrap();
            let request = Request::StepLeaf { index: 0 };
            let response = ready(&mut reader, &request);
            public.authenticate_v1(&request, &response, None).unwrap();
            let count = public.leaf_count_v1().min(13) as u32;
            let run = Request::StepRun { first: 0, count };
            public.authenticate_v1(&run, &reader.poll_v1(&run).unwrap().unwrap(), None).unwrap();
            if public.leaf_count_v1() > 1 {
                let node = Request::StepNode { level: (64 - (public.leaf_count_v1() - 1).leading_zeros()) as u8, index: 0 };
                public.authenticate_v1(&node, &reader.poll_v1(&node).unwrap().unwrap(), None).unwrap();
            }
            if tiled {
                let mut ids = PalwTirPublicIdsAssemblerV1::default();
                let q = Request::Ids { offset: 0 };
                let sealed = ids.push_v1(&public, &q, &reader.poll_v1(&q).unwrap().unwrap()).unwrap().unwrap();
                for q in [Request::RowsNode { level: 2, index: 0 }, Request::TraceTile { row: 1, tile: 0 }] {
                    public.authenticate_v1(&q, &reader.poll_v1(&q).unwrap().unwrap(), Some(&sealed)).unwrap();
                }
                let at = reader.rows_start + 4;
                original.bytes.lock().unwrap()[at as usize] ^= 1;
                assert!(reader.poll_v1(&Request::TraceTile { row: 0, tile: 0 }).is_err());
            }
            let mut offset = [0; 8];
            reader.offsets.seek(SeekFrom::Start(0)).unwrap();
            reader.offsets.read_exact(&mut offset).unwrap();
            original.bytes.lock().unwrap()[(u64::from_le_bytes(offset) + LEAF_HEAD) as usize] ^= 1;
            assert!(reader.poll_v1(&request).is_err());
            assert!(original.largest.load(Ordering::SeqCst) <= READ);
            drop(reader);
            assert!(std::fs::read_dir(directory.path()).unwrap().next().is_none());
        }
    }
    #[test]
    fn changed_header_truncation_and_overflow_refuse_without_a_credential() {
        let (binding, public, original) = fixture(true);
        let directory = tempfile::tempdir().unwrap();
        original.bytes.lock().unwrap()[8] ^= 1;
        let mut reader = CaptureReader::open_v1(
            original.clone(),
            binding.clone(),
            public.clone(),
            PalwPromptIdsFormV1::Flat,
            directory.path(),
            None,
        )
        .unwrap();
        assert!(reader.poll_v1(&Request::StepLeaf { index: 0 }).is_err());
        assert!(need_v1(&binding, &public, u64::MAX, 0).is_err());
        original.bytes.lock().unwrap().truncate(8);
        let mut reader = CaptureReader::open_v1(original, binding, public, PalwPromptIdsFormV1::Flat, directory.path(), None).unwrap();
        assert!(reader.poll_v1(&Request::StepLeaf { index: 0 }).is_err());
    }
}
