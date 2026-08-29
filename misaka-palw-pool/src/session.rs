//! **The bond gate: who is allowed to take work from this pool, and how that is decided.**
//!
//! One rule, and it is not a policy the operator writes down — it is a question put to the chain:
//! *does this chain hold a bond at this outpoint, is its registered verification key the one this
//! miner has, and does that bond have room to back another claim?* A miner that cannot answer yes
//! to all three is refused before it is handed a single nonce.
//!
//! # Why the gate is at the door and not at the block
//!
//! Admission would refuse the block anyway — a bond the chain does not know cannot sign an
//! attempt into a header. Checking at the door costs the pool one lookup and saves the miner every
//! inference it would have spent learning the same thing from a rejected block. The gate is a
//! courtesy in the direction of the miner and a defence in the direction of the pool: work handed
//! to an unbonded stranger is a template built and a slot held for a solution that could never be
//! mounted.
//!
//! # Three things the gate is careful about
//!
//! * **The key is the chain's answer, never the miner's.** `Hello` carries a public key; it is
//!   compared against `bond_registered_pubkey`, and the signature is verified against the CHAIN's
//!   copy. A miner that sends someone else's bond with its own key fails the comparison, and one
//!   that sends someone else's bond with THEIR key fails the signature.
//! * **One bond, one session.** Two miners on one bond derive one anchor and grind one space (see
//!   `protocol.rs`), so the second is not extra work — it is a duplicate that would race its twin
//!   for the same block and lose the loser's inference. The second connection is refused, saying so.
//! * **A refusal says which of the three failed**, because "rejected" with no reason is how an
//!   operator spends an evening on a typo in an outpoint.

use crate::protocol::{PALW_POOL_AUTH_MLDSA87_CONTEXT, pool_auth_message_v1};

/// What the chain says about a bond, as the gate needs it. The pool's chain adapter fills this in
/// from `PalwProducerFactsV2`; the gate never reaches for chain state itself, which is what makes
/// it testable without one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BondStandingV1 {
    /// False when the chain has no bond at that outpoint at all.
    pub known: bool,
    /// The ML-DSA-87 verification key the bond registered, as the chain holds it.
    pub registered_pubkey: Vec<u8>,
    /// Empty when this bond may produce now; otherwise the chain's own reason it may not — a
    /// spent epoch budget, a full exposure ceiling, a key mismatch.
    pub not_ready_reason: String,
}

/// Why a miner was turned away. Each variant is a different thing for an operator to fix, which
/// is the whole reason they are not one string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateRefusalV1 {
    /// The pool and the miner do not speak the same protocol.
    ProtocolMismatch { theirs: u32, ours: u32 },
    /// `<txid>:<index>` did not parse, or the payout address did not.
    Malformed(String),
    /// The chain holds no bond there. **This is the "you need a bond" answer**, and it is the one
    /// a miner joining without one gets.
    BondUnknown { bond: String },
    /// The chain knows the bond, but it registered a different key than this miner holds.
    BondKeyMismatch,
    /// The bond is real and the key matches, but the chain says it cannot produce right now.
    BondNotReady { reason: String },
    /// The signature over the auth message did not verify under the bond's registered key.
    BadSignature,
    /// Another live session already holds this bond.
    BondAlreadyConnected { bond: String },
}

impl std::fmt::Display for GateRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProtocolMismatch { theirs, ours } => {
                write!(f, "this pool speaks pool protocol v{ours} and the miner speaks v{theirs}")
            }
            Self::Malformed(what) => write!(f, "{what}"),
            Self::BondUnknown { bond } => write!(
                f,
                "this chain holds no bond at {bond} — a pool miner must register a bond of its own before it can take work \
                 (kaspad --palw-register-bond), because the job an attempt commits to is derived from the bond that signs it"
            ),
            Self::BondKeyMismatch => {
                write!(f, "the key this miner holds is not the one that bond registered — the chain decides which key a bond is")
            }
            Self::BondNotReady { reason } => write!(f, "the chain refuses this bond for now: {reason}"),
            Self::BadSignature => write!(f, "the auth signature did not verify under the key the chain says this bond registered"),
            Self::BondAlreadyConnected { bond } => write!(
                f,
                "another session already holds bond {bond} — one bond is one job, so a second miner on it would grind the \
                 first one's search space rather than add to it"
            ),
        }
    }
}

/// A miner's opening claim, parsed. Nothing here is believed yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MinerIdentityV1 {
    pub bond: kaspa_consensus_core::tx::TransactionOutpoint,
    /// As spelled on the wire — the auth message binds this spelling, so it is kept verbatim.
    pub bond_text: String,
    pub pubkey: Vec<u8>,
    pub pay_address: kaspa_addresses::Address,
    pub pay_address_text: String,
    pub agent: String,
}

/// Parse and shape-check a `Hello`. Does not touch the chain — that is [`admit_v1`]'s half.
pub fn identify_v1(
    protocol: u32,
    bond: &str,
    pubkey_hex: &str,
    pay_address: &str,
    agent: &str,
    prefix: kaspa_addresses::Prefix,
) -> Result<MinerIdentityV1, GateRefusalV1> {
    if protocol != crate::protocol::PALW_POOL_PROTOCOL_VERSION {
        return Err(GateRefusalV1::ProtocolMismatch { theirs: protocol, ours: crate::protocol::PALW_POOL_PROTOCOL_VERSION });
    }
    let (txid, index) = bond.split_once(':').ok_or_else(|| GateRefusalV1::Malformed(format!("'{bond}' is not <txid>:<index>")))?;
    let transaction_id: kaspa_consensus_core::tx::TransactionId =
        txid.parse().map_err(|e| GateRefusalV1::Malformed(format!("'{txid}' is not a transaction id: {e}")))?;
    let index: u32 = index.parse().map_err(|e| GateRefusalV1::Malformed(format!("'{index}' is not an output index: {e}")))?;
    let pubkey = crate::protocol::from_hex(pubkey_hex).map_err(|e| GateRefusalV1::Malformed(format!("pubkey: {e}")))?;
    if pubkey.is_empty() {
        return Err(GateRefusalV1::Malformed("the miner sent an empty verification key".into()));
    }
    let address: kaspa_addresses::Address =
        pay_address.try_into().map_err(|e| GateRefusalV1::Malformed(format!("'{pay_address}' is not an address: {e:?}")))?;
    // **The payout must be payable on THIS network.** A miner that sends a mainnet address to a
    // testnet pool would be handed a template whose coinbase it could never spend, and would find
    // out by never being paid rather than by being told.
    if address.prefix != prefix {
        return Err(GateRefusalV1::Malformed(format!("'{pay_address}' is a {} address and this pool is on {prefix}", address.prefix)));
    }
    Ok(MinerIdentityV1 {
        bond: kaspa_consensus_core::tx::TransactionOutpoint::new(transaction_id, index),
        bond_text: bond.to_string(),
        pubkey,
        pay_address: address,
        pay_address_text: pay_address.to_string(),
        agent: agent.chars().filter(|c| !c.is_control()).take(64).collect(),
    })
}

/// **The gate.** The chain's answer about the bond, plus the miner's signature over the session's
/// own challenge, decide whether this miner may take work.
///
/// `signature` is checked under [`PALW_POOL_AUTH_MLDSA87_CONTEXT`] against the key the CHAIN
/// registered — not the one the miner sent — so a miner that names a bond it does not hold fails
/// here even though it supplied a key that matches its own signature.
pub fn admit_v1(
    identity: &MinerIdentityV1,
    standing: &BondStandingV1,
    session_nonce: &[u8; 32],
    network_id: &str,
    signature: &[u8],
) -> Result<(), GateRefusalV1> {
    if !standing.known {
        return Err(GateRefusalV1::BondUnknown { bond: identity.bond_text.clone() });
    }
    if standing.registered_pubkey != identity.pubkey {
        return Err(GateRefusalV1::BondKeyMismatch);
    }
    if !standing.not_ready_reason.is_empty() {
        return Err(GateRefusalV1::BondNotReady { reason: standing.not_ready_reason.clone() });
    }
    let message = pool_auth_message_v1(session_nonce, network_id, &identity.bond_text, &identity.pubkey, &identity.pay_address_text);
    if !verify_pool_auth_v1(&standing.registered_pubkey, message.as_byte_slice(), signature) {
        return Err(GateRefusalV1::BadSignature);
    }
    Ok(())
}

/// ML-DSA-87 verification under the pool's own context. Total: a malformed key or signature is
/// `false`, never a panic — both arrive from the network.
pub fn verify_pool_auth_v1(pubkey: &[u8], message: &[u8], signature: &[u8]) -> bool {
    let Ok(key_bytes): Result<[u8; 2592], _> = pubkey.try_into() else { return false };
    let Ok(sig_bytes): Result<[u8; 4627], _> = signature.try_into() else { return false };
    let key = libcrux_ml_dsa::ml_dsa_87::MLDSA87VerificationKey::new(key_bytes);
    let sig = libcrux_ml_dsa::ml_dsa_87::MLDSA87Signature::new(sig_bytes);
    libcrux_ml_dsa::ml_dsa_87::verify(&key, message, PALW_POOL_AUTH_MLDSA87_CONTEXT, &sig).is_ok()
}

/// Sign the pool's challenge — the miner's half of [`admit_v1`], here so that the two halves are
/// one implementation and cannot drift into disagreeing about the domain.
pub fn sign_pool_auth_v1(
    signing_key: &libcrux_ml_dsa::ml_dsa_87::MLDSA87SigningKey,
    session_nonce: &[u8; 32],
    network_id: &str,
    bond_text: &str,
    pubkey: &[u8],
    pay_address_text: &str,
) -> Result<Vec<u8>, String> {
    let message = pool_auth_message_v1(session_nonce, network_id, bond_text, pubkey, pay_address_text);
    Ok(libcrux_ml_dsa::ml_dsa_87::sign(signing_key, message.as_byte_slice(), PALW_POOL_AUTH_MLDSA87_CONTEXT, [0x9Cu8; 32])
        .map_err(|e| format!("ML-DSA-87 sign: {e:?}"))?
        .as_ref()
        .to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_addresses::{Prefix, Version};

    fn address() -> String {
        kaspa_addresses::Address::new(Prefix::Testnet, Version::PubKeyHashMlDsa87, &[3u8; 64]).to_string()
    }

    fn identity(kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair) -> MinerIdentityV1 {
        let bond = format!("{}:0", kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0));
        identify_v1(
            crate::protocol::PALW_POOL_PROTOCOL_VERSION,
            &bond,
            &crate::protocol::to_hex(kp.verification_key.as_ref()),
            &address(),
            "miner/test",
            Prefix::Testnet,
        )
        .expect("a well-formed hello")
    }

    fn standing(kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair) -> BondStandingV1 {
        BondStandingV1 { known: true, registered_pubkey: kp.verification_key.as_ref().to_vec(), not_ready_reason: String::new() }
    }

    /// The happy path, and it is the whole loop: the miner signs the pool's challenge with the key
    /// the chain says its bond registered, and is let in.
    #[test]
    fn a_bonded_miner_that_holds_its_key_is_admitted() {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x11u8; 32]);
        let (id, st, nonce) = (identity(&kp), standing(&kp), [0x42u8; 32]);
        let sig =
            sign_pool_auth_v1(&kp.signing_key, &nonce, "testnet-11", &id.bond_text, &id.pubkey, &id.pay_address_text).expect("signs");
        assert_eq!(admit_v1(&id, &st, &nonce, "testnet-11", &sig), Ok(()));
    }

    /// **No bond, no pool.** The requirement, as the one refusal a miner without a bond gets.
    #[test]
    fn a_miner_the_chain_holds_no_bond_for_is_refused() {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x11u8; 32]);
        let id = identity(&kp);
        let nobody = BondStandingV1 { known: false, registered_pubkey: Vec::new(), not_ready_reason: String::new() };
        let sig = sign_pool_auth_v1(&kp.signing_key, &[0x42u8; 32], "testnet-11", &id.bond_text, &id.pubkey, &id.pay_address_text)
            .expect("signs");
        // Even with a perfectly good signature over its own key: the bond is what is missing.
        let refusal = admit_v1(&id, &nobody, &[0x42u8; 32], "testnet-11", &sig).expect_err("no bond, no work");
        assert_eq!(refusal, GateRefusalV1::BondUnknown { bond: id.bond_text.clone() });
        assert!(refusal.to_string().contains("--palw-register-bond"), "the refusal says how to fix it: {refusal}");
    }

    /// Naming somebody else's bond fails on the key, and holding the right key for the wrong bond
    /// fails on the signature. Both directions of the same impersonation.
    #[test]
    fn a_bond_that_is_not_this_miners_is_refused_either_way() {
        let mine = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x11u8; 32]);
        let theirs = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x22u8; 32]);
        let (id, nonce) = (identity(&mine), [0x42u8; 32]);
        let sig = sign_pool_auth_v1(&mine.signing_key, &nonce, "testnet-11", &id.bond_text, &id.pubkey, &id.pay_address_text)
            .expect("signs");

        // The chain says that bond registered somebody else's key.
        assert_eq!(admit_v1(&id, &standing(&theirs), &nonce, "testnet-11", &sig), Err(GateRefusalV1::BondKeyMismatch));

        // And claiming their key as one's own gets as far as the signature, which is where holding
        // the key stops being assertable.
        let impersonation = MinerIdentityV1 { pubkey: theirs.verification_key.as_ref().to_vec(), ..id.clone() };
        assert_eq!(
            admit_v1(&impersonation, &standing(&theirs), &nonce, "testnet-11", &sig),
            Err(GateRefusalV1::BadSignature),
            "the chain's key verifies the chain's key's signatures, and this miner does not have it"
        );
    }

    /// A signature is bound to its session, its network and its payout — replaying one is not a
    /// way in.
    #[test]
    fn a_signature_does_not_travel_between_sessions() {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x11u8; 32]);
        let (id, st) = (identity(&kp), standing(&kp));
        let sig = sign_pool_auth_v1(&kp.signing_key, &[0x42u8; 32], "testnet-11", &id.bond_text, &id.pubkey, &id.pay_address_text)
            .expect("signs");
        assert_eq!(admit_v1(&id, &st, &[0x43u8; 32], "testnet-11", &sig), Err(GateRefusalV1::BadSignature), "another session");
        assert_eq!(admit_v1(&id, &st, &[0x42u8; 32], "testnet-10", &sig), Err(GateRefusalV1::BadSignature), "another network");
        let elsewhere = MinerIdentityV1 {
            pay_address_text: kaspa_addresses::Address::new(Prefix::Testnet, Version::PubKeyHashMlDsa87, &[9u8; 64]).to_string(),
            ..id.clone()
        };
        assert_eq!(admit_v1(&elsewhere, &st, &[0x42u8; 32], "testnet-11", &sig), Err(GateRefusalV1::BadSignature), "another payout");
    }

    /// A bond the chain knows but will not let produce is refused with the chain's own words.
    #[test]
    fn a_bond_with_no_room_is_refused_with_the_chains_reason() {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x11u8; 32]);
        let id = identity(&kp);
        let spent = BondStandingV1 {
            not_ready_reason: "the bond's exposure ceiling leaves no room for another claim".into(),
            ..standing(&kp)
        };
        let refusal = admit_v1(&id, &spent, &[0x42u8; 32], "testnet-11", &[]).expect_err("no room");
        assert!(matches!(refusal, GateRefusalV1::BondNotReady { ref reason } if reason.contains("exposure ceiling")));
    }

    /// Shape checks happen before anything expensive, and each one names itself.
    #[test]
    fn a_malformed_hello_is_named_rather_than_guessed_at() {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x11u8; 32]);
        let key = crate::protocol::to_hex(kp.verification_key.as_ref());
        let bond = format!("{}:0", kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0));
        let v = crate::protocol::PALW_POOL_PROTOCOL_VERSION;

        assert!(matches!(
            identify_v1(v + 1, &bond, &key, &address(), "", Prefix::Testnet),
            Err(GateRefusalV1::ProtocolMismatch { .. })
        ));
        assert!(matches!(identify_v1(v, "not-an-outpoint", &key, &address(), "", Prefix::Testnet), Err(GateRefusalV1::Malformed(_))));
        assert!(matches!(identify_v1(v, &bond, "zz", &address(), "", Prefix::Testnet), Err(GateRefusalV1::Malformed(_))));
        assert!(matches!(identify_v1(v, &bond, "", &address(), "", Prefix::Testnet), Err(GateRefusalV1::Malformed(_))));
        assert!(matches!(identify_v1(v, &bond, &key, "not-an-address", "", Prefix::Testnet), Err(GateRefusalV1::Malformed(_))));
        // A mainnet payout on a testnet pool is a payout the miner could never spend.
        let mainnet = kaspa_addresses::Address::new(Prefix::Mainnet, Version::PubKeyHashMlDsa87, &[3u8; 64]).to_string();
        assert!(matches!(identify_v1(v, &bond, &key, &mainnet, "", Prefix::Testnet), Err(GateRefusalV1::Malformed(_))));
        // A control character in the agent string does not reach the pool's log.
        let id = identify_v1(v, &bond, &key, &address(), "evil\nagent", Prefix::Testnet).expect("agent is sanitized, not refused");
        assert_eq!(id.agent, "evilagent");
    }

    /// Garbage from the network is `false`, not a panic — every one of these arrives from a socket.
    #[test]
    fn verification_is_total_over_what_arrives_from_a_socket() {
        assert!(!verify_pool_auth_v1(&[], b"m", &[]));
        assert!(!verify_pool_auth_v1(&[7u8; 2592], b"m", &[9u8; 4627]));
        assert!(!verify_pool_auth_v1(&[7u8; 10], b"m", &[9u8; 4627]));
    }
}
