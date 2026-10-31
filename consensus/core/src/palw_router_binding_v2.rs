//! **RFC-0006 routed-reader consumer v2: the immutable per-reader router binding** (kernel-route aux table 139, declared on object
//! tag 244 past `palw_memory_routing_court_v2`).
//!
//! # What was missing
//!
//! A routed reader — a `.routed` expert projection or the routed gated multiply — is adjudicated from the expert ids in a committed
//! router row, and the legacy court appends "the unique q36 router of the reader's whole layer table" (`qwen36_router_topk_slot`).
//! A table with two routers, or with a Kimi router, has no such row: the court refuses, and choosing one of them would be the court
//! choosing its own evidence.
//!
//! # What this module adds
//!
//! The class registrant states, ONCE and immutably, which router each routed reader of each layer-table template reads. The court
//! then never chooses: it reads the binding the chain holds for the claim's class, accepts it only for claims accepted at or after
//! the block that declared it, and refuses a reader the binding does not name. Every binding is checked against the class's own
//! registered graph before it is stored:
//!
//! * the reader set is EXACTLY the profile's routed readers (none missing, none extra, none twice);
//! * each named router is a router node (q36 or Kimi family) of the same table, strictly before the reader (the graph is a DAG in
//!   template order);
//! * the router's width is two lanes per chosen expert, its score universe is its own input row's width (at least `k`), and the
//!   reader's own `k` (its expert blocks) is the router's;
//! * a combine that consumes routing WEIGHTS reads its own family's router through its declared reference — q36 and Kimi weight
//!   arithmetic stay separate semantics (the Kimi combine's routing reference is a Kimi router, the q36 combine's a q36 router).
//!
//! The binding authenticates which committed row a reader uses. It is data identity, not a proof that any computation is correct;
//! the verdict is the court's deterministic recomputation over the bound router's authenticated row (PRINCIPLES §8).
use crate::Hash64;
use crate::palw_step::{PalwLayerKindV1, PalwShapeProfileV3, PalwStepOutLenV1, PalwStepTableV1, kernel_semantics_id_v1};
use crate::palw_step_refute::{PalwRouterFamilyV2, palw_router_family_v2};

pub const PALW_ROUTER_BINDING_VERSION_V2: u16 = 1;
/// **Kernel-route aux table 139** — one row per class, keyed by the 64-byte class id.
pub const PALW_ROUTER_BINDING_TABLE_V2: u8 = 139;
/// The ML-DSA-87 context of the class registrant's signature over [`palw_router_binding_message_v2`].
pub const PALW_ROUTER_BINDING_SIGNATURE_CONTEXT_V2: &[u8] = b"misaka/palw/router-binding/v2";

/// Which layer-table template a binding entry addresses.
pub const PALW_ROUTER_BINDING_TABLE_GDN_V2: u8 = 0;
pub const PALW_ROUTER_BINDING_TABLE_ATTN_V2: u8 = 1;

/// One routed reader of a layer-table template and the router it reads (both table-relative), applying to every layer instance of
/// that template.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwRoutedReaderBindingV2 {
    /// [`PALW_ROUTER_BINDING_TABLE_GDN_V2`] or [`PALW_ROUTER_BINDING_TABLE_ATTN_V2`].
    pub table: u8,
    pub reader: u16,
    pub router: u16,
}

/// **The registrant's declaration** (carried on tag 244, signed by the class's registrant bond).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwRouterBindingDeclarationV2 {
    pub version: u16,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    /// The class's registered graph, carried whole and authenticated by `shape_profile_id() == class_id` (the class id IS the shape
    /// profile id), so the binding is checked against the graph the claims run without the chain holding it.
    pub profile: PalwShapeProfileV3,
    /// Every routed reader, ascending by `(table, reader)`.
    pub readers: Vec<PalwRoutedReaderBindingV2>,
}

/// **The stored row** (aux 139). Immutable once written; read for claims accepted at or after `bound_from_daa`.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwRouterBindingRowV2 {
    pub version: u16,
    pub bound_from_daa: u64,
    pub readers: Vec<PalwRoutedReaderBindingV2>,
}

/// The message the registrant signs: the network domain and the whole declaration.
pub fn palw_router_binding_message_v2(network_domain: Hash64, declaration: &PalwRouterBindingDeclarationV2) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).personal(b"MISAKA_PALW_V1").to_state();
    h.update(b"router_binding_declaration/v2");
    h.update(network_domain.as_byte_slice());
    h.update(&borsh::to_vec(declaration).expect("a router binding declaration has an infallible Vec writer"));
    Hash64::from_bytes(h.finalize().as_bytes().try_into().expect("the requested hash length"))
}

fn table_kind_v2(table: u8) -> Option<PalwStepTableV1> {
    match table {
        PALW_ROUTER_BINDING_TABLE_GDN_V2 => Some(PalwStepTableV1::Gdn),
        PALW_ROUTER_BINDING_TABLE_ATTN_V2 => Some(PalwStepTableV1::Attn),
        _ => None,
    }
}

/// The layer-table templates a profile actually instantiates, with their binding tag.
fn instantiated_tables_v2(profile: &PalwShapeProfileV3) -> Vec<(u8, &[crate::palw_step::PalwStepNodeV1])> {
    let mut out = Vec::new();
    if (0..profile.layer_count).any(|l| profile.layer_kind(l) == PalwLayerKindV1::GatedDeltaNet) {
        out.push((PALW_ROUTER_BINDING_TABLE_GDN_V2, profile.gdn_nodes.as_slice()));
    }
    if (0..profile.layer_count).any(|l| profile.layer_kind(l) == PalwLayerKindV1::Attention) {
        out.push((PALW_ROUTER_BINDING_TABLE_ATTN_V2, profile.attn_nodes.as_slice()));
    }
    out
}

/// **Every routed reader of the profile**, ascending by `(table, reader)` — the set a binding must name exactly.
pub fn palw_routed_readers_v2(profile: &PalwShapeProfileV3) -> Vec<(u8, u16)> {
    let mut out = Vec::new();
    for (tag, table) in instantiated_tables_v2(profile) {
        for (index, node) in table.iter().enumerate() {
            if crate::palw_step_refute::qwen36_reads_routing_v1(node) {
                out.push((tag, index as u16));
            }
        }
    }
    out
}

fn fixed(out: &PalwStepOutLenV1) -> Option<u64> {
    match out {
        PalwStepOutLenV1::Fixed { elements } => Some(u64::from(*elements)),
        PalwStepOutLenV1::KvScaled { .. } => None,
    }
}

/// The number of expert blocks a routed reader's output carries (`None`: a reader whose arithmetic the court does not serve).
fn reader_blocks_v2(profile: &PalwShapeProfileV3, reader: &crate::palw_step::PalwStepNodeV1) -> Option<u64> {
    let out = fixed(&reader.out_len)?;
    let name = reader.weight_name.as_str();
    let block =
        if name.ends_with(".ffn_gate_exps.routed") || name.ends_with(".ffn_up_exps.routed") || name.ends_with(".ffn_expert_gated.a16")
        {
            u64::from(profile.ffn_dim)
        } else if name.ends_with(".ffn_down_exps.routed") {
            u64::from(profile.hidden_dim)
        } else {
            return None;
        };
    (block != 0 && out.is_multiple_of(block)).then(|| out / block)
}

/// **Is `router` an admissible router for `reader` in `table`?** `Ok(k)` with the router's `k`, or why not.
fn admissible_router_v2(
    profile: &PalwShapeProfileV3,
    table: &[crate::palw_step::PalwStepNodeV1],
    reader: u16,
    router: u16,
) -> Result<u64, &'static str> {
    if router >= reader {
        return Err("a bound router must precede its reader in the table");
    }
    let r = table.get(router as usize).ok_or("a bound router is outside the table")?;
    let reader_node = table.get(reader as usize).ok_or("a bound reader is outside the table")?;
    palw_router_family_v2(r.kernel_semantics_id).ok_or("a bound router is not a router node")?;
    let width = fixed(&r.out_len).ok_or("a router's width is fixed")?;
    if width < 2 || !width.is_multiple_of(2) {
        return Err("a router's row is two lanes per chosen expert");
    }
    let k = width / 2;
    let source = *r.input_refs.first().ok_or("a router names its score row")?;
    if source >= crate::palw_step::PALW_STEP_INPUT_SENTINEL_MIN || source >= router {
        return Err("a router's score row is an earlier node of its table");
    }
    let universe = fixed(&table[source as usize].out_len).ok_or("a router's score universe is fixed-width")?;
    if universe < k {
        return Err("a router chooses more experts than its score universe holds");
    }
    if reader_blocks_v2(profile, reader_node) != Some(k) {
        return Err("a routed reader's expert blocks are not its router's k");
    }
    Ok(k)
}

/// The routing reference a weight-consuming combine reads, and the family its arithmetic requires.
fn combine_routing_ref_v2(node: &crate::palw_step::PalwStepNodeV1) -> Option<(usize, PalwRouterFamilyV2)> {
    if node.kernel_semantics_id == kernel_semantics_id_v1(crate::palw_step_refute::KDESC_Q36_MOE_COMBINE) {
        Some((1, PalwRouterFamilyV2::Qwen36))
    } else if node.kernel_semantics_id == kernel_semantics_id_v1(crate::palw_step_refute::KDESC_KIMI_MOE_COMBINE) {
        Some((2, PalwRouterFamilyV2::Kimi))
    } else {
        None
    }
}

/// **Weight semantics stay with their family**: every combine's declared routing reference is a router of the combine's own
/// family. `Err` names the first violation.
pub fn palw_combine_router_families_v2(profile: &PalwShapeProfileV3) -> Result<(), &'static str> {
    for (_, table) in instantiated_tables_v2(profile) {
        for node in table {
            let Some((ordinal, family)) = combine_routing_ref_v2(node) else { continue };
            let r = *node.input_refs.get(ordinal).ok_or("a combine names its routing reference")?;
            if r >= crate::palw_step::PALW_STEP_INPUT_SENTINEL_MIN {
                return Err("a combine's routing reference is a table node");
            }
            let router = table.get(r as usize).ok_or("a combine's routing reference is outside the table")?;
            if palw_router_family_v2(router.kernel_semantics_id) != Some(family) {
                return Err("a combine reads another family's routing weights: q36 and Kimi weight arithmetic are separate");
            }
        }
    }
    Ok(())
}

/// **The whole declaration check** — version, the class's own graph, the exact reader set, each binding's admissibility, the
/// combine families. Pure; the fold adds the registrant, the fence and immutability.
pub fn palw_router_binding_check_v2(declaration: &PalwRouterBindingDeclarationV2) -> Result<(), &'static str> {
    if declaration.version != PALW_ROUTER_BINDING_VERSION_V2 {
        return Err("router binding version is not canonical");
    }
    let profile = &declaration.profile;
    if profile.shape_profile_id() != declaration.class_id {
        return Err("the carried graph is not the class's registered profile");
    }
    profile.validate_shape().map_err(|_| "the class's graph is not a valid canonical graph")?;
    let required = palw_routed_readers_v2(profile);
    if required.is_empty() {
        return Err("the class has no routed reader to bind");
    }
    let named: Vec<(u8, u16)> = declaration.readers.iter().map(|b| (b.table, b.reader)).collect();
    if named != required {
        return Err("the binding must name exactly the class's routed readers, ascending, once each");
    }
    for binding in &declaration.readers {
        let kind = table_kind_v2(binding.table).ok_or("unknown table tag")?;
        let table = match kind {
            PalwStepTableV1::Gdn => profile.gdn_nodes.as_slice(),
            _ => profile.attn_nodes.as_slice(),
        };
        admissible_router_v2(profile, table, binding.reader, binding.router)?;
    }
    palw_combine_router_families_v2(profile)
}

/// **Could this class be bound at all?** Every routed reader has at least one admissible router and the combines keep their
/// families — the registration-time coverage question, asked before any binding exists.
pub fn palw_router_binding_possible_v2(profile: &PalwShapeProfileV3) -> bool {
    for (tag, table) in instantiated_tables_v2(profile) {
        for (index, node) in table.iter().enumerate() {
            if !crate::palw_step_refute::qwen36_reads_routing_v1(node) {
                continue;
            }
            let reader = index as u16;
            if !(0..reader).any(|router| admissible_router_v2(profile, table, reader, router).is_ok()) {
                return false;
            }
            let _ = tag;
        }
    }
    palw_combine_router_families_v2(profile).is_ok()
}

/// **Does `row` bind every routed reader of `profile`** (the claim-gate / court coverage question once a binding exists)?
pub fn palw_router_binding_covers_v2(profile: &PalwShapeProfileV3, row: &PalwRouterBindingRowV2) -> bool {
    let named: Vec<(u8, u16)> = row.readers.iter().map(|b| (b.table, b.reader)).collect();
    named == palw_routed_readers_v2(profile)
}

/// **The global slot of the router bound to the routed reader at `out_slot`** — `None` for a reader the row does not name (or a
/// slot outside a layer table). The court passes this as `bound_router`; `qwen36_router_slot_for_reader_v2` re-checks it.
pub fn palw_bound_router_slot_v2(profile: &PalwShapeProfileV3, row: &PalwRouterBindingRowV2, out_slot: u32) -> Option<u32> {
    let (_, layer) = profile.resolve_node_slot(out_slot)?;
    let layer = layer?;
    let (kind, tag) = match profile.layer_kind(layer) {
        PalwLayerKindV1::GatedDeltaNet => (PalwStepTableV1::Gdn, PALW_ROUTER_BINDING_TABLE_GDN_V2),
        PalwLayerKindV1::Attention => (PalwStepTableV1::Attn, PALW_ROUTER_BINDING_TABLE_ATTN_V2),
    };
    let first = profile.global_node_slot(kind, layer, 0)?;
    let reader = u16::try_from(out_slot.checked_sub(first)?).ok()?;
    let binding = row.readers.iter().find(|b| b.table == tag && b.reader == reader)?;
    first.checked_add(u32::from(binding.router))
}

/// **The table-relative index of the router `row` binds to `reader` of the template tagged `table`** — `None` where the row does
/// not name the reader. The admission re-derivation prices the routing appendix from this router, never from a table-wide guess.
pub fn palw_bound_router_index_v2(row: &PalwRouterBindingRowV2, table: u8, reader: u16) -> Option<u16> {
    row.readers.iter().find(|b| b.table == table && b.reader == reader).map(|b| b.router)
}

/// The aux 139 key of a class's row.
pub fn palw_router_binding_key_v2(class_id: &Hash64) -> Vec<u8> {
    class_id.as_byte_slice().to_vec()
}

#[cfg(test)]
mod tests;
