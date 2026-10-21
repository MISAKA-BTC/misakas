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
    /// Checks only the actual borrowed material for the derived coordinate
    /// relation. The constructor already authenticates immutable prompt form,
    /// parameter openings, source-bound complete IDs and complete step units.
    /// No fallback, acquisition, paid supply or future retention is asserted.
    pub(crate) fn check_root_input_cover_v1(&self,
        cover:&crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalRootInputCoverV1)->Result<(),String> {
        use crate::palw_tir_court_v1::parameter_cover::RootInputBoundaryV1 as I;
        if cover.parameters_v1().source_id!=self.source.source_id_v1() {
            return Err("builder input cover from another original source".into());
        }
        for root in cover.roots_v1() {for input in &root.inputs {
            match *input {
                I::PromptPosition(pos)=>match self.prompt {
                    PromptMaterialV1::Flat(ids) if ids.get(pos as usize).is_some()=>{},
                    PromptMaterialV1::Merkle(tiles) if tiles.binary_search_by_key(
                        &(pos/crate::palw_prompt_ids_v1::PALW_PROMPT_IDS_TILE_LEN),|p|p.tile_index).is_ok()=>{},
                    _=>return Err("builder input cover lacks authenticated prompt coordinate".into()),
                },
                I::GeneratedId(index)=>{
                    if self.ids.is_none_or(|ids|ids.ids_v1().get(index as usize).is_none()) {
                        return Err("builder input cover lacks source-bound generated IDs".into());
                    }
                },
                I::CarryStepLeaf(index)=>{
                    if !matches!(self.unit(Q::StepLeaf{index}),Some(R::StepLeaf{..})) {
                        return Err("builder input cover lacks authenticated carry unit".into());
                    }
                },
            }
        }}
        Ok(())
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


/// Private ADR-0199 A terminal request. There is no filer-owned body/prefix:
/// the request identifies one retained canonical terminal. No wire decoder,
/// queue slot, fee exemption, duty completion or settlement follows from it.
#[derive(Clone,Debug,PartialEq,Eq,borsh::BorshSerialize)]
pub(crate) struct SignedSharedTerminalIntentV1 {
    pub version:u16,pub claim:Hash64,pub scope_id:Hash64,pub policy_id:Hash64,
    pub leaf:u64,pub root_id:Hash64,pub path:Vec<u8>,pub parent_id:Hash64,
    pub filer:crate::palw_state_v2::PalwBondKeyV2,pub signature:Vec<u8>,
}
pub(crate) const SHARED_TERMINAL_INTENT_CONTEXT_V1:&[u8]=b"misaka-palw/shared-terminal-intent/v1/mldsa87";
impl SignedSharedTerminalIntentV1 {
    pub(crate) fn signing_message(&self,network:Hash64)->Result<Hash64,String> {
        if self.version!=1 || self.path.len()>crate::palw_court_shared_tir_round_v1::SHARED_TIR_MAX_PATH_V1 {
            return Err("shared terminal intent version/path".into());
        }
        crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/shared-terminal-intent/v1",
            &(network,self.version,self.claim,self.scope_id,self.policy_id,self.leaf,self.root_id,
                &self.path,self.parent_id,self.filer))
    }
}
impl SharedTirBuilderStoreV1<'_> {
    /// Read-only private construction, never a native accepting entry point.
    /// The caller must admit signature/root/source/ancestor verification,
    /// scratch, material ownership, builder/output and actual carriage costs
    /// before calling; `limit` meters builder work only. It is not a paid permit.
    /// Resolve the actual retained root/ancestors, not caller-supplied phase,
    /// binding, rules, evidence bytes or a serialized authentication flag.
    pub(crate) fn build_terminal(&self,state:&PalwChainStateV2,
        params:&crate::palw_state_v2::PalwStateParamsV2,
        rules:&crate::palw_court_close_receipt_v1::NativeCourtRulesV1,
        intent:&SignedSharedTerminalIntentV1,
        limit:crate::palw_tir_court_v1::PalwTirBuilderWorkLimitsV1)
        ->Result<(crate::palw_tir_court_v1::PalwTirConeRefutationV1,
            crate::palw_tir_court_v1::PalwTirBuilderMeterV1),String> {
        use crate::palw_court_shared_tir_round_v1 as rounds;
        use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key,PalwStagedCourtRowV1 as Row};
        if intent.signature.len()!=crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
            return Err("shared terminal intent signature length".into());
        }
        let bond=state.bond(&intent.filer).ok_or("shared terminal filer missing")?;
        if !matches!(bond.status,crate::palw_state_v2::PalwBondStatusV2::Active)
            || bond.collateral<params.min_collateral_sompi() {
            return Err("shared terminal filer not active/above existing floor".into());
        }
        if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(&bond.pubkey,
            intent.signing_message(rules.network_domain)?.as_byte_slice(),&intent.signature,SHARED_TERMINAL_INTENT_CONTEXT_V1) {
            return Err("shared terminal intent signature invalid".into());
        }
        let c=crate::palw_court_shared_tir_state_v1::context(state,intent.claim)?;
        let r=rounds::root(state,intent.claim,intent.leaf)?;
        if intent.scope_id!=c.scope_id || self.source.source_id_v1()!=c.source_id
            || self.source.header_v1()!=&crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&r.control.root.finalize.binding)
            || intent.policy_id!=r.control.policy_id
            || intent.policy_id!=crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(rules)?
            || intent.root_id!=rounds::root_statement_id(r)?
            || rules.at.daa_score<r.rules.at.daa_score || rules.at.daa_score>=c.hard_daa {
            return Err("shared terminal intent changes original/root/policy/interval".into());
        }
        let prepared=rounds::prepare_shared_tir_round_v1(state,intent.claim,intent.leaf,&intent.path)?;
        if intent.parent_id!=prepared.parent_id || prepared.phase.terminal_range().is_none() {
            return Err("shared terminal intent missing terminal/parent".into());
        }
        for depth in 0..intent.path.len() {
            let key=Key::CourtSharedRound(rounds::round_id(intent.root_id,&intent.path[..depth])?);
            let Some(Row::CourtSharedRound(a))=state.staged_court_row_v1(&key)
                else{return Err("shared terminal ancestor missing".into());};
            if a.rules.at.daa_score>rules.at.daa_score {return Err("shared terminal precedes accepted ancestor".into());}
        }
        let claim=state.claim(&intent.claim).ok_or("shared terminal original claim missing")?;
        let binding=crate::palw_court_v2::tir_binding_filled_v1(state,claim,&r.control.root.finalize.binding).map_err(|e|e.to_string())?;
        crate::palw_court_v2::check_tir_binding_is_the_claims_v1(state,claim,&binding).map_err(|e|e.to_string())?;
        let prompt_form=if rules.prompt_ids_merkle {PromptForm::MerkleV1}else{PromptForm::Flat};
        let native=crate::palw_tir_court_v1::PalwTirCourtRulesV1 {max_step_leaf_count:rules.step_ladder,prompt_form,
            limits:crate::palw_court_v2::palw_tir_court_limits_v1(&rules.court)};
        let (mut bottom,meter)=crate::palw_tir_court_v1::build_tir_dissect_bottom_bounded_v1(
            &binding,&prepared.phase,self,&native,limit).map_err(|e|e.to_string())?;
        if bottom.output_opening.leaf_index!=intent.leaf
            || bottom.output_preimage!=r.control.root.finalize.output_preimage {
            return Err("shared terminal builder changed canonical output".into());
        }
        crate::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut bottom.binding);
        crate::palw_court_shared_tir_bottom_v1::check_shared_bottom_cost_v1(&bottom,&rules.court)?;
        Ok((bottom,meter))
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

#[path = "palw_court_shared_constraint_builder_v1.rs"]
pub(crate) mod constraint_terminal;

#[path = "palw_court_shared_tokenizer_builder_v1.rs"]
pub(crate) mod tokenizer_terminal;
