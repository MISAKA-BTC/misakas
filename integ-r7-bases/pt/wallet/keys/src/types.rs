//!
//!  Key-related type aliases used by the wallet framework.
//!

#[cfg(feature = "legacy-secp256k1")]
use std::sync::Arc;

#[cfg(feature = "legacy-secp256k1")]
pub type ExtendedPublicKeySecp256k1 = kaspa_bip32::ExtendedPublicKey<secp256k1::PublicKey>;

#[cfg(feature = "legacy-secp256k1")]
pub type ExtendedPublicKeys = Arc<Vec<ExtendedPublicKeySecp256k1>>;
