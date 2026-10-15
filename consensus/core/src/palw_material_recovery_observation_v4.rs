//! Read-only native metadata for private-process acceptance observations.
//! Nothing here admits a session, clears material, prices service or approves a
//! release. Each variable row is decoded under its actual typed wire owner.
use crate::palw_material_release_recovery_v2::{
    PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, PalwMaterialRecoveryRowV2, PalwMaterialRecoveryTermsV2,
};
use crate::palw_monetary_material_v1::{PALW_MONETARY_MATERIAL_TABLE_V1, PalwMonetaryMaterialRowV1};
use crate::palw_native_resource_v1::{NativeRetainedV1, reserve_native_requests_v1, retain_native_value_decode_v1};
use crate::palw_state_v2::PalwChainStateV2;
use crate::tx::TransactionOutpoint;
use crate::{BlockHash, Hash64};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeRecoveryObservationRowV4 {
    pub root: Hash64,
    pub consent: Hash64,
    pub recovery: Hash64,
    pub class: Hash64,
    pub executor: TransactionOutpoint,
    pub artifact_root: Hash64,
    pub providers: [Option<TransactionOutpoint>; 8],
    pub accepted_daa: u64,
    pub enrollment_until_daa: u64,
    pub bound_claim: Option<Hash64>,
    pub enrollment_finalized: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeRecoveryObservationV4 {
    pub version: u16,
    pub checkpoint: BlockHash,
    pub daa: u64,
    pub import_epoch: u64,
    pub finalized_checkpoint: Option<BlockHash>,
    pub rows: Vec<NativeRecoveryObservationRowV4>,
    pub release_approval: bool,
    pub security_soundness_measured: bool,
}

fn recovery_row(state: &PalwChainStateV2, root: Hash64) -> Result<Option<NativeRetainedV1<PalwMaterialRecoveryRowV2>>, String> {
    let Some(route) = state.kernel_route() else { return Ok(None) };
    // Borrow the existing key instead of allocating a temporary Vec just to
    // perform the tuple lookup. Observation bounds do not authorize mutation.
    let Some((_, bytes)) = route
        .aux
        .iter()
        .find(|((table, key), _)| *table == PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2 && key.as_slice() == root.as_byte_slice())
    else {
        return Ok(None);
    };
    retain_native_value_decode_v1(bytes, || borsh::from_slice::<PalwMaterialRecoveryRowV2>(bytes)).map(Some).map_err(|e| e.to_string())
}

fn recovery_id(terms: &PalwMaterialRecoveryTermsV2) -> Result<Hash64, String> {
    struct HashWriter<F>(F);
    impl<F: FnMut(&[u8])> std::io::Write for HashWriter<F> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            (self.0)(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let length =
        u64::try_from(borsh::object_length(terms).map_err(|e| e.to_string())?).map_err(|_| "recovery terms length overflow")?;
    let mut state = misaka_palw_kernel::hash::keyed(b"MISAKA/PALW/RELEASE-RECOVERY-CONSENT/V2");
    state.update(&length.to_le_bytes());
    borsh::to_writer(
        HashWriter(|bytes: &[u8]| {
            state.update(bytes);
        }),
        terms,
    )
    .map_err(|e| e.to_string())?;
    Ok(Hash64::from_bytes(misaka_palw_kernel::hash::finish(state)))
}

pub fn native_recovery_observation_v4(
    state: &PalwChainStateV2,
    checkpoint: BlockHash,
    daa: u64,
    import_epoch: u64,
    finalized: Option<(BlockHash, &PalwChainStateV2)>,
    maximum_rows: u32,
) -> Result<NativeRetainedV1<NativeRecoveryObservationV4>, String> {
    let count =
        state.kernel_route().map_or(0, |r| r.aux.keys().filter(|(table, _)| *table == PALW_MONETARY_MATERIAL_TABLE_V1).count());
    if count > maximum_rows as usize {
        return Err("native recovery observation exceeds its compiled contract bound".into());
    }
    // Fixed projection rows have no nested heap. Hashes stream the borrowed
    // fixed terms; provider/manifest maps are separately preflight-decoded.
    let bytes = (count as u64)
        .checked_mul(std::mem::size_of::<NativeRecoveryObservationRowV4>() as u64)
        .and_then(|n| n.checked_add(std::mem::size_of::<NativeRecoveryObservationV4>() as u64))
        .ok_or("native observation allocation overflow")?;
    let lease = reserve_native_requests_v1(bytes).map_err(|e| e.to_string())?;
    let mut rows = Vec::with_capacity(count);
    if let Some(route) = state.kernel_route() {
        for ((table, key), encoded) in &route.aux {
            if *table != PALW_MONETARY_MATERIAL_TABLE_V1 {
                continue;
            }
            let monetary = retain_native_value_decode_v1(encoded, || borsh::from_slice::<PalwMonetaryMaterialRowV1>(encoded))
                .map_err(|e| e.to_string())?;
            if key.as_slice() != monetary.terms.root.as_byte_slice() {
                return Err("native monetary row key differs from its signed root".into());
            }
            let Some(session) = monetary.terms.native_session.as_ref() else { continue };
            let Some(recovery) = recovery_row(state, monetary.terms.root)? else { continue };
            let PalwMaterialRecoveryRowV2::Active(contract) = &*recovery else {
                return Err("live native monetary row has a terminal recovery contract".into());
            };
            if contract.terms.root != monetary.terms.root || !contract.generations.iter().any(|g| g.consent == monetary.consent) {
                return Err("native monetary/recovery generation binding differs".into());
            }
            if !(2..=8).contains(&monetary.providers.len()) {
                return Err("native monetary provider count differs from its bounded roster".into());
            }
            let id = recovery_id(&contract.terms)?;
            let enrollment_finalized = if let Some((_, final_state)) = finalized {
                match recovery_row(final_state, monetary.terms.root)? {
                    Some(row) => {
                        recovery_id(&row.contract().terms)? == id
                            && row.contract().generations.iter().any(|g| g.consent == monetary.consent)
                    }
                    None => false,
                }
            } else {
                false
            };
            let mut providers = [None; 8];
            for (slot, provider) in providers.iter_mut().zip(&monetary.providers) {
                *slot = Some(provider.provider.0);
            }
            rows.push(NativeRecoveryObservationRowV4 {
                root: monetary.terms.root,
                consent: monetary.consent,
                recovery: id,
                class: session.class_id,
                executor: session.executor.0,
                artifact_root: session.artifact_root,
                providers,
                accepted_daa: monetary.accepted_daa,
                enrollment_until_daa: monetary.terms.enrollment_until_daa,
                bound_claim: monetary.native_binding.as_ref().map(|b| b.claim),
                enrollment_finalized,
            });
        }
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        NativeRecoveryObservationV4 {
            version: 4,
            checkpoint,
            daa,
            import_epoch,
            finalized_checkpoint: finalized.map(|(point, _)| point),
            rows,
            release_approval: false,
            security_soundness_measured: false,
        },
        lease,
    ))
}
