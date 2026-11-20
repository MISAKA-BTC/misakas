//! Opaque native input for the real ordered fold. A validated transport alone
//! cannot construct it: native original/draw/slot/signature preparation must
//! also succeed. This installs no public profile or production price policy.
use super::{ManifestRequestCarrierErrorV1, ValidatedManifestRequestCarrierV1, replay::SharedManifestRequestReplaySeedV1};
use crate::{
    Hash64, TransactionId,
    palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1, TirNativeRequestsV1, reserve_native_pending_requests_v1},
    palw_positive_contract_v2::tfp1_original::manifest_request::PreparedPaidManifestRequestV1,
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2},
    tx::ValidatedTransaction,
};
use std::sync::Arc;

/// Immutable Full-UTXO input for one physical acceptance position. This is
/// neither a verified Panel request nor a paid receipt. Rebuild it if UTXO
/// filtering changes the parent, inclusion point, ordinal or preceding fee.
#[derive(Clone)]
pub struct NativeManifestRequestCandidateV3 {
    seed: SharedManifestRequestReplaySeedV1,
}
crate::palw_native_resource_v1::native_struct!(NativeManifestRequestCandidateV3; seed);

fn transport_error(error: ManifestRequestCarrierErrorV1) -> NativeDecodeErrorV1<String> {
    match error {
        ManifestRequestCarrierErrorV1::Resource(coverage) => NativeDecodeErrorV1::Resource(coverage),
        other => NativeDecodeErrorV1::Decode(format!("native ManifestRequest transport: {other:?}")),
    }
}

impl NativeManifestRequestCandidateV3 {
    /// The accepting owner supplies the actual Full-validated transaction and
    /// fees already allocated from that same transaction. Unrelated payloads
    /// return None; malformed PMR payloads remain errors, never ordinary data.
    pub fn from_validated_v3(
        validated: &ValidatedTransaction<'_>,
        preceding_fee: u64,
        parent_root: Hash64,
        carrier_block: Hash64,
        at: PalwBlockContextV2,
        accepted_ordinal: usize,
    ) -> Result<Option<Self>, NativeDecodeErrorV1<String>> {
        let Some(handoff) =
            ValidatedManifestRequestCarrierV1::prepare_v1(validated, preceding_fee, carrier_block, at, accepted_ordinal)
                .map_err(transport_error)?
        else {
            return Ok(None);
        };
        if at.block == Hash64::default()
            || at.block == crate::blockhash::ORIGIN
            || carrier_block == crate::blockhash::ORIGIN
            || u32::try_from(accepted_ordinal).is_err()
        {
            return Err(transport_error(ManifestRequestCarrierErrorV1::Context));
        }
        Ok(Some(Self { seed: handoff.into_replay_seed_v1(parent_root).map_err(transport_error)? }))
    }

    pub fn claim_v3(&self) -> Hash64 {
        self.seed.context_v1().scope.claim
    }

    pub fn accepted_ordinal_v3(&self) -> usize {
        self.seed.accepted_ordinal_v1()
    }

    pub fn carrier_v3(&self) -> TransactionId {
        self.seed.carrier_v1()
    }

    /// Check immutable candidate ownership before doing native archive work.
    pub fn check_context_v3(&self, parent_root: Hash64, at: &PalwBlockContextV2) -> Result<(), NativeDecodeErrorV1<String>> {
        self.seed.check_context_v3(parent_root, at).map_err(transport_error)
    }

    /// Resolve authority again from the exact executed prefix. Only immutable
    /// archived admission inputs may be reused across candidate retries; this
    /// result must be rebuilt when any preceding candidate changes. A native
    /// pricing owner must provide the complete request tariff before calling.
    pub fn prepare_at_v3(
        &self,
        original: &crate::palw_positive_contract_v2::RestoredProtectedOriginalV3,
        prefix: &PalwChainStateV2,
        parent_root: Hash64,
        at: &PalwBlockContextV2,
        quote_charge: impl FnOnce(
            crate::palw_court_service_v1::ServiceCost,
            u64,
        ) -> Result<u64, misaka_palw_positive_contract::ContractError>,
    ) -> Result<SharedPreparedNativeManifestRequestV3, NativeDecodeErrorV1<String>> {
        if original.claim_v3() != self.claim_v3() {
            return Err(NativeDecodeErrorV1::Decode("restored original names another request claim".into()));
        }
        let handoff = self.seed.candidate_at_v1(parent_root, at).map_err(transport_error)?;
        let prepared = original.prepare_paid_request_v3(prefix, &handoff, *at, quote_charge).map_err(|error| match error {
            NativeDecodeErrorV1::Resource(coverage) => NativeDecodeErrorV1::Resource(coverage),
            NativeDecodeErrorV1::Decode(error) => {
                NativeDecodeErrorV1::Decode(format!("native ManifestRequest preparation: {error:?}"))
            }
        })?;
        PreparedNativeManifestRequestV3::new_v3(prepared, self.seed.clone(), parent_root, at).map_err(transport_error)
    }

    /// Reconcile a fold's fee projection with this exact UTXO candidate before
    /// the accepting owner deducts it. Equal aggregate fees are insufficient.
    pub fn collectible_fee_v3(&self, effect: &super::replay::ManifestRequestFeeV1) -> Result<u64, NativeDecodeErrorV1<String>> {
        self.seed.collectible_for_seed_v3(effect).map_err(transport_error)
    }

    /// During disposable UTXO reselection only, an old effect may name a
    /// different accepted position or preceding fee. That requires a fresh
    /// projection, not reuse of the effect. Accounting corruption and local
    /// resource failures remain errors. Strict collection uses collectible_fee_v3.
    pub fn matching_collectible_fee_v3(
        &self,
        effect: &super::replay::ManifestRequestFeeV1,
    ) -> Result<Option<u64>, NativeDecodeErrorV1<String>> {
        match self.seed.collectible_for_seed_v3(effect) {
            Ok(fee) => Ok(Some(fee)),
            Err(ManifestRequestCarrierErrorV1::Context) => Ok(None),
            Err(error) => Err(transport_error(error)),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PreparedNativeManifestRequestV3 {
    pub(crate) prepared: PreparedPaidManifestRequestV1,
    pub(crate) seed: SharedManifestRequestReplaySeedV1,
}

pub type SharedPreparedNativeManifestRequestV3 = Arc<NativeRetainedV1<PreparedNativeManifestRequestV3>>;
crate::palw_native_resource_v1::native_struct!(PreparedNativeManifestRequestV3; prepared, seed);

impl PreparedNativeManifestRequestV3 {
    /// Recover only the immutable Full-UTXO input. This carries no reusable
    /// Panel authority: the next complete prefix must call prepare_at_v3 again.
    pub fn candidate_v3(&self) -> NativeManifestRequestCandidateV3 {
        NativeManifestRequestCandidateV3 { seed: self.seed.clone() }
    }

    pub(crate) fn new_v3(
        prepared: PreparedPaidManifestRequestV1,
        seed: SharedManifestRequestReplaySeedV1,
        parent_root: Hash64,
        at: &PalwBlockContextV2,
    ) -> Result<SharedPreparedNativeManifestRequestV3, ManifestRequestCarrierErrorV1> {
        use ManifestRequestCarrierErrorV1::Resource;
        let candidate = seed.candidate_at_v1(parent_root, at)?;
        candidate.request_at_v1(prepared.carrier_v1())?;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<NativeRetainedV1<Self>>(1).map_err(Resource)?;
        q.exact_v1::<usize>(3).map_err(Resource)?; // Arc counters and alignment
        for _ in 0..3 {
            q.vec_v1::<(usize, SharedPreparedNativeManifestRequestV3)>(1).map_err(Resource)?;
        }
        let owner = reserve_native_pending_requests_v1(q.bytes_v1()).map_err(Resource)?;
        Ok(Arc::new(NativeRetainedV1::from_owner_reserved_v1(Self { prepared, seed }, owner)))
    }

    pub fn accepted_ordinal_v3(&self) -> usize {
        self.seed.accepted_ordinal_v1()
    }

    pub fn carrier_v3(&self) -> TransactionId {
        self.seed.carrier_v1()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{point, populated, tx};
    use super::*;

    #[test]
    fn native_request_candidate_preserves_transport_and_utxo_refusals() {
        let transaction = tx();
        let at = point();
        let parent = Hash64::from_u64_word(91);
        let build = |transaction: &crate::tx::Transaction, fee, preceding, parent, block, at, ordinal| {
            NativeManifestRequestCandidateV3::from_validated_v3(&populated(transaction, fee), preceding, parent, block, at, ordinal)
        };
        let candidate = build(&transaction, 20, 8, parent, at.block, at, 2).unwrap().unwrap();
        assert_eq!(candidate.carrier_v3(), transaction.id());
        assert_eq!(candidate.accepted_ordinal_v3(), 2);
        assert_eq!(candidate.claim_v3(), candidate.seed.context_v1().scope.claim);
        candidate.check_context_v3(parent, &at).unwrap();
        assert!(candidate.check_context_v3(Hash64::from_u64_word(92), &at).is_err());
        assert!(candidate.check_context_v3(parent, &PalwBlockContextV2 { subsidy: at.subsidy + 1, ..at }).is_err());
        for (fee, preceding, root, block, point, ordinal) in [
            (19, 8, parent, at.block, at, 2), // populated inputs actually pay 20
            (20, 21, parent, at.block, at, 2),
            (20, 8, Hash64::default(), at.block, at, 2),
            (20, 8, parent, crate::blockhash::ORIGIN, at, 2),
            (20, 8, parent, at.block, PalwBlockContextV2 { block: Hash64::default(), ..at }, 2),
            (20, 8, parent, at.block, PalwBlockContextV2 { block: crate::blockhash::ORIGIN, ..at }, 2),
        ] {
            assert!(build(&transaction, fee, preceding, root, block, point, ordinal).is_err());
        }
        if let Ok(ordinal) = usize::try_from(u64::from(u32::MAX) + 1) {
            assert!(build(&transaction, 20, 8, parent, at.block, at, ordinal).is_err());
        }
        let mut unrelated = transaction.clone();
        unrelated.payload = b"unrelated".to_vec();
        unrelated.finalize();
        assert!(build(&unrelated, 20, 8, parent, at.block, at, 2).unwrap().is_none());
        let mut unknown = transaction.clone();
        unknown.payload[..4].copy_from_slice(b"PMR9");
        unknown.finalize();
        assert!(build(&unknown, 20, 8, parent, at.block, at, 2).is_err());
    }

    #[test]
    fn native_request_fee_collection_binds_full_acceptance_context() {
        let transaction = tx();
        let at = point();
        let parent = Hash64::from_u64_word(91);
        let build = |preceding, root, block, at, ordinal| {
            NativeManifestRequestCandidateV3::from_validated_v3(&populated(&transaction, 20), preceding, root, block, at, ordinal)
                .unwrap()
                .unwrap()
        };
        let candidate = build(8, parent, at.block, at, 2);
        let untouched = candidate.seed.candidate_at_v1(parent, &at).unwrap();
        let effect = candidate.seed.project_after_candidate_v1(&untouched, 0).unwrap();
        assert_eq!(candidate.collectible_fee_v3(&effect).unwrap(), 12);
        assert_eq!(candidate.matching_collectible_fee_v3(&effect).unwrap(), Some(12));
        for other in [
            build(7, parent, at.block, at, 2),
            build(8, Hash64::from_u64_word(92), at.block, at, 2),
            build(8, parent, Hash64::from_u64_word(93), at, 2),
            build(8, parent, at.block, PalwBlockContextV2 { daa_score: at.daa_score + 1, ..at }, 2),
            build(8, parent, at.block, at, 3),
        ] {
            assert!(other.collectible_fee_v3(&effect).is_err());
            assert_eq!(other.matching_collectible_fee_v3(&effect).unwrap(), None);
        }
        // Merely projecting or collecting never spends the seed's ledger.
        let mut other_tx = transaction.clone();
        other_tx.lock_time += 1;
        other_tx.finalize();
        let other = NativeManifestRequestCandidateV3::from_validated_v3(&populated(&other_tx, 20), 8, parent, at.block, at, 2)
            .unwrap()
            .unwrap();
        assert!(other.collectible_fee_v3(&effect).is_err());
        assert_eq!(other.matching_collectible_fee_v3(&effect).unwrap(), None);
        assert_eq!(candidate.seed.candidate_at_v1(parent, &at).unwrap().remaining_fee_v1(), 12);
    }
}
