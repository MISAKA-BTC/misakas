// //!
// //! This file contains most common imports that
// //! are used internally in this crate.
// //!

// Curve-independent prelude used by the always-on modules (`derivation_path`,
// `secret`) and shared with the classical key modules.
pub use crate::error::Error;
pub use crate::result::Result;
pub use borsh::{BorshDeserialize, BorshSerialize};
pub use kaspa_bip32::ChildNumber;
pub use serde::{Deserialize, Serialize};
pub use std::str::FromStr;
pub use wasm_bindgen::prelude::*;
pub use zeroize::*;

// kaspa-pq PQ-only (ADR-0019 §14): the remainder of the wallet-key prelude is
// only reachable from the classical secp256k1 key modules (publickey /
// privatekey / keypair / xprv / xpub / derivation), which are gated behind
// `legacy-secp256k1`. Keeping these re-exports gated keeps the PQ-only build
// secp-free and warning-free.
#[cfg(feature = "legacy-secp256k1")]
pub use crate::derivation_path::DerivationPath;
#[cfg(feature = "legacy-secp256k1")]
pub use crate::privatekey::PrivateKey;
#[cfg(feature = "legacy-secp256k1")]
pub use crate::publickey::{PublicKey, PublicKeyArrayT};
#[cfg(feature = "legacy-secp256k1")]
pub use crate::xprv::{XPrv, XPrvT};
#[cfg(feature = "legacy-secp256k1")]
pub use crate::xpub::{XPub, XPubT};
#[cfg(feature = "legacy-secp256k1")]
pub use async_trait::async_trait;
#[cfg(feature = "legacy-secp256k1")]
pub use js_sys::Array;
#[cfg(feature = "legacy-secp256k1")]
pub use kaspa_addresses::{Address, Version as AddressVersion};
#[cfg(feature = "legacy-secp256k1")]
pub use kaspa_bip32::{ExtendedPrivateKey, ExtendedPublicKey, SecretKey};
#[cfg(feature = "legacy-secp256k1")]
pub use kaspa_consensus_core::network::{NetworkId, NetworkTypeT};
#[cfg(feature = "legacy-secp256k1")]
pub use kaspa_utils::hex::*;
#[cfg(feature = "legacy-secp256k1")]
pub use kaspa_wasm_core::types::*;
#[cfg(feature = "legacy-secp256k1")]
pub use std::collections::HashMap;
#[cfg(feature = "legacy-secp256k1")]
pub use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "legacy-secp256k1")]
pub use std::sync::{Arc, Mutex, MutexGuard};
#[cfg(feature = "legacy-secp256k1")]
pub use workflow_wasm::convert::*;
