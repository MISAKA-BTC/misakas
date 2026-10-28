//! **One mnemonic, four keys, and never the same key twice** — ADR-0063 SA-2.
//!
//! Decision 1's BIP39 half is conditional on a derivation this tree shares with the web wallet,
//! and that derivation does not exist yet: `wallet-core-bundle.js` carries a bip39 implementation,
//! this tree carries none, and the two have never agreed on how a phrase becomes an ML-DSA-87
//! seed. **That is still the blocker, and nothing here invents it.** What is written down here is
//! the half that must be true WHATEVER the wallet-side answer turns out to be: once one phrase
//! stands behind several keys, the roles must be domain-separated at the derivation, or they
//! collapse into each other.
//!
//! Why that is not a nicety: a payout key equal to a bond key turns a wallet compromise into a
//! slashable-collateral compromise. The payout key lives in a browser and signs spends all day;
//! the bond key authorises retirement and is the identity a court convicts. They have different
//! blast radii, and a shared derivation with no role in it gives them the same one.
//!
//! **Nothing calls this yet.** It is constants plus a known-answer test, so that when the wallet
//! derivation is specified the role separation is already pinned rather than being designed in the
//! same pass that is busy matching another implementation. The KAT is what makes the labels a
//! commitment: change a label, change the construction, or reorder the preimage, and the vectors
//! below stop matching — which is a test failure rather than a silently different address.
#![allow(dead_code)]

use zeroize::Zeroizing;

/// The number of bytes a role seed carries — the same 32 an ML-DSA-87 seed takes, so a role seed
/// is a drop-in for [`kaspa_pq_validator_core::ValidatorKey::from_seed`] when the master half
/// exists.
pub const ROLE_SEED_LEN: usize = 32;

/// What a key is FOR. The four roles an operator actually holds today, which is the set the
/// separation has to cover; adding a fifth means adding a label and a vector, both of which the
/// KAT will demand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyRole {
    /// Authorises registration, capability declaration and retirement of PALW collateral. The key
    /// a court convicts.
    Bond,
    /// The node's operating identity — `palw_operator_id_v2` is derived from it.
    Operator,
    /// Receives rewards. The one an operator most wants on a phone, and therefore the one whose
    /// compromise must not reach the bond.
    Payout,
    /// The web wallet's spending key.
    Wallet,
}

impl KeyRole {
    /// Every role, so a test cannot check three of four and pass.
    pub const ALL: [KeyRole; 4] = [KeyRole::Bond, KeyRole::Operator, KeyRole::Payout, KeyRole::Wallet];

    /// **The pinned domain-separation string.** These are the constant SA-2 asks for: the role is
    /// IN the derivation, not applied beside it, so two roles cannot produce one key however the
    /// master is obtained. Versioned (`/v1`) because a change here is a change of address for
    /// everyone who ever derived under it — not something to make in place.
    pub const fn derivation_label(self) -> &'static str {
        match self {
            KeyRole::Bond => "misaka-key-role/v1/bond",
            KeyRole::Operator => "misaka-key-role/v1/operator",
            KeyRole::Payout => "misaka-key-role/v1/payout",
            KeyRole::Wallet => "misaka-key-role/v1/wallet",
        }
    }
}

/// Derive this role's 32-byte seed from a master secret.
///
/// BLAKE2b keyed by the MASTER, over a length-prefixed role label. Keyed rather than
/// `hash(label || master)` so the output is unobtainable without the master; length-prefixed so no
/// two labels can ever be one another's prefix-extension, which is the shape a "role separation"
/// most often fails in.
///
/// The master is deliberately typed as bytes rather than as a mnemonic: **how a BIP39 phrase
/// becomes these 32 bytes is exactly the unspecified half**, and writing a guess here would be the
/// silent-wrong-address failure D1 refuses. When the wallet publishes its derivation, it fills in
/// the argument; this function does not move.
pub fn role_seed_v1(master: &[u8], role: KeyRole) -> Zeroizing<[u8; ROLE_SEED_LEN]> {
    let label = role.derivation_label().as_bytes();
    let mut state = blake2b_simd::Params::new().hash_length(ROLE_SEED_LEN).key(master).to_state();
    state.update(&(label.len() as u64).to_le_bytes());
    state.update(label);
    let mut out = Zeroizing::new([0u8; ROLE_SEED_LEN]);
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed master, so the vectors below are reproducible by anyone: bytes 0x00..0x1f.
    fn master() -> [u8; 32] {
        let mut m = [0u8; 32];
        for (i, b) in m.iter_mut().enumerate() {
            *b = i as u8;
        }
        m
    }

    /// **The known-answer test SA-2 requires.** These vectors are this construction's commitment:
    /// change a label, swap the keying for a plain hash, drop the length prefix, or reorder the
    /// preimage, and every one of them fails. That is the point — the derivation is a thing two
    /// implementations must agree on byte for byte, so it needs a fixture rather than a docstring.
    #[test]
    fn role_seeds_are_pinned() {
        let m = master();
        let expected = [
            (KeyRole::Bond, "486b362fefe2aa16ee96e7e83db36c51e0944cca8cd05412f4daffd1cd6f30e7"),
            (KeyRole::Operator, "f6a19a15262688b95bc6a4571378d39e61b0be5b1c501159f857d32d8b50fac8"),
            (KeyRole::Payout, "08c78a8a93499a9ca8c1dc1fc863aa8150cce86cf5d587e0b46715ff7234847d"),
            (KeyRole::Wallet, "52d4d58c0690e71e23bf41ca66ea2943754d26c9b55fc5802a99a4c8d799f64b"),
        ];
        for (role, hex) in expected {
            assert_eq!(faster_hex::hex_string(role_seed_v1(&m, role).as_ref()), hex, "{role:?} moved — the derivation is a pin");
        }
    }

    /// **One mnemonic must never yield the same key in two roles.** The KAT above would still pass
    /// if two labels were copy-pasted equal and both vectors updated together, so the property is
    /// asserted separately from the vectors: all four differ, pairwise.
    #[test]
    fn one_master_yields_four_different_keys() {
        let m = master();
        let seeds: Vec<[u8; ROLE_SEED_LEN]> = KeyRole::ALL.iter().map(|r| *role_seed_v1(&m, *r)).collect();
        for i in 0..seeds.len() {
            for j in (i + 1)..seeds.len() {
                assert_ne!(
                    seeds[i],
                    seeds[j],
                    "{:?} and {:?} derive to the same key — a payout compromise would be a bond compromise",
                    KeyRole::ALL[i],
                    KeyRole::ALL[j]
                );
            }
        }
        // …and the labels themselves are distinct, which is the reason they differ.
        let mut labels: Vec<&str> = KeyRole::ALL.iter().map(|r| r.derivation_label()).collect();
        labels.sort_unstable();
        let count = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), count, "two roles share a derivation label");
    }

    /// A different master gives different keys in every role — the separation is per-role, not a
    /// constant added to a master-independent value.
    #[test]
    fn a_different_master_moves_every_role() {
        let a = master();
        let mut b = master();
        b[31] ^= 1;
        for role in KeyRole::ALL {
            assert_ne!(*role_seed_v1(&a, role), *role_seed_v1(&b, role), "{role:?}");
        }
    }
}
