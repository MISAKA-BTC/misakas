//! Original-bound numerical parameter footprint, not paid admission, custody,
//! delivery, verification credit or a promise that the footprint is affordable.
use super::PalwTirPublicSourceV1;
use crate::Hash64;
use crate::palw_tir_court_v1::parameter_cover::{ParameterCoverBudgetV1,PreparedParameterCoverV1,CoverErrorV1};
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
/// Source-bound, conservative numerical-root input coordinates. Private
/// constructor; this is data dependency geometry, not custody/complete admission.
#[derive(Debug,PartialEq,Eq)]
pub struct OriginalRootInputCoverV1 {
    parameters:OriginalParameterCoverV1,
    roots:Vec<NamedRootInputCoverV1>,
}
#[derive(Debug,PartialEq,Eq)]
pub struct NamedRootInputCoverV1 {
    pub leaf:u64,
    pub inputs:Vec<crate::palw_tir_court_v1::parameter_cover::RootInputBoundaryV1>,
}
impl OriginalRootInputCoverV1 {
    pub fn parameters_v1(&self)->&OriginalParameterCoverV1 {&self.parameters}
    pub fn roots_v1(&self)->&[NamedRootInputCoverV1] {&self.roots}
}
fn native_input_requests_v1(leaves:u64,edges:u64)->Result<u64,crate::palw_native_resource_v1::NativeCoverageV1> {
    use crate::palw_native_resource_v1::{TirNativeRequestsV1,NativeCoverageV1,btree_node_envelope_bytes_v1};
    use crate::palw_tir_court_v1::parameter_cover::RootInputBoundaryV1 as I;
    let overflow=||NativeCoverageV1::overflow::<OriginalRootInputCoverV1>("original input dependency requests");
    let mut q=TirNativeRequestsV1::new();
    q.vec_v1::<NamedRootInputCoverV1>(leaves)?;
    q.vec_v1::<I>(edges)?; // total retained per-root boundary lengths
    let mut minimum=TirNativeRequestsV1::new();minimum.vec_v1::<I>(1)?;
    q.add_v1(leaves.checked_mul(minimum.bytes_v1()).ok_or_else(overflow)?)?;
    q.add_v1(edges.checked_mul(btree_node_envelope_bytes_v1::<I,()>() as u64).ok_or_else(overflow)?)?;
    q.exact_v1::<OriginalRootInputCoverV1>(1)?;
    Ok(q.bytes_v1())
}
fn add(a:u64,b:u64)->Result<u64,CoverErrorV1>{a.checked_add(b).ok_or(CoverErrorV1::Local("original parameter cover counter overflow"))}
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
            return Err(CoverErrorV1::Local("original parameter cover leaf budget exhausted").native());
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
        let cover=source.parameter_cover_prepared_typed_v1(prepared,budget).map_err(CoverErrorV1::native)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(cover,lease))
    }

    /// Additional coordinate envelope for every numerical root: actual token
    /// positions and previous-occurrence committed carry units. Does not claim
    /// all source/semantic/termination branches or data availability.
    pub fn root_input_cover_native_v1(
        source:&crate::palw_native_resource_v1::NativeRetainedV1<Self>,
        registered:&crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout:&crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget:OriginalParameterCoverBudgetV1)
        ->Result<crate::palw_native_resource_v1::NativeRetainedV1<OriginalRootInputCoverV1>,
            crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        Self::root_input_cover_native_reserved_v1(source,registered,layout,budget,
            crate::palw_native_resource_v1::reserve_native_requests_v1)
    }
    fn root_input_cover_native_reserved_v1<R>(
        source:&crate::palw_native_resource_v1::NativeRetainedV1<Self>,
        registered:&crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout:&crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget:OriginalParameterCoverBudgetV1,reserve:R)
        ->Result<crate::palw_native_resource_v1::NativeRetainedV1<OriginalRootInputCoverV1>,
            crate::palw_native_resource_v1::NativeDecodeErrorV1<String>>
    where R:FnOnce(u64)->Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
        crate::palw_native_resource_v1::NativeCoverageV1> {
        use crate::palw_native_resource_v1::{NativeDecodeErrorV1 as E,NativeRetainedV1,
            standalone_tir_program_shape_v1};
        if source.header.step_leaf_count>budget.max_original_leaves {
            return Err(CoverErrorV1::Local("original parameter cover leaf budget exhausted").native());
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
        let additional=additional.checked_add(native_input_requests_v1(source.header.step_leaf_count,budget.max_total_edges)
            .map_err(E::Resource)?).ok_or_else(||E::Resource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<OriginalRootInputCoverV1>("input owner growth")))?;
        if let Some(ticket)=lease.as_mut() {ticket.grow_memory_v1(additional).map_err(E::Resource)?;}
        let prepared=PreparedParameterCoverV1::new(Box::new(verified),binding.job_context).map_err(E::Decode)?;
        let mut roots=Vec::new();
        let parameters=source.parameter_cover_prepared_tracked_v1(prepared,budget,Some(&mut roots)).map_err(CoverErrorV1::native)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(OriginalRootInputCoverV1{parameters,roots},lease))
    }

    fn parameter_cover_prepared_v1(&self,prepared:PreparedParameterCoverV1,
        budget:OriginalParameterCoverBudgetV1)->Result<OriginalParameterCoverV1,String> {
        self.parameter_cover_prepared_typed_v1(prepared,budget).map_err(CoverErrorV1::legacy)
    }
    fn parameter_cover_prepared_typed_v1(&self,prepared:PreparedParameterCoverV1,
        budget:OriginalParameterCoverBudgetV1)->Result<OriginalParameterCoverV1,CoverErrorV1> {
        self.parameter_cover_prepared_tracked_v1(prepared,budget,None)
    }
    fn parameter_cover_prepared_tracked_v1(&self,prepared:PreparedParameterCoverV1,
        budget:OriginalParameterCoverBudgetV1,mut inputs:Option<&mut Vec<NamedRootInputCoverV1>>)
        ->Result<OriginalParameterCoverV1,CoverErrorV1> {
        let mut out=OriginalParameterCoverV1{source_id:self.source_id,original_leaves:self.header.step_leaf_count,
            roots:Vec::new(),union:Vec::new(),total_edges:0,total_coordinates:0,root_memberships:0};
        let mut union=BTreeSet::new();
        for leaf in 0..out.original_leaves {
            let remaining_edges=budget.max_total_edges.checked_sub(out.total_edges).ok_or(CoverErrorV1::Local("original cover edge accounting"))?;
            let remaining_coordinates=budget.max_total_coordinates.checked_sub(out.total_coordinates).ok_or(CoverErrorV1::Local("original cover coordinate accounting"))?;
            let remaining_memberships=budget.max_root_memberships.checked_sub(out.root_memberships).ok_or(CoverErrorV1::Local("original cover membership accounting"))?;
            let walk_budget=ParameterCoverBudgetV1{max_edges:remaining_edges,max_coordinates:remaining_coordinates};
            let mut boundaries=BTreeSet::new(); // no allocations in the legacy None path
            let cover=if inputs.is_some() {
                prepared.named_with_inputs_v1(leaf,walk_budget,remaining_memberships,&mut boundaries)?
            } else {prepared.named_with_leaf_limit_typed(leaf,walk_budget,remaining_memberships)?};
            let Some(cover)=cover else {continue};
            if let Some(out)=inputs.as_mut() {
                out.push(NamedRootInputCoverV1{leaf,inputs:boundaries.into_iter().collect()});
            }
            let members=u64::try_from(cover.leaves.len()).map_err(|_|"original cover membership count overflow")?;
            if members>remaining_memberships {return Err(CoverErrorV1::Local("original parameter cover membership budget exhausted"));}
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
        assert!(matches!(PalwTirPublicSourceV1::parameter_cover_native_v1(&source,&f.record,&f.layout,low),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));
        for small in [OriginalParameterCoverBudgetV1{max_original_leaves:0,..B},
            OriginalParameterCoverBudgetV1{max_total_coordinates:0,..B},
            OriginalParameterCoverBudgetV1{max_root_memberships:0,..B}] {
            assert!(source.parameter_cover_v1(&f.record,&f.layout,small).is_err());
            assert!(matches!(PalwTirPublicSourceV1::parameter_cover_native_v1(&source,&f.record,&f.layout,small),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));
        }
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

#[cfg(test)]
mod root_input_tests {
    use super::*;
    use crate::palw_tir_court_v1::parameter_cover::RootInputBoundaryV1 as I;
    #[test]
    fn actual_root_input_coordinates_keep_parameter_relation_and_authenticate_carry_units() {
        let f=crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1();
        let original=f.frame.original_v1();
        let source=PalwTirPublicSourceV1::new_native_v1(&f.record,&f.layout,f.frame.header_v1(),
            original.execution_root,original.trace_root,f.rules.maximum_leaves).unwrap();
        let budget=OriginalParameterCoverBudgetV1{max_original_leaves:1_000_000,max_total_edges:10_000_000,
            max_total_coordinates:1_000_000,max_root_memberships:1_000_000};
        let old=source.parameter_cover_v1(&f.record,&f.layout,budget).unwrap();
        let input=PalwTirPublicSourceV1::root_input_cover_native_v1(&source,&f.record,&f.layout,budget).unwrap();
        assert_eq!(input.parameters,old);
        assert_eq!(input.roots.iter().map(|r|r.leaf).collect::<Vec<_>>(),old.roots.iter().map(|r|r.leaf).collect::<Vec<_>>());
        let mut carry=0;let mut tokens=0;
        use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
        for root in &input.roots {for boundary in &root.inputs {match *boundary {
            I::CarryStepLeaf(index)=>{
                carry+=1;assert!(index<root.leaf);
                let query=super::super::PalwTirPublicRequestV1::StepLeaf{index};
                let response=super::super::PalwTirPublicResponseV1::StepLeaf {
                    preimage:f.execution.step_leaf(index).unwrap(),opening:f.execution.step_opening(index).unwrap()};
                let unit=source.authenticate_v1(&query,&response,None).unwrap();
                assert_eq!(unit.source_id_v1(),source.source_id_v1());
                assert_eq!(unit.request_v1(),query);
            },
            I::PromptPosition(pos)=>{tokens+=1;assert!(pos<source.header.job_context.declared_prefill_tokens);},
            I::GeneratedId(index)=>{tokens+=1;assert!(index<source.header.job_context.exact_decode_tokens-1);},
        }}}
        assert!(carry>0,"fixture must exercise actual previous-occurrence carry material");
        // Token reads can be hidden behind authenticated committed carries;
        // the complete parameter cover must not invent prompt dependencies.
        let _=tokens;
        let binding=source.header.resolve_registered_layout_v1(&f.record,&f.layout,
            source.header.committed_execution_root,source.maximum_leaves).unwrap();
        let verified=crate::palw_tir_step_v1::verify_tir_binding_v1(&binding,source.maximum_leaves).unwrap();
        let shape=crate::palw_native_resource_v1::standalone_tir_program_shape_v1(&f.record.program).unwrap();
        let prelude=PalwTirPublicSourceV1::native_construction_requests_v1(&f.record,&f.layout,&source.header).unwrap();
        let additional=PreparedParameterCoverV1::native_all_roots_requests_v1(&verified.space.program,&shape,
            source.header.step_leaf_count,budget).unwrap()+native_input_requests_v1(source.header.step_leaf_count,budget.max_total_edges).unwrap();
        let total=prelude.checked_add(additional).unwrap();
        for quota in [prelude-1,total-1,total] {
            let checker=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(
                crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1{memory_bytes:quota,database_bytes:0});
            let result=PalwTirPublicSourceV1::root_input_cover_native_reserved_v1(&source,&f.record,&f.layout,budget,
                |bytes|checker.reserve_requests_v1(bytes,0).map(Some));
            if quota==total {let owned=result.unwrap();assert_eq!(*owned,*input);assert_eq!(checker.outstanding(),(total,0));drop(owned);}
            else {assert!(matches!(result,Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));}
            assert_eq!(checker.outstanding(),(0,0));
        }
        assert!(native_input_requests_v1(u64::MAX,u64::MAX).is_err());
    }
    #[test]
    fn actual_input0_numerical_roots_name_prompt_and_generated_coordinates() {
        use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1 as F;
        for form in [F::Flat,F::MerkleV1] {
            let f=crate::palw_tir_public_claim_v1::palw_tir_public_input_history_frame_fixture_v1(form);
            let original=f.frame.original_v1();
            assert_eq!(f.frame.plan_v1().owner_work.rules.prompt_form,form);
            let source=PalwTirPublicSourceV1::new_native_v1(&f.record,&f.layout,f.frame.header_v1(),
                original.execution_root,original.trace_root,f.rules.maximum_leaves).unwrap();
            let budget=OriginalParameterCoverBudgetV1{max_original_leaves:1_000_000,max_total_edges:10_000_000,
                max_total_coordinates:1_000_000,max_root_memberships:1_000_000};
            let cover=PalwTirPublicSourceV1::root_input_cover_native_v1(&source,&f.record,&f.layout,budget).unwrap();
            assert_eq!(cover.parameters,source.parameter_cover_v1(&f.record,&f.layout,budget).unwrap());
            assert!(cover.roots.iter().any(|r|r.inputs.iter().any(|i|matches!(i,I::PromptPosition(_)))));
            assert!(cover.roots.iter().any(|r|r.inputs.iter().any(|i|matches!(i,I::GeneratedId(_)))));
        }
    }
    #[test]
    fn token_source_helper_is_identical_to_native_court_split_at_boundaries() {
        use crate::palw_tir_step_v1::{PalwTirJobShapeV1,PalwTirTokenSourceV1 as T};
        let job=PalwTirJobShapeV1{prefill:3,positions:6};
        assert_eq!(job.token_source_v1(0),T::Prompt(0));
        assert_eq!(job.token_source_v1(2),T::Prompt(2));
        assert_eq!(job.token_source_v1(3),T::Generated(0));
        assert_eq!(job.token_source_v1(5),T::Generated(2));
    }
}
