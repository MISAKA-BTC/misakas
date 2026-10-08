//! **RFC-0012 D1 — attacks on the maturity offset, through the REAL fold on testnet-12's own `Params`.**
//!
//! The question (docs/design/palw/rfc-0012-policy-proposal.md §10): what would `safe` do if the evidence rule counted work at
//! `Final + 3,000`, `Final + 1,000` or `Final + 600` instead of v1's 5,400 after acceptance? Nothing here changes the shipped rule.
//! v1 is the only rule in the tree; the three alternatives exist **only in this file**, as a function that re-times the fact the real
//! conversion produced (`native_facts_of_block_v1`) and moves the lifecycle closure to the same instant.
//!
//! **What is real.** Every block is `apply_palw_transition_v7`: the claim reaches `Final`, a conviction (`PanelFalseValid` with an
//! executor equivocation) or a data-availability default reverses it, the sweep retires it. The extraction of evidence from the fold's
//! own deltas (`native_delta_evidence_v1`), the fact conversion, the lifecycle closure (`native_open_from_v1`) and the certificate
//! (`certify_native_prefix_v1`) are the shipped functions.
//!
//! **What is a seam, named.**
//! * The chain is one block per DAA, `blue == DAA`, linear: there is no DAG, no fork choice, no PoW. A "private fork" is therefore not
//!   raced here (that is a PALW common-prefix question this branch does NOT prove); what is shown is what the certificate does when a
//!   branch carrying a reversal arrives, and what the node's own reorg bound does with the depth (see the arithmetic test).
//! * The fixture can only drive the floor claim without the class registry, so the fact is converted from the same record relabelled to a
//!   REAL class (as `rfc0012_native_evidence_fold` does).
//! * The policy is `D = 1, W = 1, no cap`: it isolates TIME. A real policy adds anchors (`+F_off = 123` DAA each) on top of every lag below.
//! * One claim, one effect that matters (the accepting block at DAA 1,001); effects exist at every DAA from 1,000.
//!
//! Run: cargo test -p kaspa-consensus-core --test rfc0012_safe_maturity_attacks -- --nocapture --test-threads=1

#[path = "dos_l5_common.rs"]
mod common;
use common::*;
#[path = "rfc0012_fold_fixture/mod.rs"]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_native_settlement_v1::{
    MatureUsefulWorkV1, NativeDeltaEvidenceV1, NativeEffectV1, PalwSettlementPolicyV1, certify_native_prefix_v1,
    native_delta_evidence_v1, native_facts_of_block_v1, native_open_from_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateV2Error, PalwVoidReasonV2,
};
use std::collections::{BTreeMap, BTreeSet};

/// The rule that times the evidence. `V1` is the shipped one; the others are this file's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    /// `max(trace_retention, Final + claim_retirement)` — the shipped rule (5,400 after acceptance for an ordinary claim).
    V1,
    /// `Final + X`, closure at the same instant.
    FinalPlus(u64),
}

const RULES: [Rule; 4] = [Rule::V1, Rule::FinalPlus(3_000), Rule::FinalPlus(1_000), Rule::FinalPlus(600)];

impl Rule {
    fn name(self) -> String {
        match self {
            Rule::V1 => "v1 (5,400 after acceptance)".into(),
            Rule::FinalPlus(x) => format!("Final + {x}"),
        }
    }
}

/// What one block of the claim's life shows, as the node would read it from the sink at that block.
#[derive(Clone, Debug)]
struct Obs {
    daa: u64,
    /// `None`: the claim is no longer in state (retired).
    phase: Option<PalwClaimPhaseV2>,
    frontier_blue: u64,
    /// A DA session is open on the claim.
    open_da: bool,
    /// The claim has been voided by a delta at or before this block (cumulative).
    voided: bool,
}

fn observe(
    l: &Life,
    s: &PalwChainStateV2,
    daa: u64,
    delta: &kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2,
    prior_void: bool,
) -> Obs {
    Obs {
        daa,
        phase: s.claim(&l.id).map(|c| c.phase.clone()),
        frontier_blue: s.safe_frontier().0,
        open_da: s.da_sessions_iter().any(|((claim, _), _)| *claim == l.id),
        voided: prior_void || native_delta_evidence_v1(delta).voided.contains(&l.id),
    }
}

/// One step of the claim's life from `from` (the state AFTER block `from_daa`), with `events` folded into the block at their DAA.
/// `keep` states are returned for later branching. A refusal ends the walk and is returned with the DAA.
fn walk(
    l: &Life,
    from: &PalwChainStateV2,
    from_daa: u64,
    to_daa: u64,
    events: &BTreeMap<u64, Vec<PalwConsensusObjectV2>>,
    keep: &BTreeSet<u64>,
    prior_void: bool,
) -> (Vec<Obs>, BTreeMap<u64, PalwChainStateV2>, Option<(u64, PalwStateV2Error)>) {
    let (mut obs, mut kept) = (Vec::new(), BTreeMap::new());
    let mut s = from.clone();
    let mut void = prior_void;
    if keep.contains(&from_daa) {
        kept.insert(from_daa, s.clone());
    }
    for daa in from_daa + 1..=to_daa {
        let objects = events.get(&daa).cloned().unwrap_or_default();
        match try_step(&l.p, &l.sp, &s, daa, &objects, PalwBlockWorkV3::None, Hash64::default(), 0, true) {
            Ok((next, delta)) => {
                let o = observe(l, &next, daa, &delta, void);
                void = o.voided;
                obs.push(o);
                s = next;
                if keep.contains(&daa) {
                    kept.insert(daa, s.clone());
                }
            }
            Err(e) => return (obs, kept, Some((daa, e))),
        }
    }
    (obs, kept, None)
}

/// The fact the real conversion makes of the claim's `Final` (relabelled to a REAL class), under v1.
fn v1_fact(l: &Life) -> MatureUsefulWorkV1 {
    let finalized = native_delta_evidence_v1(&l.final_delta);
    let real = genesis_classes(&l.p)[1].0;
    let mut relabelled = NativeDeltaEvidenceV1::default();
    relabelled.finalized_attempts.push((l.id, {
        let mut c = finalized.finalized_attempts[0].1.clone();
        c.class_id = real;
        c
    }));
    let open = BTreeSet::new();
    let facts = native_facts_of_block_v1(
        &rules(l, &l.after_retirement, &open),
        &relabelled,
        &BTreeSet::new(),
        (l.accepted_daa, l.accepted_daa),
    );
    assert_eq!(facts.len(), 1, "one fact");
    facts[0]
}

fn policy() -> PalwSettlementPolicyV1 {
    PalwSettlementPolicyV1 { settled_anchor_depth: 1, unique_mature_work: 1, max_operator_permille: 1000, max_class_permille: 1000 }
}

/// Is the claim an unresolved obligation at this block, under `rule`? v1 asks the SHIPPED `native_open_from_v1` (a `Final` claim is open
/// until its retention lapses, a retired or voided one is resolved, a DA session keeps it open); the alternatives close a `Final` claim
/// at `Final + X` instead and are this file's.
fn claim_open(l: &Life, rule: Rule, o: &Obs) -> bool {
    match rule {
        Rule::V1 => {
            let claims = o.phase.iter().map(|p| (l.accepted_daa, p.clone(), l.retention_daa));
            let sessions = o.open_da.then_some(Some(l.accepted_daa));
            native_open_from_v1(claims, sessions, o.daa) != u64::MAX
        }
        Rule::FinalPlus(x) => {
            o.open_da
                || match &o.phase {
                    None | Some(PalwClaimPhaseV2::Voided { .. }) => false,
                    Some(PalwClaimPhaseV2::Final { final_daa }) => o.daa < final_daa + x,
                    Some(_) => true,
                }
        }
    }
}

/// **The DAA of the newest effect the certificate calls `safe` at this block, under `rule`** (`None`: nothing certifies).
fn safe_daa(l: &Life, fact: &MatureUsefulWorkV1, rule: Rule, o: &Obs) -> Option<u64> {
    let open_from = if claim_open(l, rule, o) { l.accepted_daa } else { u64::MAX };
    let effects: Vec<NativeEffectV1> = (l.accepted_daa - 1..=o.daa)
        .map(|d| NativeEffectV1 { daa: d, blue: d, frontier_covers: o.frontier_blue >= d, lifecycle_closed: d < open_from })
        .collect();
    let mut f = *fact;
    if let Rule::FinalPlus(x) = rule {
        f.matured_daa = l.final_daa + x;
    }
    // A voided claim, and a claim with an open DA session, are never evidence (native_facts_of_block_v1).
    let facts: Vec<MatureUsefulWorkV1> = if o.voided || o.open_da { Vec::new() } else { vec![f] };
    let prefix = certify_native_prefix_v1(policy(), &effects, o.daa, &facts);
    prefix.safe.map(|i| l.accepted_daa - 1 + i as u64)
}

/// The first DAA at which `safe` reaches the claim's accepting block (or past it), over an honest walk.
fn first_safe(l: &Life, fact: &MatureUsefulWorkV1, rule: Rule, honest: &[Obs]) -> Option<u64> {
    honest.iter().find(|o| safe_daa(l, fact, rule, o).is_some_and(|d| d >= l.accepted_daa)).map(|o| o.daa)
}

fn offsets(l: &Life, base: &[u64]) -> BTreeSet<u64> {
    base.iter().map(|k| l.final_daa + k).collect()
}

// ---------------------------------------------------------------------------------------------------------------------------------

/// **EXPECTED (written before the run).** Without any attack, `safe` first reaches the accepting block exactly when BOTH the fact is
/// mature and the claim's closure has passed: v1 at `accepted + 5,400` (retention; it is later than the retirement at `Final + 3,001`),
/// `Final + 3,000` at the retirement-side closure `Final + 3,000`, and the two short rules at `Final + 1,000` / `Final + 600`
/// although the claim is still in state (that is the point of the alternative). Before that moment `safe` does not include it.
#[test]
fn rfc0012_d1_a_without_an_attack_safe_covers_the_claim_exactly_at_the_rules_instant() {
    let l = live_life();
    let fact = v1_fact(&l);
    let last = l.accepted_daa + 6_600;
    let (honest, _, refused) = walk(&l, &l.at_final, l.final_daa, last, &BTreeMap::new(), &BTreeSet::new(), false);
    assert!(refused.is_none(), "{refused:?}");
    eprintln!(
        "[d1-a] claim accepted {} Final {} (+{}) retired {} (Final+{}), retention lapses {} (accepted+{})",
        l.accepted_daa,
        l.final_daa,
        l.final_daa - l.accepted_daa,
        l.retired_daa,
        l.retired_daa - l.final_daa,
        l.retention_daa,
        l.retention_daa - l.accepted_daa
    );
    eprintln!("[d1-a] frontier after Final = blue {}", honest[0].frontier_blue);
    for rule in RULES {
        let at = first_safe(&l, &fact, rule, &honest).expect("safe reaches the claim");
        eprintln!(
            "[d1-a] {:<28} safe covers the accepting block at DAA {at} = accepted + {} = Final + {}",
            rule.name(),
            at - l.accepted_daa,
            at - l.final_daa
        );
        match rule {
            Rule::V1 => assert_eq!(at, l.retention_daa, "v1: the retention (acceptance + 5,400)"),
            Rule::FinalPlus(x) => assert_eq!(at, l.final_daa + x, "Final + {x}: the rule's own instant"),
        }
        // Not a DAA before.
        let before = honest.iter().find(|o| o.daa == at - 1).unwrap();
        assert!(safe_daa(&l, &fact, rule, before).is_none_or(|d| d < l.accepted_daa), "{}: not before", rule.name());
    }
}

/// **EXPECTED.** The last DAA at which the REAL fold still reverses a `Final` claim, by objective conviction. The claim leaves state at
/// `Final + claim_retirement` (the sweep's terminal arm), so a conviction in a block after that cannot void it
/// (`reverse_convicted_final` finds no claim) while a conviction in a block up to that sweep still can. The sweep below finds the
/// boundary by running the real fold; the assertions pin what it found and the relation to `claim_retirement`.
#[test]
fn rfc0012_d1_b_the_last_daa_a_final_can_be_convicted_is_the_retirement() {
    let l = live_life();
    let retirement = l.sp.claim_retirement_daa();
    let ks: Vec<u64> = vec![
        1, 5, 100, 599, 600, 601, 999, 1_000, 1_001, 1_500, 2_000, 2_500, 2_900, 2_990, 2_998, 2_999, 3_000, 3_001, 3_002, 3_003,
        3_004, 3_010,
    ];
    let keep: BTreeSet<u64> = ks.iter().map(|k| l.final_daa + k - 1).collect();
    let (_, kept, refused) = walk(&l, &l.at_final, l.final_daa, l.final_daa + 3_010, &BTreeMap::new(), &keep, false);
    assert!(refused.is_none());
    let mut last_reversed = 0;
    let mut first_not = None;
    for k in &ks {
        let parent = &kept[&(l.final_daa + k - 1)];
        let in_state_before = parent.claim(&l.id).is_some();
        let res = try_step(
            &l.p,
            &l.sp,
            parent,
            l.final_daa + k,
            &[false_valid(&l, l.valid_seats[0])],
            PalwBlockWorkV3::None,
            Hash64::default(),
            0,
            true,
        );
        let outcome = match &res {
            Ok((s, delta)) => {
                let voided = native_delta_evidence_v1(delta).voided.contains(&l.id);
                format!(
                    "folds; claim before={in_state_before}; void in delta={voided}; claim after={:?}",
                    s.claim(&l.id).map(|c| c.phase.clone())
                )
            }
            Err(e) => format!("REFUSED {e:?}"),
        };
        eprintln!("[d1-b] conviction in the block at Final+{k:<5} {outcome}");
        let reversed = matches!(&res, Ok((_, d)) if native_delta_evidence_v1(d).voided.contains(&l.id));
        if reversed {
            last_reversed = *k;
        } else if first_not.is_none() {
            first_not = Some(*k);
        }
    }
    eprintln!(
        "[d1-b] last reversing conviction: Final+{last_reversed}; first that does not: Final+{first_not:?}; claim_retirement = {retirement}; retired at Final+{}",
        l.retired_daa - l.final_daa
    );
    assert!(last_reversed >= 2_999, "the court window is honoured to the end: Final+{last_reversed}");
    assert!(last_reversed <= retirement + 2, "and no later than the retirement (+ the sweep's own block): Final+{last_reversed}");
    assert!(first_not.is_some_and(|k| k > last_reversed), "a boundary exists inside the sweep");
}

/// **EXPECTED (written before the run, and WRONG in one respect — recorded).** The same boundary for the DATA-AVAILABILITY channel
/// (material withholding). A `Final` claim is accusable only while it is held and its vesting row is unmatured; the session lasts
/// `W_disclose` (1,200) and a default reverses the `Final` (`reverse_convicted_final(ProducerWithholding)`). The expectation was that a
/// session is cut off when the claim retires, so a default lands by `Final + 3,000`. **The real fold says otherwise:** the last
/// accusation is admitted in the block at `Final + claim_retirement` (the claim is gone from the next block: `MissingClaim`), and an
/// open session HOLDS the claim past its retirement — the default lands `W_disclose + 1` blocks after the accusation, i.e. as late as
/// `Final + 4,201`. The policy proposal's 10.2 said "by F + 3,000" from reading the code; this is the measurement that corrects it.
#[test]
fn rfc0012_d1_c_the_last_daa_a_withheld_final_can_be_accused_and_when_the_default_lands() {
    let l = live_life();
    let retirement = l.sp.claim_retirement_daa();
    let disclose = kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(&l.sp);
    let accuser = l.valid_seats[0];
    let accs: Vec<u64> = vec![5, 600, 1_000, 1_500, 1_799, 1_800, 1_801, 1_900, 2_500, 2_998, 2_999, 3_000, 3_001];
    let keep: BTreeSet<u64> = accs.iter().map(|a| l.final_daa + a - 1).collect();
    let (_, kept, refused) = walk(&l, &l.at_final, l.final_daa, l.final_daa + 3_010, &BTreeMap::new(), &keep, false);
    assert!(refused.is_none());
    let mut last_admitted = 0;
    let mut last_defaulted = None;
    for a in &accs {
        let parent = &kept[&(l.final_daa + a - 1)];
        let event = BTreeMap::from([(
            l.final_daa + a,
            vec![PalwConsensusObjectV2::DefaultAccused { claim: l.id, missing_event_index: 0, accuser, signature: vec![] }],
        )]);
        let (obs, _, refused) = walk(&l, parent, l.final_daa + a - 1, l.final_daa + a + disclose + 3, &event, &BTreeSet::new(), false);
        match refused {
            Some((daa, e)) => {
                eprintln!("[d1-c] accusation at Final+{a:<5} REFUSED at DAA Final+{}: {e:?}", daa - l.final_daa);
                assert!(*a > retirement, "only an accusation after the retirement is refused (Final+{a})");
                assert!(matches!(e, PalwStateV2Error::MissingClaim(_)), "and by name: {e:?}");
            }
            None => {
                last_admitted = *a;
                let session_blocks = obs.iter().filter(|o| o.open_da).count();
                let end = obs.last().unwrap();
                let defaulted = obs.iter().find(|o| o.voided).map(|o| o.daa - l.final_daa);
                eprintln!(
                    "[d1-c] accusation at Final+{a:<5} admitted; session open for {session_blocks} blocks; reversed at Final+{defaulted:?}; at the end claim={:?}",
                    end.phase
                );
                assert_eq!(defaulted, Some(a + disclose + 1), "the default lands W_disclose + 1 blocks after the accusation");
                assert_eq!(session_blocks as u64, disclose + 1);
                assert!(
                    matches!(end.phase, Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. })),
                    "reversed as a WITHHOLDING, not a proof of fraud"
                );
                last_defaulted = Some(*a + disclose + 1);
            }
        }
    }
    eprintln!(
        "[d1-c] last admitted accusation: Final+{last_admitted}; the latest default it produces lands at Final+{last_defaulted:?}; claim_retirement {retirement}; W_disclose {disclose}"
    );
    assert_eq!(last_admitted, retirement, "accusable through the block at Final + claim_retirement");
    assert_eq!(
        last_defaulted,
        Some(retirement + disclose + 1),
        "the DA reversal horizon is retirement + W_disclose + 1, not retirement"
    );
}

/// **EXPECTED.** What `safe` does while material is withheld, per rule. An honest accuser files `DefaultAccused` at `Final + a`; the
/// session holds the claim open (`claims_with_open_da` removes its fact, `native_open_from_v1` keeps the lifecycle open) for
/// `W_disclose + 1` blocks and then the default voids it.
/// * a rule that had NOT yet counted the claim (`a <= X`) never counts it: held, then dropped when the default lands;
/// * a rule that HAD counted it (`a > X`, only the short rules) loses `safe` at the accusation, not at the default.
/// No rule counts the claim at any block of the session, and no rule counts it after the default.
#[test]
fn rfc0012_d1_e_withheld_material_holds_safe_for_the_session_and_a_short_rule_loses_it_at_the_accusation() {
    let l = live_life();
    let fact = v1_fact(&l);
    let disclose = kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(&l.sp);
    let accuser = l.valid_seats[0];
    let accs: Vec<u64> = vec![50, 590, 700, 1_000, 1_500, 2_999, 3_000];
    let keep: BTreeSet<u64> = accs.iter().map(|a| l.final_daa + a - 1).collect();
    let (honest, kept, _) = walk(&l, &l.at_final, l.final_daa, l.final_daa + 3_010, &BTreeMap::new(), &keep, false);
    let by_daa: BTreeMap<u64, &Obs> = honest.iter().map(|o| (o.daa, o)).collect();
    let covers = |rule: Rule, o: &Obs| safe_daa(&l, &fact, rule, o).is_some_and(|d| d >= l.accepted_daa);
    for a in &accs {
        let parent = &kept[&(l.final_daa + a - 1)];
        let event = BTreeMap::from([(
            l.final_daa + a,
            vec![PalwConsensusObjectV2::DefaultAccused { claim: l.id, missing_event_index: 0, accuser, signature: vec![] }],
        )]);
        let (branch, _, refused) =
            walk(&l, parent, l.final_daa + a - 1, l.final_daa + a + disclose + 3, &event, &BTreeSet::new(), false);
        assert!(refused.is_none(), "{refused:?}");
        let before = by_daa[&(l.final_daa + a - 1)];
        for rule in RULES {
            let counted_before = covers(rule, before);
            let during: Vec<bool> = branch.iter().filter(|o| o.open_da).map(|o| covers(rule, o)).collect();
            let after_default: Vec<bool> = branch.iter().filter(|o| !o.open_da && o.voided).map(|o| covers(rule, o)).collect();
            eprintln!(
                "[d1-e] {:<28} accusation at Final+{a:<5} counted-before={counted_before:<5} counted-during-session={} (over {} blocks) counted-after-default={}",
                rule.name(),
                during.iter().any(|c| *c),
                during.len(),
                after_default.iter().any(|c| *c)
            );
            assert!(
                !during.iter().any(|c| *c),
                "{} at Final+{a}: no rule counts a claim whose material is under accusation",
                rule.name()
            );
            assert!(
                !after_default.iter().any(|c| *c),
                "{} at Final+{a}: and none counts it once the default has voided it",
                rule.name()
            );
            let instant = match rule {
                Rule::V1 => l.retention_daa - l.final_daa,
                Rule::FinalPlus(x) => x,
            };
            assert_eq!(
                counted_before,
                *a > instant,
                "{} at Final+{a}: counted before the accusation iff it is past the rule's instant",
                rule.name()
            );
        }
    }
}

/// **The reorg bound the node already has, against the lag each rule gives `safe`.** A private branch can only displace what `safe`
/// stands on if the node would switch to it; the node refuses a sink candidate that does not contain its finality point
/// (`sink_search`, `virtual_finality_point`), `finality_depth = window_challenge / 2` blue score (600 on t12). Every rule's `safe` lags
/// the sink by at least `F_off + X` DAA (before the D anchors a real policy adds); the table is printed and the relation to the bound is
/// asserted in DAA (the harness has `blue == DAA`; on a real chain it holds if blue score grows at least `600 / lag` per DAA).
/// What this does NOT show: that the branch cannot be built (PALW common-prefix security is NOT proven here), nor a node that was
/// offline or eclipsed through the lag (its own finality point is older).
#[test]
fn rfc0012_d1_f_the_lag_of_each_rule_against_the_nodes_own_reorg_bound() {
    let l = live_life();
    let finality_depth = bundle(&l.p).state.window_challenge() / 2;
    let f_off = l.final_daa - l.accepted_daa;
    eprintln!("[d1-f] node finality depth = {finality_depth} blue score; F_off (honest quorum) = {f_off} DAA");
    for rule in RULES {
        let lag = match rule {
            Rule::V1 => l.retention_daa - l.accepted_daa,
            Rule::FinalPlus(x) => f_off + x,
        };
        eprintln!(
            "[d1-f] {:<28} safe lag from acceptance >= {lag} DAA = {:.2} x the finality depth; at 24 DAA/h {:.1} h, at 30 DAA/h {:.1} h",
            rule.name(),
            lag as f64 / finality_depth as f64,
            lag as f64 / 24.0,
            lag as f64 / 30.0
        );
        assert!(lag > finality_depth, "{}: `safe` sits deeper than the node's own reorg bound", rule.name());
    }
    assert_eq!(finality_depth, 600);
}

/// **EXPECTED (collusion).** Producer + every seat of the panel colluded (`live_life`: five `Valid` receipts for an execution whose executor
/// equivocated) and the claim is `Final` at +123. An outside party holds the equivocation evidence and files the conviction in the block
/// at `Final + k`. For each rule and each `k` the certificate is read the block before and the block of the conviction:
/// * `k` below the rule's instant: `safe` never counted the claim — caught in time;
/// * the rule's instant `<= k <=` the last reversible DAA: `safe` HAD covered the accepting block and the conviction takes it back —
///   `safe` retreats (the claim is retracted by the voided set);
/// * `k` past the last reversible DAA (the claim retired): the conviction cannot reverse it; for EVERY rule the work stays counted.
#[test]
fn rfc0012_d1_d_a_colluding_producer_and_panel_is_caught_in_time_only_if_safe_waits_for_the_court() {
    let l = live_life();
    let fact = v1_fact(&l);
    let ks: Vec<u64> =
        vec![50, 590, 599, 600, 601, 610, 990, 999, 1_000, 1_001, 1_010, 2_000, 2_990, 2_999, 3_000, 3_001, 3_002, 3_050];
    let keep: BTreeSet<u64> = ks.iter().map(|k| l.final_daa + k - 1).collect();
    let last = l.final_daa + 3_060;
    let (honest, kept, _) = walk(&l, &l.at_final, l.final_daa, last, &BTreeMap::new(), &keep, false);
    let by_daa: BTreeMap<u64, &Obs> = honest.iter().map(|o| (o.daa, o)).collect();
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    enum Verdict {
        CaughtBeforeSafe,
        SafeRetracted,
        NotReversible,
    }
    // The rule's instant, in DAA after `Final`: the first block at which it counts the claim.
    let instant = |rule: Rule| match rule {
        Rule::V1 => l.retention_daa - l.final_daa,
        Rule::FinalPlus(x) => x,
    };
    let mut table: BTreeMap<(String, u64), Verdict> = BTreeMap::new();
    let mut reversible: BTreeMap<u64, bool> = BTreeMap::new();
    for k in &ks {
        let parent = &kept[&(l.final_daa + k - 1)];
        let event = BTreeMap::from([(l.final_daa + k, vec![false_valid(&l, l.valid_seats[0])])]);
        let (branch, _, refused) = walk(&l, parent, l.final_daa + k - 1, l.final_daa + k + 3, &event, &BTreeSet::new(), false);
        // A conviction the fold REFUSES (the claim is gone) reverses nothing: the chain simply does not move.
        let reversed = refused.is_none() && branch.iter().any(|o| o.voided);
        reversible.insert(*k, reversed);
        let before = by_daa[&(l.final_daa + k - 1)];
        let after = branch.first().unwrap_or(before);
        for rule in RULES {
            let safe_before = safe_daa(&l, &fact, rule, before).is_some_and(|d| d >= l.accepted_daa);
            let safe_after = safe_daa(&l, &fact, rule, after).is_some_and(|d| d >= l.accepted_daa);
            let verdict = match (reversed, safe_before, safe_after) {
                (false, _, _) => Verdict::NotReversible,
                (true, false, _) => Verdict::CaughtBeforeSafe,
                (true, true, false) => Verdict::SafeRetracted,
                (true, true, true) => panic!("{} k={k}: a reversal that safe did not notice", rule.name()),
            };
            eprintln!(
                "[d1-d] {:<28} conviction at Final+{k:<5} reversed={reversed:<5} refused={:<5} safe-before={safe_before:<5} safe-after={safe_after:<5} -> {verdict:?}",
                rule.name(),
                refused.is_some()
            );
            // The certificate and the fold agree with the arithmetic: the claim counted at the block before iff the conviction's
            // block is past the rule's instant.
            assert_eq!(
                safe_before,
                *k > instant(rule),
                "{} at Final+{k}: counted before the conviction iff k > the instant",
                rule.name()
            );
            table.insert((rule.name(), *k), verdict);
        }
    }
    let v = |rule: Rule, k: u64| table[&(rule.name(), k)];
    let last_reversible = *ks.iter().filter(|k| reversible[*k]).max().unwrap();
    eprintln!("[d1-d] last reversible conviction in this sweep: Final+{last_reversible}");
    // v1: every reversible conviction is caught before `safe` counts the claim (its instant is Final+5,277).
    for k in ks.iter().filter(|k| reversible[*k]) {
        assert_eq!(v(Rule::V1, *k), Verdict::CaughtBeforeSafe, "v1 at {k}");
    }
    // Final+3,000: caught up to and including the conviction in the instant's own block; a conviction one block LATER that the fold still
    // accepts would take back work `safe` already counted — the pinned relation below says whether the fold has such a block.
    for k in ks.iter().filter(|k| reversible[*k] && **k <= 3_000) {
        assert_eq!(v(Rule::FinalPlus(3_000), *k), Verdict::CaughtBeforeSafe, "Final+3,000 at {k}");
    }
    eprintln!(
        "[d1-d] Final+3,000: a reversible conviction AFTER its instant exists in this sweep: {}",
        ks.iter().any(|k| reversible[k] && *k > 3_000)
    );
    // The shorter rules count the claim while the court can still convict it: `safe` retreats.
    for k in ks.iter().filter(|k| reversible[*k] && **k > 1_000) {
        assert_eq!(v(Rule::FinalPlus(1_000), *k), Verdict::SafeRetracted, "Final+1,000 at {k}");
    }
    for k in ks.iter().filter(|k| reversible[*k] && **k > 600) {
        assert_eq!(v(Rule::FinalPlus(600), *k), Verdict::SafeRetracted, "Final+600 at {k}");
    }
    for k in ks.iter().filter(|k| **k <= 1_000) {
        assert_eq!(v(Rule::FinalPlus(1_000), *k), Verdict::CaughtBeforeSafe, "Final+1,000 at {k}: inside its wait");
    }
    for k in ks.iter().filter(|k| **k <= 600) {
        assert_eq!(v(Rule::FinalPlus(600), *k), Verdict::CaughtBeforeSafe, "Final+600 at {k}: inside its wait");
    }
}
