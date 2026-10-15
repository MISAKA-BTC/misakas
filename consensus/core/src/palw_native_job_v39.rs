//! Original39 native job and monetary lifecycle. These raw rows never grant
//! admission on their own: the native fold authenticates current operators,
//! original consent, shared89 nonce, real debit and all original resource leases.
//! Requested maximum and actual first-EOS generation are separate commitments.
use crate::{
    palw_native_execution_v39::{CheckedNativeClassV39, NativeExecutionPoliciesV39},
    palw_state_v2::PalwBondKeyV2,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_job39::{ActualInputResolverV39, CheckedJobMaximumV39},
    flat_point39::FlatClaimV39,
    flat_quota11::{
        NativeRequesterNonceWitnessV11, TrustedEpochV11,
        owning39::{CheckedQuotaV39, NativeFundingAnchorV39},
    },
    hash::{Digest, object_id},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NativeJobTerminalV39 {
    Funded,
    Challengeable,
    Final,
    Refunded,
    Convicted,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeJobOriginalV39 {
    pub version: u16,
    pub class: Digest,
    pub source: Digest,
    pub policy: Digest,
    pub maximum: Digest,
    pub class_scope: Digest,
    pub class_resources: Digest,
    pub maximum_scope: Digest,
    pub maximum_resources: Digest,
    pub maximum_positions: u32,
    pub input_manifest: Digest,
    pub quota: Digest,
    pub shared_plan: Digest,
    pub source_plan: Digest,
    pub paged_plan: Digest,
    pub consent: Digest,
    pub maximum_contract: Digest,
    pub transport: Digest,
    pub queue: Digest,
    pub physical: Digest,
    pub final_policy: Digest,
    pub final_authority: Digest,
    pub final_basis: Digest,
    pub requester: PalwBondKeyV2,
    pub producer: PalwBondKeyV2,
    pub requester_operator: Digest,
    pub producer_operator: Digest,
    pub nonce: Digest,
    pub reward: u64,
    pub reserve: u64,
    pub finder: u64,
    pub actual_debit: u64,
    pub debit_receipt: Digest,
    pub admitted_block: Digest,
    pub admitted: u64,
    pub interval: u64,
    pub commit_until: u64,
    pub challenge_window: u64,
    pub final_tail: u64,
    pub deadline: u64,
}
impl NativeJobOriginalV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-paid-job/v39", self)
    }
    pub fn job(&self) -> Digest {
        object_id(
            b"misaka-palw/native/original-job-id/v39",
            &(39u16, self.class, self.maximum, self.nonce, self.requester, self.producer),
        )
    }
    pub fn funding_anchor(&self) -> NativeFundingAnchorV39 {
        NativeFundingAnchorV39 {
            class: self.class,
            source: self.source,
            maximum: self.maximum,
            job: self.job(),
            funding: self.root(),
            consent: self.consent,
            requester: crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&self.requester),
            producer: crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&self.producer),
            requester_operator: self.requester_operator,
            producer_operator: self.producer_operator,
            nonce: self.nonce,
            admitted: self.admitted,
            deadline: self.deadline,
            reward: self.reward,
            actual_debit: self.actual_debit,
            debit_receipt: self.debit_receipt,
        }
    }
    pub fn check(
        &self,
        class: &CheckedNativeClassV39,
        maximum: &CheckedJobMaximumV39,
        quota: &CheckedQuotaV39,
        p: &NativeExecutionPoliciesV39,
    ) -> Result<(), String> {
        p.check(p.network_domain)?;
        let execution = &p.execution.execution;
        let requester = crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&self.requester);
        let producer = crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&self.producer);
        let interval_end = self
            .interval
            .checked_add(1)
            .and_then(|n| n.checked_mul(p.execution.quota.interval_daa))
            .ok_or("native39 original interval overflow")?;
        let latest = self
            .commit_until
            .checked_add(self.challenge_window)
            .and_then(|n| n.checked_add(self.final_tail))
            .ok_or("native39 original complete challenge horizon overflow")?;
        if self.version != 39
            || self.class != class.root()
            || self.source != class.source().root()
            || self.policy != p.root()
            || class.wire().policy != p.root()
            || self.maximum != maximum.root()
            || maximum.request().source != self.source
            || self.class_scope != class.scope().root()
            || self.class_resources != class.resources().root()
            || self.maximum_scope != maximum.resources().scope().root()
            || self.maximum_resources != maximum.resources().root()
            || self.maximum_positions != maximum.maximum_positions()
            || self.maximum_positions > class.wire().max_positions
            || self.input_manifest != maximum.input_manifest().root()
            || self.quota != quota.root()
            || quota.class() != self.class
            || quota.maximum() != self.maximum
            || quota.policy() != p.root()
            || self.shared_plan != quota.plan().record().root()
            || self.source_plan != quota.source().root()
            || self.paged_plan != quota.paged().root()
            || self.final_policy != p.final_policy
            || self.final_authority != final_authority_v39(p)
            || self.physical != p.physical_service
            || requester != maximum.request().requester
            || producer != maximum.request().producer
            || self.nonce != maximum.request().nonce
            || self.requester == self.producer
            || self.requester_operator == self.producer_operator
            || self.reward == 0
            || self.reward > execution.reward_cap
            || self.actual_debit != self.reward
            || self.reserve != execution.required_producer_reserve
            || self.finder
                != u64::try_from(self.reserve as u128 * execution.finder_permille as u128 / 1000)
                    .map_err(|_| "native39 original finder extent")?
            || self.finder > self.reserve
            || self.interval != self.admitted / p.execution.quota.interval_daa
            || self.commit_until <= self.admitted
            || self.challenge_window == 0
            || self.final_tail == 0
            || latest >= self.deadline
            || self.deadline >= interval_end
            || self.deadline.checked_sub(self.admitted).is_none_or(|n| n > p.maximum_horizon)
            || [
                self.consent,
                self.maximum_contract,
                self.transport,
                self.queue,
                self.physical,
                self.final_policy,
                self.final_basis,
                self.requester_operator,
                self.producer_operator,
                self.debit_receipt,
                self.admitted_block,
            ]
            .contains(&[0; 64])
        {
            return Err("native39 original source/maximum/consent/coins/nonce/policy/full resource horizon differs".into());
        }
        Ok(())
    }
}
/// Native-only current admission capability. It cannot be decoded, cloned from
/// a producer DTO or constructed by an SDK. The fold supplies it only AFTER
/// original nonce/debit/consent/quota/queue preflights and actual coin mutation.
pub(crate) struct CheckedNativeFundingV39 {
    original: NativeJobOriginalV39,
}
impl CheckedNativeFundingV39 {
    pub(crate) fn from_current_native(
        original: NativeJobOriginalV39,
        class: &CheckedNativeClassV39,
        maximum: &CheckedJobMaximumV39,
        quota: &CheckedQuotaV39,
        p: &NativeExecutionPoliciesV39,
        nonce: &NativeRequesterNonceWitnessV11,
        epoch: &TrustedEpochV11,
    ) -> Result<Self, String> {
        original.check(class, maximum, quota, p)?;
        if epoch.block_daa != original.admitted
            || epoch.block_hash != original.admitted_block
            || epoch.network_interval != original.interval
        {
            return Err("native39 Fund is not original current chain epoch".into());
        }
        misaka_palw_kernel::flat_quota11::owning39::check_quota_admission_v39(
            quota,
            &original.funding_anchor(),
            &p.execution.quota,
            nonce,
            epoch,
        )?;
        Ok(Self { original })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeAcceptedClaimV39 {
    pub claim: Digest,
    pub actual_scope: Digest,
    pub actual_resources: Digest,
    pub input_binding: Digest,
    pub generated: Digest,
    pub positions: u32,
    pub committed: u64,
    pub challenge_end: u64,
    pub final_after: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeSettlementV39 {
    pub version: u16,
    pub job: Digest,
    pub original_funding: Digest,
    pub terminal: NativeJobTerminalV39,
    pub claim: Option<Digest>,
    pub proof: Digest,
    pub established: u64,
    pub original_escrow: u64,
    pub producer_credit: u64,
    pub requester_credit: u64,
    pub actual_slash: u64,
    pub finder_credit: u64,
    pub burned: u64,
}
impl NativeSettlementV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-actual-settlement/v39", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeJobBookV39 {
    version: u16,
    original: NativeJobOriginalV39,
    claim: Option<NativeAcceptedClaimV39>,
    terminal: NativeJobTerminalV39,
    escrow: u64,
    settlement: Option<NativeSettlementV39>,
}
impl NativeJobBookV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-live-job-book/v39", self)
    }
    pub fn id(&self) -> Digest {
        self.original.job()
    }
    pub fn original(&self) -> &NativeJobOriginalV39 {
        &self.original
    }
    pub fn funding(&self) -> Digest {
        self.original.root()
    }
    pub fn claim(&self) -> Option<&NativeAcceptedClaimV39> {
        self.claim.as_ref()
    }
    pub fn terminal(&self) -> NativeJobTerminalV39 {
        self.terminal
    }
    pub fn escrow(&self) -> u64 {
        self.escrow
    }
    pub fn settlement(&self) -> Option<&NativeSettlementV39> {
        self.settlement.as_ref()
    }
    pub(crate) fn fund(grant: CheckedNativeFundingV39) -> Self {
        let reward = grant.original.reward;
        Self {
            version: 39,
            original: grant.original,
            claim: None,
            terminal: NativeJobTerminalV39::Funded,
            escrow: reward,
            settlement: None,
        }
    }
    pub fn check(
        &self,
        class: &CheckedNativeClassV39,
        maximum: &CheckedJobMaximumV39,
        quota: &CheckedQuotaV39,
        p: &NativeExecutionPoliciesV39,
    ) -> Result<(), String> {
        self.original.check(class, maximum, quota, p)?;
        if self.version != 39 {
            return Err("native39 unknown book version".into());
        }
        match (&self.claim, self.terminal) {
            (None, NativeJobTerminalV39::Funded | NativeJobTerminalV39::Refunded) => {}
            (Some(c), NativeJobTerminalV39::Challengeable | NativeJobTerminalV39::Final | NativeJobTerminalV39::Convicted) => {
                if [c.claim, c.actual_scope, c.actual_resources, c.input_binding, c.generated].contains(&[0; 64])
                    || c.positions == 0
                    || c.positions > self.original.maximum_positions
                    || c.committed < self.original.admitted
                    || c.committed >= self.original.commit_until
                    || c.challenge_end
                        != c.committed.checked_add(self.original.challenge_window).ok_or("native39 original challenge overflow")?
                    || c.final_after
                        != c.challenge_end.checked_add(self.original.final_tail).ok_or("native39 original final clock overflow")?
                    || c.final_after >= self.original.deadline
                {
                    return Err("native39 actual claim/maximum/original clocks differ".into());
                }
            }
            _ => return Err("native39 terminal has no matching original claim".into()),
        }
        match (&self.settlement, self.terminal) {
            (None, NativeJobTerminalV39::Funded | NativeJobTerminalV39::Challengeable) => {
                if self.escrow != self.original.reward {
                    return Err("native39 original escrow was changed".into());
                }
            }
            (Some(s), NativeJobTerminalV39::Final | NativeJobTerminalV39::Refunded | NativeJobTerminalV39::Convicted) => {
                if self.escrow != 0
                    || s.version != 39
                    || s.job != self.id()
                    || s.original_funding != self.funding()
                    || s.terminal != self.terminal
                    || s.claim != self.claim.as_ref().map(|c| c.claim)
                    || s.original_escrow != self.original.reward
                    || s.producer_credit.checked_add(s.requester_credit) != Some(s.original_escrow)
                    || s.finder_credit.checked_add(s.burned) != Some(s.actual_slash)
                    || s.actual_slash > self.original.reserve
                    || s.established < self.original.admitted
                {
                    return Err("native39 original terminal monetary conservation differs".into());
                }
                match self.terminal {
                    NativeJobTerminalV39::Final => {
                        if s.producer_credit != self.original.reward
                            || s.requester_credit != 0
                            || s.actual_slash != 0
                            || s.proof == [0; 64]
                            || self.claim.as_ref().is_none_or(|c| s.established <= c.final_after)
                        {
                            return Err("native39 Final lacks original matured claim/full producer credit".into());
                        }
                    }
                    NativeJobTerminalV39::Refunded => {
                        if s.producer_credit != 0
                            || s.requester_credit != self.original.reward
                            || s.actual_slash != 0
                            || s.proof != [0; 64]
                            || s.established <= self.original.deadline
                        {
                            return Err("native39 unclaimed refund changed original escrow/clock".into());
                        }
                    }
                    NativeJobTerminalV39::Convicted => {
                        if s.producer_credit != 0
                            || s.requester_credit != self.original.reward
                            || s.actual_slash != self.original.reserve
                            || s.finder_credit != self.original.finder
                            || s.proof == [0; 64]
                            || s.established > self.original.deadline
                        {
                            return Err("native39 conviction lacks original collectible reserve/verdict clock".into());
                        }
                    }
                    _ => unreachable!(),
                }
            }
            _ => return Err("native39 terminal settlement missing".into()),
        }
        Ok(())
    }
    pub(crate) fn commit(
        &mut self,
        class: &CheckedNativeClassV39,
        maximum: &CheckedJobMaximumV39,
        quota: &CheckedQuotaV39,
        p: &NativeExecutionPoliciesV39,
        producer: &PalwBondKeyV2,
        now: u64,
        claim: &FlatClaimV39,
        generated: &[u32],
        inputs: &ActualInputResolverV39<'_>,
        actual_reserved: u64,
    ) -> Result<(), String> {
        self.check(class, maximum, quota, p)?;
        let (tokens, actual) = maximum.check_actual(class.source(), generated)?;
        let input_binding = inputs.root();
        if inputs.maximum() != maximum.root() || inputs.tokens() != tokens.as_slice() {
            return Err("native39 Commit actual input resolver changes original maximum/tokens".into());
        }
        if self.terminal != NativeJobTerminalV39::Funded
            || *producer != self.original.producer
            || now < self.original.admitted
            || now >= self.original.commit_until
            || actual_reserved < self.original.reserve
            || claim.version != 39
            || claim.source != self.original.source
            || claim.maximum_scope != self.original.maximum_scope
            || claim.actual_scope != actual.scope().root()
            || claim.positions as usize != tokens.len()
            || claim.input_binding != input_binding
            || input_binding == [0; 64]
            || claim.segment_roots.len() != misaka_palw_kernel::seg::segment_count_v1(claim.positions) as usize
            || claim.segment_roots.iter().any(|r| *r == [0; 64])
        {
            return Err("native39 Commit changes original producer/maximum/actual EOS/input/trace claim".into());
        }
        let challenge_end = now.checked_add(self.original.challenge_window).ok_or("native39 challenge overflow")?;
        let final_after = challenge_end.checked_add(self.original.final_tail).ok_or("native39 final clock overflow")?;
        self.claim = Some(NativeAcceptedClaimV39 {
            claim: claim.root(),
            actual_scope: actual.scope().root(),
            actual_resources: actual.root(),
            input_binding,
            generated: object_id(b"misaka-palw/native/original-stopped-generation/v39", &generated),
            positions: claim.positions,
            committed: now,
            challenge_end,
            final_after,
        });
        self.terminal = NativeJobTerminalV39::Challengeable;
        self.check(class, maximum, quota, p)
    }
    pub(crate) fn finalize(
        &mut self,
        conditions: &CheckedNativeFinalConditionsV39,
        closure: &NativeFinalClosureV39,
    ) -> Result<NativeSettlementV39, String> {
        let c = self.claim.as_ref().ok_or("native39 original claim absent")?;
        if self.terminal != NativeJobTerminalV39::Challengeable
            || self.settlement.is_some()
            || closure.job != self.id()
            || closure.funding != self.funding()
            || closure.claim != c.claim
            || closure.maximum != self.original.maximum_contract
            || closure.source != self.original.source_plan
            || closure.physical != self.original.physical
            || closure.queue != self.original.queue
            || closure.pending != 0
            || closure.owners != 0
            || closure.source_obligations != 0
            || closure.trace_obligations != 0
            || closure.original_closure == [0; 64]
            || closure.now <= c.final_after
            || conditions.policy_root() != self.original.final_policy
            || conditions.authority_root() != self.original.final_authority
            || conditions.basis_root() != self.original.final_basis
            || conditions.complete_path_daa() > self.original.challenge_window
        {
            return Err("native39 Final lacks genuine original independent conditions/full original obligations closure".into());
        }
        // Original paid rights survive a sparse clock jump. There is no new
        // challenge horizon and no old quota/window credit returned here.
        let proof = object_id(
            b"misaka-palw/native/original-verified-Final-conditions/v39",
            &(
                self.funding(),
                c.claim,
                conditions.policy_root(),
                conditions.authority_root(),
                conditions.basis_root(),
                closure.original_closure,
            ),
        );
        self.settle(NativeJobTerminalV39::Final, closure.now, proof, 0)
    }
    pub(crate) fn refund_unclaimed(&mut self, now: u64) -> Result<NativeSettlementV39, String> {
        if self.terminal != NativeJobTerminalV39::Funded
            || self.claim.is_some()
            || self.settlement.is_some()
            || now <= self.original.deadline
        {
            return Err("native39 original unclaimed escrow is not refundable".into());
        }
        self.settle(NativeJobTerminalV39::Refunded, now, [0; 64], 0)
    }
    pub(crate) fn convict(
        &mut self,
        now: u64,
        proof: Digest,
        actual_collected: u64,
        finder_operator: Digest,
    ) -> Result<NativeSettlementV39, String> {
        if self.terminal != NativeJobTerminalV39::Challengeable
            || self.settlement.is_some()
            || now < self.original.admitted
            || now > self.original.deadline
            || proof == [0; 64]
            || actual_collected != self.original.reserve
            || finder_operator == self.original.producer_operator
            || finder_operator == [0; 64]
        {
            return Err("native39 conviction lacks original independent proof/actual reserve collection".into());
        }
        self.settle(NativeJobTerminalV39::Convicted, now, proof, actual_collected)
    }
    fn settle(
        &mut self,
        terminal: NativeJobTerminalV39,
        now: u64,
        proof: Digest,
        actual_slash: u64,
    ) -> Result<NativeSettlementV39, String> {
        if self.settlement.is_some() || self.escrow != self.original.reward {
            return Err("native39 original escrow already settled".into());
        }
        let producer_credit = if terminal == NativeJobTerminalV39::Final { self.original.reward } else { 0 };
        let finder_credit = if terminal == NativeJobTerminalV39::Convicted { self.original.finder } else { 0 };
        let s = NativeSettlementV39 {
            version: 39,
            job: self.id(),
            original_funding: self.funding(),
            terminal,
            claim: self.claim.as_ref().map(|c| c.claim),
            proof,
            established: now,
            original_escrow: self.original.reward,
            producer_credit,
            requester_credit: self.original.reward - producer_credit,
            actual_slash,
            finder_credit,
            burned: actual_slash.checked_sub(finder_credit).ok_or("native39 actual finder exceeds collected amount")?,
        };
        self.escrow = 0;
        self.terminal = terminal;
        self.settlement = Some(s.clone());
        Ok(s)
    }
}
/// Native-fold-only closure metadata, derived from original paid source/trace/
/// protection/queue rows. No Borsh/serde constructor or producer boolean exists.
pub(crate) struct NativeFinalClosureV39 {
    pub job: Digest,
    pub funding: Digest,
    pub claim: Digest,
    pub maximum: Digest,
    pub source: Digest,
    pub physical: Digest,
    pub queue: Digest,
    pub now: u64,
    pub pending: u32,
    pub owners: u32,
    pub source_obligations: u32,
    pub trace_obligations: u32,
    pub original_closure: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTemporalV39 {
    pub version: u16,
    pub requests: u32,
    pub answer_window: u64,
    pub maturity: u64,
    pub acquisition: u64,
    pub proof_window: u64,
    pub commit_window: u64,
    pub challenge_window: u64,
    pub final_tail: u64,
    pub total: u64,
}
impl NativeTemporalV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-complete-opportunity/v39", self)
    }
    pub fn derive(p: &NativeExecutionPoliciesV39, requests: u32) -> Result<Self, String> {
        let h = &p.execution.execution.horizons;
        let acquisition = (requests as u64)
            .checked_mul(h.da_answer)
            .and_then(|n| n.checked_add(h.proof_grace))
            .ok_or("native39 original acquisition clock overflow")?;
        let proof_window =
            h.seal_ttl.max(acquisition.checked_add(h.seal_maturity).ok_or("native39 original proof maturity overflow")?);
        let final_tail = proof_window
            .checked_add(h.da_answer)
            .and_then(|n| n.checked_add(h.proof_grace))
            .and_then(|n| n.checked_add(h.retention))
            .ok_or("native39 original proof/delivery tail overflow")?;
        let total = h
            .commit_window
            .checked_add(h.challenge_window)
            .and_then(|n| n.checked_add(final_tail))
            .and_then(|n| n.checked_add(1))
            .ok_or("native39 original complete horizon overflow")?;
        if requests == 0 || h.da_answer == 0 || total > p.maximum_horizon || h.commit_window == 0 || h.challenge_window == 0 {
            return Err("native39 complete original opportunity does not fit policy".into());
        }
        Ok(Self {
            version: 39,
            requests,
            answer_window: h.da_answer,
            maturity: h.seal_maturity,
            acquisition,
            proof_window,
            commit_window: h.commit_window,
            challenge_window: h.challenge_window,
            final_tail,
            total,
        })
    }
    pub fn deadline(&self, now: u64, interval: u64) -> Result<u64, String> {
        let d = now.checked_add(self.total).ok_or("native39 original deadline overflow")?;
        let end =
            (now / interval).checked_add(1).and_then(|n| n.checked_mul(interval)).ok_or("native39 original interval overflow")?;
        if d >= end {
            return Err("native39 complete original opportunity crosses admitted interval".into());
        }
        Ok(d)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMaximumContractV39 {
    pub version: u16,
    pub class: Digest,
    pub source: Digest,
    pub maximum: Digest,
    pub policy: Digest,
    pub class_scope: Digest,
    pub class_resources: Digest,
    pub requested_scope: Digest,
    pub requested_resources: Digest,
    pub input_manifest: Digest,
    pub quota: Digest,
    pub shared_plan: Digest,
    pub source_plan: Digest,
    pub paged_plan: Digest,
    pub transport: Digest,
    pub queue: Digest,
    pub queue_policy: Digest,
    pub physical: Digest,
    pub temporal: NativeTemporalV39,
    pub final_policy: Digest,
    pub final_authority: Digest,
    pub final_basis: crate::palw_native_final_v27::NativeFinalClassBasisV27,
}
impl NativeMaximumContractV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-complete-maximum-contract/v39", self)
    }
}
pub struct CheckedNativeMaximumV39 {
    record: NativeMaximumContractV39,
    queue: crate::palw_native_queue_v30::CheckedNativeQueuePlanV30,
    conditions: CheckedNativeFinalConditionsV39,
}
impl CheckedNativeMaximumV39 {
    pub fn record(&self) -> &NativeMaximumContractV39 {
        &self.record
    }
    pub fn queue(&self) -> &crate::palw_native_queue_v30::CheckedNativeQueuePlanV30 {
        &self.queue
    }
    pub fn conditions(&self) -> &CheckedNativeFinalConditionsV39 {
        &self.conditions
    }
    pub fn carriage(&self) -> &crate::palw_native_paged_carriage_v29::CheckedNativePagedCarriagePlanV29 {
        self.queue.paged_carriage().expect("owning39 checked paged carriage")
    }
    pub fn derive(
        class: &CheckedNativeClassV39,
        maximum: &CheckedJobMaximumV39,
        quota: &CheckedQuotaV39,
        p: &NativeExecutionPoliciesV39,
        final_policy: &crate::palw_native_final_v27::NativeFinalPolicyV27,
        ledger: &misaka_palw_kernel::ledger::LedgerPolicyV1,
        mass: &crate::palw_native_carriage_v22::NativeCarriageMassParamsV22,
        service: &crate::palw_flat_execution_capacity_v22::NativeCarriageServiceV22,
    ) -> Result<Self, String> {
        use misaka_palw_kernel::flat_resources10::ResourceAxisV10 as A;
        p.check(p.network_domain)?;
        if final_policy.root() != p.final_policy
            || service.root() != p.physical_service
            || class.wire().policy != p.root()
            || quota.class() != class.root()
            || quota.maximum() != maximum.root()
            || maximum.request().source != class.source().root()
            || maximum.maximum_positions() > class.wire().max_positions
        {
            return Err("native39 original class/source/max/independent Final policy differs".into());
        }
        let temporal = NativeTemporalV39::derive(p, quota.source().requests_per_owner)?;
        let proof_service =
            temporal.proof_window.checked_sub(temporal.maturity).ok_or("native39 original proof transport window absent")?;
        let carriage = crate::palw_native_paged_carriage_v29::CheckedNativePagedCarriagePlanV29::derive_v39(
            class.source(),
            maximum,
            quota,
            temporal.root(),
            temporal.answer_window,
            proof_service,
            temporal.commit_window,
            service,
            mass,
        )?;
        let queue = crate::palw_native_queue_v30::CheckedNativeQueuePlanV30::derive_paged(&final_policy.queue_policy, mass, carriage)?;
        let resources = &maximum.resources().vector().quantities;
        let cold = [
            A::Macs,
            A::ScalarSteps,
            A::IndexSteps,
            A::ValidatedElements,
            A::ParameterHashBytes,
            A::TraceHashBytes,
            A::MetadataWork,
            A::CopiedReferenceBytes,
            A::RawParameterStorage,
            A::DeliveryBytes,
            A::StoredEvidenceBytes,
            A::CourtWork,
        ]
        .into_iter()
        .try_fold(0u128, |n, a| n.checked_add(resources.get(a)).ok_or("native39 complete cold cost overflow"))?;
        // The claimant cannot supply a cheap acquisition count or work vector.
        // Price every sequential public request and complete court attempt from
        // the ORIGINAL independently derived maximum, even after early EOS.
        let request = quota.plan().record().action_envelopes[1].get(A::MetadataWork);
        let work = cold
            .checked_add(
                request.checked_mul(quota.source().requests_per_owner as u128).ok_or("native39 complete acquisition cost overflow")?,
            )
            .and_then(|n| n.checked_add(quota.paged().court.get(A::MetadataWork).checked_mul(2)?))
            .and_then(|n| n.checked_add(quota.paged().court.get(A::CourtWork).checked_mul(2)?))
            .ok_or("native39 complete proof cost overflow")?;
        let reference_cost = ledger
            .dismissal_fee_v1(u64::try_from(work).map_err(|_| "native39 original proof cost outside monetary range")?)
            .checked_add(
                ledger
                    .demand_bond
                    .checked_mul(quota.source().requests_per_owner as u64)
                    .ok_or("native39 original demand cost overflow")?,
            )
            .ok_or("native39 complete monetary cost overflow")?;
        let execution = &p.execution.execution;
        let finder = u64::try_from(execution.required_producer_reserve as u128 * execution.finder_permille as u128 / 1000)
            .map_err(|_| "native39 finder range")?;
        let max_reference = u64::try_from(
            resources
                .get(A::RawParameterStorage)
                .checked_add(resources.get(A::ReferenceTensorPayload))
                .and_then(|n| n.checked_add(resources.get(A::StoredEvidenceBytes)))
                .ok_or("native39 complete reference bytes overflow")?,
        )
        .map_err(|_| "native39 complete reference bytes outside profile")?;
        let basis = crate::palw_native_final_v27::NativeFinalClassBasisV27 {
            class: class.root(),
            program: class.source().program_root(),
            inventory: class.source().inventory_root(),
            real_parameter: class.source().parameters().root(),
            original_max_scope: class.scope().root(),
            original_max_resource: class.resources().root(),
            execution_policy: execution.root(),
            quota_policy: p.execution.quota.root(),
            discovery_policy: p.execution.discovery.root(),
            source_protocol: p.source_protocol39,
            verifier_profile: misaka_palw_kernel::flat_resources39::ordinary_court_id_v39(),
            physical_service: p.physical_service,
            complete_reference_cost_sompi: reference_cost,
            maximum_producer_gain_sompi: execution
                .reward_cap
                .checked_add(execution.external_gain_cap)
                .ok_or("native39 original maximum gain overflow")?,
            original_producer_reserve_sompi: execution.required_producer_reserve,
            original_finder_award_sompi: finder,
            original_default_obligation_sompi: execution.default_obligation,
            maximum_reference_bytes: max_reference,
            maximum_court_bytes: quota.paged().maximum_bytes,
            maximum_court_work: class.scope().maximum_work,
            maximum_court_scratch: class.scope().maximum_scratch,
        };
        let conditions = CheckedNativeFinalConditionsV39::derive(p, final_policy, &basis, &temporal, service.implemented)?;
        let record = NativeMaximumContractV39 {
            version: 39,
            class: class.root(),
            source: class.source().root(),
            maximum: maximum.root(),
            policy: p.root(),
            class_scope: class.scope().root(),
            class_resources: class.resources().root(),
            requested_scope: maximum.resources().scope().root(),
            requested_resources: maximum.resources().root(),
            input_manifest: maximum.input_manifest().root(),
            quota: quota.root(),
            shared_plan: quota.plan().record().root(),
            source_plan: quota.source().root(),
            paged_plan: quota.paged().root(),
            transport: queue.record().original_carriage,
            queue: queue.record().root(),
            queue_policy: queue.record().policy,
            physical: p.physical_service,
            temporal,
            final_policy: p.final_policy,
            final_authority: conditions.authority_root(),
            final_basis: basis,
        };
        Ok(Self { record, queue, conditions })
    }
}

/// Producer-authored maximum admission, signed before any requester can reserve
/// its resources. The generation vector is deliberately absent. All variable
/// fields use bounded nested codecs before allocation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeConsentWireV39 {
    pub version: u16,
    pub class: Digest,
    pub requester: PalwBondKeyV2,
    pub requester_operator: Digest,
    pub producer_operator: Digest,
    pub reward: u64,
    pub expires: u64,
    pub maximum_contract: Digest,
    pub maximum: Vec<u8>,
    pub material: Vec<u8>,
}
impl NativeConsentWireV39 {
    pub const MAX_BYTES: usize = (8 << 20) + (64 << 10) + 8192;
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        use std::io::{Cursor, Read};
        if raw.len() > Self::MAX_BYTES {
            return Err("native39 consent maximum extent".into());
        }
        let mut r = Cursor::new(raw);
        let version = u16::deserialize_reader(&mut r).map_err(|_| "native39 consent version")?;
        let class = Digest::deserialize_reader(&mut r).map_err(|_| "native39 consent class")?;
        let requester = PalwBondKeyV2::deserialize_reader(&mut r).map_err(|_| "native39 consent requester")?;
        let requester_operator = Digest::deserialize_reader(&mut r).map_err(|_| "native39 consent operator")?;
        let producer_operator = Digest::deserialize_reader(&mut r).map_err(|_| "native39 consent operator")?;
        let reward = u64::deserialize_reader(&mut r).map_err(|_| "native39 consent reward")?;
        let expires = u64::deserialize_reader(&mut r).map_err(|_| "native39 consent expiry")?;
        let maximum_contract = Digest::deserialize_reader(&mut r).map_err(|_| "native39 consent contract")?;
        let mut blob = |cap: usize| -> Result<Vec<u8>, String> {
            let n = u32::deserialize_reader(&mut r).map_err(|_| "native39 nested maximum length")? as usize;
            if n > cap || n > raw.len().saturating_sub(r.position() as usize) {
                return Err("native39 nested preallocation extent".into());
            }
            let mut b = vec![0; n];
            r.read_exact(&mut b).map_err(|_| "native39 nested maximum bytes")?;
            Ok(b)
        };
        let maximum = blob((8 << 20) + 4096)?;
        let material = blob(64 << 10)?;
        let value = Self {
            version,
            class,
            requester,
            requester_operator,
            producer_operator,
            reward,
            expires,
            maximum_contract,
            maximum,
            material,
        };
        if version != 39 || r.position() != raw.len() as u64 || borsh::to_vec(&value).map_err(|e| e.to_string())? != raw {
            return Err("native39 consent canonical grammar".into());
        }
        Ok(value)
    }
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/producer-signed-maximum/v39", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeConsentV39 {
    pub wire: NativeConsentWireV39,
    pub producer: PalwBondKeyV2,
    pub accepted: u64,
    pub interval: u64,
    pub checked_maximum: NativeMaximumContractV39,
}
impl NativeConsentV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/accepted-original-producer-consent/v39", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeFundWireV39 {
    pub version: u16,
    pub consent: Digest,
}
impl NativeFundWireV39 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() != 66 {
            return Err("native39 Fund exact fixed extent".into());
        }
        let x: Self = borsh::from_slice(raw).map_err(|_| "native39 Fund grammar")?;
        if x.version != 39 {
            return Err("native39 Fund version".into());
        }
        Ok(x)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeCommitWireV39 {
    pub version: u16,
    pub job: Digest,
    pub generated: Vec<u32>,
    pub claim: FlatClaimV39,
}
impl NativeCommitWireV39 {
    pub const MAX_BYTES: usize = (9 << 20);
    pub fn from_bytes(raw: &[u8], maximum: u32) -> Result<Self, String> {
        use std::io::Cursor;
        if raw.len() > Self::MAX_BYTES {
            return Err("native39 original compactCommit extent".into());
        }
        let mut r = Cursor::new(raw);
        let version = u16::deserialize_reader(&mut r).map_err(|_| "native39 Commit version")?;
        let job = Digest::deserialize_reader(&mut r).map_err(|_| "native39 Commit job")?;
        let count = u32::deserialize_reader(&mut r).map_err(|_| "native39 generated count")? as usize;
        if count > maximum as usize || count > raw.len().saturating_sub(r.position() as usize) / 4 {
            return Err("native39 generated preallocation maximum".into());
        }
        let mut generated = Vec::with_capacity(count);
        for _ in 0..count {
            generated.push(u32::deserialize_reader(&mut r).map_err(|_| "native39 generated IDs")?);
        }
        let claim_version = u16::deserialize_reader(&mut r).map_err(|_| "native39 claim version")?;
        let mut digest = || Digest::deserialize_reader(&mut r).map_err(|_| "native39 claim digest".to_string());
        let source = digest()?;
        let claim_maximum = digest()?;
        let generated_root = digest()?;
        let maximum_scope = digest()?;
        let actual_scope = digest()?;
        let input_binding = digest()?;
        let positions = u32::deserialize_reader(&mut r).map_err(|_| "native39 claim positions")?;
        let n = u32::deserialize_reader(&mut r).map_err(|_| "native39 claim segment count")? as usize;
        if positions == 0
            || positions > maximum
            || n != misaka_palw_kernel::seg::segment_count_v1(positions) as usize
            || n > raw.len().saturating_sub(r.position() as usize) / 64
        {
            return Err("native39 compact roots preallocation extent".into());
        }
        let mut segment_roots = Vec::with_capacity(n);
        for _ in 0..n {
            segment_roots.push(Digest::deserialize_reader(&mut r).map_err(|_| "native39 segment root")?);
        }
        let claim = FlatClaimV39 {
            version: claim_version,
            source,
            maximum: claim_maximum,
            generated: generated_root,
            maximum_scope,
            actual_scope,
            input_binding,
            positions,
            segment_roots,
        };
        let value = Self { version, job, generated, claim };
        if version != 39
            || claim_version != 39
            || r.position() != raw.len() as u64
            || borsh::to_vec(&value).map_err(|e| e.to_string())? != raw
        {
            return Err("native39 Commit canonical grammar".into());
        }
        Ok(value)
    }
}
impl CheckedNativeFundingV39 {
    pub(crate) fn original(&self) -> &NativeJobOriginalV39 {
        &self.original
    }
}
#[derive(Clone, Copy, Debug)]
pub struct BorrowedNativeProofPartV39<'a> {
    pub mode: u8,
    pub job: Digest,
    pub seal: Digest,
    pub index: u16,
    pub count: u16,
    pub total: u32,
    pub body: &'a [u8],
}
pub fn borrowed_native_proof_part_v39(raw: &[u8]) -> Option<BorrowedNativeProofPartV39<'_>> {
    if raw.len() < 143 || raw.len() > 143 + (32 << 10) || raw[..2] != 39u16.to_le_bytes() {
        return None;
    }
    let mode = raw[2];
    let index = u16::from_le_bytes(raw[131..133].try_into().ok()?);
    let count = u16::from_le_bytes(raw[133..135].try_into().ok()?);
    let total = u32::from_le_bytes(raw[135..139].try_into().ok()?);
    let n = u32::from_le_bytes(raw[139..143].try_into().ok()?) as usize;
    if mode > 1
        || count == 0
        || count > 2048
        || total == 0
        || total > 64 << 20
        || count as u64 != (total as u64).div_ceil(32 << 10)
        || mode == 0 && (index >= count || n != raw.len() - 143)
        || mode == 1 && (index != 0 || n != 0 || raw.len() != 143)
    {
        return None;
    }
    Some(BorrowedNativeProofPartV39 {
        mode,
        job: raw[3..67].try_into().ok()?,
        seal: raw[67..131].try_into().ok()?,
        index,
        count,
        total,
        body: &raw[143..],
    })
}
/// Permanent original single-credit anchor. It identifies one job-specific
/// actual payout queue, surviving queue drain and job metadata collection.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeSingleCreditV39 {
    pub version: u16,
    pub job: Digest,
    pub funding: Digest,
    pub terminal: Digest,
    pub settlement: Digest,
    pub claim: Option<Digest>,
    pub recipient: PalwBondKeyV2,
    pub payout: kaspa_hashes::Hash64,
    pub queue: kaspa_hashes::Hash64,
    pub amount: u64,
    pub established: u64,
}
impl NativeSingleCreditV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-actual-single-credit/v39", self)
    }
}

/// Owning39 authority identity. Economic detection/expected-profit observations
/// are reporting metrics, not numerical activation gates. Hash/Merkle paths
/// authenticate original source/material/trace identity only; local courts
/// recompute a disclosed disputed relation. A funded escrow Final grants no
/// block reward or Final weight and cannot replace sampled-validator approval.
pub fn final_conditions_id_v39() -> Digest {
    object_id(b"misaka-palw/native/original-Final-authority-rules/v39",&(
        39u16,
        misaka_palw_kernel::flat_resources39::ordinary_court_id_v39(),
        misaka_palw_kernel::flat_public39::public_protocol_id_v39(),
        b"metrics-report-only;original-paid-coins/shared21/queue30/mass/temporal;independent-finalizer;public-local-reexecution-courts;identity-only-hash-signature-merkle;escrowFinal-zero-B-F-R;sampled-validator-credit-separate;no-cryptographic-LLM-correctness-proof".to_vec(),
    ))
}

/// New owning39 rules explicitly separate evaluation metrics from admission.
/// Old27 wire and consumers retain their original meaning. This token cannot be
/// deserialized or minted by a claimant's calibration record or success flag.
pub struct CheckedNativeFinalConditionsV39 {
    policy: Digest,
    authority: Digest,
    basis: Digest,
    path: u64,
    metrics: NativeFinalMetricsV39,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeFinalMetricsV39 {
    /// None is UNKNOWN. No probability or expected-profit threshold grants or
    /// revokes funded escrow rights under owning39.
    pub detection_fraction: Option<(u64, u64)>,
    pub complete_reference_cost_sompi: u64,
    pub maximum_gain_sompi: u64,
    pub collectible_reserve_sompi: u64,
    pub finder_award_sompi: u64,
}
pub fn final_authority_v39(p: &NativeExecutionPoliciesV39) -> Digest {
    object_id(b"misaka-palw/native/original-final-authority/v39", &(p.root(), final_conditions_id_v39()))
}
impl CheckedNativeFinalConditionsV39 {
    pub fn policy_root(&self) -> Digest {
        self.policy
    }
    pub fn authority_root(&self) -> Digest {
        self.authority
    }
    pub fn basis_root(&self) -> Digest {
        self.basis
    }
    pub fn complete_path_daa(&self) -> u64 {
        self.path
    }
    pub fn metrics(&self) -> &NativeFinalMetricsV39 {
        &self.metrics
    }
    fn derive(
        p: &NativeExecutionPoliciesV39,
        environment: &crate::palw_native_final_v27::NativeFinalPolicyV27,
        basis: &crate::palw_native_final_v27::NativeFinalClassBasisV27,
        temporal: &NativeTemporalV39,
        actual_service: bool,
    ) -> Result<Self, String> {
        // The authenticated environment supplies measured resource/time and
        // actual service assumptions, never a class/model/compiler allowlist.
        // In particular do NOT call old27.check/check_class: those frozen
        // consumers intentionally have different numerical economic gates.
        environment.queue_policy.check(p.network_domain)?;
        if environment.version != 27
            || environment.network_domain != p.network_domain
            || environment.root() != p.final_policy
            || !actual_service
            || environment.calibrations.len() > 128
            || environment.calibrations.windows(2).any(|w| w[0].profile >= w[1].profile)
            || basis.original_finder_award_sompi > basis.original_producer_reserve_sompi
            || basis.execution_policy != p.execution.execution.root()
            || basis.quota_policy != p.execution.quota.root()
            || basis.discovery_policy != p.execution.discovery.root()
            || basis.source_protocol != p.source_protocol39
            || basis.physical_service != p.physical_service
        {
            return Err("native39 original environment/resources/collateral identity differs".into());
        }
        let c = environment
            .calibrations
            .iter()
            .find(|c| c.profile == basis.verifier_profile)
            .ok_or("native39 executable public verifier profile lacks resource/time service coverage")?;
        if basis.maximum_reference_bytes > c.maximum_reference_bytes
            || basis.maximum_court_bytes > c.maximum_court_bytes
            || basis.maximum_court_work > c.maximum_court_work
            || basis.maximum_court_scratch > c.maximum_court_scratch
            || !c.original_source_ready
            || !c.whole_claim_detector_ready
            || !c.actual_delivery_ready
            || (environment.purpose.is_qualified() && !c.independent_node_ready)
            || [c.measurement_root, c.source_availability_root, c.real_delivery_service_root].contains(&[0; 64])
            || (environment.purpose.is_qualified() && c.independent_native_final_root == [0; 64])
        {
            return Err("native39 original source/executable detector/time/resource/delivery conditions unready".into());
        }
        let path = [
            c.measured_cold_reference_daa,
            c.measured_sequential_discovery_daa,
            c.measured_prove_before_seal_daa,
            c.measured_proof_delivery_daa,
            c.restart_reorg_margin_daa,
        ]
        .into_iter()
        .try_fold(0u64, |n, x| n.checked_add(x))
        .ok_or("native39 measured path overflow")?;
        let original = temporal.acquisition.checked_add(temporal.proof_window).ok_or("native39 path overflow")?;
        if path == 0 || path > original || path > temporal.challenge_window {
            return Err("native39 measured complete path exceeds original prepaid opportunity".into());
        }
        let fraction = (c.whole_claim_detection_denominator != 0
            && c.whole_claim_detection_numerator <= c.whole_claim_detection_denominator
            && c.whole_claim_detection_root != [0; 64])
            .then_some((c.whole_claim_detection_numerator, c.whole_claim_detection_denominator));
        Ok(Self {
            policy: environment.root(),
            authority: final_authority_v39(p),
            basis: basis.root(),
            path,
            metrics: NativeFinalMetricsV39 {
                detection_fraction: fraction,
                complete_reference_cost_sompi: basis.complete_reference_cost_sompi,
                maximum_gain_sompi: basis.maximum_producer_gain_sompi,
                collectible_reserve_sompi: basis.original_producer_reserve_sompi,
                finder_award_sompi: basis.original_finder_award_sompi,
            },
        })
    }
}
