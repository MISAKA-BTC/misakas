//! Off-chain, whole-graph commitments from an actual mapped checkpoint.
//! A fixture grants no chain eligibility, source equivalence or Final. Consumers
//! reconstruct candidate/job/network-domain evidence and authenticate its inputs.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::Hash64;
use misaka_palw_kernel::{hash::Digest, trace::ParamCommitmentsV1};

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct RealCheckpointExecutionFixtureV11 {
    pub version: u16,
    pub artifact_sha256: [u8; 32],
    pub protocol_sha256: [u8; 32],
    pub program_bytes: Vec<u8>,
    pub params: ParamCommitmentsV1,
    pub model_inventory_root: Hash64,
    pub prompt: Vec<u32>,
    pub generated: Vec<u32>,
    /// Every executed position, occurrence and node, without parameter payloads
    /// or private tensors. Generation has the native fixed-count greedy rule.
    pub commitments: Vec<Vec<Vec<Digest>>>,
}

impl RealCheckpointExecutionFixtureV11 {
    /// Verify the independently pinned complete fixture before any dynamic
    /// Borsh directory/parameter allocation. This provides file integrity, not
    /// chain inclusion, source equivalence, class eligibility or Final.
    pub fn from_pinned_bytes(bytes: &[u8], expected_sha256: [u8; 32]) -> Result<Self, String> {
        use sha2::{Digest, Sha256};
        if bytes.len() > 64 << 20 {
            return Err("real execution fixture exceeds original bounded file cap".into());
        }
        let observed: [u8; 32] = Sha256::digest(bytes).into();
        if observed != expected_sha256 {
            return Err("real execution fixture differs from independently pinned complete SHA256".into());
        }
        let f: Self = borsh::from_slice(bytes).map_err(|_| "pinned real execution fixture framing".to_string())?;
        if f.version != 11 {
            return Err("pinned real execution fixture version differs".into());
        }
        Ok(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn same_length_mutation_is_rejected_by_complete_hash_before_decode() {
        let f = RealCheckpointExecutionFixtureV11 {
            version: 11,
            artifact_sha256: [1; 32],
            protocol_sha256: [2; 32],
            program_bytes: vec![3, 4, 5],
            params: ParamCommitmentsV1::default(),
            model_inventory_root: Hash64::from_bytes([6; 64]),
            prompt: vec![7],
            generated: vec![8],
            commitments: vec![vec![vec![[9; 64]]]],
        };
        let raw = borsh::to_vec(&f).unwrap();
        let pin = Sha256::digest(&raw).into();
        assert_eq!(RealCheckpointExecutionFixtureV11::from_pinned_bytes(&raw, pin).unwrap().commitments, f.commitments);
        let mut changed = raw.clone();
        *changed.last_mut().unwrap() ^= 1;
        assert!(
            borsh::from_slice::<RealCheckpointExecutionFixtureV11>(&changed).is_ok(),
            "the mutation is a valid different statement"
        );
        assert_eq!(
            RealCheckpointExecutionFixtureV11::from_pinned_bytes(&changed, pin).unwrap_err(),
            "real execution fixture differs from independently pinned complete SHA256"
        );
        let mut malformed = raw;
        malformed[0] = 255;
        assert_eq!(
            RealCheckpointExecutionFixtureV11::from_pinned_bytes(&malformed, pin).unwrap_err(),
            "real execution fixture differs from independently pinned complete SHA256"
        );
    }
}
