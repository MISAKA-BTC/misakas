//! Literal family operands authenticated against the current admitted original.
//! Data openings establish identity only; the existing courts adjudicate rendering.
use crate::{Hash64, palw_state_v2::PalwChainStateV2};
use crate::palw_native_resource_v1::{NativeRetainedV1 as Held, NativeDecodeErrorV1 as E, NativeCoverageV1 as C,
    TirNativeRequestsV1, reserve_native_requests_v1};
use crate::palw_court_family_material_v1::{self as m,FamilyMaterialTargetV1 as Target};
use crate::palw_court_family_supplier_v1::FamilySupplierRoleV1 as Role;
use crate::palw_fp_constraint_job_v1::{self as t,PalwTokenTableV1 as Table};
use crate::palw_public_token_table_v1::PalwPublicTokenTableSourceV1 as Source;
use crate::palw_step_leg::{PalwStepMerkleTreeV1 as Tree,PalwStepOpeningV1 as Opening};
fn bad(s:impl Into<String>)->E<String>{E::Decode(s.into())}
fn capacity(s:&str)->E<String>{E::Resource(C::unsupported::<RootedSemanticTableV1>("rooted family semantic operand",s))}
/// A single authenticated tree is retained, so all-ID readers perform O(log vocabulary)
/// opening work per ID. The allocation ticket stays attached through shared node ownership.
pub struct RootedSemanticTableV1 { table:Table, tree:Tree }
impl RootedSemanticTableV1 {pub fn table_v1(&self)->&Table{&self.table}}
struct Cursor<'a>{b:&'a[u8],p:usize}
impl<'a> Cursor<'a>{
    fn take(&mut self,n:usize)->Result<&'a[u8],E<String>>{let end=self.p.checked_add(n).ok_or_else(||bad("table frame overflow"))?;let b=self.b.get(self.p..end).ok_or_else(||bad("truncated table frame"))?;self.p=end;Ok(b)}
    fn n(&mut self)->Result<u32,E<String>>{Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four")))}
}
fn walk<'a>(bytes:&'a[u8],mut entry:impl FnMut(Option<&'a[u8]>)->Result<(),E<String>>,mut eog:impl FnMut(u32)->Result<(),E<String>>)->Result<(u32,u32,Hash64),E<String>>{
    let mut c=Cursor{b:bytes,p:0};if c.take(8)?!=t::PALW_TOKEN_TABLE_FILE_MAGIC_V1{return Err(bad("table magic"));}
    let n=c.n()?;if n==0||u64::from(n)>=t::PALW_TOKEN_TABLE_MAX_LEAVES_V1{return Err(bad("table vocabulary bound"));}
    for _ in 0..n {let v=match c.take(1)?[0]{0=>None,1=>{let len=c.n()? as usize;Some(c.take(len)?)},_=>return Err(bad("table option tag"))};entry(v)?;}
    let ne=c.n()?;if ne>n{return Err(bad("table EOG cardinality"));}
    for _ in 0..ne {let id=c.n()?;if id>=n{return Err(bad("table EOG outside vocabulary"));}eog(id)?;}
    let tokenizer=Hash64::from_bytes(c.take(64)?.try_into().expect("hash"));if c.p!=bytes.len(){return Err(bad("table trailing bytes"));}
    Ok((n,ne,tokenizer))
}
fn decode(bytes:&[u8])->Result<Held<RootedSemanticTableV1>,E<String>>{
    let mut payload=0u64;let (n,ne,tokenizer)=walk(bytes,|v|{payload=payload.checked_add(v.map_or(0,|b|b.len() as u64)).ok_or_else(||capacity("table payload count"))?;Ok(())},|_|Ok(()))?;
    let leaves=u64::from(n)+1;let mut q=TirNativeRequestsV1::new();
    q.exact_v1::<RootedSemanticTableV1>(1).map_err(E::Resource)?;
    q.exact_v1::<Option<Vec<u8>>>(u64::from(n)).map_err(E::Resource)?;
    q.exact_v1::<u8>(payload).map_err(E::Resource)?;q.exact_v1::<u32>(u64::from(ne)*2).map_err(E::Resource)?;
    // Leaf hashes + every retained tree level + construction Vec-header growth.
    q.exact_v1::<Hash64>(leaves.checked_mul(4).ok_or_else(||capacity("tree request overflow"))?).map_err(E::Resource)?;
    q.exact_v1::<Vec<Hash64>>(64).map_err(E::Resource)?;
    let lease=reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
    let mut entries=Vec::new();entries.try_reserve_exact(n as usize).map_err(|_|capacity("table entries"))?;
    let mut ids=Vec::new();ids.try_reserve_exact(ne as usize).map_err(|_|capacity("table EOG"))?;
    walk(bytes,|v|{let value=if let Some(b)=v{let mut out=Vec::new();out.try_reserve_exact(b.len()).map_err(|_|capacity("table rendering"))?;out.extend_from_slice(b);Some(out)}else{None};entries.push(value);Ok(())},|id|{ids.push(id);Ok(())})?;
    ids.sort_unstable();if ids.windows(2).any(|w|w[0]==w[1]){return Err(bad("table EOG duplicate IDs"));}
    let table=Table{entries,eog_token_ids:ids,tokenizer_id:tokenizer};
    let mut hashes=Vec::new();hashes.try_reserve_exact(leaves as usize).map_err(|_|capacity("table leaves"))?;
    for (id,body) in table.entries.iter().enumerate(){hashes.push(t::table_leaf_hash_v1(id as u32,body.as_deref()));}
    hashes.push(t::table_eog_leaf_hash_v1(&table.eog_token_ids));
    let tree=Tree::build_capped_v1(&hashes,t::PALW_TOKEN_TABLE_MAX_LEAVES_V1).map_err(|e|bad(e.to_string()))?;
    Ok(Held::from_owner_reserved_v1(RootedSemanticTableV1{table,tree},lease))
}
pub fn palw_retain_rooted_family_table_v1(state:&PalwChainStateV2,claim:Hash64)->Result<Option<Held<RootedSemanticTableV1>>,E<String>>{
    if crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,claim).is_none() || state.claim_semantics_v1(&claim).is_none_or(|s|s.token_table_root.is_none()){return Ok(None)}
    // The full TFO2 table was already authenticated and supplied by its actual
    // accepted carrier. Read that retained identity directly; no imaginary
    // kind3 uploader, duplicate fee or second upload is introduced.
    if let Some(admission)=m::read_authenticated_family_table_inventory_v1(state,claim)?{
        let value=decode(&admission.bytes)?;
        let sem=state.claim_semantics_v1(&claim).ok_or_else(||bad("semantic snapshot missing"))?;
        if Some(value.root_v1())!=sem.token_table_root||value.tokenizer_id_v1()!=sem.tokenizer_id{return Err(bad("admission table changes immutable statement"))}
        return Ok(Some(value))
    }
    let inventory=m::derive_bound_current_family_inventory_classified_v3(state,claim,Role::SemanticTable)?;
    let Some(body)=m::read_authenticated_family_material_classified_v1(state,&inventory,Target::SemanticTable)? else{return Ok(None)};
    let value=decode(&body.bytes)?;let sem=state.claim_semantics_v1(&claim).ok_or_else(||bad("semantic snapshot missing"))?;
    if Some(value.root_v1())!=sem.token_table_root||value.tokenizer_id_v1()!=sem.tokenizer_id{return Err(bad("rooted table changes immutable statement"));}
    Ok(Some(value))
}
pub fn palw_retain_rooted_tokenizer_raw_v1(state:&PalwChainStateV2,claim:Hash64)->Result<Option<Held<Vec<u8>>>,E<String>>{
    if crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,claim).is_none() || state.claim_semantics_v1(&claim).is_none_or(|s|s.raw_tokenizer_bytes.is_none()){return Ok(None)}
    let inventory=m::derive_bound_current_family_inventory_classified_v3(state,claim,Role::TokenizerArtifact)?;
    let Some(body)=m::read_authenticated_family_material_classified_v1(state,&inventory,Target::TokenizerRaw)? else{return Ok(None)};
    let sem=state.claim_semantics_v1(&claim).ok_or_else(||bad("semantic snapshot missing"))?;
    let raw=m::family_tokenizer_raw_payload_v1(&body.bytes,sem.raw_tokenizer_bytes.ok_or_else(||bad("actual tokenizer EOF absent"))?).map_err(bad)?;
    let mut q=TirNativeRequestsV1::new();q.exact_v1::<Vec<u8>>(1).map_err(E::Resource)?;q.exact_v1::<u8>(raw.len() as u64).map_err(E::Resource)?;
    let lease=reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;let mut bytes=Vec::new();bytes.try_reserve_exact(raw.len()).map_err(|_|capacity("raw tokenizer operand"))?;bytes.extend_from_slice(raw);
    Ok(Some(Held::from_owner_reserved_v1(bytes,lease)))
}
impl Source for RootedSemanticTableV1 {
    fn root_v1(&self)->Hash64{self.tree.root()}
    fn tokenizer_id_v1(&self)->Hash64{self.table.tokenizer_id}
    fn vocab_v1(&self)->u32{self.table.vocab()}
    fn eog_count_v1(&self)->u32{self.table.eog_token_ids.len() as u32}
    fn eog_part_v1(&self,o:u32,m:u32)->Result<Vec<u32>,String>{if m>16_384{return Err("EOG part bound".into());}let rest=self.table.eog_token_ids.get(o as usize..).ok_or("EOG offset")?;Ok(rest[..rest.len().min(m as usize)].to_vec())}
    fn entry_length_v1(&self,id:u32)->Result<Option<u32>,String>{self.table.entry_length_v1(id)}
    fn read_part_v1(&self,id:u32,o:u32,m:u32)->Result<Vec<u8>,String>{self.table.read_part_v1(id,o,m)}
    fn opening_v1(&self,id:u32)->Result<Opening,String>{
        let leaf=if id==self.vocab_v1(){t::table_eog_leaf_hash_v1(&self.table.eog_token_ids)}else{t::table_leaf_hash_v1(id,self.table.entries.get(id as usize).ok_or("table ID")?.as_deref())};
        Ok(Opening{leaf_index:u64::from(id),leaf_hash:leaf,siblings:self.tree.path_v1(id as usize).map_err(|e|e.to_string())?})
    }
}
impl Source for Held<RootedSemanticTableV1>{
    fn root_v1(&self)->Hash64{(**self).root_v1()}fn tokenizer_id_v1(&self)->Hash64{(**self).tokenizer_id_v1()}fn vocab_v1(&self)->u32{(**self).vocab_v1()}fn eog_count_v1(&self)->u32{(**self).eog_count_v1()}
    fn eog_part_v1(&self,o:u32,m:u32)->Result<Vec<u32>,String>{(**self).eog_part_v1(o,m)}fn entry_length_v1(&self,id:u32)->Result<Option<u32>,String>{(**self).entry_length_v1(id)}fn read_part_v1(&self,id:u32,o:u32,m:u32)->Result<Vec<u8>,String>{(**self).read_part_v1(id,o,m)}fn opening_v1(&self,id:u32)->Result<Opening,String>{(**self).opening_v1(id)}
}
#[cfg(test)] mod tests{use super::*;
#[test]fn rooted_table_single_tree_preserves_original_openings(){let table=Table{entries:vec![None,Some(vec![]),Some(b"abc".to_vec())],eog_token_ids:vec![2],tokenizer_id:Hash64::from_u64_word(7)};let bytes=t::palw_token_table_file_encode_v1(&table);let held=decode(&bytes).unwrap();assert_eq!(held.root_v1(),table.root());for id in 0..=3{assert_eq!(held.opening_v1(id).unwrap(),table.opening_v1(id).unwrap());}let mut changed=bytes.clone();changed.push(0);assert!(decode(&changed).is_err());assert!(decode(&bytes[..bytes.len()-1]).is_err());assert_eq!(held.entry_length_v1(0).unwrap(),None);assert_eq!(held.entry_length_v1(1).unwrap(),Some(0));}
}

/// Consumer fixtures install the exact persisted row shape. Signed supplier
/// admission/Begin/Part/Finish are exercised separately by the full-fold tests.
#[cfg(test)]
pub(crate) fn install_material_for_consumer_test_v1(state:&mut PalwChainStateV2,claim:Hash64,role:Role,target:Target,bytes:Vec<u8>){
    let inventory=m::derive_current_family_inventory_classified_v1(state,claim,role).unwrap();
    let id=inventory.target_id_v1(target).unwrap();
    let row=m::FamilyMaterialBodyV1{contract:Hash64::from_u64_word(811),target,inventory:inventory.id_v1(),digest:crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&bytes),bytes,
        completed_by:*state.bonds_iter().next().expect("fixture bond").0,accepted_daa:state.last_point().map_or(0,|p|p.daa_score),material_until_daa:u64::MAX};
    state.set_kernel_aux_row_for_tests(m::FAMILY_MATERIAL_TABLE_V1,m::material_aux_key_v1(3,id,None),Some(borsh::to_vec(&row).unwrap()));
}
