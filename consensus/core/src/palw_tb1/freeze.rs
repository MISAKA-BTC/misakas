//! Private EXEC/PANEL round and closure commitments. Each round commits to its
//! schedule before dealer preparation. Consumer roots come from the owning
//! registration or Original fold. No public network recognizes this codec.
use sha3::{Digest, Sha3_512};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tb1MiningPurpose {
    Exec,
    Panel,
}

#[derive(Clone, Copy)]
pub struct Tb1MiningRound {
    pub chain_id: [u8; 64],
    pub roster_root: [u8; 64],
    pub role_epoch: u64,
    pub logical_round: u64,
    pub purpose: Tb1MiningPurpose,
    pub schedule_id: [u8; 64],
    pub close_blue_score: u64,
    pub release_from_daa: u64,
    pub deadline_exclusive_daa: u64,
}

impl Tb1MiningRound {
    pub fn id(&self) -> [u8; 64] {
        let purpose = match self.purpose {
            Tb1MiningPurpose::Exec => b"EXEC".as_slice(),
            Tb1MiningPurpose::Panel => b"PANEL".as_slice(),
        };
        Sha3_512::new()
            .chain_update(b"MISAKA/private/TB1/BGW-PQ/round/v1")
            .chain_update(self.chain_id)
            .chain_update(self.roster_root)
            .chain_update(self.role_epoch.to_be_bytes())
            .chain_update([purpose.len() as u8])
            .chain_update(purpose)
            .chain_update(self.logical_round.to_be_bytes())
            .chain_update(self.schedule_id)
            .chain_update(self.close_blue_score.to_be_bytes())
            .chain_update(self.release_from_daa.to_be_bytes())
            .chain_update(self.deadline_exclusive_daa.to_be_bytes())
            .finalize()
            .into()
    }
}

#[derive(Clone, Copy)]
pub struct Tb1MiningClosure {
    pub round: Tb1MiningRound,
    pub qual_root: [u8; 64],
    pub consumer_set_root: [u8; 64],
    pub policy_root: [u8; 64],
}

impl Tb1MiningClosure {
    /// Published in the isolated close block's miner data. Use its already
    /// known selected parent so the block never commits to its own hash.
    pub fn commitment(&self, parent: crate::Hash64) -> [u8; 64] {
        Sha3_512::new()
            .chain_update(b"MISAKA/private/TB1/mining-freeze/v1")
            .chain_update(self.round.id())
            .chain_update(self.qual_root)
            .chain_update(parent.as_byte_slice())
            .chain_update(self.consumer_set_root)
            .chain_update(self.policy_root)
            .finalize()
            .into()
    }
}
