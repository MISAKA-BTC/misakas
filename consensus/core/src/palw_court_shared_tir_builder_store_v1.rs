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
use crate::palw_step_refute::{PalwDecodeTokenPinV1, PalwTiledDecodeTokensV1, PalwTiledDecodePinV1};
use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1 as PromptForm;

/// Inputs remain owned by the caller. Only complete authenticated units and
/// canonical parameter openings are consulted. Missing inputs return None.
/// Prompt form is resolved from the retained original work rules, never chosen by input.
pub(crate) struct SharedTirBuilderStoreV1<'a> {
    source: Source,
    units: &'a [Unit],
    params: &'a [PalwArtifactOpeningV1],
    ids: Option<&'a Ids>,
    prompt: PromptMaterialV1<'a>,
}
/// Borrowed input description, not an authentication or custody credential.
#[derive(Clone,Copy)]
pub(crate) enum PromptMaterialV1<'a> { Missing, Flat(&'a [u32]), Merkle(&'a [PalwPromptIdsOpeningV1]) }
fn check_prompt_material(ctx:&crate::palw_v2::PalwJobContextV2,form:PromptForm,input:PromptMaterialV1<'_>)->Result<(),String> {
    use crate::palw_prompt_ids_v1::{prompt_token_ids_match_v1,verify_prompt_ids_opening_v1,prompt_ids_tile_count_v1};
    match (form,input) {
        (_,PromptMaterialV1::Missing)=>Ok(()),
        (PromptForm::Flat,PromptMaterialV1::Flat(ids))=>{
            if ids.len()!=ctx.declared_prefill_tokens as usize || !prompt_token_ids_match_v1(form,ids,&ctx.prompt_token_ids_hash) {
                return Err("builder flat prompt does not match original count/commitment".into());
            } Ok(())
        },
        (PromptForm::MerkleV1,PromptMaterialV1::Merkle(tiles))=>{
            let count=prompt_ids_tile_count_v1(u64::from(ctx.declared_prefill_tokens)).ok_or("builder prompt tile count overflow")?;
            if tiles.len() as u64>count || tiles.windows(2).any(|w|w[0].tile_index>=w[1].tile_index) {
                return Err("builder prompt tiles exceed original or repeat/out of order".into());
            }
            for tile in tiles {verify_prompt_ids_opening_v1(&ctx.prompt_token_ids_hash,ctx.declared_prefill_tokens,tile).map_err(|e|e.to_string())?;}
            Ok(())
        },_=>Err("builder prompt material changes native original form".into()),
    }
}
impl<'a> SharedTirBuilderStoreV1<'a> {
    pub(crate) fn new(state: &PalwChainStateV2, claim: Hash64, scope: Hash64,
        units: &'a [Unit], params: &'a [PalwArtifactOpeningV1], ids: Option<&'a Ids>) -> Result<Self,String> {
        Self::new_with_prompt(state,claim,scope,units,params,ids,PromptMaterialV1::Missing)
    }
    pub(crate) fn new_with_prompt(state:&PalwChainStateV2,claim:Hash64,scope:Hash64,
        units:&'a [Unit],params:&'a [PalwArtifactOpeningV1],ids:Option<&'a Ids>,prompt:PromptMaterialV1<'a>)->Result<Self,String> {
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
        let frame=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,claim).ok_or("builder original frame absent")?;
        check_prompt_material(&source.header_v1().job_context,frame.plan_v1().owner_work.rules.prompt_form,prompt)?;
        Ok(Self {source,units,params,ids,prompt})
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
    fn prompt_token_ids(&self)->Option<Vec<u32>> {
        match self.prompt {PromptMaterialV1::Flat(ids)=>Some(ids.to_vec()),_=>None}
    }
    fn prompt_ids_opening(&self,tile:u32)->Option<PalwPromptIdsOpeningV1> {
        let PromptMaterialV1::Merkle(tiles)=self.prompt else {return None;};
        tiles.binary_search_by_key(&tile,|p|p.tile_index).ok().map(|i|tiles[i].clone())
    }
    fn row_pin(&self,row:u32,lane:u32)->Option<PalwTiledDecodePinV1> {
        if lane>=self.source.vocab_v1() {return None;}
        let ids=self.ids?;let committed=*ids.ids_v1().get(row as usize)?;
        let tile=|lane:u32|lane / crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u32;
        let (a,b)=match (self.unit(Q::TraceTile {row,tile:tile(committed)})?,self.unit(Q::TraceTile {row,tile:tile(lane)})?) {
            (R::TraceTile {row_root:a,row_opening:ao,lanes:al,opening:ap},
             R::TraceTile {row_root:b,row_opening:bo,lanes:bl,opening:bp}) if a==b && ao==bo =>( (a,ao,al,ap),(bl,bp) ),
            _=>return None,
        };
        Some(PalwTiledDecodePinV1 {position:row,generated_token_ids:ids.ids_v1().to_vec(),row_root:*a.0,
            row_opening:a.1.clone(),committed_tile_lanes:a.2.clone(),committed_opening:a.3.clone(),
            beat_tile_lanes:b.0.clone(),beat_opening:b.1.clone(),beat_lane:lane})
    }
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

#[cfg(test)]
mod prompt_material_tests {
    use super::*;
    #[test]
    fn flat_prompt_checks_count_commitment_and_native_form() {
        let x=crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let ctx=&x.binding.job_context;
        check_prompt_material(ctx,PromptForm::Flat,PromptMaterialV1::Flat(&x.prompt)).unwrap();
        assert!(check_prompt_material(ctx,PromptForm::MerkleV1,PromptMaterialV1::Flat(&x.prompt)).is_err());
        let mut bad=x.prompt.clone();bad[0]^=1;
        assert!(check_prompt_material(ctx,PromptForm::Flat,PromptMaterialV1::Flat(&bad)).is_err());
        assert!(check_prompt_material(ctx,PromptForm::Flat,PromptMaterialV1::Flat(&x.prompt[..x.prompt.len()-1])).is_err());
        check_prompt_material(ctx,PromptForm::Flat,PromptMaterialV1::Missing).unwrap();
    }
    #[test]
    fn merkle_prompt_checks_ragged_tiles_order_count_and_commitment() {
        use crate::palw_prompt_ids_v1::{PALW_PROMPT_IDS_TILE_LEN,prompt_ids_opening_v1,prompt_token_ids_commitment_v1};
        // Synthetic identity/format case, not a modified registered original or admission test.
        let x=crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let ids:Vec<u32>=(0..PALW_PROMPT_IDS_TILE_LEN+3).map(|i|i%8).collect();
        let mut ctx=x.binding.job_context.clone();ctx.declared_prefill_tokens=ids.len() as u32;
        ctx.prompt_token_ids_hash=prompt_token_ids_commitment_v1(PromptForm::MerkleV1,&ids).unwrap();
        let tiles=vec![prompt_ids_opening_v1(&ids,0).unwrap(),prompt_ids_opening_v1(&ids,PALW_PROMPT_IDS_TILE_LEN).unwrap()];
        check_prompt_material(&ctx,PromptForm::MerkleV1,PromptMaterialV1::Merkle(&tiles)).unwrap();
        check_prompt_material(&ctx,PromptForm::MerkleV1,PromptMaterialV1::Merkle(&tiles[1..])).unwrap();
        assert!(check_prompt_material(&ctx,PromptForm::Flat,PromptMaterialV1::Merkle(&tiles)).is_err());
        let duplicate=vec![tiles[0].clone(),tiles[0].clone()];
        assert!(check_prompt_material(&ctx,PromptForm::MerkleV1,PromptMaterialV1::Merkle(&duplicate)).is_err());
        let mut bad=tiles.clone();bad[1].tile_ids[0]^=1;
        assert!(check_prompt_material(&ctx,PromptForm::MerkleV1,PromptMaterialV1::Merkle(&bad)).is_err());
        ctx.declared_prefill_tokens-=1;
        assert!(check_prompt_material(&ctx,PromptForm::MerkleV1,PromptMaterialV1::Merkle(&tiles)).is_err());
    }
}
