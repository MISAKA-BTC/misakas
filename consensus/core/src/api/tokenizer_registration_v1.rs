//! Read-only RFC-0051 snapshot envelope. The prospective clock is not an
//! accepted block. In particular, no ORIGIN scratch block escapes this API.
use crate::{Hash64, palw_state_v2::PalwBlockContextV2};

pub struct TokenizerRegistrationReadSnapshotV1<T> {
    pub parent_point: PalwBlockContextV2,
    pub parent_root: Hash64,
    pub quote_daa: u64,
    pub quote_blue_score: u64,
    pub value: T,
}
