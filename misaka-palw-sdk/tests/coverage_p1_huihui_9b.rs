//! **Coverage P1 — `huihui-ai/Huihui-Qwen3.5-9B-abliterated` @`05b9e7c9`, retested on the IR route from its headers** (COV-P1P2,
//! 2026-10-08; record `docs/design/palw/coverage-p1p2-record.md`).
//!
//! An ignored probe, not a regression test: it needs a header-only snapshot of the repository (`config.json`, the index, the four
//! shard headers in sparse files of their real length, `tokenizer.json` — no weight byte), read the way the SDK preflight reads a
//! repository (`preflight::source`). For each declared context it lowers the text decoder shape-only, then asks the registration gate
//! (`palw_tir_registration_preflight_at_v1`) under `palw_t12_shipped_params()` at the judged heights:
//!
//! 1. the DEFAULT layout (tile 64, history tile 64, logits tile 4,096, the court's checkpoint interval) — refusal 3's layout;
//! 2. the layout search the SDK runs (`tir_choose_layout_judged_v1`), at the same gate;
//! 3. the close sizing of the widest layout the search tried, by BOTH twins, with the cap lifted: the true work and the commit point
//!    at which it crosses `PALW_TIR_CLOSE_SIZING_WORK_CAP_V1` (refusal 2's `cap + 1` is a sentinel, this is the measurement).
//!
//! Run: `COV_P1_DIR=<snapshot> [COV_P1_CONTEXTS=8192,512] [COV_P1_DAA=5585,9000] [COV_P1_SIZE=1]
//! cargo test --offline -p misaka-palw-sdk --test coverage_p1_huihui_9b -- --ignored --nocapture`
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_tir_attempt_v1::{
    PalwTirJobFactsV1, palw_tir_attempt_canonical_of_v1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1,
};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use kaspa_hashes::Hash64;
use misaka_palw_sdk::preflight::{Options, model, source};
use misaka_palw_sdk::tir_layout::{
    TirLayoutChoiceV1, tir_choose_layout_judged_v1, tir_court_checkpoint_interval_v1, tir_layout_tiles_v1, tir_program_with_scheme_v1,
};
use std::path::PathBuf;

fn env_list<T: std::str::FromStr>(key: &str, default: &[T]) -> Vec<T>
where
    T: Clone,
{
    std::env::var(key).map(|v| v.split(',').filter_map(|c| c.trim().parse().ok()).collect()).unwrap_or_else(|_| default.to_vec())
}

/// The object the gate judges: the formula's canonical job, a placeholder bond, weightless (the SDK preflight's).
fn probe_object(
    bundle: &PalwConsensusParamsV2,
    class: &PalwTirClassV1,
    root: Hash64,
) -> Result<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2, String> {
    let program = class.decode_program().map_err(|e| e.to_string())?;
    let canonical = palw_tir_attempt_canonical_v1(class).ok_or("a context too narrow for a canonical job")?;
    let facts = PalwTirJobFactsV1::of(class, &program, class.class_id(&root));
    let bond = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        kaspa_consensus_core::tx::TransactionId::from_bytes([0; 64]),
        0,
    ));
    kaspa_consensus_core::palw_tir_admission_v1::palw_tir_post_genesis_registration_v1(
        class.clone(),
        palw_tir_job_context_v1(&facts, canonical),
        root,
        0,
        u128::MAX,
        1,
        0,
        bond,
        Vec::new(),
        bundle.court.max_step_leaf_count(),
    )
    .map_err(|e| format!("{} ({e})", e.code()))
}

/// The gate at `daa`, as code + the structured refusal.
fn judge(params: &Params, bundle: &PalwConsensusParamsV2, class: &PalwTirClassV1, root: Hash64, daa: u64) -> Result<(), String> {
    use kaspa_consensus_core::palw_tir_admission_v1::palw_tir_registration_preflight_at_v1;
    let object = probe_object(bundle, class, root)?;
    palw_tir_registration_preflight_at_v1(params, bundle, &object, daa, &[]).map(|_| ()).map_err(|e| {
        let decided = kaspa_consensus_core::palw_refusal_v1::palw_refusal_decided_by_v1(
            params.palw_held_context_active_at(daa),
            Some(daa),
            params.palw_held_context.map(|f| f.daa_score()),
        );
        format!("{} ({e}) [refusal {}]", e.code(), e.refusal_v1(&decided).to_json())
    })?;
    misaka_palw_sdk::tir_layout::tir_canonical_job_answerable_at_v1(params, class, daa)
}

/// Both close-sizing twins over `class`, with the cap lifted to `cap`: `(twin, work, elapsed, worst close, per-point work)`.
fn size_both(class: &PalwTirClassV1, court: bool, cap: u64) {
    use kaspa_consensus_core::palw_tir_close_size_v1 as z;
    let program = class.decode_program().expect("decodes");
    let space = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(class).expect("a step space");
    let inventory = kaspa_consensus_core::palw_tir_court_v1::PalwTirInventoryIndexV1::new(&program).expect("an inventory");
    let facts = PalwTirJobFactsV1::of(class, &program, Hash64::default());
    let formula = palw_tir_attempt_canonical_of_v1(class.layout.max_context).expect("a canonical job");
    let expected = palw_tir_job_context_v1(&facts, formula);
    let deepest = kaspa_consensus_core::palw_v2::PalwJobContextV2 {
        declared_prefill_tokens: 1,
        exact_decode_tokens: class.layout.max_context,
        max_context_tokens: u32::MAX,
        ..expected
    };
    let sizing = z::PalwTirCloseSizingV1 { form: z::PalwTirParamFormV1::Multiproof, court, cap, stop_above: None };
    let twins: [(&str, bool); 2] =
        [("range (palw_tir_fence2, live from DAA 3,600)", true), ("element (below palw_tir_fence2)", false)];
    for (name, range) in twins {
        if !range && std::env::var("COV_P1_ELEMENT").is_err() {
            println!("    sizing {name}: skipped (COV_P1_ELEMENT unset)");
            continue;
        }
        let t = std::time::Instant::now();
        let mut trace = Vec::new();
        let out = if range {
            kaspa_consensus_core::palw_tir_close_range_v1::palw_tir_worst_closes_range_trace_v1(
                &space, &inventory, &deepest, &sizing, &mut trace,
            )
        } else {
            z::palw_tir_worst_closes_trace_v1(&space, &inventory, &deepest, &sizing, &mut trace)
        };
        let elapsed = t.elapsed();
        match out {
            Ok((bounds, work)) => {
                let worst = bounds.iter().map(|b| b.close_bytes).max().unwrap_or(0);
                let worst_root = bounds.iter().map(|b| b.root_claim_bytes).max().unwrap_or(0);
                println!(
                    "    sizing {name}: work {work} steps ({:.3}x the 2^26 cap) in {elapsed:?}; {} commit points; worst close {worst} B, worst root claim {worst_root} B",
                    work as f64 / (1u64 << 26) as f64,
                    bounds.len()
                );
            }
            Err(e) => println!("    sizing {name}: {e} (cap {cap}) after {elapsed:?}"),
        }
        let cap26 = 1u64 << 26;
        let mut before = 0u64;
        for (b, n, after) in &trace {
            let marker = if before <= cap26 && *after > cap26 { "  <== crosses 2^26" } else { "" };
            if std::env::var("COV_P1_TRACE_ALL").is_ok() || !marker.is_empty() {
                println!("      point ({b}, {n}): {} steps, cumulative {after}{marker}", after - before);
            }
            before = *after;
        }
        if let Some((b, n, w)) = trace
            .iter()
            .zip(std::iter::once(&(0, 0, 0)).chain(trace.iter()))
            .map(|(cur, prev)| (cur.0, cur.1, cur.2 - prev.2))
            .max_by_key(|x| x.2)
        {
            let node = &program.blocks[b as usize].nodes[n as usize];
            println!(
                "      the costliest point: ({b}, {n}) at {w} steps — {} -> {:?}, h-reductions {}",
                node.prim.name(),
                node.out.shape,
                kaspa_consensus_core::palw_tir_dissect_v1::palw_tir_cone_reductions_v1(&program.blocks[b as usize], n).len()
            );
            if let Some(last) = trace.last() {
                // The commit point after the last one sized: where a sizing stopped at its cap, the one that crossed it.
                let points: Vec<(u8, u16)> = program
                    .blocks
                    .iter()
                    .enumerate()
                    .flat_map(|(bi, bl)| {
                        bl.nodes.iter().enumerate().filter(|(_, x)| x.commit).map(move |(ni, _)| (bi as u8, ni as u16))
                    })
                    .collect();
                if let Some(i) = points.iter().position(|p| *p == (last.0, last.1))
                    && let Some(next) = points.get(i + 1)
                {
                    let nn = &program.blocks[next.0 as usize].nodes[next.1 as usize];
                    println!("      after the last sized point: ({}, {}) — {} -> {:?}", next.0, next.1, nn.prim.name(), nn.out.shape);
                }
            }
        }
    }
}

/// **The kernel route** (ADR-0172, RFC-0011 §§13.1/15): the reference VerificationPlan of `program` at `positions`, checked under each K2
/// descriptor ARMED (the node's template arms all three when `palw_probabilistic_constraints_v1` is active), then every registration gate
/// the real node's fold applies after it — `public_prosecution_complete_v1` under the interim ledger policy, one block's court budget,
/// and the carriers (`carrier_fit_v1` at `PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1`). Every gate is reported, not only the first.
fn k2_route(program: &misaka_palw_tir::program::TirProgramV1, positions: u32) {
    use misaka_palw_kernel::check::check_plan_v1;
    use misaka_palw_kernel::descriptor::{KernelScheduleV1, KernelStatusV1, k2_tir_v1_descriptor, k2_tir_v2_descriptor};
    use misaka_palw_kernel::gate::public_prosecution_complete_v1;
    use misaka_palw_kernel::plan::plan_for_tir_program_v1;
    use misaka_palw_kernel::public::ProfileMaterialV1;
    let bytes = program.encode();
    let root = misaka_palw_kernel::public::program_root_v1(&bytes);
    let nodes: u64 = program.occurrences().iter().map(|(b, _)| program.blocks[*b as usize].nodes.len() as u64).sum();
    let policy = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::default(), Hash64::default());
    let carrier = kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1;
    for (name, d) in [("K2-TIR-v1", k2_tir_v1_descriptor()), ("K2-TIR-v2", k2_tir_v2_descriptor())] {
        let armed = KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 });
        let plan = match plan_for_tir_program_v1(&d, program, root, positions) {
            Ok(p) => p,
            Err((family, why)) => {
                println!("    {name} @{positions}: KERNEL_EXTENSION_REQUIRED ({}: {why})", family.name());
                continue;
            }
        };
        let b = &plan.budgets;
        println!(
            "    {name} @{positions}: plan {} B, {} relations; per position: verifier work {}, evidence {} B, {} probabilistic instances; artifact {} B; worst court {} B / {} work",
            plan.encoded_len(),
            plan.relations.len(),
            b.verifier_work_per_position,
            b.evidence_bytes_per_position,
            b.probabilistic_instances_per_position,
            b.artifact_bytes,
            b.worst_court_bytes,
            b.worst_court_work
        );
        match check_plan_v1(&armed, &d, program, root, &plan, 0) {
            Ok(a) => println!(
                "      check_plan_v1 (armed): PASS — eps <= 2^-{}, claim verifier work {}, claim evidence {} B",
                a.error_bits, a.claim_verifier_work, a.claim_evidence_bytes
            ),
            Err(o) => println!("      check_plan_v1 (armed): {} — {}", o.code(), o.to_string().chars().take(500).collect::<String>()),
        }
        match public_prosecution_complete_v1(&d, &plan, nodes, &ProfileMaterialV1::kernel_route(true), &policy.prosecution) {
            Ok(g) => {
                println!(
                    "      public_prosecution_complete_v1: PASS — public {} B, opening {} B, filing {} B, response {} B, court work {}, verifier RAM {} B, retained {} B, sessions {}",
                    g.max_public_bytes,
                    g.max_opening_bytes,
                    g.max_filing_bytes,
                    g.max_response_bytes,
                    g.max_court_work,
                    g.max_verifier_ram,
                    g.max_retained_state,
                    g.max_concurrent_sessions
                );
                println!(
                    "      block court budget: {} (court work {} vs {})",
                    if g.max_court_work <= policy.max_court_work_per_block { "PASS" } else { "REFUSED" },
                    g.max_court_work,
                    policy.max_court_work_per_block
                );
                match misaka_palw_kernel::ledger::carrier_fit_v1(&g, carrier, carrier, carrier) {
                    Ok(()) => println!("      carrier_fit_v1 ({carrier} B): PASS"),
                    Err(e) => println!("      carrier_fit_v1 ({carrier} B): REFUSED — {e}"),
                }
            }
            Err(gaps) => println!("      public_prosecution_complete_v1: REFUSED — {gaps:?}"),
        }
    }
}

#[test]
#[ignore]
fn huihui_qwen35_9b_ir_route_from_headers() {
    let dir = PathBuf::from(std::env::var("COV_P1_DIR").expect("COV_P1_DIR names the header-only snapshot"));
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let kind = source::detect(&dir).expect("a HF directory");
    let src = source::open(&dir, kind, None, reg).expect("the headers");
    let tokenizer_bytes = std::fs::read(dir.join("tokenizer.json")).unwrap_or_default();
    let tokenizer_id = Hash64::from_bytes(misaka_palw_tir_lower::artifact::tokenizer_id_of(&tokenizer_bytes));
    let config_bytes = std::fs::read(dir.join("config.json")).expect("config.json");
    let root = {
        let mut st = blake2b_simd::Params::new().hash_length(64).key(b"g14-synthetic-artifact-root/v1").to_state();
        st.update(&config_bytes);
        st.update(&tokenizer_bytes);
        let mut b = [0u8; 64];
        b.copy_from_slice(st.finalize().as_bytes());
        Hash64::from_bytes(b)
    };
    let params = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("testnet-12 is a V2 network") };
    let contexts: Vec<u32> = env_list("COV_P1_CONTEXTS", &[8_192, 512]);
    let heights: Vec<u64> = env_list("COV_P1_DAA", &[5_585, 9_000]);
    for &ctx in &contexts {
        let held = std::env::var("COV_P1_HELD").is_ok();
        let opts = Options { max_context: Some(ctx), held, ..Options::default() };
        let analysis = model::analyze(&src, &opts, reg, None);
        println!("== context {ctx}");
        for b in &analysis.blockers {
            println!(
                "  blocker {}{} — {} {:?}",
                b.code,
                b.arg.as_deref().map(|a| format!("({a})")).unwrap_or_default(),
                b.what.chars().take(400).collect::<String>(),
                b.evidence.iter().take(4).collect::<Vec<_>>()
            );
        }
        for n in analysis.notes.iter().take(12) {
            println!("  note: {}", n.chars().take(300).collect::<String>());
        }
        if let Some(a) = &analysis.artifact {
            println!(
                "  artifact estimate: {} B (params {} B, program {} B, tokenizer {} B), ~{} inventory leaves; download needed {:?} of {:?} B, left out {} B — {}",
                a.estimate_bytes,
                a.params_bytes,
                a.program_bytes,
                a.tokenizer_bytes,
                a.inventory_leaves_estimate,
                a.download_bytes_needed,
                a.download_bytes_total,
                a.left_out_bytes,
                a.note
            );
        }
        let Some(program) = analysis.program.clone() else {
            println!("  no program");
            continue;
        };
        let program = tir_program_with_scheme_v1(&program, None).expect("the tiled scheme");
        let fixed = program.states.iter().filter(|s| matches!(s.kind, misaka_palw_tir::program::StateKind::Fixed { .. })).count();
        let commits: usize = program.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum();
        println!(
            "  program {} B, {} blocks, {} params, {} states ({fixed} fixed), {commits} commit points, history bound {}",
            program.encode().len(),
            program.blocks.len(),
            program.params.len(),
            program.states.len(),
            program.history_bound
        );
        if std::env::var("COV_P1_K2").is_ok() {
            k2_route(&program, ctx);
        }
        if std::env::var("COV_P1_K2_ONLY").is_ok() {
            continue;
        }
        let leaves =
            analysis.artifact.as_ref().map(|a| a.inventory_leaves_estimate.min(u32::MAX as u64) as u32).unwrap_or(1 << 16).max(2);
        for &daa in &heights {
            println!("  -- DAA {daa}");
            // 1. The default layout: tile 64, history tile 64, logits tile 4,096, the court's interval.
            let choice = TirLayoutChoiceV1 { max_context: Some(ctx), ..Default::default() };
            // Past the network's context ceiling the SDK will not tile it: the same tiles at the widest context it does, the context
            // then written as declared — so the GATE names the refusal, in its own order.
            let mut layout = match tir_layout_tiles_v1(&params, &program, &choice) {
                Ok(l) => l,
                Err(why) => {
                    println!("    tiles: {why} — judged at the declared context anyway");
                    let narrower = TirLayoutChoiceV1 { max_context: Some(262_144), ..choice };
                    let mut l = tir_layout_tiles_v1(&params, &program, &narrower).expect("tiles at the network ceiling");
                    l.max_context = ctx;
                    l
                }
            };
            match tir_court_checkpoint_interval_v1(&params, bundle, &program, &layout) {
                Ok(c) => layout.checkpoint_interval = c,
                Err(e) => println!("    default layout: no checkpoint interval: {e}"),
            }
            let class =
                PalwTirClassV1 { version: PALW_TIR_CLASS_VERSION_V1, program: program.encode(), layout: layout.clone(), tokenizer_id };
            println!(
                "    default layout (tile {}, h_tile {}, logits tile {:?}, interval {}): {}",
                choice.tile_len,
                layout.h_tile,
                layout.commit_tiles.last(),
                layout.checkpoint_interval,
                match judge(&params, bundle, &class, root, daa) {
                    Ok(()) => "ADMITTED".to_string(),
                    Err(e) => e,
                }
            );
            // 2. The SDK's layout search, at this gate.
            let t = std::time::Instant::now();
            let judged = |c: &PalwTirClassV1| judge(&params, bundle, c, root, daa);
            // `COV_P1_FIXED=<logits tile>,<h_tile>,<interval>`: that layout, judged once (no search).
            let fixed: Option<Vec<u32>> =
                std::env::var("COV_P1_FIXED").ok().map(|v| v.split(',').map(|x| x.trim().parse().expect("a number")).collect());
            let chosen = match &fixed {
                Some(f) => {
                    let mut l = tir_layout_tiles_v1(
                        &params,
                        &program,
                        &TirLayoutChoiceV1 { logits_tile: Some(f[0]), h_chunk: f[1], ..choice },
                    )
                    .expect("tiles");
                    l.checkpoint_interval = f[2];
                    let class = PalwTirClassV1 {
                        version: PALW_TIR_CLASS_VERSION_V1,
                        program: program.encode(),
                        layout: l.clone(),
                        tokenizer_id,
                    };
                    Ok(misaka_palw_sdk::tir_layout::TirChosenLayoutV1 { layout: l, admission: judged(&class) })
                }
                None => tir_choose_layout_judged_v1(&params, bundle, &program, tokenizer_id, leaves, &choice, true, &judged),
            };
            match &chosen {
                Ok(c) => println!(
                    "    search ({:?}): logits tile {:?}, h_tile {}, interval {} -> {}",
                    t.elapsed(),
                    c.layout.commit_tiles.last(),
                    c.layout.h_tile,
                    c.layout.checkpoint_interval,
                    match &c.admission {
                        Ok(()) => "ADMITTED".to_string(),
                        Err(e) => e.clone(),
                    }
                ),
                Err(e) => println!("    search: {e}"),
            }
            // `COV_P1_WRITE_G14=<dir>`: the admitted class as a lane-D fixture (`fixtures/g14/shipped/*.json`, the format
            // `g14_registration_fixture.rs` writes), so the consensus E2E carries it through the real node path.
            if let (Ok(out), Ok(c)) = (std::env::var("COV_P1_WRITE_G14"), &chosen)
                && c.admission.is_ok()
            {
                let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
                let class = PalwTirClassV1 {
                    version: PALW_TIR_CLASS_VERSION_V1,
                    program: program.encode(),
                    layout: c.layout.clone(),
                    tokenizer_id,
                };
                let name = format!("huihui-qwen3.5-9b-{}", if ctx % 1024 == 0 { format!("{}k", ctx / 1024) } else { ctx.to_string() });
                let json = serde_json::json!({
                    "name": name,
                    "checkpoint": "huihui-ai/Huihui-Qwen3.5-9B-abliterated@05b9e7c9b978ba29bdb8f50a49c30e4b91183339 (text decoder; headers only)",
                    "generator": "misaka-palw-sdk/tests/coverage_p1_huihui_9b.rs (COV-P1P2)",
                    "weights_loaded": false,
                    "artifact_root_synthetic": true,
                    "artifact_root_hex": hex(root.as_byte_slice()),
                    "tokenizer_id_hex": hex(tokenizer_id.as_byte_slice()),
                    "max_context": ctx,
                    "program_borsh_hex": hex(&class.program),
                    "layout_borsh_hex": hex(&borsh::to_vec(&class.layout).unwrap()),
                    "class_id_hex": hex(class.class_id(&root).as_byte_slice()),
                    "ruleset": format!("palw_t12_shipped_params, judged at DAA {daa}"),
                });
                std::fs::write(PathBuf::from(&out).join(format!("{name}.json")), serde_json::to_vec_pretty(&json).unwrap()).unwrap();
                println!("    wrote {out}/{name}.json");
            }
            // 3. The sizing at the searched layout, cap lifted.
            if std::env::var("COV_P1_SIZE").is_ok()
                && let Ok(c) = &chosen
            {
                let class = PalwTirClassV1 {
                    version: PALW_TIR_CLASS_VERSION_V1,
                    program: program.encode(),
                    layout: c.layout.clone(),
                    tokenizer_id,
                };
                let court = kaspa_consensus_core::palw_tir_admission_v1::PalwTirAdmissionRulesV1::at(&params, daa)
                    .is_some_and(|r| r.court.is_some());
                let cap: u64 = std::env::var("COV_P1_SIZE_CAP").ok().and_then(|v| v.parse().ok()).unwrap_or(1 << 32);
                size_both(&class, court, cap);
            }
        }
    }
}
