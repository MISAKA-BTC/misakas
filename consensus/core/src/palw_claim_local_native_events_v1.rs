//! Native credit events use their real journal point. Session birth remains birth.
//! This projector consumes an authenticated retained fold frame and its ordered
//! journal, not a producer list, output-hash coincidence, or a current snapshot.
use super::*;
use crate::palw_native_resource_v1::{
    BTreeNodeEnvelopeV1, NativeDecodeErrorV1, NativeRetainedV1, TirNativeRequestsV1, native_value_requests_v1,
    preflight_native_value_v1, reserve_native_requests_v1,
};
use crate::palw_state_v2::{PALW_EXEC_V2_TABLE_ROOTS_V1, PALW_EXEC_V2_TABLE_SLICES_V1};
use crate::palw_work_slice_v2::{PalwWorkRootPhaseV2, PalwWorkRootV2, PalwWorkSliceRowV2};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwClaimLocalNativeEventV1 {
    pub work: PalwClaimLocalWorkRefV1,
    pub point: PalwBlockContextV2,
    /// Position in this authenticated native journal, including other owners.
    pub journal_ordinal: u64,
    pub inputs: BTreeSet<PalwClaimLocalWorkRefV1>,
}

fn decode<T: BorshDeserialize + BorshSerialize + crate::palw_native_resource_v1::NativeWireScanV1>(
    bytes: &[u8],
) -> Result<NativeRetainedV1<T>, PalwClaimCheckpointErrorV1> {
    decode_claim_local_value_v1(bytes).map_err(|e| match e {
        NativeDecodeErrorV1::Resource(e) => PalwClaimCheckpointErrorV1::Resource(e),
        NativeDecodeErrorV1::Decode(_) => PalwClaimCheckpointErrorV1::Semantic("malformed native event journal row"),
    })
}
impl PalwClaimLiabilityFrameV1 {
    pub fn native_credit_events_v1(&self) -> Result<NativeRetainedV1<Vec<PalwClaimLocalNativeEventV1>>, PalwClaimCheckpointErrorV1> {
        self.with_context_carriages_v1(|prior, _current| {
            let overflow = || {
                PalwClaimCheckpointErrorV1::Resource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<Self>(
                    "native credit event inventory",
                ))
            };
            let mut q = TirNativeRequestsV1::new();
            q.add_v1(native_value_requests_v1(&prior.exec_v2).map_err(PalwClaimCheckpointErrorV1::Resource)?)
                .map_err(PalwClaimCheckpointErrorV1::Resource)?;
            let count = u64::try_from(self.native_changes.len()).map_err(|_| overflow())?;
            let slices = (prior.exec_v2.slices.len() as u64).checked_add(count).ok_or_else(overflow)?;
            q.exact_v1::<PalwClaimLocalNativeEventV1>(count).map_err(PalwClaimCheckpointErrorV1::Resource)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1, ()>>(
                count.checked_mul(slices.checked_add(1).ok_or_else(overflow)?).ok_or_else(overflow)?,
            )
            .map_err(PalwClaimCheckpointErrorV1::Resource)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, PalwWorkRootV2>>(count).map_err(PalwClaimCheckpointErrorV1::Resource)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<(Hash64, u32), PalwWorkSliceRowV2>>(count)
                .map_err(PalwClaimCheckpointErrorV1::Resource)?;
            // Admit every possible replacement's deep heap before copying it into
            // the replay map. Decoders separately retain their temporary owner.
            for c in self.native_changes.iter().filter(|c| c.owner == PalwClaimLocalRowOwnerV1::ExecV2) {
                if let Some(bytes) = &c.new {
                    let shape = match c.table {
                        PALW_EXEC_V2_TABLE_ROOTS_V1 => {
                            Some(preflight_native_value_v1::<PalwWorkRootV2>(bytes.as_slice(), bytes.len() as u64))
                        }
                        PALW_EXEC_V2_TABLE_SLICES_V1 => {
                            Some(preflight_native_value_v1::<PalwWorkSliceRowV2>(bytes.as_slice(), bytes.len() as u64))
                        }
                        _ => None,
                    };
                    if let Some(shape) = shape {
                        let shape = shape.map_err(|e| claim_local_checkpoint_io_v1(e, "native event preflight refused row"))?;
                        q.add_v1(shape.logical_requests_v1()).map_err(PalwClaimCheckpointErrorV1::Resource)?;
                    }
                }
            }
            let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwClaimCheckpointErrorV1::Resource)?;
            let mut exec = prior.exec_v2.clone();
            let mut events = Vec::with_capacity(self.native_changes.len());
            for (ordinal, c) in self.native_changes.iter().enumerate() {
                if c.owner != PalwClaimLocalRowOwnerV1::ExecV2 {
                    continue;
                }
                match c.table {
                    PALW_EXEC_V2_TABLE_SLICES_V1 => {
                        let key: (Hash64, u32) =
                            borsh::from_slice(&c.key).map_err(|_| PalwClaimCheckpointErrorV1::Semantic("native slice key"))?;
                        if let Some(bytes) = &c.new {
                            exec.slices.insert(key, (*decode::<PalwWorkSliceRowV2>(bytes)?).clone());
                        } else {
                            exec.slices.remove(&key);
                        }
                    }
                    PALW_EXEC_V2_TABLE_ROOTS_V1 => {
                        let root: Hash64 =
                            borsh::from_slice(&c.key).map_err(|_| PalwClaimCheckpointErrorV1::Semantic("native session key"))?;
                        let Some(bytes) = &c.new else {
                            exec.roots.remove(&root);
                            continue;
                        };
                        let row = decode::<PalwWorkRootV2>(bytes)?;
                        if let PalwWorkRootPhaseV2::Settled { settled_daa } = row.phase {
                            let original = exec
                                .roots
                                .get(&root)
                                .ok_or(PalwClaimCheckpointErrorV1::Semantic("native settlement has no prior session"))?;
                            if !matches!(original.phase, PalwWorkRootPhaseV2::Settled { .. }) {
                                if settled_daa != self.point.daa_score || original.opened_daa > settled_daa {
                                    return Err("native settlement changed its executed point".into());
                                }
                                let mut inputs = BTreeSet::from([PalwClaimLocalWorkRefV1::ExecSession { root }]);
                                for index in 0..row.next_index {
                                    let slice = exec
                                        .slices
                                        .get(&(root, index))
                                        .ok_or(PalwClaimCheckpointErrorV1::Semantic("native settlement consumes unavailable slice"))?;
                                    if slice.accepted_daa > settled_daa {
                                        return Err("native settlement consumes future slice".into());
                                    }
                                    inputs.insert(PalwClaimLocalWorkRefV1::ExecSlice { root, index });
                                }
                                events.push(PalwClaimLocalNativeEventV1 {
                                    work: PalwClaimLocalWorkRefV1::ExecSettlement { root, block: self.point.block },
                                    point: self.point,
                                    journal_ordinal: ordinal as u64,
                                    inputs,
                                });
                            }
                        }
                        exec.roots.insert(root, (*row).clone());
                    }
                    _ => {}
                }
            }
            Ok(NativeRetainedV1::from_owner_reserved_v1(events, lease))
        })
    }
}
