//! RFC-0023 stage 2: claim dependencies and a material-bearing liability checkpoint.
//!
//! This module supplies the deterministic checkpoint checks. Runtime admission must derive the
//! records from committed inputs and reservations; a producer's list is never the authority.
//! Integration status: dependency/checkpoint foundations and the dormant fence only. Runtime
//! admission, reward/credit folding, native lifecycle, pruning and IBD are not connected yet.
//! Hashes below authenticate bytes only. Certification remains the selected validators' approval
//! under Gate C's stated honest-fraction premise; no computation-validity proof is introduced.
use crate::palw_state_v2::PalwBlockContextV2;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash64, ZERO_HASH64};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClaimLocalRecordV1 {
    pub claim: Hash64,
    pub accepted_daa: u64,
    pub accepted_blue_score: u64,
    /// The shared-DAG block that accepted this claim, including when its carrying block was RED.
    pub accepting_block: Hash64,
    /// Only an attempt produces a frontier anchor; FP receipt work alone does not.
    pub anchors: bool,
    /// Actual consumed outputs; populated by admission, never inferred from model identity.
    pub inputs: BTreeSet<Hash64>,
    pub execution_root: Hash64,
    /// Reservation identities, not bond identities: two reservations on one bond remain separate.
    pub reservations: BTreeSet<Hash64>,
    pub final_claim: bool,
    pub certified: bool,
    pub voided: bool,
    /// True only after all applicable prosecution, DA and court obligations have closed.
    pub liability_closed: bool,
    /// Every byte object needed to pursue this liability, including its pinned policy,
    /// registry/randomness evidence and open court state. The admission-derived manifest is exact.
    pub material: BTreeSet<Hash64>,
}

pub fn palw_liability_material_id_v1(bytes: &[u8]) -> Hash64 {
    let hash = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/liability-material/v1").hash(bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(hash.as_bytes());
    Hash64::from_bytes(out)
}

/// A checkpoint includes the actual material. A root or a URL cannot stand in for missing bytes.
/// The exact liability state contains collateral reservations, detection policies, registration /
/// randomness evidence and court state as canonically derived by the fold at this point.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClaimLocalCheckpointV1 {
    pub point: PalwBlockContextV2,
    pub records: BTreeMap<Hash64, PalwClaimLocalRecordV1>,
    pub liability_state: Vec<u8>,
    pub material: BTreeMap<Hash64, Vec<u8>>,
}

impl PalwClaimLocalCheckpointV1 {
    /// Both expected arguments come from committed state, not the checkpoint sender. Comparing the
    /// complete material manifest prevents a sender replacing a complete archive with one spot check.
    pub fn validate(
        &self,
        point: &PalwBlockContextV2,
        expected: &BTreeMap<Hash64, PalwClaimLocalRecordV1>,
        expected_liability_state: &[u8],
    ) -> Result<PalwValidatedClaimCheckpointV1<'_>, &'static str> {
        if self.point != *point || self.records != *expected || self.liability_state != expected_liability_state {
            return Err("checkpoint differs from the fold's liability state");
        }
        let mut required = BTreeSet::new();
        for (id, record) in &self.records {
            if *id != record.claim || record.accepted_daa > point.daa_score || record.accepted_blue_score > point.blue_score {
                return Err("checkpoint claim identity or acceptance point differs");
            }
            if record.inputs.contains(id)
                || record.inputs.iter().any(|input| {
                    self.records
                        .get(input)
                        .is_some_and(|r| r.accepted_daa > record.accepted_daa || r.accepted_blue_score > record.accepted_blue_score)
                })
            {
                return Err("checkpoint input was not available at admission");
            }
            if (record.voided && (record.final_claim || record.certified))
                || (record.certified && !record.final_claim)
                || (record.liability_closed && !record.final_claim && !record.voided)
            {
                return Err("checkpoint phase, certification and liability flags disagree");
            }
            if record.inputs.iter().any(|input| !self.records.contains_key(input)) {
                return Err("checkpoint lost a consumed claim");
            }
            if !record.liability_closed {
                if record.material.is_empty() {
                    return Err("open liability has no material manifest");
                }
                required.extend(record.material.iter().copied());
            }
        }
        // Actual consumed outputs form a causal DAG. Shared execution / reservation groups are
        // handled separately below and may be mutual; they must not mask a cycle in input provenance.
        let mut pending: BTreeMap<Hash64, usize> = self.records.iter().map(|(id, r)| (*id, r.inputs.len())).collect();
        let mut consumers: BTreeMap<Hash64, Vec<Hash64>> = BTreeMap::new();
        for (id, record) in &self.records {
            for input in &record.inputs {
                consumers.entry(*input).or_default().push(*id);
            }
        }
        let mut ready: VecDeque<Hash64> = pending.iter().filter(|(_, n)| **n == 0).map(|(id, _)| *id).collect();
        let mut visited = 0;
        while let Some(id) = ready.pop_front() {
            visited += 1;
            for consumer in consumers.get(&id).into_iter().flatten() {
                let n = pending.get_mut(consumer).expect("consumer is a checkpoint record");
                *n -= 1;
                if *n == 0 {
                    ready.push_back(*consumer);
                }
            }
        }
        if visited != self.records.len() {
            return Err("checkpoint input provenance contains a cycle");
        }
        if !required.is_empty() && self.liability_state.is_empty() {
            return Err("open liabilities lost their policy, registry, randomness or court state");
        }
        if required != self.material.keys().copied().collect() {
            return Err("checkpoint is missing material or contains unbound material");
        }
        if self.material.iter().any(|(id, bytes)| palw_liability_material_id_v1(bytes) != *id) {
            return Err("checkpoint material bytes differ from their identity");
        }
        Ok(PalwValidatedClaimCheckpointV1 { checkpoint: self })
    }

    /// The dependency closure includes shared execution and reservation identities. Model IDs
    /// deliberately do not occur in this schema. Missing input provenance fails closed.
    fn dependencies(&self, claim: Hash64) -> Result<BTreeSet<Hash64>, &'static str> {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([claim]);
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id) {
                continue;
            }
            let record = self.records.get(&id).ok_or("missing dependency record")?;
            queue.extend(record.inputs.iter().copied());
            for (other_id, other) in &self.records {
                if (record.execution_root != ZERO_HASH64 && record.execution_root == other.execution_root)
                    || !record.reservations.is_disjoint(&other.reservations)
                {
                    queue.push_back(*other_id);
                }
            }
        }
        Ok(seen)
    }

    /// Reversible credit / reward eligibility. Liability expiry alone never certifies a claim.
    fn credit_eligible(&self, claim: Hash64) -> bool {
        self.dependencies(claim).is_ok_and(|ids| {
            ids.iter().all(|id| {
                let r = &self.records[id];
                r.final_claim && r.certified && !r.voided
            })
        })
    }

    /// Irreversible checkpoints use only credit whose complete dependency closure is past every
    /// liability. An unresolved claim remains in the checkpoint and supplies no such credit.
    fn irreversible_eligible(&self, claim: Hash64) -> bool {
        self.credit_eligible(claim) && self.dependencies(claim).is_ok_and(|ids| ids.iter().all(|id| self.records[id].liability_closed))
    }

    /// Revocation is directed: users of A are affected by A; A is not affected by a fault in a
    /// consumer. Shared execution roots and reservations form genuine shared liability groups.
    fn affected_by(&self, convicted: Hash64) -> Result<BTreeSet<Hash64>, &'static str> {
        if !self.records.contains_key(&convicted) {
            return Err("convicted claim is absent from the checkpoint");
        }
        let mut affected = BTreeSet::new();
        for id in self.records.keys() {
            if self.dependencies(*id)?.contains(&convicted) {
                affected.insert(*id);
            }
        }
        Ok(affected)
    }
}

/// Read capabilities exist only after exact state and byte validation. Runtime code cannot rank
/// forks or settle effects directly from a peer-supplied checkpoint's self-declared flags.
pub struct PalwValidatedClaimCheckpointV1<'a> {
    checkpoint: &'a PalwClaimLocalCheckpointV1,
}
impl PalwValidatedClaimCheckpointV1<'_> {
    pub fn credit_eligible(&self, claim: Hash64) -> bool {
        self.checkpoint.credit_eligible(claim)
    }
    pub fn irreversible_eligible(&self, claim: Hash64) -> bool {
        self.checkpoint.irreversible_eligible(claim)
    }
    pub fn dependencies(&self, claim: Hash64) -> Result<BTreeSet<Hash64>, &'static str> {
        self.checkpoint.dependencies(claim)
    }
    pub fn affected_by(&self, claim: Hash64) -> Result<BTreeSet<Hash64>, &'static str> {
        self.checkpoint.affected_by(claim)
    }
    /// Reversible frontier for fork ranking, separate from the irreversible checkpoint support.
    pub fn frontier(&self) -> Option<&PalwClaimLocalRecordV1> {
        self.checkpoint
            .records
            .values()
            .filter(|r| r.anchors && self.credit_eligible(r.claim))
            .max_by_key(|r| (r.accepted_blue_score, r.claim))
    }
    pub fn irreversible_frontier(&self) -> Option<&PalwClaimLocalRecordV1> {
        self.checkpoint
            .records
            .values()
            .filter(|r| r.anchors && self.irreversible_eligible(r.claim))
            .max_by_key(|r| (r.accepted_blue_score, r.claim))
    }
    /// Settlement of recorded effects does not assert that every recorded claim is correct. The
    /// unresolved set and its retrievable material remain bound, and only irrevocable credit backs it.
    pub fn settled(&self, effect_daa: u64) -> bool {
        self.irreversible_frontier().is_some_and(|r| r.accepted_daa >= effect_daa)
    }
    pub fn lifecycle_preserved_through(&self, blue: u64) -> bool {
        if blue > self.checkpoint.point.blue_score {
            return false;
        }
        self.checkpoint
            .records
            .values()
            .filter(|r| r.accepted_blue_score <= blue && !r.liability_closed)
            .all(|r| !r.material.is_empty() && r.material.iter().all(|m| self.checkpoint.material.contains_key(m)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn fixture() -> PalwClaimLocalCheckpointV1 {
        let data = b"public replay data, pinned policy, draw and court state".to_vec();
        let material = palw_liability_material_id_v1(&data);
        let mut records = BTreeMap::new();
        for n in 1..=5 {
            records.insert(
                id(n),
                PalwClaimLocalRecordV1 {
                    claim: id(n),
                    accepted_daa: n,
                    accepted_blue_score: n,
                    accepting_block: id(300 + n),
                    anchors: true,
                    inputs: BTreeSet::new(),
                    execution_root: id(100 + n),
                    reservations: BTreeSet::from([id(200 + n)]),
                    final_claim: n != 1,
                    certified: n != 1,
                    voided: false,
                    liability_closed: n == 2,
                    material: BTreeSet::from([material]),
                },
            );
        }
        records.get_mut(&id(3)).unwrap().inputs.insert(id(1));
        records.get_mut(&id(4)).unwrap().execution_root = id(101);
        records.get_mut(&id(5)).unwrap().reservations = BTreeSet::from([id(201)]);
        PalwClaimLocalCheckpointV1 {
            point: PalwBlockContextV2 { block: id(400), daa_score: 10, blue_score: 10, subsidy: 0 },
            records,
            liability_state: vec![1, 2, 3],
            material: BTreeMap::from([(material, data)]),
        }
    }
    #[test]
    fn rfc23s2_independence_shared_execution_and_actual_reservations() {
        let c = fixture();
        assert!(c.credit_eligible(id(2)));
        assert!(c.irreversible_eligible(id(2)));
        let checked = c.validate(&c.point, &c.records, &c.liability_state).unwrap();
        assert!(checked.settled(2));
        assert!(!checked.settled(3));
        assert!(checked.lifecycle_preserved_through(5));
        assert!(!checked.lifecycle_preserved_through(11), "a checkpoint cannot cover a future effect");
        assert_eq!(checked.irreversible_frontier().unwrap().claim, id(2));
        for n in [1, 3, 4, 5] {
            assert!(!c.credit_eligible(id(n)));
        }
        assert_eq!(c.affected_by(id(1)).unwrap(), BTreeSet::from([id(1), id(3), id(4), id(5)]));
        assert_eq!(c.affected_by(id(3)).unwrap(), BTreeSet::from([id(3)]));
    }
    #[test]
    fn rfc23s2_checkpoint_requires_the_exact_state_and_actual_bound_material() {
        let c = fixture();
        c.validate(&c.point, &c.records, &c.liability_state).unwrap();
        let mut bad = c.clone();
        bad.material.clear();
        assert!(bad.validate(&c.point, &c.records, &c.liability_state).is_err());
        let mut bad = c.clone();
        bad.material.values_mut().next().unwrap()[0] ^= 1;
        assert!(bad.validate(&c.point, &c.records, &c.liability_state).is_err());
        let mut bad = c.clone();
        bad.records.remove(&id(1));
        assert!(bad.validate(&c.point, &c.records, &c.liability_state).is_err());
        assert!(c.validate(&PalwBlockContextV2 { daa_score: 11, ..c.point }, &c.records, &c.liability_state).is_err());
        let mut cycle = c.clone();
        cycle.records.get_mut(&id(1)).unwrap().inputs.insert(id(3));
        cycle.records.get_mut(&id(3)).unwrap().accepted_daa = 1;
        cycle.records.get_mut(&id(3)).unwrap().accepted_blue_score = 1;
        assert!(cycle.validate(&cycle.point, &cycle.records, &cycle.liability_state).is_err());
    }
    #[test]
    fn rfc23s2_heartbeat_and_free_prompt_alone_cannot_support_a_checkpoint() {
        let mut c = fixture();
        c.records.clear();
        c.material.clear();
        let checked = c.validate(&c.point, &c.records, &c.liability_state).unwrap();
        assert!(checked.frontier().is_none());
        assert!(!checked.settled(0));
        let mut c = fixture();
        for r in c.records.values_mut() {
            r.anchors = false;
        }
        let checked = c.validate(&c.point, &c.records, &c.liability_state).unwrap();
        assert!(checked.credit_eligible(id(2)), "FP may have certified spend weight");
        assert!(checked.frontier().is_none());
        assert!(!checked.settled(2), "FP never becomes an attempt anchor");
    }

    #[test]
    fn rfc23s2_checkpoint_codec_and_late_conviction_keep_independent_support() {
        let c = fixture();
        let bytes = borsh::to_vec(&c).unwrap();
        let mut restored: PalwClaimLocalCheckpointV1 = borsh::from_slice(&bytes).unwrap();
        restored.validate(&c.point, &c.records, &c.liability_state).unwrap();
        restored.records.get_mut(&id(1)).unwrap().voided = true;
        assert!(restored.irreversible_eligible(id(2)));
        assert_eq!(restored.affected_by(id(1)).unwrap(), c.affected_by(id(1)).unwrap());
        let reverted: PalwClaimLocalCheckpointV1 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(reverted, c);
    }
}

impl crate::config::params::Params {
    pub fn sync_palw_claim_local_settlement_v1(&mut self) {
        use crate::config::params::ForkActivation;
        let from = self.palw_claim_local_settlement_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_claim_local_settlement_from_daa(from);
        }
    }
    pub fn validate_palw_claim_local_settlement_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::{
            config::params::ForkActivation,
            palw_mode_v2::{PalwConsensusMode, PalwModeV2Error},
        };
        let from = self.palw_claim_local_settlement_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.claim_local_settlement_from_daa(),
            _ => None,
        };
        if from != mirror {
            return Err(PalwModeV2Error::Invalid("claim-local settlement fence disagrees with its state mirror"));
        }
        if from.is_some() {
            return Err(PalwModeV2Error::Invalid(
                "RFC-0023 stage 2 cannot arm: runtime integration is incomplete and material retention / transition drills are unmeasured",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod fence_tests {
    use crate::config::params::{
        DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_t12_shipped_params,
    };
    #[test]
    fn rfc23s2_fence_is_dormant_some_only_and_never_armable_without_measurements() {
        for mut p in [MAINNET_PARAMS, TESTNET_PARAMS, SIMNET_PARAMS, DEVNET_PARAMS, palw_t12_shipped_params()] {
            assert!(p.palw_claim_local_settlement_v1.is_none());
            p.validate_palw_claim_local_settlement_v1().unwrap();
            assert!(p.palw_fences_v1().contains(&("palw_claim_local_settlement_v1", None)));
            let old = p.consensus_params_id();
            let identity = p.consensus_identity_id();
            p.palw_claim_local_settlement_v1 = Some(ForkActivation::never());
            p.sync_palw_claim_local_settlement_v1();
            p.validate_palw_claim_local_settlement_v1().unwrap();
            assert_eq!(p.consensus_params_id(), old);
            assert_eq!(p.consensus_identity_id(), identity);
            p.palw_claim_local_settlement_v1 = Some(ForkActivation::new(100));
            p.sync_palw_claim_local_settlement_v1();
            assert_ne!(p.consensus_params_id(), old);
            assert!(p.validate_palw_claim_local_settlement_v1().is_err());
        }
    }
}
