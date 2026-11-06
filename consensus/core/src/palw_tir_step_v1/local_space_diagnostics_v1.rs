//! Finite typed roots/diagnostics for existing local V1/strict-V2 constructors.
//! No constructor, decoder, acceptance decision, host lease or native capability.
use super::{BlockSpaceV1, CommitNodeV1, PalwTirStateInstanceV1, PalwTirStepError, PalwTirStepSpaceV1, PalwTirStepSpaceV2};
use crate::palw_native_resource_v1::{
    NativeCoverageV1, SourceProgramLocalErrorV1, TirNativeRequestsV1, growing_vec_request_capacity_v1,
    require_source_program_local_profile_v1,
};
use std::{any::type_name, mem::size_of};

/// Logical requested objects only, not complete compiler stack frames or RSS.
/// Root bytes and diagnostic bytes are disjoint. Diagnostic result envelopes
/// deliberately also count complete result roots, even when that duplicates a
/// constructor destination: explicit conservative overlap, not hidden slack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTirStepSpaceFixedRequestsV1 {
    pub space_root_bytes: u64,
    pub constructor_diagnostic_bytes: u64,
    pub coverage_diagnostic_bytes: u64,
    pub diagnostic_peak_bytes: u64,
}
type E = SourceProgramLocalErrorV1;
fn add(a: u64, b: u64) -> Result<u64, E> {
    a.checked_add(b).ok_or(E::ArithmeticOverflow)
}
fn request(n: usize) -> Result<u64, E> {
    growing_vec_request_capacity_v1::<u8>(u64::try_from(n).map_err(|_| E::ArithmeticOverflow)?).ok_or(E::ArithmeticOverflow)
}
fn decimal_digits(mut n: u64) -> usize {
    let mut digits = 1;
    while n >= 10 {
        n /= 10;
        digits += 1;
    }
    digits
}
fn coverage_pair<T>(component: &'static str) -> Result<u64, E> {
    add(request(component.len())?, request(type_name::<T>().len())?)
}

/// Call only under the already acquired WHOLE configured metadata owner guard.
/// Success and fixed public errors allocate no heap. Exact private native type
/// names are borrowed here; SDK callers need not copy or guess their lengths.
///
/// This augments geometry-only quote_local_tir_step_space_additional_v1. It
/// does NOT include geometry Vecs, layout/program/ProgramInfo allocations,
/// original Source, class/header/compiler/context/prompt or caller error maps.
/// The complete Source admitted envelope must separately coexist: its FOUR
/// named diagnostic writers already cover decode/NF/validation error text and
/// their Program-prefix conversion. We add constructor-only layout/literal
/// text and private NativeCoverage overflow strings here. No source byte-size
/// multiplier or Source.quote().owner_requests() floor substitutes for that S.
///
/// Bounds below apply to canonical/NF Source programs (MAX_BLOCKS/NODES/STATES)
/// and the unchanged existing constructors/private geometry arithmetic only.
/// Allocator metadata, physical quotas and arbitrary retained errors/aliases
/// remain external. The result is data and grants no constructor/native right.
pub fn quote_local_tir_step_space_fixed_requests_v1() -> Result<LocalTirStepSpaceFixedRequestsV1, E> {
    require_source_program_local_profile_v1()?;
    // Actual constructor literals, independently maintained from layout rules.
    let literals = [
        "wide Q24 requires an explicit v2 step space; legacy step leaves are four-byte lanes",
        "max_context must be in [1, history_bound]",
        "the checkpoint interval must be at least 1",
        "h_tile must be a power of two in [1, 4096]",
        "the post block writes state: it runs only at positions whose logits are consumed",
        "under the tiled logits scheme the logits node's tile length divides the scheme's 4,096 lanes",
        "more commit tiles than committed nodes",
        "one state tile per state declaration",
        "unsupported class version",
        "class exceeds the existing program/layout format bounds",
        "v2 steps require the exact wide Q24 scheme",
        "wide logits must be the committed i64 final post node",
        "fewer commit tiles than committed nodes",
        "validation metadata does not match the actual program",
    ];
    let literal = literals.iter().map(|s| s.len()).max().unwrap_or(0);
    use misaka_palw_tir::program::{MAX_BLOCKS, MAX_NODES_PER_BLOCK, MAX_STATES};
    // Literal fragments + actual decimal maxima; no format!/String allocation.
    let block = decimal_digits((MAX_BLOCKS - 1) as u64);
    let node = decimal_digits((MAX_NODES_PER_BLOCK - 1) as u64);
    let state = decimal_digits((MAX_STATES - 1) as u64);
    let tile = decimal_digits(u64::from(u32::MAX));
    let formatted = [
        "layout version  is not ".len() + decimal_digits(u64::from(u16::MAX)) + 1,
        "block  node : tile_len  is outside [4, 65536]".len() + block + node + tile,
        "state : a checkpoint tile of  lanes is outside [4, 65536]".len() + state + tile,
        "history : h_tile ×  lanes must be in [1, 65536]".len() + state + tile,
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    // Pinned Rust formatting initial estimate <=2*total text; G includes checked
    // geometric growth. Static String::from needs at most its literal length.
    let constructor_payload = (literal as u64).max(request(formatted.checked_mul(2).ok_or(E::ArithmeticOverflow)?)?);
    let constructor_roots = [
        size_of::<PalwTirStepError>(),
        size_of::<String>(),
        size_of::<Result<PalwTirStepSpaceV1, PalwTirStepError>>(),
        size_of::<Result<PalwTirStepSpaceV2, PalwTirStepError>>(),
    ]
    .into_iter()
    .try_fold(0, |n, v| add(n, v as u64))?;
    // Superset of the reachable existing geometry-overflow (component,T) pairs;
    // only one is live. Sum roots but MAX mutually exclusive string payloads.
    let coverage_payload = [
        coverage_pair::<PalwTirStepSpaceV1>("StepSpace structural cardinality")?,
        coverage_pair::<BlockSpaceV1>("TIR concrete array request")?,
        coverage_pair::<CommitNodeV1>("TIR concrete array request")?,
        coverage_pair::<CommitNodeV1>("TIR growing Vec capacity")?,
        coverage_pair::<(u8, Option<u16>)>("TIR concrete array request")?,
        coverage_pair::<u32>("TIR concrete array request")?,
        coverage_pair::<u32>("TIR growing Vec capacity")?,
        coverage_pair::<PalwTirStateInstanceV1>("TIR concrete array request")?,
        coverage_pair::<PalwTirStateInstanceV1>("TIR growing Vec capacity")?,
        coverage_pair::<Option<u16>>("TIR concrete array request")?,
        coverage_pair::<Option<u16>>("TIR growing Vec capacity")?,
        coverage_pair::<TirNativeRequestsV1>("coexisting TIR typed requests")?,
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    let coverage_roots = [
        size_of::<NativeCoverageV1>(),
        2 * size_of::<String>(),
        size_of::<Result<u64, NativeCoverageV1>>(),
        size_of::<Result<u64, E>>(),
    ]
    .into_iter()
    .try_fold(0, |n, v| add(n, v as u64))?;
    let constructor_diagnostic_bytes = add(constructor_roots, constructor_payload)?;
    let coverage_diagnostic_bytes = add(coverage_roots, coverage_payload)?;
    Ok(LocalTirStepSpaceFixedRequestsV1 {
        space_root_bytes: size_of::<PalwTirStepSpaceV1>().max(size_of::<PalwTirStepSpaceV2>()) as u64,
        constructor_diagnostic_bytes,
        coverage_diagnostic_bytes,
        diagnostic_peak_bytes: constructor_diagnostic_bytes.max(coverage_diagnostic_bytes),
    })
}

#[cfg(test)]
mod tests;
