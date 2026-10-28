//! Proposed owner-independent request universe for RFC-0026 F03. Shape/scope
//! derivation only, not admission, service packets, paid capacity or activation.
//! Enumerates every request admitted by the current native source validator,
//! including overlapping runs and non-bottom interior levels. A later adopted
//! profile may share response data, but cannot silently omit legal choices.
use crate::palw_tir_court_v1::{palw_tir_step_tree_height_v1 as height, palw_tir_step_tree_width_v1 as width};
use crate::{
    Hash64,
    palw_tir_public_claim_v1::PalwTirPublicClaimV1,
    palw_tir_public_source_v1::{
        PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1, PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1, PalwTirPublicRequestV1 as Request,
        PalwTirPublicSourceV1 as Source,
    },
};

fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| "shared request count overflow".into())
}
fn mul(a: u64, b: u64) -> Result<u64, String> {
    a.checked_mul(b).ok_or_else(|| "shared request count overflow".into())
}
fn nodes(n: u64) -> Result<u64, String> {
    (1..=height(n)).try_fold(0, |sum, level| add(sum, width(n, level).ok_or("shared tree geometry")?))
}
fn node_at(n: u64, mut ordinal: u64) -> Result<(u8, u64), String> {
    for level in 1..=height(n) {
        let count = width(n, level).ok_or("shared tree geometry")?;
        if ordinal < count {
            return Ok((level, ordinal));
        }
        ordinal -= count;
    }
    Err("shared tree ordinal is outside geometry".into())
}
#[derive(Clone, Debug)]
struct Geometry {
    n: u64,
    d: u64,
    tiles: u64,
    families: [u64; 7],
    total: u64,
}
impl Geometry {
    fn new(n: u64, d: u64, vocab: u64, tiled: bool) -> Result<Self, String> {
        if n == 0 || vocab == 0 {
            return Err("shared request geometry is empty".into());
        }
        let runs = (1..=n.min(u64::from(PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1))).try_fold(0, |sum, count| add(sum, n - (count - 1)))?;
        let tiles = vocab.div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
        let families = [
            nodes(n)?,
            n,
            runs,
            if tiled { nodes(d)? } else { 0 },
            if tiled { mul(d, nodes(tiles)?)? } else { 0 },
            if tiled { mul(d, tiles)? } else { 0 },
            if tiled { d.div_ceil(u64::from(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1)) } else { 0 },
        ];
        let total = families.iter().try_fold(0, |sum, &count| add(sum, count))?;
        Ok(Self { n, d, tiles, families, total })
    }
    fn native_demands(&self) -> Result<Self, String> {
        let mut geometry = self.clone();
        // Native owned Ids{0} concatenates the complete source ID sequence.
        geometry.families[6] = u64::from(geometry.families[6] != 0);
        geometry.total = geometry.families.iter().try_fold(0, |sum, &n| add(sum, n))?;
        Ok(geometry)
    }
    fn at(&self, mut ordinal: u64) -> Result<Request, String> {
        if ordinal >= self.total {
            return Err("shared request ordinal is outside complete set".into());
        }
        let mut family = 0;
        while ordinal >= self.families[family] {
            ordinal -= self.families[family];
            family += 1;
        }
        Ok(match family {
            0 => {
                let (level, index) = node_at(self.n, ordinal)?;
                Request::StepNode { level, index }
            }
            1 => Request::StepLeaf { index: ordinal },
            2 => {
                let mut count = 1;
                while ordinal >= self.n - (count - 1) {
                    ordinal -= self.n - (count - 1);
                    count += 1;
                }
                Request::StepRun { first: ordinal, count: u32::try_from(count).map_err(|_| "shared run count overflow")? }
            }
            3 => {
                let (level, index) = node_at(self.d, ordinal)?;
                Request::RowsNode { level, index }
            }
            4 => {
                let per_row = nodes(self.tiles)?;
                let (level, index) = node_at(self.tiles, ordinal % per_row)?;
                Request::TileNode { row: u32::try_from(ordinal / per_row).map_err(|_| "shared row overflow")?, level, index }
            }
            5 => Request::TraceTile {
                row: u32::try_from(ordinal / self.tiles).map_err(|_| "shared row overflow")?,
                tile: u32::try_from(ordinal % self.tiles).map_err(|_| "shared tile overflow")?,
            },
            6 => Request::Ids {
                offset: u32::try_from(mul(ordinal, u64::from(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1))?)
                    .map_err(|_| "shared ID offset overflow")?,
            },
            _ => unreachable!("ordinal was checked against exact total"),
        })
    }
}

/// The source must come from native original geometry; this checks its binding
/// to the supplied frame. The caller still authenticates accepted state/profile.
/// No owner, queue position, requester, relay, wall clock or participation input.
pub struct TirSharedRequestSetV1<'a> {
    source: &'a Source,
    geometry: Geometry,
    claim: Hash64,
    allocation: Hash64,
    native_rules: Hash64,
    shared_profile: Hash64,
    custodians: u64,
    producer: crate::palw_state_v2::PalwBondKeyV2,
}
impl<'a> TirSharedRequestSetV1<'a> {
    pub fn new(frame: &PalwTirPublicClaimV1, source: &'a Source, shared_profile: Hash64) -> Result<Self, String> {
        let p = frame.plan_v1();
        if shared_profile == Hash64::default()
            || p.rules_id == Hash64::default()
            || p.allocation_id == Hash64::default()
            || p.original_claim != crate::palw_freeprompt_v3::fp_claim_id_v3(frame.original_v1())
            || p.source_id != source.source_id_v1()
            || frame.header_v1() != source.header_v1()
        {
            return Err("shared request set substitutes original scope or lacks profile".into());
        }
        Ok(Self {
            source,
            geometry: Geometry::new(
                source.leaf_count_v1(),
                u64::from(source.decode_count_v1()),
                u64::from(source.vocab_v1()),
                source.supports_tiled_trace_v1(),
            )?,
            claim: p.original_claim,
            allocation: p.allocation_id,
            native_rules: p.rules_id,
            shared_profile,
            custodians: frame.custody_v1().pledge_v1().maximum_custodians,
            producer: frame.custody_v1().pledge_v1().producer,
        })
    }
    /// Identity of the complete closed source-query universe, not a funded
    /// admission certificate. Existing per-query IDs remain unchanged.
    pub fn scope_id(&self) -> Result<Hash64, String> {
        let bytes = borsh::to_vec(&(
            1u16,
            self.claim,
            self.allocation,
            self.native_rules,
            self.shared_profile,
            self.source.source_id_v1(),
            self.geometry.total,
        ))
        .map_err(|e| e.to_string())?;
        Ok(Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka-palw/shared-tir-query-universe/v1")
                .hash(&bytes)
                .as_bytes()
                .try_into()
                .expect("64"),
        ))
    }
    pub fn source_id(&self) -> Hash64 {
        self.source.source_id_v1()
    }
    pub fn decode_count(&self) -> u32 {
        self.source.decode_count_v1()
    }
    /// Original-wide numerical topology beside the complete source universe.
    /// Native court/demand rules must be supplied by the authenticated caller;
    /// this does not price terminal witnesses or create service packets.
    pub fn numerical_tree_upper(
        &self,
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        court: &crate::palw_mode_v2::PalwCourtParamsV2,
        demand: crate::palw_tir_fence2_v1::PalwTirDemandRulesV1,
    ) -> Result<crate::palw_tir_public_source_v1::numerical_inventory_v1::TirPublicNumericalInventoryV1, String> {
        self.source.numerical_tree_upper_v1(registered, layout, court, demand)
    }
    /// All distinct producer-signed numerical disclosures, including actual new
    /// DTO framing and every path byte. Roots and terminal witnesses remain
    /// separate unfinished costs. This creates no packet or funded admission.
    pub fn numerical_disclosure_transport_upper(
        &self,
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        court: &crate::palw_mode_v2::PalwCourtParamsV2,
        demand: crate::palw_tir_fence2_v1::PalwTirDemandRulesV1,
    ) -> Result<TirSharedNumericalTransportUpperV1, String> {
        let numerical = self.numerical_tree_upper(registered, layout, court, demand)?;
        let empty = crate::palw_tir_dissect_v1::PalwTirDissectRoundV1 { version: 1, children: Vec::new() };
        let empty_body = borsh::object_length(&empty).map_err(|e| e.to_string())? as u64;
        // Unsent fixed-width serialization exemplar, never an admitted control
        // or asserted signature/hash. No signature/body buffer is allocated.
        let exemplar = crate::palw_court_shared_tir_round_v1::SignedSharedTirRoundV1 {
            version: 1,
            claim: self.claim,
            leaf: 0,
            root_id: self.source_id(),
            path: Vec::new(),
            parent_id: self.source_id(),
            signer: self.producer,
            round: Box::new(empty),
            signature: Vec::new(),
        };
        let overhead = signed_wire_length(&exemplar)?.checked_sub(empty_body).ok_or("numerical disclosure framing underflow")?;
        let tree = &numerical.trees;
        let signed_bytes_upper = add(add(tree.round_body_bytes_upper, mul(tree.disclosures, overhead)?)?, tree.disclosure_path_bytes)?;
        let maximum_signed_bytes_upper = if tree.disclosures == 0 {
            0
        } else {
            add(
                add(tree.max_round_body_bytes_upper, overhead)?,
                u64::from(tree.max_depth.checked_sub(1).ok_or("numerical disclosure depth")?),
            )?
        };
        Ok(TirSharedNumericalTransportUpperV1 {
            scope_id: self.scope_id()?,
            numerical,
            signed_bytes_upper,
            maximum_signed_bytes_upper,
        })
    }
    /// Canonical numerical evidence slots under existing enforced payload
    /// ceilings, not a class-specific tight estimate or carrier admission.
    /// No source/semantic/archive/repeated-ingress/CPU/IO/retention/fee costs.
    pub fn numerical_evidence_transport_upper(
        &self,
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        court: &crate::palw_mode_v2::PalwCourtParamsV2,
        demand: crate::palw_tir_fence2_v1::PalwTirDemandRulesV1,
    ) -> Result<TirSharedNumericalEvidenceUpperV1, String> {
        let disclosures = self.numerical_disclosure_transport_upper(registered, layout, court, demand)?;
        let n = &disclosures.numerical;
        // Exactly the new root/bottom DTO field grammar with a zero-width
        // payload hole and empty signature. Unsent serialization shapes only.
        let root_fixed = signed_wire_length(&(
            1u16,
            self.claim,
            self.scope_id()?,
            self.source_id(),
            0u64,
            n.arity,
            self.producer,
            (),
            Vec::<u8>::new(),
        ))?;
        let bottom_fixed = signed_wire_length(&(
            1u16,
            self.claim,
            0u64,
            self.source_id(),
            Vec::<u8>::new(),
            self.source_id(),
            self.producer,
            (),
            Vec::<u8>::new(),
        ))?;
        let root_one = add(crate::palw_court_shared_tir_root_v1::shared_root_body_ceiling_v1()?, root_fixed)?;
        // Existing native proof enum has a one-byte TirDissection tag. The
        // shared control carries its body directly, without that old tag.
        let bottom_body = court.max_close_bytes().checked_sub(1).ok_or("native bottom ceiling lacks proof tag")?;
        let bottom_one = add(bottom_body, bottom_fixed)?;
        let roots_signed_bytes_upper = mul(n.eligible_roots, root_one)?;
        let bottoms_signed_bytes_upper = add(mul(n.trees.terminals, bottom_one)?, n.trees.terminal_path_bytes)?;
        let maximum_root_signed_bytes_upper = if n.eligible_roots == 0 { 0 } else { root_one };
        let maximum_bottom_signed_bytes_upper =
            if n.trees.terminals == 0 { 0 } else { add(bottom_one, u64::from(n.trees.max_depth))? };
        let signed_controls = add(add(n.eligible_roots, n.trees.disclosures)?, n.trees.terminals)?;
        let signed_bytes_upper = add(add(roots_signed_bytes_upper, disclosures.signed_bytes_upper)?, bottoms_signed_bytes_upper)?;
        Ok(TirSharedNumericalEvidenceUpperV1 {
            disclosures,
            roots_signed_bytes_upper,
            bottoms_signed_bytes_upper,
            maximum_root_signed_bytes_upper,
            maximum_bottom_signed_bytes_upper,
            signed_controls,
            signed_bytes_upper,
        })
    }
    /// A conservative SUM for every legal source query, without enumerating the
    /// O(N*256 + rows*tiles) universe. This is source data/parser/hash/RAM only,
    /// NOT a full service graph, signature/IO/fee or physical capacity bound.
    pub fn source_data_upper(&self) -> Result<TirSharedSourceDataUpperV1, String> {
        let mut result = TirSharedSourceDataUpperV1::default();
        self.visit_source_bounds(|count, _, bound| result.add(count, bound))?;
        if result.queries != self.len() {
            return Err("shared source bound omitted a legal family".into());
        }
        Ok(result)
    }
    /// Grouped maxima cover every source choice without enumerating queries.
    fn visit_source_bounds(
        &self,
        mut visit: impl FnMut(u64, Request, crate::palw_tir_public_source_v1::PalwTirPublicUnitBoundV1) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut charge = |count: u64, q: Request| -> Result<(), String> {
            if count == 0 {
                return Ok(());
            }
            visit(count, q, self.source.unit_bound_v1(&q)?)
        };
        // At each level the first node is full or the only partial node. Its
        // frontier is maximal; the ragged last node never has more children.
        for level in 1..=height(self.geometry.n) {
            charge(width(self.geometry.n, level).ok_or("shared node geometry")?, Request::StepNode { level, index: 0 })?;
        }
        charge(self.geometry.n, Request::StepLeaf { index: 0 })?;
        for count in 1..=self.geometry.n.min(u64::from(PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1)) {
            charge(self.geometry.n - (count - 1), Request::StepRun { first: 0, count: count as u32 })?;
        }
        if self.geometry.families[3..].iter().any(|&n| n != 0) && self.geometry.d != 0 {
            let d = self.geometry.d;
            for level in 1..=height(d) {
                charge(width(d, level).ok_or("shared rows geometry")?, Request::RowsNode { level, index: 0 })?;
            }
            for level in 1..=height(self.geometry.tiles) {
                charge(
                    mul(d, width(self.geometry.tiles, level).ok_or("shared tile geometry")?)?,
                    Request::TileNode { row: 0, level, index: 0 },
                )?;
            }
            let last = self.geometry.tiles - 1;
            charge(mul(d, last)?, Request::TraceTile { row: 0, tile: 0 })?;
            charge(d, Request::TraceTile { row: 0, tile: u32::try_from(last).map_err(|_| "shared tile overflow")? })?;
            let chunks = d.div_ceil(u64::from(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1));
            charge(chunks - 1, Request::Ids { offset: 0 })?;
            charge(
                1,
                Request::Ids {
                    offset: u32::try_from(mul(chunks - 1, u64::from(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1))?)
                        .map_err(|_| "shared ID offset overflow")?,
                },
            )?;
        }
        Ok(())
    }
    /// Distinct state-advancing DATA controls only. Not full funded admission,
    /// repeated/invalid ingress, semantic/court duties, CPU/IO/storage or fees.
    pub fn signed_data_transport_upper(&self) -> Result<TirSharedDataTransportUpperV1, String> {
        use crate::palw_court_shared_tir_stream_v1::{SharedStreamActionV1 as A, SharedStreamTargetV1 as T, SignedSharedStreamV1};
        if self.custodians == 0 {
            return Err("shared data transport lacks original custodian bound".into());
        }
        let request_id = self.request_id(0)?;
        let scope = self.scope_id()?;
        // These unsent serialization shapes measure fixed-width fields only.
        // Their source hash is a width exemplar, not an asserted body digest;
        // empty signature/body Vecs are never signed, decoded or ingested.
        let request = crate::palw_court_shared_tir_frontier_v1::SignedSharedRequestV1 {
            version: 1,
            scope,
            ordinal: 0,
            request_id,
            signer: self.producer,
            signature: Vec::new(),
        };
        let reply = crate::palw_court_shared_tir_response_v1::SignedSharedDataResponseV1 {
            version: 1,
            claim: self.claim,
            scope_id: scope,
            request_id,
            signer: self.producer,
            query_block: self.source.source_id_v1(),
            query_daa: 0,
            data_digest: self.source.source_id_v1(),
            signature: Vec::new(),
        };
        let opening_bytes = signed_wire_length(&request)?;
        let reply_bytes = signed_wire_length(&reply)?;
        let replies = mul(self.len(), self.custodians)?;
        let mut result = TirSharedDataTransportUpperV1 {
            queries: self.len(),
            maximum_custodians: self.custodians,
            fresh_responses: replies,
            signed_controls: add(self.len(), replies)?,
            signed_envelope_bytes: add(mul(self.len(), opening_bytes)?, mul(replies, reply_bytes)?)?,
            maximum_envelope_bytes: opening_bytes.max(reply_bytes),
            ..Default::default()
        };
        let mut charge = |count: u64, body: u64, target: T| -> Result<(), String> {
            let streams = mul(count, self.custodians)?;
            let sample = |action| SignedSharedStreamV1 {
                version: 1,
                claim: self.claim,
                scope,
                target: target.clone(),
                signer: self.producer,
                action,
                signature: Vec::new(),
            };
            let begin = signed_wire_length(&sample(A::Begin { total_bytes: body, body_hash: self.source.source_id_v1() }))?;
            let part = signed_wire_length(&sample(A::Part { stream_id: self.source.source_id_v1(), offset: 0, bytes: Vec::new() }))?;
            let finish = signed_wire_length(&sample(A::Finish { stream_id: self.source.source_id_v1() }))?;
            result.add_streams(streams, body, begin, part, finish)
        };
        self.visit_source_bounds(|count, q, bound| {
            if !matches!(q, Request::Ids { .. }) {
                charge(count, bound.maximum_response_bytes, T::Unit { ordinal: 0, request_id })?;
            }
            Ok(())
        })?;
        // IDs use one FullIds stream per literal signer, never one full stream
        // for every chunk-query. Fresh responses still cover each query.
        if self.geometry.families[6] != 0 {
            let (body, _) = crate::palw_tir_public_owned_v1::request_body_bound_v1(self.source, Request::Ids { offset: 0 })?;
            charge(1, body, T::FullIds)?;
        }
        Ok(result)
    }
    pub(crate) fn has_full_ids_stream_v3(&self) -> bool {
        self.geometry.families[6] != 0
    }
    pub fn len(&self) -> u64 {
        self.geometry.total
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn request_at(&self, ordinal: u64) -> Result<Request, String> {
        let request = self.geometry.at(ordinal)?;
        self.source.check_request_v1(&request)?;
        Ok(request)
    }
    /// Checked inverse for the actual StepLeaf family. No overlapping StepRun
    /// ordinal reconstruction or heap/index allocation is involved.
    pub fn step_leaf_ordinal_v1(&self, index: u64) -> Result<u64, String> {
        if index >= self.geometry.n {
            return Err("shared StepLeaf index outside native source".into());
        }
        let ordinal = self.geometry.families[0].checked_add(index).ok_or("shared StepLeaf ordinal overflow")?;
        if self.request_at(ordinal)? != (Request::StepLeaf { index }) {
            return Err("shared StepLeaf ordinal roundtrip differs".into());
        }
        Ok(ordinal)
    }
    /// Shared request identity only, NOT a complete service-packet/duty ID:
    /// signer role, turn, carrier fragment and original eligibility remain to bind.
    pub fn request_id(&self, ordinal: u64) -> Result<Hash64, String> {
        let bytes = borsh::to_vec(&(
            1u16,
            self.claim,
            self.allocation,
            self.native_rules,
            self.shared_profile,
            self.source.source_id_v1(),
            self.request_at(ordinal)?,
        ))
        .map_err(|e| e.to_string())?;
        Ok(Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka-palw/shared-tir-request/v1")
                .hash(&bytes)
                .as_bytes()
                .try_into()
                .expect("64"),
        ))
    }
}

/// Borsh counts actual codec fields without allocating a signature/body buffer.
fn signed_wire_length<T: borsh::BorshSerialize>(shape: &T) -> Result<u64, String> {
    add(borsh::object_length(shape).map_err(|e| e.to_string())? as u64, crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64)
}
/// Numerical disclosure transport only. No root/terminal/transaction wrapper,
/// mass, execution/retention work, fee, reservation or admission capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirSharedNumericalTransportUpperV1 {
    pub scope_id: Hash64,
    pub numerical: crate::palw_tir_public_source_v1::numerical_inventory_v1::TirPublicNumericalInventoryV1,
    pub signed_bytes_upper: u64,
    pub maximum_signed_bytes_upper: u64,
}
/// Canonical roots/all disclosures/all terminal evidence, each once. Native
/// payload ceilings are conservative upper bounds, not expected usage or a
/// minimal capital requirement. Fragmentation/transaction/mass remain unclosed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirSharedNumericalEvidenceUpperV1 {
    pub disclosures: TirSharedNumericalTransportUpperV1,
    pub roots_signed_bytes_upper: u64,
    pub bottoms_signed_bytes_upper: u64,
    pub maximum_root_signed_bytes_upper: u64,
    pub maximum_bottom_signed_bytes_upper: u64,
    pub signed_controls: u64,
    pub signed_bytes_upper: u64,
}
/// Derived envelope/control components for one original's distinct DATA states.
/// No transaction wrapper, payment, admission, duty or complete-cost capability.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TirSharedDataTransportUpperV1 {
    pub queries: u64,
    pub maximum_custodians: u64,
    pub streams: u64,
    pub parts: u64,
    pub fresh_responses: u64,
    pub signed_controls: u64,
    pub signed_envelope_bytes: u64,
    pub streamed_body_bytes: u64,
    pub maximum_envelope_bytes: u64,
    pub longest_stream_controls: u64,
}
impl TirSharedDataTransportUpperV1 {
    fn add_streams(&mut self, streams: u64, body: u64, begin: u64, part: u64, finish: u64) -> Result<(), String> {
        if streams == 0 {
            return Ok(());
        }
        if body == 0 {
            return Err("shared transport stream is empty".into());
        }
        let limit = crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64;
        let parts = body.div_ceil(limit);
        let controls = add(parts, 2)?;
        let bytes = add(add(begin, finish)?, add(body, mul(parts, part)?)?)?;
        let next = Self {
            streams: add(self.streams, streams)?,
            parts: add(self.parts, mul(streams, parts)?)?,
            signed_controls: add(self.signed_controls, mul(streams, controls)?)?,
            signed_envelope_bytes: add(self.signed_envelope_bytes, mul(streams, bytes)?)?,
            streamed_body_bytes: add(self.streamed_body_bytes, mul(streams, body)?)?,
            maximum_envelope_bytes: self.maximum_envelope_bytes.max(begin).max(finish).max(add(part, body.min(limit))?),
            longest_stream_controls: self.longest_stream_controls.max(controls),
            ..*self
        };
        *self = next;
        Ok(())
    }
}

/// Sum of the existing native source-unit bounds. Values are derived upper
/// components, never measured CPU or an adopted complete reservation profile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TirSharedSourceDataUpperV1 {
    pub queries: u64,
    pub response_bytes: u64,
    pub parser_steps: u64,
    pub hash_input_bytes: u64,
    pub resident_ram_bytes_sum: u64,
}
impl TirSharedSourceDataUpperV1 {
    fn add(&mut self, count: u64, b: crate::palw_tir_public_source_v1::PalwTirPublicUnitBoundV1) -> Result<(), String> {
        let next = Self {
            queries: add(self.queries, count)?,
            response_bytes: add(self.response_bytes, mul(count, b.maximum_response_bytes)?)?,
            parser_steps: add(self.parser_steps, mul(count, b.maximum_parser_steps)?)?,
            hash_input_bytes: add(self.hash_input_bytes, mul(count, b.maximum_hash_input_bytes)?)?,
            resident_ram_bytes_sum: add(self.resident_ram_bytes_sum, mul(count, b.resident_ram_bytes)?)?,
        };
        *self = next;
        Ok(())
    }
}

/// Logical requests on the EXISTING native owned route, not source-level ID
/// chunks. No owner slot, eligibility, funded service or future turn is inferred.
pub struct TirNativeDemandSetV1<'a> {
    queries: TirSharedRequestSetV1<'a>,
    geometry: Geometry,
}
impl<'a> TirNativeDemandSetV1<'a> {
    pub fn new(frame: &PalwTirPublicClaimV1, source: &'a Source, shared_profile: Hash64) -> Result<Self, String> {
        let queries = TirSharedRequestSetV1::new(frame, source, shared_profile)?;
        let geometry = queries.geometry.native_demands()?;
        Ok(Self { queries, geometry })
    }
    pub fn len(&self) -> u64 {
        self.geometry.total
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn request_at(&self, ordinal: u64) -> Result<Request, String> {
        let request = self.geometry.at(ordinal)?;
        crate::palw_tir_public_owned_v1::request_body_bound_v1(self.queries.source, request)?;
        Ok(request)
    }
    pub fn response_at(&self, ordinal: u64) -> Result<TirNativeResponseGraphV1, String> {
        self.response_for(self.request_at(ordinal)?)
    }
    /// Exact existing native bound, including ALL source ID chunks. This is
    /// only the response subgraph: preparation, warm work, demand, reacquire,
    /// collect, witness and physical/carrier costs are separate obligations.
    pub fn response_for(&self, request: Request) -> Result<TirNativeResponseGraphV1, String> {
        let (maximum_body_bytes, maximum_resident_ram_bytes) =
            crate::palw_tir_public_owned_v1::request_body_bound_v1(self.queries.source, request)?;
        let q = &self.queries;
        let bytes = borsh::to_vec(&(1u16, q.claim, q.allocation, q.native_rules, q.shared_profile, q.source.source_id_v1(), request))
            .map_err(|e| e.to_string())?;
        let logical_id = Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka-palw/shared-tir-native-demand/v1")
                .hash(&bytes)
                .as_bytes()
                .try_into()
                .expect("64"),
        );
        Ok(TirNativeResponseGraphV1 { logical_id, request, maximum_body_bytes, maximum_resident_ram_bytes })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirNativeResponseGraphV1 {
    pub logical_id: Hash64,
    pub request: Request,
    pub maximum_body_bytes: u64,
    pub maximum_resident_ram_bytes: u64,
}
/// A declared body-length branch of the response subgraph. Not an assertion
/// that bytes authenticate, its signer is liable, or it has service capacity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirNativeResponseBranchV1 {
    total_bytes: u64,
    parts: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirNativeResponseStageV1 {
    Begin { total_bytes: u64 },
    Part { offset: u64, bytes: u64 },
    Finish,
}
impl TirNativeResponseGraphV1 {
    pub fn branch(&self, total_bytes: u64) -> Result<TirNativeResponseBranchV1, String> {
        if total_bytes == 0 || total_bytes > self.maximum_body_bytes {
            return Err("native response branch exceeds its actual request body bound".into());
        }
        let parts = total_bytes.div_ceil(crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64);
        parts.checked_add(2).ok_or("native response stage count overflow")?;
        Ok(TirNativeResponseBranchV1 { total_bytes, parts })
    }
    pub fn maximum_stages(&self) -> Result<u64, String> {
        Ok(self.branch(self.maximum_body_bytes)?.len())
    }
}
impl TirNativeResponseBranchV1 {
    pub fn len(&self) -> u64 {
        self.parts + 2
    }
    pub fn is_empty(&self) -> bool {
        false
    }
    /// Exactly one accepted predecessor before each Part/Finish. This serial
    /// dependence is not removed by parallel signers or a third-party relay.
    pub fn predecessor(&self, ordinal: u64) -> Result<Option<u64>, String> {
        self.stage_at(ordinal)?;
        Ok(ordinal.checked_sub(1))
    }
    pub fn stage_at(&self, ordinal: u64) -> Result<TirNativeResponseStageV1, String> {
        if ordinal >= self.len() {
            return Err("native response stage ordinal is outside its branch".into());
        }
        if ordinal == 0 {
            return Ok(TirNativeResponseStageV1::Begin { total_bytes: self.total_bytes });
        }
        if ordinal == self.len() - 1 {
            return Ok(TirNativeResponseStageV1::Finish);
        }
        let part = crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64;
        let offset = (ordinal - 1).checked_mul(part).ok_or("native response part offset overflow")?;
        Ok(TirNativeResponseStageV1::Part { offset, bytes: part.min(self.total_bytes - offset) })
    }
    /// Construct the existing native action from actual bytes; native signature,
    /// accepting clock, work permits, chunk lookup and Finish authentication are
    /// still mandatory. Never changes the protocol's part size or action tags.
    pub fn action_at(
        &self,
        ordinal: u64,
        demand_id: Hash64,
        answerer: crate::palw_state_v2::PalwBondKeyV2,
        body: &[u8],
    ) -> Result<crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1, String> {
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as A;
        if u64::try_from(body.len()).map_err(|_| "native body length overflow")? != self.total_bytes {
            return Err("native response body differs from its declared branch".into());
        }
        Ok(match self.stage_at(ordinal)? {
            TirNativeResponseStageV1::Begin { total_bytes } => A::Begin {
                demand_id,
                answerer,
                total_bytes,
                body_hash: crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(body),
            },
            TirNativeResponseStageV1::Part { offset, bytes } => {
                let lo = usize::try_from(offset).map_err(|_| "native part index overflow")?;
                let len = usize::try_from(bytes).map_err(|_| "native part length overflow")?;
                A::Part { demand_id, answerer, offset, bytes: body[lo..lo + len].to_vec() }
            }
            TirNativeResponseStageV1::Finish => A::Finish { demand_id, answerer },
        })
    }
}

/// Observed native response duty, not service/default authorization. In
/// particular its owner-scoped identity is NOT a shared-frontier packet ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirNativeResponseDutyV1 {
    pub id: Hash64,
    pub logical_id: Hash64,
    pub key: crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1,
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub answerer: crate::palw_state_v2::PalwBondKeyV2,
    pub demand_id: Hash64,
    pub turn: u64,
    pub opened_daa: u64,
    pub respond_until_daa: u64,
    pub original_hard_daa: u64,
    pub response_fits_original_hard: bool,
    pub clock: TirNativeDutyClockV1,
    pub stage: TirNativeDutyStageV1,
    pub remaining_response_stages_upper: u64,
    pub remaining_native_moves: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirNativeDutyClockV1 {
    BeforeResponse,
    ResponseWindow,
    ResponseExpired,
    CollectionClosed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirNativeDutyStageV1 {
    Begin { maximum_body_bytes: u64 },
    Part { body_hash: Hash64, total_bytes: u64, offset: u64, bytes: u64, accepted_head: Hash64 },
    Finish { body_hash: Hash64, total_bytes: u64, accepted_head: Hash64 },
}
/// Reads ORIGINAL frame, accepted owner row and current Ready set. No caller
/// supplies a source, turn, body cursor, custody set or native leaf ceiling.
/// `ctx` is the native caller's accepting context, not a wall-clock timestamp.
/// Expired/unfunded-in-moves duties remain visible: absence of a usable route
/// is not silently interpreted as personal default. None means this row has no
/// uncollected unfinished response for this actor, not that a claim can Final.
pub fn tir_native_response_duty_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    ctx: &crate::palw_state_v2::PalwBlockContextV2,
    key: crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1,
    answerer: crate::palw_state_v2::PalwBondKeyV2,
    shared_profile: Hash64,
) -> Result<Option<TirNativeResponseDutyV1>, String> {
    use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
    let Key::TirAcquisition(claim, ..) = key else {
        return Err("response duty needs an original native acquisition key".into());
    };
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
        .ok_or("response duty has no retained ORIGINAL frame")?;
    let Some(Row::TirAcquisition(row)) = state.staged_court_row_v1(&key) else {
        return Err("response duty has no accepted native acquisition".into());
    };
    row.check_structure_v1(&key)?;
    let source = crate::palw_tir_public_state_v1::palw_tir_public_source_for_row_v1(state, row, frame.descriptor_v1().maximum_leaves)?;
    if let Some(reference) = row.owned.preparation_v1() {
        let Some(Row::TirPreparation(prep)) =
            state.staged_court_row_v1(&Key::TirPreparation(reference.claim, reference.allocation_id))
        else {
            return Err("response duty lost its original Ready state".into());
        };
        crate::palw_tir_public_state_v1::palw_tir_public_source_for_preparation_row_v1(
            state,
            prep,
            frame.descriptor_v1().maximum_leaves,
        )?;
        row.owned.check_preparation_v1(&prep.preparation)?;
    } else {
        return Err("response duty is not bound to original claim-wide preparation".into());
    }
    if !row.owned.response_custodians_v1().contains(&answerer) {
        return Err("actor has no literal Ready response duty; an owner response is voluntary".into());
    }
    let Some(pending) = row.owned.active_v1() else {
        return Ok(None);
    };
    if pending.collected || pending.slots.get(&answerer).is_some_and(|slot| slot.finished.is_some()) {
        return Ok(None);
    }
    let plan = frame.plan_v1();
    let graph = TirNativeDemandSetV1::new(frame, &source, shared_profile)?.response_for(pending.request)?;
    let hard = plan
        .terms
        .accepted_daa
        .checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
        .ok_or("response duty original hard deadline overflow")?;
    let part_size = crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64;
    let (stage, remaining, stage_scope) = match pending.slots.get(&answerer) {
        None => (
            TirNativeDutyStageV1::Begin { maximum_body_bytes: graph.maximum_body_bytes },
            graph.maximum_stages()?,
            (0u8, graph.maximum_body_bytes, 0u64, 0u64, Hash64::default(), Hash64::default()),
        ),
        Some(slot) => {
            graph.branch(slot.total_bytes)?;
            if slot.accepted_offset < slot.total_bytes {
                let left = slot.total_bytes - slot.accepted_offset;
                let bytes = left.min(part_size);
                (
                    TirNativeDutyStageV1::Part {
                        body_hash: slot.body_hash,
                        total_bytes: slot.total_bytes,
                        offset: slot.accepted_offset,
                        bytes,
                        accepted_head: slot.head,
                    },
                    left.div_ceil(part_size).checked_add(1).ok_or("response duty stages overflow")?,
                    (1u8, slot.total_bytes, slot.accepted_offset, bytes, slot.body_hash, slot.head),
                )
            } else {
                (
                    TirNativeDutyStageV1::Finish {
                        body_hash: slot.body_hash,
                        total_bytes: slot.total_bytes,
                        accepted_head: slot.head,
                    },
                    1,
                    (2u8, slot.total_bytes, slot.accepted_offset, 0u64, slot.body_hash, slot.head),
                )
            }
        }
    };
    let clock = if ctx.daa_score < pending.opened_daa {
        TirNativeDutyClockV1::BeforeResponse
    } else if ctx.daa_score < pending.respond_until_daa {
        TirNativeDutyClockV1::ResponseWindow
    } else if ctx.daa_score < row.owned.contract_v1().terms.collect_until_daa {
        TirNativeDutyClockV1::ResponseExpired
    } else {
        TirNativeDutyClockV1::CollectionClosed
    };
    let moves = row
        .owned
        .contract_v1()
        .plan
        .envelope
        .maximum_moves
        .checked_sub(row.owned.spent_v1().moves)
        .ok_or("response duty native move budget is inconsistent")?;
    let scope = borsh::to_vec(&(
        1u16,
        graph.logical_id,
        key,
        answerer,
        pending.demand_id,
        pending.turn,
        pending.opened_daa,
        pending.respond_until_daa,
        hard,
        stage_scope,
    ))
    .map_err(|e| e.to_string())?;
    let id = Hash64::from_bytes(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(b"misaka-palw/native-tir-response-duty/v1")
            .hash(&scope)
            .as_bytes()
            .try_into()
            .expect("64"),
    );
    Ok(Some(TirNativeResponseDutyV1 {
        id,
        logical_id: graph.logical_id,
        key,
        claim,
        allocation_id: plan.allocation_id,
        source_id: plan.source_id,
        answerer,
        demand_id: pending.demand_id,
        turn: pending.turn,
        opened_daa: pending.opened_daa,
        respond_until_daa: pending.respond_until_daa,
        original_hard_daa: hard,
        response_fits_original_hard: pending.respond_until_daa <= hard,
        clock,
        stage,
        remaining_response_stages_upper: remaining,
        remaining_native_moves: moves,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn court_tir_shared_transport_counts_actual_signed_codecs_and_all_source_choices() {
        use crate::palw_court_shared_tir_stream_v1::{
            SharedStreamActionV1 as A, SharedStreamTargetV1 as T, SignedSharedStreamV1 as S,
        };
        let sig = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
        let part = crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64;
        for custodians in [1, 4] {
            let fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, custodians);
            let source = fx.frame.source_v1(&fx.record, &fx.layout, fx.rules.maximum_leaves).unwrap();
            let set = TirSharedRequestSetV1::new(&fx.frame, &source, Hash64::from_u64_word(1)).unwrap();
            let upper = set.signed_data_transport_upper().unwrap();
            let scope = set.scope_id().unwrap();
            let request_id = set.request_id(0).unwrap();
            let h = source.source_id_v1();
            let control = |target, action| S {
                version: 1,
                claim: fx.frame.plan_v1().original_claim,
                scope,
                target,
                signer: fx.frame.custody_v1().pledge_v1().producer,
                action,
                signature: vec![0; sig],
            };
            // Independent concrete Borsh oracle at exact and ragged boundaries.
            for target in [T::Unit { ordinal: 0, request_id }, T::FullIds] {
                for body in [1, part, part + 1, 2 * part + 17] {
                    let begin = control(target.clone(), A::Begin { total_bytes: body, body_hash: h });
                    let finish = control(target.clone(), A::Finish { stream_id: h });
                    let mut bytes = borsh::to_vec(&begin).unwrap().len() as u64 + borsh::to_vec(&finish).unwrap().len() as u64;
                    for offset in (0..body).step_by(part as usize) {
                        bytes += borsh::to_vec(&control(
                            target.clone(),
                            A::Part { stream_id: h, offset, bytes: vec![7; (body - offset).min(part) as usize] },
                        ))
                        .unwrap()
                        .len() as u64;
                    }
                    let mut total = TirSharedDataTransportUpperV1::default();
                    total
                        .add_streams(
                            1,
                            body,
                            borsh::object_length(&begin).unwrap() as u64,
                            borsh::object_length(&control(target.clone(), A::Part { stream_id: h, offset: 0, bytes: Vec::new() }))
                                .unwrap() as u64,
                            borsh::object_length(&finish).unwrap() as u64,
                        )
                        .unwrap();
                    assert_eq!(total.signed_envelope_bytes, bytes);
                    assert_eq!(total.signed_controls, body.div_ceil(part) + 2);
                }
            }
            let mut streams = 0;
            let mut bodies = 0;
            let mut parts = 0;
            let mut id_queries = 0;
            let mut max_part = 0;
            for i in 0..set.len() {
                let q = set.request_at(i).unwrap();
                if matches!(q, Request::Ids { .. }) {
                    id_queries += 1;
                    continue;
                }
                let b = source.unit_bound_v1(&q).unwrap().maximum_response_bytes;
                streams += custodians;
                bodies += b * custodians;
                parts += b.div_ceil(part) * custodians;
                max_part = max_part.max(b.min(part));
            }
            if id_queries != 0 {
                let b = crate::palw_tir_public_owned_v1::request_body_bound_v1(&source, Request::Ids { offset: 0 }).unwrap().0;
                streams += custodians;
                bodies += b * custodians;
                parts += b.div_ceil(part) * custodians;
                max_part = max_part.max(b.min(part));
            }
            assert_eq!(upper.queries, set.len());
            assert_eq!(upper.streams, streams);
            assert_eq!(upper.fresh_responses, set.len() * custodians);
            assert!(upper.streamed_body_bytes >= bodies && upper.parts >= parts);
            assert_eq!(upper.signed_controls, set.len() + upper.fresh_responses + 2 * streams + upper.parts);
            assert!(upper.signed_envelope_bytes > upper.streamed_body_bytes);
            assert!(upper.maximum_envelope_bytes >= sig as u64 + max_part);
        }
        let mut total = TirSharedDataTransportUpperV1::default();
        let before = total;
        assert!(total.add_streams(u64::MAX, 1, 1, 1, 1).is_err());
        assert_eq!(total, before);
        assert!(total.add_streams(1, u64::MAX, 1, 1, 1).is_err());
        assert_eq!(total, before);
    }
    #[test]
    fn court_tir_shared_source_data_upper_covers_every_native_unit() {
        let fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, 4);
        let source = fx.frame.source_v1(&fx.record, &fx.layout, fx.rules.maximum_leaves).unwrap();
        let set = TirSharedRequestSetV1::new(&fx.frame, &source, Hash64::from_u64_word(1)).unwrap();
        let bound = set.source_data_upper().unwrap();
        let mut actual = TirSharedSourceDataUpperV1::default();
        for i in 0..set.len() {
            actual.add(1, source.unit_bound_v1(&set.request_at(i).unwrap()).unwrap()).unwrap();
        }
        assert_eq!(actual.queries, bound.queries);
        assert_eq!(bound.queries, set.len());
        assert!(
            actual.response_bytes <= bound.response_bytes
                && actual.parser_steps <= bound.parser_steps
                && actual.hash_input_bytes <= bound.hash_input_bytes
                && actual.resident_ram_bytes_sum <= bound.resident_ram_bytes_sum
        );
        let other = TirSharedRequestSetV1::new(&fx.frame, &source, Hash64::from_u64_word(2)).unwrap();
        assert_ne!(set.scope_id().unwrap(), other.scope_id().unwrap());
        assert_eq!(bound, other.source_data_upper().unwrap(), "profile namespace cannot invent a cheaper source body");
    }
    #[test]
    fn court_tir_shared_node_first_frontier_dominates_ragged_last_nodes() {
        for n in [2, 3, 1023, 1024, 1025, 4097, 65_537] {
            for level in 1..=height(n) {
                let (_, first, end) = crate::palw_tir_court_v1::palw_tir_step_node_frontier_v1(n, level, 0).unwrap();
                for index in 0..width(n, level).unwrap() {
                    let (_, lo, hi) = crate::palw_tir_court_v1::palw_tir_step_node_frontier_v1(n, level, index).unwrap();
                    assert!(hi - lo <= end - first, "n={n}, level={level}, index={index}");
                }
            }
        }
    }
    #[test]
    fn court_tir_shared_source_cost_overflow_is_atomic() {
        use crate::palw_tir_public_source_v1::PalwTirPublicUnitBoundV1 as B;
        let unit = B { maximum_response_bytes: 1, maximum_parser_steps: 0, maximum_hash_input_bytes: 0, resident_ram_bytes: 0 };
        let mut total = TirSharedSourceDataUpperV1::default();
        total.add(u64::MAX, unit).unwrap();
        let old = total;
        assert!(total.add(1, unit).is_err());
        assert_eq!(old, total);
        let mut total = TirSharedSourceDataUpperV1::default();
        assert!(total.add(2, B { maximum_response_bytes: u64::MAX, ..unit }).is_err());
        assert_eq!(total, TirSharedSourceDataUpperV1::default());
    }
    #[test]
    fn court_tir_shared_requests_match_native_validator_and_bind_scope() {
        use std::collections::BTreeSet;
        let fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, 4);
        let source = fx.frame.source_v1(&fx.record, &fx.layout, fx.rules.maximum_leaves).unwrap();
        let profile = Hash64::from_u64_word(1);
        let set = TirSharedRequestSetV1::new(&fx.frame, &source, profile).unwrap();
        let encoded: BTreeSet<_> = (0..set.len()).map(|i| borsh::to_vec(&set.request_at(i).unwrap()).unwrap()).collect();
        assert_eq!(encoded.len() as u64, set.len());
        assert!(set.request_at(set.len()).is_err());
        let n = source.leaf_count_v1();
        let d = source.decode_count_v1();
        let tiles = u64::from(source.vocab_v1()).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
        let mut oracle = BTreeSet::new();
        let mut consider = |q| {
            if source.check_request_v1(&q).is_ok() {
                oracle.insert(borsh::to_vec(&q).unwrap());
            }
        };
        // Independent over-approximation, filtered by the real validator,
        // including invalid boundaries rather than copying the count formula.
        for level in 0..=height(n.max(u64::from(d)).max(tiles)) + 1 {
            for index in 0..=n.max(u64::from(d)).max(tiles) {
                consider(Request::StepNode { level, index });
                consider(Request::RowsNode { level, index });
                for row in 0..=d {
                    consider(Request::TileNode { row, level, index });
                }
            }
        }
        for index in 0..=n {
            consider(Request::StepLeaf { index });
            for count in 0..=PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1 + 1 {
                consider(Request::StepRun { first: index, count });
            }
        }
        for row in 0..=d {
            for tile in 0..=tiles as u32 {
                consider(Request::TraceTile { row, tile });
            }
        }
        for offset in 0..=d {
            consider(Request::Ids { offset });
        }
        assert_eq!(encoded, oracle);
        let other = TirSharedRequestSetV1::new(&fx.frame, &source, Hash64::from_u64_word(2)).unwrap();
        let ids: BTreeSet<_> = (0..set.len())
            .map(|i| {
                assert_ne!(set.request_id(i).unwrap(), other.request_id(i).unwrap());
                set.request_id(i).unwrap()
            })
            .collect();
        assert_eq!(ids.len() as u64, set.len());
        assert!(TirSharedRequestSetV1::new(&fx.frame, &source, Hash64::default()).is_err());
        let other_frame = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(Some((7, 1)), 4);
        let other_source =
            other_frame.frame.source_v1(&other_frame.record, &other_frame.layout, other_frame.rules.maximum_leaves).unwrap();
        assert!(
            TirSharedRequestSetV1::new(&fx.frame, &other_source, profile).is_err(),
            "another original source cannot share the claim"
        );
    }
    #[test]
    fn court_tir_shared_requests_check_large_geometry_and_run_family_boundaries() {
        assert!(Geometry::new(u64::MAX, 1, 1, true).is_err());
        for n in [1, 2, 3, 1023, 1024, 1025, 2_000_000] {
            for tiled in [false, true] {
                let g = Geometry::new(n, 1025, 4097, tiled).unwrap();
                assert!(g.total > n);
                assert!(g.at(0).is_ok());
                assert!(g.at(g.total - 1).is_ok());
                assert!(g.at(g.total).is_err());
                let r = n.min(u64::from(PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1));
                assert_eq!(u128::from(g.families[2]), u128::from(r) * (2 * u128::from(n) - u128::from(r) + 1) / 2);
                let last_run = g.families[..3].iter().sum::<u64>() - 1;
                assert_eq!(g.at(last_run).unwrap(), Request::StepRun { first: n - r, count: r as u32 });
                let mut first = 0;
                for (family, &count) in g.families.iter().enumerate() {
                    if count != 0 {
                        for ordinal in [first, first + count - 1] {
                            assert_eq!(borsh::to_vec(&g.at(ordinal).unwrap()).unwrap()[0], family as u8);
                        }
                    }
                    first += count;
                }
                assert_eq!(first, g.total);
                if !tiled {
                    assert_eq!(g.families[3..], [0; 4]);
                }
            }
        }
    }
    #[test]
    fn court_tir_native_demands_match_owned_validator_and_native_body_bounds() {
        use std::collections::BTreeSet;
        let fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, 4);
        let source = fx.frame.source_v1(&fx.record, &fx.layout, fx.rules.maximum_leaves).unwrap();
        let profile = Hash64::from_u64_word(21);
        let queries = TirSharedRequestSetV1::new(&fx.frame, &source, profile).unwrap();
        let native = TirNativeDemandSetV1::new(&fx.frame, &source, profile).unwrap();
        let oracle: BTreeSet<_> = (0..queries.len())
            .filter_map(|i| {
                let q = queries.request_at(i).unwrap();
                crate::palw_tir_public_owned_v1::request_body_bound_v1(&source, q)
                    .ok()
                    .map(|bound| (borsh::to_vec(&q).unwrap(), bound))
            })
            .collect();
        let mut actual = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for i in 0..native.len() {
            let graph = native.response_at(i).unwrap();
            actual.insert((borsh::to_vec(&graph.request).unwrap(), (graph.maximum_body_bytes, graph.maximum_resident_ram_bytes)));
            assert!(ids.insert(graph.logical_id));
            assert_eq!(graph.request, native.request_at(i).unwrap());
            assert_ne!(
                graph.logical_id,
                queries.request_id(i).unwrap(),
                "source query and logical native request have separate namespaces"
            );
            assert!(graph.branch(0).is_err());
            assert!(graph.branch(graph.maximum_body_bytes + 1).is_err());
            let branch = graph.branch(graph.maximum_body_bytes).unwrap();
            assert_eq!(branch.len(), graph.maximum_stages().unwrap());
        }
        assert_eq!(actual, oracle);
        assert_eq!(actual.len() as u64, native.len());
        assert!(native.request_at(native.len()).is_err());
        assert!(native.response_for(Request::Ids { offset: 1024 }).is_err());
        let other = TirNativeDemandSetV1::new(&fx.frame, &source, Hash64::from_u64_word(22)).unwrap();
        assert_ne!(native.response_at(0).unwrap().logical_id, other.response_at(0).unwrap().logical_id);
        // Arithmetic-only large ID geometry, not an executed/benchmarked model.
        for d in [1, 1024, 1025, 2048, 2049, 20_000] {
            let queries = Geometry::new(3, d, 4097, true).unwrap();
            let native = queries.native_demands().unwrap();
            assert_eq!(queries.families[6], d.div_ceil(1024));
            assert_eq!(native.families[6], 1);
            assert_eq!(queries.total - native.total, queries.families[6] - 1);
            assert_eq!(native.at(native.total - 1).unwrap(), Request::Ids { offset: 0 });
        }
    }
    #[test]
    fn court_tir_response_graph_preserves_every_canonical_fragment_and_predecessor() {
        // Synthetic branch arithmetic only; no response authentication or cost.
        let graph = TirNativeResponseGraphV1 {
            logical_id: Hash64::from_u64_word(1),
            request: Request::StepLeaf { index: 0 },
            maximum_body_bytes: 32_770,
            maximum_resident_ram_bytes: 0,
        };
        let part = crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64;
        for total in 1..=graph.maximum_body_bytes {
            let branch = graph.branch(total).unwrap();
            assert_eq!(branch.stage_at(0).unwrap(), TirNativeResponseStageV1::Begin { total_bytes: total });
            assert_eq!(branch.predecessor(0).unwrap(), None);
            let mut cursor = 0;
            for i in 1..branch.len() - 1 {
                let TirNativeResponseStageV1::Part { offset, bytes } = branch.stage_at(i).unwrap() else { panic!("part") };
                assert_eq!(offset, cursor);
                assert!(bytes > 0 && bytes <= part);
                assert_eq!(bytes, (total - offset).min(part));
                cursor += bytes;
                assert_eq!(branch.predecessor(i).unwrap(), Some(i - 1));
            }
            assert_eq!(cursor, total);
            assert_eq!(branch.stage_at(branch.len() - 1).unwrap(), TirNativeResponseStageV1::Finish);
            assert_eq!(branch.predecessor(branch.len() - 1).unwrap(), Some(branch.len() - 2));
            assert!(branch.stage_at(branch.len()).is_err());
            assert!(branch.predecessor(u64::MAX).is_err());
        }
        let mut huge = graph.clone();
        huge.maximum_body_bytes = u64::MAX;
        let branch = huge.branch(u64::MAX).unwrap();
        let TirNativeResponseStageV1::Part { offset, bytes } = branch.stage_at(branch.len() - 2).unwrap() else { panic!("last part") };
        assert_eq!(offset.checked_add(bytes), Some(u64::MAX));
    }

    #[test]
    fn court_tir_response_graph_actions_match_native_body_hash_and_chunk_bytes() {
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as A;
        let fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, 4);
        let signer = fx.frame.custody_v1().pledge_v1().producer;
        let body: Vec<_> = (0..32_770).map(|i| (i % 251) as u8).collect();
        // A multi-part synthetic body, deliberately not claimed as authenticated evidence.
        let graph = TirNativeResponseGraphV1 {
            logical_id: Hash64::from_u64_word(3),
            request: Request::StepLeaf { index: 0 },
            maximum_body_bytes: body.len() as u64,
            maximum_resident_ram_bytes: 0,
        };
        let branch = graph.branch(body.len() as u64).unwrap();
        let demand = Hash64::from_u64_word(4);
        assert!(matches!(branch.action_at(0,demand,signer,&body).unwrap(),A::Begin { demand_id,answerer,total_bytes,body_hash }
            if demand_id==demand && answerer==signer && total_bytes==body.len() as u64
            && body_hash==crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body)));
        let mut assembled = Vec::new();
        for (i, chunk) in body.chunks(crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1).enumerate() {
            let A::Part { demand_id, answerer, offset, bytes } = branch.action_at(i as u64 + 1, demand, signer, &body).unwrap() else {
                panic!("part")
            };
            assert_eq!((demand_id, answerer, offset), (demand, signer, assembled.len() as u64));
            assert_eq!(bytes, chunk);
            assembled.extend(bytes);
        }
        assert_eq!(assembled, body);
        assert!(matches!(branch.action_at(branch.len()-1,demand,signer,&body).unwrap(),A::Finish { demand_id,answerer }
            if demand_id==demand && answerer==signer));
        assert!(branch.action_at(0, demand, signer, &body[..body.len() - 1]).is_err());
        assert!(branch.action_at(branch.len(), demand, signer, &body).is_err());
    }
}
