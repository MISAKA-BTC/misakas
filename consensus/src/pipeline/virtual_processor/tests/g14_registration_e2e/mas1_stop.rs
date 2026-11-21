//! Private MODEL_STOP readback on the same branch as a signed registration.
use super::*;
use crate::pipeline::virtual_processor::tests::new_miner_data;
use kaspa_consensus_core::palw_mas1::{Admission, Assessment, StopReason, new_mining_allowed};
use kaspa_rpc_core::RpcRawBlock;
use libcrux_ml_dsa::ml_dsa_65;
use misaka_palw_remote::tb1_exposure::mas1_control::{
    MiningDescriptor, OpenStopCase, RiskReason, SIGN_DOMAIN, StopAuthority, StopCase, StopEnrollment, StopScope, StopSignature,
};

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct ControlReplay {
    pub blocks: Vec<RpcRawBlock>,
    signatures: Vec<Vec<u8>>,
}

async fn accept(env: &mut Env, remote: Option<&ControlReplay>, i: usize, marker: [u8; 64]) -> Block {
    if let Some(r) = remote {
        let block = Block::try_from(r.blocks[i].clone()).unwrap();
        arrive(&env.chain, block.clone(), "MAS1 control").await;
        block
    } else {
        let mut miner = new_miner_data();
        miner.extra_data = marker.to_vec();
        env.chain.heartbeat_paying(miner, env.config.params.target_time_per_block(), vec![]).await
    }
}

pub(super) async fn exercise(
    env: &mut Env,
    class_id: Hash64,
    artifact: Hash64,
    registration: Hash64,
    remote: Option<&ControlReplay>,
    fork: &[Block],
) -> ControlReplay {
    let pairs = (0..4).map(|i| ml_dsa_65::generate_key_pair([160 + i; 32])).collect::<Vec<_>>();
    let public = pairs.iter().map(|p| p.verification_key.clone()).collect::<Vec<_>>();
    let network = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        env.config.params.net.to_string().as_bytes(),
        Some(env.config.params.genesis.hash),
    );
    let authority = StopAuthority { network: network.as_bytes(), epoch: 1, tb1_roster: [8; 64], keys: &public, fault_budget: 1 };
    let auth_marker = authority.marker(4).unwrap();
    let auth_block = accept(env, remote, 0, auth_marker).await;
    let enrollment = StopEnrollment { accepting_block: auth_block.hash(), authorized_marker: auth_marker, max_roster: 4 };
    let case = StopCase {
        scope: StopScope::ArtifactProfile,
        descriptor: MiningDescriptor {
            artifact: artifact.as_bytes(),
            program: class_id.as_bytes(),
            profile: [66; 64],
            readout: [37; 64],
            rule_version: [2; 64],
        },
        generation: 1,
        case_id: registration.as_bytes(),
        reason: RiskReason::UnresolvedAdmission,
        evidence: [14; 64],
        policy: [65; 64],
        deadline_exclusive_daa: 100,
    };
    let origin = accept(env, remote, 1, case.marker(auth_block.hash(), &auth_marker)).await;
    let opened = OpenStopCase::read(env.chain.ctx.consensus.as_ref(), &authority, &enrollment, &case, origin.hash()).unwrap();
    let other = StopCase { scope: StopScope::RuleVersion, ..case };
    assert!(OpenStopCase::read(env.chain.ctx.consensus.as_ref(), &authority, &enrollment, &other, origin.hash()).is_err());
    let sigs: Vec<_> = if let Some(r) = remote {
        r.signatures.iter().map(|s| ml_dsa_65::MLDSA65Signature::new(s.as_slice().try_into().unwrap())).collect()
    } else {
        pairs[..3].iter().map(|p| ml_dsa_65::sign(&p.signing_key, opened.signing_body(), SIGN_DOMAIN, [73; 32]).unwrap()).collect()
    };
    let rows: Vec<_> = sigs.iter().enumerate().map(|(i, s)| StopSignature { slot: i as u32, signature: s }).collect();
    assert!(opened.certificate_marker(&rows[..2]).is_err());
    let duplicates = [
        StopSignature { slot: 0, signature: &sigs[0] },
        StopSignature { slot: 0, signature: &sigs[0] },
        StopSignature { slot: 2, signature: &sigs[2] },
    ];
    assert!(opened.certificate_marker(&duplicates).is_err());
    assert!(opened.read_stop(env.chain.ctx.consensus.as_ref(), origin.hash(), &rows, &case.descriptor).is_err());
    let block = accept(env, remote, 2, opened.certificate_marker(&rows).unwrap()).await;
    let stop = opened.read_stop(env.chain.ctx.consensus.as_ref(), block.hash(), &rows, &case.descriptor).unwrap();
    assert_eq!(stop, StopReason::BftSafetyStop);
    let unrelated = MiningDescriptor { artifact: [0; 64], ..case.descriptor };
    assert!(opened.read_stop(env.chain.ctx.consensus.as_ref(), block.hash(), &rows, &unrelated).is_err());
    let assessment = Assessment::default();
    assert_eq!(assessment.decision(), Admission::ReviewHold);
    assert!(!new_mining_allowed(false, assessment.decision(), &assessment, &[stop], true));
    let (_, state) = env.chain.tip_state();
    assert!(state.model_market(&class_id).is_some());
    println!(
        "MAS1 pid={} case={} accepted={} signatures={} assessment={:?} stop={stop:?} market=open",
        std::process::id(),
        origin.hash(),
        block.hash(),
        rows.len(),
        assessment.decision()
    );
    for b in fork {
        arrive(&env.chain, b.clone(), "branch without MAS1 registration or control").await;
    }
    assert!(opened.read_stop(env.chain.ctx.consensus.as_ref(), block.hash(), &rows, &case.descriptor).is_err());
    println!("MAS1 pid={} detached_stop_rejected=true", std::process::id());
    ControlReplay {
        blocks: [auth_block, origin, block].iter().map(RpcRawBlock::from).collect(),
        signatures: sigs.iter().map(|s| s.as_slice().to_vec()).collect(),
    }
}
