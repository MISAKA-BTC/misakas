//! Potential dependency closure is compared against the existing native builder;
//! these fixtures do not establish original admission or an affordable profile.
#[path="palw_tir_fixture_common.rs"] mod fixture;
use fixture::*;
use kaspa_consensus_core::palw_tir_court_v1::{build_tir_root_claim_v1,PalwTirParamOpeningV1};
use kaspa_consensus_core::palw_tir_court_v1::parameter_cover::{derive_parameter_cover_v1,ParameterCoverBudgetV1};
use kaspa_consensus_core::palw_tir_dissect_v1::palw_tir_dissect_site_v1;
const BUDGET:ParameterCoverBudgetV1=ParameterCoverBudgetV1{max_edges:10_000_000,max_coordinates:1_000_000};
fn coordinates(p:&PalwTirParamOpeningV1)->Vec<u32> {match p {
    PalwTirParamOpeningV1::None=>vec![],
    PalwTirParamOpeningV1::Single(p)=>p.opened.iter().map(|(i,_)|*i).collect(),
    PalwTirParamOpeningV1::Composite(_)=>panic!("fixture uses a single original artifact"),
}}
#[test]
fn potential_cover_contains_every_sampled_native_finalize_parameter() {
    let mut cases=0;
    for f in fixtures() {
        let Some(intervals)=&f.intervals else {continue};
        let x=f.honest();let store=Store{f:&f,x:&x};
        for (i,l) in f.leaves.iter().enumerate().filter(|(_,l)|palw_tir_dissect_site_v1(&f.space,intervals,l).is_some()).take(4) {
            let root=build_tir_root_claim_v1(&x.binding,i as u64,&store,&RULES).unwrap();
            let cover=derive_parameter_cover_v1(&x.binding,i as u64,BUDGET).unwrap_or_else(|e|panic!("{} {i}: {e}",f.name));
            assert!(cover.leaves.windows(2).all(|w|w[0]<w[1]));
            for p in coordinates(&root.finalize.params) {assert!(cover.leaves.binary_search(&p).is_ok(),"{} leaf{i} misses parameter{p}",f.name);}
            let _=l;cases+=1;
            assert!(derive_parameter_cover_v1(&x.binding,i as u64,ParameterCoverBudgetV1{max_edges:0,max_coordinates:0}).is_err());
        }
    }
    assert!(cases>=8,"must exercise multiple native history programs");
}

#[test]
fn gather_and_select_single_execution_read_sets_are_not_a_potential_cover() {
    use misaka_palw_tir::{DType,Ref,TensorType};
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::demand::{MapSource,MapSourceRequest,DemandRequest,DemandTarget,DemandContext,DemandLimits,eval_demanded};
    let mut pb=ProgramBuilder::new(2,misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
    let left=pb.param("left",DType::I32,&[2],false);
    let right=pb.param("right",DType::I32,&[2],false);
    let carry=vec![TensorType::fixed(DType::I32,&[1])];
    let (pre,target)={let mut b=pb.block("pre",vec![]);
        let chosen=b.select(Ref::Input(0),left,right,DType::I32);
        let gathered=b.gather(chosen,Ref::Input(0),0,0);
        let out=b.reshape_fixed(gathered,&[1]);
        let Ref::Node(n)=out else {unreachable!()};(b.finish(&[out]),n)};
    let layer={let mut b=pb.block("pass",carry.clone());let v=b.reshape_fixed(Ref::CarryIn(0),&[1]);b.finish(&[v])};
    let post={let mut b=pb.block("post",carry);let v=b.reshape_fixed(Ref::CarryIn(0),&[1]);b.commit(v);b.finish(&[])};
    let p=pb.finish(pre,vec![layer],post,0);
    let info=misaka_palw_tir::validate::validate(&p).unwrap();
    let mut sets=Vec::new();
    for token in [0u32,1] {
        let mut src=MapSource::default();src.tokens.insert(0,token);
        src.params.insert((0,None),vec![10,11]);src.params.insert((1,None),vec![20,21]);
        let req=DemandRequest{target:DemandTarget::Node{ctx:DemandContext{pos:0,occurrence:0},node:target},elements:&[0]};
        let (values,_)=eval_demanded(&p,&info,&req,&mut src,&DemandLimits::UNLIMITED).unwrap();
        assert_eq!(values,vec![if token==0 {20}else{11}]);
        sets.push(src.requests.iter().filter_map(|r|match r {MapSourceRequest::Param{param,index,..}=>Some((*param,*index)),_=>None}).collect::<Vec<_>>());
    }
    assert_eq!(sets,vec![vec![(1,0)],vec![(0,1)]]);
    assert_ne!(sets[0],sets[1],"neither legal execution's reads cover the other branch/index");
}

/// A real native history-dissection fixture whose reduction controls BOTH a
/// Select branch and a Gather row. Only the tail piece of each possible row is
/// needed. The unrelated committed parameter is deliberately outside this cone.
fn dynamic_fixture(parameter_free:bool, per_layer:bool, fixed_replay:bool)->(Fixture,u16) {
    use misaka_palw_tir::{DType,Ref,TensorType,MapParams,Tensor};
    use misaka_palw_tir::builder::ProgramBuilder;
    let mut pb=ProgramBuilder::new(4,misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
    let columns=if parameter_free {1}else{8193};
    let shape=[4,columns];
    let (left,right)=if parameter_free {
        let a=pb.konst(DType::I32,&shape,&vec![7;4*columns as usize]);(a,a)
    } else {
        (pb.param("left",DType::I32,&shape,per_layer),pb.param("right",DType::I32,&shape,per_layer))
    };
    let unrelated=pb.param("unrelated",DType::I8,&[1],per_layer);
    let replay=if fixed_replay {Some(pb.param("writer",DType::I8,&[1],per_layer))}else{None};
    let history=pb.hist_state("history",DType::I8,&[1],8,true);
    let state=if fixed_replay {Some(pb.fixed_state("replay",DType::I32,&[1],-128,127,true))}else{None};
    let carry=vec![TensorType::fixed(DType::I32,&[1])];
    let pre={let mut b=pb.block("pre",vec![]);let t=b.cast(Ref::Input(0),DType::I32);let t=b.reshape_fixed(t,&[1]);b.finish(&[t])};
    let (layer,target)={let mut b=pb.block("history",carry.clone());
        let row=b.clamp(Ref::CarryIn(0),0,3,DType::I32);let row=b.cast(row,DType::I8);
        let h=b.hist_append(history,row);let total=b.reduce_sum(h,0,DType::I32);let mut value=b.reshape_fixed(total,&[]);
        if let Some(state)=state {let s=b.reshape_fixed(Ref::State(state),&[]);value=b.add(value,s,DType::I32);}
        let selected=b.select(value,left,right,DType::I32);
        let clamped=b.clamp(value,0,3,DType::I32);let ix=b.cast(clamped,DType::Idx);let ix=b.reshape_fixed(ix,&[1]);
        let gathered=b.gather(selected,ix,0,0);let tail=b.slice(gathered,1,columns-1,1);let out=b.reshape_fixed(tail,&[1]);b.commit(out);
        let Ref::Node(target)=out else{unreachable!()};
        // This other committed cone must not contaminate a potential cover.
        let other=b.cast(unrelated,DType::I32);b.commit(other);
        if let (Some(state),Some(replay))=(state,replay) {b.state_write(state,replay);}
        (b.finish(&[out]),target)};
    let post={let mut b=pb.block("post",carry);let out=b.reshape_fixed(Ref::CarryIn(0),&[1]);b.commit(out);b.finish(&[])};
    let p=pb.finish(pre,if per_layer {vec![layer,layer]}else{vec![layer]},post,0);
    let mut params=MapParams::default();
    for (j,d) in p.params.iter().enumerate() {
        let layers=if d.per_layer {vec![Some(0),Some(1)]}else{vec![None]};
        for layer in layers {let n=d.shape.iter().map(|n|*n as usize).product();
            params.tensors.insert((j as u16,layer),Tensor{dtype:d.dtype,shape:d.shape.iter().map(|n|*n as usize).collect(),data:vec![if d.name=="writer"{1}else{7};n]});
        }
    }
    (fixture_with("dynamic-parameter-cover".into(),p,params,vec![0,1,2,3],4,2),target)
}
fn named_leaf(f:&Fixture,target:u16,position:u32,occurrence:u16)->u64 {
    f.leaves.iter().position(|l|l.position==position&&matches!(l.kind,
        kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1::Commit{node,occurrence:o,..} if node==target&&o==u32::from(occurrence)))
        .expect("the named native commit leaf exists") as u64
}
fn assert_subset(params:&PalwTirParamOpeningV1,cover:&[u32]) {
    for leaf in coordinates(params) {assert!(cover.binary_search(&leaf).is_ok(),"potential cover misses native parameter{leaf}");}
}
#[test]
fn derived_dynamic_cover_has_both_branches_every_row_and_no_unrelated_piece() {
    use kaspa_consensus_core::palw_tir_court_v1::PalwTirInventoryIndexV1;
    let (f,target)=dynamic_fixture(false,false,false);let x=f.honest();let leaf=named_leaf(&f,target,3,1);
    let cover=derive_parameter_cover_v1(&x.binding,leaf,BUDGET).unwrap();
    let inventory=PalwTirInventoryIndexV1::new(&f.space.program).unwrap();
    let mut expected=Vec::new();
    for param in [0u16,1] {for row in 0u64..4 {expected.push(inventory.leaf_of(param,None,(row*8193+8192)*4).unwrap());}}
    expected.sort_unstable();assert_eq!(cover.leaves,expected,"only actual possible tail pieces of the two branches");
    assert!(cover.leaves.len()<inventory.leaf_count() as usize,"never substitute the entire registered inventory");
}
#[test]
fn parameter_free_dissection_has_empty_cover_and_per_layer_scopes_remain_separate() {
    use kaspa_consensus_core::palw_tir_court_v1::PalwTirInventoryIndexV1;
    let (f,target)=dynamic_fixture(true,false,false);let x=f.honest();
    let leaf=named_leaf(&f,target,3,1);let cover=derive_parameter_cover_v1(&x.binding,leaf,BUDGET).unwrap();
    assert!(cover.leaves.is_empty(),"an unrelated parameter does not make this cone require model bytes");
    let (f,target)=dynamic_fixture(false,true,false);let x=f.honest();let inventory=PalwTirInventoryIndexV1::new(&f.space.program).unwrap();
    let mut covers=Vec::new();for layer in [0u16,1] {
        let leaf=named_leaf(&f,target,3,layer+1);let cover=derive_parameter_cover_v1(&x.binding,leaf,BUDGET).unwrap();
        let mut expected=Vec::new();for param in [0u16,1] {for row in 0u64..4 {expected.push(inventory.leaf_of(param,Some(layer),(row*8193+8192)*4).unwrap());}}
        expected.sort_unstable();assert_eq!(cover.leaves,expected);covers.push(cover.leaves);
    }
    assert!(covers[0].iter().all(|p|!covers[1].contains(p)),"another occurrence must not authorize another layer's model bytes");
}
#[test]
fn fixed_replay_adds_the_actual_writer_parameter_only_before_the_checkpoint() {
    use kaspa_consensus_core::palw_tir_court_v1::PalwTirInventoryIndexV1;
    let (f,target)=dynamic_fixture(false,false,true);let x=f.honest();let inventory=PalwTirInventoryIndexV1::new(&f.space.program).unwrap();
    let writer=inventory.leaf_of(3,None,0).unwrap();
    let replay=derive_parameter_cover_v1(&x.binding,named_leaf(&f,target,1,1),BUDGET).unwrap();
    let checkpoint=derive_parameter_cover_v1(&x.binding,named_leaf(&f,target,2,1),BUDGET).unwrap();
    assert!(replay.leaves.contains(&writer),"noncommit writer from the previous context really replays");
    assert!(!checkpoint.leaves.contains(&writer),"native checkpoint is authenticated claim material, not a model-byte demand");
}

#[test]
fn adversarial_dynamic_roots_and_every_terminal_path_stay_inside_the_cover() {
    use kaspa_consensus_core::Hash64;
    use kaspa_consensus_core::palw_artifact::{open_artifact_leaf_v1,palw_artifact_multiproof_from_openings_v1};
    use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
    use kaspa_consensus_core::palw_tir_court_v1::{PalwTirInventoryIndexV1,check_tir_root_claim_v1,
        build_tir_dissect_round_v1,build_tir_dissect_bottom_v1,check_tir_dissect_bottom_v1};
    use kaspa_consensus_core::palw_tir_dissect_v1::{PalwTirDissectPhaseV1,PalwTirDissectChoiceV1,PALW_TIR_DISSECT_OBJECT_VERSION_V1};
    let (f,target)=dynamic_fixture(false,false,false);let x=f.honest();let store=Store{f:&f,x:&x};
    let leaf=named_leaf(&f,target,3,1);let site=palw_tir_dissect_site_v1(&f.space,f.intervals.as_ref().unwrap(),&f.leaves[leaf as usize]).unwrap();
    let cover=derive_parameter_cover_v1(&x.binding,leaf,BUDGET).unwrap();
    let honest=build_tir_root_claim_v1(&x.binding,leaf,&store,&RULES).unwrap();
    assert_eq!(honest.totals.partials.len(),1);assert_eq!(honest.totals.partials[0].len(),1);
    let inventory=PalwTirInventoryIndexV1::new(&f.space.program).unwrap();
    let mut played=0;let mut roots=Vec::new();
    for total in 0i128..=3 {
        let mut root=honest.clone();root.totals.partials[0][0]=total;
        let param=if total==0 {1}else{0};
        let coordinate=inventory.leaf_of(param,None,(total as u64*8193+8192)*4).unwrap();
        let opening=open_artifact_leaf_v1(&f.ops,coordinate).unwrap();
        root.finalize.params=PalwTirParamOpeningV1::Single(palw_artifact_multiproof_from_openings_v1(&[opening]).unwrap());
        check_tir_root_claim_v1(&root,leaf,&RULES).unwrap_or_else(|e|panic!("adversarial total{total}: {e}"));
        assert_subset(&root.finalize.params,&cover.leaves);roots.push(coordinates(&root.finalize.params));
        let session=Hash64::from_bytes([0xC1;64]);
        let phase=PalwTirDissectPhaseV1::open(session,leaf,&site,&root,2,0,10).unwrap();
        let mut pending=vec![(phase,1u64)];
        while let Some((mut phase,t))=pending.pop() {
            if phase.turn()==PalwBisectTurnV1::Terminal {
                let bottom=build_tir_dissect_bottom_v1(&x.binding,&phase,&store,&RULES).unwrap();
                assert_subset(&bottom.params,&cover.leaves);
                // False totals may be convicted. Malformed/missing carriage is
                // never accepted as evidence that a path was exercised.
                let verdict=check_tir_dissect_bottom_v1(&phase,&bottom,leaf,&RULES);
                assert!(verdict.is_ok()||matches!(verdict,Err(kaspa_consensus_core::palw_step_refute::PalwStepRefuteError::NoFaultFound)));
                played+=1;continue;
            }
            let round=build_tir_dissect_round_v1(&x.binding,&phase,site.tile_positions,&store,&RULES).unwrap();
            // The adversary supplies bounded nonnegative child claims which
            // fold to its false total; truth remains an independent bottom read.
            let mut round=round;
            for (reduction,claimed) in phase.claim().partials.iter().enumerate() {
                for (e,total) in claimed.iter().enumerate() {
                    for child in &mut round.children {child.partials[reduction][e]=0;}
                    round.children.last_mut().unwrap().partials[reduction][e]=*total;
                }
            }
            phase.apply_round(&round,t,10).unwrap();
            for child in 0..phase.child_ranges().len() {
                let mut branch=phase.clone();branch.apply_choice(&PalwTirDissectChoiceV1{version:PALW_TIR_DISSECT_OBJECT_VERSION_V1,session_id:session,round:phase.round(),child:child as u8},t+1,10).unwrap();
                pending.push((branch,t+2));
            }
        }
    }
    assert!(roots.windows(2).all(|w|w[0]!=w[1]),"each accepted false total selects another authenticated parameter leaf");
    assert_eq!(played,8,"four legal root totals times both terminal history tiles");
}
