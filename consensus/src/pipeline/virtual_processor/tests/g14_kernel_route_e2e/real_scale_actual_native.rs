//! Actual-weight 32-position delivery witness through the node's carrier, court and settlement.
//! This is a mechanics test: the parent's activation, artifact-attestation and OPV-eligibility
//! test seams remain explicit. Excluding the eligibility seam, the node derives NotOnboarded.
//! This does not establish conformance/onboarding, source fidelity, full computation
//! correctness, network activation or a public DA service for this model.
use super::*;
use kaspa_rpc_core::GetPalwKernelClaimResponse;
use kaspa_rpc_core::convert::palw_kernel::palw_kernel_claim_response_v1;
use misaka_palw_kernel::element::{SegConvictionKindV1, verify_seg_fault_v1};
use misaka_palw_kernel::seg::{SEG_EVIDENCE_VERSION_V2, SegmentedEvidenceV2, claim_root_v1, job_input_root_v2};
use misaka_palw_kernel::verify::DismissalV1;
use misaka_palw_sdk::kernel_execution::DecodeCourtBundleV3;

fn unhex(s: &str) -> Vec<u8> {
    assert_eq!(s.len() % 2, 0);
    let mut out = vec![0; s.len() / 2];
    faster_hex::hex_decode(s.as_bytes(), &mut out).unwrap();
    out
}

/// RPC op 210's own builder, carried through JSON exactly as an external client consumes it.
/// The node is locally synced/authenticated; JSON itself is not an inclusion proof.
fn rpc_read(net: &Net, id: Digest) -> (GetPalwKernelClaimResponse, misaka_palw_kernel::seg_ledger::SegClaimViewV1) {
    let answer = palw_kernel_claim_response_v1(net.chain.ctx.consensus.consensus_clone().as_ref(), Hash64::from_bytes(id)).unwrap();
    let answer: GetPalwKernelClaimResponse = serde_json::from_slice(&serde_json::to_vec(&answer).unwrap()).unwrap();
    assert!(answer.available && answer.found);
    assert_eq!(answer.kind, "segmented");
    assert_eq!(answer.claim_id, Hash64::from_bytes(id).to_string());
    let record = SegmentedClaimRecordV1::from_bytes(&unhex(&answer.public_record)).unwrap();
    let header: EvidenceHeaderV1 = borsh::from_slice(&unhex(&answer.record_header)).unwrap();
    // Independently anchor the header in the synced node's class row, not in the record.
    let class: Digest = unhex(&answer.class_id).try_into().unwrap();
    assert_eq!(header, net.ledger().classes[&class].header(class));
    let producer: Digest = unhex(&answer.producer_bond).try_into().unwrap();
    assert_eq!(record.job_id.as_slice(), unhex(&answer.job_id));
    let view = record.view_for_claim(&header, &id, &producer).unwrap();
    // A coherent replacement of the last delivered token used to pass view(header): it is
    // not fed into the trace. Claim identity must reject it before any court is selected.
    let mut altered = record.clone();
    altered.generated[0] = (altered.generated[0] + 1) % view.program.token_bound;
    assert!(altered.view(&header).is_ok(), "header/trace roots alone do not bind final delivery");
    assert!(altered.view_for_claim(&header, &id, &producer).is_err());
    (answer, view)
}

async fn commit_recorded(w: &mut SegWorld, bundle: &DecodeCourtBundleV3, producer: usize, generated: Vec<u32>) -> Digest {
    let job = w.tiled_job_generating(&bundle.tokens, 1).await;
    let class = &w.net.ledger().classes[&w.class];
    let roots = misaka_palw_kernel::seg::segment_roots_of_position_roots_v1(&bundle.position_roots);
    let evidence = SegmentedEvidenceV2 {
        version: SEG_EVIDENCE_VERSION_V2,
        header: class.header(w.class),
        job_input_root: job_input_root_v2(bundle.tokens.len() as u32, &prompt_root_of_ids_v1(&bundle.tokens), &[]),
        positions: bundle.position_roots.len() as u32,
        segment_len: SEG_LEN_V4,
        claim_root: claim_root_v1(bundle.position_roots.len() as u32, &roots),
        suite: misaka_palw_kernel::evidence::SuiteParamsV1::of(&class.descriptor),
    };
    let claim = KernelClaimV1 { job_id: job, producer_bond: w.net.kid(producer), generated, evidence_root: evidence.root() };
    let id = claim.id();
    w.seal_and_commit(producer, claim, evidence, roots).await;
    assert!(w.net.ledger().claims.contains_key(&id));
    id
}

#[tokio::test]
async fn recorded_actual_weight_delivery_convicts_through_rpc_carriers_and_an_honest_claim_redeems_once() {
    kaspa_core::log::try_init_logger("warn");
    let bundle: DecodeCourtBundleV3 = borsh::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/design/palw/tir/evidence/qwen25-real-native-court-bundle.borsh"
    )))
    .unwrap();
    assert_eq!((bundle.version, bundle.tokens.len(), bundle.position_roots.len()), (1, 32, 32));
    assert_eq!((bundle.honest_generated.as_slice(), bundle.wrong_generated.as_slice()), (&[1][..], &[2][..]));
    assert_eq!(bundle.params.by_instance.len(), 1636);
    assert_ne!(bundle.inventory_root, bundle.params.root(), "inventory and kernel roots are distinct domains");
    let program = TirProgramV1::decode_canonical(&bundle.program_bytes).unwrap();
    assert_eq!(program.occurrences().iter().map(|(b, _)| program.blocks[*b as usize].nodes.len()).sum::<usize>(), 7658);
    let descriptor = k2_tir_v4_descriptor();
    assert_eq!(bundle.plan, plan_for_tir_program_v1(&descriptor, &program, program_root_v1(&bundle.program_bytes), 32).unwrap());
    // No weights or decoded private producer capture enter the node test.
    let f = SegFixture { program, params: MapParams::default(), plan: bundle.plan.clone(), pc: bundle.params.clone() };
    let net = Net::over_cfg(kernel_config_opv(Vec::new()), TestConsensus::new);
    kernel_route_test_attest_artifact_v1(Hash64::from_bytes(f.pc.root()), 0);
    assert!(
        kaspa_consensus_core::palw_opv_bootstrap_v1::palw_complete_check_domain_v1(&f.program, 32).is_err(),
        "this stateful actual program cannot use the small stateless complete-check bootstrap"
    );
    // Explicit existing mechanics seam, never a claim that this model passed onboarding.
    // Its facts apply from the beginning on BOTH nodes. Changing this global hook after
    // folding a refused registration would incorrectly admit that earlier object on replay.
    let mut w = SegWorld::register_with_cap(net, f, 32_000).await;
    let route = w.net.api().unwrap();
    let ledger = route.ledger().unwrap();
    let policy = route.header.opv.as_ref().unwrap();
    let no_hook = kaspa_consensus_core::palw_opv_bootstrap_v1::OpvEligibilityViewV1 {
        policy,
        denied: &[],
        min_effective_bits: 128,
        sampled_gates_reward: false,
        test_eligible: &[],
    };
    let facts = kaspa_consensus_core::palw_opv_bootstrap_v1::OpvClassFactsV1::of_registration(
        descriptor.digest(),
        &bundle.program_bytes,
        &bundle.plan,
        &bundle.params,
    );
    assert_eq!(
        route.opv_eligibility_v1(&ledger, &facts, w.net.daa(), &no_hook),
        Err(kaspa_consensus_core::palw_opv_bootstrap_v1::OpvIneligibleV1::NotOnboarded),
        "the real derived gate stays closed: no onboarding evidence or KernelBound row exists"
    );
    let (producer, outsider) = (0, w.net.chain.bonds.len() - 1);
    let lie = commit_recorded(&mut w, &bundle, producer, bundle.wrong_generated.clone()).await;
    let (read, view) = rpc_read(&w.net, lie);
    assert_eq!(view.program.encode(), bundle.program_bytes);
    assert_eq!(view.params, bundle.params);
    assert_eq!(view.positions, 32);
    let proof = SegFaultV1::from_bytes(&bundle.fault.to_bytes()).unwrap();
    let convicted = verify_seg_fault_v1(&view.context(), &proof).unwrap();
    assert_eq!((convicted.position, convicted.kind), (31, SegConvictionKindV1::Decode { index: 0 }));
    assert!(proof.to_bytes().len() as u64 <= w.net.ledger().classes[&w.class].bounds.max_filing_bytes);
    let (slash, minted, reward) = (w.net.slashed(producer), minted_and_owed(&w.net, &w.net.chain, outsider), read.reserved_sompi);
    // A copied position is neither admitted as a conviction nor allowed to slash the honest bond.
    let mut copied = proof.clone();
    let SegFaultV1::Decode(f) = &mut copied else { unreachable!() };
    f.logits.node.as_mut().unwrap().position = 30;
    assert!(matches!(verify_seg_fault_v1(&view.context(), &copied), Err(DismissalV1::NotAuthentic(_))));
    w.file(outsider, &lie, &copied).await;
    assert!(!w.net.ledger().claims[&lie].convicted);
    assert_eq!(w.net.slashed(producer), slash);
    w.file(outsider, &lie, &proof).await;
    assert!(w.net.ledger().claims[&lie].convicted);
    assert!(matches!(w.net.claim_state(&lie), ClaimStateV1::Convicted { .. }));
    assert_eq!(w.net.slashed(producer) - slash, reward, "the actual claim's reservation is slashed");
    let after = minted_and_owed(&w.net, &w.net.chain, outsider);
    assert!(after.0 + after.1 > minted.0 + minted.1, "the independent accuser is paid");
    let slashed_once = w.net.slashed(producer);
    w.file(outsider, &lie, &proof).await;
    assert_eq!(w.net.slashed(producer), slashed_once, "duplicate conviction cannot slash twice");
    let honest = commit_recorded(&mut w, &bundle, producer, bundle.honest_generated.clone()).await;
    let (_, clean) = rpc_read(&w.net, honest);
    let mut weaker = proof;
    let SegFaultV1::Decode(f) = &mut weaker else { unreachable!() };
    f.rival = bundle.wrong_generated[0];
    assert_eq!(verify_seg_fault_v1(&clean.context(), &weaker), Err(DismissalV1::NoFault));
    let slash = w.net.slashed(producer);
    let before = minted_and_owed(&w.net, &w.net.chain, producer);
    let reward = w.net.ledger().policy.claim_reward;
    w.file(outsider, &honest, &weaker).await;
    assert!(!w.net.ledger().claims[&honest].convicted);
    assert_eq!(w.net.slashed(producer), slash, "honest delivery cannot be convicted by a weaker rival");
    w.net.beat_to(w.net.daa() + 80).await;
    assert!(matches!(w.net.claim_state(&honest), ClaimStateV1::Final { .. }));
    assert!(w.net.ledger().claims[&honest].rewarded);
    let paid = minted_and_owed(&w.net, &w.net.chain, producer);
    assert_eq!(paid.0 + paid.1 - before.0 - before.1, reward);
    w.net.beat_to(w.net.daa() + 5).await;
    assert_eq!(minted_and_owed(&w.net, &w.net.chain, producer), (paid.0 + paid.1, 0), "coinbase redeems the Final reward once");
    let second = w.net.replay().await;
    w.net.assert_same(&second, "independent replay of actual-weight delivery court and Final");
    assert_eq!(minted_and_owed(&w.net, &second, producer), (paid.0 + paid.1, 0));
    assert_eq!(minted_and_owed(&w.net, &second, outsider), minted_and_owed(&w.net, &w.net.chain, outsider));
}
