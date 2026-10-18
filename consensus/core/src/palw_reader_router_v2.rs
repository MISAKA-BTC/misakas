//! Versioned per-reader routing identity. These declarations identify which
//! committed router row a reader consumes. They do not establish computation
//! correctness: router arithmetic and the reader are directly recomputed by
//! their own deterministic court consumers. Validator approval remains separate.
//!
//! Allocation: native carrier 244 and kernel auxiliary table 136 (class key64),
//! journaled by the existing kernel row delta and retained in its 0xEC carriage.
//! Old profiles, roles, V1 implicit-router selection and old roots are unchanged.
use crate::Hash64;
use crate::palw_native_resource_v1::{NativeCoverageV1, TirNativeRequestsV1, reserve_native_requests_v1};
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step::{PALW_STEP_INPUT_SENTINEL_MIN, PalwShapeProfileV3, PalwStepNodeV1, PalwStepOutLenV1};
use crate::palw_step_refute::{
    KDESC_KIMI_MOE_COMBINE, KDESC_KIMI_ROUTER_TOPK, KDESC_Q36_MOE_COMBINE, KDESC_Q36_ROUTER_TOPK, kernel_semantics_id_v1,
};

pub const PALW_READER_ROUTER_TABLE_V2: u8 = 136;
pub const PALW_READER_ROUTER_VERSION_V2: u16 = 2;
pub const PALW_READER_ROUTER_DOMAIN_V2: &[u8] = b"misaka-palw/reader-router/declaration/v2";
/// Existing registrant identity signature role, with an independent versioned
/// message domain. This signs the whole declaration, never a challenger choice.
pub const PALW_READER_ROUTER_SIGNATURE_CONTEXT_V2: &[u8] = crate::palw_class_semantics_v1::PALW_CLASS_SEMANTICS_SIGNATURE_CONTEXT_V1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwReaderRouterBindingV2 {
    pub reader_slot: u32,
    pub router_slot: u32,
}

/// Native class owner signs one complete, sorted mapping before claims. There
/// is no caller-chosen expert universe, timestamp, runtime or tokenizer identity.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwReaderRouterDeclarationV2 {
    pub version: u16,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub bindings: Vec<PalwReaderRouterBindingV2>,
}

/// Acceptance supplies the original registrant and actual native DAA. Persisted
/// until its class is removed, independent of claim GC, snapshots or restart.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwReaderRouterRowV2 {
    pub declaration: PalwReaderRouterDeclarationV2,
    pub registrant: PalwBondKeyV2,
    pub bound_from_daa: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum PalwReaderRouterErrorV2 {
    #[error("reader/router identity: {0}")]
    Identity(&'static str),
    #[error("local reader/router capacity unavailable: {0:?}")]
    Resource(NativeCoverageV1),
}
impl From<NativeCoverageV1> for PalwReaderRouterErrorV2 {
    fn from(why: NativeCoverageV1) -> Self {
        Self::Resource(why)
    }
}
fn bad(message: &'static str) -> PalwReaderRouterErrorV2 {
    PalwReaderRouterErrorV2::Identity(message)
}

/// Whole-layer uniqueness is deliberately absent: two independent routers can
/// coexist when every reader has its own immutable selected-router binding.
pub fn needs_reader_router_v2(node: &PalwStepNodeV1) -> bool {
    crate::palw_step_refute::qwen36_reads_routing_v1(node)
        || node.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_MOE_COMBINE)
        || node.kernel_semantics_id == kernel_semantics_id_v1(KDESC_KIMI_MOE_COMBINE)
}

fn fixed(node: &PalwStepNodeV1) -> Result<u32, PalwReaderRouterErrorV2> {
    match node.out_len {
        PalwStepOutLenV1::Fixed { elements } if elements > 0 => Ok(elements),
        _ => Err(bad("routing identity requires a nonzero fixed-width registered row")),
    }
}

impl PalwReaderRouterDeclarationV2 {
    /// No allocation or sort: the wire mapping must already be canonical.
    pub fn check_shape(&self, maximum_nodes: u32) -> Result<(), PalwReaderRouterErrorV2> {
        if self.version != PALW_READER_ROUTER_VERSION_V2
            || self.bindings.is_empty()
            || self.bindings.len() as u64 > u64::from(maximum_nodes)
        {
            return Err(bad("reader/router declaration version or geometry-derived count is invalid"));
        }
        let mut previous = None;
        for b in &self.bindings {
            if b.reader_slot >= maximum_nodes
                || b.router_slot >= maximum_nodes
                || b.router_slot >= b.reader_slot
                || previous.is_some_and(|old| old >= b.reader_slot)
            {
                return Err(bad("reader/router slots must be sorted, unique and refer to an earlier router"));
            }
            previous = Some(b.reader_slot);
        }
        Ok(())
    }

    /// Resolve a mapping only after this declaration has been authenticated as
    /// the immutable native class row and checked against the actual profile.
    pub fn selected_router(&self, reader_slot: u32) -> Option<u32> {
        self.bindings.binary_search_by_key(&reader_slot, |b| b.reader_slot).ok().map(|i| self.bindings[i].router_slot)
    }

    /// The accepted profile is authoritative. Geometry, expert count and the
    /// router family all come from it, not from untrusted court evidence.
    pub fn check_profile(&self, profile: &PalwShapeProfileV3) -> Result<(), PalwReaderRouterErrorV2> {
        self.check_shape(profile.global_node_count())?;
        // shape_profile_id uses a canonical Vec. Reserve its actual encoding
        // request before hashing; validation otherwise borrows existing rows.
        let mut q = TirNativeRequestsV1::new();
        q.vec_v1::<u8>((borsh::object_length(profile).map_err(|_| bad("cannot measure registered profile"))? as u64).max(1024))?;
        let _hash = reserve_native_requests_v1(q.bytes_v1())?;
        if profile.shape_profile_id() != self.class_id {
            return Err(bad("routing declaration names another registered profile"));
        }
        let mut supplied = self.bindings.iter();
        for slot in 0..profile.global_node_count() {
            let (reader, layer) =
                profile.resolve_node_slot(slot).ok_or_else(|| bad("registered profile contains an unresolved slot"))?;
            if !needs_reader_router_v2(reader) {
                continue;
            }
            let binding = supplied.next().ok_or_else(|| bad("a routed reader has no immutable router binding"))?;
            if binding.reader_slot != slot {
                return Err(bad("binding set is not the exact registered routed-reader inventory"));
            }
            let layer = layer.ok_or_else(|| bad("routed reader is outside a layer table"))?;
            let (router, router_layer) =
                profile.resolve_node_slot(binding.router_slot).ok_or_else(|| bad("selected router is absent"))?;
            if router_layer != Some(layer) {
                return Err(bad("selected router belongs to a different layer table"));
            }
            if router.kernel_semantics_id != kernel_semantics_id_v1(KDESC_Q36_ROUTER_TOPK)
                && router.kernel_semantics_id != kernel_semantics_id_v1(KDESC_KIMI_ROUTER_TOPK)
            {
                return Err(bad("selected row is not a registered Q36 or Kimi router"));
            }
            let reader_k = reader.kernel_semantics_id;
            if (reader_k == kernel_semantics_id_v1(KDESC_Q36_MOE_COMBINE)
                && router.kernel_semantics_id != kernel_semantics_id_v1(KDESC_Q36_ROUTER_TOPK))
                || (reader_k == kernel_semantics_id_v1(KDESC_KIMI_MOE_COMBINE)
                    && router.kernel_semantics_id != kernel_semantics_id_v1(KDESC_KIMI_ROUTER_TOPK))
            {
                return Err(bad("combine family differs from its selected router arithmetic"));
            }
            let routing_width = fixed(router)?;
            if !routing_width.is_multiple_of(2) || router.input_refs.len() != 1 || router.tile_len == 0 {
                return Err(bad("selected router must contain ids and weights with one score input"));
            }
            let table = profile.layer_table(layer);
            let mut table_first = None;
            for (local, candidate) in table.iter().enumerate() {
                if std::ptr::eq(candidate, router) {
                    table_first = binding.router_slot.checked_sub(local as u32);
                    break;
                }
            }
            let table_first = table_first.ok_or_else(|| bad("selected router is not in its declared layer table"))?;
            let score_reference = router.input_refs[0];
            if score_reference >= PALW_STEP_INPUT_SENTINEL_MIN {
                return Err(bad("selected router score source is not an actual node"));
            }
            let score_slot = table_first.checked_add(u32::from(score_reference)).ok_or_else(|| bad("router score slot overflows"))?;
            if score_slot >= binding.router_slot {
                return Err(bad("selected router score source does not precede its output"));
            }
            let (score, score_layer) =
                profile.resolve_node_slot(score_slot).ok_or_else(|| bad("selected router score node is absent"))?;
            if score_layer != Some(layer) || routing_width / 2 > fixed(score)? {
                return Err(bad("selected router chooses more experts than its own score universe"));
            }
            let reader_width = fixed(reader)?;
            let k = routing_width / 2;
            if crate::palw_step_refute::qwen36_reads_routing_v1(reader) && !reader_width.is_multiple_of(k) {
                return Err(bad("routed reader width does not consist of whole selected expert blocks"));
            }
            let combine = reader.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_MOE_COMBINE)
                || reader.kernel_semantics_id == kernel_semantics_id_v1(KDESC_KIMI_MOE_COMBINE);
            if combine {
                let routing_ref = if reader.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_MOE_COMBINE) { 1 } else { 2 };
                if reader.input_refs.len() != routing_ref + 1 {
                    return Err(bad("combine input count differs from its exact kernel family"));
                }
                let routed_ref = reader.input_refs[0];
                if routed_ref >= PALW_STEP_INPUT_SENTINEL_MIN {
                    return Err(bad("combine routed outputs are not a node"));
                }
                let (routed, routed_layer) = profile
                    .resolve_node_slot(table_first + u32::from(routed_ref))
                    .ok_or_else(|| bad("combine routed output is absent"))?;
                if routed_layer != Some(layer) || k.checked_mul(reader_width) != Some(fixed(routed)?) {
                    return Err(bad("combine routed output width differs from its selected router expert count"));
                }
                if reader.kernel_semantics_id == kernel_semantics_id_v1(KDESC_KIMI_MOE_COMBINE) {
                    let shared_ref = reader.input_refs[1];
                    if shared_ref >= PALW_STEP_INPUT_SENTINEL_MIN {
                        return Err(bad("Kimi shared outputs are not a node"));
                    }
                    let (shared, shared_layer) = profile
                        .resolve_node_slot(table_first + u32::from(shared_ref))
                        .ok_or_else(|| bad("Kimi shared output is absent"))?;
                    if shared_layer != Some(layer)
                        || reader_width.checked_mul(crate::palw_kimi_k3_ops::KIMI_K3_SHARED_EXPERTS as u32) != Some(fixed(shared)?)
                    {
                        return Err(bad("Kimi shared output width differs from the kernel's two shared experts"));
                    }
                }
                let reference = *reader.input_refs.get(routing_ref).ok_or_else(|| bad("combine lacks its explicit routing input"))?;
                if reference >= PALW_STEP_INPUT_SENTINEL_MIN
                    || table_first.checked_add(u32::from(reference)) != Some(binding.router_slot)
                {
                    return Err(bad("combine explicit routing input differs from the immutable selected router"));
                }
            }
        }
        if supplied.next().is_some() {
            return Err(bad("router declaration contains an unrelated extra reader"));
        }
        Ok(())
    }
}

/// Stream the complete signed mapping, with no canonical Vec or deep clone.
pub fn reader_router_message_v2(network: Hash64, declaration: &PalwReaderRouterDeclarationV2) -> Hash64 {
    struct Stream(blake2b_simd::State);
    impl std::io::Write for Stream {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut stream = Stream(blake2b_simd::Params::new().hash_length(64).key(PALW_READER_ROUTER_DOMAIN_V2).to_state());
    borsh::to_writer(&mut stream, &(network, declaration)).expect("hash stream writer cannot fail");
    Hash64::from_bytes(stream.0.finalize().as_bytes().try_into().expect("64-byte hash"))
}

/// Resolve this reader against its authenticated mapping and exact native
/// profile. Consumers must obtain the row from `reader_router_of_claim_v2`;
/// accepting a challenger's declaration here would choose its evidence.
pub fn selected_reader_router_v2<'a>(
    profile: &'a PalwShapeProfileV3,
    declaration: &PalwReaderRouterDeclarationV2,
    reader_slot: u32,
) -> Result<(u32, &'a PalwStepNodeV1), PalwReaderRouterErrorV2> {
    declaration.check_profile(profile)?;
    let slot = declaration.selected_router(reader_slot).ok_or_else(|| bad("reader lacks its immutable binding"))?;
    let (router, _) = profile.resolve_node_slot(slot).ok_or_else(|| bad("selected router is absent"))?;
    Ok((slot, router))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::palw_step::{PalwStepNodeRoleV1, PalwStepOpKindV1};
    pub(crate) fn profile_and_mapping(kimi: bool) -> (PalwShapeProfileV3, PalwReaderRouterDeclarationV2) {
        let mut p = crate::palw_step_refute::tests::profile();
        let node = |desc: &str, width, refs: &[u16], name: &str| PalwStepNodeV1 {
            op_kind: PalwStepOpKindV1::MatMulQuant,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: name.into(),
            weight_dtypes: vec![],
            out_len: PalwStepOutLenV1::Fixed { elements: width },
            tile_len: 4,
            kernel_semantics_id: kernel_semantics_id_v1(desc),
            input_refs: refs.to_vec(),
        };
        let router = if kimi { KDESC_KIMI_ROUTER_TOPK } else { KDESC_Q36_ROUTER_TOPK };
        let combine = if kimi { KDESC_KIMI_MOE_COMBINE } else { KDESC_Q36_MOE_COMBINE };
        p.gdn_nodes = vec![
            node(crate::palw_step_refute::KDESC_Q36_SIGMOID, 16, &[crate::palw_step::PALW_STEP_INPUT_LAYER_IN], "score_a"),
            node(router, 4, &[0], "route_a"),
            node(crate::palw_step_refute::KDESC_Q36_SIGMOID, 16, &[0], "score_b"),
            node(router, 8, &[2], "route_b"),
            node(crate::palw_step_refute::KDESC_Q36_MATMUL_GROUPED, 16, &[0], "a.routed"),
            node(crate::palw_step_refute::KDESC_Q36_MATMUL_GROUPED, 32, &[0], "b.routed"),
            node(combine, 8, if kimi { &[4, 0, 1] } else { &[4, 1] }, "combine_a"),
            node(combine, 8, if kimi { &[5, 0, 3] } else { &[5, 3] }, "combine_b"),
        ];
        let base = p.pre_nodes.len() as u32;
        let d = PalwReaderRouterDeclarationV2 {
            version: 2,
            class_id: p.shape_profile_id(),
            artifact_root: Hash64::from_u64_word(7),
            bindings: [(4, 1), (5, 3), (6, 1), (7, 3)]
                .map(|(reader, router)| PalwReaderRouterBindingV2 { reader_slot: base + reader, router_slot: base + router })
                .to_vec(),
        };
        (p, d)
    }
    #[test]
    fn independent_q36_and_kimi_routers_bind_every_reader_without_layer_uniqueness() {
        for kimi in [false, true] {
            let (p, d) = profile_and_mapping(kimi);
            d.check_profile(&p).unwrap();
            assert_ne!(d.selected_router(d.bindings[0].reader_slot), d.selected_router(d.bindings[1].reader_slot));
            let mut wrong = d.clone();
            wrong.bindings[3].router_slot = wrong.bindings[0].router_slot;
            assert!(wrong.check_profile(&p).is_err(), "combine's explicit third input cannot be rebound");
            let mut missing = d.clone();
            missing.bindings.remove(0);
            assert!(missing.check_profile(&p).is_err());
            let mut duplicate = d.clone();
            duplicate.bindings[1] = duplicate.bindings[0];
            assert!(duplicate.check_profile(&p).is_err());
            let mut foreign = d.clone();
            foreign.class_id = Hash64::from_u64_word(99);
            assert!(foreign.check_profile(&p).is_err());
            let a = reader_router_message_v2(Hash64::from_u64_word(1), &d);
            assert_ne!(a, reader_router_message_v2(Hash64::from_u64_word(2), &d));
            let mut changed = d.clone();
            changed.artifact_root = Hash64::from_u64_word(9);
            assert_ne!(a, reader_router_message_v2(Hash64::from_u64_word(1), &changed));
        }
    }
    #[test]
    fn actual_q36_and_kimi_graphs_bind_their_own_combine_ref_and_score_universe() {
        let q36 = crate::palw_qwen36_profile::qwen36_profile_v2(crate::palw_qwen36_profile::QWEN36_35B_A3B).unwrap();
        let kimi = crate::palw_kimi_k3_profile::kimi_k3_profile_v1(crate::palw_kimi_k3_profile::KIMI_K3_CARD).unwrap();
        for profile in [q36, kimi] {
            // These actual source graphs contain one router in each layer. This
            // fixture constructs the explicit owner mapping; production lookup
            // never substitutes this uniqueness scan for a missing declaration.
            let mut bindings = Vec::new();
            for reader_slot in 0..profile.global_node_count() {
                let (reader, layer) = profile.resolve_node_slot(reader_slot).unwrap();
                if !needs_reader_router_v2(reader) {
                    continue;
                }
                let routers: Vec<_> = (0..reader_slot)
                    .filter(|slot| {
                        let (n, l) = profile.resolve_node_slot(*slot).unwrap();
                        l == layer
                            && (n.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_ROUTER_TOPK)
                                || n.kernel_semantics_id == kernel_semantics_id_v1(KDESC_KIMI_ROUTER_TOPK))
                    })
                    .collect();
                assert_eq!(routers.len(), 1);
                bindings.push(PalwReaderRouterBindingV2 { reader_slot, router_slot: routers[0] });
            }
            let declaration = PalwReaderRouterDeclarationV2 {
                version: 2,
                class_id: profile.shape_profile_id(),
                artifact_root: Hash64::from_u64_word(7),
                bindings,
            };
            declaration.check_profile(&profile).unwrap();
        }
    }

    #[test]
    fn declaration_has_additive_wire_kind_and_old_fence_refuses_activation() {
        let (_, d) = profile_and_mapping(false);
        let object =
            crate::palw_state_v2::PalwConsensusObjectV2::ReaderRouterDeclaredV2 { declaration: Box::new(d), signature: vec![1] };
        assert_eq!(borsh::to_vec(&object).unwrap()[0], 244);
        assert!(crate::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object).is_ok());
        let mut p = crate::config::params::TESTNET_PARAMS;
        p.palw_memory_routing_court_v2 = Some(crate::config::params::ForkActivation::new(1));
        assert!(p.validate_palw_memory_routing_court_v2().is_err());
    }
}
