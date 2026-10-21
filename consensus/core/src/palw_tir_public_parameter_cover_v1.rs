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
        self.parameter_cover_prepared_v1(prepared,budget)
    }

    /// Additional native allocation owner for the all-original parameter relation.
    /// Source (and any numerical geometry) keep their own live reservations.
    /// This authenticates original/program coordinates, not material availability,
    /// prompt dependencies, native CPU, fees, custody or complete admission.
    pub fn parameter_cover_native_v1(
        source:&crate::palw_native_resource_v1::NativeRetainedV1<Self>,
        registered:&crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout:&crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget:OriginalParameterCoverBudgetV1)
        ->Result<crate::palw_native_resource_v1::NativeRetainedV1<OriginalParameterCoverV1>,
            crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        Self::parameter_cover_native_reserved_v1(source,registered,layout,budget,
            crate::palw_native_resource_v1::reserve_native_requests_v1)
    }
    fn parameter_cover_native_reserved_v1<R>(
        source:&crate::palw_native_resource_v1::NativeRetainedV1<Self>,
        registered:&crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout:&crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget:OriginalParameterCoverBudgetV1,reserve:R)
        ->Result<crate::palw_native_resource_v1::NativeRetainedV1<OriginalParameterCoverV1>,
            crate::palw_native_resource_v1::NativeDecodeErrorV1<String>>
    where R:FnOnce(u64)->Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
        crate::palw_native_resource_v1::NativeCoverageV1> {
        use crate::palw_native_resource_v1::{NativeDecodeErrorV1 as E,NativeRetainedV1,
            standalone_tir_program_shape_v1};
        if source.header.step_leaf_count>budget.max_original_leaves {
            return Err(E::Decode("original parameter cover leaf budget exhausted".into()));
        }
        // Reserve before resolving/decoding/verifying. This is an additional
        // conservative construction peak, not a borrow of Source's ticket.
        let requests=Self::native_construction_requests_v1(registered,layout,&source.header)?;
        let mut lease=reserve(requests).map_err(E::Resource)?;
        let binding=source.header.resolve_registered_layout_v1(registered,layout,
            source.header.committed_execution_root,source.maximum_leaves).map_err(E::Decode)?;
        let verified=crate::palw_tir_step_v1::verify_tir_binding_v1(&binding,source.maximum_leaves)
            .map_err(|e|E::Decode(e.to_string()))?;
        if verified.class_id!=source.class_id||verified.context_hash!=source.context_hash {
            return Err(E::Decode("original parameter cover substitutes source geometry".into()));
        }
        let shape=standalone_tir_program_shape_v1(&registered.program).map_err(|e|E::Decode(e.to_string()))?;
        let additional=PreparedParameterCoverV1::native_all_roots_requests_v1(
            &verified.space.program,&shape,source.header.step_leaf_count,budget)?;
        if let Some(ticket)=lease.as_mut() {ticket.grow_memory_v1(additional).map_err(E::Resource)?;}
        let prepared=PreparedParameterCoverV1::new(Box::new(verified),binding.job_context).map_err(E::Decode)?;
        let cover=source.parameter_cover_prepared_v1(prepared,budget).map_err(E::Decode)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(cover,lease))
    }

    fn parameter_cover_prepared_v1(&self,prepared:PreparedParameterCoverV1,
        budget:OriginalParameterCoverBudgetV1)->Result<OriginalParameterCoverV1,String> {
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

#[cfg(test)]
mod native_owner_tests {
    use super::*;
    const B:OriginalParameterCoverBudgetV1=OriginalParameterCoverBudgetV1 {
        max_original_leaves:1_000_000,max_total_edges:10_000_000,
        max_total_coordinates:1_000_000,max_root_memberships:1_000_000,
    };
    #[test]
    fn native_original_parameter_cover_preserves_actual_all_root_relation() {
        let f=crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1();
        let original=f.frame.original_v1();
        let source=PalwTirPublicSourceV1::new_native_v1(&f.record,&f.layout,f.frame.header_v1(),
            original.execution_root,original.trace_root,f.rules.maximum_leaves).unwrap();
        let legacy=source.parameter_cover_v1(&f.record,&f.layout,B).unwrap();
        let native=PalwTirPublicSourceV1::parameter_cover_native_v1(&source,&f.record,&f.layout,B).unwrap();
        assert!(!legacy.roots.is_empty());
        assert!(!legacy.union.is_empty());
        assert_eq!(*native,legacy);
        assert_eq!(native.root_memberships,native.roots.iter().map(|r|r.parameters.len() as u64).sum::<u64>());
        let low=OriginalParameterCoverBudgetV1 {max_total_edges:0,..B};
        assert!(PalwTirPublicSourceV1::parameter_cover_native_v1(&source,&f.record,&f.layout,low).is_err());
        let binding=source.header.resolve_registered_layout_v1(&f.record,&f.layout,
            source.header.committed_execution_root,source.maximum_leaves).unwrap();
        let verified=crate::palw_tir_step_v1::verify_tir_binding_v1(&binding,source.maximum_leaves).unwrap();
        let shape=crate::palw_native_resource_v1::standalone_tir_program_shape_v1(&f.record.program).unwrap();
        let prelude=PalwTirPublicSourceV1::native_construction_requests_v1(&f.record,&f.layout,&source.header).unwrap();
        let additional=PreparedParameterCoverV1::native_all_roots_requests_v1(
            &verified.space.program,&shape,source.header.step_leaf_count,B).unwrap();
        let total=prelude.checked_add(additional).unwrap();
        for quota in [prelude-1,total-1,total] {
            let checker=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(
                crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1 {memory_bytes:quota,database_bytes:0});
            let result=PalwTirPublicSourceV1::parameter_cover_native_reserved_v1(&source,&f.record,&f.layout,B,
                |bytes|checker.reserve_requests_v1(bytes,0).map(Some));
            if quota==total {
                let owned=result.unwrap();
                assert_eq!(*owned,legacy);
                assert_eq!(checker.outstanding(),(total,0));
                drop(owned);
            } else {assert!(matches!(result,Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));}
            assert_eq!(checker.outstanding(),(0,0));
        }
        let unbounded=OriginalParameterCoverBudgetV1 {max_original_leaves:u64::MAX,
            max_total_edges:u64::MAX,max_total_coordinates:u64::MAX,max_root_memberships:u64::MAX};
        assert!(matches!(PreparedParameterCoverV1::native_all_roots_requests_v1(
            &verified.space.program,&shape,source.header.step_leaf_count,unbounded),
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));
    }
}
