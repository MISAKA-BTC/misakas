mod palw_panel {
    #[path = "/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet/kaspad/src/palw_panel/source_support.rs"]
    pub(crate) mod source_support;
    pub(crate) mod readiness_memory_and_stuck_carrier_tests {
pub(crate) trait CodeNeedleV1 {
    fn find_code(&self, needle: &str) -> Option<usize>;
    fn rfind_code(&self, needle: &str) -> Option<usize>;
    fn contains_code(&self, needle: &str) -> bool {
        self.find_code(needle).is_some()
    }
    fn count_code(&self, needle: &str) -> usize;
}
fn code_span_v1(source: &str, needle: &str) -> Option<std::ops::Range<usize>> {
    // Known call-shape guards only: not encoded-value or general Rust AST equality.
    let compact = |text: &str| {
        let positions: Vec<usize> = text.bytes().enumerate().filter_map(|(at, b)| (!b.is_ascii_whitespace()).then_some(at)).collect();
        positions
            .iter()
            .enumerate()
            .filter_map(|(at, original)| {
                let trailing_argument_comma =
                    text.as_bytes()[*original] == b',' && positions.get(at + 1).is_some_and(|next| text.as_bytes()[*next] == b')');
                (!trailing_argument_comma).then_some((*original, text.as_bytes()[*original]))
            })
            .collect::<Vec<_>>()
    };
    let haystack = compact(source);
    let wanted: Vec<u8> = compact(needle).into_iter().map(|(_, byte)| byte).collect();
    if wanted.is_empty() {
        return Some(0..0);
    }
    haystack
        .windows(wanted.len())
        .position(|window| window.iter().map(|(_, byte)| byte).copied().eq(wanted.iter().copied()))
        .map(|at| haystack[at].0..haystack[at + wanted.len() - 1].0 + 1)
}
impl CodeNeedleV1 for str {
    fn find_code(&self, needle: &str) -> Option<usize> {
        code_span_v1(self, needle).map(|span| span.start)
    }
    fn rfind_code(&self, needle: &str) -> Option<usize> {
        let mut rest = self;
        let mut offset = 0;
        let mut last = None;
        while let Some(span) = code_span_v1(rest, needle) {
            if span.is_empty() { break; }
            last = Some(offset + span.start);
            offset += span.end;
            rest = &rest[span.end..];
        }
        last
    }
    fn count_code(&self, needle: &str) -> usize {
        let mut rest = self;
        let mut count = 0;
        while let Some(span) = code_span_v1(rest, needle) {
            if span.is_empty() {
                break;
            }
            count += 1;
            rest = &rest[span.end..];
        }
        count
    }
}

    }
}
    #[test]
    fn the_panel_wires_the_capture_arm_j1_and_the_tick() {
        use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
        let whole = crate::palw_panel::source_support::former_parent_production_source_v1();
        let source = whole;
        let this = include_str!("/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet/kaspad/src/palw_reporter_filer.rs");
        let body = |signature: &str| {
            let start = this.find(signature).unwrap_or_else(|| panic!("{signature}"));
            &this[start..start + this[start..].find("\n    }\n").expect("its end")]
        };
        // (a)
        let fault = source.find_code("CaptureSamplesV1::FaultAt { leaf, refutation, openings, prompt_opening } => {").expect("the arm");
        let arm = &source[fault..fault + source[fault..].find_code("self.persist_foreign_material(&duty.claim_id, &bytes);").unwrap()];
        let at = |needle: &str| arm.find(needle).unwrap_or_else(|| panic!("the fault arm no longer contains {needle:?}"));
        let order = [
            at("self.note_seat_fault_v1(duty.claim_id, leaf, 1);"),
            at("let refuted = self.capture_arm_filing_v1("),
            at("let court = 'court: {"),
            at("PalwConsensusObjectV2::ShardCourtAccused {"),
            at("Ok(()) => break 'court Some((session_id, object)),"),
            at("self.capture_arm_files_v1("),
            arm.rfind("break 'verdict None;").expect("the arm ends"),
        ];
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
        assert!(!arm[order[2]..order[5]].contains_code("break 'verdict"), "no exit of the accusation's block skips kind 4");
        assert!(
            !arm.contains_code("court_pending.push(") && !arm.contains_code("court_outbox.push_pending(") && !arm.contains_code("accused.insert("),
            "the decision queues, not the arm"
        );
        // The decision is `palw_capture_arm_files_v1` (its order is tested on a book:
        // `the_capture_arm_files_kind_4_first_and_backs_a_replay_filing_it_finds`); the panel runs it
        // over its own door, and nothing else.
        let decide = body("pub(super) fn capture_arm_files_v1(");
        assert!(decide.contains_code("palw_capture_arm_files_v1(filer, accused, court_outbox, duty.claim_id, leaf, refuted, court, |filer, filing| {\n            \
             self.reporter_filer_file_v1(session, filer, filing, bond_key, network_domain)\n        });"
        ));
        assert!(
            !decide.contains_code("court_pending.push(")
                && !decide.contains_code("court_outbox.push_pending(")
                && !decide.contains_code("accused.insert("),
            "the decision queues, not the panel"
        );
        // (b)
        assert!(body("pub(super) fn capture_arm_filing_v1(").contains_code("self.consensus_config.params.palw_rcore_plus_active_at(current_daa),\n            bond_key,\n            duty.executor_bond,"
        ));
        let j1 = body("pub(super) async fn j1_auto_probe_v1(");
        let guard = j1
            .find_code("if !self.consensus_config.params.palw_rcore_plus_active_at(current_daa) || duty.executor_bond == bond_key {")
            .expect("J1's guard");
        assert!(guard < j1.find_code("filer.take_probe(duty.claim_id, bytes, current_daa)").expect("the probe"));
        // (c)
        const TICK: &str = "self.reporter_filer_tick_v1(&session, &mut reporter_filer, &mut court_outbox, &mut accused);";
        assert_eq!(source.matches("self.reporter_filer_tick_v1(").count(), 1);
        let tick = source.find(TICK).expect("the tick, with every seam it prunes");
        let submitter = source.find_code("// --- the collector + submitter's half ---").expect("the submitter");
        let worker = include_str!("/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet/kaspad/src/palw_panel.rs");
        let seat_call = worker.find("self.seat_progress_tick_v1(").expect("seat stage");
        assert!(seat_call < tick && tick < submitter, "seat faults precede the filer tick and carrier phase");
        // The tick also advances terminal filings since the terminal carriage (`tick_with_terminal_v2`); the same seams are pruned.
        assert!(body("pub(super) fn reporter_filer_tick_v1(").contains_code("filer.tick_with_terminal_v2(\n            court_outbox,\n            accused,\n            |entry, gated| \
             session.palw_reporter_filing_read_v1("
        ));
        // (d)
        let hooks: Vec<usize> = source.match_indices("self.j1_auto_probe_v1(").map(|(at, _)| at).collect();
        assert_eq!(hooks.len(), 2, "two hooks");
        for (hook, refused) in hooks.iter().zip([
            // RFC-0001 §A: the capture is checked under its claim's decode rule (`fp_verify_capture_under_job_v1`).
            "if fp_verdict != PalwMaterialVerdictV1::Matches {",
            "if arm == PalwMaterialArmV1::Nothing {",
        ]) {
            let arm = source.find(refused).unwrap_or_else(|| panic!("{refused}"));
            let call = &source[*hook..*hook + source[*hook..].find_code(".await;").expect("awaited")];
            assert!(arm < *hook && source[arm..*hook].lines().count() <= 3, "{refused} hooks J1 at once");
            assert!(call.contains_code("&mut reporter_filer,") && call.contains_code("roots)"), "with the roots the arm refused");
        }
    }
