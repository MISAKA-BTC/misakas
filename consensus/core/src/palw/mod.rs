//! Neutral PALW namespace for the existing consensus primitives.

pub mod panel;

pub mod bond {
    pub use crate::mldsa87_primitives::p2pkh_mldsa87_spk;
}

pub mod compute {
    pub use crate::vlt::*;
    pub use crate::vlt::{VLT_BPS as PALW_BPS, VLT_MICRO as PALW_MICRO, VLT_PAYLOAD_VERSION_V1 as PALW_PAYLOAD_VERSION_V1};
}

pub mod types {
    pub use crate::dns_finality::*;
    pub use crate::dns_finality::DnsParams as PalwParams;
}
