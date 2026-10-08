//! RFC-0010: dormant configuration, public-chain snapshot/admission adapters and versioned fold.
//! Historical lane A remains unchanged. The production fold (`palw_panel_v3_fold_v1`, a child of `palw_state_v2`) wires the
//! engine into `PalwChainStateV2` — Some-only root block, deltas 170–173, carriage tail `0xED` — and hands every binding to the
//! V2 receipt/court machinery as an ordinary panel record. **No certified entropy source is approved**
//! ([`crate::palw_panel_beacon_v1`]), so this binary refuses EVERY attempted activation, including custom params.

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
            let mirror = fence.map(|fence| PalwPanelV3ParamsV1 { from_daa: fence.activation.daa_score(), policy: fence.policy, network, ruleset });
            bundle.state = bundle.state.clone().with_panel_v3(mirror);
        }
    }

    pub fn validate_palw_permissionless_panel_v1(&self) -> Result<(), PalwModeV2Error> {
        if let Some(rule) = self.palw_permissionless_panel_v1.filter(|r| r.activation != ForkActivation::never()) {
            rule.policy.validate().map_err(|_| {
                PalwModeV2Error::Invalid("palw_permissionless_panel_v1 has invalid timing, capacity or beacon parameters")
            })?;
            return Err(PalwModeV2Error::Invalid(
                "palw_permissionless_panel_v1 cannot be armed: RFC-0010 approves no certified Panel beacon (BEACON_UNAVAILABLE: no Panel-independent Final exists, `approved_panel_beacon_policies_v1` is empty) and its bias/withholding/P0-10 review is external; R-core+, the panel economy and audit_2026_09_23 must also be in force before it, and RFC-0014's gates must pass first",
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
    /// settlements. The shipped scheme registry is empty and no Final on this chain is Panel-independent, so it refuses everything.
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
