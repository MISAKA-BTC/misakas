//! **A-2 uniformity through the real pipeline — the mixed-verdict pin over every new kind** (the A2U review, generalising X8R's P11
//! to every kind the live build cannot decode).
//!
//! testnet-12 declares the audit fence, so the live build (int-12, `rcore/int-12` @ `0b1c11b87`) TOLERATES a lifecycle payload it
//! cannot decode: the block stands, the carrier is skipped. Every kind added after it (`PALW_LIFECYCLE_NEW_KINDS_V1`) and every header
//! carriage form (`PalwHeaderFormFenceV1`) must, below its fence, be read exactly as the live build reads it — the same block verdict,
//! the same skip, no charge, no budget, no state write — or a mixed fleet splits before any fence.
//!
//! Node A runs testnet-12 with harness cards and every owning fence unarmed (as shipped), on two rulesets: as LAUNCHED, and with the
//! live release compressed below DAA 330 (every flag day the live chain has crossed, at heights a test reaches — the ruleset live
//! int-12 judges blocks under today). Beside an ordinary chain (heartbeats, REAL attempts) it carries, on funded 0x4b carriers:
//!
//! * **every new kind, generically**: each `PALW_LIFECYCLE_NEW_KINDS_V1` row's kind decoded from a zero-filled body (a kind a lane
//!   adds is in this sweep the moment its row exists), plus a hand-built well-formed, signed one of each;
//! * every may-ride refusal a new kind has: unsigned, an empty kernel encoding, an over-bound Panel proof, an envelope wrapping no
//!   registration — each, before the review, a block the live build accepts and this build refused;
//! * a tag-254 payload no build decodes (the reference) and an int-12 kind carrying a value the live build re-reads (a generative
//!   class whose profile byte is HFX's `Head = 6`);
//! * `ObjectChunk` groups whose assembled bytes are new kinds — three that complete in one chunk, one that completes in its second;
//! * **a mixed block**: a live-build kind that folds (a quorum's `ReceiptLicensed`, which licenses a real claim), a duplicate the
//!   walk refuses, and new kinds riding unjudged, all in one block.
//!
//! Every carrier is in its block, every block is valid, the licence licenses, and nothing else is folded: no kernel route state, no
//! Panel V3 state, no completed group. A `PFS4` receipt header is refused on the header path AND the pruning-proof path with the
//! live build's own refusal. Node B (unarmed) and node C (every owning fence armed far above the chain) replay every block: the same
//! statuses, refusals, sink, PALW state root and UTXO multiset at every block.
//!
//! `A2U_INT12_REPLAY_OUT=<dir>` also writes each ruleset's chain (`<dir>/a2u-<ruleset>.borsh`: the headers, transactions, refusals,
//! per-block roots and the view) for the replay through the live release itself (`a2u_int12_replay.rs.int12`,
//! `scripts/verify/a2u-int12-replay.sh`; `docs/design/palw/integration/a2-uniformity-new-kinds.md` §5).
use super::t12_round_lane_e2e::{
    T12Chain, card_payout_spk, sign_spend, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards,
};
use super::{OnetimeTxSelector, new_miner_data};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::virtual_state::VirtualStateStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::palw_lifecycle_objects_v2::{
    PALW_A2_KIND_FENCE_TABLE_V1, PALW_KERNEL_ROUTE_INNER_KINDS_V1, PALW_LIFECYCLE_NEW_KINDS_V1, PALW_LIFECYCLE_TX_VERSION_V2,
    PalwKernelInnerFenceV1, PalwLifecycleKindFenceV1, PalwLifecycleKindOwnerV1, PalwLifecycleTxPayloadV2,
    palw_int12_kind_refused_past_fence_v1, palw_lifecycle_kind_owner_v1, palw_lifecycle_object_may_ride_v2,
};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_tir_designated_answers_v1::{
    PalwTirDesignatedEventV1, PalwTirDesignatedOutcomeV1, PalwTirDesignatedRequestV1, PalwTirRequestKeyV1,
    palw_tir_designated_outcome_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBondKeyV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, palw_object_chunk_group_id_v1,
};
use kaspa_consensus_core::pow_layer0::{
    POW_ALGO_ID_PALW_RECEIPT_V3, PalwAttemptLaneV1, PalwHeaderFormFenceV1, PowLayer0Error, check_palw_commitment_shape_at,
};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use std::sync::Arc;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// Node C's owning fences: far above anything this chain reaches.
const FAR: u64 = 1_000_000;
/// What a carrier pays: enough to open a chunk group's slot wherever its rent is armed.
const FEE: u64 = 200_000_000;
/// Past this DAA the compressed release is the ruleset live testnet-12 runs today (the int-11 list at 40, its last ρ step at +285).
const RELEASE_PRESENT_DAA: u64 = 330;

/// The ruleset the chain runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ruleset {
    /// testnet-12 as launched (`palw_t12_launch_params_v1`): the ruleset of the chain's first 750 DAA.
    Launch,
    /// The live release compressed ([`arm_the_release`]): the ruleset live int-12 judges every new block under.
    Release,
}

impl Ruleset {
    fn name(self) -> &'static str {
        match self {
            Self::Launch => "launch",
            Self::Release => "release",
        }
    }
}

/// **The live testnet-12 release, compressed** — the launch ruleset with every flag day the live chain has crossed, in its order,
/// at heights a test reaches (as `g14_registration_e2e`'s `t12_release_compressed`): the DAA-750 list at 20, the second at 24, the
/// capacity list at 28, `palw_tir_v1` at 32, `palw_tir_fence2` at 36, the int-11 list at 40. Each fence through its own `set`. The
/// replay through int-12 arms the same lists with int-12's own entries.
fn arm_the_release(params: &mut Params) {
    use kaspa_consensus_core::config::params::{
        PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FENCE2_FENCES_V1,
        PALW_T12_TIR_FLAG_DAY_FENCES_V1, palw_t12_arm_int11_flag_day_at_v1,
    };
    for (list, at) in [
        (PALW_T12_POST_LAUNCH_FENCES_V1, 20),
        (PALW_T12_POST_LAUNCH_FENCES_V2, 24),
        (PALW_T12_POST_LAUNCH_FENCES_V3, 28),
        (PALW_T12_TIR_FLAG_DAY_FENCES_V1, 32),
        (PALW_T12_TIR_FENCE2_FENCES_V1, 36),
    ] {
        for fence in list {
            (fence.set)(params, Some(ForkActivation::new(at)));
        }
    }
    palw_t12_arm_int11_flag_day_at_v1(params, Some(40));
}

/// **Every owning fence at `at`** — the lifecycle kinds', the header forms' and the kernel-route inner kinds' — through exhaustive
/// matches over their `ALL` lists, so a fence a lane adds does not compile here until this harness arms it. The documented
/// validation bypass: `validate_palw_v2` refuses each of them at every real height.
/// G14 original paid TIR conviction (tags 170-171) at `at`: the conviction fence, and the staged TIR route, the staged court, R-core+
/// and the aggregate liability each armed at or below `at` (the objective-offence ledger through [`arm_objective_offence_at_or_below`]).
fn arm_tir_public_conviction_at_or_below(params: &mut Params, at: ForkActivation) {
    params.palw_tir_public_conviction_v1 = Some(at);
    for field in [&mut params.palw_staged_tir_route_v1, &mut params.palw_staged_public_court_v1, &mut params.palw_rcore_plus] {
        if field.is_none_or(|f| f == ForkActivation::never() || f.daa_score() > at.daa_score()) {
            *field = Some(at);
        }
    }
    arm_objective_offence_at_or_below(params, at);
    if params.palw_capacity_aggregate_liability.as_ref().is_none_or(|l| l.activation.daa_score() > at.daa_score()) {
        params.palw_capacity_aggregate_liability =
            Some(kaspa_consensus_core::palw_aggregate_liability_v1::PalwCapacityLiabilityV1::t12_at_v1(at));
    }
}

/// The objective-offence ledger and its attribution (ADR-0144 §9) at `at` unless the ruleset already arms them at or below it.
fn arm_objective_offence_at_or_below(params: &mut Params, at: ForkActivation) {
    let never = ForkActivation::never();
    for field in [&mut params.palw_objective_offence, &mut params.palw_offence_attribution] {
        if field.is_none_or(|f| f == never || f.daa_score() > at.daa_score()) {
            *field = Some(at);
        }
    }
}

#[allow(unreachable_patterns)]
fn arm_every_owning_fence(params: &mut Params, at: ForkActivation) {
    for fence in PalwLifecycleKindFenceV1::ALL {
        match fence {
            PalwLifecycleKindFenceV1::ExecPayloadV2 => {
                params.palw_exec_payload_v2 = Some(at);
                params.sync_palw_exec_payload_v2();
            }
            PalwLifecycleKindFenceV1::ProbabilisticConstraintsV1 => params.palw_probabilistic_constraints_v1 = Some(at),
            PalwLifecycleKindFenceV1::SignedRegistrationV1 => params.palw_signed_registration_v1 = Some(at),
            PalwLifecycleKindFenceV1::PermissionlessPanelV1 => {
                params.palw_permissionless_panel_v1 =
                    Some(kaspa_consensus_core::palw_permissionless_panel_v1::PalwPermissionlessPanelV1 {
                        activation: at,
                        policy: panel_policy(),
                    });
                params.sync_palw_permissionless_panel_v1();
            }
            PalwLifecycleKindFenceV1::ProviderCourtV1 => params.palw_provider_court_v1 = Some(at),
            PalwLifecycleKindFenceV1::MonetaryMaterialV1 => {
                use kaspa_consensus_core::palw_monetary_material_v1::*;
                params.palw_monetary_material_v1 = Some(PalwMonetaryMaterialFenceV1 {
                    activation: at,
                    policy: PalwMonetaryMaterialPolicyV1 {
                        version: 1,
                        max_contracts: 16,
                        max_contracts_per_provider: 8,
                        max_bytes_per_contract: 1_000_000,
                        max_bytes_per_provider: 8_000_000,
                        minimum_collateral_sompi: 1,
                        minimum_fee_sompi: 1,
                        window_daa: params.palw_bond_budget_v1.as_ref().map_or(280, |b| b.policy.window_daa),
                        response_window_daa: 64,
                        max_enrollment_daa: 100,
                        cost_report_root: Hash64::from_bytes([0xF4; 64]),
                    },
                });
                // `monetary_material_at_v1` also requires the root-exact payment/material schema (reporter V2 + material V3) in
                // force, else kinds 240-242 past their fence are refused as dormant instead of judged: arm them at or below `at`.
                params.palw_reporter_income_v2 = Some(at);
                params.palw_material_income_v3 = Some(at);
                params.sync_palw_bond_budget_v1();
            }
            // codex G14 (2026-10-11): class semantics declarations and the staged public court, each its own Some-only fence.
            PalwLifecycleKindFenceV1::ClassSemanticsV1 => params.palw_class_semantics_v1 = Some(at),
            // The staged public court and LG14-A's terminal evidence (tags 162-167) file objective offences: past their own fence
            // they also need the objective-offence ledger and its attribution (ADR-0144 §9), armed at or below `at` here.
            PalwLifecycleKindFenceV1::StagedPublicCourtV1 => {
                params.palw_staged_public_court_v1 = Some(at);
                arm_objective_offence_at_or_below(params, at);
            }
            // The TIR and greedy Float32 staged routes: their forms ride inside the staged court's kinds, so the court (and its
            // objective-offence ledger) is armed at or below `at` too.
            PalwLifecycleKindFenceV1::StagedTirRouteV1 => {
                params.palw_staged_tir_route_v1 = Some(at);
                if params.palw_staged_public_court_v1.is_none_or(|f| f.daa_score() > at.daa_score()) {
                    params.palw_staged_public_court_v1 = Some(at);
                }
                arm_objective_offence_at_or_below(params, at);
            }
            PalwLifecycleKindFenceV1::StagedFloatConstraintV1 => {
                params.palw_staged_float_constraint_v1 = Some(at);
                if params.palw_staged_public_court_v1.is_none_or(|f| f.daa_score() > at.daa_score()) {
                    params.palw_staged_public_court_v1 = Some(at);
                }
                arm_objective_offence_at_or_below(params, at);
            }
            PalwLifecycleKindFenceV1::LegacyHeldDaV2 => params.palw_legacy_held_da_v2 = Some(at),
            // Lane HFX: the `Head` variants inside tag 68 and a generative job; the fence mirrors onto the V2 bundle.
            PalwLifecycleKindFenceV1::TaskHeadsV1 => {
                params.palw_task_heads_v1 = Some(kaspa_consensus_core::palw_task_heads_v1::PalwTaskHeadsFenceV1::testnet12_v1(at));
                params.sync_palw_task_heads_v1();
            }
            // Lane LG14-A: the dispute reservation's fence and its bundle mirror.
            PalwLifecycleKindFenceV1::LegacyPublicFilerV1 => {
                params.palw_legacy_public_filer_v1 = Some(at);
                params.sync_palw_legacy_public_filer_v1();
                arm_objective_offence_at_or_below(params, at);
            }
            // Lane BUDGET: the allocation needs the budget at or below it; both with the unapproved probe policies, mirrored.
            PalwLifecycleKindFenceV1::ModelBondAllocationV1 => {
                // Caps per base unit of capital (a TEST policy, never binding here): this harness judges kinds, not budgets.
                let policy = kaspa_consensus_core::palw_bond_budget_v1::PalwBondBudgetPolicyV1 {
                    block_units_per_unit: kaspa_consensus_core::palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1,
                    reward_per_unit_sompi: 1_000,
                    final_weight_per_unit: 1_000_000,
                    max_open_claims_per_bond: u32::MAX,
                    window_daa: 1_000,
                    ..kaspa_consensus_core::palw_bond_budget_v1::PalwBondBudgetPolicyV1::unapproved_probe_v1()
                };
                params.palw_bond_budget_v1 =
                    Some(kaspa_consensus_core::palw_bond_budget_v1::PalwBondBudgetFenceV1 { activation: at, policy });
                params.palw_model_bond_allocation_v1 =
                    Some(kaspa_consensus_core::palw_bond_budget_v1::PalwModelBondAllocationFenceV1::at(at));
                params.sync_palw_bond_budget_v1();
            }
            // RFC-0008 v2 (X8R): the work-root declaration, mirrored onto the V2 bundle the fold reads.
            PalwLifecycleKindFenceV1::ExecPayloadV2 => {
                params.palw_exec_payload_v2 = Some(at);
                params.sync_palw_exec_payload_v2();
            }
            // RFC-0008 release recovery (tags 240-242's v2 rows): in force at the later of its own and the monetary material
            // fence (armed above through `MonetaryMaterialV1`), with its unapproved probe terms.
            PalwLifecycleKindFenceV1::MaterialReleaseRecoveryV2 => {
                params.palw_material_release_recovery_v2 = Some(
                    kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialReleaseRecoveryFenceV2::unapproved_probe_at(at),
                );
            }
            // RFC-0006 memory/routing court v2 (tags 243-244): in force at the later of its own and the shard court's fence.
            PalwLifecycleKindFenceV1::MemoryRoutingCourtV2 => {
                params.palw_memory_routing_court_v2 = Some(at);
                if params.palw_shard_court.is_none_or(|f| f == ForkActivation::never() || f.daa_score() > at.daa_score()) {
                    params.palw_shard_court = Some(at);
                }
            }
            // G14 original paid TIR conviction (tags 170-171): in force where the staged TIR route, the staged court, the
            // objective-offence ledger, R-core+ and the aggregate liability all are, each armed at or below `at` here.
            PalwLifecycleKindFenceV1::TirPublicConvictionV1 => arm_tir_public_conviction_at_or_below(params, at),
            // RFC-0002 02-A (tag 175): the TIR container binding, in force where the TIR lane is (`palw_tir_v1` is armed above).
            PalwLifecycleKindFenceV1::TirContainerIdentityV1 => params.palw_tir_container_identity_v1 = Some(at),
            PalwLifecycleKindFenceV1::TirDesignatedAnswersV1 => params.palw_tir_designated_answers_v1 = Some(at),
            // R-D2-READY (tag 230): D2's fence. Its mirror on the state params comes from `sync_palw_2026_10_12_ruling_fences_v1`.
            PalwLifecycleKindFenceV1::CourtModelScopeV1 => params.palw_court_model_scope_v1 = Some(at),
            // RFC-0024 (tags 201-204): the bounded door.
            PalwLifecycleKindFenceV1::TirBoundedDoorV1 => params.palw_tir_bounded_door_v1 = Some(at),
            // ADR-0206 (tag 233): validation still refuses arming (no node E2E yet); the arm keeps the harness exhaustive.
            PalwLifecycleKindFenceV1::PanelDaDisclosureV1 => params.palw_panel_da_disclosure_v1 = Some(at),
            // Selected-source bootstrap (aux 148). Public validation still refuses activation; this arm only
            // makes the kind fence exhaustive for the harness, which already expects that refusal.
            PalwLifecycleKindFenceV1::SelectedSourceBootstrapV1 => {
                let base = at.daa_score();
                let h = |n: u8| kaspa_consensus_core::Hash64::from_bytes([n; 64]);
                params.palw_selected_source_bootstrap_v1 =
                    Some(kaspa_consensus_core::palw_source_bootstrap_v1::PalwSelectedSourceBootstrapFenceV1 {
                        at,
                        policy: kaspa_consensus_core::palw_source_bootstrap_v1::PalwSelectedSourceBootstrapPolicyV1 {
                            version: 1,
                            chain_genesis: h(3),
                            network: h(4),
                            ruleset: h(5),
                            epoch: 1,
                            freeze_cutoff_daa: base,
                            anchor_cutoff_daa: base + 1,
                            starts_at_daa: base + 2,
                            ends_at_daa: base + 3,
                            stake_release_daa: base + 4,
                            selection: misaka_palw_challenge::bootstrap_epoch_v1::BootstrapPolicyV1 {
                                version: 1,
                                seats: 2,
                                quorum: 2,
                                maximum_population: 2,
                                maximum_draw_attempts: 2,
                                maximum_source_works: 2,
                            },
                            source_profile: h(6),
                            checker_scope: h(7),
                            maximum_claim_bytes: 100,
                            maximum_check_work: 100,
                            maximum_history_headers: 100,
                            security_review: h(8),
                            economic_review: h(9),
                        },
                    });
                params.sync_palw_selected_source_bootstrap_v1();
            }
        }
    }
    for form in PalwHeaderFormFenceV1::ALL {
        match form {
            PalwHeaderFormFenceV1::ExecPayloadV2 => {
                params.palw_exec_payload_v2 = Some(at);
                params.sync_palw_exec_payload_v2();
            }
            PalwHeaderFormFenceV1::ReceiptSpendV4 => params.palw_receipt_spend_v4 = Some(at),
            // The PAV3 attempt carriage shares `palw_monetary_material_v1` (armed above through `MonetaryMaterialV1`).
            PalwHeaderFormFenceV1::NativeMaterialV3 => {}
            // RFC-0002 §II.14: the PNR9 native receipt carriage's own (never armable) fence.
            PalwHeaderFormFenceV1::NativeReceiptV39 => {
                params.palw_native_mining_v39 =
                    Some(kaspa_consensus_core::palw_native_mining_v39::PalwNativeMiningFenceV39::unapproved_probe_at(at, [0; 64]))
            }
            // The PXE2 header form shares the lifecycle kind's fence (armed above, the same field).
            PalwHeaderFormFenceV1::ExecPayloadV2 => params.palw_exec_payload_v2 = Some(at),
        }
    }
    for fence in PalwKernelInnerFenceV1::ALL {
        match fence {
            PalwKernelInnerFenceV1::ClaimChallengeV2 => {
                params.palw_claim_challenge_v2 =
                    Some(kaspa_consensus_core::palw_claim_challenge_v2::PalwClaimChallengeFenceV2::unapproved_probe_at(at));
            }
            PalwKernelInnerFenceV1::PanelLifetimeV2 => {
                params.palw_panel_lifetime_v2 =
                    Some(kaspa_consensus_core::palw_panel_lifetime_v2::PalwPanelLifetimeFenceV2::unapproved_probe_at(at));
            }
            PalwKernelInnerFenceV1::PanelFreeV1 => {
                params.palw_panel_free_v1 =
                    Some(kaspa_consensus_core::palw_panel_free_v1::PalwPanelFreeFenceV1::interim_v1(at, Vec::new()));
            }
            PalwKernelInnerFenceV1::BeaconSourceFeeV1 => {
                params.palw_beacon_source_fee_v1 =
                    Some(kaspa_consensus_core::palw_beacon_source_fee_v1::PalwBeaconSourceFeeFenceV1::unapproved_probe_at(at))
            }
            PalwKernelInnerFenceV1::BeaconSourceOptInV1 => {
                params.palw_beacon_source_opt_in_v1 =
                    Some(kaspa_consensus_core::palw_beacon_source_opt_in_v1::PalwBeaconSourceFenceV1::unapproved_probe_at(at))
            }
            PalwKernelInnerFenceV1::TypedRootsV1 | PalwKernelInnerFenceV1::PanelFreeAndTypedRootsV1 => {
                params.palw_typed_roots_v1 = Some(at)
            }
            PalwKernelInnerFenceV1::ModelRefutationSealV1 => params.palw_model_refutation_seal_v1 = Some(at),
            PalwKernelInnerFenceV1::StageExecutionV14 => {
                params.palw_stage_execution_v14 =
                    Some(kaspa_consensus_core::palw_flat_stage_v14::PalwStageExecutionFenceV14::unapproved_probe_at(at, [0; 64]));
            }
            PalwKernelInnerFenceV1::FundedExecutionV11 => {
                params.palw_funded_execution_v11 = Some(
                    kaspa_consensus_core::palw_funded_execution_v11::PalwFundedExecutionFenceV11::unapproved_probe_at(at, [0; 64]),
                );
            }
            PalwKernelInnerFenceV1::VerifierPayV1 => params.palw_verifier_pay_v1 = Some(at),
            // RFC07/RFC08 signed economic contracts (inner 67–69): unapproved probe policies, mirrored below.
            PalwKernelInnerFenceV1::ModelVerificationGateV1 => {
                params.palw_model_verification_gate_v1 =
                    Some(kaspa_consensus_core::palw_econ_contracts_v1::PalwModelVerificationGateFenceV1::unapproved_probe_at(at))
            }
            PalwKernelInnerFenceV1::HorizonExtensionV1 => {
                params.palw_horizon_extension_v1 =
                    Some(kaspa_consensus_core::palw_econ_contracts_v1::PalwHorizonExtensionFenceV1::unapproved_probe_at(at))
            }
            PalwKernelInnerFenceV1::MinimumPriceV1 => {
                params.palw_minimum_price_v1 =
                    Some(kaspa_consensus_core::palw_econ_contracts_v1::PalwMinimumPriceFenceV1::unapproved_probe_at(at))
            }
            // N3 (inner 73, round 3): the unapproved probe policy (synthetic profile), mirrored below with the economic contracts.
            PalwKernelInnerFenceV1::BeaconBootstrapV1 => {
                params.palw_beacon_bootstrap_v1 =
                    Some(kaspa_consensus_core::palw_beacon_bootstrap_v1::PalwBeaconBootstrapFenceV1::unapproved_probe_at(at))
            }
            // RFC-0017 (inner 74/75): a bare height, mirrored below.
            PalwKernelInnerFenceV1::ModelEligibilityV1 => params.palw_model_eligibility_v1 = Some(at),
            // RFC-0016 V8 (inner 76): a bare height, mirrored below.
            PalwKernelInnerFenceV1::ElasticCapacityV1 => params.palw_elastic_final_capacity_v1 = Some(at),
        }
    }
    params.sync_palw_model_eligibility_v1();
    params.sync_palw_rfc0016_v1();
    params.sync_palw_claim_challenge_v2();
    params.sync_palw_econ_contracts_v1();
    params.sync_palw_permissionless_panel_v1();
    params.sync_palw_memory_routing_court_v2();
    // Every other fence a LANDED row of the central table names (formulas, state encodings, the live build's kinds judged anew):
    // a row that lands with a fence this harness does not arm fails here, so node C judges every block with it far above.
    for row in PALW_A2_KIND_FENCE_TABLE_V1.iter().filter(|row| row.landed) {
        match row.fence {
            "palw_probabilistic_constraints_v1"
            | "palw_signed_registration_v1"
            | "palw_permissionless_panel_v1"
            | "palw_provider_court_v1"
            | "palw_model_bond_allocation_v1"
            | "palw_exec_payload_v2"
            | "palw_receipt_spend_v4"
            | "palw_panel_free_v1"
            | "palw_typed_roots_v1"
            | "palw_legacy_held_da_v2"
            | "palw_task_heads_v1"
            | "palw_legacy_public_filer_v1"
            | "palw_beacon_source_opt_in_v1"
            | "palw_beacon_source_fee_v1"
            | "palw_claim_challenge_v2"
            | "palw_panel_lifetime_v2"
            | "palw_monetary_material_v1"
            | "palw_material_release_recovery_v2"
            | "palw_memory_routing_court_v2"
            | "palw_tir_public_conviction_v1"
            | "palw_native_mining_v39"
            | "palw_staged_tir_route_v1"
            | "palw_staged_float_constraint_v1"
            | "palw_exec_payload_v2"
            | "palw_model_refutation_seal_v1"
            | "palw_funded_execution_v11"
            | "palw_stage_execution_v14"
            | "palw_class_semantics_v1"
            | "palw_staged_public_court_v1"
            | "palw_model_verification_gate_v1"
            | "palw_horizon_extension_v1"
            | "palw_minimum_price_v1"
            | "palw_beacon_bootstrap_v1" => {} // armed above, through their enums
            // Armed at or below `at` with the monetary material kinds above; never moved later.
            "palw_reporter_income_v2" | "palw_material_income_v3" => {
                params.palw_reporter_income_v2.get_or_insert(at);
                params.palw_material_income_v3.get_or_insert(at);
                params.sync_palw_bond_budget_v1();
            }
            "palw_paid_consumer_lifecycle_v1" => {
                params.palw_paid_consumer_lifecycle_v1 =
                    Some(kaspa_consensus_core::palw_bond_budget_v1::PalwPaidConsumerLifecycleFenceV1 {
                        activation: at,
                        policy: kaspa_consensus_core::palw_bond_budget_v1::PalwPaidConsumerLifecyclePolicyV1 {
                            version: 1,
                            service_span_daa: 60,
                            dispute_tail_daa: 700,
                            material_tail_daa: 500,
                        },
                    });
                params.sync_palw_bond_budget_v1();
            }
            "palw_tir_shard_segment_v2" => params.palw_tir_shard_segment_v2 = Some(at),
            "palw_verifier_pay_v1" => params.palw_verifier_pay_v1 = Some(at),
            // Bare heights mirrored onto the V2 bundle the fold reads, armed as `fork_id_v1`'s probe arms them (landed after this
            // harness was last run: validation 2026-10-17). Far above the chain they judge nothing here.
            "palw_settlement_once_v1" => {
                params.palw_settlement_once_v1 = Some(at);
                params.sync_palw_settlement_once_v1();
            }
            "palw_paid_claim_trap_v1" => {
                params.palw_paid_claim_trap_v1 = Some(at);
                params.sync_palw_paid_claim_trap_v1();
            }
            "palw_class_registration_origin_v1" => {
                params.palw_class_registration_origin_v1 = Some(at);
                params.sync_palw_class_registration_origin_v1();
            }
            // The 2026-10-12 rulings' rules (register X3, N4, D2): bare heights and X3's unapproved probe policy, mirrored onto the
            // V2 bundle the fold reads. Each keys on a claim's acceptance DAA, so far above the chain they judge nothing here.
            "palw_court_model_scope_v1" | "palw_public_evidence_reward_v1" | "palw_claim_hard_deadline_v1" => {
                params.palw_court_model_scope_v1 = Some(at);
                params.palw_public_evidence_reward_v1 = Some(at);
                params.palw_claim_hard_deadline_v1 =
                    Some(kaspa_consensus_core::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1::unapproved_probe_at(at));
                params.sync_palw_2026_10_12_ruling_fences_v1();
            }
            // RFC-0006 R6-CELL round 4's coverage closure: a bare height, mirrored onto the V2 bundle the fold and the court read.
            // Its registration refusal keys on the block's DAA and its claim gate / court close on a claim's acceptance DAA, so far
            // above the chain it judges nothing here.
            "palw_cell_coverage_closure_v1" => {
                params.palw_cell_coverage_closure_v1 = Some(at);
                params.sync_palw_cell_coverage_closure_v1();
            }
            // RFC-0017's model eligibility: a bare height, mirrored onto the V2 bundle the fold and the gates read. Every gate keys on a
            // claim's acceptance DAA or a class's registration DAA (a class registered below the fence reads as grandfathered
            // `Eligible`), so far above the chain it judges nothing here; inner 74/75 ride the kernel route's A-2 gate.
            "palw_model_eligibility_v1" => {
                params.palw_model_eligibility_v1 = Some(at);
                params.sync_palw_model_eligibility_v1();
            }
            // ADR-0175 needs model lines at or below it (`validate_palw_model_immutable_v1`); artifact root ownership is t12's.
            // testnet-12 runs model lines from genesis: never move them (a later height would disarm the live registry below it).
            "palw_model_immutable_v1" => {
                if params.palw_model_lines_fence().is_none() {
                    params.palw_model_lines = Some(at);
                }
                params.palw_model_immutable_v1 = Some(at);
            }
            // ADR-0206 (tag 233): a bare height, mirrored onto the V2 bundle; keyed on a claim's acceptance, so far above the chain
            // it judges nothing here.
            "palw_panel_da_disclosure_v1" => {
                params.palw_panel_da_disclosure_v1 = Some(at);
                params.sync_palw_g14_gates_v1();
            }
            other => panic!("a landed A-2 row's fence {other} is not armed by this harness: arm it here ({row:?})"),
        }
    }
}

/// testnet-12 with harness cards on `ruleset`; `armed` sets every owning fence at that DAA.
fn config_with(ruleset: Ruleset, armed: Option<u64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (mut config, _, premine, floats) = t12_with_harness_cards();
    if ruleset == Ruleset::Release {
        arm_the_release(&mut config.params);
        config.params.validate_palw_v2().expect("the live release, compressed, is a runnable ruleset");
    }
    if let Some(at) = armed {
        arm_every_owning_fence(&mut config.params, ForkActivation::new(at));
        assert!(config.params.validate_palw_v2().is_err(), "the real validation refuses what this harness bypasses");
    }
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("testnet-12 is V2") };
    let bundle = bundle.clone();
    (config, bundle, premine, floats)
}

fn panel_policy() -> kaspa_consensus_core::palw_permissionless_panel_v1::PanelPolicyV1 {
    let challenge = kaspa_consensus_core::palw_panel_beacon_v1::challenge::policy::reference_policy_v1(1, 1, 10, 1, 1);
    kaspa_consensus_core::palw_permissionless_panel_v1::PanelPolicyV1 {
        seal_depth_blocks: 2,
        seal_wait_daa: 40,
        bond_maturity_daa: 1,
        beacon_period_daa: 40,
        beacon_wait_daa: 12,
        assignment_delay_daa: 1,
        receipt_window_daa: 12,
        seat_count: 5,
        outsider_seats: 0,
        max_retries: 1,
        min_collateral: 1,
        max_candidates: 64,
        max_pending: 64,
        max_pending_per_bond: 16,
        max_assignments_per_block: 8,
        max_admissions_per_block: 8,
        max_tracked_claims: 256,
        max_beacons_per_block: 2,
        max_beacon_proof_bytes: 4096,
        beacon_scheme: kaspa_consensus_core::palw_panel_beacon_v1::panel_beacon_scheme_of_v1(&challenge),
    }
}

/// Each card's spendable coin: its fee float, then the change of its last carrier.
struct Wallet {
    coins: Vec<(TransactionOutpoint, UtxoEntry)>,
}

impl Wallet {
    /// A funded 0x4b carrier of `payload` by card `card`, its change the card's next coin.
    fn carrier(&mut self, config: &Config, card: usize, payload: Vec<u8>) -> Transaction {
        let (outpoint, entry) = self.coins[card].clone();
        let change = entry.amount - FEE;
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(change, card_payout_spk(card))],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        sign_spend(&mut tx, entry, card, config.params.storage_mass_parameter);
        self.coins[card] = (TransactionOutpoint::new(tx.id(), 0), UtxoEntry::new(change, card_payout_spk(card), 0, false));
        tx
    }
}

fn payload_of(object: &Obj) -> Vec<u8> {
    borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() }).expect("serializes")
}

/// **The smallest value of `T` a near-zero body decodes to** (the core A2U tests' `minimal_decode`): a zero-filled body with each
/// byte borsh refuses stepped up until it decodes (an enum whose first variant is `= 1` refuses tag 0, and the refused byte is the
/// last one it read).
fn minimal_decode<T: borsh::BorshDeserialize>(prefix: &[u8]) -> Option<T> {
    let mut bytes = prefix.to_vec();
    bytes.extend_from_slice(&[0u8; 16_384]);
    for _ in 0..4_096 {
        let mut reader = &bytes[..];
        match T::deserialize(&mut reader) {
            Ok(value) => return Some(value),
            Err(_) => {
                let refused = (bytes.len() - reader.len()).checked_sub(1)?;
                if refused < prefix.len() || bytes[refused] == u8::MAX {
                    return None;
                }
                bytes[refused] += 1;
            }
        }
    }
    None
}

fn zeros<T: borsh::BorshDeserialize>() -> T {
    minimal_decode(&[]).expect("a near-zero encoding decodes")
}

/// **The kind with object tag `tag`, every field (nearly) zero** — generic over the enum, so a kind a lane adds to
/// `PALW_LIFECYCLE_NEW_KINDS_V1` is carried here without an edit (a kind no near-zero body decodes to needs a hand-built sample in
/// [`well_formed`], and the panic says so).
fn zero_filled_kind(tag: u8) -> Obj {
    let object: Obj = match tag {
        170 | 171 => tir_public_sample(tag),
        _ => minimal_decode(&[tag])
            .unwrap_or_else(|| panic!("kind {tag} decodes from no near-zero body; give it a hand-built sample instead")),
    };
    assert_eq!(borsh::to_vec(&object).unwrap()[0], tag);
    object
}

/// **Kinds 170/171 (G14 original paid TIR control and atomic Valid grant/Ready, `palw_tir_public_conviction_v1`)**: their bodies
/// fix lengths (ML-DSA-87 signatures, a bounded owned part) no near-zero body decodes to, so they take the hand-built shape core's
/// `zero_filled_kind` uses (`palw_tir_public_carriage_v1`'s `#[cfg(test)]` fixtures, which this crate cannot reach). Shape only:
/// nothing here is signed or admissible — below its fence the kind must ride undecided like every other new kind.
fn tir_public_sample(tag: u8) -> Obj {
    use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
    use kaspa_consensus_core::palw_tir_public_carriage_v1::{
        PalwTirPublicControlActionV1, PalwTirPublicControlTargetV1, PalwTirPublicControlV1, PalwTirPublicGrantReadyV1,
    };
    use kaspa_consensus_core::palw_tir_public_custody_v1::PalwTirPublicCustodyGrantV1;
    use kaspa_consensus_core::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1;
    let h = Hash64::from_u64_word;
    let bond = |n: u64| PalwBondKeyV2(TransactionOutpoint::new(h(n), 0));
    let sig = || vec![2u8; MLDSA87_SIGNATURE_LEN];
    match tag {
        170 => Obj::TirPublicControlV1 {
            control: Box::new(PalwTirPublicControlV1 {
                version: 1,
                claim: h(1),
                allocation_id: h(2),
                source_id: h(3),
                target: PalwTirPublicControlTargetV1::Acquisition { owner: bond(7), reserved_daa: 10 },
                expected_turn: 4,
                declared_rent_sompi: 0,
                signer: bond(8),
                action: PalwTirPublicControlActionV1::Acquisition(PalwTirPublicOwnedActionV1::Part {
                    demand_id: h(4),
                    answerer: bond(8),
                    offset: 16_384,
                    bytes: vec![1, 2, 3],
                }),
                signature: sig(),
            }),
        },
        171 => {
            let receipt = kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2 {
                claim: h(1),
                verdict: kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2::Valid,
                seat_bond: bond(8),
                signed_daa: 20,
                signature: sig(),
            };
            let grant = PalwTirPublicCustodyGrantV1 {
                version: 1,
                claim: receipt.claim,
                seat: receipt.seat_bond,
                signed_daa: receipt.signed_daa,
                pledge_id: h(20),
                valid_receipt_digest: PalwTirPublicCustodyGrantV1::receipt_digest_v1(&receipt),
                signature: sig(),
            };
            let descriptor = kaspa_consensus_core::palw_backend::PalwTirPublicPreparedDescriptorV1 {
                version: 1,
                source_id: h(3),
                class_id: h(10),
                artifact_root: h(11),
                execution_root: h(12),
                trace_root: h(13),
                step_root: h(14),
                step_leaf_count: 19,
                decode_count: 3,
                vocab: 8,
                material_until_daa: 1000,
                manifest_digest: h(15),
                retained_bytes: 1024,
            };
            Obj::TirPublicGrantReadyV1 {
                grant: Box::new(PalwTirPublicGrantReadyV1 {
                    version: 1,
                    claim: h(1),
                    allocation_id: h(2),
                    source_id: h(3),
                    signer: bond(8),
                    receipt,
                    grant,
                    descriptor,
                    signature: sig(),
                }),
            }
        }
        _ => unreachable!("only kinds 170 and 171 take this sample"),
    }
}

/// One well-formed, signed object of every kind the live build cannot decode that has a hand-built sample.
fn well_formed(bond: PalwBondKeyV2) -> Vec<Obj> {
    let h = Hash64::from_bytes([3; 64]);
    let registration = Obj::ClassRegistered {
        class_id: h,
        artifact_root: h,
        slash_value_per_pwu: 1,
        pwu_rule: kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(10),
        initial_target: 1,
        share_permille: 0,
        activation_daa: 0,
        admission: None,
    };
    vec![
        Obj::ArtifactBoundV1 { v2_class: h, kernel_param_root: h, signer: bond, signature: vec![1; 64] },
        Obj::ArtifactBindingChallengedV1 {
            v2_class: h,
            kernel_param_root: h,
            challenger: bond,
            proof: Box::new(zeros()),
            signature: vec![1; 64],
        },
        Obj::KernelBoundV1 { v2_class: h, kernel_class: h, challenge_policy_id: h, signer: bond, signature: vec![1; 64] },
        Obj::ConformanceCommittedV1 { commitment: Box::new(zeros()), signer: bond, signature: vec![1; 64] },
        Obj::SignedRegistrationV1 {
            registration: Box::new(registration),
            valid_from_daa: 0,
            valid_until_daa: 10_000,
            fork_digest: kaspa_consensus_core::Hash::from_bytes([4; 32]),
            signer: bond,
            signature: vec![1; 64],
        },
        Obj::ConformanceEvidenceV1 { v2_class: h, action: Box::new(zeros()), signer: bond, signature: vec![1; 64] },
        Obj::KernelRouteV1 { bytes: vec![5; 256], signer: bond, signature: vec![1; 64] },
        Obj::KernelConstraintReceiptV1 { receipt: Box::new(zeros()), signature: vec![1; 64] },
        Obj::PanelBeaconProofV3 {
            proof: Box::new(kaspa_consensus_core::palw_permissionless_panel_v1::BeaconProofV1 {
                epoch: 1,
                output: h,
                proof: vec![6; 32],
            }),
        },
    ]
}

/// Every form a new kind's may-ride arm refuses that fits a transaction: unsigned (each signed kind), an empty kernel encoding, an
/// over-bound Panel proof and an envelope wrapping no registration. (An over-bound kernel encoding is past what a block carries; it
/// reaches a node only assembled from chunks, which `ObjectChunk` groups below exercise.)
fn malformed(bond: PalwBondKeyV2) -> Vec<Obj> {
    let h = Hash64::from_bytes([3; 64]);
    let mut out: Vec<Obj> = well_formed(bond)
        .into_iter()
        .filter_map(|mut object| {
            let signature = match &mut object {
                Obj::ArtifactBoundV1 { signature, .. }
                | Obj::ArtifactBindingChallengedV1 { signature, .. }
                | Obj::KernelBoundV1 { signature, .. }
                | Obj::ConformanceCommittedV1 { signature, .. }
                | Obj::SignedRegistrationV1 { signature, .. }
                | Obj::ConformanceEvidenceV1 { signature, .. }
                | Obj::KernelRouteV1 { signature, .. }
                | Obj::KernelConstraintReceiptV1 { signature, .. } => signature,
                _ => return None,
            };
            signature.clear();
            Some(object)
        })
        .collect();
    out.push(Obj::KernelRouteV1 { bytes: Vec::new(), signer: bond, signature: vec![1; 64] });
    out.push(Obj::PanelBeaconProofV3 {
        proof: Box::new(kaspa_consensus_core::palw_permissionless_panel_v1::BeaconProofV1 {
            epoch: 1,
            output: h,
            proof: vec![6; kaspa_consensus_core::palw_permissionless_panel_v1::MAX_BEACON_PROOF_BYTES_V1 as usize + 1],
        }),
    });
    out.push(Obj::SignedRegistrationV1 {
        registration: Box::new(Obj::KernelBoundV1 {
            v2_class: h,
            kernel_class: h,
            challenge_policy_id: h,
            signer: bond,
            signature: vec![1],
        }),
        valid_from_daa: 0,
        valid_until_daa: 10_000,
        fork_digest: kaspa_consensus_core::Hash::from_bytes([4; 32]),
        signer: bond,
        signature: vec![1; 64],
    });
    for object in &out {
        assert!(palw_lifecycle_object_may_ride_v2(object).is_err(), "a may-ride refusal: {object:?}");
    }
    out
}

/// **A live-build kind carrying a value the live build re-reads**: a generative class registration (tag 68, armed on testnet-12)
/// whose hand-read profile byte is HFX's `Head = 6`. The live build decodes it and judges it with its own rules; below
/// `palw_task_heads_v1` every build must give that verdict (`PalwInt12WireChangeV1::CarriedReread`).
/// **A provider's answer (tag 152, lane DA16), signed** — the near-zero kind with a signature present, so its may-ride arm passes and
/// only the fence decides. Below `palw_provider_court_v1` a group of `ObjectChunk`s assembling to it is the live build's undecodable
/// bytes at the completing chunk: no gate judges it, the certification cap counts it as int-12 does, the fold refuses it in int-12's
/// words.
fn provider_answer() -> Obj {
    let mut answer = zero_filled_kind(152);
    let Obj::ProviderAnswerV1 { signature, .. } = &mut answer else { unreachable!("tag 152") };
    *signature = vec![1; 64];
    assert_eq!(palw_lifecycle_object_may_ride_v2(&answer), Ok(()));
    answer
}

/// **ADR-0175's refusals by name** (`Int12RefusedByName`): signed objects of the live build's kinds that mutate a registration —
/// a version published, promoted, withdrawn, a lineage head rolled back, a benefits declaration granting `EARLY_VERSION`. The live
/// build decodes and judges them (its isolation passes each: signed); below `palw_model_immutable_v1` every build judges them by
/// int-12's rule, past it they are recognized and refused, the block standing.
fn adr0175_objects() -> Vec<Obj> {
    let mut out = Vec::new();
    for tag in [27u8, 28, 29, 81, 37] {
        let mut object = zero_filled_kind(tag);
        match &mut object {
            Obj::ModelVersionPublished { signature, .. }
            | Obj::ModelVersionPromoted { signature, .. }
            | Obj::ModelVersionWithdrawn { signature, .. }
            | Obj::LineageHeadRolledBack { signature, .. } => *signature = vec![1; 64],
            Obj::ModelLineBenefitsDeclared { tiers, signature, .. } => {
                *signature = vec![1; 64];
                let mut tier: kaspa_consensus_core::palw_model_benefits_v1::PalwModelBenefitTierV1 = zeros();
                tier.grants = kaspa_consensus_core::palw_model_benefits_v1::grant::EARLY_VERSION;
                tiers.push(tier);
            }
            other => unreachable!("tag {tag}: {other:?}"),
        }
        assert_eq!(palw_lifecycle_object_may_ride_v2(&object), Ok(()), "the live build's isolation passes it: {object:?}");
        assert_eq!(palw_int12_kind_refused_past_fence_v1(&object), Some("palw_model_immutable_v1"), "{object:?}");
        out.push(object);
    }
    out
}

/// **Every kernel-route inner kind, carried inside tag 110** — each `PALW_KERNEL_ROUTE_INNER_KINDS_V1` row's kind from a near-zero
/// body, encoded as the route carries it (`version ‖ borsh`), so an inner kind a lane adds (G14-R4's 15, K2S's 16–18, G14R's 20) is
/// carried the moment its row exists. Below tag 110's fence every one is the live build's undecodable payload.
fn kernel_inner_kinds(bond: PalwBondKeyV2) -> Vec<(u8, Obj)> {
    PALW_KERNEL_ROUTE_INNER_KINDS_V1
        .iter()
        .map(|(inner, name, _)| {
            let object: misaka_palw_kernel::route::KernelRouteObjectV1 =
                minimal_decode(&[*inner]).unwrap_or_else(|| panic!("inner {inner} ({name}) decodes from a near-zero body"));
            (*inner, Obj::KernelRouteV1 { bytes: object.encode(), signer: bond, signature: vec![1; 64] })
        })
        .collect()
}

/// Harness card `card`'s ML-DSA-87 signature over `message` under the model registry's context (the processor's
/// `palw_model_check_bond_signature`).
fn card_signs(card: usize, message: Hash64) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(
        &TestConsensus::palw_v2_registry_keypair(card as u64).signing_key,
        message.as_byte_slice(),
        kaspa_consensus_core::palw_model_lines_v1::PALW_MODEL_LINE_MLDSA87_CONTEXT,
        [0x22u8; 32],
    )
    .expect("sign")
    .as_ref()
    .to_vec()
}

/// **A line founded on a genesis class, and its next version published as a preview, each signed by card 1** — the live build's
/// registry at work (testnet-12 runs it from genesis). Returns the founding, the publication (carried once the founding folded) and
/// the line's id, which ADR-0175 derives differently past its fence (`model_line_id_at_v1`).
fn adr0175_line(chain: &T12Chain, config: &Config, base: Hash64, immutable: bool) -> (Obj, Obj, Hash64) {
    use kaspa_consensus_core::palw_model_lines_v1::{
        model_line_id_at_v1, palw_model_line_founded_message_v1, palw_model_version_message_v1,
    };
    let (_, state) = chain.tip_state();
    let class_id = *state
        .classes_iter()
        .find(|(id, class)| **id != base && matches!(class.status, kaspa_consensus_core::palw_state_v2::PalwClassStatusV2::Active))
        .expect("an Active genesis class besides the floor")
        .0;
    let card = 1usize;
    let founder = chain.bonds[card];
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let name = b"a2u-adr-0175".to_vec();
    let (root1, root2) = (Hash64::from_bytes([0xa1; 64]), Hash64::from_bytes([0xa2; 64]));
    let founded = Obj::ModelLineFounded {
        class_id,
        root: root1,
        founder,
        name: name.clone(),
        signature: card_signs(card, palw_model_line_founded_message_v1(domain, &class_id, &name, &founder, &root1)),
    };
    let line_id = model_line_id_at_v1(&class_id, &root1, &founder, &name, immutable);
    let published = Obj::ModelVersionPublished {
        line_id,
        version: 2,
        root: root2,
        parent: Some(1),
        adopted_from: None,
        runtime_hash: None,
        dataset_commitment: None,
        training_config_hash: None,
        notes_hash: None,
        preview: true,
        signature: card_signs(
            card,
            palw_model_version_message_v1(domain, &line_id, 2, &root2, Some(1), None, None, None, None, None, true),
        ),
    };
    (founded, published, line_id)
}

fn reread_probes() -> Vec<Obj> {
    let mut class = zero_filled_kind(68);
    let Obj::ClassRegisteredGenV1 { admission, .. } = &mut class else { unreachable!("tag 68") };
    admission.class.profile = 6;
    vec![class]
}

/// **Forms appended inside int-12 kinds** (HFX, `PalwInt12WireChangeV1::CarriedAppended`): a tag-68 registration whose profile offers are
/// the appended `Head` variant (3), and a tag-87 tensor commitment whose job body is `Head` (2). The live build cannot decode either:
/// it tolerates the carrier and folds nothing; below `palw_task_heads_v1` every build must do the same, and a node with the fence armed
/// far above the chain agrees.
fn appended_probes() -> Vec<Obj> {
    use kaspa_consensus_core::palw_task_heads_v1::{
        PALW_HEAD_PROBLEM_SINGLE_LABEL_V1, PALW_HEAD_TASK_SEQUENCE_V1, PalwGenHeadBodyV1, PalwGenHeadOffersV1,
    };
    let mut class = zero_filled_kind(68);
    let Obj::ClassRegisteredGenV1 { admission, .. } = &mut class else { unreachable!("tag 68") };
    admission.class.profile = 6;
    admission.class.offers.profile = kaspa_consensus_core::palw_gen_class_v1::PalwGenProfileOffersV1::Head(PalwGenHeadOffersV1 {
        task: PALW_HEAD_TASK_SEQUENCE_V1,
        problem: PALW_HEAD_PROBLEM_SINGLE_LABEL_V1,
        labels: 2,
        label_map_root: kaspa_consensus_core::palw_task_heads_v1::palw_head_label_map_root_v1(&["a", "b"]),
        pair_separator: vec![],
        entailment_label: None,
        position_scalar: None,
    });
    let mut job = zero_filled_kind(87);
    let Obj::GenTensorCommitted { job: tensor_job, .. } = &mut job else { unreachable!("tag 87") };
    tensor_job.body = kaspa_consensus_core::palw_gen_job_v1::PalwGenBodyV1::Head(PalwGenHeadBodyV1 {
        input: kaspa_consensus_core::palw_gen_job_v1::PalwGenEmbeddingInputV1::Text {
            token_ids_hash: Hash64::from_bytes([1; 64]),
            tokens: 4,
        },
        task: PALW_HEAD_TASK_SEQUENCE_V1,
        position: 0,
        output: 3,
    });
    vec![class, job]
}

/// The chunks of a group carrying `inner`, cut in `parts` (each `ObjectChunk` names the group id of the whole).
fn chunks_of(inner: &Obj, parts: usize) -> Vec<Obj> {
    let whole = borsh::to_vec(inner).unwrap();
    let group = palw_object_chunk_group_id_v1(&whole);
    let size = whole.len().div_ceil(parts);
    whole
        .chunks(size)
        .enumerate()
        .map(|(index, bytes)| Obj::ObjectChunk { group, index: index as u8, count: parts as u8, bytes: bytes.to_vec() })
        .collect()
}

fn group_of(chunk: &Obj) -> Hash64 {
    let Obj::ObjectChunk { group, .. } = chunk else { unreachable!("a chunk") };
    *group
}

/// A node's view, as the comparison reads it: the sink, the PALW state root, the virtual UTXO multiset.
fn view(chain: &T12Chain) -> (BlockHash, Hash64, Hash64) {
    let (_, state) = chain.tip_state();
    let multiset = chain.vp().virtual_stores.read().state.get().unwrap().multiset.clone().finalize();
    (chain.sink(), state.state_root(), multiset)
}

fn chain_blocks(chain: &T12Chain) -> Vec<Block> {
    let vp = chain.vp();
    let genesis = chain.config.params.genesis.hash;
    let (mut hashes, mut at) = (Vec::new(), chain.sink());
    while at != genesis {
        hashes.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    hashes.reverse();
    hashes.into_iter().map(|h| chain.ctx.consensus.get_block(h).expect("the node holds its chain")).collect()
}

/// A receipt-lane (algo-7) block on `chain`'s sink whose carriage is `PFS4` — built, not inserted.
fn pfs4_block(chain: &mut T12Chain, config: &Config) -> MutableBlock {
    let mut template = chain
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    stamp_harness_time(&config.params, &mut template.block.header, chain.ctx.simulated_time + config.params.target_time_per_block());
    let mut block = template.block;
    block.header.pow_algo_id = POW_ALGO_ID_PALW_RECEIPT_V3;
    let mut commitment = kaspa_consensus_core::palw_receipt_v4::PALW_RECEIPT_V4_CARRIAGE_MAGIC.to_vec();
    commitment.extend_from_slice(&[0xA2; 96]);
    block.header.palw_commitment = commitment;
    block.header.finalize();
    block
}

/// An attempt-lane block on `chain`'s sink whose carriage is a malformed `PAV3` — built, not inserted. Algo 6 is the attempt id
/// the live build's gate decodes as PAV2 under every lane it can be asked with.
fn pav3_block(chain: &mut T12Chain, config: &Config) -> MutableBlock {
    let mut template = chain
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    stamp_harness_time(&config.params, &mut template.block.header, chain.ctx.simulated_time + config.params.target_time_per_block());
    let mut block = template.block;
    block.header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2;
    let mut commitment = kaspa_consensus_core::palw_attempt_material_v3::PALW_ATTEMPT_MATERIAL_WIRE_MAGIC_V3.to_vec();
    commitment.extend_from_slice(&[0xA3; 96]);
    block.header.palw_commitment = commitment;
    block.header.finalize();
    block
}

/// **The live build's refusals of a header carriage** — its shape gate (which has no arm for a form added after it) under every
/// binding and lane it can be asked with; each refuses.
fn live_build_shape_refusals(block: &MutableBlock) -> Vec<PowLayer0Error> {
    let mut out = Vec::new();
    for bound in [false, true] {
        for lane in [PalwAttemptLaneV1::Unfenced, PalwAttemptLaneV1::LegacyArm, PalwAttemptLaneV1::ExecutionArm] {
            out.push(
                check_palw_commitment_shape_at(block.header.pow_algo_id, &block.header.palw_commitment, bound, lane)
                    .expect_err("the live build's gate refuses a new-form carriage under every binding and lane"),
            );
        }
    }
    out
}

/// What [`replay`] compares at each block, and what the dump carries for the replay through int-12.
type Dump = (
    u8,
    BlockHash,
    Vec<(kaspa_consensus_core::header::Header, Vec<Transaction>)>,
    Vec<(kaspa_consensus_core::header::Header, Vec<Transaction>, String)>,
    String,
    Vec<(BlockHash, Hash64)>,
    (BlockHash, Hash64, Hash64),
);

/// The PALW state root each of `blocks` recorded (its delta's root, as the node stored it).
fn roots_along(chain: &T12Chain, blocks: &[Block]) -> Vec<(BlockHash, Hash64)> {
    let vp = chain.vp();
    let store = vp.palw_state_v2_store.read();
    blocks.iter().map(|b| (b.header.hash, store.state_root_of(b.header.hash).expect("a chain block's delta is stored"))).collect()
}

async fn replay(
    config: &Config,
    bundle: &PalwConsensusParamsV2,
    premine: &Premine,
    floats: &Premine,
    blocks: &[Block],
    refused: &[(Block, String)],
) -> T12Chain {
    let node = t12_genesis_chain(config, bundle, premine, floats);
    for block in blocks {
        let hash = block.header.hash;
        node.ctx
            .consensus
            .validate_and_insert_block(block.clone())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("block {hash}: {e}"));
        assert_eq!(node.ctx.consensus.block_status(hash), BlockStatus::StatusUTXOValid, "block {hash}");
    }
    for (block, why) in refused {
        let err =
            node.ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await.expect_err("refused on every node");
        assert_eq!(&err.to_string(), why, "the same refusal");
    }
    node
}

/// What a block's carriers may weigh, in bytes (payloads plus each carrier's ML-DSA-87 key and signature): the block's transient
/// storage mass is four times its bytes and its limit is 500,000, so 120 KB keeps every block under it.
const BLOCK_CARRIAGE_BYTES: usize = 120_000;
/// One carrier's own bytes beside its payload: the input's ML-DSA-87 signature and key, the output, the header fields.
const CARRIER_OVERHEAD_BYTES: usize = 7_600;

/// Carry `payloads` in order on heartbeats — at most eight a block (one carrier per card) and at most [`BLOCK_CARRIAGE_BYTES`] —
/// and demand each carrier is in its block. Returns how many blocks carried them.
async fn carry(a: &mut T12Chain, wallet: &mut Wallet, config: &Config, payloads: &[(String, Vec<u8>)]) -> usize {
    let ttpb = config.params.target_time_per_block();
    let mut batches: Vec<&[(String, Vec<u8>)]> = Vec::new();
    let (mut start, mut bytes) = (0usize, 0usize);
    for (i, (_, payload)) in payloads.iter().enumerate() {
        let weight = payload.len() + CARRIER_OVERHEAD_BYTES;
        if i > start && (i - start == 8 || bytes + weight > BLOCK_CARRIAGE_BYTES) {
            batches.push(&payloads[start..i]);
            (start, bytes) = (i, 0);
        }
        bytes += weight;
    }
    batches.push(&payloads[start..]);
    for batch in &batches {
        // Each carrier spends its card's coin; a carrier held back must hand the coin back (its change never exists).
        let coins_before = wallet.coins.clone();
        let built: Vec<Transaction> =
            batch.iter().enumerate().map(|(card, (_, payload))| wallet.carrier(config, card, payload.clone())).collect();
        // **Node-local template policy is not a block rule.** The H-1 gate (`palw_mempool_h1_carrier_refusal`, asked again at the
        // template) rehearses the fold on an object the gate covers — here the objective evidence a zero-filled new kind assembles
        // (tags 160–167) — and keeps the carrier out of THIS node's template. That never splits consensus: block validity does not
        // ask it, and a block another node mines with the carrier is judged by the fold alone, which rides a kind below its fence
        // unjudged. The harness keeps the template's behaviour: a refused carrier is asserted to be refused by that policy only
        // (validation 2026-10-17: the expectation that every carrier rides THIS template was stale), and the block carries the rest.
        let refusals = a.ctx.consensus.palw_h1_carrier_refusals_v1(&built.iter().cloned().map(Arc::new).collect::<Vec<_>>());
        let mut txs = Vec::new();
        let mut kept = Vec::new();
        for (card, ((tx, (what, payload)), refusal)) in built.into_iter().zip(batch.iter()).zip(refusals).enumerate() {
            match refusal {
                Some(why) => {
                    wallet.coins[card] = coins_before[card].clone();
                    let tag = payload.get(2).copied();
                    assert!(
                        tag.is_some_and(|tag| PALW_LIFECYCLE_NEW_KINDS_V1.iter().any(|(t, _, _)| *t == tag)),
                        "only a new kind below its fence may be held back by the node-local H-1 policy: {what}: {why}"
                    );
                    eprintln!("[a2u] {what}: kept out of this node's template by the node-local H-1 policy ({why}); not a block rule");
                }
                None => {
                    txs.push(tx);
                    kept.push(what);
                }
            }
        }
        let block = a.heartbeat(ttpb, txs.clone()).await;
        for (tx, what) in txs.iter().zip(kept) {
            assert!(block.transactions.iter().any(|t| t.id() == tx.id()), "carried, its block valid: {what}");
        }
    }
    a.heartbeat(ttpb, Vec::new()).await; // accepts the last batch
    batches.len()
}

/// **Every seat of the claim's panel signs `Valid`** and the node assembles the licence its panel service would submit (the
/// assembler picks the set the ruleset's door takes).
/// `None` where the ruleset licenses the claim otherwise (past RFC-0007's vertex fence a claim licenses by the vertices' tally, and the
/// node's assembler offers no receipt set).
fn licence_for(chain: &T12Chain, claim_id: Hash64) -> Option<Obj> {
    use kaspa_consensus_core::palw_panel_v2::{
        PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
    };
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        chain.config.params.net.to_string().as_bytes(),
        Some(chain.config.params.genesis.hash),
    );
    let (_, state) = chain.tip_state();
    let panel = state.panel(&claim_id).expect("a bound claim has a panel").clone();
    let signed_daa = chain.ctx.consensus.get_virtual_daa_score();
    let receipts: Vec<PalwSeatReceiptV2> = panel
        .seats
        .iter()
        .map(|seat| {
            let card = chain.bonds.iter().position(|b| *b == seat.bond).expect("every seat is a genesis card");
            let message = palw_receipt_message_v2(network_domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &TestConsensus::palw_v2_registry_keypair(card as u64).signing_key,
                message.as_byte_slice(),
                PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                [0x11u8; 32],
            )
            .expect("sign")
            .as_ref()
            .to_vec();
            PalwSeatReceiptV2 { claim: claim_id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: seat.bond, signed_daa, signature }
        })
        .collect();
    let object = chain.vp().palw_v2_receipt_quorum_assemble_impl(claim_id, &receipts)?;
    assert!(
        matches!(object, Obj::ReceiptLicensed { .. } | Obj::ReceiptLicensedV2 { .. } | Obj::OptimisticLicensed { .. }),
        "every seat's Valid licenses the claim: {object:?}"
    );
    Some(object)
}

/// **Below their fences, every kind and form the live build cannot decode is judged as it judges them — beside live-build kinds that
/// fold and are refused in the same blocks — and a node with every owning fence armed far above the chain agrees on every verdict,
/// refusal, root and UTXO multiset** (see the module doc). Returns the dump for the replay through int-12.
async fn the_mixed_verdict_chain(ruleset: Ruleset) -> Dump {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = config_with(ruleset, None);
    let (armed_far, armed_far_bundle, _, _) = config_with(ruleset, Some(FAR));
    // A node with every owning fence in force from DAA 1: it judges nothing here but the chunk reader and a pruning proof, to show the
    // reading turns at the fence.
    let (armed_now, armed_now_bundle, _, _) = config_with(ruleset, Some(1));
    let now = t12_genesis_chain(&armed_now, &armed_now_bundle, &premine, &floats);
    assert_eq!(config.params.palw_lifecycle_kind_fences_v1(), Default::default(), "testnet-12 arms no owning fence");
    assert_eq!(config.params.palw_header_form_fences_v1(), Default::default(), "nor the PFS4 form's");
    assert!(config.params.palw_audit_2026_09_11_fence().is_some(), "testnet-12 declares the audit fence: undecodable is tolerated");
    let mut a = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let ttpb = config.params.target_time_per_block();
    let mut wallet = Wallet { coins: floats.clone() };
    let tag = ruleset.name();

    // ---- the plain chain, up to the ruleset's present: heartbeats and a REAL attempt (its claim licensed below) ----
    let present = if ruleset == Ruleset::Release { RELEASE_PRESENT_DAA } else { 2 };
    while a.daa_of(a.sink()) < present {
        a.heartbeat(ttpb, Vec::new()).await;
    }
    let (_, claim_id) = a.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let plain_view = view(&a);

    // ---- the payloads: every new kind (zero-filled and hand-built), every malformed form, the reference, the re-read probe ----
    let bond = a.bonds[1];
    let mut payloads: Vec<(String, Vec<u8>)> = Vec::new();
    for (kind_tag, name, _) in PALW_LIFECYCLE_NEW_KINDS_V1 {
        payloads.push((format!("{name} (tag {kind_tag}), zero-filled"), payload_of(&zero_filled_kind(*kind_tag))));
    }
    for object in well_formed(bond) {
        payloads.push((format!("well-formed {object:?}").chars().take(60).collect(), payload_of(&object)));
    }
    for object in malformed(bond) {
        payloads.push((format!("malformed {object:?}").chars().take(60).collect(), payload_of(&object)));
    }
    let mut reference = borsh::to_vec(&PALW_LIFECYCLE_TX_VERSION_V2).unwrap();
    reference.extend_from_slice(&[254, 1, 2, 3]);
    payloads.push(("the reference: tag 254, which no build decodes".into(), reference.clone()));
    for probe in reread_probes() {
        payloads.push(("a live-build kind carrying a re-read value (tag 68, profile 6)".into(), payload_of(&probe)));
    }
    for probe in appended_probes() {
        payloads.push((
            format!("an appended Head form inside a live-build kind: {probe:?}").chars().take(70).collect(),
            payload_of(&probe),
        ));
    }
    for object in adr0175_objects() {
        payloads.push((format!("ADR-0175: {object:?}").chars().take(60).collect(), payload_of(&object)));
    }
    for (inner, object) in kernel_inner_kinds(bond) {
        payloads.push((format!("kernel-route inner kind {inner} inside tag 110"), payload_of(&object)));
    }
    let route = Obj::KernelRouteV1 { bytes: vec![5; 512], signer: bond, signature: vec![1; 64] };
    let one_chunk: Vec<Obj> = [route.clone(), well_formed(bond)[1].clone(), well_formed(bond)[5].clone(), provider_answer()]
        .iter()
        .map(|inner| chunks_of(inner, 1).remove(0))
        .collect();
    for chunk in &one_chunk {
        payloads.push(("a one-chunk group of a new kind".into(), payload_of(chunk)));
    }
    let two_chunks = chunks_of(&Obj::KernelRouteV1 { bytes: vec![7; 2048], signer: bond, signature: vec![1; 64] }, 2);
    // Another answer than the one-chunk group's (a group id is the hash of the whole bytes).
    let answer_chunks = chunks_of(
        &{
            let mut answer = provider_answer();
            let Obj::ProviderAnswerV1 { signature, .. } = &mut answer else { unreachable!("tag 152") };
            *signature = vec![2; 64];
            answer
        },
        2,
    );
    payloads.insert(0, ("a two-chunk group's opening part".into(), payload_of(&two_chunks[0])));
    payloads.insert(1, ("a chunked provider answer's opening part (tag 152)".into(), payload_of(&answer_chunks[0])));
    payloads.push(("a two-chunk group's completing part".into(), payload_of(&two_chunks[1])));
    payloads.push(("a chunked provider answer's completing part (tag 152)".into(), payload_of(&answer_chunks[1])));
    for (kind_tag, _, _) in PALW_LIFECYCLE_NEW_KINDS_V1 {
        assert!(
            payloads.iter().any(|(_, p)| p.get(2) == Some(kind_tag)),
            "kind {kind_tag} is carried (the payload's object tag follows its u16 version)"
        );
    }
    carry(&mut a, &mut wallet, &config, &payloads).await;

    // ---- ADR-0175's kinds folding under the live build's rule: a line founded (tag 26), then its next version published (27) ----
    let (founded, published, line_id) = adr0175_line(&a, &config, bundle.state.base_class_id(), false);
    carry(&mut a, &mut wallet, &config, &[("a line founded (tag 26)".into(), payload_of(&founded))]).await;
    carry(&mut a, &mut wallet, &config, &[("its next version published (tag 27)".into(), payload_of(&published))]).await;
    assert_eq!(
        a.tip_state().1.model_versions_of(&line_id).len(),
        2,
        "below palw_model_immutable_v1 the live build's registry applies the version (every node and int-12 replay this)"
    );

    // ---- the mixed block: an ATTEMPT block (its claim folds) carrying a licence that folds, a duplicate the walk refuses, and new
    //      kinds riding unjudged ----
    a.attempt_at_the_anchor_slot(claim_id, 7).await;
    let licence = licence_for(&a, claim_id);
    assert_eq!(
        licence.is_some(),
        ruleset == Ruleset::Launch,
        "the launch ruleset licenses by receipts, the release by the vertex tally"
    );
    let mut mixed: Vec<(String, Vec<u8>)> = Vec::new();
    if let Some(licence) = &licence {
        mixed.push(("the claim's licence (a live-build kind that folds)".into(), payload_of(licence)));
        mixed.push(("the same licence again (a live-build kind the walk refuses)".into(), payload_of(licence)));
    }
    mixed.extend::<Vec<(String, Vec<u8>)>>(vec![
        ("a kernel route object".into(), payload_of(&well_formed(bond)[6])),
        ("an unsigned onboarding object".into(), payload_of(&malformed(bond)[0])),
        ("a Panel V3 proof".into(), payload_of(&well_formed(bond)[8])),
        ("the reference".into(), reference),
        ("a zero-filled kernel receipt".into(), payload_of(&zero_filled_kind(111))),
        ("a signed provider answer (tag 152)".into(), payload_of(&provider_answer())),
    ]);
    let mixed_bytes: usize = mixed.iter().map(|(_, p)| p.len() + CARRIER_OVERHEAD_BYTES).sum();
    assert!(mixed.len() <= 8 && mixed_bytes <= BLOCK_CARRIAGE_BYTES, "the mixed carriers fit ONE block ({mixed_bytes} bytes)");
    let txs: Vec<Transaction> =
        mixed.iter().enumerate().map(|(card, (_, payload))| wallet.carrier(&config, card, payload.clone())).collect();
    let (attempt_block, mixed_claim) = a.attempt(3, ttpb, txs.clone(), &|_| true).await;
    for (tx, (what, _)) in txs.iter().zip(&mixed) {
        assert!(attempt_block.transactions.iter().any(|t| t.id() == tx.id()), "carried by the attempt block, valid: {what}");
    }
    a.heartbeat(ttpb, Vec::new()).await; // accepts the attempt block's carriers
    let (_, state) = a.tip_state();
    assert!(state.claim(&mixed_claim).is_some(), "the attempt block's own claim folded beside the carriers");
    if licence.is_some() {
        assert!(
            matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
            "the live-build kind in the mixed block folded: the claim is licensed ({:?})",
            state.claim(&claim_id).unwrap().phase
        );
    }

    // ---- nothing was folded, charged or opened past what the live build does ----
    assert!(state.kernel_route().is_none(), "no kernel route state: every 104–111 object and every inner kind skipped");
    assert!(state.panel_v3().is_none(), "no Panel V3 state: the 120 objects skipped");
    for chunk in &one_chunk {
        assert!(
            state.pending_chunk_group(&group_of(chunk)).is_none(),
            "a one-chunk group of a new kind completes nothing and opens nothing"
        );
    }
    // The two-chunk group: its opening part is an ordinary `ObjectChunk` the live build accepts too (it opens the group where the
    // slot's rent is paid); its completing part assembles bytes that build cannot decode, so it is refused and the group stays open.
    for opening in [&two_chunks[0], &answer_chunks[0]] {
        if let Some(pending) = state.pending_chunk_group(&group_of(opening)) {
            assert_eq!(pending.parts.keys().copied().collect::<Vec<_>>(), vec![0], "the completing part was refused, as undecodable");
        }
    }
    // The walk's chunk reader — the certification cap's input — answers "undecodable" below the fence and the kind past it.
    let daa = a.daa_of(a.sink());
    for chunk in &one_chunk {
        assert!(a.vp().palw_kernel_chunk_inner(&state, chunk, daa).is_none(), "below the fence: undecodable to the cap");
        let inner = now.vp().palw_kernel_chunk_inner(&state, chunk, daa).expect("past the fence: the kind");
        assert!(matches!(palw_lifecycle_kind_owner_v1(&inner), PalwLifecycleKindOwnerV1::Fence(_)));
    }

    // ---- the PFS4 header: the live build's refusal, on the header path and on the pruning-proof path ----
    let pfs4 = pfs4_block(&mut a, &config);
    let live = live_build_shape_refusals(&pfs4);
    let err = a.ctx.consensus.validate_and_insert_block(pfs4.clone().to_immutable()).virtual_state_task.await.expect_err("refused");
    let pfs4_refusal = err.to_string();
    assert!(
        live.iter().any(|e| RuleError::BadPalwCommitmentShape(e.to_string()).to_string() == pfs4_refusal),
        "the header path refuses a PFS4 header with the live build's gate's refusal: {pfs4_refusal} (the gate's: {live:?})"
    );
    let mut proof: Vec<Vec<Arc<kaspa_consensus_core::header::Header>>> = vec![Vec::new(); config.params.max_block_level as usize + 1];
    proof[0].push(Arc::new(pfs4.header.clone()));
    let proof_refusal =
        |chain: &T12Chain| chain.ctx.consensus.validate_pruning_proof_standalone(&proof).expect_err("refused").to_string();
    let a_proof = proof_refusal(&a);
    assert!(
        live.iter().any(|e| {
            kaspa_consensus_core::errors::pruning::PruningImportError::PruningProofBadPalwCommitment(
                pfs4.header.hash,
                0,
                e.to_string(),
            )
            .to_string()
                == a_proof
        }),
        "the pruning-proof path refuses it with the live build's gate's refusal: {a_proof}"
    );
    assert_ne!(proof_refusal(&now), a_proof, "past the fence the form is the V4 carriage, judged by its own decode");

    // ---- the PAV3 attempt header: the live build's refusal (a PAV2 decode, refused for its magic), on both paths ----
    let pav3 = pav3_block(&mut a, &config);
    let live = live_build_shape_refusals(&pav3);
    let err = a.ctx.consensus.validate_and_insert_block(pav3.clone().to_immutable()).virtual_state_task.await.expect_err("refused");
    let pav3_refusal = err.to_string();
    assert!(
        live.iter().any(|e| RuleError::BadPalwCommitmentShape(e.to_string()).to_string() == pav3_refusal),
        "the header path refuses a PAV3 header with the live build's gate's refusal: {pav3_refusal} (the gate's: {live:?})"
    );
    let mut proof: Vec<Vec<Arc<kaspa_consensus_core::header::Header>>> = vec![Vec::new(); config.params.max_block_level as usize + 1];
    proof[0].push(Arc::new(pav3.header.clone()));
    let proof_refusal =
        |chain: &T12Chain| chain.ctx.consensus.validate_pruning_proof_standalone(&proof).expect_err("refused").to_string();
    let a_proof = proof_refusal(&a);
    assert!(
        live.iter().any(|e| {
            kaspa_consensus_core::errors::pruning::PruningImportError::PruningProofBadPalwCommitment(
                pav3.header.hash,
                0,
                e.to_string(),
            )
            .to_string()
                == a_proof
        }),
        "the pruning-proof path refuses a PAV3 header with the live build's gate's refusal: {a_proof}"
    );
    assert_ne!(proof_refusal(&now), a_proof, "past the fence the form is the PAV3 carriage, judged by its own decode");
    // Both refused headers, each with its own refusal (validation 2026-10-17: the PAV3 section shadowed the PFS4 refusal, so the
    // replay compared the PFS4 block against the PAV3 message).
    let refused = vec![(pfs4.to_immutable(), pfs4_refusal), (pav3.to_immutable(), pav3_refusal)];

    // ---- every node agrees, block by block ----
    let blocks = chain_blocks(&a);
    let roots = roots_along(&a, &blocks);
    for (other, other_bundle, name) in [(&config, &bundle, "unarmed"), (&armed_far, &armed_far_bundle, "armed far above")] {
        let node = replay(other, other_bundle, &premine, &floats, &blocks, &refused).await;
        assert_eq!(roots_along(&node, &blocks), roots, "the {name} node: the PALW state root at every block");
        assert_eq!(view(&node), view(&a), "the {name} node: sink, PALW root, UTXO multiset");
        assert_eq!(proof_refusal(&node), a_proof, "the {name} node's pruning proof");
    }
    assert_ne!(plain_view.0, view(&a).0, "the carriers moved the chain");
    eprintln!(
        "[a2u {tag}] {} chain blocks to DAA {}, {} carriers, 2 refused headers; every node agrees",
        blocks.len(),
        a.daa_of(a.sink()),
        payloads.len() + mixed.len()
    );

    (
        match ruleset {
            Ruleset::Launch => 0,
            Ruleset::Release => 1,
        },
        config.params.genesis.hash,
        blocks.iter().map(|b| ((*b.header).clone(), (*b.transactions).clone())).collect(),
        refused.iter().map(|(b, why)| ((*b.header).clone(), (*b.transactions).clone(), why.clone())).collect(),
        a_proof,
        roots,
        view(&a),
    )
}

/// Write `dump` for the replay through int-12 when `A2U_INT12_REPLAY_OUT=<dir>` is set.
fn write_dump(ruleset: Ruleset, dump: &Dump) {
    if let Ok(dir) = std::env::var("A2U_INT12_REPLAY_OUT") {
        let path = std::path::Path::new(&dir).join(format!("a2u-{}.borsh", ruleset.name()));
        std::fs::write(&path, borsh::to_vec(dump).expect("serializes")).expect("writes the replay file");
        eprintln!("[a2u] wrote {} blocks and {} refusals to {}", dump.2.len(), dump.3.len(), path.display());
    }
}

#[tokio::test]
async fn t12_a2u_mixed_verdicts_every_new_kind_below_its_fence_launch_ruleset() {
    let dump = the_mixed_verdict_chain(Ruleset::Launch).await;
    write_dump(Ruleset::Launch, &dump);
}

#[tokio::test]
async fn t12_a2u_mixed_verdicts_every_new_kind_below_its_fence_release_ruleset() {
    let dump = the_mixed_verdict_chain(Ruleset::Release).await;
    write_dump(Ruleset::Release, &dump);
}

/// One chain for the ADR-0175 pair: testnet-12 (its model registry in force from genesis), `palw_model_immutable_v1` at DAA 1 when
/// `immutable` (its own validation passing). A line is founded (allowed on both: past the fence it is an independent registration),
/// then its next version is published — signed by the line's developer — beside the five refused kinds, in ONE block. Returns the
/// line's versions.
async fn adr0175_versions_after_publishing(immutable: bool) -> usize {
    let (mut config, _, premine, floats) = t12_with_harness_cards();
    assert!(config.params.palw_model_lines_fence().is_some_and(|f| f.is_active(1)), "testnet-12 runs the model registry");
    if immutable {
        config.params.palw_model_immutable_v1 = Some(ForkActivation::new(1));
        config.params.validate_palw_model_immutable_v1().expect("ADR-0175's prerequisites: model lines, artifact root ownership");
    }
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("testnet-12 is V2") };
    let bundle = bundle.clone();
    let mut a = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let ttpb = config.params.target_time_per_block();
    while a.daa_of(a.sink()) < 2 {
        a.heartbeat(ttpb, Vec::new()).await;
    }
    let mut wallet = Wallet { coins: floats.clone() };
    let (founded, published, line_id) = adr0175_line(&a, &config, bundle.state.base_class_id(), immutable);
    carry(&mut a, &mut wallet, &config, &[("a line founded".into(), payload_of(&founded))]).await;
    assert_eq!(a.tip_state().1.model_versions_of(&line_id).len(), 1, "the line is founded (immutable: {immutable})");
    let mut payloads: Vec<(String, Vec<u8>)> = vec![("its next version published".into(), payload_of(&published))];
    payloads.extend(adr0175_objects().iter().map(|o| (format!("ADR-0175: {o:?}").chars().take(60).collect(), payload_of(o))));
    let blocks = carry(&mut a, &mut wallet, &config, &payloads).await;
    assert_eq!(blocks, 1, "one block carries the publication and the five refused kinds");
    let versions = a.tip_state().1.model_versions_of(&line_id).len();
    eprintln!(
        "[a2u adr-0175] immutable {immutable}: {} definition updates in one valid block; the line holds {versions} version(s)",
        payloads.len()
    );
    versions
}

/// **ADR-0175 at its fence** (`Int12RefusedByName`, the ADR's "有効化と履歴"): the live build's rule applies a developer's signed
/// version; with `palw_model_immutable_v1` in force the same publication is recognized and NOT applied — and in both the block
/// carrying it (and the five other refused kinds) stands, each carrier in it. Below the fence the mixed-verdict chains above carry
/// the same kinds and every node, int-12 included, agrees.
#[tokio::test]
async fn t12_a2u_adr0175_past_its_fence_a_definition_update_is_refused_and_its_block_stands() {
    kaspa_core::log::try_init_logger("warn");
    assert_eq!(adr0175_versions_after_publishing(false).await, 2, "the live build's rule: the version is applied");
    assert_eq!(adr0175_versions_after_publishing(true).await, 1, "past palw_model_immutable_v1: refused, not applied");
}

/// ADR-0191 step B (tag 245): a designated-custodian request opened on `claim`/`root`, with two designated custodians and a one-DAA
/// window each. The custodian identities are opaque to the fold (it checks only membership of the answerer), so fixed 32-byte ids
/// stand in for them; the key is the real claim's id and the chain's sink hash.
fn designated_request(claim: Hash64, root: Hash64, opened_daa: u64) -> PalwTirDesignatedRequestV1 {
    let (mut claim_digest, mut root_digest) = ([0u8; 32], [0u8; 32]);
    claim_digest.copy_from_slice(&claim.as_bytes()[..32]);
    root_digest.copy_from_slice(&root.as_bytes()[..32]);
    PalwTirDesignatedRequestV1 {
        key: PalwTirRequestKeyV1 { claim: claim_digest, root: root_digest, coordinates: 7 },
        designated: vec![[0xC1; 32], [0xC2; 32]],
        obligation_daa: 1_000,
        opened_daa,
    }
}

/// **The tag-245 fold in a real block** (ADR-0191 step B). Armed at DAA 0 in a params copy (the conviction fence it needs armed with
/// it), a block carrying the opening of a designated request, then a block carrying its answer by the first designated custodian,
/// are both accepted; the tip's registry reads `Answered` for the request.
#[tokio::test]
async fn t12_a2u_tir_designated_event_folds_in_a_real_block_past_its_fence() {
    kaspa_core::log::try_init_logger("warn");
    let (mut config, _, premine, floats) = t12_with_harness_cards();
    arm_tir_public_conviction_at_or_below(&mut config.params, ForkActivation::new(0));
    config.params.palw_tir_designated_answers_v1 = Some(ForkActivation::new(0));
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("testnet-12 is V2") };
    let bundle = bundle.clone();
    let mut a = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let ttpb = config.params.target_time_per_block();
    while a.daa_of(a.sink()) < 2 {
        a.heartbeat(ttpb, Vec::new()).await;
    }
    let (_, claim_id) = a.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let mut wallet = Wallet { coins: floats.clone() };

    let request = designated_request(claim_id, a.sink(), 1);
    let key = request.key;
    let opened = Obj::TirDesignatedEventV1 { event: PalwTirDesignatedEventV1::Open(request.clone()) };
    carry(&mut a, &mut wallet, &config, &[("a designated request opened (tag 245)".into(), payload_of(&opened))]).await;
    let (_, state) = a.tip_state();
    assert_eq!(state.tir_designated_record(&key).map(|r| &r.request), Some(&request), "the opening is folded into the registry");

    let answered_by = request.designated[0];
    let body = [0xB7; 32];
    let answer_daa = a.daa_of(a.sink());
    let answer = Obj::TirDesignatedEventV1 {
        event: PalwTirDesignatedEventV1::Answer { key, custodian: answered_by, body, at_daa: answer_daa },
    };
    carry(&mut a, &mut wallet, &config, &[("the first designated custodian answers (tag 245)".into(), payload_of(&answer))]).await;

    let (_, state) = a.tip_state();
    let record = state.tir_designated_record(&key).expect("the request is in the registry at the tip");
    let tip_daa = a.daa_of(a.sink());
    assert_eq!(
        palw_tir_designated_outcome_v1(record, tip_daa).expect("the outcome is defined"),
        PalwTirDesignatedOutcomeV1::Answered { by: answered_by, body },
        "the tip's registry reads Answered for the event"
    );
    eprintln!("[a2u tir-designated] request opened at DAA 1, answered at DAA {answer_daa}; the tip (DAA {tip_daa}) reads Answered");
}

/// **Dormant (fence None, the t12 params)**: the same tag-245 carriers are valid, carried and skipped exactly as before the fold
/// existed (A-2): the blocks stand, and the registry stays empty.
#[tokio::test]
async fn t12_a2u_tir_designated_event_below_its_fence_is_skipped_and_writes_nothing() {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = config_with(Ruleset::Launch, None);
    assert!(config.params.palw_tir_designated_answers_v1_fence().is_none(), "the t12 params arm no designated-answer fence");
    let mut a = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let ttpb = config.params.target_time_per_block();
    while a.daa_of(a.sink()) < 2 {
        a.heartbeat(ttpb, Vec::new()).await;
    }
    let (_, claim_id) = a.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let mut wallet = Wallet { coins: floats.clone() };

    let request = designated_request(claim_id, a.sink(), 1);
    let key = request.key;
    let opened = Obj::TirDesignatedEventV1 { event: PalwTirDesignatedEventV1::Open(request.clone()) };
    let answer = Obj::TirDesignatedEventV1 {
        event: PalwTirDesignatedEventV1::Answer {
            key,
            custodian: request.designated[0],
            body: [0xB7; 32],
            at_daa: a.daa_of(a.sink()),
        },
    };
    let blocks = carry(
        &mut a,
        &mut wallet,
        &config,
        &[
            ("a designated request opened (tag 245), dormant".into(), payload_of(&opened)),
            ("a designated answer (tag 245), dormant".into(), payload_of(&answer)),
        ],
    )
    .await;
    assert!(blocks >= 1, "the dormant carriers were carried in blocks that stand");
    let (_, state) = a.tip_state();
    assert!(state.tir_designated_records().is_empty(), "below the fence the registry is never written");
    assert!(state.tir_designated_record(&key).is_none(), "and the request is not in it");
}
