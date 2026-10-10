//! RFC-0010: dormant configuration, public-chain snapshot/admission adapters and versioned fold.
//! Historical lane A remains unchanged. The production fold (`palw_panel_v3_fold_v1`, a child of `palw_state_v2`) wires the
//! engine into `PalwChainStateV2` — Some-only root block, deltas 170–173, carriage tail `0xED` — and hands every binding to the
//! V2 receipt/court machinery as an ordinary panel record. **No certified entropy source is approved**
//! ([`crate::palw_panel_beacon_v1`]), so this binary refuses EVERY attempted activation, including custom params and orphaned
//! state mirrors. OPV source plumbing is present but does not approve a beacon policy.

use crate::{
    Hash64,
    config::params::{ForkActivation, Params},
    palw_mode_v2::{PalwConsensusMode, PalwModeV2Error, palw_ruleset_id_v2},
    palw_panel_v2::{PalwPanelDrawPolicyV1, palw_panel_stake_base_bonds_judging_v1},
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2},
};
pub use misaka_palw_panel::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPermissionlessPanelV1 {
    pub activation: ForkActivation,
    pub policy: PanelPolicyV1,
}

/// **The fence, mirrored on the V2 bundle's state params** (`PalwStateParamsV2::panel_v3`), which the fold reads: its height, its
/// complete policy, and the engine's chain and ruleset identities. Not Borsh and not hashed — the fence itself is what the params
/// and schedule ids name (Some-only), and `validate_palw_permissionless_panel_v1` refuses an armed one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPanelV3ParamsV1 {
    pub from_daa: u64,
    pub policy: PanelPolicyV1,
    /// `palw_network_domain_v2_for(network, genesis)`: a chain, not a name.
    pub network: Hash64,
    /// `palw_ruleset_id_v2` of the bundle the mirror rides on.
    pub ruleset: Hash64,
}

/// Migration is decided by acceptance, never by a later anchor/retry block's height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelClaimRuleV1 {
    HistoricalLaneA,
    PermissionlessV3,
}

pub fn panel_claim_rule_v1(fence: Option<PalwPermissionlessPanelV1>, accepted_daa: u64) -> PanelClaimRuleV1 {
    if fence.is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(accepted_daa)) {
        PanelClaimRuleV1::PermissionlessV3
    } else {
        PanelClaimRuleV1::HistoricalLaneA
    }
}

impl crate::palw_state_v2::PalwStateParamsV2 {
    /// **Which anchoring rule governs a claim's Panel** — decided by the claim's ACCEPTANCE (`accepted_daa`), the V2 bundle's
    /// mirror of the fence read through [`panel_claim_rule_v1`]'s rule. `HistoricalLaneA` for every claim of every shipped preset.
    pub fn panel_claim_rule_v1(&self, accepted_daa: u64) -> PanelClaimRuleV1 {
        if self.panel_v3_rule_at(accepted_daa) { PanelClaimRuleV1::PermissionlessV3 } else { PanelClaimRuleV1::HistoricalLaneA }
    }
}

impl PalwPermissionlessPanelV1 {
    pub fn commitment_bytes(self) -> Vec<u8> {
        let mut bytes = WIRE_VERSION_V1.to_le_bytes().to_vec();
        bytes.extend(self.activation.daa_score().to_le_bytes());
        bytes.extend(borsh::to_vec(&self.policy).expect("canonical Panel policy"));
        bytes
    }
}

impl Params {
    /// **The fence's mirror** on the V2 bundle's state params. Dormant: `None` on every shipped preset; a fixture that bypasses
    /// `validate_palw_permissionless_panel_v1` (as `consensus/core/tests/rfc0010_permissionless_panel.rs` does) calls this.
    pub fn sync_palw_permissionless_panel_v1(&mut self) {
        let fence = self.palw_permissionless_panel_v1.filter(|fence| fence.activation != ForkActivation::never());
        let network = crate::palw_attempt_v2::palw_network_domain_v2_for(self.net.to_string().as_bytes(), Some(self.genesis.hash));
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            let ruleset = palw_ruleset_id_v2(bundle);
            let mirror = fence.map(|fence| PalwPanelV3ParamsV1 {
                from_daa: fence.activation.daa_score(),
                policy: fence.policy,
                network,
                ruleset,
            });
            bundle.state = bundle.state.clone().with_panel_v3(mirror);
        }
    }

    pub fn validate_palw_permissionless_panel_v1(&self) -> Result<(), PalwModeV2Error> {
        // The processor reads the bundle's mirror, while fingerprints and the fork schedule read
        // the top-level fence. A mirror without that fence would silently run an uncommitted rule.
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            let expected =
                self.palw_permissionless_panel_v1.filter(|fence| fence.activation != ForkActivation::never()).map(|fence| {
                    PalwPanelV3ParamsV1 {
                        from_daa: fence.activation.daa_score(),
                        policy: fence.policy,
                        network: crate::palw_attempt_v2::palw_network_domain_v2_for(
                            self.net.to_string().as_bytes(),
                            Some(self.genesis.hash),
                        ),
                        ruleset: palw_ruleset_id_v2(bundle),
                    }
                });
            if bundle.state.panel_v3().copied() != expected {
                return Err(PalwModeV2Error::Invalid("palw_permissionless_panel_v1 state mirror differs from its committed fence"));
            }
        }
        if let Some(rule) = self.palw_permissionless_panel_v1.filter(|r| r.activation != ForkActivation::never()) {
            rule.policy.validate().map_err(|_| {
                PalwModeV2Error::Invalid("palw_permissionless_panel_v1 has invalid timing, capacity or beacon parameters")
            })?;
            return Err(PalwModeV2Error::Invalid(
                "palw_permissionless_panel_v1 cannot be armed: RFC-0010 approves no certified Panel beacon (`approved_panel_beacon_policies_v1` is empty) and its bias/withholding/P0-10 review is external; R-core+, the panel economy and audit_2026_09_23 must also be in force before it, and RFC-0014's gates must pass first",
            ));
        }
        Ok(())
    }
}

impl From<PalwBondKeyV2> for BondIdV1 {
    fn from(b: PalwBondKeyV2) -> Self {
        Self { transaction: b.0.transaction_id, index: b.0.index }
    }
}

// A wrapper avoids orphan impls on the leaf-crate and core-crate external types.
pub fn panel_bond_key_v1(b: BondIdV1) -> PalwBondKeyV2 {
    PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: b.transaction, index: b.index })
}

fn commitment<T: borsh::BorshSerialize>(domain: &[u8], value: &T) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).to_state();
    h.update(&(domain.len() as u32).to_le_bytes());
    h.update(domain);
    h.update(&borsh::to_vec(value).expect("canonical public Panel facts"));
    Hash64::from_bytes(h.finalize().as_bytes().try_into().unwrap())
}

/// Construct from an already accepted V2 record, before its mutable receipt/court/spend phase.
/// No signature, timestamp or envelope encoding enters the immutable payload commitment.
/// Missing canonical work identity is refused (P0-10 cannot be bypassed by using execution roots).
pub fn panel_admitted_claim_v1(
    state: &PalwChainStateV2,
    claim_id: Hash64,
    required_exposure: u64,
) -> Result<AdmittedClaimV1, PanelErrorV1> {
    let claim = state.claim(&claim_id).ok_or(PanelErrorV1::InvalidSnapshot)?;
    let producer = state.bond(&claim.bond).ok_or(PanelErrorV1::InvalidSnapshot)?;
    if claim.phase.is_terminal() || required_exposure == 0 {
        return Err(PanelErrorV1::InvalidSnapshot);
    }
    let work_id = claim.work_id.filter(|id| *id != Hash64::default()).ok_or(PanelErrorV1::InvalidSnapshot)?;
    // Source type/quanta must be immutable; the free-prompt spend ledger is deliberately excluded.
    let source = match &claim.source {
        crate::palw_state_v2::PalwClaimSourceV2::Attempt => (0u8, 0u32),
        crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { quanta, .. } => (1, *quanta),
    };
    let immutable_fields = commitment(
        b"misaka-palw/panel-v3/claim-fields",
        &(
            source,
            claim.class_id,
            claim.bond,
            claim.pwu,
            claim.trace_root,
            claim.output_root,
            claim.execution_root,
            claim.trace_chunk_count,
            claim.trace_retention_daa,
            claim.reserved,
            claim.immature_contribution,
            claim.escrowed_reward,
            claim.work_leaves,
            claim.rights_reserved,
            claim.job_identity,
            producer.payout_payload,
            state.claim_root(&claim_id),
        ),
    );
    Ok(AdmittedClaimV1 {
        claim_id,
        work_id,
        class_id: claim.class_id,
        producer: claim.bond.into(),
        producer_operator: producer.operator_id,
        producer_key: commitment(b"misaka-palw/panel-v3/key", &producer.pubkey),
        immutable_fields,
        required_exposure,
    })
}

/// Freeze *structural* eligibility, not mutable load. Both class and outsider populations come
/// from public maturity, capability and readiness rules; operator_of_v1/genesis lists are absent.
/// The caller resolves the public draw policy AT THE PRE-ENTROPY CHECKPOINT, not at binding time.
/// Existing readiness rules remain conservative until new encoded profiles have their own fence.
#[allow(clippy::too_many_arguments)]
pub fn panel_snapshot_candidates_v1(
    state: &PalwChainStateV2,
    claim_id: Hash64,
    floor_class: Hash64,
    checkpoint_daa: u64,
    policy: PanelPolicyV1,
    draw: PalwPanelDrawPolicyV1,
    capability_proof: bool,
) -> Result<Vec<SeatCandidateV1>, PanelErrorV1> {
    policy.validate()?;
    let claim = state.claim(&claim_id).ok_or(PanelErrorV1::InvalidSnapshot)?;
    let registered_by = checkpoint_daa.checked_sub(policy.bond_maturity_daa);
    if registered_by.is_none() {
        return Ok(Vec::new());
    }
    let mut seats = std::collections::BTreeMap::<BondIdV1, SeatCandidateV1>::new();
    for (class, role) in [(claim.class_id, CLASS_ROLE_V1), (floor_class, OUTSIDER_ROLE_V1)] {
        if role == OUTSIDER_ROLE_V1 && policy.outsider_seats == 0 {
            continue;
        }
        let eligible = palw_panel_stake_base_bonds_judging_v1(
            state,
            &claim_id,
            &class,
            policy.min_collateral,
            registered_by,
            capability_proof,
            draw.readiness,
            draw.economy,
            policy.seat_count,
        )
        .map_err(|_| PanelErrorV1::InvalidSnapshot)?;
        for (bond, row) in eligible {
            // The authenticated state's root commits readiness/capability records as well as the
            // checkpoint. A later local artifact, top-up or declaration cannot mutate this value.
            let entry = seats.entry((*bond).into()).or_insert_with(|| SeatCandidateV1 {
                bond: (*bond).into(),
                operator: row.operator_id,
                key: commitment(b"misaka-palw/panel-v3/key", &row.pubkey),
                collateral: row.collateral,
                registered_daa: row.registered_daa,
                capability_root: commitment(b"misaka-palw/panel-v3/capability", &row.capable_classes),
                readiness_root: commitment(b"misaka-palw/panel-v3/readiness", &(state.state_root(), class)),
                roles: 0,
            });
            entry.roles |= role;
        }
    }
    if seats.len() > policy.max_candidates as usize {
        return Err(PanelErrorV1::ResourceLimit);
    }
    Ok(seats.into_values().collect())
}

/// **The population of a claim drawn per shard** (RFC-0006 × RFC-0010, agent SHARD): the stratified twin of
/// [`panel_snapshot_candidates_v1`], under the same structural rules at the same checkpoint. Stratum `s` (shard `s` of the class's
/// plan of `strata.count` shards) is the bonds that may judge the shard's readiness class
/// ([`crate::palw_tir_shard_v1::palw_tir_shard_ready_class_v1`] — the population lane A's per-shard draw reads): each carries
/// the CLASS role and bit `s`. For an outsider-judged claim, the base class's population carries the OUTSIDER role (no bit). A
/// bond in several populations is one candidate with every role and bit it earned. In bond order, each with its bits.
#[allow(clippy::too_many_arguments)]
pub fn panel_stratified_candidates_v1(
    state: &PalwChainStateV2,
    claim_id: Hash64,
    floor_class: Hash64,
    checkpoint_daa: u64,
    policy: PanelPolicyV1,
    draw: PalwPanelDrawPolicyV1,
    capability_proof: bool,
    strata: &PanelStrataV1,
) -> Result<Vec<(SeatCandidateV1, u64)>, PanelErrorV1> {
    policy.validate()?;
    strata.validate()?;
    let claim = state.claim(&claim_id).ok_or(PanelErrorV1::InvalidSnapshot)?;
    let Some(registered_by) = checkpoint_daa.checked_sub(policy.bond_maturity_daa) else { return Ok(Vec::new()) };
    let mut populations: Vec<(Hash64, u8, u64)> = (0..strata.count)
        .map(|stratum| {
            (
                crate::palw_tir_shard_v1::palw_tir_shard_ready_class_v1(&claim.class_id, strata.count, stratum),
                CLASS_ROLE_V1,
                1u64 << stratum,
            )
        })
        .collect();
    if strata.outsider {
        populations.push((floor_class, OUTSIDER_ROLE_V1, 0));
    }
    let readiness_root = commitment(b"misaka-palw/panel-v3/readiness", &(state.state_root(), claim.class_id));
    let mut seats = std::collections::BTreeMap::<BondIdV1, (SeatCandidateV1, u64)>::new();
    for (class, role, bit) in populations {
        let eligible = palw_panel_stake_base_bonds_judging_v1(
            state,
            &claim_id,
            &class,
            policy.min_collateral,
            Some(registered_by),
            capability_proof,
            draw.readiness,
            draw.economy,
            strata.stride(),
        )
        .map_err(|_| PanelErrorV1::InvalidSnapshot)?;
        for (bond, row) in eligible {
            let entry = seats.entry((*bond).into()).or_insert_with(|| {
                (
                    SeatCandidateV1 {
                        bond: (*bond).into(),
                        operator: row.operator_id,
                        key: commitment(b"misaka-palw/panel-v3/key", &row.pubkey),
                        collateral: row.collateral,
                        registered_daa: row.registered_daa,
                        capability_root: commitment(b"misaka-palw/panel-v3/capability", &row.capable_classes),
                        readiness_root,
                        roles: 0,
                    },
                    0,
                )
            });
            entry.0.roles |= role;
            entry.1 |= bit;
        }
    }
    if seats.len() > policy.max_candidates as usize {
        return Err(PanelErrorV1::ResourceLimit);
    }
    Ok(seats.into_values().collect())
}

/// Headroom from the existing one-ledger filter, excluding this engine's own reservations.
/// Callers must pass the same pre-object state for every due claim in the fold.
pub fn panel_live_headroom_v1(state: &PalwChainStateV2, bond: BondIdV1, filter: &crate::palw_panel_v2::PalwPanelValidLockV1) -> u128 {
    let key = panel_bond_key_v1(bond);
    if !state.bond(&key).is_some_and(|b| matches!(b.status, crate::palw_state_v2::PalwBondStatusV2::Active)) {
        return 0;
    }
    filter.room_v1(state, &key).unwrap_or(0)
}

/// Public-chain adapter for offline/shadow folds (the production fold reads its own view over the transition builder).
pub struct PalwPanelChainViewV1<'a> {
    pub state: &'a PalwChainStateV2,
    pub floor_class: Hash64,
    pub checkpoint_daa: u64,
    pub policy: PanelPolicyV1,
    pub draw: PalwPanelDrawPolicyV1,
    pub capability_proof: bool,
    pub lock_filter: crate::palw_panel_v2::PalwPanelValidLockV1,
}

impl ConsensusViewV1 for PalwPanelChainViewV1<'_> {
    fn candidates(&self, claim: &AdmittedClaimV1) -> Result<Vec<SeatCandidateV1>, PanelErrorV1> {
        panel_snapshot_candidates_v1(
            self.state,
            claim.claim_id,
            self.floor_class,
            self.checkpoint_daa,
            self.policy,
            self.draw,
            self.capability_proof,
        )
    }

    fn available_collateral(&self, bond: &BondIdV1) -> u128 {
        panel_live_headroom_v1(self.state, *bond, &self.lock_filter)
    }

    /// The production adapter ([`crate::palw_panel_beacon_v1`]): a proof is a borsh `WorkBeaconV1` verified against THIS chain's
    /// settlements. The shipped scheme registry is empty, so it refuses everything. This shadow view carries no OPV extras.
    fn verify_beacon(&self, request: &BeaconRequestV1, proof: &BeaconProofV1) -> Result<(), PanelErrorV1> {
        let history = crate::palw_state_v2::ChainPanelBeaconHistoryV1::new(
            self.state,
            self.floor_class,
            None,
            &crate::palw_state_v2::PalwPanelV3BeaconSourceV1::Chain,
            self.state.last_point().map(|point| point.daa_score).unwrap_or(0),
        );
        crate::palw_panel_beacon_v1::verify_panel_beacon_for_engine_v1(
            &crate::palw_panel_beacon_v1::approved_panel_beacon_policies_v1(),
            &history,
            request,
            proof,
        )
    }

    fn terminal_claim(&self, claim: &Hash64) -> bool {
        self.state.claim(claim).is_some_and(|r| r.phase.is_terminal())
    }
}

/// An explicit carrier, if later adopted, must contain the complete automatically derived facts.
/// A producer cannot choose claims, seats, order, a fresh seed or a different exposure amount.
pub fn validate_panel_binding_facts_v3(
    derived: &PanelFoldEventsV1,
    carried: &std::collections::BTreeMap<Hash64, PanelBoundV3>,
) -> Result<(), PanelErrorV1> {
    if &derived.bindings == carried { Ok(()) } else { Err(PanelErrorV1::InvalidSnapshot) }
}

// ---------------------------------------------------------------------------------------------
// Observation (RFC-0010, versioned): what an operator, an explorer or a public verifier reads of a V3 claim
// ---------------------------------------------------------------------------------------------

/// The version of every observation record below. Fields are only ever appended; a reader of version 1 ignores what it does not know.
pub const PANEL_V3_OBSERVATION_VERSION_V1: u16 = 1;

/// **One claim's permissionless-Panel status**: the rule that governs it, its V2 phase, and — for a claim the engine tracks — the
/// seal, the frozen snapshot, the epoch's certified-output state, the assignment (seed, seats, exposure, witness block), the retries
/// and the terminal reason. JSON (camelCase, `u128` as decimal strings); read-only, no verdict reads it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelV3ClaimStatusV1 {
    pub version: u16,
    pub claim_id: Hash64,
    /// `permissionlessV3` or `historicalLaneA`, decided by the claim's acceptance.
    pub rule: &'static str,
    pub accepted_daa: u64,
    pub v2_phase: String,
    /// The engine's phase for a tracked claim: `pendingSeal`, `sealed`, `entropyReady`, `bound`, `released`, `voided`.
    pub engine_phase: Option<&'static str>,
    pub acceptance_order: Option<u64>,
    pub seal: Option<ClaimSealV1>,
    pub snapshot: Option<PanelV3SnapshotSummaryV1>,
    pub beacon: Option<PanelV3BeaconStatusV1>,
    pub assignment: Option<PanelV3AssignmentV1>,
    /// Redraws made so far (`bindings − 1`).
    pub retries: u16,
    pub terminal: Option<PanelV3TerminalV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelV3SnapshotSummaryV1 {
    pub root: Hash64,
    pub checkpoint: Hash64,
    pub checkpoint_daa: u64,
    pub candidates: u32,
    pub policy_id: Hash64,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelV3BeaconStatusV1 {
    pub epoch: u64,
    pub release_daa: u64,
    pub deadline_daa: u64,
    pub scheme: Hash64,
    /// `collecting` (the window is open, nothing retained), `certified` (the output is retained) or `unavailable` (the window closed
    /// with nothing retained: `BEACON_UNAVAILABLE`, non-fraud).
    pub state: &'static str,
    pub output: Option<Hash64>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelV3AssignmentV1 {
    pub retry_index: u16,
    pub seed: Hash64,
    pub beacon_id: Hash64,
    pub seats: Vec<BondIdV1>,
    pub assignment_point: u64,
    pub bound_daa: u64,
    /// The inclusion witness only; no derivation reads it.
    pub binding_block: Hash64,
    /// Per seat, decimal.
    pub exposure: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelV3TerminalV1 {
    /// `SEAL_UNAVAILABLE`, `BEACON_UNAVAILABLE`, `NO_CAPABLE_PANEL`, `PANEL_UNAVAILABLE` (all non-fraud) or `RELEASED` (the claim left
    /// its Panel's receipts: licensed, final, voided by V2, or retired).
    pub reason: &'static str,
    pub daa: u64,
    pub fraud: bool,
}

/// The engine's phase as an observation name.
pub fn panel_v3_phase_name_v1(phase: &ClaimPhaseV3) -> &'static str {
    match phase {
        ClaimPhaseV3::PendingSeal => "pendingSeal",
        ClaimPhaseV3::Sealed => "sealed",
        ClaimPhaseV3::EntropyReady { .. } => "entropyReady",
        ClaimPhaseV3::Bound(_) => "bound",
        ClaimPhaseV3::Released { .. } => "released",
        ClaimPhaseV3::Voided { .. } => "voided",
    }
}

/// The contract's name for a non-fraud ending (`misaka-palw-challenge`'s lifecycle uses the same spelling).
pub fn panel_v3_reason_code_v1(reason: NonFraudReasonV1) -> &'static str {
    match reason {
        NonFraudReasonV1::SealUnavailable => "SEAL_UNAVAILABLE",
        NonFraudReasonV1::BeaconUnavailable => "BEACON_UNAVAILABLE",
        NonFraudReasonV1::NoCapablePanel => "NO_CAPABLE_PANEL",
        NonFraudReasonV1::PanelUnavailable => "PANEL_UNAVAILABLE",
    }
}

fn v2_phase_name(phase: &crate::palw_state_v2::PalwClaimPhaseV2) -> String {
    use crate::palw_state_v2::PalwClaimPhaseV2 as P;
    match phase {
        P::Provisional => "provisional".into(),
        P::PanelBound { .. } => "panelBound".into(),
        P::ReceiptLicensed { .. } => "receiptLicensed".into(),
        P::Final { .. } => "final".into(),
        P::DefaultDisputed { .. } => "defaultDisputed".into(),
        P::Voided { reason, .. } => format!("voided:{reason:?}"),
    }
}

/// **A claim's permissionless-Panel status at `state`**, or `None` for a claim the state does not hold.
pub fn panel_v3_claim_status_v1(
    state: &PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    claim_id: &Hash64,
) -> Option<PanelV3ClaimStatusV1> {
    let claim = state.claim(claim_id)?;
    let rule = match params.panel_claim_rule_v1(claim.accepted_daa) {
        PanelClaimRuleV1::PermissionlessV3 => "permissionlessV3",
        PanelClaimRuleV1::HistoricalLaneA => "historicalLaneA",
    };
    let mut status = PanelV3ClaimStatusV1 {
        version: PANEL_V3_OBSERVATION_VERSION_V1,
        claim_id: *claim_id,
        rule,
        accepted_daa: claim.accepted_daa,
        v2_phase: v2_phase_name(&claim.phase),
        engine_phase: None,
        acceptance_order: None,
        seal: None,
        snapshot: None,
        beacon: None,
        assignment: None,
        retries: 0,
        terminal: None,
    };
    let Some(engine) = state.panel_v3() else { return Some(status) };
    let Some(record) = engine.claim_rows().get(claim_id) else { return Some(status) };
    status.engine_phase = Some(panel_v3_phase_name_v1(&record.phase));
    status.acceptance_order = Some(record.acceptance_order);
    status.seal = record.seal.clone();
    status.snapshot = record.snapshot.as_ref().map(|snapshot| PanelV3SnapshotSummaryV1 {
        root: snapshot.root,
        checkpoint: snapshot.checkpoint,
        checkpoint_daa: snapshot.checkpoint_daa,
        candidates: snapshot.candidates.len() as u32,
        policy_id: snapshot.policy_id,
    });
    let policy = engine.policy();
    if let Some(seal) = &record.seal {
        let retained = engine.beacon_rows().get(&seal.beacon_epoch).copied();
        let deadline = seal.anchor_slot.saturating_add(policy.beacon_wait_daa);
        status.beacon = Some(PanelV3BeaconStatusV1 {
            epoch: seal.beacon_epoch,
            release_daa: seal.anchor_slot,
            deadline_daa: deadline,
            scheme: policy.beacon_scheme,
            state: match &record.phase {
                ClaimPhaseV3::Voided { reason: NonFraudReasonV1::BeaconUnavailable, .. } => "unavailable",
                ClaimPhaseV3::PendingSeal => "collecting",
                ClaimPhaseV3::Sealed if retained.is_none() => "collecting",
                _ => "certified",
            },
            output: retained,
        });
    }
    let bound = match &record.phase {
        ClaimPhaseV3::Bound(binding) => Some(binding),
        _ => record.binding_history.last(),
    };
    status.assignment = bound.map(|binding| PanelV3AssignmentV1 {
        retry_index: binding.retry_index,
        seed: binding.panel_seed_v3,
        beacon_id: binding.beacon_id,
        seats: binding.seats.clone(),
        assignment_point: binding.assignment_point,
        bound_daa: binding.bound_daa,
        binding_block: binding.binding_block,
        exposure: binding.exposure.to_string(),
    });
    status.retries = (record.binding_history.len() as u16).saturating_sub(1);
    status.terminal = match &record.phase {
        ClaimPhaseV3::Voided { daa, reason } => {
            Some(PanelV3TerminalV1 { reason: panel_v3_reason_code_v1(*reason), daa: *daa, fraud: false })
        }
        ClaimPhaseV3::Released { daa } => Some(PanelV3TerminalV1 { reason: "RELEASED", daa: *daa, fraud: false }),
        _ => None,
    };
    Some(status)
}

/// **The engine at a glance**: its cursor, how many claims sit in each phase, and the epochs holding a certified output.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelV3OverviewV1 {
    pub version: u16,
    /// Whether the fence's engine exists on this chain.
    pub active: bool,
    pub tip: Option<Hash64>,
    pub height: Option<u64>,
    pub daa: Option<u64>,
    pub policy_id: Option<Hash64>,
    pub tracked_claims: u32,
    pub retained_work_ids: u32,
    pub pending_seal: u32,
    pub sealed: u32,
    pub entropy_ready: u32,
    pub bound: u32,
    pub released: u32,
    pub voided: u32,
    pub certified_epochs: Vec<u64>,
}

pub fn panel_v3_overview_v1(state: &PalwChainStateV2) -> PanelV3OverviewV1 {
    let mut overview = PanelV3OverviewV1 {
        version: PANEL_V3_OBSERVATION_VERSION_V1,
        active: false,
        tip: None,
        height: None,
        daa: None,
        policy_id: None,
        tracked_claims: 0,
        retained_work_ids: 0,
        pending_seal: 0,
        sealed: 0,
        entropy_ready: 0,
        bound: 0,
        released: 0,
        voided: 0,
        certified_epochs: Vec::new(),
    };
    let Some(engine) = state.panel_v3() else { return overview };
    overview.active = true;
    overview.tip = Some(engine.tip());
    overview.height = Some(engine.height());
    overview.daa = Some(engine.daa());
    overview.policy_id = Some(engine.policy().id());
    overview.tracked_claims = engine.claim_rows().len() as u32;
    overview.retained_work_ids = engine.work_id_rows().len() as u32;
    overview.certified_epochs = engine.beacon_rows().keys().copied().collect();
    for record in engine.claim_rows().values() {
        match record.phase {
            ClaimPhaseV3::PendingSeal => overview.pending_seal += 1,
            ClaimPhaseV3::Sealed => overview.sealed += 1,
            ClaimPhaseV3::EntropyReady { .. } => overview.entropy_ready += 1,
            ClaimPhaseV3::Bound(_) => overview.bound += 1,
            ClaimPhaseV3::Released { .. } => overview.released += 1,
            ClaimPhaseV3::Voided { .. } => overview.voided += 1,
        }
    }
    overview
}

/// The most claim records one observation returns, and the number it returns when none is named.
pub const PANEL_V3_OBSERVATION_MAX_CLAIMS_V1: usize = 64;
pub const PANEL_V3_OBSERVATION_DEFAULT_CLAIMS_V1: usize = 16;

/// Release support is distinct from an engine observed in a fixture or imported branch state.
/// This release's validator refuses every armed height; its approved beacon registry is empty.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelV3ReleaseStatusV1 {
    pub activation_supported: bool,
    pub approved_beacon_schemes: Vec<Hash64>,
}

pub fn panel_v3_release_status_v1() -> PanelV3ReleaseStatusV1 {
    PanelV3ReleaseStatusV1 {
        activation_supported: false,
        approved_beacon_schemes: crate::palw_panel_beacon_v1::approved_panel_beacon_policies_v1()
            .iter()
            .map(crate::palw_panel_beacon_v1::panel_beacon_scheme_of_v1)
            .collect(),
    }
}

/// **What `getPalwPanelV3Status` (RPC op 220) answers**: the engine at a glance and the status of the claims asked for (or, when none
/// is named, the first tracked claims in id order). A pure read of the tip state; no verdict reads it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelV3ObservationV1 {
    pub version: u16,
    pub overview: PanelV3OverviewV1,
    pub claims: Vec<PanelV3ClaimStatusV1>,
    /// Named claims the state does not hold (never a silent absence).
    pub unknown: Vec<Hash64>,
    /// Appended read-only metadata: an active engine alone does not establish release readiness.
    pub release: PanelV3ReleaseStatusV1,
}

impl PanelV3ObservationV1 {
    /// The camelCase JSON document (`u128` amounts are decimal strings; keys are only ever appended).
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("an observation is plain data and serializes")
    }
}

/// `ids` empty: the first `limit` tracked claims (0 = the default, capped). `ids` named: those claims (at most the cap), each either a
/// status or listed in `unknown`.
pub fn panel_v3_observation_v1(
    state: &PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    ids: &[Hash64],
    limit: usize,
) -> PanelV3ObservationV1 {
    let cap = match limit {
        0 => PANEL_V3_OBSERVATION_DEFAULT_CLAIMS_V1,
        n => n.min(PANEL_V3_OBSERVATION_MAX_CLAIMS_V1),
    };
    let named: Vec<Hash64> = if ids.is_empty() {
        state.panel_v3().map(|engine| engine.claim_rows().keys().take(cap).copied().collect()).unwrap_or_default()
    } else {
        ids.iter().take(PANEL_V3_OBSERVATION_MAX_CLAIMS_V1).copied().collect()
    };
    let mut claims = Vec::new();
    let mut unknown = Vec::new();
    for id in named {
        match panel_v3_claim_status_v1(state, params, &id) {
            Some(status) => claims.push(status),
            None => unknown.push(id),
        }
    }
    PanelV3ObservationV1 {
        version: PANEL_V3_OBSERVATION_VERSION_V1,
        overview: panel_v3_overview_v1(state),
        claims,
        unknown,
        release: panel_v3_release_status_v1(),
    }
}
