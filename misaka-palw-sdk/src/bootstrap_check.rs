//! File-derived complete-check bootstrap preparation, without node-private state.
//!
//! These builders export signed objects; native inclusion, collateral, retry and
//! carrier fees remain consensus checks. Preparing a binding is no registration
//! receipt or claim of useful work, model provenance, or release approval.

use crate::complete_check::{COMPLETE_CHECK_MAX_CONTAINER_BYTES_V1, complete_artifact_operands_v1};
use crate::kernel_rows::{KernelRowV1, KernelRowsSnapshotV1};
use crate::onboarding_chain::EnvelopeSigner;
use crate::runtime_pack::commit::Refusal;
use kaspa_consensus_core::palw_kernel_route_v1::{
    PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1, PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, PalwKernelRouteStateV1,
    palw_kernel_route_message_v1, palw_kernel_route_template_typed_v1,
};
use kaspa_consensus_core::palw_onboarding_v1::{
    ArtifactBindingStateV1, PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1, PALW_ONBOARDING_MLDSA87_CONTEXT_V1,
    palw_onboarding_message_v1,
};
use kaspa_consensus_core::palw_opv_bootstrap_v1::{
    CompleteCheckDomainV1, PALW_COMPLETE_CHECK_FEE_SOMPI_V1, PALW_COMPLETE_CHECK_MAX_POST_BYTES_V1, complete_check_post_v1,
    complete_check_reference_v1, palw_complete_check_domain_v1, palw_onboarding_complete_check_policy_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_hashes::Hash64;
pub use misaka_palw_challenge::{ConformanceCommitmentV1, RootV1, SubjectKindV1};
use misaka_palw_challenge::{OnboardingStateV1, OnboardingStepV1};
use misaka_palw_kernel::descriptor::k2_tir_v2_descriptor;
use misaka_palw_kernel::gate::class_prosecution_bounds_v1;
use misaka_palw_kernel::ledger::{carrier_fit_v1, single_class_id_v1};
use misaka_palw_kernel::mode::VerificationModeV1;
use misaka_palw_kernel::plan::{VerificationPlanV1, plan_for_tir_program_v1};
use misaka_palw_kernel::public::{ProfileMaterialV1, program_root_v1};
use misaka_palw_kernel::route::KernelRouteObjectV1;
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_tir::{MapParams, TirProgramV1};
use std::path::Path;

/// Chosen before preparation. These cap the two existing consensus bond costs;
/// carrier transaction fees have their own submit-object authorization.
#[derive(Clone, Copy, Debug)]
pub struct CompleteBootstrapCostsV1 {
    pub maximum_binding_reservation_sompi: u64,
    pub maximum_check_fee_sompi: u64,
}
impl CompleteBootstrapCostsV1 {
    pub fn check(self) -> Result<(), Refusal> {
        if self.maximum_binding_reservation_sompi < PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1
            || self.maximum_check_fee_sompi < PALW_COMPLETE_CHECK_FEE_SOMPI_V1
        {
            return Err(Refusal::new(
                "COST_LIMIT",
                "the existing binding reservation or complete-check bond fee exceeds the caller's cap",
            ));
        }
        Ok(())
    }
}

/// Independent pins from one caller-authenticated chain view. Serving two
/// consistent inventories is insufficient: the caller authenticates these pins.
#[derive(Clone, Copy, Debug)]
pub struct BootstrapSnapshotPinsV1 {
    pub ledger_root: Hash64,
    pub aux_root: Hash64,
    pub tip_daa: u64,
}

/// The exact complete-check class/artifact that the caller locally retained.
/// The source can be any bounded stateless PALWTIR1 artifact; no seeded fixture,
/// claimed Final, tensor root, or conformance outcome is supplied by the builder.
pub struct CompleteBootstrapArtifactV1 {
    class: Hash64,
    artifact_root: Hash64,
    kernel_class: Hash64,
    kernel_param_root: Hash64,
    pub domain: CompleteCheckDomainV1,
    registrant: PalwBondKeyV2,
    program: TirProgramV1,
    plan: VerificationPlanV1,
    pc: ParamCommitmentsV1,
    layout_root: Hash64,
}

impl CompleteBootstrapArtifactV1 {
    /// The registration is the caller's retained native registration object, not
    /// a node's claim that it has been included. Its program, identity and file
    /// roots are re-derived; native inclusion remains the consensus authority.
    pub fn from_registration_file(
        registration: &PalwConsensusObjectV2,
        artifact: &Path,
        max_positions: u32,
        costs: CompleteBootstrapCostsV1,
    ) -> Result<Self, Refusal> {
        costs.check()?;
        let PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, artifact_root, admission, .. } = registration else {
            return Err(Refusal::new("NOT_TIR_REGISTRATION", "supply the locally retained ClassRegisteredTirV1 object"));
        };
        if admission.class.program.len() > crate::complete_check::COMPLETE_CHECK_MAX_PROGRAM_BYTES_V1
            || admission.class.layout.commit_tiles.len() > crate::complete_check::COMPLETE_CHECK_MAX_PROGRAM_BYTES_V1
            || admission.class.layout.state_tiles.len() > crate::complete_check::COMPLETE_CHECK_MAX_PROGRAM_BYTES_V1
        {
            return Err(Refusal::new("REGISTRATION_LIMIT", "the retained class exceeds the bounded preparation limits"));
        }
        if admission.class.version != kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1
            || admission.class.class_id(artifact_root) != *class_id
        {
            return Err(Refusal::new(
                "CLASS_SUBSTITUTED",
                "the registration's class id differs from its program/layout/tokenizer/artifact",
            ));
        }
        if max_positions == 0 || max_positions > admission.class.layout.max_context {
            return Err(Refusal::new("POSITION_LIMIT", "the plan's positions must fit the locally retained native class layout"));
        }
        let container = misaka_palw_tir_artifact::PalwTirContainerV1::open_bounded(
            artifact,
            COMPLETE_CHECK_MAX_CONTAINER_BYTES_V1,
            COMPLETE_CHECK_MAX_CONTAINER_BYTES_V1 as u32,
        )
        .map_err(|e| Refusal::new("ARTIFACT_UNREADABLE", e.to_string()))?;
        if container.header.program != admission.class.program
            || container.header.tokenizer_id != admission.class.tokenizer_id.as_bytes()
            || (!container.header.layout.is_empty()
                && container.header.layout != borsh::to_vec(&admission.class.layout).expect("a class layout serializes"))
        {
            return Err(Refusal::new(
                "FILE_SUBSTITUTED",
                "the artifact's program, tokenizer or declared layout differs from the registration",
            ));
        }
        let domain = palw_complete_check_domain_v1(&container.program, max_positions)
            .map_err(|e| Refusal::new("NOT_COMPLETELY_CHECKABLE", e))?;
        let operands = complete_artifact_operands_v1(&container, &container.program, &domain)?;
        let reference =
            complete_check_reference_v1(&container.program, &domain, &operands).map_err(|e| Refusal::new(e.code, e.detail))?;
        if reference.artifact_root != *artifact_root {
            return Err(Refusal::new(
                "ARTIFACT_SUBSTITUTED",
                "the file's complete inventory differs from the registration's artifact root",
            ));
        }
        let post =
            complete_check_post_v1(&container.program, &domain, [0; 64], operands).map_err(|e| Refusal::new(e.code, e.detail))?;
        if borsh::to_vec(&post).expect("a complete post serializes").len() > PALW_COMPLETE_CHECK_MAX_POST_BYTES_V1 {
            return Err(Refusal::new("POST_LIMIT", "the whole-artifact post does not fit the consensus complete-check limit"));
        }
        let mut params = MapParams::default();
        for entry in &container.header.tensors {
            params.tensors.insert(
                (entry.param, entry.layer),
                container.read_tensor(entry.param, entry.layer).map_err(|e| Refusal::new("ARTIFACT_UNREADABLE", e.to_string()))?,
            );
        }
        let pc = ParamCommitmentsV1::of(&params);
        if pc.root() != reference.kernel_param_root {
            return Err(Refusal::new("FILE_MOVED", "the file's tensor commitments changed during preparation"));
        }
        let descriptor = k2_tir_v2_descriptor();
        let plan = plan_for_tir_program_v1(&descriptor, &container.program, program_root_v1(&container.header.program), max_positions)
            .map_err(|e| Refusal::new("PLAN_REFUSED", format!("{e:?}")))?;
        let kernel_class = Hash64::from_bytes(single_class_id_v1(
            descriptor.digest(),
            &container.header.program,
            &plan,
            &pc,
            VerificationModeV1::PanelLicensed,
        ));
        Ok(Self {
            class: *class_id,
            artifact_root: *artifact_root,
            kernel_class,
            kernel_param_root: Hash64::from_bytes(pc.root()),
            domain,
            registrant: admission.registrant_bond,
            program: container.program,
            plan,
            pc,
            layout_root: admission.class.layout_digest(),
        })
    }

    pub fn class(&self) -> Hash64 {
        self.class
    }
    pub fn artifact_root(&self) -> Hash64 {
        self.artifact_root
    }
    pub fn kernel_class(&self) -> Hash64 {
        self.kernel_class
    }
    pub fn kernel_param_root(&self) -> Hash64 {
        self.kernel_param_root
    }

    /// Exact file-derived semantic roots for a caller's local conformance
    /// statement. These are not receipts for inclusion, maturity or provenance.
    pub fn program_root(&self) -> [u8; 64] {
        program_root_v1(&self.program.encode())
    }
    pub fn verification_plan_root(&self) -> [u8; 64] {
        self.plan.root()
    }
    pub fn layout_root(&self) -> [u8; 64] {
        self.layout_root.as_bytes()
    }
    pub fn kernel_descriptor_id(&self) -> [u8; 64] {
        k2_tir_v2_descriptor().digest()
    }

    pub fn registrant(&self) -> PalwBondKeyV2 {
        self.registrant
    }

    fn check_bond(&self, bond: PalwBondKeyV2) -> Result<(), Refusal> {
        if bond != self.registrant {
            return Err(Refusal::new(
                "REGISTRANT_SUBSTITUTED",
                "only this registration's registrant prepares its bindings or commitment",
            ));
        }
        Ok(())
    }

    fn route(
        &self,
        snapshot: &KernelRowsSnapshotV1,
        rows: Vec<KernelRowV1>,
        pins: BootstrapSnapshotPinsV1,
        network_domain: Hash64,
    ) -> Result<PalwKernelRouteStateV1, Refusal> {
        let route = authenticate_bootstrap_snapshot_v1(snapshot, rows, pins, network_domain)?;
        let binding = route
            .artifact_binding_v1(&self.class, &self.kernel_param_root)
            .ok_or_else(|| Refusal::new("BINDING_MISSING", "the exact file has no rooted native artifact binding"))?;
        if binding.binder != self.registrant
            || binding.bound_daa > pins.tip_daa
            || !matches!(binding.state_at(pins.tip_daa), ArtifactBindingStateV1::Matured | ArtifactBindingStateV1::Final)
        {
            return Err(Refusal::new(
                "BINDING_NOT_MATURE",
                "the exact binding is refuted, pending, from the future or belongs to another bond",
            ));
        }
        Ok(route)
    }

    fn check_record(&self, route: &PalwKernelRouteStateV1) -> Result<(), Refusal> {
        let record = route
            .kernel_class_record_v1(&self.kernel_class)
            .ok_or_else(|| Refusal::new("KERNEL_CLASS_MISSING", "the exact file-derived kernel class has not registered"))?;
        if record.descriptor != k2_tir_v2_descriptor().digest()
            || record.program_bytes != self.program.encode()
            || record.plan != self.plan
            || record.param_commitments != self.pc
        {
            return Err(Refusal::new(
                "KERNEL_CLASS_SUBSTITUTED",
                "the rooted kernel record differs from the file-derived program, plan or parameters",
            ));
        }
        Ok(())
    }

    /// Tag 104: local preparation; the node checks native registration and free
    /// collateral before actually reserving the fixed consensus amount.
    pub fn signed_artifact_binding(
        &self,
        network_domain: Hash64,
        bond: PalwBondKeyV2,
        signer: &dyn EnvelopeSigner,
    ) -> Result<PalwConsensusObjectV2, Refusal> {
        self.check_bond(bond)?;
        let payload = borsh::to_vec(&(self.class, self.kernel_param_root)).expect("two digests serialize");
        let signature = onboarding_signature(network_domain, 104, bond, &payload, signer)?;
        Ok(PalwConsensusObjectV2::ArtifactBoundV1 {
            v2_class: self.class,
            kernel_param_root: self.kernel_param_root,
            signer: bond,
            signature,
        })
    }

    /// Actual kernel registration (tag 110/inner 1), after authenticated mature
    /// binding. Public court and DA bounds use the rooted network's policy.
    pub fn signed_kernel_registration(
        &self,
        snapshot: &KernelRowsSnapshotV1,
        rows: Vec<KernelRowV1>,
        pins: BootstrapSnapshotPinsV1,
        network_domain: Hash64,
        bond: PalwBondKeyV2,
        signer: &dyn EnvelopeSigner,
    ) -> Result<PalwConsensusObjectV2, Refusal> {
        self.check_bond(bond)?;
        let route = self.route(snapshot, rows, pins, network_domain)?;
        if route.kernel_class_record_v1(&self.kernel_class).is_some() {
            return Err(Refusal::new("KERNEL_CLASS_EXISTS", "the exact kernel class is already registered"));
        }
        let descriptor = k2_tir_v2_descriptor();
        let template = palw_kernel_route_template_typed_v1(route.header.policy, route.header.opv, route.header.typed_roots);
        misaka_palw_kernel::check::check_plan_v1(
            &template.schedule,
            &descriptor,
            &self.program,
            program_root_v1(&self.program.encode()),
            &self.plan,
            pins.tip_daa,
        )
        .map_err(|e| Refusal::new("PLAN_REFUSED", e.to_string()))?;
        let bounds = class_prosecution_bounds_v1(
            &descriptor,
            &self.plan,
            &self.program,
            &ProfileMaterialV1::kernel_route(true),
            &route.header.policy.prosecution,
        )
        .map_err(|e| Refusal::new("COURT_REFUSED", format!("{e:?}")))?;
        if bounds.max_court_work > route.header.policy.max_court_work_per_block {
            return Err(Refusal::new("COURT_REFUSED", "the worst public court does not fit the rooted per-block budget"));
        }
        carrier_fit_v1(
            &bounds,
            PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1,
            PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1,
            PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1,
        )
        .map_err(|e| Refusal::new("CARRIER_LIMIT", e))?;
        let object = KernelRouteObjectV1::RegisterClass {
            descriptor: descriptor.digest(),
            program_bytes: self.program.encode(),
            plan: self.plan.clone(),
            param_commitments: self.pc.clone(),
        };
        let bytes = object.encode();
        if bytes.len() > PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 {
            return Err(Refusal::new("CARRIER_LIMIT", "the kernel registration exceeds the native assembled-object bound"));
        }
        let message = palw_kernel_route_message_v1(network_domain, &bond, &bytes);
        let signature = checked_signature(message, PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, signer)?;
        Ok(PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: bond, signature })
    }

    pub fn signed_kernel_binding(
        &self,
        snapshot: &KernelRowsSnapshotV1,
        rows: Vec<KernelRowV1>,
        pins: BootstrapSnapshotPinsV1,
        network_domain: Hash64,
        bond: PalwBondKeyV2,
        signer: &dyn EnvelopeSigner,
    ) -> Result<PalwConsensusObjectV2, Refusal> {
        self.check_bond(bond)?;
        let route = self.route(snapshot, rows, pins, network_domain)?;
        self.check_record(&route)?;
        if route.kernel_binding_v1(&self.class).is_some() {
            return Err(Refusal::new("KERNEL_BINDING_EXISTS", "this native class is already kernel-bound"));
        }
        let policy = Hash64::from_bytes(palw_onboarding_complete_check_policy_v1().id());
        let payload = borsh::to_vec(&(self.class, self.kernel_class, policy)).expect("three digests serialize");
        let signature = onboarding_signature(network_domain, 106, bond, &payload, signer)?;
        Ok(PalwConsensusObjectV2::KernelBoundV1 {
            v2_class: self.class,
            kernel_class: self.kernel_class,
            challenge_policy_id: policy,
            signer: bond,
            signature,
        })
    }

    /// The caller supplies source/implementation/scope/resource commitments; no
    /// benchmark, provenance, soundness or economic result is invented here.
    pub fn signed_conformance_commitment(
        &self,
        snapshot: &KernelRowsSnapshotV1,
        rows: Vec<KernelRowV1>,
        pins: BootstrapSnapshotPinsV1,
        genesis: Hash64,
        network_domain: Hash64,
        commitment: ConformanceCommitmentV1,
        bond: PalwBondKeyV2,
        signer: &dyn EnvelopeSigner,
    ) -> Result<PalwConsensusObjectV2, Refusal> {
        self.check_bond(bond)?;
        let route = self.route(snapshot, rows, pins, network_domain)?;
        self.check_record(&route)?;
        commitment.well_formed().map_err(|e| Refusal::new("COMMITMENT_MALFORMED", e))?;
        let binding = route
            .kernel_binding_v1(&self.class)
            .ok_or_else(|| Refusal::new("KERNEL_BINDING_MISSING", "bind the native class before its conformance commitment"))?;
        if binding.binder != bond
            || binding.bound_daa > pins.tip_daa
            || binding.kernel_class != self.kernel_class
            || binding.kernel_param_root != self.kernel_param_root
            || binding.plan_root.as_bytes() != self.plan.root()
            || binding.challenge_policy_id.as_bytes() != palw_onboarding_complete_check_policy_v1().id()
            || commitment.subject_kind != SubjectKindV1::ModelConformance
            || commitment.candidate_id != self.class.as_bytes()
            || commitment.chain_genesis != genesis.as_bytes()
            || commitment.ruleset_id != route.header.policy.ruleset_digest
            || commitment.challenge_policy_id != binding.challenge_policy_id.as_bytes()
            || commitment.kernel_descriptor_id != k2_tir_v2_descriptor().digest()
            || commitment.artifact_root != self.artifact_root.as_bytes()
            || commitment.program_root != program_root_v1(&self.program.encode())
            || commitment.verification_plan_root != self.plan.root()
            || commitment.layout_root != self.layout_root.as_bytes()
        {
            return Err(Refusal::new(
                "COMMITMENT_SUBSTITUTED",
                "the supplied commitment or rooted binding differs from this file's class, network, roots or complete policy",
            ));
        }
        if let Some(prior) = route.conformance_attempt_v1(&self.class) {
            if prior.record.state != OnboardingStateV1::RegisteredDormant || prior.last_end.is_some_and(|(_, at)| at >= pins.tip_daa) {
                return Err(Refusal::new("ATTEMPT_EXISTS", "an open/passed attempt or same-block retry cannot be replaced"));
            }
            let mut record = prior.record;
            record
                .apply(OnboardingStepV1::ConformanceCommitted { commitment_root: commitment.statement_root() })
                .map_err(|e| Refusal::new("ATTEMPT_REFUSED", e.to_string()))?;
        }
        let payload = borsh::to_vec(&commitment).expect("a commitment serializes");
        let signature = onboarding_signature(network_domain, 107, bond, &payload, signer)?;
        Ok(PalwConsensusObjectV2::ConformanceCommittedV1 { commitment: Box::new(commitment), signer: bond, signature })
    }
}

fn onboarding_signature(
    network: Hash64,
    tag: u8,
    bond: PalwBondKeyV2,
    payload: &[u8],
    signer: &dyn EnvelopeSigner,
) -> Result<Vec<u8>, Refusal> {
    checked_signature(palw_onboarding_message_v1(network, tag, &bond, payload), PALW_ONBOARDING_MLDSA87_CONTEXT_V1, signer)
}

fn checked_signature(message: Hash64, context: &[u8], signer: &dyn EnvelopeSigner) -> Result<Vec<u8>, Refusal> {
    let signature = signer.sign_with_context(message.as_byte_slice(), context).map_err(|e| Refusal::new("SIGNER_FAILED", e))?;
    if !kaspa_txscript::verify_mldsa87_with_context(&signer.public_key(), message.as_byte_slice(), &signature, context)
        .map_err(|e| Refusal::new("SIGNATURE_MALFORMED", e.to_string()))?
    {
        return Err(Refusal::new("SIGNATURE_INVALID", "the exported signature does not verify under the signer's public key"));
    }
    Ok(signature)
}

/// Authenticate the complete inventory before executing a local file or loading
/// a signing key. The pins must be obtained independently from a trusted view.
pub fn authenticate_bootstrap_snapshot_v1(
    snapshot: &KernelRowsSnapshotV1,
    rows: Vec<KernelRowV1>,
    pins: BootstrapSnapshotPinsV1,
    network_domain: Hash64,
) -> Result<PalwKernelRouteStateV1, Refusal> {
    if snapshot.ledger_root != pins.ledger_root || snapshot.aux_root != pins.aux_root || snapshot.tip_daa != pins.tip_daa {
        return Err(Refusal::new("READS_NOT_THE_CHAIN", "the served snapshot differs from the caller's trusted roots or current DAA"));
    }
    if rows.len() as u64 != snapshot.total_rows {
        return Err(Refusal::new("INCOMPLETE_ROWS", "the advertised kernel inventory is incomplete"));
    }
    let route = PalwKernelRouteStateV1::from_served_rows_v1(&snapshot.header, rows, &pins.ledger_root, &pins.aux_root)
        .map_err(|e| Refusal::new("READS_NOT_THE_CHAIN", e))?;
    if route.header.policy.network_domain != network_domain.as_bytes() || route.header.scalars.daa > pins.tip_daa {
        return Err(Refusal::new("READS_NOT_THE_CHAIN", "the rooted route belongs to another network or a future DAA"));
    }
    Ok(route)
}
