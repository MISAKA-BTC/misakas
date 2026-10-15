//! Authenticated finite round rows. Source finances retain the original V5
//! native debit/reserve/refund contract; a staged message creates a new target.
use crate::palw_claim_challenge_v2::*;
use crate::palw_kernel_route_v1::PalwKernelRouteStateV1;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::claim_interactive_v3::*;
use misaka_palw_kernel::claim_paid_v2::*;
use misaka_palw_kernel::hash::Digest;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPaidClaimRoundV3 {
    pub enrollment: Digest,
    pub round: u32,
    pub prior_transcript: Digest,
    pub message_root: Digest,
    pub accepted_daa: u64,
    pub subject: misaka_palw_challenge::ChallengeSubjectV1,
    pub context: misaka_palw_challenge::BeaconContextV1,
    pub target: misaka_palw_kernel::beacon_source::BeaconSourceTargetV1,
    pub sponsor: Option<crate::palw_state_v2::PalwBondKeyV2>,
    pub lock: Option<PalwPaidClaimLockV2>,
    pub transcript_root: Digest,
    pub receipts: Vec<PaidClaimReceiptV2>,
}
impl PalwPaidClaimRoundV3 {
    pub fn id(&self) -> Digest {
        round_id_v3(self.enrollment, self.round)
    }
    pub fn validate(&self, a: &PalwPaidClaimAuthorityV2, l: &misaka_palw_kernel::ledger::KernelLedgerV1) -> Result<(), String> {
        let plan = a.class.interactive_plan.as_ref().ok_or("round has no signed compiled plan")?;
        a.validate(l)?;
        if self.enrollment != a.enrollment.id()
            || self.round >= plan.rounds
            || self.message_root == [0; 64]
            || (self.round == 0) != (self.prior_transcript == [0; 64])
            || !a.enrollment.lifetime.receipt_open(self.accepted_daa)
            || self.receipts.len() > crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_INTERIM_SEATS_V1
        {
            return Err("round identity/message/order/clock/capacity mismatch".into());
        }
        let (subject, context, target) = if plan.mode == misaka_palw_challenge::InteractiveModeV1::StagedBeacon {
            let subject = round_subject_v3(&a.subject, plan, self.enrollment, self.round, self.prior_transcript, self.message_root);
            let mut context = a.context.clone();
            context.commitment_root = subject.commitment_root;
            context.commitment_position = self.accepted_daa;
            context.challenge_epoch = u64::from(self.round);
            let target = claim_source_target_v2(a.source_policy, context.clone(), a.enrollment.lifetime, &l.policy)?;
            if target.terms.seal_start <= self.accepted_daa {
                return Err("staged source must open strictly after its message".into());
            }
            (subject, context, target)
        } else {
            (a.subject.clone(), a.context.clone(), a.source_target.clone())
        };
        if self.subject != subject || self.context != context || self.target != target {
            return Err("round replaces the committed message/future source authority".into());
        }
        if let Some(lock) = &self.lock {
            if lock.enrollment != self.enrollment
                || lock.context != self.context.id()
                || lock.quorum == 0
                || lock.seats.len() > crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_INTERIM_SEATS_V1
                || lock.seats.len() < lock.quorum as usize
                || lock.locked_daa < self.accepted_daa
                || !a.enrollment.lifetime.receipt_open(lock.locked_daa)
                || self.transcript_root
                    != transcript_root_v3(
                        plan,
                        self.enrollment,
                        self.round,
                        self.prior_transcript,
                        self.message_root,
                        lock.context,
                        lock.seed,
                    )
            {
                return Err("round lock/transcript/lifetime mismatch".into());
            }
            let mut unique = std::collections::BTreeSet::new();
            for receipt in &self.receipts {
                if !receipt.complete
                    || receipt.version != 2
                    || receipt.round != self.round
                    || receipt.message_root != self.message_root
                    || receipt.enrollment != self.enrollment
                    || receipt.assignment != lock.assignment
                    || receipt.source_context != lock.context
                    || receipt.sample_seed != lock.seed
                    || receipt.transcript_root != self.transcript_root
                    || receipt.evidence_root != a.enrollment.commit_root
                    || receipt.verification_class != a.class.id()
                    || receipt.signed_daa < lock.locked_daa
                    || !a.enrollment.lifetime.receipt_open(receipt.signed_daa)
                    || !lock.seats.iter().any(|s| crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(s) == receipt.signer)
                    || !unique.insert(receipt.signer)
                {
                    return Err("round receipt authority/duplicate/full-opening mismatch".into());
                }
            }
        } else if !self.receipts.is_empty() || self.transcript_root != [0; 64] {
            return Err("unlocked round cannot carry success".into());
        }
        Ok(())
    }
    pub fn complete(&self) -> bool {
        self.lock.as_ref().is_some_and(|l| l.source_valid && self.receipts.len() >= l.quorum as usize)
    }
}
/// Common authenticated source adapter for initial and staged contexts. Never
/// infer sponsor consent or budget from a producer's nomination.
pub struct PalwPaidClaimSourceAuthorityV3 {
    pub context: misaka_palw_challenge::BeaconContextV1,
    pub source_target: misaka_palw_kernel::beacon_source::BeaconSourceTargetV1,
    pub source_policy: crate::palw_beacon_source_opt_in_v1::PalwBeaconSourcePolicyV1,
    pub authorized_daa: u64,
    pub sponsor: Option<crate::palw_state_v2::PalwBondKeyV2>,
    pub base_claim: Digest,
}
impl PalwKernelRouteStateV1 {
    pub fn paid_claim_round_v3(&self, enrollment: &Digest, round: u32) -> Result<PalwPaidClaimRoundV3, String> {
        let row: PalwPaidClaimRoundV3 = self
            .aux_row(PAID_CLAIM_ROUNDS_V2, &borsh::to_vec(&round_id_v3(*enrollment, round)).unwrap())
            .ok_or("paid round missing")?;
        if row.enrollment != *enrollment || row.round != round {
            return Err("paid round key/identity mismatch".into());
        }
        let a = self.paid_claim_enrollment_v3(enrollment)?;
        row.validate(&a, &*self.ledger()?)?;
        Ok(row)
    }
    pub fn paid_claim_enrollment_v3(&self, enrollment: &Digest) -> Result<PalwPaidClaimAuthorityV2, String> {
        let index: PalwPaidClaimIndexV3 = self
            .aux_row(PAID_CLAIM_ENROLLMENTS_V2, &paid_claim_index_key_v3(PAID_CLAIM_ID_INDEX_PREFIX_V3, enrollment))
            .ok_or("paid enrollment index missing")?;
        if index.version != 3 || index.kind != PalwPaidClaimIndexKindV3::Enrollment || index.owner != *enrollment {
            return Err("paid enrollment index substituted".into());
        }
        let a: PalwPaidClaimAuthorityV2 = self
            .aux_row(PAID_CLAIM_ENROLLMENTS_V2, &borsh::to_vec(&index.target).unwrap())
            .ok_or("paid enrollment index target missing")?;
        if a.enrollment.id() != *enrollment || a.context.id() != index.target {
            return Err("paid enrollment index does not bind its primary authority".into());
        }
        Ok(a)
    }

    pub fn paid_claim_source_authority_v3(&self, context: &Digest) -> Result<PalwPaidClaimSourceAuthorityV3, String> {
        if let Some(a) = self.aux_row::<PalwPaidClaimAuthorityV2>(PAID_CLAIM_ENROLLMENTS_V2, &borsh::to_vec(context).unwrap()) {
            a.validate(&*self.ledger()?)?;
            if a.class.source_policy.interactive_mode == misaka_palw_challenge::InteractiveModeV1::StagedBeacon {
                return Err("staged source needs a producer message and fresh round target".into());
            }
            if a.context.id() != *context || self.beacon_source_target_v4(context)? != a.source_target {
                return Err("initial source target mismatch".into());
            }
            return Ok(PalwPaidClaimSourceAuthorityV3 {
                context: a.context,
                source_target: a.source_target,
                source_policy: a.source_policy,
                authorized_daa: a.authorized_daa,
                sponsor: a.sponsor,
                base_claim: a.enrollment.base_claim,
            });
        }
        let index: PalwPaidClaimIndexV3 = self
            .aux_row(PAID_CLAIM_ROUNDS_V2, &paid_claim_index_key_v3(PAID_CLAIM_ID_INDEX_PREFIX_V3, context))
            .ok_or("paid staged context index missing")?;
        if index.version != 3 || index.kind != PalwPaidClaimIndexKindV3::StagedContext || index.owner != *context {
            return Err("paid staged context index invalid".into());
        }
        let row: PalwPaidClaimRoundV3 =
            self.aux_row(PAID_CLAIM_ROUNDS_V2, &borsh::to_vec(&index.target).unwrap()).ok_or("staged source index target missing")?;
        let a = self.paid_claim_enrollment_v3(&row.enrollment)?;
        row.validate(&a, &*self.ledger()?)?;
        if row.id() != index.target || row.context.id() != *context || self.beacon_source_target_v4(context)? != row.target {
            return Err("staged source index substitutes its round/context/target".into());
        }
        Ok(PalwPaidClaimSourceAuthorityV3 {
            context: row.context,
            source_target: row.target,
            source_policy: a.source_policy,
            authorized_daa: row.accepted_daa,
            sponsor: row.sponsor,
            base_claim: a.enrollment.base_claim,
        })
    }

    pub fn paid_claim_round_check_authority_v3(
        &self,
        enrollment: &Digest,
        round: u32,
        now: u64,
    ) -> Result<PaidClaimCheckAuthorityV2, String> {
        let a = self.paid_claim_enrollment_v3(enrollment)?;
        let row = self.paid_claim_round_v3(enrollment, round)?;
        let (contract, sources, promises) =
            self.paid_beacon_material_v5(&row.context.id())?.ok_or("paid round source funding missing")?;
        let misaka_palw_challenge::SealedBeaconStateV3::Locked(beacon) =
            misaka_palw_challenge::collect_paid_opt_in_work_beacon_v5(&row.context, &contract, &sources, &promises, now)
                .map_err(|e| e.to_string())?
        else {
            return Err("round paid source not locked".into());
        };
        let mut prior_rounds = Vec::new();
        if a.class.source_policy.interactive_mode == misaka_palw_challenge::InteractiveModeV1::TranscriptBoundFiatShamir {
            for previous in 0..round {
                let p = self.paid_claim_round_v3(enrollment, previous)?;
                let lock = p.lock.ok_or("previous FS challenge missing")?;
                prior_rounds.push((p.message_root, lock.seed));
            }
        }
        PaidClaimCheckAuthorityV2::for_interactive_round_v3(
            &a.class,
            &a.enrollment,
            &row.context,
            &row.subject,
            &beacon,
            round,
            row.message_root,
            row.prior_transcript,
            row.accepted_daa,
            now,
            &prior_rounds,
        )
    }
    /// Pure aux projection, used while loading the V1 ledger; never recurse into
    /// a source collector or into ledger(). Every round must be complete.
    pub fn paid_claim_interactive_complete_v3(
        &self,
        a: &PalwPaidClaimAuthorityV2,
        l: &misaka_palw_kernel::ledger::KernelLedgerV1,
    ) -> Result<bool, String> {
        let plan = a.class.interactive_plan.as_ref().ok_or("interactive plan missing")?;
        let mut prior = [0; 64];
        for round in 0..plan.rounds {
            let Some(row) = self.aux_row::<PalwPaidClaimRoundV3>(
                PAID_CLAIM_ROUNDS_V2,
                &borsh::to_vec(&round_id_v3(a.enrollment.id(), round)).unwrap(),
            ) else {
                return Ok(false);
            };
            if row.enrollment != a.enrollment.id() || row.round != round {
                return Err("paid round projection key/identity mismatch".into());
            }
            row.validate(a, l)?;
            if row.prior_transcript != prior || !row.complete() {
                return Ok(false);
            }
            prior = row.transcript_root;
        }
        Ok(true)
    }
}
