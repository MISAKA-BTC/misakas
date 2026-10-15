//! First-EOS-or-maximum native text semantics. Fixed-count execution11 is
//! unchanged. Admission reserves the ORIGINAL maximum; actual generation only
//! selects its exact committed extent and can never refund that reservation.
use crate::palw_native_final_v27::{CheckedNativeFinalConditionsV27, NativeFinalClassBasisV27};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_resources29::{FlatResourcePlanV29, derive_flat_resources_v29},
    flat_scope25::CheckedResourceScopeV25,
    hash::{Digest, object_id},
    seg::{SEG_LEN_V4, claim_root_v1},
    trace::ParamCommitmentsV1,
};
use misaka_palw_tir::program::TirProgramV1;

pub const TEXT_STOP_VERSION_V29: u16 = 29;
pub const TEXT_STOP_BOOK_BYTES_V29: usize = 8192;
pub const TEXT_STOP_MAX_PARTS_V29: u16 = 512;
pub const TEXT_STOP_PART_BYTES_V29: usize = 32 << 10;
pub const TEXT_STOP_MAX_FILING_BYTES_V29: usize = 16 << 20;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTextRequestV29 {
    pub version: u16,
    pub class: Digest,
    pub requester: Digest,
    pub requester_operator: Digest,
    pub producer: Digest,
    pub producer_operator: Digest,
    pub nonce: Digest,
    pub prompt_root: Digest,
    pub prompt_len: u32,
    pub maximum_new_tokens: u32,
    pub eos_token: u32,
    pub reward: u64,
}
impl NativeTextRequestV29 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-eos-maximum-request/v29", self)
    }
    pub fn maximum_positions(&self) -> Result<u32, String> {
        if self.version != 29
            || self.prompt_len == 0
            || self.maximum_new_tokens == 0
            || self.reward == 0
            || self.requester == [0; 64]
            || self.producer == [0; 64]
            || self.requester_operator == self.producer_operator
        {
            return Err("text29 requires original bounded nonempty decoder request and independent operators".into());
        }
        self.prompt_len
            .checked_add(self.maximum_new_tokens)
            .and_then(|n| n.checked_sub(1))
            .ok_or("text29 maximum extent overflow".into())
    }
    pub fn check_prompt(&self, program: &TirProgramV1, prompt: &[u32]) -> Result<(), String> {
        let max = self.maximum_positions()?;
        if self.prompt_len as usize != prompt.len()
            || prompt_root_v29(prompt) != self.prompt_root
            || max > program.history_bound
            || self.eos_token >= program.token_bound
            || prompt.iter().any(|t| *t >= program.token_bound)
        {
            return Err("text29 original prompt/EOS/token/context scope differs".into());
        }
        Ok(())
    }
}
pub fn prompt_root_v29(prompt: &[u32]) -> Digest {
    object_id(b"misaka-palw/native/original-eos-prompt/v29", &prompt)
}
/// The tokenizer/emission-stop contract is data owned by requester AND producer.
/// Consensus does not infer EOS from a model name or a host tokenizer package.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTextMaximumV29 {
    pub version: u16,
    pub request: Digest,
    pub program: Digest,
    pub parameter: Digest,
    pub inventory: Digest,
    pub class_maximum_positions: u32,
    pub class_scope25: Digest,
    pub class_resource29: Digest,
    pub maximum_positions: u32,
    pub maximum_scope25: Digest,
    pub maximum_resource29: Digest,
    pub full_court_profile25: Digest,
    pub final_policy27: Digest,
    pub final_basis27: Digest,
    pub source_acquisition: Digest,
    pub public_pursuit: Digest,
    pub physical_lease_plan: Digest,
    pub original_shared_quota_plan: Digest,
    pub original_fee_plan: Digest,
    pub paged_proof_plan: Digest,
    pub source_caller_plan: Digest,
    pub queue_policy: Digest,
    pub queue_plan: Digest,
    pub challenge_window: u64,
    pub retention_tail: u64,
    pub commit_window: u64,
    pub complete_job_horizon: u64,
    pub producer_reserve: u64,
    pub finder_award: u64,
}
impl NativeTextMaximumV29 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-eos-maximum-contract/v29", self)
    }
}
pub struct CheckedNativeTextMaximumV29 {
    record: NativeTextMaximumV29,
    resources: FlatResourcePlanV29,
    maximum_job_resources: FlatResourcePlanV29,
}
impl CheckedNativeTextMaximumV29 {
    pub fn record(&self) -> &NativeTextMaximumV29 {
        &self.record
    }
    pub fn resources(&self) -> &FlatResourcePlanV29 {
        &self.resources
    }
    pub fn maximum_job_resources(&self) -> &FlatResourcePlanV29 {
        &self.maximum_job_resources
    }
    /// Original native source, quote roots and policy grant are authenticated by
    /// the consumer before this call. Full25 derivation remains public/open.
    pub fn derive(
        program: &TirProgramV1,
        pc: &ParamCommitmentsV1,
        request: &NativeTextRequestV29,
        prompt: &[u32],
        record: NativeTextMaximumV29,
        basis: &NativeFinalClassBasisV27,
        final_conditions: &CheckedNativeFinalConditionsV27,
    ) -> Result<Self, String> {
        request.check_prompt(program, prompt)?;
        let maximum_job_resources = derive_flat_resources_v29(program, pc, request.maximum_positions()?, None)?;
        let resources = derive_flat_resources_v29(program, pc, record.class_maximum_positions, None)?;
        if record.version != 29
            || record.request != request.root()
            || record.program != misaka_palw_kernel::public::program_root_v1(&program.encode())
            || record.parameter != pc.root()
            || record.maximum_positions != request.maximum_positions()?
            || record.class_maximum_positions < record.maximum_positions
            || record.class_maximum_positions > program.history_bound
            || record.class_scope25 != resources.court.root()
            || record.class_resource29 != resources.vector.root()
            || record.maximum_scope25 != maximum_job_resources.court.root()
            || record.maximum_resource29 != maximum_job_resources.vector.root()
            || record.full_court_profile25 != resources.court.court_set
            || record.final_policy27 != final_conditions.policy_root()
            || record.final_basis27 != final_conditions.basis_root()
            || record.final_basis27 != basis.root()
            || basis.class != request.class
            || basis.program != record.program
            || basis.real_parameter != record.parameter
            || basis.inventory != record.inventory
            || basis.original_max_scope != record.class_scope25
            || basis.original_max_resource != record.class_resource29
            || basis.verifier_profile != record.full_court_profile25
            || record.producer_reserve != basis.original_producer_reserve_sompi
            || record.finder_award != basis.original_finder_award_sompi
            || record.challenge_window < final_conditions.complete_path_daa()
            || record.complete_job_horizon
                <= record
                    .commit_window
                    .checked_add(record.challenge_window)
                    .and_then(|n| n.checked_add(record.retention_tail))
                    .ok_or("text29 original clock overflow")?
            || [
                record.inventory,
                record.source_acquisition,
                record.public_pursuit,
                record.physical_lease_plan,
                record.original_shared_quota_plan,
                record.original_fee_plan,
                record.paged_proof_plan,
                record.source_caller_plan,
                record.queue_policy,
                record.queue_plan,
            ]
            .contains(&[0; 64])
        {
            return Err("text29 changes original maximum source/resources/Final/temporal/economic contract".into());
        }
        Ok(Self { record, resources, maximum_job_resources })
    }
    pub fn check_actual(
        &self,
        program: &TirProgramV1,
        pc: &ParamCommitmentsV1,
        request: &NativeTextRequestV29,
        generated: &[u32],
    ) -> Result<CheckedNativeTextActualV29, String> {
        if request.root() != self.record.request
            || pc.root() != self.record.parameter
            || misaka_palw_kernel::public::program_root_v1(&program.encode()) != self.record.program
        {
            return Err("text29 actual rebinds original program/PC/request".into());
        }
        let stop = check_generated_v29(request, program.token_bound, generated)?;
        let count = u32::try_from(generated.len()).map_err(|_| "text29 actual length overflow")?;
        let positions = request.prompt_len.checked_add(count).and_then(|n| n.checked_sub(1)).ok_or("text29 actual extent overflow")?;
        let resource = derive_flat_resources_v29(program, pc, positions, None)?;
        resource
            .vector
            .quantities
            .within(&self.maximum_job_resources.vector.quantities)
            .map_err(|a| format!("text29 actual exceeds original maximum axis {a:?}"))?;
        Ok(CheckedNativeTextActualV29 {
            request: request.root(),
            generated_root: object_id(b"misaka-palw/native/exact-stopped-output/v29", &generated),
            count,
            positions,
            scope: resource.court.root(),
            resource: resource.vector.root(),
            stop,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NativeTextStopV29 {
    FirstEos,
    Maximum,
}
pub fn check_generated_v29(request: &NativeTextRequestV29, token_bound: u32, generated: &[u32]) -> Result<NativeTextStopV29, String> {
    request.maximum_positions()?;
    if generated.is_empty()
        || generated.len() > request.maximum_new_tokens as usize
        || generated.iter().any(|t| *t >= token_bound)
        || request.eos_token >= token_bound
        || generated[..generated.len() - 1].contains(&request.eos_token)
    {
        return Err("text29 output is empty/oversized/outside vocabulary or continues after first EOS".into());
    }
    if generated.last() == Some(&request.eos_token) {
        Ok(NativeTextStopV29::FirstEos)
    } else if generated.len() == request.maximum_new_tokens as usize {
        Ok(NativeTextStopV29::Maximum)
    } else {
        Err("text29 output truncates before EOS and original maximum".into())
    }
}
pub struct CheckedNativeTextActualV29 {
    request: Digest,
    generated_root: Digest,
    count: u32,
    positions: u32,
    scope: Digest,
    resource: Digest,
    stop: NativeTextStopV29,
}
impl CheckedNativeTextActualV29 {
    pub fn positions(&self) -> u32 {
        self.positions
    }
    pub fn count(&self) -> u32 {
        self.count
    }
    pub fn scope_root(&self) -> Digest {
        self.scope
    }
    pub fn resource_root(&self) -> Digest {
        self.resource
    }
    pub fn generated_root(&self) -> Digest {
        self.generated_root
    }
}
/// Fixed paid receipt supplied ONLY by native after consent, nonce, original
/// coins, shared21 maximum reservation, QT/R, source and physical lease commit.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTextFundingV29 {
    pub request: NativeTextRequestV29,
    pub maximum: NativeTextMaximumV29,
    pub consent: Digest,
    pub escrow_debit: Digest,
    pub maximum_reservation: Digest,
    pub original_shared_subject: Digest,
    pub admitted_daa: u64,
    pub admitted_interval: u64,
    pub commit_deadline: u64,
    pub deadline: u64,
}
impl NativeTextFundingV29 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-eos-funded-receipt/v29", self)
    }
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/native/eos-job/v29", &(29u16, self.request.root()))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTextClaimV29 {
    pub job: Digest,
    pub producer: Digest,
    pub vector: Digest,
    pub trace: Digest,
    pub final_state: Digest,
    pub actual_positions: u32,
    pub actual_generated: u32,
    pub generated_root: Digest,
    pub actual_scope25: Digest,
    pub actual_resource29: Digest,
    pub stop: NativeTextStopV29,
    pub committed_daa: u64,
    pub challenge_end: u64,
    pub retained_until: u64,
}
impl NativeTextClaimV29 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/exact-eos-claim/v29", self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NativeTextTerminalV29 {
    Funded,
    Challengeable,
    Final,
    Convicted,
    Refunded,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTextSettlementV29 {
    pub job: Digest,
    pub original_funding: Digest,
    pub claim: Option<Digest>,
    pub terminal: NativeTextTerminalV29,
    pub escrow_debit: u64,
    pub producer: u64,
    pub requester: u64,
    pub actual_collected_slash: u64,
    pub finder: Digest,
    pub finder_credit: u64,
    pub burned: u64,
    pub proof: Digest,
}
impl NativeTextSettlementV29 {
    /// Root of the exact terminal money split. A producer Final credit queue
    /// binds it through the shared unique-credit identity (`native_final_credit_queue_v27`),
    /// so audit32 / redemption33 verify Text29 credits with the same verifier as v11.
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/text-terminal-settlement/v29", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTextBookV29 {
    funded: NativeTextFundingV29,
    claim: Option<NativeTextClaimV29>,
    terminal: NativeTextTerminalV29,
    escrow: u64,
    settlement: Option<NativeTextSettlementV29>,
}
impl NativeTextBookV29 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/eos-job-book/v29", self)
    }
    pub fn funding(&self) -> &NativeTextFundingV29 {
        &self.funded
    }
    pub fn claim(&self) -> Option<&NativeTextClaimV29> {
        self.claim.as_ref()
    }
    pub fn terminal(&self) -> NativeTextTerminalV29 {
        self.terminal
    }
    pub fn escrow(&self) -> u64 {
        self.escrow
    }
    pub fn settlement(&self) -> Option<&NativeTextSettlementV29> {
        self.settlement.as_ref()
    }
    pub(crate) fn from_native_funding(checked: &CheckedNativeTextMaximumV29, funded: NativeTextFundingV29) -> Result<Self, String> {
        if funded.maximum != checked.record
            || funded.request.root() != funded.maximum.request
            || funded.consent == [0; 64]
            || funded.escrow_debit == [0; 64]
            || funded.maximum_reservation == [0; 64]
            || funded.original_shared_subject == [0; 64]
            || funded.commit_deadline
                != funded.admitted_daa.checked_add(funded.maximum.commit_window).ok_or("text29 commit clock overflow")?
            || funded.deadline
                != funded.admitted_daa.checked_add(funded.maximum.complete_job_horizon).ok_or("text29 liability overflow")?
        {
            return Err("text29 native original funded receipt differs".into());
        }
        let reward = funded.request.reward;
        Ok(Self { funded, claim: None, terminal: NativeTextTerminalV29::Funded, escrow: reward, settlement: None })
    }
    pub fn restore(bytes: &[u8], original_funding: Digest, expected_book: Digest) -> Result<Self, String> {
        if bytes.len() > TEXT_STOP_BOOK_BYTES_V29 {
            return Err("text29 fixed book cap".into());
        }
        let b: Self = borsh::from_slice(bytes).map_err(|_| "text29 bounded book grammar")?;
        if b.root() != expected_book
            || b.funded.root() != original_funding
            || b.funded.request.root() != b.funded.maximum.request
            || matches!(b.terminal, NativeTextTerminalV29::Funded | NativeTextTerminalV29::Challengeable) != (b.settlement.is_none())
            || b.escrow != if b.settlement.is_none() { b.funded.request.reward } else { 0 }
            || b.claim.is_some() != (b.terminal != NativeTextTerminalV29::Funded && b.terminal != NativeTextTerminalV29::Refunded)
        {
            return Err("text29 rooted original funding/book/conservation differs".into());
        }
        if let Some(s) = &b.settlement {
            if s.job != b.funded.id()
                || s.original_funding != b.funded.root()
                || s.terminal != b.terminal
                || s.claim != b.claim.as_ref().map(NativeTextClaimV29::root)
                || s.escrow_debit != b.funded.request.reward
                || s.producer.checked_add(s.requester) != Some(s.escrow_debit)
                || s.finder_credit.checked_add(s.burned) != Some(s.actual_collected_slash)
                || s.actual_collected_slash > b.funded.maximum.producer_reserve
                || (b.terminal == NativeTextTerminalV29::Final
                    && (s.producer != s.escrow_debit || s.requester != 0 || s.actual_collected_slash != 0))
                || (b.terminal == NativeTextTerminalV29::Refunded
                    && (s.requester != s.escrow_debit || s.producer != 0 || s.actual_collected_slash != 0))
                || (b.terminal == NativeTextTerminalV29::Convicted
                    && (s.requester != s.escrow_debit
                        || s.producer != 0
                        || s.actual_collected_slash != b.funded.maximum.producer_reserve
                        || s.finder_credit != b.funded.maximum.finder_award
                        || s.proof == [0; 64]))
            {
                return Err("text29 restored terminal original principal and collected collateral differ".into());
            }
        }
        Ok(b)
    }
    pub(crate) fn commit(
        &mut self,
        actual: &CheckedNativeTextActualV29,
        producer: Digest,
        vector: Digest,
        trace: Digest,
        final_state: Digest,
        now: u64,
        segment_roots: &[Digest],
        declared_claim_root: Digest,
    ) -> Result<(), String> {
        let f = &self.funded;
        if self.terminal != NativeTextTerminalV29::Funded
            || self.claim.is_some()
            || actual.request != f.request.root()
            || producer != f.request.producer
            || now < f.admitted_daa
            || now > f.commit_deadline
            || vector == [0; 64]
            || trace == [0; 64]
            || final_state == [0; 64]
            || segment_roots.len() != actual.positions.div_ceil(SEG_LEN_V4) as usize
            || claim_root_v1(actual.positions, segment_roots) != declared_claim_root
        {
            return Err("text29 actual claim is duplicate/late/borrowed or directory extent differs".into());
        }
        let challenge_end = now.checked_add(f.maximum.challenge_window).ok_or("text29 challenge overflow")?;
        let retained_until = challenge_end.checked_add(f.maximum.retention_tail).ok_or("text29 retention overflow")?;
        if retained_until >= f.deadline {
            return Err("text29 claim exceeds original paid maximum horizon".into());
        }
        self.claim = Some(NativeTextClaimV29 {
            job: f.id(),
            producer,
            vector,
            trace,
            final_state,
            actual_positions: actual.positions,
            actual_generated: actual.count,
            generated_root: actual.generated_root,
            actual_scope25: actual.scope,
            actual_resource29: actual.resource,
            stop: actual.stop,
            committed_daa: now,
            challenge_end,
            retained_until,
        });
        self.terminal = NativeTextTerminalV29::Challengeable;
        Ok(())
    }
    fn settle(&mut self, s: NativeTextSettlementV29) -> Result<NativeTextSettlementV29, String> {
        if self.settlement.is_some()
            || self.escrow != self.funded.request.reward
            || s.job != self.funded.id()
            || s.original_funding != self.funded.root()
            || s.escrow_debit != self.escrow
            || s.producer.checked_add(s.requester) != Some(s.escrow_debit)
            || s.finder_credit.checked_add(s.burned) != Some(s.actual_collected_slash)
            || s.claim != self.claim.as_ref().map(NativeTextClaimV29::root)
        {
            return Err("text29 actual original principal/slash credit is already spent or rebound".into());
        }
        self.escrow = 0;
        self.terminal = s.terminal;
        self.settlement = Some(s.clone());
        Ok(s)
    }
    pub(crate) fn cancel(&mut self, now: u64) -> Result<NativeTextSettlementV29, String> {
        if self.terminal != NativeTextTerminalV29::Funded || now <= self.funded.deadline {
            return Err("text29 cannot cancel committed/immature job".into());
        }
        self.settle(NativeTextSettlementV29 {
            job: self.funded.id(),
            original_funding: self.funded.root(),
            claim: None,
            terminal: NativeTextTerminalV29::Refunded,
            escrow_debit: self.escrow,
            producer: 0,
            requester: self.escrow,
            actual_collected_slash: 0,
            finder: [0; 64],
            finder_credit: 0,
            burned: 0,
            proof: [0; 64],
        })
    }
    /// Native independently verifies the exact original paid proof/default,
    /// current finder operator and ACTUAL collectible producer collateral first.
    pub(crate) fn convict(
        &mut self,
        proof: Digest,
        finder: Digest,
        finder_operator: Digest,
        actual_collected: u64,
        now: u64,
    ) -> Result<NativeTextSettlementV29, String> {
        let c = self.claim.as_ref().ok_or("text29 conviction has no original claim")?;
        if self.terminal != NativeTextTerminalV29::Challengeable
            || now < c.committed_daa
            || now >= self.funded.deadline
            || proof == [0; 64]
            || finder == self.funded.request.producer
            || finder_operator == self.funded.request.producer_operator
            || actual_collected != self.funded.maximum.producer_reserve
            || self.funded.maximum.finder_award > actual_collected
        {
            return Err("text29 original independent court/default and actual collateral differ".into());
        }
        self.settle(NativeTextSettlementV29 {
            job: self.funded.id(),
            original_funding: self.funded.root(),
            claim: Some(c.root()),
            terminal: NativeTextTerminalV29::Convicted,
            escrow_debit: self.escrow,
            producer: 0,
            requester: self.escrow,
            actual_collected_slash: actual_collected,
            finder,
            finder_credit: self.funded.maximum.finder_award,
            burned: actual_collected - self.funded.maximum.finder_award,
            proof,
        })
    }
    pub(crate) fn finalize(
        &mut self,
        conditions: &CheckedNativeFinalConditionsV27,
        closed: &CheckedNativeTextCloseV29,
    ) -> Result<NativeTextSettlementV29, String> {
        let c = self.claim.as_ref().ok_or("text29 has no committed claim")?;
        if self.terminal != NativeTextTerminalV29::Challengeable
            || closed.job != self.funded.id()
            || closed.claim != c.root()
            || closed.book != self.root()
            || closed.original_funding != self.funded.root()
            || closed.now <= c.retained_until
            || closed.final_policy != self.funded.maximum.final_policy27
            || closed.final_basis != self.funded.maximum.final_basis27
            || conditions.policy_root() != closed.final_policy
            || conditions.basis_root() != closed.final_basis
        {
            return Err("text29 Final does not close authenticated original resources/claims/pending liability".into());
        }
        self.settle(NativeTextSettlementV29 {
            job: self.funded.id(),
            original_funding: self.funded.root(),
            claim: Some(c.root()),
            terminal: NativeTextTerminalV29::Final,
            escrow_debit: self.escrow,
            producer: self.escrow,
            requester: 0,
            actual_collected_slash: 0,
            finder: [0; 64],
            finder_credit: 0,
            burned: 0,
            proof: object_id(b"misaka-palw/native/text-original-final-conditions-receipt/v29", &(conditions.policy_root(),conditions.basis_root())),
        })
    }
}
/// Constructed by the native fold only after authenticating fixed source/trace,
/// discovery/proof/capacity rows and collecting closed originals. No carrier
/// supplies pending counts or an alleged source-equivalence boolean.
pub(crate) struct CheckedNativeTextCloseV29 {
    pub(crate) job: Digest,
    pub(crate) claim: Digest,
    pub(crate) book: Digest,
    pub(crate) original_funding: Digest,
    pub(crate) now: u64,
    pub(crate) final_policy: Digest,
    pub(crate) final_basis: Digest,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> NativeTextRequestV29 {
        NativeTextRequestV29 {
            version: 29,
            class: [1; 64],
            requester: [2; 64],
            requester_operator: [3; 64],
            producer: [4; 64],
            producer_operator: [5; 64],
            nonce: [0; 64],
            prompt_root: [6; 64],
            prompt_len: 24,
            maximum_new_tokens: 10,
            eos_token: 9,
            reward: 20,
        }
    }
    #[test]
    fn first_eos_or_original_maximum_never_credits_truncated_or_continued_generation() {
        let r = request();
        assert_eq!(r.maximum_positions().unwrap(), 33);
        assert_eq!(check_generated_v29(&r, 10, &[1, 9]).unwrap(), NativeTextStopV29::FirstEos);
        assert_eq!(check_generated_v29(&r, 10, &[1; 10]).unwrap(), NativeTextStopV29::Maximum);
        assert_eq!(check_generated_v29(&r, 10, &[9]).unwrap(), NativeTextStopV29::FirstEos);
        for bad in [vec![], vec![1], vec![9, 1], vec![1, 9, 9], vec![10, 9], vec![1; 11]] {
            assert!(check_generated_v29(&r, 10, &bad).is_err());
        }
    }
}

/// Open canonical registration; no model name, signer allowlist, compiler id or
/// trusted registry membership is part of class eligibility. The native source
/// binding authenticates these ORIGINAL bytes/PC/R and all source components.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTextClassV29 {
    pub version: u16,
    pub source_binding: Digest,
    pub original_program: Digest,
    pub real_parameter: Digest,
    pub inventory: Digest,
    pub original_source_components: Digest,
    pub maximum_positions: u32,
    pub court_profile25: Digest,
    pub maximum_scope25: Digest,
    pub maximum_resource29: Digest,
}
impl NativeTextClassV29 {
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/native/full-source-text-class/v29", self)
    }
}
/// Created only in the native fold after exact initial producer authorization,
/// canonical principal nonce, caller-paid derivation, source and shared maximum
/// reservations are authenticated in this current chain epoch. A public shape
/// derivation alone cannot authorize QT/liability or reserve another producer.
pub struct CheckedNativeTextAdmissionV29 {
    block: Digest,
    daa: u64,
    maximum: Digest,
    request: Digest,
    funded: Digest,
    producer: Digest,
    producer_operator: Digest,
    requester_operator: Digest,
    nonce_counter: Digest,
    original_consent: Digest,
    original_fee: Digest,
}
impl CheckedNativeTextAdmissionV29 {
    pub fn block(&self) -> Digest {
        self.block
    }
    pub fn daa(&self) -> u64 {
        self.daa
    }
    pub fn producer(&self) -> Digest {
        self.producer
    }
    pub fn producer_operator(&self) -> Digest {
        self.producer_operator
    }
    pub fn requester_operator(&self) -> Digest {
        self.requester_operator
    }
    pub fn maximum_root(&self) -> Digest {
        self.maximum
    }
    pub fn request_root(&self) -> Digest {
        self.request
    }
    pub fn funding_root(&self) -> Digest {
        self.funded
    }
    pub fn check_current(
        &self,
        block: Digest,
        daa: u64,
        maximum: &CheckedNativeTextMaximumV29,
        funded: &NativeTextFundingV29,
    ) -> Result<(), String> {
        if self.block != block
            || self.daa != daa
            || self.maximum != maximum.record().root()
            || self.maximum != funded.maximum.root()
            || self.request != funded.request.root()
            || self.funded != funded.root()
            || self.producer != funded.request.producer
            || self.producer_operator != funded.request.producer_operator
            || self.requester_operator != funded.request.requester_operator
            || funded.admitted_daa != daa
            || self.original_consent != funded.consent
            || self.nonce_counter == [0; 64]
            || self.original_fee == [0; 64]
        {
            return Err("text29 current native admission/consent/principal nonce/paid maximum differs".into());
        }
        Ok(())
    }
    pub(crate) fn from_current_native(
        block: Digest,
        daa: u64,
        maximum: &CheckedNativeTextMaximumV29,
        funded: &NativeTextFundingV29,
        nonce_counter: Digest,
        original_fee: Digest,
    ) -> Result<Self, String> {
        if block == [0; 64]
            || nonce_counter == [0; 64]
            || original_fee == [0; 64]
            || funded.admitted_daa != daa
            || funded.maximum != *maximum.record()
            || funded.maximum.request != funded.request.root()
            || funded.consent == [0; 64]
        {
            return Err("text29 lacks genuine current native Fund admission".into());
        }
        Ok(Self {
            block,
            daa,
            maximum: maximum.record().root(),
            request: funded.request.root(),
            funded: funded.root(),
            producer: funded.request.producer,
            producer_operator: funded.request.producer_operator,
            requester_operator: funded.request.requester_operator,
            nonce_counter,
            original_consent: funded.consent,
            original_fee,
        })
    }
}

pub const TEXT_REGISTER_ACTION_V29: u8 = 27;
pub const TEXT_PREPARE_ACTION_V29: u8 = 28;
pub const TEXT_ACCEPT_ACTION_V29: u8 = 29;
pub const TEXT_FUND_ACTION_V29: u8 = 30;
pub const TEXT_COMMIT_ACTION_V29: u8 = 31;
pub const TEXT_PAGE_ACTION_V29: u8 = 32;
pub const TEXT_OWN_ACTION_V29: u8 = 33;
pub const TEXT_REVEAL_ACTION_V29: u8 = 34;
pub const TEXT_CLOSE_ACTION_V29: u8 = 35;
pub const TEXT_REQUEST_ACTION_V29: u8 = 36;
pub const TEXT_ANSWER_ACTION_V29: u8 = 37;
pub const TEXT_DEFAULT_ACTION_V29: u8 = 38;
pub const TEXT_DISCOVERY_ACTION_V29: u8 = 39;
pub const TEXT_COLLECT_ACTION_V29: u8 = 40;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTextRegisterV29 {
    pub version: u16,
    pub source_binding: Digest,
    pub original_source_components: Digest,
    pub maximum_positions: u32,
    pub program_bytes: Vec<u8>,
    pub real_parameters: ParamCommitmentsV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTextPrepareV29 {
    pub version: u16,
    pub request: NativeTextRequestV29,
    pub prompt: Vec<u32>,
}
/// Initial producer signature covers the independently derived preparation,
/// every maximum/source/clock/fee/queue/Final root and finite expiry.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTextConsentV29 {
    pub version: u16,
    pub preparation: Digest,
    pub request: Digest,
    pub maximum: NativeTextMaximumV29,
    pub producer: Digest,
    pub producer_operator: Digest,
    pub requester: Digest,
    pub requester_operator: Digest,
    pub nonce: Digest,
    pub admitted_interval: u64,
    pub accepted_daa: u64,
    pub expires_daa: u64,
    pub public_protocol: Digest,
    pub queue_policy: Digest,
    pub queue_plan: Digest,
}
impl NativeTextConsentV29 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-full-text-consent/v29", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTextFundV29 {
    pub version: u16,
    pub preparation: Digest,
    pub initial_consent: Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTextPostV29 {
    pub version: u16,
    pub job: Digest,
    pub original_funding: Digest,
    pub maximum_contract: Digest,
    pub requester_nonce: Digest,
    pub generated: Vec<u32>,
    pub actual_scope25: Digest,
    pub actual_resource29: Digest,
    pub evidence: misaka_palw_kernel::seg::SegmentedEvidenceV2,
    pub segment_roots: Vec<Digest>,
    pub final_state: Digest,
}
impl NativeTextPostV29 {
    pub fn trace_root(&self) -> Digest {
        object_id(b"misaka-palw/native/exact-stopped-public-trace/v29", self)
    }
    pub fn vector(&self) -> Digest {
        object_id(b"misaka-palw/native/exact-stopped-vector/v29", &(29u16, self.job, self.trace_root()))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeTextOwnV29 {
    pub version: u16,
    pub job: Digest,
    pub nonce: Digest,
    pub proof_root: Digest,
    pub seal_root: Digest,
    pub filing_bytes: u32,
    pub discovery: bool,
}

/// Standalone causal text source namespace; mixed decoder components use the
/// distinct typed Source28 pipeline adapter and never substitute input PC here.
pub fn native_text_source_components_v29(binding: Digest, program: Digest, real_pc: Digest, inventory: Digest) -> Digest {
    object_id(b"misaka-palw/native/complete-causal-text-source-components/v29", &(29u16, binding, program, real_pc, inventory))
}

// Read-only native allocation inventory; no state/root/codec field is added.
#[path = "palw_native_resource_v1/private/palw_native_text_stop_v29.rs"]
mod native_resource_inventory_v1;
