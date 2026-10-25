//! Actual retained held transition: typed predecessor assembly and guard transfer.
//! The replay/serialization performed by honest_chunks_v2 is still a separate boundary.
use super::*;
use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, memory_v2::PalwGdnCourtErrorV2};

#[allow(clippy::too_many_arguments)]
pub(super) fn transition_with_owned_predecessor_v3<K: Base0FpRecomputeKernelsV1>(
    material: &Base0GdnMaterialV2<'_>,
    leg: &Base0CheckpointsV1,
    node_slot: u32,
    head: u32,
    index: u32,
    accused: Base0GdnRetainedCheckpointWitnessV4,
    kernels: impl FnMut() -> K,
) -> Result<Base0GdnWitnessBuiltV4<PalwGdnTransitionRefutationV2>, NativeDecodeErrorV1<String>> {
    let kernels = std::cell::RefCell::new(kernels);
    let failure: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
    // A typed Result is intentional: predecessor construction Resource must not
    // disappear into checkpoint absence or a string-only callback side channel.
    let checkpoint = |i: u32| -> Result<Option<(PalwCheckpointLeafV2, Vec<Vec<u8>>)>, PalwGdnCourtErrorV2> {
        let Some(leaf) = leg.leaves.get(i as usize) else { return Ok(None) };
        let fresh = (&mut *kernels.borrow_mut())();
        match honest_chunks_v2(material, leaf, fresh) {
            Ok(Some(chunks)) => Ok(Some((leaf.clone(), chunks))),
            Ok(None) => {
                *failure.borrow_mut() =
                    Some(format!("checkpoint {i} is already not the job's state: accuse the first forged checkpoint"));
                Err(PalwStepRefuteError::Unadjudicable.into())
            }
            Err(why) => {
                *failure.borrow_mut() = Some(why);
                Err(PalwStepRefuteError::Unadjudicable.into())
            }
        }
    };
    let tile = tile_source_v2(material);
    let mut predecessor_owner = None;
    let built = accused.try_build_v4(|accused| {
        let built = kaspa_consensus_core::palw_step_refute::memory_v2::gdn_transition_refutation_with_accused_retained_v3(
            &capture_v2(material, leg),
            node_slot,
            head,
            index,
            accused,
            &tile,
            &checkpoint,
        )
        .map_err(|why| base0_gdn_court_error_with_replay_v3(why, failure.borrow_mut().take()))?;
        let (refutation, owner) = built.into_parts_v3();
        predecessor_owner = owner;
        Ok(refutation)
    })?;
    // No await or unguarded carrier return between the native owner and the
    // existing value-before-owner court/output handoff carrier.
    Ok(built.with_predecessor_owner_v3(predecessor_owner))
}
