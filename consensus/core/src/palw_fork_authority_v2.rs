//! The ONE fork-choice authority, as functions every selection site calls (ADR-0042 Decision 9,
//! PR-08) — virtual tip, IBD commit, pruning point, finality/deep-reorg, restart recovery.
//!
//! P0-5's disease was plural authority: the virtual sink consulted PALW weight while the
//! header-selected-tip store, IBD, pruning and finality kept ordering by blue work — one node,
//! two canonical-chain views, a fork with no attacker. The V1 lineage renamed the header store
//! to a *download hint* (P0-5's landing); this module is the other half: the decisions
//! themselves, expressed once, over [`compare_palw_candidates_v1`], so a site that wants a
//! different answer has no function to call.
//!
//! Everything here is pure over candidate orders and state facts. The pipeline consumes these
//! functions when `PalwConsensusMode::ConsensusV2` exists to demand them (PR-10) — wiring a
//! dead handle into today's blue-work pipeline would be surface without semantics, the
//! half-flipped shape this ruleset exists to remove. What must be true EARLIER is that the
//! rules are fixed, total, and tested — which is this file.
//!
//! The four decisions:
//!
//! * **Tip selection** ([`select_palw_tip_v2`]): the comparator's maximum, permutation-invariant
//!   by totality.
//! * **IBD commit** ([`decide_ibd_commit_v2`]): a staged challenger replaces the incumbent only
//!   by STRICTLY winning the comparator — ties keep the incumbent, so a re-derived equal chain
//!   can never churn the sink.
//! * **Pruning ceiling** ([`pruning_ceiling_v2`]): pruning never passes the safe frontier.
//!   Below it every claim is `Final`/`Voided` and the carriage summarizes them; above it live
//!   court evidence and unresolved claims still need their history. A pruning point above the
//!   frontier would delete the record mid-trial.
//! * **Deep reorg** ([`decide_deep_reorg_v2`]): a challenger that crosses the finality depth
//!   must ALSO strictly win the comparator — depth alone (blue work alone) reopens exactly the
//!   fabrication the frontier-first ordering exists to stop.

use crate::BlockHash;
use crate::palw_fork_choice::{PalwCandidateOrderV1, compare_palw_candidates_v1};
use core::cmp::Ordering;

/// The comparator's maximum over any candidate set — the virtual tip, and the restart-recovery
/// answer (recovery is selection over the same persisted orders; a second "recovery order"
/// would be a second authority).
///
/// Total order ⇒ permutation-invariant; `None` only on an empty set. Callers hand in every
/// candidate they'd consider — this function deliberately has no way to say "and also prefer
/// X for another reason."
pub fn select_palw_tip_v2(candidates: impl IntoIterator<Item = PalwCandidateOrderV1>) -> Option<PalwCandidateOrderV1> {
    candidates.into_iter().max_by(compare_palw_candidates_v1)
}

/// What an IBD staging decision may do (the V2 face of the staging commit gate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwIbdCommitV2 {
    /// The challenger strictly wins the comparator: commit the staged consensus.
    Commit,
    /// It does not: keep the incumbent. Ties keep the incumbent on purpose — a re-derived
    /// byte-equal chain, or one equal in every key, must never churn the sink back and forth
    /// between two nodes that happened to stage in different orders.
    KeepIncumbent,
}

/// The IBD-complete commit rule: strictly greater, or nothing.
pub fn decide_ibd_commit_v2(incumbent: &PalwCandidateOrderV1, challenger: &PalwCandidateOrderV1) -> PalwIbdCommitV2 {
    match compare_palw_candidates_v1(challenger, incumbent) {
        Ordering::Greater => PalwIbdCommitV2::Commit,
        Ordering::Equal | Ordering::Less => PalwIbdCommitV2::KeepIncumbent,
    }
}

/// The highest blue score pruning may reach on the selected chain: the safe frontier. Everything
/// at or below it is resolved (`Final`/`Voided`) and travels summarized in the state carriage
/// (ADR-0043 §4); everything above it may still be evidence — an unresolved claim's history, an
/// open court's committed roots — and history under trial is not prunable.
pub fn pruning_ceiling_v2(safe_frontier_blue_score: u64) -> u64 {
    safe_frontier_blue_score
}

/// Whether a proposed pruning point respects the ceiling.
pub fn pruning_point_allowed_v2(candidate_point_blue_score: u64, safe_frontier_blue_score: u64) -> bool {
    candidate_point_blue_score <= pruning_ceiling_v2(safe_frontier_blue_score)
}

/// What the finality / deep-reorg gate may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwDeepReorgV2 {
    /// The challenger strictly wins the one comparator: the reorg is legitimate under the same
    /// authority every other site uses.
    Allow,
    /// It does not. Depth, raw blue work, arrival order — none of them are the authority.
    Refuse,
}

/// The deep-reorg rule: one comparator, here too. A private fork can pile blue work without
/// limit, but its frontier died at the fork point (piles do not mature — no one could collect
/// receipts on a chain nobody saw), so frontier-first ordering refuses it HERE with the same
/// three keys the virtual tip used. A gate that consulted anything else would be the second
/// authority coming back through the basement.
pub fn decide_deep_reorg_v2(incumbent: &PalwCandidateOrderV1, challenger: &PalwCandidateOrderV1) -> PalwDeepReorgV2 {
    match compare_palw_candidates_v1(challenger, incumbent) {
        Ordering::Greater => PalwDeepReorgV2::Allow,
        Ordering::Equal | Ordering::Less => PalwDeepReorgV2::Refuse,
    }
}

/// Convenience for sites that hold `(block, order)` pairs: the selected block hash.
pub fn select_palw_tip_hash_v2(candidates: impl IntoIterator<Item = (BlockHash, PalwCandidateOrderV1)>) -> Option<BlockHash> {
    candidates.into_iter().max_by(|a, b| compare_palw_candidates_v1(&a.1, &b.1)).map(|(hash, _)| hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hash64;
    use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2};
    use crate::palw_state_v2::{
        PalwBlockContextV2, PalwBondKeyV2, PalwConsensusObjectV2, PalwPanelSeatV2, PalwPwuRuleV2, PalwStateBookV2, PalwStateParamsV2,
    };
    use crate::tx::{TransactionId, TransactionOutpoint};

    /// Operator identities are DERIVED from a key now, so the fixtures carry a key and let the
    /// state machine mint the id — the same path a real registration takes.
    fn op_key(v: u64) -> Vec<u8> {
        vec![v as u8; 8]
    }

    fn op_id(v: u64) -> Hash64 {
        crate::palw_state_v2::palw_operator_id_v2(&op_key(v))
    }

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn order(frontier: u64, safe: u128, immature: u128, id: u64) -> PalwCandidateOrderV1 {
        PalwCandidateOrderV1::new(frontier, safe, immature, h64(id))
    }

    #[test]
    fn selection_is_the_comparators_maximum_whatever_the_arrival_order() {
        let set = vec![order(100, 10, 3, 4), order(101, 1, 0, 9), order(100, 11, 0, 2), order(100, 10, 3, 1)];
        let forward = select_palw_tip_v2(set.clone()).unwrap();
        let mut reversed = set.clone();
        reversed.reverse();
        assert_eq!(select_palw_tip_v2(reversed).unwrap(), forward);
        assert_eq!(forward.safe_frontier_blue_score, 101, "the deepest frontier wins");
        assert!(select_palw_tip_v2(Vec::new()).is_none(), "an empty set selects nothing, never a default");
    }

    #[test]
    fn ibd_commit_requires_a_strict_win_and_ties_keep_the_incumbent() {
        let incumbent = order(100, 10, 0, 1);
        assert_eq!(decide_ibd_commit_v2(&incumbent, &order(101, 1, 0, 2)), PalwIbdCommitV2::Commit, "a deeper frontier commits");
        assert_eq!(decide_ibd_commit_v2(&incumbent, &order(100, 10, 0, 1)), PalwIbdCommitV2::KeepIncumbent, "equal keeps");
        assert_eq!(decide_ibd_commit_v2(&incumbent, &order(99, 999, 999, 2)), PalwIbdCommitV2::KeepIncumbent, "a heavier pile loses");
    }

    #[test]
    fn deep_reorgs_answer_to_the_same_comparator_and_nothing_else() {
        let incumbent = order(100, 10, 0, 1);
        // The fabrication shape: enormous immature weight, dead frontier. Refused however deep.
        assert_eq!(decide_deep_reorg_v2(&incumbent, &order(40, 9, u128::MAX / 2, 2)), PalwDeepReorgV2::Refuse);
        // A challenger that genuinely matured further is allowed — the gate is not a veto on
        // reorgs, it is the same authority applied at depth.
        assert_eq!(decide_deep_reorg_v2(&incumbent, &order(101, 10, 0, 2)), PalwDeepReorgV2::Allow);
        assert_eq!(decide_deep_reorg_v2(&incumbent, &incumbent.clone()), PalwDeepReorgV2::Refuse, "equal is not a reorg reason");
    }

    #[test]
    fn pruning_never_passes_the_safe_frontier() {
        assert!(pruning_point_allowed_v2(0, 0));
        assert!(pruning_point_allowed_v2(100, 100), "the frontier itself is prunable — everything at it is resolved");
        assert!(!pruning_point_allowed_v2(101, 100), "one block past the frontier is history under trial");
    }

    // ---- the register's P0-5 differential, at the substrate level ----

    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(
            100,
            10,
            10,
            20,
            500,
            1000,
            h64(1),
            4,
            1000,
            100,
            1000,
            0,
        )
        .unwrap()
    }

    fn bond_key(v: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
    }

    fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: BlockHash::from_u64_word(block), daa_score: daa, blue_score: blue, subsidy: 0 }
    }

    fn attempt(pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
        let bond = bond_key(1).0;
        PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain: h64(999),
                challenge: challenge_v2(h64(999), h64(5), 1_700, nonce, h64(1), &bond),
                class_id: h64(1),
                executor_bond: bond,
                executor_pubkey: vec![7; 4],
                operator_id: op_id(0x21),
                artifact_root: h64(11),
                trace_root: h64(31),
                output_root: h64(32),
                pwu,
                trace_manifest_root: h64(33),
                trace_chunk_count: 4,
                trace_retention_daa: 999_999,
                execution_root: h64(41),
            },
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        }
    }

    fn registrations() -> Vec<PalwConsensusObjectV2> {
        vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(1),
                pubkey: vec![7; 4],
                operator_pubkey: op_key(0x21),
                collateral: 100_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            },
        ]
    }

    /// **The audit register's P0-5 red test, at the substrate level.** One DAG — a matured
    /// branch M and a heavier-but-immature branch P — applied to two books in OPPOSITE orders.
    /// Then every "site" asks its question through this module: the virtual tip, the IBD commit
    /// between the two branch heads, the deep-reorg gate, the pruning ceiling, and restart
    /// recovery (re-selection over reloaded state). Every answer must be identical on both
    /// books, and every answer must be the SAME chain — one authority, wherever you stand.
    #[test]
    fn palw_v2_all_selection_sites_agree() {
        let genesis_block = BlockHash::from_u64_word(0);

        // The DAG, described once. Branch M: one 40-pwu claim walked to Final (frontier reaches
        // its tip). Branch P: three 1000-pwu claims left Provisional (heavy, immature, frontier
        // dead at the fork point).
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(0x90) }];
        let branch_m: Vec<(u64, u64, u64, Vec<PalwConsensusObjectV2>, Option<PalwAttemptEnvelopeV2>)> = vec![
            (2, 101, 2, vec![], Some(env)),
            (3, 102, 3, vec![PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None),
            (4, 103, 4, vec![PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }], None),
            (5, 124, 5, vec![], None),
        ];
        let branch_p: Vec<(u64, u64, u64, Vec<PalwConsensusObjectV2>, Option<PalwAttemptEnvelopeV2>)> = vec![
            (12, 101, 2, vec![], Some(attempt(1000, 11))),
            (13, 102, 3, vec![], Some(attempt(1000, 12))),
            (14, 103, 4, vec![], Some(attempt(1000, 13))),
        ];

        let apply_branch =
            |book: &mut PalwStateBookV2, branch: &[(u64, u64, u64, Vec<PalwConsensusObjectV2>, Option<PalwAttemptEnvelopeV2>)]| {
                let mut parent = BlockHash::from_u64_word(1);
                for (b, daa, blue, objects, att) in branch {
                    book.apply_block(parent, ctx(*b, *daa, *blue), objects, att.as_ref()).unwrap();
                    parent = BlockHash::from_u64_word(*b);
                }
            };

        let build = |m_first: bool| -> PalwStateBookV2 {
            let mut book = PalwStateBookV2::new(params());
            book.insert_genesis(genesis_block);
            book.apply_block(genesis_block, ctx(1, 100, 1), &registrations(), None).unwrap();
            if m_first {
                apply_branch(&mut book, &branch_m);
                apply_branch(&mut book, &branch_p);
            } else {
                apply_branch(&mut book, &branch_p);
                apply_branch(&mut book, &branch_m);
            }
            book
        };

        let m_tip = BlockHash::from_u64_word(5);
        let p_tip = BlockHash::from_u64_word(14);

        let mut answers = Vec::new();
        for m_first in [true, false] {
            let book = build(m_first);
            let order_of = |tip: BlockHash| book.state_of(&tip).unwrap().candidate_order(tip);

            // Site 1: virtual tip selection over both branch heads.
            let selected = select_palw_tip_hash_v2([(m_tip, order_of(m_tip)), (p_tip, order_of(p_tip))]).unwrap();
            // Site 2: IBD commit — the pile stages against the matured incumbent, and the
            // matured branch stages against the pile.
            let pile_challenges = decide_ibd_commit_v2(&order_of(m_tip), &order_of(p_tip));
            let matured_challenges = decide_ibd_commit_v2(&order_of(p_tip), &order_of(m_tip));
            // Site 3: the deep-reorg gate on the same two contests.
            let pile_reorg = decide_deep_reorg_v2(&order_of(m_tip), &order_of(p_tip));
            // Site 4: the pruning ceiling on the selected chain.
            let ceiling = pruning_ceiling_v2(book.state_of(&selected).unwrap().safe_frontier().0);
            // Site 5: restart recovery — reload every candidate's state through the carriage and
            // re-select. Same answer, or a reboot is a fork.
            let recovered = {
                let reload = |tip: BlockHash| {
                    let state = book.state_of(&tip).unwrap();
                    let carriage = crate::palw_state_v2::PalwStateCarriageV2::from_state(state);
                    carriage.into_state(&params(), Some(state.state_root())).unwrap().candidate_order(tip)
                };
                select_palw_tip_hash_v2([(m_tip, reload(m_tip)), (p_tip, reload(p_tip))]).unwrap()
            };
            answers.push((selected, pile_challenges, matured_challenges, pile_reorg, ceiling, recovered));
        }

        assert_eq!(answers[0], answers[1], "two application orders answered differently at some site — the P0-5 partition");
        let (selected, pile_challenges, matured_challenges, pile_reorg, ceiling, recovered) = answers[0];
        assert_eq!(selected, m_tip, "every site prefers the matured chain over the heavier pile");
        assert_eq!(pile_challenges, PalwIbdCommitV2::KeepIncumbent, "IBD refuses the pile");
        assert_eq!(matured_challenges, PalwIbdCommitV2::Commit, "IBD accepts the matured chain over the pile");
        assert_eq!(pile_reorg, PalwDeepReorgV2::Refuse, "the deep-reorg gate refuses the pile with the same keys");
        // The frontier is the block whose WORK matured (blue score 2, the block that carried the
        // attempt), not the block where the last transition happened to land — see the frontier
        // rule in `palw_state_v2`. Pruning may reach exactly there: the claim below it is Final
        // and travels summarized, while block 3's panel record and block 4's licence are history
        // the claim at 2 no longer needs. A ceiling at the TIP would have been the old rule's
        // answer, and the old rule also handed a workless fork a tip-deep frontier for free.
        assert_eq!(ceiling, 2, "pruning may reach the deepest matured block and no further");
        assert_eq!(recovered, selected, "restart recovery re-selects the same tip through the carriage");
    }
}
