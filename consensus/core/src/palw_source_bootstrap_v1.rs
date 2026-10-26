//! Claim-level, locked-stake selected approval for the first useful-work source.
//!
//! Hashes below identify data. Approval is conditional on honest frozen stake,
//! participation, acquisition and anchor-bias assumptions; no calculation is
//! cryptographically proved. A historical optimistic settlement alone is never
//! an approved source. V1 beacon final-path IDs keep their original meaning.
use crate::{Hash64, config::params::ForkActivation, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_challenge::bootstrap_epoch_v1::{BootstrapCandidateV1, BootstrapEpochV1, BootstrapPolicyV1, BootstrapSeatV1};

pub const PALW_SELECTED_SOURCE_BOOTSTRAP_TABLE_V1: u8 = 137;

/// A runtime observation obtained only through this node's coherent own-state
/// getter. The retained native state owns all epoch/work/cohort allocations;
/// this type has no wire deserializer and imports no remote authority.
#[derive(Debug)]
pub struct PalwSourceBootstrapObservationV1 {
    pub checkpoint: Hash64,
    pub virtual_daa: u64,
    pub import_epoch: u64,
    pub state: std::sync::Arc<crate::palw_native_resource_v1::NativeChainStateV1>,
}

/// The bootstrap journal authenticates approvals and historical settlement.
/// It does not yet authenticate a downstream assignment's transitive seed /
/// checker graph. Keep that missing consumer authority explicit rather than
/// calling the collector with an invented independent (`false`) callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwSourceBootstrapConsumerUnavailableV1 {
    NativeConsumerAbsent,
    VersionedLineageUnimplemented,
}
#[derive(Clone, Copy, Debug)]
pub struct PalwSourceBootstrapConsumerObservationV1 {
    pub source: Option<misaka_palw_challenge::approved_source_v1::ApprovedSourceEventV1>,
    pub unavailable: PalwSourceBootstrapConsumerUnavailableV1,
}
pub const PALW_SELECTED_SOURCE_BOOTSTRAP_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/source-bootstrap/approval/v1";
pub const PALW_SELECTED_SOURCE_BOOTSTRAP_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/source-bootstrap/message/v1";

impl crate::palw_state_v2::PalwStateParamsV2 {
    /// Read-only mirror check for explicit private construction. It does not
    /// bypass the public startup refusal or approve any policy value.
    pub fn selected_source_bootstrap_v1_mirror_matches(&self, activation: u64, policy: &PalwSelectedSourceBootstrapPolicyV1) -> bool {
        self.selected_source_bootstrap_v1.as_ref().is_some_and(|(at, p)| *at == activation && p == policy)
    }
}

/// Local preparation faults are not assertions against a carrier or peer.
/// In particular missing own history is not permission to omit a journal row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwSourceBootstrapContextFaultV1 {
    Capacity(crate::palw_native_resource_v1::NativeCoverageV1),
    CapacityUnavailable,
    HistoryUnavailable,
    StoreUnreadable,
}
impl std::fmt::Display for PalwSourceBootstrapContextFaultV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Capacity(e) => write!(f, "{e}"),
            Self::CapacityUnavailable => f.write_str("own bootstrap snapshot capacity is unavailable"),
            Self::HistoryUnavailable => f.write_str("own bootstrap selected-chain history is unavailable"),
            Self::StoreUnreadable => f.write_str("own bootstrap native store/context is unreadable"),
        }
    }
}

/// One finite, explicitly configured epoch. There is no automatic retry epoch
/// or fallback anchor. Values are policy inputs, not approved release defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSelectedSourceBootstrapPolicyV1 {
    pub version: u16,
    pub chain_genesis: Hash64,
    pub network: Hash64,
    pub ruleset: Hash64,
    pub epoch: u64,
    /// DAA cutoffs. The node resolves the last selected-chain block <= cutoff,
    /// authenticates its header and, for freeze, its native AFTER-state.
    pub freeze_cutoff_daa: u64,
    pub anchor_cutoff_daa: u64,
    pub starts_at_daa: u64,
    pub ends_at_daa: u64,
    /// Fixed stake retention includes the source's court/liability horizon.
    /// A source not settled before this bound supplies no event.
    pub stake_release_daa: u64,
    pub selection: BootstrapPolicyV1,
    /// Exactly one code/scope profile for this first-source contract. This is
    /// claim checker coverage, never model qualification or a model coefficient.
    pub source_profile: Hash64,
    pub checker_scope: Hash64,
    pub maximum_claim_bytes: u64,
    pub maximum_check_work: u64,
    /// Exact selected-chain header visits, not a DAA-distance approximation.
    pub maximum_history_headers: u32,
    /// Review identities document assumptions; nonzero IDs do not approve them.
    pub security_review: Hash64,
    pub economic_review: Hash64,
}
impl PalwSelectedSourceBootstrapPolicyV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.selection.validate().map_err(|_| "bootstrap selection policy")?;
        if self.version != 1
            || self.freeze_cutoff_daa >= self.anchor_cutoff_daa
            || self.anchor_cutoff_daa >= self.starts_at_daa
            || self.starts_at_daa >= self.ends_at_daa
            || self.ends_at_daa >= self.stake_release_daa
            || self.maximum_claim_bytes == 0
            || self.maximum_check_work == 0
            || self.maximum_history_headers == 0
            || [
                self.chain_genesis,
                self.network,
                self.ruleset,
                self.source_profile,
                self.checker_scope,
                self.security_review,
                self.economic_review,
            ]
            .contains(&Hash64::default())
        {
            return Err("bootstrap policy identity/interval/coverage is incomplete");
        }
        Ok(())
    }
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(hash_serialized(b"MISAKA/PALW/SOURCE-BOOTSTRAP-POLICY/V1", self))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSelectedSourceBootstrapFenceV1 {
    pub at: ForkActivation,
    pub policy: PalwSelectedSourceBootstrapPolicyV1,
}
impl crate::config::params::Params {
    pub fn sync_palw_selected_source_bootstrap_v1(&mut self) {
        let mirror =
            self.palw_selected_source_bootstrap_v1.filter(|f| f.at != ForkActivation::never()).map(|f| (f.at.daa_score(), f.policy));
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state.selected_source_bootstrap_v1 = mirror;
        }
    }
    pub fn palw_selected_source_bootstrap_at(&self, daa: u64) -> Option<&PalwSelectedSourceBootstrapPolicyV1> {
        self.palw_selected_source_bootstrap_v1
            .as_ref()
            .filter(|f| f.at.is_active(daa) && f.at != ForkActivation::never())
            .map(|f| &f.policy)
    }
    pub fn palw_selected_source_bootstrap_fence(&self) -> Option<ForkActivation> {
        self.palw_selected_source_bootstrap_v1.map(|f| f.at).filter(|f| *f != ForkActivation::never())
    }
    pub fn validate_palw_selected_source_bootstrap_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        let mirror =
            self.palw_selected_source_bootstrap_v1.filter(|f| f.at != ForkActivation::never()).map(|f| (f.at.daa_score(), f.policy));
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
            && bundle.state.selected_source_bootstrap_v1 != mirror
        {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                "bootstrap native policy mirror is not synchronized with Params",
            ));
        }
        let Some(f) = self.palw_selected_source_bootstrap_v1.filter(|f| f.at != ForkActivation::never()) else { return Ok(()) };
        f.policy.validate().map_err(crate::palw_mode_v2::PalwModeV2Error::Invalid)?;
        if f.at.daa_score() > f.policy.freeze_cutoff_daa {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid("bootstrap lock fence must precede its population freeze"));
        }
        // This refusal is removed only after every native/worker/collector
        // consumer and consolidated real-node acceptance are complete.
        Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
            "selected-source bootstrap cannot be activated before native lineage/worker coverage and security/economic acceptance",
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceBootstrapMemberV1 {
    pub bond: PalwBondKeyV2,
    pub locked_weight: u64,
}
impl PalwSourceBootstrapMemberV1 {
    pub fn candidate(&self) -> BootstrapCandidateV1 {
        BootstrapCandidateV1 {
            bond: crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&self.bond),
            locked_weight: self.locked_weight,
        }
    }
}

/// Produced only by the node's own selected-chain snapshot resolver, never by
/// carrier bytes or an RPC reply. Retain its allocation owner across the fold.
/// Once journaled the frozen roster is recovered from authenticated own state;
/// a pruned snapshot is not silently replaced with the current stake roster.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceBootstrapOwnContextV1 {
    pub policy: PalwSelectedSourceBootstrapPolicyV1,
    pub frozen_block: Hash64,
    pub frozen_state_root: Hash64,
    pub frozen_block_daa: u64,
    pub population: Vec<PalwSourceBootstrapMemberV1>,
    /// The exact frozen native registry exceeded the policy count. No partial
    /// population is retained or drawn; the entire fixed epoch is unavailable.
    pub population_refused: bool,
    pub anchor: Option<(Hash64, u64)>,
}
impl crate::palw_native_resource_v1::NativeRetainedV1<PalwSourceBootstrapOwnContextV1> {
    /// Move this owner into shared fold extras after reserving the concrete Arc
    /// envelope. Clones share the same ticket; no deep roster clone is made.
    pub fn into_shared_source_bootstrap_v1(self) -> Result<std::sync::Arc<Self>, crate::palw_native_resource_v1::NativeCoverageV1> {
        use crate::palw_native_resource_v1::{NativeRetainedV1, reserve_native_requests_v1};
        let bytes = (std::mem::size_of::<Self>() + std::mem::size_of::<[usize; 2]>() + std::mem::align_of::<Self>()) as u64;
        let (value, mut lease) = self.into_parts_v1();
        if let Some(owner) = &mut lease {
            owner.grow_memory_v1(bytes)?;
        } else {
            lease = reserve_native_requests_v1(bytes)?;
        }
        Ok(std::sync::Arc::new(NativeRetainedV1::from_owner_reserved_v1(value, lease)))
    }
}
impl PalwSourceBootstrapOwnContextV1 {
    /// Allocation API for the own-node historical-state resolver. This factory
    /// checks shape and constructs the roster; it does not authenticate a remote
    /// block/root or substitute for the resolver's real chain/import checks.
    pub fn from_own_frozen_state_v1(
        policy: PalwSelectedSourceBootstrapPolicyV1,
        frozen_block: Hash64,
        frozen_state_root: Hash64,
        frozen_block_daa: u64,
        frozen: &crate::palw_state_v2::PalwChainStateV2,
        anchor: Option<(Hash64, u64)>,
    ) -> Result<crate::palw_native_resource_v1::NativeRetainedV1<Self>, crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_native_resource_v1::{NativeCoverageV1, NativeRetainedV1, reserve_native_requests_v1};
        use crate::palw_state_v2::{PalwBondStatusV2, PalwStateV2Error as E};
        policy.validate().map_err(|e| E::KernelRouteRefused(e.into()))?;
        let eligible = |key: &PalwBondKeyV2, row: &crate::palw_state_v2::PalwBondStateV2| {
            matches!(row.status, PalwBondStatusV2::Active) && frozen.slashable_available_v2(key, frozen_block_daa, None) > 0
        };
        let mut n = 0usize;
        let mut population_refused = false;
        for (key, row) in frozen.bonds_iter() {
            if eligible(key, row) {
                n = n.checked_add(1).ok_or(E::Overflow("bootstrap frozen candidate count"))?;
                if n > policy.selection.maximum_population as usize {
                    population_refused = true;
                    n = 0;
                    break;
                }
            }
        }
        let bytes =
            n.checked_mul(std::mem::size_of::<PalwSourceBootstrapMemberV1>()).ok_or(E::Overflow("bootstrap frozen member vector"))?;
        let lease = reserve_native_requests_v1(bytes as u64).map_err(E::NativeResource)?;
        let mut population = Vec::new();
        population.try_reserve_exact(n).map_err(|_| {
            E::NativeResource(NativeCoverageV1::unsupported::<Self>("bootstrap frozen roster", "allocator refusal after reservation"))
        })?;
        for (key, row) in frozen.bonds_iter() {
            if population_refused {
                break;
            }
            if eligible(key, row) {
                let locked_weight = u64::try_from(frozen.slashable_available_v2(key, frozen_block_daa, None))
                    .map_err(|_| E::Overflow("bootstrap native locked weight"))?;
                population.push(PalwSourceBootstrapMemberV1 { bond: *key, locked_weight });
            }
        }
        population.sort_unstable_by_key(|member| member.candidate().bond);
        let context = Self { policy, frozen_block, frozen_state_root, frozen_block_daa, population, population_refused, anchor };
        context.validate_shape().map_err(|e| E::KernelRouteRefused(e.into()))?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(context, lease))
    }
    /// Retained fixed-anchor update for the own-node resolver. New input bytes
    /// cannot use this to authenticate an anchor or replace a frozen roster.
    pub fn with_anchor_owned_v1(
        &self,
        anchor: Option<(Hash64, u64)>,
    ) -> Result<crate::palw_native_resource_v1::NativeRetainedV1<Self>, crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_native_resource_v1::{NativeHeapV1, NativeRetainedV1, reserve_native_requests_v1};
        use crate::palw_state_v2::PalwStateV2Error as E;
        if self.anchor.is_some() && self.anchor != anchor {
            return Err(E::KernelRouteRefused("bootstrap own context cannot change its fixed anchor".into()));
        }
        let bound = self.native_heap_v1("bootstrap own context anchor copy");
        let lease = reserve_native_requests_v1(bound.bytes().ok_or(E::NativeResource(bound))?).map_err(E::NativeResource)?;
        let mut next = self.clone();
        next.anchor = anchor;
        next.validate_shape().map_err(|e| E::KernelRouteRefused(e.into()))?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(next, lease))
    }
    pub fn validate_shape(&self) -> Result<(), &'static str> {
        self.policy.validate()?;
        if self.frozen_block == Hash64::default()
            || self.frozen_state_root == Hash64::default()
            || self.frozen_block_daa > self.policy.freeze_cutoff_daa
            || self.population.len() > self.policy.selection.maximum_population as usize
            || (self.population_refused && !self.population.is_empty())
            || self.population.iter().any(|m| m.locked_weight == 0)
            || self.population.windows(2).any(|w| w[0].candidate().bond >= w[1].candidate().bond)
            || self.anchor.is_some_and(|(h, d)| h == Hash64::default() || d > self.policy.anchor_cutoff_daa)
        {
            return Err("bootstrap own-chain context has invalid shape");
        }
        Ok(())
    }
    pub fn population_root(&self) -> Result<[u8; 64], &'static str> {
        self.validate_shape()?;
        // Same canonical Borsh framing as bootstrap_population_root_v1, without
        // allocating another roster or encoded Vec.
        let mut h = blake2b_simd::Params::new().hash_length(64).key(b"MISAKA/PALW/BOOTSTRAP-POPULATION/V1").to_state();
        let n = u32::try_from(self.population.len()).map_err(|_| "bootstrap roster overflow")?;
        h.update(&(4 + u64::from(n) * 72).to_le_bytes());
        h.update(&n.to_le_bytes());
        for m in &self.population {
            let c = m.candidate();
            h.update(&c.bond);
            h.update(&c.locked_weight.to_le_bytes());
        }
        Ok(h.finalize().as_bytes().try_into().expect("64-byte digest"))
    }
    pub fn epoch(&self) -> Result<BootstrapEpochV1, &'static str> {
        if self.population_refused || self.population.is_empty() {
            return Err("fixed bootstrap epoch has no positive frozen locked-stake population; no replacement roster");
        }
        let (anchor, actual_daa) = self.anchor.ok_or("bootstrap future own-chain anchor is unavailable")?;
        if actual_daa <= self.policy.freeze_cutoff_daa {
            return Err("fixed bootstrap interval has no native anchor after population freeze; no substitute anchor");
        }
        Ok(BootstrapEpochV1 {
            chain_genesis: self.policy.chain_genesis.as_bytes(),
            ruleset: self.policy.ruleset.as_bytes(),
            policy: self.policy.selection.id().map_err(|_| "bootstrap policy encoding")?,
            population_root: self.population_root()?,
            anchor: anchor.as_bytes(),
            epoch: self.policy.epoch,
            population_frozen_at: self.policy.freeze_cutoff_daa,
            anchor_position: self.policy.anchor_cutoff_daa,
            starts_at: self.policy.starts_at_daa,
            ends_at: self.policy.ends_at_daa,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceBootstrapEpochRowV1 {
    pub context: PalwSourceBootstrapOwnContextV1,
    pub cohort: Vec<BootstrapSeatV1>,
    /// Fixed rejection exhaustion or absence of a post-freeze anchor is
    /// terminal for the epoch, without stopping ordinary native blocks.
    pub draw_refused: bool,
    pub source_works: u32,
}

/// All material and scope are fixed by native claim rows before approval. A
/// checker signs this exact statement after checking the actual public material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceBootstrapWorkV1 {
    pub epoch: Hash64,
    pub policy: Hash64,
    pub claim: Hash64,
    pub producer: Hash64,
    pub consumer: Hash64,
    pub canonical_work: Hash64,
    pub source_profile: Hash64,
    pub checker_scope: Hash64,
    pub class_binding: Hash64,
    pub execution_commitment: Hash64,
    pub material_scope: Hash64,
    pub accepted_daa: u64,
    pub deadline_daa: u64,
}
impl PalwSourceBootstrapWorkV1 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(hash_serialized(b"MISAKA/PALW/SOURCE-BOOTSTRAP-WORK/V1", self))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceBootstrapApprovalRowV1 {
    pub slot: u16,
    pub signer: PalwBondKeyV2,
    pub signed_daa: u64,
    pub accepted_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceBootstrapWorkRowV1 {
    pub work: PalwSourceBootstrapWorkV1,
    pub approvals: Vec<PalwSourceBootstrapApprovalRowV1>,
    /// Only native historical settlement may fill this; a timer in a worker
    /// cannot turn approvals or an optimistic Final into source eligibility.
    pub settlement: Option<(u64, u64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwSourceBootstrapActionV1 {
    Open { policy: Hash64, epoch: u64, claim: Hash64 },
    Approve { work: Hash64, slot: u16, statement: Hash64, signed_daa: u64 },
}

pub fn palw_source_bootstrap_message_v1(network: Hash64, actor: PalwBondKeyV2, action: &PalwSourceBootstrapActionV1) -> Hash64 {
    Hash64::from_bytes(hash_serialized(PALW_SELECTED_SOURCE_BOOTSTRAP_MESSAGE_DOMAIN_V1, &(network, actor, action)))
}

/// Borsh identity framing streamed directly into the digest; does not allocate
/// an encoded copy of a native claim body or roster.
pub(crate) fn hash_serialized<T: BorshSerialize>(domain: &[u8], value: &T) -> [u8; 64] {
    struct DigestWriter(blake2b_simd::State);
    impl std::io::Write for DigestWriter {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let n = borsh::object_length(value).expect("in-memory source identity serializes");
    let mut writer = DigestWriter(blake2b_simd::Params::new().hash_length(64).key(domain).to_state());
    writer.0.update(&(n as u64).to_le_bytes());
    value.serialize(&mut writer).expect("digest writer is infallible");
    writer.0.finalize().as_bytes().try_into().expect("64-byte digest")
}
