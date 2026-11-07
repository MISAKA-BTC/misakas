//! Generic artifact preparation publishes only a complete, layer-bound v3 parameter map.
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use std::path::Path;
use std::process::Command;

fn fixture(name: &str) -> (std::path::PathBuf, misaka_palw_tir_sketch::fixture::TirSketchFixtureV1) {
    let f = misaka_palw_tir_sketch::fixture::dense_moe_v1(7);
    let path = std::env::temp_dir().join(format!("kernel-params-{name}-{}.palwtir", std::process::id()));
    misaka_palw_tir_artifact::write_container_v1(&path, &f.program, vec![], [0; 64], "independent parameters".into(), &mut |j, l| {
        Ok(f.params.tensors[&(j, l)].to_le_bytes())
    })
    .unwrap();
    (path, f)
}
fn run(input: &Path, out: &Path, workspace: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_palw-class"))
        .args([
            "kernel-params",
            input.to_str().unwrap(),
            "--tensor-workspace-mib",
            workspace,
            "--payload-limit-mib",
            "1",
            "--out",
            out.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap()
}
#[test]
fn generic_cli_emits_the_exact_v3_map_and_roots_without_model_registry_or_activation() {
    let (input, f) = fixture("success");
    let out = input.with_extension("params.borsh");
    let result = run(&input, &out, "1");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let actual: ParamCommitmentsV1 = borsh::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    let expected = ParamCommitmentsV1::of_v3(&f.params);
    assert_eq!(actual, expected);
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["schema"], "misaka.palw.kernel-params.v3");
    assert_eq!(json["param_root"], misaka_palw_sdk::runtime_pack::commit::hex(&expected.root()));
    assert_eq!(
        json["program_root"],
        misaka_palw_sdk::runtime_pack::commit::hex(&misaka_palw_kernel::public::program_root_v1(&f.program.encode()))
    );
    assert_eq!(json["instances"].as_u64().unwrap(), f.params.tensors.len() as u64);
    assert_eq!(json["registration_or_activation_granted"], false);
    std::fs::remove_file(input).unwrap();
    std::fs::remove_file(out).unwrap();
}
#[test]
fn refusal_never_replaces_the_source_or_publishes_a_partial_preparation() {
    let (input, _) = fixture("refusals");
    let original = std::fs::read(&input).unwrap();
    assert_eq!(run(&input, &input, "1").status.code(), Some(1));
    assert_eq!(std::fs::read(&input).unwrap(), original);
    let out = input.with_extension("params.borsh");
    std::fs::write(&out, b"previous complete result").unwrap();
    for cap in ["0", "18446744073709551615", "-1"] {
        assert_eq!(run(&input, &out, cap).status.code(), Some(1));
        assert_eq!(std::fs::read(&out).unwrap(), b"previous complete result");
    }
    std::fs::write(&input, &original[..original.len() - 1]).unwrap();
    assert_eq!(run(&input, &out, "1").status.code(), Some(1));
    assert_eq!(std::fs::read(&out).unwrap(), b"previous complete result");
    std::fs::remove_file(input).unwrap();
    std::fs::remove_file(out).unwrap();
}
