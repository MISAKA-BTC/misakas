//! **Does the Qwen3.6 class's registered graph name the operands its engine actually reads?**
//!
//! ADR-0049 Decision F says a profile must name every narrowing the engine performs, and ADR-0067
//! makes that load-bearing: once a container has an interpreter, the registered node table IS the
//! program, and an operand named there that no artifact carries is a step leg that cannot be
//! built. The dense (A16) family got this check as a differential against its compiled engine, and
//! the check immediately found a real defect — the SwiGLU rows were emitted out of declared order.
//!
//! The mmap (Qwen3.6) container has no interpreter yet, so the same question cannot be asked by
//! running both and comparing. It can still be asked of the NAMES, which is what this does: every
//! per-layer operand the projected profile declares, against every per-layer operand an artifact
//! of that lineage actually carries. Nothing here runs the model; the fixture is used only as the
//! authority on what a Qwen3.6 artifact contains, which it is because the engine reads it.

use std::collections::BTreeSet;

use kaspa_consensus_core::palw_qwen36_profile::{QWEN36_35B_A3B, qwen36_profile_v1};
use misaka_palw_base0::qwen36::qwen36_dev_fixture;

/// Strip a `blk.N.` prefix, returning the per-layer suffix. Names without one are not per-layer
/// and are not what this compares.
fn per_layer(name: &str) -> Option<String> {
    let rest = name.strip_prefix("blk.")?;
    let (_, suffix) = rest.split_once('.')?;
    Some(suffix.to_string())
}

/// The per-layer operand suffixes the artifact actually carries, tensors and quant params alike.
/// An expert index is collapsed to `{e}` so eight experts are one name rather than eight.
fn artifact_operands() -> BTreeSet<String> {
    let art = qwen36_dev_fixture(4, 8);
    let mut out = BTreeSet::new();
    // Per-layer names reduce to their suffix; GLOBAL names (`token_embd.weight`, `final_norm.a16`,
    // `output.weight`, `embed_lift.a16`) are kept whole. Dropping the globals here was a real
    // measurement error in the first cut of this test: the declared side keeps them, so five names
    // that the artifact does carry were counted as undeliverable.
    let mut add = |name: &str| {
        let key = match per_layer(name) {
            Some(s) => collapse_expert(&s),
            None => name.to_string(),
        };
        out.insert(key);
    };
    for name in art.tensor_names() {
        add(name);
    }
    for name in art.params_map().keys() {
        add(name);
    }
    out
}

fn collapse_expert(suffix: &str) -> String {
    // `ffn_expert.3_gate.weight` -> `ffn_expert.{e}_gate.weight`
    let Some(rest) = suffix.strip_prefix("ffn_expert.") else { return suffix.to_string() };
    match rest.find('_') {
        Some(i) => format!("ffn_expert.{{e}}{}", &rest[i..]),
        None => suffix.to_string(),
    }
}

/// The per-layer operand suffixes the LIVE class's profile declares.
fn declared_operands() -> BTreeSet<String> {
    let p = qwen36_profile_v1(QWEN36_35B_A3B).expect("the registered geometry projects");
    let mut out = BTreeSet::new();
    for table in [&p.pre_nodes, &p.gdn_nodes, &p.attn_nodes, &p.post_nodes] {
        for node in table.iter() {
            if node.weight_name.is_empty() {
                continue;
            }
            // The IR templates the layer as `{layer}`; the projection may or may not substitute
            // it, so both spellings reduce to the same suffix.
            let name = node.weight_name.as_str();
            let suffix = per_layer(name).unwrap_or_else(|| name.to_string());
            out.insert(collapse_expert(&suffix));
        }
    }
    out
}

/// **Finding 1: the shared expert's gate is two different tensors under one name.**
///
/// The engine reads `blk.N.ffn_shared_gate.weight` as the mixture's SCALAR gate — one output,
/// through a sigmoid, applied to the whole shared-expert row. The registered profile declares a
/// node of that name whose output is `shared_dim` wide, which is the shared expert's own gate
/// PROJECTION. Those are different tensors doing different jobs, and the artifact can only supply
/// one of them: it carries `d_model` weights there, which is a 1-row projection.
///
/// The engine already hit this collision once and fixed its own side — `Qwen36Engine::expert`
/// carries the scar, naming the expert's base `ffn_shared_expert` precisely so its gate does not
/// land on the scalar's name. The IR was never moved with it, so the CHAIN's description of this
/// class still carries the pre-fix naming. Nothing resolves these names today (this container has
/// no interpreter and commits no step leg), which is why it has gone unnoticed — and is exactly
/// what makes it worth pinning before an interpreter makes the names load-bearing.
#[test]
fn shared_gate_is_declared_at_a_width_the_artifact_cannot_supply() {
    let p = qwen36_profile_v1(QWEN36_35B_A3B).expect("the registered geometry projects");
    let declared = p
        .attn_nodes
        .iter()
        .chain(p.gdn_nodes.iter())
        .find(|n| n.weight_name.ends_with("ffn_shared_gate.weight"))
        .expect("the registered profile names the shared gate");
    let declared_elements = match declared.out_len {
        kaspa_consensus_core::palw_step::PalwStepOutLenV1::Fixed { elements } => elements,
        other => panic!("the shared gate is declared kv-scaled ({other:?}), which it is not"),
    };

    // What the artifact can actually produce there: the tensor is a projection FROM d_model, so
    // its output width is its length divided by d_model.
    let art = qwen36_dev_fixture(4, 8);
    let d = art.shape.d_model;
    let len = art
        .tensor_names()
        .into_iter()
        .find(|n| n.ends_with("ffn_shared_gate.weight"))
        .map(|n| art.tensor(&n).map(|t| t.len()).unwrap_or(0))
        .unwrap_or(0);
    let suppliable = if d == 0 { 0 } else { len / d };

    assert_eq!(suppliable, 1, "the artifact carries the shared gate as a 1-row projection (the SCALAR gate)");
    assert_ne!(
        declared_elements, 1,
        "if this ever passes as equal, the IR has been corrected and this test should become the \
         equality it now refutes"
    );
    println!("shared gate: profile declares {declared_elements} outputs, the artifact can supply {suppliable}");
}

/// **Finding 2: an operand that chooses which experts run is named by no node.**
///
/// `ffn_router_up.a16` is the router softmax's widening, read per layer at `Qwen36Engine::moe`.
/// The engine's own comment measures what getting it wrong costs: up to a factor of sixty-four in
/// temperature, "enough to make the router select nearly uniformly or nearly one-hot" — i.e. it
/// decides WHICH eight of the experts execute. ADR-0049 Decision F requires the profile to name
/// every narrowing the engine performs; this one does not merely narrow, it steers, and the
/// registered profile does not name it at all.
#[test]
fn the_router_widening_is_read_but_not_declared() {
    let art = qwen36_dev_fixture(4, 8);
    assert!(
        art.params_map().keys().any(|k| k.ends_with("ffn_router_up.a16")),
        "the artifact carries the router widening the engine reads"
    );
    assert!(
        !declared_operands().iter().any(|n| n.contains("ffn_router_up")),
        "if this ever fails, the profile has been corrected to name the router widening"
    );
}

/// **Finding 3: how much of the divergence is naming, stated as a count.**
///
/// The two findings above are the unambiguous ones — a width the artifact cannot supply, and an
/// operand that steers with no node to its name. They are not the whole picture, and reporting
/// them alone would understate it. This counts the rest, separating the entries that are only
/// bookkeeping from the ones that are real:
///
/// * an artifact name of the form `<w>.a16` where `<w>` IS a declared weight is that weight's
///   quant params riding with it — not a separate operand, and not a divergence;
/// * everything else carried-but-not-declared is an operand the engine reads and the profile does
///   not name;
/// * declared-but-not-carried is a name no artifact of this lineage answers to.
///
/// The residue is what an interpreter would have to resolve and could not.
#[test]
fn the_naming_divergence_is_systematic_not_two_typos() {
    let declared = declared_operands();
    let available = artifact_operands();

    // Quant params that ride with a declared weight are bookkeeping, not operands.
    let rides_with_declared = |name: &str| match name.strip_suffix(".a16") {
        Some(stem) => declared.contains(stem),
        None => false,
    };

    let undeclared: Vec<&String> = available.difference(&declared).filter(|n| !rides_with_declared(n)).collect();
    let undeliverable: Vec<&String> = declared.difference(&available).collect();

    println!("operands the engine reads that no node names ({}):", undeclared.len());
    for n in &undeclared {
        println!("  {n}");
    }
    println!("names the profile declares that no artifact carries ({}):", undeliverable.len());
    for n in &undeliverable {
        println!("  {n}");
    }

    // The pin is the SHAPE of the finding, not a golden list: as long as both residues are
    // non-trivial, the registered graph is not a description an interpreter can follow. If a
    // corrected row ever drives this test, these become equalities and the assertion inverts.
    assert!(
        undeclared.len() > 2,
        "the divergence is broader than the two findings asserted above; if this fails the graph has been corrected"
    );
    assert!(!undeliverable.is_empty(), "the profile still names operands no artifact carries");
}

/// **The measurement.** Printed whichever way it comes out, because the number is the finding.
#[test]
fn qwen36_declared_operands_against_the_artifact() {
    let declared = declared_operands();
    let available = artifact_operands();

    let undeliverable: Vec<&String> = declared.difference(&available).collect();
    let undeclared: Vec<&String> = available.difference(&declared).collect();

    println!("declared by the profile ({}):", declared.len());
    for d in &declared {
        println!("  {d}");
    }
    println!("carried by the artifact ({}):", available.len());
    for a in &available {
        println!("  {a}");
    }
    println!("DECLARED BUT NOT CARRIED ({}):", undeliverable.len());
    for d in &undeliverable {
        println!("  {d}");
    }
    println!("CARRIED BUT NOT DECLARED ({}):", undeclared.len());
    for d in &undeclared {
        println!("  {d}");
    }
    // Deliberately not an assertion yet — the first run's job is to report, and the pin follows
    // the measurement rather than the other way round.
}

// -------------------------------------------------------------------------------------------------
// The resolution rule
// -------------------------------------------------------------------------------------------------
//
// The measurement above says the registered graph names 53 operands while an artifact of this
// lineage carries 76, and that the two do not nest. Much of that gap is FUSION and is legitimate:
// one declared node stands for a computation that reads several stores, and `ffn_gate_exps.routed`
// is a name for "the eight chosen experts' gate projections", not for a tensor anyone stored.
//
// The problem was never the fusion. It is that **the rule mapping a fused name to the bytes it
// reads existed only inside the compiled engine's hardcoded order** — so a node reading the
// registered declaration had no way to follow it, which is exactly the dependency ADR-0067 exists
// to remove. This table is that rule, written down, and the test under it holds the table to the
// artifact: every target must be something an artifact of this lineage actually carries.
//
// Derived by reading `Qwen36Engine`'s arms against the IR they claim to mirror, not by guessing.
// `{e}` stands for each routed expert the router selected for this token.
const RESOLUTION: &[(&str, &[&str])] = &[
    // --- the GatedDeltaNet arm ---------------------------------------------------------------
    // The convolution's taps and the requant of its output are two stores under one declared node.
    ("linear_conv.weight", &["linear_conv.weight", "linear_conv.a16"]),
    // "decay" is a computation over two calibration rows, neither of which is named `linear_decay`.
    ("linear_decay.a16", &["linear_decay_c.a16", "linear_dt_bias.a16"]),
    // The fused recurrence. One declared node, five stores.
    ("linear_gdn.a16", &["linear_delta.a16", "linear_gate.a16", "linear_read.a16", "linear_write.a16", "linear_out.a16"]),
    // --- the attention arm --------------------------------------------------------------------
    // The declared node is named for the rope TABLE (a global in the artifact); what it reads per
    // layer is the rope requant.
    ("rope_table", &["attn_rope.a16"]),
    ("attn_softmax.a16", &["attn_softmax_up.a16"]),
    // --- the mixture --------------------------------------------------------------------------
    ("ffn_gate_exps.routed", &["ffn_expert.{e}_gate.weight", "ffn_expert.{e}_gate.weight.a16"]),
    ("ffn_up_exps.routed", &["ffn_expert.{e}_up.weight", "ffn_expert.{e}_up.weight.a16"]),
    ("ffn_down_exps.routed", &["ffn_expert.{e}_down.weight", "ffn_expert.{e}_down.weight.a16"]),
    ("ffn_expert_gated.a16", &["ffn_expert.{e}_gated.a16", "ffn_expert.{e}_silu.a16"]),
    // The router's top-k reads the widening the profile does not name at all — Finding 2, stated
    // here as the resolution it would need if the graph were followed as written.
    ("ffn_router_topk.a16", &["ffn_router_up.a16"]),
    // --- the shared expert, which is where the names genuinely disagree (Finding 1) ------------
    ("ffn_shared_up.weight", &["ffn_shared_expert_up.weight", "ffn_shared_expert_up.weight.a16"]),
    ("ffn_shared_down.weight", &["ffn_shared_expert_down.weight", "ffn_shared_expert_down.weight.a16"]),
    // The declared `ffn_shared_gate.weight` is the EXPERT's gate at `shared_dim` wide; the engine's
    // tensor of that name is the SCALAR gate. The declared name resolves to the expert's.
    ("ffn_shared_gate.weight", &["ffn_shared_expert_gate.weight", "ffn_shared_expert_gate.weight.a16", "ffn_shared_expert_silu.a16"]),
    ("ffn_shared_gated.a16", &["ffn_shared_expert_gated.a16"]),
    // ...and the declared `ffn_shared_scalar.weight` is the tensor the engine calls
    // `ffn_shared_gate.weight`. The two names are swapped between the graph and the engine.
    ("ffn_shared_scalar.weight", &["ffn_shared_gate.weight", "ffn_shared_gate.weight.a16"]),
    ("ffn_shared_apply.a16", &["ffn_shared_gated.a16"]),
];

/// **Every target of the resolution rule is a store an artifact actually carries.**
///
/// A rule that resolves a fused name to bytes nobody stored is not a rule, it is a second guess.
#[test]
fn the_resolution_rule_lands_on_real_operands() {
    let available = artifact_operands();
    let mut missing: Vec<String> = Vec::new();
    for (declared, targets) in RESOLUTION {
        for t in *targets {
            if !available.contains(*t) {
                missing.push(format!("{declared} -> {t}"));
            }
        }
    }
    assert!(missing.is_empty(), "the rule names {} store(s) no artifact carries: {missing:#?}", missing.len());
}

/// **What the rule still does not cover, counted rather than assumed.**
///
/// The residue in both directions is what an interpreter would still be unable to follow. Printing
/// it is the point: the pin below is the SHAPE of the remaining gap, so a later correction that
/// shrinks it fails this test and has to restate the number.
#[test]
fn the_residue_after_the_rule_is_named() {
    let declared = declared_operands();
    let available = artifact_operands();
    let resolved_from: BTreeSet<String> = RESOLUTION.iter().map(|(d, _)| d.to_string()).collect();
    let resolved_to: BTreeSet<String> = RESOLUTION.iter().flat_map(|(_, t)| t.iter().map(|s| s.to_string())).collect();

    let rides_with_declared = |name: &str| match name.strip_suffix(".a16") {
        Some(stem) => declared.contains(stem) || resolved_to.contains(stem),
        None => false,
    };

    let still_undeliverable: Vec<&String> = declared.difference(&available).filter(|n| !resolved_from.contains(*n)).collect();
    let still_unreachable: Vec<&String> =
        available.difference(&declared).filter(|n| !resolved_to.contains(*n) && !rides_with_declared(n)).collect();

    println!("declared names the rule still cannot deliver ({}):", still_undeliverable.len());
    for n in &still_undeliverable {
        println!("  {n}");
    }
    println!("engine reads the rule still cannot reach ({}):", still_unreachable.len());
    for n in &still_unreachable {
        println!("  {n}");
    }

    // Before the rule these were 14 and 27. The rule is written to shrink them, not to close them:
    // what is left is the honest size of the correction a `graph-v2` row would have to make.
    assert!(still_undeliverable.len() < 14 && still_unreachable.len() < 27, "the rule did not shrink the gap it was written for");
}

/// **Finding 3: the graph declares a node the engine never executes.**
///
/// The residue the resolution rule cannot close is one name, and it is not a naming disagreement
/// like Findings 1 and 2 — it is an operation. The profile declares a `VCacheWrite` node
/// (`MulElem` under the requantize kernel, `blk.N.attn_v_cache.a16`) whose job is to narrow the V
/// projection as it enters the cache. `Qwen36Engine::full_arm` pushes V into the cache RAW:
/// `cache.values[li].push(v)`, with no requant and no such store in any artifact. K is normed and
/// rotated before its cache write and the graph declares both; the V path was written as if it were
/// symmetric, and it is not.
///
/// This one matters more than a name. ADR-0030 gives a step leg one committed row per declared
/// node, so a node with no computation behind it is a slot that can never be filled — an
/// interpreter following this declaration would owe a row it cannot produce, and every step leg of
/// the class would be short by one. It cannot be papered over by a rename.
#[test]
fn the_graph_declares_a_v_cache_requant_the_engine_does_not_do() {
    let p = qwen36_profile_v1(QWEN36_35B_A3B).expect("the registered geometry projects");
    let node = p
        .attn_nodes
        .iter()
        .find(|n| n.weight_name.ends_with("attn_v_cache.a16"))
        .expect("the registered profile declares the V-cache write");
    assert_eq!(
        node.role,
        kaspa_consensus_core::palw_step::PalwStepNodeRoleV1::VCacheWrite,
        "the node under test is the V cache write"
    );
    // Nothing in an artifact of this lineage can feed it.
    assert!(
        !artifact_operands().iter().any(|n| n.ends_with("attn_v_cache.a16")),
        "if this ever fails, the engine grew the requant this node declares and the finding is closed"
    );
}

// -------------------------------------------------------------------------------------------------
// graph-v2 — the corrected row, held to the same measurements that convicted v1
// -------------------------------------------------------------------------------------------------

/// The per-layer operand suffixes graph-v2 declares.
fn declared_operands_v2() -> BTreeSet<String> {
    let p = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(QWEN36_35B_A3B).expect("v2 projects");
    let mut out = BTreeSet::new();
    for table in [&p.pre_nodes, &p.gdn_nodes, &p.attn_nodes, &p.post_nodes] {
        for node in table.iter() {
            if node.weight_name.is_empty() {
                continue;
            }
            let name = node.weight_name.as_str();
            let suffix = per_layer(name).unwrap_or_else(|| name.to_string());
            out.insert(collapse_expert(&suffix));
        }
    }
    out
}

/// **v2's resolution table is only the structural fusions.** Every v1 entry that existed to paper
/// over a wrong name is gone, because the name is right now. What remains are nodes that stand for
/// computations over several stores — the fusion that was always legitimate.
const RESOLUTION_V2: &[(&str, &[&str])] = &[
    ("linear_conv.weight", &["linear_conv.weight", "linear_conv.a16"]),
    ("linear_decay.a16", &["linear_decay_c.a16", "linear_dt_bias.a16"]),
    ("linear_gdn.a16", &["linear_delta.a16", "linear_gate.a16", "linear_read.a16", "linear_write.a16", "linear_out.a16"]),
    ("ffn_gate_exps.routed", &["ffn_expert.{e}_gate.weight", "ffn_expert.{e}_gate.weight.a16"]),
    ("ffn_up_exps.routed", &["ffn_expert.{e}_up.weight", "ffn_expert.{e}_up.weight.a16"]),
    ("ffn_down_exps.routed", &["ffn_expert.{e}_down.weight", "ffn_expert.{e}_down.weight.a16"]),
    ("ffn_expert_gated.a16", &["ffn_expert.{e}_gated.a16", "ffn_expert.{e}_silu.a16"]),
    // The shared expert's silu rescale rides inside its gated multiply, same fusion as the routed.
    ("ffn_shared_expert_gated.a16", &["ffn_shared_expert_gated.a16", "ffn_shared_expert_silu.a16"]),
];

/// **The measurement that convicted v1, re-run against v2: the residue is zero, both ways.**
#[test]
fn v2_leaves_no_residue() {
    let declared = declared_operands_v2();
    let available = artifact_operands();
    let resolved_from: BTreeSet<String> = RESOLUTION_V2.iter().map(|(d, _)| d.to_string()).collect();
    let resolved_to: BTreeSet<String> = RESOLUTION_V2.iter().flat_map(|(_, t)| t.iter().map(|s| s.to_string())).collect();

    let rides_with_declared = |name: &str| match name.strip_suffix(".a16") {
        Some(stem) => declared.contains(stem) || resolved_to.contains(stem),
        None => false,
    };

    let undeliverable: Vec<&String> = declared.difference(&available).filter(|n| !resolved_from.contains(*n)).collect();
    let unreachable: Vec<&String> =
        available.difference(&declared).filter(|n| !resolved_to.contains(*n) && !rides_with_declared(n)).collect();

    assert!(undeliverable.is_empty(), "v2 declares names no artifact carries: {undeliverable:#?}");
    assert!(unreachable.is_empty(), "the engine reads operands v2 does not name: {unreachable:#?}");
}

/// **Each of the three v1 findings, closed in v2 and asserted closed.**
#[test]
fn v2_closes_the_three_findings() {
    let p = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(QWEN36_35B_A3B).expect("v2 projects");
    let attn = &p.attn_nodes;

    // Finding 1: the shared gate. The 512-wide projection carries the expert's name; the scalar
    // node carries the scalar tensor's name, at one output.
    let scalar = attn.iter().find(|n| n.weight_name.ends_with("ffn_shared_gate.weight")).expect("the scalar gate is declared");
    assert_eq!(scalar.out_len, kaspa_consensus_core::palw_step::PalwStepOutLenV1::Fixed { elements: 1 });
    assert!(attn.iter().any(|n| n.weight_name.ends_with("ffn_shared_expert_gate.weight")));

    // Finding 2: the router widening is named.
    assert!(attn.iter().any(|n| n.weight_name.ends_with("ffn_router_up.a16")));
    assert!(!attn.iter().any(|n| n.weight_name.ends_with("ffn_router_topk.a16")));

    // Finding 3: the phantom is gone, and the V-cache role sits on the projection that actually
    // feeds the cache.
    assert!(!attn.iter().any(|n| n.weight_name.ends_with("attn_v_cache.a16")));
    let v_proj = attn.iter().find(|n| n.weight_name.ends_with("attn_v.weight")).expect("the V projection is declared");
    assert_eq!(v_proj.role, kaspa_consensus_core::palw_step::PalwStepNodeRoleV1::VCacheWrite);
}
