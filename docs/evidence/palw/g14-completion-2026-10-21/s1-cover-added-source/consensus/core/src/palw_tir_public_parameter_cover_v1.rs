//! Original-bound numerical parameter footprint, not paid admission, custody,
//! delivery, verification credit or a promise that the footprint is affordable.
use super::PalwTirPublicSourceV1;
use crate::Hash64;
use crate::palw_tir_court_v1::parameter_cover::{ParameterCoverBudgetV1,PreparedParameterCoverV1};
use std::collections::BTreeSet;

/// Explicit caller-admitted exploration and output bounds. No defaults, adopted
/// network constants or model exemptions are introduced by this read-only API.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct OriginalParameterCoverBudgetV1 {
    pub max_original_leaves:u64,
    pub max_total_edges:u64,
    pub max_total_coordinates:u64,
    /// Counts repeated coordinates separately under each named root. A union
    /// limit alone cannot bound the retained root-to-parameter relation.
    pub max_root_memberships:u64,
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct NamedParameterCoverV1 {
    pub leaf:u64,
    pub parameters:Vec<u32>,
    pub explored_edges:u64,
    pub explored_coordinates:u64,
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct OriginalParameterCoverV1 {
    pub source_id:Hash64,
    pub original_leaves:u64,
    /// Ascending original leaf order, including parameter-free numerical roots.
    pub roots:Vec<NamedParameterCoverV1>,
    /// Ascending unique registered inventory coordinates across all roots.
    pub union:Vec<u32>,
    pub total_edges:u64,
    pub total_coordinates:u64,
    pub root_memberships:u64,
}
fn add(a:u64,b:u64)->Result<u64,String>{a.checked_add(b).ok_or("original parameter cover counter overflow".into())}
impl PalwTirPublicSourceV1 {
    /// Reconstruct the actual native original from its retained header and the
    /// registered class/layout. No producer root or model bytes are inputs.
    /// Decode/verification/index/range-analysis allocation occurs once, outside
    /// the exploration meter, and requires separate native resource preflight.
    /// Each leaf_at enumeration also has native CPU work beyond the Walk meter.
    /// The caller must charge that scan, scratch/retained output, authentication,
    /// acquisition, retention and financial obligations before admitting claims.
    pub fn parameter_cover_v1(&self,
        registered:&crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout:&crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget:OriginalParameterCoverBudgetV1)->Result<OriginalParameterCoverV1,String> {
        if self.header.step_leaf_count>budget.max_original_leaves {
            return Err("original parameter cover leaf budget exhausted".into());
        }
        let binding=self.header.resolve_registered_layout_v1(registered,layout,
            self.header.committed_execution_root,self.maximum_leaves)?;
        let verified=crate::palw_tir_step_v1::verify_tir_binding_v1(&binding,self.maximum_leaves).map_err(|e|e.to_string())?;
        if verified.class_id!=self.class_id||verified.context_hash!=self.context_hash {
            return Err("original parameter cover substitutes source geometry".into());
        }
        // Binding verification, inventory index and range analysis are shared
        // for all roots. Only the finite potential-coordinate walks repeat.
        let prepared=PreparedParameterCoverV1::new(Box::new(verified),binding.job_context)?;
        let mut out=OriginalParameterCoverV1{source_id:self.source_id,original_leaves:self.header.step_leaf_count,
            roots:Vec::new(),union:Vec::new(),total_edges:0,total_coordinates:0,root_memberships:0};
        let mut union=BTreeSet::new();
        for leaf in 0..out.original_leaves {
            let remaining_edges=budget.max_total_edges.checked_sub(out.total_edges).ok_or("original cover edge accounting")?;
            let remaining_coordinates=budget.max_total_coordinates.checked_sub(out.total_coordinates).ok_or("original cover coordinate accounting")?;
            let remaining_memberships=budget.max_root_memberships.checked_sub(out.root_memberships).ok_or("original cover membership accounting")?;
            let Some(cover)=prepared.named_with_leaf_limit(leaf,ParameterCoverBudgetV1{max_edges:remaining_edges,
                max_coordinates:remaining_coordinates},remaining_memberships)? else {continue};
            let members=u64::try_from(cover.leaves.len()).map_err(|_|"original cover membership count overflow")?;
            if members>remaining_memberships {return Err("original parameter cover membership budget exhausted".into());}
            out.total_edges=add(out.total_edges,cover.explored_edges)?;
            out.total_coordinates=add(out.total_coordinates,cover.explored_coordinates)?;
            out.root_memberships=add(out.root_memberships,members)?;
            union.extend(cover.leaves.iter().copied());
            out.roots.push(NamedParameterCoverV1{leaf,parameters:cover.leaves,
                explored_edges:cover.explored_edges,explored_coordinates:cover.explored_coordinates});
        }
        out.union=union.into_iter().collect();
        Ok(out)
    }
}
