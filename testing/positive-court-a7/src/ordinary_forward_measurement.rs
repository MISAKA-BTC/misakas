//! Fixed actual-trained cohort, same owned Params ordinary→dense comparison.
//! This library component consumes EXTERNALLY PREPARED trace-free bounded
//! metadata; it does not provide whole startup or certify caller-owned heap.
//! No model substitution, new context, binding roots, native Result/funds/P,
//! cold/RSS guarantee or Panel/Node role completion is produced.
use kaspa_consensus_core::Hash64;
use misaka_palw_tir_exec::node::{
    memory::TirMemoryReserverV1,
    owned_artifact_params_v2::forward::{
        LocalForwardErrorV2, LocalForwardInputsV2, LocalForwardLoadBudgetV2, LocalForwardLoadUsageV2, LocalWideForwardBudgetV2,
        LocalWideForwardOutputV2, LocalWideForwardUsageV2, load_local_wide_forward_v2,
        measurement::{DenseMeasurementBudgetV2, DenseMeasurementErrorV2, DenseMeasurementOutputV2, DenseMeasurementUsageV2},
    },
};
use std::{fs::File, time::Instant};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedTrainedModelV2 {
    SmolLm2360M,
    SmolLm2135M,
    Pythia70M,
}
pub struct FixedModelPinV2 {
    pub model: &'static str,
    pub revision: &'static str,
    pub original_max_context: u32,
    pub artifact_bytes: u64,
    pub expected_artifact_sha256: &'static str,
    pub class_id: &'static str,
    pub artifact_root: &'static str,
    pub registered_tokenizer: &'static str,
    pub original_context_hash: &'static str,
    pub prompt: &'static [u32],
}
const SMOL_PROMPT: &[u32] = &[504, 3446, 282, 451, 4308, 314, 288, 2004, 3465, 20330, 351, 253, 8212, 284, 41088, 10295];
const PYTHIA_PROMPT: &[u32] = &[510, 4096, 273, 436, 3368, 310, 281, 1924, 4217, 13782, 342, 247, 10799, 285, 41374, 10636];
impl FixedTrainedModelV2 {
    /// Pins copied from saved cohort inventory/old short local run records, not
    /// from candidate metadata. They authenticate no native registration state.
    pub fn pin(self) -> FixedModelPinV2 {
        match self {
            Self::SmolLm2360M => FixedModelPinV2 {
                model: "HuggingFaceTB/SmolLM2-360M",
                revision: "f8027fd0eaeea54caa13c31d31b9fdc459c38b49",
                original_max_context: 8192,
                artifact_bytes: 436395392,
                expected_artifact_sha256: "d17a82c83fcd86736feb60b05e492ebae79f5e2507600bc35ec2c024a2fe36c8",
                class_id: "ad6c9866ee15de51ad18c4c83c595dfafd646a931cc18db77b8bbe627b8e9f555dd87f3efb81943d098ff32729d9c5c645a88571f65b5e007061234fa5489476",
                artifact_root: "041029123feed837f46db580ee1b89092e0c1700b4cc033c9089f50afd587be75ceab1f985700042ee934194f244fafbd9e4940299621a8c3e2b09529dd8ec12",
                registered_tokenizer: "57c55fb228e8bb272c1d3e3a75eb8e7a238fe2e6a7ccc5e4cd6b874c1278fbc79342c2d466bc52d53000bb0de6de57cb196fc0a413c2751d45cfb69551615ac2",
                original_context_hash: "530ef1cb986eb4ad8c9b6fea385ff38db5d4c2e7817f9e503373d2413c79cabba7de0805c3ef863baeefd8b5e94a5789930d748b592b6c9d4377d8892c8f9de0",
                prompt: SMOL_PROMPT,
            },
            Self::SmolLm2135M => FixedModelPinV2 {
                model: "HuggingFaceTB/SmolLM2-135M",
                revision: "93efa2f097d58c2a74874c7e644dbc9b0cee75a2",
                original_max_context: 8192,
                artifact_bytes: 180336320,
                expected_artifact_sha256: "48c72278b0d9b45c7ab92b6e70f5fbcef47fa86cc8644cd4307798354639fe90",
                class_id: "7a2616b4b132946d21b1d396c0a82f69a144fb7f692394314efb35e3f8bff1f231b857f174e1165ad92c65ce742e1bcc57dfb11e1e5eab02b67b8cf87436c625",
                artifact_root: "8e31580dc81f6548b9e440c9b6d45fe63bff1d42ed2dbb4b342bdb58fc4590f0b8210f10a2a23e6c352c866a4313004b547ec363997e5757d2bce31a6838eec4",
                registered_tokenizer: "57c55fb228e8bb272c1d3e3a75eb8e7a238fe2e6a7ccc5e4cd6b874c1278fbc79342c2d466bc52d53000bb0de6de57cb196fc0a413c2751d45cfb69551615ac2",
                original_context_hash: "a04696aa8960188b22832ac696c75d987b1a0b22831612a1d4ff5bea6cf7577f72c231308d5e8811b15377677f06942957100253dfeab2c498d6e9d1e9f219e9",
                prompt: SMOL_PROMPT,
            },
            Self::Pythia70M => FixedModelPinV2 {
                model: "EleutherAI/pythia-70m",
                revision: "a39f36b100fe8a5377810d56c3f4789b9c53ac42",
                original_max_context: 2048,
                artifact_bytes: 103402560,
                expected_artifact_sha256: "2d1010327664743f240ea8d5bc089daed5a704d4340fea322d1f9cafbe009651",
                class_id: "356fbf891ef8ff98cc008c5187c7105bbc9eb32a53225a54361493c96fdc75f3af9d7acb1789a4fa9866771442fa1a55a8217371278bf88130ece5d2e4af91e1",
                artifact_root: "427a912e722f173d1d0fe4b02dac4148709e9a70d245c3012768a5962fa3f6736ea9cb921b8824b9712a1f718f5dc389c00c1d81fa9c8f552fd358c96ba10ef4",
                registered_tokenizer: "9ccc3c319505ec74ccccb984a97bbd955c2c3f30e4c8f2dbe95ff8e13b213d84074d705cf61decf466eb2026e4a1b9ebf3e33f32abdc0653382ea0a669941a43",
                original_context_hash: "d9ca9eae0abcafa087e4e06ba9fcd8a8f71f0f435735e3302a7725d2465fb9a22eb728f4a7d3075bffcdc6208d791b999e6f1f25574864ac7b41d792379ee868",
                prompt: PYTHIA_PROMPT,
            },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderFileStampV2 {
    pub device: u64,
    pub inode: u64,
    pub bytes: u64,
    pub mtime: i64,
    pub mtime_ns: i64,
    pub ctime: i64,
    pub ctime_ns: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainedMeasureErrorV2 {
    UnsupportedPlatform,
    SourceIo,
    Changed,
    Pin,
    Load(LocalForwardErrorV2),
    Ordinary(LocalForwardErrorV2),
    Dense(DenseMeasurementErrorV2),
}
/// Capture BEFORE bounded header reading on the same descriptor; no reopen.
/// Stability is observed metadata, NOT an atomic immutable filesystem snapshot.
pub fn header_file_stamp_v2(file: &File) -> Result<HeaderFileStampV2, TrainedMeasureErrorV2> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let s = file.metadata().map_err(|_| TrainedMeasureErrorV2::SourceIo)?;
        if !s.is_file() {
            return Err(TrainedMeasureErrorV2::SourceIo);
        }
        Ok(HeaderFileStampV2 {
            device: s.dev(),
            inode: s.ino(),
            bytes: s.len(),
            mtime: s.mtime(),
            mtime_ns: s.mtime_nsec(),
            ctime: s.ctime(),
            ctime_ns: s.ctime_nsec(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Err(TrainedMeasureErrorV2::UnsupportedPlatform)
    }
}
fn hash64(text: &str) -> Result<Hash64, TrainedMeasureErrorV2> {
    if text.len() != 128 {
        return Err(TrainedMeasureErrorV2::Pin);
    }
    let mut out = [0u8; 64];
    for (i, b) in text.as_bytes().chunks_exact(2).enumerate() {
        let s = std::str::from_utf8(b).map_err(|_| TrainedMeasureErrorV2::Pin)?;
        out[i] = u8::from_str_radix(s, 16).map_err(|_| TrainedMeasureErrorV2::Pin)?;
    }
    Ok(Hash64::from_bytes(out))
}
#[derive(Clone, Copy, Debug)]
pub struct TrainedPhaseTimesV2 {
    pub owned_parameter_load_ns: u128,
    pub ordinary_ns: u128,
    pub dense_prepare_ns: u128,
    pub dense_run_ns: u128,
}
/// No JSON construction/copy here. Serialization/output buffers, process control,
/// metadata preparation, input SHA hashing and retained exports stay externally
/// owned. The results own their true run/extra guards and drop data before them.
pub struct TrainedOrdinaryMeasurementV2 {
    model: FixedTrainedModelV2,
    ordinary: LocalWideForwardOutputV2,
    dense: DenseMeasurementOutputV2,
    times: TrainedPhaseTimesV2,
    load_usage: LocalForwardLoadUsageV2,
    ordinary_usage: LocalWideForwardUsageV2,
    dense_usage: DenseMeasurementUsageV2,
}
impl TrainedOrdinaryMeasurementV2 {
    pub fn model(&self) -> FixedTrainedModelV2 {
        self.model
    }
    pub fn ordinary(&self) -> &LocalWideForwardOutputV2 {
        &self.ordinary
    }
    pub fn dense(&self) -> &DenseMeasurementOutputV2 {
        &self.dense
    }
    pub fn times(&self) -> TrainedPhaseTimesV2 {
        self.times
    }
    pub fn load_usage(&self) -> LocalForwardLoadUsageV2 {
        self.load_usage
    }
    pub fn ordinary_usage(&self) -> LocalWideForwardUsageV2 {
        self.ordinary_usage
    }
    pub fn dense_usage(&self) -> DenseMeasurementUsageV2 {
        self.dense_usage
    }
    pub fn scope(&self) -> &'static str {
        "same-owned-parameters ordinary CPU then legacy dense; local short-job comparison only"
    }
}
/// REAL API calls, not a success stub. Actual execution is only possible after
/// the caller supplies fully prepared bounded Header/Source/Compiled/class/job/
/// geometry and ACTUAL complete external owned ticket. This helper neither
/// constructs nor certifies that whole metadata census (source.owner_requests
/// is only structdelta). r3+measurement bridge and that caller are dependencies.
/// Budgets must persist across retries. Same required ledger handles all stages.
/// Context is the unchanged recorded16-prompt/1-decode local noncanonical job,
/// with ORIGINAL maximum8192/8192/2048. No native assignment or full-length claim.
/// Input expected artifact SHA is a saved pin only; this function verifies owned
/// weight root/class/context, NOT full-file SHA or publisher/model correctness.
#[allow(clippy::too_many_arguments)]
pub fn measure_prepared_trained_v2(
    model: FixedTrainedModelV2,
    file: &mut File,
    at_header: HeaderFileStampV2,
    inputs: LocalForwardInputsV2<'_>,
    prompt: &[u32],
    loads: &mut LocalForwardLoadBudgetV2,
    runs: &mut LocalWideForwardBudgetV2,
    denses: &mut DenseMeasurementBudgetV2,
    reserver: &TirMemoryReserverV1,
) -> Result<TrainedOrdinaryMeasurementV2, TrainedMeasureErrorV2> {
    let pin = model.pin();
    let now = header_file_stamp_v2(file)?;
    if now != at_header {
        return Err(TrainedMeasureErrorV2::Changed);
    }
    if now.bytes != pin.artifact_bytes
        || inputs.expected_class != hash64(pin.class_id)?
        || inputs.artifact != hash64(pin.artifact_root)?
        || inputs.expected_execution != hash64(pin.original_context_hash)?
        || inputs.class.tokenizer_id != hash64(pin.registered_tokenizer)?
        || inputs.class.layout.max_context != pin.original_max_context
        || inputs.original_context.max_context_tokens != pin.original_max_context
        || inputs.original_context.declared_prefill_tokens != 16
        || inputs.original_context.exact_decode_tokens != 1
        || prompt != pin.prompt
    {
        return Err(TrainedMeasureErrorV2::Pin);
    }
    let start = Instant::now();
    let owner = load_local_wide_forward_v2(file, inputs, loads, reserver).map_err(TrainedMeasureErrorV2::Load)?;
    let load_ns = start.elapsed().as_nanos();
    if header_file_stamp_v2(file)? != at_header {
        return Err(TrainedMeasureErrorV2::Changed);
    }
    let start = Instant::now();
    let ordinary = owner.forward_local(prompt, runs, reserver).map_err(TrainedMeasureErrorV2::Ordinary)?;
    let ordinary_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let prepared = owner.prepare_dense_measurement(&ordinary, denses).map_err(TrainedMeasureErrorV2::Dense)?;
    let prepare_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let dense = prepared.run(prompt, reserver).map_err(TrainedMeasureErrorV2::Dense)?;
    let dense_ns = start.elapsed().as_nanos();
    if header_file_stamp_v2(file)? != at_header {
        return Err(TrainedMeasureErrorV2::Changed);
    }
    Ok(TrainedOrdinaryMeasurementV2 {
        model,
        ordinary,
        dense,
        times: TrainedPhaseTimesV2 {
            owned_parameter_load_ns: load_ns,
            ordinary_ns,
            dense_prepare_ns: prepare_ns,
            dense_run_ns: dense_ns,
        },
        load_usage: loads.usage(),
        ordinary_usage: runs.usage(),
        dense_usage: denses.usage(),
    })
}
