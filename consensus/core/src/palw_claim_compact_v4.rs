//! Versioned native compact admission. Never armed by a shipped preset.
//! V3 full-material policies do not authorize this contract or its new proof binding.
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::{Digest, object_id};

pub const PAID_CLAIM_COMPACT_V4: u8 = 135;
pub const COMPACT_PROOF_BINDING_PREFIX_V4: u8 = 0x50;
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClaimCompactPolicyV4 {
    pub max_contracts: u32,
    pub maximum_opening_bytes: u64,
    pub maximum_proof_bytes: u64,
    pub maximum_verifier_work: u128,
    pub maximum_stream_words: u64,
    pub require_independent_review: bool,
}
impl PalwClaimCompactPolicyV4 {
    pub fn id(&self) -> Digest {
        object_id(b"MISAKA/PALW/CLAIM-COMPACT-POLICY/V4", self)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.max_contracts == 0
            || self.max_contracts > 65_536
            || self.maximum_opening_bytes == 0
            || self.maximum_opening_bytes > 64 * 1024 * 1024
            || self.maximum_proof_bytes == 0
            || self.maximum_proof_bytes > 64 * 1024 * 1024
            || self.maximum_verifier_work == 0
            || self.maximum_stream_words == 0
        {
            return Err("compact V4 needs finite named native capacities".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClaimCompactFenceV4 {
    pub activation: ForkActivation,
    pub policy: PalwClaimCompactPolicyV4,
}
impl Params {
    pub fn validate_palw_claim_compact_v4(&self) -> Result<(), PalwModeV2Error> {
        if let Some(f) = self.palw_claim_compact_v4 {
            f.policy.validate().map_err(PalwModeV2Error::InvalidOwned)?;
            if f.activation != ForkActivation::never() {
                return Err(PalwModeV2Error::InvalidOwned("compact V4 remains dormant pending native acceptance, independent cumulative soundness/source and economic review".into()));
            }
        }
        Ok(())
    }
    /// Explicitly separate from V3 synchronization: old private profiles keep
    /// their exact semantics and fingerprint until a new profile opts in.
    pub fn sync_palw_claim_compact_v4(&mut self) {
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_claim_compact_v4(self.palw_claim_compact_v4);
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPaidCompactContractV4 {
    pub registration: misaka_palw_kernel::claim_compact_v1::native_v4::CompactClaimRegistrationV4,
    pub registered_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPaidCompactProofBindingV4 {
    pub version: u16,
    pub enrollment: Digest,
    pub round: u32,
    pub plan: Digest,
    pub seed: Digest,
    pub proof_root: Digest,
    /// Canonical proof bytes are retained in authenticated native carriage;
    /// no pre-seed provider consent is silently extended to new transcript data.
    pub proof: misaka_palw_kernel::claim_compact_v1::native_v4::NativeCompactProofV4,
    pub producer: Digest,
    pub bound_daa: u64,
}
pub fn compact_proof_binding_key_v4(enrollment: Digest, round: u32) -> Vec<u8> {
    let id = misaka_palw_kernel::claim_interactive_v3::round_id_v3(enrollment, round);
    [vec![COMPACT_PROOF_BINDING_PREFIX_V4], id.to_vec()].concat()
}

impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    pub fn paid_compact_contract_v4(&self, plan: &Digest) -> Result<PalwPaidCompactContractV4, String> {
        let row: PalwPaidCompactContractV4 =
            self.aux_row(PAID_CLAIM_COMPACT_V4, plan.as_ref()).ok_or("native compact registration missing")?;
        if row.registration.id() != *plan {
            return Err("native compact registration key mismatch".into());
        }
        Ok(row)
    }
    pub fn paid_compact_proof_binding_v4(
        &self,
        enrollment: Digest,
        round: u32,
    ) -> Result<Option<PalwPaidCompactProofBindingV4>, String> {
        let Some(bytes) = self.aux.get(&(PAID_CLAIM_COMPACT_V4, compact_proof_binding_key_v4(enrollment, round))) else {
            return Ok(None);
        };
        let row: PalwPaidCompactProofBindingV4 = borsh::from_slice(bytes).map_err(|e| e.to_string())?;
        if row.version != 4
            || row.enrollment != enrollment
            || row.round != round
            || row.proof_root == [0; 64]
            || row.proof_root != row.proof.root()
        {
            return Err("native compact proof binding key/version/root mismatch".into());
        }
        Ok(Some(row))
    }
    /// Structural row verification never draws entropy. Exact post-seed proof
    /// roots are authenticated from aux135 rather than supplied by a caller.
    pub fn validate_paid_round_v4(
        &self,
        row: &crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3,
        a: &crate::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2,
        l: &misaka_palw_kernel::ledger::KernelLedgerV1,
    ) -> Result<(), String> {
        let binding = self.paid_compact_proof_binding_v4(row.enrollment, row.round)?;
        let plan = a.class.interactive_plan.as_ref().ok_or("round plan missing")?;
        if plan.version != 4 {
            if binding.is_some() {
                return Err("V3 round cannot have a V4 proof binding".into());
            }
            return row.validate(a, l);
        }
        let contract = self.paid_compact_contract_v4(&plan.id())?;
        contract.registration.validate(l)?;
        if contract.registration.plan != *plan || contract.registration.contract.base_class != a.class.base_class {
            return Err("paid class substitutes registered compact plan/base class".into());
        }
        if let Some(binding) = &binding {
            let lock = row.lock.as_ref().ok_or("compact proof bound before future source lock")?;
            binding.proof.validate_shape(
                plan.id(),
                lock.seed,
                u64::try_from(contract.registration.contract.maximum_proof_bytes)
                    .map_err(|_| "compact proof cap not representable")?,
            )?;
            if binding.plan != plan.id()
                || binding.seed != lock.seed
                || binding.producer != a.enrollment.producer
                || binding.bound_daa < lock.locked_daa
                || !a.enrollment.lifetime.receipt_open(binding.bound_daa)
            {
                return Err("compact proof binding replaces source/producer/lifetime".into());
            }
        }
        row.validate_with_compact_v4(a, l, binding.map(|b| b.proof_root))
    }
}

impl crate::palw_state_v2::PalwChainStateV2 {
    pub fn assert_paid_compact_consistency_v4(
        &self,
        params: &crate::palw_state_v2::PalwStateParamsV2,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        let refuse = |why: String| crate::palw_state_v2::PalwStateV2Error::CarriageInconsistent(format!("compact V4: {why}"));
        let Some(route) = self.kernel_route() else { return Ok(()) };
        let mut rows = route.aux.range((PAID_CLAIM_COMPACT_V4, Vec::new())..(PAID_CLAIM_COMPACT_V4 + 1, Vec::new())).peekable();
        if rows.peek().is_none() {
            return Ok(());
        }
        let now = self.last_point().map_or(0, |p| p.daa_score);
        let policy = params.claim_compact_v4_at(now).ok_or_else(|| refuse("rows without active compiled V4 capability".into()))?;
        policy.validate().map_err(&refuse)?;
        let ledger = route.ledger().map_err(&refuse)?;
        let mut contracts = 0u32;
        for ((_, key), bytes) in rows {
            if key.len() == 64 {
                let row: PalwPaidCompactContractV4 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                let registration = &row.registration;
                if key.as_slice() != registration.id()
                    || row.registered_daa > now
                    || params.claim_compact_v4_at(row.registered_daa).is_none()
                    || registration.plan.maximum_material_bytes > policy.maximum_opening_bytes
                    || registration.contract.maximum_proof_bytes > u128::from(policy.maximum_proof_bytes)
                    || registration.contract.maximum_verifier_work > policy.maximum_verifier_work
                {
                    return Err(refuse("contract key/native capacity/timeline mismatch".into()));
                }
                registration.validate(&ledger).map_err(&refuse)?;
                contracts = contracts.checked_add(1).ok_or_else(|| refuse("contract count overflow".into()))?;
                if contracts > policy.max_contracts {
                    return Err(refuse("contract capacity exceeded".into()));
                }
            } else if key.len() == 65 && key[0] == COMPACT_PROOF_BINDING_PREFIX_V4 {
                let binding: PalwPaidCompactProofBindingV4 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                if *key != compact_proof_binding_key_v4(binding.enrollment, binding.round) || binding.bound_daa > now {
                    return Err(refuse("proof binding key/timeline mismatch".into()));
                }
                let a = route.paid_claim_enrollment_v3(&binding.enrollment).map_err(&refuse)?;
                let round: crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3 = route
                    .aux_row(
                        crate::palw_claim_challenge_v2::PAID_CLAIM_ROUNDS_V2,
                        misaka_palw_kernel::claim_interactive_v3::round_id_v3(binding.enrollment, binding.round).as_ref(),
                    )
                    .ok_or_else(|| refuse("orphan compact proof binding".into()))?;
                route.validate_paid_round_v4(&round, &a, &ledger).map_err(&refuse)?;
            } else {
                return Err(refuse("unknown compact namespace key".into()));
            }
        }
        Ok(())
    }
}
