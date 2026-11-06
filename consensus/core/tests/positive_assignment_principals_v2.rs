//! Registration-derived DATA fixtures, not native admission or work eligibility.
use kaspa_consensus_core::{
    Hash64,
    palw_positive_metadata_v2::{PositiveMetadataErrorV2, positive_registered_principal_v2},
    palw_state_v2::{
        PalwBondKeyV2, PalwChainStateV2, PalwStateCarriageV2, PalwStateParamsV2, palw_bond_state_from_registration_v2,
        palw_operator_id_v2,
    },
    tx::TransactionOutpoint,
};
use misaka_palw_positive_contract::identity::{PUBKEY_BYTES, principal};
#[test]
fn distinct_registered_operator_and_work_keys_keep_their_own_identity_domains() {
    let work = libcrux_ml_dsa::ml_dsa_87::portable::generate_key_pair([1; 32]);
    let operator = libcrux_ml_dsa::ml_dsa_87::portable::generate_key_pair([2; 32]);
    let key: &[u8; PUBKEY_BYTES] = work.verification_key.as_ref();
    let operator_key: &[u8; PUBKEY_BYTES] = operator.verification_key.as_ref();
    let bond = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(3), 0));
    let registered = palw_bond_state_from_registration_v2(key, operator_key, 100, Hash64::from_u64_word(4), 0, Default::default());
    assert_ne!(registered.operator_id, palw_operator_id_v2(key));
    assert_eq!(registered.operator_id, palw_operator_id_v2(operator_key));
    let params = PalwStateParamsV2::new(500, 1, 1, 1, 1, 1, Hash64::from_u64_word(5), 2, 1000, 1, 1, 1).unwrap();
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    carriage.bonds.insert(bond, registered);
    let state = carriage.into_state(&params, None).unwrap();
    assert_eq!(positive_registered_principal_v2(&state, &bond), Ok(principal(key)));
    let missing = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(6), 0));
    assert_eq!(positive_registered_principal_v2(&state, &missing), Err(PositiveMetadataErrorV2::MissingBond));
}
