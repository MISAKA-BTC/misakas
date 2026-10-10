//! RFC-0009 L2's attack tests (the RFC's mandatory list) and the rules around them. Every chain here is L1-verified from a trusted
//! checkpoint exactly as a client does it; every opening is checked against an attested root; the comparator calls are the node's own.

use super::*;
use crate::checkpoint::{Checkpoint, CheckpointError, SignedCheckpointV1, verify_signed_checkpoint};
use crate::verify::{
    CheckpointTrustV1, ModeLabelV1, TrustedCheckpointV1, VerifyLimitsV1, mode_label_v1, signing_gate_v1, verify_header_chain_v1,
};
use kaspa_consensus_core::palw_fork_choice_commitment_v1::{
    PalwForkChoiceErrorV1, PalwWeightAllocationSlotV1, palw_fork_choice_envelope_root_v1,
};
use kaspa_consensus_core::palw_state_proof_v1::{prove_bonds_v1, verify_bond_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwStateParamsV2, apply_palw_transition_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

const STEP: u64 = 120_000;
const T0: u64 = 1_800_000_000_000;

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn domain() -> Hash64 {
    h(0xD0)
}

/// One block: its header, the fork-choice leaf of its post-state and the ADR-0043 root that leaf is enveloped with.
#[derive(Clone)]
struct Blk {
    header: Header,
    leaf: PalwForkChoiceLeafV1,
    inner: Hash64,
}

type Keys = (u64, u128, u128, u64); // (frontier blue score, safe weight, bounded immature, bonds)

fn header(parents: Vec<Hash64>, daa: u64, blue: u64, ts: u64, root: Hash64, salt: u64) -> Header {
    let mut x = Header::new_finalized(
        1,
        vec![parents].try_into().unwrap(),
        h(2),
        h(3),
        h(4),
        ts,
        0x1d00_ffff,
        salt,
        kaspa_consensus_core::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1,
        daa,
        blue.into(),
        blue,
        h(5),
    )
    .with_palw_state_root(root);
    x.finalize();
    x
}

fn leaf_of(x: &Header, (frontier, safe, immature, bonds): Keys) -> PalwForkChoiceLeafV1 {
    PalwForkChoiceLeafV1 {
        leaf_version: PALW_FORK_CHOICE_LEAF_VERSION_V1,
        block: x.hash,
        daa_score: x.daa_score,
        blue_score: x.blue_score,
        safe_frontier_blue_score: frontier,
        safe_frontier: h(frontier),
        safe_weight: safe,
        bounded_immature: immature,
        bonds_len: bonds,
        weight_allocation: PalwWeightAllocationSlotV1::NONE,
    }
}

fn checkpoint() -> Blk {
    let x = header(vec![h(1)], 100, 100, T0, h(0x51), 0);
    Blk { leaf: leaf_of(&x, (10, 10, 0, 8)), inner: h(0x1C), header: x }
}

/// The next block: its header commits the PREVIOUS block's post-state in the fork-choice form.
fn next(prev: &Blk, salt: u64, keys: Keys, inner: Hash64) -> Blk {
    let root = palw_fork_choice_envelope_root_v1(&prev.leaf, &prev.inner);
    let p = &prev.header;
    let x = header(vec![p.hash], p.daa_score + 1, p.blue_score + 1, p.timestamp + STEP, root, salt);
    Blk { leaf: leaf_of(&x, keys), inner, header: x }
}

/// `n` blocks after `from`, the last one with `tip` keys and the others carrying the checkpoint's.
fn branch(from: &Blk, n: usize, salt: u64, tip: Keys) -> Vec<Blk> {
    let mut out = vec![from.clone()];
    for i in 0..n {
        let keys = if i + 1 == n { tip } else { (10, 10, i as u128, 8) };
        out.push(next(out.last().unwrap(), salt * 1000 + i as u64, keys, h(salt * 1000 + 0x100 + i as u64)));
    }
    out
}

fn trusted_cp(c: &Blk) -> TrustedCheckpointV1 {
    TrustedCheckpointV1 {
        block: c.header.hash,
        daa_score: c.header.daa_score,
        trust: CheckpointTrustV1::Signed { issued_at_daa: 100 },
    }
}

fn view(peer: &str, c: &Blk, blocks: &[Blk], now: u64) -> PeerViewV1 {
    let headers: Vec<Header> = blocks.iter().map(|b| b.header.clone()).collect();
    let chain = verify_header_chain_v1(&trusted_cp(c), &headers, domain(), now, &VerifyLimitsV1::default()).expect("L1");
    PeerViewV1 { peer: peer.into(), chain }
}

fn ruleset() -> ClientRulesetV1 {
    ClientRulesetV1 {
        network_id: "testnet-12".into(),
        genesis: "g".into(),
        consensus_params_id: "p".into(),
        consensus_schedule_id: "s".into(),
    }
}

/// A toy signature: the digest's first three bytes, under the key `pk` (the checkpoint tests' convention).
fn toy(pubkey: &[u8], msg: &[u8], sig: &[u8]) -> bool {
    pubkey == b"pk" && sig == &msg[..3]
}
fn trusted_keys() -> Vec<(Vec<u8>, Vec<u8>)> {
    vec![(b"k1".to_vec(), b"pk".to_vec())]
}

fn attest_at(b: &Blk, issued: u64, dns: Option<PalwDnsGateFactV1>) -> ForkChoiceEvidenceV1 {
    let mut a = ForkChoiceAttestationV1 {
        network_id: "testnet-12".into(),
        consensus_params_id: "p".into(),
        consensus_schedule_id: "s".into(),
        block: b.header.hash,
        block_daa: b.header.daa_score,
        committed_root: palw_fork_choice_envelope_root_v1(&b.leaf, &b.inner),
        leaf_version: PALW_FORK_CHOICE_LEAF_VERSION_V1,
        dns_gate: dns,
        issued_at_daa: issued,
        key_id: b"k1".to_vec(),
        signature: Vec::new(),
    };
    a.signature = a.signing_digest().as_bytes().as_slice()[..3].to_vec();
    ForkChoiceEvidenceV1 { attestation: a, opening: PalwForkChoiceOpeningV1 { leaf: b.leaf, inner_root: b.inner } }
}
fn attest(b: &Blk, issued: u64) -> ForkChoiceEvidenceV1 {
    attest_at(b, issued, None)
}

fn rules() -> ForkChoiceRulesV1 {
    ForkChoiceRulesV1 {
        commitment: Some(ForkActivation::new(0)),
        strict_win: None,
        ibd_strict: None,
        frontier_provenance: None,
        dns_gate: None,
        dns_retired: None,
        rule_e: None,
        bond_budget: None,
        finality_depth: 600,
        panel: None,
    }
}

/// Every block's opening, as peers serve them (op 203) — untrusted input the walk checks.
fn openings(branches: &[&[Blk]]) -> Vec<PalwForkChoiceOpeningV1> {
    branches.iter().flat_map(|b| b.iter()).map(|x| PalwForkChoiceOpeningV1 { leaf: x.leaf, inner_root: x.inner }).collect()
}

fn run(
    views: &[PeerViewV1],
    evidence: &[ForkChoiceEvidenceV1],
    rules: &ForkChoiceRulesV1,
    limits: &L2LimitsV1,
    chain_openings: &[PalwForkChoiceOpeningV1],
) -> Result<L2VerdictV1, L2StopV1> {
    let keys = trusted_keys();
    let rs = ruleset();
    verify_fork_choice_v1(&L2InputV1 { views, evidence, chain_openings, trusted: &keys, ruleset: &rs, rules, limits }, &toy)
}

/// The canonical branch A (3 blocks, deeper matured frontier) and the heavier branch B (6 blocks, far more blue work and weight, a
/// shallower frontier): A wins the PALW order, B wins on blue work.
struct Fork {
    c: Blk,
    a: Vec<Blk>,
    b: Vec<Blk>,
    now: u64,
    /// Every block's opening on both branches.
    o: Vec<PalwForkChoiceOpeningV1>,
}
fn fork() -> Fork {
    let c = checkpoint();
    let a = branch(&c, 3, 1, (60, 100, 0, 8));
    let b = branch(&c, 6, 2, (50, 500, 1_000, 8));
    let o = openings(&[&a, &b]);
    Fork { now: T0 + 6 * STEP, c, a, b, o }
}

fn chosen_tip(v: &L2VerdictV1) -> Hash64 {
    match v {
        L2VerdictV1::Established { chosen, .. } => chosen.tip_hash(),
        other => panic!("not established: {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The mandatory attack tests
// ---------------------------------------------------------------------------------------------------------------------------------

/// **1. A fork that wins on raw blue work and loses the PALW order is never chosen** — not when both are shown and weighed (the
/// comparator picks A, under the plain rule and under strict-win with the shallow-tie answer taken both ways), not when B is the only
/// weighable one (STOP), not when the economic keys tie (STOP: GHOSTDAG or arrival order would decide).
#[test]
fn a_heavier_blue_work_fork_that_loses_the_palw_order_is_never_chosen() {
    let f = fork();
    let (ta, tb) = (f.a.last().unwrap(), f.b.last().unwrap());
    assert!(tb.header.blue_work > ta.header.blue_work, "B is heavier in blue work");
    let views = [view("p1", &f.c, &f.a, f.now), view("p2", &f.c, &f.b, f.now)];
    let ev = [attest(ta, 106), attest(tb, 106)];
    let lim = L2LimitsV1::default();

    let v = run(&views, &ev, &rules(), &lim, &f.o).unwrap();
    assert_eq!(chosen_tip(&v), ta.header.hash, "the PALW order picks A");
    let L2VerdictV1::Established { refused, .. } = &v else { unreachable!() };
    assert_eq!(refused, &vec![tb.header.hash], "B is refused by the comparator");
    // The views' order and the evidence order do not matter.
    let rev_views = [views[1].clone(), views[0].clone()];
    let rev_ev = [ev[1].clone(), ev[0].clone()];
    assert_eq!(chosen_tip(&run(&rev_views, &rev_ev, &rules(), &lim, &f.o).unwrap()), ta.header.hash);
    // Strict-win (and its IBD sibling) armed: still A, with the shallow-tie GHOSTDAG answer taken both ways.
    let strict = ForkChoiceRulesV1 { strict_win: Some(ForkActivation::new(0)), ibd_strict: Some(ForkActivation::new(0)), ..rules() };
    assert_eq!(chosen_tip(&run(&views, &ev, &strict, &lim, &f.o).unwrap()), ta.header.hash);
    // Only B weighable (A shown but not attested): STOP — an unweighable candidate is not a loser here.
    assert_eq!(run(&views, &ev[1..], &rules(), &lim, &f.o), Err(L2StopV1::Unweighable(ta.header.hash)));
    // A tie on every economic key (A' carries B's keys): the node's choice would rest on GHOSTDAG or arrival order — STOP, never B.
    let a_tied = branch(&f.c, 3, 3, (50, 500, 1_000, 8));
    let tied_views = [view("p1", &f.c, &a_tied, f.now), view("p2", &f.c, &f.b, f.now)];
    let tied_ev = [attest(a_tied.last().unwrap(), 106), attest(tb, 106)];
    assert_eq!(run(&tied_views, &tied_ev, &strict, &lim, &openings(&[&a_tied, &f.b])), Err(L2StopV1::NoRobustWinner));
    // The label: VERIFIED_REMOTE only with the L2 verdict, and the trust is named.
    assert_eq!(mode_label_v1(false, true, true, &v.status()), ModeLabelV1::VerifiedRemote);
    assert!(v.trust_line().contains("issuer 'k1'") && v.trust_line().contains("refused by the comparator"), "{}", v.trust_line());
    assert!(signing_gate_v1(mode_label_v1(false, true, true, &v.status()), None).is_ok());
}

/// **2. A correct Merkle proof of a non-canonical state is refused**: B's header commits a real state and the bond proof opens under it
/// — and is refused, because B is not the chosen chain. On A the same proof is accepted at a header the attestation covers, and refused
/// past it.
#[test]
fn a_correct_proof_of_a_non_canonical_state_is_refused() {
    // A real PALW state with one bond, and its bond-table proof.
    let params = PalwStateParamsV2::new(100, 1, 1, 1, 500, 1_000, h(1), 4, 1_000, 100, 1_000, 0).unwrap();
    let bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0));
    let reg = Obj::BondRegistered {
        bond,
        pubkey: vec![7; 2592],
        operator_pubkey: vec![8; 8],
        collateral: 1 << 40,
        payout_payload: h(0x9A),
        capable_classes: Default::default(),
        signature: Vec::new(),
    };
    let cx = PalwBlockContextV2 { block: h(10), daa_score: 1, blue_score: 1, subsidy: 0 };
    let (state, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &params, &cx, &[reg], None).unwrap();
    let real = state.state_root();
    let proof = prove_bonds_v1(&state);

    // Both branches' first block carries the real state; each second header commits it (enveloped).
    let c = checkpoint();
    let a1 = next(&c, 11, (10, 10, 0, 8), real);
    let a2 = next(&a1, 12, (10, 10, 0, 8), h(0xA2));
    let a3 = next(&a2, 13, (60, 100, 0, 8), h(0xA3));
    let a = vec![c.clone(), a1.clone(), a2, a3];
    let b1 = next(&c, 21, (10, 10, 0, 8), real);
    let mut b = vec![c.clone(), b1.clone()];
    for i in 0..4 {
        let last = b.last().unwrap().clone();
        b.push(next(&last, 22 + i, if i == 3 { (50, 500, 1_000, 8) } else { (10, 10, 0, 8) }, h(0xB2 + i)));
    }
    let now = T0 + 5 * STEP;
    let views = [view("p1", &c, &a, now), view("p2", &c, &b, now)];
    let ev = [attest(a.last().unwrap(), 105), attest(b.last().unwrap(), 105)];
    let served = openings(&[&a, &b]);
    let v = run(&views, &ev, &rules(), &L2LimitsV1::default(), &served).unwrap();
    assert_eq!(chosen_tip(&v), a.last().unwrap().header.hash);

    // The proof is CORRECT under B's header (B2 commits the real state, enveloped) …
    let b1_opening = PalwForkChoiceOpeningV1 { leaf: b1.leaf, inner_root: b1.inner };
    let b2 = &b[2].header;
    assert_eq!(b1_opening.committed_root(), b2.palw_state_root);
    assert!(verify_bond_v1(&proof, b1_opening.inner_root, &bond).is_ok(), "the proof itself holds");
    // … and refused: B is not the chosen chain.
    let refused = l3_root_under_l2_v1(&v, b2, &served, &rules()).unwrap_err();
    assert!(refused.contains("not on the chosen chain"), "{refused}");
    // Under A's header the same proof is accepted (the attestation covers it): the openings from the attested tip A3 down to A2's
    // parent A1 are walked, and A1's opening (the root A2 commits) gives the inner root.
    let a1_opening = PalwForkChoiceOpeningV1 { leaf: a1.leaf, inner_root: a1.inner };
    let a2_opening = PalwForkChoiceOpeningV1 { leaf: a[2].leaf, inner_root: a[2].inner };
    let root = l3_root_under_l2_v1(&v, &a[2].header, &[a2_opening, a1_opening], &rules()).unwrap();
    assert_eq!(root, real);
    assert_eq!(verify_bond_v1(&proof, root, &bond).unwrap().pubkey, vec![7; 2592]);
    // Without the openings the envelope cannot be unwrapped past the fence, nor the path walked; a forged opening does not hash to the
    // header's root.
    assert!(l3_root_under_l2_v1(&v, &a[2].header, &[], &rules()).is_err());
    assert!(l3_root_under_l2_v1(&v, &a[2].header, &[a1_opening], &rules()).is_err(), "A2's own opening (under A3's root) is missing");
    let mut forged = a1_opening;
    forged.inner_root = h(0xBAD);
    assert!(l3_root_under_l2_v1(&v, &a[2].header, &[a2_opening, forged], &rules()).is_err());
    // Not established → no proof is believed at all.
    assert!(l3_root_under_l2_v1(&L2VerdictV1::Unverified("x".into()), &a[2].header, &served, &rules()).is_err());
    // Past the attested block's child: a single-chain verdict attested at A1 covers A2 (its child, which commits exactly the attested
    // root), not A3.
    let wide = L2LimitsV1 { max_attested_lag_daa: 3, ..L2LimitsV1::default() };
    let v1 = run(&[view("p1", &c, &a, now), view("p2", &c, &a, now)], &[attest(&a1, 103)], &rules(), &wide, &served).unwrap();
    assert_eq!(l3_root_under_l2_v1(&v1, &a[2].header, &[], &rules()), Ok(real), "the attested opening itself");
    let past = l3_root_under_l2_v1(&v1, &a[3].header, &served, &rules()).unwrap_err();
    assert!(past.contains("past the attested block's child"), "{past}");
}

/// **3. A hidden competing tip is caught through a second peer** — or through an attestation naming a block no peer shows.
#[test]
fn a_hidden_tip_is_found_through_a_second_peer_or_an_attestation() {
    let f = fork();
    let (ta, tb) = (f.a.last().unwrap(), f.b.last().unwrap());
    let lim = L2LimitsV1::default();
    // One peer, showing only B: never above HEADER_VERIFIED (one peer cannot show what it hides).
    let only_b = [view("p2", &f.c, &f.b, f.now)];
    let v = run(&only_b, &[attest(tb, 106)], &rules(), &lim, &f.o).unwrap();
    assert!(matches!(&v, L2VerdictV1::Unverified(why) if why.contains("independent peer")), "{v:?}");
    assert_eq!(mode_label_v1(false, true, true, &v.status()), ModeLabelV1::HeaderVerifiedForkChoiceUnverified);
    // Two peers both hiding A, and the issuer attesting A's tip (its node is on A): the attestation names a block no peer shows — STOP.
    let both_hide = [view("p2", &f.c, &f.b, f.now), view("p3", &f.c, &f.b, f.now)];
    assert_eq!(
        run(&both_hide, &[attest(ta, 106), attest(tb, 106)], &rules(), &lim, &f.o),
        Err(L2StopV1::AttestedBlockHidden(ta.header.hash))
    );
    // A second peer shows A: the conflict is weighed and A wins.
    let revealed = [view("p2", &f.c, &f.b, f.now), view("p1", &f.c, &f.a, f.now)];
    assert_eq!(chosen_tip(&run(&revealed, &[attest(ta, 106), attest(tb, 106)], &rules(), &lim, &f.o).unwrap()), ta.header.hash);
    // A peer merely behind (a prefix of A) is consistent, and counts as a peer.
    let behind = [view("p1", &f.c, &f.a, f.now), view("p4", &f.c, &f.a[..2], f.now)];
    assert_eq!(chosen_tip(&run(&behind, &[attest(ta, 103)], &rules(), &lim, &f.o).unwrap()), ta.header.hash);
    // The same peer twice is one peer.
    let twice = [view("p1", &f.c, &f.a, f.now), view("p1", &f.c, &f.a, f.now)];
    assert!(matches!(run(&twice, &[attest(ta, 103)], &rules(), &lim, &f.o).unwrap(), L2VerdictV1::Unverified(_)));
}

/// **4. A stale checkpoint is refused**: a stale attestation is refused by name (the label falls to HEADER_VERIFIED), a single chain
/// that moved past its attested block by more than the lag bound is not verified, and a stale signed checkpoint is refused before any of it.
#[test]
fn a_stale_attestation_or_checkpoint_is_refused_by_name() {
    let f = fork();
    let ta = f.a.last().unwrap();
    let views = [view("p1", &f.c, &f.a, f.now), view("p4", &f.c, &f.a, f.now)];
    let lim = L2LimitsV1::default();
    // Issued at DAA 100, the view at 103: older than 2 DAA.
    let v = run(&views, &[attest(ta, 100)], &rules(), &lim, &f.o).unwrap();
    assert!(matches!(&v, L2VerdictV1::Unverified(why) if why.contains("Stale")), "{v:?}");
    assert_eq!(mode_label_v1(false, true, true, &v.status()), ModeLabelV1::HeaderVerifiedForkChoiceUnverified);
    assert!(signing_gate_v1(mode_label_v1(false, true, true, &v.status()), None).is_err(), "no signature without the opt-in");
    // Issued in the future of the view: refused.
    assert!(matches!(run(&views, &[attest(ta, 200)], &rules(), &lim, &f.o).unwrap(), L2VerdictV1::Unverified(_)));
    // Fresh, but the tip is two DAA past the attested block (lag bound 1): not verified.
    let v = run(&views, &[attest(&f.a[1], 103)], &rules(), &lim, &f.o).unwrap();
    assert!(matches!(&v, L2VerdictV1::Unverified(why) if why.contains("past the attested block")), "{v:?}");
    // One DAA past it: verified.
    assert_eq!(chosen_tip(&run(&views, &[attest(&f.a[2], 103)], &rules(), &lim, &f.o).unwrap()), ta.header.hash);
    // An untrusted issuer, another ruleset, a forged signature: refused by name.
    let mut untrusted = attest(ta, 103);
    untrusted.attestation.key_id = b"k9".to_vec();
    let mut other_rules = attest(ta, 103);
    other_rules.attestation.consensus_schedule_id = "s2".into();
    let mut forged = attest(ta, 103);
    forged.attestation.committed_root = h(0xF0);
    for (bad, what) in [(untrusted, "not trusted"), (other_rules, "another ruleset"), (forged, "signature")] {
        let v = run(&views, &[bad], &rules(), &lim, &f.o).unwrap();
        assert!(matches!(&v, L2VerdictV1::Unverified(why) if why.contains(what)), "{what}: {v:?}");
    }
    // The signed checkpoint the views start from, stale: refused before any of the above runs.
    let signed = SignedCheckpointV1 {
        checkpoint: Checkpoint { network_id: "testnet-12".into(), daa_score: 100, block_hash: f.c.header.hash },
        issued_at_daa: 100,
        key_id: b"k1".to_vec(),
        signature: Vec::new(),
    };
    let mut signed = signed;
    signed.signature = signed.signing_digest().as_bytes().as_slice()[..3].to_vec();
    assert!(matches!(
        verify_signed_checkpoint(&signed, &trusted_keys(), "testnet-12", 2_000, 500, toy),
        Err(CheckpointError::Stale { .. })
    ));
}

/// **5. A restart, another peer: the same verdict** — the verification is a pure function of the checkpoint, the attestations and the
/// bytes served now; a new peer's doctored copy is refused at L1, whoever served the first.
#[test]
fn a_restarted_client_with_another_peer_reaches_the_same_verdict() {
    let f = fork();
    let (ta, tb) = (f.a.last().unwrap(), f.b.last().unwrap());
    let ev = [attest(ta, 106), attest(tb, 106)];
    let lim = L2LimitsV1::default();
    let first = run(&[view("p1", &f.c, &f.a, f.now), view("p2", &f.c, &f.b, f.now)], &ev, &rules(), &lim, &f.o).unwrap();
    // After a restart the client asks two other peers serving the same DAG.
    let second = run(&[view("q7", &f.c, &f.b, f.now), view("q8", &f.c, &f.a, f.now)], &ev, &rules(), &lim, &f.o).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.trust_line(), second.trust_line());
    // A new peer's doctored copy of A (a header's root swapped) does not pass L1, so it never reaches L2.
    let mut doctored: Vec<Header> = f.a.iter().map(|b| b.header.clone()).collect();
    doctored[2].palw_state_root = h(0xBAD);
    assert!(verify_header_chain_v1(&trusted_cp(&f.c), &doctored, domain(), f.now, &VerifyLimitsV1::default()).is_err());
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The rules around them
// ---------------------------------------------------------------------------------------------------------------------------------

/// **The DNS BFT gate (live testnet-12: the overlay is in Bootstrap, nothing confirmed).** In Bootstrap, or Active with nothing
/// confirmed, the gate never refuses and the comparator decides; an Active anchor below the fork binds both sides alike; an Active anchor
/// on one side only decides instead of the comparator — STOP; an attestation without the facts — STOP.
#[test]
fn the_dns_gate_in_bootstrap_lets_the_comparator_decide_and_a_one_sided_anchor_stops() {
    let f = fork();
    let (ta, tb) = (f.a.last().unwrap(), f.b.last().unwrap());
    let views = [view("p1", &f.c, &f.a, f.now), view("p2", &f.c, &f.b, f.now)];
    let lim = L2LimitsV1::default();
    let gated = ForkChoiceRulesV1 { dns_gate: Some(ForkActivation::new(0)), ..rules() };
    let with = |fact: PalwDnsGateFactV1| [attest_at(ta, 106, Some(fact)), attest_at(tb, 106, Some(fact))];
    let bootstrap = PalwDnsGateFactV1 { stage_active: false, confirmed_anchor: None };
    let v = run(&views, &with(bootstrap), &gated, &lim, &f.o).unwrap();
    assert_eq!(chosen_tip(&v), ta.header.hash, "Bootstrap: the comparator");
    // The mode output names how the gate was accounted for, beside the issuer.
    assert!(matches!(&v, L2VerdictV1::Established { dns_gate: L2DnsGateV1::NeverRefuses, .. }), "{v:?}");
    assert!(v.trust_line().contains("Bootstrap") && v.trust_line().contains("issuer 'k1'"), "{}", v.trust_line());
    let active_none = PalwDnsGateFactV1 { stage_active: true, confirmed_anchor: None };
    assert_eq!(chosen_tip(&run(&views, &with(active_none), &gated, &lim, &f.o).unwrap()), ta.header.hash, "nothing confirmed");
    let common = PalwDnsGateFactV1 { stage_active: true, confirmed_anchor: Some((f.c.header.hash, 100)) };
    let v = run(&views, &with(common), &gated, &lim, &f.o).unwrap();
    assert_eq!(chosen_tip(&v), ta.header.hash, "an anchor both contain");
    assert!(matches!(&v, L2VerdictV1::Established { dns_gate: L2DnsGateV1::AnchorOnEveryCandidate, .. }), "{v:?}");
    let old = PalwDnsGateFactV1 { stage_active: true, confirmed_anchor: Some((h(0x0D), 50)) };
    assert_eq!(chosen_tip(&run(&views, &with(old), &gated, &lim, &f.o).unwrap()), ta.header.hash, "an anchor below the checkpoint");
    let one_sided = PalwDnsGateFactV1 { stage_active: true, confirmed_anchor: Some((f.b[2].header.hash, 102)) };
    assert!(matches!(run(&views, &with(one_sided), &gated, &lim, &f.o), Err(L2StopV1::DnsGateMayDecide(_))));
    assert!(matches!(run(&views, &[attest(ta, 106), attest(tb, 106)], &gated, &lim, &f.o), Err(L2StopV1::DnsGateMayDecide(_))));
    // Retired: the gate never runs, the facts are not needed.
    let retired = ForkChoiceRulesV1 { dns_retired: Some(ForkActivation::new(0)), ..gated };
    let v = run(&views, &[attest(ta, 106), attest(tb, 106)], &retired, &lim, &f.o).unwrap();
    assert_eq!(chosen_tip(&v), ta.header.hash);
    assert!(matches!(&v, L2VerdictV1::Established { dns_gate: L2DnsGateV1::NotRunning, .. }), "{v:?}");
    // A single chain needs no gate facts at all (nothing to refuse).
    let single = [view("p1", &f.c, &f.a, f.now), view("p4", &f.c, &f.a, f.now)];
    let v = run(&single, &[attest(ta, 103)], &gated, &lim, &f.o).unwrap();
    assert_eq!(chosen_tip(&v), ta.header.hash);
    assert!(matches!(&v, L2VerdictV1::Established { dns_gate: L2DnsGateV1::NotNeeded, .. }), "{v:?}");
}

/// An issuer the chain contradicts (the attested block's child commits another root) is refused by name; an opening that does not
/// verify, or a state below the commitment fence, is not a verified value.
#[test]
fn an_issuer_the_chain_contradicts_and_an_opening_that_does_not_hold_are_refused() {
    let f = fork();
    let views = [view("p1", &f.c, &f.a, f.now), view("p4", &f.c, &f.a, f.now)];
    let lim = L2LimitsV1 { max_attested_lag_daa: 3, ..L2LimitsV1::default() };
    let mut lying = attest(&f.a[1], 103);
    lying.opening.leaf.safe_weight = 1_000_000; // the issuer signs a root for inflated keys
    lying.attestation.committed_root = palw_fork_choice_envelope_root_v1(&lying.opening.leaf, &lying.opening.inner_root);
    lying.attestation.signature = lying.attestation.signing_digest().as_bytes().as_slice()[..3].to_vec();
    assert!(matches!(run(&views, &[lying], &rules(), &lim, &f.o), Err(L2StopV1::IssuerContradicted { .. })));
    // A served opening that does not hash to the attested root.
    let mut bad = attest(&f.a[3], 103);
    bad.opening.leaf.bounded_immature += 1;
    assert!(
        matches!(run(&views, &[bad], &rules(), &lim, &f.o).unwrap(), L2VerdictV1::Unverified(why) if why.contains("does not hold"))
    );
    // Below the commitment fence no opening exists: L2 cannot be established.
    let dormant = ForkChoiceRulesV1 { commitment: None, ..rules() };
    let v = run(&views, &[attest(&f.a[3], 103)], &dormant, &lim, &f.o).unwrap();
    assert!(matches!(&v, L2VerdictV1::Unverified(why) if why.contains("below the fence")), "{v:?}");
    assert_eq!(
        PalwForkChoiceOpeningV1 { leaf: f.a[3].leaf, inner_root: f.a[3].inner }.verify(
            &h(1),
            &PalwForkChoicePointV1 { block: f.a[3].header.hash, daa_score: 103, blue_score: 103 },
            None
        ),
        Err(PalwForkChoiceErrorV1::BelowFence { daa: 103 })
    );
    // The shipped rules: the fence is dormant everywhere, so the client stays HEADER_VERIFIED past its checkpoint.
    let shipped = ForkChoiceRulesV1::of(&kaspa_consensus_core::config::params::TESTNET_PARAMS);
    assert_eq!(shipped.commitment, None);
}

/// The finality seal and ADR-0065 D2: what nodes would never weigh, or may veto, is a STOP.
#[test]
fn a_sealed_split_and_an_unbounded_frontier_provenance_veto_stop() {
    let f = fork();
    let (ta, tb) = (f.a.last().unwrap(), f.b.last().unwrap());
    let views = [view("p1", &f.c, &f.a, f.now), view("p2", &f.c, &f.b, f.now)];
    let ev = [attest(ta, 106), attest(tb, 106)];
    let lim = L2LimitsV1::default();
    let shallow_finality = ForkChoiceRulesV1 { finality_depth: 5, ..rules() };
    assert!(matches!(run(&views, &ev, &shallow_finality, &lim, &f.o), Err(L2StopV1::Sealed { .. })));
    let panel = kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2::new(5, 3, 20).unwrap();
    let d2 = ForkChoiceRulesV1 { frontier_provenance: Some(ForkActivation::new(0)), panel: Some(panel), ..rules() };
    // No panel parameters to bound a quorum with: STOP.
    let no_panel = ForkChoiceRulesV1 { panel: None, ..d2 };
    assert!(matches!(run(&views, &ev, &no_panel, &lim, &f.o), Err(L2StopV1::FrontierProvenance(_))));
    // The fork block (the checkpoint) held 8 bonds and A holds 8 — both read off verified leaves: nothing minted, D2 cannot veto.
    assert_eq!(chosen_tip(&run(&views, &ev, &d2, &lim, &f.o).unwrap()), ta.header.hash);
    // A branch that minted 3 bonds could seat a quorum of (5, 3): STOP.
    let minted = branch(&f.c, 3, 4, (60, 100, 0, 11));
    let minted_views = [view("p1", &f.c, &minted, f.now), view("p2", &f.c, &f.b, f.now)];
    let minted_ev = [attest(minted.last().unwrap(), 106), attest(tb, 106)];
    let minted_o = openings(&[&minted, &f.b]);
    assert!(matches!(run(&minted_views, &minted_ev, &d2, &lim, &minted_o), Err(L2StopV1::FrontierProvenance(_))));
    // Without the openings down to the fork block nothing about the paths is read: STOP before any of it.
    assert!(matches!(run(&views, &ev, &d2, &lim, &[]), Err(L2StopV1::SelectedChainUnverified { .. })));
}

/// **A path through a merged block is not the selected chain.** L1 accepts any parent link, and nobody ever checks a merged block's own
/// root — so a peer can route a view through one whose root commits a forged post-state of its parent (another inner root, more bonds).
/// That forged opening hashes to the merged block's root, and is refused anyway: the walk of openings down from the attested tip finds
/// that the tip's root names its real selected parent. At L3 and in a conflict alike; the honest path passes both.
#[test]
fn a_path_through_a_merged_block_with_a_forged_root_is_refused() {
    let c = checkpoint();
    let a = branch(&c, 2, 1, (10, 10, 0, 8)); // [c, a1, a2]
    let (a1, a2) = (&a[1], &a[2]);
    // H: a merged block naming a1 as its parent and committing a FORGED post-state of a1.
    let mut fake = a1.leaf;
    fake.bonds_len = 99;
    let forged = PalwForkChoiceOpeningV1 { leaf: fake, inner_root: h(0xF00) };
    let p1 = &a1.header;
    let hx = header(vec![p1.hash], p1.daa_score + 1, p1.blue_score + 1, p1.timestamp + STEP, forged.committed_root(), 777);
    // T: the attested tip — selected parent a2 (it commits a2's post-state) — merging H.
    let p2 = &a2.header;
    let root_t = palw_fork_choice_envelope_root_v1(&a2.leaf, &a2.inner);
    let tx = header(vec![p2.hash, hx.hash], p2.daa_score + 1, p2.blue_score + 2, p2.timestamp + STEP, root_t, 778);
    let t = Blk { leaf: leaf_of(&tx, (60, 100, 0, 8)), inner: h(0x7777), header: tx };
    let h_blk = Blk { leaf: leaf_of(&hx, (10, 10, 0, 8)), inner: h(0x8888), header: hx.clone() };
    let routed = vec![c.clone(), a1.clone(), h_blk, t.clone()];
    let honest = vec![c.clone(), a1.clone(), a2.clone(), t.clone()];
    let now = t.header.timestamp;
    let point_a1 = PalwForkChoicePointV1 { block: p1.hash, daa_score: p1.daa_score, blue_score: p1.blue_score };
    assert!(forged.verify(&hx.palw_state_root, &point_a1, rules().commitment).is_ok(), "the forged opening opens H's root");
    let mut served = openings(&[&honest]);
    served.push(forged);
    let ev = [attest(&t, t.header.daa_score)];
    let lim = L2LimitsV1::default();

    // L3: on the routed view H is "on the chosen chain", and its forged parent state is refused.
    let routed_views = [view("p1", &c, &routed, now), view("p2", &c, &routed, now)];
    let v = run(&routed_views, &ev, &rules(), &lim, &served).unwrap();
    let why = l3_root_under_l2_v1(&v, &hx, &served, &rules()).unwrap_err();
    assert!(why.contains("not on the attested block's verified selected chain"), "{why}");
    // The honest view: a2 is T's selected parent, and a proof at a2 opens under a1's real inner root.
    let honest_views = [view("p1", &c, &honest, now), view("p2", &c, &honest, now)];
    let v = run(&honest_views, &ev, &rules(), &lim, &served).unwrap();
    assert_eq!(l3_root_under_l2_v1(&v, &a2.header, &served, &rules()), Ok(a1.inner));

    // In a conflict: the routed path's fork point and seal would be read off a path that is not T's selected chain — STOP.
    let b = branch(&c, 4, 2, (50, 500, 1_000, 8));
    let mut served_b = served.clone();
    served_b.extend(openings(&[&b]));
    let tb = b.last().unwrap();
    let ev2 = [attest(&t, tb.header.daa_score), attest(tb, tb.header.daa_score)];
    let now2 = now.max(tb.header.timestamp);
    let conflict = [view("p1", &c, &routed, now2), view("p2", &c, &b, now2)];
    assert!(matches!(
        run(&conflict, &ev2, &rules(), &lim, &served_b),
        Err(L2StopV1::SelectedChainUnverified { tip, .. }) if tip == t.header.hash
    ));
    let conflict = [view("p1", &c, &honest, now2), view("p2", &c, &b, now2)];
    assert_eq!(chosen_tip(&run(&conflict, &ev2, &rules(), &lim, &served_b).unwrap()), t.header.hash);
}

/// **A comparator the v1 leaf cannot feed** (ADR-0178 rule E, its fence in force at the fork point or a tip): a conflict STOPs — the
/// openings do not carry its inputs — while a single chain, which needs no comparator, is still verified.
#[test]
fn a_comparator_whose_inputs_the_leaf_does_not_carry_stops_a_conflict() {
    let f = fork();
    let (ta, tb) = (f.a.last().unwrap(), f.b.last().unwrap());
    let lim = L2LimitsV1::default();
    let e = ForkChoiceRulesV1 { rule_e: Some(ForkActivation::new(0)), ..rules() };
    let views = [view("p1", &f.c, &f.a, f.now), view("p2", &f.c, &f.b, f.now)];
    assert!(matches!(run(&views, &[attest(ta, 106), attest(tb, 106)], &e, &lim, &f.o), Err(L2StopV1::LeafV1Insufficient(_))));
    // In force only at the later tip (106): the fork point and A's tip are below it, a node's incumbent may be at either — still STOP.
    let late = ForkChoiceRulesV1 { rule_e: Some(ForkActivation::new(105)), ..rules() };
    assert!(matches!(run(&views, &[attest(ta, 106), attest(tb, 106)], &late, &lim, &f.o), Err(L2StopV1::LeafV1Insufficient(_))));
    let single = [view("p1", &f.c, &f.a, f.now), view("p4", &f.c, &f.a, f.now)];
    assert_eq!(chosen_tip(&run(&single, &[attest(ta, 103)], &e, &lim, &f.o).unwrap()), ta.header.hash);
}

/// **ADR-0176 D3 — one versioned allocation for every reader.** Where `palw_bond_budget_v1` may be in force the comparator reads the
/// leaf slot's budget-capped weights; this build reads no allocation version, so a conflict there STOPs by name, a single chain whose
/// attested leaf names no allocation at such a point is not verified, and a leaf naming an allocation this build does not read is
/// refused even when the issuer signed its root. Unarmed (every network today), nothing changes.
#[test]
fn a_bond_budget_allocation_is_read_by_one_versioned_slot_or_the_client_stops() {
    let f = fork();
    let (ta, tb) = (f.a.last().unwrap(), f.b.last().unwrap());
    let lim = L2LimitsV1::default();
    let views = [view("p1", &f.c, &f.a, f.now), view("p2", &f.c, &f.b, f.now)];
    let ev = [attest(ta, 106), attest(tb, 106)];
    assert_eq!(chosen_tip(&run(&views, &ev, &rules(), &lim, &f.o).unwrap()), ta.header.hash, "unarmed: the comparator decides");
    let budget = ForkChoiceRulesV1 { bond_budget: Some(ForkActivation::new(106)), ..rules() };
    assert!(matches!(run(&views, &ev, &budget, &lim, &f.o), Err(L2StopV1::BondBudgetAllocation(_))));
    // A single chain past the fence: its attested leaf names no allocation — not the node's leaf there, so L2 is not established.
    let single = [view("p1", &f.c, &f.a, f.now), view("p4", &f.c, &f.a, f.now)];
    let at_101 = ForkChoiceRulesV1 { bond_budget: Some(ForkActivation::new(101)), ..rules() };
    let v = run(&single, &[attest(ta, 103)], &at_101, &lim, &f.o).unwrap();
    assert!(matches!(&v, L2VerdictV1::Unverified(why) if why.contains("names no bond-budget allocation")), "{v:?}");
    // Below the fence the same chain is verified.
    assert_eq!(chosen_tip(&run(&single, &[attest(ta, 103)], &budget, &lim, &f.o).unwrap()), ta.header.hash);
    // A leaf that names allocation version 1, under a root the issuer signed: refused (this build does not read version 1), never
    // weighed by the fold's weights.
    let mut tip = ta.clone();
    tip.leaf.weight_allocation =
        PalwWeightAllocationSlotV1 { version: 1, allocation_root: h(0xA0), capped_safe_weight: 1, capped_bounded_immature: 0 };
    let a2: Vec<Blk> = f.a[..f.a.len() - 1].iter().cloned().chain(std::iter::once(tip.clone())).collect();
    let v = run(&[view("p1", &f.c, &a2, f.now), view("p4", &f.c, &a2, f.now)], &[attest(&tip, 103)], &at_101, &lim, &f.o).unwrap();
    assert!(matches!(&v, L2VerdictV1::Unverified(why) if why.contains("allocation version 1")), "{v:?}");
}

/// The robust evaluation is the node's own functions: a strict economic winner dominates under every variant; a hash-only winner
/// dominates only where strict-win cannot be in force.
#[test]
fn robust_dominance_is_the_intersection_of_the_in_force_rules() {
    let o = |f: u64, s: u128, i: u128, c: u64| PalwCandidateOrderV1::new(f, s, i, h(c));
    let strict = ForkChoiceRulesV1 { strict_win: Some(ForkActivation::new(0)), ..rules() };
    assert!(robustly_dominates_v1(&o(60, 100, 0, 1), &o(50, 500, 1000, 2), &rules(), &[103, 106]));
    assert!(robustly_dominates_v1(&o(60, 100, 0, 1), &o(50, 500, 1000, 2), &strict, &[103, 106]));
    assert!(!robustly_dominates_v1(&o(50, 500, 1000, 2), &o(60, 100, 0, 1), &strict, &[103, 106]));
    // An all-economic tie won on the hash: the plain rule says yes, strict-win (either tie answer) does not agree.
    let (hi, lo) = (o(5, 5, 5, 9), o(5, 5, 5, 1));
    assert!(robustly_dominates_v1(&hi, &lo, &rules(), &[1]));
    assert!(!robustly_dominates_v1(&hi, &lo, &strict, &[1]));
    // Strict-win armed at only one of the two tips' DAA: both variants must agree.
    let straddle = ForkChoiceRulesV1 { strict_win: Some(ForkActivation::new(105)), ..rules() };
    assert!(!robustly_dominates_v1(&hi, &lo, &straddle, &[103, 106]));
}

/// The attestation digest covers every field (a field changed after signing breaks the signature).
#[test]
fn the_attestation_digest_covers_every_field() {
    let f = fork();
    let base =
        attest_at(f.a.last().unwrap(), 106, Some(PalwDnsGateFactV1 { stage_active: false, confirmed_anchor: None })).attestation;
    let d = base.signing_digest();
    let edits: Vec<Box<dyn Fn(&mut ForkChoiceAttestationV1)>> = vec![
        Box::new(|a| a.network_id.push('x')),
        Box::new(|a| a.consensus_params_id.push('x')),
        Box::new(|a| a.consensus_schedule_id.push('x')),
        Box::new(|a| a.block = h(77)),
        Box::new(|a| a.block_daa += 1),
        Box::new(|a| a.committed_root = h(78)),
        Box::new(|a| a.leaf_version += 1),
        Box::new(|a| a.dns_gate = None),
        Box::new(|a| a.dns_gate = Some(PalwDnsGateFactV1 { stage_active: true, confirmed_anchor: None })),
        Box::new(|a| a.dns_gate = Some(PalwDnsGateFactV1 { stage_active: false, confirmed_anchor: Some((h(1), 1)) })),
        Box::new(|a| a.issued_at_daa += 1),
        Box::new(|a| a.key_id.push(1)),
    ];
    for (i, edit) in edits.iter().enumerate() {
        let mut a = base.clone();
        edit(&mut a);
        assert_ne!(a.signing_digest(), d, "edit {i} must move the digest");
    }
}

/// **Cost per verified view on testnet-12-sized inputs** (MEASURED, ignored — run alone: `cargo test -p misaka-palw-remote --lib
/// l2fc_cost -- --ignored --nocapture`): L1 over a cold start of 1,000 headers, one refresh of 3 headers per peer, the L2 verdict with
/// two weighed candidates, one real ML-DSA-87 verification, and the bytes on the wire.
#[test]
#[ignore]
fn l2fc_cost_per_view() {
    use std::time::Instant;
    let c = checkpoint();
    let long = branch(&c, 999, 5, (60, 100, 0, 8));
    let now = long.last().unwrap().header.timestamp;
    let headers: Vec<Header> = long.iter().map(|b| b.header.clone()).collect();
    let cp = trusted_cp(&c);
    let lim = VerifyLimitsV1 { max_headers: 2_000, ..VerifyLimitsV1::default() };
    let t = Instant::now();
    let reps = 5;
    for _ in 0..reps {
        verify_header_chain_v1(&cp, &headers, domain(), now, &lim).unwrap();
    }
    let cold = t.elapsed() / reps;
    // One ML-DSA-87 verification (an attempt header's carriage, or an attestation).
    let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([9u8; 32]);
    let msg = [5u8; 64];
    let sig = libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, &msg, b"ctx", [1u8; 32]).unwrap();
    let t = Instant::now();
    let n = 200;
    for _ in 0..n {
        assert!(kaspa_txscript::verify_mldsa87_with_context(kp.verification_key.as_ref(), &msg, sig.as_ref(), b"ctx").unwrap());
    }
    let mldsa = t.elapsed() / n;
    // One L2 verdict with two weighed candidates.
    let f = fork();
    let views = [view("p1", &f.c, &f.a, f.now), view("p2", &f.c, &f.b, f.now)];
    let ev = [attest(f.a.last().unwrap(), 106), attest(f.b.last().unwrap(), 106)];
    let t = Instant::now();
    let n = 2_000;
    for _ in 0..n {
        run(&views, &ev, &rules(), &L2LimitsV1::default(), &f.o).unwrap();
    }
    let l2 = t.elapsed() / n;
    let header_bytes = |x: &Header| borsh::to_vec(x).map(|v| v.len()).unwrap_or(0);
    let heartbeat = header_bytes(&f.a[1].header);
    let opening = kaspa_consensus_core::palw_fork_choice_commitment_v1::PALW_FORK_CHOICE_LEAF_BYTES_V1 + 64;
    println!("L2FC-COST l1_cold_1000_headers={cold:?} per_header={:?}", cold / 1000);
    println!("L2FC-COST mldsa87_verify={mldsa:?}");
    println!("L2FC-COST l2_verdict_two_candidates={l2:?}");
    println!("L2FC-COST heartbeat_header_borsh_bytes={heartbeat} opening_bytes={opening}");
    // Option A's per-block cost: `state_root()` over a state of testnet-12's carriage size (bonds with 2.6 KB keys, ~57 MB).
    let params = PalwStateParamsV2::new(100, 1, 1, 1, 500, 1_000, h(1), 4, 1_000, 100, 1_000, 0).unwrap();
    let bonds: Vec<Obj> = (0..20_000u64)
        .map(|i| Obj::BondRegistered {
            bond: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0x10_0000 + i), 0)),
            pubkey: [i.to_le_bytes().to_vec(), vec![(i % 251) as u8; 2584]].concat(),
            operator_pubkey: i.to_le_bytes().to_vec(),
            collateral: 1 << 40,
            payout_payload: h(0x9A),
            capable_classes: Default::default(),
            signature: Vec::new(),
        })
        .collect();
    let cx = PalwBlockContextV2 { block: h(10), daa_score: 1, blue_score: 1, subsidy: 0 };
    let (state, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &params, &cx, &bonds, None).unwrap();
    let bond_table: usize = prove_bonds_v1(&state).collection.rows.iter().map(|(k, v)| k.len() + v.len()).sum();
    let t = Instant::now();
    let root = state.state_root();
    let root_time = t.elapsed();
    let t = Instant::now();
    let opening = kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwForkChoiceOpeningV1::of(&state).unwrap();
    let leaf_time = t.elapsed();
    assert_eq!(opening.inner_root, root);
    println!(
        "L2FC-COST synthetic_state bonds=20000 bond_table_bytes={bond_table} state_root={root_time:?} opening_of_state={leaf_time:?} preimage_bytes={}",
        state.state_root_preimage().len()
    );
}
