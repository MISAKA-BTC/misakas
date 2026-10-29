//! **PALW status views (lane UX-A, read-only): what a participant needs to see past the DAA 16,600 arming** — a claim's lifecycle
//! (Final / SecurityCertified / held / void with a typed code and a plain reason, the deadline, the Gate C draws and approvals), the
//! model tier (each class's tier, `c` and `κ`), the bond budget (design ops 250–253: a bond's budget, a claim's budget, the model
//! allocation, a bond's capital assignment) and the arming status (G14 route families armed now, the pinned records and waivers).
//!
//! **Nothing here is a rule.** Every value is read through the function the fold or an existing read already uses
//! ([`crate::palw_security_certification_v1::palw_claim_certification_status_v1`], the certification row's own counters,
//! [`crate::palw_model_tier_v1::palw_model_tier_at_v1`], [`crate::palw_model_tier_v1::palw_model_tier_coefficient_permille_v1`], the
//! budget engine's rows, [`crate::palw_g14_gates_v1::PalwG14RouteMirrorV1::route_armed`]); a value the chain does not hold is `null`.
//! No rule reads these documents.

use crate::palw_bond_budget_v1::{PalwBudgetClaimRowV1, PalwBudgetVectorV1};
use crate::palw_g14_gates_v1::PalwG14RouteFamilyV1;
use crate::palw_security_certification_v1 as cert;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwStateParamsV2};
use crate::palw_user_views_v1::{PalwUserViewClockV1, palw_bond_key_text_v1, palw_claim_phase_text_v1};
use kaspa_hashes::Hash64;
use serde_json::{Value, json};

/// The documents' schema version (each carries `"version"`).
pub const PALW_USER_STATUS_VERSION_V1: u32 = 1;
/// The cap on the rows a list view returns (classes, models, waivers per fence are counted whole).
pub const PALW_USER_STATUS_ROWS_V1: usize = 200;

/// What a status read is asked about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwStatusViewV1 {
    /// A chain claim id, or a kernel-route claim id (looked up when no chain claim has it).
    ClaimLifecycle(Hash64),
    /// One class, or every class (capped).
    ModelTier(Option<Hash64>),
    Arming,
    /// View `bond-budget` (design op 250's reader).
    BondBudget(PalwBondKeyV2),
    /// View `claim-budget` (design 251).
    ClaimBudget(Hash64),
    /// View `model-allocation` (design 252): one model, or every model row (capped).
    ModelAllocation(Option<Hash64>),
    /// View `capital-assignment` (design 253).
    CapitalAssignment(PalwBondKeyV2),
    /// Lane J1: view `kernel-classes` — every kernel class (id, root, tier, attested).
    KernelClasses,
    /// Lane J1: view `kernel-job` — one kernel job's status and escrow.
    KernelJob(Hash64),
    /// Lane J1: view `kernel-result` — one kernel job's result tokens, provisional or certified.
    KernelResult(Hash64),
}

/// **Every typed refusal / outcome code a participant can meet, with the plain reason the CLI prints.** Codes are the strings the
/// fold and the RPC already use (refusal `code`s, certification states, void reasons); the texts are explanations only.
pub const PALW_STATUS_CODE_TEXTS_V1: &[(&str, &str)] = &[
    // Admission refusals (`ClassNotRewardable.code`).
    ("G14_ROUTE_ABSENT", "the claim's family has no armed G14 public-conviction route: no reward, weight or consensus credit"),
    ("G14_JOB_VERSION_UNJUDGEABLE", "the FP job version (constraint / prefix) has no armed G14 route yet: refused at admission"),
    ("CELL_UNCOVERABLE", "the class's cell coverage has a gap: no claim on any lane is taken"),
    ("ELIG_NOT_ELIGIBLE", "the class's eligibility stage carries no reward-bearing block right"),
    ("ELIG_PROBATION_BUDGET", "the class's probation budget is spent"),
    ("G03_FRAMELESS_TIR_ORIGINAL", "a TIR original without its public frame (TFP1) is refused"),
    ("N4_NONPUBLIC_EVIDENCE", "non-public evidence (image / source ids) earns no new reward"),
    ("TIR_CLASS_CONTAINER_UNBOUND", "the TIR class is not bound to its PALWART3 container"),
    ("COURT_RETENTION_BELOW_GUARANTEE", "the node's court retention is below the guarantee the class needs"),
    // Gate C (SecurityCertified) states.
    ("not-governed", "accepted below Gate C: the released rules apply, no certification is needed"),
    ("pending", "Final reached (or not yet) and the drawn verifiers have not decided: reward held, no consensus credit yet"),
    ("certified", "SecurityCertified: k of m drawn verifiers reproduced the claim; the held reward is paid"),
    ("public-check-certified", "SecurityCertified through the class's public check"),
    ("not-certified", "fewer than k positive attestations by the deadline: the held reward is not paid (not a slash)"),
    ("vetoed", "the veto count of negative attestations was reached"),
    ("no-population", "the verifier pool could not fill m positions"),
    ("expired", "no draw happened by the certify-by deadline"),
    ("withdrawn", "the claim stopped being Final (voided or reversed) before a decision"),
    ("revoked", "the claim was certified, then voided: it holds no consensus credit"),
    // Void reasons (`getPalwClaims` spelling).
    ("bind_timeout", "voided: no Panel bound the claim in time"),
    ("receipt_timeout", "voided: the receipt window ended without a licence"),
    ("court_fraud", "voided: a court convicted the claim of a computation fault"),
    ("prefix_export_fraud", "voided: the producer's signed prefix export was refuted"),
    ("producer_withholding", "voided: the producer withheld material a court asked for (DA default)"),
    ("no_capable_panel", "voided: no capable Panel"),
    ("unavailable_quorum", "voided: the quorum was unavailable"),
    ("not_replay_backed", "voided: not replay-backed"),
    ("court_default", "voided: a party defaulted in court"),
    ("court_held_verdict", "voided: a held dissection's verdict"),
    ("aggregate_forfeit", "voided: the bond's aggregate forfeiture"),
    ("panel_unavailable", "voided: the Panel was unavailable (no producer fault)"),
    ("seal_unavailable", "voided: the seal was unavailable"),
    ("beacon_unavailable", "voided: the beacon was unavailable"),
    ("permissionless_no_capable_panel", "voided: no capable permissionless Panel"),
    ("work_root_expired", "voided: the work session was not ready by its expiry (uncharged)"),
    ("work_slice_proven_false", "voided: a work slice was convicted through the kernel route"),
    ("work_slice_defaulted", "voided: a work slice defaulted through the kernel route"),
    ("panel_da_disclosure_default", "voided: the PanelDa prompt was not disclosed by its deadline (a DA default, not a conviction)"),
];

/// The plain reason for `code`, if it is one this build names.
pub fn palw_status_code_text_v1(code: &str) -> Option<&'static str> {
    PALW_STATUS_CODE_TEXTS_V1.iter().find(|(c, _)| *c == code).map(|(_, t)| *t)
}

fn coded(code: &str) -> Value {
    json!({ "code": code, "text": palw_status_code_text_v1(code) })
}

fn void_code(reason: &str) -> String {
    // `PalwVoidReasonV2`'s Debug name in snake case — the spelling `getPalwClaims` uses.
    let mut out = String::new();
    for (i, ch) in reason.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            break;
        }
    }
    out
}

fn hex64(b: &[u8; 64]) -> String {
    faster_hex::hex_string(b)
}

fn certification_json(row: &cert::PalwClaimCertificationRowV1, policy: Option<&cert::PalwSecurityCertificationPolicyV1>) -> Value {
    json!({
        "attempt": row.attempt,
        "finalDaa": row.final_daa,
        "committedDaa": row.committed_daa,
        "anchorSpan": row.anchor.map(|(span, _)| span),
        "drawn": !row.draws.is_empty(),
        "drawnDaa": if row.draws.is_empty() { Value::Null } else { Value::from(row.drawn_daa) },
        "deadlineDaa": if row.draws.is_empty() { Value::Null } else { Value::from(row.deadline_daa) },
        "draws": row.draws.len(),
        "attestations": row.attestations.len(),
        "approvals": row.positive_positions(),
        "negatives": row.negative_positions(),
        "m": policy.map(|p| p.m),
        "k": policy.map(|p| p.k),
        "veto": policy.map(|p| p.veto),
        "heldRewardSompi": row.held_reward,
        "outcome": row.outcome.map(|(o, _)| format!("{o:?}")),
        "outcomeDaa": row.outcome.map(|(_, d)| d),
    })
}

/// **`claimLifecycle`**: a chain claim (phase, void reason with its code, Gate C state with its code, the certification row: draws,
/// approvals of k/m, deadline, held reward, outcome) or, when no chain claim has the id, a kernel-route claim's certification row and
/// its held Final reward. `None` when neither is held.
pub fn palw_claim_lifecycle_json_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim_id: &Hash64,
    clock: &PalwUserViewClockV1,
) -> Option<Value> {
    let rfc18 = params.rfc0018_v1();
    let policy = rfc18.map(|m| &m.certification);
    let now = clock.next_daa;
    if let Some(claim) = state.claim(claim_id) {
        let (phase, phase_daa, void) = palw_claim_phase_text_v1(&claim.phase);
        let void_code = void.as_deref().map(void_code);
        let status = cert::palw_claim_certification_status_v1(state, params, claim_id, claim);
        let row = state.kernel_route_state_v1().and_then(|route| {
            route.aux_row::<cert::PalwClaimCertificationRowV1>(
                cert::PALW_SECURITY_CERTIFICATION_TABLE_V1,
                &cert::certification_claim_key_v1(&cert::palw_certification_id_v1(claim_id)),
            )
        });
        let final_reached = matches!(claim.phase, PalwClaimPhaseV2::Final { .. });
        let held = final_reached && status == "pending";
        let fused = state.class(&claim.class_id).is_some_and(|c| c.fused_attention);
        let attempt_refusal = crate::palw_g14_gates_v1::palw_g14_route_attempt_refusal_for_class_v1(params, now, fused);
        let mut reasons = Vec::new();
        if let Some(code) = &void_code {
            reasons.push(coded(code));
        }
        reasons.push(coded(status));
        return Some(json!({
            "version": PALW_USER_STATUS_VERSION_V1,
            "kind": "palwClaimLifecycle",
            "route": "chain",
            "nowDaa": now,
            "claimId": claim_id.to_string(),
            "classId": claim.class_id.to_string(),
            "bond": palw_bond_key_text_v1(&claim.bond),
            "phase": phase,
            "phaseDaa": phase_daa,
            "final": final_reached,
            "voided": void_code.is_some(),
            "voidCode": void_code,
            "gateCGoverns": crate::palw_rfc0018_v1::palw_gate_c_governs_v1(params, claim),
            "securityCertified": status == "certified" || status == "public-check-certified",
            "certificationCode": status,
            "held": held,
            "certification": row.as_ref().map(|r| certification_json(r, policy)),
            "newAttemptOfClassNow": match &attempt_refusal {
                Some(why) => json!({ "admitted": false, "code": crate::palw_g14_gates_v1::PALW_G14_ROUTE_ABSENT_CODE_V1, "why": why }),
                None => json!({ "admitted": true }),
            },
            "reasons": reasons,
        }));
    }
    let kernel = claim_id.as_bytes();
    let row = cert::palw_kernel_claim_certification_row_v1(state, &kernel);
    let hold = cert::palw_kernel_claim_reward_hold_v1(state, &kernel);
    if row.is_none() && hold.is_none() {
        return None;
    }
    let code = match row.as_ref().and_then(|r| r.outcome) {
        None => "pending",
        Some((cert::PalwCertificationOutcomeV1::Certified, _)) => "certified",
        Some((cert::PalwCertificationOutcomeV1::PublicCheckCertified, _)) => "public-check-certified",
        Some((cert::PalwCertificationOutcomeV1::NotCertified, _)) => "not-certified",
        Some((cert::PalwCertificationOutcomeV1::Vetoed, _)) => "vetoed",
        Some((cert::PalwCertificationOutcomeV1::NoPopulation, _)) => "no-population",
        Some((cert::PalwCertificationOutcomeV1::Expired, _)) => "expired",
        Some((cert::PalwCertificationOutcomeV1::Withdrawn, _)) => "withdrawn",
    };
    Some(json!({
        "version": PALW_USER_STATUS_VERSION_V1,
        "kind": "palwClaimLifecycle",
        "route": "kernel",
        "nowDaa": now,
        "claimId": claim_id.to_string(),
        "final": row.is_some() || hold.is_some(),
        "securityCertified": cert::palw_kernel_claim_security_certified_v1(state, &kernel),
        "certificationCode": code,
        "held": hold.is_some(),
        "heldReward": hold.as_ref().map(|h| json!({
            "amountSompi": h.amount, "heldDaa": h.held_daa, "producerBond": hex64(&h.producer_bond), "posterBond": hex64(&h.poster_bond),
        })),
        "certification": row.as_ref().map(|r| certification_json(r, policy)),
        "reasons": [coded(code)],
    }))
}

fn tier_row(state: &PalwChainStateV2, params: &PalwStateParamsV2, class_id: &Hash64, now: u64) -> Value {
    use crate::palw_model_tier_v1 as tier;
    let t = tier::palw_model_tier_at_v1(state, params, class_id, now);
    let source = match t {
        tier::PalwModelTierV1::Approved(tier::PalwTierSourceV1::GenesisAssembly) => json!("genesis-assembly"),
        tier::PalwModelTierV1::Approved(tier::PalwTierSourceV1::GenesisList) => json!("genesis-catalog"),
        tier::PalwModelTierV1::Approved(tier::PalwTierSourceV1::AnnualFork { edition, from_daa }) => {
            json!({ "annualEdition": edition, "fromDaa": from_daa })
        }
        tier::PalwModelTierV1::Registered => Value::Null,
    };
    json!({
        "classId": class_id.to_string(),
        "held": state.class(class_id).is_some(),
        "tier": t.name(),
        "source": source,
        "cPermille": tier::palw_model_tier_coefficient_permille_v1(params.model_tier_v1(), t, now),
    })
}

/// **`modelTier`**: whether the tier fence is in force, `c` and `κ`, the catalog editions, and each class's tier (one class, or every
/// class the state holds, capped).
pub fn palw_model_tier_json_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2, class: Option<&Hash64>, now: u64) -> Option<Value> {
    let mirror = params.model_tier_v1();
    let (rows, total): (Vec<Value>, usize) = match class {
        Some(c) => {
            state.class(c)?;
            (vec![tier_row(state, params, c, now)], 1)
        }
        None => (
            state.classes_iter().take(PALW_USER_STATUS_ROWS_V1).map(|(id, _)| tier_row(state, params, id, now)).collect(),
            state.classes_iter().count(),
        ),
    };
    Some(json!({
        "version": PALW_USER_STATUS_VERSION_V1,
        "kind": "palwModelTier",
        "nowDaa": now,
        "fence": mirror.map(|m| json!({
            "fromDaa": m.from_daa,
            "active": m.active_at(now),
            "registeredCPermille": m.registered_permille,
            "kappaPermille": m.registered_cap_permille,
            "catalogEditions": m.catalogs.editions.iter().map(|e| json!({ "activationDaa": e.activation_daa })).collect::<Vec<_>>(),
        })),
        "classCount": total,
        "truncated": total > rows.len(),
        "classes": rows,
        "notes": ["the tier is an economic right only: every claim of every tier still needs SecurityCertified (Gate C)"],
    }))
}

const G14_FAMILIES: &[(PalwG14RouteFamilyV1, &str)] = &[
    (PalwG14RouteFamilyV1::LegacyPanelAttempt, "legacyPanelAttempt"),
    (PalwG14RouteFamilyV1::LegacyFusedLeaf, "legacyFusedLeaf"),
    (PalwG14RouteFamilyV1::FpSelection, "fpSelection"),
    (PalwG14RouteFamilyV1::FpConstraint, "fpConstraint"),
    (PalwG14RouteFamilyV1::FpPrefix, "fpPrefix"),
    (PalwG14RouteFamilyV1::FpTokenizer, "fpTokenizer"),
    (PalwG14RouteFamilyV1::NonPublicEvidence, "nonPublicEvidence"),
    (PalwG14RouteFamilyV1::KernelRoutePanelOne, "kernelRoutePanelOne"),
    (PalwG14RouteFamilyV1::FpPanelDaDisclosure, "fpPanelDaDisclosure"),
];

/// **`arming`**: each G14 route family — the fences it needs and whether it is armed for a claim accepted now (framed and unframed) —,
/// Gate C's height and policy, and the pinned arming records and typed waivers this binary carries (grouped by fence).
pub fn palw_arming_json_v1(params: &PalwStateParamsV2, now: u64) -> Value {
    let mirror = params.g14_route_required_v1();
    let families: Vec<Value> = G14_FAMILIES
        .iter()
        .map(|(f, name)| {
            json!({
                "family": name,
                "fences": crate::palw_g14_gates_v1::palw_g14_route_family_fences_v1(*f),
                "armed": mirror.map(|m| m.route_armed(*f, now, false)),
                "armedFramed": mirror.map(|m| m.route_armed(*f, now, true)),
            })
        })
        .collect();
    let mut by_fence: std::collections::BTreeMap<&str, (u64, usize, usize, usize)> = Default::default();
    for w in crate::palw_t12_17000_records_v1::PALW_T12_17000_WAIVERS_V1 {
        let e = by_fence.entry(w.fence).or_insert((w.activation_daa, 0, 0, 0));
        e.1 += 1;
        match w.kind {
            // F1 kinds (ACT2): skipped drills and unrun node E2Es count as "not run"; estimates / agent review only in the total.
            crate::palw_arming_waiver_v1::PalwArmingWaiverKindV1::DrillSkipped
            | crate::palw_arming_waiver_v1::PalwArmingWaiverKindV1::E2ENotRun => e.2 += 1,
            crate::palw_arming_waiver_v1::PalwArmingWaiverKindV1::UserDecision => e.3 += 1,
            crate::palw_arming_waiver_v1::PalwArmingWaiverKindV1::ProvisionalEstimate
            | crate::palw_arming_waiver_v1::PalwArmingWaiverKindV1::AgentReview => {}
        }
    }
    let waivers: Vec<Value> = by_fence
        .iter()
        .map(|(fence, (daa, n, not_run, decision))| {
            json!({ "fence": fence, "activationDaa": daa, "waivers": n, "notRun": not_run, "userDecision": decision })
        })
        .collect();
    let rfc18 = params.rfc0018_v1();
    json!({
        "version": PALW_USER_STATUS_VERSION_V1,
        "kind": "palwArming",
        "nowDaa": now,
        "g14RouteRequired": mirror.map(|m| json!({ "fromDaa": m.from_daa, "governs": m.governs(now) })),
        "routeFamilies": families,
        "gateC": rfc18.map(|m| json!({
            "fromDaa": m.gate_c_from_daa,
            "inForce": m.gate_c_at(now),
            "m": m.certification.m,
            "k": m.certification.k,
            "veto": m.certification.veto,
            "attestWindowDaa": m.certification.attest_window_daa,
            "certifyByDaa": m.certification.certify_by_daa,
            "prosecutionWindowDaa": m.certification.prosecution_window_daa,
        })),
        "records": {
            "g14": crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORDS_V1.len(),
            "kernelRoute": crate::palw_kernel_route_arming_v1::PALW_KERNEL_ROUTE_ARMING_RECORDS_V1.len(),
            "rfc0018": crate::palw_rfc0018_v1::PALW_RFC0018_ARMING_RECORDS_V1.len(),
            "modelTier": crate::palw_model_tier_v1::PALW_MODEL_TIER_ARMING_RECORDS_V1.len(),
        },
        "waivers": waivers,
        "notes": ["every waiver records a requirement that was NOT run as pinned evidence (or a user decision); it is never a pass"],
    })
}

fn vector_json(v: &PalwBudgetVectorV1) -> Value {
    json!({
        "claims": v.claims,
        "blockUnits": v.block_units,
        "rewardSompi": v.reward_sompi,
        "finalWeight": v.final_weight.to_string(),
        "roundRights": v.round_rights,
    })
}

fn budget_fence(params: &PalwStateParamsV2, now: u64) -> Value {
    match params.bond_budget() {
        None => json!({ "status": "fence dormant" }),
        Some(m) => json!({
            "status": if m.active_at(now) { "active" } else { "scheduled" },
            "fromDaa": m.from_daa,
            "allocationFromDaa": m.allocation.as_ref().map(|(from, _)| *from),
            "allocationEpoch": m.allocation_epoch_at(now),
        }),
    }
}

fn budget_doc(kind: &str, params: &PalwStateParamsV2, now: u64, body: Value) -> Value {
    let mut doc = json!({
        "version": PALW_USER_STATUS_VERSION_V1,
        "kind": kind,
        "nowDaa": now,
        "fence": budget_fence(params, now),
    });
    if let (Value::Object(d), Value::Object(b)) = (&mut doc, body) {
        d.extend(b);
    }
    doc
}

fn claim_budget_json(row: &PalwBudgetClaimRowV1) -> Value {
    json!({
        "bond": palw_bond_key_text_v1(&row.bond),
        "model": row.model.map(|(m, epoch)| json!({ "model": m.to_string(), "epoch": epoch })),
        "acceptedDaa": row.accepted_daa,
        "reuseNotBefore": row.reuse_not_before,
        "reserved": vector_json(&row.reserved),
        "consumed": vector_json(&row.consumed),
        "open": row.open,
        "inWindow": row.in_window,
        "live": row.live,
        "origin": format!("{:?}", row.origin),
        "liabilitySompi": row.liability_sompi.to_string(),
    })
}

/// **View `bond-budget`**: a bond's capital, caps, window, open claims and next release. `None` for a bond the registry
/// does not hold; the engine's figures are `null` until the fence's first block.
pub fn palw_bond_budget_json_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2, bond: &PalwBondKeyV2, now: u64) -> Option<Value> {
    let reg = state.bond(bond)?;
    let engine = state.bond_budget();
    let caps = params.bond_budget().map(|m| state.bond_budget_caps_v1(&m.policy, bond));
    let row = engine.and_then(|b| b.bond_row(bond));
    Some(budget_doc(
        "palwBondBudget",
        params,
        now,
        json!({
            "bond": palw_bond_key_text_v1(bond),
            "capitalSompi": reg.collateral,
            "engine": engine.is_some(),
            "caps": caps.as_ref().map(vector_json),
            "window": row.map(|r| vector_json(&r.window)),
            "openClaims": row.map(|r| r.open_claims),
            "latestReleaseDaa": row.map(|r| r.latest_release_daa),
            "openLiabilitySompi": row.map(|r| r.open_liability_sompi.to_string()),
            "openFinalWeight": row.map(|r| r.open_final_weight.to_string()),
            "liabilityHoldUntil": row.map(|r| r.liability_hold_until),
            "withdrawalHeld": state.bond_budget_window_holds(bond, now),
        }),
    ))
}

/// **View `claim-budget`**: a claim's reservation, consumption, `reuse_not_before` and model/epoch. `None` without a row.
pub fn palw_claim_budget_json_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2, claim: &Hash64, now: u64) -> Option<Value> {
    let row = state.bond_budget()?.claim_row(claim)?;
    Some(budget_doc(
        "palwClaimBudget",
        params,
        now,
        json!({
            "claimId": claim.to_string(),
            "row": claim_budget_json(row),
            "finalWeightGranted": state.bond_budget_final_weight(claim).map(|w| w.to_string()),
        }),
    ))
}

/// **View `model-allocation`**: the epoch, `ΣA`, accrued, and per model `S_m`, `A_m`, `available_m`, `reserved_m` with the
/// model's tier and `c_m` (one model, or every row, capped). Answers with the fence status even before the engine exists.
pub fn palw_model_allocation_json_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    model: Option<&Hash64>,
    now: u64,
) -> Option<Value> {
    let engine = state.bond_budget();
    let model_json = |id: &Hash64, row: &crate::palw_bond_budget_v1::PalwModelBudgetRowV1| {
        let tier = crate::palw_model_tier_v1::palw_model_tier_of_model_v1(state, params, id, now);
        json!({
            "model": id.to_string(),
            "epoch": row.epoch,
            "capitalSompi": row.capital.to_string(),
            "weight": row.weight.to_string(),
            "reservedSompi": row.reserved_sompi,
            "availableSompi": engine.map(|b| b.model_available(id)),
            "tier": tier.name(),
            "cPermille": crate::palw_model_tier_v1::palw_model_tier_coefficient_permille_v1(params.model_tier_v1(), tier, now),
        })
    };
    let (rows, total): (Vec<Value>, usize) = match (engine, model) {
        (Some(b), Some(m)) => (b.model_row(m).map(|r| vec![model_json(m, r)]).unwrap_or_default(), 1),
        (Some(b), None) => (b.models.iter().take(PALW_USER_STATUS_ROWS_V1).map(|(id, r)| model_json(id, r)).collect(), b.models.len()),
        (None, _) => (Vec::new(), 0),
    };
    if model.is_some() && rows.is_empty() && engine.is_some() {
        return None;
    }
    let epoch = engine.and_then(|b| b.header.allocation);
    Some(budget_doc(
        "palwModelAllocation",
        params,
        now,
        json!({
            "engine": engine.is_some(),
            "epoch": epoch.map(|e| json!({
                "index": e.index, "startDaa": e.start_daa, "sumA": e.sum_a.to_string(), "accruedSompi": e.accrued_sompi,
            })),
            "modelCount": total,
            "truncated": total > rows.len(),
            "models": rows,
        }),
    ))
}

/// **View `capital-assignment`**: a bond's effective and pending assignments and their sequence. `None` for a bond the
/// registry does not hold.
pub fn palw_capital_assignment_json_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    bond: &PalwBondKeyV2,
    now: u64,
) -> Option<Value> {
    state.bond(bond)?;
    let row = state.bond_budget().and_then(|b| b.assignment_row(bond));
    let pairs = |v: &Vec<(Hash64, u64)>| v.iter().map(|(m, a)| json!({ "model": m.to_string(), "amountSompi": a })).collect::<Vec<_>>();
    Some(budget_doc(
        "palwCapitalAssignment",
        params,
        now,
        json!({
            "bond": palw_bond_key_text_v1(bond),
            "assigned": row.is_some(),
            "sequence": row.map(|r| r.sequence),
            "effective": row.map(|r| pairs(&r.effective)),
            "pending": row.and_then(|r| r.pending.as_ref().map(pairs)),
            "pendingSinceEpoch": row.map(|r| r.pending_since_epoch),
        }),
    ))
}

/// **The one dispatcher** the consensus read calls (`palw_status_view_v1`).
pub fn palw_status_view_json_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    view: PalwStatusViewV1,
    clock: &PalwUserViewClockV1,
) -> Option<Value> {
    let now = clock.next_daa;
    match view {
        PalwStatusViewV1::ClaimLifecycle(c) => palw_claim_lifecycle_json_v1(state, params, &c, clock),
        PalwStatusViewV1::ModelTier(c) => palw_model_tier_json_v1(state, params, c.as_ref(), now),
        PalwStatusViewV1::Arming => Some(palw_arming_json_v1(params, now)),
        PalwStatusViewV1::BondBudget(b) => palw_bond_budget_json_v1(state, params, &b, now),
        PalwStatusViewV1::ClaimBudget(c) => palw_claim_budget_json_v1(state, params, &c, now),
        PalwStatusViewV1::ModelAllocation(m) => palw_model_allocation_json_v1(state, params, m.as_ref(), now),
        PalwStatusViewV1::CapitalAssignment(b) => palw_capital_assignment_json_v1(state, params, &b, now),
        PalwStatusViewV1::KernelClasses => crate::palw_user_job_views_v1::palw_kernel_classes_json_v1(state, params, clock),
        PalwStatusViewV1::KernelJob(j) => crate::palw_user_job_views_v1::palw_kernel_job_json_v1(state, &j, clock),
        PalwStatusViewV1::KernelResult(j) => crate::palw_user_job_views_v1::palw_kernel_result_json_v1(state, &j, clock),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn void_codes_match_the_rpc_spelling() {
        assert_eq!(void_code("PanelDaDisclosureDefault"), "panel_da_disclosure_default");
        assert_eq!(void_code("CourtFraud"), "court_fraud");
        assert!(palw_status_code_text_v1("panel_da_disclosure_default").is_some());
        assert!(palw_status_code_text_v1(crate::palw_g14_gates_v1::PALW_G14_ROUTE_ABSENT_CODE_V1).is_some());
        assert!(palw_status_code_text_v1(crate::palw_g14_gates_v1::PALW_G14_JOB_UNJUDGEABLE_CODE_V1).is_some());
    }

    #[test]
    fn arming_reads_without_any_fence() {
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, Hash64::from_bytes([1; 64]), 4, 1000, 100, 1000, 0).unwrap();
        let doc = palw_arming_json_v1(&params, 10);
        assert!(doc["gateC"].is_null() && doc["g14RouteRequired"].is_null(), "no fence: nothing armed");
        assert_eq!(doc["kind"], "palwArming");
        assert_eq!(doc["routeFamilies"].as_array().unwrap().len(), G14_FAMILIES.len());
    }
}
