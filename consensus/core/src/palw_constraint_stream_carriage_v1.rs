//! Signed prospective staged court moves. No unsigned byte may poison an owner's legacy leaf
//! continuation: intermediate chunks authenticate the owner even before the full leaf completes.
use crate::Hash64;
use crate::palw_constraint_stream_court_v1::{PalwConstraintStreamCourtV1, PalwConstraintStreamPhaseV1};
use crate::palw_freeprompt_v3::PalwFreePromptJobV3;
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step_leg::{PalwStepBindingV2, PalwStepOpeningV1};
use crate::palw_step_refute::PalwTiledDecodePinV1;
use borsh::{BorshDeserialize, BorshSerialize};
pub const PALW_CONSTRAINT_STREAM_CONTEXT_V1: &[u8] = b"misaka-palw/constraint-stream/v1/mldsa87";
pub const PALW_CONSTRAINT_STREAM_TAIL_V1: u8 = 0xF8;
pub const PALW_PREFIX_BATCH_MAX_ACTIONS_V1: usize = 64;
pub const PALW_PREFIX_BATCH_ENVELOPE_BYTES_V1: usize = 16_384;
pub const PALW_PREFIX_BATCH_CARRIER_BYTES_V1: usize = 90_000;
/// Seed transport is independent of a legacy direct objective offence cap. It is still bounded
/// by the admitted court carrier ceiling, never a limit chosen by the challenger.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwAuxiliaryPublicCourtSeedV1 {
    /// The original registered table leaf; its complete bytes authenticate only at EOF.
    Tokenizer {
        id: u32,
        present: bool,
        length: u32,
        opening: PalwStepOpeningV1,
        raw_tokenizer_bytes: u64,
        tokenizer_key_events: u64,
        /// Verifier-held operand length; authenticated only by the original flat hash at EOF.
        operand_tokenizer_bytes: u64,
    },
    /// Must equal the outer rule-owned class step ladder; no prover-selected weakening.
    Prefix {
        maximum_leaves: u64,
        /// Fee metadata must equal the literal accepted prefix job and immutable relation.
        prefix_tokens: u32,
        relation: crate::palw_class_semantics_v1::PalwPrefixStateRelationV1,
    },
    PublicPrefix {
        maximum_leaves: u64,
        prefix_tokens: u32,
        relation: crate::palw_class_semantics_v1::PalwPrefixStateRelationV1,
        /// Exact producer, qualified current Panel and independent owner slot count.
        slots: u32,
    },
    FlatConstraint {
        source_id: Hash64,
        answerer: PalwBondKeyV2,
    },
    /// Greedy Float32 Constraint over the accepted Float32 source slot of `answerer`. Appended
    /// form owned by `palw_staged_float_constraint_v1` (rides unjudged below it, A-2 uniform).
    FloatConstraint {
        source_id: Hash64,
        answerer: PalwBondKeyV2,
    },
}
/// Content-addressed profile already retained by the claim's class certification. The byte
/// quote prices hydration; it cannot substitute a different graph for that historic class.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRetainedCourtProfileV1 {
    pub class_id: Hash64,
    pub original_bytes: u64,
}
pub fn palw_retained_profile_skeleton_v1(profile: &crate::palw_step::PalwShapeProfileV3) -> crate::palw_step::PalwShapeProfileV3 {
    let mut result = profile.clone();
    for table in [&mut result.pre_nodes, &mut result.gdn_nodes, &mut result.attn_nodes, &mut result.post_nodes] {
        for node in table {
            node.weight_name.clear();
            node.weight_dtypes.clear();
        }
    }
    result.transcendental_bindings.clear();
    result.contraction_facts.clear();
    result
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwConstraintStreamSeedV1 {
    pub version: u16,
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub binding: PalwStepBindingV2,
    /// None reads the literal retained claim-rooted job; Some must equal that exact job.
    pub job: Option<Box<PalwFreePromptJobV3>>,
    pub pin: PalwTiledDecodePinV1,
    pub witness: Option<u32>,
    pub eog_count: u32,
    pub eog_opening: PalwStepOpeningV1,
    /// Prospective typed court selection. None preserves constrained-selection behavior.
    pub auxiliary: Option<PalwAuxiliaryPublicCourtSeedV1>,
    pub signature: Vec<u8>,
    pub retained_profile: Option<PalwRetainedCourtProfileV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwConstraintStreamLeafV1 {
    pub version: u16,
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub seed_id: Hash64,
    pub id: u32,
    pub present: bool,
    pub length: u32,
    pub opening: PalwStepOpeningV1,
    pub signature: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwConstraintStreamActionV1 {
    EogPart {
        offset: u32,
        ids: Vec<u32>,
    },
    EogFinish,
    RenderingPart {
        offset: u32,
        bytes: Vec<u8>,
    },
    RenderingFinish,
    EogTokenFinish,
    TokenizerPacket(crate::palw_tokenizer_table_stream_v1::PalwTokenizerStreamPacketV1),
    TokenizerSeal,
    TokenizerRenderingPart {
        offset: u32,
        bytes: Vec<u8>,
    },
    Prefix(crate::palw_prefix_family_court_v1::PalwPrefixCourtActionV1),
    PrefixPublicDemand(crate::palw_prefix_public_source_v1::PalwPrefixPublicDemandV1),
    PrefixPublicAnswer(crate::palw_prefix_public_source_v1::PalwPrefixPublicAnswerV1),
    /// Flat normal actions only. The inner family enum has no Batch variant, so arbitrary
    /// nested recursive deserialization is impossible. Work remains charged per action.
    PrefixBatch(Vec<crate::palw_prefix_family_court_v1::PalwPrefixCourtActionV1>),
    /// Owner-selected collection cannot be starved by the bounded maintenance sweep.
    PrefixPublicCollect {
        cursor_id: Hash64,
        demand_id: Hash64,
    },
    BundleOpen {
        plan: crate::palw_staged_bundle_v1::PalwStagedBundlePlanV1,
    },
    ActivateStage {
        previous_seed: Hash64,
        next_seed: Hash64,
    },
    InventoryPart {
        offset: u64,
        bytes: Vec<u8>,
    },
    InventoryFinish,
    SelectedEventDemand {
        binding: crate::palw_selected_event_source_v1::PalwSelectedEventBindingV1,
        demand: crate::palw_selected_event_source_v1::PalwSelectedEventDemandV1,
    },
    SelectedEventAnswer(crate::palw_selected_event_source_v1::PalwSelectedEventAnswerV1),
    SelectedEventCollect {
        coordinate: crate::palw_selected_event_source_v1::PalwSelectedEventCoordinateV1,
        demand_id: Hash64,
    },
    ArtifactInventoryOpen {
        class_id: Hash64,
        offer: crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryOfferV1,
    },
    ArtifactInventoryMove(crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryActionV1),
    HistoryCustodyGrant {
        grant: crate::palw_history_custody_v1::PalwHistoryCustodyGrantV1,
        receipt: crate::palw_panel_v2::PalwSeatReceiptV2,
    },
    HistoryRecovery {
        stage_seed: Hash64,
        request_head: Hash64,
        sequence: u64,
        demand_opened_daa: u64,
        action: crate::palw_history_custody_v1::PalwHistoryRecoveryActionV1,
    },
    FlatSourceOpen {
        binding: crate::palw_selected_event_source_v1::PalwSelectedEventBindingV1,
    },
    FlatSourceAnswer(crate::palw_flat_constraint_stream_v1::PalwFlatConstraintSourceAnswerV1),
    FlatSourceNominate {
        source_id: Hash64,
        position: u32,
        beat: Option<u32>,
    },
    FlatSourceCollect,
    // Greedy Float32 Constraint source: appended forms owned by `palw_staged_float_constraint_v1`
    // (`crate::palw_staged_route_forms_v1`); below it they ride unjudged, A-2 uniform.
    FloatSourceOpen {
        binding: crate::palw_selected_event_source_v1::PalwSelectedEventBindingV1,
    },
    FloatSourceAnswer(crate::palw_float_constraint_source_v1::PalwFloatSourceAnswerV1),
    FloatSourceNominate {
        source_id: Hash64,
        position: u32,
        beat: Option<u32>,
    },
    FloatSourceCollect,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwConstraintStreamMoveV1 {
    pub version: u16,
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub seed_id: Hash64,
    /// Explicit phase prevents replaying an equal-offset chunk for another token occurrence.
    pub phase: PalwConstraintStreamPhaseV1,
    pub action: PalwConstraintStreamActionV1,
    pub signature: Vec<u8>,
}
fn message(label: &[u8], fields: &[&[u8]]) -> Vec<u8> {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/constraint-stream/move/v1").to_state();
    h.update(&(label.len() as u32).to_le_bytes());
    h.update(label);
    for field in fields {
        h.update(&(field.len() as u64).to_le_bytes());
        h.update(field);
    }
    h.finalize().as_bytes().to_vec()
}
fn encode<T: BorshSerialize>(t: &T) -> Vec<u8> {
    borsh::to_vec(t).expect("stream types serialize")
}
pub(crate) fn bounded_encoded_len<T: BorshSerialize>(value: &T, cap: usize) -> Result<usize, String> {
    struct Counter {
        used: usize,
        cap: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            let n = self.used.checked_add(b.len()).filter(|n| *n <= self.cap).ok_or(std::io::ErrorKind::InvalidData)?;
            self.used = n;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { used: 0, cap };
    value.serialize(&mut counter).map_err(|_| "staged signed carrier/body exceeds its fixed bound".to_string())?;
    Ok(counter.used)
}
fn check_signature_shape(version: u16, signature: &[u8]) -> Result<(), String> {
    if version != 1 || signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
        return Err("stream move version/signature is outside fixed owner format".into());
    }
    Ok(())
}
impl PalwConstraintStreamSeedV1 {
    pub fn signing_message(&self) -> Vec<u8> {
        message(
            b"seed",
            &[
                &encode(&self.version),
                &encode(&self.claim),
                &encode(&self.reserver),
                &encode(&self.reserved_daa),
                &encode(&self.binding),
                &encode(&self.job),
                &encode(&self.pin),
                &encode(&self.witness),
                &encode(&self.eog_count),
                &encode(&self.eog_opening),
                &encode(&self.auxiliary),
                &encode(&self.retained_profile),
            ],
        )
    }
    /// Prepare a bounded reference before signing. Hydration still authenticates the literal
    /// retained chain profile and accepted claim independently; this helper grants no authority.
    pub fn use_retained_profile_v1(&mut self) -> Result<(), String> {
        if self.retained_profile.is_some() {
            return Err("seed already references an immutable profile".into());
        }
        let profile = &self.binding.shape_profile;
        let class_id = profile.shape_profile_id();
        if self.binding.job_context.shape_profile_id != class_id {
            return Err("seed profile is not the signed binding's declared class".into());
        }
        let original_bytes = borsh::to_vec(profile).map_err(|e| e.to_string())?.len() as u64;
        self.binding.shape_profile = palw_retained_profile_skeleton_v1(profile);
        self.retained_profile = Some(PalwRetainedCourtProfileV1 { class_id, original_bytes });
        Ok(())
    }
    pub fn resolved_binding_v1(
        &self,
        actual_class: Hash64,
        profile: Option<&crate::palw_step::PalwShapeProfileV3>,
    ) -> Result<PalwStepBindingV2, String> {
        let Some(reference) = &self.retained_profile else { return Ok(self.binding.clone()) };
        let profile = profile.ok_or("staged reference has no retained authenticated class profile")?;
        if reference.class_id != actual_class
            || self.binding.job_context.shape_profile_id != actual_class
            || profile.shape_profile_id() != actual_class
            || borsh::to_vec(profile).map_err(|e| e.to_string())?.len() as u64 != reference.original_bytes
            || self.binding.shape_profile != palw_retained_profile_skeleton_v1(profile)
        {
            return Err("staged profile reference changes accepted class identity, bytes or exact skeleton".into());
        }
        let mut result = self.binding.clone();
        result.shape_profile = profile.clone();
        Ok(result)
    }
    pub fn check(&self, carrier_maximum: u64) -> Result<(), String> {
        check_signature_shape(self.version, &self.signature)?;
        struct Counter(u64);
        impl std::io::Write for Counter {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0 = self.0.checked_add(b.len() as u64).ok_or(std::io::ErrorKind::InvalidData)?;
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut n = Counter(0);
        self.serialize(&mut n).map_err(|e| e.to_string())?;
        if n.0 > carrier_maximum {
            return Err("stream seed exceeds admitted bounded carrier".into());
        }
        Ok(())
    }
    pub fn court(&self, expected_claim_root: Hash64, ladder: u64) -> Result<PalwConstraintStreamCourtV1, String> {
        let job = self.job.as_deref().ok_or("unrooted seed cannot resolve its public job reference")?;
        self.court_with_rooted_job(expected_claim_root, ladder, job)
    }
    /// Local replay helper only. Native admission obtains `pin` from its accepted
    /// literal source row, never from a caller's serialized pin or checkpoint.
    pub fn court_from_authenticated_flat_pin_v1(
        &self,
        root: Hash64,
        job: &PalwFreePromptJobV3,
        binding: &PalwStepBindingV2,
        pin: &crate::palw_flat_constraint_stream_v1::PalwFlatConstraintPinV1,
    ) -> Result<PalwConstraintStreamCourtV1, String> {
        self.check(90_000)?;
        if !matches!(self.auxiliary, Some(PalwAuxiliaryPublicCourtSeedV1::FlatConstraint { .. }))
            || self.resolved_binding_v1(job.class_id, Some(&binding.shape_profile))? != *binding
            || self.job.as_deref().is_some_and(|actual| actual != job)
        {
            return Err("flat local replay substitutes original signed auxiliary or public context".into());
        }
        let seed = Hash64::from_bytes(self.signing_message().try_into().map_err(|_| "flat original seed digest width")?);
        PalwConstraintStreamCourtV1::new_from_flat_pin_with_seed_context_v1(
            self.claim,
            self.reserver,
            self.reserved_daa,
            root,
            binding,
            job,
            pin,
            self.witness,
            self.eog_count,
            &self.eog_opening,
            Some(seed),
        )
    }
    /// Local replay helper only (the Float32 twin of [`Self::court_from_authenticated_flat_pin_v1`]).
    /// Native admission obtains `pin` from its accepted literal source slot.
    pub fn court_from_authenticated_float_pin_v1(
        &self,
        root: Hash64,
        original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        binding: &PalwStepBindingV2,
        pin: &crate::palw_float_constraint_source_v1::PalwFloatConstraintPinV1,
    ) -> Result<PalwConstraintStreamCourtV1, String> {
        self.check(90_000)?;
        let job = &original.job;
        if !matches!(self.auxiliary, Some(PalwAuxiliaryPublicCourtSeedV1::FloatConstraint { .. }))
            || self.resolved_binding_v1(job.class_id, Some(&binding.shape_profile))? != *binding
            || self.job.as_deref().is_some_and(|actual| actual != job)
        {
            return Err("Float32 local replay substitutes original signed auxiliary or public context".into());
        }
        let seed = Hash64::from_bytes(self.signing_message().try_into().map_err(|_| "Float32 original seed digest width")?);
        PalwConstraintStreamCourtV1::new_from_float_pin_with_original_stop_context_v1(
            self.claim,
            self.reserver,
            self.reserved_daa,
            root,
            binding,
            job,
            pin,
            self.witness,
            self.eog_count,
            &self.eog_opening,
            original,
            Some(seed),
        )
    }
    pub fn court_with_rooted_job(
        &self,
        expected_claim_root: Hash64,
        ladder: u64,
        actual_job: &PalwFreePromptJobV3,
    ) -> Result<PalwConstraintStreamCourtV1, String> {
        if self.retained_profile.is_some() {
            return Err("unrooted helper cannot hydrate retained profile".into());
        }
        self.court_with_resolved_binding_v1(expected_claim_root, ladder, actual_job, &self.binding)
    }
    pub fn court_with_resolved_binding_v1(
        &self,
        expected_claim_root: Hash64,
        ladder: u64,
        actual_job: &PalwFreePromptJobV3,
        binding: &PalwStepBindingV2,
    ) -> Result<PalwConstraintStreamCourtV1, String> {
        let resolved = self.resolved_binding_v1(actual_job.class_id, Some(&binding.shape_profile))?;
        if &resolved != binding {
            return Err("hydration changes signed non-profile binding fields".into());
        }
        if self.job.as_deref().is_some_and(|j| j != actual_job) {
            return Err("stream seed changes literal retained job".into());
        }
        if self.auxiliary.is_some() {
            return Err("auxiliary seed is not a constrained-selection court".into());
        }
        PalwConstraintStreamCourtV1::new_with_seed_context_v1(
            self.claim,
            self.reserver,
            self.reserved_daa,
            expected_claim_root,
            binding,
            actual_job,
            &self.pin,
            self.witness,
            self.eog_count,
            &self.eog_opening,
            ladder,
            self.retained_profile.as_ref().map(|_| Hash64::from_bytes(self.signing_message().try_into().expect("seed digest width"))),
        )
    }
}
impl PalwConstraintStreamLeafV1 {
    pub fn signing_message(&self) -> Vec<u8> {
        message(
            b"leaf",
            &[
                &encode(&self.version),
                &encode(&self.claim),
                &encode(&self.reserver),
                &encode(&self.reserved_daa),
                &encode(&self.seed_id),
                &encode(&self.id),
                &encode(&self.present),
                &encode(&self.length),
                &encode(&self.opening),
            ],
        )
    }
    pub fn check(&self) -> Result<(), String> {
        check_signature_shape(self.version, &self.signature)?;
        if self.opening.siblings.len() > 23 {
            return Err("stream leaf opening exceeds global table proof depth".into());
        }
        Ok(())
    }
}
impl PalwConstraintStreamMoveV1 {
    /// Bind every signed part to the accepted backwards cursor and demand. A
    /// previously signed part must not become valid for the next record's slot.
    pub fn check_history_scope_v1(&self, recovery: &crate::palw_history_custody_v1::PalwHistoryRecoveryV1) -> Result<(), String> {
        let PalwConstraintStreamActionV1::HistoryRecovery { stage_seed, request_head, sequence, demand_opened_daa, action } =
            &self.action
        else {
            return Err("not a history recovery carriage".into());
        };
        let request = recovery.remaining_request_v1().ok_or("history traversal has already reached genesis")?;
        let opened = if matches!(action, crate::palw_history_custody_v1::PalwHistoryRecoveryActionV1::Demand) {
            0
        } else {
            recovery.demand.as_ref().ok_or("history answer has no accepted literal demand")?.opened_daa
        };
        if (self.claim, self.reserver, self.reserved_daa, *stage_seed, *request_head, *sequence, *demand_opened_daa)
            != (request.claim, request.owner, request.reserved_daa, request.seed_id, request.head, request.sequence, opened)
        {
            return Err("signed history carriage substitutes current record head, sequence or original demand clock".into());
        }
        Ok(())
    }
    /// Only this typed Answer changes signature authority. Every historic/owned action
    /// remains owner-signed, including public-source Demand and local prefix continuation.
    pub fn authorization_bond_v1(&self) -> PalwBondKeyV2 {
        match &self.action {
            PalwConstraintStreamActionV1::PrefixPublicAnswer(answer) => answer.answerer,
            PalwConstraintStreamActionV1::SelectedEventAnswer(answer) => answer.answerer,
            PalwConstraintStreamActionV1::HistoryCustodyGrant { grant, .. } => grant.seat,
            PalwConstraintStreamActionV1::HistoryRecovery { action, .. } => action.answerer_v1().unwrap_or(self.reserver),
            PalwConstraintStreamActionV1::FlatSourceAnswer(answer) => answer.answerer,
            PalwConstraintStreamActionV1::FloatSourceAnswer(answer) => answer.answerer,
            _ => self.reserver,
        }
    }
    pub fn signing_message(&self) -> Vec<u8> {
        message(
            b"move",
            &[
                &encode(&self.version),
                &encode(&self.claim),
                &encode(&self.reserver),
                &encode(&self.reserved_daa),
                &encode(&self.seed_id),
                &encode(&self.phase),
                &encode(&self.action),
            ],
        )
    }
    pub fn check(&self) -> Result<(), String> {
        check_signature_shape(self.version, &self.signature)?;
        if matches!(
            &self.action,
            PalwConstraintStreamActionV1::BundleOpen { .. }
                | PalwConstraintStreamActionV1::ActivateStage { .. }
                | PalwConstraintStreamActionV1::InventoryPart { .. }
                | PalwConstraintStreamActionV1::InventoryFinish
                | PalwConstraintStreamActionV1::SelectedEventDemand { .. }
                | PalwConstraintStreamActionV1::SelectedEventAnswer(_)
                | PalwConstraintStreamActionV1::SelectedEventCollect { .. }
                | PalwConstraintStreamActionV1::ArtifactInventoryOpen { .. }
                | PalwConstraintStreamActionV1::ArtifactInventoryMove(_)
                | PalwConstraintStreamActionV1::HistoryCustodyGrant { .. }
                | PalwConstraintStreamActionV1::HistoryRecovery { .. }
                | PalwConstraintStreamActionV1::FlatSourceOpen { .. }
                | PalwConstraintStreamActionV1::FlatSourceAnswer(_)
                | PalwConstraintStreamActionV1::FlatSourceNominate { .. }
                | PalwConstraintStreamActionV1::FlatSourceCollect
                | PalwConstraintStreamActionV1::FloatSourceOpen { .. }
                | PalwConstraintStreamActionV1::FloatSourceAnswer(_)
                | PalwConstraintStreamActionV1::FloatSourceNominate { .. }
                | PalwConstraintStreamActionV1::FloatSourceCollect
        ) {
            if self.phase != PalwConstraintStreamPhaseV1::PrefixCourt {
                return Err("bundle control changes its fixed carrier phase".into());
            }
            bounded_encoded_len(self, PALW_PREFIX_BATCH_CARRIER_BYTES_V1)?;
            if let PalwConstraintStreamActionV1::BundleOpen { plan } = &self.action {
                plan.check_v1()?;
            }
            if let PalwConstraintStreamActionV1::InventoryPart { bytes, .. } = &self.action
                && (bytes.is_empty() || bytes.len() > 65_536)
            {
                return Err("inventory packet exceeds fixed raw-hash chunk".into());
            }
        }
        if let PalwConstraintStreamActionV1::SelectedEventAnswer(answer) = &self.action {
            answer.check_v1()?;
        }
        if let PalwConstraintStreamActionV1::FlatSourceAnswer(answer) = &self.action {
            answer.action.check_wire_v1()?;
        }
        if let PalwConstraintStreamActionV1::FloatSourceAnswer(answer) = &self.action {
            answer.action.check_wire_v1()?;
        }
        if let PalwConstraintStreamActionV1::HistoryCustodyGrant { grant, receipt } = &self.action {
            grant.check_receipt_v1(receipt)?;
        }
        if let PalwConstraintStreamActionV1::HistoryRecovery { action, .. } = &self.action {
            action.check_wire_v1()?;
        }
        if let PalwConstraintStreamActionV1::ArtifactInventoryOpen { offer, .. } = &self.action {
            offer.check_v1()?;
        }
        if let PalwConstraintStreamActionV1::ArtifactInventoryMove(
            crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryActionV1::Part { bytes, .. },
        ) = &self.action
            && (bytes.is_empty() || bytes.len() > 65_536)
        {
            return Err("artifact inventory part exceeds native bounded carrier".into());
        }
        if let PalwConstraintStreamActionV1::PrefixPublicAnswer(answer) = &self.action {
            answer.check()?;
            bounded_encoded_len(self, PALW_PREFIX_BATCH_CARRIER_BYTES_V1)?;
        }
        if matches!(
            &self.action,
            PalwConstraintStreamActionV1::PrefixPublicDemand(_)
                | PalwConstraintStreamActionV1::PrefixPublicAnswer(_)
                | PalwConstraintStreamActionV1::PrefixBatch(_)
                | PalwConstraintStreamActionV1::PrefixPublicCollect { .. }
        ) && self.phase != PalwConstraintStreamPhaseV1::PrefixCourt
        {
            return Err("public prefix acquisition changes typed prefix phase".into());
        }
        if let PalwConstraintStreamActionV1::PrefixBatch(actions) = &self.action {
            if actions.is_empty() || actions.len() > PALW_PREFIX_BATCH_MAX_ACTIONS_V1 {
                return Err("prefix batch has no actions or exceeds fixed nonrecursive logical bound".into());
            }
            bounded_encoded_len(actions, PALW_PREFIX_BATCH_CARRIER_BYTES_V1 - PALW_PREFIX_BATCH_ENVELOPE_BYTES_V1)?;
            bounded_encoded_len(self, PALW_PREFIX_BATCH_CARRIER_BYTES_V1)?;
        }
        match &self.action {
            PalwConstraintStreamActionV1::EogPart { ids, .. }
                if ids.is_empty() || ids.len() > crate::palw_eog_stream_v1::PALW_EOG_STREAM_PART_IDS_V1 =>
            {
                Err("stream EOG move exceeds fixed bounded work".into())
            }
            PalwConstraintStreamActionV1::RenderingPart { bytes, .. }
                if bytes.is_empty() || bytes.len() > crate::palw_rendering_stream_v1::PALW_RENDERING_STREAM_PART_BYTES_V1 =>
            {
                Err("stream rendering move exceeds fixed bounded work".into())
            }
            PalwConstraintStreamActionV1::TokenizerPacket(packet)
                if packet.bytes.len() > 65536
                    || packet.proofs.len() > 64
                    || borsh::to_vec(packet).map_err(|e| e.to_string())?.len() > 90000 =>
            {
                Err("tokenizer packet exceeds admitted bounded carrier".into())
            }
            PalwConstraintStreamActionV1::TokenizerRenderingPart { bytes, .. } if bytes.is_empty() || bytes.len() > 65536 => {
                Err("tokenizer rendering part exceeds bounded work".into())
            }
            PalwConstraintStreamActionV1::Prefix(action) if borsh::to_vec(action).map_err(|e| e.to_string())?.len() > 90000 => {
                Err("prefix action exceeds bounded staged carrier".into())
            }
            _ => Ok(()),
        }
    }
    /// Acceptance checks the owner signature AND actual rooted claim/reservation BEFORE this.
    pub fn apply_to(&self, court: &mut PalwConstraintStreamCourtV1) -> Result<(), String> {
        self.check()?;
        if self.seed_id != court.seed_id() || self.phase != court.phase() {
            return Err("stream move changes rooted seed/token phase".into());
        }
        match &self.action {
            PalwConstraintStreamActionV1::EogPart { offset, ids } => court.push_eog(self.seed_id, *offset, ids),
            PalwConstraintStreamActionV1::EogFinish => court.finish_eog(),
            PalwConstraintStreamActionV1::RenderingPart { offset, bytes } => court.push_rendering(self.seed_id, *offset, bytes),
            PalwConstraintStreamActionV1::RenderingFinish => court.finish_rendering(),
            PalwConstraintStreamActionV1::EogTokenFinish => court.finish_eog_token(self.seed_id),
            PalwConstraintStreamActionV1::Prefix(_)
            | PalwConstraintStreamActionV1::PrefixPublicDemand(_)
            | PalwConstraintStreamActionV1::PrefixPublicAnswer(_)
            | PalwConstraintStreamActionV1::PrefixBatch(_)
            | PalwConstraintStreamActionV1::PrefixPublicCollect { .. }
            | PalwConstraintStreamActionV1::TokenizerPacket(_)
            | PalwConstraintStreamActionV1::TokenizerSeal
            | PalwConstraintStreamActionV1::TokenizerRenderingPart { .. } => {
                Err("typed auxiliary move is not a constrained court action".into())
            }
            PalwConstraintStreamActionV1::BundleOpen { .. }
            | PalwConstraintStreamActionV1::ActivateStage { .. }
            | PalwConstraintStreamActionV1::InventoryPart { .. }
            | PalwConstraintStreamActionV1::InventoryFinish
            | PalwConstraintStreamActionV1::SelectedEventDemand { .. }
            | PalwConstraintStreamActionV1::SelectedEventAnswer(_)
            | PalwConstraintStreamActionV1::SelectedEventCollect { .. }
            | PalwConstraintStreamActionV1::ArtifactInventoryOpen { .. }
            | PalwConstraintStreamActionV1::ArtifactInventoryMove(_)
            | PalwConstraintStreamActionV1::HistoryCustodyGrant { .. }
            | PalwConstraintStreamActionV1::HistoryRecovery { .. }
            | PalwConstraintStreamActionV1::FlatSourceOpen { .. }
            | PalwConstraintStreamActionV1::FlatSourceAnswer(_)
            | PalwConstraintStreamActionV1::FlatSourceNominate { .. }
            | PalwConstraintStreamActionV1::FlatSourceCollect => Err("bundle control is not a semantic court action".into()),
        }
    }
}
pub fn palw_staged_verify_mldsa87_v1(key: &[u8], message: &[u8], signature: &[u8], context: &[u8]) -> bool {
    let Ok(key): Result<[u8; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN], _> = key.try_into() else { return false };
    let Ok(signature): Result<[u8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN], _> = signature.try_into() else { return false };
    let key = libcrux_ml_dsa::ml_dsa_87::MLDSA87VerificationKey::new(key);
    let signature = libcrux_ml_dsa::ml_dsa_87::MLDSA87Signature::new(signature);
    libcrux_ml_dsa::ml_dsa_87::portable::verify(&key, message, context, &signature).is_ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{TransactionId, TransactionOutpoint};
    fn movement() -> PalwConstraintStreamMoveV1 {
        PalwConstraintStreamMoveV1 {
            version: 1,
            claim: Hash64::from_u64_word(1),
            reserver: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(2), index: 3 }),
            reserved_daa: 40,
            seed_id: Hash64::from_u64_word(5),
            phase: PalwConstraintStreamPhaseV1::Prefix(0),
            action: PalwConstraintStreamActionV1::RenderingPart { offset: 0, bytes: vec![b'a'] },
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }
    }
    #[test]
    fn signed_history_parts_bind_exact_current_record_and_accepted_demand_before_buffer_mutation() {
        use crate::palw_history_custody_v1::{PalwHistoryRecoveryActionV1 as Action, PalwHistoryRecoveryV1};
        let mut carrier = movement();
        let stage = Hash64::from_u64_word(50);
        let head = Hash64::from_u64_word(51);
        let signer = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(52), 0));
        let request = crate::palw_prefix_history_v1::PalwPrefixHistoryRequestV1 {
            claim: carrier.claim,
            owner: carrier.reserver,
            reserved_daa: carrier.reserved_daa,
            seed_id: stage,
            head,
            sequence: 2,
            material_until_daa: 1000,
        };
        let mut recovery = PalwHistoryRecoveryV1::new_v1(request, 50, 900, 950, 10, 1, [signer].into_iter().collect()).unwrap();
        carrier.phase = PalwConstraintStreamPhaseV1::PrefixCourt;
        carrier.action = PalwConstraintStreamActionV1::HistoryRecovery {
            stage_seed: stage,
            request_head: head,
            sequence: 2,
            demand_opened_daa: 0,
            action: Action::Demand,
        };
        carrier.check_history_scope_v1(&recovery).unwrap();
        recovery.apply_v1(&Action::Demand, 50).unwrap();
        carrier.action = PalwConstraintStreamActionV1::HistoryRecovery {
            stage_seed: stage,
            request_head: head,
            sequence: 2,
            demand_opened_daa: 50,
            action: Action::Begin { answerer: signer, total_bytes: 1 },
        };
        carrier.check_history_scope_v1(&recovery).unwrap();
        let original_message = carrier.signing_message();
        for field in 0..3 {
            let mut stale = carrier.clone();
            let PalwConstraintStreamActionV1::HistoryRecovery { request_head, sequence, demand_opened_daa, .. } = &mut stale.action
            else {
                unreachable!()
            };
            match field {
                0 => *request_head = Hash64::from_u64_word(99),
                1 => *sequence -= 1,
                _ => *demand_opened_daa += 1,
            }
            assert_ne!(stale.signing_message(), original_message);
            assert!(stale.check_history_scope_v1(&recovery).is_err());
            assert!(recovery.demand.as_ref().unwrap().buffers.is_empty());
        }
        let mut next = recovery.clone();
        next.remaining_sequence = 1;
        next.remaining_head = Hash64::from_u64_word(53);
        let next_demand = next.demand.as_mut().unwrap();
        next_demand.request.sequence = 1;
        next_demand.request.head = next.remaining_head;
        assert!(carrier.check_history_scope_v1(&next).is_err(), "old same-offset signed Begin cannot poison the next record slot");
        let shared: crate::palw_staged_court_state_v1::PalwSharedCourtV1<_> = Box::new(recovery.clone()).into();
        assert_eq!(borsh::to_vec(&shared).unwrap(), borsh::to_vec(&recovery).unwrap(), "COW is transparent on the rooted wire");
        let mut branch = shared.clone();
        assert!(std::ptr::eq(shared.as_ref(), branch.as_ref()));
        branch.apply_v1(&Action::Begin { answerer: signer, total_bytes: 1 }, 51).unwrap();
        assert!(!std::ptr::eq(shared.as_ref(), branch.as_ref()));
        assert!(shared.demand.as_ref().unwrap().buffers.is_empty());
        assert_eq!(branch.demand.as_ref().unwrap().buffers.len(), 1);
    }
    #[test]
    fn public_prefix_answer_signer_is_typed_and_every_literal_identity_is_signed() {
        use crate::palw_prefix_public_source_v1::{PalwPrefixPublicAnswerActionV1, PalwPrefixPublicAnswerV1};
        let mut answer = movement();
        let signer = PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(99), index: 3 });
        answer.phase = PalwConstraintStreamPhaseV1::PrefixCourt;
        answer.action = PalwConstraintStreamActionV1::PrefixPublicAnswer(PalwPrefixPublicAnswerV1 {
            answerer: signer,
            cursor_id: Hash64::from_u64_word(11),
            demand_id: Hash64::from_u64_word(12),
            action: PalwPrefixPublicAnswerActionV1::Finish,
        });
        assert_eq!(answer.authorization_bond_v1(), signer);
        answer.check().unwrap();
        let signed = answer.signing_message();
        for mutation in 0..7 {
            let mut changed = answer.clone();
            match mutation {
                0 => changed.reserver = signer,
                1 => changed.claim = Hash64::from_u64_word(100),
                2 => changed.reserved_daa += 1,
                3 => changed.seed_id = Hash64::from_u64_word(100),
                4 => changed.phase = PalwConstraintStreamPhaseV1::Prefix(0),
                _ => {
                    let PalwConstraintStreamActionV1::PrefixPublicAnswer(a) = &mut changed.action else { unreachable!() };
                    if mutation == 5 { a.answerer = answer.reserver } else { a.demand_id = Hash64::from_u64_word(100) }
                }
            }
            assert_ne!(changed.signing_message(), signed);
        }
        let owner_move = movement();
        assert_eq!(owner_move.authorization_bond_v1(), owner_move.reserver);
    }
    #[test]
    fn flat_prefix_batch_has_measured_signed_headroom_and_independent_body_action_bounds() {
        use crate::palw_prefix_family_court_v1::PalwPrefixCourtActionV1 as A;
        let mut packet = movement();
        packet.phase = PalwConstraintStreamPhaseV1::PrefixCourt;
        packet.action = PalwConstraintStreamActionV1::PrefixBatch(vec![A::FinishTile; 64]);
        packet.check().unwrap();
        let PalwConstraintStreamActionV1::PrefixBatch(actions) = &packet.action else { unreachable!() };
        let body = borsh::to_vec(actions).unwrap().len();
        let carrier = borsh::to_vec(&packet).unwrap().len();
        eprintln!(
            "prefix batch signed envelope exact_bytes={} signature_bytes={}",
            carrier - body,
            crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
        );
        assert!(carrier - body < PALW_PREFIX_BATCH_ENVELOPE_BYTES_V1);
        let mut answer = packet.clone();
        answer.action =
            PalwConstraintStreamActionV1::PrefixPublicAnswer(crate::palw_prefix_public_source_v1::PalwPrefixPublicAnswerV1 {
                answerer: packet.reserver,
                cursor_id: Hash64::from_u64_word(1),
                demand_id: Hash64::from_u64_word(2),
                action: crate::palw_prefix_public_source_v1::PalwPrefixPublicAnswerActionV1::Finish,
            });
        assert!(borsh::to_vec(&answer).unwrap().len() < PALW_PREFIX_BATCH_ENVELOPE_BYTES_V1);
        packet.action = PalwConstraintStreamActionV1::PrefixBatch(vec![]);
        assert!(packet.check().is_err());
        packet.action = PalwConstraintStreamActionV1::PrefixBatch(vec![A::FinishTile; 65]);
        assert!(packet.check().is_err());
        packet.action = PalwConstraintStreamActionV1::PrefixBatch(vec![A::TilePart { offset: 0, bytes: vec![0; 65_536] }]);
        packet.check().unwrap();
        packet.action = PalwConstraintStreamActionV1::PrefixBatch(vec![A::TilePart { offset: 0, bytes: vec![0; 73_616] }]);
        assert!(packet.check().is_err());
    }
    #[test]
    fn signed_move_binds_literal_owner_claim_reservation_phase_offset_and_bytes() {
        let m = movement();
        m.check().unwrap();
        let signed = m.signing_message();
        let mut mutations = Vec::new();
        let mut x = m.clone();
        x.claim = Hash64::default();
        mutations.push(x);
        let mut x = m.clone();
        x.reserver.0.index += 1;
        mutations.push(x);
        let mut x = m.clone();
        x.reserved_daa += 1;
        mutations.push(x);
        let mut x = m.clone();
        x.phase = PalwConstraintStreamPhaseV1::Prefix(1);
        mutations.push(x);
        let mut x = m.clone();
        x.action = PalwConstraintStreamActionV1::RenderingPart { offset: 1, bytes: vec![b'a'] };
        mutations.push(x);
        let mut x = m.clone();
        x.action = PalwConstraintStreamActionV1::RenderingPart { offset: 0, bytes: vec![b'x'] };
        mutations.push(x);
        for changed in mutations {
            assert_ne!(changed.signing_message(), signed);
        }
        let mut wrong = m.clone();
        wrong.signature.pop();
        assert!(wrong.check().is_err());
        wrong = m;
        wrong.action = PalwConstraintStreamActionV1::RenderingPart { offset: 0, bytes: vec![0; 65537] };
        assert!(wrong.check().is_err());
    }
    #[test]
    fn prefix_typed_carrier_binds_family_transition_and_bounds_hostile_body() {
        use crate::palw_prefix_family_court_v1::PalwPrefixCourtActionV1 as A;
        let mut m = movement();
        m.phase = PalwConstraintStreamPhaseV1::PrefixCourt;
        m.action = PalwConstraintStreamActionV1::Prefix(A::TilePart { offset: 0, bytes: vec![1, 0, 0, 0] });
        m.check().unwrap();
        let message = m.signing_message();
        let encoded = borsh::to_vec(&m).unwrap();
        let decoded: PalwConstraintStreamMoveV1 = borsh::from_slice(&encoded).unwrap();
        assert_eq!(decoded, m);
        let mut foreign = m.clone();
        foreign.phase = PalwConstraintStreamPhaseV1::Tokenizer;
        assert_ne!(message, foreign.signing_message());
        foreign = m.clone();
        foreign.action = PalwConstraintStreamActionV1::Prefix(A::FinishTile);
        assert_ne!(message, foreign.signing_message());
        foreign = m.clone();
        foreign.action = PalwConstraintStreamActionV1::Prefix(A::TilePart { offset: 4, bytes: vec![1, 0, 0, 0] });
        assert_ne!(message, foreign.signing_message());
        m.action = PalwConstraintStreamActionV1::Prefix(A::TilePart { offset: 0, bytes: vec![0; 90_001] });
        assert!(m.check().is_err());
    }
    #[test]
    fn registered_graph_variable_metadata_can_exceed_a_single_seed_carrier() {
        let (_, mut binding, _, _) = crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(false, false);
        let base = borsh::to_vec(&binding.shape_profile).unwrap().len();
        binding.shape_profile.transcendental_bindings = vec![
            crate::palw_step::PalwTranscendentalBindingV1 {
                site: crate::palw_step::PalwTranscendentalSiteV1::VectorExpPolynomial,
                algorithm_id: Hash64::from_u64_word(31),
            };
            1500
        ];
        // Registration's shape gate has no count ceiling here; the extra bindings do not
        // change node count/width or reachable kernels. This is a carrier regression witness,
        // not a claim that this synthetic registration has run through an actual service.
        binding.shape_profile.validate_shape().unwrap();
        let profile_bytes = borsh::to_vec(&binding.shape_profile).unwrap().len();
        assert_eq!(profile_bytes - base, 1500 * 65);
        let (_, a) = crate::palw_decode_constraint_v1::tests::public_constrained_case_for_tests_v1(false);
        let mut seed = PalwConstraintStreamSeedV1 {
            version: 1,
            claim: Hash64::from_u64_word(32),
            reserver: movement().reserver,
            reserved_daa: 40,
            binding,
            job: None,
            pin: a.pin,
            witness: None,
            eog_count: 0,
            eog_opening: a.eog.opening,
            auxiliary: Some(PalwAuxiliaryPublicCourtSeedV1::Prefix {
                maximum_leaves: 1 << 32,
                prefix_tokens: 2,
                relation: crate::palw_class_semantics_v1::PalwPrefixStateRelationV1::Base0Int8KvV1,
            }),
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            retained_profile: None,
        };
        let seed_bytes = borsh::to_vec(&seed).unwrap().len();
        eprintln!("G14 full profile={profile_bytes} seed(job=None)={seed_bytes} carrier=90000");
        assert!(profile_bytes > 90_000);
        assert!(seed_bytes > 90_000);
        assert!(seed.check(90_000).is_err());
        let full = seed.binding.shape_profile.clone();
        let class = full.shape_profile_id();
        seed.binding.job_context.shape_profile_id = class;
        seed.binding.shape_profile = palw_retained_profile_skeleton_v1(&full);
        seed.retained_profile = Some(PalwRetainedCourtProfileV1 { class_id: class, original_bytes: profile_bytes as u64 });
        let compact_size = borsh::to_vec(&seed).unwrap().len();
        eprintln!("G14 referenced profile seed={compact_size} original_profile={profile_bytes}");
        assert!(compact_size < 90_000);
        seed.check(90_000).unwrap();
        let identity = seed.signing_message();
        let resolved = seed.resolved_binding_v1(class, Some(&full)).unwrap();
        assert_eq!(resolved.shape_profile, full);
        assert_eq!(seed.signing_message(), identity);
        assert!(seed.resolved_binding_v1(Hash64::default(), Some(&full)).is_err());
        assert!(seed.resolved_binding_v1(class, None).is_err());
        let mut false_quote = seed.clone();
        false_quote.retained_profile.as_mut().unwrap().original_bytes -= 1;
        assert!(false_quote.resolved_binding_v1(class, Some(&full)).is_err());
        assert_ne!(false_quote.signing_message(), identity);
        let mut false_stub = seed.clone();
        false_stub.binding.shape_profile.hidden_dim += 1;
        assert!(false_stub.resolved_binding_v1(class, Some(&full)).is_err());
        let mut later_profile = full.clone();
        later_profile.base0_rms_eps_q += 1;
        assert!(seed.resolved_binding_v1(class, Some(&later_profile)).is_err());
    }
}
