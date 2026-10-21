//! ADR-0199 A read-only borrowed material view for the existing native builder.
//! No local/network fallback, persistence, paid ingress or availability certificate.
//! The caller must admit source reconstruction, scans, clones, multiproof/range
//! assembly and builder/output ownership. This is not a complete resource profile.
use crate::{Hash64, palw_state_v2::PalwChainStateV2};
use crate::palw_artifact::PalwArtifactOpeningV1;
use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
use crate::palw_tir_public_source_v1::{PalwTirPublicSourceV1 as Source,
    PalwTirPublicAuthenticatedUnitV1 as Unit, PalwTirPublicAuthenticatedIdsV1 as Ids,
    PalwTirPublicRequestV1 as Q, PalwTirPublicResponseV1 as R};
use crate::palw_step_leg::{PalwStepTileLeafV1, PalwStepOpeningV1};
use crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1;
use crate::palw_step_refute::{PalwDecodeTokenPinV1, PalwTiledDecodeTokensV1};

/// Inputs remain owned by the caller. Only complete authenticated units and
/// canonical parameter openings are consulted. Missing inputs return None.
/// Prompt material and row-pin assembly are not implemented and never fall back.
pub(crate) struct SharedTirBuilderStoreV1<'a> {
    source: Source,
    units: &'a [Unit],
    params: &'a [PalwArtifactOpeningV1],
    ids: Option<&'a Ids>,
}
impl<'a> SharedTirBuilderStoreV1<'a> {
    pub(crate) fn new(state: &PalwChainStateV2, claim: Hash64, scope: Hash64,
        units: &'a [Unit], params: &'a [PalwArtifactOpeningV1], ids: Option<&'a Ids>) -> Result<Self,String> {
        let c=crate::palw_court_shared_tir_state_v1::context(state,claim)?;
        if scope!=c.scope_id || units.len() as u64>c.queries {return Err("builder material scope/cardinality".into());}
        let source=c.source(state)?;
        for (i,u) in units.iter().enumerate() {
            if u.source_id_v1()!=source.source_id_v1()
                || units[..i].iter().any(|p|p.request_v1()==u.request_v1()) {
                return Err("builder material wrong source or duplicate query".into());
            }
        }
        if params.windows(2).any(|w|w[0].leaf_index>=w[1].leaf_index) {
            return Err("builder parameters must be unique ascending coordinates".into());
        }
        c.with_parameter_verifier(state,scope,|v| {
            for p in params {v.check(p).map_err(|e|e.to_string())?;} Ok(())
        })?;
        if ids.is_some_and(|i|i.source_id_v1()!=source.source_id_v1()) {
            return Err("builder IDs from another original source".into());
        }
        Ok(Self {source,units,params,ids})
    }
    fn unit(&self,q:Q)->Option<&R> {
        self.units.iter().find(|u|u.request_v1()==q).map(|u|u.response_v1())
    }
}
impl PalwTirEvidenceStoreV1 for SharedTirBuilderStoreV1<'_> {
    fn step_leaf(&self,index:u64)->Option<PalwStepTileLeafV1> {
        match self.unit(Q::StepLeaf {index})? {R::StepLeaf {preimage,..}=>Some(preimage.clone()),_=>None}
    }
    fn step_opening(&self,index:u64)->Option<PalwStepOpeningV1> {
        match self.unit(Q::StepLeaf {index})? {R::StepLeaf {opening,..}=>Some(opening.clone()),_=>None}
    }
    fn step_range_siblings(&self,first:u64,count:u64)->Option<Vec<Hash64>> {
        if count==0 {return None;}
        let last=first.checked_add(count.checked_sub(1)?)?;
        // Even a long run can be assembled from its two authenticated boundary
        // openings. No full leaf-hash tree or hidden execution store is read.
        let (a,b)=match (self.unit(Q::StepLeaf {index:first})?,self.unit(Q::StepLeaf {index:last})?) {
            (R::StepLeaf {opening:a,..},R::StepLeaf {opening:b,..})=>(a,b),_=>return None,
        };
        crate::palw_step_leg::step_range_siblings_from_edges_capped_v1(self.source.leaf_count_v1(),a,b,
            self.source.header_v1().step_merkle_root,self.source.leaf_count_v1()).ok()
    }
    fn param_opening(&self,leaf:u32)->Option<PalwArtifactOpeningV1> {
        self.params.binary_search_by_key(&leaf,|p|p.leaf_index).ok().map(|i|self.params[i].clone())
    }
    fn prompt_token_ids(&self)->Option<Vec<u32>> {None}
    fn prompt_ids_opening(&self,_tile:u32)->Option<PalwPromptIdsOpeningV1> {None}
    fn decode_pin(&self)->Option<PalwDecodeTokenPinV1> {
        if !self.source.supports_tiled_trace_v1() {return None;}
        let ids=self.ids?;
        Some(PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 {
            rows_root:ids.rows_root_v1(),generated_token_ids:ids.ids_v1().to_vec(),
        }))
    }
    fn step_node(&self,level:u8,index:u64)->Option<(Vec<Hash64>,Vec<Hash64>)> {
        match self.unit(Q::StepNode {level,index})? {R::StepNode {frontier,siblings}=>Some((frontier.clone(),siblings.clone())),_=>None}
    }
    fn row_node(&self,level:u8,index:u64)->Option<(Vec<Hash64>,Vec<Hash64>)> {
        match self.unit(Q::RowsNode {level,index})? {R::RowsNode {frontier,siblings}=>Some((frontier.clone(),siblings.clone())),_=>None}
    }
}
