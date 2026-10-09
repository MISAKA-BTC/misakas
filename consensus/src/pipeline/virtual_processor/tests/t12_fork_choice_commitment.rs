//! **RFC-0009 L2 on a real testnet-12 chain: the fork-choice commitment crosses its fence, the node serves openings, a client verifies.**
//!
//! `Params::palw_fork_choice_commitment_v1` is dormant on every preset; this arms it on the harness ruleset at a low height and drives
//! the real processor across it with heartbeats (every template built by the node, every block validated by it — construction equals
//! validation, or the block would not become the sink):
//!
//! * a post-state whose point stands below the fence is committed as the flat ADR-0043 root, and from the fence on as the envelope —
//!   and the chain child's header commits exactly what op 203 serves for its parent;
//! * an op-202 collection proof at a header past the fence opens under the ADR-0043 root inside the envelope (unwrapped with the parent's
//!   opening), and no longer under the header's root directly;
//! * a remote client (`misaka-palw-remote::l2`) verifies the headers from a checkpoint (L1), opens the tip's keys from an attested root
//!   (L2) and proves a bond at a header the attestation covers (L3): `VERIFIED_REMOTE`, with the issuer named.

use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_fork_choice_commitment_v1::{
    PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1, PalwForkChoiceOpeningV1, PalwForkChoicePointV1,
};
use kaspa_consensus_core::palw_state_proof_v1::verify_bond_v1;
use kaspa_hashes::Hash64;
use misaka_palw_remote::l2::{
    ForkChoiceAttestationV1, ForkChoiceEvidenceV1, ForkChoiceRulesV1, L2InputV1, L2LimitsV1, L2VerdictV1, PeerViewV1,
    l3_root_under_l2_v1, verify_fork_choice_v1,
};
use misaka_palw_remote::verify::{
    CheckpointTrustV1, ClientRulesetV1, ModeLabelV1, TrustedCheckpointV1, VerifyLimitsV1, mode_label_v1, verify_header_chain_v1,
};

const FENCE: u64 = 3;

#[tokio::test]
async fn the_fork_choice_commitment_crosses_its_fence_and_a_remote_client_verifies_the_tip() {
    let (mut config, bundle, premine, floats) = t12_with_harness_cards();
    config.params.palw_fork_choice_commitment_v1 = Some(ForkActivation::new(FENCE));
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let ttpb = config.params.target_time_per_block();
    // Heartbeats until three blocks stand past the fence (the harness clock moves the DAA a tick per slot, not per block), within one
    // op-203 request.
    let mut blocks = Vec::new();
    while blocks.len() < PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1
        && blocks.iter().filter(|b: &&kaspa_consensus_core::block::Block| b.header.daa_score >= FENCE).count() < 3
    {
        blocks.push(chain.heartbeat(ttpb, Vec::new()).await);
    }
    eprintln!("[l2fc] DAA scores: {:?}", blocks.iter().map(|b| b.header.daa_score).collect::<Vec<_>>());
    let hashes: Vec<Hash64> = blocks.iter().map(|b| b.header.hash).collect();
    let served = chain.ctx.consensus.palw_fork_choice_openings_v1(&hashes).expect("the node serves openings");
    assert_eq!(served.sink, *hashes.last().unwrap());
    let entries: Vec<_> = served.entries.iter().map(|(b, e)| (*b, e.clone().expect("every chain block is weighable"))).collect();

    // Below the fence the flat root, from it the envelope — and the chain child commits exactly what op 203 serves.
    let (mut below, mut past) = (0, 0);
    for (i, (block, e)) in entries.iter().enumerate() {
        assert_eq!(e.opening.leaf.block, *block);
        assert_eq!(e.committed_form, e.opening.leaf.daa_score >= FENCE, "the form is the post-state's point's");
        if e.committed_form {
            past += 1;
            assert_eq!(e.committed_root, e.opening.committed_root());
            assert_ne!(e.committed_root, e.opening.inner_root);
        } else {
            below += 1;
            assert_eq!(e.committed_root, e.opening.inner_root, "dormant: the ADR-0043 root, byte for byte");
        }
        if let Some(child) = blocks.get(i + 1) {
            assert_eq!(child.header.palw_state_root, e.committed_root, "block {i}'s chain child commits what the node serves");
            if e.committed_form {
                let point = PalwForkChoicePointV1 { block: *block, daa_score: e.header.daa_score, blue_score: e.header.blue_score };
                let order = e.opening.verify(&child.header.palw_state_root, &point, config.params.palw_fork_choice_commitment_v1);
                assert_eq!(order.unwrap(), e.opening.leaf.order());
            }
        }
    }
    assert!(below > 0 && past > 0, "the chain crossed the fence ({below} below, {past} past)");

    // L3 through the envelope: op 202 at the first header whose parent's post-state is past the fence (below the tip, so the client's
    // walk from the tip down to it takes more than one step) opens under the inner root, unwrapped with the parent's opening.
    let p = (1..entries.len()).find(|&i| entries[i - 1].1.committed_form).expect("a header commits an envelope");
    assert!(p + 1 < entries.len(), "the pinned header {p} is below the tip");
    let pinned = hashes[p];
    let (header, proof) = chain.ctx.consensus.palw_state_proof_v1(pinned, b"bonds").expect("op 202 serves past the fence");
    let parent_opening: PalwForkChoiceOpeningV1 = entries[p - 1].1.opening;
    assert_eq!(parent_opening.committed_root(), header.palw_state_root);
    for bond in &chain.bonds {
        verify_bond_v1(&proof, parent_opening.inner_root, bond).expect("a genesis bond, under the inner root");
    }
    assert!(verify_bond_v1(&proof, header.palw_state_root, &chain.bonds[0]).is_err(), "the envelope is not the ADR-0043 root");

    // A remote client: L1 from a checkpoint, L2 from an attested root it opens itself, L3 at a covered header.
    let cp = &blocks[0].header;
    let trusted = TrustedCheckpointV1 {
        block: cp.hash,
        daa_score: cp.daa_score,
        trust: CheckpointTrustV1::Signed { issued_at_daa: cp.daa_score },
    };
    let headers: Vec<_> = blocks.iter().map(|b| (*b.header).clone()).collect();
    let tip = headers.last().unwrap().clone();
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let lim = VerifyLimitsV1 { max_headers: 100, ..VerifyLimitsV1::default() };
    let l1 = verify_header_chain_v1(&trusted, &headers, domain, tip.timestamp, &lim).expect("L1 over the node's own headers");
    let views = [PeerViewV1 { peer: "p1".into(), chain: l1.clone() }, PeerViewV1 { peer: "p2".into(), chain: l1 }];
    let ruleset = ClientRulesetV1::of(&config.params);
    let tip_entry = &entries.last().unwrap().1;
    let mut attestation = ForkChoiceAttestationV1 {
        network_id: ruleset.network_id.clone(),
        consensus_params_id: ruleset.consensus_params_id.clone(),
        consensus_schedule_id: ruleset.consensus_schedule_id.clone(),
        block: tip.hash,
        block_daa: tip.daa_score,
        committed_root: tip_entry.committed_root,
        leaf_version: tip_entry.opening.leaf.leaf_version,
        dns_gate: served.dns_gate,
        issued_at_daa: tip.daa_score,
        key_id: b"own-node".to_vec(),
        signature: Vec::new(),
    };
    attestation.signature = attestation.signing_digest().as_bytes().as_slice()[..3].to_vec();
    let toy = |pk: &[u8], msg: &[u8], sig: &[u8]| pk == b"pk" && sig == &msg[..3];
    let keys = vec![(b"own-node".to_vec(), b"pk".to_vec())];
    let rules = ForkChoiceRulesV1::of(&config.params);
    let evidence = [ForkChoiceEvidenceV1 { attestation, opening: tip_entry.opening }];
    // Every opening op 203 served — untrusted until the walk from the attested tip checks each against the next header's root.
    let served_openings: Vec<PalwForkChoiceOpeningV1> = entries.iter().map(|(_, e)| e.opening).collect();
    let input = L2InputV1 {
        views: &views,
        evidence: &evidence,
        chain_openings: &served_openings,
        trusted: &keys,
        ruleset: &ruleset,
        rules: &rules,
        limits: &L2LimitsV1::default(),
    };
    let verdict = verify_fork_choice_v1(&input, &toy).expect("no STOP on one chain");
    assert!(matches!(&verdict, L2VerdictV1::Established { chosen, .. } if chosen.tip_hash() == tip.hash), "{verdict:?}");
    assert_eq!(mode_label_v1(false, true, true, &verdict.status()), ModeLabelV1::VerifiedRemote);
    assert!(verdict.trust_line().contains("issuer 'own-node'"));
    // The proof's header is below the attested tip: the walk from the tip down to its parent checks it is the tip's selected chain.
    let root = l3_root_under_l2_v1(&verdict, &header, &served_openings, &rules).expect("a covered header");
    assert_eq!(root, parent_opening.inner_root);
    verify_bond_v1(&proof, root, &chain.bonds[0]).expect("L3 under L2");
}
