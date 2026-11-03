//! Official PALW attempt executor. Reads `PALW_ANCHOR` (128 hex) and prints the roots
//! `kaspad --palw-produce` would commit for that anchor, plus a material file.
//!
//! ```text
//! PALW_ANCHOR=<128 hex> palw-attempt-exec --material-out capture.bin
//! PALW_ANCHOR=<128 hex> palw-attempt-exec --class <8k class> --artifact-root <inventory root> \
//!     --artifact /path/qwen25-1.5b-a16-8k.palwart --material-out capture.bin
//! ```
//!
//! The floor (`PALW-BASE-0`) is the default and needs no artifact. An 8k draw needs the same
//! container the public producer holds.

use std::path::PathBuf;

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_hashes::Hash64;

use misaka_palw_sdk::PalwClassSdk;

/// Run the anchor the way `kaspad --palw-produce` runs one draw: the network's court, the class the
/// chain named, and `job_for_anchor` so the prompt is the anchor's and not the caller's.
fn execute_producer_anchor_v1(
    params: &Params,
    class_id: Hash64,
    artifact_root: Hash64,
    artifacts: &[PathBuf],
    anchor: Hash64,
) -> Result<PalwExecutionOutcomeV1, String> {
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err(format!("{} has no ConsensusV2 bundle, so it has no PALW executor", params.net));
    };
    let sdk = PalwClassSdk::builtin_v1(bundle.court, params.palw_prompt_ids_form_v1(), params.net.to_string().into_bytes())
        .with_attempt_rules_v1(kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_rules_of_params_v1(params))
        .with_held_answerability_v1(params.palw_held_answerability_v1())
        .with_prefill_run_cap_v1(Some(misaka_palw_base0::qwen25_a16_backend::A16_PREFILL_RUN_POSITIONS as u32));
    let holdings = artifacts.iter().map(|path| sdk.load_artifact(path)).collect::<Result<Vec<_>, _>>()?;
    let backend = sdk.resolve(class_id, artifact_root, &holdings)?;
    let (job, prompt) = backend.job_for_anchor(anchor)?;
    backend.execute(&job, &prompt)
}

/// The floor class and its derived artifact root on this network. Both come from the same bundle the
/// producer resolves against.
fn floor_class_and_root_v1(params: &Params) -> Result<(Hash64, Hash64), String> {
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err(format!("{} has no floor class", params.net));
    };
    let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().map_err(|e| format!("{e:?}"))?;
    Ok((bundle.base_class_id, root))
}

fn params_for_network_v1(network: &str) -> Result<Params, String> {
    let id = network.parse::<NetworkId>().map_err(|e| format!("network {network:?}: {e}"))?;
    Ok(Params::from(id))
}

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("[palw-attempt-exec] fatal: {msg}");
    std::process::exit(1);
}

fn main() {
    let mut network = "testnet-12".to_string();
    let mut class: Option<String> = None;
    let mut artifact_root: Option<String> = None;
    let mut artifacts: Vec<PathBuf> = Vec::new();
    let mut material_out: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = |name: &str| args.next().unwrap_or_else(|| die(format!("{name} needs a value")));
        match flag.as_str() {
            "--network" => network = value("--network"),
            "--class" => class = Some(value("--class")),
            "--artifact-root" => artifact_root = Some(value("--artifact-root")),
            "--artifact" => artifacts.push(PathBuf::from(value("--artifact"))),
            "--material-out" => material_out = Some(PathBuf::from(value("--material-out"))),
            "--help" | "-h" => {
                eprintln!(
                    "palw-attempt-exec [--network testnet-12] [--class HEX] [--artifact-root HEX] \
                     [--artifact PATH]... --material-out PATH\nPALW_ANCHOR must be set (128 hex)."
                );
                return;
            }
            other => die(format!("unknown flag {other}")),
        }
    }
    let anchor_text = std::env::var("PALW_ANCHOR").unwrap_or_else(|_| die("PALW_ANCHOR is not set"));
    let anchor: Hash64 = anchor_text.trim().parse().unwrap_or_else(|_| die("PALW_ANCHOR is not a 128-hex hash"));
    let material_out = material_out.unwrap_or_else(|| die("--material-out is required"));
    let params = params_for_network_v1(&network).unwrap_or_else(|e| die(e));
    let (floor_class, floor_root) = floor_class_and_root_v1(&params).unwrap_or_else(|e| die(e));
    let class_id =
        class.as_deref().map(|s| s.parse::<Hash64>().unwrap_or_else(|_| die("--class is not a 128-hex hash"))).unwrap_or(floor_class);
    let root = artifact_root
        .as_deref()
        .map(|s| s.parse::<Hash64>().unwrap_or_else(|_| die("--artifact-root is not a 128-hex hash")))
        .unwrap_or(floor_root);
    let outcome = execute_producer_anchor_v1(&params, class_id, root, &artifacts, anchor).unwrap_or_else(|e| die(e));
    if let Some(parent) = material_out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| die(format!("cannot create {}: {e}", parent.display())));
        }
    }
    std::fs::write(&material_out, &outcome.material).unwrap_or_else(|e| die(format!("cannot write {}: {e}", material_out.display())));
    println!(
        "{}",
        serde_json::json!({
            "trace_root": outcome.trace_root.to_string(),
            "output_root": outcome.output_root.to_string(),
            "execution_root": outcome.execution_root.to_string(),
            "trace_manifest_root": outcome.trace_manifest_root.to_string(),
            "trace_chunk_count": outcome.trace_chunk_count,
            "material_file": material_out.display().to_string(),
        })
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 8k class the public testnet-12 manifest names (`qwen25-1.5b-a16-8k.palwart`).
    const T12_QWEN25_8K_CLASS_ID: &str = "ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01";
    /// Inventory root of that 8k class, the `--artifact-root` a remote miner passes.
    const T12_QWEN25_8K_INVENTORY_ROOT: &str = "88096dc177826d880c1c5fca4ec93cffe5ab51af108ed169a8e03cd4726308f91263f79f81904b043327bfa277e3558b1656f14259f6dd33603a9f91871aae20";

    #[test]
    fn the_executor_matches_the_producer_resolve_and_execute_on_the_floor() {
        let params = params_for_network_v1("testnet-12").unwrap();
        let (class_id, root) = floor_class_and_root_v1(&params).unwrap();
        let anchor = Hash64::from_u64_word(0x15500);
        let via = execute_producer_anchor_v1(&params, class_id, root, &[], anchor).unwrap();

        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("t12") };
        let net = params.net.to_string().into_bytes();
        let form = params.palw_prompt_ids_form_v1();
        let sdk = PalwClassSdk::builtin_v1(bundle.court, form, net)
            .with_attempt_rules_v1(kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_rules_of_params_v1(&params))
            .with_held_answerability_v1(params.palw_held_answerability_v1());
        let backend = sdk.resolve(class_id, root, &[]).unwrap();
        let (job, prompt) = backend.job_for_anchor(anchor).unwrap();
        let direct = backend.execute(&job, &prompt).unwrap();

        assert_eq!(via.trace_root, direct.trace_root);
        assert_eq!(via.output_root, direct.output_root);
        assert_eq!(via.execution_root, direct.execution_root);
        assert_eq!(via.trace_manifest_root, direct.trace_manifest_root);
        assert_eq!(via.trace_chunk_count, direct.trace_chunk_count);
        assert_eq!(via.material, direct.material);
    }

    #[test]
    fn an_8k_class_without_its_artifact_is_refused() {
        let params = params_for_network_v1("testnet-12").unwrap();
        let class_id: Hash64 = T12_QWEN25_8K_CLASS_ID.parse().unwrap();
        let root: Hash64 = T12_QWEN25_8K_INVENTORY_ROOT.parse().unwrap();
        let err = match execute_producer_anchor_v1(&params, class_id, root, &[], Hash64::from_u64_word(1)) {
            Err(err) => err,
            Ok(_) => panic!("an 8k class with no artifact resolved"),
        };
        assert!(err.contains("holds no artifact"), "{err}");
    }
}
