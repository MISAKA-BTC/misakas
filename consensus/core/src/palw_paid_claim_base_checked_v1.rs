//! Structural paid-base lookup for native consumers. Presence, corruption and
//! local capacity are distinct. No ledger reconstruction or approval is done.
use super::*;
use crate::palw_kernel_route_v1::PalwKernelRouteStateV1;
use crate::palw_native_resource_v1::{
    NativeDecodeErrorV1 as E, NativeRetainedV1, preflight_native_value_v1, retain_native_value_decode_classified_v1,
};
fn bad(why: &str) -> E<String> {
    E::Decode(why.into())
}
impl PalwKernelRouteStateV1 {
    /// Read the exact base index and both authority indexes without rebuilding
    /// the ledger (which projects paid consumer rows itself). Only a genuinely
    /// absent base index is None; a present incomplete/corrupt binding is Err.
    pub fn paid_claim_authority_for_base_checked_v1(
        &self,
        claim: &Digest,
    ) -> Result<Option<NativeRetainedV1<PalwPaidClaimAuthorityV2>>, E<String>> {
        let mut key = [0u8; 65];
        key[0] = PAID_CLAIM_BASE_INDEX_PREFIX_V3;
        key[1..].copy_from_slice(claim);
        let Some(bytes) =
            crate::palw_material_challenge_checkpoint_v4::material_challenge_fixed_row_v4(self, PAID_CLAIM_ENROLLMENTS_V2, &key)
                .map_err(E::Resource)?
        else {
            return Ok(None);
        };
        preflight_native_value_v1::<PalwPaidClaimIndexV3>(bytes, bytes.len() as u64).map_err(|why| E::Decode(why.to_string()))?;
        let index = retain_native_value_decode_classified_v1(bytes, || {
            borsh::from_slice::<PalwPaidClaimIndexV3>(bytes).map_err(|why| why.to_string())
        })?;
        if index.version != 3 || index.kind != PalwPaidClaimIndexKindV3::BaseClaim || index.owner != *claim {
            return Err(bad("paid base index substitutes its version/kind/owner"));
        }
        let authority = self.paid_claim_authority_checked_v6(&index.target)?;
        if authority.version != 2 || authority.enrollment.version != 2 || authority.enrollment.base_claim != *claim {
            return Err(bad("paid base authority substitutes its version/base claim"));
        }
        let indexed = self.paid_claim_enrollment_checked_v6(&authority.enrollment.id())?;
        if *indexed != *authority {
            return Err(bad("paid base/enrollment authority indexes disagree"));
        }
        Ok(Some(authority))
    }
}
