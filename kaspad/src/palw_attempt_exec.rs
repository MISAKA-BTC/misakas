//! The official attempt executor (`palw-attempt-exec`).
//!
//! A remote miner passes `PALW_ANCHOR` and this runs the same resolution the node's producer uses:
//! `PalwBackendRegistry::for_node_v1`, then `resolve`, `job_for_anchor`, and `execute`. The floor
//! (`PALW-BASE-0`) is derived and needs no artifact file. An 8k class needs the chain's
//! `qwen25-1.5b-a16-8k.palwart` on `--artifact`, the same file the producer was given.

use std::path::PathBuf;

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_hashes::Hash64;

use crate::palw_backends::{PalwBackendRegistry, load_class_holdings_v1};

/// Run the anchor the way `kaspad --palw-produce` runs one draw: the network's court, the class the
/// chain named, and `job_for_anchor` so the prompt is the anchor's and not the caller's.
pub fn execute_producer_anchor_v1(
    params: &Params,
    class_id: Hash64,
    artifact_root: Hash64,
    artifacts: &[PathBuf],
    anchor: Hash64,
) -> Result<PalwExecutionOutcomeV1, String> {
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err(format!("{} has no ConsensusV2 bundle, so it has no PALW executor", params.net));
    };
    let net = params.net.to_string().into_bytes();
    let form = params.palw_prompt_ids_form_v1();
    let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(bundle.court, form, net.clone());
    let holdings = if artifacts.is_empty() {
        Vec::new()
    } else {
        load_class_holdings_v1("palw-attempt-exec", &sdk, artifacts, 0, misaka_palw_sdk::PalwWeightResidencyV1::PageCache)
    };
    let registry = PalwBackendRegistry::for_node_v1(params, bundle.court, form, holdings, net, true);
    let backend = registry.resolve(class_id, artifact_root)?;
    let (job, prompt) = backend.job_for_anchor(anchor)?;
    backend.execute(&job, &prompt)
}

/// The floor class and its derived artifact root on this network. Both come from the same bundle the
/// producer resolves against.
pub fn floor_class_and_root_v1(params: &Params) -> Result<(Hash64, Hash64), String> {
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err(format!("{} has no floor class", params.net));
    };
    let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().map_err(|e| format!("{e:?}"))?;
    Ok((bundle.base_class_id, root))
}

pub fn params_for_network_v1(network: &str) -> Result<Params, String> {
    let id = network.parse::<NetworkId>().map_err(|e| format!("network {network:?}: {e}"))?;
    Ok(Params::from(id))
}

/// The 8k class the public testnet-12 manifest names (`qwen25-1.5b-a16-8k.palwart`).
pub const T12_QWEN25_8K_CLASS_ID: &str =
    "ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01";
/// Inventory root of that 8k class, the `--artifact-root` a remote miner passes.
pub const T12_QWEN25_8K_INVENTORY_ROOT: &str =
    "88096dc177826d880c1c5fca4ec93cffe5ab51af108ed169a8e03cd4726308f91263f79f81904b043327bfa277e3558b1656f14259f6dd33603a9f91871aae20";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_executor_matches_the_producer_resolve_and_execute_on_the_floor() {
        let params = params_for_network_v1("testnet-12").unwrap();
        let (class_id, root) = floor_class_and_root_v1(&params).unwrap();
        let anchor = Hash64::from_u64_word(0x15500);
        let via = execute_producer_anchor_v1(&params, class_id, root, &[], anchor).unwrap();

        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("t12") };
        let net = params.net.to_string().into_bytes();
        let form = params.palw_prompt_ids_form_v1();
        let registry = PalwBackendRegistry::for_node_v1(&params, bundle.court, form, Vec::new(), net, true);
        let backend = registry.resolve(class_id, root).unwrap();
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

    #[test]
    fn the_panel_flag_accepts_an_http_provider_beside_a_directory() {
        let specs = misaka_palw_remote::transport::parse_provider_list_v1(
            "http://169.58.232.113:8088,/var/lib/palw-evidence",
        )
        .unwrap();
        assert!(matches!(specs[0], misaka_palw_remote::transport::ProviderSpecV1::Http(_)));
        assert!(matches!(specs[1], misaka_palw_remote::transport::ProviderSpecV1::Dir(_)));
    }
}
