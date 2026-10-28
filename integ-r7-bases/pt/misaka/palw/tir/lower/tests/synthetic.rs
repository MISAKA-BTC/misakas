//! Every tiny architecture config (`tests/configs/tiny/`) with SEEDED SYNTHETIC weights: the HL
//! program validates, runs several positions deterministically (bit-identical on a rerun),
//! stays finite, is causal (a later token never changes an earlier position's logits), and
//! records statistics at every site — no HF files involved.

use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::hl::{self, BlockRole, HlProgram, StateKind};
use misaka_palw_tir_lower::parse_config_str;
use std::path::Path;

fn tiny() -> Vec<(String, HlProgram)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/tiny");
    let mut out = Vec::new();
    let mut names: Vec<_> = std::fs::read_dir(&dir).expect("tiny configs").flatten().map(|e| e.path()).collect();
    names.sort();
    for p in names {
        let name = p.file_stem().unwrap().to_string_lossy().to_string();
        let spec = parse_config_str(&std::fs::read_to_string(&p).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let prog = hl::build_program(&spec).unwrap_or_else(|e| panic!("{name}: {e}"));
        out.push((name, prog));
    }
    assert!(out.len() >= 50, "expected the full tiny corpus, found {}", out.len());
    out
}

const TOKENS: [usize; 7] = [3, 17, 42, 5, 17, 63, 8];

#[test]
fn every_architecture_runs_deterministically_finitely_and_causally() {
    for (name, prog) in tiny() {
        prog.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
        let params = ParamStore::synthetic(&prog, 7);
        let run = |toks: &[usize]| Session::new(&prog, &params).run(toks).unwrap_or_else(|e| panic!("{name}: {e}"));
        let a = run(&TOKENS);
        let b = run(&TOKENS);
        let bits = |v: &Vec<Vec<f32>>| v.iter().flatten().map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&a), bits(&b), "{name}: rerun is not bit-identical");
        assert!(a.iter().flatten().all(|x| x.is_finite()), "{name}: non-finite logits");
        assert_eq!(a.len(), TOKENS.len());
        assert!(a.iter().all(|r| r.len() == prog.vocab));
        // Positions differ from each other (the state and the position actually flow).
        assert_ne!(bits(&vec![a[1].clone()]), bits(&vec![a[4].clone()]), "{name}: positions 1 and 4 (same token) collapsed");
        // Causality: changing token 5 leaves positions 0..5 untouched and changes position 5.
        let mut t2 = TOKENS;
        t2[5] = 1;
        let c = run(&t2);
        for p in 0..5 {
            assert_eq!(bits(&vec![a[p].clone()]), bits(&vec![c[p].clone()]), "{name}: position {p} saw a future token");
        }
        assert_ne!(bits(&vec![a[5].clone()]), bits(&vec![c[5].clone()]), "{name}: the token at position 5 had no effect");
    }
}

#[test]
fn a_different_seed_gives_different_weights_and_logits() {
    for (name, prog) in tiny().into_iter().take(8) {
        let p1 = ParamStore::synthetic(&prog, 1);
        let p2 = ParamStore::synthetic(&prog, 2);
        let a = Session::new(&prog, &p1).run(&TOKENS[..3]).unwrap();
        let b = Session::new(&prog, &p2).run(&TOKENS[..3]).unwrap();
        assert_ne!(a, b, "{name}");
    }
}

#[test]
fn every_site_records_statistics_for_calibration() {
    for (name, prog) in tiny() {
        let params = ParamStore::synthetic(&prog, 3);
        let mut s = Session::new(&prog, &params).with_site_stats();
        s.run(&TOKENS[..4]).unwrap();
        let stats = s.sites.as_ref().unwrap();
        for (bi, b) in prog.blocks.iter().enumerate() {
            let layers: Vec<String> = match b.role {
                BlockRole::Pre => vec!["pre.".into()],
                BlockRole::Post => vec!["post.".into()],
                BlockRole::Layer => {
                    prog.schedule.iter().enumerate().filter(|(_, k)| **k as usize == bi).map(|(l, _)| format!("L{l}.")).collect()
                }
            };
            for site in prog.sites(bi) {
                for l in &layers {
                    let key = format!("{l}{site}");
                    let st = stats.get(&key).unwrap_or_else(|| panic!("{name}: site `{key}` recorded nothing"));
                    assert!(st.count > 0 && st.absmax.is_finite(), "{name}: {key}");
                }
            }
        }
    }
}

#[test]
fn composite_ops_report_their_internal_sites() {
    let progs = tiny();
    let find = |n: &str| progs.iter().find(|(x, _)| x == n).map(|(_, p)| p).unwrap();
    for (arch, sub) in [
        ("qwen3_next", "L0.gdn.core.state"),
        ("llama", "L0.attn.ctx.probs"),
        ("mamba2", "L0.mamba2.scan.state"),
        ("mixtral", "L0.moe.routed.hidden"),
    ] {
        let prog = find(arch);
        let params = ParamStore::synthetic(prog, 5);
        let mut s = Session::new(prog, &params).with_site_stats();
        s.run(&TOKENS[..3]).unwrap();
        assert!(s.sites.as_ref().unwrap().contains_key(sub), "{arch}: no `{sub}`");
    }
}

#[test]
fn schedules_reflect_layer_patterns() {
    let progs = tiny();
    let get = |n: &str| progs.iter().find(|(x, _)| x == n).map(|(_, p)| p.clone()).unwrap();
    // Gemma-3: [sliding, sliding, full] → two block kinds, two rope tables, windowed Hist states.
    let g = get("gemma3");
    assert_eq!(g.schedule.len(), 3);
    assert_eq!(g.schedule[0], g.schedule[1]);
    assert_ne!(g.schedule[1], g.schedule[2]);
    assert!(g.states.iter().any(|s| s.kind == StateKind::Hist { window: Some(4) }));
    assert!(g.states.iter().any(|s| s.kind == StateKind::Hist { window: None }));
    // Qwen3-Next: GDN / attention mix, the GDN layers carry Fixed state and a conv window.
    let q = get("qwen3_next");
    assert!(q.states.iter().any(|s| s.name == "gdn.S" && s.kind == StateKind::Fixed));
    assert!(q.states.iter().any(|s| s.name == "gdn.conv.window"));
    // Jamba (tiny: attention and experts both on odd layers): mamba+mlp / attn+moe alternate.
    let j = get("jamba");
    assert_eq!(j.schedule, vec![j.schedule[0], j.schedule[1], j.schedule[0], j.schedule[1]]);
    assert_ne!(j.schedule[0], j.schedule[1]);
    assert!(j.blocks[j.schedule[0] as usize].name.starts_with("mamba") && j.blocks[j.schedule[1] as usize].name.starts_with("attn"));
    // DeepSeek: dense first layer, MoE after.
    let d = get("deepseek_v3");
    assert_ne!(d.schedule[0], d.schedule[1]);
}
