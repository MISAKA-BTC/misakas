//! Construction boundary for the unpublished integration probe only.
//!
//! Ordinary kaspad has no Args activation path for this constructor. An explicit
//! feature-required private binary selects only exact named fixture factories.
//! Production presets and their startup refusals remain unchanged; the helper runs an explicitly supplied private
//! Config over loopback services and a separately owned, fingerprinted database.

use crate::{
    args::Args,
    daemon::{Runtime, create_core_with_runtime_inner, validate_args},
};
use kaspa_consensus_core::config::private_population::derive_private_population_v1;
use kaspa_consensus_core::config::{
    Config,
    drill::{
        PalwDrillKeyringV1, palw_drill_network_v1, palw_t12_drill_genesis_block_v1, palw_t12_drill_genesis_utxos_v1,
        palw_t12_drill_main_key_v1, palw_t12_public_genesis_txid_v1,
    },
};
use kaspa_core::core::Core;
use kaspa_rpc_service::service::RpcCoreService;
use kaspa_utils::networking::ContextualNetAddress;
use kaspa_wrpc_server::service::WrpcEncoding;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::Arc,
};

mod bootstrap;
pub use bootstrap::export_private_complete_bootstrap_v1;
mod claim_paid_bootstrap;
mod paid_request;
pub use paid_request::generate_private_paid_request_v4;
pub use claim_paid_bootstrap::{PrivatePaidNativeRequestV4, PrivatePaidProgramRequestV3, export_private_paid_program_work_v3};
mod paid_claim_factory;
pub use paid_claim_factory::private_paid_claim_params_v3;
mod paid_v4_factory;
pub use paid_v4_factory::private_paid_claim_params_v4;
mod paid_bootstrap;
pub use paid_bootstrap::export_private_paid_bootstrap_v3;
mod paid_v4_bootstrap;
pub use paid_v4_bootstrap::export_private_paid_bootstrap_v4;
mod native_provider;
pub use native_provider::PrivateNativeCustodySpecV3;
pub(crate) mod recovery_ack_carrier_v4;
mod recovery_observation_v4;
mod exec_origin_coverage_v1;
mod kernel_paid_producer_v4;
pub use kernel_paid_producer_v4::PrivateKernelPaidWorkSpecV4;
mod kernel_paid_checker_v4;
pub use kernel_paid_checker_v4::PrivateKernelPaidCheckSpecV4;
mod kernel_paid_replace_v4;
pub use kernel_paid_replace_v4::PrivateKernelPaidReplaceSpecV4;
pub use recovery_ack_carrier_v4::PrivateRecoveryAckFeeSpecV4;

pub fn parse_private_recovery_provider_roster_v4(
    raw: &str,
) -> Result<Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>, String> {
    let providers = raw
        .split(',')
        .map(crate::palw_producer::parse_outpoint)
        .map(|outpoint| outpoint.map(kaspa_consensus_core::palw_state_v2::PalwBondKeyV2))
        .collect::<Result<Vec<_>, _>>()?;
    if !(2..=8).contains(&providers.len()) || providers.windows(2).any(|p| p[0] >= p[1]) {
        return Err("recovery providers must be a sorted distinct roster of 2..8 bonds".into());
    }
    Ok(providers)
}

pub const PRIVATE_PROBE_MARKER_V1: &str = "rfc12-private-probe-v1.json";

/// Exact private native-settlement dependency bundle. The factory retains real
/// Rule E and its header commitment; only release-arming refusals are excepted.
pub fn private_native_settlement_params_v1(
    mut params: kaspa_consensus_core::config::params::Params,
) -> Result<kaspa_consensus_core::config::params::Params, String> {
    use kaspa_consensus_core::{
        config::params::ForkActivation,
        palw_native_settlement_v1::{PalwDnsRetirementV1, PalwSettlementPolicyV1},
    };
    params.palw_fork_choice_rule_e_v1 = Some(ForkActivation::new(1));
    params.palw_fork_choice_commitment_v1 = Some(ForkActivation::new(1));
    params.palw_dns_retirement = Some(PalwDnsRetirementV1 {
        activation: ForkActivation::new(1),
        settlement: PalwSettlementPolicyV1 {
            settled_anchor_depth: 1,
            unique_mature_work: 1,
            max_operator_permille: 1000,
            max_class_permille: 1000,
        },
        legacy_evidence_horizon_daa: 5,
    });
    validate_private_native_relations(&params)?;
    Ok(params)
}

/// The smallest later retirement height a private profile accepts: DAA 1 stays the original profile, so a later
/// height is never a second name for it.
pub const PRIVATE_LATE_RETIREMENT_MIN_DAA_V1: u64 = 2;

/// **RFC-0012 §4.4 (user decision 2026-10-15, §8): the private-only profile that arms `palw_dns_retirement_v1`
/// at a later, explicit DAA**, so a local multi-process run accumulates real DNS-validator history (bonds,
/// attestations, coinbase validator shares, quality liabilities, reserve, exits) before the fence. Applied to
/// an already built private profile (which armed retirement at DAA 1): only the retirement height moves; the
/// settlement policy, the legacy evidence horizon, Rule E and its commitment (both DAA 1, so at or below the
/// fence) are unchanged, and every production value/dependency validator runs again on the result. Reachable
/// only from the feature-gated private entry point; no preset, `Params` default or public network reads it.
pub fn private_late_retirement_params_v1(
    mut params: kaspa_consensus_core::config::params::Params,
    retirement_daa: u64,
) -> Result<kaspa_consensus_core::config::params::Params, String> {
    use kaspa_consensus_core::config::params::ForkActivation;
    if retirement_daa < PRIVATE_LATE_RETIREMENT_MIN_DAA_V1 || retirement_daa == u64::MAX {
        return Err(format!(
            "a later private retirement must be a finite DAA of at least {PRIVATE_LATE_RETIREMENT_MIN_DAA_V1} (DAA 1 is the original profile)"
        ));
    }
    let retirement = params.palw_dns_retirement.as_mut().ok_or("this private profile arms no DNS retirement to move")?;
    if retirement.activation != ForkActivation::new(1) {
        return Err("the later retirement applies only to a profile built with its original DAA-1 retirement".into());
    }
    retirement.activation = ForkActivation::new(retirement_daa);
    validate_private_native_relations(&params)?;
    Ok(params)
}

/// The exact salted factory policy a running Config must equal, where its only permitted departure is the later
/// private retirement height ([`private_late_retirement_params_v1`]): every other value still has to match.
fn private_expected_retirement_v1(
    expected: kaspa_consensus_core::config::params::Params,
    config: &Config,
) -> Result<kaspa_consensus_core::config::params::Params, String> {
    use kaspa_consensus_core::config::params::ForkActivation;
    match config.params.palw_dns_retirement.map(|r| r.activation) {
        Some(activation) if activation != ForkActivation::new(1) => private_late_retirement_params_v1(expected, activation.daa_score()),
        _ => Ok(expected),
    }
}

fn validate_private_native_relations(params: &kaspa_consensus_core::config::params::Params) -> Result<(), String> {
    use kaspa_consensus_core::config::params::ForkActivation;
    params.validate_palw_fork_choice_rule_e_order_v1().map_err(|e| e.to_string())?;
    if params.palw_fork_choice_rule_e_v1 != Some(ForkActivation::new(1))
        || params.palw_fork_choice_commitment_v1 != Some(ForkActivation::new(1))
        || params.palw_dns_retirement.is_none_or(|r| r.activation == ForkActivation::never() || r.activation.daa_score() < 1)
    {
        return Err("private retirement requires Rule E/commitment at DAA 1 and a finite retirement at or after them".into());
    }
    let mut checked = params.clone();
    checked.palw_probabilistic_constraints_v1 = None;
    // The typed roots ride the kernel route this copy makes dormant (the paid-claim profiles arm them from DAA 0).
    checked.palw_typed_roots_v1 = None;
    checked.palw_signed_registration_v1 = None;
    checked.palw_panel_free_v1 = None;
    // A paid-claim profile (v3/v4): its explicitly unapproved release fences are validated by its own relations
    // (`paid_claim_factory::validate_private_paid_claim_relations_v3` and the v4 scope check) and are dormant here as there,
    // so the production validators below judge everything else, the retirement among it.
    if checked.palw_claim_challenge_v2.is_some() {
        let never = ForkActivation::never();
        if let Some(f) = checked.palw_beacon_source_opt_in_v1.as_mut() {
            f.activation = never;
        }
        if let Some(f) = checked.palw_beacon_source_fee_v1.as_mut() {
            f.activation = never;
        }
        if let Some(f) = checked.palw_claim_challenge_v2.as_mut() {
            f.activation = never;
        }
        if let Some(f) = checked.palw_monetary_material_v1.as_mut() {
            f.activation = never;
        }
        if let Some(f) = checked.palw_material_release_recovery_v2.as_mut() {
            f.activation = never;
        }
        if let Some(f) = checked.palw_panel_lifetime_v2.as_mut() {
            f.activation = never;
        }
        if let Some(f) = checked.palw_paid_consumer_lifecycle_v1.as_mut() {
            f.activation = never;
        }
        checked.palw_reporter_income_v2 = Some(never);
        checked.palw_material_income_v3 = Some(never);
        checked.palw_provider_court_v1 = Some(never);
        checked.sync_palw_claim_challenge_v2();
        checked.sync_palw_permissionless_panel_v1();
    }
    // The OPV mechanics budget rides the Panel-free fence (RFC-0015 §8.3 lead default); its policy is checked here, its arming
    // refusal excepted like the fence's.
    if let Some(budget) = checked.palw_bond_budget_v1.as_ref().filter(|f| f.activation != ForkActivation::never()) {
        budget.policy.validate().map_err(str::to_owned)?;
        checked.palw_bond_budget_v1 = None;
        checked.sync_palw_bond_budget_v1();
    }
    // The private entry point retains the armed commitment/Rule E/retirement
    // while running every production value and dependency validator.
    checked.validate_palw_v1().map_err(|e| e.to_string())?;
    checked.validate_palw_v2_private_rule_e_v1().map_err(|e| e.to_string())
}

/// Exact complete-only private policy for a public registration/bootstrap probe.
/// The test constructor alone can run this policy; ordinary startup continues
/// to refuse the three unreleased activation fences. No class, capability,
/// claim, trace, receipt, Final or initial monetary row is inserted here.
pub fn private_complete_check_params_v1(
    salt: &kaspa_consensus_core::config::drill::PalwDrillSaltV1,
) -> Result<kaspa_consensus_core::config::params::Params, String> {
    use kaspa_consensus_core::{
        config::params::ForkActivation, palw_panel_free_v1::PalwPanelFreeFenceV1, palw_tir_v1::PalwTirFenceV1,
    };
    let mut params = kaspa_consensus_core::config::drill::palw_chain_params_v1(palw_drill_network_v1(), Some(salt))?;
    // T12 already arms the canonical economic bundle from genesis. The IR's
    // mirror and its own prerequisite validator must agree at the probe height.
    if params.palw_canonical_work_daa() != Some(0) {
        return Err("the complete-check private factory requires the shipped T12 canonical economic basis at genesis".into());
    }
    params.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(0)));
    params.sync_palw_tir_v1();
    params.validate_palw_tir_v1().map_err(|e| e.to_string())?;
    params.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(0));
    params.palw_signed_registration_v1 = Some(ForkActivation::new(0));
    let panel_free = PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(0), Vec::new());
    panel_free.validate_value()?;
    if panel_free.sampled_conformance_gates_reward
        || panel_free.min_effective_bits != kaspa_consensus_core::palw_panel_free_v1::PALW_OPV_MIN_EFFECTIVE_BITS_V1
    {
        return Err("the complete-only private policy must retain the complete reward gate and full effective target".into());
    }
    params.palw_panel_free_v1 = Some(panel_free);
    // RFC-0015 §8.3 (lead default 2026-10-15, pending user review): a network with an OPV policy budgets every Panel-free claim, so
    // this private profile carries the UNAPPROVED mechanics budget (the paid profile replaces it with its own explicit policy).
    params.arm_palw_opv_mechanics_budget_v1();
    params = private_native_settlement_params_v1(params)?;
    validate_private_complete_relations(&params)?;
    Ok(params)
}

fn validate_private_complete_relations(params: &kaspa_consensus_core::config::params::Params) -> Result<(), String> {
    // Only the explicit release-arming refusal is excepted. Every other startup
    // invariant, including the TIR mirror, economics, lifecycle fence audit and
    // the fixed Panel-free value, is checked by its production implementation.
    params.palw_panel_free_v1.as_ref().ok_or("missing private Panel-free policy")?.validate_value()?;
    validate_private_native_relations(params)
}

/// Test-policy identity, not a release result or economic/security approval.
pub fn private_probe_manifest(config: &Config) -> serde_json::Value {
    let genesis_money = config.palw_drill_genesis_salt.as_ref().map(|salt| {
        let key = palw_t12_drill_main_key_v1(salt);
        let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&key.payload);
        let initial_utxos = match config.private_population {
            Some(profile) => derive_private_population_v1(salt, profile).expect("validated private population").utxos,
            None => palw_t12_drill_genesis_utxos_v1(salt),
        };
        let mut utxos: Vec<_> = initial_utxos
            .into_iter()
            .filter(|(_, entry)| entry.script_public_key == script)
            .map(|(outpoint, entry)| {
                serde_json::json!({
                    "transactionId": outpoint.transaction_id.to_string(), "index": outpoint.index,
                    "amount": entry.amount.to_string(), "blockDaaScore": entry.block_daa_score,
                    "isCoinbase": entry.is_coinbase,
                })
            })
            .collect();
        utxos.sort_by_key(|row| (row["transactionId"].as_str().unwrap().to_owned(), row["index"].as_u64().unwrap()));
        serde_json::json!({"address": key.address(kaspa_addresses::Prefix::Testnet).to_string(), "utxos": utxos})
    });
    let retirement = config.params.palw_dns_retirement.map(|r| {
        serde_json::json!({
            "activationDaa": r.activation.daa_score(),
            "policyId": r.settlement.id().to_string(),
            "depth": r.settlement.settled_anchor_depth,
            "uniqueWork": r.settlement.unique_mature_work.to_string(),
            "maxOperatorPermille": r.settlement.max_operator_permille,
            "maxClassPermille": r.settlement.max_class_permille,
            "legacyEvidenceHorizonDaa": r.legacy_evidence_horizon_daa,
        })
    });
    let mut manifest = serde_json::json!({
        "version": 1,
        "kind": "rfc12-private-integration-probe",
        "paidRequestGenerator": true,
        "importCrashFailpoints": {
            "available": cfg!(feature = "rfc12-import-failpoints"),
            "rendezvousVersion": 1,
        },
        "network": config.params.net.to_string(),
        "genesis": config.params.genesis.hash.to_string(),
        "consensusParamsId": config.params.consensus_params_id().to_string(),
        "saltId": config.palw_drill_genesis_salt.as_ref().map(|salt| salt.id()),
        "genesisMoneyProbe": genesis_money,
        "population": config.private_population.map(|profile| serde_json::json!({
            "version": 1, "totalBonds": profile.total_bonds(),
            "premineSupplySompi": kaspa_consensus_core::config::premine::MISAKA_PREMINE_CAP_SOMPI.to_string(),
            "capabilitiesAdded": false, "participantIndependenceProven": false,
        })),
        "retirement": retirement,
        "forkChoiceRuleEDaa": config.params.palw_fork_choice_rule_e_v1.map(|at| at.daa_score()),
        "forkChoiceCommitmentDaa": config.params.palw_fork_choice_commitment_v1.map(|at| at.daa_score()),
        "skipProofOfWork": config.params.skip_proof_of_work,
        "canonicalWorkDaa": config.params.palw_canonical_work_daa(),
        "completeOnlyPrivatePolicy": config.params.palw_probabilistic_constraints_v1.is_some()
            && config.params.palw_signed_registration_v1.is_some() && config.params.palw_panel_free_v1.is_some(),
        "releaseApproval": false,
        "securitySoundnessMeasured": false,
    });
    // Preserve existing native-v1/population-v1/complete-v1 marker bytes. Only
    // the independently fingerprinted new profile gains its additional fields.
    if let Some(fence) = config.params.palw_claim_challenge_v2.filter(|f| f.activation.is_active(0)) {
        manifest["paidClaimPrivatePolicy"] = serde_json::json!(true);
        manifest["paidClaimMaximumRounds"] = serde_json::json!(fence.policy.maximum_rounds);
        // 2026-10-15: the mechanics-only Panel lifetime and paid consumer lifecycle the paid-v3 factory arms beside PAV3.
        if let Some(lifetime) = config.params.palw_panel_lifetime_v2.filter(|f| f.activation.is_active(0)) {
            manifest["paidPanelLifetimePolicyId"] = serde_json::json!(lifetime.policy.id().to_string());
        }
        if let Some(consumer) = config.params.palw_paid_consumer_lifecycle_v1.as_ref().filter(|f| f.activation.is_active(0)) {
            manifest["paidConsumerLifecyclePolicy"] = serde_json::json!({
                "digest": consumer.policy.digest().to_string(), "serviceSpanDaa": consumer.policy.service_span_daa,
                "disputeTailDaa": consumer.policy.dispute_tail_daa, "materialTailDaa": consumer.policy.material_tail_daa,
                "approved": false,
            });
        }
    }
    if config.params.palw_material_release_recovery_v2.is_some() {
        // Paid-v4 private profile: PAV3 + material release recovery V2. Compact V4 is retired (PRINCIPLES §8).
        manifest["privatePaidProfileVersion"] = serde_json::json!(4);
        manifest["compactClaimPolicy"] = serde_json::json!({"retired": true, "reason": "PRINCIPLES §8"});
    }
    if let Some(recovery) = config.params.palw_material_release_recovery_v2.as_ref() {
        manifest["releaseRecoveryPolicy"] = serde_json::json!({
            "id": recovery.policy.id().to_string(), "activationDaa": recovery.activation.daa_score(),
            "maxReservedContracts": recovery.policy.max_reserved_contracts,
            "maxGenerationsPerContract": recovery.policy.max_generations_per_contract,
            "maxProviderStreams": recovery.policy.max_provider_streams,
            "maxReservedHistoryCells": recovery.policy.max_reserved_history_cells,
            "scopeMarker": recovery.policy.measurement_root.to_string(), "approved": false,
        });
    }
    manifest
}

fn validate_private_probe(args: &Args, config: &Config) -> Result<(), String> {
    validate_args(args).map_err(|error| error.to_string())?;
    if args.network() != palw_drill_network_v1() || config.params.net != args.network() {
        return Err("the private probe requires its testnet-12 identity and matching Args".into());
    }
    let salt = config.palw_drill_genesis_salt.as_ref().ok_or("the private probe needs an explicit salted genesis")?;
    let expected_genesis = match config.private_population {
        Some(profile) => {
            let expected = derive_private_population_v1(salt, profile)?;
            let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(actual) = &config.params.palw_consensus_mode else {
                return Err("the private population needs its native registry".into());
            };
            let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(fixed) = &expected.params.palw_consensus_mode
            else {
                unreachable!()
            };
            if actual.genesis_objects != fixed.genesis_objects {
                return Err("private population registrations differ from the exact salt/size/funded factory".into());
            }
            expected.params.genesis.hash
        }
        None => palw_t12_drill_genesis_block_v1(salt).hash,
    };
    if config.params.genesis.hash != expected_genesis {
        return Err("the private Config does not carry the salt/profile's exact genesis".into());
    }
    #[cfg(feature = "devnet-prealloc")]
    if !config.initial_utxo_set.is_empty() {
        return Err("private population refuses arbitrary initial monetary rows".into());
    }
    #[cfg(feature = "devnet-prealloc")]
    if args.num_prealloc_utxos.is_some() || args.prealloc_address.is_some() {
        return Err("private construction refuses arbitrary Args preallocation".into());
    }
    if config.params.skip_proof_of_work {
        return Err("the process/P2P probe must validate work; skip_proof_of_work is refused".into());
    }
    // Normal Args validation sees no public drill flag in this construction
    // path. Reuse its actual key checks against the Config's salt anyway.
    crate::palw_drill::palw_drill_keys_are_drill_only_v1(args, &PalwDrillKeyringV1::new(*salt)).map_err(|error| error.to_string())?;
    for outpoint in [&args.palw_producer_bond, &args.palw_fee_outpoint, &args.stake_bond].into_iter().flatten() {
        let outpoint = crate::palw_producer::parse_outpoint(outpoint).map_err(|error| error.to_string())?;
        if palw_t12_public_genesis_txid_v1(&outpoint.transaction_id) {
            return Err("a private probe must never name a public genesis monetary outpoint".into());
        }
    }
    let private_complete = config.params.palw_probabilistic_constraints_v1.is_some()
        || config.params.palw_signed_registration_v1.is_some()
        || config.params.palw_panel_free_v1.is_some();
    if config.params.palw_claim_compact_v4.is_some() || config.params.palw_material_release_recovery_v2.is_some() {
        if config.private_population.is_some() {
            return Err("the paid-v4 private probe has no population override".into());
        }
        let expected = private_expected_retirement_v1(private_paid_claim_params_v4(salt)?, config)?;
        if config.params.consensus_params_id() != expected.consensus_params_id() {
            return Err("private paid-v4 Config differs from its exact salted factory policy".into());
        }
        paid_v4_factory::validate_private_paid_claim_relations_v4(&config.params)?;
    } else if config.params.palw_claim_challenge_v2.is_some_and(|f| f.activation.is_active(0)) {
        if config.private_population.is_some() {
            return Err("the paid-v3 private probe has no population override".into());
        }
        let expected = private_expected_retirement_v1(private_paid_claim_params_v3(salt)?, config)?;
        if config.params.consensus_params_id() != expected.consensus_params_id() {
            return Err("private paid-claim Config differs from its exact salted factory policy".into());
        }
        paid_claim_factory::validate_private_paid_claim_relations_v3(&config.params)?;
    } else if private_complete {
        if config.private_population.is_some() {
            return Err("the complete-only private bootstrap profile has no population override".into());
        }
        let expected = private_expected_retirement_v1(private_complete_check_params_v1(salt)?, config)?;
        if config.params.consensus_params_id() != expected.consensus_params_id() {
            return Err("private complete-check Config differs from its exact salted factory policy".into());
        }
        validate_private_complete_relations(&config.params)?;
    } else {
        if config.params.palw_dns_retirement.is_some() {
            validate_private_native_relations(&config.params)?;
        } else {
            config.params.validate_palw_v1().map_err(|error| error.to_string())?;
            config.params.validate_palw_v2().map_err(|error| error.to_string())?;
        }
    }
    let mut applied = config.clone();
    args.apply_to_config(&mut applied);
    if applied.params.consensus_params_id() != config.params.consensus_params_id()
        || applied.palw_drill_genesis_salt != config.palw_drill_genesis_salt
    {
        return Err("private probe Args may not replace its pinned consensus Config".into());
    }
    if !args.disable_dns_seeding
        || args.outbound_target != 0
        || !args.disable_upnp
        || !args.connect_peers.is_empty()
        || !args.add_peers.is_empty()
        || args.externalip.is_some()
    {
        return Err("the private probe requires controlled loopback peers, no discovery or UPnP".into());
    }
    if args.palw_drill_genesis_salt.is_some() || args.override_params_file.is_some() {
        return Err("private Config injection cannot be mixed with Args parameter overrides".into());
    }
    let loopback = |address: ContextualNetAddress| address.has_port() && address.normalize(0).ip.is_loopback();
    if !args.listen.is_some_and(loopback) || !args.rpclisten.is_some_and(loopback) {
        return Err("the private probe requires explicit loopback P2P and gRPC ports".into());
    }
    for (address, encoding) in [(&args.rpclisten_borsh, WrpcEncoding::Borsh), (&args.rpclisten_json, WrpcEncoding::SerdeJson)] {
        if let Some(address) = address
            && !loopback(address.to_address(&config.params.net.network_type, &encoding))
        {
            return Err("every private probe wRPC endpoint must be loopback".into());
        }
    }
    if args.evm_rpc_listen.is_some_and(|address| !loopback(address)) {
        return Err("the private probe EVM endpoint must be loopback".into());
    }
    if !args.appdir.as_ref().is_some_and(|directory| Path::new(directory).is_absolute()) {
        return Err("the private probe needs an explicit absolute owned app directory".into());
    }
    Ok(())
}

/// Refuse to reuse arbitrary existing data or another private policy. A cold
/// restart is allowed only with the exact previously pinned private identity.
fn pin_owned_probe_directory(args: &Args, config: &Config) -> Result<(), String> {
    let directory = Path::new(args.appdir.as_ref().ok_or("missing private app directory")?);
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let marker = directory.join(PRIVATE_PROBE_MARKER_V1);
    let expected = private_probe_manifest(config);
    if marker.exists() {
        let actual: serde_json::Value =
            serde_json::from_slice(&fs::read(&marker).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
        if actual != expected {
            return Err("private probe DB fingerprint/policy differs from its pinned identity".into());
        }
        return Ok(());
    }
    if fs::read_dir(directory).map_err(|error| error.to_string())?.next().is_some() {
        return Err("a private probe may initialize only its empty owned directory".into());
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(marker).map_err(|error| error.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(&expected).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    #[cfg(unix)]
    fs::File::open(directory).and_then(|directory| directory.sync_all()).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn create_private_integration_core_with_runtime(
    runtime: &Runtime,
    args: &Args,
    fd_total_budget: i32,
    config: Config,
) -> Result<(Arc<Core>, Arc<RpcCoreService>), String> {
    prepare_private_integration_probe(args, &config)?;
    kaspa_core::warn!("RFC12 PRIVATE INTEGRATION PROBE: injected test policy; release/soundness evidence remains unapproved");
    let observer = config.params.palw_material_release_recovery_v2.is_some().then(|| {
        Box::new(|ctx: crate::palw_native_custody_v3::PalwNativeCustodyContextV3| {
            recovery_observation_v4::start(&ctx)?;
            Ok(None)
        }) as crate::palw_native_custody_v3::PalwNativeCustodyHookV3
    });
    Ok(create_core_with_runtime_inner(runtime, args, fd_total_budget, Some(config), observer))
}

/// The paid-v3 private node with the RFC-0008 PAV3 loopback native custody wiring (unverified source): a provider service
/// over fixture provider bonds and/or the exchange this node's producer seals through. The spec is checked against the
/// salted fixture and the producer's own key/bond/operator before any service or DB opens.
pub fn create_private_integration_core_with_native_custody_v3(
    runtime: &Runtime,
    args: &Args,
    fd_total_budget: i32,
    config: Config,
    native: PrivateNativeCustodySpecV3,
) -> Result<(Arc<Core>, Arc<RpcCoreService>), String> {
    if config.params.palw_claim_compact_v4.is_some() || config.params.palw_material_release_recovery_v2.is_some() {
        return Err("new compact/recovery capabilities require the explicit paid-v4 construction entry".into());
    }
    if !config.params.palw_claim_challenge_v2.is_some_and(|f| f.activation.is_active(0))
        || config.params.palw_monetary_material_v1.is_none()
    {
        return Err("the private PAV3 native custody wiring belongs to the paid-v3 profile only".into());
    }
    native.check_fixture(&config, args)?;
    prepare_private_integration_probe(args, &config)?;
    kaspa_core::warn!(
        "RFC12 PRIVATE INTEGRATION PROBE: injected test policy and a PAV3 loopback native custody exchange; release/soundness \
         evidence remains unapproved"
    );
    Ok(create_core_with_runtime_inner(runtime, args, fd_total_budget, Some(config), Some(native.into_hook())))
}

/// Explicit new profile entry; its policy identity includes compact proof and
/// current-state release/ACK recovery. It cannot reuse a paid-v3 database marker.
pub fn create_private_integration_core_with_native_custody_v4(
    runtime: &Runtime,
    args: &Args,
    fd_total_budget: i32,
    config: Config,
    native: PrivateNativeCustodySpecV3,
    ack_fees: Option<PrivateRecoveryAckFeeSpecV4>,
    recovery_providers: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
) -> Result<(Arc<Core>, Arc<RpcCoreService>), String> {
    create_private_native_custody_v4_inner(
        runtime,
        args,
        fd_total_budget,
        config,
        Some(native),
        ack_fees,
        recovery_providers,
        None,
        None,
        None,
    )
}

/// Explicit all-target kernel producer entry. Its input and limits are supplied
/// separately from provider ACK funding; ordinary Args cannot activate it.
#[allow(clippy::too_many_arguments)]
pub fn create_private_integration_core_with_kernel_paid_work_v4(
    runtime: &Runtime,
    args: &Args,
    fd_total_budget: i32,
    config: Config,
    native: PrivateNativeCustodySpecV3,
    ack_fees: Option<PrivateRecoveryAckFeeSpecV4>,
    recovery_providers: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    work: PrivateKernelPaidWorkSpecV4,
) -> Result<(Arc<Core>, Arc<RpcCoreService>), String> {
    if !args.palw_produce {
        return Err("the explicit kernel paid-work consumer requires this node's configured producer key/bond".into());
    }
    create_private_native_custody_v4_inner(
        runtime,
        args,
        fd_total_budget,
        config,
        Some(native),
        ack_fees,
        recovery_providers,
        Some(work),
        None,
        None,
    )
}

/// Explicit checker identity, provider endpoint and fee funding are supplied in
/// its own specification. A checker-only node needs no producer exchange/key.
#[allow(clippy::too_many_arguments)]
pub fn create_private_integration_core_with_kernel_paid_consumers_v4(
    runtime: &Runtime,
    args: &Args,
    fd_total_budget: i32,
    config: Config,
    native: Option<PrivateNativeCustodySpecV3>,
    ack_fees: Option<PrivateRecoveryAckFeeSpecV4>,
    recovery_providers: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    work: Option<PrivateKernelPaidWorkSpecV4>,
    checker: Option<PrivateKernelPaidCheckSpecV4>,
    replace: Option<PrivateKernelPaidReplaceSpecV4>,
) -> Result<(Arc<Core>, Arc<RpcCoreService>), String> {
    if (work.is_some() || replace.is_some()) && (!args.palw_produce || native.is_none()) {
        return Err("kernel paid work/replacement requires the configured producer (sponsor) and authenticated custody exchange".into());
    }
    create_private_native_custody_v4_inner(
        runtime,
        args,
        fd_total_budget,
        config,
        native,
        ack_fees,
        recovery_providers,
        work,
        checker,
        replace,
    )
}

#[allow(clippy::too_many_arguments)]
fn create_private_native_custody_v4_inner(
    runtime: &Runtime,
    args: &Args,
    fd_total_budget: i32,
    config: Config,
    native: Option<PrivateNativeCustodySpecV3>,
    ack_fees: Option<PrivateRecoveryAckFeeSpecV4>,
    recovery_providers: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    kernel_work: Option<PrivateKernelPaidWorkSpecV4>,
    kernel_checker: Option<PrivateKernelPaidCheckSpecV4>,
    kernel_replace: Option<PrivateKernelPaidReplaceSpecV4>,
) -> Result<(Arc<Core>, Arc<RpcCoreService>), String> {
    paid_v4_factory::validate_private_paid_claim_relations_v4(&config.params)?;
    if let Some(native) = &native {
        native.check_fixture(&config, args)?;
    } else if kernel_checker.is_none()
        || kernel_work.is_some()
        || kernel_replace.is_some()
        || ack_fees.is_some()
        || !recovery_providers.is_empty()
        || args.palw_produce
    {
        return Err("checker-only construction requires its own checker specification and no producer/custody funding".into());
    }
    if native.as_ref().is_some_and(|n| !n.providers.is_empty()) != ack_fees.is_some() {
        return Err(
            "a paid-v4 hosted provider service requires its own explicit recovery ACK fee payer; an exchange-only node does not"
                .into(),
        );
    }
    if (!recovery_providers.is_empty()
        && (!(2..=8).contains(&recovery_providers.len()) || recovery_providers.windows(2).any(|p| p[0] >= p[1])))
        || (args.palw_produce && recovery_providers.is_empty())
    {
        return Err("a producing paid-v4 custody node needs an explicit sorted distinct roster of 2..8 provider bonds".into());
    }
    prepare_private_integration_probe(args, &config)?;
    kaspa_core::warn!(
        "RFC12 PRIVATE paid-v4: release-recovery mechanics (compact V4 retired); acceptance, soundness and economic approval pending"
    );
    let hook: crate::palw_native_custody_v3::PalwNativeCustodyHookV3 = match (native, ack_fees) {
        (Some(native), Some(fees)) => native.into_hook_with_service_v4(Box::new(move |service, ctx| fees.start(service, ctx))),
        (Some(native), None) => native.into_hook(),
        (None, None) => Box::new(|_| Ok(None)),
        (None, Some(_)) => return Err("recovery ACK funding requires a hosted custody provider".into()),
    };
    let hook = Box::new(move |mut ctx: crate::palw_native_custody_v3::PalwNativeCustodyContextV3| {
        ctx.recovery_provider_roster_v4 = recovery_providers;
        // Prepare from this node's actual manager/config/key before the hook
        // consumes the context. An uploaded file supplies work, not authority.
        let producer = kernel_work.map(|work| work.prepare(&ctx)).transpose()?;
        let checker = kernel_checker.map(|checker| checker.prepare(&ctx)).transpose()?;
        let replacement = kernel_replace.map(|replace| replace.prepare(&ctx)).transpose()?;
        recovery_observation_v4::start(&ctx)?;
        exec_origin_coverage_v1::start(&ctx)?;
        let exchange = hook(ctx)?;
        if let Some(producer) = producer {
            producer
                .start(exchange.as_ref().ok_or("kernel paid work requires an explicit authenticated provider exchange")?.clone())?;
        }
        if let Some(checker) = checker {
            checker.start()?;
        }
        if let Some(replacement) = replacement {
            replacement
                .start(exchange.as_ref().ok_or("kernel replacement requires an explicit authenticated provider exchange")?.clone())?;
        }
        Ok(exchange)
    });
    Ok(create_core_with_runtime_inner(runtime, args, fd_total_budget, Some(config), Some(hook)))
}

/// Validate and pin ownership before Runtime opens a log or any node service.
/// Repeated calls accept only the same exact salted private policy.
pub fn prepare_private_integration_probe(args: &Args, config: &Config) -> Result<(), String> {
    validate_private_probe(args, config)?;
    pin_owned_probe_directory(args, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::config::drill::{PalwDrillSaltV1, palw_chain_params_v1};

    #[test]
    fn private_complete_policy_is_exact_value_valid_and_still_publicly_refused() {
        let salt = PalwDrillSaltV1::from_bytes([0x15; 32]).unwrap();
        let baseline = palw_chain_params_v1(palw_drill_network_v1(), Some(&salt)).unwrap();
        let params = private_complete_check_params_v1(&salt).unwrap();
        assert_eq!(params.genesis.hash, baseline.genesis.hash);
        assert_eq!(params.genesis.bits, baseline.genesis.bits);
        assert!(!params.skip_proof_of_work);
        assert_eq!(params.palw_canonical_work_daa(), Some(0));
        assert!(params.palw_fork_choice_rule_e_active_at(1));
        assert_eq!(params.palw_fork_choice_commitment_v1, Some(kaspa_consensus_core::config::params::ForkActivation::new(1)));
        params.validate_palw_fork_choice_rule_e_order_v1().unwrap();
        assert!(params.validate_palw_v2().is_err(), "ordinary startup keeps its release refusal");
        let opv = params.palw_panel_free_v1.as_ref().unwrap();
        assert!(!opv.sampled_conformance_gates_reward);
        assert_eq!(opv.min_effective_bits, 128);
        opv.validate_value().unwrap();
        validate_private_complete_relations(&params).unwrap();
        for field in 0..5 {
            let mut changed = params.clone();
            match field {
                0 => changed.palw_fork_choice_rule_e_v1 = None,
                1 => changed.palw_fork_choice_commitment_v1 = None,
                2 => changed.palw_dns_retirement.as_mut().unwrap().settlement.unique_mature_work = 0,
                3 => changed.dns_params.as_mut().unwrap().full_reward_split_daa_score = 2,
                _ => changed.blockrate.target_time_per_block += 1,
            }
            assert!(
                validate_private_complete_relations(&changed).is_err(),
                "private release exception cannot hide invalid dependency/value {field}"
            );
        }
        let mut config = Config::new(params);
        config.palw_drill_genesis_salt = Some(salt);
        let args = Args {
            testnet: true,
            testnet_suffix: 12,
            disable_dns_seeding: true,
            outbound_target: 0,
            disable_upnp: true,
            listen: Some("127.0.0.1:53001".parse().unwrap()),
            rpclisten: Some("127.0.0.1:53002".parse().unwrap()),
            appdir: Some(std::env::temp_dir().join("unopened-rfc12-complete-probe").display().to_string()),
            ..Args::default()
        };
        validate_private_probe(&args, &config).unwrap();
        let manifest = private_probe_manifest(&config);
        assert_eq!(manifest["canonicalWorkDaa"], 0);
        assert_eq!(manifest["completeOnlyPrivatePolicy"], true);
        assert_eq!(manifest["releaseApproval"], false);
        let mut changed = config.clone();
        changed.params.palw_panel_free_v1.as_mut().unwrap().min_effective_bits = 2;
        assert!(validate_private_probe(&args, &changed).unwrap_err().contains("exact salted factory"));
        let mut changed = config.clone();
        changed.params.palw_panel_free_v1.as_mut().unwrap().sampled_conformance_gates_reward = true;
        assert!(validate_private_probe(&args, &changed).is_err());
        let mut changed = config.clone();
        changed.params.palw_tir_v1.as_mut().unwrap().activation = kaspa_consensus_core::config::params::ForkActivation::new(2);
        assert!(validate_private_probe(&args, &changed).is_err(), "stale mirror/changed schedule is not an exception");
    }

    /// The private profile's probe manifest and validators recurse deeper than a libtest thread's 2 MiB in a debug build: the
    /// body runs on its own 64 MiB thread (what the node gives its own validation), its panic re-raised.
    #[test]
    fn private_late_retirement_moves_only_the_fence_and_keeps_the_daa1_profile() {
        let worker = std::thread::Builder::new()
            .name("private-late-retirement".into())
            .stack_size(64 << 20)
            .spawn(private_late_retirement_moves_only_the_fence_and_keeps_the_daa1_profile_body)
            .expect("a test thread");
        if let Err(panic) = worker.join() {
            std::panic::resume_unwind(panic);
        }
    }

    fn private_late_retirement_moves_only_the_fence_and_keeps_the_daa1_profile_body() {
        use kaspa_consensus_core::config::params::ForkActivation;
        let salt = PalwDrillSaltV1::from_bytes([0x16; 32]).unwrap();
        let original = private_paid_claim_params_v4(&salt).unwrap();
        assert_eq!(original.palw_dns_retirement.unwrap().activation, ForkActivation::new(1), "the DAA-1 profile is unchanged");
        let late = private_late_retirement_params_v1(original.clone(), 200).unwrap();
        let retirement = late.palw_dns_retirement.unwrap();
        assert_eq!(retirement.activation, ForkActivation::new(200));
        assert_eq!(retirement.settlement.id(), original.palw_dns_retirement.unwrap().settlement.id());
        assert_eq!(retirement.legacy_evidence_horizon_daa, original.palw_dns_retirement.unwrap().legacy_evidence_horizon_daa);
        assert_eq!(late.palw_fork_choice_rule_e_v1, Some(ForkActivation::new(1)), "Rule E stays at or below the fence");
        assert_eq!(late.palw_fork_choice_commitment_v1, Some(ForkActivation::new(1)));
        assert_eq!(late.genesis.hash, original.genesis.hash);
        assert_ne!(late.consensus_params_id(), original.consensus_params_id(), "the height is part of the identity");
        for refused in [0, 1, u64::MAX] {
            assert!(private_late_retirement_params_v1(original.clone(), refused).is_err(), "refused height {refused}");
        }
        assert!(private_late_retirement_params_v1(late.clone(), 300).is_err(), "only a DAA-1 profile moves, once");
        let mut config = Config::new(late.clone());
        config.palw_drill_genesis_salt = Some(salt);
        let expected = private_expected_retirement_v1(private_paid_claim_params_v4(&salt).unwrap(), &config).unwrap();
        assert_eq!(expected.consensus_params_id(), late.consensus_params_id());
        assert_eq!(private_probe_manifest(&config)["retirement"]["activationDaa"], 200);
        let complete = private_late_retirement_params_v1(private_complete_check_params_v1(&salt).unwrap(), 200).unwrap();
        let mut config = Config::new(complete);
        config.palw_drill_genesis_salt = Some(salt);
        let args = Args {
            testnet: true,
            testnet_suffix: 12,
            disable_dns_seeding: true,
            outbound_target: 0,
            disable_upnp: true,
            listen: Some("127.0.0.1:53011".parse().unwrap()),
            rpclisten: Some("127.0.0.1:53012".parse().unwrap()),
            appdir: Some(std::env::temp_dir().join("unopened-rfc12-late-retirement-probe").display().to_string()),
            ..Args::default()
        };
        validate_private_probe(&args, &config).unwrap();
        let mut moved = config.clone();
        moved.params.palw_panel_free_v1.as_mut().unwrap().min_effective_bits = 2;
        assert!(validate_private_probe(&args, &moved).is_err(), "only the retirement height may depart from the factory");
    }

    #[test]
    fn private_constructor_refuses_public_identity_skipped_work_and_uncontrolled_peers() {
        let salt = PalwDrillSaltV1::from_bytes([0x12; 32]).unwrap();
        let mut config = Config::new(palw_chain_params_v1(palw_drill_network_v1(), Some(&salt)).unwrap());
        config.palw_drill_genesis_salt = Some(salt);
        let args = Args {
            testnet: true,
            testnet_suffix: 12,
            disable_dns_seeding: true,
            outbound_target: 0,
            disable_upnp: true,
            listen: Some("127.0.0.1:53001".parse().unwrap()),
            rpclisten: Some("127.0.0.1:53002".parse().unwrap()),
            appdir: Some(std::env::temp_dir().join("unopened-rfc12-test-probe").display().to_string()),
            ..Args::default()
        };
        assert!(validate_private_probe(&args, &config).is_ok());
        config.params.skip_proof_of_work = true;
        assert!(validate_private_probe(&args, &config).unwrap_err().contains("skip_proof_of_work"));
        config.params.skip_proof_of_work = false;
        let mut uncontrolled = args.clone();
        uncontrolled.outbound_target = 1;
        assert!(validate_private_probe(&uncontrolled, &config).unwrap_err().contains("controlled"));
        let mut public_payout = args.clone();
        public_payout.palw_heartbeat_miner_address = Some(
            kaspa_addresses::Address::new(kaspa_addresses::Prefix::Testnet, kaspa_addresses::Version::PubKeyHashMlDsa87, &[0; 64])
                .to_string(),
        );
        assert!(validate_private_probe(&public_payout, &config).unwrap_err().contains("not an address of drill"));
        let mut public = config.clone();
        public.palw_drill_genesis_salt = None;
        assert!(validate_private_probe(&args, &public).unwrap_err().contains("salted"));
    }

    #[test]
    fn private_prepare_pins_empty_ownership_before_runtime_can_open_logs() {
        let root = tempfile::tempdir().unwrap();
        let owned = root.path().join("owned");
        let salt = PalwDrillSaltV1::from_bytes([0x16; 32]).unwrap();
        let mut config = Config::new(palw_chain_params_v1(palw_drill_network_v1(), Some(&salt)).unwrap());
        config.palw_drill_genesis_salt = Some(salt);
        let args = Args {
            testnet: true,
            testnet_suffix: 12,
            disable_dns_seeding: true,
            outbound_target: 0,
            disable_upnp: true,
            listen: Some("127.0.0.1:53001".parse().unwrap()),
            rpclisten: Some("127.0.0.1:53002".parse().unwrap()),
            appdir: Some(owned.display().to_string()),
            ..Args::default()
        };
        prepare_private_integration_probe(&args, &config).unwrap();
        let marker = owned.join(PRIVATE_PROBE_MARKER_V1);
        let before = std::fs::read(&marker).unwrap();
        assert_eq!(std::fs::read_dir(&owned).unwrap().count(), 1, "no logger/database path may precede the ownership pin");
        prepare_private_integration_probe(&args, &config).unwrap();
        assert_eq!(std::fs::read(&marker).unwrap(), before);
        let mut another = config.clone();
        another.params = private_native_settlement_params_v1(another.params).unwrap();
        assert!(
            prepare_private_integration_probe(&args, &another).is_err(),
            "a valid different private policy cannot reuse the owned DB"
        );
        assert_eq!(std::fs::read(&marker).unwrap(), before);
        let unrelated = root.path().join("unrelated");
        std::fs::create_dir(&unrelated).unwrap();
        std::fs::write(unrelated.join("existing"), b"another task").unwrap();
        let unrelated_args = Args { appdir: Some(unrelated.display().to_string()), ..args };
        assert!(prepare_private_integration_probe(&unrelated_args, &config).is_err());
        assert_eq!(std::fs::read_dir(&unrelated).unwrap().count(), 1);
        assert_eq!(std::fs::read(unrelated.join("existing")).unwrap(), b"another task");
    }

    #[test]
    fn private_population_constructor_refuses_replaced_factory_rows_or_size() {
        use kaspa_consensus_core::config::private_population::PrivatePopulationV1;
        let salt = PalwDrillSaltV1::from_bytes([0x14; 32]).unwrap();
        let profile = PrivatePopulationV1::new(17).unwrap();
        let derived = derive_private_population_v1(&salt, profile).unwrap();
        let mut config = Config::new(derived.params);
        config.private_population = Some(profile);
        config.palw_drill_genesis_salt = Some(salt);
        let args = Args {
            testnet: true,
            testnet_suffix: 12,
            disable_dns_seeding: true,
            outbound_target: 0,
            disable_upnp: true,
            listen: Some("127.0.0.1:53001".parse().unwrap()),
            rpclisten: Some("127.0.0.1:53002".parse().unwrap()),
            appdir: Some(std::env::temp_dir().join("unopened-rfc12-population-probe").display().to_string()),
            ..Args::default()
        };
        assert!(validate_private_probe(&args, &config).is_ok());
        #[cfg(feature = "devnet-prealloc")]
        {
            let mut preallocation = args.clone();
            preallocation.num_prealloc_utxos = Some(1);
            preallocation.prealloc_address =
                Some(palw_t12_drill_main_key_v1(&salt).address(kaspa_addresses::Prefix::Testnet).to_string());
            assert!(validate_private_probe(&preallocation, &config).is_err());
        }
        let mut wrong_size = config.clone();
        wrong_size.private_population = Some(PrivatePopulationV1::new(18).unwrap());
        assert!(validate_private_probe(&args, &wrong_size).unwrap_err().contains("exact salt/size/funded factory"));
        let mut changed = config.clone();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut changed.params.palw_consensus_mode
        else {
            unreachable!()
        };
        let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::BondRegistered { collateral, .. } =
            bundle.genesis_objects.last_mut().unwrap()
        else {
            unreachable!()
        };
        *collateral += 1;
        assert!(validate_private_probe(&args, &changed).unwrap_err().contains("exact salt/size/funded factory"));
        let manifest = private_probe_manifest(&config);
        assert_eq!(manifest["population"]["totalBonds"], 17);
        assert_ne!(
            manifest["genesis"],
            private_probe_manifest(&Config::new(palw_chain_params_v1(palw_drill_network_v1(), Some(&salt)).unwrap()))["genesis"]
        );
    }

    #[test]
    fn private_probe_db_restarts_only_under_its_exact_fingerprint() {
        let salt = PalwDrillSaltV1::from_bytes([0x13; 32]).unwrap();
        let mut config = Config::new(palw_chain_params_v1(palw_drill_network_v1(), Some(&salt)).unwrap());
        config.palw_drill_genesis_salt = Some(salt);
        let directory = kaspa_database::utils::get_kaspa_tempdir();
        let args = Args { appdir: Some(directory.path().display().to_string()), ..Args::default() };
        pin_owned_probe_directory(&args, &config).unwrap();
        fs::write(directory.path().join("owned-db-test-file"), b"retained").unwrap();
        pin_owned_probe_directory(&args, &config).unwrap();
        let mut different = config.clone();
        different.params.skip_proof_of_work = true;
        assert!(pin_owned_probe_directory(&args, &different).unwrap_err().contains("fingerprint"));
        assert_eq!(fs::read(directory.path().join("owned-db-test-file")).unwrap(), b"retained");
        let other = kaspa_database::utils::get_kaspa_tempdir();
        fs::write(other.path().join("existing-data"), b"keep").unwrap();
        let args = Args { appdir: Some(other.path().display().to_string()), ..Args::default() };
        assert!(pin_owned_probe_directory(&args, &config).unwrap_err().contains("empty owned"));
        assert_eq!(fs::read(other.path().join("existing-data")).unwrap(), b"keep");
        assert!(!other.path().join(PRIVATE_PROBE_MARKER_V1).exists());
    }
}
