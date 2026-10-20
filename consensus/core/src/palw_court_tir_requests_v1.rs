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
}
