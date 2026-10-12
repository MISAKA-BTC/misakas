//! **LoRA adapters, unmerged, against HF's merged weights** (RFC-0004: candidate = parent +
//! adapter).
//!
//! Each tiny adapter (`tools/gen_hf_lora_fixtures.py`, PEFT format, no `peft`) is attached to its
//! parent fixture and checked in five steps:
//! 1. the float reference of the candidate, unmerged (`W·x + s·B·A·x`), against transformers with
//!    the adapter merged into the weights (`W + s·B·A`);
//! 2. the parent is lowered, calibrated and materialised on its own;
//! 3. the candidate is lowered with its adapter params behind the parent's. It is materialised
//!    with the parent's calibration plus the adapter's own sites, so its first `P` params must be
//!    the parent's artifact byte for byte, and the rest is the adapter's section;
//! 4. the integer candidate is held against HF's merged logits, which is fidelity;
//! 5. the extra cost is measured: params, bytes, MACs a position, and admission.

use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session, SiteStat};
use misaka_palw_tir_lower::lower::{self, LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::{Checkpoint, Overlay, TensorSource};
use misaka_palw_tir_lower::{fidelity, lora};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

struct Case {
    top1: f64,
    kl: f64,
    kl_parent_vs_merged: f64,
    adapter_params: usize,
    adapter_bytes: usize,
    macs: (u64, u64),
}

fn metrics(int: &[Vec<f64>], hf: &[Vec<f64>]) -> (f64, f64) {
    let lse = |v: &[f64]| {
        let m = v.iter().cloned().fold(f64::MIN, f64::max);
        m + v.iter().map(|x| (x - m).exp()).sum::<f64>().ln()
    };
    let am = |v: &[f64]| v.iter().enumerate().fold((0, f64::MIN), |b, (i, x)| if *x > b.1 { (i, *x) } else { b }).0;
    let (mut agree, mut kl) = (0usize, 0f64);
    for (g, w) in int.iter().zip(hf) {
        let (lp, lq) = (lse(w), lse(g));
        kl += w.iter().zip(g).map(|(p, q)| (p - lp).exp() * ((p - lp) - (q - lq))).sum::<f64>();
        agree += usize::from(am(g) == am(w));
    }
    (agree as f64 / int.len() as f64, kl / int.len() as f64)
}

fn run(name: &str) -> Case {
    let ad_dir = root().join("hf-lora").join(name);
    let ref_: serde_json::Value = serde_json::from_slice(&std::fs::read(ad_dir.join("logits.json")).unwrap()).unwrap();
    let base = ref_["base"].as_str().unwrap();
    let base_dir = root().join("hf").join(base);
    let tokens: Vec<usize> = ref_["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
    let rows = |k: &str| -> Vec<Vec<f64>> {
        ref_[k].as_array().unwrap().iter().map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()).collect()
    };
    let (merged, parent_hf) = (rows("logits_merged"), rows("logits_base"));
    let cfg = std::fs::read_to_string(base_dir.join("config.json")).unwrap();
    let opts = LowerOpts { max_window: Some(64), ..LowerOpts::default() };
    // The parent, as `palw-tir-fidelity` prepares it.
    let parent = fidelity::prepare(&cfg, &opts).expect("parent");
    let (hl_p, bind_p) = (&parent.hl, &parent.binding);
    let ck = Checkpoint::open(&base_dir).expect("checkpoint");
    let (pf, _) = ParamStore::from_source(hl_p, bind_p, &ck).expect("parent params");
    // The candidate, as `palw-tir-fidelity --adapter` prepares it: the parent's spec with the
    // adapter attached, its params last, over the adapter's tensors too.
    let (cand, p) = fidelity::prepare_candidate(&parent, &std::fs::read_to_string(ad_dir.join("adapter_config.json")).unwrap(), &opts)
        .expect("candidate");
    let (hl_c, bind_c) = (&cand.hl, &cand.binding);
    let ad = Checkpoint::open(&ad_dir.join("adapter_model.safetensors")).expect("adapter");
    lora::check_adapter_tensors(&cand.spec, &ad.names()).expect("every adapter tensor is read");
    let (cf, unused) = ParamStore::from_source(hl_c, bind_c, &Overlay { base: &ck, over: &ad }).expect("candidate params");
    assert!(unused.is_empty(), "{name}: tensors not read: {unused:?}");
    // 1. The unmerged float candidate is HF's merged model.
    let fl: Vec<Vec<f64>> =
        Session::new(hl_c, &cf).run(&tokens).unwrap().iter().map(|r| r.iter().map(|x| *x as f64).collect()).collect();
    let rel = (fl.concat().iter().zip(merged.concat()).map(|(a, b)| (a - b) * (a - b)).sum::<f64>()
        / merged.concat().iter().map(|b| b * b).sum::<f64>())
    .sqrt();
    eprintln!("{name}: the float candidate (unmerged) vs HF merged: rel {rel:.2e}");
    assert!(rel < 1e-5, "{name}: float vs merged rel {rel}");
    // 2. The parent's program, calibration and artifact.
    let calib = fidelity::random_sequences(hl_p.vocab, 6, 32, 7);
    let quiet = |_: usize, _: usize| {};
    let lp_loader = Resident(Arc::new(pf));
    let stats_p = fidelity::calibrate(hl_p, &lp_loader, &calib, &quiet).expect("parent calibration");
    let lw_p = &parent.lowered;
    assert!(lw_p.program.params.iter().all(|p| !p.name.contains(lower::LORA_MARK)), "{name}: a parent param looks like an adapter's");
    let mat_p = materialise(lw_p, hl_p, &lp_loader, &stats_p, &QuantPolicy::default(), &quiet).expect("parent artifact");
    // 3. The candidate: adapter params last; the parent's calibration plus the adapter's sites.
    let lc_loader = Resident(Arc::new(cf));
    let stats_c = fidelity::calibrate(hl_c, &lc_loader, &calib, &quiet).expect("candidate calibration");
    let adapter_sites = stats_c.keys().filter(|k| k.contains(lower::LORA_MARK)).count();
    let stats: BTreeMap<String, SiteStat> = fidelity::candidate_stats(&stats_p, stats_c);
    assert!(adapter_sites > 0 && stats.len() == stats_p.len() + adapter_sites, "{name}: the parent's sites plus the adapter's");
    let lw_c = &cand.lowered;
    assert_eq!(p, lw_p.program.params.len(), "{name}: P is the parent's param count");
    assert_eq!(lw_c.program.params[..p], lw_p.program.params[..], "{name}: the candidate's first params are not the parent's");
    let mat_c = materialise(lw_c, hl_c, &lc_loader, &stats, &QuantPolicy::default(), &quiet).expect("candidate artifact");
    for (key, t) in &mat_p.params.tensors {
        assert_eq!(mat_c.params.tensors.get(key), Some(t), "{name}: parent tensor {key:?} changed");
    }
    let adapter: Vec<_> = mat_c.params.tensors.iter().filter(|((j, _), _)| *j as usize >= p).collect();
    let adapter_bytes: usize = adapter.iter().map(|(_, t)| t.len() * t.dtype.width()).sum();
    assert_eq!(mat_c.params.tensors.len(), mat_p.params.tensors.len() + adapter.len(), "{name}: sections");
    assert_eq!((mat_c.resid_scale, mat_c.logits_scale), (mat_p.resid_scale, mat_p.logits_scale), "{name}: the parent's scales");
    // 4. Fidelity: the integer candidate against HF's merged logits.
    let il = fidelity::int_logits(&lw_c.program, &mat_c.params, &tokens, mat_c.logits_scale, &|_| {}).expect("integer run");
    let (top1, kl) = metrics(&il, &merged);
    let (_, kl_pm) = metrics(&parent_hf, &merged);
    // 5. Cost and admission.
    let inputs = misaka_palw_tir_lower::admission::default_inputs();
    let ap = misaka_palw_tir_lower::admission::admit(&lw_p.program, &inputs).expect("parent admitted");
    let ac = misaka_palw_tir_lower::admission::admit(&lw_c.program, &inputs).expect("candidate admitted");
    let largest = |p: &misaka_palw_tir::TirProgramV1| p.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0);
    eprintln!(
        "{name}: integer vs HF merged top-1 {top1:.3}, KL {kl:.5} (the parent is KL {kl_pm:.4} from merged); adapter section {} params, {adapter_bytes} B over the parent's {} B; MACs a position {} → {} (+{:.1}%); largest block {} → {} nodes",
        lw_c.program.params.len() - p,
        mat_p.params.bytes(),
        ap.position.cost.macs,
        ac.position.cost.macs,
        100.0 * (ac.position.cost.macs as f64 / ap.position.cost.macs as f64 - 1.0),
        largest(&lw_p.program),
        largest(&lw_c.program),
    );
    Case {
        top1,
        kl,
        kl_parent_vs_merged: kl_pm,
        adapter_params: lw_c.program.params.len() - p,
        adapter_bytes,
        macs: (ap.position.cost.macs, ac.position.cost.macs),
    }
}

fn check(name: &str) {
    let c = run(name);
    assert!(c.top1 >= 0.9, "{name}: top-1 {}", c.top1);
    assert!(c.kl < 0.02, "{name}: KL {}", c.kl);
    assert!(
        c.kl < c.kl_parent_vs_merged / 10.0,
        "{name}: the adapter's effect is not reproduced (KL {} vs parent {})",
        c.kl,
        c.kl_parent_vs_merged
    );
    assert!(c.adapter_params > 0 && c.adapter_bytes > 0 && c.macs.1 > c.macs.0);
}

#[test]
fn llama_q_k_v_o_and_mlp_rank_16() {
    check("llama_r16");
}

#[test]
fn qwen2_all_linear_rank_4() {
    check("qwen2_all_r4");
}

#[test]
fn phi_rslora_rank_64() {
    check("phi_rs_r64");
}

#[test]
fn mistral_q_v_rank_8_fractional_alpha() {
    check("mistral_qv_r8");
}

/// An adapter's tensors for a module the lowering does not adapt (a router here) are refused, never
/// left unread; and PEFT's all-linear over a fused projection is refused like a named one.
#[test]
fn unread_adapter_tensors_and_fused_all_linear_are_refused() {
    let spec =
        misaka_palw_tir_lower::parse_config_str(&std::fs::read_to_string(root().join("hf/llama/config.json")).unwrap()).unwrap();
    let mut s = spec.clone();
    lora::attach(&mut s, r#"{"peft_type":"LORA","r":8,"lora_alpha":16,"target_modules":["q_proj"]}"#).unwrap();
    let mut names: Vec<String> = (0..s.layers.len())
        .flat_map(|l| ["A", "B"].map(|x| format!("base_model.model.model.layers.{l}.self_attn.q_proj.lora_{x}.weight")))
        .collect();
    lora::check_adapter_tensors(&s, &names).expect("q_proj's pairs are read");
    names.push("base_model.model.model.layers.0.mlp.gate.lora_A.weight".into());
    let e = lora::check_adapter_tensors(&s, &names).unwrap_err().to_string();
    assert!(e.contains("does not adapt") && e.contains("mlp.gate.lora_A"), "{e}");
    let phi3 = misaka_palw_tir_lower::parse_config_str(&std::fs::read_to_string(root().join("hf/phi3_longrope/config.json")).unwrap())
        .unwrap();
    let targets = lora::targets(&phi3);
    assert!(
        targets.iter().any(|t| t.leaf == "qkv_proj" && t.fused) && targets.iter().any(|t| t.leaf == "o_proj" && !t.fused),
        "{targets:?}"
    );
    let e = lora::attach(&mut phi3.clone(), r#"{"peft_type":"LORA","r":8,"lora_alpha":16,"target_modules":"all-linear"}"#)
        .expect_err("all-linear over qkv_proj")
        .to_string();
    assert!(e.contains("fused"), "{e}");
}

/// What is not modelled is refused by name, never ignored.
#[test]
fn unmodelled_adapters_are_refused() {
    let spec =
        misaka_palw_tir_lower::parse_config_str(&std::fs::read_to_string(root().join("hf/llama/config.json")).unwrap()).unwrap();
    for (cfg, why) in [
        (r#"{"peft_type":"LORA","r":8,"lora_alpha":16,"target_modules":["q_proj"],"use_dora":true}"#, "DoRA"),
        (r#"{"peft_type":"LORA","r":8,"lora_alpha":16,"target_modules":["q_proj"],"bias":"all"}"#, "bias"),
        (r#"{"peft_type":"LORA","r":8,"lora_alpha":16,"target_modules":["q_proj"],"modules_to_save":["lm_head"]}"#, "modules_to_save"),
        (r#"{"peft_type":"LORA","r":8,"lora_alpha":16,"target_modules":["q_proj"],"layers_to_transform":[0]}"#, "layers_to_transform"),
        (r#"{"peft_type":"LORA","r":8,"lora_alpha":16,"target_modules":["qkv_proj"]}"#, "fused"),
        (r#"{"peft_type":"LORA","r":8,"lora_alpha":16,"target_modules":["q_proj"],"use_rslora":true}"#, "rsLoRA"),
        (r#"{"peft_type":"LORA","r":8,"lora_alpha":16,"target_modules":["nonexistent"]}"#, "matches no"),
        (r#"{"peft_type":"IA3","target_modules":["q_proj"]}"#, "peft_type"),
    ] {
        let mut s = spec.clone();
        let e = lora::attach(&mut s, cfg).expect_err(why).to_string();
        assert!(e.contains(why), "{why}: {e}");
    }
}
