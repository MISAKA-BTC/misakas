//! Prospective, source-local physical storage quote. No funding/birth/credit permit.
use super::*;
use crate::palw_native_resource_v1::{NativeCoverageV1 as C, NativeDecodeErrorV1 as E};
use crate::palw_tir_prefix_export_relation_v2::NativeSourceExportSetV2;
#[derive(Clone, Copy, Debug)]
pub(super) struct ProspectiveArchiveHorizonV10 {
    pub accept_from: u64,
    pub accept_until: u64,
    pub material_until: u64,
}
/// These gaps are separate from the measured TIR birth/source row family.
/// None may be interpreted as zero or as an already funded retention scope.
#[derive(Clone, Copy, Debug)]
pub(super) struct MissingArchiveCostV10 {
    pub whole_f1_history: bool,
    pub global_gc_rebuild: bool,
    pub credit_population_projection: bool,
    pub admission_control_and_service_inventory: bool,
    pub future_rewrite_or_retention_events: bool,
    pub unmeasured_native_identity_work: bool,
}
pub(super) struct NativeSourceArchiveCostQuoteV10 {
    source: Hash64,
    resource_plan: Hash64,
    ordered_exports: Hash64,
    horizon: ProspectiveArchiveHorizonV10,
    install_frame_bytes: u64,
    postblock_frame_bytes_max: u64,
    source_body_rows: u64,
    durable_bytes_max: u64,
    undo_bytes_max: u64,
    lookup_read_bytes_max: u64,
    hash_bytes_max: u64,
    retire_rows_max: u64,
    native_requests_max: u64,
    missing: MissingArchiveCostV10,
}
impl NativeSourceArchiveCostQuoteV10 {
    pub(super) fn source(&self) -> Hash64 {
        self.source
    }
    pub(super) fn resource_plan(&self) -> Hash64 {
        self.resource_plan
    }
    pub(super) fn ordered_exports(&self) -> Hash64 {
        self.ordered_exports
    }
    pub(super) fn horizon(&self) -> ProspectiveArchiveHorizonV10 {
        self.horizon
    }
    pub(super) fn frame_bytes(&self) -> (u64, u64) {
        (self.install_frame_bytes, self.postblock_frame_bytes_max)
    }
    pub(super) fn source_body_rows(&self) -> u64 {
        self.source_body_rows
    }
    pub(super) fn durable_bytes_max(&self) -> u64 {
        self.durable_bytes_max
    }
    pub(super) fn undo_bytes_max(&self) -> u64 {
        self.undo_bytes_max
    }
    pub(super) fn lookup_read_bytes_max(&self) -> u64 {
        self.lookup_read_bytes_max
    }
    pub(super) fn hash_bytes_max(&self) -> u64 {
        self.hash_bytes_max
    }
    pub(super) fn retire_rows_max(&self) -> u64 {
        self.retire_rows_max
    }
    pub(super) fn native_requests_max(&self) -> u64 {
        self.native_requests_max
    }
    pub(super) fn missing(&self) -> MissingArchiveCostV10 {
        self.missing
    }
}
fn bad(s: &str) -> E<String> {
    E::Decode(s.into())
}
fn add(a: u64, b: u64) -> Result<u64, E<String>> {
    a.checked_add(b).ok_or_else(|| {
        E::Resource(C::overflow::<NativeSourceArchiveCostQuoteV10>(
            "source archive cost sum",
        ))
    })
}
fn mul(a: u64, b: u64) -> Result<u64, E<String>> {
    a.checked_mul(b).ok_or_else(|| {
        E::Resource(C::overflow::<NativeSourceArchiveCostQuoteV10>(
            "source archive cost product",
        ))
    })
}
fn bytes(v: &impl borsh::BorshSerialize) -> Result<u64, E<String>> {
    borsh::object_length(v)
        .map(|n| n as u64)
        .map_err(|_| bad("source archive canonical length"))
}
/// Native opaque preparation plus native canonical export-set. This factory is
/// deliberately usable BEFORE escrow or plan installation. Horizon is a
/// prospective signature-bound proposal; actual accepting block hash is neither
/// fabricated here nor required in a producer signature.
pub(super) fn quote_native_source_archive_v10(
    prepared: &super::palw_tir_public_fold_v1::PreparedOriginalRequirementsV1,
    class: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
    semantics: Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
    rules: &crate::palw_court_family_material_v1::FamilyOriginalRuleContextV2,
    exports: &NativeSourceExportSetV2<'_>,
    finite: ProspectiveArchiveHorizonV10,
) -> Result<NativeSourceArchiveCostQuoteV10, E<String>> {
    let frame = prepared.frame_v1();
    let plan = frame.plan_v1();
    if semantics != prepared.archive_semantics_v10() || *rules != prepared.archive_rules_v10() {
        return Err(bad(
            "source archive quote substitutes actual native semantic/rule preparation",
        ));
    }

    if finite.accept_from > finite.accept_until || finite.accept_until >= finite.material_until {
        return Err(bad(
            "source archive requires a finite nonempty proposed retention interval",
        ));
    }
    if plan.source_id != frame.descriptor_v1().source_id || exports.totals_v2().export_count == 0 {
        return Err(bad(
            "source archive native prepared/source/export-set mismatch",
        ));
    }
    // Authenticate the same source/class/layout relation under its real leased
    // preparation; arbitrary registered rows do not merely contribute lengths.
    let source =
        frame.source_identity_native_v2(class, layout, frame.descriptor_v1().maximum_leaves)?;
    if source.source_id_v1() != plan.source_id {
        return Err(bad("source archive class/layout substitute native source"));
    }
    exports.visit_specs_v2(|source, spec| {
        if source.header_v2() != frame.header_v1()
            || spec.base_claim != plan.original_claim
            || spec.source_id != plan.source_id
            || spec.liability_until > finite.material_until
            || spec.material_until > finite.material_until
        {
            return Err(bad(
                "source archive export set substitutes source or exceeds proposed signed horizon",
            ));
        }
        Ok(())
    })?;
    let install = bytes(frame)?;
    let current_custody = bytes(frame.original_custody_v1())?;
    let custody = frame
        .original_custody_v1()
        .physical_codec_bound_v16()
        .map_err(bad)?;
    let postblock = add(
        install
            .checked_sub(current_custody)
            .ok_or_else(|| bad("source archive custody length underflow"))?,
        custody.encoded_bytes_max,
    )?;
    // Assume both snapshots differ prospectively. Actual writer content-address
    // sharing reduces this only after authentic byte equality, never by fiat.
    let mut body_bytes = add(install, postblock)?;
    for n in [
        bytes(class)?,
        bytes(layout)?,
        bytes(rules)?,
        exports.canonical_encoded_bytes_v3(),
    ] {
        body_bytes = add(body_bytes, n)?
    }
    if let Some(semantics) = semantics {
        body_bytes = add(body_bytes, bytes(semantics)?)?
    }
    let rows = 6 + u64::from(semantics.is_some());
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
        crate::palw_native_resource_v1::native_value_requests_v1(rules),
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
    let native = add(
        add(q.bytes_v1(), custody.native_requests_max)?,
        exports.source_preparation_requests_v2(),
    )?;
    Ok(NativeSourceArchiveCostQuoteV10 {
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
    })
}

include!("palw_claim_local_born_source_archive_quote_v12.rs");
