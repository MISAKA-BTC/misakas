//! Private proof and actor-owned chunk preparation; synthetic grammar only.
use kaspa_consensus_core::{
    Hash64,
    palw_flat_model_vector_v7::FlatNativeSealRowV7,
    palw_kernel_route_v1::*,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2 as O},
    tx::TransactionOutpoint,
};
use kaspa_pq_validator_core::ValidatorKey;
use misaka_palw_kernel::{
    flat_court5::*, flat_court7::FlatFaultV7, flat_seal7::*, route::KernelRouteObjectV1 as K, seg::NodeOpeningV1,
};
use misaka_palw_sdk::flat_vector_v7::*;
fn h(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}
fn bond(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(h(n), n as u32))
}
fn secret() -> FlatProofSecretV7 {
    let proof = FlatFaultV7::V5(FlatFaultV5::Malformed(FlatMalformedFaultV5 {
        output: NodeOpeningV1 {
            position: 0,
            occurrence: 0,
            node: 0,
            commitment: [1; 64],
            node_siblings: vec![[2; 64]; 8],
            position_root: [3; 64],
            position_siblings: vec![],
        },
        header: FlatHeaderV5::Flat { dtype: 3, shape: vec![1], root: [4; 64] },
    }))
    .to_bytes()
    .unwrap();
    FlatProofSecretV7::new(h(1), h(2), bond(3), [4; 64], proof).unwrap()
}
fn row(s: &FlatProofSecretV7) -> FlatNativeSealRowV7 {
    FlatNativeSealRowV7 {
        bond: s.bond,
        reserved: 10,
        seal: FlatSealRowV7 {
            version: 7,
            vector: s.vector.as_bytes(),
            actor: palw_kernel_bond_id_v1(&s.bond),
            seal: flat_proof_seal_v7(&s.vector.as_bytes(), &palw_kernel_bond_id_v1(&s.bond), &s.salt, &s.proof).unwrap(),
            posted_daa: 10,
            mature_daa: 13,
            expires_daa: 110,
            spent: false,
        },
    }
}
#[test]
fn private7_codec_network_actor_counts_exact_mixed_proof_and_redacted_debug() {
    let s = secret();
    let b = s.to_bytes().unwrap();
    assert_eq!(FlatProofSecretV7::from_bytes(&b).unwrap(), s);
    assert!(!format!("{s:?}").contains("salt"));
    assert!(misaka_palw_sdk::flat_candidate_v6::FlatProofSecretV6::from_bytes(&b).is_err());
    for n in 0..b.len() {
        assert!(FlatProofSecretV7::from_bytes(&b[..n]).is_err());
    }
    let mut bad = b.clone();
    bad[262..266].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(FlatProofSecretV7::from_bytes(&bad).is_err());
    let mut bad = b;
    bad.push(0);
    assert!(FlatProofSecretV7::from_bytes(&bad).is_err());
    assert!(s.seal_object(h(2), &s.bond).is_err());
    assert!(s.seal_object(h(1), &bond(4)).is_err());
    let raw = FlatFaultV7::from_bytes(&s.proof).unwrap();
    let FlatFaultV7::V5(old) = raw else { panic!() };
    assert!(FlatProofSecretV7::new(h(1), h(2), bond(3), [4; 64], old.to_bytes().unwrap()).is_err());
}
#[test]
fn reveal7_checks_own_mature_unspent_namespace_and_preserves_exact_proof() {
    let s = secret();
    let r = row(&s);
    assert!(matches!(s.reveal_object(h(1),&s.bond,&r,13).unwrap(),K::RevealFlatModelProofV7 {proof,..} if proof==s.proof));
    for now in [0, 12, 110, u64::MAX] {
        assert!(s.reveal_object(h(1), &s.bond, &r, now).is_err());
    }
    for field in 0..5 {
        let mut r = r.clone();
        match field {
            0 => r.bond = bond(4),
            1 => r.seal.version = 6,
            2 => r.seal.actor[0] ^= 1,
            3 => r.seal.seal[0] ^= 1,
            _ => r.seal.spent = true,
        };
        assert!(s.reveal_object(h(1), &s.bond, &r, 13).is_err());
    }
}
#[test]
fn signed7_large_reveal_chunks_bind_same_actor_vector_inner_and_outer_signatures() {
    let key = ValidatorKey::from_seed([7; 32]);
    let s = secret();
    let r = row(&s);
    let object = s.reveal_object(h(1), &s.bond, &r, 13).unwrap();
    let signed = sign_flat_object_v7(&key, h(1), s.bond, &object).unwrap();
    let parts = flat_reveal_carriers_v7(&key, h(1), s.bond, signed.clone(), &r, 13, 1024).unwrap();
    assert!(parts.len() > 1);
    for p in &parts {
        let O::KernelRouteChunkV1 { chunk, signature } = p else { panic!() };
        assert_eq!(chunk.opener, s.bond);
        assert_eq!(chunk.target, PalwKernelChunkTargetV1::FlatProofV7 { vector: s.vector, actor: palw_kernel_bond_id_v1(&s.bond) });
        assert!(
            kaspa_txscript::verify_mldsa87_with_context(
                key.public_key(),
                palw_kernel_chunk_message_v1(h(1), chunk).as_byte_slice(),
                signature,
                PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1
            )
            .unwrap()
        );
    }
    assert!(flat_reveal_carriers_v7(&key, h(2), s.bond, signed.clone(), &r, 13, 1024).is_err());
    assert!(flat_reveal_carriers_v7(&key, h(1), bond(4), signed.clone(), &r, 13, 1024).is_err());
    assert!(flat_reveal_carriers_v7(&key, h(1), s.bond, signed.clone(), &r, 12, 1024).is_err());
    assert!(
        flat_object_carriers_v7(
            sign_flat_object_v7(
                &key,
                h(1),
                s.bond,
                &K::DemandFlatModelUnitV7 {
                    vector: s.vector.as_bytes(),
                    slot: 0,
                    position: 0,
                    occurrence: 0,
                    node: 0,
                    kind: 2,
                    index: 0
                }
            )
            .unwrap()
        )
        .is_ok()
    );
}
