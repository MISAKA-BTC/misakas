//! M*-49 watcher opt-in. Pool membership authenticates willingness and locked stake;
//! it neither verifies a model nor grants computation credit.
use crate::hash::{Digest, object_id};
use borsh::{BorshDeserialize, BorshSerialize};

/// A finite, explicitly signed pool reservation. The native route authenticates the
/// watcher and reserves the stake through `lock_until_daa`; no early exit exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct VerifierWatchTermsV1 {
    pub version: u16,
    pub network: Digest,
    pub ruleset: Digest,
    pub class: Digest,
    pub watcher: Digest,
    pub stake: u64,
    pub first_epoch: u64,
    pub last_epoch: u64,
    pub lock_until_daa: u64,
    pub policy: Digest,
    pub nonce: u64,
}
impl VerifierWatchTermsV1 {
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/verifier-watch-terms/v1", self)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1 || self.stake == 0 || self.first_epoch > self.last_epoch || self.lock_until_daa == u64::MAX {
            return Err("invalid finite watcher reservation");
        }
        Ok(())
    }
}
