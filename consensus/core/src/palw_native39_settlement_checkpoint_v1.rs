//! A consensus-derived archival certificate for an exact closed native prefix.
//! Header/root authentication transfers its identity during ordinary pruning IBD;
//! no RPC publication, model computation proof, or added work quantity is trusted.
use crate::palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1};
use crate::palw_native_settlement_v1::{PalwDnsRetirementV1, PalwSettlementPolicyV1};
use crate::{
    Hash, Hash64,
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwStateV2Error},
};
use borsh::{BorshDeserialize, BorshSerialize};
pub const NATIVE39_SETTLEMENT_CHECKPOINT_TABLE_V1: u8 = 194;
pub const NATIVE39_SETTLEMENT_CHECKPOINT_KEY_V1: [u8; 1] = [1];
/// Trusted immutable network context, supplied only by the native processor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Native39SettlementFoldV1 {
    pub retirement: PalwDnsRetirementV1,
    pub ruleset: Hash,
    pub evm_activation_daa: u64,
    pub fork_choice_commitment: Option<crate::config::params::ForkActivation>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Native39SettlementCheckpointV1 {
    pub version: u16,
    pub ruleset: Hash,
    pub policy_id: Hash64,
    pub retirement_daa: u64,
    pub depth_required: u64,
    pub work_required: u128,
    pub operator_permille: u16,
    pub class_permille: u16,
    pub effect: PalwBlockContextV2,
    pub point: PalwBlockContextV2,
    pub parent: PalwBlockContextV2,
    pub parent_state_root: Hash64,
    pub parent_leaf: Option<crate::palw_fork_choice_commitment_v1::PalwForkChoiceLeafV1>,
    pub frontier: Hash64,
    pub frontier_blue: u64,
    pub depth: u64,
    pub work: u128,
    pub max_operator_work: u128,
    pub max_class_work: u128,
}
crate::palw_native_resource_v1::native_struct!(Native39SettlementCheckpointV1;version,ruleset,policy_id,retirement_daa,depth_required,work_required,operator_permille,class_permille,effect,point,parent,parent_state_root,parent_leaf,frontier,frontier_blue,depth,work,max_operator_work,max_class_work);
crate::palw_native_resource_v1::wire::wire_struct!(Native39SettlementCheckpointV1;u16,Hash,Hash64,u64,u64,u128,u16,u16,PalwBlockContextV2,PalwBlockContextV2,PalwBlockContextV2,Hash64,Option<crate::palw_fork_choice_commitment_v1::PalwForkChoiceLeafV1>,Hash64,u64,u64,u128,u128,u128);
fn bad(s: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.to_string())
}
impl Native39SettlementCheckpointV1 {
    pub fn policy_v1(&self) -> PalwSettlementPolicyV1 {
        PalwSettlementPolicyV1 {
            settled_anchor_depth: self.depth_required,
            unique_mature_work: self.work_required,
            max_operator_permille: self.operator_permille,
            max_class_permille: self.class_permille,
        }
    }
    pub fn parent_committed_root_v1(&self, fence: Option<crate::config::params::ForkActivation>) -> Hash64 {
        self.parent_leaf.as_ref().map_or(self.parent_state_root, |leaf| {
            crate::palw_fork_choice_commitment_v1::palw_committed_root_of_parts_v1(leaf, &self.parent_state_root, fence)
        })
    }
    pub fn validate_shape_v1(&self) -> Result<(), PalwStateV2Error> {
        if self.version != 1
            || !self.policy_v1().valid()
            || self.policy_id != self.policy_v1().id()
            || self.effect.block == Hash64::default()
            || self.effect.daa_score > self.point.daa_score
            || self.effect.blue_score > self.frontier_blue
            || self.parent.block == Hash64::default()
            || self.parent_state_root == Hash64::default()
            || self.parent.daa_score > self.point.daa_score
            || self.parent.blue_score > self.point.blue_score
            || self.depth < self.depth_required
            || self.work < self.work_required
            || self.frontier == Hash64::default()
        {
            return Err(bad("native39 settlement checkpoint scope/policy/evidence"));
        }
        if self.frontier != self.effect.block
            || self.frontier_blue != self.effect.blue_score
            || self.max_operator_work == 0
            || self.max_class_work == 0
            || self.max_operator_work > self.work
            || self.max_class_work > self.work
            || self
                .max_operator_work
                .checked_mul(1000)
                .zip(self.work.checked_mul(u128::from(self.operator_permille)))
                .is_none_or(|(a, b)| a > b)
            || self
                .max_class_work
                .checked_mul(1000)
                .zip(self.work.checked_mul(u128::from(self.class_permille)))
                .is_none_or(|(a, b)| a > b)
        {
            return Err(bad("native39 checkpoint irreversible coverage/concentration"));
        }
        if self.parent_leaf.as_ref().is_some_and(|leaf| {
            leaf.block != self.parent.block || leaf.daa_score != self.parent.daa_score || leaf.blue_score != self.parent.blue_score
        }) {
            return Err(bad("native39 checkpoint parent leaf clock differs"));
        }
        Ok(())
    }
}
pub fn read_native39_settlement_checkpoint_v1(
    state: &PalwChainStateV2,
) -> Result<Option<NativeRetainedV1<Native39SettlementCheckpointV1>>, PalwStateV2Error> {
    let Some(route) = state.kernel_route_state_v1() else { return Ok(None) };
    let Some(bytes) = route.aux.get(&(NATIVE39_SETTLEMENT_CHECKPOINT_TABLE_V1, NATIVE39_SETTLEMENT_CHECKPOINT_KEY_V1.to_vec())) else {
        return Ok(None);
    };
    let row =
        crate::palw_native_resource_v1::retain_native_value_decode_classified_v1::<Native39SettlementCheckpointV1, std::io::Error>(
            bytes,
            || borsh::from_slice(bytes),
        )
        .map_err(|e| match e {
            NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
            NativeDecodeErrorV1::Decode(e) => bad(e),
        })?;
    row.validate_shape_v1()?;
    Ok(Some(row))
}
pub fn validate_native39_settlement_checkpoint_context_v1(
    state: &PalwChainStateV2,
    params: &crate::config::params::Params,
) -> Result<(), PalwStateV2Error> {
    let Some(row) = read_native39_settlement_checkpoint_v1(state)? else { return Ok(()) };
    let retirement =
        params.palw_dns_retirement.ok_or_else(|| bad("native39 archived settlement requires actual retirement policy"))?;
    if row.ruleset != params.consensus_params_id()
        || row.policy_id != retirement.settlement.id()
        || row.retirement_daa != retirement.activation.daa_score()
        || row.effect.daa_score < params.evm_activation_daa_score
        || !retirement.activation.is_active(row.point.daa_score)
        || state.last_point().is_none_or(|p| row.point.daa_score > p.daa_score || row.point.blue_score > p.blue_score)
        || state.safe_frontier().0 < row.frontier_blue
    {
        return Err(bad("native39 archived settlement differs from actual network/header frontier context"));
    }
    if params.palw_fork_choice_commitment_v1.is_some_and(|f| f.is_active(row.parent.daa_score)) && row.parent_leaf.is_none() {
        return Err(bad("native39 archived settlement requires actual parent fork-choice leaf"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn point(n: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: Hash64::from_u64_word(n), daa_score: n, blue_score: n, subsidy: 0 }
    }
    fn schema_fixture() -> Native39SettlementCheckpointV1 {
        let policy = PalwSettlementPolicyV1 {
            settled_anchor_depth: 2,
            unique_mature_work: 10,
            max_operator_permille: 500,
            max_class_permille: 500,
        };
        Native39SettlementCheckpointV1 {
            version: 1,
            ruleset: Hash::from_bytes([3; 32]),
            policy_id: policy.id(),
            retirement_daa: 1,
            depth_required: 2,
            work_required: 10,
            operator_permille: 500,
            class_permille: 500,
            effect: point(5),
            point: point(12),
            parent: point(11),
            parent_state_root: Hash64::from_u64_word(44),
            parent_leaf: None,
            frontier: point(5).block,
            frontier_blue: 5,
            depth: 2,
            work: 10,
            max_operator_work: 5,
            max_class_work: 5,
        }
    }
    #[test]
    fn native39_archival_generic_policy_does_not_reuse_covered_quantity_for_new_effects() {
        use crate::palw_native_settlement_v1::{MatureUsefulWorkV1 as Fact, certify_native_effect_v1};
        // Arithmetic only: real source-family authentication is the owning fold's
        // responsibility. Mixed closed facts retain separate identity/operators;
        // the archival prefix is never another additive work fact.
        let fact = |n: u64, work: u128| Fact {
            identity: Hash64::from_u64_word(n),
            anchor: Hash64::from_u64_word(n + 100),
            operator: Hash64::from_u64_word(n + 200),
            class: Hash64::from_u64_word(n + 300),
            anchor_blue: n,
            accepted_blue: n,
            anchor_daa: n,
            accepted_daa: n,
            matured_daa: 20,
            work,
        };
        let policy = PalwSettlementPolicyV1 {
            settled_anchor_depth: 2,
            unique_mature_work: 10,
            max_operator_permille: 500,
            max_class_permille: 500,
        };
        let mixed = [fact(10, 5), fact(11, 5)];
        let certified = certify_native_effect_v1(policy, (10, 10), 30, true, true, true, true, &mixed).unwrap();
        assert_eq!((certified.depth, certified.work), (2, 10));
        let later = [mixed[0], mixed[1], fact(15, 1)];
        assert!(
            certify_native_effect_v1(policy, (12, 12), 30, true, true, true, true, &later).is_err(),
            "one later anchor cannot borrow an older prefix certificate's work/depth"
        );
        let mut concentrated = mixed;
        concentrated[1].operator = concentrated[0].operator;
        assert!(certify_native_effect_v1(policy, (10, 10), 30, true, true, true, true, &concentrated).is_err());
        assert!(
            certify_native_effect_v1(policy, (10, 10), 30, false, true, true, true, &mixed).is_err(),
            "archival evidence does not establish an unexecuted EVM effect"
        );
    }
    #[test]
    fn native39_archival_schema_preserves_exact_policy_clocks_and_concentration() {
        // Codec/arithmetic fixture only; native construction has its real-node
        // signed-receipt, maturity, reorg, GC and pruning-import campaign.
        let row = schema_fixture();
        row.validate_shape_v1().unwrap();
        let bytes = borsh::to_vec(&row).unwrap();
        crate::palw_native_resource_v1::preflight_native_value_v1::<Native39SettlementCheckpointV1>(
            bytes.as_slice(),
            bytes.len() as u64,
        )
        .unwrap();
        assert_eq!(borsh::from_slice::<Native39SettlementCheckpointV1>(&bytes).unwrap(), row);
        let mut concentrated = row.clone();
        concentrated.max_operator_work = 6;
        assert!(concentrated.validate_shape_v1().is_err());
        let mut changed_policy = row.clone();
        changed_policy.work_required = 11;
        assert!(changed_policy.validate_shape_v1().is_err());
        let mut future = row.clone();
        future.effect = point(13);
        assert!(future.validate_shape_v1().is_err());
        let mut liable_frontier = row.clone();
        liable_frontier.frontier = point(8).block;
        liable_frontier.frontier_blue = 8;
        assert!(liable_frontier.validate_shape_v1().is_err());
        let mut overflow = row;
        overflow.work = u128::MAX;
        assert!(overflow.validate_shape_v1().is_err());
    }
}

// Exact scalar coverage for the archival parent commitment and trusted context.
// This adds no encoded fields to the pre-existing leaf or policy definitions.
crate::palw_native_resource_v1::native_struct!(crate::palw_fork_choice_commitment_v1::PalwWeightAllocationSlotV1;version,allocation_root,capped_safe_weight,capped_bounded_immature);
crate::palw_native_resource_v1::wire::wire_struct!(crate::palw_fork_choice_commitment_v1::PalwWeightAllocationSlotV1;u16,Hash64,u128,u128);
crate::palw_native_resource_v1::native_struct!(crate::palw_fork_choice_commitment_v1::PalwForkChoiceLeafV1;leaf_version,block,daa_score,blue_score,safe_frontier_blue_score,safe_frontier,safe_weight,bounded_immature,bonds_len,weight_allocation);
crate::palw_native_resource_v1::wire::wire_struct!(crate::palw_fork_choice_commitment_v1::PalwForkChoiceLeafV1;u16,Hash64,u64,u64,u64,Hash64,u128,u128,u64,crate::palw_fork_choice_commitment_v1::PalwWeightAllocationSlotV1);
crate::palw_native_resource_v1::native_struct!(PalwSettlementPolicyV1;settled_anchor_depth,unique_mature_work,max_operator_permille,max_class_permille);
crate::palw_native_resource_v1::native_struct!(PalwDnsRetirementV1;activation,settlement,legacy_evidence_horizon_daa);
crate::palw_native_resource_v1::native_struct!(Native39SettlementFoldV1;retirement,ruleset,evm_activation_daa,fork_choice_commitment);
