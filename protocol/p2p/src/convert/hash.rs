use super::error::ConversionError;
use crate::pb as protowire;
use kaspa_hashes::{Hash, Hash64};

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<Hash> for protowire::Hash {
    fn from(hash: Hash) -> Self {
        Self { bytes: Vec::from(hash.as_bytes()) }
    }
}

impl From<&Hash> for protowire::Hash {
    fn from(hash: &Hash) -> Self {
        Self { bytes: Vec::from(hash.as_bytes()) }
    }
}

// PR-9.5c: `MerkleRoot` / `AcceptedIdMerkleRoot` widened to
// `Hash64`. The proto field on the wire is still `bytes` (a
// dynamic-length protobuf bytes field), so the only change is the
// byte count: 64 instead of 32. The same conversion shape applies
// for both width.
impl From<Hash64> for protowire::Hash {
    fn from(hash: Hash64) -> Self {
        Self { bytes: Vec::from(hash.as_bytes()) }
    }
}

impl From<&Hash64> for protowire::Hash {
    fn from(hash: &Hash64) -> Self {
        Self { bytes: Vec::from(hash.as_bytes()) }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::Hash> for Hash {
    type Error = ConversionError;

    fn try_from(hash: protowire::Hash) -> Result<Self, Self::Error> {
        Ok(Self::from_bytes(hash.bytes.as_slice().try_into()?))
    }
}

impl TryFrom<protowire::Hash> for Hash64 {
    type Error = ConversionError;

    fn try_from(hash: protowire::Hash) -> Result<Self, Self::Error> {
        Ok(Self::from_bytes(hash.bytes.as_slice().try_into()?))
    }
}
