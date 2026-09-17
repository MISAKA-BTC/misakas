//! ADR-0127 Decision 3 — **confirmations count settled anchors.**
//!
//! A selected-chain block that carries an attempt is a PALW settlement anchor, and the attempt's
//! claim is the anchor's claim. A transaction accepted by the chain block at DAA score `d` is
//! *settled* once the chain's safe frontier — the deepest anchor whose claim is `Final` with nothing
//! unresolved beneath it — stands at or past `d`, and its *settlement depth* is the number of anchors
//! at or after `d` whose claims are `Final`. Never a count of blocks: an execution block (ADR-0125)
//! or a heartbeat block carries no claim, so a thousand of them and no new `Final` claim is depth 0.
//!
//! The answer is a read of the sink's [`PalwChainStateV2`] — its frontier, its claims and their
//! phases — and of nothing outside it. It is exactly as available on a PALW network that runs no
//! overlay beside it as on one that does.
//!
//! What the state cannot say, the answer says it cannot. A terminal claim retires from the state
//! `claim_retirement_daa` after it went terminal, so for an old `d` the retained `Final` claims may
//! be fewer than the anchors that were there: [`PalwSettlementV1::depth_is_lower_bound`] is set
//! whenever that is possible, and is never set where it is not.

use crate::palw_state_v2::{PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwStateParamsV2};
use kaspa_hashes::ZERO_HASH64;

/// What the sink's PALW state says about the transactions the selected chain accepted at one DAA
/// score (ADR-0127 Decision 3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwSettlementV1 {
    /// The DAA score of the chain block the state stands at — the point this answer is true at.
    pub sink_daa: u64,
    /// The safe frontier stands at or past the asked DAA score, so everything the selected chain
    /// accepted there is settled. False on a chain that has no frontier yet.
    pub settled: bool,
    /// The settlement depth: attempt claims the state retains in `Final` whose accepting chain block
    /// is at or after the asked DAA score. Anchors, not blocks.
    pub depth: u64,
    /// Attempt claims at or after the asked DAA score still on their way — neither `Final` nor
    /// `Voided`. A voided claim is no anchor and is in neither count.
    pub pending: u64,
    /// Claims at or after the asked DAA score may have retired from the state, so `depth` counts at
    /// least that many anchors and possibly fewer than there were.
    pub depth_is_lower_bound: bool,
    pub safe_frontier_blue_score: u64,
    /// The DAA score of the chain block that accepted the frontier's claim; 0 with no frontier.
    pub safe_frontier_daa: u64,
}

/// **The settlement read** (ADR-0127 Decision 3) — `state` is the sink's, `params` the rules it was
/// folded under, `daa_score` the accepting chain block's DAA score (a UTXO entry's
/// `block_daa_score`).
///
/// The frontier names the block that CARRIED its claim, which since ADR-0058 may be a merged block
/// whose own DAA score is below the chain block that accepted it; what orders the chain is the
/// accepting block. So the frontier is dated by its claim's `accepted_daa` while the state retains
/// the claim, and by `frontier_header_daa` — the carrying header's DAA score, the conservative
/// reading — once it has retired. `None` only when a frontier exists and neither date is at hand: a
/// frontier this node cannot date is an answer it does not have, not an unsettled one.
pub fn palw_settlement_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    frontier_header_daa: Option<u64>,
    daa_score: u64,
) -> Option<PalwSettlementV1> {
    let (safe_frontier_blue_score, frontier) = state.safe_frontier();
    let has_frontier = frontier != ZERO_HASH64;
    let (mut depth, mut pending, mut frontier_claim_daa) = (0u64, 0u64, None);
    for (_, claim) in state.claims_iter() {
        let is_final = matches!(claim.phase, PalwClaimPhaseV2::Final { .. });
        // The fold sets the frontier from a `Final` claim of any source, so it is dated by one.
        if has_frontier && is_final && claim.accepted_block == frontier && claim.accepted_blue_score == safe_frontier_blue_score {
            frontier_claim_daa = Some(claim.accepted_daa);
        }
        // An anchor is a block's attempt; a free-prompt commitment licenses work and anchors nothing.
        let anchors = match claim.source {
            PalwClaimSourceV2::Attempt => true,
            PalwClaimSourceV2::FreePrompt { .. } => false,
        };
        if !anchors || claim.accepted_daa < daa_score {
            continue;
        }
        // Every phase named, so a phase added to the lattice has to be placed here to compile.
        match claim.phase {
            PalwClaimPhaseV2::Final { .. } => depth += 1,
            PalwClaimPhaseV2::Voided { .. } => {}
            PalwClaimPhaseV2::Provisional
            | PalwClaimPhaseV2::PanelBound { .. }
            | PalwClaimPhaseV2::ReceiptLicensed { .. }
            | PalwClaimPhaseV2::DefaultDisputed { .. } => pending += 1,
        }
    }
    let safe_frontier_daa = if has_frontier { frontier_claim_daa.or(frontier_header_daa)? } else { 0 };
    let sink_daa = state.last_point().map_or(0, |point| point.daa_score);
    // A claim retires at the first block whose DAA score is past `terminal_daa + retirement`, and a
    // claim accepted at or after `d` went terminal at or after `d`. So none of them can have left
    // while `sink_daa <= d + retirement`; and none has left at all while no `Final` attempt ever
    // retired, which `retired_safe_weight` records (every attempt claims at least one pwu).
    let retirement = params.claim_retirement_daa();
    let depth_is_lower_bound = retirement > 0 && state.retired_safe_weight() > 0 && sink_daa > daa_score.saturating_add(retirement);
    Some(PalwSettlementV1 {
        sink_daa,
        settled: has_frontier && safe_frontier_daa >= daa_score,
        depth,
        pending,
        depth_is_lower_bound,
        safe_frontier_blue_score,
        safe_frontier_daa,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlockHash;
    use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2};
    use crate::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
    use crate::palw_state_v2::{
        PalwBlockContextV2, PalwBondKeyV2, PalwConsensusObjectV2, PalwPanelSeatV2, PalwPwuRuleV2, apply_palw_transition_v2,
        palw_operator_id_v2,
    };
    use crate::tx::{TransactionId, TransactionOutpoint};
    use kaspa_hashes::Hash64;

    // ---- fixtures: the state tests' lattice, built through the public fold -------------------

    fn params() -> PalwStateParamsV2 {
        // bind 10, receipt 10, challenge 20, court 500 — the state tests' windows.
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 1000, 0).unwrap().with_fp_quanta(8, 64).unwrap()
    }

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn block(v: u64) -> BlockHash {
        BlockHash::from_u64_word(v)
    }

    fn bond() -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 })
    }

    fn at(block_word: u64, daa: u64) -> PalwBlockContextV2 {
        // One chain block a step: the blue score is the block's own number.
        PalwBlockContextV2 { block: block(block_word), daa_score: daa, blue_score: block_word, subsidy: 0 }
    }

    fn register() -> Vec<PalwConsensusObjectV2> {
        vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond(),
                pubkey: vec![7; 4],
                operator_pubkey: vec![21; 8],
                collateral: 1_000,
                payout_payload: Hash64::from_u64_word(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ]
    }

    fn attempt(nonce: u64) -> PalwAttemptEnvelopeV2 {
        let network_domain = h64(999);
        PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain,
                challenge: challenge_v2(network_domain, h64(5), 1_700, nonce, h64(1), &bond().0),
                class_id: h64(1),
                executor_bond: bond().0,
                executor_pubkey: vec![7; 4],
                operator_id: palw_operator_id_v2(&[21; 8]),
                artifact_root: h64(11),
                trace_root: h64(31),
                output_root: h64(32),
                pwu: 40,
                trace_manifest_root: h64(33),
                trace_chunk_count: 4,
                trace_retention_daa: 999_999,
                execution_root: h64(41),
            },
            signature: vec![0; 8],
        }
    }

    fn bind(claim: Hash64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::PanelBound {
            claim,
            anchor: h64(77),
            seats: vec![PalwPanelSeatV2 { bond: bond(), operator_id: h64(90) }],
        }
    }

    fn license(claim: Hash64) -> PalwConsensusObjectV2 {
        // The transition never verifies a signature — that is the acceptance layer's.
        PalwConsensusObjectV2::ReceiptLicensed {
            claim,
            receipts: vec![PalwSeatReceiptV2 {
                claim: Hash64::default(),
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: bond(),
                signed_daa: 0,
                signature: Vec::new(),
            }],
        }
    }

    fn step(
        parent: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        point: PalwBlockContextV2,
        objects: &[PalwConsensusObjectV2],
        work: Option<&PalwAttemptEnvelopeV2>,
    ) -> PalwChainStateV2 {
        let (state, _) = apply_palw_transition_v2(parent, p, &point, objects, work).expect("the block folds");
        state.assert_internal_consistency(p).expect("the fold keeps its own books");
        state
    }

    fn read(state: &PalwChainStateV2, p: &PalwStateParamsV2, header_daa: Option<u64>, d: u64) -> PalwSettlementV1 {
        palw_settlement_v1(state, p, header_daa, d).expect("the frontier is dated")
    }

    /// Anchors A (DAA 101) and B (102) licensed and swept `Final` at DAA 125; anchor C (120) still
    /// `Provisional`. Returns the state at the sweep and the three claim ids.
    fn two_final_one_pending(p: &PalwStateParamsV2) -> (PalwChainStateV2, [Hash64; 3]) {
        let (a, b, c) = (attempt(1), attempt(2), attempt(3));
        let ids = [attempt_id_v2(&a.attempt), attempt_id_v2(&b.attempt), attempt_id_v2(&c.attempt)];
        let s = step(&PalwChainStateV2::genesis(), p, at(1, 100), &register(), None);
        let s = step(&s, p, at(2, 101), &[], Some(&a));
        let s = step(&s, p, at(3, 102), &[bind(ids[0])], Some(&b));
        let s = step(&s, p, at(4, 103), &[bind(ids[1]), license(ids[0])], None);
        let s = step(&s, p, at(5, 104), &[license(ids[1])], None);
        let s = step(&s, p, at(6, 120), &[], Some(&c));
        // A's challenge window closes at 123 and B's at 124; C's bind deadline is 130.
        let s = step(&s, p, at(7, 125), &[], None);
        assert!(matches!(s.claim(&ids[0]).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
        assert!(matches!(s.claim(&ids[1]).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
        assert!(matches!(s.claim(&ids[2]).unwrap().phase, PalwClaimPhaseV2::Provisional));
        (s, ids)
    }

    /// **A chain with no `Final` claim has no frontier, and nothing on it is settled** — however many
    /// blocks it has and whatever DAA score is asked.
    #[test]
    fn nothing_is_settled_before_the_first_final() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        for d in [0, 1, u64::MAX] {
            assert_eq!(read(&genesis, &p, None, d), PalwSettlementV1::default(), "the zero point settles nothing (d = {d})");
        }
        let s = step(&genesis, &p, at(1, 100), &register(), None);
        let s = step(&s, &p, at(2, 101), &[], Some(&attempt(1)));
        let answer = read(&s, &p, None, 0);
        assert!(!answer.settled && answer.depth == 0, "a Provisional anchor is not a settled one: {answer:?}");
        assert_eq!((answer.pending, answer.sink_daa), (1, 101));
    }

    /// **Depth counts `Final` anchors at or after `d`; pending counts the ones still on their way;
    /// settled is the frontier's.** The boundaries are pinned on both sides of each anchor.
    #[test]
    fn depth_counts_final_anchors_at_or_after_the_accepting_block() {
        let p = params();
        let (s, _) = two_final_one_pending(&p);
        assert_eq!(s.safe_frontier(), (3, block(3)), "the frontier is B, the deepest Final with nothing open beneath it");
        let expect = |d: u64, settled: bool, depth: u64, pending: u64| {
            let answer = read(&s, &p, None, d);
            assert_eq!((answer.settled, answer.depth, answer.pending), (settled, depth, pending), "at d = {d}: {answer:?}");
            assert!(!answer.depth_is_lower_bound, "a network that never retires has an exact depth");
            assert_eq!((answer.sink_daa, answer.safe_frontier_blue_score, answer.safe_frontier_daa), (125, 3, 102));
        };
        expect(0, true, 2, 1);
        expect(101, true, 2, 1);
        expect(102, true, 1, 1);
        expect(103, false, 0, 1);
        expect(120, false, 0, 1);
        expect(121, false, 0, 0);
        expect(u64::MAX, false, 0, 0);
    }

    /// **Blocks that carry no claim add no depth and settle nothing** — the heartbeat's and the
    /// execution lane's blocks, however many. C voids at its bind deadline and leaves both counts.
    #[test]
    fn a_burst_of_blocks_without_claims_is_zero_confirmations() {
        let p = params();
        let (mut s, ids) = two_final_one_pending(&p);
        let before = read(&s, &p, None, 103);
        for word in 8..1_008u64 {
            s = step(&s, &p, at(word, 125 + word), &[], None);
        }
        assert!(matches!(s.claim(&ids[2]).unwrap().phase, PalwClaimPhaseV2::Voided { .. }), "nobody bound C");
        let after = read(&s, &p, None, 103);
        assert_eq!((after.settled, after.depth), (before.settled, before.depth), "a thousand blocks, no new Final anchor");
        assert_eq!((after.settled, after.depth, after.pending), (false, 0, 0), "and a voided anchor is neither: {after:?}");
        assert_eq!(read(&s, &p, None, 101).depth, 2, "the settled anchors are still two");
    }

    /// **A `Final` anchor above an unresolved one is depth, not settlement.** The frontier waits for
    /// everything beneath it: B is `Final` while A — accepted first — is still licensed, so B counts
    /// toward depth and no DAA score is settled until A resolves.
    #[test]
    fn a_final_anchor_above_an_open_one_counts_but_does_not_settle() {
        let p = params();
        let (a, b) = (attempt(1), attempt(2));
        let (ida, idb) = (attempt_id_v2(&a.attempt), attempt_id_v2(&b.attempt));
        let s = step(&PalwChainStateV2::genesis(), &p, at(1, 100), &register(), None);
        let s = step(&s, &p, at(2, 101), &[], Some(&a));
        let s = step(&s, &p, at(3, 102), &[bind(ida)], Some(&b));
        let s = step(&s, &p, at(4, 103), &[bind(idb)], None);
        let s = step(&s, &p, at(5, 104), &[license(idb)], None);
        let s = step(&s, &p, at(6, 110), &[license(ida)], None);
        // B's window closed at 124; A's closes at 130.
        let s = step(&s, &p, at(7, 125), &[], None);
        assert!(matches!(s.claim(&idb).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
        assert!(matches!(s.claim(&ida).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        let answer = read(&s, &p, None, 102);
        assert_eq!((answer.settled, answer.depth, answer.pending), (false, 1, 0), "{answer:?}");
        assert_eq!(read(&s, &p, None, 101).pending, 1, "A is pending at its own DAA score");

        let s = step(&s, &p, at(8, 131), &[], None);
        let answer = read(&s, &p, None, 102);
        assert_eq!((answer.settled, answer.depth, answer.safe_frontier_daa), (true, 1, 102), "A resolved, B settles: {answer:?}");
        assert_eq!((read(&s, &p, None, 101).depth, read(&s, &p, None, 101).settled), (2, true));
    }

    /// **The frontier is dated by the chain block that accepted its claim, not by the carrying
    /// header** — the two differ for a merged attempt — and by the header only once the claim has
    /// retired. With neither, the read has no answer rather than a wrong one.
    #[test]
    fn the_frontier_is_dated_by_its_claim_then_by_its_header_and_otherwise_not_at_all() {
        let p = params();
        let (s, _) = two_final_one_pending(&p);
        assert_eq!(read(&s, &p, Some(7), 102).safe_frontier_daa, 102, "a retained claim outranks the header's own score");
        assert!(read(&s, &p, Some(7), 102).settled);

        let p = params().with_claim_retirement_daa(50).expect("retirement past the abandon hold");
        let (s, ids) = two_final_one_pending(&p);
        // C voids at its bind deadline (130); A and B retire at 125 + 50 = 175.
        let s = step(&s, &p, at(8, 131), &[], None);
        let s = step(&s, &p, at(9, 176), &[], None);
        assert!(s.claim(&ids[0]).is_none() && s.claim(&ids[1]).is_none(), "both Final anchors retired");
        assert_eq!(s.safe_frontier(), (3, block(3)), "the frontier never retreats");
        assert_eq!(palw_settlement_v1(&s, &p, None, 101), None, "a frontier this node cannot date is no answer");
        let answer = read(&s, &p, Some(102), 101);
        assert_eq!((answer.settled, answer.safe_frontier_daa, answer.depth), (true, 102, 0), "{answer:?}");
        assert!(answer.depth_is_lower_bound, "the anchors that made it settled are gone from the count");
    }

    /// **A lower bound exactly where retirement can have reached `d`, and nowhere else.** A claim at
    /// or after `d` went terminal at or after `d` and retires past `terminal + retirement`, so the
    /// flag is `sink > d + retirement` — pinned on both sides — and never on a chain where no `Final`
    /// anchor has retired yet, nor on one that never retires.
    #[test]
    fn depth_is_a_lower_bound_only_where_retirement_can_have_reached_the_asked_score() {
        let p = params().with_claim_retirement_daa(50).expect("retirement past the abandon hold");
        let (s, _) = two_final_one_pending(&p);
        let s = step(&s, &p, at(8, 131), &[], None);
        assert_eq!(s.retired_safe_weight(), 0, "nothing Final has retired yet");
        assert!(!read(&s, &p, None, 0).depth_is_lower_bound, "no retirement yet, so every count is exact");

        let s = step(&s, &p, at(9, 176), &[], None);
        assert_eq!(s.retired_safe_weight(), 80, "A and B retired, 40 pwu each");
        assert!(read(&s, &p, Some(102), 125).depth_is_lower_bound, "176 > 125 + 50");
        assert!(!read(&s, &p, Some(102), 126).depth_is_lower_bound, "176 = 126 + 50: nothing at or after 126 can have left");

        let never = params();
        let (s, _) = two_final_one_pending(&never);
        let mut s = s;
        for word in 8..40u64 {
            s = step(&s, &never, at(word, 125 + 10 * word), &[], None);
        }
        assert!(!read(&s, &never, None, 0).depth_is_lower_bound, "a network that never retires never undercounts");
    }

    // ---- ADR-0127 Decision 7: the guard ------------------------------------------------------

    /// The PALW V2 settlement path's sources, as ADR-0127 Decision 7 lists them, and the read this
    /// module adds to it. Paths are built by `concat!` so a renamed file fails here, by name.
    macro_rules! settlement_path_sources {
        ($($file:literal),* $(,)?) => { [$(($file, concat!(env!("CARGO_MANIFEST_DIR"), "/src/", $file))),*] };
    }
    const SETTLEMENT_PATH_SOURCES: [(&str, &str); 11] = settlement_path_sources!(
        "palw_state_v2.rs",
        "palw_panel_v2.rs",
        "palw_attempt_v2.rs",
        "palw_admission_v2.rs",
        "palw_fork_choice.rs",
        "palw_fork_authority_v2.rs",
        "palw_panel_economy_v1.rs",
        "palw_reward_v2.rs",
        "palw_producer_v2.rs",
        "palw_execution_lane_v1.rs",
        "palw_settlement_v1.rs",
    );

    /// The names Decision 7 forbids, spelled as the ADR spells them. Each is matched by its words
    /// (see [`names_term`]), so `DnsParams` also finds `dns_params` and `DNSParams`, and a term's last
    /// word finds its longer forms — `dns_confirm` is `dns_confirmation` and `dns_confirmed`,
    /// `validator` is `validators` and `kaspa_pq_validator_core`, `StakeBond` is `StakeBondKey`.
    const FORBIDDEN: [&str; 15] = [
        "dns_finality",
        "DnsParams",
        "dns_params",
        "DnsState",
        "dns_confirm",
        "vlt",
        "Vlt",
        "StakeAttestation",
        "StakeBond",
        "stake_bond",
        "ActiveBondView",
        "validator",
        "Validator",
        "beacon",
        "Beacon",
    ];

    fn is_identifier_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    /// **Everything in Rust source that is not code, blanked**: line and doc comments, nested block
    /// comments, string literals (plain, byte and C, and raw with any number of `#`) and char and
    /// byte literals. A blanked character becomes a space and a newline stays a newline, so every
    /// line keeps its number. A lifetime or a label is code and stays.
    fn strip_non_code(source: &str) -> String {
        let chars: Vec<char> = source.chars().collect();
        let mut out: Vec<char> = Vec::with_capacity(chars.len());
        let blank = |out: &mut Vec<char>, span: &[char]| out.extend(span.iter().map(|&c| if c == '\n' { '\n' } else { ' ' }));
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let next = chars.get(i + 1).copied();
            // Line and doc comments.
            if c == '/' && next == Some('/') {
                let end = chars[i..].iter().position(|&c| c == '\n').map_or(chars.len(), |n| i + n);
                blank(&mut out, &chars[i..end]);
                i = end;
                continue;
            }
            // Block comments, which nest.
            if c == '/' && next == Some('*') {
                let (mut depth, mut j) = (0usize, i);
                while j < chars.len() {
                    if chars[j] == '/' && chars.get(j + 1) == Some(&'*') {
                        depth += 1;
                        j += 2;
                    } else if chars[j] == '*' && chars.get(j + 1) == Some(&'/') {
                        depth -= 1;
                        j += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        j += 1;
                    }
                }
                let end = j.min(chars.len());
                blank(&mut out, &chars[i..end]);
                i = end;
                continue;
            }
            // Raw strings: `r"…"`, `r#"…"#`, `br"…"`, `cr"…"` — no escapes, closed by a quote and as
            // many hashes as opened them. `r#ident` is a raw identifier and stays code.
            let starts_token = i == 0 || !is_identifier_char(chars[i - 1]);
            let raw_prefix = match (c, next) {
                ('r', Some('"' | '#')) => Some(1),
                ('b' | 'c', Some('r')) if matches!(chars.get(i + 2), Some('"' | '#')) => Some(2),
                _ => None,
            };
            if let Some(prefix) = raw_prefix.filter(|_| starts_token) {
                let mut j = i + prefix;
                let mut hashes = 0;
                while chars.get(j) == Some(&'#') {
                    hashes += 1;
                    j += 1;
                }
                if chars.get(j) == Some(&'"') {
                    let mut k = j + 1;
                    let end = loop {
                        if k >= chars.len() {
                            break chars.len();
                        }
                        if chars[k] == '"' && (1..=hashes).all(|h| chars.get(k + h) == Some(&'#')) {
                            break k + 1 + hashes;
                        }
                        k += 1;
                    };
                    blank(&mut out, &chars[i..end]);
                    i = end;
                    continue;
                }
            }
            // Plain, byte and C strings (the prefix letter stays, as a one-letter identifier).
            if c == '"' {
                let mut j = i + 1;
                while j < chars.len() && chars[j] != '"' {
                    j += if chars[j] == '\\' { 2 } else { 1 };
                }
                let end = (j + 1).min(chars.len());
                blank(&mut out, &chars[i..end]);
                i = end;
                continue;
            }
            // Char and byte literals — `'x'`, `'"'`, `'\''`, `'\u{2192}'` — but not a lifetime.
            if c == '\'' {
                let end = if next == Some('\\') {
                    chars[i + 3.min(chars.len() - i)..].iter().position(|&c| c == '\'').map(|n| i + 3 + n + 1)
                } else if chars.get(i + 2) == Some(&'\'') {
                    Some(i + 3)
                } else {
                    None
                };
                if let Some(end) = end.map(|end| end.min(chars.len())) {
                    blank(&mut out, &chars[i..end]);
                    i = end;
                    continue;
                }
            }
            out.push(c);
            i += 1;
        }
        out.into_iter().collect()
    }

    /// **`#[cfg(test)] mod …` blanked, whole** — a test module exercises the settlement path and is
    /// not on it (the state tests build receipt-lane spends that name their own block fields, for
    /// one). Runs on stripped code, where every brace is a real one. Only modules: a `#[cfg(test)]`
    /// function inside the path's code stays in the scan.
    fn blank_test_modules(code: &str) -> String {
        let mut chars: Vec<char> = code.chars().collect();
        let skip_ws = |chars: &[char], mut j: usize| {
            while chars.get(j).is_some_and(|c| c.is_whitespace()) {
                j += 1;
            }
            j
        };
        let word_at = |chars: &[char], j: usize, word: &str| {
            let w: Vec<char> = word.chars().collect();
            chars.get(j..j + w.len()) == Some(&w[..]) && !chars.get(j + w.len()).is_some_and(|&c| is_identifier_char(c))
        };
        // The index one past the bracket that closes the one at `open`.
        let close = |chars: &[char], open: usize| {
            let (opening, closing) = match chars[open] {
                '[' => ('[', ']'),
                '(' => ('(', ')'),
                _ => ('{', '}'),
            };
            let mut depth = 0usize;
            for (k, &c) in chars.iter().enumerate().skip(open) {
                if c == opening {
                    depth += 1;
                } else if c == closing {
                    depth -= 1;
                    if depth == 0 {
                        return k + 1;
                    }
                }
            }
            chars.len()
        };
        let mut i = 0;
        while i < chars.len() {
            // `#[cfg(test)]`, spaced however.
            let attribute_end = (|| {
                let mut j = i;
                for token in ["#", "[", "cfg", "(", "test", ")", "]"] {
                    j = skip_ws(&chars, j);
                    if token.len() == 1 {
                        if chars.get(j) != token.chars().next().as_ref() {
                            return None;
                        }
                        j += 1;
                    } else {
                        if !word_at(&chars, j, token) {
                            return None;
                        }
                        j += token.len();
                    }
                }
                Some(j)
            })();
            let Some(mut j) = attribute_end.filter(|_| chars[i] == '#') else {
                i += 1;
                continue;
            };
            // Further attributes, a visibility, then `mod name` and its body or its `;`.
            j = skip_ws(&chars, j);
            while chars.get(j) == Some(&'#') && chars.get(skip_ws(&chars, j + 1)) == Some(&'[') {
                j = skip_ws(&chars, close(&chars, skip_ws(&chars, j + 1)));
            }
            if word_at(&chars, j, "pub") {
                j = skip_ws(&chars, j + 3);
                if chars.get(j) == Some(&'(') {
                    j = skip_ws(&chars, close(&chars, j));
                }
            }
            if !word_at(&chars, j, "mod") {
                i += 1;
                continue;
            }
            j = skip_ws(&chars, j + 3);
            while chars.get(j).is_some_and(|&c| is_identifier_char(c)) {
                j += 1;
            }
            j = skip_ws(&chars, j);
            let end = match chars.get(j) {
                Some(';') => j + 1,
                Some('{') => close(&chars, j),
                _ => {
                    i += 1;
                    continue;
                }
            };
            for c in &mut chars[i..end] {
                if *c != '\n' {
                    *c = ' ';
                }
            }
            i = end;
        }
        chars.into_iter().collect()
    }

    /// The lower-cased words an identifier is spelled with: split at `_`, before a capital that
    /// follows a lower-case letter or a digit, and before the last capital of a run followed by a
    /// lower-case letter. `DnsParams`, `dns_params` and `DNSParams` are all `dns params`.
    fn identifier_words(identifier: &str) -> Vec<String> {
        let chars: Vec<char> = identifier.chars().collect();
        let mut words = Vec::new();
        let mut word = String::new();
        for (k, &c) in chars.iter().enumerate() {
            if c == '_' {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
                continue;
            }
            let previous = k.checked_sub(1).map(|p| chars[p]);
            let boundary = c.is_uppercase()
                && previous.is_some_and(|p| {
                    p.is_lowercase() || p.is_ascii_digit() || (p.is_uppercase() && chars.get(k + 1).is_some_and(|n| n.is_lowercase()))
                });
            if boundary && !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            word.extend(c.to_lowercase());
        }
        if !word.is_empty() {
            words.push(word);
        }
        words
    }

    /// An identifier names a term when the term's words run contiguously through the identifier's,
    /// the last one allowed to continue (`confirm` → `confirmation`).
    fn names_term(identifier: &[String], term: &[String]) -> bool {
        let Some((last, head)) = term.split_last() else { return false };
        identifier.windows(term.len()).any(|window| window[..head.len()] == *head && window[head.len()].starts_with(last.as_str()))
    }

    /// One forbidden name in code: where, which identifier, and the term it names.
    struct Found {
        file: String,
        line: usize,
        identifier: String,
        term: &'static str,
    }

    impl std::fmt::Display for Found {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}:{}: `{}` names `{}`", self.file, self.line, self.identifier, self.term)
        }
    }

    /// Every forbidden name `source`'s code spells outside its test modules, in source order.
    fn forbidden_names(file: &str, source: &str) -> Vec<Found> {
        let terms: Vec<(&'static str, Vec<String>)> = FORBIDDEN.iter().map(|term| (*term, identifier_words(term))).collect();
        let code = blank_test_modules(&strip_non_code(source));
        let mut found = Vec::new();
        for (number, line) in code.lines().enumerate() {
            let mut rest = line;
            while let Some(start) = rest.find(is_identifier_char) {
                let token_len = rest[start..].find(|c: char| !is_identifier_char(c)).unwrap_or(rest.len() - start);
                let token = &rest[start..start + token_len];
                rest = &rest[start + token_len..];
                if token.starts_with(|c: char| c.is_ascii_digit()) {
                    continue; // a number, `0xBEAC` included
                }
                let words = identifier_words(token);
                if let Some((term, _)) = terms.iter().find(|(_, term)| names_term(&words, term)) {
                    found.push(Found { file: file.to_string(), line: number + 1, identifier: token.to_string(), term });
                }
            }
        }
        found
    }

    /// [`forbidden_names`] as the lines a failure prints.
    fn forbidden_lines(file: &str, source: &str) -> Vec<String> {
        forbidden_names(file, source).iter().map(Found::to_string).collect()
    }

    /// **Where the path's code still names the overlay, pinned: `(file, identifier, occurrences)`.**
    ///
    /// ADR-0127 §4 names the separations that are not complete; this is the one the guard found
    /// that §4 does not list. `palw_state_v2.rs` defines the coinbase's output-cap arithmetic, and
    /// two of its terms are the slots it reserves for the overlay's §E payouts
    /// (`PALW_V2_COINBASE_EXTRA_OUTPUTS` and `…_BOUNDED` sum them). They are counts the coinbase
    /// isolation bound shares with the overlay's payout code — no settlement decision, no claim and
    /// no frontier reads them — and moving them beside that code is housekeeping outside this read.
    ///
    /// Exact counts, so the list cannot hide growth (a new use fails) or rot (a removed use fails
    /// until its row goes). A case-sensitive scan for `validator` misses both, which is how a manual
    /// scan found the tree clean.
    const KNOWN_UNSEPARATED: [(&str, &str, usize); 2] =
        [("palw_state_v2.rs", "PALW_V2_MAX_VALIDATOR_PAYOUTS", 3), ("palw_state_v2.rs", "PALW_V2_MAX_DEFERRED_VALIDATOR_PAYOUTS", 2)];

    /// **ADR-0127 Decision 7: the settlement path names no DNS finality, no DNS parameters or state,
    /// no DNS confirmation, no VLT, no stake attestation or stake bond, no active-bond view, no
    /// validator and no beacon** — in code: comments, strings and test modules are not the path —
    /// but for the rows of [`KNOWN_UNSEPARATED`], exactly.
    #[test]
    fn the_settlement_path_names_none_of_the_overlay() {
        let mut found: Vec<Found> = Vec::new();
        let mut functions_scanned = 0;
        for (file, path) in SETTLEMENT_PATH_SOURCES {
            let source = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
            let code = blank_test_modules(&strip_non_code(&source));
            // A guard that scans nothing is green for nothing: each file still has its code.
            let functions = code.split(|c: char| !is_identifier_char(c)).filter(|t| *t == "fn").count();
            assert!(functions > 0, "{file}: the scan left no functions to read");
            functions_scanned += functions;
            found.extend(forbidden_names(file, &source));
        }
        assert!(functions_scanned > 500, "the path's code was scanned, not blanked away: {functions_scanned} functions");
        for (file, identifier, expected) in KNOWN_UNSEPARATED {
            let occurrences = found.iter().filter(|f| f.file == file && f.identifier == identifier).count();
            assert_eq!(occurrences, expected, "{file}: `{identifier}` is pinned at {expected} uses and the code has {occurrences}");
        }
        let violations: Vec<String> = found
            .iter()
            .filter(|f| !KNOWN_UNSEPARATED.iter().any(|(file, identifier, _)| f.file == *file && f.identifier == *identifier))
            .map(Found::to_string)
            .collect();
        assert!(violations.is_empty(), "the PALW settlement path names the overlay (ADR-0127 Decision 7):\n{}", violations.join("\n"));
    }

    /// **The stripper hides what is not code and nothing that is.** Each forbidden word below sits
    /// in a line comment, a doc comment, a nested block comment, a string with an escaped quote, a
    /// byte string, a raw string, a hashed raw string holding a quote, after a char literal of a
    /// quote and in a test module — and is ignored; the same words as identifiers are found, on the
    /// lines they are on, beside a lifetime and a raw identifier that must stay code.
    #[test]
    fn the_guard_reads_code_and_only_code() {
        let hidden = concat!(
            "// beacon in a line comment\n",
            "/// Validator in a doc comment\n",
            "/* vlt /* nested dns_finality */ StakeBond */\n",
            "fn f<'a>(x: &'a str) -> char { let _ = \"a \\\" beacon\"; let _ = b\"validator\"; '\"' }\n",
            "const R: &str = r\"DnsState\";\n",
            "const H: &str = r#\"stake_bond \"quoted\" ActiveBondView\"#;\n",
            "fn g() { let q = '\\''; let r#match = 'v'; let _ = (q, r#match); }\n",
            "#[cfg(test)]\n",
            "mod tests { fn beacon_block() { let validator = 1; } }\n",
            "fn tail() {}\n",
        );
        assert_eq!(forbidden_lines("hidden.rs", hidden), Vec::<String>::new(), "stripped:\n{}", strip_non_code(hidden));
        assert_eq!(strip_non_code(hidden).lines().count(), hidden.lines().count(), "every line keeps its number");
        assert!(blank_test_modules(&strip_non_code(hidden)).contains("fn tail"), "the code after a test module is scanned");

        let spelled = concat!(
            "use crate::dns_finality::X;\n",
            "fn f<'a>(v: &'a ValidatorSet) -> u64 { v.dns_confirmed_anchor_daa }\n",
            "struct S { beacons: u8, key: StakeBondKey, view: kaspa_pq_validator_core::View }\n",
            "#[cfg(test)] fn not_a_module() { let vlt = 0; }\n",
            "let x = 'x'; let dns_params = x;\n",
        );
        assert_eq!(
            forbidden_lines("spelled.rs", spelled),
            vec![
                "spelled.rs:1: `dns_finality` names `dns_finality`",
                "spelled.rs:2: `ValidatorSet` names `validator`",
                "spelled.rs:2: `dns_confirmed_anchor_daa` names `dns_confirm`",
                "spelled.rs:3: `beacons` names `beacon`",
                "spelled.rs:3: `StakeBondKey` names `StakeBond`",
                "spelled.rs:3: `kaspa_pq_validator_core` names `validator`",
                "spelled.rs:4: `vlt` names `vlt`",
                "spelled.rs:5: `dns_params` names `DnsParams`",
            ]
        );
        // Words, not substrings: a word that merely contains a term (`invalidate`, `revlt`, `dnsx`)
        // or a term's words apart (`stake`, `bond`) is no name; a longer form of its last word is,
        // and so is an upper-case constant, which a case-sensitive scan misses.
        let words =
            "fn f(invalidate: u8, revlt: u8, dnsx: u8, stake: u8, bond: u8, beaconless: u8) -> u64 { MAX_VALIDATOR_PAYOUTS }\n";
        assert_eq!(
            forbidden_lines("words.rs", words),
            vec!["words.rs:1: `beaconless` names `beacon`", "words.rs:1: `MAX_VALIDATOR_PAYOUTS` names `validator`"]
        );
        assert_eq!(identifier_words("DNSParams"), vec!["dns", "params"]);
        assert_eq!(identifier_words("palw_v2Validator"), vec!["palw", "v2", "validator"]);
    }
}
