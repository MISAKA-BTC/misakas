//! Explicit continuation of an original signed V6 commit. Its source policy,
//! work and enrollment stay unchanged; current generation and outer signature
//! are separate. An accepted native binding/lock is monitored without another
//! wrapper or funding. A draw erased before the retained native anchor cannot
//! be restored from local WAL metadata by this builder.
use kaspa_consensus_core::{
    Hash64,
    palw_kernel_route_v1::{
        PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, PalwKernelRouteStateV1, palw_kernel_bond_id_v1, palw_kernel_route_message_v1,
    },
    palw_native_resource_v1::{
        NativeRetainedV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_paid_material_generation_v5::{original_paid_bootstrap_commit_message_v7, paid_material_generation_from_route_v5},
    palw_state_v2::PalwConsensusObjectV2,
};
use misaka_palw_kernel::{claim_paid_v2::PaidClaimObjectV2, route::KernelRouteObjectV1};

/// The returned unsigned index19 must enter a new immutable WAL with explicit
/// separate inputs and fee cap. No transport result authenticates approval,
/// settlement or finality. The native consumer rechecks active policy/recovery
/// and both the original index16 signature and the newly signed wrapper.
pub fn paid_claim_material_generation_object_v7(
    route: &PalwKernelRouteStateV1,
    original: &PalwConsensusObjectV2,
    producer_pubkey: &[u8],
) -> Result<NativeRetainedV1<KernelRouteObjectV1>, String> {
    let PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, signature } = original else {
        return Err("V7 original WAL is not a signed native kernel object".into());
    };
    let network = Hash64::from_bytes(route.header.policy.network_domain);
    if !kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
        producer_pubkey,
        palw_kernel_route_message_v1(network, signer, bytes).as_byte_slice(),
        signature,
        PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
    ) {
        return Err("V7 original WAL signature/network/producer differs".into());
    }
    let raw = bytes.get(1..).ok_or("V7 original route version absent")?;
    preflight_native_value_v1::<KernelRouteObjectV1>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
    let decoded = retain_native_value_decode_v1(raw, || KernelRouteObjectV1::decode(bytes)).map_err(|e| e.to_string())?;
    let KernelRouteObjectV1::PaidClaimV2 {
        object: PaidClaimObjectV2::CommitWithApprovedBootstrapV6 { enrollment, commit, salt, source_policy },
    } = &*decoded
    else {
        return Err("V7 requires original index16; legacy V2/index15 or nested wrappers are refused".into());
    };
    if enrollment.producer != palw_kernel_bond_id_v1(signer) || commit.producer() != enrollment.producer || *source_policy == [0; 64] {
        return Err("V7 original WAL changes its actor or source policy".into());
    }
    // Resolve the accepted base directly through the native typed ledger. The
    // absence of a paid authority is not permission to duplicate a base claim.
    if route.ledger_checked_v1().map_err(|e| e.to_string())?.claims.contains_key(&enrollment.base_claim) {
        return Err("V7 base already accepted; monitor its existing native binding/anchor/checker lock".into());
    }
    let expected = original_paid_bootstrap_commit_message_v7(network, signer, enrollment, commit, salt, source_policy)?;
    if expected != palw_kernel_route_message_v1(network, signer, bytes) {
        return Err("V7 original canonical signed index16 bytes differ".into());
    }
    let generation = paid_material_generation_from_route_v5(route, enrollment, *signer).map_err(|e| e.to_string())?;
    let _assembly = reserve_native_requests_v1(
        (std::mem::size_of::<KernelRouteObjectV1>() as u64)
            .checked_add(signature.len() as u64)
            .ok_or("V7 wrapper assembly overflow")?,
    )
    .map_err(|e| e.to_string())?;
    let (decoded, _decoded_owner) = decoded.into_parts_v1();
    let KernelRouteObjectV1::PaidClaimV2 {
        object: PaidClaimObjectV2::CommitWithApprovedBootstrapV6 { enrollment, commit, salt, source_policy },
    } = decoded
    else {
        unreachable!("original index16 shape checked")
    };
    NativeRetainedV1::retain_existing_v1(KernelRouteObjectV1::PaidClaimV2 {
        object: PaidClaimObjectV2::CommitApprovedBootstrapWithMaterialGenerationV7 {
            enrollment,
            commit,
            salt,
            source_policy,
            generation,
            original_signature: signature.clone(),
        },
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod source_controls {
    use super::*;
    use kaspa_consensus_core::{palw_state_v2::PalwBondKeyV2, tx::TransactionOutpoint};
    use misaka_palw_kernel::{
        claim_paid_material_generation_v5::PaidClaimMaterialGenerationV5,
        claim_paid_v2::{PaidClaimEnrollmentV2, PaidClaimLifetimeV2},
        route::SaltedCommitV1,
        spec::{SpecClaimV1, retrieval::RetrievalClaimV1},
    };

    #[test]
    fn continuation_keeps_original_index16_policy_and_draw_identity_separate_from_index19() {
        // Synthetic encoding/identity control only. There is no native signed
        // admission, real work, approval, fee settlement or successful recovery.
        let actor = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([1; 64]), 2));
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
        let network = Hash64::from_bytes([9; 64]);
        let salt = [10; 64];
        let policy = [11; 64];
        let original = KernelRouteObjectV1::PaidClaimV2 {
            object: PaidClaimObjectV2::CommitWithApprovedBootstrapV6 {
                enrollment: e.clone(),
                commit: commit.clone(),
                salt,
                source_policy: policy,
            },
        };
        let original_bytes = original.encode();
        assert_eq!(original_bytes[2], 16);
        let original_message = original_paid_bootstrap_commit_message_v7(network, &actor, &e, &commit, &salt, &policy).unwrap();
        assert_eq!(original_message, palw_kernel_route_message_v1(network, &actor, &original_bytes));
        assert_ne!(
            original_message,
            original_paid_bootstrap_commit_message_v7(network, &actor, &e, &commit, &salt, &[12; 64]).unwrap()
        );
        let generation = PaidClaimMaterialGenerationV5 {
            version: 5,
            enrollment: e.id(),
            root: e.base_claim,
            initial_consent: e.material_consent,
            recovery: [13; 64],
            current_consent: [14; 64],
            sequence: 1,
        };
        generation.validate(&e).unwrap();
        let wrapper = KernelRouteObjectV1::PaidClaimV2 {
            object: PaidClaimObjectV2::CommitApprovedBootstrapWithMaterialGenerationV7 {
                enrollment: e.clone(),
                commit: commit.clone(),
                salt,
                source_policy: policy,
                generation,
                original_signature: vec![15; 8],
            },
        };
        let bytes = wrapper.encode();
        assert_eq!(bytes[2], 19);
        assert_ne!(original_message, palw_kernel_route_message_v1(network, &actor, &bytes));
        let decoded = KernelRouteObjectV1::decode(&bytes).unwrap();
        let KernelRouteObjectV1::PaidClaimV2 {
            object:
                PaidClaimObjectV2::CommitApprovedBootstrapWithMaterialGenerationV7 {
                    enrollment,
                    commit: preserved_commit,
                    salt: preserved_salt,
                    source_policy,
                    generation: preserved_generation,
                    original_signature,
                },
        } = decoded
        else {
            panic!("new wrapper lost its explicit namespace")
        };
        assert_eq!(enrollment, e);
        assert_eq!(preserved_commit, commit);
        assert_eq!(preserved_salt, salt);
        assert_eq!(source_policy, policy);
        assert_eq!(preserved_generation, generation);
        assert_eq!(original_signature, vec![15; 8]);
        let old_wrapper = KernelRouteObjectV1::PaidClaimV2 {
            object: PaidClaimObjectV2::CommitWithMaterialGenerationV5 {
                enrollment: e.clone(),
                commit,
                salt,
                generation,
                original_signature,
            },
        };
        assert_eq!(old_wrapper.encode()[2], 15);
        let mut next = generation;
        next.sequence = 2;
        next.current_consent = [16; 64];
        next.validate(&e).unwrap();
        assert_eq!(next.enrollment, e.id());
    }
}
