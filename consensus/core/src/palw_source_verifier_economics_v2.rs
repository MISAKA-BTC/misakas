//! Private, unapproved selected-source CHECKER economics. Aux146 is a new
//! contract; aux139 router rows and historical approvals retain their meanings.
//! Queue allocation is not payment. Only the common consumed prefix can ACK it.
use crate::{
    Hash64,
    config::params::{ForkActivation, Params},
    palw_kernel_route_v1::PalwKernelRouteStateV1,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDecodeErrorV1 as E, NativeRetainedV1, NativeWireScanV1, preflight_native_value_v1,
        reserve_native_requests_v1, retain_native_value_decode_classified_v1,
    },
    palw_source_bootstrap_v1::{PalwSourceBootstrapEpochRowV1, PalwSourceBootstrapWorkRowV1},
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2},
};
use borsh::{BorshDeserialize, BorshSerialize};
pub const SOURCE_VERIFIER_ECONOMICS_TABLE_V2: u8 = 146;
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceVerifierEconomicsPolicyV2 {
    pub version: u16,
    pub network: Hash64,
    pub bootstrap_policy: Hash64,
    pub exposure_per_work_sompi: u64,
    pub minimum_fee_per_checker_sompi: u64,
    pub maximum_funded_works: u32,
    pub maximum_liabilities: u32,
}
impl PalwSourceVerifierEconomicsPolicyV2 {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 2
            || self.network == Hash64::default()
            || self.bootstrap_policy == Hash64::default()
            || self.exposure_per_work_sompi == 0
            || self.minimum_fee_per_checker_sompi == 0
            || self.maximum_funded_works == 0
            || self.maximum_liabilities == 0
        {
            return Err("source checker economics needs explicit finite nonzero private terms".into());
        }
        Ok(())
    }
    pub fn id(&self) -> Hash64 {
        identity_v2(b"MISAKA/PALW/SOURCE-VERIFIER-POLICY/V2", self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSourceVerifierEconomicsFenceV2 {
    pub activation: ForkActivation,
    pub policy: PalwSourceVerifierEconomicsPolicyV2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceVerifierFeeTermsV2 {
    pub version: u16,
    pub policy: Hash64,
    pub work: Hash64,
    pub sponsor: PalwBondKeyV2,
    pub fee_per_checker_sompi: u64,
}
impl PalwSourceVerifierFeeTermsV2 {
    pub fn id(&self) -> Hash64 {
        identity_v2(b"MISAKA/PALW/SOURCE-VERIFIER-FUNDING/V2", self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceVerifierEconomicsEpochV2 {
    pub version: u16,
    pub activation: u64,
    pub policy: PalwSourceVerifierEconomicsPolicyV2,
    pub epoch: Hash64,
    pub until_daa: u64,
    pub funded_works: u32,
    pub liabilities: u32,
}
impl PalwSourceVerifierEconomicsEpochV2 {
    pub(crate) fn same_contract(&self, other: &Self) -> bool {
        self.version == other.version
            && self.activation == other.activation
            && self.policy == other.policy
            && self.epoch == other.epoch
            && self.until_daa == other.until_daa
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceVerifierFundingV2 {
    pub version: u16,
    pub terms: PalwSourceVerifierFeeTermsV2,
    pub epoch: Hash64,
    pub funded_daa: u64,
    pub until_daa: u64,
    pub distinct_payees: u16,
    pub prepaid: u64,
    pub held: u64,
    /// Cumulative irrevocable checker allocations, INCLUDING acknowledged ones.
    pub queued: u64,
    /// Cumulative principal allocated to the sponsor, not an income or an ACK.
    pub refunded: u64,
    pub forfeited: u64,
    pub closed: bool,
    pub refund_queue: Option<Hash64>,
    pub refund_queued_block: Option<Hash64>,
    pub refund_acknowledged_block: Option<Hash64>,
    pub refund_acknowledged_daa: Option<u64>,
}
impl PalwSourceVerifierFundingV2 {
    pub fn conserves(&self) -> bool {
        self.held.checked_add(self.queued).and_then(|n| n.checked_add(self.refunded)).and_then(|n| n.checked_add(self.forfeited))
            == Some(self.prepaid)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwSourceVerifierDutyStatusV2 {
    Active,
    Released { block: Hash64, daa: u64 },
    Convicted { block: Hash64, daa: u64, collected: u64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwSourceVerifierEntitlementV2 {
    Held,
    Forfeited { block: Hash64 },
    Queued { block: Hash64, queue: Hash64 },
    QueueAcknowledged { queued_block: Hash64, queue: Hash64, acknowledged_block: Hash64, acknowledged_daa: u64, amount: u64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceVerifierLiabilityV2 {
    pub version: u16,
    pub policy: Hash64,
    pub work: Hash64,
    pub claim: Hash64,
    pub actor: PalwBondKeyV2,
    pub funding: Hash64,
    pub exposure: u64,
    pub fee: u64,
    pub accepted_daa: u64,
    pub until_daa: u64,
    pub status: PalwSourceVerifierDutyStatusV2,
    pub entitlement: PalwSourceVerifierEntitlementV2,
}
impl PalwSourceVerifierLiabilityV2 {
    pub fn id(&self) -> Hash64 {
        identity_v2(b"MISAKA/PALW/SOURCE-VERIFIER-DUTY/V2", &(self.policy, self.work, self.actor))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceVerifierBondHoldV2 {
    pub version: u16,
    pub actor: PalwBondKeyV2,
    pub policy: Hash64,
    pub until_daa: u64,
    pub reserved: u128,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceVerifierCourtOutcomeV2 {
    pub version: u16,
    pub policy: Hash64,
    pub work: Hash64,
    pub claim: Hash64,
    pub actor: PalwBondKeyV2,
    pub block: Hash64,
    pub daa: u64,
    pub accuser: Hash64,
    pub exposure: u64,
    pub collected: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceVerifierDutyRefV2 {
    pub actor: PalwBondKeyV2,
    pub work: Hash64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceVerifierQueueV2 {
    pub version: u16,
    pub work: Hash64,
    pub actor: PalwBondKeyV2,
    pub funding: Hash64,
    pub duty: Option<Hash64>,
    pub queue: Hash64,
    pub payload: Hash64,
    pub amount: u64,
    pub queued_block: Hash64,
    pub queued_daa: u64,
}
pub(crate) fn queue_receipt_id(funding: Hash64, duty: Option<Hash64>, actor: PalwBondKeyV2, amount: u64, block: Hash64) -> Hash64 {
    identity_v2(b"MISAKA/PALW/SOURCE-VERIFIER-QUEUE/V2", &(funding, duty, actor, amount, block))
}
impl PalwSourceVerifierQueueV2 {
    pub fn id(&self) -> Hash64 {
        queue_receipt_id(self.funding, self.duty, self.actor, self.amount, self.queued_block)
    }
}
pub(crate) fn identity_v2<T: BorshSerialize>(domain: &[u8], value: &T) -> Hash64 {
    struct W(blake2b_simd::State);
    impl std::io::Write for W {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut w = W(blake2b_simd::Params::new().hash_length(64).key(domain).to_state());
    value.serialize(&mut w).expect("fixed native identity stream");
    Hash64::from_bytes(w.0.finalize().as_bytes().try_into().expect("64 bytes"))
}
pub(crate) fn key_v2(prefix: u8, id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = prefix;
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
pub(crate) fn actor_key_v2(prefix: u8, actor: &PalwBondKeyV2) -> [u8; 69] {
    let mut k = [0; 69];
    k[0] = prefix;
    borsh::to_writer(&mut &mut k[1..], actor).expect("fixed bond");
    k
}
pub(crate) fn liability_key_v2(actor: &PalwBondKeyV2, work: Hash64) -> [u8; 133] {
    let mut k = [0; 133];
    k[..69].copy_from_slice(&actor_key_v2(2, actor));
    k[69..].copy_from_slice(work.as_byte_slice());
    k
}
pub(crate) fn work_actor_key(work: Hash64, actor: &PalwBondKeyV2) -> [u8; 133] {
    let mut k = [0; 133];
    k[..65].copy_from_slice(&key_v2(6, work));
    borsh::to_writer(&mut &mut k[65..], actor).expect("fixed bond");
    k
}
pub(crate) fn pair_key(prefix: u8, left: Hash64, right: Hash64) -> [u8; 129] {
    let mut k = [0; 129];
    k[..65].copy_from_slice(&key_v2(prefix, left));
    k[65..].copy_from_slice(right.as_byte_slice());
    k
}
pub(crate) fn horizon_key(until: u64, work: Hash64) -> [u8; 73] {
    let mut k = [0; 73];
    k[0] = 5;
    k[1..9].copy_from_slice(&until.to_be_bytes());
    k[9..].copy_from_slice(work.as_byte_slice());
    k
}
pub(crate) fn decode_v2<T: NativeWireScanV1 + BorshDeserialize>(bytes: &[u8]) -> Result<NativeRetainedV1<T>, E<String>> {
    preflight_native_value_v1::<T>(bytes, bytes.len() as u64).map_err(|why| E::Decode(why.to_string()))?;
    retain_native_value_decode_classified_v1(bytes, || borsh::from_slice(bytes).map_err(|why| why.to_string()))
}
pub(crate) fn read_v2<T: NativeWireScanV1 + BorshDeserialize, const N: usize>(
    route: &PalwKernelRouteStateV1,
    key: &[u8; N],
) -> Result<Option<NativeRetainedV1<T>>, E<String>> {
    let Some(bytes) =
        crate::palw_material_challenge_checkpoint_v4::material_challenge_fixed_row_v4(route, SOURCE_VERIFIER_ECONOMICS_TABLE_V2, key)
            .map_err(E::Resource)?
    else {
        return Ok(None);
    };
    decode_v2(bytes).map(Some)
}
/// First matching ordered index row, with its exact key. No population scan and
/// no result collection; the key reservation lives through the range lookup.
pub(crate) fn first_v2<T: NativeWireScanV1 + BorshDeserialize, const N: usize>(
    route: &PalwKernelRouteStateV1,
    prefix: &[u8],
    after: Option<&[u8; N]>,
) -> Result<Option<([u8; N], NativeRetainedV1<T>)>, E<String>> {
    let _key = reserve_native_requests_v1(
        (N as u64).checked_mul(2).ok_or_else(|| E::Resource(NativeCoverageV1::overflow::<T>("source index key")))?,
    )
    .map_err(E::Resource)?;
    let mut seek = Vec::new();
    seek.try_reserve_exact(N).map_err(|why| E::Resource(NativeCoverageV1::unsupported::<T>("source index key", &why.to_string())))?;
    seek.extend_from_slice(after.map(|k| k.as_slice()).unwrap_or(prefix));
    let bound = if after.is_some() {
        std::ops::Bound::Excluded((SOURCE_VERIFIER_ECONOMICS_TABLE_V2, seek))
    } else {
        std::ops::Bound::Included((SOURCE_VERIFIER_ECONOMICS_TABLE_V2, seek))
    };
    let Some(((table, k), bytes)) = route.aux.range((bound, std::ops::Bound::Unbounded)).next() else { return Ok(None) };
    if *table != SOURCE_VERIFIER_ECONOMICS_TABLE_V2 || !k.starts_with(prefix) {
        return Ok(None);
    }
    let key = k.as_slice().try_into().map_err(|_| E::Decode("source index key extent".into()))?;
    Ok(Some((key, decode_v2(bytes)?)))
}
pub(crate) fn govern_v2(
    params: &PalwStateParamsV2,
    epoch: &PalwSourceBootstrapEpochRowV1,
    now: u64,
) -> Result<Option<PalwSourceVerifierEconomicsEpochV2>, E<String>> {
    let Some((at, policy)) = params.selected_source_economics_v2 else { return Ok(None) };
    policy.validate().map_err(E::Decode)?;
    if at > epoch.context.policy.freeze_cutoff_daa
        || policy.bootstrap_policy != epoch.context.policy.id()
        || policy.network != epoch.context.policy.network
        || policy.maximum_funded_works > epoch.context.policy.selection.maximum_source_works
    {
        return Err(E::Decode("source economics differs from frozen bootstrap contract".into()));
    }
    if now < at || epoch.context.anchor.is_none() {
        return Ok(None);
    }
    let id = epoch
        .context
        .epoch()
        .map_err(|why| E::Decode(why.into()))?
        .id()
        .map_err(|_| E::Decode("source economic epoch identity".into()))?;
    Ok(Some(PalwSourceVerifierEconomicsEpochV2 {
        version: 2,
        activation: at,
        policy,
        epoch: Hash64::from_bytes(id),
        until_daa: epoch.context.policy.stake_release_daa,
        funded_works: 0,
        liabilities: 0,
    }))
}
impl Params {
    pub fn sync_palw_source_verifier_economics_v2(&mut self) {
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &mut self.palw_consensus_mode {
            b.state.selected_source_economics_v2 = self
                .palw_source_verifier_economics_v2
                .filter(|f| f.activation != ForkActivation::never())
                .map(|f| (f.activation.daa_score(), f.policy));
        }
    }
    pub fn validate_palw_source_verifier_economics_v2(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error as M;
        let mirror = self
            .palw_source_verifier_economics_v2
            .filter(|f| f.activation != ForkActivation::never())
            .map(|f| (f.activation.daa_score(), f.policy));
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &self.palw_consensus_mode
            && b.state.selected_source_economics_v2 != mirror
        {
            return Err(M::Invalid("source economics mirror differs"));
        }
        if let Some(f) = self.palw_source_verifier_economics_v2 {
            f.policy.validate().map_err(M::InvalidOwned)?;
        }
        if mirror.is_some() {
            return Err(M::Invalid("source checker economics remains unapproved and dormant pending native/campaign acceptance"));
        }
        Ok(())
    }
}
impl PalwStateParamsV2 {
    pub fn selected_source_economics_v2_mirror_matches(&self, at: u64, p: &PalwSourceVerifierEconomicsPolicyV2) -> bool {
        self.selected_source_economics_v2.as_ref().is_some_and(|(a, b)| *a == at && b == p)
    }
}
impl PalwChainStateV2 {
    /// Legacy view-only interface. Native admission must use the checked
    /// sibling so local capacity does not become insufficient collateral.
    pub fn source_verifier_reserved_v2(&self, actor: &PalwBondKeyV2, now: u64) -> u128 {
        self.source_verifier_reserved_checked_v2(actor, now).unwrap_or(u128::MAX)
    }
    /// Fail closed through explicit release, INCLUDING sparse ticks after H,
    /// while retaining typed local resource failures for native consumers.
    pub fn source_verifier_reserved_checked_v2(&self, actor: &PalwBondKeyV2, _now: u64) -> Result<u128, E<String>> {
        let Some(route) = self.kernel_route() else { return Ok(0) };
        if route
            .aux
            .range((SOURCE_VERIFIER_ECONOMICS_TABLE_V2, Vec::new())..(SOURCE_VERIFIER_ECONOMICS_TABLE_V2 + 1, Vec::new()))
            .next()
            .is_none()
        {
            return Ok(0);
        }
        let hold = read_v2::<PalwSourceVerifierBondHoldV2, 69>(route, &actor_key_v2(3, actor))?;
        let Some(h) = hold else {
            if first_v2::<PalwSourceVerifierLiabilityV2, 133>(route, &actor_key_v2(2, actor), None)?.is_some() {
                return Err(E::Decode("source duty lacks aggregate hold".into()));
            }
            return Ok(0);
        };
        let g =
            read_v2::<PalwSourceVerifierEconomicsEpochV2, 1>(route, &[0])?.ok_or_else(|| E::Decode("hold lacks governor".into()))?;
        if g.version != 2
            || g.policy.validate().is_err()
            || g.funded_works > g.policy.maximum_funded_works
            || g.liabilities > g.policy.maximum_liabilities
            || h.version != 2
            || h.actor != *actor
            || h.policy != g.policy.id()
            || h.until_daa != g.until_daa
        {
            return Err(E::Decode("source hold substitutes contract".into()));
        }
        Ok(h.reserved)
    }
}
impl PalwChainStateV2 {
    pub fn source_verifier_funding_v2(
        &self,
        work: Hash64,
    ) -> Result<Option<NativeRetainedV1<PalwSourceVerifierFundingV2>>, E<String>> {
        let Some(route) = self.kernel_route() else { return Ok(None) };
        let Some(f) = read_v2::<PalwSourceVerifierFundingV2, 65>(route, &key_v2(1, work))? else { return Ok(None) };
        let g = self
            .source_verifier_economics_epoch_v2()?
            .ok_or_else(|| E::Decode("source funding lacks current native governor".into()))?;
        let source = self
            .source_bootstrap_work_v1(work)
            .map_err(state_fault_v2)?
            .ok_or_else(|| E::Decode("source funding names no native work".into()))?;
        let epoch = self
            .source_bootstrap_epoch_v1()
            .map_err(state_fault_v2)?
            .ok_or_else(|| E::Decode("source funding epoch absent".into()))?;
        let now = self.last_point().map_or(0, |p| p.daa_score);
        let bytes = (epoch.cohort.len() as u64)
            .checked_mul(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<[u8; 64], ()>>() as u64)
            .ok_or_else(|| E::Resource(NativeCoverageV1::overflow::<PalwSourceVerifierFundingV2>("source payee count")))?;
        let _workspace = reserve_native_requests_v1(bytes).map_err(E::Resource)?;
        let distinct: std::collections::BTreeSet<_> = epoch.cohort.iter().map(|seat| seat.bond).collect();
        if f.version != 2
            || f.terms.version != 2
            || f.terms.work != work
            || source.work.id() != work
            || f.terms.policy != g.policy.id()
            || f.epoch != g.epoch
            || f.until_daa != g.until_daa
            || f.terms.fee_per_checker_sompi < g.policy.minimum_fee_per_checker_sompi
            || f.funded_daa < source.work.accepted_daa
            || f.funded_daa > source.work.deadline_daa
            || f.funded_daa >= f.until_daa
            || f.funded_daa > now
            || usize::from(f.distinct_payees) != distinct.len()
            || !f.conserves()
            || f.prepaid
                != f.terms
                    .fee_per_checker_sompi
                    .checked_mul(u64::from(f.distinct_payees))
                    .ok_or_else(|| E::Decode("source prepaid overflow".into()))?
        {
            return Err(E::Decode("source funding substitutes its exact work/epoch/policy/clock/amount".into()));
        }
        Ok(Some(f))
    }
    pub fn source_verifier_economics_epoch_v2(
        &self,
    ) -> Result<Option<NativeRetainedV1<PalwSourceVerifierEconomicsEpochV2>>, E<String>> {
        let Some(route) = self.kernel_route() else { return Ok(None) };
        let Some(g) = read_v2::<PalwSourceVerifierEconomicsEpochV2, 1>(route, &[0])? else { return Ok(None) };
        let source = self
            .source_bootstrap_epoch_v1()
            .map_err(state_fault_v2)?
            .ok_or_else(|| E::Decode("source economic governor has no native epoch".into()))?;
        let id = source
            .context
            .epoch()
            .map_err(|why| E::Decode(why.into()))?
            .id()
            .map_err(|_| E::Decode("source epoch identity".into()))?;
        if g.version != 2
            || g.policy.validate().is_err()
            || g.policy.bootstrap_policy != source.context.policy.id()
            || g.policy.network != source.context.policy.network
            || g.epoch != Hash64::from_bytes(id)
            || g.until_daa != source.context.policy.stake_release_daa
            || g.activation > source.context.policy.freeze_cutoff_daa
            || g.funded_works > g.policy.maximum_funded_works
            || g.liabilities > g.policy.maximum_liabilities
        {
            return Err(E::Decode("source governor substitutes current frozen epoch/contract".into()));
        }
        Ok(Some(g))
    }
}
fn state_fault_v2(why: crate::palw_state_v2::PalwStateV2Error) -> E<String> {
    match why {
        crate::palw_state_v2::PalwStateV2Error::NativeResource(why) => E::Resource(why),
        why => E::Decode(why.to_string()),
    }
}

/// V1 approval bytes never acquire economic meaning retroactively. In a
/// governed epoch each actual signer must have its own exact funded duty.
pub(crate) fn validate_source_economic_provenance_v2(
    route: &PalwKernelRouteStateV1,
    epoch: &PalwSourceBootstrapEpochRowV1,
    work: &PalwSourceBootstrapWorkRowV1,
) -> Result<(), E<String>> {
    let Some(g) = read_v2::<PalwSourceVerifierEconomicsEpochV2, 1>(route, &[0])? else { return Ok(()) };
    let eid =
        epoch.context.epoch().map_err(|why| E::Decode(why.into()))?.id().map_err(|_| E::Decode("source epoch identity".into()))?;
    if g.policy.bootstrap_policy != epoch.context.policy.id()
        || g.policy.network != epoch.context.policy.network
        || g.epoch != Hash64::from_bytes(eid)
        || g.until_daa != epoch.context.policy.stake_release_daa
    {
        return Err(E::Decode("source economic governor substitutes native epoch".into()));
    }
    let Some(f) = read_v2::<PalwSourceVerifierFundingV2, 65>(route, &key_v2(1, work.work.id()))? else {
        if work.approvals.is_empty() {
            return Ok(());
        }
        return Err(E::Decode("governed source approvals lack prepaid funding".into()));
    };
    if f.version != 2
        || f.terms.version != 2
        || f.terms.work != work.work.id()
        || f.terms.policy != g.policy.id()
        || f.epoch != g.epoch
        || f.until_daa != g.until_daa
        || !f.conserves()
        || f.terms.fee_per_checker_sompi < g.policy.minimum_fee_per_checker_sompi
    {
        return Err(E::Decode("source funding identity/conservation".into()));
    }
    for a in &work.approvals {
        let d = read_v2::<PalwSourceVerifierLiabilityV2, 133>(route, &liability_key_v2(&a.signer, work.work.id()))?
            .ok_or_else(|| E::Decode("governed approval has no signed actor duty".into()))?;
        if d.actor != a.signer
            || d.work != work.work.id()
            || d.claim != work.work.claim
            || d.policy != g.policy.id()
            || d.funding != f.terms.id()
            || d.exposure != g.policy.exposure_per_work_sompi
            || d.fee != f.terms.fee_per_checker_sompi
            || d.until_daa != g.until_daa
            || d.accepted_daa > a.accepted_daa
        {
            return Err(E::Decode("source approval substitutes funded duty".into()));
        }
    }
    Ok(())
}
impl PalwChainStateV2 {
    /// One namespace pass, O(rows log rows) indexed joins. Workspace is reserved
    /// before constructing its three maps. This is restore/whole-state checking,
    /// not a per-action population scan or a physical RSS claim.
    pub fn assert_source_verifier_economics_consistency_v2(
        &self,
        params: &PalwStateParamsV2,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1;
        use crate::palw_state_v2::PalwStateV2Error as S;
        use std::collections::BTreeMap;
        let bad = |why: &str| S::CarriageInconsistent(format!("source checker economics: {why}"));
        let native = |why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, |why| bad(&why));
        let Some(route) = self.kernel_route() else { return Ok(()) };
        let range =
            || route.aux.range((SOURCE_VERIFIER_ECONOMICS_TABLE_V2, Vec::new())..(SOURCE_VERIFIER_ECONOMICS_TABLE_V2 + 1, Vec::new()));
        if range().next().is_none() {
            // Absence is only a pre-activation / pre-anchor state. Once the
            // native anchored epoch should have prepared its governor, deleting
            // the entire namespace must not turn its duties into a legacy epoch.
            let now = self.last_point().map_or(0, |p| p.daa_score);
            if params.selected_source_economics_v2.as_ref().is_some_and(|(at, _)| now >= *at)
                && let Some(epoch) = self.source_bootstrap_epoch_v1()?
                && govern_v2(params, &epoch, now).map_err(&native)?.is_some()
            {
                return Err(bad("active anchored epoch lacks economic governor"));
            }
            return Ok(());
        }
        let epoch = self.source_bootstrap_epoch_v1()?.ok_or_else(|| bad("namespace lacks native source epoch"))?;
        let now = self.last_point().map_or(0, |p| p.daa_score);
        let expected =
            govern_v2(params, &epoch, now).map_err(&native)?.ok_or_else(|| bad("namespace outside explicit private contract"))?;
        let g = read_v2::<PalwSourceVerifierEconomicsEpochV2, 1>(route, &[0])
            .map_err(&native)?
            .ok_or_else(|| bad("namespace lacks governor"))?;
        if !g.same_contract(&expected)
            || g.funded_works > g.policy.maximum_funded_works
            || g.liabilities > g.policy.maximum_liabilities
        {
            return Err(bad("governor identity/counters"));
        }
        let f = u64::from(g.funded_works);
        let l = u64::from(g.liabilities);
        let limit = 1u64
            .checked_add(f.checked_mul(3).ok_or_else(|| bad("row count overflow"))?)
            .and_then(|n| n.checked_add(l.checked_mul(6)?))
            .ok_or_else(|| bad("row count overflow"))?;
        if range().take(usize::try_from(limit.saturating_add(1)).map_err(|_| bad("row count platform"))?).count() as u64 > limit {
            return Err(bad("namespace exceeds actual counter envelope"));
        }
        let bytes = f
            .checked_mul(std::mem::size_of::<BTreeNodeEnvelopeV1<Hash64, (u64, u64, u64)>>() as u64)
            .and_then(|n| n.checked_add(l.checked_mul(std::mem::size_of::<BTreeNodeEnvelopeV1<PalwBondKeyV2, u128>>() as u64)?))
            .and_then(|n| {
                n.checked_add(
                    (f.checked_add(l)?).checked_mul(std::mem::size_of::<BTreeNodeEnvelopeV1<Hash64, (Hash64, u64)>>() as u64)?,
                )
            })
            .ok_or_else(|| {
                S::NativeResource(NativeCoverageV1::overflow::<PalwSourceVerifierEconomicsEpochV2>("source restore workspace"))
            })?;
        let _workspace = reserve_native_requests_v1(bytes).map_err(S::NativeResource)?;
        let mut totals: BTreeMap<Hash64, (u64, u64, u64)> = BTreeMap::new();
        let mut holds: BTreeMap<PalwBondKeyV2, u128> = BTreeMap::new();
        let mut queues: BTreeMap<Hash64, (Hash64, u64)> = BTreeMap::new();
        let mut funding_count = 0u32;
        let mut liability_count = 0u32;
        for ((_, k), raw) in range() {
            match (k.first().copied(), k.len()) {
                (Some(0), 1) => {}
                (Some(1), 65) => {
                    funding_count = funding_count.checked_add(1).ok_or_else(|| bad("funding counter overflow"))?;
                    let work = Hash64::from_bytes(k[1..].try_into().expect("64 bytes"));
                    // The same checked authority used by private workers also
                    // verifies distinct cohort payees and original funding clocks.
                    let row = self
                        .source_verifier_funding_v2(work)
                        .map_err(&native)?
                        .ok_or_else(|| bad("funding vanished during restore"))?;
                    let source = self.source_bootstrap_work_v1(work)?.ok_or_else(|| bad("funding has no source work"))?;
                    validate_source_economic_provenance_v2(route, &epoch, &source).map_err(&native)?;
                    if row.terms.work != work
                        || row.epoch != g.epoch
                        || row.until_daa != g.until_daa
                        || row.funded_daa > now
                        || row.distinct_payees == 0
                        || row.prepaid
                            != row
                                .terms
                                .fee_per_checker_sompi
                                .checked_mul(u64::from(row.distinct_payees))
                                .ok_or_else(|| bad("funding prepaid overflow"))?
                        || !row.conserves()
                    {
                        return Err(bad("funding count/clock/conservation"));
                    }
                    let horizon = read_v2::<u8, 73>(route, &horizon_key(row.until_daa, work)).map_err(&native)?;
                    if row.closed {
                        if row.held != 0 || horizon.is_some() {
                            return Err(bad("closed funding still held/indexed"));
                        }
                    } else if horizon.as_deref() != Some(&1) {
                        return Err(bad("open funding lacks horizon index"));
                    }
                    if row.refunded == 0 {
                        if row.refund_queue.is_some()
                            || row.refund_queued_block.is_some()
                            || row.refund_acknowledged_block.is_some()
                            || row.refund_acknowledged_daa.is_some()
                        {
                            return Err(bad("zero principal has refund receipt"));
                        }
                    } else if !row.closed
                        || row.refund_queue.is_none()
                        || row.refund_queued_block.is_none()
                        || row.refund_acknowledged_block.is_some() != row.refund_acknowledged_daa.is_some()
                    {
                        return Err(bad("refund receipt framing"));
                    }
                    if row.refunded > 0 && row.refund_acknowledged_block.is_none() {
                        let queue = row.refund_queue.ok_or_else(|| bad("pending refund queue absent"))?;
                        let block = row.refund_queued_block.ok_or_else(|| bad("pending refund block absent"))?;
                        let id = queue_receipt_id(row.terms.id(), None, row.terms.sponsor, row.refunded, block);
                        let q = read_v2::<PalwSourceVerifierQueueV2, 129>(route, &pair_key(8, queue, id))
                            .map_err(&native)?
                            .ok_or_else(|| bad("pending refund lacks exact queue receipt"))?;
                        if q.work != work || q.duty.is_some() || q.actor != row.terms.sponsor || q.amount != row.refunded {
                            return Err(bad("principal refund receipt substituted"));
                        }
                    }
                    if let Some(at) = row.refund_acknowledged_daa
                        && (at <= row.until_daa || at > now || row.refund_acknowledged_block == row.refund_queued_block)
                    {
                        return Err(bad("refund ACK clock"));
                    }
                }
                (Some(2), 133) => {
                    liability_count = liability_count.checked_add(1).ok_or_else(|| bad("duty counter overflow"))?;
                    let d = decode_v2::<PalwSourceVerifierLiabilityV2>(raw).map_err(&native)?;
                    if k.as_slice() != liability_key_v2(&d.actor, d.work)
                        || d.version != 2
                        || d.policy != g.policy.id()
                        || d.exposure != g.policy.exposure_per_work_sompi
                        || d.until_daa != g.until_daa
                        || d.accepted_daa > now
                    {
                        return Err(bad("duty identity/clock"));
                    }
                    let source = self.source_bootstrap_work_v1(d.work)?.ok_or_else(|| bad("duty source absent"))?;
                    let fund = read_v2::<PalwSourceVerifierFundingV2, 65>(route, &key_v2(1, d.work))
                        .map_err(&native)?
                        .ok_or_else(|| bad("duty funding absent"))?;
                    if d.claim != source.work.claim
                        || d.funding != fund.terms.id()
                        || d.fee != fund.terms.fee_per_checker_sompi
                        || d.accepted_daa < fund.funded_daa
                        || !source.approvals.iter().any(|a| a.signer == d.actor && a.accepted_daa == d.accepted_daa)
                    {
                        return Err(bad("duty substitutes original funded approval"));
                    }
                    if read_v2::<Hash64, 133>(route, &work_actor_key(d.work, &d.actor)).map_err(&native)?.as_deref() != Some(&d.id())
                        || read_v2::<PalwSourceVerifierDutyRefV2, 129>(route, &pair_key(7, d.claim, d.id()))
                            .map_err(&native)?
                            .as_deref()
                            != Some(&PalwSourceVerifierDutyRefV2 { actor: d.actor, work: d.work })
                    {
                        return Err(bad("duty index missing/substituted"));
                    }
                    if read_v2::<PalwSourceVerifierBondHoldV2, 69>(route, &actor_key_v2(3, &d.actor)).map_err(&native)?.is_none() {
                        return Err(bad("duty aggregate missing"));
                    }
                    let total = totals.entry(d.work).or_default();
                    match d.status {
                        PalwSourceVerifierDutyStatusV2::Active => {
                            if !matches!(d.entitlement, PalwSourceVerifierEntitlementV2::Held) {
                                return Err(bad("active duty has terminal entitlement"));
                            }
                            let held = holds.entry(d.actor).or_default();
                            *held = held.checked_add(u128::from(d.exposure)).ok_or_else(|| bad("actor exposure sum overflow"))?;
                        }
                        PalwSourceVerifierDutyStatusV2::Released { daa, .. } => {
                            if daa <= d.until_daa || daa > now {
                                return Err(bad("release before inclusive horizon"));
                            }
                        }
                        PalwSourceVerifierDutyStatusV2::Convicted { block, daa, collected } => {
                            if daa < d.accepted_daa
                                || daa > d.until_daa
                                || daa > now
                                || collected > d.exposure
                                || !matches!(d.entitlement, PalwSourceVerifierEntitlementV2::Forfeited { block: forfeited } if forfeited == block)
                            {
                                return Err(bad("conviction clock/entitlement"));
                            }
                            let c = read_v2::<PalwSourceVerifierCourtOutcomeV2, 65>(route, &key_v2(4, d.id()))
                                .map_err(&native)?
                                .ok_or_else(|| bad("conviction lacks court receipt"))?;
                            if c.block != block || c.daa != daa || c.collected != collected {
                                return Err(bad("conviction receipt differs"));
                            }
                            total.1 = total.1.checked_add(d.fee).ok_or_else(|| bad("forfeited sum overflow"))?;
                        }
                    }
                    match d.entitlement {
                        PalwSourceVerifierEntitlementV2::Held => {
                            total.2 = total.2.checked_add(d.fee).ok_or_else(|| bad("held sum overflow"))?
                        }
                        PalwSourceVerifierEntitlementV2::Queued { block, queue } => {
                            let receipt = queue_receipt_id(d.funding, Some(d.id()), d.actor, d.fee, block);
                            let q = read_v2::<PalwSourceVerifierQueueV2, 129>(route, &pair_key(8, queue, receipt))
                                .map_err(&native)?
                                .ok_or_else(|| bad("queued duty lacks its exact receipt index"))?;
                            if q.work != d.work
                                || q.actor != d.actor
                                || q.duty != Some(d.id())
                                || q.amount != d.fee
                                || q.queued_block != block
                            {
                                return Err(bad("queued duty receipt substituted"));
                            }
                            total.0 = total.0.checked_add(d.fee).ok_or_else(|| bad("queued sum overflow"))?;
                        }
                        PalwSourceVerifierEntitlementV2::QueueAcknowledged {
                            queued_block,
                            acknowledged_block,
                            acknowledged_daa,
                            amount,
                            ..
                        } => {
                            if queued_block == acknowledged_block
                                || acknowledged_daa <= d.until_daa
                                || acknowledged_daa > now
                                || amount != d.fee
                            {
                                return Err(bad("fee ACK identity/clock/amount"));
                            }
                            total.0 = total.0.checked_add(d.fee).ok_or_else(|| bad("paid sum overflow"))?;
                        }
                        PalwSourceVerifierEntitlementV2::Forfeited { .. } => {}
                    }
                }
                (Some(3), 69) => {
                    let h = decode_v2::<PalwSourceVerifierBondHoldV2>(raw).map_err(&native)?;
                    if k.as_slice() != actor_key_v2(3, &h.actor)
                        || h.version != 2
                        || h.policy != g.policy.id()
                        || h.until_daa != g.until_daa
                    {
                        return Err(bad("actor aggregate identity"));
                    }
                }
                (Some(4), 65) => {
                    let c = decode_v2::<PalwSourceVerifierCourtOutcomeV2>(raw).map_err(&native)?;
                    let d = read_v2::<PalwSourceVerifierLiabilityV2, 133>(route, &liability_key_v2(&c.actor, c.work))
                        .map_err(&native)?
                        .ok_or_else(|| bad("orphan court outcome"))?;
                    if c.version != 2
                        || c.policy != g.policy.id()
                        || c.claim != d.claim
                        || c.exposure != d.exposure
                        || k.as_slice() != key_v2(4, d.id())
                        || !matches!(d.status, PalwSourceVerifierDutyStatusV2::Convicted { .. })
                    {
                        return Err(bad("court outcome substitutes duty"));
                    }
                }
                (Some(5), 73) => {
                    if *decode_v2::<u8>(raw).map_err(&native)? != 1 {
                        return Err(bad("horizon marker"));
                    }
                    let work = Hash64::from_bytes(k[9..].try_into().expect("64 bytes"));
                    let f = read_v2::<PalwSourceVerifierFundingV2, 65>(route, &key_v2(1, work))
                        .map_err(&native)?
                        .ok_or_else(|| bad("orphan horizon"))?;
                    if f.closed || k.as_slice() != horizon_key(f.until_daa, work) {
                        return Err(bad("horizon identity"));
                    }
                }
                (Some(6), 133) => {
                    let actor = borsh::from_slice(&k[65..]).map_err(|_| bad("work index actor"))?;
                    let work = Hash64::from_bytes(k[1..65].try_into().expect("64 bytes"));
                    let d = read_v2::<PalwSourceVerifierLiabilityV2, 133>(route, &liability_key_v2(&actor, work))
                        .map_err(&native)?
                        .ok_or_else(|| bad("orphan work index"))?;
                    if *decode_v2::<Hash64>(raw).map_err(&native)? != d.id() {
                        return Err(bad("work index identity"));
                    }
                }
                (Some(7), 129) => {
                    let r = decode_v2::<PalwSourceVerifierDutyRefV2>(raw).map_err(&native)?;
                    let d = read_v2::<PalwSourceVerifierLiabilityV2, 133>(route, &liability_key_v2(&r.actor, r.work))
                        .map_err(&native)?
                        .ok_or_else(|| bad("orphan claim index"))?;
                    if k.as_slice() != pair_key(7, d.claim, d.id()) {
                        return Err(bad("claim index identity"));
                    }
                }
                (Some(8), 129) => {
                    let q = decode_v2::<PalwSourceVerifierQueueV2>(raw).map_err(&native)?;
                    if q.version != 2
                        || q.amount == 0
                        || q.queued_daa > now
                        || k.as_slice() != pair_key(8, q.queue, q.id())
                        || q.queue != crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&q.payload)
                    {
                        return Err(bad("queue identity/clock/amount"));
                    }
                    let fund = read_v2::<PalwSourceVerifierFundingV2, 65>(route, &key_v2(1, q.work))
                        .map_err(&native)?
                        .ok_or_else(|| bad("orphan queue funding"))?;
                    if fund.terms.id() != q.funding || q.queued_daa <= fund.until_daa {
                        return Err(bad("queue funding/horizon substituted"));
                    }
                    if let Some(id) = q.duty {
                        let d = read_v2::<PalwSourceVerifierLiabilityV2, 133>(route, &liability_key_v2(&q.actor, q.work))
                            .map_err(&native)?
                            .ok_or_else(|| bad("orphan fee queue"))?;
                        if d.id() != id
                            || d.funding != q.funding
                            || d.fee != q.amount
                            || !matches!(d.entitlement,PalwSourceVerifierEntitlementV2::Queued{block,queue}if block==q.queued_block&&queue==q.queue)
                        {
                            return Err(bad("queue differs from original duty"));
                        }
                    } else if !fund.closed
                        || fund.terms.sponsor != q.actor
                        || fund.refunded != q.amount
                        || fund.refund_queue != Some(q.queue)
                        || fund.refund_queued_block != Some(q.queued_block)
                        || fund.refund_acknowledged_block.is_some()
                    {
                        return Err(bad("queue differs from principal refund"));
                    }
                    let total = queues.entry(q.queue).or_insert((q.payload, 0));
                    if total.0 != q.payload {
                        return Err(bad("mixed queue payload"));
                    }
                    total.1 = total.1.checked_add(q.amount).ok_or_else(|| bad("queue amount overflow"))?;
                }
                _ => return Err(bad("unknown aux146 key family/extent")),
            }
        }
        if funding_count != g.funded_works || liability_count != g.liabilities {
            return Err(bad("counter differs from actual rows"));
        }
        // Second linear pass consumes pre-reserved aggregate maps; no actor ×
        // liability nested walk, and no per-action call to this whole-state check.
        for ((_, k), raw) in range() {
            match k[0] {
                1 => {
                    let f = decode_v2::<PalwSourceVerifierFundingV2>(raw).map_err(&native)?;
                    let t = totals.get(&f.terms.work).copied().unwrap_or_default();
                    if f.queued != t.0 || f.forfeited != t.1 || f.held < t.2 {
                        return Err(bad("funding differs from duty totals"));
                    }
                }
                3 => {
                    let h = decode_v2::<PalwSourceVerifierBondHoldV2>(raw).map_err(&native)?;
                    if h.reserved != holds.get(&h.actor).copied().unwrap_or(0) {
                        return Err(bad("hold differs from active duties"));
                    }
                }
                _ => {}
            }
        }
        for (key, payout) in self.pending_payouts_iter() {
            if let Some((payload, amount)) = queues.remove(key)
                && (payout.payload != payload || payout.amount < amount)
            {
                return Err(bad("source queue exceeds native payout"));
            }
        }
        if !queues.is_empty() {
            return Err(bad("source queue absent from native payout"));
        }
        Ok(())
    }
}

impl PalwChainStateV2 {
    /// Restore-only rederivation into the shared primary exposure accumulator.
    /// Keep the returned ticket alive until the caller drops that accumulator.
    /// Native admission uses the already journaled primary ledger, not this scan.
    pub fn add_source_verifier_exposures_v2(
        &self,
        exposure: &mut std::collections::BTreeMap<PalwBondKeyV2, u128>,
    ) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1;
        use crate::palw_state_v2::PalwStateV2Error as S;
        let Some(route) = self.kernel_route() else { return Ok(None) };
        let rows = || {
            route
                .aux
                .range((SOURCE_VERIFIER_ECONOMICS_TABLE_V2, Vec::new())..(SOURCE_VERIFIER_ECONOMICS_TABLE_V2 + 1, Vec::new()))
                .filter(|((_, key), _)| key.first() == Some(&3))
        };
        let count = rows().count() as u64;
        if count == 0 {
            return Ok(None);
        }
        let bytes = count.checked_mul(std::mem::size_of::<BTreeNodeEnvelopeV1<PalwBondKeyV2, u128>>() as u64).ok_or_else(|| {
            S::NativeResource(NativeCoverageV1::overflow::<PalwSourceVerifierBondHoldV2>("source exposure restore nodes"))
        })?;
        let lease = reserve_native_requests_v1(bytes).map_err(S::NativeResource)?;
        let g = read_v2::<PalwSourceVerifierEconomicsEpochV2, 1>(route, &[0])
            .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, S::CarriageInconsistent))?
            .ok_or_else(|| S::CarriageInconsistent("source exposure has no governor".into()))?;
        if g.version != 2
            || g.policy.validate().is_err()
            || g.funded_works > g.policy.maximum_funded_works
            || g.liabilities > g.policy.maximum_liabilities
        {
            return Err(S::CarriageInconsistent("source exposure governor invalid".into()));
        }
        for ((_, key), bytes) in rows() {
            let h = decode_v2::<PalwSourceVerifierBondHoldV2>(bytes)
                .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, S::CarriageInconsistent))?;
            if key.as_slice() != actor_key_v2(3, &h.actor) || h.version != 2 || h.policy != g.policy.id() || h.until_daa != g.until_daa
            {
                return Err(S::CarriageInconsistent("source exposure aggregate identity".into()));
            }
            if h.reserved > 0 {
                let total = exposure.entry(h.actor).or_default();
                *total = total.checked_add(h.reserved).ok_or(S::Overflow("source exposure restore sum"))?;
            }
        }
        Ok(lease)
    }
}
