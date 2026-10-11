//! Non-reward, explicitly versioned mixed-court candidate preparation. Stored
//! separately from execution classes and legacy conformance candidates; the
//! same bonded actor index bounds both preparation catalogs. No Final rights.
use super::*;
use crate::{flat_replay7::FlatReplayEnvelopeV7, flat_scope7::FlatResourceScopeV7};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatCandidatePlanV7 {
    pub version: u16,
    /// Artifact/job role only. The new court set is independently explicit.
    pub descriptor: Digest,
    pub program_root: Digest,
    pub court_set: Digest,
    pub profile: Digest,
    pub max_positions: u32,
    pub encoder: Option<(u16, u32)>,
    pub scope_root: Digest,
    pub replay_root: Digest,
}
impl FlatCandidatePlanV7 {
    pub fn root(&self) -> Digest {
        crate::hash::object_id(b"misaka-palw/kernel/flat-candidate-plan/v7", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatCandidateRecordV7 {
    pub binding: Digest,
    pub plan: FlatCandidatePlanV7,
    pub program_bytes: Vec<u8>,
    pub param_commitments: ParamCommitmentsV1,
}
impl FlatCandidateRecordV7 {
    pub fn id(&self) -> Digest {
        crate::hash::object_id(
            b"misaka-palw/kernel/flat-candidate-class/v7",
            &(self.binding, self.plan.root(), self.param_commitments.root(), VerificationModeV1::OptimisticPublicVerification),
        )
    }
    pub fn checked(&self) -> Result<FlatCandidateViewV7, String> {
        let v =
            derive_flat_candidate_v7(&self.plan.descriptor, &self.program_bytes, &self.param_commitments, self.plan.max_positions)?;
        if v.plan != self.plan {
            return Err("stored flat-v7 plan differs from complete independent derivation".into());
        }
        Ok(v)
    }
}
pub struct FlatCandidateViewV7 {
    pub plan: FlatCandidatePlanV7,
    pub program: TirProgramV1,
    pub encoder: Option<crate::seg_encoder::EncoderBindingV1>,
    pub scope: FlatResourceScopeV7,
    pub replay: FlatReplayEnvelopeV7,
}
pub fn derive_flat_candidate_v7(
    descriptor: &Digest,
    program_bytes: &[u8],
    pc: &ParamCommitmentsV1,
    max_positions: u32,
) -> Result<FlatCandidateViewV7, String> {
    let d = [crate::descriptor::k2_tir_v4_descriptor(), crate::descriptor::k2_tir_v5_descriptor()]
        .into_iter()
        .find(|d| d.digest() == *descriptor)
        .ok_or("unknown flat-v7 artifact/job role")?;
    let program = TirProgramV1::decode_canonical(program_bytes).map_err(|e| e.to_string())?;
    let encoder = if crate::descriptor::is_encoder_v1(&d) {
        let e = crate::seg_encoder::encoder_binding_v1(&program)?;
        crate::seg_encoder::prove_encoder_ranges_v1(&program, &e)?;
        Some(e)
    } else {
        misaka_palw_tir::interval::analyze_ranges(&program).map_err(|e| format!("flat-v7 exact ranges are not proven: {e}"))?;
        None
    };
    let real = encoder.map(|e| e.first_input as usize).unwrap_or(program.params.len());
    let declared = declared_param_instances(&program.params[..real], program.schedule.layers.len());
    check_commitment_set(pc, &declared, &declared)?;
    let scope = crate::flat_scope7::flat_resource_scope_v7(&program, max_positions, encoder.as_ref()).map_err(|e| format!("{e:?}"))?;
    let replay = crate::flat_replay7::flat_replay_envelope_v7(&program, &scope, encoder.as_ref()).map_err(|e| format!("{e:?}"))?;
    let plan = FlatCandidatePlanV7 {
        version: 7,
        descriptor: *descriptor,
        program_root: program_root_v1(program_bytes),
        court_set: scope.court_set,
        profile: scope.profile,
        max_positions,
        encoder: scope.encoder,
        scope_root: scope.root(),
        replay_root: replay.root(),
    };
    Ok(FlatCandidateViewV7 { plan, program, encoder, scope, replay })
}
/// Reserve before canonical decode, range checks or unrolled wiring. A byte
/// bounds one scheduled occurrence; each has at most MAX_NODES_PER_BLOCK nodes.
/// This conservative structural debit is not canonical work or reward weight.
pub fn flat_candidate_registration_work_v7(program_bytes: usize, pc_instances: usize) -> Option<u64> {
    let b = u64::try_from(program_bytes).ok()?;
    let pc = u64::try_from(pc_instances).ok()?;
    b.checked_mul(b)?
        .checked_add(b.checked_add(2)?.checked_mul(misaka_palw_tir::program::MAX_NODES_PER_BLOCK as u64)?.checked_mul(1024)?)?
        .checked_add(pc.checked_mul(69 * 128)?)?
        .checked_add(16384)
}
/// Native binding checks and the submitted parameter-root hash are paid before
/// the consumer examines them, separately from canonical preparation.
pub fn flat_candidate_binding_work_v7(program_bytes: usize, pc_instances: usize) -> Option<u64> {
    u64::try_from(program_bytes)
        .ok()?
        .checked_mul(64)?
        .checked_add(u64::try_from(pc_instances).ok()?.checked_mul(69 * 128)?)?
        .checked_add(1024)
}
impl FlatCandidateViewV7 {
    pub(crate) fn check_policy(&self, ledger: &KernelLedgerV1) -> Result<(), String> {
        ledger.known_descriptor(&self.plan.descriptor)?;
        ledger.check_court_work_v1(self.scope.worst_court_work)?;
        let ram = ledger.policy.prosecution.max_verifier_ram;
        if [
            self.replay.reference.position_tensor_bytes,
            self.replay.reference.largest_operand_tensor_bytes,
            self.replay.reference.history_rows_tensor_bytes,
            self.replay.topk_auxiliary_bytes,
        ]
        .iter()
        .any(|x| *x > ram)
        {
            return Err("flat-v7 ordinary component memory exceeds consumer policy".into());
        }
        Ok(())
    }
}
impl KernelLedgerV1 {
    pub(super) fn register_flat_candidate_v7(
        &mut self,
        binding: Digest,
        descriptor: Digest,
        program_bytes: &[u8],
        pc: &ParamCommitmentsV1,
        max_positions: u32,
        scope_root: Digest,
        replay_root: Digest,
        owner: Digest,
    ) -> Result<Digest, KernelRefusalV1> {
        const NAME: &str = "RegisterFlatModelCandidateV7";
        let rule = |s: String| KernelRefusalV1::rule(NAME, s);
        self.opv_register_gate(NAME)?;
        let d = self.known_descriptor(&descriptor).map_err(rule)?;
        if self.schedule.standing_at(&d.digest(), self.daa) != crate::descriptor::KernelStandingV1::Active {
            return Err(rule("flat-v7 artifact/job role is not active".into()));
        }
        if !self.attested_artifacts.contains(&pc.root()) {
            return Err(rule("flat-v7 model scope is not authenticated by the consumer".into()));
        }
        if self.conformance_class_owners.get(&owner).is_some_and(|x| x.len() >= MAX_CONFORMANCE_CLASSES_PER_BOND_V1) {
            return Err(rule("the shared conformance candidate catalog is full".into()));
        }
        let work = flat_candidate_registration_work_v7(program_bytes.len(), pc.by_instance.len())
            .ok_or_else(|| rule("flat-v7 admission overflow".into()))?;
        self.check_admission_work_v1(work).map_err(rule)?;
        self.charge(NAME, work)?;
        let v = derive_flat_candidate_v7(&descriptor, program_bytes, pc, max_positions).map_err(rule)?;
        if scope_root != v.scope.root() || replay_root != v.replay.root() {
            return Err(rule("flat-v7 declaration differs from whole-graph scope/replay".into()));
        }
        // Local ordinary-reference component payloads are checked against the
        // consumer's policy. They are not asserted to be total allocator/RSS.
        v.check_policy(self).map_err(rule)?;
        let record =
            FlatCandidateRecordV7 { binding, plan: v.plan, program_bytes: program_bytes.to_vec(), param_commitments: pc.clone() };
        let class = record.id();
        if self.flat_conformance_classes_v7.contains_key(&class) || self.classes.contains_key(&class) {
            return Err(rule("flat-v7 candidate already exists".into()));
        }
        self.flat_conformance_classes_v7.insert(class, (owner, record));
        self.conformance_class_owners.entry(owner).or_default().insert(class);
        Ok(class)
    }
}
