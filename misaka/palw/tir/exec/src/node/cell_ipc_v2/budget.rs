//! Finite logical IPC backing and cumulative connection admission. The quote
//! is not a full-session host lease or a physical RSS bound. A server may hold
//! a separate host guard for its actual executor workspace; I/O/copy envelopes
//! here remain explicit local caps. Independent retained owners must be summed
//! by the caller in `external_retained_bytes`.

use kaspa_consensus_core::{palw_tir_step_v1::PalwTirStepSpaceV2, palw_v2::PalwJobContextV2};

use crate::{TirPlan, node::memory::tir_executor_storage_need_wide_q24_v2};

pub(crate) const PARSER_SCRATCH_BYTES: u64 = 8192;

/// Explicit per-frame, native backing and cumulative connection ceilings.
/// Work is a symbolic logical charge, not measured CPU instructions or time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WideCellLimitsV2 {
    pub max_frame_bytes: u64,
    pub max_decoded_bytes: u64,
    pub max_operation_bytes: u64,
    pub max_total_read_bytes: u64,
    pub max_total_write_bytes: u64,
    pub max_messages: u64,
    pub max_steps: u64,
    pub max_program_bytes: u64,
    pub max_params: u64,
    /// Aggregate parameter payload bytes in a message.
    pub max_param_bytes: u64,
    pub max_values: u64,
    /// Aggregate lanes across value, carry and logits vectors in a message.
    pub max_lanes: u64,
    pub max_carry_vectors: u64,
    pub max_rank: u64,
    pub max_name_bytes: u64,
    pub max_workspace_bytes: u64,
    /// Cumulative symbolic work for this connection, including accepted I/O.
    pub max_work: u64,
    pub external_retained_bytes: u64,
}

/// Logical simultaneous backing for one operation. Source/native/collector and
/// input/output copies may coexist: the base includes two maximum frames and
/// two maximum decoded owners, plus workspace, parser scratch and externals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WideCellSessionQuoteV2 {
    pub frame_bytes: u64,
    pub decoded_bytes: u64,
    pub workspace_bytes: u64,
    pub external_retained_bytes: u64,
    pub operation_peak_bytes: u64,
}

/// Accepted logical use of one connection. Counts never reset on protocol
/// Hello/Open/Close. Errors do not refund earlier accepted charges. These are
/// accepted wire-admission counters, including the eight-byte frame prefix,
/// not measurements of physical I/O or CPU instructions. A rejected oversized
/// prefix is not a physical-I/O meter entry and does not refund prior charges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WideCellUsageV2 {
    pub read_bytes: u64,
    pub written_bytes: u64,
    pub messages: u64,
    pub steps: u64,
    pub work: u64,
    /// Cumulative caller-retained response backing; never reclaimed implicitly.
    pub exported_bytes: u64,
}

fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| "wide cell budget addition overflow".into())
}

fn mul(a: u64, b: u64) -> Result<u64, String> {
    a.checked_mul(b).ok_or_else(|| "wide cell budget multiplication overflow".into())
}

fn ceiling(value: u64, limit: u64, name: &str) -> Result<(), String> {
    if value > limit {
        return Err(format!("wide cell {name} ceiling"));
    }
    Ok(())
}

fn quote(limits: WideCellLimitsV2, workspace_bytes: u64) -> Result<WideCellSessionQuoteV2, String> {
    ceiling(workspace_bytes, limits.max_workspace_bytes, "executor workspace")?;
    let operation_peak_bytes = add(
        add(mul(limits.max_frame_bytes, 2)?, mul(limits.max_decoded_bytes, 2)?)?,
        add(workspace_bytes, add(PARSER_SCRATCH_BYTES, limits.external_retained_bytes)?)?,
    )?;
    ceiling(operation_peak_bytes, limits.max_operation_bytes, "operation backing")?;
    Ok(WideCellSessionQuoteV2 {
        frame_bytes: limits.max_frame_bytes,
        decoded_bytes: limits.max_decoded_bytes,
        workspace_bytes,
        external_retained_bytes: limits.external_retained_bytes,
        operation_peak_bytes,
    })
}

/// Client backing does not include a local executor workspace.
pub fn quote_client(limits: WideCellLimitsV2) -> Result<WideCellSessionQuoteV2, String> {
    quote(limits, 0)
}

/// Borrow the already admitted actual wide geometry, plan and original context.
/// The shared storage helper counts metadata without encoding into a new Vec;
/// this operation neither decodes a program nor constructs another executor.
pub fn quote_server(
    space: &PalwTirStepSpaceV2,
    plan: &TirPlan,
    ctx: &PalwJobContextV2,
    limits: WideCellLimitsV2,
) -> Result<WideCellSessionQuoteV2, String> {
    quote(limits, tir_executor_storage_need_wide_q24_v2(space, plan, ctx)?)
}

/// Private connection owner. Admissions calculate every new counter first and
/// commit atomically only on success. A later decode/execution/I/O failure does
/// not undo that accepted admission, and there is deliberately no reset API.
pub(crate) struct WireBudget {
    limits: WideCellLimitsV2,
    read_bytes: u64,
    written_bytes: u64,
    messages: u64,
    steps: u64,
    work: u64,
    exported_bytes: u64,
}

impl WireBudget {
    pub(crate) fn new(limits: WideCellLimitsV2) -> Self {
        Self { limits, read_bytes: 0, written_bytes: 0, messages: 0, steps: 0, work: 0, exported_bytes: 0 }
    }

    pub(crate) fn usage(&self) -> WideCellUsageV2 {
        WideCellUsageV2 {
            read_bytes: self.read_bytes,
            written_bytes: self.written_bytes,
            messages: self.messages,
            steps: self.steps,
            work: self.work,
            exported_bytes: self.exported_bytes,
        }
    }

    /// `n` is the raw body length, excluding its eight-byte transport prefix.
    /// Admit the full wire charge before allocating that body. Codec limits
    /// separately cap all nested requests before their respective allocations.
    pub(crate) fn admit_read(&mut self, n: u64) -> Result<(), String> {
        ceiling(n, self.limits.max_frame_bytes, "read frame")?;
        let framed = add(n, 8)?;
        let read_bytes = add(self.read_bytes, framed)?;
        let messages = add(self.messages, 1)?;
        let work = add(self.work, mul(framed, 128)?)?;
        ceiling(read_bytes, self.limits.max_total_read_bytes, "cumulative read bytes")?;
        ceiling(messages, self.limits.max_messages, "cumulative message count")?;
        ceiling(work, self.limits.max_work, "cumulative work")?;
        self.read_bytes = read_bytes;
        self.messages = messages;
        self.work = work;
        Ok(())
    }

    /// `n` is the raw body length. Charge it and the eight-byte transport prefix
    /// before allocating/serializing its frame.
    pub(crate) fn admit_write(&mut self, n: u64) -> Result<(), String> {
        ceiling(n, self.limits.max_frame_bytes, "write frame")?;
        let framed = add(n, 8)?;
        let written_bytes = add(self.written_bytes, framed)?;
        let messages = add(self.messages, 1)?;
        let work = add(self.work, mul(framed, 128)?)?;
        ceiling(written_bytes, self.limits.max_total_write_bytes, "cumulative write bytes")?;
        ceiling(messages, self.limits.max_messages, "cumulative message count")?;
        ceiling(work, self.limits.max_work, "cumulative work")?;
        self.written_bytes = written_bytes;
        self.messages = messages;
        self.work = work;
        Ok(())
    }

    pub(crate) fn charge_work(&mut self, n: u64) -> Result<(), String> {
        let work = add(self.work, n)?;
        ceiling(work, self.limits.max_work, "cumulative work")?;
        self.work = work;
        Ok(())
    }

    /// Admit one step and its symbolic kernel work before executor activity.
    pub(crate) fn admit_step(&mut self, kernel_work: u64) -> Result<(), String> {
        let steps = add(self.steps, 1)?;
        let work = add(self.work, kernel_work)?;
        ceiling(steps, self.limits.max_steps, "cumulative step count")?;
        ceiling(work, self.limits.max_work, "cumulative work")?;
        self.steps = steps;
        self.work = work;
        Ok(())
    }

    /// Previously exported client responses coexist with the next full base
    /// operation. Their counters survive every protocol state transition.
    pub(crate) fn ensure_peak(&self, base: &WideCellSessionQuoteV2) -> Result<(), String> {
        ceiling(add(base.operation_peak_bytes, self.exported_bytes)?, self.limits.max_operation_bytes, "retained export backing")
    }

    /// The current decoded response is already covered by the current base
    /// operation. At handoff only accumulate its ownership; do not add it to
    /// that base a second time. The next operation must call ensure_peak.
    pub(crate) fn export(&mut self, n: u64) -> Result<(), String> {
        let exported_bytes = add(self.exported_bytes, n)?;
        ceiling(exported_bytes, self.limits.max_operation_bytes, "cumulative exported backing")?;
        self.exported_bytes = exported_bytes;
        Ok(())
    }
}
