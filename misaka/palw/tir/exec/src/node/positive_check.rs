//! A1 cold-Node EXACT_STREAM core. No network activation or credit authority.
//!
//! Admission authenticates the role commitments outside this module. The caller
//! supplies the rooted registration/job and the Panel binding fixed at admission.
//! Hash membership authenticates values; every canonical cone is also evaluated.
//! A rejected Panel candidate never constitutes a Miner conviction.

/// Experimental pure geometry/work plan for original sparse programs. This
/// module allocates no derivation/wire ID and never changes the active checker.
#[doc(hidden)]
pub mod expanded_plan;

use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_decode_select_v2::PalwDecodeSamplingV2,
    palw_native_resource_v1::{NativeWireScanV1, preflight_native_value_v1},
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_step_refute::{PalwDecodeTokenPinV1, PalwStepRefuteError, tiled_logits_scheme_id_v1},
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_court_v1::{
        PalwTirConeRefutationV1, PalwTirCourtRulesV1, PalwTirLogitsConsistencyV1, PalwTirTraceEventDisclosureV1, PalwTirTraceLanesV1,
        check_tir_cone_refutation_v1, check_tir_logits_consistency_v1,
    },
    palw_tir_step_v1::{PalwTirLeafKindV1, PalwTirLeafV1, PalwTirStepBindingV1, PalwTirStepSpaceV1, verify_tir_binding_v1},
    palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PALW_V2_MAX_NETWORK_ID_BYTES, PalwJobContextV2},
};
use misaka_palw_tir::{DType, Dim, demand::DemandLimits};

/// Local admission limits, not wire/profile parameter assignments. Reservations
/// deliberately charge a full per-cone evaluation and part at EVERY leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckLimits {
    pub max_leaves: u64,
    pub max_part_bytes: u64,
    pub max_part_heap: u64,
    pub max_total_bytes: u64,
    pub max_elements_per_leaf: u64,
    pub max_terms_per_leaf: u64,
    pub max_total_elements: u64,
    pub max_total_terms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CostEnvelope {
    pub parts: u64,
    pub encoded_bytes: u64,
    pub evaluation_elements: u64,
    pub evaluation_terms: u64,
    /// Additional decoded part storage. Excludes the admitted program/plan and
    /// interpreter workspace; this is not an end-to-end Node RSS measurement.
    pub decoded_part_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckError {
    Binding,
    UnsupportedProfile,
    Budget,
    Wire,
    Order,
    PanelEvidence,
    /// The checker could not adjudicate (including an evaluation allowance
    /// that is too small). This is not evidence of a false Panel value.
    Unverified,
    Incomplete,
    Closed,
}

/// Node-derived plan: no peer-provided relation list or progress is accepted.
/// Canonical enumeration includes commit points, checkpoints and history tiles;
/// the court expands dependency edges for each cone. The initial profile requires
/// every node to be committed: output-only cones can otherwise omit unused lanes
/// of a non-committed intermediate while still claiming that node's whole work.
/// This type intentionally has no deserializer or mutable field access.
#[derive(Clone, Debug)]
pub struct CoveragePlan {
    space: PalwTirStepSpaceV1,
    job: PalwJobContextV2,
    artifact: Hash64,
    program_bytes: usize,
    limits: CheckLimits,
    form: PalwPromptIdsFormV1,
    leaves: u64,
    vocab: u64,
    cost: CostEnvelope,
}

impl CoveragePlan {
    pub fn derive(
        registered: &PalwTirClassV1,
        artifact: Hash64,
        job: &PalwJobContextV2,
        form: PalwPromptIdsFormV1,
        limits: CheckLimits,
    ) -> Result<Self, CheckError> {
        // This initial profile replays the entire execution from its ordinary
        // initial state. Inherited-prefix contexts need their own authenticated
        // state bridge and must not silently enter this profile.
        if job.version != PALW_TRACE_COMMITMENT_VERSION_V2 {
            return Err(CheckError::UnsupportedProfile);
        }
        if job.network_id.is_empty()
            || job.network_id.len() > PALW_V2_MAX_NETWORK_ID_BYTES
            || job.declared_prefill_tokens.checked_add(job.exact_decode_tokens).is_none_or(|n| n > job.max_context_tokens)
        {
            return Err(CheckError::Binding);
        }
        if registered.class_id(&artifact) != job.shape_profile_id || registered.tokenizer_id != job.tokenizer_id {
            return Err(CheckError::Binding);
        }
        let space = PalwTirStepSpaceV1::new(registered).map_err(|_| CheckError::Binding)?;
        if space.program.blocks.iter().any(|block| block.nodes.iter().any(|node| !node.commit)) {
            return Err(CheckError::UnsupportedProfile);
        }
        if Hash64::from_bytes(space.program.logits_scheme_id) != tiled_logits_scheme_id_v1()
            || job.trace_scheme_id != tiled_logits_scheme_id_v1()
        {
            return Err(CheckError::UnsupportedProfile);
        }
        let logits = &space.program.blocks[space.program.schedule.post as usize].nodes[space.program.logits as usize];
        if logits.out.dtype != DType::I32 {
            return Err(CheckError::UnsupportedProfile);
        }
        let vocab = logits
            .out
            .shape
            .iter()
            .try_fold(1u64, |n, dim| match dim {
                Dim::Fixed(d) => n.checked_mul(*d as u64),
                _ => None,
            })
            .ok_or(CheckError::UnsupportedProfile)?;
        if vocab == 0 || vocab > u32::MAX as u64 {
            return Err(CheckError::UnsupportedProfile);
        }
        let leaves = space.leaf_count_capped(job, limits.max_leaves).map_err(|_| CheckError::Binding)?;
        if leaves == 0
            || job.exact_decode_tokens == 0
            || limits.max_part_bytes == 0
            || limits.max_part_heap == 0
            || limits.max_elements_per_leaf == 0
            || limits.max_terms_per_leaf == 0
        {
            return Err(CheckError::Budget);
        }
        let reserve = |unit: u64, cap: u64| leaves.checked_mul(unit).filter(|x| *x <= cap).ok_or(CheckError::Budget);
        let cost = CostEnvelope {
            parts: leaves,
            encoded_bytes: reserve(limits.max_part_bytes, limits.max_total_bytes)?,
            evaluation_elements: reserve(limits.max_elements_per_leaf, limits.max_total_elements)?,
            evaluation_terms: reserve(limits.max_terms_per_leaf, limits.max_total_terms)?,
            decoded_part_bytes: limits.max_part_heap,
        };
        Ok(Self { space, job: job.clone(), artifact, program_bytes: registered.program.len(), limits, form, leaves, vocab, cost })
    }
    pub fn leaf_count(&self) -> u64 {
        self.leaves
    }
    pub fn leaf(&self, index: u64) -> Option<PalwTirLeafV1> {
        (index < self.leaves).then(|| self.space.leaf_at(&self.job, index)).flatten()
    }
    pub fn cost(&self) -> CostEnvelope {
        self.cost
    }
    /// Validate the admitted Panel binding by reference before cloning a
    /// candidate's vectors or hashing its generated output. Shape equality is
    /// checked first so foreign oversized program/layout/token vectors cannot
    /// trigger serialization or program decoding as a side effect of rejection.
    pub fn validate_witness(&self, binding: &PalwTirStepBindingV1, generated: &[u32]) -> Result<(), CheckError> {
        if generated.len() != self.job.exact_decode_tokens as usize {
            return Err(CheckError::PanelEvidence);
        }
        if binding.job_context != self.job
            || binding.artifact_root != self.artifact
            || binding.class.program.len() != self.program_bytes
            || binding.class.layout != self.space.layout
            || binding.step_leaf_count != self.leaves
        {
            return Err(CheckError::Binding);
        }
        if binding.class.class_id(&self.artifact) != self.job.shape_profile_id {
            return Err(CheckError::Binding);
        }
        verify_tir_binding_v1(binding, self.limits.max_leaves).map_err(|_| CheckError::Binding)?;
        if generated.iter().any(|x| *x as u64 >= self.vocab) {
            return Err(CheckError::PanelEvidence);
        }
        Ok(())
    }
    fn rules(&self) -> PalwTirCourtRulesV1 {
        PalwTirCourtRulesV1 {
            max_step_leaf_count: self.limits.max_leaves,
            prompt_form: self.form,
            limits: DemandLimits { max_elements: self.limits.max_elements_per_leaf, max_terms: self.limits.max_terms_per_leaf },
        }
    }
}

/// Computation result only. A0/A4 retain ownership of case identity, admission,
/// role signatures, deadlines and verdicts; A5 retains Gate C and payment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckOutcome {
    Checking,
    Matched,
    OutputMismatch,
    WitnessRejected,
    EndedUnverified,
}

/// Read-only diagnostics, never a restart token or an authority to skip work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckProgress {
    pub next_leaf: u64,
    pub checked_rows: u32,
    pub next_lane: u64,
    pub charged_bytes: u64,
    pub charged_elements: u64,
    pub charged_terms: u64,
    pub outcome: CheckOutcome,
}

/// Bounded active state; owns no model weights or dense execution trace. A
/// restart must replay authenticated parts, rather than restoring a success bit.
pub struct ExactChecker {
    plan: CoveragePlan,
    binding: PalwTirStepBindingV1,
    generated: Vec<u32>,
    expected_output: Hash64,
    progress: CheckProgress,
    best: Option<(i32, u32)>,
}

impl ExactChecker {
    /// `binding` and `generated` must be the authenticated Panel commitment;
    /// `expected_output` must be the separately authenticated Miner result.
    /// Fixed-length greedy semantics are the only supported sampling profile.
    pub fn begin_greedy(
        plan: CoveragePlan,
        binding: PalwTirStepBindingV1,
        generated: Vec<u32>,
        expected_output: Hash64,
        sampling: PalwDecodeSamplingV2,
    ) -> Result<Self, CheckError> {
        if !sampling.is_greedy() {
            return Err(CheckError::UnsupportedProfile);
        }
        plan.validate_witness(&binding, &generated)?;
        Ok(Self {
            plan,
            binding,
            generated,
            expected_output,
            best: None,
            progress: CheckProgress {
                next_leaf: 0,
                checked_rows: 0,
                next_lane: 0,
                charged_bytes: 0,
                charged_elements: 0,
                charged_terms: 0,
                outcome: CheckOutcome::Checking,
            },
        })
    }
    pub fn progress(&self) -> CheckProgress {
        self.progress
    }

    /// Scan lengths before allocating; failed attempts spend their reservation.
    /// Any stopped candidate is terminal, so replay cannot reset its work budget.
    pub fn submit_encoded(&mut self, cone: &[u8], trace: Option<&[u8]>) -> Result<(), CheckError> {
        if self.progress.outcome != CheckOutcome::Checking {
            return Err(CheckError::Closed);
        }
        let result = self.submit_inner(cone, trace);
        if let Err(error) = &result {
            self.progress.outcome =
                if *error == CheckError::Unverified { CheckOutcome::EndedUnverified } else { CheckOutcome::WitnessRejected };
        }
        result
    }
    fn submit_inner(&mut self, cone: &[u8], trace: Option<&[u8]>) -> Result<(), CheckError> {
        if self.progress.next_leaf >= self.plan.leaves {
            return Err(CheckError::Order);
        }
        let bytes = (cone.len() as u64).checked_add(trace.map_or(0, |b| b.len() as u64)).ok_or(CheckError::Budget)?;
        if bytes > self.plan.limits.max_part_bytes {
            return Err(CheckError::Budget);
        }
        let next_bytes = self
            .progress
            .charged_bytes
            .checked_add(bytes)
            .filter(|x| *x <= self.plan.limits.max_total_bytes)
            .ok_or(CheckError::Budget)?;
        let elements = self
            .progress
            .charged_elements
            .checked_add(self.plan.limits.max_elements_per_leaf)
            .filter(|x| *x <= self.plan.limits.max_total_elements)
            .ok_or(CheckError::Budget)?;
        let terms = self
            .progress
            .charged_terms
            .checked_add(self.plan.limits.max_terms_per_leaf)
            .filter(|x| *x <= self.plan.limits.max_total_terms)
            .ok_or(CheckError::Budget)?;
        self.progress.charged_bytes = next_bytes;
        self.progress.charged_elements = elements;
        self.progress.charged_terms = terms;
        let heap = scan::<PalwTirConeRefutationV1>(cone)?
            .checked_add(trace.map(scan::<PalwTirTraceEventDisclosureV1>).transpose()?.unwrap_or(0))
            .ok_or(CheckError::Budget)?;
        if heap > self.plan.limits.max_part_heap {
            return Err(CheckError::Budget);
        }
        let cone: PalwTirConeRefutationV1 = borsh::from_slice(cone).map_err(|_| CheckError::Wire)?;
        let trace: Option<PalwTirTraceEventDisclosureV1> = trace.map(borsh::from_slice).transpose().map_err(|_| CheckError::Wire)?;
        self.check_part(&cone, trace.as_ref())?;
        self.progress.next_leaf += 1;
        Ok(())
    }
    fn check_part(&mut self, cone: &PalwTirConeRefutationV1, trace: Option<&PalwTirTraceEventDisclosureV1>) -> Result<(), CheckError> {
        if cone.binding != self.binding {
            return Err(CheckError::Binding);
        }
        if cone.output_opening.leaf_index != self.progress.next_leaf {
            return Err(CheckError::Order);
        }
        if let Some(pin) = &cone.decode_tokens {
            match pin {
                PalwDecodeTokenPinV1::TiledV1(pin) if pin.generated_token_ids == self.generated => {}
                _ => return Err(CheckError::PanelEvidence),
            }
        }
        exact(check_tir_cone_refutation_v1(cone, &self.plan.rules()))?;
        let leaf = self.plan.leaf(self.progress.next_leaf).ok_or(CheckError::Order)?;
        let PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } = leaf.kind else {
            return if trace.is_none() { Ok(()) } else { Err(CheckError::PanelEvidence) };
        };
        if occurrence as usize != self.plan.space.occurrences().len() - 1 || node != self.plan.space.program.logits {
            return if trace.is_none() { Ok(()) } else { Err(CheckError::PanelEvidence) };
        }
        let row = leaf
            .position
            .checked_add(1)
            .and_then(|x| x.checked_sub(self.plan.job.declared_prefill_tokens))
            .ok_or(CheckError::Order)?;
        if row != self.progress.checked_rows || first_element != self.progress.next_lane {
            return Err(CheckError::Order);
        }
        let Some(PalwTirTraceEventDisclosureV1::Tiled {
            binding,
            generated_token_ids,
            row_root,
            row_opening,
            tile_lanes,
            tile_opening,
        }) = trace
        else {
            return Err(CheckError::Incomplete);
        };
        if binding.as_ref() != &self.binding || generated_token_ids != &self.generated {
            return Err(CheckError::Binding);
        }
        exact(check_tir_logits_consistency_v1(
            &PalwTirLogitsConsistencyV1 {
                binding: cone.binding.clone(),
                step_opening: cone.output_opening.clone(),
                step_preimage: cone.output_preimage.clone(),
                trace: PalwTirTraceLanesV1::Tiled {
                    generated_token_ids: generated_token_ids.clone(),
                    row_root: *row_root,
                    row_opening: row_opening.clone(),
                    tile_lanes: tile_lanes.clone(),
                    tile_opening: tile_opening.clone(),
                },
            },
            &self.plan.rules(),
        ))?;
        if cone.output_preimage.values_le.len() != leaf.value_count as usize * 4 {
            return Err(CheckError::PanelEvidence);
        }
        for bytes in cone.output_preimage.values_le.chunks_exact(4) {
            let value = i32::from_le_bytes(bytes.try_into().map_err(|_| CheckError::Wire)?);
            if self.best.is_none_or(|(best, _)| value > best) {
                self.best = Some((value, self.progress.next_lane as u32));
            }
            self.progress.next_lane += 1;
        }
        if self.progress.next_lane > self.plan.vocab {
            return Err(CheckError::Order);
        }
        if self.progress.next_lane == self.plan.vocab {
            if self.best.map(|x| x.1) != self.generated.get(row as usize).copied() {
                return Err(CheckError::PanelEvidence);
            }
            self.progress.checked_rows += 1;
            self.progress.next_lane = 0;
            self.best = None;
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<CheckOutcome, CheckError> {
        if self.progress.outcome != CheckOutcome::Checking {
            return Err(CheckError::Closed);
        }
        if self.progress.next_leaf != self.plan.leaves
            || self.progress.checked_rows != self.plan.job.exact_decode_tokens
            || self.progress.next_lane != 0
        {
            return Err(CheckError::Incomplete);
        }
        self.progress.outcome = if palw_attempt_output_root_v1(&self.plan.job, &self.generated) == self.expected_output {
            CheckOutcome::Matched
        } else {
            CheckOutcome::OutputMismatch
        };
        Ok(self.progress.outcome)
    }
}

fn exact<T>(result: Result<T, PalwStepRefuteError>) -> Result<(), CheckError> {
    match result {
        Err(PalwStepRefuteError::NoFaultFound) => Ok(()),
        Err(PalwStepRefuteError::Unadjudicable | PalwStepRefuteError::WeightUnavailable) => Err(CheckError::Unverified),
        _ => Err(CheckError::PanelEvidence),
    }
}
fn scan<T: NativeWireScanV1>(bytes: &[u8]) -> Result<u64, CheckError> {
    preflight_native_value_v1::<T>(std::io::Cursor::new(bytes), bytes.len() as u64)
        .map(|shape| shape.logical_requests_v1())
        .map_err(|_| CheckError::Wire)
}
