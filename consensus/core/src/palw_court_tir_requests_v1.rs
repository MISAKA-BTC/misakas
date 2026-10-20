//! Proposed owner-independent request universe for RFC-0026 F03. Shape/scope
//! derivation only, not admission, service packets, paid capacity or activation.
//! Enumerates every request admitted by the current native source validator,
//! including overlapping runs and non-bottom interior levels. A later adopted
//! profile may share response data, but cannot silently omit legal choices.
use crate::{Hash64, palw_tir_public_claim_v1::PalwTirPublicClaimV1,
    palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Request, PalwTirPublicSourceV1 as Source,
        PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1, PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1}};
use crate::palw_tir_court_v1::{palw_tir_step_tree_height_v1 as height, palw_tir_step_tree_width_v1 as width};

fn add(a: u64, b: u64) -> Result<u64, String> { a.checked_add(b).ok_or_else(|| "shared request count overflow".into()) }
fn mul(a: u64, b: u64) -> Result<u64, String> { a.checked_mul(b).ok_or_else(|| "shared request count overflow".into()) }
fn nodes(n: u64) -> Result<u64, String> {
    (1..=height(n)).try_fold(0, |sum, level| add(sum, width(n, level).ok_or("shared tree geometry")?))
}
fn node_at(n: u64, mut ordinal: u64) -> Result<(u8, u64), String> {
    for level in 1..=height(n) {
        let count = width(n, level).ok_or("shared tree geometry")?;
        if ordinal < count { return Ok((level, ordinal)); }
        ordinal -= count;
    }
    Err("shared tree ordinal is outside geometry".into())
}
#[derive(Clone, Debug)]
struct Geometry { n: u64, d: u64, tiles: u64, families: [u64; 7], total: u64 }
impl Geometry {
    fn new(n: u64, d: u64, vocab: u64, tiled: bool) -> Result<Self, String> {
        if n == 0 || vocab == 0 { return Err("shared request geometry is empty".into()); }
        let runs = (1..=n.min(u64::from(PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1)))
            .try_fold(0, |sum, count| add(sum, n - (count - 1)))?;
        let tiles = vocab.div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
        let families = [nodes(n)?, n, runs, if tiled { nodes(d)? } else { 0 },
            if tiled { mul(d, nodes(tiles)?)? } else { 0 },
            if tiled { mul(d, tiles)? } else { 0 },
            if tiled { d.div_ceil(u64::from(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1)) } else { 0 }];
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
        if ordinal >= self.total { return Err("shared request ordinal is outside complete set".into()); }
        let mut family = 0;
        while ordinal >= self.families[family] { ordinal -= self.families[family]; family += 1; }
        Ok(match family {
            0 => { let (level, index) = node_at(self.n, ordinal)?; Request::StepNode { level, index } },
            1 => Request::StepLeaf { index: ordinal },
            2 => {
                let mut count = 1;
                while ordinal >= self.n - (count - 1) { ordinal -= self.n - (count - 1); count += 1; }
                Request::StepRun { first: ordinal, count: u32::try_from(count).map_err(|_| "shared run count overflow")? }
            },
            3 => { let (level, index) = node_at(self.d, ordinal)?; Request::RowsNode { level, index } },
            4 => { let per_row = nodes(self.tiles)?; let (level, index) = node_at(self.tiles, ordinal % per_row)?;
                Request::TileNode { row: u32::try_from(ordinal / per_row).map_err(|_| "shared row overflow")?, level, index } },
            5 => Request::TraceTile { row: u32::try_from(ordinal / self.tiles).map_err(|_| "shared row overflow")?,
                tile: u32::try_from(ordinal % self.tiles).map_err(|_| "shared tile overflow")? },
            6 => Request::Ids { offset: u32::try_from(mul(ordinal, u64::from(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1))?)
                .map_err(|_| "shared ID offset overflow")? },
            _ => unreachable!("ordinal was checked against exact total"),
        })
    }
}

/// The source must come from native original geometry; this checks its binding
/// to the supplied frame. The caller still authenticates accepted state/profile.
/// No owner, queue position, requester, relay, wall clock or participation input.
pub struct TirSharedRequestSetV1<'a> {
    source: &'a Source, geometry: Geometry, claim: Hash64, allocation: Hash64, native_rules: Hash64, shared_profile: Hash64,
}
impl<'a> TirSharedRequestSetV1<'a> {
    pub fn new(frame: &PalwTirPublicClaimV1, source: &'a Source, shared_profile: Hash64) -> Result<Self, String> {
        let p = frame.plan_v1();
        if shared_profile == Hash64::default() || p.rules_id == Hash64::default() || p.allocation_id == Hash64::default()
            || p.original_claim != crate::palw_freeprompt_v3::fp_claim_id_v3(frame.original_v1())
            || p.source_id != source.source_id_v1() || frame.header_v1() != source.header_v1() {
            return Err("shared request set substitutes original scope or lacks profile".into());
        }
        Ok(Self { source, geometry: Geometry::new(source.leaf_count_v1(), u64::from(source.decode_count_v1()),
            u64::from(source.vocab_v1()), source.supports_tiled_trace_v1())?, claim: p.original_claim,
            allocation: p.allocation_id, native_rules: p.rules_id, shared_profile })
    }
    pub fn len(&self) -> u64 { self.geometry.total }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
    pub fn request_at(&self, ordinal: u64) -> Result<Request, String> {
        let request = self.geometry.at(ordinal)?;
        self.source.check_request_v1(&request)?;
        Ok(request)
    }
    /// Shared request identity only, NOT a complete service-packet/duty ID:
    /// signer role, turn, carrier fragment and original eligibility remain to bind.
    pub fn request_id(&self, ordinal: u64) -> Result<Hash64, String> {
        let bytes = borsh::to_vec(&(1u16, self.claim, self.allocation, self.native_rules, self.shared_profile,
            self.source.source_id_v1(), self.request_at(ordinal)?)).map_err(|e| e.to_string())?;
        Ok(Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/shared-tir-request/v1")
            .hash(&bytes).as_bytes().try_into().expect("64")))
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
    pub fn len(&self) -> u64 { self.geometry.total }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
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
        let bytes = borsh::to_vec(&(1u16, q.claim, q.allocation, q.native_rules, q.shared_profile,
            q.source.source_id_v1(), request)).map_err(|e| e.to_string())?;
        let logical_id = Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64)
            .key(b"misaka-palw/shared-tir-native-demand/v1").hash(&bytes).as_bytes().try_into().expect("64"));
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
pub struct TirNativeResponseBranchV1 { total_bytes: u64, parts: u64 }
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
    pub fn maximum_stages(&self) -> Result<u64, String> { Ok(self.branch(self.maximum_body_bytes)?.len()) }
}
impl TirNativeResponseBranchV1 {
    pub fn len(&self) -> u64 { self.parts + 2 }
    pub fn is_empty(&self) -> bool { false }
    /// Exactly one accepted predecessor before each Part/Finish. This serial
    /// dependence is not removed by parallel signers or a third-party relay.
    pub fn predecessor(&self, ordinal: u64) -> Result<Option<u64>, String> {
        self.stage_at(ordinal)?;
        Ok(ordinal.checked_sub(1))
    }
    pub fn stage_at(&self, ordinal: u64) -> Result<TirNativeResponseStageV1, String> {
        if ordinal >= self.len() { return Err("native response stage ordinal is outside its branch".into()); }
        if ordinal == 0 { return Ok(TirNativeResponseStageV1::Begin { total_bytes: self.total_bytes }); }
        if ordinal == self.len() - 1 { return Ok(TirNativeResponseStageV1::Finish); }
        let part = crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64;
        let offset = (ordinal - 1).checked_mul(part).ok_or("native response part offset overflow")?;
        Ok(TirNativeResponseStageV1::Part { offset, bytes: part.min(self.total_bytes - offset) })
    }
    /// Construct the existing native action from actual bytes; native signature,
    /// accepting clock, work permits, chunk lookup and Finish authentication are
    /// still mandatory. Never changes the protocol's part size or action tags.
    pub fn action_at(&self, ordinal: u64, demand_id: Hash64,
        answerer: crate::palw_state_v2::PalwBondKeyV2, body: &[u8])
        -> Result<crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1, String> {
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as A;
        if u64::try_from(body.len()).map_err(|_| "native body length overflow")? != self.total_bytes {
            return Err("native response body differs from its declared branch".into());
        }
        Ok(match self.stage_at(ordinal)? {
            TirNativeResponseStageV1::Begin { total_bytes } => A::Begin { demand_id, answerer, total_bytes,
                body_hash: crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(body) },
            TirNativeResponseStageV1::Part { offset, bytes } => {
                let lo = usize::try_from(offset).map_err(|_| "native part index overflow")?;
                let len = usize::try_from(bytes).map_err(|_| "native part length overflow")?;
                A::Part { demand_id, answerer, offset, bytes: body[lo..lo+len].to_vec() }
            },
            TirNativeResponseStageV1::Finish => A::Finish { demand_id, answerer },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let n = source.leaf_count_v1(); let d = source.decode_count_v1();
        let tiles = u64::from(source.vocab_v1()).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
        let mut oracle = BTreeSet::new();
        let mut consider = |q| { if source.check_request_v1(&q).is_ok() { oracle.insert(borsh::to_vec(&q).unwrap()); } };
        // Independent over-approximation, filtered by the real validator,
        // including invalid boundaries rather than copying the count formula.
        for level in 0..=height(n.max(u64::from(d)).max(tiles)) + 1 {
            for index in 0..=n.max(u64::from(d)).max(tiles) {
                consider(Request::StepNode { level, index }); consider(Request::RowsNode { level, index });
                for row in 0..=d { consider(Request::TileNode { row, level, index }); }
            }
        }
        for index in 0..=n {
            consider(Request::StepLeaf { index });
            for count in 0..=PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1 + 1 { consider(Request::StepRun { first: index, count }); }
        }
        for row in 0..=d { for tile in 0..=tiles as u32 { consider(Request::TraceTile { row, tile }); } }
        for offset in 0..=d { consider(Request::Ids { offset }); }
        assert_eq!(encoded, oracle);
        let other = TirSharedRequestSetV1::new(&fx.frame, &source, Hash64::from_u64_word(2)).unwrap();
        let ids: BTreeSet<_> = (0..set.len()).map(|i| { assert_ne!(set.request_id(i).unwrap(), other.request_id(i).unwrap());
            set.request_id(i).unwrap() }).collect();
        assert_eq!(ids.len() as u64, set.len());
        assert!(TirSharedRequestSetV1::new(&fx.frame, &source, Hash64::default()).is_err());
        let other_frame = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(Some((7, 1)), 4);
        let other_source = other_frame.frame.source_v1(&other_frame.record, &other_frame.layout, other_frame.rules.maximum_leaves).unwrap();
        assert!(TirSharedRequestSetV1::new(&fx.frame, &other_source, profile).is_err(), "another original source cannot share the claim");
    }
    #[test]
    fn court_tir_shared_requests_check_large_geometry_and_run_family_boundaries() {
        assert!(Geometry::new(u64::MAX, 1, 1, true).is_err());
        for n in [1, 2, 3, 1023, 1024, 1025, 2_000_000] {
            for tiled in [false, true] {
                let g = Geometry::new(n, 1025, 4097, tiled).unwrap();
                assert!(g.total > n);
                assert!(g.at(0).is_ok()); assert!(g.at(g.total - 1).is_ok()); assert!(g.at(g.total).is_err());
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
                if !tiled { assert_eq!(g.families[3..], [0; 4]); }
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
        let oracle: BTreeSet<_> = (0..queries.len()).filter_map(|i| {
            let q = queries.request_at(i).unwrap();
            crate::palw_tir_public_owned_v1::request_body_bound_v1(&source, q).ok()
                .map(|bound| (borsh::to_vec(&q).unwrap(), bound))
        }).collect();
        let mut actual = BTreeSet::new(); let mut ids = BTreeSet::new();
        for i in 0..native.len() {
            let graph = native.response_at(i).unwrap();
            actual.insert((borsh::to_vec(&graph.request).unwrap(), (graph.maximum_body_bytes, graph.maximum_resident_ram_bytes)));
            assert!(ids.insert(graph.logical_id));
            assert_eq!(graph.request, native.request_at(i).unwrap());
            assert_ne!(graph.logical_id, queries.request_id(i).unwrap(), "source query and logical native request have separate namespaces");
            assert!(graph.branch(0).is_err());
            assert!(graph.branch(graph.maximum_body_bytes+1).is_err());
            let branch = graph.branch(graph.maximum_body_bytes).unwrap();
            assert_eq!(branch.len(), graph.maximum_stages().unwrap());
        }
        assert_eq!(actual,oracle); assert_eq!(actual.len() as u64,native.len());
        assert!(native.request_at(native.len()).is_err());
        assert!(native.response_for(Request::Ids { offset: 1024 }).is_err());
        let other = TirNativeDemandSetV1::new(&fx.frame,&source,Hash64::from_u64_word(22)).unwrap();
        assert_ne!(native.response_at(0).unwrap().logical_id,other.response_at(0).unwrap().logical_id);
        // Arithmetic-only large ID geometry, not an executed/benchmarked model.
        for d in [1,1024,1025,2048,2049,20_000] {
            let queries = Geometry::new(3,d,4097,true).unwrap();
            let native = queries.native_demands().unwrap();
            assert_eq!(queries.families[6],d.div_ceil(1024));
            assert_eq!(native.families[6],1);
            assert_eq!(queries.total-native.total,queries.families[6]-1);
            assert_eq!(native.at(native.total-1).unwrap(),Request::Ids { offset:0 });
        }
    }
    #[test]
    fn court_tir_response_graph_preserves_every_canonical_fragment_and_predecessor() {
        // Synthetic branch arithmetic only; no response authentication or cost.
        let graph = TirNativeResponseGraphV1 { logical_id:Hash64::from_u64_word(1),
            request:Request::StepLeaf { index:0 },maximum_body_bytes:32_770,maximum_resident_ram_bytes:0 };
        let part = crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64;
        for total in 1..=graph.maximum_body_bytes {
            let branch = graph.branch(total).unwrap();
            assert_eq!(branch.stage_at(0).unwrap(),TirNativeResponseStageV1::Begin { total_bytes:total });
            assert_eq!(branch.predecessor(0).unwrap(),None);
            let mut cursor=0;
            for i in 1..branch.len()-1 {
                let TirNativeResponseStageV1::Part { offset,bytes }=branch.stage_at(i).unwrap() else { panic!("part") };
                assert_eq!(offset,cursor); assert!(bytes>0 && bytes<=part);
                assert_eq!(bytes,(total-offset).min(part)); cursor+=bytes;
                assert_eq!(branch.predecessor(i).unwrap(),Some(i-1));
            }
            assert_eq!(cursor,total); assert_eq!(branch.stage_at(branch.len()-1).unwrap(),TirNativeResponseStageV1::Finish);
            assert_eq!(branch.predecessor(branch.len()-1).unwrap(),Some(branch.len()-2));
            assert!(branch.stage_at(branch.len()).is_err()); assert!(branch.predecessor(u64::MAX).is_err());
        }
        let mut huge=graph.clone();huge.maximum_body_bytes=u64::MAX;
        let branch=huge.branch(u64::MAX).unwrap();
        let TirNativeResponseStageV1::Part { offset,bytes }=branch.stage_at(branch.len()-2).unwrap() else { panic!("last part") };
        assert_eq!(offset.checked_add(bytes),Some(u64::MAX));
    }

    #[test]
    fn court_tir_response_graph_actions_match_native_body_hash_and_chunk_bytes() {
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as A;
        let fx=crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None,4);
        let signer=fx.frame.custody_v1().pledge_v1().producer;
        let body:Vec<_>=(0..32_770).map(|i| (i%251) as u8).collect();
        // A multi-part synthetic body, deliberately not claimed as authenticated evidence.
        let graph=TirNativeResponseGraphV1 { logical_id:Hash64::from_u64_word(3),request:Request::StepLeaf { index:0 },
            maximum_body_bytes:body.len() as u64,maximum_resident_ram_bytes:0 };
        let branch=graph.branch(body.len() as u64).unwrap();let demand=Hash64::from_u64_word(4);
        assert!(matches!(branch.action_at(0,demand,signer,&body).unwrap(),A::Begin { demand_id,answerer,total_bytes,body_hash }
            if demand_id==demand && answerer==signer && total_bytes==body.len() as u64
            && body_hash==crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body)));
        let mut assembled=Vec::new();
        for (i,chunk) in body.chunks(crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1).enumerate() {
            let A::Part { demand_id,answerer,offset,bytes }=branch.action_at(i as u64+1,demand,signer,&body).unwrap() else { panic!("part") };
            assert_eq!((demand_id,answerer,offset),(demand,signer,assembled.len() as u64));
            assert_eq!(bytes,chunk);assembled.extend(bytes);
        }
        assert_eq!(assembled,body);
        assert!(matches!(branch.action_at(branch.len()-1,demand,signer,&body).unwrap(),A::Finish { demand_id,answerer }
            if demand_id==demand && answerer==signer));
        assert!(branch.action_at(0,demand,signer,&body[..body.len()-1]).is_err());
        assert!(branch.action_at(branch.len(),demand,signer,&body).is_err());
    }

}
