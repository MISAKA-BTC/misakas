//! Bounded public cell-run acquisition. Node policy only; refusal never creates a Valid receipt.
use super::{PALW_TIR_RUN_ASK_RETRY_SECS_V1, PALW_TIR_RUN_ASKS_IN_FLIGHT_V1, resource};
use borsh::BorshDeserialize;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_step_leg::{PalwStepRangeOpeningV1, PalwStepTileLeafV1};
use kaspa_consensus_core::palw_tir_court_v1::{PalwTirStepRunDisclosureV1, check_tir_step_run_disclosure_v1};
use misaka_palw_tir_exec::node::{TirCaptureV1, TirCellV1, TirRunInputsV1, cell_runs_capped_v1};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

// Last field: None for a seat/watch mask, Some(named leaf) for a public court's
// occurrence/prefix closure. Equal shard geometry is not equal requested coverage.
pub(in crate::palw_panel) type CellRunKeyV1 = (Hash64, u16, u16, u16, u32, Option<u64>);
const MAX_LEAVES: usize = 65_536;
const MAX_RUNS: usize = 1024;
const MAX_CACHE_BYTES: u64 = 256 << 20;
const ROLE: &str = "tir-cell-runs";

pub(in crate::palw_panel) struct CellRunPursuitV1 {
    binding: kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1,
    // First/count/maximum bytes of each requested leaf. Never one slot per whole-job leaf.
    chunks: Vec<(u64, Vec<u32>)>,
    asked: Vec<Option<Instant>>,
    answered: Vec<bool>,
    inputs: TirRunInputsV1,
    memory: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
    pub until: u64,
}
impl CellRunPursuitV1 {
    pub fn new(claim: Hash64, capture: &TirCaptureV1, cells: &[TirCellV1], until: u64) -> Result<Self, String> {
        Self::new_with_ledger(claim, capture, cells, until, &crate::palw_memory_ledger::host_ledger_v1())
    }
    pub fn new_with_ledger(
        claim: Hash64,
        capture: &TirCaptureV1,
        cells: &[TirCellV1],
        until: u64,
        ledger: &Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1>,
    ) -> Result<Self, String> {
        // Planning itself is admitted, before descriptors or geometry are materialized.
        let key = crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
            role: ROLE,
            class_id: capture.binding.class.class_id(&capture.binding.artifact_root),
            job: claim,
        };
        let planning_bytes = (capture.binding.class.program.len() as u64)
            .checked_mul(64)
            .and_then(|n| n.checked_add(super::PALW_TIR_SHARD_CELL_SCRATCH_BYTES_V1))
            .ok_or("the planning program envelope overflows")?;
        let planning = ledger.reserve(key.clone(), planning_bytes).map_err(|e| e.to_string())?;
        let space =
            kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(&capture.binding.class).map_err(|e| e.to_string())?;
        let mut wanted = BTreeMap::new();
        for cell in cells {
            for (first, count, _) in cell_runs_capped_v1(&space, &capture.binding.job_context, cell, MAX_LEAVES)? {
                for index in first..first.checked_add(u64::from(count)).ok_or("a cell run overflows")? {
                    if !wanted.contains_key(&index) && wanted.len() >= MAX_LEAVES {
                        return Err("the node's requested cell leaf budget is full".into());
                    }
                    if u32::try_from(index)
                        .ok()
                        .and_then(kaspa_consensus_core::palw_tir_shard_v1::palw_tir_runs_request_index_v1)
                        .is_none()
                    {
                        return Err("a requested cell leaf exceeds the signed off-chain run address space".into());
                    }
                    let leaf = space.leaf_at(&capture.binding.job_context, index).ok_or("a requested cell leaf is outside the job")?;
                    let max =
                        leaf.value_count.checked_mul(4).and_then(|n| n.checked_add(64)).ok_or("a cell value byte bound overflows")?;
                    wanted.insert(index, max);
                }
            }
        }
        if wanted.is_empty() {
            return Err("the cell run request is empty".into());
        }
        let mut chunks: Vec<(u64, Vec<u32>)> = Vec::new();
        for (index, max) in wanted {
            match chunks.last_mut() {
                Some((first, lanes)) if *first + lanes.len() as u64 == index && lanes.len() < 256 => lanes.push(max),
                _ => {
                    if chunks.len() >= MAX_RUNS {
                        return Err("the node's requested cell run budget is full".into());
                    }
                    chunks.push((index, vec![max]));
                }
            }
        }
        let bytes = chunks
            .iter()
            .try_fold(0u64, |total, (_, lanes)| {
                let raw = lanes.iter().try_fold(0u64, |sum, max| sum.checked_add(u64::from(*max) + 256))?;
                total.checked_add(raw.checked_mul(16)?.checked_add(16_384)?)
            })
            .and_then(|n| n.checked_add((capture.binding.class.program.len() as u64).checked_mul(64)?))
            .and_then(|n| n.checked_add(super::PALW_TIR_SHARD_CELL_SCRATCH_BYTES_V1))
            .ok_or("the cell run cache bound overflows")?;
        if bytes > MAX_CACHE_BYTES {
            return Err("the node's requested cell cache byte budget is full".into());
        }
        // Reserve the lasting data before releasing the planning guard.
        let memory = ledger.reserve(key, bytes).map_err(|e| e.to_string())?;
        drop(planning);
        let count = chunks.len();
        Ok(Self {
            binding: capture.binding.clone(),
            chunks,
            asked: vec![None; count],
            answered: vec![false; count],
            inputs: TirRunInputsV1::default(),
            memory: Some(memory),
            until,
        })
    }
    pub fn matches(&self, capture: &TirCaptureV1) -> bool {
        self.binding == capture.binding
    }
    pub fn asks_due(&mut self, now: Instant) -> Vec<(u64, u32)> {
        let waiting =
            |at: &Option<Instant>| at.is_some_and(|t| now.saturating_duration_since(t).as_secs() < PALW_TIR_RUN_ASK_RETRY_SECS_V1);
        let mut pending = self.asked.iter().zip(&self.answered).filter(|(at, done)| !**done && waiting(at)).count();
        let mut out = Vec::new();
        for i in 0..self.chunks.len() {
            if pending >= PALW_TIR_RUN_ASKS_IN_FLIGHT_V1 {
                break;
            }
            if self.answered[i] || waiting(&self.asked[i]) {
                continue;
            }
            self.asked[i] = Some(now);
            pending += 1;
            out.push((self.chunks[i].0, self.chunks[i].1.len() as u32));
        }
        out
    }
    pub fn drain(&mut self, claim: Hash64, pool: &mut HashMap<(Hash64, u32), Vec<Vec<u8>>>, ladder: u64) -> (usize, usize) {
        let (mut kept, mut refused) = (0, 0);
        for i in 0..self.chunks.len() {
            if self.answered[i] || self.asked[i].is_none() {
                continue;
            }
            let (first, sizes) = &self.chunks[i];
            let Some(index) =
                u32::try_from(*first).ok().and_then(kaspa_consensus_core::palw_tir_shard_v1::palw_tir_runs_request_index_v1)
            else {
                continue;
            };
            let Some(replies) = pool.remove(&(claim, index)) else { continue };
            for bytes in replies {
                let result = decode_requested_run_v1(&bytes, &self.binding, *first, sizes).and_then(|d| {
                    check_tir_step_run_disclosure_v1(
                        self.binding.full_logits_trace_root,
                        self.binding.committed_execution_root,
                        *first,
                        sizes.len() as u32,
                        &d,
                        ladder,
                    )
                    .map_err(|e| e.to_string())?;
                    self.inputs.add_run(
                        self.binding.step_leaf_count,
                        &self.binding.step_merkle_root,
                        &d.range,
                        &d.preimages,
                        &self.binding.job_context,
                        &self.binding.class.class_id(&self.binding.artifact_root),
                        ladder,
                    )
                });
                if result.is_err() {
                    refused += 1;
                    continue;
                }
                self.answered[i] = true;
                kept += 1;
                break;
            }
        }
        (kept, refused)
    }
    pub fn complete(&self) -> bool {
        self.answered.iter().all(|x| *x)
    }
    pub fn finish(mut self) -> Arc<resource::ReservedRowsV1<TirRunInputsV1>> {
        Arc::new(resource::ReservedRowsV1::new(std::mem::take(&mut self.inputs), self.memory.take().expect("cache memory held")))
    }
}

// Binding is compared as canonical bytes before deserializing any peer-controlled length.
// Remaining Vec lengths are checked before allocation; trailing bytes are refused.
fn decode_requested_run_v1(
    bytes: &[u8],
    binding: &kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1,
    first: u64,
    sizes: &[u32],
) -> Result<PalwTirStepRunDisclosureV1, String> {
    fn u32_at(r: &mut &[u8]) -> Result<u32, String> {
        u32::deserialize_reader(r).map_err(|e| e.to_string())
    }
    let mut stripped = binding.clone();
    kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut stripped);
    let prefix = borsh::to_vec(&stripped).map_err(|e| e.to_string())?;
    let ceiling = sizes
        .iter()
        .try_fold(prefix.len() as u64 + 16_384, |n, x| n.checked_add(u64::from(*x) + 128))
        .ok_or("reply size bound overflows")?;
    if bytes.len() as u64 > ceiling || !bytes.starts_with(&prefix) {
        return Err("the reply is oversized or carries another binding".into());
    }
    let mut r = &bytes[prefix.len()..];
    if u32_at(&mut r)? as usize != sizes.len() {
        return Err("the reply preimage count differs from the request".into());
    }
    let mut preimages = Vec::with_capacity(sizes.len());
    for max in sizes {
        let version = u16::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let coord = kaspa_consensus_core::palw_step::PalwStepCoordinateV1::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let value_count = u32_at(&mut r)?;
        let length = u32_at(&mut r)?;
        if length > *max || length as usize > r.len() {
            return Err("the reply leaf byte length exceeds its admitted bound".into());
        }
        let values_le = r[..length as usize].to_vec();
        r = &r[length as usize..];
        preimages.push(PalwStepTileLeafV1 { version, coord, value_count, values_le });
    }
    let got_first = u64::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
    if got_first != first || u32_at(&mut r)? as usize != sizes.len() {
        return Err("the reply opens another run".into());
    }
    let mut leaf_hashes = Vec::with_capacity(sizes.len());
    for _ in sizes {
        leaf_hashes.push(Hash64::deserialize_reader(&mut r).map_err(|e| e.to_string())?);
    }
    let n = u32_at(&mut r)?;
    if n > 128 || r.len() != n as usize * 64 {
        return Err("the reply sibling count/trailing bytes are not bounded".into());
    }
    let mut siblings = Vec::with_capacity(n as usize);
    for _ in 0..n {
        siblings.push(Hash64::deserialize_reader(&mut r).map_err(|e| e.to_string())?);
    }
    Ok(PalwTirStepRunDisclosureV1 {
        binding: binding.clone(),
        preimages,
        range: PalwStepRangeOpeningV1 { first_leaf_index: first, leaf_hashes, siblings },
    })
}
