//! **`WEIGHTS_EXPR_V1` — weights as data** (FR-01).
//!
//! No transformers, no Python: a tiny fixture's checkpoint is RE-LAID in the storage a model of another
//! family uses (DBRX's flat experts, Granite's fused shared expert, ERNIE's `[1, E]` bias), an adapter
//! written as DATA says how to read it, and the parameters it binds must equal — exactly, tensor by tensor —
//! the ones the built-in binding reads from the original layout. Then the grammar is checked against every
//! binding the built-in adapters produce: every `Src` of every fixture must survive `to_expr` →
//! `parse_weight_expr` unchanged, which is what makes the surface *general* and not a list of special cases.

use misaka_palw_tir_lower::hf_schema::{AdapterChoice, Level, ReadOptions, TensorIndex};
use misaka_palw_tir_lower::model::{ReportResult, analyze};
use misaka_palw_tir_lower::quantfmt::QuantRegistry;
use misaka_palw_tir_lower::spec::ModelSpec;
use misaka_palw_tir_lower::weights::expr::{parse_weight_expr, to_expr};
use misaka_palw_tir_lower::weights::stream::{eval_src_rows, row_blocks, src_row_space};
use misaka_palw_tir_lower::weights::{Binding, Checkpoint, MapSource, Resolver, Tensor, TensorSource, check_weights, eval_src, layers_of_param};
use misaka_palw_tir_lower::{hf_config, hf_weights, hl, parse_config_str};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf").join(name)
}

fn config(name: &str) -> String {
    std::fs::read_to_string(fixture(name).join("config.json")).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// A model's spec, HL program and weight binding, read through `adapter` (`None`: the built-in choice).
fn read(cfg: &str, adapter: Option<String>) -> Result<(ModelSpec, hl::HlProgram, Binding), String> {
    let opts = ReadOptions { adapter: adapter.map_or(AdapterChoice::Auto, AdapterChoice::Text) };
    let spec = hf_config::parse_config_str_read(cfg, &opts, QuantRegistry::builtin()).map_err(|e| e.to_string())?;
    let prog = hl::build_program(&spec).map_err(|e| e.to_string())?;
    let binding = hf_weights::bind(&spec, &prog).map_err(|e| e.to_string())?;
    Ok((spec, prog, binding))
}

/// An adapter extending `parent` that renames nothing and only adds `weights` (and, when given, replaces the
/// `names` variable — the roles the expressions make unnecessary are dropped from it).
fn adapter(parent: &str, names: Option<Value>, weights: Value) -> String {
    let mut a = json!({
        "format": "misaka.palw.model-adapter.v1",
        "id": format!("{parent}-relaid"),
        "extends": [parent],
        "spec": {"hf": {"weights": weights}},
    });
    if let Some(n) = names {
        a["vars"] = json!([{"name": "names", "value": {"$merge": [{"$var": "names_base"}, n]}}]);
    }
    a.to_string()
}

fn cat_rows(parts: &[Tensor]) -> Tensor {
    let cols = parts[0].shape[1];
    let rows: usize = parts.iter().map(|t| t.shape[0]).sum();
    Tensor::new(vec![rows, cols], parts.iter().flat_map(|t| t.data.iter().copied()).collect())
}

fn transpose2(t: &Tensor) -> Tensor {
    let (r, c) = (t.shape[0], t.shape[1]);
    let mut d = vec![0f32; r * c];
    for i in 0..r {
        for j in 0..c {
            d[j * r + i] = t.data[i * c + j];
        }
    }
    Tensor::new(vec![c, r], d)
}

/// The layers a param is bound for (model layers), or `None` for a global one.
fn layers(prog: &hl::HlProgram, pi: usize) -> Vec<Option<usize>> {
    if prog.params[pi].per_layer {
        layers_of_param(prog, pi as u32).into_iter().map(|l| Some(prog.model_layer(l))).collect()
    } else {
        vec![None]
    }
}

/// Every param of `pa` (bound over `sa`) equals, as a tensor and exactly, the same param of `pb` (over `sb`)
/// at every layer. Returns how many (param, layer) pairs were compared.
#[allow(clippy::too_many_arguments)]
fn assert_same_params(
    what: &str,
    pa: &hl::HlProgram,
    ba: &Binding,
    sa: &dyn TensorSource,
    pb: &hl::HlProgram,
    bb: &Binding,
    sb: &dyn TensorSource,
) -> usize {
    assert_eq!(pa.params, pb.params, "{what}: the two readings declare different params");
    let none = BTreeMap::new();
    let (ra, rb) = (Resolver::new(sa, &ba.aliases), Resolver::new(sb, &bb.aliases));
    let mut n = 0;
    for (pi, d) in pa.params.iter().enumerate() {
        for l in layers(pa, pi) {
            let x = eval_src(&ba.srcs[pi], &ra, l, &none).unwrap_or_else(|e| panic!("{what}: `{}` {l:?} (original layout): {e}", d.name));
            let y = eval_src(&bb.srcs[pi], &rb, l, &none).unwrap_or_else(|e| panic!("{what}: `{}` {l:?} (re-laid layout): {e}", d.name));
            assert_eq!(x, y, "{what}: `{}` {l:?}", d.name);
            n += 1;
        }
    }
    n
}

/// DBRX stores every MoE layer's experts as three flat tensors, `[E·I, D]` each (gate `w1`, up `v1`, down
/// `w2` — the down matrices stored `[I, D]` per expert, i.e. transposed), and names them without `.weight`.
#[test]
fn dbrx_flat_experts_bind_by_expression() {
    let (cfg, dir) = (config("mixtral"), fixture("mixtral"));
    let ck = Checkpoint::open(&dir).unwrap();
    let (spec0, prog0, bind0) = read(&cfg, None).unwrap();
    let layer_count = spec0.layers.len();
    let (e, inter, hidden) = (4usize, 64usize, 32usize);
    // Re-lay the experts.
    let mut relaid = MapSource::default();
    for n in ck.names() {
        if !n.contains(".experts.") {
            relaid.0.insert(n.clone(), ck.load(&n).unwrap());
        }
    }
    for l in 0..layer_count {
        let get = |role: &str, x: usize| ck.load(&format!("model.layers.{l}.block_sparse_moe.experts.{x}.{role}.weight")).unwrap();
        let gate: Vec<Tensor> = (0..e).map(|x| get("w1", x)).collect();
        let up: Vec<Tensor> = (0..e).map(|x| get("w3", x)).collect();
        let down: Vec<Tensor> = (0..e).map(|x| transpose2(&get("w2", x))).collect();
        relaid.0.insert(format!("model.layers.{l}.block_sparse_moe.experts_mlp_w1"), cat_rows(&gate));
        relaid.0.insert(format!("model.layers.{l}.block_sparse_moe.experts_mlp_v1"), cat_rows(&up));
        relaid.0.insert(format!("model.layers.{l}.block_sparse_moe.experts_mlp_w2"), cat_rows(&down));
    }
    let weights = |skip: &str| {
        let mut w = serde_json::Map::new();
        for (param, tensor, down) in [
            ("moe.experts.gate", "experts_mlp_w1", false),
            ("moe.experts.up", "experts_mlp_v1", false),
            ("moe.experts.down", "experts_mlp_w2", true),
        ] {
            if param == skip {
                continue;
            }
            let mut expr = vec![json!(format!("{{p}}layers.{{L}}.block_sparse_moe.{tensor}")), json!({"reshape": [e, inter, hidden]})];
            if down {
                expr.push(json!("transpose"));
            }
            w.insert(param.to_string(), Value::Array(expr));
        }
        Value::Object(w)
    };
    // The roles of the three expert tensors are NOT named: the expressions make them unnecessary.
    let names = json!({"moe.router": "{p}layers.{L}.block_sparse_moe.gate"});
    let (spec1, prog1, bind1) = read(&cfg, Some(adapter("mixtral", Some(names.clone()), weights("")))).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(spec1.hf.weights.len(), 3);
    let compared = assert_same_params("dbrx-style experts", &prog0, &bind0, &ck, &prog1, &bind1, &relaid);
    assert!(compared > 10, "{compared}");
    // The shape check and the unread-tensor report agree with the binding.
    let rep = check_weights(&prog1, &bind1, &relaid);
    assert!(rep.errors.is_empty(), "{:?}", rep.errors);
    assert!(rep.unused.is_empty(), "{:?}", rep.unused);
    // The feature is reported, with the params it binds.
    let f = spec1.features().into_iter().find(|f| f.id.0 == "WEIGHTS_EXPR_V1").expect("WEIGHTS_EXPR_V1 is reported");
    assert!(f.detail.contains("3 param(s)") && f.detail.contains("moe.experts.down"), "{}", f.detail);
    assert!(!spec0.features().iter().any(|f| f.id.0 == "WEIGHTS_EXPR_V1"));

    // A param that is NOT overridden keeps the old error when its role is missing — the tolerance is only
    // for the params an expression replaces.
    let partial = read(&cfg, Some(adapter("mixtral", Some(names), weights("moe.experts.up")))).unwrap_err();
    assert!(partial.contains("no HF tensor name for role `moe.up`"), "{partial}");

    // A typo is never a silent no-op: the key must name a param of the graph, and the message names its neighbours.
    let typo = read(&cfg, Some(adapter("mixtral", None, json!({"moe.expert.gate": ["x"]})))).unwrap_err();
    assert!(typo.contains("moe.expert.gate") && typo.contains("moe.experts.gate"), "{typo}");

    // Streaming: the gate and up experts (a reshape that keeps the last axis) are evaluated by row ranges,
    // block for block equal to the whole; the down experts (a transpose) are loaded whole.
    let none = BTreeMap::new();
    let r = Resolver::new(&relaid, &bind1.aliases);
    for (pi, d) in prog1.params.iter().enumerate() {
        if !d.name.starts_with("moe.experts.") {
            continue;
        }
        let l = layers(&prog1, pi)[0];
        let space = src_row_space(&bind1.srcs[pi], &r, l, &none).unwrap();
        if d.name == "moe.experts.down" {
            assert_eq!(space, None, "a transposed value is not row-streamable");
            continue;
        }
        let (rows, cols) = space.expect("a reshape that keeps the last axis streams");
        let whole = eval_src(&bind1.srcs[pi], &r, l, &none).unwrap();
        for bs in [1usize, 7, 64, rows] {
            let mut got = Vec::new();
            for b in row_blocks(rows, bs) {
                got.extend(eval_src_rows(&bind1.srcs[pi], &r, l, &none, b).unwrap().data);
            }
            assert_eq!(got, whole.data, "{} block {bs}", d.name);
        }
        assert_eq!(whole.numel(), rows * cols);
    }
}

/// Granite-4 stores the shared expert's gate and up fused in one tensor (`input_linear [2I, D]`, gate rows first).
#[test]
fn a_fused_shared_expert_binds_by_row_ranges() {
    let (cfg, dir) = (config("qwen2_moe"), fixture("qwen2_moe"));
    let ck = Checkpoint::open(&dir).unwrap();
    let (spec0, prog0, bind0) = read(&cfg, None).unwrap();
    assert!(spec0.layers.iter().any(|l| matches!(&l.ffn, misaka_palw_tir_lower::spec::Ffn::Moe(m) if m.shared.is_some())));
    let shared = 24usize;
    let mut relaid = MapSource::default();
    for n in ck.names() {
        if !(n.contains(".shared_expert.gate_proj.") || n.contains(".shared_expert.up_proj.")) {
            relaid.0.insert(n.clone(), ck.load(&n).unwrap());
        }
    }
    let mut fused = 0;
    for l in 0..spec0.layers.len() {
        let (g, u) = (format!("model.layers.{l}.mlp.shared_expert.gate_proj.weight"), format!("model.layers.{l}.mlp.shared_expert.up_proj.weight"));
        if ck.shape(&g).is_some() {
            relaid.0.insert(format!("model.layers.{l}.mlp.shared_fused.weight"), cat_rows(&[ck.load(&g).unwrap(), ck.load(&u).unwrap()]));
            fused += 1;
        }
    }
    assert!(fused > 0);
    let names = json!({
        "moe.router": "{p}layers.{L}.mlp.gate",
        "moe.gate": "{p}layers.{L}.mlp.experts.{E}.gate_proj",
        "moe.up": "{p}layers.{L}.mlp.experts.{E}.up_proj",
        "moe.down": "{p}layers.{L}.mlp.experts.{E}.down_proj",
        "moe.shared.down": "{p}layers.{L}.mlp.shared_expert.down_proj",
        "moe.shared_gate": "{p}layers.{L}.mlp.shared_expert_gate",
    });
    let weights = json!({
        "moe.shared.gate.w": ["{p}layers.{L}.mlp.shared_fused.weight", {"rows": [0, shared]}],
        "moe.shared.up.w": ["{p}layers.{L}.mlp.shared_fused.weight", {"rows": [shared, shared]}],
    });
    let (_, prog1, bind1) = read(&cfg, Some(adapter("qwen2-moe", Some(names), weights))).unwrap_or_else(|e| panic!("{e}"));
    assert_same_params("fused shared expert", &prog0, &bind0, &ck, &prog1, &bind1, &relaid);
    let rep = check_weights(&prog1, &bind1, &relaid);
    assert!(rep.errors.is_empty() && rep.unused.is_empty(), "{:?} {:?}", rep.errors, rep.unused);
}

/// ERNIE-4.5's `e_score_correction_bias` is stored `[1, E]`; the graph wants `[E]`. Without the step the
/// message names the fix; with `{"reshape": "param"}` the param binds, equal to the original.
#[test]
fn a_size_one_axis_is_a_reshape_step_never_a_silent_squeeze() {
    let (cfg, dir) = (config("deepseek_v3"), fixture("deepseek_v3"));
    let ck = Checkpoint::open(&dir).unwrap();
    let (_, prog0, bind0) = read(&cfg, None).unwrap();
    let mut relaid = MapSource::default();
    let mut squeezed = 0;
    for n in ck.names() {
        let t = ck.load(&n).unwrap();
        if n.ends_with(".e_score_correction_bias") {
            let len = t.numel();
            relaid.0.insert(n, Tensor::new(vec![1, len], t.data));
            squeezed += 1;
        } else {
            relaid.0.insert(n, t);
        }
    }
    assert!(squeezed > 0);
    // As is: the shape check refuses, and says what to add.
    let rep = check_weights(&prog0, &bind0, &relaid);
    let err = rep.errors.iter().find(|e| e.contains("moe.sel_bias")).unwrap_or_else(|| panic!("{:?}", rep.errors));
    assert!(err.contains("size-1") && err.contains("\"reshape\": \"param\""), "{err}");
    // With the step: bound, and equal to the original binding on the original layout.
    let weights = json!({"moe.sel_bias": ["{p}layers.{L}.mlp.gate.e_score_correction_bias", {"reshape": "param"}]});
    let (_, prog1, bind1) = read(&cfg, Some(adapter("deepseek-v3", None, weights))).unwrap_or_else(|e| panic!("{e}"));
    assert_same_params("squeezed bias", &prog0, &bind0, &ck, &prog1, &bind1, &relaid);
    let rep = check_weights(&prog1, &bind1, &relaid);
    assert!(rep.errors.is_empty() && rep.unused.is_empty(), "{:?} {:?}", rep.errors, rep.unused);
}

/// **The grammar is general**: every `Src` that any built-in binding produces, on every fixture, survives
/// `to_expr` → `parse_weight_expr` unchanged. The enumerated layouts of the binder (fused qkv in three
/// layouts, fused gate/up, stacked experts, interleaved experts, conv1d, table shards, `A_log`, RWKV's
/// rescale, per-layer slices) are therefore shorthand for expressions, not things only Rust can say.
#[test]
fn the_enumerated_layouts_are_expressions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root).unwrap().flatten().map(|e| e.path()).filter(|d| d.join("config.json").exists()).collect();
    dirs.sort();
    assert!(dirs.len() >= 60, "{} fixtures", dirs.len());
    let mut ops: BTreeSet<String> = BTreeSet::new();
    let (mut checked, mut failed) = (0usize, Vec::new());
    for dir in &dirs {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let spec = parse_config_str(&std::fs::read_to_string(dir.join("config.json")).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let prog = hl::build_program(&spec).unwrap_or_else(|e| panic!("{name}: {e}"));
        let binding = hf_weights::bind(&spec, &prog).unwrap_or_else(|e| panic!("{name}: {e}"));
        for (d, src) in prog.params.iter().zip(&binding.srcs) {
            let Some(expr) = to_expr(src) else {
                assert!(src.is_quant(), "{name} `{}`: only a pre-quantised source has no expression", d.name);
                continue;
            };
            for step in expr.as_array().unwrap().iter().skip(1) {
                match step {
                    Value::String(s) => {
                        ops.insert(s.clone());
                    }
                    Value::Object(o) => {
                        for k in o.keys().filter(|k| *k != "count") {
                            // `take` and `map` are told apart by their argument's form.
                            let tag = match (k.as_str(), &o[k]) {
                                ("take", Value::Object(t)) if t.contains_key("per_layer") => "take.per_layer".to_string(),
                                ("take", Value::Object(t)) if t.contains_key("block") => "take.strided".to_string(),
                                ("map", Value::String(s)) => format!("map.{s}"),
                                ("map", Value::Object(m)) => format!("map.{}", m.keys().next().unwrap()),
                                _ => k.clone(),
                            };
                            ops.insert(tag);
                        }
                    }
                    other => panic!("{other}"),
                }
            }
            match parse_weight_expr(&d.name, &expr, &d.shape, d.per_layer) {
                Ok(back) if &back == src => checked += 1,
                Ok(back) => failed.push(format!("{name} `{}`: {src:?} came back as {back:?}", d.name)),
                Err(e) => failed.push(format!("{name} `{}`: {e} (from {expr})", d.name)),
            }
        }
    }
    assert!(failed.is_empty(), "{} of {} bindings do not round-trip:\n{}", failed.len(), checked + failed.len(), failed.join("\n"));
    // The fixtures use every step of the grammar (the three maps are `neg_exp`, RWKV's rescale and — only in a
    // test of its own — `scale`): the round trip covers the surface, not a corner of it.
    for want in ["transpose", "reshape", "rows", "take", "take.strided", "take.per_layer", "stack", "pad_rows", "map.neg_exp", "map.rescale_by_layer"] {
        assert!(ops.contains(want), "no fixture binding uses `{want}`: {ops:?}");
    }
    assert!(checked > 1000, "{checked}");
}

// ───────────────────────── the corpus acceptance (header only) ─────────────────────────

fn fr01(id: &str, file: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fr01").join(id).join(file);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The names and shapes transformers' own tiny model saved (`tools/corpus/specs/<id>/tensors.json` of the corpus
/// lane): the ground truth of the storage, with no weights.
fn index_of(id: &str) -> TensorIndex {
    let v: Value = serde_json::from_str(&fr01(id, "tensors.json")).unwrap();
    TensorIndex::from_shapes(v.as_object().unwrap().iter().map(|(n, e)| {
        (n.clone(), e["shape"].as_array().unwrap().iter().map(|d| d.as_u64().unwrap() as usize).collect::<Vec<usize>>())
    }))
}

/// FR-01's acceptance: `dbrx`, `granitemoehybrid` and `ernie4_5_moe` — the three corpus entries whose only
/// novelty was how their tensors are laid out — are Level B with their adapters as DATA. Their `before.json`
/// (the third-party adapters, names only) are refused by the same check, naming what does not fit.
#[test]
fn the_three_corpus_families_that_needed_weights_as_data_are_level_b_now() {
    for id in ["dbrx", "granitemoehybrid", "ernie4_5_moe"] {
        let cfg: Value = serde_json::from_str(&fr01(id, "config.json")).unwrap();
        let tensors = index_of(id);
        let run = |file: &str| analyze(&cfg, Some(&tensors), &ReadOptions { adapter: AdapterChoice::Text(fr01(id, file)) });
        let after = run("adapter.json");
        assert_eq!(after.result, ReportResult::Lowerable, "{id}: {}", after.render());
        assert_eq!(after.level, Level::B, "{id}");
        assert!(after.weight_errors.is_empty() && after.unread_tensors.is_empty(), "{id}: {:?} {:?}", after.weight_errors, after.unread_tensors);
        assert!(after.features.iter().any(|f| f.id == "WEIGHTS_EXPR_V1"), "{id}: {:?}", after.features.iter().map(|f| &f.id).collect::<Vec<_>>());
        assert!(!after.new_consensus_primitive_required && !after.new_court_kernel_required, "{id}");
        // The pack carries them now: with no adapter given, the built-in one claims the class and says the same.
        let auto = analyze(&cfg, Some(&tensors), &ReadOptions::default());
        assert_eq!(auto.result, ReportResult::Lowerable, "{id}: {}", auto.render());
        assert_eq!(auto.level, Level::B, "{id}");
        assert!(matches!(&auto.adapter, misaka_palw_tir_lower::hf_schema::AdapterSource::BuiltIn { id: a, .. } if a.starts_with(&id[..4])), "{id}: {:?}", auto.adapter);
        // Before: refused, with the numbers (a missing tensor, or a shape the graph does not accept).
        let before = run("before.json");
        assert!(matches!(before.result, ReportResult::NotLowerable { .. }), "{id}: the names-only adapter must not pass:\n{}", before.render());
        assert!(!before.weight_errors.is_empty(), "{id}: {}", before.render());
    }
    // The shape of the "before" error is what an author sees: for ERNIE it names the fix.
    let cfg: Value = serde_json::from_str(&fr01("ernie4_5_moe", "config.json")).unwrap();
    let b = analyze(&cfg, Some(&index_of("ernie4_5_moe")), &ReadOptions { adapter: AdapterChoice::Text(fr01("ernie4_5_moe", "before.json")) });
    assert!(b.render().contains("size-1"), "{}", b.render());
}
