//! **RFC09: a remote client's choice under rule E + Gate C is the full node's choice** (op 203 v2, `misaka_palw_remote::l2_rule_e`).
//!
//! Two nodes on one ruleset with the fork-choice commitment, ADR-0178 rule E and RFC-0018 Gate C armed from DAA 1 (so every header
//! commits the rule-E leaf v3 envelope), partitioned into two forks (E-V1's scenario: the GHOSTDAG-heavier side worked by one bond,
//! the lighter by two), then healed. The remote client sees only what op 203 v2 serves (the committed leaves of both paths, each
//! tip's v3 window above the fork, the fork's v3 registry openings) and an issuer's attestation of each tip's committed root; it
//! verifies every byte with the node's own readers and must choose exactly the node's sink. Missing or tampered evidence STOPs.
use super::*;
use super::rule_e::{free_attempt, free_slot};
use kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwForkChoiceExtrasRequestV2;
use misaka_palw_remote::l2::{ForkChoiceAttestationV1, L2LimitsV1, L2StopV1, PeerViewV1};
use misaka_palw_remote::l2_rule_e::{
    CommittedServedV2, PALW_FORK_CHOICE_LEAF_VERSION_RULE_E_V3, RuleEEvidenceV2, RuleEInputV2, verify_fork_choice_rule_e_v2,
};
use misaka_palw_remote::verify::{ChainPointV1, ClientRulesetV1, VerifiedChainV1};

const SPLIT_V2: usize = 24;

fn net_rule_e_gate_c() -> Net {
    let (mut config, _, premine, floats) = parts_ruled(None, true);
    config.params.palw_fork_choice_commitment_v1 = Some(ForkActivation::new(1));
    // Gate C's certification sweep is a kernel route move: the kernel route is in force with it.
    config.params.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(0));
    config.params.palw_independently_work_certified_v1 = Some(ForkActivation::new(1));
    config.params.sync_palw_rfc0018_v1();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else {
        unreachable!("ConsensusV2")
    };
    let bundle = bundle.clone();
    let heavy = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let light = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let (x, y) = (pay(&config, &floats, card_payout_spk(5)), pay(&config, &floats, card_payout_spk(6)));
    Net { config, bundle, premine, floats, heavy, light, nonce: 1 << 40, x, y }
}

/// `tip`'s selected chain from `checkpoint` (inclusive) as L1 would have verified it.
fn chain_of(node: &T12Chain, checkpoint: BlockHash, tip: BlockHash) -> VerifiedChainV1 {
    let mut hashes = vec![tip];
    let mut at = tip;
    while at != checkpoint {
        at = node.vp().ghostdag_store.get_selected_parent(at).expect("a selected parent");
        hashes.push(at);
    }
    hashes.reverse();
    let points = hashes
        .iter()
        .map(|h| {
            let header = node.vp().headers_store.get_header(*h).unwrap();
            ChainPointV1 { hash: *h, daa_score: header.daa_score, blue_score: header.blue_score, palw_state_root: header.palw_state_root }
        })
        .collect();
    let tip_header = (*node.vp().headers_store.get_header(tip).unwrap()).clone();
    VerifiedChainV1 { checkpoint, hashes, tip: tip_header, not_checked: Vec::new(), points }
}

/// Op 203 v2 as the node serves it, for every block of `chains`, the windows above `fork_blue` and every harness bond's registry.
fn served(node: &T12Chain, chains: &[&VerifiedChainV1], fork_blue: u64) -> (Vec<CommittedServedV2>, Vec<(BlockHash, Hash64)>) {
    use kaspa_consensus_core::api::ConsensusApi;
    let extras = PalwForkChoiceExtrasRequestV2 { rule_e_window_above_blue: Some(fork_blue), registry_bonds: node.bonds.clone() };
    let mut blocks: Vec<BlockHash> = chains.iter().flat_map(|c| c.hashes.iter().copied()).collect();
    blocks.sort();
    blocks.dedup();
    let (mut out, mut roots) = (Vec::new(), Vec::new());
    for chunk in blocks.chunks(16) {
        let served = node.ctx.consensus.palw_fork_choice_openings_v2(chunk, &extras).expect("op 203 v2 serves");
        for (block, entry) in served.entries {
            let e = entry.unwrap_or_else(|why| panic!("{block} is served: {why}"));
            roots.push((block, e.committed_root));
            out.push(CommittedServedV2 {
                block,
                leaf: e.committed_leaf,
                inner_root: e.opening.inner_root,
                window: e.rule_e_window.map(|w| (fork_blue, borsh::from_slice(&w).expect("a window opening"))),
                registry: node
                    .bonds
                    .iter()
                    .copied()
                    .zip(e.registry_openings.iter().map(|o| borsh::from_slice(o).expect("a registry opening")))
                    .collect(),
            });
        }
    }
    (out, roots)
}

fn attest(ruleset: &ClientRulesetV1, block: BlockHash, daa: u64, committed_root: Hash64) -> ForkChoiceAttestationV1 {
    let mut att = ForkChoiceAttestationV1 {
        network_id: ruleset.network_id.clone(),
        genesis: ruleset.genesis.clone(),
        consensus_params_id: ruleset.consensus_params_id.clone(),
        consensus_schedule_id: ruleset.consensus_schedule_id.clone(),
        block,
        block_daa: daa,
        committed_root,
        leaf_version: PALW_FORK_CHOICE_LEAF_VERSION_RULE_E_V3,
        dns_gate: None,
        issued_at_daa: daa,
        key_id: b"issuer".to_vec(),
        signature: Vec::new(),
    };
    att.signature = att.signing_digest().as_bytes().to_vec();
    att
}

#[tokio::test]
async fn rfc09_remote_rule_e_gate_c_choice_is_the_full_nodes_and_refuses_missing_or_tampered_evidence() {
    kaspa_core::log::try_init_logger(&std::env::var("G14_LOG").unwrap_or_else(|_| "warn".into()));
    let mut n = net_rule_e_gate_c();
    let fork = shared_prefix(&mut n).await;
    let (mut hb, mut lb) = (Vec::new(), Vec::new());
    for slot in 0..SPLIT_V2 {
        hb.extend(free_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
        if slot % 6 == 0 {
            hb.push(free_attempt(&mut n.heavy, 5).await.0);
        }
        lb.extend(free_slot(&mut n.light, &mut n.nonce, 1, Vec::new()).await);
        if slot < 2 {
            lb.push(free_attempt(&mut n.light, 2 + slot).await.0);
        }
    }
    let (ht, lt) = (n.heavy.sink(), n.light.sink());
    feed(&mut n.light, &hb).await;
    feed(&mut n.heavy, &lb).await;
    let node_choice = n.light.sink();
    assert_eq!(n.heavy.sink(), node_choice, "both full nodes agree");
    assert!(node_choice == ht || node_choice == lt);
    let rule_fork = n.light.vp().palw_rule_e_fork_v1(lt, ht).expect("the pair has a fork");
    assert_eq!(rule_fork.fork, fork, "the partition's fork point");
    let (ca, cb) = (chain_of(&n.light, fork, lt), chain_of(&n.light, fork, ht));
    let (served, roots) = served(&n.light, &[&ca, &cb], rule_fork.fork_blue_score);
    let ruleset = ClientRulesetV1::of(&n.config.params);
    let root_of = |b: BlockHash| roots.iter().find(|(x, _)| *x == b).unwrap().1;
    let evidence = vec![
        RuleEEvidenceV2 { attestation: attest(&ruleset, lt, ca.tip.daa_score, root_of(lt)) },
        RuleEEvidenceV2 { attestation: attest(&ruleset, ht, cb.tip.daa_score, root_of(ht)) },
    ];
    let views = vec![PeerViewV1 { peer: "a".into(), chain: ca.clone() }, PeerViewV1 { peer: "b".into(), chain: cb.clone() }];
    let trusted = vec![(b"issuer".to_vec(), b"issuer-key".to_vec())];
    let limits = L2LimitsV1 { min_peers: 2, max_attestation_age_daa: 10_000, max_attested_lag_daa: 1 };
    let verify = |pk: &[u8], msg: &[u8], sig: &[u8]| pk == b"issuer-key" && msg == sig;
    let run = |served: &[CommittedServedV2], evidence: &[RuleEEvidenceV2]| {
        verify_fork_choice_rule_e_v2(
            &RuleEInputV2 { views: &views, evidence, served, trusted: &trusted, ruleset: &ruleset, params: &n.config.params, limits: &limits },
            &verify,
        )
    };
    // The remote's choice is the full node's.
    let established = run(&served, &evidence).expect("the remote establishes the fork choice");
    assert_eq!(established.chosen.tip_hash(), node_choice, "remote = full node choice under rule E + Gate C");
    assert_eq!(established.refused.len(), 1);

    // Missing evidence: a tip's window, the fork's registry, a path leaf, an attestation — STOP.
    let without_window: Vec<_> =
        served.iter().cloned().map(|mut s| { if s.block == lt { s.window = None; } s }).collect();
    assert!(matches!(run(&without_window, &evidence), Err(L2StopV1::RuleEEvidence(_))), "a missing window stops");
    let path_block = ca.hashes[ca.hashes.len() / 2];
    let without_leaf: Vec<_> = served.iter().filter(|s| s.block != path_block).cloned().collect();
    assert!(matches!(run(&without_leaf, &evidence), Err(L2StopV1::SelectedChainUnverified { .. })), "a missing path leaf stops");
    assert!(matches!(run(&served, &evidence[..1]), Err(L2StopV1::Unweighable(_))), "an unattested tip stops");
    // Tampered evidence: a flipped leaf byte, a forged attested root, a substituted window — STOP.
    let flipped: Vec<_> = served
        .iter()
        .cloned()
        .map(|mut s| {
            if s.block == path_block {
                let last = s.leaf.len() - 1;
                s.leaf[last] ^= 1;
            }
            s
        })
        .collect();
    assert!(matches!(run(&flipped, &evidence), Err(L2StopV1::SelectedChainUnverified { .. })), "a tampered leaf stops");
    let mut forged = evidence.clone();
    forged[0].attestation = attest(&ruleset, lt, ca.tip.daa_score, Hash64::from_u64_word(7));
    assert!(run(&served, &forged).is_err(), "an attested root no served leaf opens stops");
    let lt_window = served.iter().find(|s| s.block == lt).unwrap().window.clone();
    let swapped: Vec<_> = served
        .iter()
        .cloned()
        .map(|mut s| {
            if s.block == ht {
                s.window = lt_window.clone();
            }
            s
        })
        .collect();
    assert!(run(&swapped, &evidence).is_err(), "another tip's window does not verify against this tip's leaf");
}
