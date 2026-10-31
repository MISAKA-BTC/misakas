//! Layout-free, explicit metadata for an EXISTING schema-1 TIR class row.
//!
//! The caller supplies the key, artifact and row from the same authenticated
//! native view. Recomputing their identity does not authenticate that provenance
//! or grant registration, execution, admission, court or credit authority.
//! Program-derived fields are rechecked; layout_digest, tokenizer_id and
//! max_context remain rooted-row metadata. The latter cannot be reconstructed
//! from a layout digest. Complete row/layout validation still requires the
//! authenticated layout preimage and the full-layout resolver.
//!
//! No serialized type, legacy fact bit, reader dispatch or hash domain changes.

use crate::{
    Hash64,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_admission_v1::{PALW_TIR_CLASS_RECORD_VERSION_V1, PalwTirClassRecordV1},
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, palw_tir_class_id_from_digests_v1},
};
use misaka_palw_tir::{
    DType, TirError, TirProgramV1,
    program::{HISTORY_BOUND_V1_HELD, MAX_PROGRAM_BYTES},
};

/// A metadata-only wire-lane classification. Legacy four-byte logits include
/// ALL old committable dtypes (I8/I16/I32/Idx), not just the arithmetic I32 type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirRowTraceProfileV2 {
    FlatLegacy4Byte,
    TiledLegacy4Byte,
    TiledI64Q24,
}

/// Caller-selected LOCAL input/result caps, not consensus rules or a heap/work
/// lease. Decode/re-encode/validation and graph/cone dissection metadata need a
/// separate owner reservation. Dissection scans bounded program graphs, never
/// model positions or weights; its temporary work is not capped by result count.
#[derive(Clone, Copy, Debug)]
pub struct TirRowProfileLimitsV2 {
    pub max_program_bytes: usize,
    /// Zero is valid for a row with no dissected commit points.
    pub max_dissected_points: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TirRowProfileErrorV2 {
    Budget,
    RowVersion,
    RowMismatch,
    ClassIdentity,
    ContextBound,
    UnsupportedScheme,
    OutputDtype,
    LogitsWidth,
    Program(TirError),
}

/// Private, nonserialized metadata; deliberately no raw-facts, row conversion,
/// layout/step geometry, canonical-job or native-permission getter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTirRowProfileV2 {
    class_id: Hash64,
    profile: TirRowTraceProfileV2,
    logits_dtype: DType,
    token_bound: u32,
    held: bool,
    row_max_context: u32,
    logits_vocab: u32,
}

impl ResolvedTirRowProfileV2 {
    /// Recheck a row's bounded original program and its derived metadata. The
    /// artifact is independently obtained from native classes[expected_class_id],
    /// not guessed from this row. Both the native key and row facts ID must
    /// match the existing class-ID digest. This is consistency, not provenance.
    pub fn resolve(
        expected_class_id: Hash64,
        artifact: Hash64,
        record: &PalwTirClassRecordV1,
        limits: TirRowProfileLimitsV2,
    ) -> Result<Self, TirRowProfileErrorV2> {
        // Refuse oversized declared/actual input BEFORE hashing, decoding or
        // walking graphs. Never clone the caller's row/program/dissection list.
        if limits.max_program_bytes == 0
            || record.program.is_empty()
            || record.program.len() > MAX_PROGRAM_BYTES
            || record.program.len() > limits.max_program_bytes
            || record.program_bytes as usize > MAX_PROGRAM_BYTES
            || record.program_bytes as usize > limits.max_program_bytes
            || record.dissected.len() > limits.max_dissected_points
        {
            return Err(TirRowProfileErrorV2::Budget);
        }
        if record.version != PALW_TIR_CLASS_RECORD_VERSION_V1 {
            return Err(TirRowProfileErrorV2::RowVersion);
        }
        record.check_program_v1().map_err(|_| TirRowProfileErrorV2::RowMismatch)?;
        // decode_canonical includes strict encoding/re-encoding AND structural
        // validation, so all schedule/node indexing below is already checked.
        let program = TirProgramV1::decode_canonical(&record.program).map_err(TirRowProfileErrorV2::Program)?;
        let scheme = Hash64::from_bytes(program.logits_scheme_id);
        let profile = if scheme == crate::palw_step_refute::flat_logits_scheme_id_v1() {
            TirRowTraceProfileV2::FlatLegacy4Byte
        } else if scheme == crate::palw_step_refute::tiled_logits_scheme_id_v1() {
            TirRowTraceProfileV2::TiledLegacy4Byte
        } else if program.wide_logits_q24() {
            TirRowTraceProfileV2::TiledI64Q24
        } else {
            return Err(TirRowProfileErrorV2::UnsupportedScheme);
        };
        let logits = &program.blocks[program.schedule.post as usize].nodes[program.logits as usize];
        let dtype_ok =
            if profile == TirRowTraceProfileV2::TiledI64Q24 { logits.out.dtype == DType::I64 } else { logits.out.dtype.committable() };
        if !dtype_ok {
            return Err(TirRowProfileErrorV2::OutputDtype);
        }
        let logits_vocab =
            u32::try_from(logits.out.elements_at(1)).ok().filter(|v| *v > 0).ok_or(TirRowProfileErrorV2::LogitsWidth)?;
        let held = program.history_bound == HISTORY_BOUND_V1_HELD;
        if record.facts.token_bound != program.token_bound
            || record.facts.held != held
            // Frozen EXACT old predicate: wide remains false, never a family bit.
            || record.facts.tiled != (profile == TirRowTraceProfileV2::TiledLegacy4Byte)
            || record.prim_set_id != Hash64::from_bytes(program.prim_set_id)
            || record.logits_vocab != logits_vocab
        {
            return Err(TirRowProfileErrorV2::RowMismatch);
        }
        if record.facts.max_context == 0 || record.facts.max_context > program.history_bound {
            return Err(TirRowProfileErrorV2::ContextBound);
        }
        let class_id = palw_tir_class_id_from_digests_v1(
            PALW_TIR_CLASS_VERSION_V1,
            &record.graph_ir_root,
            &record.layout_digest,
            &artifact,
            &record.tokenizer_id,
        );
        if class_id != expected_class_id || class_id != record.facts.class_id {
            return Err(TirRowProfileErrorV2::ClassIdentity);
        }
        let dissected = crate::palw_tir_dissect_v1::palw_tir_dissected_commit_points_v1(&program);
        if dissected.len() > limits.max_dissected_points {
            return Err(TirRowProfileErrorV2::Budget);
        }
        if dissected != record.dissected {
            return Err(TirRowProfileErrorV2::RowMismatch);
        }
        Ok(Self {
            class_id,
            profile,
            logits_dtype: logits.out.dtype,
            token_bound: program.token_bound,
            held,
            row_max_context: record.facts.max_context,
            logits_vocab,
        })
    }

    pub fn class_id(&self) -> Hash64 {
        self.class_id
    }
    pub fn profile(&self) -> TirRowTraceProfileV2 {
        self.profile
    }
    pub fn logits_dtype(&self) -> DType {
        self.logits_dtype
    }
    pub fn program_scheme_id(&self) -> Hash64 {
        match self.profile {
            TirRowTraceProfileV2::FlatLegacy4Byte => crate::palw_step_refute::flat_logits_scheme_id_v1(),
            TirRowTraceProfileV2::TiledLegacy4Byte => crate::palw_step_refute::tiled_logits_scheme_id_v1(),
            TirRowTraceProfileV2::TiledI64Q24 => Hash64::from_bytes(misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2),
        }
    }
    /// Flat legacy contexts use their original trace identity, not the program
    /// scheme ID. This getter does not construct or authorize any job context.
    pub fn context_trace_scheme_id(&self) -> Hash64 {
        if self.profile == TirRowTraceProfileV2::FlatLegacy4Byte {
            crate::palw_v2::trace_scheme_id_v2()
        } else {
            self.program_scheme_id()
        }
    }
    pub fn token_bound(&self) -> u32 {
        self.token_bound
    }
    pub fn held(&self) -> bool {
        self.held
    }
    /// Authenticated ROW metadata if the caller supplied an authenticated row.
    /// Range-checked against the program, NOT reconstructed/validated layout.
    pub fn row_max_context(&self) -> u32 {
        self.row_max_context
    }
    pub fn logits_vocab(&self) -> u32 {
        self.logits_vocab
    }
    /// Wire bytes per logits lane, independent of its arithmetic dtype's width.
    pub fn lane_width_bytes(&self) -> u8 {
        if self.profile == TirRowTraceProfileV2::TiledI64Q24 { 8 } else { 4 }
    }
    /// Logits-data tiles only: NOT step/layout geometry or a DA admission policy.
    pub fn logits_tiles(&self) -> u32 {
        if self.profile == TirRowTraceProfileV2::FlatLegacy4Byte {
            1
        } else {
            self.logits_vocab.div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u32)
        }
    }
    pub fn prompt_ids_form(&self, network: PalwPromptIdsFormV1) -> PalwPromptIdsFormV1 {
        if self.held { PalwPromptIdsFormV1::MerkleV1 } else { network }
    }
}

#[cfg(test)]
mod tests;
