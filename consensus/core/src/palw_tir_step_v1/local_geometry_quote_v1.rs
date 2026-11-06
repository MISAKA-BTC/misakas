//! Additive owner-local access to the EXISTING typed StepSpace Vec census.
//! No decoder, layout/profile decision, native admission, lease or new recipe.

use super::step_space_requests_from_counts_v1;
use crate::palw_native_resource_v1::{LocalSourceProgramOwnerV1, SourceProgramLocalErrorV1, require_source_program_local_profile_v1};
use misaka_palw_tir::program::StateKind;

/// Additional construction/retained StepSpace Vec requests for THIS immutable
/// canonical/NF Source. Counts only B block lengths and S state declarations;
/// callers must precharge those local visits on every attempted/retried quote.
/// The success path allocates no collection, program encoding or diagnostic.
///
/// Acquire the whole configured metadata owner guard BEFORE calling this helper.
/// Existing private arithmetic may build a bounded NativeCoverage diagnostic on
/// overflow; it is dropped here and only the fixed public error escapes. This
/// local facade does not change the old private/native error order or semantics.
///
/// Does NOT include the Space root, original Source owner, a fresh decoded
/// program/ProgramInfo/validation pass, layout/class copies, header, compiler,
/// hash/error scratch, context/prompt or caller aliases. In particular, never
/// replace complete Source admission with Source.quote().owner_requests(): that
/// getter is ONLY the struct delta. Retain every co-live owner and host guard.
///
/// This is the same numeric census for narrow/wide geometry storage. It neither
/// accepts a profile nor bypasses the unchanged V1/strict-V2 constructors.
/// A returned u64 is logical data, never a capability or physical RSS bound.
pub fn quote_local_tir_step_space_additional_v1(source: &LocalSourceProgramOwnerV1) -> Result<u64, SourceProgramLocalErrorV1> {
    require_source_program_local_profile_v1()?;
    let count = |n: usize| u64::try_from(n).map_err(|_| SourceProgramLocalErrorV1::ArithmeticOverflow);
    let p = source.program();
    let blocks = count(p.blocks.len())?;
    let nodes = p.blocks.iter().try_fold(0u64, |total, block| {
        total.checked_add(count(block.nodes.len())?).ok_or(SourceProgramLocalErrorV1::ArithmeticOverflow)
    })?;
    let layers = count(p.schedule.layers.len())?;
    let (mut fixed_global, mut fixed_layer, mut hist_global, mut hist_layer) = (0u64, 0u64, 0u64, 0u64);
    for state in &p.states {
        let slot = match (state.kind, state.per_layer) {
            (StateKind::Fixed { .. }, false) => &mut fixed_global,
            (StateKind::Fixed { .. }, true) => &mut fixed_layer,
            (StateKind::Hist { .. }, false) => &mut hist_global,
            (StateKind::Hist { .. }, true) => &mut hist_layer,
        };
        *slot = slot.checked_add(1).ok_or(SourceProgramLocalErrorV1::ArithmeticOverflow)?;
    }
    step_space_requests_from_counts_v1(blocks, nodes, layers, fixed_global, fixed_layer, hist_global, hist_layer).map_err(|error| {
        drop(error);
        SourceProgramLocalErrorV1::ArithmeticOverflow
    })
}

#[cfg(test)]
mod tests;
