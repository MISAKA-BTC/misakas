// Included inside native_source_archive_cost_v10. No fee, birth, export or paid authority.
pub(super) struct NativeBornSourceArchiveQuoteV12 {
    measured: NativeSourceArchiveCostQuoteV10,
    claim: Hash64,
    point: PalwBlockContextV2,
    source_record: Hash64,
    original_bundle: Hash64,
    read_work: BornArchiveReadWorkV12,
}
/// Descriptive same-source canonical work facts, not a metadata CPU tariff.
/// Existing measured.native_requests remains a measured structural subset;
/// native decoder/key/index transient overhead is still explicitly missing.
#[derive(Clone,Copy,Debug)]
pub(super) struct BornArchiveReadWorkV12 {
    pub source_manifest_bytes:u64,
    pub original_bundle_bytes:u64,
    pub input_identity_bytes:u64,
    pub signed_export_identity_count:u64,
    pub additional_native_reader_peak_missing:bool,
}
impl NativeBornSourceArchiveQuoteV12 {
    pub(super) fn measured(&self) -> &NativeSourceArchiveCostQuoteV10 { &self.measured }
    pub(super) fn claim(&self) -> Hash64 { self.claim }
    pub(super) fn accepting_point(&self) -> PalwBlockContextV2 { self.point }
    pub(super) fn source_record(&self) -> Hash64 { self.source_record }
    pub(super) fn original_bundle(&self) -> Hash64 { self.original_bundle }
    pub(super) fn read_work(&self)->BornArchiveReadWorkV12 {self.read_work}
}
fn state_error_v12(e: PalwStateV2Error) -> E<String> {
    match e { PalwStateV2Error::NativeResource(c) => E::Resource(c), e => E::Decode(e.to_string()) }
}
// Borsh streaming; no serialized Vec or unadmitted temporary payload.
fn identity_v12(v: &impl borsh::BorshSerialize) -> Result<Hash64,E<String>> {
    struct W(blake2b_simd::State);
    impl std::io::Write for W {
        fn write(&mut self,x:&[u8])->std::io::Result<usize>{self.0.update(x);Ok(x.len())}
        fn flush(&mut self)->std::io::Result<()>{Ok(())}
    }
    let mut w=W(blake2b_simd::Params::new().hash_length(64).key(b"misaka/native-born-quote/v12").to_state());
    borsh::BorshSerialize::serialize(v,&mut w).map_err(|_|bad("born quote canonical identity"))?;
    let mut out=[0;64];out.copy_from_slice(w.0.finalize().as_bytes());Ok(out.into())
}
/// Exact accepted source data factory. Re-reads the native Source8 record at its
/// full stored point and reads immutable Source16 bodies from the SAME state.
/// ClaimGC/current raw-ID reuse does not redirect either read. Missing retained
/// data is unavailable, never an invented zero-cost source or a closed duty.
/// Horizon remains a PROPOSAL, not a funded/producer-authorized retention grant.
pub(super) fn quote_native_source_archive_for_born_v12(
    state:&PalwChainStateV2,
    admission:&super::palw_claim_local_source_admission_v8::SourceAdmissionViewV8,
    exports:&NativeSourceExportSetV2<'_>,
    finite:ProspectiveArchiveHorizonV10,
)->Result<NativeBornSourceArchiveQuoteV12,E<String>> {
    let claim=admission.archive_quote_claim_id_v12();
    let point=admission.accepting_point();
    let source_record=admission.archive_quote_record_id_v12().map_err(state_error_v12)?;
    let actual=super::palw_claim_local_source_admission_v8::for_accepting_point_v8(state,claim,point)
        .map_err(state_error_v12)?.ok_or_else(||bad("born quote actual source admission unavailable"))?;
    if actual.archive_quote_record_id_v12().map_err(state_error_v12)?!=source_record {
        return Err(bad("born quote source admission belongs to another state"));
    }
    let origin=super::palw_claim_local_born_tir_source_v16::original_provenance_v16(state,claim,point)
        .map_err(state_error_v12)?.ok_or_else(||bad("born quote immutable original archive unavailable"))?;
    if !origin.archive_quote_complete_v12() || origin.source()!=claim || origin.accepting_point()!=point
        || actual.original_claim()!=origin.original()
        || identity_v12(&actual.inputs())?!=identity_v12(&origin.inputs())? {
        return Err(bad("born quote source/native archive identity mismatch"));
    }
    let original_bundle=origin.archive_quote_bundle_id_v12().map_err(state_error_v12)?;
    let read_work=BornArchiveReadWorkV12 {
        source_manifest_bytes:actual.archive_quote_record_bytes_v12().map_err(state_error_v12)?,
        original_bundle_bytes:origin.archive_quote_bundle_bytes_v12().map_err(state_error_v12)?,
        input_identity_bytes:add(bytes(&actual.inputs())?,bytes(&origin.inputs())?)?,
        signed_export_identity_count:exports.totals_v2().export_count,
        additional_native_reader_peak_missing:true,
    };
    let frame=origin.admitted_signed_frame();
    let born=origin.born_postblock_frame();
    let class=origin.class(); let layout=origin.layout();
    let born_class=origin.archive_quote_born_class_v12();
    let born_layout=origin.archive_quote_born_layout_v12();
    let semantics=origin.archive_quote_semantics_v12();
    let rules=origin.rules();
    let plan=frame.plan_v1();
    if finite.accept_from>finite.accept_until || point.daa_score<finite.accept_from
        || point.daa_score>finite.accept_until || finite.accept_until>=finite.material_until
        || plan.original_claim!=claim || exports.totals_v2().export_count==0 {
        return Err(bad("born quote point/source/finite interval mismatch"));
    }
    let admitted_source=frame.source_identity_native_v2(class,layout,frame.descriptor_v1().maximum_leaves)?;
    let born_source=born.source_identity_native_v2(born_class,born_layout,born.descriptor_v1().maximum_leaves)?;
    if admitted_source.source_id_v1()!=plan.source_id || born_source.source_id_v1()!=plan.source_id
        || born.plan_v1().resource_plan_id!=plan.resource_plan_id || born.plan_v1().original_claim!=claim {
        return Err(bad("born quote substitutes signed or postblock source"));
    }
    let mut envelopes=origin.inputs().iter().filter_map(|i|match i {
        crate::palw_claim_local_v1::PalwClaimLocalInputV1::PrefixDependency{claim:id,envelope} if *id==claim=>Some(envelope.as_ref()),
        _=>None,
    });
    let envelope=envelopes.next().ok_or_else(||bad("born quote original signed exports unavailable"))?;
    if envelopes.next().is_some(){return Err(bad("born quote ambiguous original export population"));}
    envelope.check_v1().map_err(|_|bad("born quote original export canonical shape"))?;
    if envelope.exports.len() as u64 != exports.totals_v2().export_count {
        return Err(bad("born quote substitutes canonical export population"));
    }
    // Original admission stores exports in strict ordinal order; native set
    // authenticates signatures/source and uses that same order. No n*n scans.
    let mut original_exports=envelope.exports.iter();
    exports.visit_specs_v2(|source,spec| {
        let signed=original_exports.next().ok_or_else(||bad("born quote missing signed export"))?;
        if signed.intent.id_v1()!=spec.intent_id || signed.intent.ordinal!=spec.export_ordinal
            || source.header_v2()!=born.header_v1() || spec.base_claim!=claim
            || spec.source_id!=plan.source_id || spec.material_until>finite.material_until
            || spec.liability_until>finite.material_until {
            return Err(bad("born quote actual signed export/source/horizon mismatch"));
        }
        Ok(())
    })?;
    if original_exports.next().is_some(){return Err(bad("born quote extra original export"));}
    let install=bytes(frame)?;
    // These are actual retained bodies, not custody forecasts or mutated Frames.
    let postblock=bytes(born)?;
    let mut body_bytes=add(install,postblock)?;
    for n in [bytes(class)?,bytes(layout)?,bytes(born_class)?,bytes(born_layout)?,bytes(&rules)?,exports.canonical_encoded_bytes_v3()] {
        body_bytes=add(body_bytes,n)?;
    }
    if let Some(s)=semantics {body_bytes=add(body_bytes,bytes(s)?)?;}
    // Both class/layout snapshots are included; no assumed content sharing.
    let rows=8+u64::from(semantics.is_some());
    // 64-byte source-local keys: Patricia persistent tree <=2R-1 nodes.
    // Wire node bound: enum1 + bits4 + Vec length4 + ceil((64*9+1)/8)
    // prefix73 + children128 =210. Leaf is <=this same bound.
    let local_nodes = mul(rows, 2)?
        .checked_sub(1)
        .ok_or_else(|| bad("source archive empty node population"))?;
    let depth = 64 * 9 + 1;
    // Two shared global directories (exact birth and exact allocation). Only
    // their bounded paths are copied; unrelated global rows are never charged.
    let directory_path_nodes = mul(2, add(mul(depth, 2)?, 1)?)?;
    let nodes = add(local_nodes, directory_path_nodes)?;
    let node_bytes = mul(nodes, 65 + 210)?;
    let body_rows = mul(rows, 65 + 4)?;
    // Source-scoped current I84+ComponentRoot90; directory I20+Root90;
    // one manifest B65+fixed envelope489; one A65+(2+4+90+4+72).
    // Manifest width includes actual point ONLY as a codec width, not authority.
    let fixed = 84 + 90 + 2 * (20 + 90) + 69 + 489 + 69 + 152 + 65 + 172;
    let durable = add(add(add(body_bytes, node_bytes)?, body_rows)?, fixed)?;
    // Both original/postblock publications and exact release/inverse carry old
    // and new bytes. Do not pretend old metadata is zero after first publish.
    // Known one install + one postblock publication + actual original retirement.
    // Each source-local update can create <=2*depth+1 immutable path nodes;
    // full old/new delta envelope is <=500 bytes per maximal N row. Arbitrary
    // future export/credit/retention events are explicitly MISSING below.
    let undo = add(
        mul(durable, 4)?,
        mul(mul(add(rows, 8)?, add(mul(depth, 2)?, 1)?)?, 500)?,
    )?;
    let reads = add(
        mul(body_bytes, add(mul(rows, 4)?, 8)?)?,
        mul(mul(add(rows, 2)?, mul(depth, 4)?)?, 275)?,
    )?;
    let hashes = add(
        mul(body_bytes, add(mul(rows, 8)?, 16)?)?,
        mul(mul(add(rows, 2)?, mul(depth, 8)?)?, 227)?,
    )?;
    let retire = add(add(nodes, rows)?, 8)?;
    let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<(u8, Vec<u8>), Vec<u8>>>(
        retire,
    )
    .map_err(E::Resource)?;
    q.exact_v1::<crate::palw_claim_local_context_nodes_v2::ContextNodeV2>(add(nodes, depth)?)
        .map_err(E::Resource)?;
    q.exact_v1::<u8>(add(mul(durable, 2)?, mul(depth, 275)?)?)
        .map_err(E::Resource)?;
    for demand in [
        crate::palw_native_resource_v1::native_value_requests_v1(frame),
        crate::palw_native_resource_v1::native_value_requests_v1(class),
        crate::palw_native_resource_v1::native_value_requests_v1(layout),
        crate::palw_native_resource_v1::native_value_requests_v1(&rules),
        crate::palw_native_resource_v1::native_value_requests_v1(born),
        crate::palw_native_resource_v1::native_value_requests_v1(born_class),
        crate::palw_native_resource_v1::native_value_requests_v1(born_layout),
    ] {
        q.add_v1(demand.map_err(E::Resource)?)
            .map_err(E::Resource)?;
    }
    if let Some(semantics) = semantics {
        q.add_v1(
            crate::palw_native_resource_v1::native_value_requests_v1(semantics)
                .map_err(E::Resource)?,
        )
        .map_err(E::Resource)?;
    }
    let native = add(q.bytes_v1(), exports.source_preparation_requests_v2())?;
    let measured = NativeSourceArchiveCostQuoteV10 {
        source: plan.source_id,
        resource_plan: plan.resource_plan_id,
        ordered_exports: exports.canonical_commitment_v3(),
        horizon: finite,
        install_frame_bytes: install,
        postblock_frame_bytes_max: postblock,
        source_body_rows: rows,
        durable_bytes_max: durable,
        undo_bytes_max: undo,
        lookup_read_bytes_max: reads,
        hash_bytes_max: hashes,
        retire_rows_max: retire,
        native_requests_max: native,
        missing: MissingArchiveCostV10 {
            whole_f1_history: true,
            global_gc_rebuild: true,
            credit_population_projection: true,
            admission_control_and_service_inventory: true,
            future_rewrite_or_retention_events: true,
            unmeasured_native_identity_work: true,
        },
    };
    Ok(NativeBornSourceArchiveQuoteV12 { measured, claim, point, source_record, original_bundle, read_work })
}
