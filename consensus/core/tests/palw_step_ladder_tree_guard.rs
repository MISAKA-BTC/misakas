//! **The ladder is an ARGUMENT, and the uncapped names are review bait** (ADR-0084 U-08; mainnet
//! audit 2026-09-06, H-4 and M-4).
//!
//! `check_execution_step_refutation_v1` and `check_execution_step_refutation_capped_v1` differ by
//! a suffix and sit ten lines apart. The 2026-09-05 sweep that made the court's walk take the
//! ruleset's ladder covered `consensus/` and left `kaspad/` and `misaka-palw-base0/`; the seat
//! then built its evidence at `2^26` and graded it at `2^22`, and the failure was silent because
//! the refusal landed in a redraw arm. A grep is the only thing that can see "the name three
//! lines up, not this one", so this is a grep with an allowlist: every remaining call of an
//! uncapped step walker must be NAMED here with the reason it is right, which turns a silent
//! reintroduction into a review.
//!
//! It reads the tree's own source deliberately — a guard scoped to one crate would pass on the
//! day the defect returns two crates over, which is exactly how it arrived.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // <root>/consensus/core/ -> <root>
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).expect("consensus/core has two parents").to_path_buf()
}

/// Every crate that can hold a step-tree walk. Adding a crate here is how a new one comes under
/// the guard; a crate that disappears makes the scan assertion fail rather than silently shrink.
const SCANNED: &[&str] = &[
    "consensus/core/src",
    "consensus/src",
    "consensus/pow/src",
    "kaspad/src",
    "misaka-palw-base0/src",
    "misaka-palw-sdk/src",
    "misaka-palw/src",
    "misaka-palw-gateway/src",
    "misaka-palw-agent/src",
    "kaspa-pq-validator-core/src",
];

/// **Tier A — the ADJUDICATORS.** Every one of these has a `_capped_v1` sibling that takes the
/// ruleset's `max_step_leaf_count`. A production call of the uncapped name is a call that grades
/// evidence at the executor's 2^22 on a network whose classes are admitted at 2^26.
const TIER_A: &[&str] = &[
    "check_execution_step_refutation_v1",
    "check_execution_step_refutation_opened_v1",
    "check_step_refutation_v1",
    "check_tiled_decode_token_refutation_v1",
];

/// **Tier B — the ASSEMBLERS and the WALKS.** Same rule, one level down: these build or open a
/// step tree, and building at one ladder while checking at another is the same defect wearing a
/// different name.
const TIER_B: &[&str] = &[
    "base0_refutation_from_capture_v1",
    "base0_binding_from_capture_with_profile_v1",
    "base0_open_fp_interval_v1",
    "base0_open_fp_interval_sparse_v1",
    "base0_verify_fp_interval_opening_v1",
    "base0_verify_fp_interval_opening_with_state_v1",
    "base0_fp_interval_opening_seat_state_v1",
    "base0_material_matches_claim_v1",
    "base0_replay_from_checkpoint_v1",
    "base0_step_merkle_root_v1",
    "step_merkle_root_v1",
    "step_merkle_path_v1",
    "step_merkle_range_siblings_v1",
    "step_opening_v1",
    "step_opening_root_v1",
    "step_opening_from_range_v1",
    "step_range_opening_root_v1",
    "step_range_siblings_from_range_v1",
];

/// `(relative path, called name, why this one is right)`.
///
/// A tracked entry whose file no longer contains that call FAILS, so the list cannot outlive its
/// subjects — the `host_security_tree_guard.rs` rule, for its reason.
const ALLOWED: &[(&str, &str, &str)] = &[
    // ---- the uncapped forwarders' own bodies: this is where the default lives ----
    ("consensus/core/src/palw_step_refute.rs", "check_execution_step_refutation_opened_v1", "the uncapped forwarder IS this call"),
    (
        "consensus/core/src/palw_step_leg.rs",
        "step_merkle_root_v1",
        "PalwStepBindingV2's own leaf-hash root: the binding's own count is the bound",
    ),
    ("misaka-palw-base0/src/legs.rs", "base0_binding_from_capture_with_profile_v1", "the uncapped forwarder's own body"),
    ("misaka-palw-base0/src/fp_interval.rs", "base0_open_fp_interval_v1", "the uncapped forwarder's own body"),
    // ---- trees that are NOT the step tree ----
    (
        "consensus/core/src/palw_prompt_ids_v1.rs",
        "step_merkle_root_v1",
        "the PROMPT-ID tile tree, bounded by n_ctx; PromptLongerThanTheStepTree is its own named refusal (ADR-0081 D3)",
    ),
    ("consensus/core/src/palw_prompt_ids_v1.rs", "step_opening_v1", "the prompt-id tile tree, as above"),
    ("consensus/core/src/palw_prompt_ids_v1.rs", "step_opening_root_v1", "the prompt-id tile tree, as above"),
    (
        "consensus/core/src/palw_step_refute.rs",
        "step_merkle_root_v1",
        "the tiled decode-token event tree (rows and tiles), not the step tree",
    ),
    ("consensus/core/src/palw_step_refute.rs", "step_opening_v1", "the tiled decode-token event tree, as above"),
    ("misaka-palw-base0/src/legs.rs", "step_merkle_root_v1", "the checkpoint leg's own leaf vector, bounded by the checkpoint count"),
    ("misaka-palw-base0/src/legs.rs", "step_opening_v1", "the checkpoint leg, as above"),
    ("misaka-palw-base0/src/fp_interval.rs", "step_opening_v1", "the checkpoint leg, as above"),
    ("misaka-palw-base0/src/fp_interval.rs", "step_opening_root_v1", "opened against binding.checkpoint_count, the checkpoint leg"),
    // ---- CONSENSUS paths whose ladder is a FENCED question, not a bug ----
    (
        "consensus/core/src/palw_carriage.rs",
        "check_execution_step_refutation_v1",
        "adjudicate_step_conviction_carriage_v1: a Stage-1 consensus WEIGHT path (palw_facts::resolve_block_facts_v1). \
         It holds no ruleset and a wider ladder here would change which blocks keep their PALW weight — a fenced \
         consensus change belonging to the ADR-0084 U-08 arming, NOT a node-local repair. Tracked, not fixed.",
    ),
    (
        "consensus/core/src/palw_e2e_adjudicability.rs",
        "check_execution_step_refutation_v1",
        "certify_e2e_family_v1 / certify_e2e_free_prompt_lane_v1, the FamilyCertified transition's grader. Thirty lines \
         above, the same function DELIBERATELY counts leaves at PALW_CONTEXT_LADDER_MAX_STEP_LEAVES ('the certifier holds \
         no ruleset … what it CAN do is refuse to invent one'), and then grades the two refutations at 2^22 — so no drill \
         over a class wider than 2^22 can ever certify, and it fails as HonestRunConvicted. Making the two agree is a \
         VALIDITY change (a refused drill would newly certify) and needs the U-08 fence. Tracked, not fixed.",
    ),
    // ---- no production caller today ----
    (
        "misaka-palw-base0/src/produce.rs",
        "base0_replay_from_checkpoint_v1",
        "base0_anchored_leaf_replay_v1, whose only callers in the tree are its own tests; the capped sibling exists at \
         base0_replay_from_checkpoint_capped_v1 and is what a production caller must use",
    ),
];

fn rust_sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let root = repo_root();
    let mut paths = Vec::new();
    for dir in SCANNED {
        walk(&root.join(dir), &mut paths);
    }
    assert!(paths.len() > 100, "the scan found only {} files — the guard is not looking at the tree", paths.len());
    paths.into_iter().filter_map(|p| std::fs::read_to_string(&p).ok().map(|s| (p, s))).collect()
}

fn relative(path: &Path) -> String {
    path.strip_prefix(repo_root()).unwrap_or(path).display().to_string()
}

/// **A bare `#[cfg(test)]` is NOT a test boundary in this tree, and neither is `mod tests`.**
///
/// This tree has both traps. `qwen25_a16_backend.rs:965` is a `#[cfg(test)]` above a production-
/// shaped `pub(crate) fn` helper, which is why "the first `#[cfg(test)]`" cuts too early — the
/// `inline-cfg-test-misclassifies-test-regions` finding, on record. And its test region is
/// `mod free_prompt_tests`, while `palw_step_refute.rs` spells its `pub(crate) mod tests`, so
/// "the first `mod tests`" cuts too late in one file and never in the other: matching only the
/// bare form declared fifteen of that file's test lines production and would have buried the four
/// real findings.
///
/// The boundary that is true of both is the CONJUNCTION: the first `#[cfg(test)]` whose next
/// non-blank line declares a module, at any visibility and under any name. A file with no such
/// pair is production throughout.
fn production_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    fn is_module_head(line: &str) -> bool {
        let t = line.trim_start();
        let t = t.strip_prefix("pub(crate) ").or_else(|| t.strip_prefix("pub(super) ")).or_else(|| t.strip_prefix("pub ")).unwrap_or(t);
        t.starts_with("mod ")
    }
    let lines: Vec<&str> = source.lines().collect();
    let cut = lines
        .iter()
        .enumerate()
        .find(|(n, l)| {
            l.trim() == "#[cfg(test)]" && lines[n + 1..].iter().find(|next| !next.trim().is_empty()).is_some_and(|next| is_module_head(next))
        })
        .map(|(n, _)| n)
        .unwrap_or(usize::MAX);
    source.lines().enumerate().take(cut).filter(|(_, l)| {
        let t = l.trim_start();
        !(t.starts_with("//") || t.starts_with("///") || t.starts_with("//!") || t.starts_with('*'))
    })
}

fn scan(banned: &[&str], tier: &str) {
    let mut findings = Vec::new();
    let mut matched_allowlist: Vec<(&str, &str)> = Vec::new();
    for (path, source) in rust_sources() {
        let rel = relative(&path);
        for (n, line) in production_lines(&source) {
            for name in banned {
                // A call, not a mention: the name followed by `(`. `_capped_v1` cannot match,
                // because the banned names end in `_v1` and the capped ones end in `_capped_v1`.
                if !line.contains(&format!("{name}(")) {
                    continue;
                }
                // A `pub fn <name>(` is the definition, not a call.
                if line.contains(&format!("fn {name}")) {
                    continue;
                }
                match ALLOWED.iter().find(|(f, c, _)| *f == rel && c == name) {
                    Some((f, c, _)) => matched_allowlist.push((f, c)),
                    None => findings.push(format!("{rel}:{}: calls `{name}` at the DEFAULT ladder", n + 1)),
                }
            }
        }
    }
    assert!(
        findings.is_empty(),
        "ADR-0084 U-08 ({tier}): a step walk must take the ruleset's `max_step_leaf_count`, never the executor's 2^22. \
         The `_capped_v1` sibling of each name takes it. If a call is genuinely right at the default — a different tree, \
         a forwarder's own body, or a consensus path whose ladder is a fenced question — add it to ALLOWED with the \
         reason. Found:\n  {}",
        findings.join("\n  ")
    );
    // The allowlist may not outlive its subjects.
    for (file, call, _) in ALLOWED {
        if !banned.contains(call) {
            continue;
        }
        assert!(
            matched_allowlist.contains(&(file, call)),
            "ALLOWED names {file} / `{call}`, and no production line there calls it any more — delete the entry rather than \
             leaving a permission for a call that no longer exists"
        );
    }
}

#[test]
fn no_production_path_adjudicates_a_step_at_the_default_ladder() {
    scan(TIER_A, "the adjudicators");
}

#[test]
fn no_production_path_builds_or_opens_a_step_tree_at_the_default_ladder() {
    scan(TIER_B, "the assemblers and the walks");
}

/// **The positive half**, so the two tests above cannot be satisfied by deleting the seat's checks.
/// The seat still grades, still opens, and still does both at a number it reads from its ruleset.
#[test]
fn the_seat_still_grades_its_samples_and_does_it_at_the_rulesets_ladder() {
    let panel = std::fs::read_to_string(repo_root().join("kaspad/src/palw_panel.rs")).expect("kaspad/src/palw_panel.rs is in the tree");
    let body = panel.split("fn fp_capture_samples_clear").nth(1).expect("the seat's sample check still exists");
    let body = &body[..body.len().min(4_000)];
    assert!(body.contains("check_execution_step_refutation_capped_v1"), "the seat must still adjudicate its samples");
    assert!(
        panel.contains("self.config.court.max_step_leaf_count()"),
        "and the ladder it grades at must come from the ruleset the node runs, not from a constant"
    );
}

/// **M-4: the ceiling/`ExactBudgetReached` pairing is spelled ONCE.**
///
/// ADR-0074 Decision 7 makes the stop reason a FUNCTION of the executed count — `executed ==
/// limit` must be `ExactBudgetReached`, and `EndOfGeneration` must come with `executed < limit`.
/// Six production sites in `misaka-palw-base0` wrote that pairing by hand and all six wrote it as
/// the ceiling, so no caller could express an early stop and every seat rebuilt an early-stopping
/// claim's context at a budget the claim never ran. `palw_fp_run_facts_for_executed_v1` is the one
/// derivation; this is what stops a seventh hand-written site from being added later.
#[test]
fn the_stop_reason_is_derived_from_the_executed_count_in_exactly_one_place() {
    const NEEDLE: &str = "stop_reason: PalwFpStopReasonV3::ExactBudgetReached";
    const QUALIFIED: &str = "stop_reason: kaspa_consensus_core::palw_freeprompt_v3::PalwFpStopReasonV3::ExactBudgetReached";
    let mut findings = Vec::new();
    for (path, source) in rust_sources() {
        let rel = relative(&path);
        // The derivation itself lives here and is the one place the pairing may be written.
        if rel == "consensus/core/src/palw_fp_execution_v3.rs" {
            continue;
        }
        for (n, line) in production_lines(&source) {
            if line.contains(NEEDLE) || line.contains(QUALIFIED) {
                findings.push(format!("{rel}:{}", n + 1));
            }
        }
    }
    assert!(
        findings.is_empty(),
        "ADR-0074 Decision 7: the stop reason follows from the executed count, so it is derived by \
         `palw_fp_run_facts_for_executed_v1` and written nowhere else. Hand-spelled at:\n  {}",
        findings.join("\n  ")
    );
    // The positive half: the derivation exists, and it can still say EndOfGeneration.
    let src = std::fs::read_to_string(repo_root().join("consensus/core/src/palw_fp_execution_v3.rs")).expect("the derivation's file");
    assert!(src.contains("pub fn palw_fp_run_facts_for_executed_v1"), "the one derivation must exist");
    assert!(src.contains("PalwFpStopReasonV3::EndOfGeneration"), "and it must be able to name the early stop");
}
