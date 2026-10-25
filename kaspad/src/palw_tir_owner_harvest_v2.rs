//! Publish authenticated finished material before recording its accepted reference.
//! A local read/allocation refusal must leave that reference retryable.
use super::{OwnerCaseV1, PalwTirPublicFinishedV1};
use kaspa_consensus_core::Hash64;
use std::sync::Arc;

pub(super) fn harvest_accepted_v2(
    case: &mut OwnerCaseV1,
    id: Hash64,
    load: impl FnOnce() -> Result<Option<PalwTirPublicFinishedV1>, String>,
) -> Result<bool, String> {
    if case.harvested.contains(&id) {
        return Ok(false);
    }
    let Some(material) = load()? else {
        return Ok(false);
    };
    match material {
        PalwTirPublicFinishedV1::Unit(unit) => {
            if !case.units.iter().any(|u| u.request_v1() == unit.request_v1()) {
                case.units.push(Arc::new(unit));
            }
        }
        PalwTirPublicFinishedV1::Ids(ids) => case.ids = Some(ids),
    }
    case.harvested.insert(id);
    Ok(true)
}

#[cfg(test)]
#[path = "palw_tir_owner_harvest_tests_v2.rs"]
mod tests;
