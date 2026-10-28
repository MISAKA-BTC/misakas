use kaspa_consensus_core::subnets::SubnetworkConversionError;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum ConversionError {
    #[error("General p2p conversion error")]
    General,

    #[error("Optional field is None while expected to be Some")]
    NoneValue,

    #[error("IP has illegal length {0}")]
    IllegalIPLength(usize),

    /// **A handshake fingerprint field is a fixed-width hash, and an unauthenticated peer supplies
    /// it** (audit3 H11). The transport accepts messages up to 1 GB, the proto declares these as
    /// plain `bytes` with no cap, and nothing between the wire and the comparison bounded them —
    /// so a single connection could hand the node a gigabyte to hex-render into a log line before
    /// it was registered anywhere. Refused at the boundary, where the field's real width is known.
    #[error("handshake fingerprint field `{0}` is {1} bytes, over the {2}-byte maximum")]
    OversizedFingerprint(&'static str, usize, usize),

    #[error("Bytes size mismatch error {0}")]
    ArrayBytesSizeError(#[from] std::array::TryFromSliceError),

    #[error("Bytes size mismatch error {0}")]
    UintBytesSizeError(#[from] kaspa_math::uint::TryFromSliceError),

    #[error("Integer parsing error: {0}")]
    IntCastingError(#[from] std::num::TryFromIntError),

    #[error(transparent)]
    AddressParsingError(#[from] std::net::AddrParseError),

    #[error(transparent)]
    IdentityError(#[from] uuid::Error),

    #[error(transparent)]
    SubnetParsingError(#[from] SubnetworkConversionError),

    #[error(transparent)]
    CompressedParentsError(#[from] kaspa_consensus_core::errors::header::CompressedParentsError),
}
