// ISOLATED include inside native palw_tir_public_fold_v1, after actual requirements.
// No wire contract/fee/paid capability. Codec facts never fabricate a Claim/ctx.
use crate::palw_native_resource_v1::{NativeCoverageV1 as ArchiveCoverageV11, NativeDecodeErrorV1 as ArchiveErrorV11};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ArchiveBodyFactV11 {
    pub tag: u8,
    pub canonical_bytes_max: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ArchiveMetadataWorkV11 {
    pub canonical_body_bytes_bound: u64,
    pub hashed_canonical_bytes: u64,
    pub matching_source_rows: u64,
    pub actor_bond_reads: u64,
    pub pledge_signature_checks: u64,
    pub service_packet_members: u64,
    pub persistent_path_updates: u64,
    pub maximum_node_hashes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ArchiveAdjunctMissingV11 {
    AcceptedInputProvenance,
    ActualPostblockClassAndLayout,
    NativeFamilyServicePackets,
    NativeBaseServiceSchedule,
    NativeWholeBranchActorInventory,
    FutureSourceRewrites,
    NativeMetadataWorkTariff,
    NativeGcMetadata,
}
/// The member count/codec of the real source+signer+retention branch universe.
/// Family packets are a separate native adjunct, never assumed zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NativeBaseServiceArchiveFactsV11 {
    pub packet_count: u64,
    pub binding_codec_bytes: u64,
    pub packets_codec_bytes: u64,
    pub packet_identity_checks: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProspectiveBaseServiceV11 {
    Measured(NativeBaseServiceArchiveFactsV11),
    MissingNativeAdmissionSchedule,
    MissingNativeSourceGraph,
}
/// Positive geometry for ONLY these measured tag roles, not an additive delta
/// to an overlapping r4 physical quote and not a complete source16 cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct KnownArchiveAdjunctGeometryV11 {
    pub rows: u64,
    pub canonical_bytes: u64,
    pub publication_wire_max: u64,
    pub known_event_undo_wire_max: u64,
    pub native_requests_max: u64,
    pub maximum_mutation_nodes: u64,
}
/// Canonical facts remain independent of ABI-dependent local allocation facts.
/// No accepting block/hash/subsidy, paid receipt, complete flag or CPU rate.
pub(super) struct NativePreparedArchiveAdjunctQuoteV11 {
    claim: Hash64,
    source: Hash64,
    resource_plan: Hash64,
    allocation: Hash64,
    job: Hash64,
    control: Hash64,
    base_semantics_present: bool,
    class: Hash64,
    execution: Hash64,
    output: Hash64,
    trace: Hash64,
    bodies: [ArchiveBodyFactV11; 11],
    base_service: ProspectiveBaseServiceV11,
    native_requests: u64,
    work: ArchiveMetadataWorkV11,
    named_source_actor: PreparedLegacySourceActorV1,
    named_drawn_actors: u64,
    future_custodian_slots: u64,
}
impl NativePreparedArchiveAdjunctQuoteV11 {
    pub(super) fn claim(&self) -> Hash64 {
        self.claim
    }
    pub(super) fn source(&self) -> Hash64 {
        self.source
    }
    pub(super) fn allocation(&self) -> Hash64 {
        self.allocation
    }
    pub(super) fn resource_plan(&self) -> Hash64 {
        self.resource_plan
    }
    pub(super) fn source_job(&self) -> Hash64 {
        self.job
    }
    pub(super) fn control_commitment(&self) -> Hash64 {
        self.control
    }
    pub(super) fn bodies(&self) -> &[ArchiveBodyFactV11] {
        &self.bodies
    }
    /// Incremental body bytes relative to the frozen v10-r4 SUBSET. The full
    /// Option semantics tag15 overlaps its Some raw payload, but adds its real
    /// Option byte (or a full None byte when r4 omitted that row). Other known
    /// tags here are absent from r4. This excludes N/B/I/A and service geometry.
    pub(super) fn additional_body_bytes_vs_v10_r4(&self) -> Result<u64, ArchiveErrorV11<String>> {
        let mut n = 1u64;
        for b in self.bodies {
            if b.tag != 15 {
                n = archive_add_v11(n, b.canonical_bytes_max)?;
            }
        }
        Ok(n)
    }
    pub(super) fn additional_body_rows_vs_v10_r4(&self) -> u64 {
        self.bodies.len() as u64 - u64::from(self.base_semantics_present)
    }
    pub(super) fn known_body_geometry_v11(&self) -> Result<KnownArchiveAdjunctGeometryV11, ArchiveErrorV11<String>> {
        let spec = crate::palw_claim_local_context_nodes_v2::ComponentSpecV2 {
            owner: 16,
            table: 2,
            schema: 10,
            key_bytes: 64,
            value_bytes: u64::MAX,
        };
        let mut wire = 0u64;
        let mut requests = self.native_requests;
        let mut bytes = 0u64;
        let mut nodes = 0u64;
        for body in self.bodies {
            let g = crate::palw_claim_local_context_nodes_v2::mutation_geometry(spec, body.canonical_bytes_max)
                .map_err(super::palw_claim_local_paid_archive_v2::state_error)
                .map_err(archive_state_error_v11)?;
            // Existing generic helper counts a global I18. Actual source bodies
            // use owned I84; admit the exact additional66 keybytes for this role.
            wire = archive_add_v11(wire, archive_add_v11(g.retained_wire, 66)?)?;
            requests = archive_add_v11(requests, archive_add_v11(g.retained_heap, 132)?)?;
            nodes = archive_add_v11(nodes, g.max_nodes)?;
            bytes = archive_add_v11(bytes, body.canonical_bytes_max)?;
        }
        Ok(KnownArchiveAdjunctGeometryV11 {
            rows: self.bodies.len() as u64,
            canonical_bytes: bytes,
            publication_wire_max: wire,
            known_event_undo_wire_max: archive_mul_v11(wire, 4)?,
            native_requests_max: requests,
            maximum_mutation_nodes: nodes,
        })
    }
    pub(super) fn base_service(&self) -> ProspectiveBaseServiceV11 {
        self.base_service
    }
    pub(super) fn native_requests_max(&self) -> u64 {
        self.native_requests
    }
    pub(super) fn metadata_work(&self) -> ArchiveMetadataWorkV11 {
        self.work
    }
    pub(super) fn actors(&self) -> (PreparedLegacySourceActorV1, u64, u64) {
        (self.named_source_actor, self.named_drawn_actors, self.future_custodian_slots)
    }
    pub(super) fn missing(&self) -> impl Iterator<Item = ArchiveAdjunctMissingV11> {
        use ArchiveAdjunctMissingV11::*;
        [
            Some(AcceptedInputProvenance),
            Some(ActualPostblockClassAndLayout),
            Some(NativeFamilyServicePackets),
            Some(NativeWholeBranchActorInventory),
            Some(FutureSourceRewrites),
            Some(NativeMetadataWorkTariff),
            Some(NativeGcMetadata),
            matches!(
                self.base_service,
                ProspectiveBaseServiceV11::MissingNativeAdmissionSchedule | ProspectiveBaseServiceV11::MissingNativeSourceGraph
            )
            .then_some(NativeBaseServiceSchedule),
        ]
        .into_iter()
        .flatten()
    }
    /// Actual native pre-debit row recheck. Never manufactures unknown clocks.
    /// Ordinary caller supplies its already-created local fresh Claim, before
    /// reserve_for_claim/funding. Client quote does NOT need this operation.
    pub(super) fn recheck_actual_fresh_claim_v11(
        &self,
        id: Hash64,
        c: &PalwClaimStateV2,
        ctx: &PalwBlockContextV2,
    ) -> Result<(), ArchiveErrorV11<String>> {
        if id != self.claim
            || c.class_id != self.class
            || c.bond != self.named_source_actor.bond
            || c.execution_root != self.execution
            || c.output_root != self.output
            || c.trace_root != self.trace
            || c.accepted_daa != ctx.daa_score
            || c.accepted_blue_score != ctx.blue_score
            || c.accepted_block != ctx.block
            || !matches!(&c.source,PalwClaimSourceV2::FreePrompt{spent,..} if spent.is_empty())
            || !matches!(c.phase, PalwClaimPhaseV2::Provisional)
            || c.rebound_daa.is_some()
            || c.work_id.is_none()
            || c.rcore != PalwClaimRcoreV1::default()
            || c.job_identity != Hash64::default() && c.job_identity != self.job
        {
            return Err(archive_bad_v11("actual original Claim is not this native fresh shape"));
        }
        let n = archive_length_v11(c)?;
        if n != self.bodies[0].canonical_bytes_max {
            return Err(archive_bad_v11("actual original Claim codec exceeds measured native fresh schema"));
        }
        Ok(())
    }
}
fn archive_bad_v11(why: &str) -> ArchiveErrorV11<String> {
    ArchiveErrorV11::Decode(why.into())
}
fn archive_add_v11(a: u64, b: u64) -> Result<u64, ArchiveErrorV11<String>> {
    a.checked_add(b).ok_or_else(|| {
        ArchiveErrorV11::Resource(ArchiveCoverageV11::overflow::<NativePreparedArchiveAdjunctQuoteV11>("archive adjunct sum"))
    })
}
fn archive_mul_v11(a: u64, b: u64) -> Result<u64, ArchiveErrorV11<String>> {
    a.checked_mul(b).ok_or_else(|| {
        ArchiveErrorV11::Resource(ArchiveCoverageV11::overflow::<NativePreparedArchiveAdjunctQuoteV11>("archive adjunct product"))
    })
}
fn archive_length_v11<T: borsh::BorshSerialize>(v: &T) -> Result<u64, ArchiveErrorV11<String>> {
    borsh::object_length(v).map(|n| n as u64).map_err(|_| archive_bad_v11("archive adjunct canonical codec length"))
}
fn archive_state_error_v11(e: PalwStateV2Error) -> ArchiveErrorV11<String> {
    match e {
        PalwStateV2Error::NativeResource(c) => ArchiveErrorV11::Resource(c),
        e => ArchiveErrorV11::Decode(e.to_string()),
    }
}
fn archive_geometry_error_v11(e: PreparedGeometryErrorV1) -> ArchiveErrorV11<String> {
    match e {
        PreparedGeometryErrorV1::Native(e) => e,
        PreparedGeometryErrorV1::State(e) => archive_state_error_v11(e),
        e => ArchiveErrorV11::Decode(format!("archive adjunct preparation: {e:?}")),
    }
}
fn archive_requirements_error_v11(e: PreparedRequirementsErrorV1) -> ArchiveErrorV11<String> {
    match e {
        PreparedRequirementsErrorV1::Native(e) => e,
        PreparedRequirementsErrorV1::Geometry(e) => archive_geometry_error_v11(e),
        e => ArchiveErrorV11::Decode(format!("archive adjunct requirements: {e:?}")),
    }
}
/// Stream actual TFP1 bytes against the actual pairing without decoding or
/// allocating a second original payload. Prefix and EOF remain exact.
fn archive_literal_carrier_matches_v11(
    literal: &crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1,
    raw: &[u8],
) -> Result<(), ArchiveErrorV11<String>> {
    struct Match<'a> {
        remaining: &'a [u8],
    }
    impl std::io::Write for Match<'_> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if !self.remaining.starts_with(b) {
                return Err(std::io::Error::other("literal TFP1 mismatch"));
            }
            self.remaining = &self.remaining[b.len()..];
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut sink = Match {
        remaining: raw
            .strip_prefix(crate::palw_tir_public_fp_envelope_v1::PALW_TIR_PUBLIC_FP_ENVELOPE_MAGIC_V1)
            .ok_or_else(|| archive_bad_v11("archive adjunct actual funding carrier TFP1 magic"))?,
    };
    borsh::BorshSerialize::serialize(literal, &mut sink)
        .map_err(|_| archive_bad_v11("archive adjunct actual funding carrier substitutes literal TFP1"))?;
    if !sink.remaining.is_empty() {
        return Err(archive_bad_v11("archive adjunct actual funding carrier TFP1 trailing bytes"));
    }
    Ok(())
}
fn archive_control_id_v11(v: &PalwConsensusObjectV2) -> Result<Hash64, ArchiveErrorV11<String>> {
    struct W(blake2b_simd::State);
    impl std::io::Write for W {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut w = W(blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/source-archive-control/v11").to_state());
    borsh::BorshSerialize::serialize(v, &mut w).map_err(|_| archive_bad_v11("archive adjunct control digest"))?;
    let mut bytes = [0u8; 64];
    bytes.copy_from_slice(w.0.finalize().as_bytes());
    Ok(Hash64::from_bytes(bytes))
}
/// Fixed fresh FP Claim codec, directly mirroring actual objects.rs fields:
/// Source(FreePrompt/quanta/emptyspent)9; class64; bond68; PWU8;
/// accepted8/reboundNone1/blue8/carrying64; three roots192; chunks4/H8;
/// reserved16/immature16/escrow8/leaves8; workIdSome65; Provisional1;
/// rights16/job64; defaultRcore24. Unknown scalar VALUES affect no width.
/// This is not a zero-filled Claim, a future point or a source-state commitment.
pub(super) fn native_fresh_fp_claim_codec_v11() -> u64 {
    9 + 64 + 68 + 8 + 8 + 1 + 8 + 64 + 192 + 4 + 8 + 16 + 16 + 8 + 8 + 65 + 1 + 16 + 64 + 24
}
/// Same distinction for original fresh escrow: settled_by is None. This
/// geometrical bound confers no funded/outpoint/amount/claim authority.
fn native_fresh_original_escrow_codec_v11() -> u64 {
    2 + 64 + 64 + 68 + 8 + 64 + 64 + 8 + 8 + 8 + 8 + 8 + 1 + 1 + 8 + 8 + 64 + 64
}
impl PreparedOriginalRequirementsV1 {
    /// Positive shape quote from actual opaque native preparation and actual
    /// extractor object. Neither a future accepting point nor a fundingtoken
    /// is required; current native-context adjuncts are measured separately.
    pub(super) fn quote_archive_object_shape_v11(
        &self,
        literal: &crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1,
        object: &PalwConsensusObjectV2,
    ) -> Result<(Hash64, u64, u64, u64), ArchiveErrorV11<String>> {
        self.archive_object_shape_inner_v11(literal, object, false, crate::palw_native_resource_v1::reserve_native_requests_v1)
    }
    #[cfg(test)]
    pub(super) fn quote_archive_object_shape_with_checker_v11(
        &self,
        literal: &crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1,
        object: &PalwConsensusObjectV2,
        checker: &std::sync::Arc<crate::palw_native_resource_v1::NativeDemandCheckerV1>,
    ) -> Result<(Hash64, u64, u64, u64), ArchiveErrorV11<String>> {
        self.archive_object_shape_inner_v11(literal, object, true, |n| checker.reserve_requests_v1(n, 0).map(Some))
    }
    fn archive_object_shape_inner_v11(
        &self,
        literal: &crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1,
        object: &PalwConsensusObjectV2,
        explicit_checker: bool,
        reserve: impl FnOnce(u64) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, ArchiveCoverageV11>,
    ) -> Result<(Hash64, u64, u64, u64), ArchiveErrorV11<String>> {
        // New source-local identity/counting passes hold a transient input
        // budget BEFORE any existing native job-identity helper is invoked.
        let scratch = if explicit_checker {
            crate::palw_native_resource_v1::native_typed_value_requests_v1(object)
        } else {
            crate::palw_native_resource_v1::native_value_requests_v1(object)
        }
        .map_err(ArchiveErrorV11::Resource)?;
        let scratch = archive_add_v11(
            scratch,
            if explicit_checker {
                crate::palw_native_resource_v1::native_typed_value_requests_v1(literal)
            } else {
                crate::palw_native_resource_v1::native_value_requests_v1(literal)
            }
            .map_err(ArchiveErrorV11::Resource)?,
        )?;
        let _scratch = reserve(scratch).map_err(ArchiveErrorV11::Resource)?;
        let (id, original) = paired_claim(object).map_err(archive_state_error_v11)?;
        let frame = self.frame_v1();
        if literal.header != *frame.header_v1() || literal.pledge != *frame.custody_v1().pledge_v1() {
            return Err(archive_bad_v11("archive adjunct literal TFP1 differs from native preparation"));
        }
        // Exact extractor projection including prompt IDs/prefix state. Its
        // existing bounded decoder runs under the input reservation above.
        literal.matches_object_v1(object).map_err(|e| archive_bad_v11(&e))?;
        if id != frame.plan_v1().original_claim || original != frame.original_v1() {
            return Err(archive_bad_v11("archive adjunct object substitutes genuine prepared original"));
        }
        // Native opaque preparation already authenticated this pledge/source.
        // This quote grants no admission; compare the literal prepared job and
        // existing projection, never re-run a hidden allocating DTO decoder.
        let PalwConsensusObjectV2::FreePromptCommitted {
            class_id,
            bond,
            execution_root,
            trace_root,
            output_root,
            executor_pubkey,
            job_pin,
            work_leaves,
            prompt_tokens,
            decode_tokens_executed,
            ..
        } = object
        else {
            unreachable!()
        };
        let c = frame.original_v1();
        if *class_id != c.job.class_id
            || bond.0 != c.job.executor_bond
            || executor_pubkey != &c.job.executor_pubkey
            || *execution_root != c.execution_root
            || *trace_root != c.trace_root
            || *output_root != c.output_root
            || *job_pin != crate::palw_fp_execution_v3::palw_fp_job_pin_v1(c)
            || *work_leaves != c.work_leaves
            || *prompt_tokens != c.job.prompt_tokens
            || *decode_tokens_executed != c.decode_tokens_executed
        {
            return Err(archive_bad_v11("archive adjunct control projection differs from actual prepared job"));
        }
        let bytes = archive_length_v11(object)?;
        Ok((archive_control_id_v11(object)?, bytes, native_fresh_fp_claim_codec_v11(), native_fresh_original_escrow_codec_v11()))
    }
    /// Read-only actual-context rederivation BEFORE original/new-plan funding.
    /// No state/claim/service book is created or cloned to obtain a quote.
    pub(super) fn quote_archive_adjuncts_v11(
        &self,
        b: &TransitionBuilder<'_>,
        ctx: &PalwBlockContextV2,
        object: &PalwConsensusObjectV2,
    ) -> Result<NativePreparedArchiveAdjunctQuoteV11, ArchiveErrorV11<String>> {
        self.geometry.prepared.check_preclaim_context_v1(b, ctx).map_err(archive_geometry_error_v11)?;
        let frame = self.frame_v1();
        let plan = frame.plan_v1();
        let literal = b
            .extras
            .tir_public_originals
            .get(&plan.original_claim)
            .ok_or_else(|| archive_bad_v11("archive adjunct actual TFP1 pairing missing"))?;
        let (control, control_bytes, claim_bytes, escrow_bytes) = self.quote_archive_object_shape_v11(literal, object)?;
        let pairing = b
            .extras
            .tir_funded_originals
            .get(&plan.original_claim)
            .ok_or_else(|| archive_bad_v11("archive adjunct requires actual transaction-local funded envelope, not payment"))?;
        archive_literal_carrier_matches_v11(literal, &pairing.envelope().original_envelope)?;
        let t = &pairing.envelope().funding.terms;
        if pairing.claim() != plan.original_claim
            || t.claim != plan.original_claim
            || t.execution_root != frame.original_v1().execution_root
            || t.source_id != plan.source_id
            || t.job_identity != frame.custody_v1().pledge_v1().job_identity
        {
            return Err(archive_bad_v11("archive adjunct signed funding carrier substitutes source/job"));
        }
        // The actual opaque prepared catalog already authenticated the source
        // pledge. Recheck current literal native authority without counting or
        // rerunning a signature operation that this quote does not need.
        let actor = self.actors.source_actor_v1();
        let bond = b.state.bond(&actor.bond).ok_or_else(|| archive_bad_v11("archive adjunct native source actor missing"))?;
        if !matches!(bond.status, PalwBondStatusV2::Active)
            || bond.pubkey != frame.original_v1().job.executor_pubkey
            || bond.operator_id != actor.operator
        {
            return Err(archive_bad_v11("archive adjunct native source actor authority changed"));
        }
        let semantics = self.geometry.prepared.semantic_snapshot.as_ref();
        let eof = semantics.and_then(|sem| b.state.staged_tokenizer_inventory_v1(sem.tokenizer_id));
        if let Some(eof) = eof {
            eof.check_v1().map_err(|e| archive_bad_v11(&e))?;
            if semantics.is_none_or(|s| s.tokenizer_id != eof.artifact_hash) {
                return Err(archive_bad_v11("archive adjunct tokenizer EOF differs from native semantic snapshot"));
            }
        }
        let rules = crate::palw_court_family_material_v1::FamilyOriginalRuleContextV2::from_native_v2(self.geometry.prepared.native);
        let table_bytes = if let Some(table) = pairing.semantic_table_v2() {
            // Outer Option1 + actual future scalar row + OptionSome rule1.
            archive_add_v11(1 + 64 + 68 + 64 + 1 + 4 + 8, archive_add_v11(archive_length_v11(&rules)?, table.len() as u64)?)?
        } else {
            1
        };
        let cover = crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1()
            .map_err(|e| archive_bad_v11(&format!("material intake schedule: {e:?}")))?
            .cover;
        let limits = (cover.max_original_leaves, cover.max_total_edges, cover.max_total_coordinates, cover.max_root_memberships);
        let class = b
            .state
            .tir_class_v1(&frame.original_v1().job.class_id)
            .ok_or_else(|| archive_bad_v11("archive adjunct native class missing"))?;
        let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state, frame.original_v1().job.class_id)
            .ok_or_else(|| archive_bad_v11("archive adjunct native layout missing"))?;
        // Exact contextual tag11 policy clone: reserve borrowed detection
        // population before from_rules copies it. This is a real candidate ctx,
        // not an offchain guess of the future accepting point.
        let mut policy_requests = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
        policy_requests
            .exact_v1::<crate::palw_detection_policy_v1::PalwDetectionPolicyV1>(
                crate::palw_claim_local_v1::PalwClaimLocalPolicyContextV1::detection_at_v1(b.params, ctx.daa_score).len() as u64,
            )
            .map_err(ArchiveErrorV11::Resource)?;
        let policy_request_bytes = policy_requests.bytes_v1();
        let _policy_owner =
            crate::palw_native_resource_v1::reserve_native_requests_v1(policy_request_bytes).map_err(ArchiveErrorV11::Resource)?;
        let policy = crate::palw_claim_local_v1::PalwClaimLocalPolicyContextV1::from_rules_v1(b.params, ctx.daa_score);
        let bodies = [
            ArchiveBodyFactV11 { tag: 3, canonical_bytes_max: claim_bytes },
            ArchiveBodyFactV11 { tag: 8, canonical_bytes_max: escrow_bytes },
            ArchiveBodyFactV11 { tag: 9, canonical_bytes_max: 1 + 2 + 6 * 64 + 4 * 8 }, // actual staged Row tag + SharedContext.
            ArchiveBodyFactV11 { tag: 13, canonical_bytes_max: control_bytes },
            ArchiveBodyFactV11 { tag: 14, canonical_bytes_max: archive_add_v11(4, archive_length_v11(pairing.envelope())?)? }, // source16 stores encoded Vec.
            ArchiveBodyFactV11 { tag: 15, canonical_bytes_max: archive_length_v11(&semantics)? },
            ArchiveBodyFactV11 { tag: 16, canonical_bytes_max: archive_length_v11(&eof)? },
            ArchiveBodyFactV11 { tag: 17, canonical_bytes_max: table_bytes },
            ArchiveBodyFactV11 { tag: 19, canonical_bytes_max: archive_length_v11(&limits)? },
            ArchiveBodyFactV11 { tag: 11, canonical_bytes_max: archive_length_v11(&policy)? },
            ArchiveBodyFactV11 { tag: 12, canonical_bytes_max: archive_add_v11(4, archive_length_v11(b.params)?)? },
        ];
        let mut work = ArchiveMetadataWorkV11::default();
        for body in bodies {
            work.canonical_body_bytes_bound = archive_add_v11(work.canonical_body_bytes_bound, body.canonical_bytes_max)?;
        }
        work.hashed_canonical_bytes = control_bytes;
        work.matching_source_rows = bodies.len() as u64;
        work.actor_bond_reads = 1; // Current native literal source bond was checked above.
        work.pledge_signature_checks = 0;
        work.persistent_path_updates = bodies.len() as u64;
        work.maximum_node_hashes = archive_mul_v11(work.persistent_path_updates, 2 * (64 * 9 + 1) + 1)?;
        let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
        q.add_v1(policy_request_bytes).map_err(ArchiveErrorV11::Resource)?;
        // This quotes the actual owner footprint, independently of fixed wire bytes.
        for n in [
            crate::palw_native_resource_v1::native_typed_value_requests_v1(object),
            crate::palw_native_resource_v1::native_typed_value_requests_v1(literal),
            crate::palw_native_resource_v1::native_typed_value_requests_v1(pairing.envelope()),
            crate::palw_native_resource_v1::native_typed_value_requests_v1(class),
            crate::palw_native_resource_v1::native_typed_value_requests_v1(layout),
        ] {
            q.add_v1(n.map_err(ArchiveErrorV11::Resource)?).map_err(ArchiveErrorV11::Resource)?;
        }
        let graph = match self.source_graph_v1() {
            Ok(graph) => Some(graph),
            Err(PreparedRequirementsErrorV1::MissingSharedServiceNamespace) => None,
            Err(e) => return Err(archive_requirements_error_v11(e)),
        };
        let schedule =
            crate::palw_funded_original_coordinator_v1::palw_funded_original_schedule_v1(b.extras).ok().and_then(|s| s.admission_v1());
        let base_service = match (schedule, graph) {
            (None, _) => ProspectiveBaseServiceV11::MissingNativeAdmissionSchedule,
            (Some(_), None) => ProspectiveBaseServiceV11::MissingNativeSourceGraph,
            (Some(schedule), Some(graph)) => {
                // Exact source-owned baseline only. No family roles/packets/actors
                // are guessed; A4 must add its actual prospective native family facts.
                let counts = self.frame_v1().custody_v1().original_signer_count_v11().map_err(|_| {
                    ArchiveErrorV11::Resource(ArchiveCoverageV11::overflow::<NativePreparedArchiveAdjunctQuoteV11>(
                        "native original signer count",
                    ))
                })?;
                let geometry = schedule
                    .native_archive_packet_geometry_v11(
                        counts,
                        graph.nodes_v3().count() as u64,
                        !pairing.suppliers_v2().is_empty()
                            && b.params.family_supplier_active_at(ctx.daa_score)
                            && b.extras.family_supplier_active,
                    )
                    .map_err(|e| match e {
                        crate::palw_funded_original_admission_v1::FundedAdmissionRefusalV1::Overflow => ArchiveErrorV11::Resource(
                            ArchiveCoverageV11::overflow::<NativePreparedArchiveAdjunctQuoteV11>("native service packet geometry"),
                        ),
                        e => archive_bad_v11(&format!("archive service packet geometry: {e:?}")),
                    })?;
                work.service_packet_members = geometry;
                ProspectiveBaseServiceV11::Measured(NativeBaseServiceArchiveFactsV11 {
                    packet_count: geometry,
                    binding_codec_bytes: archive_add_v11(3 * 64 + 2 * 8 + 4, archive_mul_v11(geometry, 64)?)?,
                    packets_codec_bytes: archive_add_v11(4, archive_mul_v11(geometry, 112)?)?,
                    packet_identity_checks: geometry,
                })
            }
        };
        Ok(NativePreparedArchiveAdjunctQuoteV11 {
            claim: plan.original_claim,
            source: plan.source_id,
            resource_plan: plan.resource_plan_id,
            allocation: plan.allocation_id,
            job: frame.custody_v1().pledge_v1().job_identity,
            control,
            base_semantics_present: semantics.is_some(),
            class: frame.original_v1().job.class_id,
            execution: frame.original_v1().execution_root,
            output: frame.original_v1().output_root,
            trace: frame.original_v1().trace_root,
            bodies,
            base_service,
            native_requests: q.bytes_v1(),
            work,
            named_source_actor: actor,
            named_drawn_actors: frame.custody_v1().drawn_panel_v1().len() as u64,
            future_custodian_slots: self.actors.potential_source_slots_v1(),
        })
    }
}
