//! **The 2026-09-19 audit: an operator identity must be HELD, not declared.**
//!
//! `operator_id` is the unit of panel dedup and of the executor exclusion — sortition drops every
//! bond whose `operator_id` equals the executor's — and it is derived from an `operator_pubkey`
//! that the registration signs under the BOND key. That proves the registrant chose those bytes; it
//! proves nothing about holding them, so a registrant could name a victim's identity and take that
//! victim out of the jury of every claim he produces, free and for ever.
//!
//! Past the fence the registration carries two signatures and the second is over this message under
//! its own context. This is the signed half, with a real ML-DSA-87 key rather than a stub verifier.

use kaspa_consensus_core::palw_state_v2::{
    PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT, PalwBondKeyV2, palw_operator_possession_message_v1,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

fn verify(pk: &[u8], msg: &[u8], sig: &[u8]) -> bool {
    kaspa_txscript::verify_mldsa87_with_context(pk, msg, sig, PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT).unwrap_or(false)
}

#[test]
fn a_possession_proof_binds_the_bond_the_key_and_the_network() {
    let domain = Hash64::from_u64_word(0x11);
    let operator = kaspa_pq_validator_core::ValidatorKey::from_seed([3u8; 32]);
    let stranger = kaspa_pq_validator_core::ValidatorKey::from_seed([4u8; 32]);
    let bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(7), 0));
    let other_bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(8), 0));
    let bond_pubkey = vec![9u8; 4];

    let message = palw_operator_possession_message_v1(domain, &bond, &bond_pubkey, operator.public_key());
    let proof = operator.sign_with_context(message.as_byte_slice(), PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT).to_vec();
    assert!(verify(operator.public_key(), message.as_byte_slice(), &proof), "the holder's own proof verifies");

    // The whole point: a stranger who merely NAMES the identity cannot produce this.
    let forged = stranger.sign_with_context(message.as_byte_slice(), PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT).to_vec();
    assert!(!verify(operator.public_key(), message.as_byte_slice(), &forged), "declaring an identity is not holding it");

    // The proof is bound to its registration: another bond, another bond key, another network, and
    // the same signature is not a proof there.
    for moved in [
        palw_operator_possession_message_v1(domain, &other_bond, &bond_pubkey, operator.public_key()),
        palw_operator_possession_message_v1(domain, &bond, &[1u8; 4], operator.public_key()),
        palw_operator_possession_message_v1(Hash64::from_u64_word(0x12), &bond, &bond_pubkey, operator.public_key()),
    ] {
        assert_ne!(moved, message, "each field is in the digest");
        assert!(!verify(operator.public_key(), moved.as_byte_slice(), &proof), "a proof does not travel");
    }

    // And the context separates it from the registration's own signature, so neither can be lifted
    // into the other's place.
    assert!(
        !kaspa_txscript::verify_mldsa87_with_context(
            operator.public_key(),
            message.as_byte_slice(),
            &proof,
            kaspa_consensus_core::palw_state_v2::PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT,
        )
        .unwrap_or(false),
        "one context, one meaning"
    );
}
