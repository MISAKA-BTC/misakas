//! Explicit V5 transport of an original signed V2 enrollment through a native
//! material replacement. This authenticates identity, not computation, fees,
//! selected-verifier approval, finality or permission to forget an older WAL.
use crate::{
    Hash64,
    palw_kernel_route_v1::{
        PALW_KERNEL_ROUTE_OBJECT_DOMAIN_V1, PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, PalwKernelRouteStateV1,
        palw_kernel_bond_id_v1,
    },
    palw_material_release_recovery_v2::{PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, PalwMaterialRecoveryRowV2},
    palw_monetary_material_v1::{PALW_MONETARY_MATERIAL_TABLE_V1, PalwMonetaryMaterialRowV1},
    palw_native_resource_v1::{
        NativeDecodeErrorV1 as E, NativeRetainedV1, NativeWireScanV1, preflight_native_value_v1,
        retain_native_value_decode_classified_v1,
    },
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    claim_paid_material_generation_v5::PaidClaimMaterialGenerationV5 as Generation, claim_paid_v2::PaidClaimEnrollmentV2,
    route::SaltedCommitV1,
};

fn row<T: NativeWireScanV1 + BorshDeserialize>(
    route: &PalwKernelRouteStateV1,
    table: u8,
    root: Hash64,
) -> Result<NativeRetainedV1<T>, E<String>> {
    let bytes = crate::palw_material_challenge_checkpoint_v4::material_challenge_fixed_row_v4(route, table, &root.as_bytes())
        .map_err(E::Resource)?
        .ok_or_else(|| E::Decode("V5 own material generation row absent".into()))?;
    preflight_native_value_v1::<T>(bytes, bytes.len() as u64).map_err(|e| E::Decode(e.to_string()))?;
    retain_native_value_decode_classified_v1(bytes, || borsh::from_slice::<T>(bytes).map_err(|e| e.to_string()))
}

#[cfg(test)]
mod identity_controls {
    use super::*;
    use misaka_palw_kernel::{
        claim_paid_v2::{PaidClaimLifetimeV2, PaidClaimObjectV2},
        route::KernelRouteObjectV1,
        spec::{SpecClaimV1, retrieval::RetrievalClaimV1},
    };
    fn fixture() -> (PalwBondKeyV2, PaidClaimEnrollmentV2, SaltedCommitV1) {
        let actor = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_bytes([1; 64]), 2));
        let commit = SaltedCommitV1::Spec {
            claim: SpecClaimV1::Retrieval(RetrievalClaimV1 {
                job_id: [3; 64],
                producer_bond: palw_kernel_bond_id_v1(&actor),
                result: vec![],
            }),
        };
        let e = PaidClaimEnrollmentV2 {
            version: 2,
            class: [4; 64],
            job: [5; 64],
            base_claim: [6; 64],
            producer: commit.producer(),
            commit_root: [7; 64],
            lifetime: PaidClaimLifetimeV2 {
                committed_daa: 10,
                checking_until: 20,
                challenge_until: 30,
                dispute_until: 40,
                liability_until: 50,
            },
            accepted_until_daa: 12,
            material_consent: [8; 64],
            round: 0,
            prior_transcript: [0; 64],
        };
        (actor, e, commit)
    }
    #[test]
    fn streamed_original_is_exact_legacy_encoding_and_binds_network_actor_salt_and_enrollment() {
        // Identity/codec controls only: the empty retrieval is not actual work
        // or a native signed-admission/approval/settlement acceptance fixture.
        let (actor, e, commit) = fixture();
        let network = Hash64::from_bytes([9; 64]);
        let salt = [10; 64];
        let object = KernelRouteObjectV1::PaidClaimV2 {
            object: PaidClaimObjectV2::Commit { enrollment: e.clone(), commit: commit.clone(), salt },
        };
        let bytes = object.encode();
        assert_eq!(bytes[..3], [1, misaka_palw_kernel::route::TAG_PAID_CLAIM_V2, 2]);
        let original = original_paid_commit_message_v5(network, &actor, &e, &commit, &salt).unwrap();
        assert_eq!(original, crate::palw_kernel_route_v1::palw_kernel_route_message_v1(network, &actor, &bytes));
        assert_ne!(original, original_paid_commit_message_v5(Hash64::from_bytes([11; 64]), &actor, &e, &commit, &salt).unwrap());
        let other = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_bytes([1; 64]), 3));
        assert_ne!(original, original_paid_commit_message_v5(network, &other, &e, &commit, &salt).unwrap());
        assert_ne!(original, original_paid_commit_message_v5(network, &actor, &e, &commit, &[12; 64]).unwrap());
        let mut substituted = e;
        substituted.material_consent = [13; 64];
        assert_ne!(original, original_paid_commit_message_v5(network, &actor, &substituted, &commit, &salt).unwrap());
    }
    #[test]
    fn generation_wrapper_is_new_index_and_never_changes_the_original_draw_identity() {
        let (_, e, commit) = fixture();
        let id = e.id();
        let mut generation = Generation {
            version: 5,
            enrollment: id,
            root: e.base_claim,
            initial_consent: e.material_consent,
            recovery: [14; 64],
            current_consent: [15; 64],
            sequence: 1,
        };
        generation.validate(&e).unwrap();
        let object = KernelRouteObjectV1::PaidClaimV2 {
            object: PaidClaimObjectV2::CommitWithMaterialGenerationV5 {
                enrollment: e.clone(),
                commit,
                salt: [10; 64],
                generation,
                original_signature: vec![16; 8],
            },
        };
        assert_eq!(object.encode()[2], 15);
        generation.current_consent = [17; 64];
        generation.sequence = 2;
        generation.validate(&e).unwrap();
        assert_eq!(e.id(), id);
        assert_eq!(borsh::to_vec(&generation).unwrap().len(), 330);
        generation.initial_consent = [18; 64];
        assert!(generation.validate(&e).is_err(), "arbitrary historic consent cannot become another draw");
    }
}

/// Own-route metadata only. Native admission separately checks every fence and
/// both signatures. A terminal/absent history is never reconstructed from WAL.
pub fn paid_material_generation_from_route_v5(
    route: &PalwKernelRouteStateV1,
    e: &PaidClaimEnrollmentV2,
    producer: PalwBondKeyV2,
) -> Result<Generation, E<String>> {
    let root = Hash64::from_bytes(e.base_claim);
    let monetary = row::<PalwMonetaryMaterialRowV1>(route, PALW_MONETARY_MATERIAL_TABLE_V1, root)?;
    let recovery = row::<PalwMaterialRecoveryRowV2>(route, PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, root)?;
    let PalwMaterialRecoveryRowV2::Active(contract) = &*recovery else {
        return Err(E::Decode("V5 material generation is already terminal".into()));
    };
    let generation = Generation {
        version: 5,
        enrollment: e.id(),
        root: e.base_claim,
        initial_consent: contract.terms.initial_consent.as_bytes(),
        recovery: contract.id().as_bytes(),
        current_consent: monetary.consent.as_bytes(),
        sequence: monetary.sequence,
    };
    generation.validate(e).map_err(E::Decode)?;
    if e.producer != palw_kernel_bond_id_v1(&producer)
        || monetary.terms.root != root
        || contract.terms.root != root
        || monetary.terms.sponsor != producer
        || contract.terms.sponsor != producer
        || monetary.terms.nonce != contract.terms.nonce
        || monetary.terms.native_session.is_some()
        || monetary.terms.material_manifest.as_ref().is_none_or(|manifest| manifest.root != root)
        || contract.generations.is_empty()
        || contract.generations.len() > 64
        || contract.generations.first().is_none_or(|g| g.consent != contract.terms.initial_consent)
        || contract.generations.last().is_none_or(|g| g.consent != monetary.consent)
        || monetary.sequence.checked_add(1) != Some(contract.generations.len() as u64)
    {
        return Err(E::Decode("V5 own generation changes the immutable work/covenant/history".into()));
    }
    Ok(generation)
}

/// The EXACT old route message, streamed without cloning the potentially large
/// commit. Prefix bytes are unchanged V1 route / PaidClaimV2 / Commit index 2.
pub fn original_paid_commit_message_v5(
    network: Hash64,
    producer: &PalwBondKeyV2,
    e: &PaidClaimEnrollmentV2,
    commit: &SaltedCommitV1,
    salt: &[u8; 64],
) -> Result<Hash64, String> {
    struct Writer(blake2b_simd::State);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let body = (e, commit, salt);
    let length =
        (borsh::object_length(&body).map_err(|e| e.to_string())? as u64).checked_add(3).ok_or("original V2 commit length overflow")?;
    if length > crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 as u64 {
        return Err("original V2 commit exceeds native carrier bound".into());
    }
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_KERNEL_ROUTE_OBJECT_DOMAIN_V1).to_state();
    state.update(network.as_byte_slice());
    state.update(producer.0.transaction_id.as_byte_slice());
    state.update(&producer.0.index.to_le_bytes());
    state.update(&length.to_le_bytes());
    state.update(&[misaka_palw_kernel::route::KERNEL_ROUTE_VERSION_V1, misaka_palw_kernel::route::TAG_PAID_CLAIM_V2, 2]);
    let mut writer = Writer(state);
    body.serialize(&mut writer).map_err(|e| e.to_string())?;
    let mut result = [0; 64];
    result.copy_from_slice(writer.0.finalize().as_bytes());
    Ok(Hash64::from_bytes(result))
}

/// Called only by the new V5 fold arm AFTER verifying the outer actor signature.
/// Existing V2 Commit still requires equality to the current material consent.
pub fn validate_paid_material_generation_v5(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: u64,
    producer: &PalwBondKeyV2,
    e: &PaidClaimEnrollmentV2,
    commit: &SaltedCommitV1,
    salt: &[u8; 64],
    generation: &Generation,
    original_signature: &[u8],
) -> Result<(), E<String>> {
    let policy = params.material_release_recovery_at_v2(now).ok_or_else(|| E::Decode("V5 recovery capability is dormant".into()))?;
    params.monetary_material_at_v1(now).ok_or_else(|| E::Decode("V5 monetary capability is dormant".into()))?;
    let route = state.kernel_route().ok_or_else(|| E::Decode("V5 own route absent".into()))?;
    let expected = paid_material_generation_from_route_v5(route, e, *producer)?;
    if *generation != expected || commit.producer() != e.producer {
        return Err(E::Decode("V5 signed generation differs from the exact current own generation".into()));
    }
    let recovery = row::<PalwMaterialRecoveryRowV2>(route, PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, Hash64::from_bytes(e.base_claim))?;
    recovery.validate(policy).map_err(|e| E::Decode(e.to_string()))?;
    let native = state.bond(producer).ok_or_else(|| E::Decode("V5 original signer native key absent".into()))?;
    let message = original_paid_commit_message_v5(Hash64::from_bytes(route.header.policy.network_domain), producer, e, commit, salt)
        .map_err(E::Decode)?;
    if !crate::palw_attempt_material_v3::verify_ready_signature_v3(
        &native.pubkey,
        message.as_byte_slice(),
        original_signature,
        PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
    ) {
        return Err(E::Decode("V5 original V2 enrollment signature differs".into()));
    }
    Ok(())
}
