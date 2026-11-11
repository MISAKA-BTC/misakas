//! Shared immutable identity for a prospective native TFP1 and its
//! PublicOneHonest companion. The binding is available before either
//! signature, and does not authenticate an accepted carrier or reserve slots.

pub(crate) mod carrier_material;
pub(crate) mod funding;
pub(crate) mod initial_supply;
pub(crate) mod manifest_carrier;
pub(crate) mod manifest_paid;
pub(crate) mod manifest_request;
pub(crate) mod native_frame;
pub(crate) mod native_link;
pub(crate) mod preclaim;
pub(crate) mod service_booking;
pub(crate) mod service_graph;
pub(crate) mod service_invariants;

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
    public_one_honest::{MaterialScope, OriginalCommit, SignedPublicOriginalV1, prompt_text_material_v1},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeTfp1PublicOriginalFactsV1 {
    network: Hash64,
    claim: Hash64,
    source: Hash64,
    job_context: Hash64,
    artifact: Hash64,
    execution_root: Hash64,
    output: Hash64,
    trace: Hash64,
    miner: PrincipalId,
    cold_execution: bool,
}

impl NativeTfp1PublicOriginalFactsV1 {
    #[allow(dead_code)]
    pub(crate) fn native_identity_v1(&self) -> (Hash64, Hash64, Hash64) {
        (self.claim, self.source, self.execution_root)
    }

    pub(crate) fn derive_v1(original: &PalwFreePromptCommitmentV3, source: &PalwTirPublicSourceV1) -> Result<Self, ContractError> {
        if original.work_leaves != source.leaf_count_v1() || original.decode_tokens_executed != source.decode_count_v1() {
            return Err(ContractError::Scope);
        }
        Self::from_native_header_v1(original, source.header_v1(), fp_claim_id_v3(original), source.source_id_v1())
    }

    // Called only for a verified Source or an already rooted frame keyed by
    // the native claim. The link reader separately rehashes the exact header.
    fn from_native_header_v1(
        original: &PalwFreePromptCommitmentV3,
        header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        claim: Hash64,
        source: Hash64,
    ) -> Result<Self, ContractError> {
        Self::from_native_header_with_hashes_v1(
            original,
            header,
            claim,
            source,
            || Ok(palw_fp_job_pin_v1(original)),
            || header.job_context.context_hash(),
        )
    }

    /// The caller derives these identities from the exact same native inputs.
    /// The protected frame uses allocation-free canonical encoders.
    fn from_native_header_with_hashes_v1(
        original: &PalwFreePromptCommitmentV3,
        header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        claim: Hash64,
        source: Hash64,
        original_job_pin: impl FnOnce() -> Result<Hash64, ContractError>,
        job_context: impl FnOnce() -> Hash64,
    ) -> Result<Self, ContractError> {
        if original.job.class_id != header.job_context.shape_profile_id
            || original_job_pin()? != palw_fp_job_pin_of_context_v1(&header.job_context)
            || original.execution_root != header.committed_execution_root
            || original.trace_root != header.full_logits_trace_root
            || original.work_leaves != header.step_leaf_count
            || original.decode_tokens_executed != header.job_context.exact_decode_tokens
            || original.output_root == Hash64::default()
        {
            return Err(ContractError::Scope);
        }
        let key = original.job.executor_pubkey.as_slice().try_into().map_err(|_| ContractError::Identity)?;
        Ok(Self {
            network: original.job.network_domain,
            claim,
            source,
            job_context: job_context(),
            artifact: header.artifact_root,
            execution_root: original.execution_root,
            output: original.output_root,
            trace: original.trace_root,
            miner: principal(key),
            cold_execution: super::tfp1_material::require_cold_tfp1_v1(original, &header.job_context).is_ok(),
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
        Ok(Tfp1PublicOriginalObligationV1 { facts: self, profile, resource, obligation: Digest(obligation.as_bytes()) })
    }
}

/// Explicit native policy inputs. The owner resolves scope and limits from
/// its active rules; neither a companion nor a manifest may choose them.
/// In particular, this adapter does not invent a branch from the accepting
/// block, nor select manifest ceilings from a test fixture.
#[derive(Clone, Copy)]
pub(crate) struct Tfp1PublicAdmissionPolicyV1<'a> {
    pub scope: misaka_palw_positive_contract::identity::ChainScope,
    pub max_manifest_bytes: u32,
    pub manifest_scope_limits: &'a [misaka_palw_positive_contract::public_one_honest::ManifestScopeRules; MaterialScope::COUNT],
    pub manifest_total_limit: u64,
}

impl<'a> PublicOriginalAdmissionV1<'a> {
    /// Derive expected identities before either unsigned quote or signed
    /// intake. The prompt is the literal prospective/actual carrier bytes.
    /// This does not authenticate the FP/payer, prove branch ancestry, choose
    /// a public profile, or reserve any of its service/funding obligations.
    pub(crate) fn from_native_tfp1_v1(
        original: &PalwFreePromptCommitmentV3,
        source: &'a PalwTirPublicSourceV1,
        class: &'a crate::palw_tir_class_v1::PalwTirClassV1,
        policy: Tfp1PublicAdmissionPolicyV1<'a>,
        prompt_bytes: &[u8],
        obligation: Tfp1PublicOriginalObligationV1,
        budget: &AggregateBudget,
    ) -> Result<Self, ContractError> {
        let facts = NativeTfp1PublicOriginalFactsV1::derive_v1(original, source)?;
        if facts != obligation.facts || policy.scope.network != Digest(facts.network.as_bytes()) {
            return Err(ContractError::Scope);
        }
        misaka_palw_positive_contract::public_one_honest::ManifestV1::fetch_size(policy.max_manifest_bytes)?;
        if policy.manifest_total_limit < MaterialScope::COUNT as u64
            || policy.manifest_scope_limits.iter().any(|rule| rule.max_bytes == 0 || rule.chunk_bytes == 0)
        {
            return Err(ContractError::Budget);
        }
        let prompt = std::str::from_utf8(prompt_bytes).map_err(|_| ContractError::NonCanonical)?;
        let prompt =
            prompt_text_material_v1(policy.scope.ruleset, prompt, policy.manifest_scope_limits[MaterialScope::Prompt as usize])?
                .subject;
        let cold = super::public_tfp1_cold_material_v1(
            original,
            source,
            class,
            policy.manifest_scope_limits[MaterialScope::InitialState as usize],
            policy.manifest_scope_limits[MaterialScope::Dependencies as usize],
        )?;
        Ok(Self {
            scope: policy.scope,
            job: &source.header_v1().job_context,
            class,
            artifact: facts.artifact,
            miner: facts.miner,
            prompt,
            initial_state: cold.initial_state.0.subject,
            dependencies: cold.dependencies.0.subject,
            service_contract: obligation.service_contract_v1(budget),
            max_manifest_bytes: policy.max_manifest_bytes,
            manifest_scope_limits: policy.manifest_scope_limits,
            manifest_total_limit: policy.manifest_total_limit,
        })
    }
}

/// The versioned obligation committed by the companion's signed
/// service_contract. This is separate from the TFP1 pledge's resource ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tfp1PublicOriginalObligationV1 {
    facts: NativeTfp1PublicOriginalFactsV1,
    profile: Hash64,
    resource: Hash64,
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

    /// Final physical namespaces can be computed after constructing the
    /// companion, before signing or admitting it. They are deliberately not
    /// inputs to the relative resource graph that the companion commits.
    fn batch_scope_v1(self, original: misaka_palw_positive_contract::public_one_honest::OriginalId) -> Result<Hash64, ContractError> {
        if original.0 == Digest([0; 64]) {
            return Err(ContractError::Scope);
        }
        stream_digest(
            b"misaka-palw/tfp1-public-original-batch/v1",
            &(self.facts.claim, self.facts.source, original, self.profile, self.resource, self.obligation),
        )
        .map_err(|_| ContractError::NonCanonical)
    }

    pub(crate) fn check_companion_v1(self, companion: &OriginalCommit, budget: &AggregateBudget) -> Result<(), ContractError> {
        let facts = self.facts;
        if companion.execution != Digest(facts.job_context.as_bytes())
            || companion.scope.network != Digest(facts.network.as_bytes())
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
    /// Usage from the same pre-write admission inputs as the signed budget.
    /// Replaying the intake must rederive it; booking cannot reset it to zero.
    fee_used: u64,
    pairing: Option<PairedTfp1CarrierV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PairedTfp1CarrierV1 {
    literal_fp: Hash64,
    funding_contract: Hash64,
    funding_outpoint: crate::tx::TransactionOutpoint,
    funding_amount: u64,
}

#[allow(dead_code)]
impl PreparedTfp1PublicOriginalV1 {
    /// A prebookable selected-bond rank, not the later logical CaseId. The
    /// actual draw normalizes its members to these ranks. A first claimant
    /// cannot choose a new physical namespace or take another rank's slots.
    pub(crate) fn drawn_batch_reservation_scope_v1(&self, rank: u64) -> Result<Hash64, ContractError> {
        preclaim::drawn_scope_v1(self.obligation, self.companion.original, rank, self.companion.rescue.max_drawn_bonds)
    }

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
        self.check_original_metadata_v1(companion)?;
        self.check_reservation_v1(companion, reservation).map(|_| ())
    }

    fn check_tfp1_link_v1(
        &self,
        companion: &OriginalCommit,
        obligation: Tfp1PublicOriginalObligationV1,
        reservation: &PublicOriginalReservationV1<'_>,
    ) -> Result<(), ContractError> {
        obligation.check_companion_v1(companion, reservation.budget)?;
        // The current TFO3 bridge implements a fresh V3/V4 execution. Derive
        // these roots from native metadata rather than accepting matching
        // caller/candidate placeholders. Other dependency forms need their
        // own material relation before this bridge may admit them.
        if !obligation.facts.cold_execution {
            return Err(ContractError::UnsupportedProfile);
        }
        use misaka_palw_positive_contract::public_one_honest::{
            tir_cold_dependencies_for_execution_v1, tir_cold_initial_state_for_execution_v1,
        };
        let execution = Digest(self.job.context_hash().as_bytes());
        let initial = tir_cold_initial_state_for_execution_v1(
            Digest(self.class.graph_ir_root().as_bytes()),
            execution,
            self.manifest_scope_limits[MaterialScope::InitialState as usize],
        )?
        .0
        .subject;
        let dependencies =
            tir_cold_dependencies_for_execution_v1(execution, self.manifest_scope_limits[MaterialScope::Dependencies as usize])?
                .0
                .subject;
        if self.job.context_hash() != obligation.facts.job_context
            || self.artifact != obligation.facts.artifact
            || self.miner != obligation.facts.miner
            || reservation.service_obligation != obligation.obligation
            || self.initial_state != initial
            || self.dependencies != dependencies
            || companion.initial_state != initial
            || companion.dependencies != dependencies
        {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    /// Bind the exact public prompt to native scope and codec limits. This
    /// authenticates byte identity only; text-to-token validation is separate
    /// reserved work using the registered raw tokenizer.
    pub(crate) fn check_tfp1_prompt_bytes_v1(&self, bytes: &[u8]) -> Result<(), ContractError> {
        let text = std::str::from_utf8(bytes).map_err(|_| ContractError::NonCanonical)?;
        let entry = prompt_text_material_v1(self.scope.ruleset, text, self.manifest_scope_limits[MaterialScope::Prompt as usize])?;
        if entry.subject != self.prompt {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    /// Prepare from the actual TFO3 transaction, never a separately donated
    /// signed companion or prompt. The immutable native claim and accepted
    /// transaction origin must match the same reservation. Native FP/payer
    /// signature checks, complete service reservation and atomic installation
    /// remain the accepting owner's responsibilities.
    pub(crate) fn prepare_tfp1_pairing_v1(
        &self,
        pairing: &crate::palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1,
        obligation: Tfp1PublicOriginalObligationV1,
        reservation: &PublicOriginalReservationV1<'_>,
    ) -> Result<PreparedTfp1PublicOriginalV1, ContractError> {
        if pairing.claim() != obligation.facts.claim
            || pairing.public_source_id_v3() != Some(obligation.facts.source)
            || pairing.envelope().funding.terms.source_id != obligation.facts.source
            || pairing.carrier().id().as_bytes() != reservation.origin.carrier_transaction.0
        {
            return Err(ContractError::Scope);
        }
        self.check_tfp1_prompt_bytes_v1(pairing.public_prompt_bytes_v3().ok_or(ContractError::Scope)?)?;
        let mut prepared =
            self.prepare_tfp1_signed_bytes_v1(pairing.public_companion_v3().ok_or(ContractError::Scope)?, obligation, reservation)?;
        if pairing.public_manifest_bytes_v2().is_some() {
            // Reuse the authenticated companion's literal preimage. A v2
            // suffix cannot be ignored by either intake or history replay.
            let signed = SignedPublicOriginalV1::decode(pairing.public_companion_v3().ok_or(ContractError::Scope)?)?;
            carrier_material::check_pairing_material_v2(self, pairing, &signed.original)?;
        }
        // The pairing derives both slices from the same actual transaction.
        // Preserve a digest of the signed literal, independent of the
        // immutable semantic claim ID used before either signature.
        prepared.pairing = Some(PairedTfp1CarrierV1 {
            literal_fp: stream_digest(
                b"misaka-palw/tfp1-public-original-literal/v1",
                &pairing.envelope().original_envelope.as_slice(),
            )
            .map_err(|_| ContractError::NonCanonical)?,
            funding_contract: pairing.envelope().funding.terms.id_v1(),
            funding_outpoint: pairing.outpoint(),
            funding_amount: pairing.envelope().funding.terms.funding_amount_sompi,
        });
        Ok(prepared)
    }

    /// Version-2 admission preparation additionally resolves the literal HF
    /// reference against the selected parent's class/container. This returns
    /// identity metadata and native initial-supply geometry, not evidence of
    /// model availability, service delivery, or successful admission. The
    /// accepting owner resolves profile policy, prompt form and parent first.
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub(crate) fn prepare_tfp1_material_pairing_v2(
        &self,
        parent: &crate::palw_state_v2::PalwChainStateV2,
        extras: &crate::palw_state_v2::PalwTransitionExtrasV1,
        point: &crate::palw_state_v2::PalwBlockContextV2,
        prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        pairing: &crate::palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1,
        obligation: Tfp1PublicOriginalObligationV1,
        reservation: &PublicOriginalReservationV1<'_>,
    ) -> Result<
        (PreparedTfp1PublicOriginalV1, super::PublicTfp1HfArtifactV1, initial_supply::PublicTfp1InitialSupplyV1),
        carrier_material::MaterialPairingErrorV2,
    > {
        use crate::palw_native_resource_v1::{TirNativeRequestsV1, reserve_native_pending_requests_v1};
        use carrier_material::MaterialPairingErrorV2::LocalResource;
        super::require_current_public_origin_v1(reservation.origin, point)?;
        if self.job != &source.header_v1().job_context {
            return Err(ContractError::Scope.into());
        }
        super::tfp1_material::require_cold_tfp1_v1(fp, &source.header_v1().job_context)?;
        let fp_bytes = borsh::object_length(fp).map_err(|_| ContractError::NonCanonical)?;
        if fp_bytes > crate::palw_freeprompt_v3::PALW_FP_COMMITMENT_TX_MAX_BYTES {
            return Err(ContractError::Budget.into());
        }
        // The legacy FP/context/class identity helpers serialize temporary
        // preimages. Own those buffers before calling them, as the native
        // initial-supply planner does. Fixed manifest/descriptor decode and
        // contract hashes themselves have no dynamically sized allocation.
        let mut requests = TirNativeRequestsV1::new();
        for bytes in [
            fp_bytes,
            borsh::object_length(&source.header_v1().job_context).map_err(|_| ContractError::NonCanonical)?,
            borsh::object_length(&self.class.layout).map_err(|_| ContractError::NonCanonical)?,
        ] {
            requests.vec_v1::<u8>((bytes as u64).max(1024)).map_err(LocalResource)?;
        }
        let _hash_scratch = reserve_native_pending_requests_v1(requests.bytes_v1()).map_err(LocalResource)?;
        if NativeTfp1PublicOriginalFactsV1::derive_v1(fp, source)? != obligation.facts {
            return Err(ContractError::Scope.into());
        }
        let prepared = self.prepare_tfp1_pairing_v1(pairing, obligation, reservation)?;
        let descriptor = misaka_palw_positive_contract::public_one_honest::HfArtifactDescriptorV1::decode_v1(
            pairing.public_hf_descriptor_bytes_v2().ok_or(ContractError::Scope)?,
        )?;
        let hf = super::PublicTfp1HfArtifactV1::resolve_v1(
            parent,
            fp,
            source,
            descriptor,
            self.manifest_scope_limits[MaterialScope::Artifact as usize],
        )?;
        hf.check_context_v1(fp, source, self.class)?;
        drop(_hash_scratch);
        let prompt = pairing.public_prompt_bytes_v3().ok_or(ContractError::Scope)?;
        let supply = initial_supply::PublicTfp1InitialSupplyV1::derive_v1(
            parent,
            extras,
            point,
            fp,
            source,
            self.class,
            prompt_form,
            prompt,
            Some(&hf),
        )?;
        supply.check_limits_v1(
            self.max_manifest_bytes,
            self.manifest_scope_limits,
            self.manifest_total_limit,
            reservation.initial_supply,
        )?;
        let signed = SignedPublicOriginalV1::decode(pairing.public_companion_v3().ok_or(ContractError::Scope)?)?;
        let manifest = signed
            .original
            .authenticate_manifest(pairing.public_manifest_bytes_v2().ok_or(ContractError::Scope)?, self.max_manifest_bytes)?
            .decode(&signed.original, self.manifest_scope_limits, self.manifest_total_limit)?;
        supply.check_manifest_v1(prepared.companion.original, &manifest, prompt)?;
        Ok((prepared, hf, supply))
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
        Ok(PreparedTfp1PublicOriginalV1 { companion, obligation, fee_used: reservation.usage.fee, pairing: None })
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
        let scope = ChainScope { network: Digest(original.job.network_domain.as_bytes()), ruleset: d, branch: d };
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
            retention_until: 10_110, // Claim material remains retained after the hard court deadline.
            fee: 1,
            witness_attempts: 1,
        };
        let prompt_text = "Studio の prompt\n";
        let mut limits = [ManifestScopeRules { codec: d, max_bytes: 1, chunk_bytes: 1 }; MaterialScope::COUNT];
        limits[MaterialScope::Prompt as usize] = ManifestScopeRules {
            codec: misaka_palw_positive_contract::public_one_honest::prompt_text_material_codec_v1(),
            max_bytes: 64,
            chunk_bytes: 64,
        };
        limits[MaterialScope::InitialState as usize] = ManifestScopeRules {
            codec: misaka_palw_positive_contract::public_one_honest::tir_cold_initial_state_codec_v1(),
            max_bytes: 128,
            chunk_bytes: 128,
        };
        limits[MaterialScope::Dependencies as usize] = ManifestScopeRules {
            codec: misaka_palw_positive_contract::public_one_honest::tir_cold_dependencies_codec_v1(),
            max_bytes: 68,
            chunk_bytes: 68,
        };
        let cold = super::super::public_tfp1_cold_material_v1(
            original,
            &source,
            &binding.class,
            limits[MaterialScope::InitialState as usize],
            limits[MaterialScope::Dependencies as usize],
        )
        .unwrap();
        use misaka_palw_positive_contract::public_one_honest::{
            tir_cold_dependencies_material_v1, tir_cold_initial_state_material_v1, tir_ordinary_tfp1_rules_material_codec_v1,
            tir_wide_rules_material_codec_v1,
        };
        let program = binding.class.decode_program().unwrap();
        let rules = ManifestScopeRules { codec: tir_ordinary_tfp1_rules_material_codec_v1(), max_bytes: 512, chunk_bytes: 512 };
        let ordinary_rules = |program: &misaka_palw_tir::TirProgramV1, form, rules| {
            super::super::public_ordinary_tfp1_rules_material_v1(original, &source, &binding.class, program, form, rules)
        };
        let form = crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat;
        let (entry, bytes) = ordinary_rules(&program, form, rules).unwrap();
        assert_eq!(entry.total_bytes, 512);
        assert_eq!(&bytes[..64], &binding.class.graph_ir_root().as_bytes());
        assert_eq!(&bytes[5 * 64..6 * 64], &crate::palw_freeprompt_v3::fp_job_id_v3(&original.job).as_bytes());
        let other_form = ordinary_rules(&program, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1, rules).unwrap();
        assert_ne!(entry.content_root, other_form.0.content_root);
        assert_eq!(entry.subject, other_form.0.subject); // The native CU ID is unchanged.
        assert_eq!(ordinary_rules(&program, form, ManifestScopeRules { max_bytes: 511, ..rules }), Err(ContractError::Budget));
        assert_eq!(
            ordinary_rules(&program, form, ManifestScopeRules { codec: tir_wide_rules_material_codec_v1(), ..rules }),
            Err(ContractError::UnsupportedProfile)
        );
        let mut foreign_program = program.clone();
        foreign_program.token_bound += 1;
        assert_eq!(ordinary_rules(&foreign_program, form, rules), Err(ContractError::Scope));
        let mut prefix = original.clone();
        prefix.job.version = 11;
        assert_eq!(super::super::tfp1_material::require_cold_tfp1_v1(&prefix, job), Err(ContractError::UnsupportedProfile));
        let prompt = prompt_text_material_v1(scope.ruleset, prompt_text, limits[MaterialScope::Prompt as usize]).unwrap().subject;
        let policy = Tfp1PublicAdmissionPolicyV1 {
            scope,
            max_manifest_bytes: ManifestV1::WIRE_BYTES,
            manifest_scope_limits: &limits,
            manifest_total_limit: 64 + 128 + 68 + MaterialScope::COUNT as u64 - 3,
        };
        let admission = PublicOriginalAdmissionV1::from_native_tfp1_v1(
            original,
            &source,
            &binding.class,
            policy,
            prompt_text.as_bytes(),
            obligation,
            &budget,
        )
        .unwrap();
        assert_eq!(admission.prompt, prompt);
        assert_eq!(admission.initial_state, cold.initial_state.0.subject);
        assert_eq!(admission.dependencies, cold.dependencies.0.subject);
        let derive = |policy, bytes: &[u8], class| {
            PublicOriginalAdmissionV1::from_native_tfp1_v1(original, &source, class, policy, bytes, obligation, &budget).err()
        };
        assert_eq!(derive(policy, &[0xff], &binding.class), Some(ContractError::NonCanonical));
        assert_eq!(
            derive(
                Tfp1PublicAdmissionPolicyV1 { scope: ChainScope { network: Digest([99; 64]), ..scope }, ..policy },
                prompt_text.as_bytes(),
                &binding.class,
            ),
            Some(ContractError::Scope)
        );
        assert_eq!(
            derive(Tfp1PublicAdmissionPolicyV1 { manifest_total_limit: 0, ..policy }, prompt_text.as_bytes(), &binding.class),
            Some(ContractError::Budget)
        );
        let mut foreign_class = binding.class.clone();
        foreign_class.tokenizer_id = Hash64::from_u64_word(999);
        assert_eq!(derive(policy, prompt_text.as_bytes(), &foreign_class), Some(ContractError::Scope));
        let mut reservation = PublicOriginalReservationV1 {
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
            prompt,
            input_tokens: Digest(job.prompt_token_ids_hash.as_bytes()),
            tokenizer: Digest(job.tokenizer_id.as_bytes()),
            execution_rules: Digest(job.cu_ruleset_id.as_bytes()),
            generation_seed: job.execution_seed,
            initial_state: cold.initial_state.0.subject,
            dependencies: cold.dependencies.0.subject,
            prefill: job.declared_prefill_tokens,
            decode: job.exact_decode_tokens,
            max_context: job.max_context_tokens,
            output: Digest(facts.output.as_bytes()),
            trace_root: Digest(facts.execution_root.as_bytes()),
            manifest_root: d,
            service_contract: obligation.service_contract_v1(&budget),
        };
        assert_eq!(
            tir_cold_initial_state_material_v1(&companion, limits[MaterialScope::InitialState as usize]).unwrap(),
            cold.initial_state
        );
        assert_eq!(
            tir_cold_dependencies_material_v1(&companion, limits[MaterialScope::Dependencies as usize]).unwrap(),
            cold.dependencies
        );
        let sign = |original: OriginalCommit| {
            let signature =
                libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &original.id().0.0, ORIGINAL_SIGN_CONTEXT, [32; 32]).unwrap();
            borsh::to_vec(&SignedPublicOriginalV1 { original, key, signature: *signature.as_ref() }).unwrap()
        };
        assert_eq!(admission.check_tfp1_candidate_v1(&companion, obligation, &reservation), Ok(()));
        // Repeating a foreign network in both expected inputs and a correctly
        // re-signed companion cannot substitute for the native FP network.
        let foreign_scope = ChainScope { network: Digest([99; 64]), ..scope };
        let foreign_admission = PublicOriginalAdmissionV1 { scope: foreign_scope, ..admission };
        let foreign_companion = OriginalCommit { scope: foreign_scope, ..companion.clone() };
        assert_eq!(foreign_admission.check_tfp1_candidate_v1(&foreign_companion, obligation, &reservation), Err(ContractError::Scope));
        assert_eq!(
            foreign_admission.prepare_tfp1_signed_bytes_v1(&sign(foreign_companion), obligation, &reservation),
            Err(ContractError::Scope)
        );
        // Even a caller repeating the candidate's invented roots cannot
        // replace the native cold-state/dependency identities.
        let placeholder = PublicOriginalAdmissionV1 { initial_state: d, dependencies: d, ..admission };
        let invented = OriginalCommit { initial_state: d, dependencies: d, ..companion.clone() };
        assert_eq!(placeholder.check_tfp1_candidate_v1(&invented, obligation, &reservation), Err(ContractError::Scope));
        let prepared = admission.prepare_tfp1_signed_bytes_v1(&sign(companion.clone()), obligation, &reservation).unwrap();
        assert_eq!(prepared.companion_v1().original, companion.id());
        assert_ne!(prepared.companion_v1().original.0.0, facts.claim.as_bytes());
        assert_eq!(prepared.accepted_companion_v1().origin, reservation.origin);
        assert_eq!(prepared.obligation_v1(), obligation);
        assert_eq!(prepared.obligation_v1().facts_v1().native_identity_v1(), (facts.claim, facts.source, facts.execution_root));
        // Companion signatures are real. Funding is deliberately a structural
        // fixture, not evidence of payment or native FP admission.
        let mut funded = crate::palw_tir_funded_original_envelope_v1::funded_original_transport_fixture_v1();
        let fp = crate::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3 {
            version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            commitment: original.clone(),
            prompt_token_ids: fixture.execution.prompt.clone(),
            signature: vec![7; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let literal = crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1::new_v1(
            borsh::to_vec(&fp).unwrap(),
            frame.header_v1().clone(),
            frame.custody_v1().pledge_v1().clone(),
            frame.plan_v1().terms.plan.rent_sompi,
        )
        .unwrap();
        funded.funding.terms.claim = fp.claim_id();
        funded.funding.terms.job_identity = literal.pledge.job_identity;
        funded.funding.terms.execution_root = original.execution_root;
        funded.funding.terms.source_id = facts.source;
        funded.original_envelope = literal.encode_v1().unwrap();
        let signed: [u8; SignedPublicOriginalV1::WIRE_BYTES] = sign(companion.clone()).try_into().unwrap();
        let pair_carrier = |funded: &crate::palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1, prompt: &[u8]| {
            let inner = funded.encode_v1().unwrap();
            use crate::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
            let payload =
                crate::palw_tir_funded_original_envelope_v3::PalwTirFundedOriginalEnvelopeV3::from_parts_v3(&inner, prompt, &signed)
                    .unwrap()
                    .encode_v3()
                    .unwrap();
            let tx = Transaction::new(
                0,
                vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(99), 0), vec![], 0, 0)],
                vec![
                    TransactionOutput::new(1, ScriptPublicKey::default()),
                    TransactionOutput::new(
                        funded.funding.terms.funding_amount_sompi,
                        crate::mldsa87_primitives::p2pkh_mldsa87_spk(
                            &kaspa_hashes::blake2b_512_address_payload(&funded.payer_pubkey).as_bytes(),
                        ),
                    ),
                ],
                0,
                crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
                0,
                payload,
            );
            crate::palw_funded_original_coordinator_v1::palw_funded_original_pairing_v1(&tx).unwrap()
        };
        let mut pairing = pair_carrier(&funded, prompt_text.as_bytes());
        assert_eq!(admission.prepare_tfp1_pairing_v1(&pairing, obligation, &reservation), Err(ContractError::Scope));
        reservation.origin.carrier_transaction = NativeHash64(pairing.carrier().id().as_bytes());
        let prepared = admission.prepare_tfp1_pairing_v1(&pairing, obligation, &reservation).unwrap();
        let signed_only = admission.prepare_tfp1_signed_bytes_v1(&signed, obligation, &reservation).unwrap();
        assert_eq!(prepared.accepted_companion_v1(), signed_only.accepted_companion_v1());
        // Version 2 must authenticate its actual suffix before returning the
        // same prepared owner. This fixture checks metadata, not delivery or
        // native payment. Parent container resolution is tested separately.
        {
            use crate::palw_tir_funded_original_envelope_v3::PalwTirFundedOriginalEnvelopeV3;
            use misaka_palw_positive_contract::public_one_honest::{
                HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1, HfArtifactDescriptorV1, HfArtifactNativePinsV1, ManifestEntryV1,
                hf_artifact_material_codec_v1,
            };
            let mut v2_limits = limits;
            v2_limits[MaterialScope::Artifact as usize] = ManifestScopeRules {
                codec: hf_artifact_material_codec_v1(),
                max_bytes: HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1 as u64,
                chunk_bytes: HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1 as u32,
            };
            let v2_admission =
                PublicOriginalAdmissionV1 { manifest_scope_limits: &v2_limits, manifest_total_limit: 2048, ..admission };
            let descriptor = HfArtifactDescriptorV1::new_v1(
                HfArtifactNativePinsV1 {
                    class: companion.registered_class,
                    program: companion.program,
                    tokenizer: companion.tokenizer,
                    inventory: companion.artifact,
                    container: d,
                    binding_manifest: d,
                },
                "fixture/model",
                [1; 20],
                d,
                d,
            )
            .unwrap();
            let descriptor_bytes = descriptor.encode_v1().unwrap();
            let subjects = [
                companion.artifact,
                companion.program,
                companion.prompt,
                companion.input_tokens,
                companion.tokenizer,
                companion.execution_rules,
                companion.initial_state,
                companion.dependencies,
                companion.trace_root,
                companion.output,
            ];
            let mut manifest = ManifestV1 {
                service_contract: companion.service_contract,
                entries: subjects.map(|subject| ManifestEntryV1 {
                    subject,
                    content_root: d,
                    codec: d,
                    total_bytes: 1,
                    chunk_bytes: 1,
                }),
            };
            manifest.entries[MaterialScope::Artifact as usize] = descriptor.manifest_entry_v1(v2_limits[0]).unwrap();
            manifest.entries[MaterialScope::Prompt as usize] =
                prompt_text_material_v1(scope.ruleset, prompt_text, v2_limits[2]).unwrap();
            manifest.entries[MaterialScope::InitialState as usize] = cold.initial_state.0;
            manifest.entries[MaterialScope::Dependencies as usize] = cold.dependencies.0;
            let mut v2_companion = companion.clone();
            let manifest_bytes: [u8; ManifestV1::WIRE_BYTES as usize] =
                v2_companion.bind_manifest(&manifest, &v2_limits, 2048, ManifestV1::WIRE_BYTES).unwrap().try_into().unwrap();
            let v2_signed: [u8; SignedPublicOriginalV1::WIRE_BYTES] = sign(v2_companion.clone()).try_into().unwrap();
            let inner = funded.encode_v1().unwrap();
            let make = |manifest: &[u8; ManifestV1::WIRE_BYTES as usize], descriptor: &[u8]| {
                let mut tx = pairing.carrier().clone();
                tx.payload = PalwTirFundedOriginalEnvelopeV3::from_parts_with_manifest_v2(
                    &inner,
                    prompt_text.as_bytes(),
                    &v2_signed,
                    manifest,
                    descriptor,
                )
                .unwrap()
                .encode_v3()
                .unwrap();
                tx.finalize();
                crate::palw_funded_original_coordinator_v1::palw_funded_original_pairing_v1(&tx).unwrap()
            };
            let actual = make(&manifest_bytes, &descriptor_bytes);
            let v2_reservation = PublicOriginalReservationV1 {
                origin: AcceptedOrigin { carrier_transaction: NativeHash64(actual.carrier().id().as_bytes()), ..reservation.origin },
                ..reservation
            };
            assert_eq!(
                v2_admission.prepare_tfp1_pairing_v1(&actual, obligation, &v2_reservation).unwrap().companion_v1().original,
                v2_companion.id()
            );
            let mut wrong_manifest = manifest_bytes;
            wrong_manifest[0] ^= 1;
            let mut wrong_descriptor = descriptor_bytes.clone();
            *wrong_descriptor.last_mut().unwrap() ^= 1;
            for substituted in [make(&wrong_manifest, &descriptor_bytes), make(&manifest_bytes, &wrong_descriptor)] {
                let changed = PublicOriginalReservationV1 {
                    origin: AcceptedOrigin {
                        carrier_transaction: NativeHash64(substituted.carrier().id().as_bytes()),
                        ..v2_reservation.origin
                    },
                    ..v2_reservation
                };
                assert!(v2_admission.prepare_tfp1_pairing_v1(&substituted, obligation, &changed).is_err());
            }
            // A v1 body cannot be used through the material-required entry.
            assert!(
                admission
                    .prepare_tfp1_material_pairing_v2(
                        &crate::palw_state_v2::PalwChainStateV2::genesis(),
                        &crate::palw_state_v2::PalwTransitionExtrasV1::default(),
                        &crate::palw_state_v2::PalwBlockContextV2 {
                            block: Hash64::from_bytes(reservation.origin.accepting_block.0),
                            daa_score: reservation.origin.accepted_daa,
                            blue_score: reservation.origin.accepted_blue_score,
                            subsidy: 0,
                        },
                        form,
                        original,
                        &source,
                        &pairing,
                        obligation,
                        &reservation,
                    )
                    .is_err()
            );
        }
        let link_point = crate::palw_state_v2::PalwBlockContextV2 {
            block: Hash64::from_bytes(reservation.origin.accepting_block.0),
            daa_score: reservation.origin.accepted_daa,
            blue_score: reservation.origin.accepted_blue_score,
            subsidy: 0,
        };
        let admission_reservation = Digest([71; 64]);
        use crate::palw_state_v2::tests::native_writers_v1::{exercise_tfp1_link_writer_v1, prepare_tfp1_link_writer_fixture_v1};
        // Seed the admitted frame and table 198 for the typed writer test.
        // This is not an end-to-end production admission of the fixture.
        let terms = &pairing.envelope().funding.terms;
        let escrow = crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1::fresh_v1(
            terms.id_v1(),
            facts.claim,
            pairing.outpoint(),
            terms.funding_amount_sompi,
            terms.payer_key_id,
            terms.refund_payload,
            link_point.daa_score,
            misaka_palw_positive_contract::public_one_honest::OriginalClock::new(link_point.daa_score).unwrap().hard(),
            true,
        )
        .unwrap();
        let (link_state, link_params) = prepare_tfp1_link_writer_fixture_v1(
            &fixture,
            link_point,
            prepared.companion_v1().prepare_storage_record(admission_reservation),
            escrow,
        );
        assert_eq!(signed_only.prepare_native_link_v1(&link_state, &link_point, admission_reservation), Err(ContractError::Scope));
        let wrong_point = crate::palw_state_v2::PalwBlockContextV2 { block: Hash64::from_u64_word(805), ..link_point };
        assert_eq!(
            admission.prepare_tfp1_pairing_v1(&pairing, obligation, &reservation).unwrap().prepare_native_link_v1(
                &link_state,
                &wrong_point,
                admission_reservation
            ),
            Err(ContractError::Scope)
        );
        // A second literal carrier can have the same signed FP, Source and
        // companion and the same accepting point. Only the winner has the
        // exact escrow outpoint rooted by native admission.
        let mut duplicate_tx = pairing.carrier().clone();
        duplicate_tx.outputs[0].value += 1;
        duplicate_tx.finalize();
        let duplicate = crate::palw_funded_original_coordinator_v1::palw_funded_original_pairing_v1(&duplicate_tx).unwrap();
        reservation.origin.carrier_transaction = NativeHash64(duplicate_tx.id().as_bytes());
        assert_eq!(
            admission.prepare_tfp1_pairing_v1(&duplicate, obligation, &reservation).unwrap().prepare_native_link_v1(
                &link_state,
                &link_point,
                admission_reservation
            ),
            Err(ContractError::Scope)
        );
        reservation.origin.carrier_transaction = NativeHash64(pairing.carrier().id().as_bytes());
        // Populated inputs and prefix are explicit fixtures here. The native
        // archive loader must obtain these from actual accepted stores.
        let history_fee = 20;
        let history_inputs = [crate::tx::UtxoEntry::new(
            pairing.carrier().outputs.iter().map(|output| output.value).sum::<u64>() + history_fee,
            crate::tx::ScriptPublicKey::default(),
            1,
            false,
        )];
        let seed = {
            use crate::tx::{PopulatedTransaction, ValidatedTransaction};
            let validated =
                ValidatedTransaction::new(PopulatedTransaction::new(pairing.carrier(), history_inputs.to_vec()), history_fee);
            let invalid =
                ValidatedTransaction::new(PopulatedTransaction::new(pairing.carrier(), history_inputs.to_vec()), history_fee + 1);
            assert!(crate::palw_tfp1_original_history_v1::ValidatedTfp1OriginalInputsV1::prepare_v1(&invalid).is_err());
            crate::palw_tfp1_original_history_v1::ValidatedTfp1OriginalInputsV1::prepare_v1(&validated).unwrap().unwrap()
        };
        let history_parent = Hash64::from_u64_word(804);
        pairing.bind_history_inputs_v1(seed, reservation.origin, history_parent).unwrap();
        pairing.bind_history_inputs_v1(seed, reservation.origin, history_parent).unwrap();
        let mut duplicate_history = duplicate.clone();
        assert!(duplicate_history.bind_history_inputs_v1(seed, reservation.origin, history_parent).is_err());
        let rebound = AcceptedOrigin { accepted_tx_ordinal: 1, ..reservation.origin };
        assert!(pairing.bind_history_inputs_v1(seed, rebound, history_parent).is_err());
        let prefix = crate::palw_state_v2::tests::native_writers_v1::tfp1_history_prefix_fixture_v1(
            &link_state,
            &link_params,
            &link_point,
            &pairing,
        );
        // Authenticate before the native writes, then consume that same
        // immutable owner after frame/escrow installation. No post-write
        // caller can replace the companion, budget or accepting point.
        assert!(matches!(
            admission.prepare_tfp1_history_v1(&wrong_point, &pairing, obligation, &reservation, admission_reservation, &prefix),
            Err(ContractError::Scope)
        ));
        assert!(matches!(
            admission.prepare_tfp1_history_v1(&link_point, &pairing, obligation, &reservation, Digest([0; 64]), &prefix),
            Err(ContractError::Scope)
        ));
        let before_writes = || {
            admission.prepare_tfp1_history_v1(&link_point, &pairing, obligation, &reservation, admission_reservation, &prefix).unwrap()
        };
        assert!(matches!(
            before_writes().finalize_v1(&crate::palw_state_v2::PalwChainStateV2::genesis(), &link_point),
            Err(ContractError::Scope)
        ));
        assert!(matches!(before_writes().finalize_v1(&link_state, &wrong_point), Err(ContractError::Scope)));
        let history = before_writes().finalize_v1(&link_state, &link_point).unwrap();
        let one_call = admission
            .prepare_tfp1_history_link_v1(&link_state, &link_point, &pairing, obligation, &reservation, admission_reservation, &prefix)
            .unwrap();
        assert_eq!(history.link_v1(), one_call.link_v1());
        assert_eq!(history.boundary_v1(), one_call.boundary_v1());
        assert!(history.boundary_v1().check_populated_data(pairing.carrier(), &history_inputs, history_fee).is_ok());
        assert!(history.boundary_v1().check_populated_data(pairing.carrier(), &history_inputs, history_fee + 1).is_err());
        let mut foreign_carriage = crate::palw_state_v2::PalwStateCarriageV2::from_state(&link_state);
        foreign_carriage.last_point.as_mut().unwrap().subsidy += 1;
        let foreign_state = foreign_carriage.into_state(&link_params, None).unwrap();
        let foreign_prefix = crate::palw_state_v2::tests::native_writers_v1::tfp1_history_prefix_fixture_v1(
            &foreign_state,
            &link_params,
            &link_point,
            &pairing,
        );
        assert_ne!(prefix.root_v1(), foreign_prefix.root_v1());
        let linked = exercise_tfp1_link_writer_v1(link_state.clone(), link_point, history, prefix, foreign_prefix, &link_params);
        let paired = admission.prepare_tfp1_pairing_v1(&pairing, obligation, &reservation).unwrap();
        crate::palw_state_v2::tests::native_writers_v1::exercise_public_tfp1_frame_writer_v1(
            link_state.clone(),
            link_point,
            &link_params,
            facts.claim,
            companion.id(),
            budget.retention_until,
            |state| {
                let prefix = crate::palw_state_v2::tests::native_writers_v1::tfp1_history_prefix_fixture_v1(
                    state,
                    &link_params,
                    &link_point,
                    &pairing,
                );
                let frame = native_frame::PublicTfp1FrameV1::prepare_v1(
                    &paired,
                    original,
                    &source,
                    &fixture.record,
                    &link_point,
                    admission_reservation,
                    budget.retention_until,
                    fixture.rules.maximum_leaves,
                )
                .unwrap();
                let history = admission
                    .prepare_tfp1_history_v1(&link_point, &pairing, obligation, &reservation, admission_reservation, &prefix)
                    .unwrap();
                (frame, history, prefix)
            },
        );
        assert_eq!(admission.restore_tfp1_pairing_v1(&linked, &pairing, &reservation).unwrap(), paired);
        assert!(admission.restore_tfp1_pairing_v1(&link_state, &pairing, &reservation).is_err());
        let companion_only = admission.prepare_tfp1_signed_bytes_v1(&signed, obligation, &reservation).unwrap();
        use crate::palw_state_v2::{
            PalwBlockContextV2, PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras, revert_delta_v2,
        };
        use misaka_palw_positive_contract::{
            identity::principal,
            public_one_honest::{
                Claimant, MANIFEST_REQUEST_SIGN_CONTEXT, ManifestRequestContext, PanelBond, PublicDrawMemberV1,
                SignedManifestRequestV1,
            },
        };
        let draw_point = PalwBlockContextV2 { block: Hash64::from_u64_word(806), daa_score: 101, blue_score: 102, subsidy: 0 };
        let bond = PanelBond { transaction: Digest([72; 64]), index: 0 };
        let member = PublicDrawMemberV1 { bond, principal: principal(&key), rescue_reservation: Digest([73; 64]) };
        let draw = paired.companion_v1().prepare_draw(Digest([74; 64]), draw_point, &[member], Digest([75; 64])).unwrap();
        let kernel = crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
            network_domain: Hash64::from_u64_word(1),
            ruleset_digest: Hash64::from_u64_word(2),
            ..Default::default()
        };
        let (drawn, _) = apply_palw_transition_v2_with_extras(
            &linked,
            &link_params,
            &draw_point,
            &[],
            None,
            false,
            false,
            false,
            false,
            &PalwTransitionExtrasV1 {
                kernel_route: Some(kernel.clone()),
                public_draws: vec![draw.prepare_storage_record()],
                ..Default::default()
            },
        )
        .unwrap();
        let request_point = PalwBlockContextV2 { block: Hash64::from_u64_word(807), daa_score: 102, blue_score: 103, subsidy: 0 };
        let mut request_context = ManifestRequestContext {
            original: &companion,
            claimant: Claimant::DrawnBond(bond),
            signer: member.principal,
            inspection_scope: Digest([75; 64]),
            supply_obligation: Digest([76; 64]),
            reservation: member.rescue_reservation,
            max_manifest_bytes: ManifestV1::WIRE_BYTES,
            manifest_limits: paired.companion_v1().manifest_limits,
            manifest_body_bytes_reserved: u64::from(ManifestV1::WIRE_BYTES),
            schedule: paired.companion_v1().schedule,
            origin: AcceptedOrigin {
                accepting_block: NativeHash64(request_point.block.as_bytes()),
                accepted_daa: request_point.daa_score,
                accepted_blue_score: request_point.blue_score,
                carrier_block: NativeHash64([77; 64]),
                carrier_transaction: NativeHash64([78; 64]),
                accepted_tx_ordinal: 0,
                event_ordinal: 0,
            },
        };
        let request = paired
            .companion_v1()
            .drawn_manifest_request(&draw, &companion, bond, request_context.inspection_scope, request_context.supply_obligation)
            .unwrap();
        let signature =
            libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &request.signing_digest().0, MANIFEST_REQUEST_SIGN_CONTEXT, [35; 32])
                .unwrap();
        let request_case_id = request.case_id(&companion, request_context.claimant).unwrap();
        let request_wire = borsh::to_vec(&SignedManifestRequestV1 { request, key, signature: *signature.as_ref() }).unwrap();
        // Run the same signed request against the dedicated frame as well.
        // The service tariff/book remain fixtures: this verifies the native
        // reader/writer bridge, not a complete protected-profile admission.
        let (protected_drawn, _) = apply_palw_transition_v2_with_extras(
            &protected_state,
            &link_params,
            &draw_point,
            &[],
            None,
            false,
            false,
            false,
            false,
            &PalwTransitionExtrasV1 {
                kernel_route: Some(kernel.clone()),
                public_draws: vec![draw.prepare_storage_record()],
                ..Default::default()
            },
        )
        .unwrap();
        let (protected_case, _) = manifest_request::exercise_paid_request_v1(
            &paired,
            &companion_only,
            &protected_drawn,
            &draw,
            &request_context,
            &request_wire,
            request_point,
            &link_params,
        );
        assert!(protected_case.public_tfp1_original_link_v1(facts.claim).unwrap().unwrap().uses_public_frame_v1());
        let (with_case, paid_origin) = manifest_request::exercise_paid_request_v1(
            &paired,
            &companion_only,
            &drawn,
            &draw,
            &request_context,
            &request_wire,
            request_point,
            &link_params,
        );
        request_context.origin = paid_origin;
        let pending = paired
            .companion_v1()
            .prepare_drawn_manifest_storage_record(admission_reservation, &draw, &request_wire, &request_context)
            .unwrap();
        let request_digest = pending.row.request_digest();
        assert!(paired.rooted_drawn_batch_case_v1(&linked, &draw, &request_wire, &request_context).is_err());
        assert!(paired.rooted_drawn_batch_case_v1(&drawn, &draw, &request_wire, &request_context).is_err());
        let (unpaid_case, case_delta) = apply_palw_transition_v2_with_extras(
            &drawn,
            &link_params,
            &request_point,
            &[],
            None,
            false,
            false,
            false,
            false,
            &PalwTransitionExtrasV1 { kernel_route: Some(kernel), public_drawn_manifest_cases: vec![pending], ..Default::default() },
        )
        .unwrap();
        // The legacy extras path can root a pending row, but cannot confer
        // paid TFO3 continuation authority. Only the paired writer above can.
        assert!(paired.rooted_drawn_batch_case_v1(&unpaid_case, &draw, &request_wire, &request_context).is_err());
        let rooted = paired.rooted_drawn_batch_case_v1(&with_case, &draw, &request_wire, &request_context).unwrap();
        assert_eq!((rooted.claim_v1(), rooted.source_v1()), (facts.claim, facts.source));
        assert_ne!(rooted.scope_v1(), rooted.case_scope_v1());
        // A paid receipt in another physical namespace must not authorize
        // this draw rank, even if its pending request cross-reference is valid.
        {
            use crate::palw_positive_contract_v2::PUBLIC_MANIFEST_PAID_TABLE_V1;
            use manifest_paid::{PaidManifestRequestRowV1, manifest_request_charge_purpose_v1};
            let other_scope = manifest_carrier::ManifestRequestCarrierScopeV1 {
                claim: facts.claim,
                source: facts.source,
                reservation_scope: Hash64::from_u64_word(990),
            };
            let packet = crate::palw_court_shared_material_intake_v1::batch::admission::protected::manifest_request_packet_id_v1(
                facts.claim,
                other_scope.reservation_scope,
            )
            .unwrap();
            let other_paid = PaidManifestRequestRowV1::new_v1(
                other_scope,
                companion.id(),
                request_case_id,
                request_context.origin,
                request_digest,
                manifest_request_charge_purpose_v1(companion.id(), packet).unwrap(),
                7,
            )
            .unwrap();
            // The snapshot cross-reference deliberately does not reconstruct
            // the draw. Continuation must rederive rank from actual membership.
            other_paid.check_state_at_v1(&with_case, &request_point).unwrap();
            let mut carriage = crate::palw_state_v2::PalwStateCarriageV2::from_state(&with_case);
            let route = carriage.kernel_route.as_mut().unwrap();
            route.aux.retain(|(table, _), _| *table != PUBLIC_MANIFEST_PAID_TABLE_V1);
            route.aux.insert((PUBLIC_MANIFEST_PAID_TABLE_V1, packet.to_vec()), borsh::to_vec(&other_paid).unwrap());
            let wrong_rank = carriage.into_state(&link_params, None).unwrap();
            assert!(paired.rooted_drawn_batch_case_v1(&wrong_rank, &draw, &request_wire, &request_context).is_err());
        }
        let prebooked_scope = companion_only.drawn_batch_reservation_scope_v1(0).unwrap();
        assert_eq!(rooted.reservation_scope_v1(), prebooked_scope);
        assert_ne!(rooted.reservation_scope_v1(), rooted.case_scope_v1());
        assert_eq!(companion_only.drawn_batch_reservation_scope_v1(1), Err(ContractError::Budget));
        assert_eq!(paired.drawn_batch_reservation_scope_v1(u64::MAX), Err(ContractError::Budget));
        assert_eq!(rooted.case_v1().case_id_v1(), request_case_id);
        assert_eq!(rooted.case_v1().opportunity_v1().rescue_reservation_v1(), member.rescue_reservation);
        {
            use crate::{
                palw_batch_court_v1::{BatchRangeAuthenticationV1, BatchRangeV1},
                palw_court_service_v1::ServiceCost,
                palw_court_shared_material_intake_v1::batch::{
                    admission::plain_selection::{PlainSelectionBranchV3, PlainSelectionBudgetV3},
                    selection::{
                        PlainBatchSelectionV2,
                        notice::{SELECTION_NOTICE_SIGNATURE_CONTEXT_V3, SELECTION_NOTICE_VERSION_V3, SignedSelectionNoticeV3},
                    },
                },
                palw_state_v2::{PalwBondKeyV2, PalwStateCarriageV2},
            };
            let roles = 2;
            let branch = PlainSelectionBranchV3::derive_v3(
                original,
                &source,
                &fixture.record,
                &fixture.layout,
                roles,
                PlainSelectionBudgetV3 {
                    continuation: crate::palw_court_shared_material_intake_v1::batch::admission::plain_selection::SelectionContinuationBudgetV3::test_only_v3(),
                    verification: ServiceCost { bytes: 1_000_000, work: 5_000_000, io: 5_000_000, controls: 1 },
                    verification_daa: 1,
                    workspace_bytes: 128_000_000,
                    maximum_proof_bytes: 1_000_000,
                    maximum_dependencies: 64,
                    maximum_ranges_per_role: 64,
                    maximum_material_visits: 64 * u64::from(source.decode_count_v1()),
                },
            )
            .unwrap();
            let filer = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_bytes(bond.transaction.0), bond.index));
            let network = Hash64::from_u64_word(1);
            let mut notice = SignedSelectionNoticeV3 {
                version: SELECTION_NOTICE_VERSION_V3,
                network,
                claim: facts.claim,
                source: facts.source,
                branch: branch.identity_v3().unwrap(),
                case_scope: rooted.case_scope_v1(),
                reservation_scope: rooted.reservation_scope_v1(),
                position: 0,
                beat: 0,
                filer,
                signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            };
            let sign = |notice: &mut SignedSelectionNoticeV3| {
                notice.signature = *libcrux_ml_dsa::ml_dsa_87::sign(
                    &pair.signing_key,
                    notice.signing_message_v3().unwrap().as_byte_slice(),
                    SELECTION_NOTICE_SIGNATURE_CONTEXT_V3,
                    [36; 32],
                )
                .unwrap()
                .as_ref();
            };
            sign(&mut notice);
            // The earlier fixture roots a public draw, but never registered
            // this drawn bond. A draw alone cannot authenticate the notice.
            assert!(notice.authenticate_for_case_v3(network, &rooted, &branch, &source, &with_case).is_err());
            // Explicit test-only bond fixture, restored through the native
            // carriage. No paid registration or public acceptance is claimed.
            let mut carriage = PalwStateCarriageV2::from_state(&with_case);
            let mut native_bond = carriage.bonds.values().next().unwrap().clone();
            native_bond.pubkey = key.to_vec();
            carriage.bonds.insert(filer, native_bond);
            let notice_state = carriage.into_state(&link_params, None).unwrap();
            let notice_case = paired.rooted_drawn_batch_case_v1(&notice_state, &draw, &request_wire, &request_context).unwrap();
            assert_eq!(notice_case, rooted);
            let authenticated = notice.authenticate_for_case_v3(network, &notice_case, &branch, &source, &notice_state).unwrap();
            let shape = PlainBatchSelectionV2::derive_v2(original, &source, roles, 64).unwrap();
            let ranges_for_row = |position| {
                let mut ranges = (0..shape.dependency_count_v2() / roles)
                    .map(|group| {
                        let material = shape.material_at_v2(&source, position, notice.beat, group * roles).unwrap();
                        BatchRangeV1 {
                            id: material.target.id_v1(facts.claim).unwrap(),
                            bytes: u32::try_from(material.maximum_bytes).unwrap(),
                            authentication: BatchRangeAuthenticationV1::WholeRange,
                        }
                    })
                    .collect::<Vec<_>>();
                ranges.sort_unstable_by_key(|range| range.id);
                ranges
            };
            let ranges = ranges_for_row(notice.position);
            let first = branch.demand_ordinal_v3(0, 0).unwrap();
            let second = branch.demand_ordinal_v3(1, 0).unwrap();
            // This tiny fixture fits in one partition. Identical material
            // IDs still require each supplier's distinct reserved ordinal.
            authenticated.project_demand_v3(0, 0, first, &ranges).unwrap();
            authenticated.project_demand_v3(1, 0, second, &ranges).unwrap();
            assert!(authenticated.project_demand_v3(1, 0, first, &ranges).is_err());
            assert!(authenticated.project_demand_v3(0, 0, second, &ranges).is_err());
            assert!(authenticated.project_demand_v3(roles, 0, first, &ranges).is_err());
            assert!(authenticated.project_demand_v3(0, 0, first, &ranges[1..]).is_err());
            assert!(source.decode_count_v1() > 1, "fixture must exercise a different valid row");
            let other_row = ranges_for_row(1);
            assert!(authenticated.project_demand_v3(0, 0, first, &other_row).is_err());
            let mut substituted = ranges.clone();
            substituted[0].id = Hash64::from_u64_word(990);
            assert!(authenticated.project_demand_v3(0, 0, first, &substituted).is_err());
            for coordinate in 0..7 {
                let mut altered = notice.clone();
                match coordinate {
                    0 => altered.position = source.decode_count_v1(),
                    1 => altered.beat = source.vocab_v1(),
                    2 => altered.case_scope = Hash64::from_u64_word(991),
                    3 => altered.reservation_scope = Hash64::from_u64_word(992),
                    4 => altered.branch = Hash64::from_u64_word(993),
                    5 => altered.source = Hash64::from_u64_word(994),
                    6 => altered.filer.0.index += 1,
                    _ => unreachable!(),
                }
                sign(&mut altered);
                assert!(altered.authenticate_for_case_v3(network, &notice_case, &branch, &source, &notice_state).is_err());
            }
            assert!(
                notice.authenticate_for_case_v3(Hash64::from_u64_word(995), &notice_case, &branch, &source, &notice_state).is_err()
            );
            // Even a correctly signed packet under a substituted native key
            // cannot borrow the accepted draw's different principal.
            let other = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([37; 32]);
            let mut carriage = PalwStateCarriageV2::from_state(&notice_state);
            carriage.bonds.get_mut(&filer).unwrap().pubkey = other.verification_key.as_ref().to_vec();
            let substituted_state = carriage.into_state(&link_params, None).unwrap();
            let mut altered = notice.clone();
            altered.signature = *libcrux_ml_dsa::ml_dsa_87::sign(
                &other.signing_key,
                altered.signing_message_v3().unwrap().as_byte_slice(),
                SELECTION_NOTICE_SIGNATURE_CONTEXT_V3,
                [38; 32],
            )
            .unwrap()
            .as_ref();
            let substituted_case =
                paired.rooted_drawn_batch_case_v1(&substituted_state, &draw, &request_wire, &request_context).unwrap();
            assert!(matches!(altered.authenticate_for_case_v3(network, &substituted_case, &branch, &source, &substituted_state),
                Err(why) if why == "Selection notice native key differs from its accepted draw principal"));
        }
        assert!(companion_only.rooted_drawn_batch_case_v1(&with_case, &draw, &request_wire, &request_context).is_err());
        assert!(paired.rooted_drawn_batch_case_v1(&link_state, &draw, &request_wire, &request_context).is_err());
        let rolled_back = revert_delta_v2(&unpaid_case, &case_delta, &link_params).unwrap();
        assert!(paired.rooted_drawn_batch_case_v1(&rolled_back, &draw, &request_wire, &request_context).is_err());
        let restored = crate::palw_state_v2::PalwStateCarriageV2::from_state(&with_case)
            .into_state(&link_params, Some(with_case.state_root()))
            .unwrap();
        let restored_original = admission.restore_tfp1_pairing_v1(&restored, &pairing, &reservation).unwrap();
        assert_eq!(restored_original, paired);
        let restored_draw = restored_original
            .companion_v1()
            .restore_draw(
                &restored.public_draw_record_v1(companion.id()).unwrap().unwrap(),
                Digest([74; 64]),
                draw_point,
                &[member],
                Digest([75; 64]),
            )
            .unwrap();
        assert_eq!(
            restored_original.rooted_drawn_batch_case_v1(&restored, &restored_draw, &request_wire, &request_context),
            Ok(rooted)
        );
        let shifted_reservation = PublicOriginalReservationV1 {
            origin: AcceptedOrigin { accepted_tx_ordinal: reservation.origin.accepted_tx_ordinal + 1, ..reservation.origin },
            ..reservation
        };
        assert!(admission.restore_tfp1_pairing_v1(&restored, &pairing, &shifted_reservation).is_err());
        let wrong_supply = PublicOriginalReservationV1 {
            initial_supply: RemainingCapacity {
                transmitted_bytes: reservation.initial_supply.transmitted_bytes - 1,
                ..reservation.initial_supply
            },
            ..reservation
        };
        assert!(admission.prepare_tfp1_pairing_v1(&pairing, obligation, &wrong_supply).is_ok());
        assert!(admission.restore_tfp1_pairing_v1(&restored, &pairing, &wrong_supply).is_err());
        // Table 198 alone cannot distinguish these raw timing inputs: the
        // first-request deadline is unchanged. The rooted history commitment
        // must reject them before the changed readiness can be used later.
        let aliased_timing = PublicOriginalReservationV1 {
            notice: FirstNoticeDemand {
                missing_material_detected_by: reservation.notice.missing_material_detected_by + 1,
                material_request_inclusion_daa: reservation.notice.material_request_inclusion_daa - 1,
                ..reservation.notice
            },
            ..reservation
        };
        let aliased = admission.prepare_tfp1_pairing_v1(&pairing, obligation, &aliased_timing).unwrap();
        assert_eq!(aliased.companion.record(admission_reservation), paired.companion.record(admission_reservation));
        assert_ne!(aliased.companion.material_request_ready_by, paired.companion.material_request_ready_by);
        assert!(admission.restore_tfp1_pairing_v1(&restored, &pairing, &aliased_timing).is_err());
        let duplicate_reservation = PublicOriginalReservationV1 {
            origin: AcceptedOrigin { carrier_transaction: NativeHash64(duplicate_tx.id().as_bytes()), ..reservation.origin },
            ..reservation
        };
        let duplicate_prepared = admission.prepare_tfp1_pairing_v1(&duplicate, obligation, &duplicate_reservation).unwrap();
        assert!(admission.restore_tfp1_pairing_v1(&restored, &duplicate, &duplicate_reservation).is_err());
        assert!(duplicate_prepared.rooted_drawn_batch_case_v1(&restored, &draw, &request_wire, &request_context).is_err());
        let mut wrong_wire = request_wire.clone();
        *wrong_wire.last_mut().unwrap() ^= 1;
        assert!(paired.rooted_drawn_batch_case_v1(&restored, &draw, &wrong_wire, &request_context).is_err());
        // A valid transaction with a changed literal prompt cannot borrow the
        // same signed companion, even after its new actual txid is supplied.
        let foreign_prompt = pair_carrier(&funded, b"different text");
        reservation.origin.carrier_transaction = NativeHash64(foreign_prompt.carrier().id().as_bytes());
        assert_eq!(admission.prepare_tfp1_pairing_v1(&foreign_prompt, obligation, &reservation), Err(ContractError::Scope));
        // The header contains facts outside the FP commitment, including
        // artifact identity. A fresh txid and the same FP claim cannot replace
        // that Source while retaining the original signed companion.
        let mut foreign_header = literal.clone();
        foreign_header.header.artifact_root = Hash64::from_u64_word(987);
        assert_eq!(foreign_header.original_fp_v1().unwrap().claim_id(), facts.claim);
        let mut foreign_source = funded.clone();
        foreign_source.original_envelope = foreign_header.encode_v1().unwrap();
        let substituted_source = crate::palw_tir_public_source_v1::public_source_header_id_v1(&foreign_header.header).unwrap();
        assert_ne!(substituted_source, facts.source);
        for payer_source in [facts.source, substituted_source] {
            foreign_source.funding.terms.source_id = payer_source;
            let foreign_pair = pair_carrier(&foreign_source, prompt_text.as_bytes());
            reservation.origin.carrier_transaction = NativeHash64(foreign_pair.carrier().id().as_bytes());
            assert_eq!(admission.prepare_tfp1_pairing_v1(&foreign_pair, obligation, &reservation), Err(ContractError::Scope));
        }
        let mut foreign_payer = funded.clone();
        foreign_payer.funding.terms.source_id = substituted_source;
        let foreign_pair = pair_carrier(&foreign_payer, prompt_text.as_bytes());
        reservation.origin.carrier_transaction = NativeHash64(foreign_pair.carrier().id().as_bytes());
        assert_eq!(admission.prepare_tfp1_pairing_v1(&foreign_pair, obligation, &reservation), Err(ContractError::Scope));
        let historical_source_hash = blake2b_simd::Params::new()
            .hash_length(64)
            .key(b"misaka-palw/tir-public-source/v1")
            .hash(&borsh::to_vec(&literal.header).unwrap());
        assert_eq!(facts.source.as_byte_slice(), historical_source_hash.as_bytes());
        assert_eq!(admission.check_tfp1_prompt_bytes_v1(&[0xff]), Err(ContractError::NonCanonical));
        assert_eq!(admission.check_tfp1_prompt_bytes_v1(&[b'a'; 65]), Err(ContractError::Budget));
        let different_scope = PublicOriginalAdmissionV1 { scope: ChainScope { ruleset: Digest([99; 64]), ..scope }, ..admission };
        assert_eq!(different_scope.check_tfp1_prompt_bytes_v1(prompt_text.as_bytes()), Err(ContractError::Scope));

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
            OriginalCommit { initial_state: d, ..companion.clone() },
            OriginalCommit { dependencies: d, ..companion.clone() },
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
