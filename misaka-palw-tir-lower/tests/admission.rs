//! **`tir_admit_v1` on what this lowerer emits** (spec 04b §10.3): every HF tiny fixture and every
//! real configuration the lowerer accepts is admitted at the legacy court's ceilings — normal form,
//! ranges, per-position costs, every commit point's court cone and every `Fixed` state's
//! checkpoint interval — except DeepSeek-V3, which is refused by name: its position costs more
//! than `2^40` MACs at a `2^18` window. `tile_len` 64 and `h_chunk` 64 are the inputs (a layout
//! declares its own; admission's success does not depend on either for these programs).

use misaka_palw_tir::admit::{TirAdmitInputsV1, TirCeilingsV1, tir_admit_program_v1};
use misaka_palw_tir_lower::lower::{LowerOpts, lower};
use std::path::Path;

fn inputs() -> TirAdmitInputsV1 {
    TirAdmitInputsV1 { tile_len: 64, h_chunk: 64, ceilings: TirCeilingsV1::legacy_court_v1() }
}

fn admit(cfg: &str) -> Result<String, String> {
    let spec = misaka_palw_tir_lower::parse_config_str(cfg).map_err(|e| format!("not lowerable: {e}"))?;
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).map_err(|e| format!("not lowerable: {e}"))?;
    let lw = lower(&hl, &LowerOpts::default()).map_err(|e| format!("not lowerable: {e}"))?;
    let a = tir_admit_program_v1(&lw.program, &inputs()).map_err(|e| format!("REFUSED {e}"))?;
    let worst = a.cones.iter().max_by_key(|c| c.terminal().macs).expect("a cone");
    let nodes = lw.program.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0);
    Ok(format!(
        "largest block {nodes} nodes{}, {} cones, worst terminal {} MACs ({}:{}), C {}, cone work {}, step leaves {}",
        if lw.budget_fallbacks.is_empty() { String::new() } else { format!(" (fallbacks {:?})", lw.budget_fallbacks) },
        a.cones.len(),
        worst.terminal().macs,
        worst.block,
        worst.node,
        a.checkpoint_interval,
        a.cone_work,
        a.position.step_leaves
    ))
}

#[test]
fn every_hf_tiny_fixture_is_admitted() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
    let mut names: Vec<String> =
        std::fs::read_dir(&dir).expect("fixtures").map(|e| e.expect("entry").file_name().to_string_lossy().to_string()).collect();
    names.sort();
    let mut refused = Vec::new();
    for n in &names {
        let cfg = std::fs::read_to_string(dir.join(n).join("config.json")).expect("config");
        match admit(&cfg) {
            Ok(s) => eprintln!("{n:>24}: {s}"),
            Err(e) => {
                eprintln!("{n:>24}: {e}");
                refused.push(n.clone());
            }
        }
    }
    assert_eq!(names.len(), 67);
    assert!(refused.is_empty(), "refused: {refused:?}");
}

#[test]
fn every_lowerable_real_configuration_is_admitted() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real");
    let mut names: Vec<String> =
        std::fs::read_dir(&dir).expect("configs").map(|e| e.expect("entry").file_name().to_string_lossy().to_string()).collect();
    names.sort();
    let mut refused = Vec::new();
    for n in &names {
        let cfg = std::fs::read_to_string(dir.join(n)).expect("config");
        match admit(&cfg) {
            Ok(s) => eprintln!("{n:>40}: {s}"),
            Err(e) if e.starts_with("not lowerable") => eprintln!("{n:>40}: {e}"),
            Err(e) => {
                eprintln!("{n:>40}: {e}");
                assert!(e.starts_with("REFUSED max_position_macs"), "{n}: {e}");
                refused.push(n.clone());
            }
        }
    }
    // 671B parameters at a 2^18-position window: past the per-position MACs (2^40) — refused by
    // name and number, as it should be; every other lowerable configuration is admitted.
    assert_eq!(refused, vec!["deepseek-v3-bf16.json".to_string()]);
}

/// DeepSeek-V3's attention scales with its window: at `2^17` it still passes `2^40` MACs a
/// position, at `2^16` (a class declaring `max_context ≤ 65,536`) it is admitted — criterion 7's
/// "or the ceiling is revised with a stated reason" is a window, not a ceiling.
#[test]
fn deepseek_v3_is_admitted_at_a_window_of_2_16() {
    let cfg = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real/deepseek-v3-bf16.json"))
        .expect("config");
    let spec = misaka_palw_tir_lower::parse_config_str(&cfg).expect("spec");
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).expect("hl");
    let at = |w: u32| {
        let lw = lower(&hl, &LowerOpts { max_window: Some(w), ..LowerOpts::default() }).expect("lowered");
        tir_admit_program_v1(&lw.program, &inputs()).map(|a| a.position.cost.macs)
    };
    assert!(at(1 << 17).is_err());
    let macs = at(1 << 16).expect("admitted at 2^16");
    eprintln!("DeepSeek-V3 at a 2^16 window: {macs} MACs a position");
    assert!(macs <= 1 << 40);
}

/// Freeze criterion 1's evidence: the primitives every lowered architecture uses — all of them
/// among the 25 of PALW-TIR v1, none named after a model.
#[test]
fn every_lowered_architecture_uses_only_the_v1_primitives() {
    use std::collections::BTreeSet;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
    let mut names: Vec<String> =
        std::fs::read_dir(&dir).expect("fixtures").map(|e| e.expect("entry").file_name().to_string_lossy().to_string()).collect();
    names.sort();
    let mut all = BTreeSet::new();
    for n in &names {
        let cfg = std::fs::read_to_string(dir.join(n).join("config.json")).expect("config");
        let spec = misaka_palw_tir_lower::parse_config_str(&cfg).expect("spec");
        let hl = misaka_palw_tir_lower::hl::build_program(&spec).expect("hl");
        let lw = lower(&hl, &LowerOpts::default()).expect("lowered");
        let used: BTreeSet<&str> = lw.program.blocks.iter().flat_map(|b| b.nodes.iter().map(|x| x.prim.name())).collect();
        eprintln!("{n:>20} [{:2}] {}", used.len(), used.iter().copied().collect::<Vec<_>>().join(" "));
        all.extend(used);
    }
    eprintln!("union over the 57 [{}]: {}", all.len(), all.iter().copied().collect::<Vec<_>>().join(" "));
    assert!(all.iter().all(|p| misaka_palw_tir::prim::PRIM_NAMES_V1.contains(p)));
}
