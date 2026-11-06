//! Capture Borsh framing, with allocation checks before every peer-controlled Vec.
//!
//! A logits outer Vec can carry 24 native bytes per four encoded bytes (empty rows).
//! Checking the total frame length or authenticating metadata after generic Borsh decode
//! does not bound that allocation. False values remain available to courts; this is
//! resource admission, not a judgement of the capture's commitments or honesty.

use borsh::BorshDeserialize;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_step::PalwStepCoordinateV1;
use kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1;
use kaspa_consensus_core::palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1;
use kaspa_consensus_core::palw_v2::{PALW_V2_MAX_NETWORK_ID_BYTES, PalwJobContextV2};
use misaka_palw_tir::program::{MAX_BLOCKS, MAX_NODES_PER_BLOCK, MAX_PROGRAM_BYTES, MAX_STATES};

use super::backend::TirCaptureV1;
use super::wide_capture_v2::{TirWideCaptureV2, WideCaptureLimitsV2};

// The old node already reserves 3 * encoded frame + 16 MiB. Leave one encoded
// frame for the fetched bytes, exactly preserving the V1 decoded-storage budget.
const DECODE_FIXED_HEADROOM: usize = 16 << 20;

// Operation reservation for fixed parser bookkeeping and construction temporaries:
// the borrowed frame/Reader, scalar/leaf headers, job/class/binding construction,
// and capture assembly. Native Vec backing is charged separately by Reader.
// This is a requested-storage contract, not a process RSS/allocator certificate.
pub(super) const WIDE_CAPTURE_PARSER_BYTES: u64 = super::wide_capture_v2::budget::PARSER_SCRATCH_BYTES;

pub(super) fn decode(body: &[u8], frame_length: usize) -> Result<TirCaptureV1, String> {
    let limit = frame_length
        .checked_mul(2)
        .and_then(|n| n.checked_add(DECODE_FIXED_HEADROOM))
        .ok_or("the IR capture allocation bound overflows")?;
    decode_with_limit(body, limit).map_err(|e| format!("the IR capture does not decode: {e}"))
}

// Only fixed-size scalars may enter the generic Borsh route. In particular, this
// whitelist cannot accidentally deserialize a Vec before the Reader charges it.
mod fixed_scalar {
    pub trait Sealed {}
}

pub(super) trait FixedScalar: BorshDeserialize + fixed_scalar::Sealed {}
macro_rules! fixed_scalars {
    ($($t:ty),+ $(,)?) => { $(
        impl fixed_scalar::Sealed for $t {}
        impl FixedScalar for $t {}
    )+ };
}
fixed_scalars!(u16, u32, u64, i32, i64, [u8; 32], Hash64, PalwStepCoordinateV1);

pub(super) trait Word32: FixedScalar {}
impl Word32 for u32 {}
impl Word32 for i32 {}

pub(super) struct BindingCaps {
    pub(super) network_bytes: usize,
    pub(super) program_bytes: usize,
    pub(super) commit_tiles: usize,
    pub(super) state_tiles: usize,
}

impl BindingCaps {
    const LEGACY: Self =
        Self { network_bytes: usize::MAX, program_bytes: usize::MAX, commit_tiles: usize::MAX, state_tiles: usize::MAX };
    const WIDE: Self = Self {
        network_bytes: PALW_V2_MAX_NETWORK_ID_BYTES,
        program_bytes: MAX_PROGRAM_BYTES,
        commit_tiles: MAX_BLOCKS * MAX_NODES_PER_BLOCK,
        state_tiles: MAX_STATES,
    };
}

pub(super) struct Reader<'a> {
    remaining: &'a [u8],
    storage_left: usize,
    strict_capacity: bool,
}

impl<'a> Reader<'a> {
    pub(super) fn new(body: &'a [u8], limit: usize, root_size: usize, strict_capacity: bool) -> Result<Self, String> {
        Ok(Self {
            remaining: body,
            storage_left: limit.checked_sub(root_size).ok_or("capture allocation bound is too small")?,
            strict_capacity,
        })
    }

    pub(super) fn scalar<T: FixedScalar>(&mut self) -> Result<T, String> {
        T::deserialize_reader(&mut self.remaining).map_err(|e| e.to_string())
    }

    pub(super) fn reserve<T>(&mut self, count: usize) -> Result<Vec<T>, String> {
        let bytes = count.checked_mul(std::mem::size_of::<T>()).ok_or("capture allocation overflows")?;
        self.storage_left = self.storage_left.checked_sub(bytes).ok_or("capture decoded storage exceeds its reservation")?;
        let mut out = Vec::new();
        out.try_reserve_exact(count).map_err(|e| format!("capture allocation refused: {e}"))?;
        if self.strict_capacity && out.capacity() != count {
            return Err("capture allocator exceeded the exact backing request".into());
        }
        Ok(out)
    }

    pub(super) fn length(&mut self, minimum_wire_bytes: usize, maximum_count: usize) -> Result<usize, String> {
        let n = usize::try_from(self.scalar::<u32>()?).map_err(|_| "capture length does not fit usize")?;
        // Keep this check first, including its V1 error string, for truncated lengths.
        if minimum_wire_bytes == 0 || n > self.remaining.len() / minimum_wire_bytes {
            return Err("capture vector length exceeds the remaining frame".into());
        }
        if n > maximum_count {
            return Err("capture vector length exceeds its field limit".into());
        }
        Ok(n)
    }

    fn take_bytes(&mut self, n: usize) -> Result<Vec<u8>, String> {
        let mut out = self.reserve::<u8>(n)?;
        out.extend_from_slice(&self.remaining[..n]);
        self.remaining = &self.remaining[n..];
        Ok(out)
    }

    pub(super) fn bytes(&mut self, maximum_bytes: usize) -> Result<Vec<u8>, String> {
        let n = self.length(1, maximum_bytes)?;
        self.take_bytes(n)
    }

    pub(super) fn words32<T: Word32>(&mut self, maximum_count: usize) -> Result<Vec<T>, String> {
        let n = self.length(4, maximum_count)?;
        let mut out = self.reserve::<T>(n)?;
        for _ in 0..n {
            out.push(self.scalar()?);
        }
        Ok(out)
    }

    pub(super) fn words64(&mut self, maximum_count: usize) -> Result<Vec<i64>, String> {
        let n = self.length(8, maximum_count)?;
        let mut out = self.reserve::<i64>(n)?;
        for _ in 0..n {
            out.push(self.scalar()?);
        }
        Ok(out)
    }

    pub(super) fn binding(&mut self, caps: &BindingCaps) -> Result<PalwTirStepBindingV1, String> {
        let version = self.scalar()?;
        // All nested lengths are read here, never by generic binding deserialization.
        let job_context = PalwJobContextV2 {
            version: self.scalar()?,
            network_id: self.bytes(caps.network_bytes)?,
            job_id: self.scalar()?,
            job_nullifier: self.scalar()?,
            assignment_id: self.scalar()?,
            execution_seed: self.scalar()?,
            model_profile_id: self.scalar()?,
            runtime_manifest_hash: self.scalar()?,
            runtime_class_id: self.scalar()?,
            shape_profile_id: self.scalar()?,
            trace_scheme_id: self.scalar()?,
            cu_ruleset_id: self.scalar()?,
            tokenizer_id: self.scalar()?,
            prompt_token_ids_hash: self.scalar()?,
            declared_prefill_tokens: self.scalar()?,
            exact_decode_tokens: self.scalar()?,
            max_context_tokens: self.scalar()?,
        };
        let class = PalwTirClassV1 {
            version: self.scalar()?,
            program: self.bytes(caps.program_bytes)?,
            layout: PalwTirLayoutV1 {
                version: self.scalar()?,
                max_context: self.scalar()?,
                checkpoint_interval: self.scalar()?,
                h_tile: self.scalar()?,
                commit_tiles: self.words32::<u32>(caps.commit_tiles)?,
                state_tiles: self.words32::<u32>(caps.state_tiles)?,
            },
            tokenizer_id: self.scalar()?,
        };
        Ok(PalwTirStepBindingV1 {
            version,
            job_context,
            class,
            artifact_root: self.scalar()?,
            full_logits_trace_root: self.scalar()?,
            step_leaf_count: self.scalar()?,
            step_merkle_root: self.scalar()?,
            committed_execution_root: self.scalar()?,
        })
    }

    pub(super) fn finish(self) -> Result<(), String> {
        if !self.remaining.is_empty() {
            return Err("trailing bytes after the IR capture".into());
        }
        Ok(())
    }
}

fn decode_with_limit(body: &[u8], limit: usize) -> Result<TirCaptureV1, String> {
    let mut r = Reader::new(body, limit, std::mem::size_of::<TirCaptureV1>(), false)?;
    let binding = r.binding(&BindingCaps::LEGACY)?;
    let prompt = r.words32::<u32>(usize::MAX)?;
    let row_count = r.length(4, usize::MAX)?;
    let mut logits_rows = r.reserve::<Vec<i32>>(row_count)?;
    for _ in 0..row_count {
        logits_rows.push(r.words32::<i32>(usize::MAX)?);
    }
    let generated = r.words32::<u32>(usize::MAX)?;
    // u16 version, four coordinate u32s, value_count u32 and values Vec length u32.
    let leaf_count = r.length(26, usize::MAX)?;
    let mut leaves = r.reserve::<PalwStepTileLeafV1>(leaf_count)?;
    for _ in 0..leaf_count {
        leaves.push(PalwStepTileLeafV1 {
            version: r.scalar()?,
            coord: r.scalar::<PalwStepCoordinateV1>()?,
            value_count: r.scalar()?,
            values_le: r.bytes(usize::MAX)?,
        });
    }
    r.finish()?;
    Ok(TirCaptureV1 { binding, prompt, logits_rows, generated, leaves })
}

/// Decoded storage available after the caller's frame, external retained bytes,
/// and fixed parser reservation coexist. The returned cap includes the DTO itself.
pub(super) fn wide_decode_storage_limit(frame_length: usize, limits: &WideCaptureLimitsV2) -> Result<usize, String> {
    let frame = u64::try_from(frame_length).map_err(|_| "wide capture frame length overflows")?;
    if frame > limits.max_frame_bytes {
        return Err("wide capture frame exceeds its limit".into());
    }
    let operation_left = limits
        .max_operation_bytes
        .checked_sub(frame)
        .and_then(|n| n.checked_sub(limits.external_retained_bytes))
        .and_then(|n| n.checked_sub(WIDE_CAPTURE_PARSER_BYTES))
        .ok_or("wide capture operation reservation is too small")?;
    usize::try_from(limits.max_decoded_bytes.min(operation_left)).map_err(|_| "wide capture decoded bound does not fit usize".into())
}

fn native_cap(cap: u64) -> usize {
    // Counts on this wire are u32. A ceiling above native indices is effectively
    // unlimited, while every actual encoded count still undergoes checked conversion.
    usize::try_from(cap).unwrap_or(usize::MAX)
}

pub(super) fn decode_wide(body: &[u8], frame_length: usize, limits: WideCaptureLimitsV2) -> Result<TirWideCaptureV2, String> {
    let limit = wide_decode_storage_limit(frame_length, &limits)?;
    if body.len().checked_add(8) != Some(frame_length) {
        return Err("wide capture body and frame lengths differ".into());
    }
    let mut r = Reader::new(body, limit, std::mem::size_of::<TirWideCaptureV2>(), true)?;
    let binding = r.binding(&BindingCaps::WIDE)?;
    let prompt = r.words32::<u32>(native_cap(limits.max_prompt_ids))?;
    // The outer Vec encodes each inner length in four bytes, even for i64 lanes.
    let row_count = r.length(4, native_cap(limits.max_rows))?;
    let mut logits_rows = r.reserve::<Vec<i64>>(row_count)?;
    for _ in 0..row_count {
        logits_rows.push(r.words64(native_cap(limits.max_row_lanes))?);
    }
    let generated = r.words32::<u32>(native_cap(limits.max_rows))?;
    let leaf_count = r.length(26, native_cap(limits.max_leaves))?;
    let mut leaves = r.reserve::<PalwStepTileLeafV1>(leaf_count)?;
    let mut total_payload = 0u64;
    for _ in 0..leaf_count {
        let version = r.scalar()?;
        let coord = r.scalar::<PalwStepCoordinateV1>()?;
        let value_count = r.scalar()?;
        let length = r.length(1, native_cap(limits.max_leaf_payload_bytes))?;
        total_payload = total_payload
            .checked_add(u64::try_from(length).map_err(|_| "wide capture payload length overflows")?)
            .ok_or("wide capture total payload length overflows")?;
        if total_payload > limits.max_total_leaf_payload_bytes {
            return Err("wide capture total leaf payload exceeds its limit".into());
        }
        // Both payload limits and frame truncation have been checked before reserve.
        let values_le = r.take_bytes(length)?;
        leaves.push(PalwStepTileLeafV1 { version, coord, value_count, values_le });
    }
    r.finish()?;
    Ok(TirWideCaptureV2 { binding, prompt, logits_rows, generated, leaves })
}
