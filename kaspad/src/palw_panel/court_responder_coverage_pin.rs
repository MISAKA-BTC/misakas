//! **`palw_panel`'s `court_responder_coverage_pin` module**, moved out of `palw_panel.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

/// **PIN THE LIMITATION, NOT THE BEHAVIOUR: no shipped binary files a `CourtAttnRootClaimed`**
/// (ADR-0082 Decision 2; mainnet audit 2026-09-06, C-2/H-5).
///
/// §10.6 gave a fused terminal a CLOCK — the responder owes the root claim, and silence past
/// the rung convicts it — on the premise that the responder can file one. That premise is what
/// `Params::palw_court_responder_coverage` exists to suspend, and the exemption is a
/// placeholder for a responder rather than a rule anybody wants: the whole k-ary dissection
/// responder (a panel arm, plus a backend verb producing the fused site's root, its arity, its
/// binding, its opened output tile and its operand openings) is a FEATURE nobody has built.
///
/// This is the trigger to retire the exemption by activation. It scans the panel's own source
/// — the only file where a responder could live, since `palw_da_duties_v2` and
/// `palw_court_duties_v2` are both read here and nowhere else in this binary — and it goes RED
/// the day the identifier appears anywhere but the naming table. The day it does, the fix is
/// not to relax this test: it is to schedule `palw_court_responder_coverage` off, on a fresh
/// card or at a height, so the clock the ADR designed starts meaning what it says.
///
/// A comment that mentions the object will also turn it red. That is the intended cost: the
/// question "can a party in the field actually make this move" must be re-answered by hand,
/// and it is cheap to answer. This module's own text is excluded, or the pin would be counting
/// itself.
/// **ADR-0098 Decision 2, pinned where it lives.** The seat's verdict block consults the fault
/// ledger before its first arm, again between the free-prompt interval arm and the capture arm,
/// and again before the half-window tail; the capture arm's proven fault is recorded; and the
/// checkpoint root that does not recompute is recorded. Each of these was once missing, and a
/// missing one is silent: the seat simply certifies, or accuses, a claim it found a lie in.
#[test]
fn a_seat_that_found_a_fault_consults_the_ledger_before_every_verdict() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;

    let source = crate::palw_panel::source_support::former_parent_production_source_v1();
    let start = source.find_code("let verdict = 'verdict: {").expect("the verdict block");
    let end = start + source[start..].find_code("let Some(verdict) = verdict else { continue };").expect("its end");
    let block = &source[start..end];
    let at = |needle: &str| block.find(needle).unwrap_or_else(|| panic!("the verdict block no longer contains {needle:?}"));
    let gates: Vec<usize> = block.match_indices("if self.seat_found_fault_v1(&duty.claim_id) {").map(|(i, _)| i).collect();
    assert_eq!(gates.len(), 3, "three gates: before every arm, before the capture arm, before the tail");
    let replay = at(".interval_seat_outcome_v1(");
    let capture = at("palw_fp_capture_decode_v1(");
    // The half-window tail is the block's last `Unavailable` (the SEAT-S4 route's material wait and
    // the replaying partial seat's precede it, each past SEAT-R alone).
    let tail = block.rfind("break 'verdict Some(PalwReceiptVerdictV2::Unavailable").expect("the tail");
    assert!(gates[0] < replay, "the first gate precedes the first replay");
    assert!(gates.iter().any(|g| *g > replay && *g < capture), "a gate between the interval arm and the capture arm");
    assert!(gates.iter().any(|g| *g > capture && *g < tail), "a gate before the half-window tail");
    let fault = at("CaptureSamplesV1::FaultAt { leaf, refutation, openings, prompt_opening } => {");
    assert!(
        block[fault..].contains_code("self.note_seat_fault_v1(duty.claim_id, leaf, 1);"),
        "the capture arm records the fault it proves"
    );
    // ADR-0100 Decision 1: and files the one thing a seat that found a lie files — the
    // accusation — only where the chain's own refutation ladder can adjudicate it.
    let filed = at("PalwConsensusObjectV2::ShardCourtAccused {");
    assert!(filed > fault, "the accusation is built inside the fault arm");
    assert!(
        block[fault..filed].contains_code("refutation.binding.step_leaf_count > ladder"),
        "a claim above the chain's refutation ladder is recorded, not accused"
    );
    let mismatch = source.find_code("this seat's own recompute reaches").expect("the checkpoint mismatch");
    assert!(
        source[mismatch..].find_code("self.note_seat_fault_v1(duty.claim_id, 0, 0);").is_some_and(|d| d < 1_200),
        "the state root that does not recompute is recorded"
    );
}

/// **ADR-0103 Decision 1, pinned where it lives: under the held fence the challenger's half
/// opens no bisection, and the seat grades what the chain reads.** The chain refuses
/// `CourtOpened` for every claim once the fence is armed, so the one place the panel builds one
/// sits behind the fence's own predicate; and the capture arm's accusation carries the prompt
/// tile the carriage built, or on a Merkle-form network it convicts nobody (the held drill,
/// 2026-09-11).
#[test]
fn under_the_held_fence_the_panel_opens_no_bisection_and_accuses_with_the_carriage() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;

    let source = crate::palw_panel::source_support::former_parent_production_source_v1();
    let gate = source.find_code("let bisection_is_played = !self.consensus_config.params.palw_held_context_active_at(current_daa);");
    // A construction opens its braces on a line of its own; a pattern (`{ claim, .. }`) does not.
    const BUILT: &str = "PalwConsensusObjectV2::CourtOpened {\n";
    let opened = source.find(BUILT).expect("the challenger half still builds its opening");
    assert_eq!(source.matches(BUILT).count(), 1, "one place builds a bisection");
    let gate = gate.expect("the challenger half reads the held fence");
    assert!(gate < opened, "the fence is read before the bisection is built");
    assert!(
        source[gate..opened].contains_code("if bisection_is_played && (self.config.challenge || !seat_faulted.is_empty()) {"),
        "and the bisection is built only where one is played"
    );
    let sampler = &source[source.find_code("fn fp_capture_samples_clear(").expect("the sampler")..];
    let sampler = &sampler[..sampler.find("\n    }\n").expect("its end")];
    assert!(sampler.contains_code("palw_refutation_prompt_carriage_v1(prompt_ids_form, refutation)"), "the sampler carries");
    assert!(
        sampler.contains_code("check_execution_step_refutation_opened_capped_v1(&refutation, &proven, prompt_opening.as_ref(), ladder)"),
        "and grades the carriage it built"
    );
    assert!(source.contains_code("prompt_ids_opening: prompt_opening,"), "the accusation carries the tile the sampler opened");
}

/// **DoS audit 2026-09-24, #4, pinned where it lives: the capture sampler and the interval
/// route take the ledger's reservation, and the sampler re-executes a capture once.**
///
/// Before the fix the capture arm offloaded `fp_capture_samples_clear` with no reservation
/// (`reserve_replay_v1` was called by the court, the full-seat replay and the partial seat
/// only), the interval route replayed with none, and each of the sampler's up to 32 draws
/// called `refutation_for_free_prompt_index` — a whole re-execution of a fold per draw. Each of
/// the assertions below reads a line that did not exist then.
#[test]
fn the_capture_sampler_and_the_interval_route_reserve_and_the_sampler_prepares_once() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;

    let source = crate::palw_panel::source_support::former_parent_production_source_v1();
    // The capture arm: refused by name, priced dense, reserved, THEN offloaded, and the guard
    // moves into the blocking task so it outlives the samples.
    let call = source.find_code("Self::fp_capture_samples_clear(\n").expect("the sampler's one call site");
    let arm_start = source[..call].rfind("palw_fp_capture_decode_v1(").expect("the capture arm");
    let arm = &source[arm_start..call];
    // Read with the whitespace squeezed out: a call the formatter splits across lines is the same call.
    let squeezed: String = arm.chars().filter(|c| !c.is_whitespace()).collect();
    let need = squeezed.find_code(".whole_capture_memory_need_v1(").expect("the capture is priced as a whole capture");
    let reserve = squeezed.find_code("self.reserve_replay_at_widest_run_v1(\"full-seatcapture\",backend.as_mut(),").expect("and reserved");
    let offloaded = squeezed.rfind("offload(backend,move|b|{").expect("then offloaded");
    assert!(need < reserve && reserve < offloaded, "priced, reserved, offloaded — in that order");
    assert!(squeezed[reserve..].contains_code("break'verdictNone;"), "a refusal defers the duty and files nothing");
    assert!(squeezed[offloaded..].contains_code("let_held_for_the_samples=reserved;"), "the guard lives as long as the samples");
    // The sampler prepares once and draws from the prover.
    let sampler = &source[source.find_code("fn fp_capture_samples_clear(").expect("the sampler")..];
    let sampler = &sampler[..sampler.find("\n    }\n").expect("its end")];
    assert!(sampler.contains_code("backend.free_prompt_leaf_prover_v1(capture, prompt_token_ids)"), "one preparation");
    assert!(sampler.contains_code("prover.refutation_for_index(index)"), "every draw reads the prepared capture");
    assert!(!sampler.contains_code("refutation_for_free_prompt_index("), "no draw re-executes the job");
    // The interval route: reserved before its first replay.
    let route = &source[source.find_code("async fn interval_seat_outcome_v1(").expect("the interval route")..];
    let reserved =
        route.find_code("self.reserve_replay_at_widest_run_v1(\"interval-seat\", backend.as_mut(),").expect("the route reserves");
    let first_replay = route.find_code("offload(backend, move |b|").expect("its first replay");
    assert!(reserved < first_replay, "the reservation precedes every replay of the route");
    assert!(route[..first_replay].contains_code("let _held_for_the_intervals = if held > 0 {"), "held for the route's life");
}

/// **ADR-0133 S1 (4), pinned where it lives: a court close of an accused leaf resumes the V2
/// segment from its published checkpoint before assembling the per-leaf refutation.**
#[test]
fn m1_a_court_close_spends_no_discarded_segment_resume() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;

    let source = crate::palw_panel::source_support::former_parent_production_source_v1();
    let close = source.find_code("the close does not assemble from this capture").expect("the close arm");
    let reserve = source[..close].rfind("let court_reserved =").expect("the close's reservation");
    let arm = &source[reserve..close];
    assert!(arm.contains_code("offload(backend, move |b|"), "the close still runs off the runtime");
    assert!(!arm.contains_code(".replay_accused_segment_v1("), "M1: no whole-segment replay whose result nothing reads");
    assert!(arm.contains_code("refutation_for_free_prompt_index") && arm.contains_code("refutation_for_index"), "the refutation is the close");
}

/// **ADR-0133 S1 (3), pinned where it lives: a free-prompt partial seat resumes assigned
/// segments and attests the mask before the interval fallback.**
#[test]
fn a_free_prompt_partial_seat_resumes_assigned_segments_past_v2() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;

    let source = crate::palw_panel::source_support::former_parent_production_source_v1();
    let start = source.find_code("Past Verification V2 a partial seat resumes assigned segments first").expect("the free-prompt S1 arm");
    let capture = start + source[start..].find_code("palw_fp_capture_decode_v1(").expect("the capture arm");
    let arm = &source[start..capture];
    let resume = arm.find_code("palw_v2_try_partial_resume_v1(").expect("S1 runtime resume is on the live lane");
    let interval = arm.find_code(".interval_seat_outcome_v1(").expect("the interval fallback");
    assert!(resume < interval, "segment resume precedes the interval seat");
    assert!(arm.contains_code("palw_verification_v2_at(current_daa)"), "resume is past Verification V2");
    assert!(arm.contains_code("segments_attested = Some(mask)"), "a partial seat attests its mask");
    assert!(arm.contains_code("!= assignment.full_seat"), "a partial seat does not fall through to genesis replay");
    assert!(arm.contains_code("break 'verdict None"), "a partial seat that cannot resume files nothing");
    let resume_fn = source.find_code("async fn palw_v2_try_partial_resume_v1(").expect("the S1 resume helper");
    let helper_end = source[resume_fn..].find_code("/// **Ask the network for the openings").expect("the helper ends");
    let resume_body = &source[resume_fn..resume_fn + helper_end];
    assert_eq!(
        resume_body.matches("PalwV2SeatPathV1::FullReplay").count(),
        1,
        "only the designated full-replay seat may walk the whole job"
    );
    assert!(resume_body.contains_code("== assignment.full_seat"), "FullReplay is the full seat's path");
    assert!(resume_body.contains_code("PalwV2SeatPathV1::Waiting"), "a partial that cannot open waits");
}

/// **ADR-0133 S1/S2 with ADR-0152's X22, pinned where they live: the collector hands the licence
/// doors to `palw_licence_offer_order_v1` — coverage, V1, S2 — under the R-core+ fence.** Past it
/// they are asked in that order (a set at `basis_k ≥ 2` before the fast path that lands on Q-5's
/// gate); below it in the order this test pinned before X22, coverage, then the optimistic
/// licence, then the V1 quorum (`x22_offers_a_replay_backed_licence_before_s2_and_asks_lazily`
/// runs both orders; T45 — the backed V1 before S2 over the real assemblers — is the processor's
/// `t12_t45_the_collector_prefers_a_backed_v1_over_s2_and_never_waits_on_an_unbacked_one`).
#[test]
fn the_collector_assembles_coverage_then_v1_then_optimistic() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;

    let source = crate::palw_panel::source_support::former_parent_production_source_v1();
    let call = source.find_code("kaspa_consensus_core::palw_panel_v2::palw_licence_offer_order_v1(").expect("the order");
    let call = &source[call..call + 1_200];
    assert!(call.contains_code("self.consensus_config.params.palw_rcore_plus_active_at(current_daa),"), "the fence's order");
    let coverage = call.find_code(".palw_v2_receipt_coverage_assemble(claim, v3.clone())").expect("S1 coverage");
    let v1 = call.find_code(".palw_v2_receipt_quorum_assemble(claim, pool.clone())").expect("V1 quorum");
    let optimistic = call.find_code(".palw_v2_optimistic_assemble(claim, v3.clone())").expect("S2 optimistic");
    assert!(coverage < v1 && v1 < optimistic, "handed as coverage, V1, S2");
}

/// **ADR-0111, pinned where it lives: a named leaf is pursued, a leaf request is served, and a
/// held accusation is answered in its unit.** The first fault gate of the verdict block pursues
/// the seat's named leaf before it files nothing; the interval resolver answers a leaf-evidence
/// request before any interval arithmetic; the authorizer verifies a leaf request's own signed
/// message; and the data-availability loop answers a held unit before the event path reads an
/// event index the held sentinel is not.
#[test]
fn the_panel_pursues_a_named_leaf_serves_its_evidence_and_answers_the_held_court() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;

    let source = crate::palw_panel::source_support::former_parent_production_source_v1();
    let gate = source.find_code("if self.seat_found_fault_v1(&duty.claim_id) {").expect("the first fault gate");
    let pursued = source[gate..].find_code(".pursue_named_leaf_v1(").expect("the gate pursues");
    let broke = source[gate..].find_code("break 'verdict None;").expect("and files nothing");
    assert!(pursued < broke, "the pursuit runs before the gate files nothing");
    let resolver = source.find_code("fn open_retained_interval(").expect("the resolver");
    let served = source[resolver..].find_code("self.retained_leaf_evidence_v1(").expect("a leaf request is served");
    let arithmetic = source[resolver..].find_code("base0_fp_block_leaves_request_decode_v1(interval_index)").expect("the interval kinds");
    assert!(served < arithmetic, "a leaf-evidence request is answered before any interval arithmetic");
    assert!(source.contains_code("crate::palw_fp_seat::palw_fp_verify_leaf_request_v1("), "the authorizer verifies the leaf's message");
    let loop_start = source.find_code("let da_duties = session.palw_da_duties_v2(vec![bond_key]);").expect("the DA loop");
    let held = source[loop_start..].find_code("if let Some(missing) = duty.held_missing {").expect("the held unit branch");
    let event = source[loop_start..].find_code("palw_da_event_index_parts_v1(duty.missing_event_index)").expect("the event path");
    assert!(held < event, "a held unit is answered before an event index is read");
    // Every unit the court can name is answered, and with no catch-all arm — so a unit added to
    // `PalwHeldMissingV1` is a compile error there rather than an executor slashed for silence.
    // The arms are the ONE held builder's (`palw_da_held_disclosure_from_capture_v1`, P2-7),
    // which the v1 answer and R-core's both reach.
    let answer = &source[source.find_code("fn held_da_answer_v1(").expect("the held answer")..];
    let answer = &answer[..answer.find("\n    }\n").expect("its end")];
    assert!(answer.contains_code("self.fp_held_da_disclosure_v1(session, claim, missing)?"), "the v1 answer uses the shared builder");
    let body = |head: &str| {
        let at = &source[source.find(head).unwrap_or_else(|| panic!("{head}"))..];
        &at[..at.find("\n    }\n").expect("its end")]
    };
    assert!(
        body("fn fp_held_da_disclosure_v1(").contains_code("self.fp_held_da_disclosure_of_v1("),
        "the free-prompt loader builds through the one free-prompt held source"
    );
    // Module-level functions end at a column-0 brace.
    let top = |head: &str| {
        let at = &source[source.find(head).unwrap_or_else(|| panic!("{head}"))..];
        &at[..at.find("\n}\n").expect("its end")]
    };
    assert!(
        body("fn fp_held_da_disclosure_of_v1(").contains_code("palw_fp_held_disclosure_v1("),
        "which builds through the one free-prompt held builder"
    );
    assert!(
        top("fn palw_fp_held_disclosure_v1(").contains_code("palw_da_held_disclosure_from_capture_v1("),
        "which builds through the core's held builder"
    );
    let rcore = body("async fn rcore_da_answers_v1(");
    for reached in ["palw_da_claim_answers_v1(", "palw_da_built_answer_carriage_v1(", "next_answer_object_v2("] {
        assert!(rcore.contains(reached), "R-core's answer reaches {reached}");
    }
    let unit = top("pub(crate) fn palw_da_unit_answer_v1(");
    assert!(unit.contains_code("palw_fp_held_disclosure_v1("), "a free-prompt claim's held unit, through the one FP held builder");
    assert!(unit.contains_code("palw_da_held_disclosure_from_capture_v1("), "and an attempt claim's through the core's held builder");
    assert!(top("pub(crate) fn palw_da_claim_answers_v1(").contains_code("palw_da_unit_answer_v1("), "every unit of a claim");
    let core = include_str!("../../../consensus/core/src/palw_da_rcore_v1.rs");
    let builder = &core[core.find_code("pub fn palw_da_held_disclosure_from_capture_v1(").expect("the held builder")..];
    let builder = &builder[..builder.find("\n}\n").expect("its end")];
    for unit in [
        "PalwHeldMissingV1::StepLeaf { leaf } =>",
        "PalwHeldMissingV1::PromptIdsTile { tile } =>",
        "PalwHeldMissingV1::StateChunk { checkpoint, chunk } =>",
        "PalwHeldMissingV1::StepRange { first, count } =>",
    ] {
        assert!(builder.contains(unit), "the held builder has no arm {unit}");
    }
    assert!(!builder.contains_code("other =>") && !builder.contains_code("_ =>"), "no held unit falls to a catch-all refusal");
}

/// **ADR-0121 §7, pinned where the seat's fault is handled**: a `FaultInRange` whose address is
/// not a whole block of the opening asks for BOTH edges of the interval — the address cannot say
/// which edge the lie is in — and names the leaf from them through the edge verb, before the
/// whole-block path is reached.
#[test]
fn a_fault_at_an_intervals_edge_asks_for_both_edges_and_names_from_them() {

    let source = crate::palw_panel::source_support::former_parent_production_source_v1();
    let arm = &source[source.find("PalwFpIntervalVerdictV1::FaultInRange { first_leaf_index, leaf_count } => {").expect("the arm")..];
    let edge = arm.find("let (left, right) = v4.range.edges_v1(step_leaf_count);").expect("the edges are read off the opening");
    let named = arm.find("b.fp_name_the_edge_leaf_v1(").expect("and named from");
    let whole_block = arm.find("b.fp_name_the_leaf_v1(").expect("the whole-block path");
    assert!(edge < named && named < whole_block, "the edge path comes first");
    assert!(arm[edge..named].contains("[left, right]"), "both edges are asked for");
}

/// **ADR-0085 Decision 3 against a liar, pinned where the close's inputs are gathered**: the
/// close from served intervals holds the executor's leaves for the disputed leaf's block beside
/// the opening, and asks for them on the interval lane when it does not — without them a lying
/// interval's range never walks to its committed root, and the close refuses every liar.
#[test]
fn the_close_from_served_intervals_holds_and_asks_for_the_disputed_leafs_block() {

    let source = crate::palw_panel::source_support::former_parent_production_source_v1();
    let gather = &source[source.find("fn close_source_from_served_intervals_v1(").expect("the close's gathering")..];
    let gather = &gather[..gather.find("\n    }\n").expect("its end")];
    assert!(
        gather.contains("base0_fp_block_request_number_v1(&primary_v4, block)")
            && gather.contains("base0_fp_block_leaves_request_index_v1(wanted, number)"),
        "the disputed leaf's block is named, by the number its class counts in (ADR-0121 Decision 3)"
    );
    assert!(gather.contains("return Err(CloseFromServedV1::Missing(vec![index]));"), "and asked for when it is not held");
    assert!(gather.contains("held.extend(served_block);"), "and handed to the close when it is");
}

/// **ADR-0093 as built: the panel FILES the fused terminal's root claim now — and the exemption
/// that stood in for it stays unarmed.**
///
/// This pin used to count the panel's mentions of `CourtAttnRootClaimed` at two (the match arm
/// and its name) and said what to do the day that moved: not relax the count, but retire
/// `Params::palw_court_responder_coverage`, whose mercy was "a placeholder for a responder".
/// The responder is here — the root claim, the rounds, the challenger's choice and the
/// bottom's close, every number computed from a capture's evidence by the court's own kernels
/// — so the fence is retired the only way a fence that was never armed can be: it stays `None`
/// on every preset, and this test says so. Arming it now would excuse a silence the responder
/// no longer has a reason for. The producer still files no court move.
#[test]
fn the_panel_files_the_fused_terminals_moves_and_the_responder_exemption_stays_unarmed() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;

    let whole = crate::palw_panel::source_support::former_parent_production_source_v1();
    let panel = whole;
    let producer = include_str!("../palw_producer.rs");
    for built in [
        "PalwConsensusObjectV2::CourtAttnRootClaimed {",
        "PalwConsensusObjectV2::CourtAttnDissected {",
        "PalwConsensusObjectV2::CourtAttnChildChosen {",
        "PalwCourtVerdictProofV2::AttnDissection {",
    ] {
        assert!(panel.contains(built), "the panel builds `{built}` — the dissection's move");
    }
    assert!(panel.contains_code("b.attn_site_evidence("), "and every number comes from the capture's evidence");
    // ADR-0093 Decision 7, in PRODUCTION code (cut at the test modules, not at the first
    // `#[cfg(test)]`): the close reads the accused's tile off the chain and hands it to the
    // evidence, so a forged fold's bottom is built from the root claim the accused filed.
    let production = whole;
    let close = production.find_code("let accused_filing = if mv == AttnMove::Close").expect("the close reads the accused's filing");
    assert!(production[close..].contains_code("attn_root_filings_from_chain_v1(c, sid, not_before, span)"), "off the chain");
    assert!(production[close..].contains_code("accused_filing.as_ref().map(|f| &f.out_tile)"), "its tile into the capture's evidence");
    assert!(
        production[close..].contains_code(".attn_site_evidence_from_filing(filing, narrowed, carried.as_deref())"),
        "and the filing alone where the capture cannot bottom the accused, or is not held"
    );
    // ADR-0093 Decision 8: past its fence the responder files move 1 with its anchor.
    assert!(production.contains_code("PalwConsensusObjectV2::CourtAttnRootClaimedAnchored {"), "the anchored root claim is filed");
    assert!(production.contains_code(".filter(|_| params.palw_attn_anchored_root_active_at(current_daa))"), "past its fence only");
    assert_eq!(producer.matches("CourtAttnRootClaimed").count(), 0, "the producer does not file court moves at all");
    for (name, params) in [
        ("mainnet", kaspa_consensus_core::config::params::MAINNET_PARAMS),
        ("testnet", kaspa_consensus_core::config::params::TESTNET_PARAMS),
        ("simnet", kaspa_consensus_core::config::params::SIMNET_PARAMS),
        ("devnet", kaspa_consensus_core::config::params::DEVNET_PARAMS),
    ] {
        assert!(
            params.palw_court_responder_coverage.is_none(),
            "{name}: the responder exemption is retired — a fused terminal's silence is the responder's to answer"
        );
    }
}
