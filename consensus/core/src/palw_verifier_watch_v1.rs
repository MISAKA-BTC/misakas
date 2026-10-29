//! Explicitly injected M*-49 watcher policy. No network preset or economic value
//! is chosen here. All windows are finite DAA intervals under chain progress.
use crate::Hash64;
use borsh::{BorshDeserialize, BorshSerialize};
pub use misaka_palw_kernel::verifier_watch_v1::VerifierWatchTermsV1;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwVerifierWatchPolicyV1 {
    pub epoch_length_daa: u64,
    pub check_probability_ppm: u32,
    pub slots: u8,
    pub maximum_registration_epochs: u64,
    pub maximum_claim_horizon_daa: u64,
    pub beacon: misaka_palw_challenge::PostCommitChallengePolicyV1,
}
impl PalwVerifierWatchPolicyV1 {
    pub fn id(&self) -> [u8; 64] {
        misaka_palw_kernel::hash::object_id(b"misaka-palw/verifier-watch-policy/v1", self)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.epoch_length_daa == 0
            || self.check_probability_ppm == 0
            || self.check_probability_ppm > 1_000_000
            || self.slots == 0
            || self.maximum_registration_epochs == 0
            || self.maximum_claim_horizon_daa == 0
            || self.maximum_claim_horizon_daa == u64::MAX
        {
            return Err("watcher policy requires explicit finite epoch, draw and lifetime bounds");
        }
        self.beacon.validate().map_err(|_| "invalid watcher sealed-source policy")?;
        if !self.beacon.is_sealed_source() || self.beacon.anchor_delay_slots == 0 {
            return Err("watcher requires a post-commit sealed-v3 beacon");
        }
        let span = self
            .beacon
            .beacon_window_slots
            .checked_mul(2)
            .and_then(|n| n.checked_add(self.beacon.anchor_delay_slots))
            .and_then(|n| n.checked_add(self.beacon.settlement_depth_d))
            .ok_or("watcher beacon horizon overflow")?;
        if span >= self.maximum_claim_horizon_daa {
            return Err("watcher beacon leaves no finite claim response window");
        }
        self.maximum_registration_epochs
            .checked_mul(self.epoch_length_daa)
            .and_then(|n| n.checked_add(self.maximum_claim_horizon_daa))
            .ok_or("watcher registration horizon overflow")?;
        Ok(())
    }
    pub fn validate_terms(&self, terms: &VerifierWatchTermsV1, now: u64) -> Result<(), &'static str> {
        self.validate()?;
        terms.validate()?;
        if terms.policy != self.id() || terms.first_epoch <= now / self.epoch_length_daa {
            return Err("watcher entry must precede its first frozen epoch");
        }
        let epochs =
            terms.last_epoch.checked_sub(terms.first_epoch).and_then(|n| n.checked_add(1)).ok_or("watcher epoch interval overflow")?;
        if epochs > self.maximum_registration_epochs {
            return Err("watcher registration exceeds finite epoch limit");
        }
        let end = terms
            .last_epoch
            .checked_add(1)
            .and_then(|n| n.checked_mul(self.epoch_length_daa))
            .and_then(|n| n.checked_add(self.maximum_claim_horizon_daa))
            .ok_or("watcher lock horizon overflow")?;
        if terms.lock_until_daa != end {
            return Err("watcher stake must cover its last epoch and full claim liability window");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwVerifierWatchRowV1 {
    pub terms: VerifierWatchTermsV1,
    pub accepted_daa: u64,
    pub reservation: Hash64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwVerifierWatchFenceV1 {
    pub activation: crate::config::params::ForkActivation,
    pub policy: PalwVerifierWatchPolicyV1,
}
impl crate::config::params::Params {
    pub fn sync_palw_verifier_watch_v1(&mut self) {
        let maturity = (self.palw_bond_maturity, self.palw_bond_maturity_early_fence());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state.verifier_watch_v1 = self
                .palw_verifier_watch_v1
                .as_ref()
                .filter(|f| f.activation != crate::config::params::ForkActivation::never())
                .map(|f| (f.activation.daa_score(), f.policy.clone()));
            bundle.state.verifier_watch_maturity_v1 = bundle.state.verifier_watch_v1.is_some().then_some(maturity);
        }
    }
    pub fn validate_palw_verifier_watch_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error as E};
        let expected = self
            .palw_verifier_watch_v1
            .as_ref()
            .filter(|f| f.activation != crate::config::params::ForkActivation::never())
            .map(|f| (f.activation.daa_score(), f.policy.clone()));
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            if bundle.state.verifier_watch_maturity_v1
                != expected.as_ref().map(|_| (self.palw_bond_maturity, self.palw_bond_maturity_early_fence()))
            {
                return Err(E::Invalid("watcher native maturity mirror differs"));
            }
            if bundle.state.verifier_watch_v1 != expected {
                return Err(E::Invalid("watcher policy mirror differs"));
            }
        }
        if let Some((at, policy)) = expected {
            policy.validate().map_err(E::Invalid)?;
            if self.palw_verifier_pay_v1.is_none_or(|f| f.daa_score() > at)
                || self.palw_settlement_once_v1.is_none_or(|f| f.daa_score() > at)
            {
                return Err(E::Invalid("watcher pool requires verifier pay and settlement fences at or before its activation"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> PalwVerifierWatchPolicyV1 {
        PalwVerifierWatchPolicyV1 {
            epoch_length_daa: 100,
            check_probability_ppm: 500_000,
            slots: 4,
            maximum_registration_epochs: 2,
            maximum_claim_horizon_daa: 100,
            beacon: misaka_palw_challenge::policy::sealed_source_policy_v3(1, 1, 10, 1, 1),
        }
    }
    #[test]
    fn registration_is_prospective_finite_and_covers_last_claim() {
        let p = policy();
        let mut t = VerifierWatchTermsV1 {
            version: 1,
            network: [1; 64],
            ruleset: [2; 64],
            class: [3; 64],
            watcher: [4; 64],
            stake: 17,
            first_epoch: 2,
            last_epoch: 3,
            lock_until_daa: 500,
            policy: p.id(),
            nonce: 0,
        };
        assert!(p.validate_terms(&t, 199).is_ok());
        assert!(p.validate_terms(&t, 200).is_err());
        t.lock_until_daa = 499;
        assert!(p.validate_terms(&t, 199).is_err());
        t.lock_until_daa = 500;
        t.last_epoch = 4;
        assert!(p.validate_terms(&t, 199).is_err());
    }
    #[test]
    fn no_fallback_randomness_or_zero_policy() {
        let mut p = policy();
        p.beacon = misaka_palw_challenge::policy::reference_policy_v1(1, 1, 10, 1, 1);
        assert!(p.validate().is_err());
        let mut p = policy();
        p.check_probability_ppm = 0;
        assert!(p.validate().is_err());
        let mut p = policy();
        p.maximum_claim_horizon_daa = 20;
        assert!(p.validate().is_err());
    }
}

pub const TABLE: u8 = 142;
pub const WATCH: u8 = 7;
pub const EXPIRY: u8 = 8;
pub const POOL: u8 = 9;
pub const CONTEXT: u8 = 10;
pub fn key(prefix: u8, id: &[u8; 64]) -> [u8; 65] {
    let mut out = [0; 65];
    out[0] = prefix;
    out[1..].copy_from_slice(id);
    out
}

impl crate::palw_state_v2::PalwChainStateV2 {
    pub fn verifier_watch_reserved_at_checked_v1(
        &self,
        bond: &crate::palw_state_v2::PalwBondKeyV2,
        _now: u64,
    ) -> Result<u128, crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_state_v2::PalwStateV2Error as E;
        let Some(route) = self.kernel_route() else { return Ok(0) };
        let actor = crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(bond);
        let mut sum = 0u128;
        for ((table, k), bytes) in &route.aux {
            if *table != TABLE || k.len() != 65 || k[0] != WATCH {
                continue;
            }
            // Fixed-width row: no attacker-sized nested allocations are decoded.
            let row: PalwVerifierWatchRowV1 =
                borsh::from_slice(bytes).map_err(|_| E::CarriageInconsistent("malformed watcher reservation".into()))?;
            row.terms.validate().map_err(|e| E::CarriageInconsistent(e.into()))?;
            if k[1..] != row.terms.id() {
                return Err(E::CarriageInconsistent("watcher reservation key mismatch".into()));
            }
            // Expiry is an explicit journalled release, not a view-clock discount.
            if row.terms.watcher == actor {
                sum = sum.checked_add(u128::from(row.terms.stake)).ok_or(E::Overflow("watcher stake"))?;
            }
        }
        Ok(sum)
    }
    pub fn verifier_watch_reserved_at_v1(&self, bond: &crate::palw_state_v2::PalwBondKeyV2, now: u64) -> u128 {
        self.verifier_watch_reserved_at_checked_v1(bond, now).unwrap_or(u128::MAX)
    }
}

impl crate::palw_state_v2::PalwChainStateV2 {
    pub(crate) fn assert_verifier_watch_consistency_v1(
        &self,
        params: &crate::palw_state_v2::PalwStateParamsV2,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_state_v2::PalwStateV2Error as E;
        let bad = |s: &str| E::CarriageInconsistent(s.into());
        let Some(route) = self.kernel_route() else {
            return Ok(());
        };
        for ((table, k), bytes) in &route.aux {
            if *table != TABLE || !matches!(k.first(), Some(&WATCH) | Some(&EXPIRY)) {
                continue;
            }
            let (at, policy) = params.verifier_watch_v1.as_ref().ok_or_else(|| bad("watcher rows without injected policy"))?;
            if k[0] == WATCH {
                if k.len() != 65 {
                    return Err(bad("watcher row key shape"));
                }
                let row: PalwVerifierWatchRowV1 = borsh::from_slice(bytes).map_err(|_| bad("watcher row bytes"))?;
                policy.validate_terms(&row.terms, row.accepted_daa).map_err(bad)?;
                let id = row.terms.id();
                if row.accepted_daa < *at
                    || k[1..] != id
                    || row.reservation != Hash64::from_bytes(id)
                    || row.terms.network != route.header.policy.network_domain
                    || row.terms.ruleset != route.header.policy.ruleset_digest
                {
                    return Err(bad("watcher row identity/domain/cohort mismatch"));
                }
                let bond = route.bond_key_of(&row.terms.watcher).ok_or_else(|| bad("watcher native actor absent"))?;
                let held = self
                    .bond_budget()
                    .and_then(|b| b.claim_row(&row.reservation))
                    .ok_or_else(|| bad("watcher principal reservation absent"))?;
                if !held.live
                    || !held.open
                    || held.bond != bond
                    || held.accepted_daa != row.accepted_daa
                    || held.liability_sompi != u128::from(row.terms.stake)
                    || held.reserved.reward_sompi != 0
                    || held.reserved.final_weight != 0
                {
                    return Err(bad("watcher principal differs from its native budget reservation"));
                }
                let mut expiry = [0u8; 73];
                expiry[0] = EXPIRY;
                expiry[1..9].copy_from_slice(&row.terms.lock_until_daa.to_be_bytes());
                expiry[9..].copy_from_slice(&id);
                if route.aux.get(&(TABLE, expiry.to_vec())).map(Vec::as_slice) != Some(&[1u8][..]) {
                    return Err(bad("watcher reciprocal finite expiry absent"));
                }
            } else {
                if k.len() != 73 || bytes.as_slice() != [1u8] {
                    return Err(bad("watcher expiry shape"));
                }
                let id: [u8; 64] = k[9..].try_into().unwrap();
                let watch = route.aux.get(&(TABLE, key(WATCH, &id).to_vec())).ok_or_else(|| bad("orphan watcher expiry"))?;
                let row: PalwVerifierWatchRowV1 = borsh::from_slice(watch).map_err(|_| bad("watcher expiry owner bytes"))?;
                if row.terms.id() != id || row.terms.lock_until_daa.to_be_bytes() != k[1..9] {
                    return Err(bad("watcher expiry owner mismatch"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwFrozenWatcherV1 {
    pub watcher: [u8; 64],
    pub operator: Hash64,
    pub registered_daa: u64,
    pub stake: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwFrozenWatcherPoolV1 {
    pub class: [u8; 64],
    pub epoch: u64,
    pub frozen_daa: u64,
    pub maturity_window: u64,
    pub policy: [u8; 64],
    pub members: Vec<PalwFrozenWatcherV1>,
}
impl PalwFrozenWatcherPoolV1 {
    pub fn id(&self) -> [u8; 64] {
        pool_id(self.class, self.epoch, self.policy)
    }
}
pub fn pool_id(class: [u8; 64], epoch: u64, policy: [u8; 64]) -> [u8; 64] {
    misaka_palw_kernel::hash::object_id(b"misaka-palw/verifier-watch-pool/v1", &(class, epoch, policy))
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwWatcherClaimContextV1 {
    pub claim: [u8; 64],
    pub pool: PalwFrozenWatcherPoolV1,
    pub excluded_operator: Hash64,
    pub beacon: misaka_palw_challenge::BeaconContextV1,
    pub hard_deadline: u64,
    /// Fixed by the first locked beacon; capacity refusal never redraws.
    pub selected: Option<Vec<[u8; 64]>>,
    pub complete: bool,
}
/// Domain-separated draw with replacement over the immutable epoch stake. This
/// is probabilistic selection, not a claim correctness certificate.
pub fn draw_watchers_v1(
    policy: &PalwVerifierWatchPolicyV1,
    seed: &[u8; 64],
    context: &PalwWatcherClaimContextV1,
) -> Result<Vec<[u8; 64]>, &'static str> {
    policy.validate()?;
    let hash = |kind: u8, slot: u8| {
        misaka_palw_kernel::hash::object_id(
            b"misaka-palw/verifier-watch-draw/v1",
            &(seed, context.claim, context.pool.id(), kind, slot),
        )
    };
    let check = u128::from_le_bytes(hash(0, 0)[..16].try_into().unwrap()) % 1_000_000;
    if check >= u128::from(policy.check_probability_ppm) {
        return Ok(Vec::new());
    }
    let mut total = 0u128;
    let mut previous = None;
    for m in &context.pool.members {
        if m.stake == 0 || previous.is_some_and(|p| p >= m.watcher) {
            return Err("noncanonical frozen watcher pool");
        }
        previous = Some(m.watcher);
        if m.operator != context.excluded_operator {
            total = total.checked_add(u128::from(m.stake)).ok_or("watcher total overflow")?;
        }
    }
    if total == 0 {
        return Ok(Vec::new());
    }
    let mut slots = Vec::with_capacity(usize::from(policy.slots));
    for slot in 0..policy.slots {
        let mut at = u128::from_le_bytes(hash(1, slot)[..16].try_into().unwrap()) % total;
        for m in context.pool.members.iter().filter(|m| m.operator != context.excluded_operator) {
            if at < u128::from(m.stake) {
                slots.push(m.watcher);
                break;
            }
            at -= u128::from(m.stake);
        }
    }
    Ok(slots)
}

fn decode_watch_row<T: borsh::BorshDeserialize + borsh::BorshSerialize + crate::palw_native_resource_v1::NativeWireScanV1>(bytes:&[u8])
    ->Result<crate::palw_native_resource_v1::NativeRetainedV1<T>,crate::palw_state_v2::PalwStateV2Error>{
    crate::palw_claim_local_v1::decode_claim_local_value_v1(bytes).map_err(|e|match e{
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e)=>crate::palw_state_v2::PalwStateV2Error::NativeResource(e),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e)=>crate::palw_state_v2::PalwStateV2Error::CarriageInconsistent(e.to_string()),
    })
}
impl PalwFrozenWatcherPoolV1 {
    fn validate(&self,policy:&PalwVerifierWatchPolicyV1,from:u64)->Result<(),&'static str>{
        policy.validate()?;
        if self.policy!=policy.id()||self.frozen_daa<from||self.frozen_daa/policy.epoch_length_daa!=self.epoch{
            return Err("frozen watcher pool policy/epoch mismatch");
        }
        let mut previous=None;
        for m in &self.members {
            if m.stake==0||previous.is_some_and(|old|old>=m.watcher)
                ||m.registered_daa.checked_add(self.maturity_window).is_none_or(|at|at>self.frozen_daa){return Err("noncanonical or immature frozen watcher");}
            previous=Some(m.watcher);
        }
        Ok(())
    }
}
impl crate::palw_state_v2::PalwChainStateV2 {
    pub(crate) fn assert_verifier_watch_draw_consistency_v1(&self,params:&crate::palw_state_v2::PalwStateParamsV2)
        ->Result<(),crate::palw_state_v2::PalwStateV2Error>{
        use crate::palw_state_v2::PalwStateV2Error as E;
        let bad=|s:&str|E::CarriageInconsistent(s.into());
        let Some(route)=self.kernel_route() else{return Ok(());};
        for ((table,k),bytes) in &route.aux {
            if *table!=TABLE||!matches!(k.first(),Some(&POOL)|Some(&CONTEXT)){continue;}
            let (from,policy)=params.verifier_watch_v1.as_ref().ok_or_else(||bad("watcher draw rows without policy"))?;
            if k.len()!=65{return Err(bad("watcher draw key shape"));}
            if k[0]==POOL {
                let row=decode_watch_row::<PalwFrozenWatcherPoolV1>(bytes)?;row.validate(policy,*from).map_err(bad)?;
                if k[1..]!=row.id(){return Err(bad("watcher pool key identity"));}
                let (maturity,early)=params.verifier_watch_maturity_v1.ok_or_else(||bad("watcher maturity authority absent"))?;
                let floor=crate::config::params::palw_bond_maturity_window_in_force_v1(maturity,early,row.frozen_daa).unwrap_or(0)
                    .max(params.rfc0018_v1().map_or(0,|p|p.certification.verifier_maturity_daa));
                if row.maturity_window<floor{return Err(bad("frozen pool bypasses native maturity floor"));}
            } else {
                let row=decode_watch_row::<PalwWatcherClaimContextV1>(bytes)?;row.pool.validate(policy,*from).map_err(bad)?;
                if k[1..]!=row.claim||row.beacon.commitment_root!=row.claim||row.beacon.policy!=policy.beacon
                    ||row.beacon.ruleset_id!=route.header.policy.ruleset_digest
                    ||row.beacon.subject_kind!=misaka_palw_challenge::SubjectKindV1::ClaimVerification
                    ||row.beacon.commitment_position/policy.epoch_length_daa!=row.pool.epoch
                    ||row.beacon.challenge_epoch!=row.pool.epoch||row.pool.frozen_daa>row.beacon.commitment_position
                    ||!row.beacon.excluded_profiles.contains(&row.pool.class)
                    ||row.beacon.eligible_profiles.contains(&row.pool.class)
                    ||row.hard_deadline<=row.beacon.commitment_position
                    ||row.hard_deadline.checked_sub(row.beacon.commitment_position).is_none_or(|n|n>policy.maximum_claim_horizon_daa){
                    return Err(bad("watcher context differs from its original claim/epoch/window"));
                }
                if let Some(slots)=&row.selected {
                    if !slots.is_empty()&&slots.len()!=usize::from(policy.slots){return Err(bad("watcher draw slot count"));}
                    if slots.iter().any(|s|!row.pool.members.iter().any(|m|m.watcher==*s&&m.operator!=row.excluded_operator)){
                        return Err(bad("watcher draw names an excluded or absent member"));
                    }
                }
            }
        }
        Ok(())
    }
}
