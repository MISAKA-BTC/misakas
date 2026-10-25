//! Borrowed preflight for court timeout caller clones. The transition's existing
//! construction owner keeps these requests through verdict, rearm and output.
use super::super::*;

pub(super) fn quote(state: &PalwChainStateV2, daa: u64) -> Result<native_writers::NativeWriteQuoteV1, PalwStateV2Error> {
    let mut q = native_writers::NativeWriteQuoteV1::new("court deadline caller clones");
    // Match the existing strict timeout predicate. A later court close may
    // remove another due row, making this a conservative borrowed envelope.
    for (_, id) in state.court_deadlines.iter().take_while(|(deadline, _)| *deadline < daa) {
        // Missing rows retain the sweep's original MissingSession ordering.
        let Some(session) = state.court_sessions.get(id) else { continue };
        q.copy(session)?.copy(session)?; // working session + refreshed session
        if let Some(claim) = state.claims.get(&session.claim) {
            q.copy(claim)?;
        }
        if let Some(phase) = state.tir_dissections.get(id) {
            q.copy(phase.as_ref())?;
        }
    }
    Ok(q)
}
