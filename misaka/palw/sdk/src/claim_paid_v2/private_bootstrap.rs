//! Actual bounded Program work for the new private paid consumer. This generator
//! starts from caller-authenticated native rows after mathematical conformance;
//! it does not create a conformance receipt, source Final, payment or release gate.
use super::*;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::job::{KernelClaimV1, KernelJobV1};
use misaka_palw_kernel::public::TensorWireV1;
use misaka_palw_kernel::route::SaltedCommitV1;
use std::collections::BTreeMap;

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ProgramPaidBootstrapWorkV3 {
    pub version: u16,
    pub base_job: Digest,
    pub commit: SaltedCommitV1,
    pub material: PaidClaimMaterialWireV2,
    pub resource_bound: super::producer_resources::PrivateProgramResourceBoundV3,
}
/// The job is already posted and payer-owned. Greedy decoding performs real TIR
/// work for every generated token; the final committed stream, all node values,
/// all artifact tensors and exact job/evidence roots are exported for outsiders.
pub fn produce_program_work_v3(
    route: &PalwKernelRouteStateV1,
    base_job: Digest,
    producer: Digest,
    parameter_wire: BTreeMap<(u16, Option<u16>), TensorWireV1>,
) -> Result<ProgramPaidBootstrapWorkV3, Refusal> {
    let l = route.ledger().map_err(|e| Refusal::new("BOOTSTRAP_CHAIN", e))?;
    produce_program_work_from_ledger_v3(&l, base_job, producer, parameter_wire)
}
/// Same bounded direct Program producer over an already authenticated native
/// ledger projection. This function does not authenticate externally decoded
/// rows; the own-node caller retains the coherent snapshot and allocation owner.
pub fn produce_program_work_from_ledger_v3(
    l: &misaka_palw_kernel::ledger::KernelLedgerV1,
    base_job: Digest,
    producer: Digest,
    parameter_wire: BTreeMap<(u16, Option<u16>), TensorWireV1>,
) -> Result<ProgramPaidBootstrapWorkV3, Refusal> {
    let job: &KernelJobV1 = l.jobs.get(&base_job).ok_or_else(|| Refusal::new("BOOTSTRAP_JOB", "post the real native job first"))?;
    let class = l
        .classes
        .get(&job.class_binding_id)
        .ok_or_else(|| Refusal::new("BOOTSTRAP_CLASS", "the registered Program class is absent"))?;
    if producer == [0; 64]
        || job.max_new_tokens > 64
        || job.prompt.len() > 64
        || l.claims.values().any(|c| c.job_id == base_job)
        || class.descriptor.digest() != misaka_palw_kernel::descriptor::k2_tir_v2_descriptor().digest()
    {
        return Err(Refusal::new("BOOTSTRAP_SCOPE", "new finite K2-TIR-v2 Program job required"));
    }
    // The audited exhaustive primitive table covers whole retained trace,
    // input/history clones, actual evaluator scratch, Merkle and serialization
    // before artifact tensor decoding or any trace execution.
    let resource_bound = super::producer_resources::bound_program_work_v3(class, job, &parameter_wire)?;
    let mut params = misaka_palw_tir::MapParams::default();
    for (key, wire) in &parameter_wire {
        params.tensors.insert(*key, wire.decode().map_err(|e| Refusal::new("BOOTSTRAP_ARTIFACT", e))?);
    }
    if misaka_palw_kernel::trace::ParamCommitmentsV1::of(&params) != class.param_commitments {
        return Err(Refusal::new("BOOTSTRAP_ARTIFACT", "artifact tensors differ from the native class commitments"));
    }
    let (occurrence, node) = class.logits_at();
    let mut stream = job.prompt.clone();
    let mut generated = Vec::new();
    for index in 0..job.max_new_tokens {
        let trace = misaka_palw_kernel::trace::trace_v1(&class.program, &params, &stream)
            .map_err(|e| Refusal::new("BOOTSTRAP_EXECUTION", e.to_string()))?;
        let logits = trace
            .values
            .get(stream.len() - 1)
            .and_then(|p| p.get(occurrence as usize))
            .and_then(|o| o.get(node as usize))
            .ok_or_else(|| Refusal::new("BOOTSTRAP_EXECUTION", "actual logits missing"))?;
        let token = job.decode.select(logits).ok_or_else(|| Refusal::new("BOOTSTRAP_EXECUTION", "actual decode failed"))?;
        generated.push(token);
        if index + 1 < job.max_new_tokens {
            stream.push(token);
        }
    }
    let trace = misaka_palw_kernel::trace::trace_v1(&class.program, &params, &stream)
        .map_err(|e| Refusal::new("BOOTSTRAP_EXECUTION", e.to_string()))?;
    let commitments = trace.evidence();
    let wiring =
        misaka_palw_kernel::trace::WiringV1::new(&class.program).map_err(|e| Refusal::new("BOOTSTRAP_EXECUTION", e.to_string()))?;
    let evidence = misaka_palw_kernel::evidence::build_evidence_v1(
        &wiring,
        &commitments,
        &stream,
        class.header(job.class_binding_id),
        &class.descriptor,
        2,
    )
    .map_err(|e| Refusal::new("BOOTSTRAP_EVIDENCE", e))?;
    let claim = KernelClaimV1 { job_id: base_job, producer_bond: producer, generated, evidence_root: evidence.root() };
    let mut material = PaidClaimMaterialWireV2::default();
    material.tokens = stream;
    for (position, occurrences) in trace.values.iter().enumerate() {
        for (occurrence, nodes) in occurrences.iter().enumerate() {
            for (node, value) in nodes.iter().enumerate() {
                material.nodes.insert((0, position as u32, occurrence as u16, node as u16), TensorWireV1::of(value));
            }
        }
    }
    for ((parameter, layer), wire) in parameter_wire {
        material.params.insert((0, parameter, layer), wire);
    }
    let commit = SaltedCommitV1::Claim { claim, evidence, commitments: commitments.commitments };
    Ok(ProgramPaidBootstrapWorkV3 { version: 3, base_job, commit, material, resource_bound })
}
impl ProgramPaidBootstrapWorkV3 {
    /// Generate the exact prospective signed enrollment through the same SDK
    /// rehearsal used by all target forms. No existing claim can be migrated.
    pub fn enrollment_draft(
        &self,
        route: &PalwKernelRouteStateV1,
        class: Digest,
        job: Digest,
        salt: Digest,
        material_consent: Digest,
        anchor: u64,
    ) -> Result<KernelRouteObjectV1, Refusal> {
        if self.version != 3 {
            return Err(Refusal::new("BOOTSTRAP_VERSION", "unknown work contract"));
        }
        paid_claim_commit_object_derived_v2(route, class, job, self.commit.clone(), salt, material_consent, anchor)
    }
    /// Fresh authenticated snapshots are mandatory for each source/receipt step.
    pub fn verified_receipt_draft(
        &self,
        header: &[u8],
        rows: Vec<(u8, Vec<u8>, Vec<u8>)>,
        ledger_root: &Hash64,
        aux_root: &Hash64,
        context: Digest,
        signer: Digest,
        daa: u64,
    ) -> Result<KernelRouteObjectV1, Refusal> {
        let source = paid_claim_sources_from_kernel_rows_v2(header, rows, ledger_root, aux_root, &context)?;
        let checked = source.verify_material(&self.material)?;
        let complete = checked.complete.ok_or_else(|| Refusal::new("BOOTSTRAP_CHECK", format!("{:?}", checked.finding)))?;
        source.receipt(&complete, signer, daa)
    }
    pub fn signed_enrollment(
        &self,
        route: &PalwKernelRouteStateV1,
        class: Digest,
        job: Digest,
        salt: Digest,
        material_consent: Digest,
        anchor: u64,
        key: &dyn kaspa_pq_validator_core::MessageSigner,
        domain: Hash64,
        bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2, Refusal> {
        let draft = self.enrollment_draft(route, class, job, salt, material_consent, anchor)?;
        sign_paid_claim_object_v2(key, domain, bond, &draft).map_err(|e| Refusal::new("BOOTSTRAP_SIGNATURE", e))
    }
}
