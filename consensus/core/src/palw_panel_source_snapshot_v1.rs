//! Frozen source facts for an opt-in Panel epoch. Written only by the dormant
//! Panel/source fold, before that block's objects, into rooted kernel aux rows.
//! No source fee, policy approval or security guarantee follows from this row.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use misaka_palw_challenge::hash::{Digest, object_id};
use misaka_palw_challenge::{BeaconContextV1, PostCommitChallengePolicyV1, RootV1, SubjectKindV1};
use misaka_palw_panel::BeaconRequestV1;
use std::collections::BTreeSet;

/// Kernel route aux table 111 (codex used 49 — RFC-02's conformance v2 table on the integration line; renumbered at the R6781-F
/// merge).
pub const PALW_PANEL_SOURCE_SNAPSHOT_TABLE_V1: u8 = 111;
pub const MAX_PANEL_SOURCE_SNAPSHOT_ITEMS_V1: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPanelSourceSnapshotV1 {
    pub version: u16,
    pub network: Hash64,
    pub ruleset: Hash64,
    pub scheme: Hash64,
    pub epoch: u64,
    pub release_daa: u64,
    pub deadline_daa: u64,
    /// First actual selected-chain step reaching release; never a fabricated
    /// historical position when sparse DAA skips the scheduled boundary.
    pub committed_daa: u64,
    pub committed_block: Hash64,
    pub checkpoint: Hash64,
    pub checkpoint_daa: u64,
    pub eligible_profiles: BTreeSet<Digest>,
    pub excluded_profiles: BTreeSet<Digest>,
    /// Every sealed work in this epoch, including claims which later end. Such
    /// a claim cannot become its own entropy merely by leaving the pending set.
    pub assigned_work_ids: BTreeSet<Digest>,
}

impl PalwPanelSourceSnapshotV1 {
    pub fn validate_for(&self, request: &BeaconRequestV1, policy: &PostCommitChallengePolicyV1) -> Result<(), String> {
        policy.validate().map_err(|e| e.to_string())?;
        if self.version != 1
            || !policy.is_opt_in_source()
            || (self.network, self.ruleset, self.scheme, self.epoch, self.release_daa, self.deadline_daa)
                != (request.network, request.ruleset, request.scheme, request.epoch, request.release_daa, request.deadline_daa)
            || self.scheme.as_bytes() != policy.id()
            || self.committed_daa < self.release_daa
            || self.committed_daa > self.deadline_daa
            || self.checkpoint_daa > self.committed_daa
            || self.committed_block == Hash64::default()
            || self.checkpoint == Hash64::default()
            || self.assigned_work_ids.is_empty()
            || self.assigned_work_ids.contains(&[0; 64])
            || [self.eligible_profiles.len(), self.excluded_profiles.len(), self.assigned_work_ids.len()]
                .into_iter()
                .any(|n| n > MAX_PANEL_SOURCE_SNAPSHOT_ITEMS_V1)
            || !self.eligible_profiles.is_disjoint(&self.excluded_profiles)
        {
            return Err("Panel source snapshot does not bind its epoch, actual clock or complete bounded subject".into());
        }
        Ok(())
    }

    pub fn context(&self, request: &BeaconRequestV1, policy: &PostCommitChallengePolicyV1) -> Result<BeaconContextV1, String> {
        self.validate_for(request, policy)?;
        Ok(BeaconContextV1 {
            chain_genesis: self.network.as_bytes(),
            ruleset_id: self.ruleset.as_bytes(),
            policy: policy.clone(),
            subject_kind: SubjectKindV1::PanelAssignment,
            commitment_root: object_id(b"MISAKA/PALW/PANEL-SOURCE-SNAPSHOT/V1", self),
            commitment_position: self.committed_daa,
            challenge_epoch: self.epoch,
            eligible_profiles: self.eligible_profiles.clone(),
            excluded_profiles: self.excluded_profiles.clone(),
            candidate_profile_id: RootV1::Absent,
        })
    }
}

impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    pub fn panel_source_snapshot_v1(&self, epoch: u64) -> Result<PalwPanelSourceSnapshotV1, String> {
        self.aux_row(PALW_PANEL_SOURCE_SNAPSHOT_TABLE_V1, &borsh::to_vec(&epoch).expect("epoch serializes"))
            .ok_or_else(|| "the authenticated Panel epoch source snapshot is missing or malformed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panel_source_snapshot_binds_actual_sparse_clock_and_every_frozen_subject_fact() {
        let p = misaka_palw_challenge::paid_opt_in_source_policy_v5(2, 2, 20, 2, 1);
        let req = BeaconRequestV1 {
            network: Hash64::from_u64_word(1),
            ruleset: Hash64::from_u64_word(2),
            scheme: Hash64::from_bytes(p.id()),
            epoch: 11,
            release_daa: 100,
            deadline_daa: 300,
        };
        let snap = PalwPanelSourceSnapshotV1 {
            version: 1,
            network: req.network,
            ruleset: req.ruleset,
            scheme: req.scheme,
            epoch: req.epoch,
            release_daa: 100,
            deadline_daa: 300,
            committed_daa: 110,
            committed_block: Hash64::from_u64_word(3),
            checkpoint: Hash64::from_u64_word(4),
            checkpoint_daa: 90,
            eligible_profiles: BTreeSet::from([[5; 64]]),
            excluded_profiles: BTreeSet::from([[6; 64]]),
            assigned_work_ids: BTreeSet::from([[7; 64]]),
        };
        let ctx = snap.context(&req, &p).unwrap();
        assert_eq!(ctx.commitment_position, 110, "a skipped release cannot backdate source eligibility");
        for i in 0..6 {
            let mut changed = snap.clone();
            match i {
                0 => {
                    changed.eligible_profiles.insert([8; 64]);
                }
                1 => {
                    changed.assigned_work_ids.insert([9; 64]);
                }
                2 => {
                    changed.excluded_profiles.insert([10; 64]);
                }
                3 => changed.committed_block = Hash64::from_u64_word(11),
                4 => changed.checkpoint = Hash64::from_u64_word(12),
                _ => changed.committed_daa += 1,
            }
            assert_ne!(changed.context(&req, &p).unwrap().id(), ctx.id());
        }
        for i in 0..5 {
            let mut changed = snap.clone();
            match i {
                0 => changed.committed_daa = 99,
                1 => changed.committed_daa = 301,
                2 => changed.assigned_work_ids.clear(),
                3 => {
                    changed.eligible_profiles.insert([6; 64]);
                }
                _ => changed.epoch += 1,
            }
            assert!(changed.context(&req, &p).is_err());
        }
        assert_eq!(borsh::from_slice::<PalwPanelSourceSnapshotV1>(&borsh::to_vec(&snap).unwrap()).unwrap(), snap);
    }
}
