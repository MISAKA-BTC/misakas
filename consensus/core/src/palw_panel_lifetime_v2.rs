//! Signed absolute native Panel service deadlines. Old accepted claims retain
//! their historical clocks. Expiry does not waive a pending court or earned cash.
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;
use crate::palw_state_v2::{PalwBondKeyV2, PalwClaimStateV2};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use misaka_palw_challenge::hash::object_id;
pub use misaka_palw_kernel::panel_lifetime_v2::PanelConsumerLifetimeTermsV2;
pub use misaka_palw_kernel::panel_lifetime_v2::PanelLifetimeAuthorizationV2;

pub const PALW_PANEL_LIFETIME_TABLE_V2: u8 = 121;
pub const PANEL_AUTHORIZATION_PREFIX_V2: u8 = 0x41;
pub const PANEL_CAPTURE_PREFIX_V2: u8 = 0x42;

pub fn panel_authorization_key_v2(id: &[u8; 64]) -> Vec<u8> {
    [vec![PANEL_AUTHORIZATION_PREFIX_V2], id.to_vec()].concat()
}
pub fn panel_capture_key_v2(id: &Hash64) -> Vec<u8> {
    [vec![PANEL_CAPTURE_PREFIX_V2], id.as_bytes().to_vec()].concat()
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPanelAuthorizationRowV2 {
    pub terms: PanelLifetimeAuthorizationV2,
    pub signer: PalwBondKeyV2,
    pub authorized_daa: u64,
    pub authorized_block: Hash64,
    pub policy: Hash64,
    pub consumed_claim: Option<Hash64>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPanelConsentCaptureV2 {
    pub authorization: [u8; 64],
    pub claim: Hash64,
    pub accepted_daa: u64,
    pub immutable_fields: Hash64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPanelLifetimePolicyV2 {
    pub maximum_service_daa: u64,
    pub maximum_pause_daa: u64,
    pub maximum_retries: u32,
    /// Lifetime admission bound, never reset when a claim leaves its live set.
    pub maximum_enrollments: u32,
    pub maximum_enrollments_per_producer: u32,
    pub security_review_scope: [u8; 64],
    pub economic_review_scope: [u8; 64],
}
impl PalwPanelLifetimePolicyV2 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(object_id(b"MISAKA/PALW/PANEL-LIFETIME-POLICY/V2", self))
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.maximum_service_daa == 0
            || self.maximum_service_daa == u64::MAX
            || self.maximum_pause_daa > self.maximum_service_daa
            || self.maximum_enrollments == 0
            || self.maximum_enrollments_per_producer == 0
            || self.maximum_enrollments_per_producer > self.maximum_enrollments
            || self.security_review_scope == [0; 64]
            || self.economic_review_scope == [0; 64]
        {
            return Err("Panel lifetime needs finite bounds and explicit scoped review identities");
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPanelLifetimeFenceV2 {
    pub activation: ForkActivation,
    pub policy: PalwPanelLifetimePolicyV2,
}
impl PalwPanelLifetimeFenceV2 {
    /// Structural private-fixture values; this grants no policy approval and
    /// the public startup validator still refuses every armed value.
    pub fn unapproved_probe_at(activation: ForkActivation) -> Self {
        Self {
            activation,
            policy: PalwPanelLifetimePolicyV2 {
                maximum_service_daa: 10_000,
                maximum_pause_daa: 1000,
                maximum_retries: 1,
                maximum_enrollments: 64,
                maximum_enrollments_per_producer: 8,
                security_review_scope: [1; 64],
                economic_review_scope: [2; 64],
            },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPanelLifetimeMirrorV2 {
    pub from_daa: u64,
    pub policy: PalwPanelLifetimePolicyV2,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPanelLifetimeEnrollmentV2 {
    pub terms: PanelConsumerLifetimeTermsV2,
    pub signer: PalwBondKeyV2,
    pub enrolled_daa: u64,
    pub enrolled_block: Hash64,
    pub policy: Hash64,
    pub pause_started_daa: Option<u64>,
    pub total_paused_daa: u64,
    /// 0: composition of signed prospective authorization and authenticated
    /// native attempt/FP commitment; 1: additional exact producer attachment.
    pub authority_kind: u8,
}
impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    pub fn panel_authorization_v2(&self, id: &[u8; 64]) -> Result<PalwPanelAuthorizationRowV2, String> {
        let row: PalwPanelAuthorizationRowV2 = self
            .aux_row(PALW_PANEL_LIFETIME_TABLE_V2, &panel_authorization_key_v2(id))
            .ok_or("signed prospective Panel authorization missing")?;
        row.terms.validate().map_err(str::to_string)?;
        if row.terms.id() != *id
            || row.terms.producer != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&row.signer)
            || row.authorized_block == Hash64::default()
            || row.authorized_daa > row.terms.accept_by_daa
        {
            return Err("Panel prospective authorization does not bind its signature authority".into());
        }
        Ok(row)
    }
    pub fn panel_consent_capture_v2(&self, id: &Hash64) -> Result<PalwPanelConsentCaptureV2, String> {
        let capture: PalwPanelConsentCaptureV2 = self
            .aux_row(PALW_PANEL_LIFETIME_TABLE_V2, &panel_capture_key_v2(id))
            .ok_or("native Panel claim has no pre-entropy consent capture")?;
        let auth = self.panel_authorization_v2(&capture.authorization)?;
        if capture.claim != *id
            || auth.consumed_claim != Some(*id)
            || capture.accepted_daa < auth.authorized_daa
            || capture.accepted_daa < auth.terms.accept_from_daa
            || capture.accepted_daa > auth.terms.accept_by_daa
            || capture.immutable_fields == Hash64::default()
        {
            return Err("native Panel consent capture is not its one authorized execution".into());
        }
        Ok(capture)
    }
    pub fn panel_lifetime_enrollment_v2(&self, claim: &Hash64) -> Result<Option<PalwPanelLifetimeEnrollmentV2>, String> {
        let key = borsh::to_vec(&claim.as_bytes()).expect("claim id encodes");
        let Some(bytes) = self.aux.get(&(PALW_PANEL_LIFETIME_TABLE_V2, key)) else { return Ok(None) };
        let row: PalwPanelLifetimeEnrollmentV2 = borsh::from_slice(bytes).map_err(|e| format!("Panel lifetime row: {e}"))?;
        row.terms.validate().map_err(str::to_string)?;
        if row.terms.claim != claim.as_bytes()
            || row.enrolled_daa > row.terms.service_until_daa
            || row.authority_kind > 1
            || row.enrolled_block == Hash64::default()
            || row.policy == Hash64::default()
            || row.terms.producer != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&row.signer)
        {
            return Err("Panel lifetime row is not the signed claim authority".into());
        }
        let capture = self.panel_consent_capture_v2(claim)?;
        let auth = self.panel_authorization_v2(&capture.authorization)?;
        if row.terms.authorization != capture.authorization
            || row.terms.immutable_fields != capture.immutable_fields.as_bytes()
            || row.signer != auth.signer
            || row.policy != auth.policy
            || row.enrolled_daa != capture.accepted_daa
            || (row.terms.network, row.terms.ruleset, row.terms.class, row.terms.producer)
                != (auth.terms.network, auth.terms.ruleset, auth.terms.class, auth.terms.producer)
            || (
                row.terms.accept_by_daa,
                row.terms.service_until_daa,
                row.terms.liability_until_daa,
                row.terms.maximum_retries,
                row.terms.maximum_pause_daa,
            ) != (
                auth.terms.accept_by_daa,
                auth.terms.service_until_daa,
                auth.terms.liability_until_daa,
                auth.terms.maximum_retries,
                auth.terms.maximum_pause_daa,
            )
        {
            return Err("Panel exact attachment changes the producer's prospective consent".into());
        }
        Ok(Some(row))
    }
}
impl Params {
    pub fn validate_palw_panel_lifetime_v2(&self) -> Result<(), PalwModeV2Error> {
        let Some(f) = self.palw_panel_lifetime_v2.filter(|f| f.activation != ForkActivation::never()) else { return Ok(()) };
        f.policy.validate().map_err(PalwModeV2Error::Invalid)?;
        Err(PalwModeV2Error::Invalid(
            "Panel lifetime cannot be armed: scoped source/security/economic and full consumer acceptance review remain pending",
        ))
    }
}

/// Match the producer's consent against actual accepted immutable fields. A
/// sponsor/source enrollment alone grants no authority over this claim.
pub fn panel_lifetime_claim_v2(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    id: &Hash64,
    claim: &PalwClaimStateV2,
) -> Result<Option<PalwPanelLifetimeEnrollmentV2>, String> {
    let Some(mirror) = params.panel_lifetime_v2_at(claim.accepted_daa) else { return Ok(None) };
    let engine = params.panel_v3().ok_or("Panel policy missing for its signed lifetime")?;
    let row = state
        .kernel_route()
        .ok_or("Panel lifetime route missing")?
        .panel_lifetime_enrollment_v2(id)?
        .ok_or("Panel claim has no signed lifetime enrollment")?;
    let t = &row.terms;
    let authorization = state.kernel_route().ok_or("Panel route missing")?.panel_authorization_v2(&t.authorization)?;
    if row.policy != mirror.policy.id()
        || t.network != engine.network.as_bytes()
        || t.ruleset != engine.ruleset.as_bytes()
        || t.class != claim.class_id.as_bytes()
        || Some(Hash64::from_bytes(t.work)) != claim.work_id
        || t.producer != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&claim.bond)
        || claim.accepted_daa != row.enrolled_daa
        || claim.accepted_block != row.enrolled_block
        || claim.accepted_daa > t.accept_by_daa
        || claim.pwu > authorization.terms.maximum_pwu
        || claim.reserved > authorization.terms.maximum_reserved_sompi
        || claim.escrowed_reward > authorization.terms.maximum_escrow_sompi
        || claim.rights_reserved > authorization.terms.maximum_rights_sompi
    {
        return Err("Panel lifetime consent differs from actual admission".into());
    }
    Ok(Some(row))
}

impl PalwPanelLifetimeEnrollmentV2 {
    /// Include an interval still open at the object's actual inclusion position.
    /// A same-block court close has not necessarily reached the post-object
    /// counter observer yet, so persisted closed intervals alone are insufficient.
    pub fn paused_daa_at(&self, now: u64) -> Result<u64, String> {
        let open = self
            .pause_started_daa
            .map(|start| now.checked_sub(start).ok_or_else(|| "Panel pause counter starts after the observed position".to_string()))
            .transpose()?
            .unwrap_or(0);
        self.total_paused_daa.checked_add(open).ok_or_else(|| "Panel pause lifetime counter overflow".to_string())
    }
}

pub fn require_panel_receipt_time_v2(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    id: &Hash64,
    now: u64,
) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
    use crate::palw_state_v2::PalwStateV2Error;
    let Some(claim) = state.claim(id) else { return Ok(()) };
    let Some(row) = panel_lifetime_claim_v2(state, params, id, claim).map_err(PalwStateV2Error::KernelRouteRefused)? else {
        return Ok(());
    };
    if now > row.terms.service_until_daa
        || row.paused_daa_at(now).map_err(PalwStateV2Error::KernelRouteRefused)? > row.terms.maximum_pause_daa
    {
        return Err(PalwStateV2Error::KernelRouteRefused(
            "Panel receipt inclusion exceeds its signed absolute service or pause budget".into(),
        ));
    }
    Ok(())
}

pub fn require_panel_session_window_v2(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    id: &Hash64,
    required_until: u64,
) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
    use crate::palw_state_v2::PalwStateV2Error;
    let Some(claim) = state.claim(id) else { return Ok(()) };
    if let Some(row) = panel_lifetime_claim_v2(state, params, id, claim).map_err(PalwStateV2Error::KernelRouteRefused)?
        && required_until > row.terms.liability_until_daa
    {
        return Err(PalwStateV2Error::KernelRouteRefused(
            "native court or DA extension exceeds its signed Panel liability endpoint".into(),
        ));
    }
    Ok(())
}

impl crate::palw_state_v2::PalwChainStateV2 {
    /// Restart/IBD validates all three rooted row namespaces, including consumed
    /// authorizations retained after terminal claims. Capacity never resets.
    pub fn assert_panel_lifetime_consistency_v2(
        &self,
        params: &crate::palw_state_v2::PalwStateParamsV2,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_state_v2::PalwStateV2Error;
        let refuse = |why: String| PalwStateV2Error::CarriageInconsistent(format!("Panel signed lifetime: {why}"));
        let Some(route) = self.kernel_route() else { return Ok(()) };
        let now = self.last_point().map(|p| p.daa_score).unwrap_or(0);
        let mut total = 0u32;
        let mut owned = std::collections::BTreeMap::<PalwBondKeyV2, u32>::new();
        for (table, key) in route.aux.keys() {
            if *table != PALW_PANEL_LIFETIME_TABLE_V2 {
                continue;
            }
            match (key.len(), key.first().copied()) {
                (65, Some(PANEL_AUTHORIZATION_PREFIX_V2)) => {
                    let id: [u8; 64] = key[1..].try_into().expect("bounded authorization key");
                    let row = route.panel_authorization_v2(&id).map_err(&refuse)?;
                    let mirror = params
                        .panel_lifetime_v2_at(row.authorized_daa)
                        .ok_or_else(|| refuse("authorization before its compiled activation".into()))?;
                    let panel = params.panel_v3().ok_or_else(|| refuse("compiled Panel engine absent".into()))?;
                    let class = Hash64::from_bytes(row.terms.class);
                    let court = self.class_model_court_window_v1(&class).unwrap_or(params.window_court());
                    let tail = court
                        .checked_mul(2)
                        .and_then(|n| n.checked_add(params.window_challenge()))
                        .ok_or_else(|| refuse("liability tail overflow".into()))?;
                    if row.authorized_daa > now
                        || row.policy != mirror.policy.id()
                        || self.bond(&row.signer).is_none()
                        || self.class(&class).is_none()
                        || row.terms.network != panel.network.as_bytes()
                        || row.terms.ruleset != panel.ruleset.as_bytes()
                        || row
                            .authorized_daa
                            .checked_add(mirror.policy.maximum_service_daa)
                            .is_none_or(|end| row.terms.service_until_daa > end)
                        || row.terms.service_until_daa.checked_add(tail).is_none_or(|end| row.terms.liability_until_daa < end)
                        || row.terms.maximum_retries > mirror.policy.maximum_retries
                        || row.terms.maximum_retries > u32::from(panel.policy.max_retries)
                        || row.terms.maximum_pause_daa > mirror.policy.maximum_pause_daa
                    {
                        return Err(refuse("authorization changes native authority or finite policy".into()));
                    }
                    total = total.checked_add(1).ok_or_else(|| refuse("inventory overflow".into()))?;
                    let count = owned.entry(row.signer).or_default();
                    *count = count.checked_add(1).ok_or_else(|| refuse("producer inventory overflow".into()))?;
                    if total > mirror.policy.maximum_enrollments || *count > mirror.policy.maximum_enrollments_per_producer {
                        return Err(refuse("retained lifetime capacity exceeded".into()));
                    }
                    if let Some(claim) = row.consumed_claim {
                        route
                            .panel_lifetime_enrollment_v2(&claim)
                            .map_err(&refuse)?
                            .ok_or_else(|| refuse("consumed authorization has no exact native capture".into()))?;
                    }
                }
                (65, Some(PANEL_CAPTURE_PREFIX_V2)) => {
                    let id = Hash64::from_bytes(key[1..].try_into().expect("bounded capture key"));
                    route.panel_consent_capture_v2(&id).map_err(&refuse)?;
                    route
                        .panel_lifetime_enrollment_v2(&id)
                        .map_err(&refuse)?
                        .ok_or_else(|| refuse("captured execution has no exact native enrollment".into()))?;
                }
                (64, _) => {
                    let id = Hash64::from_bytes(key.as_slice().try_into().expect("bounded claim key"));
                    let row = route
                        .panel_lifetime_enrollment_v2(&id)
                        .map_err(&refuse)?
                        .ok_or_else(|| refuse("exact native enrollment disappeared".into()))?;
                    let claim = self.claim(&id).ok_or_else(|| refuse("capture has no native execution".into()))?;
                    panel_lifetime_claim_v2(self, params, &id, claim)
                        .map_err(&refuse)?
                        .ok_or_else(|| refuse("captured claim before compiled activation".into()))?;
                    if row.enrolled_daa > now || row.pause_started_daa.is_some_and(|start| start > now) {
                        return Err(refuse("capture/pause lies after the observed native point".into()));
                    }
                    if let Some(record) = self.panel_v3().and_then(|p| p.claim(&id)) {
                        if record.claim.immutable_fields.as_bytes() != row.terms.immutable_fields {
                            return Err(refuse("Panel engine differs from its pre-entropy captured statement".into()));
                        }
                    } else if !claim.phase.is_terminal() {
                        return Err(refuse("live captured native claim has no Panel admission".into()));
                    }
                }
                _ => return Err(refuse("unallocated lifetime row namespace".into())),
            }
        }
        Ok(())
    }
}
