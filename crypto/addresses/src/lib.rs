#![cfg_attr(not(feature = "std"), no_std)]

//!
//! Kaspa [`Address`] implementation (kaspa-pq fork).
//!
//! In it's string form, the [`Address`] is represented by a `bech32`-encoded
//! address string combined with a network type. The `bech32` string encoding is
//! comprised of a public key, the public key version and the resulting checksum.
//!
//! kaspa-pq uses the `kaspapq` prefix family (`kaspapq:`, `kaspapqtest:`,
//! `kaspapqsim:`, `kaspapqdev:`); see docs/adr/0001-network-isolation.md.
//!

extern crate alloc;

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use borsh::{BorshDeserialize, BorshSerialize};
use core::{
    cmp,
    fmt::{self, Display, Formatter},
    marker::PhantomData,
    str,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smallvec::SmallVec;
use thiserror::Error;

#[cfg(feature = "wasm32-sdk")]
use wasm_bindgen::prelude::wasm_bindgen;
#[cfg(feature = "wasm32-sdk")]
use workflow_wasm::convert::CastFromJs;

#[cfg(feature = "wasm32-sdk")]
mod wasm;
#[cfg(feature = "wasm32-sdk")]
pub use self::wasm::*;

mod bech32;

/// Error type produced by [`Address`] operations.
#[derive(Error, PartialEq, Eq, Debug, Clone)]
pub enum AddressError {
    #[error("The address has an invalid prefix {0}")]
    InvalidPrefix(String),
    #[error("The address prefix is missing")]
    MissingPrefix,
    #[error("The address has an invalid version {0}")]
    InvalidVersion(u8),
    #[error("The address has an invalid version {0}")]
    InvalidVersionString(String),
    #[error("The address contains an invalid character {0}")]
    DecodingError(char),
    #[error("The address checksum is invalid (must be exactly 8 bytes)")]
    BadChecksumSize,
    #[error("The address checksum is invalid")]
    BadChecksum,
    #[error("The address payload is invalid")]
    BadPayload,
    #[error("The address is invalid")]
    InvalidAddress,
    #[error("The address array is invalid")]
    InvalidAddressArray,
    #[error("{0}")]
    WASM(String),
}

/// Address prefix identifying the network type this address belongs to.
///
/// kaspa-pq uses the `kaspapq` prefix family (`kaspapq`, `kaspapqtest`,
/// `kaspapqsim`, `kaspapqdev`) instead of upstream Kaspa's `kaspa` family.
/// A mainline Kaspa address (`kaspa:...`) will fail `TryFrom<&str>` with
/// `AddressError::InvalidPrefix`, which is the user-visible component of
/// the network-isolation story; see docs/adr/0001-network-isolation.md.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum Prefix {
    #[serde(rename = "kaspapq")]
    Mainnet,
    #[serde(rename = "kaspapqtest")]
    Testnet,
    #[serde(rename = "kaspapqsim")]
    Simnet,
    #[serde(rename = "kaspapqdev")]
    Devnet,
    #[cfg(test)]
    A,
    #[cfg(test)]
    B,
}

impl Prefix {
    fn as_str(&self) -> &'static str {
        match self {
            Prefix::Mainnet => "kaspapq",
            Prefix::Testnet => "kaspapqtest",
            Prefix::Simnet => "kaspapqsim",
            Prefix::Devnet => "kaspapqdev",
            #[cfg(test)]
            Prefix::A => "a",
            #[cfg(test)]
            Prefix::B => "b",
        }
    }

    #[inline(always)]
    fn is_test(&self) -> bool {
        #[cfg(not(test))]
        return false;
        #[cfg(test)]
        matches!(self, Prefix::A | Prefix::B)
    }
}

impl Display for Prefix {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<&str> for Prefix {
    type Error = AddressError;

    fn try_from(prefix: &str) -> Result<Self, Self::Error> {
        match prefix {
            "kaspapq" => Ok(Prefix::Mainnet),
            "kaspapqtest" => Ok(Prefix::Testnet),
            "kaspapqsim" => Ok(Prefix::Simnet),
            "kaspapqdev" => Ok(Prefix::Devnet),
            #[cfg(test)]
            "a" => Ok(Prefix::A),
            #[cfg(test)]
            "b" => Ok(Prefix::B),
            _ => Err(AddressError::InvalidPrefix(prefix.to_string())),
        }
    }
}

///
///  Kaspa `Address` version (`PubKey`, `PubKey ECDSA`, `ScriptHash`)
///
/// @category Address
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
#[cfg_attr(feature = "wasm32-sdk", wasm_bindgen(js_name = "AddressVersion"))]
pub enum Version {
    /// PubKey addresses always have the version byte set to 0.
    /// Carries a 32-byte Schnorr (secp256k1) public key. Non-standard in
    /// kaspa-pq — kept in the enum for borsh-discriminant stability and
    /// for backward compatibility with parsing legacy addresses; the
    /// wallet and mempool do not emit or accept these as standard sends.
    PubKey = 0,
    /// PubKey ECDSA addresses always have the version byte set to 1.
    /// Carries a 33-byte compressed ECDSA (secp256k1) public key.
    /// Non-standard in kaspa-pq for the same reason as PubKey.
    PubKeyECDSA = 1,
    /// kaspa-pq ML-DSA-65 P2PKH (the only standard send template).
    /// Carries a 32-byte `BLAKE2b-256(public_key)`. See
    /// docs/adr/0002-mldsa65-p2pkh.md.
    PubKeyHashMlDsa65 = 2,
    /// ScriptHash addresses always have the version byte set to 8
    ScriptHash = 8,
}

impl TryFrom<&str> for Version {
    type Error = AddressError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "PubKey" => Ok(Version::PubKey),
            "PubKeyECDSA" => Ok(Version::PubKeyECDSA),
            "PubKeyHashMlDsa65" => Ok(Version::PubKeyHashMlDsa65),
            "ScriptHash" => Ok(Version::ScriptHash),
            _ => Err(AddressError::InvalidVersionString(value.to_string())),
        }
    }
}

impl Version {
    /// Address payload length for each [`Version`].
    ///
    /// `PubKeyHashMlDsa65` carries a 32-byte BLAKE2b-256 hash of the
    /// ML-DSA-65 public key. The raw 1952-byte ML-DSA-65 public key is
    /// **never** an address payload — it appears only on the spending
    /// input side. See docs/adr/0002-mldsa65-p2pkh.md.
    pub fn public_key_len(&self) -> usize {
        match self {
            Version::PubKey => 32,
            Version::PubKeyECDSA => 33,
            Version::PubKeyHashMlDsa65 => 32,
            Version::ScriptHash => 32,
        }
    }
}

impl TryFrom<u8> for Version {
    type Error = AddressError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Version::PubKey),
            1 => Ok(Version::PubKeyECDSA),
            2 => Ok(Version::PubKeyHashMlDsa65),
            8 => Ok(Version::ScriptHash),
            _ => Err(AddressError::InvalidVersion(value)),
        }
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Version::PubKey => write!(f, "PubKey"),
            Version::PubKeyECDSA => write!(f, "PubKeyECDSA"),
            Version::PubKeyHashMlDsa65 => write!(f, "PubKeyHashMlDsa65"),
            Version::ScriptHash => write!(f, "ScriptHash"),
        }
    }
}

/// Size of the payload vector of an address.
///
/// This size is the smallest SmallVec supported backing store size greater or equal to the largest
/// possible payload, which is 33 for [`Version::PubKeyECDSA`].
pub const PAYLOAD_VECTOR_SIZE: usize = 36;

/// Used as the underlying type for address payload, optimized for the largest version length (33).
pub type PayloadVec = SmallVec<[u8; PAYLOAD_VECTOR_SIZE]>;

/// [`Address`] struct that serializes to and from an address format string:
/// `kaspapq:qz0s...t8cv` on kaspa-pq mainnet (see docs/adr/0001-network-isolation.md).
///
/// @category Address
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
#[cfg_attr(feature = "wasm32-sdk", derive(CastFromJs), wasm_bindgen(inspectable))]
pub struct Address {
    #[cfg_attr(feature = "wasm32-sdk", wasm_bindgen(skip))]
    pub prefix: Prefix,
    #[cfg_attr(feature = "wasm32-sdk", wasm_bindgen(skip))]
    pub version: Version,
    #[cfg_attr(feature = "wasm32-sdk", wasm_bindgen(skip))]
    pub payload: PayloadVec,
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.version == Version::PubKey {
            write!(f, "{}", String::from(self))
        } else {
            write!(f, "{} ({})", String::from(self), self.version)
        }
    }
}

impl Address {
    pub fn new(prefix: Prefix, version: Version, payload: &[u8]) -> Self {
        if !prefix.is_test() {
            assert_eq!(payload.len(), version.public_key_len());
        }
        Self { prefix, payload: PayloadVec::from_slice(payload), version }
    }

    pub fn short(&self, n: usize) -> String {
        let payload = self.encode_payload();
        let n = cmp::min(n, payload.len() / 4);
        format!("{}:{}....{}", self.prefix, &payload[0..n], &payload[payload.len() - n..])
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", String::from(self))
    }
}

//
// Borsh serializers need to be manually implemented for `Address` since
// smallvec does not currently support Borsh
//

impl BorshSerialize for Address {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        borsh::BorshSerialize::serialize(&self.prefix, writer)?;
        borsh::BorshSerialize::serialize(&self.version, writer)?;
        // Vectors and slices are all serialized internally the same way
        borsh::BorshSerialize::serialize(&self.payload.as_slice(), writer)?;
        Ok(())
    }
}

impl BorshDeserialize for Address {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        let prefix: Prefix = borsh::BorshDeserialize::deserialize_reader(reader)?;
        let version: Version = borsh::BorshDeserialize::deserialize_reader(reader)?;
        let payload: Vec<u8> = borsh::BorshDeserialize::deserialize_reader(reader)?;
        Ok(Self::new(prefix, version, &payload))
    }
}

impl From<Address> for String {
    fn from(address: Address) -> Self {
        (&address).into()
    }
}

impl From<&Address> for String {
    fn from(address: &Address) -> Self {
        format!("{}:{}", address.prefix, address.encode_payload())
    }
}

impl TryFrom<String> for Address {
    type Error = AddressError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<&str> for Address {
    type Error = AddressError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.split_once(':') {
            Some((prefix, payload)) => Self::decode_payload(prefix.try_into()?, payload),
            None => Err(AddressError::MissingPrefix),
        }
    }
}

impl Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Default)]
        pub struct AddressVisitor<'de> {
            marker: PhantomData<Address>,
            lifetime: PhantomData<&'de ()>,
        }

        impl<'de> serde::de::Visitor<'de> for AddressVisitor<'de> {
            type Value = Address;

            fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
                write!(formatter, "string-type: string, str; bytes-type: slice of bytes, vec of bytes; map; number-type - pointer")
            }

            // TODO: see related comment in script_public_key.rs
            #[cfg(all(feature = "wasm32-sdk", target_arch = "wasm32"))]
            fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(v as u32)
            }

            #[cfg(all(feature = "wasm32-sdk", target_arch = "wasm32"))]
            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(v as u32)
            }

            #[cfg(all(feature = "wasm32-sdk", target_arch = "wasm32"))]
            fn visit_f32<E>(self, v: f32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(v as u32)
            }

            #[cfg(all(feature = "wasm32-sdk", target_arch = "wasm32"))]
            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(v as u32)
            }

            #[cfg(all(feature = "wasm32-sdk", target_arch = "wasm32"))]
            fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use wasm_bindgen::convert::RefFromWasmAbi;

                let instance_ref = unsafe { Self::Value::ref_from_abi(v) }; // TODO: add checks for safecast
                Ok(instance_ref.clone())
            }

            #[cfg(all(feature = "wasm32-sdk", target_arch = "wasm32"))]
            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(v as u32)
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Address::try_from(v).map_err(serde::de::Error::custom)
            }

            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Address::try_from(v).map_err(serde::de::Error::custom)
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Address::try_from(v).map_err(serde::de::Error::custom)
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let str = str::from_utf8(v).map_err(serde::de::Error::custom)?;
                Address::try_from(str).map_err(serde::de::Error::custom)
            }

            fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let str = str::from_utf8(v).map_err(serde::de::Error::custom)?;
                Address::try_from(str).map_err(serde::de::Error::custom)
            }

            fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let str = str::from_utf8(&v).map_err(serde::de::Error::custom)?;
                Address::try_from(str).map_err(serde::de::Error::custom)
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut prefix: Option<String> = None;
                let mut payload: Option<String> = None;

                while let Some((key, value)) = access.next_entry::<String, String>()? {
                    match key.as_ref() {
                        "prefix" => {
                            prefix = Some(value);
                        }
                        "payload" => {
                            payload = Some(value);
                        }
                        "version" => continue,
                        unknown_field => {
                            return Err(serde::de::Error::unknown_field(unknown_field, &["prefix", "payload", "version"]));
                        }
                    }
                    if prefix.is_some() && payload.is_some() {
                        break;
                    }
                }
                let (prefix, payload) = match (prefix, payload) {
                    (Some(prefix), Some(payload)) => (prefix, payload),
                    (None, _) => return Err(serde::de::Error::missing_field("prefix")),
                    (_, None) => return Err(serde::de::Error::missing_field("payload")),
                };
                Address::decode_payload(prefix.as_str().try_into().map_err(serde::de::Error::custom)?, &payload)
                    .map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_any(AddressVisitor::default())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use crate::*;
    use alloc::vec;

    // Hardcoded bech32 cases use the synthetic `a:` / `b:` prefixes (Prefix::A,
    // Prefix::B) so that they exercise the bech32 encoder/decoder without
    // depending on the specific kaspa-pq prefix strings (`kaspapq` etc.). The
    // kaspa-pq prefix family is covered by `test_kaspapq_prefix_roundtrip`
    // and `test_kaspapq_prefix_shape` below.
    fn cases() -> Vec<(Address, &'static str)> {
        // cspell:disable
        vec![
            (Address::new(Prefix::A, Version::PubKey, b""), "a:qqeq69uvrh"),
            (Address::new(Prefix::A, Version::ScriptHash, b""), "a:pq99546ray"),
            (Address::new(Prefix::B, Version::ScriptHash, b" "), "b:pqsqzsjd64fv"),
            (Address::new(Prefix::B, Version::ScriptHash, b"-"), "b:pqksmhczf8ud"),
            (Address::new(Prefix::B, Version::ScriptHash, b"0"), "b:pqcq53eqrk0e"),
            (Address::new(Prefix::B, Version::ScriptHash, b"1"), "b:pqcshg75y0vf"),
            (Address::new(Prefix::B, Version::ScriptHash, b"-1"), "b:pqknzl4e9y0zy"),
            (Address::new(Prefix::B, Version::ScriptHash, b"11"), "b:pqcnzt888ytdg"),
            (Address::new(Prefix::B, Version::ScriptHash, b"abc"), "b:ppskycc8txxxn2w"),
            (Address::new(Prefix::B, Version::ScriptHash, b"1234598760"), "b:pqcnyve5x5unsdekxqeusxeyu2"),
            (Address::new(Prefix::B, Version::ScriptHash, b"abcdefghijklmnopqrstuvwxyz"), "b:ppskycmyv4nxw6rfdf4kcmtwdac8zunnw36hvamc09aqtpppz8lk"),
            (Address::new(Prefix::B, Version::ScriptHash, b"000000000000000000000000000000000000000000"), "b:pqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrq7ag684l3"),
        ]
        // cspell:enable
    }

    /// All four kaspa-pq prefixes round-trip cleanly through encode/decode.
    #[test]
    fn test_kaspapq_prefix_roundtrip() {
        for (prefix, payload) in [
            (Prefix::Mainnet, [0u8; 32].as_slice()),
            (Prefix::Testnet, [0u8; 32].as_slice()),
            (Prefix::Simnet, [0u8; 32].as_slice()),
            (Prefix::Devnet, [0u8; 32].as_slice()),
        ] {
            let addr = Address::new(prefix, Version::PubKey, payload);
            let s: String = addr.clone().into();
            let parsed: Address = s.try_into().expect("kaspa-pq address roundtrip should parse");
            assert_eq!(parsed, addr);
        }
    }

    /// Encoded kaspa-pq addresses must use the `kaspapq*` prefix family.
    /// A mainline Kaspa `kaspa:` address must fail with `InvalidPrefix`.
    #[test]
    fn test_kaspapq_prefix_shape() {
        let mainnet: String = Address::new(Prefix::Mainnet, Version::PubKey, &[0u8; 32]).into();
        assert!(mainnet.starts_with("kaspapq:"), "got {mainnet}");
        let testnet: String = Address::new(Prefix::Testnet, Version::PubKey, &[0u8; 32]).into();
        assert!(testnet.starts_with("kaspapqtest:"), "got {testnet}");
        let simnet: String = Address::new(Prefix::Simnet, Version::PubKey, &[0u8; 32]).into();
        assert!(simnet.starts_with("kaspapqsim:"), "got {simnet}");
        let devnet: String = Address::new(Prefix::Devnet, Version::PubKey, &[0u8; 32]).into();
        assert!(devnet.starts_with("kaspapqdev:"), "got {devnet}");

        // A mainline Kaspa address is not parseable as a kaspa-pq address.
        let kaspa_mainline = "kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e".to_string();
        let parsed: Result<Address, AddressError> = kaspa_mainline.try_into();
        assert_eq!(parsed, Err(AddressError::InvalidPrefix("kaspa".into())));
    }

    #[test]
    fn check_into_string() {
        for (address, expected_address_str) in cases() {
            let address_str: String = address.into();
            assert_eq!(address_str, expected_address_str);
        }
    }

    #[test]
    fn check_from_string() {
        for (expected_address, address_str) in cases() {
            let address: Address = address_str.to_string().try_into().expect("Test failed");
            assert_eq!(address, expected_address);
        }
    }

    #[test]
    fn test_errors() {
        // Error paths exercise the bech32 decoder; payloads are reused from
        // upstream Kaspa with the prefix swapped to `kaspapq` for kaspa-pq.
        // The BadChecksum cases rely on the fact that the upstream payloads,
        // when reinterpreted under the `kaspapq` HRP, almost certainly do not
        // happen to have a valid checksum — they're tested as "any wrong
        // checksum" rather than "this specific wrong checksum".
        // cspell:disable
        let address_str: String = "kaspapq:qqqqqqqqqqqqq1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e".to_string();
        let address: Result<Address, AddressError> = address_str.try_into();
        assert_eq!(Err(AddressError::DecodingError('1')), address);

        let invalid_char = 124u8 as char;
        let address_str: String = format!("kaspapq:qqqqqqqqqqqqq{invalid_char}qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e");
        let address: Result<Address, AddressError> = address_str.try_into();
        assert_eq!(Err(AddressError::DecodingError(invalid_char)), address);

        let invalid_char = 129u8 as char;
        let address_str: String = format!("kaspapq:qqqqqqqqqqqqq{invalid_char}qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e");
        let address: Result<Address, AddressError> = address_str.try_into();
        assert!(matches!(address, Err(AddressError::DecodingError(_))));

        let address_str: String = "kaspapq1:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e".to_string();
        let address: Result<Address, AddressError> = address_str.try_into();
        assert_eq!(Err(AddressError::InvalidPrefix("kaspapq1".into())), address);

        // No `:` separator at all => MissingPrefix.
        let address_str: String = "kaspapqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e".to_string();
        let address: Result<Address, AddressError> = address_str.try_into();
        assert_eq!(Err(AddressError::MissingPrefix), address);

        // Well-formed prefix, wrong checksum.
        let address_str: String = "kaspapq:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4l".to_string();
        let address: Result<Address, AddressError> = address_str.try_into();
        assert_eq!(Err(AddressError::BadChecksum), address);

        let address_str: String = "kaspapq:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e".to_string();
        let address: Result<Address, AddressError> = address_str.try_into();
        assert_eq!(Err(AddressError::BadChecksum), address);
        // cspell:enable
    }
}
