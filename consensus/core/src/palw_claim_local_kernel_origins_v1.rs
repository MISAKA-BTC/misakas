//! Finite RFC23 admission provenance for closed kernel claims. This preserves
//! the exact executed birth of a still-rooted kernel row; it never creates a
//! ChainClaim identity, a new approval, or a model-computation proof.
use super::palw_claim_local_rewards_v1 as rewards;
use super::*;
use crate::palw_claim_local_v1::*;
use crate::palw_native_resource_v1::NativeRetainedV1;
use misaka_palw_kernel::ledger::ClaimRowV1;

pub const PALW_COMPACT_KERNEL_ORIGIN_PREFIX_V1: u8 = b'K';
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PalwClosedKernelOriginV1 {
    pub version: u16,
    pub ruleset: crate::Hash,
    pub claim: Hash64,
    pub birth: PalwBlockContextV2,
    pub source_identity: Hash64,
    pub closed_at: PalwBlockContextV2,
    pub material_until: u64,
}
crate::palw_native_resource_v1::native_struct!(PalwClosedKernelOriginV1;version,ruleset,claim,birth,source_identity,closed_at,material_until);
crate::palw_native_resource_v1::wire::wire_struct!(PalwClosedKernelOriginV1;u16,crate::Hash,Hash64,PalwBlockContextV2,Hash64,PalwBlockContextV2,u64);
fn bad(s: &str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.into())
}
fn key(id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = PALW_COMPACT_KERNEL_ORIGIN_PREFIX_V1;
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
fn identity(row: &ClaimRowV1) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
        b"misaka-palw/claim-local/kernel-origin/v1",
        &(row.producer, row.class_binding_id, row.job_id, &row.body, row.committed_daa, row.sealed_daa),
    ))
}
fn closed(state: &PalwChainStateV2, id: Hash64, row: &ClaimRowV1, at: u64) -> bool {
    use crate::palw_exec_v2_verify::{PalwSliceVerificationV1 as V, palw_exec_v2_claim_outcome_v1 as outcome};
    row.liability_until.is_some_and(|until| at > until)
        && match outcome(row, at) {
            V::ProvenFalse | V::Defaulted => true,
            V::Verified { .. } => {
                state.kernel_route.as_ref().is_some_and(|r| crate::palw_exec_v2_verify::palw_exec_v2_selected_approval_v1(r, &id))
            }
            V::Pending => false,
        }
}
pub(super) fn read(
    state: &PalwChainStateV2,
    id: Hash64,
) -> Result<Option<NativeRetainedV1<PalwClosedKernelOriginV1>>, PalwStateV2Error> {
    let Some(route) = &state.kernel_route else { return Ok(None) };
    let k = key(id);
    let Some(raw) = route.aux.iter().find(|((t, key), _)| *t == PALW_CLAIM_LOCAL_TABLE_V1 && key.as_slice() == k).map(|(_, raw)| raw)
    else {
        return Ok(None);
    };
    let origin = rewards::decoded::<PalwClosedKernelOriginV1>(raw)?;
    let Some(raw) = rewards::kernel_claim_row(state, &id) else {
        return Err(bad("compact kernel origin lost its rooted native source"));
    };
    let row = rewards::decoded::<ClaimRowV1>(raw)?;
    let _keys = crate::palw_native_resource_v1::reserve_native_requests_v1(512).map_err(PalwStateV2Error::NativeResource)?;
    if origin.version != 1
        || origin.claim != id
        || origin.birth.block == ZERO_HASH64
        || origin.birth.daa_score != row.committed_daa
        || origin.source_identity != identity(&row)
        || origin.birth.blue_score > origin.closed_at.blue_score
        || origin.birth.daa_score > origin.closed_at.daa_score
        || row.liability_until != Some(origin.material_until)
        || !closed(state, id, &row, origin.closed_at.daa_score)
        || state.last_point.is_none_or(|p| p.blue_score < origin.closed_at.blue_score || p.daa_score < origin.closed_at.daa_score)
    {
        return Err(bad("compact kernel origin actual identity/birth/material/selected closure differs"));
    }
    Ok(Some(origin))
}
pub(super) fn mint_from_frame(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    frame: &PalwClaimLiabilityFrameV1,
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) {
        return Ok(());
    }
    let Some(ruleset) = b.extras.native_fact_ruleset else { return Ok(()) };
    let _keys = crate::palw_native_resource_v1::reserve_native_requests_v1(512).map_err(PalwStateV2Error::NativeResource)?;
    for event in &frame.native_changes {
        if event.owner != PalwClaimLocalRowOwnerV1::KernelRoute
            || event.table != misaka_palw_kernel::rows::TABLE_CLAIMS_V1
            || event.old.is_some()
        {
            continue;
        }
        let Some(initial) = &event.new else { continue };
        let id = Hash64::from_bytes(event.key.as_slice().try_into().map_err(|_| bad("kernel birth key width"))?);
        if read(&b.state, id)?.is_some() {
            continue;
        }
        let Some(current) = rewards::kernel_claim_row(&b.state, &id) else { continue };
        let row = rewards::decoded::<ClaimRowV1>(current)?;
        if !closed(&b.state, id, &row, ctx.daa_score) {
            continue;
        }
        let original = rewards::decoded::<ClaimRowV1>(initial)?;
        if original.committed_daa != frame.point.daa_score || identity(&original) != identity(&row) {
            return Err(bad("closed kernel origin changed its executed admission"));
        }
        let origin = PalwClosedKernelOriginV1 {
            version: 1,
            ruleset,
            claim: id,
            birth: frame.point,
            source_identity: identity(&original),
            closed_at: *ctx,
            material_until: row.liability_until.expect("closed material scope"),
        };
        // A fixed-size typed row, admitted by the existing journal writer before
        // retaining/serializing either its new value or its old/new delta copy.
        b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &key(id), Some(&origin))?;
    }
    Ok(())
}
pub(super) fn validate_context(state: &PalwChainStateV2, global: &crate::config::params::Params) -> Result<(), PalwStateV2Error> {
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &global.palw_consensus_mode else {
        if state.kernel_route.as_ref().is_some_and(|r| {
            r.aux.keys().any(|(t, k)| *t == PALW_CLAIM_LOCAL_TABLE_V1 && k.len() == 65 && k[0] == PALW_COMPACT_KERNEL_ORIGIN_PREFIX_V1)
        }) {
            return Err(bad("compact kernel origin requires actual V2 network rules"));
        }
        return Ok(());
    };
    if let Some(route) = &state.kernel_route {
        for (table, k) in route.aux.keys() {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || k.len() != 65 || k[0] != PALW_COMPACT_KERNEL_ORIGIN_PREFIX_V1 {
                continue;
            }
            let id = Hash64::from_bytes(k[1..].try_into().expect("fixed compact kernel key"));
            let row = read(state, id)?.expect("existing compact kernel source");
            if row.ruleset != global.consensus_params_id() || !bundle.state.claim_local_settlement_active_at(row.birth.daa_score) {
                return Err(bad("compact kernel origin differs from actual network/fence"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rfc23s2_compact_kernel_birth_has_exact_typed_wire_and_namespace() {
        let id = Hash64::from_bytes([7; 64]);
        let point = PalwBlockContextV2 { block: Hash64::from_bytes([3; 64]), daa_score: 17, blue_score: 11, subsidy: 0 };
        let origin = PalwClosedKernelOriginV1 {
            version: 1,
            ruleset: crate::Hash::from_bytes([9; 32]),
            claim: id,
            birth: point,
            source_identity: Hash64::from_bytes([4; 64]),
            closed_at: PalwBlockContextV2 { daa_score: 100, blue_score: 80, ..point },
            material_until: 99,
        };
        let bytes = borsh::to_vec(&origin).unwrap();
        crate::palw_native_resource_v1::preflight_native_value_v1::<PalwClosedKernelOriginV1>(bytes.as_slice(), bytes.len() as u64)
            .unwrap();
        assert_eq!(*rewards::decoded::<PalwClosedKernelOriginV1>(&bytes).unwrap(), origin);
        assert!(rewards::decoded::<PalwClosedKernelOriginV1>(&bytes[..bytes.len() - 1]).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(rewards::decoded::<PalwClosedKernelOriginV1>(&trailing).is_err());
        assert_eq!(key(id)[0], b'K');
        assert_eq!(&key(id)[1..], id.as_byte_slice());
        // These are different liability namespaces even for identical hashes.
        assert_ne!(key(id)[0], PALW_COMPACT_CLAIM_SOURCE_PREFIX_V1);
    }
}
