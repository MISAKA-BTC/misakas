//! Shared immutable identity for a prospective native TFP1 and its
//! PublicOneHonest companion. The binding is available before either
//! signature, and does not authenticate an accepted carrier or reserve slots.

use super::{
    PreparedPublicOriginalV1, PublicOriginalAdmissionV1, PublicOriginalReservationV1, public_original_service_contract_id_v1,
};
use crate::{
    Hash64,
    palw_court_shared_tir_stream_v1::stream_digest,
    palw_fp_execution_v3::{palw_fp_job_pin_of_context_v1, palw_fp_job_pin_v1},
    palw_freeprompt_v3::{PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::{Digest, PrincipalId, principal},
    messages::AggregateBudget,
    public_one_honest::{OriginalCommit, SignedPublicOriginalV1},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeTfp1PublicOriginalFactsV1 {
    claim: Hash64,
    source: Hash64,
    job_context: Hash64,
    artifact: Hash64,
    execution_root: Hash64,
    output: Hash64,
    trace: Hash64,
    miner: PrincipalId,
}

impl NativeTfp1PublicOriginalFactsV1 {
    #[allow(dead_code)]
    pub(crate) fn native_identity_v1(&self) -> (Hash64, Hash64, Hash64) {
        (self.claim, self.source, self.execution_root)
    }

    pub(crate) fn derive_v1(original: &PalwFreePromptCommitmentV3, source: &PalwTirPublicSourceV1) -> Result<Self, ContractError> {
        let header = source.header_v1();
        if original.job.class_id != header.job_context.shape_profile_id
            || palw_fp_job_pin_v1(original) != palw_fp_job_pin_of_context_v1(&header.job_context)
            || original.execution_root != header.committed_execution_root
            || original.trace_root != header.full_logits_trace_root
            || original.work_leaves != source.leaf_count_v1()
            || original.decode_tokens_executed != source.decode_count_v1()
            || original.output_root == Hash64::default()
        {
            return Err(ContractError::Scope);
        }
        let key = original.job.executor_pubkey.as_slice().try_into().map_err(|_| ContractError::Identity)?;
        Ok(Self {
            claim: fp_claim_id_v3(original),
            source: source.source_id_v1(),
            job_context: header.job_context.context_hash(),
            artifact: header.artifact_root,
            execution_root: original.execution_root,
            output: original.output_root,
            trace: original.trace_root,
            miner: principal(key),
        })
    }

    /// Derive this only from the complete native relative graph. That graph
    /// is independent of the companion ID and both carriers' signatures.
    pub(crate) fn service_obligation_v1(
        self,
        profile: Hash64,
        resource: Hash64,
    ) -> Result<Tfp1PublicOriginalObligationV1, ContractError> {
        if profile == Hash64::default() || resource == Hash64::default() {
            return Err(ContractError::Scope);
        }
        let obligation = stream_digest(
            b"misaka-palw/tfp1-public-original-obligation/v1",
            &(
                self.claim,
                self.source,
                self.job_context,
                self.artifact,
                self.execution_root,
                self.output,
                self.trace,
                profile,
                resource,
            ),
        )
        .map_err(|_| ContractError::NonCanonical)?;
        Ok(Tfp1PublicOriginalObligationV1 { facts: self, obligation: Digest(obligation.as_bytes()) })
    }
}

/// The versioned obligation committed by the companion's signed
/// service_contract. This is separate from the TFP1 pledge's resource ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tfp1PublicOriginalObligationV1 {
    facts: NativeTfp1PublicOriginalFactsV1,
    obligation: Digest,
}

#[allow(dead_code)]
impl Tfp1PublicOriginalObligationV1 {
    pub(crate) fn facts_v1(&self) -> &NativeTfp1PublicOriginalFactsV1 {
        &self.facts
    }

    pub(crate) fn obligation_v1(self) -> Digest {
        self.obligation
    }

    pub(crate) fn service_contract_v1(self, budget: &AggregateBudget) -> Digest {
        public_original_service_contract_id_v1(self.obligation, budget)
    }

    pub(crate) fn check_companion_v1(self, companion: &OriginalCommit, budget: &AggregateBudget) -> Result<(), ContractError> {
        let facts = self.facts;
        if companion.execution != Digest(facts.job_context.as_bytes())
            || companion.output != Digest(facts.output.as_bytes())
            // The native Trace codec names the aggregate execution root;
            // its inner logits root remains bound by Source and obligation.
            || companion.trace_root != Digest(facts.execution_root.as_bytes())
            || companion.artifact != Digest(facts.artifact.as_bytes())
            || companion.miner != facts.miner
            || companion.service_contract != self.service_contract_v1(budget)
        {
            return Err(ContractError::Scope);
        }
        Ok(())
    }
}

/// Opaque verified companion and the TFP1 obligation checked in the same
/// call. A native link writer must accept this type only after resolving both
/// literal carriers and their actual accepted transaction.
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct PreparedTfp1PublicOriginalV1 {
    companion: PreparedPublicOriginalV1,
    obligation: Tfp1PublicOriginalObligationV1,
}

#[allow(dead_code)]
impl PreparedTfp1PublicOriginalV1 {
    pub(crate) fn companion_v1(&self) -> &PreparedPublicOriginalV1 {
        &self.companion
    }

    pub(crate) fn accepted_companion_v1(&self) -> super::AcceptedMaterial {
        self.companion.carrier
    }

    pub(crate) fn obligation_v1(&self) -> Tfp1PublicOriginalObligationV1 {
        self.obligation
    }
}

#[allow(dead_code)]
impl PublicOriginalAdmissionV1<'_> {
    /// Validate the unsigned companion against the exact immutable TFP1 and
    /// the same budget/obligation that will be verified after signatures.
    pub(crate) fn check_tfp1_candidate_v1(
        &self,
        companion: &OriginalCommit,
        obligation: Tfp1PublicOriginalObligationV1,
        reservation: &PublicOriginalReservationV1<'_>,
    ) -> Result<(), ContractError> {
        self.check_tfp1_link_v1(companion, obligation, reservation)?;
        self.check_original_metadata_v1(companion)
    }

    fn check_tfp1_link_v1(
        &self,
        companion: &OriginalCommit,
        obligation: Tfp1PublicOriginalObligationV1,
        reservation: &PublicOriginalReservationV1<'_>,
    ) -> Result<(), ContractError> {
        obligation.check_companion_v1(companion, reservation.budget)?;
        if self.job.context_hash() != obligation.facts.job_context
            || self.artifact != obligation.facts.artifact
            || self.miner != obligation.facts.miner
            || reservation.service_obligation != obligation.obligation
        {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    /// Verify an actual signed companion after the native TFP1 planner
    /// rederives the same obligation. The accepting owner must still bind
    /// both literal signed bodies and the real accepted transaction/event.
    pub(crate) fn prepare_tfp1_signed_bytes_v1(
        &self,
        bytes: &[u8],
        obligation: Tfp1PublicOriginalObligationV1,
        reservation: &PublicOriginalReservationV1<'_>,
    ) -> Result<PreparedTfp1PublicOriginalV1, ContractError> {
        let signed = SignedPublicOriginalV1::decode(bytes)?;
        self.check_tfp1_link_v1(&signed.original, obligation, reservation)?;
        let companion = self.prepare(&signed.original, &signed.key, &signed.signature, reservation)?;
        Ok(PreparedTfp1PublicOriginalV1 { companion, obligation })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_tir_public_claim_v1::palw_tir_public_signed_material_frame_fixture_v1;
    use misaka_palw_positive_contract::{
        identity::ChainScope,
        public_one_honest::{
            FirstNoticeDemand, ManifestScopeRules, ManifestV1, MaterialCaseDemand, MaterialScope, ORIGINAL_SIGN_CONTEXT,
            ProtectedRescueDemand,
        },
        reservations::RemainingCapacity,
        rows::{AcceptedOrigin, AggregateUsage, NativeHash64},
    };

    #[test]
    fn tfp1_public_original_pairs_exact_native_identity_before_and_after_signing() {
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([31; 32]);
        let key = *pair.verification_key.as_ref();
        let fixture = palw_tir_public_signed_material_frame_fixture_v1(&key);
        let frame = &fixture.frame;
        let original = frame.original_v1();
        let source = PalwTirPublicSourceV1::new_native_v1(
            &fixture.record,
            &fixture.layout,
            frame.header_v1(),
            original.execution_root,
            original.trace_root,
            fixture.rules.maximum_leaves,
        )
        .unwrap();
        let facts = NativeTfp1PublicOriginalFactsV1::derive_v1(original, &source).unwrap();
        assert_eq!(facts.claim, frame.plan_v1().original_claim);
        assert_eq!(facts.source, frame.plan_v1().source_id);
        assert_ne!(facts.job_context, facts.execution_root);

        let profile = Hash64::from_u64_word(1);
        let resource = Hash64::from_u64_word(2);
        let obligation = facts.service_obligation_v1(profile, resource).unwrap();
        let d = Digest([4; 64]);
        let scope = ChainScope { network: d, ruleset: d, branch: d };
        let binding = frame
            .header_v1()
            .resolve_registered_layout_v1(&fixture.record, &fixture.layout, original.execution_root, fixture.rules.maximum_leaves)
            .unwrap();
        let job = &binding.job_context;
        let supply = RemainingCapacity { transmitted_bytes: 8192, additional_work: 16, retained_bytes: 8192 };
        let budget = AggregateBudget {
            rounds: 1,
            unique_source_bytes: supply.transmitted_bytes,
            transmitted_bytes: 3 * supply.transmitted_bytes,
            additional_work: 3 * supply.additional_work,
            retained_bytes: 3 * supply.retained_bytes,
            retention_until: 10_100,
            fee: 1,
            witness_attempts: 1,
        };
        let limits = [ManifestScopeRules { codec: d, max_bytes: 1, chunk_bytes: 1 }; MaterialScope::COUNT];
        let admission = PublicOriginalAdmissionV1 {
            scope,
            job,
            class: &binding.class,
            artifact: facts.artifact,
            miner: facts.miner,
            prompt: d,
            initial_state: d,
            dependencies: d,
            service_contract: obligation.service_contract_v1(&budget),
            max_manifest_bytes: ManifestV1::WIRE_BYTES,
            manifest_scope_limits: &limits,
            manifest_total_limit: MaterialScope::COUNT as u64,
        };
        let reservation = PublicOriginalReservationV1 {
            origin: AcceptedOrigin {
                accepting_block: NativeHash64([1; 64]),
                carrier_block: NativeHash64([2; 64]),
                carrier_transaction: NativeHash64([3; 64]),
                accepted_tx_ordinal: 0,
                event_ordinal: 0,
                accepted_daa: 100,
                accepted_blue_score: 101,
            },
            service_obligation: obligation.obligation_v1(),
            notice: FirstNoticeDemand {
                draw_ready_by: 101,
                material_ready_by: 101,
                cold_acquisition_daa: 1,
                full_inspection_daa: 1,
                notice_inclusion_daa: 1,
                missing_material_detected_by: 101,
                material_request_inclusion_daa: 1,
            },
            case: MaterialCaseDemand {
                material_delivery_daa: 1,
                full_inspection_daa: 1,
                localization_daa: 1,
                filing_daa: 1,
                adjudication_and_recovery_daa: 1,
            },
            initial_supply: supply,
            rescue: ProtectedRescueDemand {
                max_drawn_bonds: 1,
                per_drawn_bond: supply,
                max_outsiders: 0,
                per_outsider: RemainingCapacity { transmitted_bytes: 0, additional_work: 0, retained_bytes: 0 },
                defensive: supply,
            },
            usage: &AggregateUsage::default(),
            budget: &budget,
        };
        let companion = OriginalCommit {
            scope,
            job: Digest(job.job_id.as_bytes()),
            execution: Digest(facts.job_context.as_bytes()),
            miner: facts.miner,
            registered_class: Digest(job.shape_profile_id.as_bytes()),
            artifact: Digest(facts.artifact.as_bytes()),
            program: Digest(fixture.record.graph_ir_root.as_bytes()),
            prompt: d,
            input_tokens: Digest(job.prompt_token_ids_hash.as_bytes()),
            tokenizer: Digest(job.tokenizer_id.as_bytes()),
            execution_rules: Digest(job.cu_ruleset_id.as_bytes()),
            generation_seed: job.execution_seed,
            initial_state: d,
            dependencies: d,
            prefill: job.declared_prefill_tokens,
            decode: job.exact_decode_tokens,
            max_context: job.max_context_tokens,
            output: Digest(facts.output.as_bytes()),
            trace_root: Digest(facts.execution_root.as_bytes()),
            manifest_root: d,
            service_contract: obligation.service_contract_v1(&budget),
        };
        let sign = |original: OriginalCommit| {
            let signature =
                libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &original.id().0.0, ORIGINAL_SIGN_CONTEXT, [32; 32]).unwrap();
            borsh::to_vec(&SignedPublicOriginalV1 { original, key, signature: *signature.as_ref() }).unwrap()
        };
        assert_eq!(admission.check_tfp1_candidate_v1(&companion, obligation, &reservation), Ok(()));
        let prepared = admission.prepare_tfp1_signed_bytes_v1(&sign(companion.clone()), obligation, &reservation).unwrap();
        assert_eq!(prepared.companion_v1().original, companion.id());
        assert_ne!(prepared.companion_v1().original.0.0, facts.claim.as_bytes());
        assert_eq!(prepared.accepted_companion_v1().origin, reservation.origin);
        assert_eq!(prepared.obligation_v1(), obligation);
        assert_eq!(prepared.obligation_v1().facts_v1().native_identity_v1(), (facts.claim, facts.source, facts.execution_root));
        let header = source.header_v1();
        let index = crate::palw_tir_court_v1::PublicOriginalTraceIndexV1 {
            full_logits_trace_root: header.full_logits_trace_root,
            step_leaf_count: header.step_leaf_count,
            step_merkle_root: header.step_merkle_root,
        };
        index.check(&companion, fixture.rules.maximum_leaves).unwrap();
        let wrong_trace = OriginalCommit { trace_root: Digest(facts.trace.as_bytes()), ..companion.clone() };
        assert!(index.check(&wrong_trace, fixture.rules.maximum_leaves).is_err());
        for foreign in [
            OriginalCommit { output: d, ..companion.clone() },
            wrong_trace,
            OriginalCommit { execution: Digest(facts.execution_root.as_bytes()), ..companion.clone() },
            OriginalCommit { registered_class: d, ..companion.clone() },
            OriginalCommit { service_contract: d, ..companion.clone() },
        ] {
            assert_eq!(admission.prepare_tfp1_signed_bytes_v1(&sign(foreign), obligation, &reservation), Err(ContractError::Scope));
        }

        // The immutable original ID binds other FP commitment fields as
        // well, even when output, execution and trace roots are unchanged.
        let mut foreign = original.clone();
        foreign.schedule_root = Hash64::from_u64_word(42);
        let foreign = NativeTfp1PublicOriginalFactsV1::derive_v1(&foreign, &source).unwrap();
        assert_ne!(foreign.claim, facts.claim);
        assert_ne!(foreign.service_obligation_v1(profile, resource).unwrap(), obligation);
        assert_ne!(facts.service_obligation_v1(profile, Hash64::from_u64_word(3)).unwrap(), obligation);
        assert_ne!(facts.service_obligation_v1(Hash64::from_u64_word(3), resource).unwrap(), obligation);

        let mut wrong_root = original.clone();
        wrong_root.execution_root = Hash64::from_u64_word(43);
        assert_eq!(NativeTfp1PublicOriginalFactsV1::derive_v1(&wrong_root, &source), Err(ContractError::Scope));
    }
}
