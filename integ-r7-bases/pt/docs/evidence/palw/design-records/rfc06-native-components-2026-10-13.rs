use std::{collections::BTreeSet, hint::black_box, time::Instant};
use kaspa_consensus_core::{mldsa87_primitives::MLDSA87_PUBKEY_LEN, palw_state_v2::{palw_bond_state_from_registration_v2, PalwBondKeyV2}, tx::TransactionOutpoint};
use kaspa_hashes::Hash64;
fn main() {
    let started = Instant::now();
    let mut seen = BTreeSet::new();
    let mut last = Vec::new();
    let mut samples = Vec::new();
    for n in 0..64u64 {
        let mut seed = [0xD6; 32]; seed[..8].copy_from_slice(&n.to_le_bytes());
        let begun = Instant::now();
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair(black_box(seed));
        samples.push(begun.elapsed().as_micros());
        last = key.verification_key.as_ref().to_vec();
        assert_eq!(last.len(), MLDSA87_PUBKEY_LEN);
        assert!(seen.insert(last.clone()));
        drop(key);
    }
    samples.sort();
    let payout = kaspa_hashes::blake2b_512_address_payload(&last);
    let state = palw_bond_state_from_registration_v2(&last, &last, 100_000_000, payout, 0, BTreeSet::from([Hash64::from_u64_word(1)]));
    let bond = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(2),0));
    let row = borsh::object_length(&state).unwrap();
    let key_bytes = borsh::object_length(&bond).unwrap();
    use kaspa_consensus_core::palw_permissionless_panel_v1::{SeatCandidateV1,BondIdV1};
    let candidate = borsh::object_length(&SeatCandidateV1 {
        bond: BondIdV1 { transaction: Hash64::from_u64_word(2), index: 0 },
        operator: state.operator_id, key: Hash64::from_u64_word(3), collateral: 100_000_000,
        registered_daa: 0, capability_root: Hash64::from_u64_word(4),
        readiness_root: Hash64::from_u64_word(5), roles: 1,
    }).unwrap();
    let lower = (row + key_bytes) * 100_000 + candidate * 400_000;
    println!("{{\"schemaVersion\":1,\"kind\":\"rfc06-native-components\",\"samples\":64,\"uniquePublicKeys\":{},\"publicKeyBytes\":{},\"keygenMinUs\":{},\"keygenMedianUs\":{},\"keygenMaxUs\":{},\"totalMs\":{},\"activeBondRecordOneCapabilityBytes\":{},\"bondKeyBytes\":{},\"candidateBorshBytes\":{},\"100kBondsAnd400kCandidatesLowerBytes\":{},\"existingTransportCapBytes\":{}}}", seen.len(),last.len(),samples[0],samples[32],samples[63],started.elapsed().as_millis(),row,key_bytes,candidate,lower,384u64<<20);
}
