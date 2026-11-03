//! RFC-0045 independent expected Result bindings from the native registry.
//! These read-only calculations authenticate no accepted Assignment origin,
//! ancestry, policy, UTXO lock, physical lease, Case, payment or credit.
use crate::{
    Hash64,
    palw_positive_metadata_v2::{
        PositiveMetadataErrorV2, PositiveMetadataLimitsV2, PositiveTirMetadataV2, positive_registered_principal_v2,
    },
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2},
    palw_tir_class_v1::PalwTirLayoutV1,
    palw_v2::PalwJobContextV2,
    tx::TransactionOutpoint,
};
use misaka_palw_positive_contract::{
    ContractError,
    assignment::{AssignmentAdmissionBundleV2, AssignmentRoleV2, AssignmentTermsV2},
    identity::{ChainScope, Digest, PUBKEY_BYTES, SIGNATURE_BYTES},
    messages::{AdmissionContext, ClaimForm, MinerResultCommit},
    rows::NativeOutpoint,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignmentBindingErrorV2 {
    Contract(ContractError),
    Metadata(PositiveMetadataErrorV2),
    Key,
    Bundle,
}
impl From<ContractError> for AssignmentBindingErrorV2 {
    fn from(e: ContractError) -> Self {
        Self::Contract(e)
    }
}
impl From<PositiveMetadataErrorV2> for AssignmentBindingErrorV2 {
    fn from(e: PositiveMetadataErrorV2) -> Self {
        Self::Metadata(e)
    }
}
fn h(d: Digest) -> Hash64 {
    Hash64::from_bytes(d.0)
}
fn d(h: Hash64) -> Digest {
    Digest(h.as_bytes())
}
fn bond(outpoint: NativeOutpoint) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(crate::TransactionId::from_bytes(outpoint.transaction.0), outpoint.index))
}

/// Opaque derivation data. This type deliberately has no native admission or
/// entitlement conversion; only a real accepted-prefix writer can grant those.
pub struct AssignmentJobBindingV2 {
    terms: AssignmentTermsV2,
    admission: AdmissionContext,
    job: PalwJobContextV2,
    prompt: Vec<u32>,
    registered_tokenizer: Hash64,
}
impl AssignmentJobBindingV2 {
    pub fn derive(
        current_state: &PalwChainStateV2,
        terms: AssignmentTermsV2,
        layout: &PalwTirLayoutV1,
        limits: PositiveMetadataLimitsV2,
        native_prompt_form: PalwPromptIdsFormV1,
    ) -> Result<Self, AssignmentBindingErrorV2> {
        terms.validate_shape()?;
        let metadata = PositiveTirMetadataV2::resolve(current_state, h(terms.registered_class), layout, limits)?;
        let miner = positive_registered_principal_v2(current_state, &bond(terms.worker))?;
        // This key identity equality is not bond work-eligibility or consent.
        if positive_registered_principal_v2(current_state, &bond(terms.poster))? != terms.payer {
            return Err(ContractError::Identity.into());
        }
        positive_registered_principal_v2(current_state, &bond(terms.prover))?;
        let (job, prompt) = metadata.derive_assignment_job(terms.id(), native_prompt_form)?.into_parts();
        let admission = AdmissionContext {
            scope: ChainScope { network: terms.network, ruleset: terms.ruleset, branch: Digest(terms.branch_anchor.0) },
            expected_job: d(job.job_id),
            expected_execution: d(job.context_hash()),
            expected_miner: miner,
            expected_input: d(job.prompt_token_ids_hash),
            registered_class: terms.registered_class,
            artifact: d(metadata.class().artifact_root),
            program: d(metadata.registered().graph_ir_root),
            tokenizer: d(job.tokenizer_id),
            execution_rules: d(job.cu_ruleset_id),
            selector: misaka_palw_positive_contract::tir::greedy_selector_id(),
            required_form: ClaimForm::PositiveResultV2,
        };
        Ok(Self { terms, admission, job, prompt, registered_tokenizer: metadata.registered().tokenizer_id })
    }
    pub fn admission(&self) -> &AdmissionContext {
        &self.admission
    }
    pub fn job(&self) -> &PalwJobContextV2 {
        &self.job
    }
    pub fn prompt(&self) -> &[u32] {
        &self.prompt
    }
    pub fn terms(&self) -> &AssignmentTermsV2 {
        &self.terms
    }
    pub fn registered_tokenizer(&self) -> Hash64 {
        self.registered_tokenizer
    }

    /// Generic r1 AdmissionContext omits seed/count/temperature/dependency
    /// fields. The assigned mode independently binds every one of them too.
    /// Output is the Miner's signed claim, checked later by the full Court.
    pub fn check_result(&self, result: &MinerResultCommit) -> Result<(), AssignmentBindingErrorV2> {
        self.admission.check(ClaimForm::PositiveResultV2, result)?;
        if result.generation_seed != self.job.execution_seed
            || result.temperature_q24 != 0
            || result.prefill != self.job.declared_prefill_tokens
            || result.decode != self.job.exact_decode_tokens
            || result.max_context != self.job.max_context_tokens
            || result.dependencies != self.terms.empty_dependencies_id()
        {
            return Err(ContractError::Scope.into());
        }
        Ok(())
    }

    /// Literal three-role signatures under the current registry, not actual
    /// work eligibility, accepted origin or a funding/reservation capability.
    pub fn authenticate_bundle_roles(
        &self,
        current_state: &PalwChainStateV2,
        bundle: &AssignmentAdmissionBundleV2,
        signatures: &[[u8; SIGNATURE_BYTES]; 3],
    ) -> Result<(), AssignmentBindingErrorV2> {
        if bundle.terms != self.terms || bundle.job_context != d(self.job.context_hash()) {
            return Err(AssignmentBindingErrorV2::Bundle);
        }
        let roles = [AssignmentRoleV2::Poster, AssignmentRoleV2::Worker, AssignmentRoleV2::Prover];
        let bonds = [self.terms.poster, self.terms.worker, self.terms.prover];
        for ((role, outpoint), signature) in roles.into_iter().zip(bonds).zip(signatures) {
            let native_bond = bond(outpoint);
            // Checks operator/key integrity before using the current public key.
            let current = positive_registered_principal_v2(current_state, &native_bond)?;
            if (role == AssignmentRoleV2::Worker && current != self.admission.expected_miner)
                || (role == AssignmentRoleV2::Poster && current != self.terms.payer)
            {
                return Err(ContractError::Identity.into());
            }
            let row = current_state.bond(&native_bond).ok_or(PositiveMetadataErrorV2::MissingBond)?;
            let key: &[u8; PUBKEY_BYTES] = row.pubkey.as_slice().try_into().map_err(|_| AssignmentBindingErrorV2::Key)?;
            bundle.authenticate_role(role, key, signature)?;
        }
        Ok(())
    }
}
