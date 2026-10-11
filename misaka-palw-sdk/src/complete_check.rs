//! Complete-check onboarding over one complete, rooted op-211 inventory.
//! The caller must authenticate the served roots and current DAA against the same
//! trusted chain view. The route's committed clock can lag when it has no busy rows.
//! This checks snapshot integrity and exhaustively executes a bounded stateless
//! program; it does not establish operator independence or sampled soundness.

use crate::kernel_rows::{KernelRowV1, KernelRowsSnapshotV1};
use crate::onboarding_chain::{
    EnvelopeSigner, FreshCompleteReportV1, PublicCompleteCheckReadsV1, conformance_evidence_object_v1, fresh_verify_complete_check_v1,
};
use crate::runtime_pack::commit::Refusal;
use crate::tir_stream::{ContainerRanges, PalwTirRangeSourceV1};
use kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1;
use kaspa_consensus_core::palw_conformance_evidence_v1::ConformanceEvidenceActionV1;
use kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1;
use kaspa_consensus_core::palw_opv_bootstrap_v1::{
    CompleteCheckDomainV1, CompleteCheckPostV1, PALW_COMPLETE_CHECK_MAX_POST_BYTES_V1, complete_check_post_v1,
    palw_complete_check_domain_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_visit_inventory_rows_v1;
use kaspa_hashes::Hash64;
use misaka_palw_tir::TirProgramV1;
use std::path::Path;

/// Local file limit, separate from the consensus 64-KiB tensor/post limits.
/// Checked before the container allocates its header or the tensors are read.
pub const COMPLETE_CHECK_MAX_CONTAINER_BYTES_V1: u64 = 2 << 20;
pub const COMPLETE_CHECK_MAX_PROGRAM_BYTES_V1: usize = misaka_palw_tir::program::MAX_PROGRAM_BYTES;

pub struct VerifiedCompleteCheckV1 {
    reads: PublicCompleteCheckReadsV1,
    program: TirProgramV1,
    registrant: PalwBondKeyV2,
    class: Hash64,
    network_domain: Hash64,
    open: bool,
    pub domain: CompleteCheckDomainV1,
}

impl VerifiedCompleteCheckV1 {
    /// The inventory must have been finished by KernelRowsCollectorV1, and its
    /// roots authenticated by the caller. `authenticated_tip_daa` must come from
    /// that same trusted chain view independently of the node being verified.
    /// The route's root-committed clock is not necessarily current: an idle tick
    /// may leave it unchanged. All semantic reads below come from one inventory.
    pub fn from_snapshot(
        snapshot: &KernelRowsSnapshotV1,
        rows: Vec<KernelRowV1>,
        class: Hash64,
        genesis: Hash64,
        network_domain: Hash64,
        authenticated_tip_daa: u64,
    ) -> Result<Self, Refusal> {
        if snapshot.tip_daa != authenticated_tip_daa {
            return Err(Refusal::new("CLOCK_NOT_THE_CHAIN", "the served DAA differs from the caller's authenticated current DAA"));
        }
        if rows.len() as u64 != snapshot.total_rows {
            return Err(Refusal::new("INCOMPLETE_ROWS", "the advertised kernel inventory is incomplete"));
        }
        let route = PalwKernelRouteStateV1::from_served_rows_v1(&snapshot.header, rows, &snapshot.ledger_root, &snapshot.aux_root)
            .map_err(|e| Refusal::new("READS_NOT_THE_CHAIN", e))?;
        let attempt =
            route.conformance_attempt_v1(&class).ok_or_else(|| Refusal::new("ATTEMPT_MISSING", "no rooted attempt for this class"))?;
        if route.header.scalars.daa > authenticated_tip_daa || attempt.committed_daa > authenticated_tip_daa {
            return Err(Refusal::new(
                "CLOCK_NOT_THE_CHAIN",
                "the rooted route or attempt is later than the authenticated current DAA",
            ));
        }
        if !attempt.is_complete_check() {
            return Err(Refusal::new("NOT_COMPLETE_CHECK", "this attempt uses a sampled policy"));
        }
        let binding = route
            .kernel_binding_v1(&class)
            .ok_or_else(|| Refusal::new("BINDING_MISSING", "no rooted kernel binding for this class"))?;
        let record = route
            .kernel_class_record_v1(&binding.kernel_class)
            .ok_or_else(|| Refusal::new("KERNEL_CLASS_MISSING", "the bound kernel class is absent"))?;
        let c = &attempt.commitment;
        let committed = route
            .conformance_v1(&class, &Hash64::from_bytes(c.artifact_root))
            .ok_or_else(|| Refusal::new("COMMITMENT_MISSING", "the rooted commitment row is absent"))?;
        if route.header.policy.network_domain != network_domain.as_bytes()
            || c.chain_genesis != genesis.as_bytes()
            || c.ruleset_id != route.header.policy.ruleset_digest
            || c.candidate_id != class.as_bytes()
            || c.subject_kind != misaka_palw_challenge::SubjectKindV1::ModelConformance
            || c.challenge_policy_id != binding.challenge_policy_id.as_bytes()
            || c.kernel_descriptor_id != record.descriptor
            || c.program_root != misaka_palw_kernel::public::program_root_v1(&record.program_bytes)
            || c.verification_plan_root != record.plan.root()
            || binding.plan_root.as_bytes() != record.plan.root()
            || binding.kernel_param_root.as_bytes() != record.param_commitments.root()
            || committed.statement_root.as_bytes() != c.statement_root()
            || committed.committed_daa != attempt.committed_daa
        {
            return Err(Refusal::new("BINDING_SUBSTITUTED", "attempt, program, plan or network differs from the rooted binding"));
        }
        let program =
            TirProgramV1::decode_canonical(&record.program_bytes).map_err(|e| Refusal::new("PROGRAM_MALFORMED", e.to_string()))?;
        let domain = palw_complete_check_domain_v1(&program, record.plan.max_positions)
            .map_err(|e| Refusal::new("NOT_COMPLETELY_CHECKABLE", e))?;
        let reads = PublicCompleteCheckReadsV1 {
            attempt_row: borsh::to_vec(&attempt).expect("a rooted attempt serializes"),
            program: record.program_bytes,
            max_positions: record.plan.max_positions,
            kernel_param_root: binding.kernel_param_root,
        };
        Ok(Self {
            reads,
            program,
            registrant: committed.signer,
            class,
            network_domain,
            open: attempt.open()
                && attempt.evidence.is_none()
                && authenticated_tip_daa
                    <= attempt
                        .committed_daa
                        .saturating_add(kaspa_consensus_core::palw_opv_bootstrap_v1::PALW_COMPLETE_CHECK_DEADLINE_DAA_V1),
            domain,
        })
    }

    /// Bind the op-231 convenience response to exactly the rooted op-211 data.
    pub fn check_rpc_reads(&self, attempt: &[u8], program: &[u8]) -> Result<(), Refusal> {
        if attempt != self.reads.attempt_row || program != self.reads.program {
            return Err(Refusal::new("READS_NOT_THE_CHAIN", "attempt or program moved between op-231 and op-211 reads"));
        }
        Ok(())
    }

    pub fn registrant(&self) -> PalwBondKeyV2 {
        self.registrant
    }

    fn operands(&self, path: &Path) -> Result<Vec<PalwArtifactOperandV1>, Refusal> {
        let unreadable = |e| Refusal::new("ARTIFACT_UNREADABLE", e);
        let size = std::fs::metadata(path).map_err(|e| unreadable(e.to_string()))?.len();
        if size > COMPLETE_CHECK_MAX_CONTAINER_BYTES_V1 {
            return Err(Refusal::new("ARTIFACT_LIMIT", "complete-check container exceeds the local file limit"));
        }
        let container = misaka_palw_tir_artifact::PalwTirContainerV1::open_bounded(
            path,
            COMPLETE_CHECK_MAX_CONTAINER_BYTES_V1,
            COMPLETE_CHECK_MAX_CONTAINER_BYTES_V1 as u32,
        )
        .map_err(|e| unreadable(e.to_string()))?;
        if container.header.program != self.reads.program {
            return Err(Refusal::new("PROGRAM_SUBSTITUTED", "the file contains a different program than the rooted class"));
        }
        complete_artifact_operands_v1(&container, &self.program, &self.domain)
    }

    pub fn verify_file(&self, path: &Path) -> Result<FreshCompleteReportV1, Refusal> {
        fresh_verify_complete_check_v1(&self.reads, &self.operands(path)?)
    }

    /// Build a post only for an open, unposted attempt and the exact authenticated
    /// artifact/binding. The node still checks deadlines, fees and signatures.
    pub fn build_post_file(&self, path: &Path) -> Result<CompleteCheckPostV1, Refusal> {
        if !self.open {
            return Err(Refusal::new("ATTEMPT_CLOSED", "this complete-check attempt is closed or already has evidence"));
        }
        let operands = self.operands(path)?;
        let report = fresh_verify_complete_check_v1(&self.reads, &operands)?;
        if !report.honest_passes {
            return Err(Refusal::new("BINDING_NOT_EQUAL", "artifact tensors differ from the rooted kernel commitments"));
        }
        let attempt: kaspa_consensus_core::palw_onboarding_v1::ConformanceAttemptRowV1 =
            borsh::from_slice(&self.reads.attempt_row).expect("previously checked rooted attempt");
        let post = complete_check_post_v1(&self.program, &self.domain, attempt.commitment.statement_root(), operands)
            .map_err(|e| Refusal::new("ARTIFACT_UNREADABLE", format!("{}: {}", e.code, e.detail)))?;
        if borsh::to_vec(&post).expect("a complete post serializes").len() > PALW_COMPLETE_CHECK_MAX_POST_BYTES_V1 {
            return Err(Refusal::new("POST_LIMIT", "complete-check post exceeds the consensus carrier limit"));
        }
        Ok(post)
    }

    pub fn signed_post_file(
        &self,
        path: &Path,
        bond: PalwBondKeyV2,
        signer: &dyn EnvelopeSigner,
    ) -> Result<PalwConsensusObjectV2, Refusal> {
        if bond != self.registrant {
            return Err(Refusal::new("REGISTRANT_SUBSTITUTED", "only the rooted binder may post this attempt's evidence"));
        }
        conformance_evidence_object_v1(
            self.network_domain,
            self.class,
            ConformanceEvidenceActionV1::PostComplete(Box::new(self.build_post_file(path)?)),
            bond,
            signer,
        )
    }
}

/// One bounded inventory reader shared by complete verification and bootstrap.
/// The caller first derives the admitted complete domain from this program.
pub(crate) fn complete_artifact_operands_v1(
    container: &misaka_palw_tir_artifact::PalwTirContainerV1,
    program: &TirProgramV1,
    domain: &CompleteCheckDomainV1,
) -> Result<Vec<PalwArtifactOperandV1>, Refusal> {
    let unreadable = |e| Refusal::new("ARTIFACT_UNREADABLE", e);
    let ranges = ContainerRanges::open_with_window(&container, 32 << 10).map_err(unreadable)?;
    let mut operands = Vec::with_capacity(domain.leaves as usize);
    let mut failure = None;
    let mut total = 0u64;
    palw_tir_visit_inventory_rows_v1(program, &mut |row| {
        if failure.is_some() {
            return;
        }
        total = match total.checked_add(row.len as u64) {
            Some(bytes) if bytes <= domain.artifact_bytes => bytes,
            _ => {
                failure = Some("inventory exceeds its admitted artifact byte limit".to_string());
                return;
            }
        };
        let mut bytes = vec![0; row.len as usize];
        let start = row.row_start as u64;
        if let Err(e) = ranges.read_range(row.param, row.layer, start..start + row.len as u64, &mut bytes) {
            failure = Some(e);
            return;
        }
        operands.push(PalwArtifactOperandV1 {
            tensor_name: program.params[row.param as usize].name.clone(),
            layer: row.layer,
            row_start: row.row_start,
            bytes,
        });
    })
    .map_err(|e| unreadable(e.to_string()))?;
    if let Some(error) = failure {
        return Err(unreadable(error));
    }
    if operands.len() != domain.leaves as usize || total != domain.artifact_bytes {
        return Err(unreadable("inventory does not equal the admitted complete domain".to_string()));
    }
    Ok(operands)
}
