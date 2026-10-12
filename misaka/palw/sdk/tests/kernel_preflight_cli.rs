//! Independent canonical TIR enters the same node-bound preflight without model/frontend data.
use std::process::Command;

fn run(path: &std::path::Path, positions: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_palw-class"))
        .args(["kernel-preflight", path.to_str().unwrap(), "--positions", positions, "--json"])
        .output()
        .unwrap()
}

#[test]
fn direct_tir_cli_reports_exact_context_node_bounds_and_dormant_shipping_status() {
    let fx = misaka_palw_tir_sketch::fixture::dense_moe_windowed_v1(7, 8);
    let bytes = fx.program.encode();
    let path = std::env::temp_dir().join(format!("kernel-preflight-direct-{}.tir", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();
    let output = run(&path, "2");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], "misaka.palw.kernel-preflight.v1");
    assert_eq!(
        report["program_root"],
        misaka_palw_sdk::runtime_pack::commit::hex(&misaka_palw_kernel::public::program_root_v1(&bytes))
    );
    assert_eq!(report["route"]["max_positions"], 2);
    assert_eq!(report["route"]["shipped"], "KERNEL_NOT_ACTIVE");
    assert_ne!(report["route"]["bucket"], "supported_active_kernel");
    let output = run(&path, "4294967295");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["route"]["max_positions"], u32::MAX);
    assert_eq!(output.status.code(), Some(2), "the large request cannot be accepted by substituting a shorter plan");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn malformed_oversized_or_zero_context_inputs_are_refused_without_frontend_expansion() {
    let path = std::env::temp_dir().join(format!("kernel-preflight-refused-{}.tir", std::process::id()));
    let fx = misaka_palw_tir_sketch::fixture::wide128_v1(7);
    let mut bytes = fx.program.encode();
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(run(&path, "0").status.code(), Some(1));
    bytes.push(1);
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(run(&path, "2").status.code(), Some(1));
    std::fs::write(&path, vec![0; misaka_palw_tir::program::MAX_PROGRAM_BYTES + 1]).unwrap();
    assert_eq!(run(&path, "2").status.code(), Some(1));
    std::fs::remove_file(path).unwrap();
}
