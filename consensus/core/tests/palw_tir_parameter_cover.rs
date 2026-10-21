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
